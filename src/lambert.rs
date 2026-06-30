//! Lambert two-point boundary-value (orbit transfer) solve.
//!
//! Thin wrapper over `sidereon_core::astro::lambert`. Battin's method lives in
//! the crate; this layer only reshapes the position / velocity vectors, maps the
//! direction-of-motion and direction-of-energy string selectors onto the core
//! enums, and re-encodes the two transfer velocities. All vectors are kilometres
//! / kilometres-per-second.

use wasm_bindgen::prelude::*;

use sidereon_core::astro::lambert::{battin, DirectionOfEnergy, DirectionOfMotion};

use crate::error::{engine_error, type_error};
use crate::marshal::vec3_finite;

fn direction_of_motion(value: &str) -> Result<DirectionOfMotion, JsValue> {
    match value {
        "short" => Ok(DirectionOfMotion::Short),
        "long" => Ok(DirectionOfMotion::Long),
        other => Err(type_error(&format!(
            "unknown direction of motion {other:?}; expected \"short\" or \"long\""
        ))),
    }
}

fn direction_of_energy(value: &str) -> Result<DirectionOfEnergy, JsValue> {
    match value {
        "low" => Ok(DirectionOfEnergy::Low),
        "high" => Ok(DirectionOfEnergy::High),
        other => Err(type_error(&format!(
            "unknown direction of energy {other:?}; expected \"low\" or \"high\""
        ))),
    }
}

/// The two transfer velocity vectors of a Lambert solution.
#[wasm_bindgen]
pub struct LambertTransfer {
    departure_velocity_km_s: Vec<f64>,
    arrival_velocity_km_s: Vec<f64>,
}

#[wasm_bindgen]
impl LambertTransfer {
    /// Transfer velocity at `r1` (departure) `[vx, vy, vz]`, km/s.
    #[wasm_bindgen(getter, js_name = departureVelocityKmS)]
    pub fn departure_velocity_km_s(&self) -> Vec<f64> {
        self.departure_velocity_km_s.clone()
    }

    /// Transfer velocity at `r2` (arrival) `[vx, vy, vz]`, km/s.
    #[wasm_bindgen(getter, js_name = arrivalVelocityKmS)]
    pub fn arrival_velocity_km_s(&self) -> Vec<f64> {
        self.arrival_velocity_km_s.clone()
    }
}

/// Solve Lambert's problem with Battin's method.
///
/// `r1`, `r2` are length-3 position `Float64Array`s (km) and `dtsec` the time of
/// flight (seconds). `v1` (length-3, km/s) is only consulted for the degenerate
/// 180-degree transfer where the transfer-plane normal is otherwise undefined.
/// `directionOfMotion` is `"short"` or `"long"`, `directionOfEnergy` is `"low"`
/// or `"high"`, and `nrev` is the number of complete revolutions. Returns the
/// departure and arrival transfer velocities. Delegates to
/// `sidereon_core::astro::lambert::battin`.
#[wasm_bindgen(js_name = lambertBattin)]
#[allow(clippy::too_many_arguments)]
pub fn lambert_battin(
    r1: &[f64],
    r2: &[f64],
    v1: &[f64],
    dtsec: f64,
    direction_of_motion_label: &str,
    direction_of_energy_label: &str,
    nrev: i32,
) -> Result<LambertTransfer, JsValue> {
    let r1 = vec3_finite("r1", r1)?;
    let r2 = vec3_finite("r2", r2)?;
    let v1 = vec3_finite("v1", v1)?;
    let dm = direction_of_motion(direction_of_motion_label)?;
    let de = direction_of_energy(direction_of_energy_label)?;
    let (v1t, v2t) = battin(&r1, &r2, &v1, dm, de, nrev, dtsec).map_err(engine_error)?;
    Ok(LambertTransfer {
        departure_velocity_km_s: v1t.to_vec(),
        arrival_velocity_km_s: v2t.to_vec(),
    })
}
