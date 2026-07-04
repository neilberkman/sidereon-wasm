//! Estimation and detection primitives.
//!
//! These functions are thin WASM exports over `sidereon_core::estimation`.
//! State and gain structs cross the JS boundary as plain objects with
//! `camelCase` fields. Numeric units match the core API: alpha-beta level and
//! rate are caller-defined scalar units, `dt` is seconds, NIS is dimensionless,
//! MAD uses the input sample unit, and CA-CFAR thresholds use the noise-level
//! unit supplied by the caller.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::estimation::{
    alpha_beta_apply_measurement as core_alpha_beta_apply_measurement,
    alpha_beta_filter_step as core_alpha_beta_filter_step,
    alpha_beta_predict as core_alpha_beta_predict,
    alpha_beta_steady_state_gains as core_alpha_beta_steady_state_gains,
    cfar_ca_false_alarm_probability as core_cfar_ca_false_alarm_probability,
    cfar_ca_multiplier_from_pfa as core_cfar_ca_multiplier_from_pfa,
    cfar_ca_pfa_from_multiplier as core_cfar_ca_pfa_from_multiplier,
    cfar_ca_threshold as core_cfar_ca_threshold, ewma_update as core_ewma_update,
    ewma_update_power_of_two as core_ewma_update_power_of_two,
    kalman_cv_steady_state_gains as core_kalman_cv_steady_state_gains,
    mad_spread as core_mad_spread, nis_expected_value as core_nis_expected_value,
    nis_gate_test as core_nis_gate_test, nis_gate_threshold as core_nis_gate_threshold,
    nis_statistic as core_nis_statistic, normalized_innovation as core_normalized_innovation,
    AlphaBetaGains as CoreAlphaBetaGains, AlphaBetaState as CoreAlphaBetaState, PrimitiveError,
    ScalarKalmanGains as CoreScalarKalmanGains, MAD_GAUSSIAN_CONSISTENCY,
};

use crate::error::{engine_error, range_error, type_error};

fn primitive_error(err: PrimitiveError) -> JsValue {
    range_error(&err.to_string())
}

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| engine_error(format!("failed to serialize result: {e}")))
}

