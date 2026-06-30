//! Satellite-ground Doppler and range-rate from a GCRS state.
//!
//! Thin wrapper over `sidereon_core::astro::doppler`. The rotating-frame
//! transport, frame transform, and range-rate projection live in the crate; this
//! layer only reshapes the GCRS position/velocity vectors and the UTC epoch,
//! builds the core time scales, and packages the result. Positive range rate
//! means receding; positive Doppler ratio means approaching.

use wasm_bindgen::prelude::*;

use sidereon_core::astro::doppler::{doppler_shift, range_rate_and_ratio};
use sidereon_core::astro::time::scales::TimeScales;

use crate::error::engine_error;
use crate::marshal::vec3_finite;

/// Range-rate and Doppler result for a carrier frequency.
#[wasm_bindgen]
pub struct DopplerShift {
    range_rate_km_s: f64,
    doppler_hz: f64,
    doppler_ratio: f64,
}

#[wasm_bindgen]
impl DopplerShift {
    /// Range rate, kilometres per second; positive means receding.
    #[wasm_bindgen(getter, js_name = rangeRateKmS)]
    pub fn range_rate_km_s(&self) -> f64 {
        self.range_rate_km_s
    }

    /// Carrier Doppler shift, hertz; positive means a frequency increase.
    #[wasm_bindgen(getter, js_name = dopplerHz)]
    pub fn doppler_hz(&self) -> f64 {
        self.doppler_hz
    }

    /// Dimensionless Doppler ratio; positive means approaching the station.
    #[wasm_bindgen(getter, js_name = dopplerRatio)]
    pub fn doppler_ratio(&self) -> f64 {
        self.doppler_ratio
    }
}

/// Build the core time scales from a UTC civil epoch, mapping a degenerate civil
/// time onto the engine error.
#[allow(clippy::too_many_arguments)]
fn time_scales(
    year: i32,
    month: i32,
    day: i32,
    hour: i32,
    minute: i32,
    second: f64,
) -> Result<TimeScales, JsValue> {
    TimeScales::from_utc(year, month, day, hour, minute, second).map_err(engine_error)
}

/// Range rate and dimensionless Doppler ratio from a GCRS state.
///
/// `gcrsPositionKm` and `gcrsVelocityKmS` are length-3 `Float64Array`s (km and
/// km/s). The station is geodetic `stationLatDeg` / `stationLonDeg` (degrees) at
/// `stationAltKm` (km). The receive epoch is the UTC civil time
/// `year`/`month`/`day`/`hour`/`minute`/`second`. Returns `[rangeRateKmS,
/// dopplerRatio]`. Delegates to
/// `sidereon_core::astro::doppler::range_rate_and_ratio`.
#[wasm_bindgen(js_name = dopplerRangeRate)]
#[allow(clippy::too_many_arguments)]
pub fn doppler_range_rate(
    gcrs_position_km: &[f64],
    gcrs_velocity_km_s: &[f64],
    station_lat_deg: f64,
    station_lon_deg: f64,
    station_alt_km: f64,
    year: i32,
    month: i32,
    day: i32,
    hour: i32,
    minute: i32,
    second: f64,
) -> Result<Vec<f64>, JsValue> {
    let position = vec3_finite("gcrsPositionKm", gcrs_position_km)?;
    let velocity = vec3_finite("gcrsVelocityKmS", gcrs_velocity_km_s)?;
    let ts = time_scales(year, month, day, hour, minute, second)?;
    let (range_rate, ratio) = range_rate_and_ratio(
        position,
        velocity,
        station_lat_deg,
        station_lon_deg,
        station_alt_km,
        &ts,
    )
    .map_err(engine_error)?;
    Ok(vec![range_rate, ratio])
}

/// Range rate, Doppler ratio, and carrier Doppler shift from a GCRS state.
///
/// Same geometry and epoch inputs as [`dopplerRangeRate`], plus the transmit
/// `frequencyHz` whose shift is reported. Delegates to
/// `sidereon_core::astro::doppler::doppler_shift`.
#[wasm_bindgen(js_name = dopplerShift)]
#[allow(clippy::too_many_arguments)]
pub fn doppler_shift_js(
    gcrs_position_km: &[f64],
    gcrs_velocity_km_s: &[f64],
    station_lat_deg: f64,
    station_lon_deg: f64,
    station_alt_km: f64,
    year: i32,
    month: i32,
    day: i32,
    hour: i32,
    minute: i32,
    second: f64,
    frequency_hz: f64,
) -> Result<DopplerShift, JsValue> {
    let position = vec3_finite("gcrsPositionKm", gcrs_position_km)?;
    let velocity = vec3_finite("gcrsVelocityKmS", gcrs_velocity_km_s)?;
    let ts = time_scales(year, month, day, hour, minute, second)?;
    let shift = doppler_shift(
        position,
        velocity,
        station_lat_deg,
        station_lon_deg,
        station_alt_km,
        &ts,
        frequency_hz,
    )
    .map_err(engine_error)?;
    Ok(DopplerShift {
        range_rate_km_s: shift.range_rate_km_s,
        doppler_hz: shift.doppler_hz,
        doppler_ratio: shift.doppler_ratio,
    })
}
