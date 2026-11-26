//! Estimation of an initial state and selected force-model parameters.

use serde::Deserialize;
use serde_json::json;
use sidereon_core::orbit_determination as core;
use wasm_bindgen::prelude::*;

use crate::earth_orientation::{Iers2010EarthOrientationProvider, PropagationContext};
use crate::error::{reject_unknown_keys, type_error, typed_error, ut1_validity, validated_object};
use crate::force_model_input::{
    force_model_kind, integrator_kind, reject_drag_property, reject_force_model_property,
    reject_integrator_options_property, DragInput, ForceModelInput, IntegratorOptionsInput,
};
use crate::force_models::ForceModel;
use crate::forces::DragForce;
use crate::marshal::vec3;
use crate::orbit_state::CartesianState;
use crate::rinex_clock::ClockInstant;
use crate::sp3::{parse_sat, Sp3};
use crate::space_weather::SpaceWeatherTable;
use crate::variational::VariationalEpoch;

type JsResult<T> = Result<T, JsValue>;

fn row_major(matrix: &nalgebra::DMatrix<f64>) -> Vec<f64> {
    (0..matrix.nrows())
        .flat_map(|row| (0..matrix.ncols()).map(move |column| matrix[(row, column)]))
        .collect()
}

/// Observed position in the propagation's inertial frame.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct InertialPositionSample(core::InertialPositionSample);

#[wasm_bindgen]
impl InertialPositionSample {
    /// Compare the stored value.
    pub fn equals(&self, other: &InertialPositionSample) -> bool {
        self.0 == other.0
    }

    /// Build an inertial position observation.
    #[wasm_bindgen(constructor)]
    pub fn new(epoch: &ClockInstant, position_gcrf_m: &[f64]) -> JsResult<Self> {
        Ok(Self(core::InertialPositionSample {
            epoch: epoch.0,
            position_gcrf_m: vec3("positionGcrfM", position_gcrf_m)?,
        }))
    }

    /// Observation epoch, in any supported time scale.
    #[wasm_bindgen(getter)]
    pub fn epoch(&self) -> ClockInstant {
        ClockInstant(self.0.epoch)
    }

    /// GCRF position in metres.
    #[wasm_bindgen(getter, js_name = positionGcrfM)]
    pub fn position_gcrf_m(&self) -> Vec<f64> {
        self.0.position_gcrf_m.to_vec()
    }
}

/// An estimated force-model parameter and its initial value.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct EstimatedForceParameter(core::EstimatedForceParameter);

#[wasm_bindgen]
impl EstimatedForceParameter {
    /// Compare the stored value.
    pub fn equals(&self, other: &EstimatedForceParameter) -> bool {
        self.0 == other.0
    }

    /// Parameter name.
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.0.name.clone()
    }

    /// Value before estimation.
    #[wasm_bindgen(getter, js_name = initialValue)]
    pub fn initial_value(&self) -> f64 {
        self.0.initial_value
    }

    /// Estimate in the force model's parameter units.
    #[wasm_bindgen(getter)]
    pub fn value(&self) -> f64 {
        self.0.value
    }
}

/// Observed minus fitted position in the satellite's radial, along-track and cross-track frame.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct OrbitRtnResidual(core::OrbitRtnResidual);

#[wasm_bindgen]
impl OrbitRtnResidual {
    /// Compare the stored value.
    pub fn equals(&self, other: &OrbitRtnResidual) -> bool {
        self.0 == other.0
    }

    /// Observation epoch in TDB seconds from J2000.
    #[wasm_bindgen(getter, js_name = epochTdbJ2000S)]
    pub fn epoch_tdb_j2000_s(&self) -> f64 {
        self.0.epoch_tdb_j2000_s
    }

    /// TT seconds since the fitted initial epoch.
    #[wasm_bindgen(getter, js_name = elapsedTtS)]
    pub fn elapsed_tt_s(&self) -> f64 {
        self.0.elapsed_tt_s
    }

    /// Radial residual in metres.
    #[wasm_bindgen(getter, js_name = radialM)]
    pub fn radial_m(&self) -> f64 {
        self.0.radial_m
    }

    /// Along-track residual in metres.
    #[wasm_bindgen(getter, js_name = alongM)]
    pub fn along_m(&self) -> f64 {
        self.0.along_m
    }

