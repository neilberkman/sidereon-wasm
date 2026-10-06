//! Station tidal displacement models (IERS): solid-earth, ocean loading, and
//! solid-earth pole tide, and the BLQ ocean-loading coefficient format.
//!
//! Thin wrapper over `sidereon_core::tides`. The Love/Shida expansions, the BLQ
//! constituent sum, the pole-tide geometry and the BLQ reader and writer all
//! live in the crate; this layer only reshapes the ECEF vectors and BLQ
//! coefficient grids and re-encodes the displacement. Every displacement is a
//! geocentric ITRF displacement in metres.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::astro::time::ValidityMode;
use sidereon_core::frame::{FrameValueError, Wgs84Geodetic};
use sidereon_core::tides::{
    ocean_tide_loading, parse_ocean_loading_blq_block, parse_ocean_loading_blq_blocks,
    solid_earth_pole_tide, solid_earth_tide, station_displacement_ecef_m_batch,
    station_displacement_ecef_m_batch_with_validity, station_displacement_ecef_m_with_validity,
    write_ocean_loading_blq_blocks, BlqParseErrorKind, BlqWriteErrorKind, OceanLoadingBlq,
    OceanLoadingBlqBlock, OceanLoadingBlqComment, OceanLoadingBlqCommentPlacement,
    StationDisplacementEpoch, StationDisplacementOptions, StationDisplacementPosition,
    StationPolarMotion, StationTideConstants as CoreStationTideConstants, TideError,
    NUM_OCEAN_CONSTITUENTS,
};

use crate::error::{
    engine_error, error_with_detail, index_arg, reject_unknown_keys, to_plain_js, type_error,
};
use crate::marshal::vec3_finite;

/// Number of BLQ components (radial, EW, NS) times the constituent count: the
/// length of each flat row-major ocean-loading grid this binding accepts.
const OCEAN_GRID_LEN: usize = 3 * NUM_OCEAN_CONSTITUENTS;
const STATION_DISPLACEMENT_KEYS: &[&str] = &[
    "stationEcefM",
    "stationGeodetic",
    "year",
    "month",
    "day",
    "hour",
    "minute",
    "second",
    "xpArcsec",
    "ypArcsec",
    "solidEarthTide",
    "poleTide",
    "oceanAmplitudeM",
    "oceanPhaseDeg",
    "constants",
    "validity",
];
const STATION_GEODETIC_KEYS: &[&str] = &["latitudeRad", "longitudeRad", "heightM"];
const STATION_DISPLACEMENT_BATCH_KEYS: &[&str] = &[
    "stationEcefM",
    "stationGeodetic",
    "epochs",
    "solidEarthTide",
    "poleTide",
    "oceanAmplitudeM",
    "oceanPhaseDeg",
    "constants",
    "validity",
];
const STATION_DISPLACEMENT_EPOCH_KEYS: &[&str] = &[
    "year", "month", "day", "hour", "minute", "second", "xpArcsec", "ypArcsec",
];

fn reject_nested_object(
    parent: &JsValue,
    key: &str,
    context: &str,
    known: &[&str],
) -> Result<(), JsValue> {
    let value = js_sys::Reflect::get(parent, &JsValue::from_str(key))
        .map_err(|_| type_error(&format!("could not read {context}")))?;
    if !value.is_null() && !value.is_undefined() {
        reject_unknown_keys(&value, context, known)?;
    }
    Ok(())
}

fn reject_array_item_keys(value: &JsValue, context: &str, known: &[&str]) -> Result<(), JsValue> {
    if js_sys::Array::is_array(value) {
        for (index, item) in js_sys::Array::from(value).iter().enumerate() {
            reject_unknown_keys(&item, &format!("{context}[{index}]"), known)?;
        }
    }
    Ok(())
}

fn reject_station_displacement_request(value: &JsValue) -> Result<(), JsValue> {
    reject_unknown_keys(
        value,
        "station displacement request",
        STATION_DISPLACEMENT_KEYS,
    )?;
    reject_nested_object(
        value,
        "stationGeodetic",
        "station geodetic coordinates",
        STATION_GEODETIC_KEYS,
    )
}

fn reject_station_displacement_batch(value: &JsValue) -> Result<(), JsValue> {
    reject_unknown_keys(
        value,
        "station displacement batch",
        STATION_DISPLACEMENT_BATCH_KEYS,
    )?;
    reject_nested_object(
        value,
        "stationGeodetic",
        "station geodetic coordinates",
        STATION_GEODETIC_KEYS,
    )?;
    reject_array_item_keys(
        &js_sys::Reflect::get(value, &JsValue::from_str("epochs"))
            .map_err(|_| type_error("could not read station displacement epochs"))?,
        "station displacement epochs",
        STATION_DISPLACEMENT_EPOCH_KEYS,
    )
}

/// Reshape a flat row-major `(3, NUM_OCEAN_CONSTITUENTS)` buffer into the BLQ
/// component-by-constituent grid, rejecting a wrong length (`TypeError`).
fn ocean_grid(name: &str, values: &[f64]) -> Result<[[f64; NUM_OCEAN_CONSTITUENTS]; 3], JsValue> {
    if values.len() != OCEAN_GRID_LEN {
        return Err(type_error(&format!(
            "{name} must have length {OCEAN_GRID_LEN} (flat row-major 3-by-{NUM_OCEAN_CONSTITUENTS}), got {}",
            values.len()
        )));
    }
    let mut grid = [[0.0_f64; NUM_OCEAN_CONSTITUENTS]; 3];
    for (component, row) in grid.iter_mut().enumerate() {
        for (constituent, cell) in row.iter_mut().enumerate() {
            *cell = values[component * NUM_OCEAN_CONSTITUENTS + constituent];
        }
    }
    Ok(grid)
}

/// Solid-earth tide displacement of an ITRF station, metres (ECEF).
///
/// `stationEcefM`, `sunEcefM`, `moonEcefM` are length-3 geocentric ECEF metre
/// vectors. The epoch is the UTC `year`/`month`/`day` plus `fractionalHour`
/// (`hour + min/60 + sec/3600`, in `[0, 24)`). Returns the displacement
/// `[dx, dy, dz]`. Delegates to `sidereon_core::tides::solid_earth_tide`.
#[wasm_bindgen(js_name = solidEarthTide)]
pub fn solid_earth_tide_js(
    station_ecef_m: &[f64],
    year: i32,
    month: i32,
    day: i32,
    fractional_hour: f64,
    sun_ecef_m: &[f64],
    moon_ecef_m: &[f64],
) -> Result<Vec<f64>, JsValue> {
    let xsta = vec3_finite("stationEcefM", station_ecef_m)?;
    let xsun = vec3_finite("sunEcefM", sun_ecef_m)?;
    let xmon = vec3_finite("moonEcefM", moon_ecef_m)?;
    let d = solid_earth_tide(&xsta, year, month, day, fractional_hour, &xsun, &xmon)
        .map_err(tide_error)?;
    Ok(d.to_vec())
}

