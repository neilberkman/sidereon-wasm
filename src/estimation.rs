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

use sidereon_core::estimation::primitives::NisGate;
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
    smooth_track_rts as core_smooth_track_rts, AlphaBetaGains as CoreAlphaBetaGains,
    AlphaBetaState as CoreAlphaBetaState, PrimitiveError,
    ScalarKalmanGains as CoreScalarKalmanGains, SmoothedTrack as CoreSmoothedTrack,
    SmoothedTrackEpoch as CoreSmoothedTrackEpoch, TrackCoordinateFrame, TrackError,
    TrackFilter as CoreTrackFilter, TrackFilterConfig as CoreTrackFilterConfig,
    TrackGatedUpdate as CoreTrackGatedUpdate, TrackInnovation as CoreTrackInnovation,
    TrackPrediction as CoreTrackPrediction, TrackRtsEpoch as CoreTrackRtsEpoch,
    TrackRtsHistory as CoreTrackRtsHistory, TrackRtsHistoryBuilder as CoreTrackRtsHistoryBuilder,
    TrackState as CoreTrackState, TrackUpdate as CoreTrackUpdate, MAD_GAUSSIAN_CONSISTENCY,
};

use crate::error::{engine_error, range_error, type_error};

fn primitive_error(err: PrimitiveError) -> JsValue {
    range_error(&err.to_string())
}

