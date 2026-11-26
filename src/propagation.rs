//! Numerical orbit propagation. Marshals one idiomatic JS request object into
//! the core `StatePropagator` and returns the sampled ephemeris. The
//! force-model composition, integrator/option defaults, and the integration
//! itself all live in core: the binding only translates the request's selectors
//! and marshals the returned states back out.

use serde::Deserialize;
use std::sync::Arc;
use wasm_bindgen::prelude::*;

use sidereon_core::astro::forces::ForceModel as CoreForceModel;
use sidereon_core::astro::frames::orientation::TdbEarthOrientationProvider;
use sidereon_core::astro::propagator::api::PropagationContext;
use sidereon_core::astro::propagator::StatePropagator as CoreStatePropagator;
use sidereon_core::astro::state::CartesianState;

use crate::domain_error::propagation_error;
use crate::earth_orientation::PropagationContext as Context;
use crate::error::{range_error, reject_unknown_keys, type_error};
use crate::force_model_input::{
    force_model_kind, force_model_requires_body_fixed_frame, integrator_kind, reject_drag_property,
    reject_force_model_property, reject_integrator_options_property, reject_object_property_keys,
    DragInput, ForceModelInput, IntegratorOptionsInput,
};
use crate::force_models::ForceModel;
use crate::space_weather::SpaceWeatherTable;

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
#[wasm_bindgen(js_name = propagateState, skip_typescript)]
pub fn propagate_state(
    #[wasm_bindgen(unchecked_param_type = "PropagateStateRequest")] request: JsValue,
    force_handles: Vec<ForceModel>,
    context: Option<Context>,
) -> Result<Ephemeris, JsValue> {
    let req = decode_request(request)?;
    let propagator = req.propagator(None)?;
    let default_context = default_ephemeris_context(&propagator);
    let context = context
        .as_ref()
        .map_or(&default_context, |context| &context.inner);
    sample_ephemeris(req.times_s, propagator, force_handles, context)
}

fn default_ephemeris_context(propagator: &CoreStatePropagator) -> PropagationContext {
    let context = PropagationContext::new();
    if force_model_requires_body_fixed_frame(&propagator.force_model) {
        context.with_body_fixed_frame_provider(Arc::new(TdbEarthOrientationProvider::new()))
    } else {
        context
    }
}

fn decode_request(request: JsValue) -> Result<PropagateRequest, JsValue> {
    reject_force_model_property(&request, "forceModel")?;
    serde_wasm_bindgen::from_value(request)
        .map_err(|error| type_error(&format!("invalid propagation request: {error}")))
}

fn core_forces(forces: Vec<ForceModel>) -> Vec<Arc<dyn CoreForceModel>> {
    forces.into_iter().map(|force| force.0).collect()
}

fn sample_ephemeris(
    times: Vec<f64>,
    propagator: CoreStatePropagator,
    additional_forces: Vec<ForceModel>,
    context: &PropagationContext,
) -> Result<Ephemeris, JsValue> {
    let states = propagator
        .ephemeris_with_additional_forces(&times, &core_forces(additional_forces), context)
        .map_err(propagation_error)?;
    Ok(Ephemeris::from_states(times, &states))
}

impl PropagateRequest {
    fn initial_state(&self) -> Result<CartesianState, JsValue> {
        Ok(CartesianState::new(
            self.epoch_s,
            fixed3(&self.position_km, "positionKm")?,
            fixed3(&self.velocity_km_s, "velocityKmS")?,
        ))
    }

    fn options(&self) -> Result<sidereon_core::astro::propagator::IntegratorOptions, JsValue> {
        let options = self.integrator_options.to_core();
        if options.initial_step <= 0.0 {
            return Err(range_error("initialStepS must be positive"));
        }
        Ok(options)
    }

