//! GNSS dilution-of-precision binding: scalar DOP from line-of-sight or
//! azimuth/elevation geometry, plus an SP3-sampled DOP series over an epoch grid.
//!
//! All DOP math is `sidereon_core::geometry`; this module reshapes flat
//! `Float64Array` geometry, validates it the way the Python layer does, and
//! packages the engine's scalars.

use std::collections::BTreeSet;
use std::f64::consts::FRAC_PI_2;

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use sidereon_core::geometry::{
    dop as core_dop, dop_at_epoch, dop_series as core_dop_series, dop_with_convention,
    error_ellipse_2x2, line_of_sight_from_az_el_deg, passes as core_passes,
    visibility_series as core_visibility_series, visible as core_visible, Dop as CoreDop,
    DopAtEpoch as CoreDopAtEpoch, DopError, DopOptions, DopWeighting as CoreDopWeighting,
    EnuConvention, ErrorEllipse2 as CoreErrorEllipse2, LineOfSight, VisibilityOptions,
    VisibilityPass as CoreVisibilityPass, VisibilitySeriesPoint as CoreVisibilitySeriesPoint,
    VisibleSatellite as CoreVisibleSatellite, Wgs84Geodetic as CoreWgs84,
};
use sidereon_core::{GnssSatelliteId, GnssSystem};

use crate::error::{engine_error, range_error, type_error};
use crate::marshal::{rows3, same_len, vec3_finite};
use crate::sp3::Sp3;

/// Read a positive integer sampling step from a JS number. Exposing the param as
/// `u64` makes wasm-bindgen marshal it as a JS `BigInt`, so an ordinary numeric
/// `stepSeconds` argument throws in the ABI before reaching Rust validation.
/// Taking `f64` keeps the JS-side type a plain number; this guards positivity and
/// integrality, then casts for the core call.
fn step_seconds_u64(value: f64) -> Result<u64, JsValue> {
    if !value.is_finite() || value <= 0.0 || value.fract() != 0.0 || value > u64::MAX as f64 {
        return Err(range_error("stepSeconds must be a positive integer"));
    }
    Ok(value as u64)
}

fn dop_err(err: DopError) -> JsValue {
    match err {
        DopError::InvalidInput { field, reason } => {
            range_error(&format!("invalid DOP input {field}: {reason}"))
        }
        DopError::TooFewSatellites => type_error("at least four geometry rows are required"),
        DopError::Singular => engine_error("singular DOP geometry"),
    }
}

/// WGS84 receiver geodetic coordinates (radians, metres) for DOP.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct Wgs84Geodetic {
    lat_rad: f64,
    lon_rad: f64,
    height_m: f64,
}

#[wasm_bindgen]
impl Wgs84Geodetic {
    /// Build a WGS84 geodetic coordinate. `heightM` defaults to 0. Throws a
    /// `RangeError` on a non-finite field or latitude outside `[-pi/2, pi/2]`.
    #[wasm_bindgen(constructor)]
    pub fn new(
        lat_rad: f64,
        lon_rad: f64,
        height_m: Option<f64>,
    ) -> Result<Wgs84Geodetic, JsValue> {
        let height_m = height_m.unwrap_or(0.0);
        if !lat_rad.is_finite() {
            return Err(range_error("latRad must be finite"));
        }
        if !(-FRAC_PI_2..=FRAC_PI_2).contains(&lat_rad) {
            return Err(range_error("latRad must be in [-pi/2, pi/2]"));
        }
        if !lon_rad.is_finite() {
            return Err(range_error("lonRad must be finite"));
        }
        if !height_m.is_finite() {
            return Err(range_error("heightM must be finite"));
        }
        Ok(Wgs84Geodetic {
            lat_rad,
            lon_rad,
            height_m,
        })
    }

    /// Geodetic latitude, radians.
    #[wasm_bindgen(getter, js_name = latRad)]
    pub fn lat_rad(&self) -> f64 {
        self.lat_rad
    }

    /// Geodetic longitude, radians east.
    #[wasm_bindgen(getter, js_name = lonRad)]
    pub fn lon_rad(&self) -> f64 {
        self.lon_rad
    }

    /// Ellipsoidal height above WGS84, metres.
    #[wasm_bindgen(getter, js_name = heightM)]
    pub fn height_m(&self) -> f64 {
        self.height_m
    }
}

impl Wgs84Geodetic {
    fn to_core(self) -> Result<CoreWgs84, JsValue> {
        CoreWgs84::new(self.lat_rad, self.lon_rad, self.height_m)
            .map_err(|e| range_error(&e.to_string()))
    }
}

/// GNSS dilution-of-precision scalars.
#[wasm_bindgen]
#[derive(Clone)]
pub struct Dop {
    gdop: f64,
    pdop: f64,
    hdop: f64,
    vdop: f64,
    tdop: f64,
    system_tdops: Vec<f64>,
}

impl From<CoreDop> for Dop {
    fn from(d: CoreDop) -> Self {
        Self {
            gdop: d.gdop,
            pdop: d.pdop,
            hdop: d.hdop,
            vdop: d.vdop,
            tdop: d.tdop,
            system_tdops: d.system_tdops.iter().map(|(_, tdop)| *tdop).collect(),
        }
    }
}

