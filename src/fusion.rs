//! GNSS/INS fusion binding.
//!
//! The stateful filter exported here owns the core INS error-state filter and
//! marshals JS object inputs into the core GNSS/INS fusion types.

use std::str::FromStr;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::fusion as core_fusion;
use sidereon_core::inertial as core_inertial;
use sidereon_core::{astro::math::mat3::Mat3, GnssSatelliteId};

use crate::error::{engine_error, range_error, type_error};
use crate::rinex_nav::BroadcastEphemeris;
use crate::sp3::Sp3;

fn fusion_error<E: core::fmt::Display>(err: E) -> JsValue {
    range_error(&err.to_string())
}

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| engine_error(format!("failed to serialize fusion result: {e}")))
}

fn parse_satellite(token: &str) -> Result<GnssSatelliteId, JsValue> {
    GnssSatelliteId::from_str(token)
        .map_err(|e| type_error(&format!("invalid satellite token {token:?}: {e}")))
}

fn vec3(name: &str, values: &[f64]) -> Result<[f64; 3], JsValue> {
    if values.len() != 3 {
        return Err(type_error(&format!(
            "{name} must contain exactly 3 numbers"
        )));
    }
    Ok([values[0], values[1], values[2]])
}

fn mat3_flat(name: &str, values: &[f64]) -> Result<Mat3, JsValue> {
    if values.len() != 9 {
        return Err(type_error(&format!(
            "{name} must contain exactly 9 row-major numbers"
        )));
    }
    Ok([
        [values[0], values[1], values[2]],
        [values[3], values[4], values[5]],
        [values[6], values[7], values[8]],
    ])
}

fn flat_mat3(value: &Mat3) -> Vec<f64> {
    value.iter().flat_map(|row| row.iter().copied()).collect()
}

fn layout_label(layout: core_fusion::ErrorStateLayout) -> &'static str {
    match layout {
        core_fusion::ErrorStateLayout::Fifteen => "fifteen",
        core_fusion::ErrorStateLayout::TwentyOne => "twentyOne",
    }
}

fn parse_layout(value: Option<&str>) -> Result<core_fusion::ErrorStateLayout, JsValue> {
    match value.unwrap_or("fifteen") {
        "fifteen" | "15" => Ok(core_fusion::ErrorStateLayout::Fifteen),
        "twentyOne" | "twenty_one" | "21" => Ok(core_fusion::ErrorStateLayout::TwentyOne),
        other => Err(type_error(&format!("invalid fusion layout {other:?}"))),
    }
}

fn parse_filter_kind(value: Option<&str>) -> Result<core_fusion::FusionFilterKind, JsValue> {
    match value.unwrap_or("ekf") {
        "ekf" | "EKF" => Ok(core_fusion::FusionFilterKind::Ekf),
        "ukf" | "UKF" => Ok(core_fusion::FusionFilterKind::Ukf),
        other => Err(type_error(&format!("invalid fusion filter kind {other:?}"))),
    }
}

fn parse_fix_status(value: Option<&str>) -> Result<core_fusion::GnssFixStatus, JsValue> {
    match value.unwrap_or("single") {
        "single" | "Single" => Ok(core_fusion::GnssFixStatus::Single),
        "float" | "Float" => Ok(core_fusion::GnssFixStatus::Float),
        "fixed" | "Fixed" => Ok(core_fusion::GnssFixStatus::Fixed),
        other => Err(type_error(&format!("invalid GNSS fix status {other:?}"))),
    }
}

fn parse_imu_grade(value: &str) -> Result<core_inertial::ImuGrade, JsValue> {
    match value {
        "mems" | "Mems" | "MEMS" => Ok(core_inertial::ImuGrade::Mems),
        "tactical" | "Tactical" => Ok(core_inertial::ImuGrade::Tactical),
        "navigation" | "Navigation" => Ok(core_inertial::ImuGrade::Navigation),
        other => Err(type_error(&format!("invalid IMU preset {other:?}"))),
    }
}

fn parse_update_options(
    input: Option<UpdateOptionsInput>,
) -> Result<core_fusion::EkfUpdateOptions, JsValue> {
    Ok(core_fusion::EkfUpdateOptions {
        innovation_gate: input.and_then(|value| value.innovation_gate).map(|gate| {
            core_fusion::InnovationGate {
                threshold_sigma: gate.threshold_sigma,
                min_rows: gate.min_rows,
            }
        }),
    })
}

fn parse_ukf_options(
    input: Option<UkfUpdateOptionsInput>,
) -> Result<core_fusion::UkfUpdateOptions, JsValue> {
    let mut out = core_fusion::UkfUpdateOptions::default();
    if let Some(input) = input {
        if let Some(transform) = input.transform {
            out.transform = core_fusion::UnscentedTransformOptions {
                alpha: transform.alpha.unwrap_or(out.transform.alpha),
                beta: transform.beta.unwrap_or(out.transform.beta),
                kappa: transform.kappa.unwrap_or(out.transform.kappa),
            };
        }
        out.innovation_gate = input
            .innovation_gate
            .map(|gate| core_fusion::InnovationGate {
                threshold_sigma: gate.threshold_sigma,
                min_rows: gate.min_rows,
            });
    }
    Ok(out)
}

fn parse_imu_spec(input: Option<ImuSpecInput>) -> Result<core_inertial::ImuSpec, JsValue> {
    match input {
        None => Ok(core_inertial::ImuSpec::mems()),
        Some(ImuSpecInput::Preset(label)) => {
            Ok(core_inertial::ImuSpec::preset(parse_imu_grade(&label)?))
        }
        Some(ImuSpecInput::Object(input)) => {
            if let Some(preset) = input.preset {
                return Ok(core_inertial::ImuSpec::preset(parse_imu_grade(&preset)?));
            }
            Ok(core_inertial::ImuSpec::datasheet(
                input
                    .accel_vrw_mps_sqrt_s
                    .ok_or_else(|| type_error("imuSpec.accelVrwMpsSqrtS is required"))?,
                input
                    .gyro_arw_rad_sqrt_s
                    .ok_or_else(|| type_error("imuSpec.gyroArwRadSqrtS is required"))?,
                input
                    .accel_bias_instab_mps2
                    .ok_or_else(|| type_error("imuSpec.accelBiasInstabMps2 is required"))?,
                input
                    .gyro_bias_instab_rps
                    .ok_or_else(|| type_error("imuSpec.gyroBiasInstabRps is required"))?,
                input
                    .accel_bias_tau_s
                    .ok_or_else(|| type_error("imuSpec.accelBiasTauS is required"))?,
                input
                    .gyro_bias_tau_s
                    .ok_or_else(|| type_error("imuSpec.gyroBiasTauS is required"))?,
                input.accel_scale_instab_ppm,
                input.gyro_scale_instab_ppm,
            ))
        }
    }
}

