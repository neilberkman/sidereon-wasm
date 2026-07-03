//! State covariance propagation and STM transport.
//!
//! This module only marshals flat JS arrays into the core covariance transport
//! types. The STM construction, Phi P Phi^T transport, RTN frame handling, and
//! process-noise math all run in `sidereon_core`.

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use sidereon_core::astro::forces::{DragForce, DragParameters, SpaceWeather};
use sidereon_core::astro::propagator::{
    transport_covariance, CovarianceFrame as CoreCovarianceFrame, CovariancePropagationOptions,
    CovarianceSegment, ForceModelKind, IntegratorKind, IntegratorOptions, LabeledCovariance6,
    ProcessNoise, StatePropagator, StateTransitionMatrix,
};
use sidereon_core::astro::state::CartesianState;

use crate::error::{engine_error, range_error, type_error};
use crate::marshal::{covariance6_flat, covariance6_from_flat, vec3_finite};

/// Frame a 6x6 state covariance is expressed in.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CovarianceFrame {
    Inertial,
    Rtn,
}

fn parse_frame(label: Option<&str>) -> Result<CoreCovarianceFrame, JsValue> {
    match label.unwrap_or("inertial") {
        "inertial" => Ok(CoreCovarianceFrame::Inertial),
        "rtn" => Ok(CoreCovarianceFrame::Rtn),
        other => Err(type_error(&format!(
            "invalid covariance frame {other:?}: expected \"inertial\" or \"rtn\""
        ))),
    }
}