#[wasm_bindgen]
impl Dop {
    /// Geometric DOP.
    #[wasm_bindgen(getter)]
    pub fn gdop(&self) -> f64 {
        self.gdop
    }

    /// Position DOP.
    #[wasm_bindgen(getter)]
    pub fn pdop(&self) -> f64 {
        self.pdop
    }

    /// Horizontal DOP.
    #[wasm_bindgen(getter)]
    pub fn hdop(&self) -> f64 {
        self.hdop
    }

    /// Vertical DOP.
    #[wasm_bindgen(getter)]
    pub fn vdop(&self) -> f64 {
        self.vdop
    }

    /// Time DOP.
    #[wasm_bindgen(getter)]
    pub fn tdop(&self) -> f64 {
        self.tdop
    }

    /// Per-clock-column time DOP: entry `i` is the cofactor standard deviation
    /// of the `i`-th clock state, indexed by clock column rather than tagged by
    /// constellation. The geometry-only `dop` path (`fromAzEl`,
    /// `fromLineOfSight`, `gnssDop`) carries no constellation context, so it
    /// leaves this **empty**; read the lone clock's value off [`tdop`](Self::tdop).
    /// Per-system entries are only populated on the multi-clock SPP path; pair
    /// them with that solve's system ordering via `SppSolution.systemTdops`.
    #[wasm_bindgen(getter, js_name = systemTdops)]
    pub fn system_tdops(&self) -> Vec<f64> {
        self.system_tdops.clone()
    }

    /// Compute DOP from ECEF line-of-sight unit rows.
    ///
    /// `lineOfSight` is a flat row-major `(n, 3)` `Float64Array` of ECEF unit
    /// vectors, `receiver` is WGS84 geodetic, optional `weights` is a positive
    /// `Float64Array` of length `n`. At least four rows are required.
    #[wasm_bindgen(js_name = fromLineOfSight)]
    pub fn from_line_of_sight(
        line_of_sight: &[f64],
        receiver: &Wgs84Geodetic,
        weights: Option<Vec<f64>>,
    ) -> Result<Dop, JsValue> {
        let rows = rows3("lineOfSight", line_of_sight, true)?;
        if rows.is_empty() {
            return Err(type_error("lineOfSight must not be empty"));
        }
        let weights = weights_or_unit(weights, rows.len())?;
        dop_from_rows(&rows, &weights, receiver)
    }

    /// Compute DOP from topocentric azimuth/elevation rows.
    ///
    /// `azimuthDeg` and `elevationDeg` are `Float64Array`s of length `n`
    /// (azimuth clockwise from geodetic north). `receiver` defines the local ENU
    /// frame. Optional `weights` is a positive `Float64Array` of length `n`.
    #[wasm_bindgen(js_name = fromAzEl)]
    pub fn from_az_el(
        azimuth_deg: &[f64],
        elevation_deg: &[f64],
        receiver: &Wgs84Geodetic,
        weights: Option<Vec<f64>>,
    ) -> Result<Dop, JsValue> {
        if azimuth_deg.is_empty() {
            return Err(type_error("azimuthDeg must not be empty"));
        }
        same_len(
            "azimuthDeg",
            azimuth_deg.len(),
            "elevationDeg",
            elevation_deg.len(),
        )?;
        let core_receiver = receiver.to_core()?;
        let mut los = Vec::with_capacity(azimuth_deg.len());
        for (i, (&az, &el)) in azimuth_deg.iter().zip(elevation_deg.iter()).enumerate() {
            if !az.is_finite() {
                return Err(range_error(&format!("azimuthDeg[{i}] must be finite")));
            }
            if !el.is_finite() || !(-90.0..=90.0).contains(&el) {
                return Err(range_error(&format!(
                    "elevationDeg[{i}] must be finite and in [-90, 90]"
                )));
            }
            los.push(line_of_sight_from_az_el_deg(az, el, core_receiver).map_err(dop_err)?);
        }
        let weights = weights_or_unit(weights, los.len())?;
        let dop = core_dop(&los, &weights, core_receiver).map_err(dop_err)?;
        Ok(dop.into())
    }
}

fn read_positive_weights(weights: &[f64]) -> Result<Vec<f64>, JsValue> {
    if weights.is_empty() {
        return Err(type_error("weights must not be empty"));
    }
    for (i, &w) in weights.iter().enumerate() {
        if !w.is_finite() || w <= 0.0 {
            return Err(range_error(&format!(
                "weights[{i}] must be finite and positive"
            )));
        }
    }
    Ok(weights.to_vec())
}

fn weights_or_unit(weights: Option<Vec<f64>>, expected: usize) -> Result<Vec<f64>, JsValue> {
    match weights {
        Some(w) => {
            let values = read_positive_weights(&w)?;
            same_len("geometry", expected, "weights", values.len())?;
            Ok(values)
        }
        None => Ok(vec![1.0; expected]),
    }
}

fn dop_from_rows(
    rows: &[[f64; 3]],
    weights: &[f64],
    receiver: &Wgs84Geodetic,
) -> Result<Dop, JsValue> {
    let los: Vec<LineOfSight> = rows
        .iter()
        .map(|r| LineOfSight::new(r[0], r[1], r[2]))
        .collect();
    let dop = core_dop(&los, weights, receiver.to_core()?).map_err(dop_err)?;
    Ok(dop.into())
}