fn parse_imu_error_model(
    input: Option<ImuErrorModelInput>,
) -> Result<core_inertial::ImuErrorModel, JsValue> {
    let Some(input) = input else {
        return Ok(core_inertial::ImuErrorModel::default());
    };
    let mut model = core_inertial::ImuErrorModel::default();
    if let Some(bias) = input.bias {
        model.bias = core_inertial::ImuBias {
            accel_mps2: vec3("imuModel.bias.accelMps2", &bias.accel_mps2)?,
            gyro_rps: vec3("imuModel.bias.gyroRps", &bias.gyro_rps)?,
        };
    }
    if let Some(calibration) = input.calibration {
        model.calibration = core_inertial::ImuCalibration {
            accel_scale_misalignment: mat3_flat(
                "imuModel.calibration.accelScaleMisalignment",
                &calibration.accel_scale_misalignment,
            )?,
            gyro_scale_misalignment: mat3_flat(
                "imuModel.calibration.gyroScaleMisalignment",
                &calibration.gyro_scale_misalignment,
            )?,
        };
    }
    Ok(model)
}

fn parse_mechanization(
    input: Option<MechanizationConfigInput>,
) -> Result<core_inertial::MechanizationConfig, JsValue> {
    let Some(input) = input else {
        return Ok(core_inertial::MechanizationConfig::default());
    };
    match input.coning_correction.as_deref().unwrap_or("off") {
        "off" | "Off" => Ok(core_inertial::MechanizationConfig {
            coning_correction: core_inertial::ConingCorrection::Off,
        }),
        other => Err(type_error(&format!(
            "invalid mechanization.coningCorrection {other:?}"
        ))),
    }
}

fn parse_loose_config(
    input: Option<LooseConfigInput>,
) -> Result<core_fusion::LooseCouplingConfig, JsValue> {
    let mut config = core_fusion::LooseCouplingConfig::default();
    if let Some(input) = input {
        if let Some(lever) = input.lever_arm_body_m {
            config.lever_arm_body_m = vec3("loose.leverArmBodyM", &lever)?;
        }
        config.update_options = parse_update_options(input.update_options)?;
        if let Some(weighting) = input.fix_status_weighting {
            config.fix_status_weighting = core_fusion::GnssFixStatusWeighting {
                single_sigma_multiplier: weighting
                    .single_sigma_multiplier
                    .unwrap_or(config.fix_status_weighting.single_sigma_multiplier),
                float_sigma_multiplier: weighting
                    .float_sigma_multiplier
                    .unwrap_or(config.fix_status_weighting.float_sigma_multiplier),
                fixed_sigma_multiplier: weighting
                    .fixed_sigma_multiplier
                    .unwrap_or(config.fix_status_weighting.fixed_sigma_multiplier),
            };
        }
        if let Some(reweighting) = input.measurement_reweighting {
            let standard = core_fusion::IggIiiMeasurementReweighting::standard();
            config.measurement_reweighting = Some(core_fusion::IggIiiMeasurementReweighting {
                k0_sigma: reweighting.k0_sigma.unwrap_or(standard.k0_sigma),
                k1_sigma: reweighting.k1_sigma.unwrap_or(standard.k1_sigma),
            });
        }
        if let Some(adaptation) = input.prediction_adaptation {
            let standard = core_fusion::YangPredictionAdaptiveFactor::standard();
            config.prediction_adaptation = Some(core_fusion::YangPredictionAdaptiveFactor {
                threshold: adaptation.threshold.unwrap_or(standard.threshold),
                outlier_gate_probability: adaptation
                    .outlier_gate_probability
                    .unwrap_or(standard.outlier_gate_probability),
            });
        }
        if let Some(stationary) = input.stationary_updates {
            config.stationary_updates = Some(core_fusion::StationaryUpdateConfig {
                detector: core_fusion::StationaryDetectorConfig {
                    window_len: stationary.detector.window_len,
                    max_specific_force_norm_error_mps2: stationary
                        .detector
                        .max_specific_force_norm_error_mps2,
                    max_body_rate_wrt_ecef_norm_rps: stationary
                        .detector
                        .max_body_rate_wrt_ecef_norm_rps,
                },
                zero_velocity_sigma_mps: stationary.zero_velocity_sigma_mps,
                zero_angular_rate_sigma_rps: stationary.zero_angular_rate_sigma_rps,
            });
        }
        if let Some(nhc) = input.non_holonomic {
            config.non_holonomic = Some(core_fusion::NonHolonomicConstraintConfig {
                lateral_velocity_sigma_mps: nhc.lateral_velocity_sigma_mps,
                vertical_velocity_sigma_mps: nhc.vertical_velocity_sigma_mps,
                min_speed_mps: nhc.min_speed_mps,
                max_body_rate_wrt_ecef_norm_rps: nhc.max_body_rate_wrt_ecef_norm_rps,
            });
        }
    }
    config.validate().map_err(fusion_error)?;
    Ok(config)
}

fn parse_tight_config(
    input: Option<TightConfigInput>,
) -> Result<core_fusion::TightCouplingConfig, JsValue> {
    let mut config = core_fusion::TightCouplingConfig::default();
    if let Some(input) = input {
        if let Some(lever) = input.lever_arm_body_m {
            config.lever_arm_body_m = vec3("tight.leverArmBodyM", &lever)?;
        }
        if let Some(value) = input.light_time {
            config.light_time = value;
        }
        if let Some(value) = input.sagnac {
            config.sagnac = value;
        }
        if let Some(value) = input.initial_clock_bias_variance_m2 {
            config.initial_clock_bias_variance_m2 = value;
        }
        if let Some(value) = input.initial_clock_drift_variance_m2_s2 {
            config.initial_clock_drift_variance_m2_s2 = value;
        }
        if let Some(value) = input.clock_bias_random_walk_m2_s {
            config.clock_bias_random_walk_m2_s = value;
        }
        if let Some(value) = input.clock_drift_random_walk_m2_s3 {
            config.clock_drift_random_walk_m2_s3 = value;
        }
        config.update_options = parse_update_options(input.update_options)?;
    }
    Ok(config)
}

fn parse_time_sync(input: TimeSyncHistoryConfigInput) -> core_fusion::TimeSyncHistoryConfig {
    core_fusion::TimeSyncHistoryConfig::new(input.imu_capacity, input.checkpoint_capacity)
}

