//! Direct broadcast ionosphere models: GPS Klobuchar and Galileo NeQuick-G.
//!
//! Thin wrapper over `sidereon_core::atmosphere::ionosphere`. The model kernels
//! (the Klobuchar L1 polynomial and frequency scaling, and the Galileo
//! coefficient-driven NeQuick-G correction) live in the crate; this layer only
//! assembles the coefficient structs, builds the receiver geodetic and the epoch
//! (the epoch via the core `split_julian_date` helper, so no date arithmetic
//! lives here), and returns the engine's positive-metre delay. The IONEX
//! vertical-TEC grid product and its slant delay are wrapped separately in
//! [`crate::ionex`].

use std::f64::consts::PI;

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use sidereon_core::astro::time::civil::split_julian_date;
use sidereon_core::astro::time::model::{Instant, JulianDateSplit, TimeScale};
use sidereon_core::atmosphere::ionosphere::{
    ionosphere_delay, klobuchar_native, nequick_g_delay_m, nequick_g_stec_tecu,
    GalileoNequickCoeffs, IonoModel, KlobucharParams, NequickGRayEval,
};
use sidereon_core::Wgs84Geodetic;

use crate::error::{engine_error, type_error};

/// pi/180 as a single rounded constant, matching the engine's other bindings.
const DEG_TO_RAD: f64 = PI / 180.0;

/// Read a length-4 coefficient row, rejecting a wrong length (`TypeError`).
fn coeffs4(name: &str, values: &[f64]) -> Result<[f64; 4], JsValue> {
    if values.len() != 4 {
        return Err(type_error(&format!(
            "{name} must have length 4, got {}",
            values.len()
        )));
    }
    Ok([values[0], values[1], values[2], values[3]])
}

/// GPS broadcast Klobuchar ionospheric group delay in the model's native units
/// (positive metres).
///
/// `alpha` and `beta` are the eight transmitted GPS Klobuchar coefficients, each
/// a length-4 `Float64Array` (`a0..a3`, `b0..b3`). Receiver latitude/longitude
/// and satellite azimuth/elevation are degrees (latitude positive north,
/// longitude positive east, azimuth clockwise from north). `tGpsS` is the GPS
/// second-of-day in `[0, 86400)`. `frequencyHz` is the carrier the dispersive
/// delay is reported on; the model evaluates the L1 delay and scales it by the
/// dispersive `(f_l1 / f)^2` factor. Delegates to
/// `sidereon_core::atmosphere::ionosphere::klobuchar_native`. Throws an `Error`
/// on an out-of-domain coefficient, angle, time, or frequency.
#[wasm_bindgen(js_name = klobucharDelay)]
#[allow(clippy::too_many_arguments)]
pub fn klobuchar_delay(
    alpha: &[f64],
    beta: &[f64],
    lat_deg: f64,
    lon_deg: f64,
    azimuth_deg: f64,
    elevation_deg: f64,
    t_gps_s: f64,
    frequency_hz: f64,
) -> Result<f64, JsValue> {
    let params = KlobucharParams {
        alpha: coeffs4("alpha", alpha)?,
        beta: coeffs4("beta", beta)?,
    };
    klobuchar_native(
        &params,
        lat_deg,
        lon_deg,
        azimuth_deg,
        elevation_deg,
        t_gps_s,
        frequency_hz,
    )
    .map_err(engine_error)
}

/// Galileo NeQuick-G single-frequency ionospheric group delay (positive metres).
///
/// `ai0`/`ai1`/`ai2` are the three Galileo broadcast NeQuick-G coefficients.
/// `latDeg`/`lonDeg` are the receiver geodetic coordinates, `azimuthDeg` /
/// `elevationDeg` the satellite topocentric angles (degrees). The receive epoch
/// is the UTC civil time `year`/`month`/`day`/`hour`/`minute`/`second`, taken in
/// Galileo System Time. `frequencyHz` is the reporting carrier. Delegates to
/// `sidereon_core::atmosphere::ionosphere::ionosphere_delay` with
/// `IonoModel::GalileoNequickG`, which dispatches to the Galileo arm. Throws an
/// `Error` on an out-of-domain input.
#[wasm_bindgen(js_name = galileoNequickDelay)]
#[allow(clippy::too_many_arguments)]
pub fn galileo_nequick_delay(
    ai0: f64,
    ai1: f64,
    ai2: f64,
    lat_deg: f64,
    lon_deg: f64,
    azimuth_deg: f64,
    elevation_deg: f64,
    year: i32,
    month: i32,
    day: i32,
    hour: i32,
    minute: i32,
    second: f64,
    frequency_hz: f64,
) -> Result<f64, JsValue> {
    let receiver = Wgs84Geodetic::new(lat_deg * DEG_TO_RAD, lon_deg * DEG_TO_RAD, 0.0)
        .map_err(engine_error)?;
    let (jd_whole, fraction) = split_julian_date(year, month, day, hour, minute, second);
    let jd = JulianDateSplit::new(jd_whole, fraction).map_err(engine_error)?;
    let epoch = Instant::from_julian_date(TimeScale::Gst, jd);
    let model = IonoModel::GalileoNequickG(GalileoNequickCoeffs { ai0, ai1, ai2 });
    ionosphere_delay(
        receiver,
        elevation_deg * DEG_TO_RAD,
        azimuth_deg * DEG_TO_RAD,
        epoch,
        frequency_hz,
        &model,
    )
    .map_err(engine_error)
}

