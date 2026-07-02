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

use serde::{Deserialize, Serialize};
use sidereon_core::astro::forces::{
    DragForce as CoreDragForce, ForceModel, J2Gravity, SpaceWeather as CoreSpaceWeather,
    TwoBodyGravity,
};
use sidereon_core::astro::propagator::api::PropagationContext;
use sidereon_core::astro::propagator::decay::{estimate_decay as core_estimate_decay, DecayConfig};
use sidereon_core::astro::propagator::driver::PropagationForceModel;
use sidereon_core::astro::propagator::numerical::IntegratorKind;
use sidereon_core::astro::state::CartesianState;

use crate::error::{engine_error, type_error};
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

#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct SpaceWeather {
    inner: CoreSpaceWeather,
}

#[wasm_bindgen]
impl SpaceWeather {
    #[wasm_bindgen(constructor)]
    pub fn new(f107: Option<f64>, f107a: Option<f64>, ap: Option<f64>) -> SpaceWeather {
        let defaults = CoreSpaceWeather::default();
        SpaceWeather {
            inner: CoreSpaceWeather {
                f107: f107.unwrap_or(defaults.f107),
                f107a: f107a.unwrap_or(defaults.f107a),
                ap: ap.unwrap_or(defaults.ap),
            },
        }
    }

    #[wasm_bindgen(getter)]
    pub fn f107(&self) -> f64 {
        self.inner.f107
    }

    #[wasm_bindgen(getter)]
    pub fn f107a(&self) -> f64 {
        self.inner.f107a
    }

    #[wasm_bindgen(getter)]
    pub fn ap(&self) -> f64 {
        self.inner.ap
    }
}

impl SpaceWeather {
    fn core(&self) -> CoreSpaceWeather {
        self.inner
    }
}

#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct DragForce {
    inner: CoreDragForce,
}

#[wasm_bindgen]
impl DragForce {
    #[wasm_bindgen(js_name = fromAreaMass)]
    pub fn from_area_mass(
        cd: f64,
        area_m2: f64,
        mass_kg: f64,
        space_weather: &SpaceWeather,
        cutoff_altitude_km: Option<f64>,
    ) -> Result<DragForce, JsValue> {
        let cutoff = cutoff_altitude_km.unwrap_or(CoreDragForce::DEFAULT_REENTRY_ALTITUDE_KM);
        Ok(DragForce {
            inner: CoreDragForce::from_area_mass(
                cd,
                area_m2,
                mass_kg,
                space_weather.core(),
                cutoff,
            )
            .map_err(engine_error)?,
        })
    }

    #[wasm_bindgen(js_name = fromBcFactor)]
    pub fn from_bc_factor(
        bc_factor_m2_kg: f64,
        space_weather: &SpaceWeather,
        cutoff_altitude_km: Option<f64>,
    ) -> Result<DragForce, JsValue> {
        let cutoff = cutoff_altitude_km.unwrap_or(CoreDragForce::DEFAULT_REENTRY_ALTITUDE_KM);
        Ok(DragForce {
            inner: CoreDragForce::from_bc_factor_m2_kg(
                bc_factor_m2_kg,
                space_weather.core(),
                cutoff,
            )
            .map_err(engine_error)?,
        })
    }

    #[wasm_bindgen(js_name = fromBallisticCoefficient)]
    pub fn from_ballistic_coefficient(
        bc_kg_m2: f64,
        space_weather: &SpaceWeather,
        cutoff_altitude_km: Option<f64>,
    ) -> Result<DragForce, JsValue> {
        let cutoff = cutoff_altitude_km.unwrap_or(CoreDragForce::DEFAULT_REENTRY_ALTITUDE_KM);
        Ok(DragForce {
            inner: CoreDragForce::from_ballistic_coefficient(
                bc_kg_m2,
                space_weather.core(),
                cutoff,
            )
            .map_err(engine_error)?,
        })
    }

    #[wasm_bindgen(getter, js_name = bcFactorM2Kg)]
    pub fn bc_factor_m2_kg(&self) -> f64 {
        self.inner.bc_factor_m2_kg()
    }

    #[wasm_bindgen(getter, js_name = cutoffAltitudeKm)]
    pub fn cutoff_altitude_km(&self) -> f64 {
        self.inner.cutoff_altitude_km()
    }

    #[wasm_bindgen(getter, js_name = spaceWeather)]
    pub fn space_weather(&self) -> SpaceWeather {
        SpaceWeather {
            inner: self.inner.space_weather(),
        }
    }

