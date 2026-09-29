//! Inertial mechanization and deterministic IMU simulation bindings.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::astro::math::mat3::Mat3;
use sidereon_core::inertial as core_inertial;

use crate::error::{engine_error, error_with_detail, reject_unknown_keys, to_plain_js, type_error};

const NAV_STATE_KEYS: &[&str] = &[
    "tJ2000S",
    "positionEcefM",
    "velocityEcefMps",
    "attitudeBodyToEcef",
    "accelBiasMps2",
    "gyroBiasRps",
];
const IMU_SAMPLE_KEYS: &[&str] = &[
    "kind",
    "tJ2000S",
    "specificForceMps2",
    "angularRateRps",
    "deltaVelocityMps",
    "deltaThetaRad",
    "dtS",
];
const CALIBRATION_KEYS: &[&str] = &["accelScaleMisalignment", "gyroScaleMisalignment"];
const BIAS_KEYS: &[&str] = &["accelMps2", "gyroRps"];
const RATE_RANDOM_WALK_KEYS: &[&str] = &["accelMps2SqrtS", "gyroRpsSqrtS"];
const IMU_SPEC_KEYS: &[&str] = &[
    "accelVrwMpsSqrtS",
    "gyroArwRadSqrtS",
    "accelBiasInstabMps2",
    "gyroBiasInstabRps",
    "accelBiasTauS",
    "gyroBiasTauS",
    "accelScaleInstabPpm",
    "gyroScaleInstabPpm",
];
const SIMULATOR_OPTIONS_KEYS: &[&str] = &[
    "output",
    "seed",
    "initialBias",
    "rateRandomWalk",
    "calibration",
];
const CORRECTED_INCREMENT_KEYS: &[&str] = &["tJ2000S", "deltaVelocityMps", "deltaThetaRad", "dtS"];
const MECHANIZATION_CONFIG_KEYS: &[&str] = &["coningCorrection"];
const IMU_ERROR_MODEL_KEYS: &[&str] = &["bias", "calibration"];
const QUATERNION_KEYS: &[&str] = &["w", "x", "y", "z"];
const STANDALONE_MECHANIZATION_KEYS: &[&str] = &["state", "increment", "config"];

fn reject_nested_object(
    parent: &JsValue,
    key: &str,
    context: &str,
    known: &[&str],
) -> Result<(), JsValue> {
    let value = js_sys::Reflect::get(parent, &JsValue::from_str(key))
        .map_err(|_| type_error(&format!("could not read {context}")))?;
    if !value.is_null() && !value.is_undefined() {
        reject_unknown_keys(&value, context, known)?;
    }
    Ok(())
}

fn reject_array_item_keys(value: &JsValue, context: &str, known: &[&str]) -> Result<(), JsValue> {
    if js_sys::Array::is_array(value) {
        for (index, item) in js_sys::Array::from(value).iter().enumerate() {
            reject_unknown_keys(&item, &format!("{context}[{index}]"), known)?;
        }
    }
    Ok(())
}

fn reject_nav_state(value: &JsValue, context: &str) -> Result<(), JsValue> {
    reject_unknown_keys(value, context, NAV_STATE_KEYS)
}

fn reject_corrected_increment(value: &JsValue, context: &str) -> Result<(), JsValue> {
    reject_unknown_keys(value, context, CORRECTED_INCREMENT_KEYS)
}

fn reject_imu_spec(value: &JsValue, context: &str) -> Result<(), JsValue> {
    reject_unknown_keys(value, context, IMU_SPEC_KEYS)
}

fn reject_calibration(value: &JsValue, context: &str) -> Result<(), JsValue> {
    reject_unknown_keys(value, context, CALIBRATION_KEYS)
}

fn reject_bias(value: &JsValue, context: &str) -> Result<(), JsValue> {
    reject_unknown_keys(value, context, BIAS_KEYS)
}

fn reject_simulator_options(value: &JsValue) -> Result<(), JsValue> {
    reject_unknown_keys(value, "IMU simulation options", SIMULATOR_OPTIONS_KEYS)?;
    reject_nested_object(value, "initialBias", "initial IMU bias", BIAS_KEYS)?;
    reject_nested_object(
        value,
        "rateRandomWalk",
        "IMU rate random walk",
        RATE_RANDOM_WALK_KEYS,
    )?;
    reject_nested_object(value, "calibration", "IMU calibration", CALIBRATION_KEYS)
}

fn reject_imu_error_model(value: &JsValue) -> Result<(), JsValue> {
    reject_unknown_keys(value, "IMU error model", IMU_ERROR_MODEL_KEYS)?;
    reject_nested_object(value, "bias", "IMU bias", BIAS_KEYS)?;
    reject_nested_object(value, "calibration", "IMU calibration", CALIBRATION_KEYS)
}

fn reject_mechanization_config(value: &JsValue, context: &str) -> Result<(), JsValue> {
    reject_unknown_keys(value, context, MECHANIZATION_CONFIG_KEYS)
}