fn build_nav_state(input: NavStateInput) -> Result<core_inertial::NavState, JsValue> {
    let state = core_inertial::NavState::new(
        input.t_j2000_s,
        vec3("initialState.positionEcefM", &input.position_ecef_m)?,
        vec3("initialState.velocityEcefMps", &input.velocity_ecef_mps)?,
        mat3_flat(
            "initialState.attitudeBodyToEcef",
            &input.attitude_body_to_ecef,
        )?,
    )
    .map_err(fusion_error)?;
    let accel = input
        .accel_bias_mps2
        .map(|values| vec3("initialState.accelBiasMps2", &values))
        .transpose()?
        .unwrap_or([0.0; 3]);
    let gyro = input
        .gyro_bias_rps
        .map(|values| vec3("initialState.gyroBiasRps", &values))
        .transpose()?
        .unwrap_or([0.0; 3]);
    state.with_biases(accel, gyro).map_err(fusion_error)
}

fn build_filter(config: JsValue) -> Result<core_fusion::InertialFilter, JsValue> {
    let input: FusionConfigInput = serde_wasm_bindgen::from_value(config)
        .map_err(|e| type_error(&format!("invalid fusion config: {e}")))?;
    let layout = parse_layout(input.layout.as_deref())?;
    let nominal = build_nav_state(input.initial_state)?;
    let state = match (input.covariance_diagonal, input.covariance) {
        (Some(diagonal), None) => {
            core_fusion::InsFilterState::from_diagonal(nominal, layout, &diagonal)
        }
        (None, Some(covariance)) => core_fusion::InsFilterState::new(nominal, layout, covariance),
        (Some(_), Some(_)) => {
            return Err(type_error(
                "provide either covarianceDiagonal or covariance, not both",
            ));
        }
        (None, None) => {
            return Err(type_error(
                "fusion config requires covarianceDiagonal or covariance",
            ));
        }
    }
    .map_err(fusion_error)?;

    let mut core_config = core_fusion::InertialFilterConfig::new(parse_imu_spec(input.imu_spec)?)
        .map_err(fusion_error)?;
    core_config.filter_kind = parse_filter_kind(input.filter_kind.as_deref())?;
    core_config.imu_model = parse_imu_error_model(input.imu_model)?;
    if let Some(imu_to_body_dcm) = input.imu_to_body_dcm {
        core_config.imu_to_body_dcm = mat3_flat("imuToBodyDcm", &imu_to_body_dcm)?;
    }
    core_config.mechanization = parse_mechanization(input.mechanization)?;
    core_config.loose = parse_loose_config(input.loose)?;
    core_config.tight = parse_tight_config(input.tight)?;
    core_config.ukf_update_options = parse_ukf_options(input.ukf)?;

    let mut filter =
        core_fusion::InertialFilter::with_config(state, core_config).map_err(fusion_error)?;
    if let Some(time_sync) = input.time_sync {
        filter
            .configure_time_sync_history(parse_time_sync(time_sync))
            .map_err(fusion_error)?;
    }
    Ok(filter)
}

fn parse_imu_sample(value: JsValue) -> Result<core_inertial::ImuSample, JsValue> {
    let input: ImuSampleInput = serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid IMU sample: {e}")))?;
    parse_imu_sample_input(input)
}

fn parse_imu_sample_input(input: ImuSampleInput) -> Result<core_inertial::ImuSample, JsValue> {
    match input.kind.as_deref().unwrap_or("rate") {
        "rate" | "Rate" => Ok(core_inertial::ImuSample::rate(
            input.t_j2000_s,
            vec3(
                "sample.specificForceMps2",
                &input
                    .specific_force_mps2
                    .ok_or_else(|| type_error("sample.specificForceMps2 is required"))?,
            )?,
            vec3(
                "sample.angularRateRps",
                &input
                    .angular_rate_rps
                    .ok_or_else(|| type_error("sample.angularRateRps is required"))?,
            )?,
        )),
        "increment" | "Increment" => Ok(core_inertial::ImuSample::increment(
            input.t_j2000_s,
            vec3(
                "sample.deltaVelocityMps",
                &input
                    .delta_velocity_mps
                    .ok_or_else(|| type_error("sample.deltaVelocityMps is required"))?,
            )?,
            vec3(
                "sample.deltaThetaRad",
                &input
                    .delta_theta_rad
                    .ok_or_else(|| type_error("sample.deltaThetaRad is required"))?,
            )?,
            input
                .dt_s
                .ok_or_else(|| type_error("sample.dtS is required"))?,
        )),
        other => Err(type_error(&format!("invalid IMU sample kind {other:?}"))),
    }
}

fn parse_loose_measurement(value: JsValue) -> Result<core_fusion::GnssFixMeasurement, JsValue> {
    let input: LooseMeasurementInput = serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid loose GNSS measurement: {e}")))?;
    let measurement = core_fusion::GnssFixMeasurement {
        t_j2000_s: input.t_j2000_s,
        position_ecef_m: vec3("measurement.positionEcefM", &input.position_ecef_m)?,
        velocity_ecef_mps: input
            .velocity_ecef_mps
            .map(|values| vec3("measurement.velocityEcefMps", &values))
            .transpose()?,
        covariance: input.covariance,
        satellites_used: input.satellites_used,
        solution_valid: input.solution_valid.unwrap_or(true),
        fix_status: parse_fix_status(input.fix_status.as_deref())?,
    };
    measurement.validate().map_err(fusion_error)?;
    Ok(measurement)
}

fn parse_tight_epoch(value: JsValue) -> Result<core_fusion::TightGnssEpoch, JsValue> {
    let input: TightEpochInput = serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid tight GNSS epoch: {e}")))?;
    let observations = input
        .observations
        .into_iter()
        .map(parse_tight_observation)
        .collect::<Result<Vec<_>, JsValue>>()?;
    core_fusion::TightGnssEpoch::new(input.t_j2000_s, observations).map_err(fusion_error)
}

fn parse_tight_observation(
    input: TightObservationInput,
) -> Result<core_fusion::TightGnssObservation, JsValue> {
    let observation = core_fusion::TightGnssObservation {
        satellite_id: parse_satellite(&input.satellite_id)?,
        pseudorange_m: input.pseudorange_m,
        pseudorange_sigma_m: input.pseudorange_sigma_m,
        range_rate: input
            .range_rate
            .map(|value| core_fusion::TightRangeRateObservation {
                measured_range_rate_m_s: value.measured_range_rate_m_s,
                sigma_m_s: value.sigma_m_s,
                satellite_clock_drift_m_s: value.satellite_clock_drift_m_s.unwrap_or(0.0),
            }),
        carrier_phase: input
            .carrier_phase
            .map(|value| core_fusion::TightCarrierPhaseObservation {
                phase_range_m: value.phase_range_m,
                sigma_m: value.sigma_m,
                float_ambiguity_m: value.float_ambiguity_m,
            }),
        ionosphere_delay_m: input.ionosphere_delay_m.unwrap_or(0.0),
        troposphere_delay_m: input.troposphere_delay_m.unwrap_or(0.0),
    };
    observation.validate().map_err(fusion_error)?;
    Ok(observation)
}

