//! Sequential kinematic precise-point-positioning filter bindings.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::astro::time::DegradeReason;
use sidereon_core::precise_positioning::{
    correct_kinematic_state as core_correct, predict_kinematic_state as core_predict,
    solve_kinematic_ppp as core_solve, KinematicConfig, KinematicEpochSolution,
    KinematicEpochStatus, KinematicMotionModel, KinematicPositionProcessNoise,
    KinematicProcessNoise, KinematicSolveError, KinematicState, KinematicUpdateSummary,
    MissingCorrection, NoEphemerisReason, SsrBiasExclusion,
};

use crate::error::type_error;
use crate::ppp::{epochs_from_js, unplaced_observations, TroposphereInput, WeightsInput};
use crate::ppp_range::PppRangeCorrections;
use crate::sp3::Sp3;

fn to_js<T: Serialize>(value: &T) -> JsValue {
    value
        .serialize(&serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true))
        .unwrap_or(JsValue::UNDEFINED)
}

fn vec3(value: &[f64], name: &str) -> Result<[f64; 3], JsValue> {
    value
        .try_into()
        .map_err(|_| type_error(&format!("{name} must contain exactly three numbers")))
}

fn matrix(value: JsValue, name: &str) -> Result<Vec<Vec<f64>>, JsValue> {
    serde_wasm_bindgen::from_value(value)
        .map_err(|error| type_error(&format!("invalid {name}: {error}")))
}

fn map(value: JsValue, name: &str) -> Result<BTreeMap<String, f64>, JsValue> {
    serde_wasm_bindgen::from_value(value)
        .map_err(|error| type_error(&format!("invalid {name}: {error}")))
}

fn error_detail(error: &KinematicSolveError) -> serde_json::Value {
    let message = error.to_string();
    match error {
        KinematicSolveError::NoEphemeris {
            satellite_id,
            reason,
        } => {
            let reason = match reason {
                NoEphemerisReason::NoEphemeris => {
                    serde_json::json!({"kind":"NO_EPHEMERIS","message":reason.to_string()})
                }
                NoEphemerisReason::MissingSatelliteClock => {
                    serde_json::json!({"kind":"MISSING_SATELLITE_CLOCK","message":reason.to_string()})
                }
                NoEphemerisReason::Reason(detail) => {
                    serde_json::json!({"kind":"REASON","detail":detail,"message":reason.to_string()})
                }
            };
            serde_json::json!({"family":"KinematicSolveError","kind":"NO_EPHEMERIS","satelliteId":satellite_id,"reason":reason,"message":message})
        }
        KinematicSolveError::SingularGeometry => {
            serde_json::json!({"family":"KinematicSolveError","kind":"SINGULAR_GEOMETRY","message":message})
        }
        KinematicSolveError::InvalidSolveOption { field, reason } => {
            serde_json::json!({"family":"KinematicSolveError","kind":"INVALID_SOLVE_OPTION","field":field,"reason":reason,"message":message})
        }
        KinematicSolveError::InvalidInput { field, reason } => {
            serde_json::json!({"family":"KinematicSolveError","kind":"INVALID_INPUT","field":field,"reason":reason,"message":message})
        }
        KinematicSolveError::MissingCorrection {
            satellite_id,
            correction,
        } => {
            let (kind, label) = match correction {
                MissingCorrection::SolidEarthTide => ("SOLID_EARTH_TIDE", None),
                MissingCorrection::PoleTide => ("POLE_TIDE", None),
                MissingCorrection::OceanLoading => ("OCEAN_LOADING", None),
                MissingCorrection::PhaseWindup => ("PHASE_WINDUP", None),
                MissingCorrection::SatelliteAntennaPco => ("SATELLITE_ANTENNA_PCO", None),
                MissingCorrection::SatelliteAntennaPcv => ("SATELLITE_ANTENNA_PCV", None),
                MissingCorrection::CodeBias => ("CODE_BIAS", None),
                MissingCorrection::SsrCodeBias => ("SSR_CODE_BIAS", None),
                MissingCorrection::PhaseBias => ("PHASE_BIAS", None),
                MissingCorrection::ReceiverAntennaFrequency(v) => {
                    ("RECEIVER_ANTENNA_FREQUENCY", Some(v.as_str()))
                }
                MissingCorrection::ReceiverAntennaPcv(v) => {
                    ("RECEIVER_ANTENNA_PCV", Some(v.as_str()))
                }
                MissingCorrection::ReceiverAntennaGeometry => ("RECEIVER_ANTENNA_GEOMETRY", None),
            };
            serde_json::json!({"family":"KinematicSolveError","kind":"MISSING_CORRECTION","satelliteId":satellite_id,"correction":{"kind":kind,"label":label,"message":correction.to_string()},"message":message})
        }
        KinematicSolveError::Ut1OutsideCoverage(reason) => {
            serde_json::json!({"family":"KinematicSolveError","kind":"UT1_OUTSIDE_COVERAGE","reasonKind":match reason { DegradeReason::BeforeCoverage => "BEFORE_COVERAGE", DegradeReason::AfterCoverage => "AFTER_COVERAGE" },"reason":reason.to_string(),"message":message})
        }
    }
}

