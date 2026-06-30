//! Standalone force-model accelerations: two-body and J2 gravity.
//!
//! Each call builds a `CartesianState` from the supplied position/velocity and
//! evaluates `sidereon_core::astro::forces::{TwoBodyGravity, J2Gravity}` under
//! the default `PropagationContext`. No acceleration formula lives here; the
//! returned vector is exactly what the core force model produces.
//!
//! Position is kilometres, velocity kilometres per second, and the returned
//! acceleration kilometres per second squared, matching the core state units.

use wasm_bindgen::prelude::*;

use sidereon_core::astro::forces::{ForceModel, J2Gravity, TwoBodyGravity};
use sidereon_core::astro::propagator::api::PropagationContext;
use sidereon_core::astro::state::CartesianState;

use crate::error::engine_error;
use crate::marshal::vec3_finite;

/// Evaluate `force` at the given state and return the acceleration as `[ax, ay, az]`.
fn acceleration(
    force: &dyn ForceModel,
    position_km: &[f64],
    velocity_km_s: &[f64],
) -> Result<Vec<f64>, JsValue> {
    let position = vec3_finite("positionKm", position_km)?;
    let velocity = vec3_finite("velocityKmS", velocity_km_s)?;
    let state = CartesianState::new(0.0, position, velocity);
    let a = force
        .acceleration(&state, &PropagationContext::default())
        .map_err(engine_error)?;
    Ok(vec![a.x, a.y, a.z])
}

/// Two-body (point-mass) gravitational acceleration in km/s^2.
///
/// `positionKm` and `velocityKmS` are length-3 `Float64Array`s in the same
/// inertial frame; the result is a length-3 `Float64Array`. Uses the core
/// Earth gravitational parameter. Throws a `RangeError` for a non-finite input
/// and an `Error` for a degenerate (zero-magnitude) position.
#[wasm_bindgen(js_name = forceTwoBodyAcceleration)]
pub fn force_twobody_acceleration(
    position_km: &[f64],
    velocity_km_s: &[f64],
) -> Result<Vec<f64>, JsValue> {
    acceleration(&TwoBodyGravity::default(), position_km, velocity_km_s)
}

/// J2 oblateness gravitational acceleration in km/s^2.
///
/// `positionKm` and `velocityKmS` are length-3 `Float64Array`s in the same
/// inertial frame; the result is the J2 perturbing acceleration as a length-3
/// `Float64Array`. Uses the core Earth gravitational parameter, equatorial
/// radius, and J2 coefficient. Throws a `RangeError` for a non-finite input and
/// an `Error` for a degenerate (zero-magnitude) position.
#[wasm_bindgen(js_name = forceJ2Acceleration)]
pub fn force_j2_acceleration(
    position_km: &[f64],
    velocity_km_s: &[f64],
) -> Result<Vec<f64>, JsValue> {
    acceleration(&J2Gravity::default(), position_km, velocity_km_s)
}