/// GNSS dilution of precision from ECEF line-of-sight unit rows and weights.
///
/// `lineOfSight` is a flat row-major `(n, 3)` `Float64Array`; `weights` is a
/// positive `Float64Array` of length `n`.
#[wasm_bindgen(js_name = gnssDop)]
pub fn gnss_dop(
    line_of_sight: &[f64],
    weights: &[f64],
    receiver: &Wgs84Geodetic,
) -> Result<Dop, JsValue> {
    let rows = rows3("lineOfSight", line_of_sight, true)?;
    if rows.is_empty() {
        return Err(type_error("lineOfSight must not be empty"));
    }
    let weights = read_positive_weights(weights)?;
    same_len("lineOfSight", rows.len(), "weights", weights.len())?;
    dop_from_rows(&rows, &weights, receiver)
}

fn parse_convention(value: &str) -> Result<EnuConvention, JsValue> {
    match value {
        "geodeticNormal" => Ok(EnuConvention::GeodeticNormal),
        "geocentricRadial" => Ok(EnuConvention::GeocentricRadial),
        other => Err(type_error(&format!(
            "unknown ENU convention {other:?}; expected \"geodeticNormal\" or \"geocentricRadial\""
        ))),
    }
}

/// GNSS dilution of precision with an explicit ENU convention for the
/// horizontal/vertical split.
///
/// `lineOfSight` is a flat row-major `(n, 3)` `Float64Array` of ECEF unit
/// vectors, `weights` a positive `Float64Array` of length `n`, and `convention`
/// is `"geodeticNormal"` (the GNSS-standard default, matching [`gnssDop`]) or
/// `"geocentricRadial"` (radial up). Only HDOP/VDOP differ between conventions
/// (by ~`1e-3` relative); GDOP/PDOP/TDOP are identical. Delegates to
/// `sidereon_core::geometry::dop_with_convention`.
#[wasm_bindgen(js_name = dopWithConvention)]
pub fn dop_with_convention_js(
    line_of_sight: &[f64],
    weights: &[f64],
    receiver: &Wgs84Geodetic,
    convention: &str,
) -> Result<Dop, JsValue> {
    let rows = rows3("lineOfSight", line_of_sight, true)?;
    if rows.is_empty() {
        return Err(type_error("lineOfSight must not be empty"));
    }
    let weights = read_positive_weights(weights)?;
    same_len("lineOfSight", rows.len(), "weights", weights.len())?;
    let convention = parse_convention(convention)?;
    let los: Vec<LineOfSight> = rows
        .iter()
        .map(|r| LineOfSight::new(r[0], r[1], r[2]))
        .collect();
    let dop =
        dop_with_convention(&los, &weights, receiver.to_core()?, convention).map_err(dop_err)?;
    Ok(dop.into())
}

/// A confidence ellipse from a 2x2 covariance block: semi-axes scaled by the
/// two-degree-of-freedom chi-square quantile `-2 ln(1 - confidence)`.
#[wasm_bindgen]
pub struct ErrorEllipse2 {
    confidence: f64,
    chi_square_scale: f64,
    semi_major: f64,
    semi_minor: f64,
    orientation_rad: f64,
}

impl From<CoreErrorEllipse2> for ErrorEllipse2 {
    fn from(e: CoreErrorEllipse2) -> Self {
        Self {
            confidence: e.confidence,
            chi_square_scale: e.chi_square_scale,
            semi_major: e.semi_major,
            semi_minor: e.semi_minor,
            orientation_rad: e.orientation_rad,
        }
    }
}

#[wasm_bindgen]
impl ErrorEllipse2 {
    /// Requested confidence probability in `(0, 1)`.
    #[wasm_bindgen(getter)]
    pub fn confidence(&self) -> f64 {
        self.confidence
    }

    /// Two-degree-of-freedom chi-square scale `-2 ln(1 - confidence)`.
    #[wasm_bindgen(getter, js_name = chiSquareScale)]
    pub fn chi_square_scale(&self) -> f64 {
        self.chi_square_scale
    }

    /// Semi-major axis length (same unit as the square root of the covariance).
    #[wasm_bindgen(getter, js_name = semiMajor)]
    pub fn semi_major(&self) -> f64 {
        self.semi_major
    }

    /// Semi-minor axis length.
    #[wasm_bindgen(getter, js_name = semiMinor)]
    pub fn semi_minor(&self) -> f64 {
        self.semi_minor
    }

    /// Semi-major-axis orientation in radians, from the first (row/col 0) axis
    /// toward the second (row/col 1) axis.
    #[wasm_bindgen(getter, js_name = orientationRad)]
    pub fn orientation_rad(&self) -> f64 {
        self.orientation_rad
    }
}