/// Ocean tide loading displacement of an ITRF station, metres (ECEF).
///
/// `stationEcefM` is a length-3 geocentric ECEF metre vector and the epoch is
/// the UTC `year`/`month`/`day` plus `fractionalHour`. `amplitudeM` and
/// `phaseDeg` are the station's BLQ coefficients as flat row-major
/// `(3, 11)` `Float64Array`s: component order radial / EW-west / NS-south, and
/// constituent order M2 S2 N2 K2 K1 O1 P1 Q1 Mf Mm Ssa. Returns the displacement
/// `[dx, dy, dz]`. Delegates to `sidereon_core::tides::ocean_tide_loading`.
#[wasm_bindgen(js_name = oceanTideLoading)]
pub fn ocean_tide_loading_js(
    station_ecef_m: &[f64],
    year: i32,
    month: i32,
    day: i32,
    fractional_hour: f64,
    amplitude_m: &[f64],
    phase_deg: &[f64],
) -> Result<Vec<f64>, JsValue> {
    let xsta = vec3_finite("stationEcefM", station_ecef_m)?;
    let blq = OceanLoadingBlq {
        amplitude_m: ocean_grid("amplitudeM", amplitude_m)?,
        phase_deg: ocean_grid("phaseDeg", phase_deg)?,
    };
    let d =
        ocean_tide_loading(&xsta, year, month, day, fractional_hour, &blq).map_err(tide_error)?;
    Ok(d.to_vec())
}

/// Solid-earth pole tide displacement of an ITRF station, metres (ECEF).
///
/// `stationEcefM` is a length-3 geocentric ECEF metre vector and the epoch is
/// the UTC `year`/`month`/`day` plus `fractionalHour`. `xpArcsec` / `ypArcsec`
/// are the polar-motion coordinates in arcseconds. Returns the displacement
/// `[dx, dy, dz]`. Delegates to `sidereon_core::tides::solid_earth_pole_tide`.
#[wasm_bindgen(js_name = solidEarthPoleTide)]
pub fn solid_earth_pole_tide_js(
    station_ecef_m: &[f64],
    year: i32,
    month: i32,
    day: i32,
    fractional_hour: f64,
    xp_arcsec: f64,
    yp_arcsec: f64,
) -> Result<Vec<f64>, JsValue> {
    let xsta = vec3_finite("stationEcefM", station_ecef_m)?;
    let d = solid_earth_pole_tide(
        &xsta,
        year,
        month,
        day,
        fractional_hour,
        xp_arcsec,
        yp_arcsec,
    )
    .map_err(tide_error)?;
    Ok(d.to_vec())
}

#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StationTideConstants {
    Conventions = 0,
    IersRoutine = 1,
}