fn filter_state_from_parts(
    state: &core_fusion::InsFilterState,
    last_body_rate_wrt_ecef_rps: [f64; 3],
) -> Result<FilterStateJs, JsValue> {
    let nominal = &state.nominal;
    let quaternion = nominal
        .attitude_quaternion_body_to_ecef()
        .map_err(fusion_error)?;
    Ok(FilterStateJs {
        t_j2000_s: nominal.t_j2000_s,
        position_ecef_m: nominal.position_ecef_m,
        velocity_ecef_mps: nominal.velocity_ecef_mps,
        attitude_body_to_ecef: flat_mat3(&nominal.attitude_body_to_ecef),
        attitude_quaternion_body_to_ecef: [quaternion.w, quaternion.x, quaternion.y, quaternion.z],
        attitude_yaw_pitch_roll_rad: nominal.attitude_yaw_pitch_roll_rad(),
        accel_bias_mps2: nominal.accel_bias_mps2,
        gyro_bias_rps: nominal.gyro_bias_rps,
        layout: layout_label(state.layout()).to_string(),
        error_state: state.error_state.as_slice().to_vec(),
        covariance: state.covariance.clone(),
        accel_scale_factor: state.accel_scale_factor,
        gyro_scale_factor: state.gyro_scale_factor,
        last_body_rate_wrt_ecef_rps,
    })
}

fn filter_state_js(filter: &core_fusion::InertialFilter) -> Result<FilterStateJs, JsValue> {
    filter_state_from_parts(filter.state(), filter.last_body_rate_wrt_ecef_rps())
}

fn tight_snapshot_js(snapshot: &core_fusion::TightFilterSnapshot) -> TightFilterSnapshotJs {
    TightFilterSnapshotJs {
        clock_bias_m: snapshot.clock_bias_m,
        clock_drift_m_s: snapshot.clock_drift_m_s,
        augmented_covariance: snapshot.augmented_covariance.clone(),
    }
}

fn snapshot_js(
    snapshot: &core_fusion::InertialFilterSnapshot,
) -> Result<InertialFilterSnapshotJs, JsValue> {
    Ok(InertialFilterSnapshotJs {
        state: filter_state_from_parts(&snapshot.state, snapshot.last_body_rate_wrt_ecef_rps)?,
        tight: tight_snapshot_js(&snapshot.tight),
    })
}

fn clock_state_js(filter: &core_fusion::InertialFilter) -> Result<TightClockStateJs, JsValue> {
    let clock = filter.tight_clock_state().map_err(fusion_error)?;
    Ok(TightClockStateJs {
        bias_m: clock.bias_m,
        drift_m_s: clock.drift_m_s,
        covariance: clock
            .covariance
            .iter()
            .flat_map(|row| row.iter().copied())
            .collect(),
    })
}

fn update_js(update: core_fusion::FusionUpdate) -> FusionUpdateJs {
    FusionUpdateJs {
        applied: update.applied,
        nis: update.nis,
        rows: update.rows,
        accepted_rows: update.accepted_rows,
        rejected_rows: update.rejected_rows,
        ekf: EkfReportJs::from(update.ekf),
    }
}

/// Stateful GNSS/INS filter resource.
#[wasm_bindgen]
pub struct GnssInsFilter {
    inner: core_fusion::InertialFilter,
}

#[wasm_bindgen]
impl GnssInsFilter {
    /// Build a filter from a JS configuration object.
    #[wasm_bindgen(constructor)]
    pub fn new(config: JsValue) -> Result<GnssInsFilter, JsValue> {
        Ok(GnssInsFilter {
            inner: build_filter(config)?,
        })
    }

    /// Build a filter, then restore its state from versioned fusion-state bytes.
    #[wasm_bindgen(js_name = fromStateBytes)]
    pub fn from_state_bytes(config: JsValue, bytes: &[u8]) -> Result<GnssInsFilter, JsValue> {
        let mut filter = build_filter(config)?;
        filter.restore_encoded_state(bytes).map_err(fusion_error)?;
        Ok(GnssInsFilter { inner: filter })
    }

    /// Current INS state, covariance, and last body-rate diagnostic.
    #[wasm_bindgen]
    pub fn state(&self) -> Result<JsValue, JsValue> {
        to_js(&filter_state_js(&self.inner)?)
    }

    /// Current tight-coupling receiver-clock state.
    #[wasm_bindgen(js_name = tightClockState)]
    pub fn tight_clock_state(&self) -> Result<JsValue, JsValue> {
        to_js(&clock_state_js(&self.inner)?)
    }

    /// Propagate the filter with one IMU rate or increment sample.
    #[wasm_bindgen]
    pub fn propagate(&mut self, sample: JsValue) -> Result<JsValue, JsValue> {
        self.inner
            .propagate(parse_imu_sample(sample)?)
            .map_err(fusion_error)?;
        self.state()
    }

    /// Propagate and record the transition for later fusion RTS smoothing.
    #[wasm_bindgen(js_name = propagateRecorded)]
    pub fn propagate_recorded(
        &mut self,
        sample: JsValue,
        history: &mut FusionRtsHistoryBuilder,
    ) -> Result<JsValue, JsValue> {
        self.inner
            .propagate_recorded(parse_imu_sample(sample)?, &mut history.inner)
            .map_err(fusion_error)?;
        self.state()
    }

    /// Propagate the filter with a JS array of IMU samples.
    #[wasm_bindgen(js_name = propagateBatch)]
    pub fn propagate_batch(&mut self, samples: JsValue) -> Result<JsValue, JsValue> {
        let samples: Vec<ImuSampleInput> = serde_wasm_bindgen::from_value(samples)
            .map_err(|e| type_error(&format!("invalid IMU sample array: {e}")))?;
        for sample in samples {
            self.inner
                .propagate(parse_imu_sample_input(sample)?)
                .map_err(fusion_error)?;
        }
        self.state()
    }