    /// Cross-track residual in metres.
    #[wasm_bindgen(getter, js_name = crossM)]
    pub fn cross_m(&self) -> f64 {
        self.0.cross_m
    }

    /// Time coordinates for propagating the fitted state to this observation.
    #[wasm_bindgen(js_name = variationalEpoch)]
    pub fn variational_epoch(&self) -> VariationalEpoch {
        VariationalEpoch(self.0.variational_epoch())
    }
}

/// Estimated initial state, selected force parameters, covariance and post-fit residuals.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct OrbitParameterFit(core::OrbitParameterFit);

#[wasm_bindgen]
impl OrbitParameterFit {
    /// Compare the stored value.
    pub fn equals(&self, other: &OrbitParameterFit) -> bool {
        self.0 == other.0
    }

    /// Satellite identifier.
    #[wasm_bindgen(getter)]
    pub fn satellite(&self) -> String {
        self.0.satellite.to_string()
    }

    /// Estimated initial state at the first observation.
    #[wasm_bindgen(getter, js_name = initialState)]
    pub fn initial_state(&self) -> CartesianState {
        CartesianState(self.0.initial_state)
    }

    /// Estimated force parameters in covariance order.
    #[wasm_bindgen(getter, js_name = forceParameters)]
    pub fn force_parameters(&self) -> Vec<EstimatedForceParameter> {
        self.0
            .force_parameters
            .iter()
            .cloned()
            .map(EstimatedForceParameter)
            .collect()
    }

    /// Parameter names in covariance order.
    #[wasm_bindgen(getter, js_name = parameterNames)]
    pub fn parameter_names(&self) -> Vec<String> {
        self.0.parameter_names.clone()
    }

    /// Estimated or unbounded covariance.
    #[wasm_bindgen(getter)]
    pub fn covariance(&self) -> OrbitParameterCovariance {
        OrbitParameterCovariance(self.0.covariance.clone())
    }

    /// Residual at each observation epoch.
    #[wasm_bindgen(getter)]
    pub fn residuals(&self) -> Vec<OrbitRtnResidual> {
        self.0
            .residuals
            .iter()
            .copied()
            .map(OrbitRtnResidual)
            .collect()
    }

    /// RMS residuals and sample counts.
    #[wasm_bindgen(getter, js_name = residualStats, unchecked_return_type = "OrbitResidualStats")]
    pub fn residual_stats(&self) -> JsResult<JsValue> {
        let stats = crate::orbit_determination::OrbitResidualStatsJs::from(self.0.residual_stats);
        crate::error::to_plain_js(&stats, "orbit residual statistics")
    }

    /// Three-dimensional RMS residual of the initial guess in metres.
    #[wasm_bindgen(getter, js_name = seedRms3dM)]
    pub fn seed_rms_3d_m(&self) -> f64 {
        self.0.seed_rms_3d_m
    }

    /// Number of Gauss-Newton iterations.
    #[wasm_bindgen(getter)]
    pub fn iterations(&self) -> usize {
        self.0.iterations
    }

    /// Condition number of the column-scaled normal equations.
    #[wasm_bindgen(getter, js_name = conditionNumber)]
    pub fn condition_number(&self) -> f64 {
        self.0.condition_number
    }

    /// Estimated standard deviation for a named parameter, if available.
    pub fn sigma(&self, name: &str) -> Option<f64> {
        self.0.sigma(name)
    }

    /// Estimated correlation between two named parameters, if available.
    pub fn correlation(&self, first: &str, second: &str) -> Option<f64> {
        self.0.correlation(first, second)
    }

    /// Estimate of the named force parameter, if present.
    #[wasm_bindgen(js_name = forceParameter)]
    pub fn force_parameter(&self, name: &str) -> Option<EstimatedForceParameter> {
        self.0
            .force_parameter(name)
            .cloned()
            .map(EstimatedForceParameter)
    }
}

/// Scaled covariance of an orbit-parameter fit, or an unbounded estimate.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct OrbitParameterCovariance(core::OrbitParameterCovariance);

#[wasm_bindgen]
impl OrbitParameterCovariance {
    /// Compare the stored value.
    pub fn equals(&self, other: &OrbitParameterCovariance) -> bool {
        self.0 == other.0
    }

    /// Result of a fit with zero residual degrees of freedom.
    #[wasm_bindgen(js_name = UNBOUNDED)]
    pub fn unbounded() -> Self {
        Self(core::OrbitParameterCovariance::Unbounded)
    }