fn from_js<T: for<'de> Deserialize<'de>>(value: JsValue, label: &str) -> Result<T, JsValue> {
    serde_wasm_bindgen::from_value(value).map_err(|e| type_error(&format!("invalid {label}: {e}")))
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AlphaBetaStateJs {
    level: f64,
    rate: f64,
}

impl From<AlphaBetaStateJs> for CoreAlphaBetaState {
    fn from(value: AlphaBetaStateJs) -> Self {
        Self {
            level: value.level,
            rate: value.rate,
        }
    }
}

impl From<CoreAlphaBetaState> for AlphaBetaStateJs {
    fn from(value: CoreAlphaBetaState) -> Self {
        Self {
            level: value.level,
            rate: value.rate,
        }
    }
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AlphaBetaGainsJs {
    alpha: f64,
    beta: f64,
}

impl From<AlphaBetaGainsJs> for CoreAlphaBetaGains {
    fn from(value: AlphaBetaGainsJs) -> Self {
        Self {
            alpha: value.alpha,
            beta: value.beta,
        }
    }
}

impl From<CoreAlphaBetaGains> for AlphaBetaGainsJs {
    fn from(value: CoreAlphaBetaGains) -> Self {
        Self {
            alpha: value.alpha,
            beta: value.beta,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AlphaBetaStepJs {
    predicted: AlphaBetaStateJs,
    updated: AlphaBetaStateJs,
    innovation: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScalarKalmanGainsJs {
    position_gain: f64,
    rate_gain: f64,
}

impl From<CoreScalarKalmanGains> for ScalarKalmanGainsJs {
    fn from(value: CoreScalarKalmanGains) -> Self {
        Self {
            position_gain: value.position_gain,
            rate_gain: value.rate_gain,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NisGateJs {
    nis: f64,
    threshold: f64,
    in_gate: bool,
    dof: usize,
}

/// Steady-state alpha-beta gains from a positive tracking index.
///
/// Returns `{ alpha, beta }`, where `alpha` is the level gain and `beta` maps
/// innovation to rate through `beta * innovation / dt`.
#[wasm_bindgen(js_name = alphaBetaSteadyStateGains)]
pub fn alpha_beta_steady_state_gains(tracking_index: f64) -> Result<JsValue, JsValue> {
    let gains = core_alpha_beta_steady_state_gains(tracking_index).map_err(primitive_error)?;
    to_js(&AlphaBetaGainsJs::from(gains))
}

/// Alpha-beta constant-rate prediction.
///
/// `state` is `{ level, rate }` and `dt` is the positive propagation interval in
/// seconds. Returns the predicted `{ level, rate }`.
#[wasm_bindgen(js_name = alphaBetaPredict)]
pub fn alpha_beta_predict(state: JsValue, dt: f64) -> Result<JsValue, JsValue> {
    let state: AlphaBetaStateJs = from_js(state, "alpha-beta state")?;
    let predicted = core_alpha_beta_predict(state.into(), dt).map_err(primitive_error)?;
    to_js(&AlphaBetaStateJs::from(predicted))
}

/// Alpha-beta measurement update applied to a predicted scalar state.
///
/// `predicted` is `{ level, rate }`, `measurement` has the same unit as
/// `level`, `dt` is seconds, and `gains` is `{ alpha, beta }`.
#[wasm_bindgen(js_name = alphaBetaApplyMeasurement)]
pub fn alpha_beta_apply_measurement(
    predicted: JsValue,
    measurement: f64,
    dt: f64,
    gains: JsValue,
) -> Result<JsValue, JsValue> {
    let predicted: AlphaBetaStateJs = from_js(predicted, "alpha-beta predicted state")?;
    let gains: AlphaBetaGainsJs = from_js(gains, "alpha-beta gains")?;
    let updated =
        core_alpha_beta_apply_measurement(predicted.into(), measurement, dt, gains.into())
            .map_err(primitive_error)?;
    to_js(&AlphaBetaStateJs::from(updated))
}

/// Run one alpha-beta predict and measurement update.
///
/// `state` is `{ level, rate }`, `measurement` has the same unit as `level`,
/// `dt` is seconds, and `gains` is `{ alpha, beta }`. Returns `{ predicted,
/// updated, innovation }`.
#[wasm_bindgen(js_name = alphaBetaFilterStep)]
pub fn alpha_beta_filter_step(
    state: JsValue,
    measurement: f64,
    dt: f64,
    gains: JsValue,
) -> Result<JsValue, JsValue> {
    let state: AlphaBetaStateJs = from_js(state, "alpha-beta state")?;
    let gains: AlphaBetaGainsJs = from_js(gains, "alpha-beta gains")?;
    let step = core_alpha_beta_filter_step(state.into(), measurement, dt, gains.into())
        .map_err(primitive_error)?;
    to_js(&AlphaBetaStepJs {
        predicted: step.predicted.into(),
        updated: step.updated.into(),
        innovation: step.innovation,
    })
}

/// Steady-state gains for a scalar constant-velocity Kalman filter.
///
/// `trackingIndex`, `dt`, and `measurementVariance` must be positive. Returns
/// `{ positionGain, rateGain }`; `rateGain * dt` equals the alpha-beta `beta`
/// gain for the same tracking index.
#[wasm_bindgen(js_name = kalmanCvSteadyStateGains)]
pub fn kalman_cv_steady_state_gains(
    tracking_index: f64,
    dt: f64,
    measurement_variance: f64,
) -> Result<JsValue, JsValue> {
    let gains = core_kalman_cv_steady_state_gains(tracking_index, dt, measurement_variance)
        .map_err(primitive_error)?;
    to_js(&ScalarKalmanGainsJs::from(gains))
}

/// Scalar normalized innovation `innovation / sqrt(innovationVariance)`.
#[wasm_bindgen(js_name = normalizedInnovation)]
pub fn normalized_innovation(innovation: f64, innovation_variance: f64) -> Result<f64, JsValue> {
    core_normalized_innovation(innovation, innovation_variance).map_err(primitive_error)
}

/// Scalar normalized innovation squared statistic.
#[wasm_bindgen(js_name = nis)]
pub fn nis(innovation: f64, innovation_variance: f64) -> Result<f64, JsValue> {
    core_nis_statistic(innovation, innovation_variance).map_err(primitive_error)
}

/// Bar-Shalom expected NIS value for `dof` measurement degrees of freedom.
#[wasm_bindgen(js_name = nisExpectedValue)]
pub fn nis_expected_value(dof: usize) -> Result<f64, JsValue> {
    core_nis_expected_value(dof).map_err(primitive_error)
}

/// Chi-square NIS gate threshold for `dof` and confidence probability.
#[wasm_bindgen(js_name = nisGateThreshold)]
pub fn nis_gate_threshold(dof: usize, confidence: f64) -> Result<f64, JsValue> {
    core_nis_gate_threshold(dof, confidence).map_err(primitive_error)
}

/// Test a scalar innovation against a chi-square NIS gate.
///
/// Returns `{ nis, threshold, inGate, dof }`, with `confidence` in `(0, 1)`.
#[wasm_bindgen(js_name = nisGate)]
pub fn nis_gate(
    innovation: f64,
    innovation_variance: f64,
    dof: usize,
    confidence: f64,
) -> Result<JsValue, JsValue> {
    let gate = core_nis_gate_test(innovation, innovation_variance, dof, confidence)
        .map_err(primitive_error)?;
    to_js(&NisGateJs {
        nis: gate.nis,
        threshold: gate.threshold,
        in_gate: gate.in_gate,
        dof: gate.dof,
    })
}

/// Gaussian consistency factor applied by [`madSpread`].
#[wasm_bindgen(js_name = madGaussianConsistency)]
pub fn mad_gaussian_consistency() -> f64 {
    MAD_GAUSSIAN_CONSISTENCY
}

/// Median absolute deviation spread estimate.
///
/// `values` is a JS number array. The returned spread is
/// `MAD_GAUSSIAN_CONSISTENCY * median(abs(value - median(values)))`, floored by
/// `scaleFloor`.
#[wasm_bindgen(js_name = madSpread)]
pub fn mad_spread(values: JsValue, scale_floor: f64) -> Result<f64, JsValue> {
    let values: Vec<f64> = from_js(values, "MAD values")?;
    core_mad_spread(&values, scale_floor).map_err(primitive_error)
}

/// Exponentially weighted moving-average update.
///
/// `alpha` must be in `[0, 1]`; the returned value is
/// `previous + alpha * (sample - previous)`.
#[wasm_bindgen(js_name = ewmaUpdate)]
pub fn ewma_update(previous: f64, sample: f64, alpha: f64) -> Result<f64, JsValue> {
    core_ewma_update(previous, sample, alpha).map_err(primitive_error)
}

/// EWMA update with `alpha = 1 / 2^shift`.
#[wasm_bindgen(js_name = ewmaUpdatePowerOfTwo)]
pub fn ewma_update_power_of_two(previous: f64, sample: f64, shift: u32) -> Result<f64, JsValue> {
    core_ewma_update_power_of_two(previous, sample, shift).map_err(primitive_error)
}

/// CA-CFAR threshold multiplier from searched-cell count and target false alarm
/// probability.
#[wasm_bindgen(js_name = cfarCaMultiplierFromPfa)]
pub fn cfar_ca_multiplier_from_pfa(
    searched_cells: usize,
    false_alarm_probability: f64,
) -> Result<f64, JsValue> {
    core_cfar_ca_multiplier_from_pfa(searched_cells, false_alarm_probability)
        .map_err(primitive_error)
}

/// CA-CFAR false alarm probability from searched-cell count and multiplier.
#[wasm_bindgen(js_name = cfarCaPfaFromMultiplier)]
pub fn cfar_ca_pfa_from_multiplier(searched_cells: usize, multiplier: f64) -> Result<f64, JsValue> {
    core_cfar_ca_pfa_from_multiplier(searched_cells, multiplier).map_err(primitive_error)
}

/// CA-CFAR absolute threshold from searched-cell count, target false alarm
/// probability, and mean noise level.
#[wasm_bindgen(js_name = cfarCaThreshold)]
pub fn cfar_ca_threshold(
    searched_cells: usize,
    false_alarm_probability: f64,
    noise_level: f64,
) -> Result<f64, JsValue> {
    core_cfar_ca_threshold(searched_cells, false_alarm_probability, noise_level)
        .map_err(primitive_error)
}

/// CA-CFAR false alarm probability from searched-cell count, absolute
/// threshold, and mean noise level.
#[wasm_bindgen(js_name = cfarCaFalseAlarmProbability)]
pub fn cfar_ca_false_alarm_probability(
    searched_cells: usize,
    threshold: f64,
    noise_level: f64,
) -> Result<f64, JsValue> {
    core_cfar_ca_false_alarm_probability(searched_cells, threshold, noise_level)
        .map_err(primitive_error)
}