    /// Apply a loose position or position-velocity GNSS fix at the current epoch.
    #[wasm_bindgen(js_name = updateLoose)]
    pub fn update_loose(&mut self, measurement: JsValue) -> Result<JsValue, JsValue> {
        let update = self
            .inner
            .update_loose(&parse_loose_measurement(measurement)?)
            .map_err(fusion_error)?;
        to_js(&update_js(update))
    }

    /// Apply a loose GNSS fix and record checkpoints for fusion RTS smoothing.
    #[wasm_bindgen(js_name = updateLooseRecorded)]
    pub fn update_loose_recorded(
        &mut self,
        measurement: JsValue,
        history: &mut FusionRtsHistoryBuilder,
    ) -> Result<JsValue, JsValue> {
        let update = self
            .inner
            .update_loose_recorded(&parse_loose_measurement(measurement)?, &mut history.inner)
            .map_err(fusion_error)?;
        to_js(&update_js(update))
    }

    /// Apply a loose GNSS fix, replaying retained IMU checkpoints when it is late.
    #[wasm_bindgen(js_name = updateLooseTimeSync)]
    pub fn update_loose_time_sync(&mut self, measurement: JsValue) -> Result<JsValue, JsValue> {
        let update = self
            .inner
            .update_loose_time_sync(&parse_loose_measurement(measurement)?)
            .map_err(fusion_error)?;
        to_js(&TimeSyncUpdateJs::from(update))
    }

    /// Apply a gated zero-velocity and zero-angular-rate update.
    #[wasm_bindgen(js_name = updateStationary)]
    pub fn update_stationary(&mut self) -> Result<JsValue, JsValue> {
        let update = self.inner.update_stationary().map_err(fusion_error)?;
        to_js(&update.map(update_js))
    }

    /// Apply a stationary update and record checkpoints when an update applies.
    #[wasm_bindgen(js_name = updateStationaryRecorded)]
    pub fn update_stationary_recorded(
        &mut self,
        history: &mut FusionRtsHistoryBuilder,
    ) -> Result<JsValue, JsValue> {
        let update = self
            .inner
            .update_stationary_recorded(&mut history.inner)
            .map_err(fusion_error)?;
        to_js(&update.map(update_js))
    }

    /// Apply a gated wheeled-vehicle non-holonomic constraint update.
    #[wasm_bindgen(js_name = updateNonHolonomic)]
    pub fn update_non_holonomic(&mut self) -> Result<JsValue, JsValue> {
        let update = self.inner.update_non_holonomic().map_err(fusion_error)?;
        to_js(&update.map(update_js))
    }

    /// Apply a non-holonomic constraint and record checkpoints when an update applies.
    #[wasm_bindgen(js_name = updateNonHolonomicRecorded)]
    pub fn update_non_holonomic_recorded(
        &mut self,
        history: &mut FusionRtsHistoryBuilder,
    ) -> Result<JsValue, JsValue> {
        let update = self
            .inner
            .update_non_holonomic_recorded(&mut history.inner)
            .map_err(fusion_error)?;
        to_js(&update.map(update_js))
    }

    /// Apply a tight raw-observation epoch against an SP3 source.
    #[wasm_bindgen(js_name = updateTightSp3)]
    pub fn update_tight_sp3(&mut self, sp3: &Sp3, epoch: JsValue) -> Result<JsValue, JsValue> {
        let update = self
            .inner
            .update_tight(&sp3.inner, &parse_tight_epoch(epoch)?)
            .map_err(fusion_error)?;
        to_js(&update_js(update))
    }

    /// Apply a tight SP3 update and record checkpoints for fusion RTS smoothing.
    #[wasm_bindgen(js_name = updateTightSp3Recorded)]
    pub fn update_tight_sp3_recorded(
        &mut self,
        sp3: &Sp3,
        epoch: JsValue,
        history: &mut FusionRtsHistoryBuilder,
    ) -> Result<JsValue, JsValue> {
        let update = self
            .inner
            .update_tight_recorded(&sp3.inner, &parse_tight_epoch(epoch)?, &mut history.inner)
            .map_err(fusion_error)?;
        to_js(&update_js(update))
    }

    /// Apply a tight raw-observation epoch against a broadcast ephemeris source.
    #[wasm_bindgen(js_name = updateTightBroadcast)]
    pub fn update_tight_broadcast(
        &mut self,
        broadcast: &BroadcastEphemeris,
        epoch: JsValue,
    ) -> Result<JsValue, JsValue> {
        let update = self
            .inner
            .update_tight(&broadcast.inner, &parse_tight_epoch(epoch)?)
            .map_err(fusion_error)?;
        to_js(&update_js(update))
    }

    /// Apply a tight broadcast update and record checkpoints for fusion RTS smoothing.
    #[wasm_bindgen(js_name = updateTightBroadcastRecorded)]
    pub fn update_tight_broadcast_recorded(
        &mut self,
        broadcast: &BroadcastEphemeris,
        epoch: JsValue,
        history: &mut FusionRtsHistoryBuilder,
    ) -> Result<JsValue, JsValue> {
        let update = self
            .inner
            .update_tight_recorded(
                &broadcast.inner,
                &parse_tight_epoch(epoch)?,
                &mut history.inner,
            )
            .map_err(fusion_error)?;
        to_js(&update_js(update))
    }

    /// Apply a tight SP3 update, replaying retained IMU checkpoints when it is late.
    #[wasm_bindgen(js_name = updateTightSp3TimeSync)]
    pub fn update_tight_sp3_time_sync(
        &mut self,
        sp3: &Sp3,
        epoch: JsValue,
    ) -> Result<JsValue, JsValue> {
        let update = self
            .inner
            .update_tight_time_sync(&sp3.inner, &parse_tight_epoch(epoch)?)
            .map_err(fusion_error)?;
        to_js(&TimeSyncUpdateJs::from(update))
    }

    /// Apply a tight broadcast update, replaying retained IMU checkpoints when it is late.
    #[wasm_bindgen(js_name = updateTightBroadcastTimeSync)]
    pub fn update_tight_broadcast_time_sync(
        &mut self,
        broadcast: &BroadcastEphemeris,
        epoch: JsValue,
    ) -> Result<JsValue, JsValue> {
        let update = self
            .inner
            .update_tight_time_sync(&broadcast.inner, &parse_tight_epoch(epoch)?)
            .map_err(fusion_error)?;
        to_js(&TimeSyncUpdateJs::from(update))
    }

    /// Replace retained-history capacities for later time-sync replay.
    #[wasm_bindgen(js_name = configureTimeSync)]
    pub fn configure_time_sync(&mut self, config: JsValue) -> Result<(), JsValue> {
        let config: TimeSyncHistoryConfigInput = serde_wasm_bindgen::from_value(config)
            .map_err(|e| type_error(&format!("invalid time-sync config: {e}")))?;
        self.inner
            .configure_time_sync_history(parse_time_sync(config))
            .map_err(fusion_error)
    }