    /// Whether finite covariance data are available.
    #[wasm_bindgen(getter, js_name = isEstimated)]
    pub fn is_estimated(&self) -> bool {
        matches!(self.0, core::OrbitParameterCovariance::Estimated { .. })
    }
    /// Formal covariance scaled by the variance factor.
    #[wasm_bindgen(getter)]
    pub fn matrix(&self) -> Option<Vec<f64>> {
        match &self.0 {
            core::OrbitParameterCovariance::Estimated { matrix, .. } => Some(row_major(matrix)),
            core::OrbitParameterCovariance::Unbounded => None,
        }
    }
    /// Matrix of parameter correlation coefficients.
    #[wasm_bindgen(getter)]
    pub fn correlation(&self) -> Option<Vec<f64>> {
        match &self.0 {
            core::OrbitParameterCovariance::Estimated { correlation, .. } => {
                Some(row_major(correlation))
            }
            core::OrbitParameterCovariance::Unbounded => None,
        }
    }
    /// A-posteriori variance factor.
    #[wasm_bindgen(getter, js_name = varianceFactor)]
    pub fn variance_factor(&self) -> Option<f64> {
        match self.0 {
            core::OrbitParameterCovariance::Estimated {
                variance_factor, ..
            } => Some(variance_factor),
            core::OrbitParameterCovariance::Unbounded => None,
        }
    }

    /// Residual components minus estimated parameters.
    #[wasm_bindgen(getter, js_name = degreesOfFreedom)]
    pub fn degrees_of_freedom(&self) -> Option<usize> {
        match self.0 {
            core::OrbitParameterCovariance::Estimated {
                degrees_of_freedom, ..
            } => Some(degrees_of_freedom),
            core::OrbitParameterCovariance::Unbounded => None,
        }
    }
}

fn exact_float(value: f64) -> serde_json::Value {
    json!({"decimal":value.to_string(),"bitsHex":format!("{:016x}",value.to_bits())})
}

fn fit_error(error: core::OrbitParameterFitError) -> JsValue {
    use core::OrbitParameterFitError as E;
    let detail = match &error {
        E::Fit(source) => json!({"kind":"fit","cause":crate::orbit_fit_error::detail(source)}),
        E::InvalidParameter { name, reason } => {
            json!({"kind":"invalidParameter","name":name,"reason":reason})
        }
        E::UnobservableParameter { satellite, name } => {
            json!({"kind":"unobservableParameter","satellite":satellite.to_string(),"name":name})
        }
        E::SingularNormalEquations {
            satellite,
            condition_number,
            weakest,
        } => {
            json!({"kind":"singularNormalEquations","satellite":satellite.to_string(),"conditionNumber":exact_float(*condition_number),"weakest":weakest})
        }
        E::IllConditionedNormalEquations {
            satellite,
            condition_number,
            limit,
            weakest,
        } => {
            json!({"kind":"illConditionedNormalEquations","satellite":satellite.to_string(),"conditionNumber":exact_float(*condition_number),"limit":exact_float(*limit),"weakest":weakest})
        }
        E::NumericalFailure {
            satellite,
            quantity,
        } => {
            json!({"kind":"numericalFailure","satellite":satellite.to_string(),"quantity":quantity})
        }
        E::DidNotConverge {
            satellite,
            iterations,
            last_step_sigmas,
        } => {
            json!({"kind":"didNotConverge","satellite":satellite.to_string(),"iterations":iterations,"lastStepSigmas":exact_float(*last_step_sigmas)})
        }
    };
    typed_error("OrbitParameterFitError", &error.to_string(), detail)
}