impl From<StationTideConstants> for CoreStationTideConstants {
    fn from(value: StationTideConstants) -> Self {
        match value {
            StationTideConstants::Conventions => Self::Conventions,
            StationTideConstants::IersRoutine => Self::IersRoutine,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StationDisplacementRequest {
    #[serde(default, deserialize_with = "crate::exact::option_vec3")]
    station_ecef_m: Option<[f64; 3]>,
    #[serde(default)]
    station_geodetic: Option<StationGeodeticInput>,
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: f64,
    #[serde(default)]
    xp_arcsec: Option<f64>,
    #[serde(default)]
    yp_arcsec: Option<f64>,
    #[serde(default = "default_true")]
    solid_earth_tide: bool,
    #[serde(default)]
    pole_tide: bool,
    #[serde(default)]
    ocean_amplitude_m: Option<Vec<Vec<f64>>>,
    #[serde(default)]
    ocean_phase_deg: Option<Vec<Vec<f64>>>,
    #[serde(default)]
    constants: Option<u32>,
    #[serde(default)]
    validity: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StationGeodeticInput {
    latitude_rad: f64,
    longitude_rad: f64,
    height_m: f64,
}

fn default_true() -> bool {
    true
}

fn tide_error(err: TideError) -> JsValue {
    let message = err.to_string();
    error_with_detail("TideError", &message, &tide_error_json(&err))
}

fn tide_frame_error(error: FrameValueError) -> TideError {
    match error {
        FrameValueError::InvalidInput { field, reason } => TideError::InvalidInput {
            field,
            kind: if reason == "must be finite" {
                sidereon_core::tides::TideInputErrorKind::NonFinite
            } else {
                sidereon_core::tides::TideInputErrorKind::OutOfRange
            },
        },
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StationDisplacementOutput {
    ecef_m: [f64; 3],
    solid_earth_tide_ecef_m: Option<[f64; 3]>,
    pole_tide_ecef_m: Option<[f64; 3]>,
    ocean_loading_ecef_m: Option<[f64; 3]>,
    degraded: Option<&'static str>,
}

#[wasm_bindgen(
    js_name = stationTideDisplacement,
    unchecked_return_type = "StationDisplacementResult"
)]
pub fn station_displacement_js(
    #[wasm_bindgen(unchecked_param_type = "StationDisplacementRequest")] request: JsValue,
) -> Result<JsValue, JsValue> {
    reject_station_displacement_request(&request)?;
    let input: StationDisplacementRequest = serde_wasm_bindgen::from_value(request)
        .map_err(|error| type_error(&format!("invalid station displacement request: {error}")))?;
    let polar_motion = match (input.xp_arcsec, input.yp_arcsec) {
        (None, None) => None,
        (Some(xp_arcsec), Some(yp_arcsec)) => {
            Some(StationPolarMotion::from_arcseconds(xp_arcsec, yp_arcsec))
        }
        _ => {
            return Err(type_error(
                "xpArcsec and ypArcsec must be supplied together",
            ))
        }
    };
    let epoch = StationDisplacementEpoch {
        year: input.year,
        month: input.month,
        day: input.day,
        hour: input.hour,
        minute: input.minute,
        second: input.second,
        polar_motion,
    };
    let position = match (input.station_ecef_m, input.station_geodetic) {
        (Some(ecef_m), None) => {
            StationDisplacementPosition::from_ecef_m(ecef_m).map_err(tide_error)?
        }
        (None, Some(geodetic)) => {
            let position = Wgs84Geodetic::new(
                geodetic.latitude_rad,
                geodetic.longitude_rad,
                geodetic.height_m,
            )
            .map_err(tide_frame_error)
            .map_err(tide_error)?;
            StationDisplacementPosition::Geodetic(position)
        }
        (Some(_), Some(_)) => {
            return Err(type_error(
                "provide stationEcefM or stationGeodetic, not both",
            ))
        }
        (None, None) => return Err(type_error("stationEcefM or stationGeodetic is required")),
    };
    let blq = match (input.ocean_amplitude_m, input.ocean_phase_deg) {
        (None, None) => None,
        (Some(amplitude_m), Some(phase_deg)) => Some(OceanLoadingBlq {
            amplitude_m: blq_grid("oceanAmplitudeM", &amplitude_m)?,
            phase_deg: blq_grid("oceanPhaseDeg", &phase_deg)?,
        }),
        _ => {
            return Err(type_error(
                "oceanAmplitudeM and oceanPhaseDeg must be supplied together",
            ))
        }
    };
    let constants = match input
        .constants
        .unwrap_or(StationTideConstants::Conventions as u32)
    {
        value if value == StationTideConstants::Conventions as u32 => {
            CoreStationTideConstants::Conventions
        }
        value if value == StationTideConstants::IersRoutine as u32 => {
            CoreStationTideConstants::IersRoutine
        }
        other => {
            return Err(type_error(&format!(
                "unknown station-tide constants {other}"
            )))
        }
    };
    let validity = match input.validity.as_deref().unwrap_or("strict") {
        "strict" => ValidityMode::Strict,
        "permissive" => ValidityMode::Permissive,
        other => return Err(type_error(&format!("unknown UT1 validity mode {other:?}"))),
    };
    let validated = station_displacement_ecef_m_with_validity(
        position,
        epoch,
        {
            let mut options = StationDisplacementOptions::default();
            options.solid_earth_tide = input.solid_earth_tide;
            options.pole_tide = input.pole_tide;
            options.ocean_loading = blq.as_ref();
            options.solid_earth_tide_constants = constants;
            options
        },
        validity,
    )
    .map_err(tide_error)?;
    let output = StationDisplacementOutput {
        ecef_m: validated.value.ecef_m,
        solid_earth_tide_ecef_m: validated.value.solid_earth_tide_ecef_m,
        pole_tide_ecef_m: validated.value.pole_tide_ecef_m,
        ocean_loading_ecef_m: validated.value.ocean_loading_ecef_m,
        degraded: validated.degraded.map(|reason| match reason {
            sidereon_core::astro::time::DegradeReason::BeforeCoverage => "beforeCoverage",
            sidereon_core::astro::time::DegradeReason::AfterCoverage => "afterCoverage",
        }),
    };
    to_plain_js(&output, "station displacement")
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StationDisplacementBatchRequest {
    #[serde(default, deserialize_with = "crate::exact::option_vec3")]
    station_ecef_m: Option<[f64; 3]>,
    #[serde(default)]
    station_geodetic: Option<StationGeodeticInput>,
    epochs: Vec<StationDisplacementBatchEpoch>,
    #[serde(default)]
    solid_earth_tide: Option<bool>,
    #[serde(default)]
    pole_tide: bool,
    #[serde(default)]
    ocean_amplitude_m: Option<Vec<Vec<f64>>>,
    #[serde(default)]
    ocean_phase_deg: Option<Vec<Vec<f64>>>,
    #[serde(default)]
    constants: Option<u32>,
    #[serde(default)]
    validity: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StationDisplacementBatchEpoch {
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: f64,
    #[serde(default)]
    xp_arcsec: Option<f64>,
    #[serde(default)]
    yp_arcsec: Option<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StationDisplacementBatchRow {
    index: usize,
    value: Option<StationDisplacementOutput>,
    error: Option<serde_json::Value>,
}

fn station_tide_constants(value: Option<u32>) -> Result<CoreStationTideConstants, JsValue> {
    match value.unwrap_or(StationTideConstants::Conventions as u32) {
        value if value == StationTideConstants::Conventions as u32 => {
            Ok(CoreStationTideConstants::Conventions)
        }
        value if value == StationTideConstants::IersRoutine as u32 => {
            Ok(CoreStationTideConstants::IersRoutine)
        }
        other => Err(type_error(&format!(
            "unknown station-tide constants {other}"
        ))),
    }
}

pub(crate) fn tide_error_json(error: &TideError) -> serde_json::Value {
    let message = error.to_string();
    match error {
        TideError::InvalidInput { field, kind } => serde_json::json!({
            "kind": "INVALID_INPUT", "field": field, "reason": tide_input_kind(kind), "message": message,
        }),
        TideError::TimeScale(sidereon_core::astro::time::CoverageError::InvalidInput {
            field,
            kind,
        }) => serde_json::json!({
            "kind": "TIME_SCALE", "coverage": {"kind": "INVALID_INPUT", "field": field, "reason": time_input_kind(kind)}, "message": message,
        }),
        TideError::TimeScale(sidereon_core::astro::time::CoverageError::OutsideCoverage(
            reason,
        )) => serde_json::json!({
            "kind": "TIME_SCALE", "coverage": {"kind": "OUTSIDE_COVERAGE", "reason": degrade_reason(*reason)}, "message": message,
        }),
        TideError::FrameTransform(source) => serde_json::json!({
            "kind": "FRAME_TRANSFORM", "source": frame_transform_json(source), "message": message,
        }),
        TideError::SunMoon(source) => serde_json::json!({
            "kind": "SUN_MOON", "source": sun_moon_json(source), "message": message,
        }),
        TideError::MissingInput { field } => serde_json::json!({
            "kind": "MISSING_INPUT", "field": field, "message": message,
        }),
        TideError::BlqParse { line, kind } => serde_json::json!({
            "kind": "BLQ_PARSE", "line": line, "reason": BlqParseReasonJs::from(kind), "message": message,
        }),
        TideError::BlqWrite { block, kind } => serde_json::json!({
            "kind": "BLQ_WRITE", "block": block, "reason": BlqWriteReasonJs::from(kind), "message": message,
        }),
    }
}

fn tide_input_kind(kind: &sidereon_core::tides::TideInputErrorKind) -> &'static str {
    use sidereon_core::tides::TideInputErrorKind as Kind;
    match kind {
        Kind::Missing => "MISSING",
        Kind::NonFinite => "NON_FINITE",
        Kind::NotPositive => "NOT_POSITIVE",
        Kind::Negative => "NEGATIVE",
        Kind::OutOfRange => "OUT_OF_RANGE",
        Kind::FloatParse => "FLOAT_PARSE",
        Kind::IntParse => "INT_PARSE",
        Kind::InvalidCivilDate => "INVALID_CIVIL_DATE",
        Kind::InvalidCivilTime => "INVALID_CIVIL_TIME",
    }
}

pub(crate) fn time_input_kind(
    kind: &sidereon_core::astro::time::TimeScaleInputErrorKind,
) -> &'static str {
    use sidereon_core::astro::time::TimeScaleInputErrorKind as Kind;
    match kind {
        Kind::Missing => "MISSING",
        Kind::NonFinite => "NON_FINITE",
        Kind::NotPositive => "NOT_POSITIVE",
        Kind::Negative => "NEGATIVE",
        Kind::OutOfRange => "OUT_OF_RANGE",
        Kind::FloatParse => "FLOAT_PARSE",
        Kind::IntParse => "INT_PARSE",
        Kind::InvalidCivilDate => "INVALID_CIVIL_DATE",
        Kind::InvalidCivilTime => "INVALID_CIVIL_TIME",
    }
}

pub(crate) fn degrade_reason(reason: sidereon_core::astro::time::DegradeReason) -> &'static str {
    match reason {
        sidereon_core::astro::time::DegradeReason::BeforeCoverage => "BEFORE_COVERAGE",
        sidereon_core::astro::time::DegradeReason::AfterCoverage => "AFTER_COVERAGE",
    }
}

fn frame_transform_json(
    error: &sidereon_core::astro::frames::transforms::FrameTransformError,
) -> serde_json::Value {
    use sidereon_core::astro::frames::transforms::FrameTransformError as Error;
    match error {
        Error::InvalidInput { field, reason } => {
            serde_json::json!({ "kind": "INVALID_INPUT", "field": field, "reason": reason })
        }
        Error::Ut1OutsideCoverage { reason } => {
            serde_json::json!({ "kind": "UT1_OUTSIDE_COVERAGE", "reason": degrade_reason(*reason) })
        }
    }
}

fn sun_moon_json(error: &sidereon_core::astro::bodies::SunMoonError) -> serde_json::Value {
    use sidereon_core::astro::bodies::SunMoonError as Error;
    match error {
        Error::InvalidInput { field, reason } => {
            serde_json::json!({ "kind": "INVALID_INPUT", "field": field, "reason": reason })
        }
        Error::FrameTransform(source) => {
            serde_json::json!({ "kind": "FRAME_TRANSFORM", "source": frame_transform_json(source) })
        }
    }
}

#[wasm_bindgen(
    js_name = stationTideDisplacementBatch,
    unchecked_return_type = "StationDisplacementBatchRow[]"
)]
pub fn station_displacement_batch_js(
    #[wasm_bindgen(unchecked_param_type = "StationDisplacementBatchRequest")] request: JsValue,
) -> Result<JsValue, JsValue> {
    reject_station_displacement_batch(&request)?;
    let input: StationDisplacementBatchRequest = serde_wasm_bindgen::from_value(request)
        .map_err(|error| type_error(&format!("invalid station displacement batch: {error}")))?;
    let position = match (input.station_ecef_m, input.station_geodetic) {
        (Some(ecef_m), None) => {
            StationDisplacementPosition::from_ecef_m(ecef_m).map_err(tide_error)?
        }
        (None, Some(geodetic)) => StationDisplacementPosition::Geodetic(
            Wgs84Geodetic::new(
                geodetic.latitude_rad,
                geodetic.longitude_rad,
                geodetic.height_m,
            )
            .map_err(tide_frame_error)
            .map_err(tide_error)?,
        ),
        (Some(_), Some(_)) => {
            return Err(type_error(
                "provide stationEcefM or stationGeodetic, not both",
            ))
        }
        (None, None) => return Err(type_error("stationEcefM or stationGeodetic is required")),
    };
    let blq = match (input.ocean_amplitude_m, input.ocean_phase_deg) {
        (None, None) => None,
        (Some(amplitude_m), Some(phase_deg)) => Some(OceanLoadingBlq {
            amplitude_m: blq_grid("oceanAmplitudeM", &amplitude_m)?,
            phase_deg: blq_grid("oceanPhaseDeg", &phase_deg)?,
        }),
        _ => {
            return Err(type_error(
                "oceanAmplitudeM and oceanPhaseDeg must be supplied together",
            ))
        }
    };
    let constants = station_tide_constants(input.constants)?;
    let validity = match input.validity.as_deref().unwrap_or("strict") {
        "strict" => ValidityMode::Strict,
        "permissive" => ValidityMode::Permissive,
        other => return Err(type_error(&format!("unknown UT1 validity mode {other:?}"))),
    };
    let epochs = input
        .epochs
        .into_iter()
        .map(|row| {
            let polar_motion = match (row.xp_arcsec, row.yp_arcsec) {
                (None, None) => Ok(None),
                (Some(xp_arcsec), Some(yp_arcsec)) => Ok(Some(
                    StationPolarMotion::from_arcseconds(xp_arcsec, yp_arcsec),
                )),
                _ => Err(type_error(
                    "each epoch must supply xpArcsec and ypArcsec together",
                )),
            }?;
            Ok(StationDisplacementEpoch {
                year: row.year,
                month: row.month,
                day: row.day,
                hour: row.hour,
                minute: row.minute,
                second: row.second,
                polar_motion,
            })
        })
        .collect::<Result<Vec<_>, JsValue>>()?;
    let mut options = StationDisplacementOptions::default();
    options.solid_earth_tide = input.solid_earth_tide.unwrap_or(true);
    options.pole_tide = input.pole_tide;
    options.ocean_loading = blq.as_ref();
    options.solid_earth_tide_constants = constants;
    let rows = match validity {
        ValidityMode::Strict => station_displacement_ecef_m_batch(position, &epochs, options)
            .into_iter()
            .map(|row| row.map(|value| (value, None)))
            .collect::<Vec<_>>(),
        ValidityMode::Permissive => {
            station_displacement_ecef_m_batch_with_validity(position, &epochs, options, validity)
                .into_iter()
                .map(|row| row.map(|validated| (validated.value, validated.degraded)))
                .collect::<Vec<_>>()
        }
    };
    let output = rows
        .into_iter()
        .enumerate()
        .map(|(index, row)| match row {
            Ok((value, degraded)) => StationDisplacementBatchRow {
                index,
                value: Some(StationDisplacementOutput {
                    ecef_m: value.ecef_m,
                    solid_earth_tide_ecef_m: value.solid_earth_tide_ecef_m,
                    pole_tide_ecef_m: value.pole_tide_ecef_m,
                    ocean_loading_ecef_m: value.ocean_loading_ecef_m,
                    degraded: degraded.map(|reason| match reason {
                        sidereon_core::astro::time::DegradeReason::BeforeCoverage => {
                            "beforeCoverage"
                        }
                        sidereon_core::astro::time::DegradeReason::AfterCoverage => "afterCoverage",
                    }),
                }),
                error: None,
            },
            Err(error) => StationDisplacementBatchRow {
                index,
                value: None,
                error: Some(tide_error_json(&error)),
            },
        })
        .collect::<Vec<_>>();
    to_plain_js(&output, "station displacement batch")
}

// --- BLQ station blocks -------------------------------------------------------

/// One retained comment or column-order header line of a BLQ block.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BlqCommentJs {
    /// `"beforeStation"`, `"beforeRow"` or `"afterRows"`.
    placement: &'static str,
    /// Zero-based coefficient row the line precedes, for `"beforeRow"`;
    /// `null` otherwise.
    row: Option<usize>,
    /// The line exactly as read, without its terminator.
    line: String,
}

/// One parsed BLQ station block. The coefficient grids are in the standard
/// constituent order M2 S2 N2 K2 K1 O1 P1 Q1 Mf Mm Ssa whatever order the file
/// declared, rows radial, EW (west positive), NS (south positive).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BlqBlockJs {
    station: String,
    amplitude_m: Vec<Vec<f64>>,
    phase_deg: Vec<Vec<f64>>,
    comments: Vec<BlqCommentJs>,
}