    fn propagator(
        &self,
        table: Option<&SpaceWeatherTable>,
    ) -> Result<CoreStatePropagator, JsValue> {
        Ok(CoreStatePropagator {
            initial: self.initial_state()?,
            force_model: force_model_kind(self.force_model.as_ref(), self.mu_km3_s2)?,
            integrator: integrator_kind(self.integrator.as_deref())?,
            options: self.options()?,
            drag: self.drag.as_ref().map(DragInput::to_core).transpose()?,
            space_weather: table.map(SpaceWeatherTable::source),
        })
    }
}

/// Propagate using a per-epoch space weather table for the configured drag model.
#[wasm_bindgen(js_name = propagateStateWithSpaceWeather, skip_typescript)]
pub fn propagate_state_with_space_weather(
    #[wasm_bindgen(unchecked_param_type = "SampledPropagationRequest")] request: JsValue,
    table: &SpaceWeatherTable,
    context: &Context,
    force_handles: Vec<ForceModel>,
) -> Result<Ephemeris, JsValue> {
    reject_unknown_keys(
        &request,
        "propagation request",
        &[
            "epochS",
            "positionKm",
            "velocityKmS",
            "timesS",
            "forceModel",
            "integrator",
            "absTol",
            "relTol",
            "initialStepS",
            "minStepS",
            "maxStepS",
            "maxSteps",
            "muKm3S2",
            "drag",
            "additionalForces",
        ],
    )?;
    reject_drag_property(&request)?;
    let req = decode_request(request)?;
    let propagator = req.propagator(Some(table))?;
    sample_ephemeris(req.times_s, propagator, force_handles, &context.inner)
}

impl Ephemeris {
    fn from_states(times: Vec<f64>, states: &[CartesianState]) -> Self {
        let mut positions = Vec::with_capacity(states.len() * 3);
        let mut velocities = Vec::with_capacity(states.len() * 3);
        for state in states {
            positions.extend_from_slice(&state.position_array());
            velocities.extend_from_slice(&state.velocity_array());
        }

        Self {
            times,
            positions,
            velocities,
        }
    }
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

/// Continuous GCRF output from DP5(4) numerical propagation.
#[wasm_bindgen]
#[derive(Clone)]
pub struct DenseOutput(pub(crate) Arc<sidereon_core::astro::propagator::dense_output::DenseOutput>);

#[wasm_bindgen]
impl DenseOutput {
    /// `[start, end]` in TDB seconds past J2000, or `undefined` for empty output.
    #[wasm_bindgen(js_name = coverageTdbSeconds)]
    pub fn coverage_tdb_seconds(&self) -> Option<Vec<f64>> {
        self.0
            .coverage_tdb_seconds()
            .map(|(start, end)| vec![start, end])
    }
}

/// Final Cartesian state and optional continuous output of numerical propagation.
#[wasm_bindgen]
pub struct PropagationResult {
    final_state: CartesianState,
    dense: Option<DenseOutput>,
}

impl From<sidereon_core::astro::propagator::PropagationResult> for PropagationResult {
    fn from(result: sidereon_core::astro::propagator::PropagationResult) -> Self {
        Self {
            final_state: result.final_state,
            dense: result.dense.map(|output| DenseOutput(Arc::new(output))),
        }
    }
}

#[wasm_bindgen]
impl PropagationResult {
    /// Cartesian state at the requested TDB epoch.
    #[wasm_bindgen(getter, js_name = finalState)]
    pub fn final_state(&self) -> crate::orbit_state::CartesianState {
        crate::orbit_state::CartesianState(self.final_state)
    }