/// Names of the six estimated Cartesian state components, in covariance order.
#[wasm_bindgen(js_name = STATE_PARAMETER_NAMES)]
pub fn state_parameter_names() -> Vec<String> {
    core::STATE_PARAMETER_NAMES.map(str::to_owned).to_vec()
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct FitOptionsInput {
    force_model: Option<ForceModelInput>,
    drag: Option<DragInput>,
    estimated_parameters: Option<Vec<String>>,
    integrator: Option<String>,
    integrator_options: Option<IntegratorOptionsInput>,
    observation_sigma_m: Option<f64>,
    max_iterations: Option<u32>,
    convergence_threshold: Option<f64>,
    max_condition_number: Option<f64>,
    min_ledger_samples: Option<u32>,
}

/// Force composition, integration and estimation controls.
#[wasm_bindgen]
#[derive(Default)]
pub struct OrbitParameterFitOptions {
    inner: core::OrbitParameterFitOptions,
    context: PropagationContext,
    space_weather: Option<SpaceWeatherTable>,
}

#[wasm_bindgen]
impl OrbitParameterFitOptions {
    /// Build from estimation controls or use the core defaults.
    #[wasm_bindgen(constructor)]
    pub fn new(
        #[wasm_bindgen(unchecked_optional_param_type = "OrbitParameterFitInput | null")]
        input: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "ForceModel[]")] force_handles: Option<
            Vec<ForceModel>,
        >,
    ) -> JsResult<Self> {
        let mut options = Self::default();
        options.inner.additional_forces = force_handles
            .unwrap_or_default()
            .into_iter()
            .map(|force| force.0)
            .collect();
        if input.is_null() || input.is_undefined() {
            return Ok(options);
        }
        reject_unknown_keys(
            &input,
            "orbit parameter fit options",
            &[
                "forceModel",
                "additionalForces",
                "drag",
                "estimatedParameters",
                "integrator",
                "integratorOptions",
                "observationSigmaM",
                "maxIterations",
                "convergenceThreshold",
                "maxConditionNumber",
                "minLedgerSamples",
            ],
        )?;
        reject_force_model_property(&input, "forceModel")?;
        reject_drag_property(&input)?;
        reject_integrator_options_property(&input)?;
        let input: FitOptionsInput = serde_wasm_bindgen::from_value(input).map_err(|error| {
            type_error(&format!("invalid orbit parameter fit options: {error}"))
        })?;
        let defaults = &mut options.inner;
        if let Some(force) = input.force_model.as_ref() {
            defaults.force_model = force_model_kind(Some(force), None)?;
        }
        defaults.drag = input.drag.as_ref().map(DragInput::to_core).transpose()?;
        defaults.estimated_parameters = input.estimated_parameters.unwrap_or_default();
        defaults.integrator = integrator_kind(input.integrator.as_deref())?;
        if let Some(integrator_options) = input.integrator_options {
            defaults.integrator_options = integrator_options.to_core();
        }
        defaults.observation_sigma_m = input
            .observation_sigma_m
            .unwrap_or(defaults.observation_sigma_m);
        defaults.max_iterations = input
            .max_iterations
            .map_or(defaults.max_iterations, |count| count as usize);
        defaults.convergence_threshold = input
            .convergence_threshold
            .unwrap_or(defaults.convergence_threshold);
        defaults.max_condition_number = input
            .max_condition_number
            .unwrap_or(defaults.max_condition_number);
        defaults.min_ledger_samples = input
            .min_ledger_samples
            .map_or(defaults.min_ledger_samples, |count| count as usize);
        Ok(options)
    }

    /// Supply a drag model.
    #[wasm_bindgen(js_name = setDrag)]
    pub fn set_drag(&mut self, drag: &DragForce) -> JsResult<()> {
        self.inner.drag = Some(drag.drag_parameters()?);
        Ok(())
    }

    /// Supply a space-weather table for the configured drag model.
    #[wasm_bindgen(js_name = setSpaceWeather)]
    pub fn set_space_weather(&mut self, table: &SpaceWeatherTable) {
        self.inner.space_weather = Some(table.source());
        self.space_weather = Some(table.clone());
    }

    /// Set the DE and Earth orientation context for the force models.
    #[wasm_bindgen(js_name = setPropagationContext)]
    pub fn set_propagation_context(&mut self, context: &PropagationContext) {
        self.context = context.clone();
        self.inner.propagation_context = context.inner.clone();
    }

    /// Build the fit's configured gravity and other built-in forces.
    #[wasm_bindgen(getter, js_name = forceModel)]
    pub fn force_model(&self) -> JsResult<ForceModel> {
        self.inner
            .force_model
            .build()
            .map(|model| ForceModel(model.into()))
            .map_err(crate::domain_error::propagation_error)
    }

    /// Configured drag force if supplied.
    #[wasm_bindgen(getter)]
    pub fn drag(&self) -> Option<DragForce> {
        self.inner.drag.map(DragForce::from_parameters)
    }

    /// Configured weather table if supplied.
    #[wasm_bindgen(getter, js_name = spaceWeather)]
    pub fn space_weather(&self) -> Option<SpaceWeatherTable> {
        self.space_weather.clone()
    }

    /// Additional force models in evaluation order.
    #[wasm_bindgen(getter, js_name = additionalForces)]
    pub fn additional_forces(&self) -> Vec<ForceModel> {
        self.inner
            .additional_forces
            .iter()
            .cloned()
            .map(ForceModel)
            .collect()
    }

    /// Estimated force parameter names in covariance order.
    #[wasm_bindgen(getter, js_name = estimatedParameters)]
    pub fn estimated_parameters(&self) -> Vec<String> {
        self.inner.estimated_parameters.clone()
    }

    /// Propagation context shared with the fit.
    #[wasm_bindgen(getter, js_name = propagationContext)]
    pub fn propagation_context(&self) -> PropagationContext {
        self.context.clone()
    }

    /// Integrator used for the trajectory and derivatives.
    #[wasm_bindgen(getter, unchecked_return_type = "'rk4' | 'dp54'")]
    pub fn integrator(&self) -> String {
        match self.inner.integrator {
            sidereon_core::astro::propagator::IntegratorKind::Rk4 => "rk4",
            sidereon_core::astro::propagator::IntegratorKind::Dp54 => "dp54",
        }
        .to_owned()
    }

    /// Integration step and tolerance controls.
    #[wasm_bindgen(getter, js_name = integratorOptions, unchecked_return_type = "Required<IntegratorOptions>")]
    pub fn integrator_options(&self) -> JsResult<JsValue> {
        let options = &self.inner.integrator_options;
        crate::error::to_plain_js(
            &json!({
                "absTol": options.abs_tol, "relTol": options.rel_tol,
                "initialStepS": options.initial_step, "minStepS": options.min_step,
                "maxStepS": options.max_step, "maxSteps": options.max_steps,
            }),
            "integrator options",
        )
    }

    /// A-priori observation standard deviation in metres.
    #[wasm_bindgen(getter, js_name = observationSigmaM)]
    pub fn observation_sigma_m(&self) -> f64 {
        self.inner.observation_sigma_m
    }

    /// Maximum number of fit iterations.
    #[wasm_bindgen(getter, js_name = maxIterations)]
    pub fn max_iterations(&self) -> usize {
        self.inner.max_iterations
    }

    /// Convergence threshold in formal standard deviations.
    #[wasm_bindgen(getter, js_name = convergenceThreshold)]
    pub fn convergence_threshold(&self) -> f64 {
        self.inner.convergence_threshold
    }

    /// Maximum condition number of the scaled normal equations.
    #[wasm_bindgen(getter, js_name = maxConditionNumber)]
    pub fn max_condition_number(&self) -> f64 {
        self.inner.max_condition_number
    }

    /// Number of residuals needed to clear the low-sample-count flag.
    #[wasm_bindgen(getter, js_name = minLedgerSamples)]
    pub fn min_ledger_samples(&self) -> usize {
        self.inner.min_ledger_samples
    }
}

