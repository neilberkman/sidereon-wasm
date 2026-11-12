//! Doppler/range-rate receiver velocity operations for precise positioning.
//!
//! The public models are plain JavaScript data and therefore support structural
//! value comparison. All prediction, validation, and least-squares numerics are
//! delegated to `sidereon-core`.

use std::str::FromStr;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::precise_positioning::{
    predict_range_rate_m_s as core_predict_range_rate_m_s, solve_velocity as core_solve_velocity,
    ReceiverVelocityState, VelocityConfig, VelocityObservation, VelocityRobustConfig,
    VelocitySolveError,
};
use sidereon_core::GnssSatelliteId;

use crate::error::type_error;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReceiverVelocityStateInput {
    #[serde(deserialize_with = "crate::exact::vec3")]
    position_m: [f64; 3],
    #[serde(deserialize_with = "crate::exact::vec3")]
    velocity_m_s: [f64; 3],
    clock_drift_m_s: f64,
}

impl ReceiverVelocityStateInput {
    fn core(&self) -> ReceiverVelocityState {
        ReceiverVelocityState {
            position_m: self.position_m,
            velocity_m_s: self.velocity_m_s,
            clock_drift_m_s: self.clock_drift_m_s,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VelocityObservationInput {
    satellite_id: String,
    #[serde(deserialize_with = "crate::exact::vec3")]
    satellite_position_m: [f64; 3],
    #[serde(deserialize_with = "crate::exact::vec3")]
    satellite_velocity_m_s: [f64; 3],
    measured_range_rate_m_s: f64,
    sigma_m_s: f64,
    satellite_clock_drift_m_s: f64,
}

impl VelocityObservationInput {
    fn core(&self) -> Result<VelocityObservation, JsValue> {
        let sat = GnssSatelliteId::from_str(&self.satellite_id).map_err(|_| {
            type_error(&format!(
                "invalid velocity observation satelliteId: {:?}",
                self.satellite_id
            ))
        })?;
        Ok(VelocityObservation {
            sat,
            satellite_position_m: self.satellite_position_m,
            satellite_velocity_m_s: self.satellite_velocity_m_s,
            measured_range_rate_m_s: self.measured_range_rate_m_s,
            sigma_m_s: self.sigma_m_s,
            satellite_clock_drift_m_s: self.satellite_clock_drift_m_s,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct VelocityRobustConfigInput {
    huber_k: f64,
    scale_floor_m_s: f64,
    max_outer: usize,
    outer_tol_m_s: f64,
}

impl Default for VelocityRobustConfigInput {
    fn default() -> Self {
        let value = VelocityRobustConfig::default();
        Self {
            huber_k: value.huber_k,
            scale_floor_m_s: value.scale_floor_m_s,
            max_outer: value.max_outer,
            outer_tol_m_s: value.outer_tol_m_s,
        }
    }
}

impl VelocityRobustConfigInput {
    fn core(&self) -> VelocityRobustConfig {
        let mut value = VelocityRobustConfig::default();
        value.huber_k = self.huber_k;
        value.scale_floor_m_s = self.scale_floor_m_s;
        value.max_outer = self.max_outer;
        value.outer_tol_m_s = self.outer_tol_m_s;
        value
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct VelocityConfigInput {
    minimum_observations: usize,
    robust: Option<VelocityRobustConfigInput>,
}

impl Default for VelocityConfigInput {
    fn default() -> Self {
        let value = VelocityConfig::default();
        Self {
            minimum_observations: value.minimum_observations,
            robust: None,
        }
    }
}

impl VelocityConfigInput {
    fn core(&self) -> VelocityConfig {
        let mut value = VelocityConfig::default();
        value.minimum_observations = self.minimum_observations;
        value.robust = self.robust.as_ref().map(VelocityRobustConfigInput::core);
        value
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RangeRatePredictionOutput {
    los_unit: [f64; 3],
    range_rate_m_s: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VelocitySolutionOutput {
    velocity_m_s: [f64; 3],
    clock_drift_m_s: f64,
    residual_rms_m_s: f64,
    used_sats: Vec<String>,
    downweighted_sats: Vec<String>,
    robust_iterations: usize,
    robust_scale_m_s: Option<f64>,
}

fn from_js<T: for<'de> Deserialize<'de>>(value: JsValue, name: &str) -> Result<T, JsValue> {
    serde_wasm_bindgen::from_value(value)
        .map_err(|error| type_error(&format!("invalid {name}: {error}")))
}

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(
            &serde_wasm_bindgen::Serializer::new()
                .serialize_maps_as_objects(true)
                .serialize_missing_as_null(true),
        )
        .map_err(|error| type_error(&format!("could not marshal velocity result: {error}")))
}

fn solve_error(error: VelocitySolveError) -> JsValue {
    let detail = match &error {
        VelocitySolveError::TooFewObservations { required, actual } => serde_json::json!({
            "family": "VelocitySolveError",
            "kind": "TOO_FEW_OBSERVATIONS",
            "required": required,
            "actual": actual,
            "message": error.to_string(),
        }),
        VelocitySolveError::InvalidInput { field, reason } => serde_json::json!({
            "family": "VelocitySolveError",
            "kind": "INVALID_INPUT",
            "field": field,
            "reason": reason,
            "message": error.to_string(),
        }),
        VelocitySolveError::SingularGeometry => serde_json::json!({
            "family": "VelocitySolveError",
            "kind": "SINGULAR_GEOMETRY",
            "message": error.to_string(),
        }),
    };
    let exception: js_sys::Error = if matches!(error, VelocitySolveError::InvalidInput { .. }) {
        js_sys::RangeError::new(&error.to_string()).into()
    } else {
        js_sys::Error::new(&error.to_string())
    };
    let _ = js_sys::Reflect::set(
        exception.as_ref(),
        &JsValue::from_str("details"),
        &detail
            .serialize(&serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true))
            .unwrap_or(JsValue::UNDEFINED),
    );
    exception.into()
}

/// Construct a stationary receiver state at `positionM`.
#[wasm_bindgen(js_name = stationaryReceiverVelocityState, unchecked_return_type = "PppReceiverVelocityState")]
pub fn stationary_receiver_velocity_state(
    #[wasm_bindgen(unchecked_param_type = "number[] | Float64Array")] position_m: JsValue,
) -> Result<JsValue, JsValue> {
    let position_m: [f64; 3] = from_js(position_m, "receiver positionM")?;
    let state = ReceiverVelocityState::stationary_at(position_m);
    to_js(&serde_json::json!({
        "positionM": state.position_m,
        "velocityMS": state.velocity_m_s,
        "clockDriftMS": state.clock_drift_m_s,
    }))
}

/// Predict one satellite range rate from observation geometry and receiver state.
///
/// Returns `undefined` when the core cannot form a finite, nonzero line of sight.
#[wasm_bindgen(js_name = predictRangeRateMS, unchecked_return_type = "PppRangeRatePrediction | undefined")]
pub fn predict_range_rate_m_s(
    #[wasm_bindgen(unchecked_param_type = "PppVelocityObservation")] observation: JsValue,
    #[wasm_bindgen(unchecked_param_type = "PppReceiverVelocityState")] receiver: JsValue,
) -> Result<JsValue, JsValue> {
    let observation: VelocityObservationInput = from_js(observation, "velocity observation")?;
    let receiver: ReceiverVelocityStateInput = from_js(receiver, "receiver velocity state")?;
    match core_predict_range_rate_m_s(&observation.core()?, receiver.core()) {
        Some(value) => to_js(&RangeRatePredictionOutput {
            los_unit: value.los_unit,
            range_rate_m_s: value.range_rate_m_s,
        }),
        None => Ok(JsValue::UNDEFINED),
    }
}

/// Solve receiver velocity and clock drift from one epoch of range-rate rows.
#[wasm_bindgen(js_name = solveVelocityLeastSquares, unchecked_return_type = "PppVelocitySolution")]
pub fn solve_velocity_least_squares(
    #[wasm_bindgen(unchecked_param_type = "PppVelocityObservation[]")] observations: JsValue,
    #[wasm_bindgen(unchecked_param_type = "number[] | Float64Array")] receiver_position_m: JsValue,
    #[wasm_bindgen(unchecked_param_type = "PppVelocityConfig | null | undefined")] config: JsValue,
) -> Result<JsValue, JsValue> {
    let observations: Vec<VelocityObservationInput> =
        from_js(observations, "velocity observations")?;
    let observations = observations
        .iter()
        .map(VelocityObservationInput::core)
        .collect::<Result<Vec<_>, _>>()?;
    let receiver_position_m: [f64; 3] = from_js(receiver_position_m, "receiver positionM")?;
    let config: VelocityConfigInput = if config.is_null() || config.is_undefined() {
        VelocityConfigInput::default()
    } else {
        from_js(config, "velocity config")?
    };
    let value = core_solve_velocity(&observations, receiver_position_m, config.core())
        .map_err(solve_error)?;
    to_js(&VelocitySolutionOutput {
        velocity_m_s: value.velocity_m_s,
        clock_drift_m_s: value.clock_drift_m_s,
        residual_rms_m_s: value.residual_rms_m_s,
        used_sats: value.used_sats.iter().map(ToString::to_string).collect(),
        downweighted_sats: value
            .downweighted_sats
            .iter()
            .map(ToString::to_string)
            .collect(),
        robust_iterations: value.robust_iterations,
        robust_scale_m_s: value.robust_scale_m_s,
    })
}

#[wasm_bindgen(typescript_custom_section)]
const TS_PPP_VELOCITY: &str = r#"
/** Receiver state for precise-positioning range-rate prediction. Plain data compares structurally. */
export interface PppReceiverVelocityState { positionM: number[]; velocityMS: number[]; clockDriftMS: number; }
/** One Doppler-derived satellite range-rate row. Plain data compares structurally. */
export interface PppVelocityObservation {
  satelliteId: string;
  satellitePositionM: number[];
  satelliteVelocityMS: number[];
  measuredRangeRateMS: number;
  sigmaMS: number;
  satelliteClockDriftMS: number;
}
/** Optional Huber/MAD reweighting controls. */
export interface PppVelocityRobustConfig { huberK?: number; scaleFloorMS?: number; maxOuter?: number; outerTolMS?: number; }
/** Velocity least-squares controls; omitted members take core defaults. */
export interface PppVelocityConfig { minimumObservations?: number; robust?: PppVelocityRobustConfig | null; }
/** Predicted line of sight and range rate. Plain data compares structurally. */
export interface PppRangeRatePrediction { losUnit: number[]; rangeRateMS: number; }
/** Receiver velocity solution. Plain data compares structurally. */
export interface PppVelocitySolution {
  velocityMS: number[];
  clockDriftMS: number;
  residualRmsMS: number;
  usedSats: string[];
  downweightedSats: string[];
  robustIterations: number;
  robustScaleMS: number | null;
}
export type PppVelocitySolveErrorDetail =
  | { family: "VelocitySolveError"; kind: "TOO_FEW_OBSERVATIONS"; required: number; actual: number; message: string }
  | { family: "VelocitySolveError"; kind: "INVALID_INPUT"; field: string; reason: string; message: string }
  | { family: "VelocitySolveError"; kind: "SINGULAR_GEOMETRY"; message: string };
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_core_defaults() {
        let config = VelocityConfigInput::default().core();
        assert_eq!(config.minimum_observations, 4);
        assert_eq!(config.robust, None);
        let robust = VelocityRobustConfigInput::default().core();
        assert_eq!(robust.huber_k, 1.345);
        assert_eq!(robust.scale_floor_m_s, 0.05);
        assert_eq!(robust.max_outer, 2);
        assert_eq!(robust.outer_tol_m_s, 1.0e-6);
    }
}