    /// Continuous output when denseOutput is enabled and the integrator is DP5(4).
    #[wasm_bindgen(getter)]
    pub fn dense(&self) -> Option<DenseOutput> {
        self.dense.clone()
    }
}

/// Numerical propagator with a built-in force configuration.
#[wasm_bindgen]
pub struct StatePropagator(CoreStatePropagator);

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct StatePropagatorOptions {
    force_model: Option<ForceModelInput>,
    integrator: Option<String>,
    integrator_options: Option<IntegratorOptionsInput>,
    drag: Option<DragInput>,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct CovarianceContextOptions {
    covariance_frame: Option<String>,
    output_frame: Option<String>,
    process_noise: Option<crate::covariance::ProcessNoiseInput>,
}

#[wasm_bindgen]
impl StatePropagator {
    /// Configure gravity, integration and optional drag for an initial Cartesian state.
    #[wasm_bindgen(constructor)]
    pub fn new(
        initial: &crate::orbit_state::CartesianState,
        #[wasm_bindgen(unchecked_optional_param_type = "StatePropagatorOptions | null")]
        input: JsValue,
    ) -> Result<Self, JsValue> {
        if !(input.is_null() || input.is_undefined()) {
            reject_unknown_keys(
                &input,
                "state propagator options",
                &["forceModel", "integrator", "integratorOptions", "drag"],
            )?;
            reject_force_model_property(&input, "forceModel")?;
            reject_drag_property(&input)?;
            reject_integrator_options_property(&input)?;
        }
        let options: Option<StatePropagatorOptions> = serde_wasm_bindgen::from_value(input)
            .map_err(|error| type_error(&format!("invalid state propagator options: {error}")))?;
        let options = options.unwrap_or_default();
        let mut propagator = CoreStatePropagator::new(
            initial.0.epoch_tdb_seconds,
            initial.0.position_array(),
            initial.0.velocity_array(),
            force_model_kind(options.force_model.as_ref(), None)?,
            integrator_kind(options.integrator.as_deref())?,
        )
        .with_options(options.integrator_options.unwrap_or_default().to_core());
        if let Some(drag) = options.drag {
            propagator = propagator.with_drag(drag.to_core()?);
        }
        Ok(Self(propagator))
    }

    /// Propagate to TDB seconds past J2000 using the configured forces and integrator.
    #[wasm_bindgen(js_name = propagateTo)]
    pub fn propagate_to(
        &self,
        epoch_tdb_seconds: f64,
        #[wasm_bindgen(unchecked_optional_param_type = "StatePropagationOptions | null")]
        options: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "ForceModel[]")] force_handles: Option<
            Vec<ForceModel>,
        >,
    ) -> Result<PropagationResult, JsValue> {
        self.propagate_to_in_context(
            epoch_tdb_seconds,
            options,
            force_handles.unwrap_or_default(),
            &PropagationContext::new(),
        )
    }