/// Fit initial state and force parameters to inertial observations.
/// The sample handles and the optional seed are consumed.
#[wasm_bindgen(js_name = fitInertialOrbitParameters)]
pub fn fit_inertial_orbit_parameters(
    samples: Vec<InertialPositionSample>,
    satellite: &str,
    initial_state: Option<CartesianState>,
    options: &OrbitParameterFitOptions,
) -> JsResult<OrbitParameterFit> {
    let samples: Vec<_> = samples.into_iter().map(|sample| sample.0).collect();
    core::fit_inertial_orbit_parameters(
        &samples,
        parse_sat(satellite)?,
        initial_state.map(|state| state.0),
        &options.inner,
    )
    .map(OrbitParameterFit)
    .map_err(fit_error)
}

/// Fit initial state and force parameters to inertial observations under a UT1 coverage policy.
/// The sample handles and the optional seed are consumed.
#[wasm_bindgen(js_name = fitInertialOrbitParametersWithValidity, unchecked_return_type = "Ut1Validated<OrbitParameterFit>")]
pub fn fit_inertial_orbit_parameters_with_validity(
    samples: Vec<InertialPositionSample>,
    satellite: &str,
    initial_state: Option<CartesianState>,
    options: &OrbitParameterFitOptions,
    #[wasm_bindgen(unchecked_param_type = "Ut1Validity")] mode: String,
) -> JsResult<JsValue> {
    let samples: Vec<_> = samples.into_iter().map(|sample| sample.0).collect();
    let result = core::fit_inertial_orbit_parameters_with_validity(
        &samples,
        parse_sat(satellite)?,
        initial_state.map(|state| state.0),
        &options.inner,
        ut1_validity(Some(mode))?,
    )
    .map_err(fit_error)?;
    validated_object(&OrbitParameterFit(result.value).into(), result.degraded)
}