fn frame_label(frame: CoreCovarianceFrame) -> &'static str {
    match frame {
        CoreCovarianceFrame::Inertial => "inertial",
        CoreCovarianceFrame::Rtn => "rtn",
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ProcessNoiseInput {
    q_radial_km2_s3: Option<f64>,
    q_transverse_km2_s3: Option<f64>,
    q_normal_km2_s3: Option<f64>,
}

fn process_noise(input: Option<ProcessNoiseInput>) -> ProcessNoise {
    if let Some(input) = input {
        ProcessNoise::RtnAccelerationPsd {
            q_radial_km2_s3: input.q_radial_km2_s3.unwrap_or(0.0),
            q_transverse_km2_s3: input.q_transverse_km2_s3.unwrap_or(0.0),
            q_normal_km2_s3: input.q_normal_km2_s3.unwrap_or(0.0),
        }
    } else {
        ProcessNoise::None
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CovariancePropagationRequest {
    epoch_s: f64,
    position_km: Vec<f64>,
    velocity_km_s: Vec<f64>,
    covariance: Vec<f64>,
    times_s: Vec<f64>,
    #[serde(default)]
    covariance_frame: Option<String>,
    #[serde(default)]
    output_frame: Option<String>,
    #[serde(default)]
    process_noise: Option<ProcessNoiseInput>,
    #[serde(default)]
    force_model: Option<String>,
    #[serde(default)]
    integrator: Option<String>,
    #[serde(default)]
    abs_tol: Option<f64>,
    #[serde(default)]
    rel_tol: Option<f64>,
    #[serde(default)]
    initial_step_s: Option<f64>,
    #[serde(default)]
    min_step_s: Option<f64>,
    #[serde(default)]
    max_step_s: Option<f64>,
    #[serde(default)]
    max_steps: Option<u32>,
    #[serde(default)]
    mu_km3_s2: Option<f64>,
    #[serde(default)]
    drag: Option<DragInput>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct SpaceWeatherInput {
    f107: Option<f64>,
    f107a: Option<f64>,
    ap: Option<f64>,
}

impl SpaceWeatherInput {
    fn to_core(&self) -> SpaceWeather {
        let defaults = SpaceWeather::default();
        SpaceWeather {
            f107: self.f107.unwrap_or(defaults.f107),
            f107a: self.f107a.unwrap_or(defaults.f107a),
            ap: self.ap.unwrap_or(defaults.ap),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DragInput {
    #[serde(default)]
    bc_factor_m2_kg: Option<f64>,
    #[serde(default)]
    ballistic_coefficient_kg_m2: Option<f64>,
    #[serde(default)]
    cd: Option<f64>,
    #[serde(default)]
    area_m2: Option<f64>,
    #[serde(default)]
    mass_kg: Option<f64>,
    #[serde(default)]
    cutoff_altitude_km: Option<f64>,
    #[serde(default)]
    space_weather: SpaceWeatherInput,
}

impl DragInput {
    fn to_core(&self) -> Result<DragParameters, JsValue> {
        let cutoff = self
            .cutoff_altitude_km
            .unwrap_or(DragForce::DEFAULT_REENTRY_ALTITUDE_KM);
        let sw = self.space_weather.to_core();
        if let Some(bc_factor) = self.bc_factor_m2_kg {
            return DragParameters::from_bc_factor_m2_kg(bc_factor, sw, cutoff)
                .map_err(engine_error);
        }
        if let Some(bc) = self.ballistic_coefficient_kg_m2 {
            return DragParameters::from_ballistic_coefficient(bc, sw, cutoff)
                .map_err(engine_error);
        }
        match (self.cd, self.area_m2, self.mass_kg) {
            (Some(cd), Some(area_m2), Some(mass_kg)) => {
                DragParameters::from_area_mass(cd, area_m2, mass_kg, sw, cutoff)
                    .map_err(engine_error)
            }
            _ => Err(type_error(
                "drag requires bcFactorM2Kg, ballisticCoefficientKgM2, or cd/areaM2/massKg",
            )),
        }
    }
}

fn force_model(label: Option<&str>, mu_km3_s2: Option<f64>) -> Result<ForceModelKind, JsValue> {
    match label.unwrap_or("two_body") {
        "two_body" => Ok(match mu_km3_s2 {
            Some(mu_km3_s2) => ForceModelKind::TwoBody { mu_km3_s2 },
            None => ForceModelKind::two_body(),
        }),
        "two_body_j2" => {
            if mu_km3_s2.is_some() {
                return Err(type_error(
                    "muKm3S2 override is only supported with forceModel \"two_body\"",
                ));
            }
            Ok(ForceModelKind::two_body_j2())
        }
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

fn integrator_options(req: &CovariancePropagationRequest) -> Result<IntegratorOptions, JsValue> {
    let defaults = IntegratorOptions::default();
    let options = IntegratorOptions {
        abs_tol: req.abs_tol.unwrap_or(defaults.abs_tol),
        rel_tol: req.rel_tol.unwrap_or(defaults.rel_tol),
        initial_step: req.initial_step_s.unwrap_or(defaults.initial_step),
        min_step: req.min_step_s.unwrap_or(defaults.min_step),
        max_step: req.max_step_s.unwrap_or(defaults.max_step),
        max_steps: req.max_steps.unwrap_or(defaults.max_steps),
        dense_output: false,
    };
    if options.initial_step <= 0.0 {
        return Err(range_error("initialStepS must be positive"));
    }
    Ok(options)
}

/// Propagated state plus covariance nodes.
#[wasm_bindgen]
pub struct CovarianceEphemeris {
    times: Vec<f64>,
    positions: Vec<f64>,
    velocities: Vec<f64>,
    covariances: Vec<f64>,
    frame: CoreCovarianceFrame,
    inner: sidereon_core::astro::propagator::CovarianceEphemeris,
}

#[wasm_bindgen]
impl CovarianceEphemeris {
    #[wasm_bindgen(getter, js_name = timesS)]
    pub fn times_s(&self) -> Vec<f64> {
        self.times.clone()
    }

    #[wasm_bindgen(getter, js_name = positionKm)]
    pub fn position_km(&self) -> Vec<f64> {
        self.positions.clone()
    }

    #[wasm_bindgen(getter, js_name = velocityKmS)]
    pub fn velocity_km_s(&self) -> Vec<f64> {
        self.velocities.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn covariance(&self) -> Vec<f64> {
        self.covariances.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn frame(&self) -> String {
        frame_label(self.frame).to_string()
    }

    #[wasm_bindgen(getter, js_name = epochCount)]
    pub fn epoch_count(&self) -> usize {
        self.times.len()
    }

    #[wasm_bindgen(js_name = covarianceAt)]
    pub fn covariance_at(&self, epoch_tdb_seconds: f64) -> Result<Vec<f64>, JsValue> {
        self.inner
            .covariance_at(epoch_tdb_seconds)
            .map(|cov| covariance6_flat(&cov))
            .map_err(engine_error)
    }
}

/// Propagate an ECI Cartesian state and 6x6 covariance to requested epochs.
#[wasm_bindgen(js_name = propagateCovariance)]
pub fn propagate_covariance(request: JsValue) -> Result<CovarianceEphemeris, JsValue> {
    let req: CovariancePropagationRequest = serde_wasm_bindgen::from_value(request)
        .map_err(|e| type_error(&format!("invalid covariance propagation request: {e}")))?;
    let position = vec3_finite("positionKm", &req.position_km)?;
    let velocity = vec3_finite("velocityKmS", &req.velocity_km_s)?;
    let covariance = covariance6_from_flat("covariance", &req.covariance)?;

    let mut propagator = StatePropagator::new(
        req.epoch_s,
        position,
        velocity,
        force_model(req.force_model.as_deref(), req.mu_km3_s2)?,
        integrator(req.integrator.as_deref())?,
    )
    .with_options(integrator_options(&req)?);

    if let Some(drag) = &req.drag {
        let drag_params = drag.to_core()?;
        propagator = propagator.with_drag(drag_params);
    }

    let options = CovariancePropagationOptions {
        process_noise: process_noise(req.process_noise),
        output_frame: parse_frame(req.output_frame.as_deref())?,
    };
    let inner = propagator
        .propagate_covariance(
            LabeledCovariance6 {
                covariance,
                frame: parse_frame(req.covariance_frame.as_deref())?,
            },
            &req.times_s,
            &options,
        )
        .map_err(engine_error)?;

    let mut times = Vec::with_capacity(inner.len());
    let mut positions = Vec::with_capacity(inner.len() * 3);
    let mut velocities = Vec::with_capacity(inner.len() * 3);
    let mut covariances = Vec::with_capacity(inner.len() * 36);
    for node in inner.nodes() {
        times.push(node.state.epoch_tdb_seconds);
        positions.extend_from_slice(&node.state.position_array());
        velocities.extend_from_slice(&node.state.velocity_array());
        covariances.extend(covariance6_flat(&node.covariance));
    }

    Ok(CovarianceEphemeris {
        times,
        positions,
        velocities,
        covariances,
        frame: options.output_frame,
        inner,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransportSegmentInput {
    state_transition_matrix: Vec<f64>,
    dt_s: f64,
    q_rotation_epoch_s: f64,
    q_rotation_position_km: Vec<f64>,
    q_rotation_velocity_km_s: Vec<f64>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct TransportRequest {
    process_noise: Option<ProcessNoiseInput>,
}

/// Covariances returned by explicit STM transport.
#[wasm_bindgen]
pub struct CovarianceTransportResult {
    covariances: Vec<f64>,
    node_count: usize,
}

#[wasm_bindgen]
impl CovarianceTransportResult {
    #[wasm_bindgen(getter)]
    pub fn covariance(&self) -> Vec<f64> {
        self.covariances.clone()
    }

    #[wasm_bindgen(getter, js_name = nodeCount)]
    pub fn node_count(&self) -> usize {
        self.node_count
    }
}

fn stm_from_flat(values: &[f64]) -> Result<StateTransitionMatrix, JsValue> {
    if values.len() != 36 {
        return Err(type_error(&format!(
            "stateTransitionMatrix must have length 36 (flat row-major 6-by-6), got {}",
            values.len()
        )));
    }
    let mut matrix = [[0.0_f64; 6]; 6];
    for (i, row) in matrix.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = values[i * 6 + j];
        }
    }
    Ok(matrix)
}

/// Transport a 6x6 covariance through caller-supplied STM segments.
#[wasm_bindgen(js_name = transportCovariance)]
pub fn transport_covariance_js(
    covariance: &[f64],
    segments: JsValue,
    options: JsValue,
) -> Result<CovarianceTransportResult, JsValue> {
    let covariance0 = covariance6_from_flat("covariance", covariance)?;
    let segment_inputs: Vec<TransportSegmentInput> = serde_wasm_bindgen::from_value(segments)
        .map_err(|e| type_error(&format!("invalid covariance segments: {e}")))?;
    let options: TransportRequest = if options.is_undefined() || options.is_null() {
        TransportRequest::default()
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid covariance transport options: {e}")))?
    };

    let mut core_segments = Vec::with_capacity(segment_inputs.len());
    for (index, input) in segment_inputs.iter().enumerate() {
        let position = vec3_finite(
            &format!("segments[{index}].qRotationPositionKm"),
            &input.q_rotation_position_km,
        )?;
        let velocity = vec3_finite(
            &format!("segments[{index}].qRotationVelocityKmS"),
            &input.q_rotation_velocity_km_s,
        )?;
        core_segments.push(CovarianceSegment {
            stm: stm_from_flat(&input.state_transition_matrix)?,
            dt_seconds: input.dt_s,
            q_rotation_state: CartesianState::new(input.q_rotation_epoch_s, position, velocity),
        });
    }

    let covariances = transport_covariance(
        covariance0,
        &core_segments,
        process_noise(options.process_noise),
    )
    .map_err(engine_error)?;
    let mut flat = Vec::with_capacity(covariances.len() * 36);
    for covariance in &covariances {
        flat.extend(covariance6_flat(covariance));
    }
    Ok(CovarianceTransportResult {
        covariances: flat,
        node_count: covariances.len(),
    })
}
