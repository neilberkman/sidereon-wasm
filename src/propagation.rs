//! Numerical orbit propagation. Marshals one idiomatic JS request object into
//! the core `StatePropagator` and returns the sampled ephemeris. The
//! force-model composition, integrator/option defaults, and the integration
//! itself all live in core: the binding only translates the request's selectors
//! and marshals the returned states back out.

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use sidereon_core::astro::propagator::StatePropagator;
use sidereon_core::astro::state::CartesianState;

use crate::error::{engine_error, range_error, type_error};
use crate::force_model_input::{
    force_model_kind, integrator_kind, DragInput, ForceModelInput, IntegratorOptionsInput,
};

/// Numerical propagation request:
/// `{ epochS, positionKm: [x, y, z], velocityKmS: [vx, vy, vz], timesS: [...] }`
/// plus the optional force-model / integrator / tolerance controls. Omitted
/// option fields fall back to the engine's `IntegratorOptions::default`, so the
/// binding holds no defaults of its own.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PropagateRequest {
    epoch_s: f64,
    position_km: Vec<f64>,
    velocity_km_s: Vec<f64>,
    times_s: Vec<f64>,
    #[serde(default)]
    force_model: Option<ForceModelInput>,
    #[serde(default)]
    integrator: Option<String>,
    #[serde(default)]
    #[serde(flatten)]
    integrator_options: IntegratorOptionsInput,
    #[serde(default)]
    mu_km3_s2: Option<f64>,
    #[serde(default)]
    drag: Option<DragInput>,
}

fn fixed3(values: &[f64], field: &str) -> Result<[f64; 3], JsValue> {
    if values.len() != 3 {
        return Err(type_error(&format!(
            "{field} must have exactly 3 elements, got {}",
            values.len()
        )));
    }
    Ok([values[0], values[1], values[2]])
}

/// Numerically propagate an ECI Cartesian state and sample it at a grid of
/// epochs.
///
/// `request` is a plain object; see the `PropagateStateRequest` TypeScript type.
/// Throws a `TypeError` for malformed input (wrong layout, unknown selector), a
/// `RangeError` for a non-positive initial step, and an `Error` if the engine's
/// propagation fails.
#[wasm_bindgen(js_name = propagateState)]
pub fn propagate_state(request: JsValue) -> Result<Ephemeris, JsValue> {
    let req: PropagateRequest = serde_wasm_bindgen::from_value(request)
        .map_err(|e| type_error(&format!("invalid propagation request: {e}")))?;

    let position = fixed3(&req.position_km, "positionKm")?;
    let velocity = fixed3(&req.velocity_km_s, "velocityKmS")?;

    let options = req.integrator_options.to_core();

    // A non-positive initial step is a caller-supplied bad numeric range; reject
    // it at the boundary with a RangeError (the JS class a developer expects)
    // rather than letting the integrator surface it as a generic Error.
    if options.initial_step <= 0.0 {
        return Err(range_error("initialStepS must be positive"));
    }

    let propagator = StatePropagator {
        initial: CartesianState::new(req.epoch_s, position, velocity),
        force_model: force_model_kind(req.force_model.as_ref(), req.mu_km3_s2)?,
        integrator: integrator_kind(req.integrator.as_deref())?,
        options,
        drag: req.drag.as_ref().map(DragInput::to_core).transpose()?,
        space_weather: None,
    };

    let states = propagator.ephemeris(&req.times_s).map_err(engine_error)?;

    let mut positions = Vec::with_capacity(states.len() * 3);
    let mut velocities = Vec::with_capacity(states.len() * 3);
    for state in &states {
        positions.extend_from_slice(&state.position_array());
        velocities.extend_from_slice(&state.velocity_array());
    }

    Ok(Ephemeris {
        times: req.times_s,
        positions,
        velocities,
    })
}

/// An ephemeris from numerical state-vector propagation: the requested output
/// epochs plus the Cartesian state at each. Arrays are flat row-major.
#[wasm_bindgen]
pub struct Ephemeris {
    times: Vec<f64>,
    positions: Vec<f64>,
    velocities: Vec<f64>,
}

#[wasm_bindgen]
impl Ephemeris {
    /// The output epochs (TDB seconds), as a `Float64Array` of length `epochCount`.
    #[wasm_bindgen(getter, js_name = timesS)]
    pub fn times_s(&self) -> Vec<f64> {
        self.times.clone()
    }

    /// ECI positions, km, flat `[x0, y0, z0, x1, ...]`, length `3 * epochCount`.
    #[wasm_bindgen(getter, js_name = positionKm)]
    pub fn position_km(&self) -> Vec<f64> {
        self.positions.clone()
    }

    /// ECI velocities, km/s, flat `[vx0, vy0, vz0, ...]`, length `3 * epochCount`.
    #[wasm_bindgen(getter, js_name = velocityKmS)]
    pub fn velocity_km_s(&self) -> Vec<f64> {
        self.velocities.clone()
    }

    /// The full state ephemeris as a flat row-major `Float64Array` of length
    /// `6 * epochCount`, each row `[x, y, z, vx, vy, vz]` (km, km/s).
    #[wasm_bindgen(getter)]
    pub fn states(&self) -> Vec<f64> {
        let n = self.times.len();
        let mut out = Vec::with_capacity(n * 6);
        for i in 0..n {
            out.extend_from_slice(&self.positions[i * 3..i * 3 + 3]);
            out.extend_from_slice(&self.velocities[i * 3..i * 3 + 3]);
        }
        out
    }

    /// Number of output epochs.
    #[wasm_bindgen(getter, js_name = epochCount)]
    pub fn epoch_count(&self) -> usize {
        self.times.len()
    }
}