fn track_error(err: TrackError) -> JsValue {
    match err {
        TrackError::DimensionMismatch { .. } => type_error(&err.to_string()),
        TrackError::InvalidInput { .. }
        | TrackError::NonPositiveDefinite { .. }
        | TrackError::NonPositiveSemidefinite { .. } => range_error(&err.to_string()),
    }
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

impl From<NisGate> for NisGateJs {
    fn from(value: NisGate) -> Self {
        Self {
            nis: value.nis,
            threshold: value.threshold,
            in_gate: value.in_gate,
            dof: value.dof,
        }
    }
}

fn frame_from_label(label: &str) -> Result<TrackCoordinateFrame, JsValue> {
    match label {
        "ecef" | "ECEF" => Ok(TrackCoordinateFrame::Ecef),
        "enu" | "ENU" => Ok(TrackCoordinateFrame::Enu),
        "callerDefinedCartesian" | "caller_defined_cartesian" => {
            Ok(TrackCoordinateFrame::CallerDefinedCartesian)
        }
        other => Err(type_error(&format!(
            "invalid track frame {other:?}: expected \"ecef\", \"enu\", or \"callerDefinedCartesian\""
        ))),
    }
}

fn frame_label(frame: TrackCoordinateFrame) -> &'static str {
    match frame {
        TrackCoordinateFrame::Ecef => "ecef",
        TrackCoordinateFrame::Enu => "enu",
        TrackCoordinateFrame::CallerDefinedCartesian => "callerDefinedCartesian",
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrackFilterConfigInput {
    frame: String,
    initial_t_s: f64,
    initial_position_m: Vec<f64>,
    initial_velocity_m_s: Vec<f64>,
    initial_covariance: Vec<Vec<f64>>,
    acceleration_variance_spectral_density_m2_s3: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrackFilterFromPositionInput {
    frame: String,
    initial_t_s: f64,
    initial_position_m: Vec<f64>,
    position_covariance_m2: Vec<Vec<f64>>,
    initial_velocity_variance_m2_s2: f64,
    acceleration_variance_spectral_density_m2_s3: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrackPositionObservationInput {
    position_m: Vec<f64>,
    covariance_m2: Vec<Vec<f64>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrackStateObservationInput {
    state: Vec<f64>,
    covariance: Vec<Vec<f64>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrackGatedPositionObservationInput {
    position_m: Vec<f64>,
    covariance_m2: Vec<Vec<f64>>,
    confidence: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackFilterConfigJs {
    frame: &'static str,
    initial_t_s: f64,
    initial_position_m: Vec<f64>,
    initial_velocity_m_s: Vec<f64>,
    initial_covariance: Vec<Vec<f64>>,
    acceleration_variance_spectral_density_m2_s3: f64,
    dimension: usize,
}

impl From<&CoreTrackFilterConfig> for TrackFilterConfigJs {
    fn from(value: &CoreTrackFilterConfig) -> Self {
        Self {
            frame: frame_label(value.frame),
            initial_t_s: value.initial_t_s,
            initial_position_m: value.initial_position_m.clone(),
            initial_velocity_m_s: value.initial_velocity_m_s.clone(),
            initial_covariance: value.initial_covariance.clone(),
            acceleration_variance_spectral_density_m2_s3: value
                .acceleration_variance_spectral_density_m2_s3,
            dimension: value.dimension(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackStateJs {
    frame: &'static str,
    t_s: f64,
    position_m: Vec<f64>,
    velocity_m_s: Vec<f64>,
    covariance: Vec<Vec<f64>>,
    state_vector: Vec<f64>,
    position_covariance_m2: Vec<Vec<f64>>,
    dimension: usize,
    state_dimension: usize,
}

impl From<&CoreTrackState> for TrackStateJs {
    fn from(value: &CoreTrackState) -> Self {
        Self {
            frame: frame_label(value.frame),
            t_s: value.t_s,
            position_m: value.position_m.clone(),
            velocity_m_s: value.velocity_m_s.clone(),
            covariance: value.covariance.clone(),
            state_vector: value.state_vector(),
            position_covariance_m2: value.position_covariance_m2(),
            dimension: value.dimension(),
            state_dimension: value.state_dimension(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackPredictionJs {
    dt_s: f64,
    transition: Vec<Vec<f64>>,
    process_noise: Vec<Vec<f64>>,
    predicted: TrackStateJs,
}

impl From<&CoreTrackPrediction> for TrackPredictionJs {
    fn from(value: &CoreTrackPrediction) -> Self {
        Self {
            dt_s: value.dt_s,
            transition: value.transition.clone(),
            process_noise: value.process_noise.clone(),
            predicted: TrackStateJs::from(&value.predicted),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackInnovationJs {
    innovation: Vec<f64>,
    innovation_covariance: Vec<Vec<f64>>,
    nis: f64,
}

impl From<&CoreTrackInnovation> for TrackInnovationJs {
    fn from(value: &CoreTrackInnovation) -> Self {
        Self {
            innovation: value.innovation.clone(),
            innovation_covariance: value.innovation_covariance.clone(),
            nis: value.nis,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackUpdateJs {
    predicted: TrackStateJs,
    updated: TrackStateJs,
    innovation: TrackInnovationJs,
    kalman_gain: Vec<Vec<f64>>,
}

impl From<&CoreTrackUpdate> for TrackUpdateJs {
    fn from(value: &CoreTrackUpdate) -> Self {
        Self {
            predicted: TrackStateJs::from(&value.predicted),
            updated: TrackStateJs::from(&value.updated),
            innovation: TrackInnovationJs::from(&value.innovation),
            kalman_gain: value.kalman_gain.clone(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackGatedUpdateJs {
    gate: NisGateJs,
    update: Option<TrackUpdateJs>,
    state: TrackStateJs,
}

impl From<&CoreTrackGatedUpdate> for TrackGatedUpdateJs {
    fn from(value: &CoreTrackGatedUpdate) -> Self {
        Self {
            gate: value.gate.into(),
            update: value.update.as_ref().map(TrackUpdateJs::from),
            state: TrackStateJs::from(&value.state),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackRtsEpochJs {
    t_s: f64,
    predicted: TrackStateJs,
    updated: TrackStateJs,
    transition_from_previous: Option<Vec<Vec<f64>>>,
}

impl From<&CoreTrackRtsEpoch> for TrackRtsEpochJs {
    fn from(value: &CoreTrackRtsEpoch) -> Self {
        Self {
            t_s: value.t_s,
            predicted: TrackStateJs::from(&value.predicted),
            updated: TrackStateJs::from(&value.updated),
            transition_from_previous: value.transition_from_previous.clone(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SmoothedTrackEpochJs {
    t_s: f64,
    state: TrackStateJs,
    rts_gain_to_next: Option<Vec<Vec<f64>>>,
}

impl From<&CoreSmoothedTrackEpoch> for SmoothedTrackEpochJs {
    fn from(value: &CoreSmoothedTrackEpoch) -> Self {
        Self {
            t_s: value.t_s,
            state: TrackStateJs::from(&value.state),
            rts_gain_to_next: value.rts_gain_to_next.clone(),
        }
    }
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct TrackFilterConfig {
    inner: CoreTrackFilterConfig,
}

#[wasm_bindgen]
impl TrackFilterConfig {
    /// Build a no-IMU track filter config from a JS object.
    #[wasm_bindgen(constructor)]
    pub fn new(config: JsValue) -> Result<TrackFilterConfig, JsValue> {
        let config: TrackFilterConfigInput = from_js(config, "track filter config")?;
        let inner = CoreTrackFilterConfig::from_position_velocity(
            frame_from_label(&config.frame)?,
            config.initial_t_s,
            config.initial_position_m,
            config.initial_velocity_m_s,
            config.initial_covariance,
            config.acceleration_variance_spectral_density_m2_s3,
        )
        .map_err(track_error)?;
        Ok(Self { inner })
    }

    /// Build a config from a position fix and uncertain zero initial velocity.
    #[wasm_bindgen(js_name = fromPosition)]
    pub fn from_position(config: JsValue) -> Result<TrackFilterConfig, JsValue> {
        let config: TrackFilterFromPositionInput = from_js(config, "track position config")?;
        let inner = CoreTrackFilterConfig::from_position(
            frame_from_label(&config.frame)?,
            config.initial_t_s,
            config.initial_position_m,
            config.position_covariance_m2,
            config.initial_velocity_variance_m2_s2,
            config.acceleration_variance_spectral_density_m2_s3,
        )
        .map_err(track_error)?;
        Ok(Self { inner })
    }

    #[wasm_bindgen(getter)]
    pub fn dimension(&self) -> usize {
        self.inner.dimension()
    }

    #[wasm_bindgen(getter)]
    pub fn frame(&self) -> String {
        frame_label(self.inner.frame).to_owned()
    }

    #[wasm_bindgen(getter, js_name = initialTS)]
    pub fn initial_t_s(&self) -> f64 {
        self.inner.initial_t_s
    }

    #[wasm_bindgen(js_name = toObject)]
    pub fn to_object(&self) -> Result<JsValue, JsValue> {
        to_js(&TrackFilterConfigJs::from(&self.inner))
    }
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct TrackFilter {
    inner: CoreTrackFilter,
}

#[wasm_bindgen]
impl TrackFilter {
    /// Build a stateful no-IMU track filter from a `TrackFilterConfig`.
    #[wasm_bindgen(constructor)]
    pub fn new(config: &TrackFilterConfig) -> Result<TrackFilter, JsValue> {
        let inner = CoreTrackFilter::new(config.inner.clone()).map_err(track_error)?;
        Ok(Self { inner })
    }

    /// Build a filter from a position fix and uncertain zero initial velocity.
    #[wasm_bindgen(js_name = fromPosition)]
    pub fn from_position(config: JsValue) -> Result<TrackFilter, JsValue> {
        let config = TrackFilterConfig::from_position(config)?;
        Self::new(&config)
    }

    #[wasm_bindgen(getter)]
    pub fn state(&self) -> Result<JsValue, JsValue> {
        to_js(&TrackStateJs::from(self.inner.state()))
    }

    #[wasm_bindgen(getter)]
    pub fn dimension(&self) -> usize {
        self.inner.dimension()
    }

    #[wasm_bindgen(getter, js_name = accelerationVarianceSpectralDensityM2S3)]
    pub fn acceleration_variance_spectral_density_m2_s3(&self) -> f64 {
        self.inner.acceleration_variance_spectral_density_m2_s3()
    }

    pub fn predict(&mut self, dt_s: f64) -> Result<JsValue, JsValue> {
        let prediction = self.inner.predict(dt_s).map_err(track_error)?;
        to_js(&TrackPredictionJs::from(&prediction))
    }

    #[wasm_bindgen(js_name = predictRecorded)]
    pub fn predict_recorded(
        &mut self,
        dt_s: f64,
        history: &mut TrackRtsHistoryBuilder,
    ) -> Result<JsValue, JsValue> {
        let prediction = self
            .inner
            .predict_recorded(dt_s, &mut history.inner)
            .map_err(track_error)?;
        to_js(&TrackPredictionJs::from(&prediction))
    }

    #[wasm_bindgen(js_name = positionInnovation)]
    pub fn position_innovation(&self, request: JsValue) -> Result<JsValue, JsValue> {
        let request: TrackPositionObservationInput =
            from_js(request, "track position observation")?;
        let innovation = self
            .inner
            .position_innovation(&request.position_m, &request.covariance_m2)
            .map_err(track_error)?;
        to_js(&TrackInnovationJs::from(&innovation))
    }

    #[wasm_bindgen(js_name = stateInnovation)]
    pub fn state_innovation(&self, request: JsValue) -> Result<JsValue, JsValue> {
        let request: TrackStateObservationInput = from_js(request, "track state observation")?;
        let innovation = self
            .inner
            .state_innovation(&request.state, &request.covariance)
            .map_err(track_error)?;
        to_js(&TrackInnovationJs::from(&innovation))
    }

    #[wasm_bindgen(js_name = updatePosition)]
    pub fn update_position(&mut self, request: JsValue) -> Result<JsValue, JsValue> {
        let request: TrackPositionObservationInput =
            from_js(request, "track position observation")?;
        let update = self
            .inner
            .update_position(&request.position_m, &request.covariance_m2)
            .map_err(track_error)?;
        to_js(&TrackUpdateJs::from(&update))
    }

    #[wasm_bindgen(js_name = updateState)]
    pub fn update_state(&mut self, request: JsValue) -> Result<JsValue, JsValue> {
        let request: TrackStateObservationInput = from_js(request, "track state observation")?;
        let update = self
            .inner
            .update_state(&request.state, &request.covariance)
            .map_err(track_error)?;
        to_js(&TrackUpdateJs::from(&update))
    }

    #[wasm_bindgen(js_name = updatePositionGated)]
    pub fn update_position_gated(&mut self, request: JsValue) -> Result<JsValue, JsValue> {
        let request: TrackGatedPositionObservationInput =
            from_js(request, "gated track position observation")?;
        let update = self
            .inner
            .update_position_gated(
                &request.position_m,
                &request.covariance_m2,
                request.confidence,
            )
            .map_err(track_error)?;
        to_js(&TrackGatedUpdateJs::from(&update))
    }

    #[wasm_bindgen(js_name = updatePositionRecorded)]
    pub fn update_position_recorded(
        &mut self,
        request: JsValue,
        history: &mut TrackRtsHistoryBuilder,
    ) -> Result<JsValue, JsValue> {
        let request: TrackPositionObservationInput =
            from_js(request, "track position observation")?;
        let update = self
            .inner
            .update_position_recorded(
                &request.position_m,
                &request.covariance_m2,
                &mut history.inner,
            )
            .map_err(track_error)?;
        to_js(&TrackUpdateJs::from(&update))
    }

    #[wasm_bindgen(js_name = updatePositionGatedRecorded)]
    pub fn update_position_gated_recorded(
        &mut self,
        request: JsValue,
        history: &mut TrackRtsHistoryBuilder,
    ) -> Result<JsValue, JsValue> {
        let request: TrackGatedPositionObservationInput =
            from_js(request, "gated track position observation")?;
        let update = self
            .inner
            .update_position_gated_recorded(
                &request.position_m,
                &request.covariance_m2,
                request.confidence,
                &mut history.inner,
            )
            .map_err(track_error)?;
        to_js(&TrackGatedUpdateJs::from(&update))
    }

    #[wasm_bindgen(js_name = recordPredictionOnly)]
    pub fn record_prediction_only(
        &self,
        history: &mut TrackRtsHistoryBuilder,
    ) -> Result<(), JsValue> {
        self.inner
            .record_prediction_only(&mut history.inner)
            .map_err(track_error)
    }
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct TrackRtsHistoryBuilder {
    inner: CoreTrackRtsHistoryBuilder,
}

#[wasm_bindgen]
impl TrackRtsHistoryBuilder {
    /// Start an empty history for manual recording.
    #[wasm_bindgen(constructor)]
    pub fn new() -> TrackRtsHistoryBuilder {
        Self {
            inner: CoreTrackRtsHistoryBuilder::empty(),
        }
    }

    /// Start a history from the filter's current state.
    #[wasm_bindgen(js_name = fromFilter)]
    pub fn from_filter(filter: &TrackFilter) -> Result<TrackRtsHistoryBuilder, JsValue> {
        let inner = CoreTrackRtsHistoryBuilder::from_filter(&filter.inner).map_err(track_error)?;
        Ok(Self { inner })
    }

    pub fn finish(&self) -> Result<TrackRtsHistory, JsValue> {
        let inner = self.inner.clone().finish().map_err(track_error)?;
        Ok(TrackRtsHistory { inner })
    }
}

impl Default for TrackRtsHistoryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct TrackRtsHistory {
    inner: CoreTrackRtsHistory,
}

#[wasm_bindgen]
impl TrackRtsHistory {
    #[wasm_bindgen(getter)]
    pub fn epochs(&self) -> Result<JsValue, JsValue> {
        let epochs = self
            .inner
            .epochs
            .iter()
            .map(TrackRtsEpochJs::from)
            .collect::<Vec<_>>();
        to_js(&epochs)
    }

    #[wasm_bindgen(getter, js_name = epochCount)]
    pub fn epoch_count(&self) -> usize {
        self.inner.epochs.len()
    }
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct SmoothedTrack {
    inner: CoreSmoothedTrack,
}

#[wasm_bindgen]
impl SmoothedTrack {
    #[wasm_bindgen(getter)]
    pub fn epochs(&self) -> Result<JsValue, JsValue> {
        let epochs = self
            .inner
            .epochs
            .iter()
            .map(SmoothedTrackEpochJs::from)
            .collect::<Vec<_>>();
        to_js(&epochs)
    }

    #[wasm_bindgen(getter, js_name = epochCount)]
    pub fn epoch_count(&self) -> usize {
        self.inner.epochs.len()
    }
}

#[wasm_bindgen(js_name = smoothTrackRts)]
pub fn smooth_track_rts(history: &TrackRtsHistory) -> Result<SmoothedTrack, JsValue> {
    let inner = core_smooth_track_rts(&history.inner).map_err(track_error)?;
    Ok(SmoothedTrack { inner })
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