// --- NeQuick-G full three-dimensional slant model ---------------------------

/// One receiver-to-satellite ray for the full NeQuick-G slant evaluation.
///
/// This is the JS object marshalled into `sidereon_core::atmosphere::ionosphere::
/// NequickGRayEval`. The three Galileo broadcast effective-ionisation
/// coefficients (`ai0`/`ai1`/`ai2`) are carried alongside the ray geometry and
/// epoch. Geodetic longitudes and latitudes are degrees; heights are metres
/// above the reference sphere. `month` is `1..=12` and `utcHours` is the UTC time
/// of day in `[0, 24]`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NequickGEvalInput {
    ai0: f64,
    ai1: f64,
    ai2: f64,
    month: u8,
    utc_hours: f64,
    station_lon_deg: f64,
    station_lat_deg: f64,
    station_height_m: f64,
    satellite_lon_deg: f64,
    satellite_lat_deg: f64,
    satellite_height_m: f64,
}

impl NequickGEvalInput {
    fn split(&self) -> (GalileoNequickCoeffs, NequickGRayEval) {
        let coeffs = GalileoNequickCoeffs {
            ai0: self.ai0,
            ai1: self.ai1,
            ai2: self.ai2,
        };
        let ray = NequickGRayEval {
            month: self.month,
            utc_hours: self.utc_hours,
            station_lon_deg: self.station_lon_deg,
            station_lat_deg: self.station_lat_deg,
            station_height_m: self.station_height_m,
            satellite_lon_deg: self.satellite_lon_deg,
            satellite_lat_deg: self.satellite_lat_deg,
            satellite_height_m: self.satellite_height_m,
        };
        (coeffs, ray)
    }
}

fn nequick_g_eval(eval: JsValue) -> Result<(GalileoNequickCoeffs, NequickGRayEval), JsValue> {
    let input: NequickGEvalInput = serde_wasm_bindgen::from_value(eval)
        .map_err(|e| type_error(&format!("invalid NeQuick-G evaluation: {e}")))?;
    Ok(input.split())
}

/// Full NeQuick-G slant total electron content along the ray, in TECU.
///
/// Unlike the compact broadcast-driven [`galileo_nequick_delay`], this evaluates
/// the complete three-dimensional NeQuick 2 electron-density profiler integrated
/// along the receiver-to-satellite ray with the reference adaptive
/// Gauss-Kronrod quadrature. `eval` is a plain object; see the `NequickGEval`
/// TypeScript type. Delegates to
/// `sidereon_core::atmosphere::ionosphere::nequick_g_stec_tecu`. Throws a
/// `TypeError` for a malformed object and an `Error` for an out-of-domain month,
/// UTC, latitude, or a geometrically invalid ray.
#[wasm_bindgen(js_name = nequickGStecTecu)]
pub fn nequick_g_stec_tecu_js(eval: JsValue) -> Result<f64, JsValue> {
    let (coeffs, ray) = nequick_g_eval(eval)?;
    nequick_g_stec_tecu(&coeffs, &ray).map_err(engine_error)
}

/// Full NeQuick-G slant ionospheric group delay (positive metres) on
/// `frequencyHz`.
///
/// The slant TEC from [`nequick_g_stec_tecu_js`] mapped to a group delay with the
/// dispersive `40.3e16 / f^2` relation. `eval` is a plain object; see the
/// `NequickGEval` TypeScript type. Delegates to
/// `sidereon_core::atmosphere::ionosphere::nequick_g_delay_m`. Throws a
/// `TypeError` for a malformed object and an `Error` for an out-of-domain input
/// or non-positive frequency.
#[wasm_bindgen(js_name = nequickGDelayM)]
pub fn nequick_g_delay_m_js(eval: JsValue, frequency_hz: f64) -> Result<f64, JsValue> {
    let (coeffs, ray) = nequick_g_eval(eval)?;
    nequick_g_delay_m(&coeffs, &ray, frequency_hz).map_err(engine_error)
}