    /// Current retained-history capacity and occupancy.
    #[wasm_bindgen(js_name = timeSyncStatus)]
    pub fn time_sync_status(&self) -> Result<JsValue, JsValue> {
        to_js(&TimeSyncHistoryStatusJs::from(
            self.inner.time_sync_history_status(),
        ))
    }

    /// Encode the current fusion state with the core versioned binary codec.
    #[wasm_bindgen(js_name = encodeState)]
    pub fn encode_state(&self) -> Result<Vec<u8>, JsValue> {
        self.inner.encode_state().map_err(fusion_error)
    }

    /// Restore this filter from versioned fusion-state bytes.
    #[wasm_bindgen(js_name = restoreState)]
    pub fn restore_state(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        self.inner
            .restore_encoded_state(bytes)
            .map_err(fusion_error)
    }
}

/// Builder for recording a fusion forward pass before RTS smoothing.
#[wasm_bindgen]
#[derive(Clone)]
pub struct FusionRtsHistoryBuilder {
    inner: core_fusion::FusionRtsHistoryBuilder,
}

#[wasm_bindgen]
impl FusionRtsHistoryBuilder {
    /// Start an empty history for manual recording.
    #[wasm_bindgen(constructor)]
    pub fn new() -> FusionRtsHistoryBuilder {
        Self {
            inner: core_fusion::FusionRtsHistoryBuilder::empty(),
        }
    }

    /// Start a history from the filter's current checkpoint.
    #[wasm_bindgen(js_name = fromFilter)]
    pub fn from_filter(filter: &GnssInsFilter) -> Result<FusionRtsHistoryBuilder, JsValue> {
        let inner = core_fusion::FusionRtsHistoryBuilder::from_filter(&filter.inner)
            .map_err(fusion_error)?;
        Ok(Self { inner })
    }

    /// Return a validated recorded history.
    pub fn finish(&self) -> Result<FusionRtsHistory, JsValue> {
        let inner = self.inner.clone().finish().map_err(fusion_error)?;
        Ok(FusionRtsHistory { inner })
    }
}

impl Default for FusionRtsHistoryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Recorded fusion forward-pass history accepted by `smoothFusionRts`.
#[wasm_bindgen]
#[derive(Clone)]
pub struct FusionRtsHistory {
    inner: core_fusion::FusionRtsHistory,
}