fn solve_error(error: KinematicSolveError) -> JsValue {
    let message = error.to_string();
    let js: JsValue = match error {
        KinematicSolveError::InvalidInput { .. }
        | KinematicSolveError::InvalidSolveOption { .. } => {
            js_sys::RangeError::new(&message).into()
        }
        _ => js_sys::Error::new(&message).into(),
    };
    let _ = js_sys::Reflect::set(&js, &"details".into(), &to_js(&error_detail(&error)));
    js
}

/// Position process-noise model used by kinematic prediction.
#[wasm_bindgen]
#[derive(Clone)]
pub struct PppKinematicPositionProcessNoise {
    inner: KinematicPositionProcessNoise,
}

#[wasm_bindgen]
impl PppKinematicPositionProcessNoise {
    #[wasm_bindgen(js_name = default)]
    pub fn default_js() -> Self {
        Self {
            inner: Default::default(),
        }
    }
    #[wasm_bindgen(js_name = randomWalk)]
    pub fn random_walk(spectral_density_m2_s: f64) -> Self {
        Self {
            inner: KinematicPositionProcessNoise::RandomWalk {
                spectral_density_m2_s,
            },
        }
    }
    #[wasm_bindgen(js_name = whiteNoiseAcceleration)]
    pub fn white_noise_acceleration(spectral_density_m2_s3: f64) -> Self {
        Self {
            inner: KinematicPositionProcessNoise::WhiteNoiseAcceleration {
                spectral_density_m2_s3,
            },
        }
    }
    #[wasm_bindgen(getter, unchecked_return_type = "PppKinematicPositionProcessNoiseKind")]
    pub fn kind(&self) -> String {
        match self.inner {
            KinematicPositionProcessNoise::RandomWalk { .. } => "randomWalk",
            KinematicPositionProcessNoise::WhiteNoiseAcceleration { .. } => {
                "whiteNoiseAcceleration"
            }
        }
        .into()
    }
    #[wasm_bindgen(getter, js_name = spectralDensityM2S)]
    pub fn spectral_density_m2_s(&self) -> Option<f64> {
        match self.inner {
            KinematicPositionProcessNoise::RandomWalk {
                spectral_density_m2_s,
            } => Some(spectral_density_m2_s),
            _ => None,
        }
    }
    #[wasm_bindgen(getter, js_name = spectralDensityM2S3)]
    pub fn spectral_density_m2_s3(&self) -> Option<f64> {
        match self.inner {
            KinematicPositionProcessNoise::WhiteNoiseAcceleration {
                spectral_density_m2_s3,
            } => Some(spectral_density_m2_s3),
            _ => None,
        }
    }
    pub fn equals(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

/// Deterministic receiver motion model.
#[wasm_bindgen]
#[derive(Clone)]
pub struct PppKinematicMotionModel {
    inner: KinematicMotionModel,
}
#[wasm_bindgen]
impl PppKinematicMotionModel {
    #[wasm_bindgen(js_name = default)]
    pub fn default_js() -> Self {
        Self {
            inner: Default::default(),
        }
    }
    pub fn hold() -> Self {
        Self {
            inner: KinematicMotionModel::Hold,
        }
    }
    #[wasm_bindgen(js_name = constantVelocity)]
    pub fn constant_velocity(velocity_m_s: &[f64]) -> Result<Self, JsValue> {
        Ok(Self {
            inner: KinematicMotionModel::ConstantVelocity {
                velocity_m_s: vec3(velocity_m_s, "velocityMPerS")?,
            },
        })
    }
    #[wasm_bindgen(getter, unchecked_return_type = "PppKinematicMotionModelKind")]
    pub fn kind(&self) -> String {
        match self.inner {
            KinematicMotionModel::Hold => "hold",
            KinematicMotionModel::ConstantVelocity { .. } => "constantVelocity",
        }
        .into()
    }
    #[wasm_bindgen(getter, js_name = velocityMPerS)]
    pub fn velocity_m_s(&self) -> Option<Vec<f64>> {
        match self.inner {
            KinematicMotionModel::ConstantVelocity { velocity_m_s } => Some(velocity_m_s.to_vec()),
            _ => None,
        }
    }
    pub fn equals(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

/// Kinematic filter process-noise settings.
#[wasm_bindgen]
#[derive(Clone)]
pub struct PppKinematicProcessNoise {
    inner: KinematicProcessNoise,
}
#[wasm_bindgen]
impl PppKinematicProcessNoise {
    #[wasm_bindgen(constructor)]
    pub fn new(
        position: Option<PppKinematicPositionProcessNoise>,
        clock_white_m2_s: Option<f64>,
        ztd_random_walk_m2_s: Option<f64>,
        ambiguity_hold_m2_s: Option<f64>,
    ) -> Self {
        let mut inner = KinematicProcessNoise::default();
        if let Some(v) = position {
            inner.position = v.inner
        }
        if let Some(v) = clock_white_m2_s {
            inner.clock_white_m2_s = v
        }
        if let Some(v) = ztd_random_walk_m2_s {
            inner.ztd_random_walk_m2_s = v
        }
        if let Some(v) = ambiguity_hold_m2_s {
            inner.ambiguity_hold_m2_s = v
        }
        Self { inner }
    }
    #[wasm_bindgen(js_name = default)]
    pub fn default_js() -> Self {
        Self {
            inner: Default::default(),
        }
    }
    #[wasm_bindgen(getter)]
    pub fn position(&self) -> PppKinematicPositionProcessNoise {
        PppKinematicPositionProcessNoise {
            inner: self.inner.position,
        }
    }
    #[wasm_bindgen(getter, js_name=clockWhiteM2S)]
    pub fn clock(&self) -> f64 {
        self.inner.clock_white_m2_s
    }
    #[wasm_bindgen(getter, js_name=ztdRandomWalkM2S)]
    pub fn ztd(&self) -> f64 {
        self.inner.ztd_random_walk_m2_s
    }
    #[wasm_bindgen(getter, js_name=ambiguityHoldM2S)]
    pub fn ambiguity(&self) -> f64 {
        self.inner.ambiguity_hold_m2_s
    }
    pub fn equals(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

/// Mutable kinematic engine state and its covariance.
#[wasm_bindgen]
#[derive(Clone)]
pub struct PppKinematicState {
    inner: KinematicState,
    covariance_m2: Vec<Vec<f64>>,
}
#[wasm_bindgen]
impl PppKinematicState {
    #[wasm_bindgen(constructor)]
    pub fn new(
        position_m: &[f64],
        clock_m: f64,
        ztd_residual_m: f64,
        ambiguities_m: JsValue,
        covariance_m2: JsValue,
    ) -> Result<Self, JsValue> {
        Ok(Self {
            inner: KinematicState {
                position_m: vec3(position_m, "positionM")?,
                clock_m,
                ztd_residual_m,
                ambiguities_m: map(ambiguities_m, "ambiguitiesM")?,
            },
            covariance_m2: matrix(covariance_m2, "covarianceM2")?,
        })
    }
    #[wasm_bindgen(js_name=default)]
    pub fn default_js() -> Self {
        let c = KinematicConfig::default();
        Self {
            inner: c.initial_state,
            covariance_m2: c.initial_covariance_m2,
        }
    }
    #[wasm_bindgen(getter,js_name=positionM)]
    pub fn position(&self) -> Vec<f64> {
        self.inner.position_m.to_vec()
    }
    #[wasm_bindgen(getter,js_name=clockM)]
    pub fn clock(&self) -> f64 {
        self.inner.clock_m
    }
    #[wasm_bindgen(getter,js_name=ztdResidualM)]
    pub fn ztd(&self) -> f64 {
        self.inner.ztd_residual_m
    }
    #[wasm_bindgen(getter,js_name=ambiguitiesM, unchecked_return_type = "Record<string, number>")]
    pub fn ambiguities(&self) -> JsValue {
        to_js(&self.inner.ambiguities_m)
    }
    #[wasm_bindgen(getter,js_name=covarianceM2, unchecked_return_type = "number[][]")]
    pub fn covariance(&self) -> JsValue {
        to_js(&self.covariance_m2)
    }
    pub fn dimension(&self) -> usize {
        self.inner.dimension()
    }
    pub fn equals(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct MeasurementInput {
    weights: WeightsInput,
    tropo: TroposphereInput,
}
impl Default for MeasurementInput {
    fn default() -> Self {
        Self {
            weights: serde_json::from_value(serde_json::json!({})).unwrap(),
            tropo: serde_json::from_value(serde_json::json!({})).unwrap(),
        }
    }
}

/// Complete kinematic PPP configuration.
#[wasm_bindgen]
#[derive(Clone)]
pub struct PppKinematicConfig {
    inner: KinematicConfig,
    measurement: JsValue,
}
#[wasm_bindgen]
impl PppKinematicConfig {
    #[wasm_bindgen(constructor)]
    pub fn new(
        initial_state: Option<PppKinematicState>,
        motion: Option<PppKinematicMotionModel>,
        process_noise: Option<PppKinematicProcessNoise>,
        new_ambiguity_variance_m2: Option<f64>,
        measurement: JsValue,
        corrections: Option<PppRangeCorrections>,
    ) -> Result<Self, JsValue> {
        let mut inner = KinematicConfig::default();
        if let Some(v) = initial_state {
            inner.initial_state = v.inner;
            inner.initial_covariance_m2 = v.covariance_m2
        }
        if let Some(v) = motion {
            inner.motion = v.inner
        }
        if let Some(v) = process_noise {
            inner.process_noise = v.inner
        }
        if let Some(v) = new_ambiguity_variance_m2 {
            inner.new_ambiguity_variance_m2 = v
        }
        let measurement = if measurement.is_null() || measurement.is_undefined() {
            to_js(
                &serde_json::json!({"weights":{"code":1.0,"phase":100.0,"elevationWeighting":false},"tropo":{"enabled":false,"estimateZtd":false,"estimateTropoGradients":false,"pressureHpa":1013.25,"temperatureK":288.15,"relativeHumidity":0.5,"vmf1":[]}}),
            )
        } else {
            measurement
        };
        {
            let v: MeasurementInput = serde_wasm_bindgen::from_value(measurement.clone())
                .map_err(|e| type_error(&format!("invalid kinematic measurement options: {e}")))?;
            inner.weights = v.weights.to_core();
            inner.tropo = v.tropo.to_core()?
        }
        if let Some(v) = corrections {
            inner.corrections = v.inner
        }
        Ok(Self { inner, measurement })
    }
    #[wasm_bindgen(js_name=default)]
    pub fn default_js() -> Self {
        Self::new(None, None, None, None, JsValue::UNDEFINED, None)
            .expect("core kinematic defaults are valid")
    }
    #[wasm_bindgen(getter,js_name=initialState)]
    pub fn initial_state(&self) -> PppKinematicState {
        PppKinematicState {
            inner: self.inner.initial_state.clone(),
            covariance_m2: self.inner.initial_covariance_m2.clone(),
        }
    }
    #[wasm_bindgen(getter)]
    pub fn motion(&self) -> PppKinematicMotionModel {
        PppKinematicMotionModel {
            inner: self.inner.motion,
        }
    }
    #[wasm_bindgen(getter,js_name=processNoise)]
    pub fn noise(&self) -> PppKinematicProcessNoise {
        PppKinematicProcessNoise {
            inner: self.inner.process_noise,
        }
    }
    #[wasm_bindgen(getter,js_name=newAmbiguityVarianceM2)]
    pub fn variance(&self) -> f64 {
        self.inner.new_ambiguity_variance_m2
    }
    #[wasm_bindgen(getter, unchecked_return_type = "PppKinematicMeasurementOptions")]
    pub fn measurement(&self) -> JsValue {
        self.measurement.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn corrections(&self) -> PppRangeCorrections {
        PppRangeCorrections {
            inner: self.inner.corrections.clone(),
        }
    }
    pub fn equals(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExclusionJs<'a> {
    epoch_index: usize,
    satellite_id: &'a str,
    ambiguity_id: &'a str,
    code_bias_missing: bool,
    phase_bias_missing: bool,
    transmit_time_failure_message: Option<String>,
    application_message: Option<String>,
}
fn exclusions(rows: &[SsrBiasExclusion]) -> JsValue {
    to_js(
        &rows
            .iter()
            .map(|v| ExclusionJs {
                epoch_index: v.epoch_index,
                satellite_id: &v.satellite_id,
                ambiguity_id: &v.ambiguity_id,
                code_bias_missing: v.code_bias_missing,
                phase_bias_missing: v.phase_bias_missing,
                // These nested core report types do not implement Display or serde yet;
                // Debug is retained only as a message, never as a discriminator.
                transmit_time_failure_message: v
                    .transmit_time_failure
                    .as_ref()
                    .map(|x| format!("{x:?}")),
                application_message: v.application.as_ref().map(|x| format!("{x:?}")),
            })
            .collect::<Vec<_>>(),
    )
}

/// Summary of one core measurement update.
#[wasm_bindgen]
pub struct PppKinematicUpdateSummary {
    inner: KinematicUpdateSummary,
}
#[wasm_bindgen]
impl PppKinematicUpdateSummary {
    #[wasm_bindgen(getter,js_name=innovationRmsM)]
    pub fn rms(&self) -> f64 {
        self.inner.innovation_rms_m
    }
    #[wasm_bindgen(getter,js_name=usedSats)]
    pub fn sats(&self) -> Vec<String> {
        self.inner.used_sats.clone()
    }
    #[wasm_bindgen(getter,js_name=ssrBiasExclusions, unchecked_return_type = "PppKinematicSsrBiasExclusion[]")]
    pub fn exclusions(&self) -> JsValue {
        exclusions(&self.inner.ssr_bias_exclusions)
    }
    #[wasm_bindgen(getter,js_name=unplacedObservations, unchecked_return_type = "PppUnplacedObservation[]")]
    pub fn unplaced(&self) -> JsValue {
        unplaced_observations(&self.inner.unplaced_observations)
    }
    pub fn equals(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

/// One epoch produced by the kinematic PPP driver.
#[wasm_bindgen]
pub struct PppKinematicEpochSolution {
    inner: KinematicEpochSolution,
}
#[wasm_bindgen]
impl PppKinematicEpochSolution {
    #[wasm_bindgen(getter,js_name=positionM)]
    pub fn position(&self) -> Vec<f64> {
        self.inner.position_m.to_vec()
    }
    #[wasm_bindgen(getter,js_name=clockM)]
    pub fn clock(&self) -> f64 {
        self.inner.clock_m
    }
    #[wasm_bindgen(getter,js_name=ztdResidualM)]
    pub fn ztd(&self) -> f64 {
        self.inner.ztd_residual_m
    }
    #[wasm_bindgen(getter,js_name=ambiguitiesM, unchecked_return_type = "Record<string, number>")]
    pub fn ambiguities(&self) -> JsValue {
        to_js(&self.inner.ambiguities_m)
    }
    #[wasm_bindgen(getter,js_name=positionCovarianceM2)]
    pub fn covariance(&self) -> Vec<f64> {
        self.inner
            .position_covariance_m2
            .into_iter()
            .flatten()
            .collect()
    }
    #[wasm_bindgen(getter,js_name=usedSats)]
    pub fn sats(&self) -> Vec<String> {
        self.inner.used_sats.clone()
    }
    #[wasm_bindgen(getter,js_name=innovationRmsM)]
    pub fn rms(&self) -> f64 {
        self.inner.innovation_rms_m
    }
    #[wasm_bindgen(getter,js_name=ssrBiasExclusions, unchecked_return_type = "PppKinematicSsrBiasExclusion[]")]
    pub fn exclusions(&self) -> JsValue {
        exclusions(&self.inner.ssr_bias_exclusions)
    }
    #[wasm_bindgen(getter,js_name=unplacedObservations, unchecked_return_type = "PppUnplacedObservation[]")]
    pub fn unplaced(&self) -> JsValue {
        unplaced_observations(&self.inner.unplaced_observations)
    }
    #[wasm_bindgen(getter, unchecked_return_type = "PppKinematicEpochStatus")]
    pub fn status(&self) -> String {
        match self.inner.status {
            KinematicEpochStatus::Updated => "updated",
            KinematicEpochStatus::PredictedOnly => "predictedOnly",
        }
        .into()
    }
    pub fn equals(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

/// Predict a mutable kinematic state through the core engine.
#[wasm_bindgen(js_name=predictKinematicState)]
pub fn predict(
    state: &mut PppKinematicState,
    dt_s: f64,
    active_ambiguity_ids: Vec<String>,
    config: &PppKinematicConfig,
) -> Result<(), JsValue> {
    core_predict(
        &mut state.inner,
        &mut state.covariance_m2,
        dt_s,
        &active_ambiguity_ids,
        &config.inner,
    )
    .map_err(solve_error)
}

/// Correct a mutable kinematic state with one PPP epoch.
#[wasm_bindgen(js_name=correctKinematicState)]
pub fn correct(
    source: &Sp3,
    epoch_index: usize,
    epoch: JsValue,
    state: &mut PppKinematicState,
    config: &PppKinematicConfig,
) -> Result<PppKinematicUpdateSummary, JsValue> {
    let mut epochs = epochs_from_js(js_sys::Array::of1(&epoch).into())?;
    core_correct(
        &source.inner,
        epoch_index,
        &epochs.remove(0),
        &mut state.inner,
        &mut state.covariance_m2,
        &config.inner,
    )
    .map(|inner| PppKinematicUpdateSummary { inner })
    .map_err(solve_error)
}

/// Solve an ordered epoch sequence with the core kinematic PPP driver.
#[wasm_bindgen(js_name=solveKinematicPpp)]
pub fn solve(
    source: &Sp3,
    epochs: JsValue,
    config: Option<PppKinematicConfig>,
) -> Result<Vec<PppKinematicEpochSolution>, JsValue> {
    core_solve(
        &source.inner,
        &epochs_from_js(epochs)?,
        config.map_or_else(KinematicConfig::default, |v| v.inner),
    )
    .map(|v| {
        v.into_iter()
            .map(|inner| PppKinematicEpochSolution { inner })
            .collect()
    })
    .map_err(solve_error)
}

#[wasm_bindgen(typescript_custom_section)]
const KINEMATIC_TYPES: &'static str = r#"
export type PppKinematicEpochStatus = "updated" | "predictedOnly";
export type PppKinematicPositionProcessNoiseKind = "randomWalk" | "whiteNoiseAcceleration";
export type PppKinematicMotionModelKind = "hold" | "constantVelocity";
export interface PppKinematicMeasurementWeights { code: number; phase: number; elevationWeighting: boolean; }
export interface PppKinematicVmfSample { mjd: number; ah: number; aw: number; }
export interface PppKinematicTroposphereOptions { enabled: boolean; estimateZtd: boolean; estimateTropoGradients: boolean; pressureHpa: number; temperatureK: number; relativeHumidity: number; vmf1: PppKinematicVmfSample[]; }
export interface PppKinematicMeasurementOptions { weights: PppKinematicMeasurementWeights; tropo: PppKinematicTroposphereOptions; }
export interface PppKinematicSsrBiasExclusion { epochIndex: number; satelliteId: string; ambiguityId: string; codeBiasMissing: boolean; phaseBiasMissing: boolean; transmitTimeFailureMessage?: string; applicationMessage?: string; }
export type PppKinematicNoEphemerisReason = { kind: "NO_EPHEMERIS" | "MISSING_SATELLITE_CLOCK"; message: string } | { kind: "REASON"; detail: string; message: string };
export type PppKinematicSolveErrorDetail =
  | { family: "KinematicSolveError"; kind: "NO_EPHEMERIS"; satelliteId: string; reason: PppKinematicNoEphemerisReason; message: string }
  | { family: "KinematicSolveError"; kind: "SINGULAR_GEOMETRY"; message: string }
  | { family: "KinematicSolveError"; kind: "INVALID_SOLVE_OPTION" | "INVALID_INPUT"; field: string; reason: string; message: string }
  | { family: "KinematicSolveError"; kind: "MISSING_CORRECTION"; satelliteId: string; correction: { kind: string; label: string | null; message: string }; message: string }
  | { family: "KinematicSolveError"; kind: "UT1_OUTSIDE_COVERAGE"; reasonKind: "BEFORE_COVERAGE" | "AFTER_COVERAGE"; reason: string; message: string };
"#;
