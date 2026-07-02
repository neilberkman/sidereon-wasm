//! Ground-observer Sun and Moon geometry binding.
//!
//! Thin wrappers over `sidereon_core::astro::bodies` observation helpers: the
//! topocentric Sun/Moon look angle, the Moon's illuminated fraction, and the
//! Moon rise/set and meridian-transit event finders. The station is a geodetic
//! site (degrees, kilometres) and each instant is a unix-microsecond epoch (the
//! same `UtcInstant` convention as the SGP4 pass surface); the analytic
//! ephemeris and the event-finder refinement are entirely the core's.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon::passes::UtcInstant;
use sidereon_core::astro::bodies::{
    find_moon_elevation_crossings as core_find_moon_elevation_crossings,
    find_moon_transits as core_find_moon_transits, moon_az_el as core_moon_az_el,
    moon_illumination as core_moon_illumination, sun_az_el as core_sun_az_el, BodyAzEl,
    MoonElevationCrossingKind, MoonElevationOptions, MoonIllumination, MoonTransitKind,
};
use sidereon_core::astro::frames::transforms::GeodeticStationKm;

use crate::error::{engine_error, range_error, type_error};

/// Build a geodetic station from degrees / kilometres, rejecting non-finite
/// fields (`RangeError`).
fn station(
    latitude_deg: f64,
    longitude_deg: f64,
    altitude_km: f64,
) -> Result<GeodeticStationKm, JsValue> {
    if !latitude_deg.is_finite() {
        return Err(range_error("latitudeDeg must be finite"));
    }
    if !longitude_deg.is_finite() {
        return Err(range_error("longitudeDeg must be finite"));
    }
    if !altitude_km.is_finite() {
        return Err(range_error("altitudeKm must be finite"));
    }
    Ok(GeodeticStationKm {
        latitude_deg,
        longitude_deg,
        altitude_km,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BodyAzElObject {
    azimuth_deg: f64,
    elevation_deg: f64,
    range_km: f64,
}

impl From<BodyAzEl> for BodyAzElObject {
    fn from(a: BodyAzEl) -> Self {
        Self {
            azimuth_deg: a.azimuth_deg,
            elevation_deg: a.elevation_deg,
            range_km: a.range_km,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MoonIlluminationObject {
    illuminated_fraction: f64,
    phase_angle_deg: f64,
}

impl From<MoonIllumination> for MoonIlluminationObject {
    fn from(m: MoonIllumination) -> Self {
        Self {
            illuminated_fraction: m.illuminated_fraction,
            phase_angle_deg: m.phase_angle_deg,
        }
    }
}

fn to_object<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(value).map_err(|e| type_error(&e.to_string()))
}

/// Topocentric azimuth/elevation/range of the Sun from a ground site.
///
/// The station is geodetic (`latitudeDeg`, `longitudeDeg`, `altitudeKm`);
/// `epochUnixUs` is the UTC instant as unix microseconds. Returns
/// `{ azimuthDeg, elevationDeg, rangeKm }` (azimuth clockwise from north).
/// Delegates to `sidereon_core::astro::bodies::sun_az_el`.
#[wasm_bindgen(js_name = sunAzEl)]
pub fn sun_az_el(
    latitude_deg: f64,
    longitude_deg: f64,
    altitude_km: f64,
    epoch_unix_us: i64,
) -> Result<JsValue, JsValue> {
    let station = station(latitude_deg, longitude_deg, altitude_km)?;
    let time = UtcInstant::from_unix_microseconds(epoch_unix_us);
    let az_el = core_sun_az_el(&station, time).map_err(engine_error)?;
    to_object(&BodyAzElObject::from(az_el))
}

/// Topocentric azimuth/elevation/range of the Moon from a ground site,
/// including the diurnal parallax. See [`sunAzEl`] for the argument shapes.
/// Delegates to `sidereon_core::astro::bodies::moon_az_el`.
#[wasm_bindgen(js_name = moonAzEl)]
pub fn moon_az_el(
    latitude_deg: f64,
    longitude_deg: f64,
    altitude_km: f64,
    epoch_unix_us: i64,
) -> Result<JsValue, JsValue> {
    let station = station(latitude_deg, longitude_deg, altitude_km)?;
    let time = UtcInstant::from_unix_microseconds(epoch_unix_us);
    let az_el = core_moon_az_el(&station, time).map_err(engine_error)?;
    to_object(&BodyAzElObject::from(az_el))
}

/// Illuminated fraction of the Moon as seen from a ground site. Returns
/// `{ illuminatedFraction, phaseAngleDeg }` (0 = new, 1 = full). See [`sunAzEl`]
/// for the argument shapes. Delegates to
/// `sidereon_core::astro::bodies::moon_illumination`.
#[wasm_bindgen(js_name = moonIllumination)]
pub fn moon_illumination(
    latitude_deg: f64,
    longitude_deg: f64,
    altitude_km: f64,
    epoch_unix_us: i64,
) -> Result<JsValue, JsValue> {
    let station = station(latitude_deg, longitude_deg, altitude_km)?;
    let time = UtcInstant::from_unix_microseconds(epoch_unix_us);
    let illum = core_moon_illumination(&station, time).map_err(engine_error)?;
    to_object(&MoonIlluminationObject::from(illum))
}

/// Topocentric geometric Moon (disk-center) elevation from a ground site,
/// degrees. See [`sunAzEl`] for the argument shapes. Delegates to
/// `sidereon_core::astro::bodies::moon_az_el` and returns its `elevationDeg`: the
/// core `moon_elevation_deg` convenience wrapper `expect`s on the geometry, so a
/// valid-shaped but out-of-range station (e.g. latitude 120) would panic the
/// wasm module; going through `moon_az_el` surfaces that as a thrown JS error.
#[wasm_bindgen(js_name = moonElevationDeg)]
pub fn moon_elevation_deg(
    latitude_deg: f64,
    longitude_deg: f64,
    altitude_km: f64,
    epoch_unix_us: i64,
) -> Result<f64, JsValue> {
    let station = station(latitude_deg, longitude_deg, altitude_km)?;
    let time = UtcInstant::from_unix_microseconds(epoch_unix_us);
    Ok(core_moon_az_el(&station, time)
        .map_err(engine_error)?
        .elevation_deg)
}

/// Options for the Moon elevation crossings finder. Every field defaults to the
/// core `MoonElevationOptions::default()`: a `-0.833` deg upper-limb threshold,
/// a 300 s scan step, and a 1 s refinement tolerance.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct MoonElevationOptionsInput {
    elevation_threshold_deg: Option<f64>,
    step_seconds: Option<f64>,
    time_tolerance_seconds: Option<f64>,
}

impl MoonElevationOptionsInput {
    fn to_core(&self) -> MoonElevationOptions {
        let d = MoonElevationOptions::default();
        MoonElevationOptions {
            elevation_threshold_deg: self
                .elevation_threshold_deg
                .unwrap_or(d.elevation_threshold_deg),
            step_seconds: self.step_seconds.unwrap_or(d.step_seconds),
            time_tolerance_seconds: self
                .time_tolerance_seconds
                .unwrap_or(d.time_tolerance_seconds),
        }
    }
}

/// One refined Moon elevation threshold crossing (moonrise / moonset).
#[wasm_bindgen]
pub struct MoonElevationCrossing {
    time_unix_us: i64,
    kind: &'static str,
    elevation_deg: f64,
}

#[wasm_bindgen]
impl MoonElevationCrossing {
    /// Refined crossing instant, unix microseconds.
    #[wasm_bindgen(getter, js_name = timeUnixUs)]
    pub fn time_unix_us(&self) -> i64 {
        self.time_unix_us
    }

    /// Crossing direction: `"rising"` (moonrise) or `"setting"` (moonset).
    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> String {
        self.kind.to_string()
    }

    /// Topocentric Moon elevation at the refined instant, degrees.
    #[wasm_bindgen(getter, js_name = elevationDeg)]
    pub fn elevation_deg(&self) -> f64 {
        self.elevation_deg
    }
}

/// Find Moon elevation threshold crossings (moonrise / moonset) over a UTC
/// window.
///
/// The station is geodetic (`latitudeDeg`, `longitudeDeg`, `altitudeKm`);
/// `startUnixUs` / `endUnixUs` bound the window in unix microseconds. `options`
/// is `{ elevationThresholdDeg?, stepSeconds?, timeToleranceSeconds? }`.
/// Delegates to `sidereon_core::astro::bodies::find_moon_elevation_crossings`.
#[wasm_bindgen(js_name = findMoonElevationCrossings)]
pub fn find_moon_elevation_crossings(
    latitude_deg: f64,
    longitude_deg: f64,
    altitude_km: f64,
    start_unix_us: i64,
    end_unix_us: i64,
    options: JsValue,
) -> Result<Vec<MoonElevationCrossing>, JsValue> {
    let station = station(latitude_deg, longitude_deg, altitude_km)?;
    let opts: MoonElevationOptionsInput = if options.is_undefined() || options.is_null() {
        MoonElevationOptionsInput::default()
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid moon elevation options: {e}")))?
    };
    let start = UtcInstant::from_unix_microseconds(start_unix_us);
    let end = UtcInstant::from_unix_microseconds(end_unix_us);
    let crossings = core_find_moon_elevation_crossings(&station, start, end, opts.to_core())
        .map_err(engine_error)?;
    Ok(crossings
        .into_iter()
        .map(|c| MoonElevationCrossing {
            time_unix_us: c.time.unix_microseconds(),
            kind: match c.kind {
                MoonElevationCrossingKind::Rising => "rising",
                MoonElevationCrossingKind::Setting => "setting",
            },
            elevation_deg: c.elevation_deg,
        })
        .collect())
}

/// One refined Moon meridian transit (culmination).
#[wasm_bindgen]
pub struct MoonTransit {
    time_unix_us: i64,
    kind: &'static str,
    elevation_deg: f64,
}

#[wasm_bindgen]
impl MoonTransit {
    /// Refined culmination instant, unix microseconds.
    #[wasm_bindgen(getter, js_name = timeUnixUs)]
    pub fn time_unix_us(&self) -> i64 {
        self.time_unix_us
    }

    /// Culmination kind: `"upper"` (due south, highest) or `"lower"` (due north,
    /// lowest).
    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> String {
        self.kind.to_string()
    }

    /// Topocentric Moon elevation at the refined instant, degrees.
    #[wasm_bindgen(getter, js_name = elevationDeg)]
    pub fn elevation_deg(&self) -> f64 {
        self.elevation_deg
    }
}

/// Find Moon meridian transits (upper and lower culminations) over a UTC window.
///
/// The station is geodetic (`latitudeDeg`, `longitudeDeg`, `altitudeKm`);
/// `startUnixUs` / `endUnixUs` bound the window in unix microseconds.
/// `stepSeconds` is the uniform scan step and `timeToleranceSeconds` the
/// refinement tolerance. Delegates to
/// `sidereon_core::astro::bodies::find_moon_transits`.
#[wasm_bindgen(js_name = findMoonTransits)]
pub fn find_moon_transits(
    latitude_deg: f64,
    longitude_deg: f64,
    altitude_km: f64,
    start_unix_us: i64,
    end_unix_us: i64,
    step_seconds: f64,
    time_tolerance_seconds: f64,
) -> Result<Vec<MoonTransit>, JsValue> {
    let station = station(latitude_deg, longitude_deg, altitude_km)?;
    let start = UtcInstant::from_unix_microseconds(start_unix_us);
    let end = UtcInstant::from_unix_microseconds(end_unix_us);
    let transits =
        core_find_moon_transits(&station, start, end, step_seconds, time_tolerance_seconds)
            .map_err(engine_error)?;
    Ok(transits
        .into_iter()
        .map(|t| MoonTransit {
            time_unix_us: t.time.unix_microseconds(),
            kind: match t.kind {
                MoonTransitKind::Upper => "upper",
                MoonTransitKind::Lower => "lower",
            },
            elevation_deg: t.elevation_deg,
        })
        .collect())
}