    pub fn acceleration(
        &self,
        epoch_s: f64,
        position_km: &[f64],
        velocity_km_s: &[f64],
    ) -> Result<Vec<f64>, JsValue> {
        let position = vec3_finite("positionKm", position_km)?;
        let velocity = vec3_finite("velocityKmS", velocity_km_s)?;
        let state = CartesianState::new(epoch_s, position, velocity);
        let a = self
            .inner
            .acceleration(&state, &PropagationContext::default())
            .map_err(engine_error)?;
        Ok(vec![a.x, a.y, a.z])
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DecayRequest {
    epoch_s: f64,
    position_km: Vec<f64>,
    velocity_km_s: Vec<f64>,
    #[serde(default)]
    force_model: Option<String>,
    #[serde(default)]
    integrator: Option<String>,
    #[serde(default)]
    reentry_altitude_km: Option<f64>,
    #[serde(default)]
    scan_step_s: Option<f64>,
    #[serde(default)]
    crossing_tolerance_s: Option<f64>,
    #[serde(default)]
    max_duration_s: Option<f64>,
    #[serde(default)]
    max_scan_samples: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DecayEstimateJs {
    time_to_decay_s: f64,
    reentry_epoch_s: f64,
    reentry_position_km: [f64; 3],
    reentry_velocity_km_s: [f64; 3],
    reentry_altitude_km: f64,
}

fn force_model(label: Option<&str>) -> Result<PropagationForceModel, JsValue> {
    match label.unwrap_or("two_body") {
        "two_body" => Ok(PropagationForceModel::TwoBody),
        "two_body_j2" => Ok(PropagationForceModel::TwoBodyJ2),
        other => Err(type_error(&format!(
            "invalid forceModel {other:?}: expected \"two_body\" or \"two_body_j2\""
        ))),
    }
}

fn integrator(label: Option<&str>) -> Result<IntegratorKind, JsValue> {
    match label.unwrap_or("dp54") {
        "dp54" => Ok(IntegratorKind::Dp54),
        "rk4" => Ok(IntegratorKind::Rk4),
        other => Err(type_error(&format!(
            "invalid integrator {other:?}: expected \"dp54\" or \"rk4\""
        ))),
    }
}

#[wasm_bindgen(js_name = estimateDecay)]
pub fn estimate_decay(drag: &DragForce, request: JsValue) -> Result<JsValue, JsValue> {
    let req: DecayRequest = serde_wasm_bindgen::from_value(request)
        .map_err(|e| type_error(&format!("invalid decay request: {e}")))?;
    let position = vec3_finite("positionKm", &req.position_km)?;
    let velocity = vec3_finite("velocityKmS", &req.velocity_km_s)?;
    let mut config = DecayConfig::new(
        sidereon_core::astro::forces::DragParameters::from_bc_factor_m2_kg(
            drag.inner.bc_factor_m2_kg(),
            drag.inner.space_weather(),
            drag.inner.cutoff_altitude_km(),
        )
        .map_err(engine_error)?,
    )
    .with_force_model(force_model(req.force_model.as_deref())?)
    .with_integrator(integrator(req.integrator.as_deref())?);
    if let Some(value) = req.reentry_altitude_km {
        config = config.with_reentry_altitude_km(value);
    }
    if let Some(value) = req.scan_step_s {
        config = config.with_scan_step_s(value);
    }
    if let Some(value) = req.crossing_tolerance_s {
        config = config.with_crossing_tolerance_s(value);
    }
    if let Some(value) = req.max_duration_s {
        config = config.with_max_duration_s(value);
    }
    if let Some(value) = req.max_scan_samples {
        config = config.with_max_scan_samples(value);
    }

    let estimate = core_estimate_decay(
        CartesianState::new(req.epoch_s, position, velocity),
        &config,
    )
    .map_err(engine_error)?;
    serde_wasm_bindgen::to_value(&DecayEstimateJs {
        time_to_decay_s: estimate.time_to_decay_s,
        reentry_epoch_s: estimate.reentry_state.epoch_tdb_seconds,
        reentry_position_km: estimate.reentry_state.position_array(),
        reentry_velocity_km_s: estimate.reentry_state.velocity_array(),
        reentry_altitude_km: estimate.reentry_altitude_km,
    })
    .map_err(|e| type_error(&e.to_string()))
}