/// Confidence ellipse from an arbitrary 2x2 covariance block.
///
/// `covariance` is a flat row-major length-4 `Float64Array` (`[c00, c01, c10,
/// c11]`); `confidence` is in `(0, 1)`. The semi-axes are the eigenvalues of the
/// symmetrized block scaled by the chi-square(2) quantile. Throws a `RangeError`
/// for a non-positive-semidefinite block or an out-of-range confidence.
/// Delegates to `sidereon_core::geometry::error_ellipse_2x2`.
#[wasm_bindgen(js_name = errorEllipse2)]
pub fn error_ellipse_2(covariance: &[f64], confidence: f64) -> Result<ErrorEllipse2, JsValue> {
    if covariance.len() != 4 {
        return Err(type_error(&format!(
            "covariance must have length 4 (flat row-major 2-by-2), got {}",
            covariance.len()
        )));
    }
    for (i, &v) in covariance.iter().enumerate() {
        if !v.is_finite() {
            return Err(range_error(&format!("covariance[{i}] must be finite")));
        }
    }
    let block = [
        [covariance[0], covariance[1]],
        [covariance[2], covariance[3]],
    ];
    let ellipse = error_ellipse_2x2(block, confidence).map_err(dop_err)?;
    Ok(ellipse.into())
}

fn parse_system(value: &str) -> Result<GnssSystem, JsValue> {
    match value.trim().to_ascii_uppercase().as_str() {
        "G" | "GPS" => Ok(GnssSystem::Gps),
        "R" | "GLO" | "GLONASS" => Ok(GnssSystem::Glonass),
        "E" | "GAL" | "GALILEO" => Ok(GnssSystem::Galileo),
        "C" | "BDS" | "BEIDOU" => Ok(GnssSystem::BeiDou),
        "J" | "QZSS" => Ok(GnssSystem::Qzss),
        "I" | "IRNSS" | "NAVIC" => Ok(GnssSystem::Navic),
        "S" | "SBAS" => Ok(GnssSystem::Sbas),
        other => Err(type_error(&format!(
            "unknown GNSS system {other:?}; expected one of G, R, E, C, J, I, S"
        ))),
    }
}

fn parse_weighting(value: &str) -> Result<CoreDopWeighting, JsValue> {
    match value {
        "unit" => Ok(CoreDopWeighting::Unit),
        "elevation" => Ok(CoreDopWeighting::Elevation),
        other => Err(type_error(&format!(
            "unknown DOP weighting {other:?}; expected \"unit\" or \"elevation\""
        ))),
    }
}

/// Options object for [`gnssDopSeries`].
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct DopSeriesOptions {
    satellites: Option<Vec<String>>,
    elevation_mask_deg: Option<f64>,
    systems: Option<Vec<String>>,
    weighting: Option<String>,
    light_time: Option<bool>,
}

/// DOP values sampled from an SP3 precise product over an epoch grid. Only
/// samples with finite DOP are retained.
#[wasm_bindgen]
pub struct DopSeries {
    step_index: Vec<i32>,
    j2000_seconds: Vec<f64>,
    gdop: Vec<f64>,
    pdop: Vec<f64>,
    hdop: Vec<f64>,
    vdop: Vec<f64>,
    tdop: Vec<f64>,
    satellite_count: Vec<i32>,
    satellites: Vec<Vec<String>>,
}

#[wasm_bindgen]
impl DopSeries {
    /// Input epoch indices for finite DOP samples, `Int32Array`.
    #[wasm_bindgen(getter, js_name = stepIndex)]
    pub fn step_index(&self) -> Vec<i32> {
        self.step_index.clone()
    }

    /// Successful sample epochs, seconds since J2000, `Float64Array`.
    #[wasm_bindgen(getter, js_name = j2000Seconds)]
    pub fn j2000_seconds(&self) -> Vec<f64> {
        self.j2000_seconds.clone()
    }

    /// Geometric DOP samples, `Float64Array`.
    #[wasm_bindgen(getter)]
    pub fn gdop(&self) -> Vec<f64> {
        self.gdop.clone()
    }

    /// Position DOP samples, `Float64Array`.
    #[wasm_bindgen(getter)]
    pub fn pdop(&self) -> Vec<f64> {
        self.pdop.clone()
    }

    /// Horizontal DOP samples, `Float64Array`.
    #[wasm_bindgen(getter)]
    pub fn hdop(&self) -> Vec<f64> {
        self.hdop.clone()
    }

    /// Vertical DOP samples, `Float64Array`.
    #[wasm_bindgen(getter)]
    pub fn vdop(&self) -> Vec<f64> {
        self.vdop.clone()
    }

    /// Time DOP samples, `Float64Array`.
    #[wasm_bindgen(getter)]
    pub fn tdop(&self) -> Vec<f64> {
        self.tdop.clone()
    }

    /// Number of satellites used at each finite sample, `Int32Array`.
    #[wasm_bindgen(getter, js_name = satelliteCount)]
    pub fn satellite_count(&self) -> Vec<i32> {
        self.satellite_count.clone()
    }

    /// Satellite tokens used at finite sample `index`, ascending in elevation.
    #[wasm_bindgen(js_name = satellitesAt)]
    pub fn satellites_at(&self, index: usize) -> Option<Vec<String>> {
        self.satellites.get(index).cloned()
    }

    /// Number of finite DOP samples.
    #[wasm_bindgen(getter, js_name = epochCount)]
    pub fn epoch_count(&self) -> usize {
        self.gdop.len()
    }
}