fn reject_standalone_mechanization(value: &JsValue) -> Result<(), JsValue> {
    reject_unknown_keys(value, "mechanization input", STANDALONE_MECHANIZATION_KEYS)?;
    reject_nested_object(value, "state", "navigation state", NAV_STATE_KEYS)?;
    reject_nested_object(
        value,
        "increment",
        "corrected IMU increment",
        CORRECTED_INCREMENT_KEYS,
    )?;
    reject_nested_object(
        value,
        "config",
        "mechanization config",
        MECHANIZATION_CONFIG_KEYS,
    )
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
enum InertialErrorDetail {
    InvalidInput {
        field: &'static str,
        reason: &'static str,
    },
    NonMonotonicSample,
    SingularCalibration,
    DegenerateAttitude,
}

fn inertial_error(error: core_inertial::InertialError) -> JsValue {
    let detail = match error {
        core_inertial::InertialError::InvalidInput { field, reason } => {
            InertialErrorDetail::InvalidInput { field, reason }
        }
        core_inertial::InertialError::NonMonotonicSample => InertialErrorDetail::NonMonotonicSample,
        core_inertial::InertialError::SingularCalibration => {
            InertialErrorDetail::SingularCalibration
        }
        core_inertial::InertialError::DegenerateAttitude => InertialErrorDetail::DegenerateAttitude,
    };
    error_with_detail("InertialError", &error.to_string(), &detail)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NavStateInput {
    t_j2000_s: f64,
    position_ecef_m: [f64; 3],
    velocity_ecef_mps: [f64; 3],
    attitude_body_to_ecef: Mat3,
    #[serde(default)]
    accel_bias_mps2: Option<[f64; 3]>,
    #[serde(default)]
    gyro_bias_rps: Option<[f64; 3]>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CalibrationInput {
    accel_scale_misalignment: Mat3,
    gyro_scale_misalignment: Mat3,
}

impl From<CalibrationInput> for core_inertial::ImuCalibration {
    fn from(value: CalibrationInput) -> Self {
        Self {
            accel_scale_misalignment: value.accel_scale_misalignment,
            gyro_scale_misalignment: value.gyro_scale_misalignment,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CalibrationOutput {
    accel_scale_misalignment: Mat3,
    gyro_scale_misalignment: Mat3,
}

impl From<core_inertial::ImuCalibration> for CalibrationOutput {
    fn from(value: core_inertial::ImuCalibration) -> Self {
        Self {
            accel_scale_misalignment: value.accel_scale_misalignment,
            gyro_scale_misalignment: value.gyro_scale_misalignment,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NavStateOutput {
    t_j2000_s: f64,
    position_ecef_m: [f64; 3],
    velocity_ecef_mps: [f64; 3],
    attitude_body_to_ecef: Mat3,
    accel_bias_mps2: [f64; 3],
    gyro_bias_rps: [f64; 3],
}

impl From<&core_inertial::NavState> for NavStateOutput {
    fn from(state: &core_inertial::NavState) -> Self {
        Self {
            t_j2000_s: state.t_j2000_s,
            position_ecef_m: state.position_ecef_m,
            velocity_ecef_mps: state.velocity_ecef_mps,
            attitude_body_to_ecef: state.attitude_body_to_ecef,
            accel_bias_mps2: state.accel_bias_mps2,
            gyro_bias_rps: state.gyro_bias_rps,
        }
    }
}

fn nav_state_from_input(input: NavStateInput) -> Result<core_inertial::NavState, JsValue> {
    let state = core_inertial::NavState::new(
        input.t_j2000_s,
        input.position_ecef_m,
        input.velocity_ecef_mps,
        input.attitude_body_to_ecef,
    )
    .map_err(inertial_error)?;
    state
        .with_biases(
            input.accel_bias_mps2.unwrap_or([0.0; 3]),
            input.gyro_bias_rps.unwrap_or([0.0; 3]),
        )
        .map_err(inertial_error)
}

fn nav_state(value: JsValue) -> Result<core_inertial::NavState, JsValue> {
    reject_nav_state(&value, "inertial navigation state")?;
    let input: NavStateInput = serde_wasm_bindgen::from_value(value)
        .map_err(|error| type_error(&format!("invalid inertial navigation state: {error}")))?;
    nav_state_from_input(input)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImuSampleInput {
    kind: String,
    t_j2000_s: f64,
    #[serde(default)]
    specific_force_mps2: Option<[f64; 3]>,
    #[serde(default)]
    angular_rate_rps: Option<[f64; 3]>,
    #[serde(default)]
    delta_velocity_mps: Option<[f64; 3]>,
    #[serde(default)]
    delta_theta_rad: Option<[f64; 3]>,
    #[serde(default)]
    dt_s: Option<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImuSampleOutput {
    kind: &'static str,
    t_j2000_s: f64,
    specific_force_mps2: Option<[f64; 3]>,
    angular_rate_rps: Option<[f64; 3]>,
    delta_velocity_mps: Option<[f64; 3]>,
    delta_theta_rad: Option<[f64; 3]>,
    dt_s: Option<f64>,
}

fn imu_sample(value: JsValue) -> Result<core_inertial::ImuSample, JsValue> {
    reject_unknown_keys(&value, "IMU sample", IMU_SAMPLE_KEYS)?;
    let input: ImuSampleInput = serde_wasm_bindgen::from_value(value)
        .map_err(|error| type_error(&format!("invalid IMU sample: {error}")))?;
    match input.kind.as_str() {
        "rate" => Ok(core_inertial::ImuSample::rate(
            input.t_j2000_s,
            input
                .specific_force_mps2
                .ok_or_else(|| type_error("specificForceMps2 is required for a rate sample"))?,
            input
                .angular_rate_rps
                .ok_or_else(|| type_error("angularRateRps is required for a rate sample"))?,
        )),
        "increment" => Ok(core_inertial::ImuSample::increment(
            input.t_j2000_s,
            input.delta_velocity_mps.ok_or_else(|| {
                type_error("deltaVelocityMps is required for an increment sample")
            })?,
            input
                .delta_theta_rad
                .ok_or_else(|| type_error("deltaThetaRad is required for an increment sample"))?,
            input
                .dt_s
                .ok_or_else(|| type_error("dtS is required for an increment sample"))?,
        )),
        other => Err(type_error(&format!(
            "IMU sample kind must be 'rate' or 'increment', got {other:?}"
        ))),
    }
}

fn sample_output(sample: core_inertial::ImuSample) -> ImuSampleOutput {
    match sample.kind {
        core_inertial::ImuSampleKind::Rate {
            specific_force_mps2,
            angular_rate_rps,
        } => ImuSampleOutput {
            kind: "rate",
            t_j2000_s: sample.t_j2000_s,
            specific_force_mps2: Some(specific_force_mps2),
            angular_rate_rps: Some(angular_rate_rps),
            delta_velocity_mps: None,
            delta_theta_rad: None,
            dt_s: None,
        },
        core_inertial::ImuSampleKind::Increment {
            delta_velocity_mps,
            delta_theta_rad,
            dt_s,
        } => ImuSampleOutput {
            kind: "increment",
            t_j2000_s: sample.t_j2000_s,
            specific_force_mps2: None,
            angular_rate_rps: None,
            delta_velocity_mps: Some(delta_velocity_mps),
            delta_theta_rad: Some(delta_theta_rad),
            dt_s: Some(dt_s),
        },
    }
}

#[wasm_bindgen]
pub struct StrapdownMechanizer {
    inner: core_inertial::StrapdownMechanizer,
}

#[wasm_bindgen]
impl StrapdownMechanizer {
    #[wasm_bindgen(constructor)]
    pub fn new(
        #[wasm_bindgen(unchecked_param_type = "InertialNavStateInput")] initial_state: JsValue,
    ) -> Result<StrapdownMechanizer, JsValue> {
        let state = nav_state(initial_state)?;
        let inner = core_inertial::StrapdownMechanizer::new(state).map_err(inertial_error)?;
        Ok(Self { inner })
    }

    #[wasm_bindgen(unchecked_return_type = "InertialNavState")]
    pub fn state(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&NavStateOutput::from(self.inner.state()))
            .map_err(|error| engine_error(error.to_string()))
    }

    #[wasm_bindgen(unchecked_return_type = "InertialNavState")]
    pub fn propagate(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "InertialImuSampleInput")] sample: JsValue,
    ) -> Result<JsValue, JsValue> {
        let sample = imu_sample(sample)?;
        let state = self.inner.propagate(sample).map_err(inertial_error)?;
        serde_wasm_bindgen::to_value(&NavStateOutput::from(state))
            .map_err(|error| engine_error(error.to_string()))
    }

    #[wasm_bindgen(js_name = setImuErrorModel)]
    pub fn set_imu_error_model(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "InertialErrorModel")] model: JsValue,
    ) -> Result<(), JsValue> {
        reject_imu_error_model(&model)?;
        let input: ImuErrorModelInput = serde_wasm_bindgen::from_value(model)
            .map_err(|error| type_error(&format!("invalid IMU error model: {error}")))?;
        let model = core_inertial::ImuErrorModel {
            bias: input.bias.into(),
            calibration: input.calibration.into(),
        };
        self.inner = self.inner.with_imu_model(model).map_err(inertial_error)?;
        Ok(())
    }

    #[wasm_bindgen(js_name = setConfig)]
    pub fn set_config(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "InertialMechanizationConfig")] config: JsValue,
    ) -> Result<(), JsValue> {
        reject_mechanization_config(&config, "mechanization config")?;
        let config: MechanizationConfigInput = serde_wasm_bindgen::from_value(config)
            .map_err(|error| type_error(&format!("invalid mechanization config: {error}")))?;
        if config.coning_correction.as_deref().unwrap_or("off") != "off" {
            return Err(type_error("only 'off' coning correction is supported"));
        }
        self.inner = self
            .inner
            .with_config(core_inertial::MechanizationConfig::default());
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MechanizationConfigInput {
    #[serde(default)]
    coning_correction: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImuErrorModelInput {
    bias: BiasInput,
    calibration: CalibrationInput,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImuSpecInput {
    accel_vrw_mps_sqrt_s: f64,
    gyro_arw_rad_sqrt_s: f64,
    accel_bias_instab_mps2: f64,
    gyro_bias_instab_rps: f64,
    accel_bias_tau_s: f64,
    gyro_bias_tau_s: f64,
    #[serde(default)]
    accel_scale_instab_ppm: Option<f64>,
    #[serde(default)]
    gyro_scale_instab_ppm: Option<f64>,
}

impl From<ImuSpecInput> for core_inertial::ImuSpec {
    fn from(value: ImuSpecInput) -> Self {
        Self::datasheet(
            value.accel_vrw_mps_sqrt_s,
            value.gyro_arw_rad_sqrt_s,
            value.accel_bias_instab_mps2,
            value.gyro_bias_instab_rps,
            value.accel_bias_tau_s,
            value.gyro_bias_tau_s,
            value.accel_scale_instab_ppm,
            value.gyro_scale_instab_ppm,
        )
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImuSpecOutput {
    accel_vrw_mps_sqrt_s: f64,
    gyro_arw_rad_sqrt_s: f64,
    accel_bias_instab_mps2: f64,
    gyro_bias_instab_rps: f64,
    accel_bias_tau_s: f64,
    gyro_bias_tau_s: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    accel_scale_instab_ppm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    gyro_scale_instab_ppm: Option<f64>,
}

impl From<core_inertial::ImuSpec> for ImuSpecOutput {
    fn from(spec: core_inertial::ImuSpec) -> Self {
        Self {
            accel_vrw_mps_sqrt_s: spec.accel_vrw_mps_sqrt_s,
            gyro_arw_rad_sqrt_s: spec.gyro_arw_rad_sqrt_s,
            accel_bias_instab_mps2: spec.accel_bias_instab_mps2,
            gyro_bias_instab_rps: spec.gyro_bias_instab_rps,
            accel_bias_tau_s: spec.accel_bias_tau_s,
            gyro_bias_tau_s: spec.gyro_bias_tau_s,
            accel_scale_instab_ppm: spec.accel_scale_instab_ppm,
            gyro_scale_instab_ppm: spec.gyro_scale_instab_ppm,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BiasInput {
    accel_mps2: [f64; 3],
    gyro_rps: [f64; 3],
}

impl From<BiasInput> for core_inertial::ImuBias {
    fn from(value: BiasInput) -> Self {
        Self {
            accel_mps2: value.accel_mps2,
            gyro_rps: value.gyro_rps,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RateRandomWalkInput {
    accel_mps2_sqrt_s: f64,
    gyro_rps_sqrt_s: f64,
}

impl From<RateRandomWalkInput> for core_inertial::ImuRateRandomWalk {
    fn from(value: RateRandomWalkInput) -> Self {
        Self::new(value.accel_mps2_sqrt_s, value.gyro_rps_sqrt_s)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SimulatorOptionsInput {
    #[serde(default = "default_output")]
    output: String,
    #[serde(default = "default_seed")]
    seed: u64,
    #[serde(default)]
    initial_bias: Option<BiasInput>,
    #[serde(default)]
    rate_random_walk: Option<RateRandomWalkInput>,
    #[serde(default)]
    calibration: Option<CalibrationInput>,
}

fn default_output() -> String {
    "increment".to_owned()
}

fn default_seed() -> u64 {
    core_inertial::DEFAULT_IMU_SIM_SEED
}

fn simulator_options(value: JsValue) -> Result<core_inertial::ImuSimulationOptions, JsValue> {
    reject_simulator_options(&value)?;
    let input: SimulatorOptionsInput = serde_wasm_bindgen::from_value(value)
        .map_err(|error| type_error(&format!("invalid IMU simulation options: {error}")))?;
    let output = match input.output.as_str() {
        "increment" => core_inertial::ImuSimulationOutput::Increment,
        "rate" => core_inertial::ImuSimulationOutput::Rate,
        other => return Err(type_error(&format!("invalid IMU output kind {other:?}"))),
    };
    let mut options = core_inertial::ImuSimulationOptions::default();
    options.output = output;
    options.seed = input.seed;
    options.initial_bias = input.initial_bias.map(Into::into).unwrap_or_default();
    options.calibration = input.calibration.map(Into::into).unwrap_or_default();
    options.rate_random_walk = input.rate_random_walk.map(Into::into);
    Ok(options)
}

#[wasm_bindgen]
pub struct ImuSimulator {
    inner: core_inertial::ImuSimulator,
}

#[wasm_bindgen]
impl ImuSimulator {
    #[wasm_bindgen(constructor)]
    pub fn new(
        #[wasm_bindgen(unchecked_param_type = "InertialImuSpec")] spec: JsValue,
        #[wasm_bindgen(unchecked_param_type = "InertialSimulatorOptions")] options: JsValue,
    ) -> Result<ImuSimulator, JsValue> {
        reject_imu_spec(&spec, "IMU specification")?;
        let spec: ImuSpecInput = serde_wasm_bindgen::from_value(spec)
            .map_err(|error| type_error(&format!("invalid IMU specification: {error}")))?;
        let inner = core_inertial::ImuSimulator::new(spec.into(), simulator_options(options)?)
            .map_err(inertial_error)?;
        Ok(Self { inner })
    }

    #[wasm_bindgen(unchecked_return_type = "InertialBias")]
    pub fn bias(&self) -> Result<JsValue, JsValue> {
        let bias = self.inner.bias();
        to_plain_js(
            &serde_json::json!({
                "accelMps2": bias.accel_mps2,
                "gyroRps": bias.gyro_rps,
            }),
            "IMU simulator bias",
        )
    }

    #[wasm_bindgen(js_name = rateRandomWalk, unchecked_return_type = "InertialBias")]
    pub fn rate_random_walk(&self) -> Result<JsValue, JsValue> {
        let bias = self.inner.rate_random_walk();
        to_plain_js(
            &serde_json::json!({
                "accelMps2": bias.accel_mps2,
                "gyroRps": bias.gyro_rps,
            }),
            "IMU simulator rate random walk",
        )
    }

    #[wasm_bindgen(js_name = sampleIncrement, unchecked_return_type = "InertialImuSample")]
    pub fn sample_increment(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "CorrectedImuIncrementInput")] increment: JsValue,
    ) -> Result<JsValue, JsValue> {
        reject_corrected_increment(&increment, "corrected IMU increment")?;
        let input: CorrectedIncrementInput = serde_wasm_bindgen::from_value(increment)
            .map_err(|error| type_error(&format!("invalid corrected IMU increment: {error}")))?;
        let sample = self
            .inner
            .sample_increment(&input.into())
            .map_err(inertial_error)?;
        to_plain_js(&sample_output(sample), "simulated IMU sample")
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CorrectedIncrementInput {
    t_j2000_s: f64,
    delta_velocity_mps: [f64; 3],
    delta_theta_rad: [f64; 3],
    dt_s: f64,
}

impl From<CorrectedIncrementInput> for core_inertial::CorrectedImuIncrement {
    fn from(value: CorrectedIncrementInput) -> Self {
        Self {
            t_j2000_s: value.t_j2000_s,
            delta_velocity_mps: value.delta_velocity_mps,
            delta_theta_rad: value.delta_theta_rad,
            dt_s: value.dt_s,
        }
    }
}

#[wasm_bindgen(js_name = simulateImuSamples, unchecked_return_type = "InertialSimulationResult")]
pub fn simulate_imu_samples_js(
    #[wasm_bindgen(unchecked_param_type = "InertialNavStateInput[]")] trajectory: JsValue,
    #[wasm_bindgen(unchecked_param_type = "InertialImuSpec")] spec: JsValue,
    #[wasm_bindgen(unchecked_param_type = "InertialSimulatorOptions")] options: JsValue,
) -> Result<JsValue, JsValue> {
    reject_array_item_keys(&trajectory, "IMU truth trajectory", NAV_STATE_KEYS)?;
    let trajectory: Vec<NavStateInput> = serde_wasm_bindgen::from_value(trajectory)
        .map_err(|error| type_error(&format!("invalid IMU truth trajectory: {error}")))?;
    let states = trajectory
        .into_iter()
        .map(nav_state_from_input)
        .collect::<Result<Vec<_>, _>>()?;
    reject_imu_spec(&spec, "IMU specification")?;
    let spec: ImuSpecInput = serde_wasm_bindgen::from_value(spec)
        .map_err(|error| type_error(&format!("invalid IMU specification: {error}")))?;
    let result =
        core_inertial::simulate_imu_samples(&states, spec.into(), simulator_options(options)?)
            .map_err(inertial_error)?;
    to_plain_js(
        &serde_json::json!({
            "samples": result.samples.into_iter().map(sample_output).collect::<Vec<_>>(),
            "biasHistory": result.bias_history.iter().map(|bias| serde_json::json!({
                "accelMps2": bias.accel_mps2,
                "gyroRps": bias.gyro_rps,
            })).collect::<Vec<_>>(),
            "rateRandomWalkHistory": result.rate_random_walk_history.iter().map(|bias| serde_json::json!({
                "accelMps2": bias.accel_mps2,
                "gyroRps": bias.gyro_rps,
            })).collect::<Vec<_>>(),
        }),
        "simulated IMU trajectory",
    )
}

#[wasm_bindgen(js_name = simulateImuSamplesFromIncrements, unchecked_return_type = "InertialSimulationResult")]
pub fn simulate_imu_samples_from_increments_js(
    #[wasm_bindgen(unchecked_param_type = "CorrectedImuIncrementInput[]")] increments: JsValue,
    #[wasm_bindgen(unchecked_param_type = "InertialImuSpec")] spec: JsValue,
    #[wasm_bindgen(unchecked_param_type = "InertialSimulatorOptions")] options: JsValue,
) -> Result<JsValue, JsValue> {
    reject_array_item_keys(
        &increments,
        "corrected IMU increments",
        CORRECTED_INCREMENT_KEYS,
    )?;
    let inputs: Vec<CorrectedIncrementInput> = serde_wasm_bindgen::from_value(increments)
        .map_err(|error| type_error(&format!("invalid corrected IMU increments: {error}")))?;
    let increments = inputs.into_iter().map(Into::into).collect::<Vec<_>>();
    reject_imu_spec(&spec, "IMU specification")?;
    let spec: ImuSpecInput = serde_wasm_bindgen::from_value(spec)
        .map_err(|error| type_error(&format!("invalid IMU specification: {error}")))?;
    let result = core_inertial::simulate_imu_samples_from_increments(
        &increments,
        spec.into(),
        simulator_options(options)?,
    )
    .map_err(inertial_error)?;
    to_plain_js(
        &serde_json::json!({
            "samples": result.samples.into_iter().map(sample_output).collect::<Vec<_>>(),
            "biasHistory": result.bias_history.iter().map(|bias| serde_json::json!({
                "accelMps2": bias.accel_mps2,
                "gyroRps": bias.gyro_rps,
            })).collect::<Vec<_>>(),
            "rateRandomWalkHistory": result.rate_random_walk_history.iter().map(|bias| serde_json::json!({
                "accelMps2": bias.accel_mps2,
                "gyroRps": bias.gyro_rps,
            })).collect::<Vec<_>>(),
        }),
        "simulated IMU increments",
    )
}

#[wasm_bindgen(js_name = trueImuIncrementBetween, unchecked_return_type = "CorrectedImuIncrementInput")]
pub fn true_imu_increment_between_js(
    #[wasm_bindgen(unchecked_param_type = "InertialNavStateInput")] start: JsValue,
    #[wasm_bindgen(unchecked_param_type = "InertialNavStateInput")] end: JsValue,
) -> Result<JsValue, JsValue> {
    let start = nav_state(start)?;
    let end = nav_state(end)?;
    let increment =
        core_inertial::true_imu_increment_between(&start, &end).map_err(inertial_error)?;
    to_plain_js(
        &serde_json::json!({
            "tJ2000S": increment.t_j2000_s,
            "deltaVelocityMps": increment.delta_velocity_mps,
            "deltaThetaRad": increment.delta_theta_rad,
            "dtS": increment.dt_s,
        }),
        "true IMU increment",
    )
}

#[wasm_bindgen(js_name = normalGravityMps2)]
pub fn normal_gravity_mps2_js(latitude_rad: f64, height_m: f64) -> Result<f64, JsValue> {
    core_inertial::normal_gravity_mps2(latitude_rad, height_m).map_err(inertial_error)
}

#[wasm_bindgen(js_name = gravityEcefMps2, unchecked_return_type = "[number, number, number]")]
pub fn gravity_ecef_mps2_js(position_ecef_m: &[f64]) -> Result<Vec<f64>, JsValue> {
    if position_ecef_m.len() != 3 {
        return Err(type_error("positionEcefM must contain exactly 3 numbers"));
    }
    let position = [position_ecef_m[0], position_ecef_m[1], position_ecef_m[2]];
    core_inertial::gravity_ecef_mps2(position)
        .map(|value| value.to_vec())
        .map_err(inertial_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QuaternionInput {
    w: f64,
    x: f64,
    y: f64,
    z: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QuaternionOutput {
    w: f64,
    x: f64,
    y: f64,
    z: f64,
}

impl From<core_inertial::AttitudeQuaternion> for QuaternionOutput {
    fn from(value: core_inertial::AttitudeQuaternion) -> Self {
        Self {
            w: value.w,
            x: value.x,
            y: value.y,
            z: value.z,
        }
    }
}

#[wasm_bindgen(js_name = dcmToQuaternion, unchecked_return_type = "InertialQuaternion")]
pub fn dcm_to_quaternion_js(
    #[wasm_bindgen(unchecked_param_type = "InertialDcm")] dcm: JsValue,
) -> Result<JsValue, JsValue> {
    let dcm: Mat3 = serde_wasm_bindgen::from_value(dcm)
        .map_err(|error| type_error(&format!("invalid direction-cosine matrix: {error}")))?;
    let quaternion = core_inertial::dcm_to_quaternion(&dcm).map_err(inertial_error)?;
    to_plain_js(&QuaternionOutput::from(quaternion), "attitude quaternion")
}

#[wasm_bindgen(js_name = quaternionToDcm, unchecked_return_type = "InertialDcm")]
pub fn quaternion_to_dcm_js(
    #[wasm_bindgen(unchecked_param_type = "InertialQuaternion")] quaternion: JsValue,
) -> Result<JsValue, JsValue> {
    reject_unknown_keys(&quaternion, "attitude quaternion", QUATERNION_KEYS)?;
    let input: QuaternionInput = serde_wasm_bindgen::from_value(quaternion)
        .map_err(|error| type_error(&format!("invalid attitude quaternion: {error}")))?;
    let quaternion = core_inertial::AttitudeQuaternion::new(input.w, input.x, input.y, input.z)
        .map_err(inertial_error)?;
    to_plain_js(
        &core_inertial::quaternion_to_dcm(quaternion),
        "attitude matrix",
    )
}

#[wasm_bindgen(js_name = attitudeYawPitchRollRad, unchecked_return_type = "[number, number, number]")]
pub fn attitude_yaw_pitch_roll_rad_js(
    #[wasm_bindgen(unchecked_param_type = "InertialDcm")] dcm: JsValue,
) -> Result<js_sys::Array, JsValue> {
    let dcm: Mat3 = serde_wasm_bindgen::from_value(dcm)
        .map_err(|error| type_error(&format!("invalid direction-cosine matrix: {error}")))?;
    // A plain `[yaw, pitch, roll]` array, as the declaration states.
    Ok(core_inertial::attitude_yaw_pitch_roll_rad(&dcm)
        .into_iter()
        .map(JsValue::from_f64)
        .collect())
}

#[wasm_bindgen(js_name = reorthonormalizeDcm, unchecked_return_type = "InertialDcm")]
pub fn reorthonormalize_dcm_js(
    #[wasm_bindgen(unchecked_param_type = "InertialDcm")] dcm: JsValue,
) -> Result<JsValue, JsValue> {
    let dcm: Mat3 = serde_wasm_bindgen::from_value(dcm)
        .map_err(|error| type_error(&format!("invalid direction-cosine matrix: {error}")))?;
    let result = core_inertial::reorthonormalize_dcm(&dcm).map_err(inertial_error)?;
    to_plain_js(&result, "re-orthonormalized attitude matrix")
}

#[wasm_bindgen(js_name = rodriguesDeltaDcm, unchecked_return_type = "InertialDcm")]
pub fn rodrigues_delta_dcm_js(delta_theta_rad: &[f64]) -> Result<JsValue, JsValue> {
    if delta_theta_rad.len() != 3 {
        return Err(type_error("deltaThetaRad must contain exactly 3 numbers"));
    }
    let delta = [delta_theta_rad[0], delta_theta_rad[1], delta_theta_rad[2]];
    let result = core_inertial::rodrigues_delta_dcm(delta).map_err(inertial_error)?;
    to_plain_js(&result, "Rodrigues attitude increment")
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StandaloneMechanizationInput {
    state: NavStateInput,
    increment: CorrectedIncrementInput,
    #[serde(default)]
    config: Option<MechanizationConfigInput>,
}

#[wasm_bindgen(js_name = mechanizeEcef, unchecked_return_type = "InertialNavState")]
pub fn mechanize_ecef_js(
    #[wasm_bindgen(unchecked_param_type = "InertialMechanizationInput")] input: JsValue,
) -> Result<JsValue, JsValue> {
    reject_standalone_mechanization(&input)?;
    let input: StandaloneMechanizationInput = serde_wasm_bindgen::from_value(input)
        .map_err(|error| type_error(&format!("invalid mechanization input: {error}")))?;
    let state = nav_state_from_input(input.state)?;
    let increment: core_inertial::CorrectedImuIncrement = input.increment.into();
    let config = input.config.unwrap_or(MechanizationConfigInput {
        coning_correction: None,
    });
    if config.coning_correction.as_deref().unwrap_or("off") != "off" {
        return Err(type_error("only 'off' coning correction is supported"));
    }
    let result = core_inertial::mechanize_ecef(
        &state,
        &increment,
        core_inertial::MechanizationConfig::default(),
    )
    .map_err(inertial_error)?;
    to_plain_js(
        &NavStateOutput::from(&result),
        "mechanized navigation state",
    )
}

#[wasm_bindgen(js_name = defaultImuSimSeed)]
pub fn default_imu_sim_seed_js() -> u64 {
    core_inertial::DEFAULT_IMU_SIM_SEED
}

#[wasm_bindgen(js_name = wgs84GravityConstants, unchecked_return_type = "Wgs84GravityConstants")]
pub fn wgs84_gravity_constants_js() -> Result<JsValue, JsValue> {
    to_plain_js(
        &serde_json::json!({
            "equatorMps2": core_inertial::WGS84_NORMAL_GRAVITY_EQUATOR_MPS2,
            "poleMps2": core_inertial::WGS84_NORMAL_GRAVITY_POLE_MPS2,
            "somiglianaK": core_inertial::WGS84_SOMIGLIANA_K,
        }),
        "WGS84 gravity constants",
    )
}

#[wasm_bindgen(js_name = randomWalkBiasTauS)]
pub fn random_walk_bias_tau_s_js() -> f64 {
    core_inertial::config::RANDOM_WALK_BIAS_TAU_S
}

#[wasm_bindgen]
#[derive(Clone, Copy)]
pub enum ImuGrade {
    Mems,
    Tactical,
    Navigation,
}

#[wasm_bindgen(js_name = imuSpecPreset, unchecked_return_type = "InertialImuSpec")]
pub fn imu_spec_preset_js(grade: ImuGrade) -> Result<JsValue, JsValue> {
    let grade = match grade {
        ImuGrade::Mems => core_inertial::ImuGrade::Mems,
        ImuGrade::Tactical => core_inertial::ImuGrade::Tactical,
        ImuGrade::Navigation => core_inertial::ImuGrade::Navigation,
    };
    let spec = core_inertial::ImuSpec::preset(grade);
    to_plain_js(&ImuSpecOutput::from(spec), "IMU preset")
}

#[wasm_bindgen(js_name = imuSpecFromDatasheet, unchecked_return_type = "InertialImuSpec")]
pub fn imu_spec_from_datasheet_js(
    #[wasm_bindgen(unchecked_param_type = "InertialImuSpec")] spec: JsValue,
) -> Result<JsValue, JsValue> {
    reject_imu_spec(&spec, "IMU specification")?;
    let input: ImuSpecInput = serde_wasm_bindgen::from_value(spec)
        .map_err(|error| type_error(&format!("invalid IMU specification: {error}")))?;
    let spec = core_inertial::ImuSpec::from(input);
    to_plain_js(&ImuSpecOutput::from(spec), "IMU specification")
}

#[wasm_bindgen(js_name = imuRateRandomWalk, unchecked_return_type = "InertialRateRandomWalk")]
pub fn imu_rate_random_walk_js(
    accel_mps2_sqrt_s: f64,
    gyro_rps_sqrt_s: f64,
) -> Result<JsValue, JsValue> {
    let value = core_inertial::ImuRateRandomWalk::new(accel_mps2_sqrt_s, gyro_rps_sqrt_s);
    to_plain_js(
        &serde_json::json!({
            "accelMps2SqrtS": value.accel_mps2_sqrt_s,
            "gyroRpsSqrtS": value.gyro_rps_sqrt_s,
        }),
        "IMU rate random walk",
    )
}

#[wasm_bindgen(js_name = gaussMarkovBiasDecay)]
pub fn gauss_markov_bias_decay_js(dt_s: f64, tau_s: f64) -> Result<f64, JsValue> {
    core_inertial::gauss_markov_bias_decay(dt_s, tau_s).map_err(inertial_error)
}

#[wasm_bindgen(js_name = gaussMarkovBiasVarianceIncrement)]
pub fn gauss_markov_bias_variance_increment_js(
    instability: f64,
    dt_s: f64,
    tau_s: f64,
) -> Result<f64, JsValue> {
    core_inertial::gauss_markov_bias_variance_increment(instability, dt_s, tau_s)
        .map_err(inertial_error)
}

#[wasm_bindgen(js_name = imuCalibrationFromScalePpm, unchecked_return_type = "InertialCalibration")]
pub fn imu_calibration_from_scale_ppm_js(
    accel_scale_ppm: &[f64],
    gyro_scale_ppm: &[f64],
) -> Result<JsValue, JsValue> {
    if accel_scale_ppm.len() != 3 || gyro_scale_ppm.len() != 3 {
        return Err(type_error(
            "accelerometer and gyroscope scale arrays must each have 3 values",
        ));
    }
    let calibration = core_inertial::ImuCalibration::from_scale_ppm(
        [accel_scale_ppm[0], accel_scale_ppm[1], accel_scale_ppm[2]],
        [gyro_scale_ppm[0], gyro_scale_ppm[1], gyro_scale_ppm[2]],
    )
    .map_err(inertial_error)?;
    to_plain_js(&CalibrationOutput::from(calibration), "IMU calibration")
}

#[wasm_bindgen(js_name = correctImuSample, unchecked_return_type = "CorrectedImuIncrementInput")]
pub fn correct_imu_sample_js(
    #[wasm_bindgen(unchecked_param_type = "InertialImuSampleInput")] sample: JsValue,
    previous_t_j2000_s: f64,
    #[wasm_bindgen(unchecked_param_type = "InertialErrorModel")] model: JsValue,
) -> Result<JsValue, JsValue> {
    let sample = imu_sample(sample)?;
    reject_imu_error_model(&model)?;
    let input: ImuErrorModelInput = serde_wasm_bindgen::from_value(model)
        .map_err(|error| type_error(&format!("invalid IMU error model: {error}")))?;
    let model = core_inertial::ImuErrorModel {
        bias: input.bias.into(),
        calibration: input.calibration.into(),
    };
    let increment = model
        .correct_sample(&sample, previous_t_j2000_s)
        .map_err(inertial_error)?;
    to_plain_js(
        &serde_json::json!({
            "tJ2000S": increment.t_j2000_s,
            "deltaVelocityMps": increment.delta_velocity_mps,
            "deltaThetaRad": increment.delta_theta_rad,
            "dtS": increment.dt_s,
        }),
        "corrected IMU increment",
    )
}

#[wasm_bindgen(js_name = validateImuSpec)]
pub fn validate_imu_spec_js(
    #[wasm_bindgen(unchecked_param_type = "InertialImuSpec")] spec: JsValue,
) -> Result<(), JsValue> {
    reject_imu_spec(&spec, "IMU specification")?;
    let input: ImuSpecInput = serde_wasm_bindgen::from_value(spec)
        .map_err(|error| type_error(&format!("invalid IMU specification: {error}")))?;
    core_inertial::ImuSpec::from(input)
        .validate()
        .map_err(inertial_error)
}

#[wasm_bindgen(js_name = validateImuCalibration)]
pub fn validate_imu_calibration_js(
    #[wasm_bindgen(unchecked_param_type = "InertialCalibration")] calibration: JsValue,
) -> Result<(), JsValue> {
    reject_calibration(&calibration, "IMU calibration")?;
    let input: CalibrationInput = serde_wasm_bindgen::from_value(calibration)
        .map_err(|error| type_error(&format!("invalid IMU calibration: {error}")))?;
    core_inertial::ImuCalibration::from(input)
        .validate()
        .map_err(inertial_error)
}

#[wasm_bindgen(js_name = validateImuBias)]
pub fn validate_imu_bias_js(
    #[wasm_bindgen(unchecked_param_type = "InertialBias")] bias: JsValue,
) -> Result<(), JsValue> {
    reject_bias(&bias, "IMU bias")?;
    let input: BiasInput = serde_wasm_bindgen::from_value(bias)
        .map_err(|error| type_error(&format!("invalid IMU bias: {error}")))?;
    core_inertial::ImuBias::from(input)
        .validate()
        .map_err(inertial_error)
}

#[wasm_bindgen(js_name = validateImuRateRandomWalk)]
pub fn validate_imu_rate_random_walk_js(
    #[wasm_bindgen(unchecked_param_type = "InertialRateRandomWalk")] value: JsValue,
) -> Result<(), JsValue> {
    reject_unknown_keys(&value, "IMU rate random walk", RATE_RANDOM_WALK_KEYS)?;
    let input: RateRandomWalkInput = serde_wasm_bindgen::from_value(value)
        .map_err(|error| type_error(&format!("invalid IMU rate random walk: {error}")))?;
    core_inertial::ImuRateRandomWalk::from(input)
        .validate()
        .map_err(inertial_error)
}

#[wasm_bindgen(js_name = validateImuSimulationOptions)]
pub fn validate_imu_simulation_options_js(
    #[wasm_bindgen(unchecked_param_type = "InertialSimulatorOptions")] value: JsValue,
) -> Result<(), JsValue> {
    simulator_options(value)?.validate().map_err(inertial_error)
}

#[wasm_bindgen(js_name = normalizeAttitudeQuaternion, unchecked_return_type = "InertialQuaternion")]
pub fn normalize_attitude_quaternion_js(
    #[wasm_bindgen(unchecked_param_type = "InertialQuaternion")] quaternion: JsValue,
) -> Result<JsValue, JsValue> {
    reject_unknown_keys(&quaternion, "attitude quaternion", QUATERNION_KEYS)?;
    let input: QuaternionInput = serde_wasm_bindgen::from_value(quaternion)
        .map_err(|error| type_error(&format!("invalid attitude quaternion: {error}")))?;
    let result = core_inertial::AttitudeQuaternion::new(input.w, input.x, input.y, input.z)
        .map_err(inertial_error)?;
    to_plain_js(
        &QuaternionOutput::from(result),
        "normalized attitude quaternion",
    )
}

#[wasm_bindgen(js_name = navStateAttitude, unchecked_return_type = "InertialAttitude")]
pub fn nav_state_attitude_js(
    #[wasm_bindgen(unchecked_param_type = "InertialNavStateInput")] state: JsValue,
) -> Result<JsValue, JsValue> {
    let state = nav_state(state)?;
    let quaternion = state
        .attitude_quaternion_body_to_ecef()
        .map_err(inertial_error)?;
    to_plain_js(
        &serde_json::json!({
            "quaternion": QuaternionOutput::from(quaternion),
            "yawPitchRollRad": state.attitude_yaw_pitch_roll_rad(),
        }),
        "navigation-state attitude",
    )
}

#[wasm_bindgen(typescript_custom_section)]
const INERTIAL_TYPES: &str = r#"
export type InertialDcm = [[number, number, number], [number, number, number], [number, number, number]];
export interface InertialNavStateInput {
  tJ2000S: number;
  positionEcefM: [number, number, number];
  velocityEcefMps: [number, number, number];
  attitudeBodyToEcef: InertialDcm;
  accelBiasMps2?: [number, number, number];
  gyroBiasRps?: [number, number, number];
}
export interface InertialNavState extends InertialNavStateInput {
  accelBiasMps2: [number, number, number];
  gyroBiasRps: [number, number, number];
}
export type InertialImuSampleInput =
  | { kind: "rate"; tJ2000S: number; specificForceMps2: [number, number, number]; angularRateRps: [number, number, number] }
  | { kind: "increment"; tJ2000S: number; deltaVelocityMps: [number, number, number]; deltaThetaRad: [number, number, number]; dtS: number };
export interface InertialImuSample {
  kind: "rate" | "increment";
  tJ2000S: number;
  specificForceMps2: [number, number, number] | null;
  angularRateRps: [number, number, number] | null;
  deltaVelocityMps: [number, number, number] | null;
  deltaThetaRad: [number, number, number] | null;
  dtS: number | null;
}
export interface CorrectedImuIncrementInput {
  tJ2000S: number;
  deltaVelocityMps: [number, number, number];
  deltaThetaRad: [number, number, number];
  dtS: number;
}
export interface InertialImuSpec {
  accelVrwMpsSqrtS: number; gyroArwRadSqrtS: number; accelBiasInstabMps2: number; gyroBiasInstabRps: number;
  accelBiasTauS: number; gyroBiasTauS: number; accelScaleInstabPpm?: number; gyroScaleInstabPpm?: number;
}
export interface InertialCalibration { accelScaleMisalignment: number[][]; gyroScaleMisalignment: number[][] }
export interface InertialErrorModel { bias: { accelMps2: [number, number, number]; gyroRps: [number, number, number] }; calibration: InertialCalibration }
export interface InertialSimulatorOptions { output?: "rate" | "increment"; seed?: bigint; initialBias?: { accelMps2: [number, number, number]; gyroRps: [number, number, number] }; rateRandomWalk?: { accelMps2SqrtS: number; gyroRpsSqrtS: number }; calibration?: InertialCalibration }
export interface InertialBias { accelMps2: [number, number, number]; gyroRps: [number, number, number] }
export interface InertialRateRandomWalk { accelMps2SqrtS: number; gyroRpsSqrtS: number }
export interface InertialSimulationResult {
  samples: InertialImuSample[];
  biasHistory: InertialBias[];
  rateRandomWalkHistory: InertialBias[];
}
export interface InertialQuaternion { w: number; x: number; y: number; z: number }
export type InertialMechanizationConfig = { coningCorrection?: "off" };
export interface InertialMechanizationInput { state: InertialNavStateInput; increment: CorrectedImuIncrementInput }
export interface Wgs84GravityConstants { equatorMps2: number; poleMps2: number; somiglianaK: number }
export type InertialErrorDetail =
  | { kind: "INVALID_INPUT"; field: string; reason: string }
  | { kind: "NON_MONOTONIC_SAMPLE" }
  | { kind: "SINGULAR_CALIBRATION" }
  | { kind: "DEGENERATE_ATTITUDE" };
export interface InertialAttitude { quaternion: InertialQuaternion; yawPitchRollRad: [number, number, number] }
"#;