impl From<&OceanLoadingBlqBlock> for BlqBlockJs {
    fn from(block: &OceanLoadingBlqBlock) -> Self {
        let grid = |rows: &[[f64; NUM_OCEAN_CONSTITUENTS]; 3]| {
            rows.iter().map(|row| row.to_vec()).collect::<Vec<_>>()
        };
        Self {
            station: block.station.clone(),
            amplitude_m: grid(&block.coefficients.amplitude_m),
            phase_deg: grid(&block.coefficients.phase_deg),
            comments: block
                .comments
                .iter()
                .map(|comment| {
                    let (placement, row) = match comment.placement {
                        OceanLoadingBlqCommentPlacement::BeforeStation => ("beforeStation", None),
                        OceanLoadingBlqCommentPlacement::BeforeRow(row) => ("beforeRow", Some(row)),
                        OceanLoadingBlqCommentPlacement::AfterRows => ("afterRows", None),
                    };
                    BlqCommentJs {
                        placement,
                        row,
                        line: comment.line.clone(),
                    }
                })
                .collect(),
        }
    }
}

const BLQ_BLOCK_KEYS: &[&str] = &["station", "amplitudeM", "phaseDeg", "comments"];
const BLQ_COMMENT_KEYS: &[&str] = &["placement", "row", "line"];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlqCommentInput {
    placement: String,
    #[serde(default)]
    row: Option<f64>,
    line: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlqBlockInput {
    station: String,
    amplitude_m: Vec<Vec<f64>>,
    phase_deg: Vec<Vec<f64>>,
    #[serde(default)]
    comments: Option<Vec<BlqCommentInput>>,
}

/// Read a `3 x 11` coefficient grid, refusing any other shape as a
/// `TypeError`. Values are taken as given: a non-finite one is the writer's to
/// refuse by row and constituent.
fn blq_grid(name: &str, rows: &[Vec<f64>]) -> Result<[[f64; NUM_OCEAN_CONSTITUENTS]; 3], JsValue> {
    if rows.len() != 3 {
        return Err(type_error(&format!(
            "{name} must hold 3 rows (radial, EW, NS), got {}",
            rows.len()
        )));
    }
    let mut grid = [[0.0_f64; NUM_OCEAN_CONSTITUENTS]; 3];
    for (index, (out, row)) in grid.iter_mut().zip(rows).enumerate() {
        if row.len() != NUM_OCEAN_CONSTITUENTS {
            return Err(type_error(&format!(
                "{name}[{index}] must hold {NUM_OCEAN_CONSTITUENTS} values (M2 S2 N2 K2 K1 O1 P1 Q1 Mf Mm Ssa), got {}",
                row.len()
            )));
        }
        out.copy_from_slice(row);
    }
    Ok(grid)
}

fn blq_placement(
    comment: &BlqCommentInput,
    path: &str,
) -> Result<OceanLoadingBlqCommentPlacement, JsValue> {
    match (comment.placement.as_str(), comment.row) {
        ("beforeStation", None) => Ok(OceanLoadingBlqCommentPlacement::BeforeStation),
        ("afterRows", None) => Ok(OceanLoadingBlqCommentPlacement::AfterRows),
        ("beforeRow", Some(row)) => Ok(OceanLoadingBlqCommentPlacement::BeforeRow(index_arg(
            row,
            &format!("{path}.row"),
        )?)),
        ("beforeRow", None) => Err(type_error(&format!(
            "{path}.row is required for placement \"beforeRow\""
        ))),
        ("beforeStation" | "afterRows", Some(_)) => Err(type_error(&format!(
            "{path}.row applies only to placement \"beforeRow\""
        ))),
        (other, _) => Err(type_error(&format!(
            "{path}.placement {other:?} is not \"beforeStation\", \"beforeRow\" or \"afterRows\""
        ))),
    }
}

fn blq_block_from_js(value: &JsValue, path: &str) -> Result<OceanLoadingBlqBlock, JsValue> {
    reject_unknown_keys(value, path, BLQ_BLOCK_KEYS)?;
    if let Ok(comments) = js_sys::Reflect::get(value, &JsValue::from_str("comments")) {
        if js_sys::Array::is_array(&comments) {
            for (index, comment) in js_sys::Array::from(&comments).iter().enumerate() {
                reject_unknown_keys(
                    &comment,
                    &format!("{path}.comments[{index}]"),
                    BLQ_COMMENT_KEYS,
                )?;
            }
        }
    }
    let input: BlqBlockInput = serde_wasm_bindgen::from_value(value.clone())
        .map_err(|err| type_error(&format!("invalid {path}: {err}")))?;
    let comments = input
        .comments
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(index, comment)| {
            Ok(OceanLoadingBlqComment {
                placement: blq_placement(comment, &format!("{path}.comments[{index}]"))?,
                line: comment.line.clone(),
            })
        })
        .collect::<Result<Vec<_>, JsValue>>()?;
    Ok(OceanLoadingBlqBlock {
        station: input.station,
        coefficients: OceanLoadingBlq {
            amplitude_m: blq_grid(&format!("{path}.amplitudeM"), &input.amplitude_m)?,
            phase_deg: blq_grid(&format!("{path}.phaseDeg"), &input.phase_deg)?,
        },
        comments,
    })
}