    /// Propagate to TDB seconds past J2000 with an explicit propagation context.
    #[wasm_bindgen(js_name = propagateToWithContext)]
    pub fn propagate_to_with_context(
        &self,
        epoch_tdb_seconds: f64,
        context: &Context,
        #[wasm_bindgen(unchecked_optional_param_type = "StatePropagationOptions | null")]
        options: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "ForceModel[]")] force_handles: Option<
            Vec<ForceModel>,
        >,
    ) -> Result<PropagationResult, JsValue> {
        self.propagate_to_in_context(
            epoch_tdb_seconds,
            options,
            force_handles.unwrap_or_default(),
            &context.inner,
        )
    }

    /// Set a per-epoch space-weather table for the configured drag.
    #[wasm_bindgen(js_name = setSpaceWeather)]
    pub fn set_space_weather(&mut self, table: &SpaceWeatherTable) -> Result<(), JsValue> {
        let source = table.source();
        let previous = self.0.space_weather.replace(source);
        if let Err(error) = self.0.force_model_with_drag() {
            self.0.space_weather = previous;
            return Err(propagation_error(error));
        }
        Ok(())
    }

    /// Evaluate Cartesian states at absolute TDB epochs.
    #[wasm_bindgen(js_name = ephemerisWithContext)]
    pub fn ephemeris_with_context(
        &self,
        epochs_tdb_seconds: &[f64],
        context: &Context,
        #[wasm_bindgen(unchecked_optional_param_type = "readonly ForceModel[] | null")]
        additional_forces: Option<Vec<ForceModel>>,
    ) -> Result<Vec<crate::orbit_state::CartesianState>, JsValue> {
        self.0
            .ephemeris_with_additional_forces(
                epochs_tdb_seconds,
                &core_forces(additional_forces.unwrap_or_default()),
                &context.inner,
            )
            .map(|states| {
                states
                    .into_iter()
                    .map(crate::orbit_state::CartesianState)
                    .collect()
            })
            .map_err(propagation_error)
    }

    /// Evaluate the flat row-major state transition matrix at an absolute TDB epoch.
    #[wasm_bindgen(js_name = stateTransitionMatrixToWithContext)]
    pub fn state_transition_matrix_to_with_context(
        &self,
        epoch_tdb_seconds: f64,
        context: &Context,
    ) -> Result<Vec<f64>, JsValue> {
        self.0
            .state_transition_matrix_to_with_context(epoch_tdb_seconds, &context.inner)
            .map(|matrix| matrix.into_iter().flatten().collect())
            .map_err(propagation_error)
    }

    /// Propagate a state covariance and optional process noise over absolute TDB epochs.
    #[wasm_bindgen(js_name = propagateCovarianceWithContext)]
    pub fn propagate_covariance_with_context(
        &self,
        covariance: &[f64],
        epochs_tdb_seconds: &[f64],
        context: &Context,
        #[wasm_bindgen(unchecked_optional_param_type = "CovarianceContextOptions | null")]
        options: JsValue,
    ) -> Result<crate::covariance::CovarianceEphemeris, JsValue> {
        use sidereon_core::astro::propagator::{CovariancePropagationOptions, LabeledCovariance6};
        if !options.is_null() && !options.is_undefined() {
            reject_unknown_keys(
                &options,
                "covariance options",
                &["covarianceFrame", "outputFrame", "processNoise"],
            )?;
            reject_object_property_keys(
                &options,
                "processNoise",
                "process noise",
                &["qRadialKm2S3", "qTransverseKm2S3", "qNormalKm2S3"],
            )?;
        }
        let input: Option<CovarianceContextOptions> = serde_wasm_bindgen::from_value(options)
            .map_err(|error| type_error(&format!("invalid covariance options: {error}")))?;
        let input = input.unwrap_or_default();
        let covariance = crate::marshal::covariance6_from_flat("covariance", covariance)?;
        let frame = crate::covariance::parse_frame(input.covariance_frame.as_deref())?;
        let mut options = CovariancePropagationOptions::default();
        options.process_noise = crate::covariance::process_noise(input.process_noise);
        options.output_frame = crate::covariance::parse_frame(input.output_frame.as_deref())?;
        self.0
            .propagate_covariance_with_context(
                LabeledCovariance6 { covariance, frame },
                epochs_tdb_seconds,
                &options,
                &context.inner,
            )
            .map(|result| {
                crate::covariance::CovarianceEphemeris::from_core(result, options.output_frame)
            })
            .map_err(propagation_error)
    }

    /// Propagate inertial state and covariance over a relative span in seconds.
    #[wasm_bindgen(js_name = propagateStateWithCovarianceWithContext, unchecked_return_type = "StateWithCovariance")]
    pub fn propagate_state_with_covariance_with_context(
        &self,
        covariance: &[f64],
        span_seconds: f64,
        context: &Context,
    ) -> Result<JsValue, JsValue> {
        let covariance = crate::marshal::covariance6_from_flat("covariance", covariance)?;
        let (state, covariance) = self
            .0
            .propagate_state_with_covariance_with_context(covariance, span_seconds, &context.inner)
            .map_err(propagation_error)?;
        crate::error::result_object(
            &[
                ("state", &crate::orbit_state::CartesianState(state).into()),
                (
                    "covariance",
                    &js_sys::Float64Array::from(
                        crate::marshal::covariance6_flat(&covariance).as_slice(),
                    )
                    .into(),
                ),
            ],
            "state and covariance",
        )
    }
}

