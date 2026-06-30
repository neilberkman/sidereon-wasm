//! Events + body-angle geometry: eclipse shadow / status and the Sun/Moon angle
//! helpers, batched over position grids.
//!
//! Every output is delegated to `sidereon-core` (`events::eclipse`,
//! `astro::angles`); this module only reshapes flat row-major `(n, 3)`
//! `Float64Array` batches and surfaces the engine numbers. Each helper crosses
//! the FFI boundary once with the per-row loop in Rust.

use wasm_bindgen::prelude::*;

use sidereon_core::astro::angles::{
    earth_angular_radius as core_earth_angular_radius, moon_angle as core_moon_angle,
    phase_angle as core_phase_angle, sun_angle as core_sun_angle,
    sun_elevation as core_sun_elevation,
};
use sidereon_core::astro::events::eclipse::{
    shadow_fraction as core_shadow_fraction, status as core_status, EclipseStatus,
};

use crate::error::engine_error;
use crate::marshal::{reject_empty, rows3, same_len};

fn status_label(status: EclipseStatus) -> &'static str {
    match status {
        EclipseStatus::Sunlit => "sunlit",
        EclipseStatus::Penumbra => "penumbra",
        EclipseStatus::Umbra => "umbra",
    }
}

/// Shadow fraction in `[0, 1]` for satellite and Sun position batches, km.
///
/// `satellitePositionKm` and `sunPositionKm` are flat row-major `(n, 3)`
/// `Float64Array`s. Returns a `Float64Array` of length `n`.
#[wasm_bindgen(js_name = shadowFraction)]
pub fn shadow_fraction(
    satellite_position_km: &[f64],
    sun_position_km: &[f64],
) -> Result<Vec<f64>, JsValue> {
    let sats = rows3("satellitePositionKm", satellite_position_km, true)?;
    let suns = rows3("sunPositionKm", sun_position_km, true)?;
    reject_empty("satellitePositionKm", &sats)?;
    same_len(
        "satellitePositionKm",
        sats.len(),
        "sunPositionKm",
        suns.len(),
    )?;
    sats.iter()
        .zip(suns.iter())
        .map(|(&sat, &sun)| core_shadow_fraction(sat, sun).map_err(engine_error))
        .collect()
}

/// Eclipse status for satellite and Sun position batches, km. Returns a
/// `string[]` of `"sunlit"` / `"penumbra"` / `"umbra"`.
#[wasm_bindgen(js_name = eclipseStatus)]
pub fn eclipse_status(
    satellite_position_km: &[f64],
    sun_position_km: &[f64],
) -> Result<Vec<String>, JsValue> {
    let sats = rows3("satellitePositionKm", satellite_position_km, true)?;
    let suns = rows3("sunPositionKm", sun_position_km, true)?;
    reject_empty("satellitePositionKm", &sats)?;
    same_len(
        "satellitePositionKm",
        sats.len(),
        "sunPositionKm",
        suns.len(),
    )?;
    sats.iter()
        .zip(suns.iter())
        .map(|(&sat, &sun)| {
            core_status(sat, sun)
                .map(|s| status_label(s).to_string())
                .map_err(engine_error)
        })
        .collect()
}

/// Angle in degrees between satellite nadir and the Sun direction.
#[wasm_bindgen(js_name = sunAngle)]
pub fn sun_angle(
    satellite_position_km: &[f64],
    sun_position_km: &[f64],
) -> Result<Vec<f64>, JsValue> {
    let sats = rows3("satellitePositionKm", satellite_position_km, true)?;
    let suns = rows3("sunPositionKm", sun_position_km, true)?;
    reject_empty("satellitePositionKm", &sats)?;
    same_len(
        "satellitePositionKm",
        sats.len(),
        "sunPositionKm",
        suns.len(),
    )?;
    sats.iter()
        .zip(suns.iter())
        .map(|(&sat, &sun)| core_sun_angle(sat, sun).map_err(engine_error))
        .collect()
}

/// Angle in degrees between satellite nadir and the Moon direction.
#[wasm_bindgen(js_name = moonAngle)]
pub fn moon_angle(
    satellite_position_km: &[f64],
    moon_position_km: &[f64],
) -> Result<Vec<f64>, JsValue> {
    let sats = rows3("satellitePositionKm", satellite_position_km, true)?;
    let moons = rows3("moonPositionKm", moon_position_km, true)?;
    reject_empty("satellitePositionKm", &sats)?;
    same_len(
        "satellitePositionKm",
        sats.len(),
        "moonPositionKm",
        moons.len(),
    )?;
    sats.iter()
        .zip(moons.iter())
        .map(|(&sat, &moon)| core_moon_angle(sat, moon).map_err(engine_error))
        .collect()
}

/// Sun elevation in degrees above the satellite local horizontal plane.
#[wasm_bindgen(js_name = sunElevation)]
pub fn sun_elevation(
    satellite_position_km: &[f64],
    sun_position_km: &[f64],
) -> Result<Vec<f64>, JsValue> {
    let sats = rows3("satellitePositionKm", satellite_position_km, true)?;
    let suns = rows3("sunPositionKm", sun_position_km, true)?;
    reject_empty("satellitePositionKm", &sats)?;
    same_len(
        "satellitePositionKm",
        sats.len(),
        "sunPositionKm",
        suns.len(),
    )?;
    sats.iter()
        .zip(suns.iter())
        .map(|(&sat, &sun)| core_sun_elevation(sat, sun).map_err(engine_error))
        .collect()
}

/// Sun-satellite-observer phase angle in degrees.
#[wasm_bindgen(js_name = phaseAngle)]
pub fn phase_angle(
    satellite_position_km: &[f64],
    sun_position_km: &[f64],
    observer_position_km: &[f64],
) -> Result<Vec<f64>, JsValue> {
    let sats = rows3("satellitePositionKm", satellite_position_km, true)?;
    let suns = rows3("sunPositionKm", sun_position_km, true)?;
    let observers = rows3("observerPositionKm", observer_position_km, true)?;
    reject_empty("satellitePositionKm", &sats)?;
    same_len(
        "satellitePositionKm",
        sats.len(),
        "sunPositionKm",
        suns.len(),
    )?;
    same_len(
        "satellitePositionKm",
        sats.len(),
        "observerPositionKm",
        observers.len(),
    )?;
    sats.iter()
        .zip(suns.iter())
        .zip(observers.iter())
        .map(|((&sat, &sun), &obs)| core_phase_angle(sat, sun, obs).map_err(engine_error))
        .collect()
}

/// Angular radius in degrees of Earth as seen from each satellite position.
#[wasm_bindgen(js_name = earthAngularRadius)]
pub fn earth_angular_radius(satellite_position_km: &[f64]) -> Result<Vec<f64>, JsValue> {
    let sats = rows3("satellitePositionKm", satellite_position_km, true)?;
    reject_empty("satellitePositionKm", &sats)?;
    sats.iter()
        .map(|&sat| core_earth_angular_radius(sat).map_err(engine_error))
        .collect()
}