#[wasm_bindgen]
impl FusionRtsHistory {
    /// Recorded epochs in forward time order.
    #[wasm_bindgen(getter)]
    pub fn epochs(&self) -> Result<JsValue, JsValue> {
        let epochs = self
            .inner
            .epochs
            .iter()
            .map(FusionRtsEpochJs::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        to_js(&epochs)
    }

    /// Number of recorded epochs.
    #[wasm_bindgen(getter, js_name = epochCount)]
    pub fn epoch_count(&self) -> usize {
        self.inner.epochs.len()
    }
}

/// Smoothed fusion trajectory returned by fixed-interval RTS smoothing.
#[wasm_bindgen]
#[derive(Clone)]
pub struct SmoothedFusionTrajectory {
    inner: core_fusion::SmoothedFusionTrajectory,
}

#[wasm_bindgen]
impl SmoothedFusionTrajectory {
    /// Smoothed epochs in the same order as the recorded history.
    #[wasm_bindgen(getter)]
    pub fn epochs(&self) -> Result<JsValue, JsValue> {
        let epochs = self
            .inner
            .epochs
            .iter()
            .map(SmoothedFusionEpochJs::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        to_js(&epochs)
    }

    /// Number of smoothed epochs.
    #[wasm_bindgen(getter, js_name = epochCount)]
    pub fn epoch_count(&self) -> usize {
        self.inner.epochs.len()
    }
}

/// Apply fixed-interval RTS smoothing to recorded fusion history.
#[wasm_bindgen(js_name = smoothFusionRts)]
pub fn smooth_fusion_rts(history: &FusionRtsHistory) -> Result<SmoothedFusionTrajectory, JsValue> {
    let inner = core_fusion::smooth_fusion_rts(&history.inner).map_err(fusion_error)?;
    Ok(SmoothedFusionTrajectory { inner })
}

/// Blend a first good post-outage fix back over an outage span.
#[wasm_bindgen(js_name = velocityMatchOutage)]
pub fn velocity_match_outage(
    states: JsValue,
    first_good_fix: JsValue,
    config: JsValue,
) -> Result<JsValue, JsValue> {
    let states: Vec<VelocityMatchStateInput> = serde_wasm_bindgen::from_value(states)
        .map_err(|e| type_error(&format!("invalid velocity-match states: {e}")))?;
    let states = states
        .into_iter()
        .map(|state| {
            core_fusion::VelocityMatchState::new(
                state.t_j2000_s,
                vec3("velocityMatch.state.positionEcefM", &state.position_ecef_m)?,
                vec3(
                    "velocityMatch.state.velocityEcefMps",
                    &state.velocity_ecef_mps,
                )?,
            )
            .map_err(fusion_error)
        })
        .collect::<Result<Vec<_>, JsValue>>()?;
    let config: VelocityMatchingInput = serde_wasm_bindgen::from_value(config)
        .map_err(|e| type_error(&format!("invalid velocity-match config: {e}")))?;
    let matched = core_fusion::velocity_match_outage(
        &states,
        &parse_loose_measurement(first_good_fix)?,
        core_fusion::VelocityMatchingConfig {
            max_outage_duration_s: config.max_outage_duration_s,
        },
    )
    .map_err(fusion_error)?;
    to_js(&VelocityMatchedTrajectoryJs::from(matched))
}

/// Decode and re-encode fusion-state bytes through the core codec.
#[wasm_bindgen(js_name = fusionStateBytesRoundTrip)]
pub fn fusion_state_bytes_round_trip(bytes: &[u8]) -> Result<Vec<u8>, JsValue> {
    let state =
        core_fusion::SerializableFusionState::decode_versioned(bytes).map_err(fusion_error)?;
    state.encode_versioned().map_err(fusion_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FusionConfigInput {
    initial_state: NavStateInput,
    layout: Option<String>,
    covariance_diagonal: Option<Vec<f64>>,
    covariance: Option<Vec<Vec<f64>>>,
    imu_spec: Option<ImuSpecInput>,
    filter_kind: Option<String>,
    imu_model: Option<ImuErrorModelInput>,
    imu_to_body_dcm: Option<Vec<f64>>,
    mechanization: Option<MechanizationConfigInput>,
    loose: Option<LooseConfigInput>,
    tight: Option<TightConfigInput>,
    ukf: Option<UkfUpdateOptionsInput>,
    time_sync: Option<TimeSyncHistoryConfigInput>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NavStateInput {
    t_j2000_s: f64,
    position_ecef_m: Vec<f64>,
    velocity_ecef_mps: Vec<f64>,
    attitude_body_to_ecef: Vec<f64>,
    accel_bias_mps2: Option<Vec<f64>>,
    gyro_bias_rps: Option<Vec<f64>>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ImuSpecInput {
    Preset(String),
    Object(ImuSpecObjectInput),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImuSpecObjectInput {
    preset: Option<String>,
    accel_vrw_mps_sqrt_s: Option<f64>,
    gyro_arw_rad_sqrt_s: Option<f64>,
    accel_bias_instab_mps2: Option<f64>,
    gyro_bias_instab_rps: Option<f64>,
    accel_bias_tau_s: Option<f64>,
    gyro_bias_tau_s: Option<f64>,
    accel_scale_instab_ppm: Option<f64>,
    gyro_scale_instab_ppm: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImuErrorModelInput {
    bias: Option<ImuBiasInput>,
    calibration: Option<ImuCalibrationInput>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImuBiasInput {
    accel_mps2: Vec<f64>,
    gyro_rps: Vec<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImuCalibrationInput {
    accel_scale_misalignment: Vec<f64>,
    gyro_scale_misalignment: Vec<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MechanizationConfigInput {
    coning_correction: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LooseConfigInput {
    lever_arm_body_m: Option<Vec<f64>>,
    update_options: Option<UpdateOptionsInput>,
    fix_status_weighting: Option<FixStatusWeightingInput>,
    measurement_reweighting: Option<IggIiiMeasurementReweightingInput>,
    prediction_adaptation: Option<YangPredictionAdaptiveFactorInput>,
    stationary_updates: Option<StationaryUpdateInput>,
    non_holonomic: Option<NonHolonomicInput>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixStatusWeightingInput {
    single_sigma_multiplier: Option<f64>,
    float_sigma_multiplier: Option<f64>,
    fixed_sigma_multiplier: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IggIiiMeasurementReweightingInput {
    k0_sigma: Option<f64>,
    k1_sigma: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct YangPredictionAdaptiveFactorInput {
    threshold: Option<f64>,
    outlier_gate_probability: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StationaryUpdateInput {
    detector: StationaryDetectorInput,
    zero_velocity_sigma_mps: f64,
    zero_angular_rate_sigma_rps: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StationaryDetectorInput {
    window_len: usize,
    max_specific_force_norm_error_mps2: f64,
    max_body_rate_wrt_ecef_norm_rps: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NonHolonomicInput {
    lateral_velocity_sigma_mps: f64,
    vertical_velocity_sigma_mps: f64,
    min_speed_mps: f64,
    max_body_rate_wrt_ecef_norm_rps: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VelocityMatchingInput {
    max_outage_duration_s: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TightConfigInput {
    lever_arm_body_m: Option<Vec<f64>>,
    light_time: Option<bool>,
    sagnac: Option<bool>,
    initial_clock_bias_variance_m2: Option<f64>,
    initial_clock_drift_variance_m2_s2: Option<f64>,
    clock_bias_random_walk_m2_s: Option<f64>,
    clock_drift_random_walk_m2_s3: Option<f64>,
    update_options: Option<UpdateOptionsInput>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateOptionsInput {
    innovation_gate: Option<InnovationGateInput>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InnovationGateInput {
    threshold_sigma: f64,
    min_rows: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UkfUpdateOptionsInput {
    transform: Option<UnscentedTransformInput>,
    innovation_gate: Option<InnovationGateInput>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UnscentedTransformInput {
    alpha: Option<f64>,
    beta: Option<f64>,
    kappa: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TimeSyncHistoryConfigInput {
    imu_capacity: usize,
    checkpoint_capacity: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImuSampleInput {
    t_j2000_s: f64,
    kind: Option<String>,
    specific_force_mps2: Option<Vec<f64>>,
    angular_rate_rps: Option<Vec<f64>>,
    delta_velocity_mps: Option<Vec<f64>>,
    delta_theta_rad: Option<Vec<f64>>,
    dt_s: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VelocityMatchStateInput {
    t_j2000_s: f64,
    position_ecef_m: Vec<f64>,
    velocity_ecef_mps: Vec<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LooseMeasurementInput {
    t_j2000_s: f64,
    position_ecef_m: Vec<f64>,
    velocity_ecef_mps: Option<Vec<f64>>,
    covariance: Vec<Vec<f64>>,
    satellites_used: usize,
    solution_valid: Option<bool>,
    fix_status: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TightEpochInput {
    t_j2000_s: f64,
    observations: Vec<TightObservationInput>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TightObservationInput {
    satellite_id: String,
    pseudorange_m: f64,
    pseudorange_sigma_m: f64,
    range_rate: Option<TightRangeRateInput>,
    carrier_phase: Option<TightCarrierPhaseInput>,
    ionosphere_delay_m: Option<f64>,
    troposphere_delay_m: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TightRangeRateInput {
    measured_range_rate_m_s: f64,
    sigma_m_s: f64,
    satellite_clock_drift_m_s: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TightCarrierPhaseInput {
    phase_range_m: f64,
    sigma_m: f64,
    float_ambiguity_m: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FilterStateJs {
    t_j2000_s: f64,
    position_ecef_m: [f64; 3],
    velocity_ecef_mps: [f64; 3],
    attitude_body_to_ecef: Vec<f64>,
    attitude_quaternion_body_to_ecef: [f64; 4],
    attitude_yaw_pitch_roll_rad: [f64; 3],
    accel_bias_mps2: [f64; 3],
    gyro_bias_rps: [f64; 3],
    layout: String,
    error_state: Vec<f64>,
    covariance: Vec<Vec<f64>>,
    accel_scale_factor: [f64; 3],
    gyro_scale_factor: [f64; 3],
    last_body_rate_wrt_ecef_rps: [f64; 3],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TightClockStateJs {
    bias_m: f64,
    drift_m_s: f64,
    covariance: Vec<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TightFilterSnapshotJs {
    clock_bias_m: f64,
    clock_drift_m_s: f64,
    augmented_covariance: Vec<Vec<f64>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InertialFilterSnapshotJs {
    state: FilterStateJs,
    tight: TightFilterSnapshotJs,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FusionRtsEpochJs {
    t_j2000_s: f64,
    predicted: InertialFilterSnapshotJs,
    updated: InertialFilterSnapshotJs,
    transition_from_previous: Option<Vec<Vec<f64>>>,
}

impl TryFrom<&core_fusion::FusionRtsEpoch> for FusionRtsEpochJs {
    type Error = JsValue;

    fn try_from(value: &core_fusion::FusionRtsEpoch) -> Result<Self, Self::Error> {
        Ok(Self {
            t_j2000_s: value.t_j2000_s,
            predicted: snapshot_js(&value.predicted)?,
            updated: snapshot_js(&value.updated)?,
            transition_from_previous: value.transition_from_previous.clone(),
        })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SmoothedFusionEpochJs {
    t_j2000_s: f64,
    snapshot: InertialFilterSnapshotJs,
    error_state_correction: Vec<f64>,
    covariance: Vec<Vec<f64>>,
    rts_gain_to_next: Option<Vec<Vec<f64>>>,
}

impl TryFrom<&core_fusion::SmoothedFusionEpoch> for SmoothedFusionEpochJs {
    type Error = JsValue;

    fn try_from(value: &core_fusion::SmoothedFusionEpoch) -> Result<Self, Self::Error> {
        Ok(Self {
            t_j2000_s: value.t_j2000_s,
            snapshot: snapshot_js(&value.snapshot)?,
            error_state_correction: value.error_state_correction.clone(),
            covariance: value.covariance.clone(),
            rts_gain_to_next: value.rts_gain_to_next.clone(),
        })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FusionUpdateJs {
    applied: bool,
    nis: f64,
    rows: usize,
    accepted_rows: usize,
    rejected_rows: usize,
    ekf: EkfReportJs,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EkfReportJs {
    applied: bool,
    normalized_innovation_squared: f64,
    accepted_rows: usize,
    rejected_rows: usize,
    innovation_gate: Option<InnovationGateReportJs>,
    innovation_covariance: Vec<Vec<f64>>,
    kalman_gain: Vec<Vec<f64>>,
    dx: Vec<f64>,
}

impl From<core_fusion::EkfCorrectionReport> for EkfReportJs {
    fn from(value: core_fusion::EkfCorrectionReport) -> Self {
        Self {
            applied: value.applied,
            normalized_innovation_squared: value.normalized_innovation_squared,
            accepted_rows: value.accepted_rows,
            rejected_rows: value.rejected_rows,
            innovation_gate: value.innovation_gate.map(InnovationGateReportJs::from),
            innovation_covariance: value.innovation_covariance,
            kalman_gain: value.kalman_gain,
            dx: value.dx,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InnovationGateReportJs {
    threshold_sigma: f64,
    min_rows: usize,
    input_rows: usize,
    accepted_rows: usize,
    rejected_rows: usize,
    max_abs_normalized_innovation: Option<f64>,
    max_rejected_abs_normalized_innovation: Option<f64>,
    coasted: bool,
}

impl From<core_fusion::InnovationGateReport> for InnovationGateReportJs {
    fn from(value: core_fusion::InnovationGateReport) -> Self {
        Self {
            threshold_sigma: value.threshold_sigma,
            min_rows: value.min_rows,
            input_rows: value.input_rows,
            accepted_rows: value.accepted_rows,
            rejected_rows: value.rejected_rows,
            max_abs_normalized_innovation: value.max_abs_normalized_innovation,
            max_rejected_abs_normalized_innovation: value.max_rejected_abs_normalized_innovation,
            coasted: value.coasted,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TimeSyncUpdateJs {
    update: FusionUpdateJs,
    late_measurement: bool,
    replayed_imu_segments: usize,
    restored_checkpoint_epoch_j2000_s: f64,
    current_epoch_j2000_s: f64,
}

impl From<core_fusion::TimeSyncUpdate> for TimeSyncUpdateJs {
    fn from(value: core_fusion::TimeSyncUpdate) -> Self {
        Self {
            update: update_js(value.update),
            late_measurement: value.late_measurement,
            replayed_imu_segments: value.replayed_imu_segments,
            restored_checkpoint_epoch_j2000_s: value.restored_checkpoint_epoch_j2000_s,
            current_epoch_j2000_s: value.current_epoch_j2000_s,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TimeSyncHistoryStatusJs {
    imu_capacity: usize,
    imu_len: usize,
    checkpoint_capacity: usize,
    checkpoint_len: usize,
    oldest_imu_epoch_j2000_s: Option<f64>,
    newest_imu_epoch_j2000_s: Option<f64>,
    oldest_checkpoint_epoch_j2000_s: Option<f64>,
    newest_checkpoint_epoch_j2000_s: Option<f64>,
}

impl From<core_fusion::TimeSyncHistoryStatus> for TimeSyncHistoryStatusJs {
    fn from(value: core_fusion::TimeSyncHistoryStatus) -> Self {
        Self {
            imu_capacity: value.imu_capacity,
            imu_len: value.imu_len,
            checkpoint_capacity: value.checkpoint_capacity,
            checkpoint_len: value.checkpoint_len,
            oldest_imu_epoch_j2000_s: value.oldest_imu_epoch_j2000_s,
            newest_imu_epoch_j2000_s: value.newest_imu_epoch_j2000_s,
            oldest_checkpoint_epoch_j2000_s: value.oldest_checkpoint_epoch_j2000_s,
            newest_checkpoint_epoch_j2000_s: value.newest_checkpoint_epoch_j2000_s,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VelocityMatchStateJs {
    t_j2000_s: f64,
    position_ecef_m: [f64; 3],
    velocity_ecef_mps: [f64; 3],
}

impl From<core_fusion::VelocityMatchState> for VelocityMatchStateJs {
    fn from(value: core_fusion::VelocityMatchState) -> Self {
        Self {
            t_j2000_s: value.t_j2000_s,
            position_ecef_m: value.position_ecef_m,
            velocity_ecef_mps: value.velocity_ecef_mps,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VelocityMatchedTrajectoryJs {
    states: Vec<VelocityMatchStateJs>,
    endpoint_position_correction_ecef_m: [f64; 3],
    endpoint_velocity_correction_ecef_mps: [f64; 3],
}

impl From<core_fusion::VelocityMatchedTrajectory> for VelocityMatchedTrajectoryJs {
    fn from(value: core_fusion::VelocityMatchedTrajectory) -> Self {
        Self {
            states: value.states.into_iter().map(Into::into).collect(),
            endpoint_position_correction_ecef_m: value.endpoint_position_correction_ecef_m,
            endpoint_velocity_correction_ecef_mps: value.endpoint_velocity_correction_ecef_mps,
        }
    }
}