impl StatePropagator {
    fn propagate_to_in_context(
        &self,
        epoch_tdb_seconds: f64,
        input: JsValue,
        additional_forces: Vec<ForceModel>,
        context: &PropagationContext,
    ) -> Result<PropagationResult, JsValue> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct StatePropagationOptionsInput {
            dense_output: Option<bool>,
        }

        if !input.is_null() && !input.is_undefined() {
            reject_unknown_keys(
                &input,
                "state propagation options",
                &["denseOutput", "additionalForces"],
            )?;
        }
        let options: Option<StatePropagationOptionsInput> = serde_wasm_bindgen::from_value(input)
            .map_err(|error| {
            type_error(&format!("invalid state propagation options: {error}"))
        })?;
        let mut integrator_options = self.0.options;
        if let Some(dense_output) = options.and_then(|options| options.dense_output) {
            integrator_options.dense_output = dense_output;
        }
        let propagator = CoreStatePropagator {
            initial: self.0.initial,
            force_model: self.0.force_model,
            integrator: self.0.integrator,
            options: integrator_options,
            drag: self.0.drag,
            space_weather: self.0.space_weather.clone(),
        };
        propagator
            .propagate_to_with_additional_forces(
                epoch_tdb_seconds,
                &core_forces(additional_forces),
                context,
            )
            .map(PropagationResult::from)
            .map_err(propagation_error)
    }
}

#[wasm_bindgen(typescript_custom_section)]
const PROPAGATION_TYPES: &str = r#"
declare const denseOutputHandle: unique symbol;

/** Continuous output is an opaque handle returned by PropagationResult. */
export interface DenseOutput {
    readonly [denseOutputHandle]: never;
}

interface SampledPropagationRequest extends IntegratorOptions {
    epochS: number;
    positionKm: Vec3;
    velocityKmS: Vec3;
    timesS: number[] | Float64Array;
    integrator?: "rk4" | "dp54";
    drag?: NumericalDragOptions;
    forceModel?: NumericalForceModel;
    muKm3S2?: number;
    /** Forces summed after the configured gravity and drag, in array order. */
    additionalForces?: readonly ForceModel[];
}

/** Cartesian trajectory input with optional gravity, drag, context and additional forces. */
export interface PropagateStateRequest extends SampledPropagationRequest {
    context?: PropagationContext | null;
    [key: string]: unknown;
    drag?: NumericalDragOptions & {
        [key: string]: unknown;
        spaceWeather?: { [key: string]: unknown };
    };
}

/**
 * Sample a numerical trajectory at the requested TDB epochs.
 * Extra request, drag and fixed space weather properties are ignored.
 */
export function propagateState(request: PropagateStateRequest): Ephemeris;

/** Sample a trajectory using table-driven atmospheric drag. */
export function propagateStateWithSpaceWeather(
    request: SampledPropagationRequest,
    table: SpaceWeatherTable,
    context: PropagationContext,
): Ephemeris;

/** Options for a propagation that returns a final state and optional dense output. */
export interface StatePropagationOptions {
    /** Retain dense integration output when using DP5(4). */
    denseOutput?: boolean;
    /** Forces summed after the configured gravity and drag, in array order. */
    additionalForces?: readonly ForceModel[];
}

/** Force and integration configuration for a numerical propagator. */
export interface StatePropagatorOptions {
    forceModel?: NumericalForceModel;
    integrator?: "rk4" | "dp54";
    integratorOptions?: IntegratorOptions;
    drag?: NumericalDragOptions;
}

/** Options for transporting an initial state covariance. */
export interface CovarianceContextOptions {
    covarianceFrame?: "inertial" | "rtn";
    outputFrame?: "inertial" | "rtn";
    processNoise?: { qRadialKm2S3?: number; qTransverseKm2S3?: number; qNormalKm2S3?: number };
}

/** A propagated inertial state and its row-major 6-by-6 covariance. */
export interface StateWithCovariance {
    state: CartesianState;
    covariance: Float64Array;
}
"#;