/// Why a BLQ header or block does not read, without its position.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind")]
enum BlqParseReasonJs {
    #[serde(rename = "EMPTY")]
    Empty,
    #[serde(rename = "MISSING_STATION")]
    MissingStation,
    #[serde(rename = "MISSING_COEFFICIENT_ROWS", rename_all = "camelCase")]
    MissingCoefficientRows {
        station: String,
        expected: usize,
        found: usize,
    },
    #[serde(rename = "TOO_MANY_COEFFICIENT_ROWS", rename_all = "camelCase")]
    TooManyCoefficientRows { station: String },
    #[serde(rename = "WRONG_COLUMN_COUNT", rename_all = "camelCase")]
    WrongColumnCount { expected: usize, found: usize },
    #[serde(rename = "INVALID_NUMBER", rename_all = "camelCase")]
    InvalidNumber { token: String },
    #[serde(rename = "NON_FINITE_NUMBER", rename_all = "camelCase")]
    NonFiniteNumber { token: String },
    #[serde(rename = "UNSUPPORTED_CONSTITUENT", rename_all = "camelCase")]
    UnsupportedConstituent { constituent: String },
    #[serde(rename = "DUPLICATE_CONSTITUENT", rename_all = "camelCase")]
    DuplicateConstituent { constituent: String },
    #[serde(rename = "MULTIPLE_BLOCKS", rename_all = "camelCase")]
    MultipleBlocks { found: usize },
}