/// Parse the optional `gnssDopSeries` / `gnssDopAtEpoch` / `gnssDopSeriesWindow`
/// options object, defaulting an absent/null value to the engine defaults.
fn parse_dop_series_options(options: JsValue) -> Result<DopSeriesOptions, JsValue> {
    if options.is_undefined() || options.is_null() {
        Ok(DopSeriesOptions::default())
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid DOP options: {e}")))
    }
}

/// Build the explicit-satellite filter and core `DopOptions` from the parsed
/// options, applying the engine defaults the same way for every DOP entry point.
#[allow(clippy::type_complexity)]
fn dop_options_from(
    opts: &DopSeriesOptions,
) -> Result<(Option<Vec<GnssSatelliteId>>, DopOptions), JsValue> {
    let elevation_mask_deg = opts
        .elevation_mask_deg
        .unwrap_or(VisibilityOptions::default().elevation_mask_deg);
    if !elevation_mask_deg.is_finite() {
        return Err(range_error("elevationMaskDeg must be finite"));
    }

    let explicit_satellites = match &opts.satellites {
        Some(values) => {
            if values.is_empty() {
                return Err(type_error("satellites must not be empty"));
            }
            Some(
                values
                    .iter()
                    .map(|v| {
                        v.parse::<GnssSatelliteId>()
                            .map_err(|e| type_error(&format!("invalid satellite token {v:?}: {e}")))
                    })
                    .collect::<Result<Vec<_>, JsValue>>()?,
            )
        }
        None => None,
    };

    let systems = match &opts.systems {
        Some(values) => {
            if values.is_empty() {
                return Err(type_error("systems must not be empty"));
            }
            Some(
                values
                    .iter()
                    .map(|v| parse_system(v))
                    .collect::<Result<BTreeSet<_>, JsValue>>()?,
            )
        }
        None => None,
    };

    let weighting = parse_weighting(opts.weighting.as_deref().unwrap_or("unit"))?;
    let dop_options = DopOptions {
        visibility: VisibilityOptions {
            elevation_mask_deg,
            systems,
        },
        weighting,
        light_time: opts.light_time.unwrap_or(false),
    };
    Ok((explicit_satellites, dop_options))
}

/// Sample SP3-derived GNSS DOP over a J2000 epoch grid.
///
/// `stationEcefM` is an ECEF metre vector (length 3); `j2000Seconds` is a
/// `Float64Array` grid in the SP3 product time scale. Samples with too few
/// satellites or singular geometry are omitted. `options` is
/// `{ satellites?, elevationMaskDeg?=5, systems?, weighting?="unit", lightTime?=false }`.
#[wasm_bindgen(js_name = gnssDopSeries)]
pub fn gnss_dop_series(
    sp3: &Sp3,
    station_ecef_m: &[f64],
    j2000_seconds: &[f64],
    options: JsValue,
) -> Result<DopSeries, JsValue> {
    let opts = parse_dop_series_options(options)?;

    if j2000_seconds.is_empty() {
        return Err(type_error("j2000Seconds must not be empty"));
    }
    for (i, &v) in j2000_seconds.iter().enumerate() {
        if !v.is_finite() {
            return Err(range_error(&format!("j2000Seconds[{i}] must be finite")));
        }
    }
    let station = vec3_finite("stationEcefM", station_ecef_m)?;
    let (explicit_satellites, dop_options) = dop_options_from(&opts)?;

    let mut out = DopSeries {
        step_index: Vec::new(),
        j2000_seconds: Vec::new(),
        gdop: Vec::new(),
        pdop: Vec::new(),
        hdop: Vec::new(),
        vdop: Vec::new(),
        tdop: Vec::new(),
        satellite_count: Vec::new(),
        satellites: Vec::new(),
    };
    let all_satellites = sp3.inner.satellites();
    for (index, &epoch) in j2000_seconds.iter().enumerate() {
        let Ok(geometry) = dop_at_epoch(
            &sp3.inner,
            all_satellites,
            explicit_satellites.as_deref(),
            station,
            epoch,
            &dop_options,
        ) else {
            continue;
        };
        out.step_index.push(index as i32);
        out.j2000_seconds.push(epoch);
        out.gdop.push(geometry.dop.gdop);
        out.pdop.push(geometry.dop.pdop);
        out.hdop.push(geometry.dop.hdop);
        out.vdop.push(geometry.dop.vdop);
        out.tdop.push(geometry.dop.tdop);
        out.satellite_count.push(geometry.satellites.len() as i32);
        out.satellites.push(
            geometry
                .satellites
                .iter()
                .map(ToString::to_string)
                .collect(),
        );
    }
    Ok(out)
}

/// Exact DOP at one epoch: the scalars plus the satellites that contributed
/// rows, from `gnssDopAtEpoch`.
#[wasm_bindgen]
pub struct DopGeometry {
    gdop: f64,
    pdop: f64,
    hdop: f64,
    vdop: f64,
    tdop: f64,
    satellites: Vec<String>,
}