/// Fit initial state and force parameters to Earth-fixed SP3 positions.
/// The optional seed is consumed.
#[wasm_bindgen(js_name = fitSp3EcefOrbitParameters)]
pub fn fit_sp3_ecef_orbit_parameters(
    product: &Sp3,
    satellite: &str,
    orientation_provider: &Iers2010EarthOrientationProvider,
    initial_state: Option<CartesianState>,
    options: &OrbitParameterFitOptions,
) -> JsResult<OrbitParameterFit> {
    core::fit_sp3_ecef_orbit_parameters(
        &product.inner,
        parse_sat(satellite)?,
        orientation_provider.0.as_ref(),
        initial_state.map(|state| state.0),
        &options.inner,
    )
    .map(OrbitParameterFit)
    .map_err(fit_error)
}

/// Fit initial state and force parameters to Earth-fixed SP3 positions under a UT1 coverage policy.
/// The optional seed is consumed.
#[wasm_bindgen(js_name = fitSp3EcefOrbitParametersWithValidity, unchecked_return_type = "Ut1Validated<OrbitParameterFit>")]
pub fn fit_sp3_ecef_orbit_parameters_with_validity(
    product: &Sp3,
    satellite: &str,
    orientation_provider: &Iers2010EarthOrientationProvider,
    initial_state: Option<CartesianState>,
    options: &OrbitParameterFitOptions,
    #[wasm_bindgen(unchecked_param_type = "Ut1Validity")] mode: String,
) -> JsResult<JsValue> {
    let result = core::fit_sp3_ecef_orbit_parameters_with_validity(
        &product.inner,
        parse_sat(satellite)?,
        orientation_provider.0.as_ref(),
        initial_state.map(|state| state.0),
        &options.inner,
        ut1_validity(Some(mode))?,
    )
    .map_err(fit_error)?;
    validated_object(&OrbitParameterFit(result.value).into(), result.degraded)
}

#[wasm_bindgen(typescript_custom_section)]
const PARAMETER_FIT_TYPES: &str = r#"
/** Controls for a force-parameter fit. */
export interface OrbitParameterFitInput {
    forceModel?: NumericalForceModel;
    additionalForces?: readonly ForceModel[];
    drag?: NumericalDragOptions;
    estimatedParameters?: string[];
    integrator?: "rk4" | "dp54";
    integratorOptions?: IntegratorOptions;
    observationSigmaM?: number;
    maxIterations?: number;
    convergenceThreshold?: number;
    maxConditionNumber?: number;
    minLedgerSamples?: number;
}

/** Binary64 diagnostic value, including infinities. */
export interface OrbitParameterFitDiagnosticNumber { decimal: string; bitsHex: string }

/** Details returned by force-parameter fitting. */
export type OrbitParameterFitErrorDetail =
  | { kind: "fit"; cause: OrbitFitErrorDetail }
  | { kind: "invalidParameter"; name: string; reason: string }
  | { kind: "unobservableParameter"; satellite: string; name: string }
  | { kind: "singularNormalEquations"; satellite: string; conditionNumber: OrbitParameterFitDiagnosticNumber; weakest: [string, number][] }
  | { kind: "illConditionedNormalEquations"; satellite: string; conditionNumber: OrbitParameterFitDiagnosticNumber; limit: OrbitParameterFitDiagnosticNumber; weakest: [string, number][] }
  | { kind: "numericalFailure"; satellite: string; quantity: string }
  | { kind: "didNotConverge"; satellite: string; iterations: number; lastStepSigmas: OrbitParameterFitDiagnosticNumber };

/** Error thrown by force-parameter fitting. */
export interface OrbitParameterFitError extends Error {
    readonly name: "OrbitParameterFitError";
    readonly kind: OrbitParameterFitErrorDetail["kind"];
    readonly detail: OrbitParameterFitErrorDetail;
}
"#;