impl From<&BlqParseErrorKind> for BlqParseReasonJs {
    fn from(kind: &BlqParseErrorKind) -> Self {
        match kind {
            BlqParseErrorKind::Empty => Self::Empty,
            BlqParseErrorKind::MissingStation => Self::MissingStation,
            BlqParseErrorKind::MissingCoefficientRows {
                station,
                expected,
                found,
            } => Self::MissingCoefficientRows {
                station: station.clone(),
                expected: *expected,
                found: *found,
            },
            BlqParseErrorKind::TooManyCoefficientRows { station } => Self::TooManyCoefficientRows {
                station: station.clone(),
            },
            BlqParseErrorKind::WrongColumnCount { expected, found } => Self::WrongColumnCount {
                expected: *expected,
                found: *found,
            },
            BlqParseErrorKind::InvalidNumber { token } => Self::InvalidNumber {
                token: token.clone(),
            },
            BlqParseErrorKind::NonFiniteNumber { token } => Self::NonFiniteNumber {
                token: token.clone(),
            },
            BlqParseErrorKind::UnsupportedConstituent { constituent } => {
                Self::UnsupportedConstituent {
                    constituent: constituent.clone(),
                }
            }
            BlqParseErrorKind::DuplicateConstituent { constituent } => Self::DuplicateConstituent {
                constituent: constituent.clone(),
            },
            BlqParseErrorKind::MultipleBlocks { found } => Self::MultipleBlocks { found: *found },
        }
    }
}

/// The `detail` of a thrown `BlqParseError`: the reason, its one-based line
/// (0 for a whole-input failure) and the engine's message.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BlqParseErrorDetailJs {
    #[serde(flatten)]
    reason: BlqParseReasonJs,
    line: usize,
    message: String,
}

/// Why a block cannot be written so that it reads back unchanged.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind")]
enum BlqWriteReasonJs {
    #[serde(rename = "EMPTY_STATION")]
    EmptyStation,
    #[serde(rename = "STATION_LINE_BREAK")]
    StationLineBreak,
    #[serde(rename = "STATION_SURROUNDING_WHITESPACE")]
    StationSurroundingWhitespace,
    #[serde(rename = "STATION_READS_AS_COMMENT")]
    StationReadsAsComment,
    #[serde(rename = "STATION_READS_AS_HEADER")]
    StationReadsAsHeader,
    #[serde(rename = "STATION_READS_AS_COEFFICIENT_ROW")]
    StationReadsAsCoefficientRow,
    /// `row` is the zero-based BLQ row: amplitudes radial, EW, NS, then
    /// phases radial, EW, NS.
    #[serde(rename = "NON_FINITE_COEFFICIENT", rename_all = "camelCase")]
    NonFiniteCoefficient { row: usize, constituent: String },
    #[serde(rename = "COMMENT_LINE_BREAK", rename_all = "camelCase")]
    CommentLineBreak { index: usize },
    #[serde(rename = "NOT_A_COMMENT_LINE", rename_all = "camelCase")]
    NotACommentLine { index: usize },
    #[serde(rename = "COMMENT_PLACEMENT_OUT_OF_RANGE", rename_all = "camelCase")]
    CommentPlacementOutOfRange { index: usize },
    #[serde(rename = "INVALID_HEADER", rename_all = "camelCase")]
    InvalidHeader {
        index: usize,
        header: BlqParseReasonJs,
    },
    #[serde(rename = "AFTER_ROWS_BEFORE_ANOTHER_BLOCK", rename_all = "camelCase")]
    AfterRowsBeforeAnotherBlock { index: usize },
    #[serde(rename = "COMMENTS_OUT_OF_PLACEMENT_ORDER", rename_all = "camelCase")]
    CommentsOutOfPlacementOrder { index: usize },
}

impl From<&BlqWriteErrorKind> for BlqWriteReasonJs {
    fn from(kind: &BlqWriteErrorKind) -> Self {
        match kind {
            BlqWriteErrorKind::EmptyStation => Self::EmptyStation,
            BlqWriteErrorKind::StationLineBreak => Self::StationLineBreak,
            BlqWriteErrorKind::StationSurroundingWhitespace => Self::StationSurroundingWhitespace,
            BlqWriteErrorKind::StationReadsAsComment => Self::StationReadsAsComment,
            BlqWriteErrorKind::StationReadsAsHeader => Self::StationReadsAsHeader,
            BlqWriteErrorKind::StationReadsAsCoefficientRow => Self::StationReadsAsCoefficientRow,
            BlqWriteErrorKind::NonFiniteCoefficient { row, constituent } => {
                Self::NonFiniteCoefficient {
                    row: *row,
                    constituent: constituent.label().to_string(),
                }
            }
            BlqWriteErrorKind::CommentLineBreak { index } => {
                Self::CommentLineBreak { index: *index }
            }
            BlqWriteErrorKind::NotACommentLine { index } => Self::NotACommentLine { index: *index },
            BlqWriteErrorKind::CommentPlacementOutOfRange { index } => {
                Self::CommentPlacementOutOfRange { index: *index }
            }
            BlqWriteErrorKind::InvalidHeader { index, kind } => Self::InvalidHeader {
                index: *index,
                header: kind.into(),
            },
            BlqWriteErrorKind::AfterRowsBeforeAnotherBlock { index } => {
                Self::AfterRowsBeforeAnotherBlock { index: *index }
            }
            BlqWriteErrorKind::CommentsOutOfPlacementOrder { index } => {
                Self::CommentsOutOfPlacementOrder { index: *index }
            }
        }
    }
}