impl From<CoreDopAtEpoch> for DopGeometry {
    fn from(g: CoreDopAtEpoch) -> Self {
        Self {
            gdop: g.dop.gdop,
            pdop: g.dop.pdop,
            hdop: g.dop.hdop,
            vdop: g.dop.vdop,
            tdop: g.dop.tdop,
            satellites: g.satellites.iter().map(ToString::to_string).collect(),
        }
    }
}

#[wasm_bindgen]
impl DopGeometry {
    /// Geometric DOP.
    #[wasm_bindgen(getter)]
    pub fn gdop(&self) -> f64 {
        self.gdop
    }

    /// Position DOP.
    #[wasm_bindgen(getter)]
    pub fn pdop(&self) -> f64 {
        self.pdop
    }

    /// Horizontal DOP.
    #[wasm_bindgen(getter)]
    pub fn hdop(&self) -> f64 {
        self.hdop
    }

    /// Vertical DOP.
    #[wasm_bindgen(getter)]
    pub fn vdop(&self) -> f64 {
        self.vdop
    }

    /// Time DOP.
    #[wasm_bindgen(getter)]
    pub fn tdop(&self) -> f64 {
        self.tdop
    }

    /// Satellite tokens that contributed line-of-sight rows.
    #[wasm_bindgen(getter)]
    pub fn satellites(&self) -> Vec<String> {
        self.satellites.clone()
    }
}

/// Compute exact GNSS DOP at a single epoch from an SP3 precise product.
///
/// `stationEcefM` is an ECEF metre vector (length 3); `j2000Seconds` the receive
/// time as continuous seconds since J2000 in the SP3 product time scale.
/// `options` is `{ satellites?, elevationMaskDeg?=5, systems?, weighting?="unit",
/// lightTime?=false }`. Delegates to `sidereon_core::geometry::dop_at_epoch`.
#[wasm_bindgen(js_name = gnssDopAtEpoch)]
pub fn gnss_dop_at_epoch(
    sp3: &Sp3,
    station_ecef_m: &[f64],
    j2000_seconds: f64,
    options: JsValue,
) -> Result<DopGeometry, JsValue> {
    let station = vec3_finite("stationEcefM", station_ecef_m)?;
    if !j2000_seconds.is_finite() {
        return Err(range_error("j2000Seconds must be finite"));
    }
    let opts = parse_dop_series_options(options)?;
    let (explicit_satellites, dop_options) = dop_options_from(&opts)?;
    let geometry = dop_at_epoch(
        &sp3.inner,
        sp3.inner.satellites(),
        explicit_satellites.as_deref(),
        station,
        j2000_seconds,
        &dop_options,
    )
    .map_err(dop_err)?;
    Ok(geometry.into())
}

/// One sampled exact-DOP point over a uniform window, from `gnssDopSeriesWindow`.
#[wasm_bindgen]
pub struct DopSeriesSample {
    step_index: usize,
    gdop: f64,
    pdop: f64,
    hdop: f64,
    vdop: f64,
    tdop: f64,
    satellites: Vec<String>,
}

#[wasm_bindgen]
impl DopSeriesSample {
    /// Zero-based sample index from the window start.
    #[wasm_bindgen(getter, js_name = stepIndex)]
    pub fn step_index(&self) -> usize {
        self.step_index
    }

    /// Geometric DOP.
    #[wasm_bindgen(getter)]
    pub fn gdop(&self) -> f64 {
        self.gdop
    }

    /// Position DOP.
    #[wasm_bindgen(getter)]
    pub fn pdop(&self) -> f64 {
        self.pdop
    }

    /// Horizontal DOP.
    #[wasm_bindgen(getter)]
    pub fn hdop(&self) -> f64 {
        self.hdop
    }

    /// Vertical DOP.
    #[wasm_bindgen(getter)]
    pub fn vdop(&self) -> f64 {
        self.vdop
    }

    /// Time DOP.
    #[wasm_bindgen(getter)]
    pub fn tdop(&self) -> f64 {
        self.tdop
    }

    /// Satellite tokens that contributed rows at this sample.
    #[wasm_bindgen(getter)]
    pub fn satellites(&self) -> Vec<String> {
        self.satellites.clone()
    }
}

/// Sample exact GNSS DOP over an inclusive uniform window `[startJ2000S,
/// endJ2000S]` at `stepSeconds` spacing, skipping singular or underdetermined
/// samples. `options` is the same shape as [`gnssDopAtEpoch`]. Delegates to
/// `sidereon_core::geometry::dop_series`.
#[wasm_bindgen(js_name = gnssDopSeriesWindow)]
pub fn gnss_dop_series_window(
    sp3: &Sp3,
    station_ecef_m: &[f64],
    start_j2000_s: f64,
    end_j2000_s: f64,
    step_seconds: f64,
    options: JsValue,
) -> Result<Vec<DopSeriesSample>, JsValue> {
    let station = vec3_finite("stationEcefM", station_ecef_m)?;
    if !start_j2000_s.is_finite() || !end_j2000_s.is_finite() {
        return Err(range_error("window epochs must be finite"));
    }
    let step_seconds = step_seconds_u64(step_seconds)?;
    let opts = parse_dop_series_options(options)?;
    let (explicit_satellites, dop_options) = dop_options_from(&opts)?;
    let points = core_dop_series(
        &sp3.inner,
        sp3.inner.satellites(),
        explicit_satellites.as_deref(),
        station,
        (start_j2000_s, end_j2000_s),
        step_seconds,
        &dop_options,
    )
    .map_err(dop_err)?;
    Ok(points
        .into_iter()
        .map(|point| DopSeriesSample {
            step_index: point.step_index,
            gdop: point.geometry.dop.gdop,
            pdop: point.geometry.dop.pdop,
            hdop: point.geometry.dop.hdop,
            vdop: point.geometry.dop.vdop,
            tdop: point.geometry.dop.tdop,
            satellites: point
                .geometry
                .satellites
                .iter()
                .map(ToString::to_string)
                .collect(),
        })
        .collect())
}

