//! Standalone tropospheric delay: Saastamoinen zenith delays, Niell mapping
//! factors, and the composed slant delay.
//!
//! Every number is produced by `sidereon_core::atmosphere::troposphere`
//! (`tropo_zenith` / `tropo_mapping` / `tropo_slant`); no formula lives here.
//! Latitude, longitude, and elevation arrive in degrees and are converted with a
//! single multiply by the precomputed `PI / 180` constant (one rounding), the
//! same conversion the Elixir/Python bindings use, so the results are bit-exact
//! across bindings. Height is the WGS84 ellipsoidal height in metres; the epoch
//! is a split Julian date (`jdWhole` + `jdFraction`) used for the Niell seasonal
//! day-of-year term.

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use sidereon_core::astro::time::model::{Instant, JulianDateSplit, TimeScale};
use sidereon_core::atmosphere::troposphere::{
    tropo_mapping, tropo_slant, tropo_zenith, MappingModel, Met, TropoModel,
};
use sidereon_core::{Error as CoreError, Wgs84Geodetic};

use crate::error::{engine_error, range_error, require_finite, type_error};

/// One rounding, matching the core/Python `math.radians` and Elixir's constant.
const DEG_TO_RAD: f64 = core::f64::consts::PI / 180.0;

fn deg_to_rad(deg: f64) -> f64 {
    deg * DEG_TO_RAD
}

fn tropo_error(error: CoreError) -> JsValue {
    match error {
        CoreError::InvalidInput(_) => range_error(&error.to_string()),
        _ => engine_error(error),
    }
}

/// Surface meteorology: `{ pressureHpa, temperatureK, relativeHumidity }`.
/// Pressure in hectopascals, temperature in kelvin, humidity a `[0, 1]` fraction.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MetInput {
    pressure_hpa: f64,
    temperature_k: f64,
    relative_humidity: f64,
}

fn met_from_js(met: JsValue) -> Result<Met, JsValue> {
    let input: MetInput = serde_wasm_bindgen::from_value(met)
        .map_err(|e| type_error(&format!("invalid meteorology: {e}")))?;
    Met::new(
        input.pressure_hpa,
        input.temperature_k,
        input.relative_humidity,
    )
    .map_err(tropo_error)
}

fn receiver(lat_deg: f64, lon_deg: f64, height_m: f64) -> Result<Wgs84Geodetic, JsValue> {
    Wgs84Geodetic::new(deg_to_rad(lat_deg), deg_to_rad(lon_deg), height_m)
        .map_err(|error| range_error(&error.to_string()))
}

fn epoch(jd_whole: f64, jd_fraction: f64) -> Result<Instant, JsValue> {
    let split = JulianDateSplit::new(jd_whole, jd_fraction)
        .map_err(|error| range_error(&error.to_string()))?;
    Ok(Instant::from_julian_date(TimeScale::Gpst, split))
}

/// Zenith hydrostatic and wet tropospheric delays, positive metres.
#[wasm_bindgen]
pub struct ZenithDelay {
    dry_m: f64,
    wet_m: f64,
}

#[wasm_bindgen]
impl ZenithDelay {
    /// Zenith hydrostatic (dry) delay, positive metres.
    #[wasm_bindgen(getter, js_name = dryM)]
    pub fn dry_m(&self) -> f64 {
        self.dry_m
    }

    /// Zenith wet delay, positive metres.
    #[wasm_bindgen(getter, js_name = wetM)]
    pub fn wet_m(&self) -> f64 {
        self.wet_m
    }
}

/// Niell hydrostatic and wet mapping factors, dimensionless.
#[wasm_bindgen]
pub struct MappingFactors {
    dry: f64,
    wet: f64,
}

#[wasm_bindgen]
impl MappingFactors {
    /// Hydrostatic mapping factor (includes the height correction).
    #[wasm_bindgen(getter)]
    pub fn dry(&self) -> f64 {
        self.dry
    }

    /// Wet mapping factor.
    #[wasm_bindgen(getter)]
    pub fn wet(&self) -> f64 {
        self.wet
    }
}

/// Saastamoinen zenith tropospheric delays from supplied meteorology.
///
/// `latDeg` is the receiver geodetic latitude in degrees, `heightM` the WGS84
/// ellipsoidal height in metres, and `met` a `{ pressureHpa, temperatureK,
/// relativeHumidity }` object. The hydrostatic term carries the latitude/height
/// gravity correction. Throws a `RangeError`/`Error` for out-of-domain input.
#[wasm_bindgen(js_name = tropoZenithDelay)]
pub fn tropo_zenith_delay(
    lat_deg: f64,
    height_m: f64,
    met: JsValue,
) -> Result<ZenithDelay, JsValue> {
    let receiver = receiver(lat_deg, 0.0, height_m)?;
    let met = met_from_js(met)?;
    let z = tropo_zenith(TropoModel::Saastamoinen, receiver, met).map_err(tropo_error)?;
    Ok(ZenithDelay {
        dry_m: z.dry_m,
        wet_m: z.wet_m,
    })
}

/// Niell hydrostatic and wet mapping factors at an elevation.
///
/// `elevationDeg` and `latDeg` are degrees, `heightM` the WGS84 ellipsoidal
/// height in metres, and (`jdWhole`, `jdFraction`) a split Julian date for the
/// seasonal day-of-year. Unity at the zenith; grows toward the horizon.
#[wasm_bindgen(js_name = tropoMappingFactors)]
pub fn tropo_mapping_factors(
    elevation_deg: f64,
    lat_deg: f64,
    height_m: f64,
    jd_whole: f64,
    jd_fraction: f64,
) -> Result<MappingFactors, JsValue> {
    let receiver = receiver(lat_deg, 0.0, height_m)?;
    let epoch = epoch(jd_whole, jd_fraction)?;
    let m = tropo_mapping(
        MappingModel::Niell,
        deg_to_rad(elevation_deg),
        receiver,
        epoch,
    )
    .map_err(tropo_error)?;
    Ok(MappingFactors {
        dry: m.dry,
        wet: m.wet,
    })
}

/// Full slant tropospheric delay, positive metres.
///
/// Composes the Saastamoinen zenith delays with the Niell mapping at
/// `elevationDeg`. The receiver is `latDeg` / `lonDeg` / `heightM`; `met` is the
/// `{ pressureHpa, temperatureK, relativeHumidity }` object; (`jdWhole`,
/// `jdFraction`) the split Julian date. Returns `0` at or below the horizon
/// (no signal path), matching the cross-binding contract.
#[wasm_bindgen(js_name = tropoSlantDelay)]
pub fn tropo_slant_delay(
    elevation_deg: f64,
    lat_deg: f64,
    lon_deg: f64,
    height_m: f64,
    met: JsValue,
    jd_whole: f64,
    jd_fraction: f64,
) -> Result<f64, JsValue> {
    let met = met_from_js(met)?;
    // A non-finite elevation is bad input, not a sub-horizon geometry: reject it
    // with a RangeError instead of folding it into the "zero below the horizon"
    // contract below (which would otherwise return 0 for -Infinity).
    require_finite(elevation_deg, "elevationDeg")?;
    if elevation_deg < 0.0 {
        // The core rejects a negative elevation as out-of-range; honor the
        // documented "zero at or below the horizon" contract here instead.
        return Ok(0.0);
    }
    let receiver = receiver(lat_deg, lon_deg, height_m)?;
    let epoch = epoch(jd_whole, jd_fraction)?;
    tropo_slant(deg_to_rad(elevation_deg), receiver, met, epoch).map_err(tropo_error)
}