/// The `detail` of a thrown `BlqWriteError`: the reason, the zero-based index
/// of the refused block in the written sequence and the engine's message.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BlqWriteErrorDetailJs {
    #[serde(flatten)]
    reason: BlqWriteReasonJs,
    block: usize,
    message: String,
}

/// A BLQ reader or writer failure as an `Error` named `BlqParseError` or
/// `BlqWriteError` with its typed `detail`. The reader and writer raise only
/// those two kinds; any other tide error keeps the engine's message.
fn blq_error(err: TideError) -> JsValue {
    let message = err.to_string();
    match &err {
        TideError::BlqParse { line, kind } => {
            let detail = BlqParseErrorDetailJs {
                reason: kind.into(),
                line: *line,
                message: message.clone(),
            };
            error_with_detail("BlqParseError", &message, &detail)
        }
        TideError::BlqWrite { block, kind } => {
            let detail = BlqWriteErrorDetailJs {
                reason: kind.into(),
                block: *block,
                message: message.clone(),
            };
            error_with_detail("BlqWriteError", &message, &detail)
        }
        _ => engine_error(message),
    }
}

/// Parse one standard Bos-Scherneck / HARDISP BLQ station block.
///
/// Lines starting with `$`, `#` or `!` are comments; a column-order header
/// (`COLUMN ORDER` followed by the constituent labels, a line of labels only,
/// or a comment in which a word `ORDER` is followed to the end of the line by
/// labels) sets the column order of every later row. Comment and header lines
/// are kept on the block with their placement. Returns an
/// `OceanLoadingBlqBlock`; a block that does not read, or text holding more
/// than one block, throws a `BlqParseError` whose `detail` is a
/// `BlqParseErrorDetail`.
#[wasm_bindgen(
    js_name = parseOceanLoadingBlqBlock,
    unchecked_return_type = "OceanLoadingBlqBlock"
)]
pub fn parse_ocean_loading_blq_block_js(text: &str) -> Result<JsValue, JsValue> {
    let block = parse_ocean_loading_blq_block(text).map_err(blq_error)?;
    to_plain_js(&BlqBlockJs::from(&block), "BLQ block")
}

/// Parse every standard station block of a BLQ file, in file order. A
/// column-order header stays in force across blocks, and lines after the last
/// block's rows are kept on that block as `"afterRows"` comments.
#[wasm_bindgen(
    js_name = parseOceanLoadingBlqBlocks,
    unchecked_return_type = "OceanLoadingBlqBlock[]"
)]
pub fn parse_ocean_loading_blq_blocks_js(text: &str) -> Result<JsValue, JsValue> {
    let blocks = parse_ocean_loading_blq_blocks(text).map_err(blq_error)?;
    let blocks: Vec<BlqBlockJs> = blocks.iter().map(BlqBlockJs::from).collect();
    to_plain_js(&blocks, "BLQ blocks")
}

/// Write one BLQ station block.
///
/// `block` has the shape [`parseOceanLoadingBlqBlock`] returns; `comments` may
/// be omitted. The retained comments are written at their placements, the
/// station line starts in the third column, and each row is written in the
/// column order its retained header declares, or the standard order when none
/// does, so the output reads back to an equal block. A block the reader would
/// not read back unchanged (an empty station or one that would read as a
/// comment, header or row, a non-finite coefficient, a comment that would not
/// read as one at its placement) is refused with a `BlqWriteError` whose
/// `detail` is a `BlqWriteErrorDetail`, never adjusted to fit. A malformed
/// object is a `TypeError`.
#[wasm_bindgen(js_name = writeOceanLoadingBlqBlock)]
pub fn write_ocean_loading_blq_block_js(
    #[wasm_bindgen(unchecked_param_type = "OceanLoadingBlqBlockInput")] block: JsValue,
) -> Result<String, JsValue> {
    let block = blq_block_from_js(&block, "block")?;
    block.to_blq_block().map_err(blq_error)
}

/// Write station blocks as one BLQ file, in order. A column-order header on
/// one block stays in force for the blocks after it, as it does when the file
/// is read, so parsing the output gives back equal blocks. A comment placed
/// after the rows of any block but the last is refused as
/// `AFTER_ROWS_BEFORE_ANOTHER_BLOCK`, since the reader takes it as part of the
/// next block.
#[wasm_bindgen(js_name = writeOceanLoadingBlqBlocks)]
pub fn write_ocean_loading_blq_blocks_js(
    #[wasm_bindgen(unchecked_param_type = "OceanLoadingBlqBlockInput[]")] blocks: JsValue,
) -> Result<String, JsValue> {
    if !js_sys::Array::is_array(&blocks) {
        return Err(type_error("blocks must be an array of BLQ blocks"));
    }
    let blocks = js_sys::Array::from(&blocks)
        .iter()
        .enumerate()
        .map(|(index, block)| blq_block_from_js(&block, &format!("blocks[{index}]")))
        .collect::<Result<Vec<_>, JsValue>>()?;
    write_ocean_loading_blq_blocks(&blocks).map_err(blq_error)
}

// The BLQ block, comment and refusal shapes. `wasm-pack` writes them into both
// `sidereon.d.ts` targets; `types/sidereon-extra.d.ts` re-exports them.
#[wasm_bindgen(typescript_custom_section)]
const TS_BLQ_DEFINITIONS: &str = r#"
/**
 * A comment or column-order header line kept on a BLQ block, exactly as read.
 * `row` is the zero-based coefficient row the line precedes for
 * `"beforeRow"`, and `null` otherwise.
 */
export type OceanLoadingBlqComment =
  | { placement: "beforeStation"; row: null; line: string }
  | { placement: "beforeRow"; row: number; line: string }
  | { placement: "afterRows"; row: null; line: string };

/**
 * One BLQ station block. `amplitudeM` and `phaseDeg` are 3 x 11 grids: rows
 * radial, EW (west positive), NS (south positive); columns M2 S2 N2 K2 K1 O1
 * P1 Q1 Mf Mm Ssa.
 */
export interface OceanLoadingBlqBlock {
  station: string;
  amplitudeM: number[][];
  phaseDeg: number[][];
  comments: OceanLoadingBlqComment[];
}

/** A block to write; `comments` may be omitted, and `row` omitted where null. */
export interface OceanLoadingBlqBlockInput {
  station: string;
  amplitudeM: ArrayLike<number>[];
  phaseDeg: ArrayLike<number>[];
  comments?:
    | (
        | { placement: "beforeStation" | "afterRows"; row?: null; line: string }
        | { placement: "beforeRow"; row: number; line: string }
      )[]
    | null;
}