// ---- SP3-backed visibility geometry -----------------------------------------
//
// Thin wrappers over `sidereon_core::geometry::{visible, visibility_series,
// passes}`. The visibility test, sampling, and pass construction all live in the
// crate; this layer only reshapes the SP3 handle, receiver, window, and filters
// and re-encodes the typed rows. Mirrors the Elixir `Sidereon.Geometry`
// SP3-backed surface.

/// Visibility filters: optional elevation mask (degrees) and constellation
/// allow-list. Both fields are optional; omitting `systems` admits every
/// constellation in the SP3 product.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct VisibilityOptionsInput {
    elevation_mask_deg: Option<f64>,
    systems: Option<Vec<String>>,
}

impl VisibilityOptionsInput {
    fn to_core(&self) -> Result<VisibilityOptions, JsValue> {
        let core_default = VisibilityOptions::default();
        let elevation_mask_deg = self
            .elevation_mask_deg
            .unwrap_or(core_default.elevation_mask_deg);
        if !elevation_mask_deg.is_finite() {
            return Err(range_error("elevationMaskDeg must be finite"));
        }
        let systems = match &self.systems {
            Some(values) => {
                if values.is_empty() {
                    return Err(type_error("systems must not be empty"));
                }
                Some(
                    values
                        .iter()
                        .map(|v| parse_system(v))
                        .collect::<Result<BTreeSet<_>, JsValue>>()?,
                )
            }
            None => None,
        };
        Ok(VisibilityOptions {
            elevation_mask_deg,
            systems,
        })
    }
}

fn visibility_options(options: JsValue) -> Result<VisibilityOptions, JsValue> {
    let input: VisibilityOptionsInput = if options.is_undefined() || options.is_null() {
        VisibilityOptionsInput::default()
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid visibility options: {e}")))?
    };
    input.to_core()
}

/// One satellite above the elevation mask at a single epoch, from `gnssVisible`.
#[wasm_bindgen]
pub struct GnssVisibleSatellite {
    satellite: String,
    elevation_deg: f64,
    azimuth_deg: f64,
}

#[wasm_bindgen]
impl GnssVisibleSatellite {
    /// Satellite token, e.g. `"G05"`.
    #[wasm_bindgen(getter)]
    pub fn satellite(&self) -> String {
        self.satellite.clone()
    }

    /// Topocentric elevation, degrees.
    #[wasm_bindgen(getter, js_name = elevationDeg)]
    pub fn elevation_deg(&self) -> f64 {
        self.elevation_deg
    }

    /// Topocentric azimuth in `[0, 360)`, degrees.
    #[wasm_bindgen(getter, js_name = azimuthDeg)]
    pub fn azimuth_deg(&self) -> f64 {
        self.azimuth_deg
    }
}

impl From<&CoreVisibleSatellite> for GnssVisibleSatellite {
    fn from(row: &CoreVisibleSatellite) -> Self {
        Self {
            satellite: row.satellite.to_string(),
            elevation_deg: row.elevation_deg,
            azimuth_deg: row.azimuth_deg,
        }
    }
}

/// Satellites visible above the elevation mask from an SP3 product at one
/// receive epoch. `stationEcefM` is a length-3 `Float64Array` (ECEF metres),
/// `j2000Seconds` the receive time as continuous seconds since J2000.
/// Delegates to `sidereon_core::geometry::visible`.
#[wasm_bindgen(js_name = gnssVisible)]
pub fn gnss_visible(
    sp3: &Sp3,
    station_ecef_m: &[f64],
    j2000_seconds: f64,
    options: JsValue,
) -> Result<Vec<GnssVisibleSatellite>, JsValue> {
    let station = vec3_finite("stationEcefM", station_ecef_m)?;
    if !j2000_seconds.is_finite() {
        return Err(range_error("j2000Seconds must be finite"));
    }
    let opts = visibility_options(options)?;
    let rows = core_visible(
        &sp3.inner,
        sp3.inner.satellites(),
        station,
        j2000_seconds,
        &opts,
    )
    .map_err(dop_err)?;
    Ok(rows.iter().map(GnssVisibleSatellite::from).collect())
}

/// Per-epoch visible-satellite count over a sampled window, from
/// `gnssVisibilitySeries`.
#[wasm_bindgen]
pub struct GnssVisibilityCount {
    step_index: usize,
    n_visible: usize,
}

#[wasm_bindgen]
impl GnssVisibilityCount {
    /// Zero-based sample index from the window start.
    #[wasm_bindgen(getter, js_name = stepIndex)]
    pub fn step_index(&self) -> usize {
        self.step_index
    }

    /// Number of satellites visible at this sample.
    #[wasm_bindgen(getter, js_name = nVisible)]
    pub fn n_visible(&self) -> usize {
        self.n_visible
    }
}

impl From<&CoreVisibilitySeriesPoint> for GnssVisibilityCount {
    fn from(point: &CoreVisibilitySeriesPoint) -> Self {
        Self {
            step_index: point.step_index,
            n_visible: point.n_visible,
        }
    }
}

/// Visible-satellite counts over `[startJ2000S, endJ2000S]` sampled every
/// `stepSeconds`. Delegates to `sidereon_core::geometry::visibility_series`.
#[wasm_bindgen(js_name = gnssVisibilitySeries)]
pub fn gnss_visibility_series(
    sp3: &Sp3,
    station_ecef_m: &[f64],
    start_j2000_s: f64,
    end_j2000_s: f64,
    step_seconds: f64,
    options: JsValue,
) -> Result<Vec<GnssVisibilityCount>, JsValue> {
    let station = vec3_finite("stationEcefM", station_ecef_m)?;
    if !start_j2000_s.is_finite() || !end_j2000_s.is_finite() {
        return Err(range_error("window epochs must be finite"));
    }
    let step_seconds = step_seconds_u64(step_seconds)?;
    let opts = visibility_options(options)?;
    let points = core_visibility_series(
        &sp3.inner,
        sp3.inner.satellites(),
        station,
        (start_j2000_s, end_j2000_s),
        step_seconds,
        &opts,
    )
    .map_err(dop_err)?;
    Ok(points.iter().map(GnssVisibilityCount::from).collect())
}

/// One sampled rise/set/peak visibility pass, from `gnssPasses`.
#[wasm_bindgen]
pub struct GnssPass {
    satellite: String,
    rise_step_index: usize,
    set_step_index: usize,
    peak_elevation_deg: f64,
    peak_step_index: usize,
}

#[wasm_bindgen]
impl GnssPass {
    /// Satellite token, e.g. `"G05"`.
    #[wasm_bindgen(getter)]
    pub fn satellite(&self) -> String {
        self.satellite.clone()
    }

    /// Zero-based sample index of the first above-mask sample.
    #[wasm_bindgen(getter, js_name = riseStepIndex)]
    pub fn rise_step_index(&self) -> usize {
        self.rise_step_index
    }

    /// Zero-based sample index of the last above-mask sample.
    #[wasm_bindgen(getter, js_name = setStepIndex)]
    pub fn set_step_index(&self) -> usize {
        self.set_step_index
    }

    /// Maximum sampled elevation in the pass, degrees.
    #[wasm_bindgen(getter, js_name = peakElevationDeg)]
    pub fn peak_elevation_deg(&self) -> f64 {
        self.peak_elevation_deg
    }

    /// Zero-based sample index of the maximum sampled elevation.
    #[wasm_bindgen(getter, js_name = peakStepIndex)]
    pub fn peak_step_index(&self) -> usize {
        self.peak_step_index
    }
}

impl From<&CoreVisibilityPass> for GnssPass {
    fn from(pass: &CoreVisibilityPass) -> Self {
        Self {
            satellite: pass.satellite.to_string(),
            rise_step_index: pass.rise_step_index,
            set_step_index: pass.set_step_index,
            peak_elevation_deg: pass.peak_elevation_deg,
            peak_step_index: pass.peak_step_index,
        }
    }
}

/// Sampled visibility passes over `[startJ2000S, endJ2000S]` at `stepSeconds`
/// spacing. Delegates to `sidereon_core::geometry::passes`.
#[wasm_bindgen(js_name = gnssPasses)]
pub fn gnss_passes(
    sp3: &Sp3,
    station_ecef_m: &[f64],
    start_j2000_s: f64,
    end_j2000_s: f64,
    step_seconds: f64,
    options: JsValue,
) -> Result<Vec<GnssPass>, JsValue> {
    let station = vec3_finite("stationEcefM", station_ecef_m)?;
    if !start_j2000_s.is_finite() || !end_j2000_s.is_finite() {
        return Err(range_error("window epochs must be finite"));
    }
    let step_seconds = step_seconds_u64(step_seconds)?;
    let opts = visibility_options(options)?;
    let found = core_passes(
        &sp3.inner,
        sp3.inner.satellites(),
        station,
        (start_j2000_s, end_j2000_s),
        step_seconds,
        &opts,
    )
    .map_err(dop_err)?;
    Ok(found.iter().map(GnssPass::from).collect())
}

#[cfg(test)]
mod drift_tests {
    //! The visibility elevation-mask default tracks the core
    //! `VisibilityOptions::default()` rather than a literal in this binding.
    use super::*;

    #[test]
    fn visibility_mask_default_tracks_core() {
        let got = VisibilityOptionsInput::default()
            .to_core()
            .expect("default visibility options are valid");
        assert_eq!(
            got.elevation_mask_deg,
            VisibilityOptions::default().elevation_mask_deg
        );
        assert!(got.systems.is_none());
    }
}