/** Why a BLQ header or block does not read, without its position. */
export type BlqParseReason =
  | { kind: "EMPTY" }
  | { kind: "MISSING_STATION" }
  | { kind: "MISSING_COEFFICIENT_ROWS"; station: string; expected: number; found: number }
  | { kind: "TOO_MANY_COEFFICIENT_ROWS"; station: string }
  | { kind: "WRONG_COLUMN_COUNT"; expected: number; found: number }
  | { kind: "INVALID_NUMBER"; token: string }
  | { kind: "NON_FINITE_NUMBER"; token: string }
  | { kind: "UNSUPPORTED_CONSTITUENT"; constituent: string }
  | { kind: "DUPLICATE_CONSTITUENT"; constituent: string }
  | { kind: "MULTIPLE_BLOCKS"; found: number };

/**
 * The `detail` of a thrown `BlqParseError`. `line` is one-based, and 0 for a
 * whole-input failure.
 */
export type BlqParseErrorDetail = BlqParseReason & { line: number; message: string };

/**
 * The `detail` of a thrown `BlqWriteError`. `block` is the zero-based index of
 * the refused block in the written sequence; `index` is a comment's index in
 * that block's `comments`; `row` is the zero-based BLQ row (amplitudes radial,
 * EW, NS, then phases).
 */
export type BlqWriteErrorDetail = (
  | { kind: "EMPTY_STATION" }
  | { kind: "STATION_LINE_BREAK" }
  | { kind: "STATION_SURROUNDING_WHITESPACE" }
  | { kind: "STATION_READS_AS_COMMENT" }
  | { kind: "STATION_READS_AS_HEADER" }
  | { kind: "STATION_READS_AS_COEFFICIENT_ROW" }
  | { kind: "NON_FINITE_COEFFICIENT"; row: number; constituent: string }
  | { kind: "COMMENT_LINE_BREAK"; index: number }
  | { kind: "NOT_A_COMMENT_LINE"; index: number }
  | { kind: "COMMENT_PLACEMENT_OUT_OF_RANGE"; index: number }
  | { kind: "INVALID_HEADER"; index: number; header: BlqParseReason }
  | { kind: "AFTER_ROWS_BEFORE_ANOTHER_BLOCK"; index: number }
  | { kind: "COMMENTS_OUT_OF_PLACEMENT_ORDER"; index: number }
) & { block: number; message: string };
"#;

#[wasm_bindgen(typescript_custom_section)]
const TS_STATION_DISPLACEMENT: &str = r#"
/** Request for station displacement in ECEF metres. Ocean grids are 3-by-11 BLQ values. */
export type StationDisplacementRequest = {
  year: number; month: number; day: number; hour: number; minute: number; second: number;
  xpArcsec?: number; ypArcsec?: number; solidEarthTide?: boolean; poleTide?: boolean;
  oceanAmplitudeM?: number[][]; oceanPhaseDeg?: number[][];
  constants?: StationTideConstants; validity?: "strict" | "permissive";
} & (
  | { stationEcefM: [number, number, number]; stationGeodetic?: never }
  | { stationEcefM?: never; stationGeodetic: { latitudeRad: number; longitudeRad: number; heightM: number } }
);
export interface StationDisplacementResult {
  ecefM: [number, number, number];
  solidEarthTideEcefM: [number, number, number] | null;
  poleTideEcefM: [number, number, number] | null;
  oceanLoadingEcefM: [number, number, number] | null;
  degraded: "beforeCoverage" | "afterCoverage" | null;
}
export type StationDisplacementBatchRequest = {
  epochs: Array<{ year: number; month: number; day: number; hour: number; minute: number; second: number; xpArcsec?: number; ypArcsec?: number }>;
  solidEarthTide?: boolean; poleTide?: boolean; oceanAmplitudeM?: number[][]; oceanPhaseDeg?: number[][];
  constants?: StationTideConstants; validity?: "strict" | "permissive";
} & (
  | { stationEcefM: [number, number, number]; stationGeodetic?: never }
  | { stationEcefM?: never; stationGeodetic: { latitudeRad: number; longitudeRad: number; heightM: number } }
);
export type TideErrorDetail =
  | { kind: "INVALID_INPUT"; field: string; reason: TideInputKind; message: string }
  | { kind: "TIME_SCALE"; coverage: { kind: "INVALID_INPUT"; field: string; reason: TideInputKind } | { kind: "OUTSIDE_COVERAGE"; reason: "BEFORE_COVERAGE" | "AFTER_COVERAGE" }; message: string }
  | { kind: "FRAME_TRANSFORM"; source: FrameTransformErrorDetail; message: string }
  | { kind: "SUN_MOON"; source: SunMoonErrorDetail; message: string }
  | { kind: "MISSING_INPUT"; field: string; message: string }
  | { kind: "BLQ_PARSE"; line: number; reason: BlqParseReason; message: string }
  | { kind: "BLQ_WRITE"; block: number; reason: BlqWriteReason; message: string };
export type TideInputKind = "MISSING" | "NON_FINITE" | "NOT_POSITIVE" | "NEGATIVE" | "OUT_OF_RANGE" | "FLOAT_PARSE" | "INT_PARSE" | "INVALID_CIVIL_DATE" | "INVALID_CIVIL_TIME";
export type FrameTransformErrorDetail =
  | { kind: "INVALID_INPUT"; field: string; reason: string }
  | { kind: "UT1_OUTSIDE_COVERAGE"; reason: "BEFORE_COVERAGE" | "AFTER_COVERAGE" };
export type SunMoonErrorDetail =
  | { kind: "INVALID_INPUT"; field: string; reason: string }
  | { kind: "FRAME_TRANSFORM"; source: FrameTransformErrorDetail };
export type BlqWriteReason =
  | { kind: "EMPTY_STATION" } | { kind: "STATION_LINE_BREAK" } | { kind: "STATION_SURROUNDING_WHITESPACE" }
  | { kind: "STATION_READS_AS_COMMENT" } | { kind: "STATION_READS_AS_HEADER" } | { kind: "STATION_READS_AS_COEFFICIENT_ROW" }
  | { kind: "NON_FINITE_COEFFICIENT"; row: number; constituent: string }
  | { kind: "COMMENT_LINE_BREAK" | "NOT_A_COMMENT_LINE" | "COMMENT_PLACEMENT_OUT_OF_RANGE" | "AFTER_ROWS_BEFORE_ANOTHER_BLOCK" | "COMMENTS_OUT_OF_PLACEMENT_ORDER"; index: number }
  | { kind: "INVALID_HEADER"; index: number; header: BlqParseReason };
export type StationDisplacementBatchRow =
  | { index: number; value: StationDisplacementResult; error: null }
  | { index: number; value: null; error: TideErrorDetail };
"#;
