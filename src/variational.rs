//! Numerical propagation of state and force-parameter sensitivities.

use sidereon_core::astro::propagator::{
    propagate_variational as integrate_variational, VariationalEpoch as CoreEpoch,
    VariationalEquations as CoreEquations, VariationalState as CoreState,
};
use wasm_bindgen::prelude::*;

use crate::domain_error::propagation_error;
use crate::earth_orientation::PropagationContext;
use crate::error::{index_arg, type_error};
use crate::force_model_input::{
    integrator_kind, reject_integrator_option_keys, IntegratorOptionsInput,
};
use crate::force_models::ForceModel;
use crate::orbit_state::CartesianState;

/// Absolute TDB and elapsed TT values for one requested epoch.
#[wasm_bindgen]
pub struct VariationalEpoch(pub(crate) CoreEpoch);

#[wasm_bindgen]
impl VariationalEpoch {
    /// Specify the TDB epoch and the TT offset in seconds.
    #[wasm_bindgen(constructor)]
    pub fn new(tdb_j2000_s: f64, elapsed_tt_s: f64) -> Self {
        Self(CoreEpoch {
            tdb_j2000_s,
            elapsed_tt_s,
        })
    }

    /// Coordinate seconds from J2000 in TDB.
    #[wasm_bindgen(getter, js_name = tdbJ2000S)]
    pub fn tdb_j2000_s(&self) -> f64 {
        self.0.tdb_j2000_s
    }

    /// Seconds elapsed since the initial epoch in TT.
    #[wasm_bindgen(getter, js_name = elapsedTtS)]
    pub fn elapsed_tt_s(&self) -> f64 {
        self.0.elapsed_tt_s
    }

    /// Derive elapsed TT given the initial and target TDB epochs.
    #[wasm_bindgen(js_name = fromTdbSeconds)]
    pub fn from_tdb_seconds(initial_tdb_j2000_s: f64, tdb_j2000_s: f64) -> Self {
        Self(CoreEpoch::from_tdb_seconds(
            initial_tdb_j2000_s,
            tdb_j2000_s,
        ))
    }

    /// Compare both time coordinates.
    pub fn equals(&self, other: &VariationalEpoch) -> bool {
        self.0 == other.0
    }
}

/// A force model, parameter values and selected sensitivity columns.
#[wasm_bindgen]
pub struct VariationalEquations {
    force: ForceModel,
    parameters: Vec<f64>,
    sensitivity_parameters: Vec<usize>,
}

#[wasm_bindgen]
impl VariationalEquations {
    /// Use replacement parameter values and select sensitivity columns by index.
    #[wasm_bindgen(constructor)]
    pub fn new(
        force: &ForceModel,
        parameters: &[f64],
        sensitivity_parameters: &[f64],
    ) -> Result<Self, JsValue> {
        Ok(Self {
            force: force.clone(),
            parameters: parameters.to_vec(),
            sensitivity_parameters: sensitivity_parameters
                .iter()
                .map(|&index| index_arg(index, "sensitivityParameters entry"))
                .collect::<Result<_, _>>()?,
        })
    }

    /// Shared handle for the force model.
    #[wasm_bindgen(getter)]
    pub fn force(&self) -> ForceModel {
        self.force.clone()
    }

    /// Replacement parameter values in model order.
    #[wasm_bindgen(getter)]
    pub fn parameters(&self) -> Vec<f64> {
        self.parameters.clone()
    }

    /// Parameter indices in sensitivity column order.
    #[wasm_bindgen(getter, js_name = sensitivityParameters)]
    pub fn sensitivity_parameters(&self) -> Vec<u32> {
        self.sensitivity_parameters
            .iter()
            .map(|&index| index as u32)
            .collect()
    }
}

/// Propagated state with its state transition matrix and sensitivity matrix.
#[wasm_bindgen]
pub struct VariationalState(CoreState);

#[wasm_bindgen]
impl VariationalState {
    /// Cartesian state at the requested epoch.
    #[wasm_bindgen(getter)]
    pub fn state(&self) -> CartesianState {
        CartesianState(self.0.state)
    }

    /// Six-by-six row-major derivative with respect to the initial state.
    #[wasm_bindgen(getter, js_name = stateTransition)]
    pub fn state_transition(&self) -> Vec<f64> {
        let matrix = &self.0.state_transition;
        (0..6)
            .flat_map(|row| (0..6).map(move |column| matrix[(row, column)]))
            .collect()
    }

    /// Row-major matrix with six rows and one column per selected force parameter.
    #[wasm_bindgen(getter)]
    pub fn sensitivity(&self) -> Vec<f64> {
        let matrix = &self.0.sensitivity;
        (0..6)
            .flat_map(|row| (0..matrix.ncols()).map(move |column| matrix[(row, column)]))
            .collect()
    }

    /// Compare the state and both matrices.
    pub fn equals(&self, other: &VariationalState) -> bool {
        self.0 == other.0
    }
}

/// Integrate the state, state transition matrices and sensitivity matrices at the requested epochs.
/// The epoch handles are consumed.
#[wasm_bindgen(js_name = propagateVariational)]
pub fn propagate_variational(
    initial: &CartesianState,
    epochs: Vec<VariationalEpoch>,
    equations: &VariationalEquations,
    #[wasm_bindgen(unchecked_param_type = "'rk4' | 'dp54'")] integrator: &str,
    #[wasm_bindgen(unchecked_param_type = "IntegratorOptions | null | undefined")] options: JsValue,
    context: &PropagationContext,
) -> Result<Vec<VariationalState>, JsValue> {
    let epochs: Vec<_> = epochs.into_iter().map(|epoch| epoch.0).collect();
    let equations = CoreEquations {
        force: equations.force.0.as_ref(),
        parameters: &equations.parameters,
        sensitivity_parameters: &equations.sensitivity_parameters,
    };
    let options = decode_options(options)?;
    integrate_variational(
        initial.0,
        &epochs,
        &equations,
        integrator_kind(Some(integrator))?,
        &options,
        &context.inner,
    )
    .map(|states| states.into_iter().map(VariationalState).collect())
    .map_err(propagation_error)
}

fn decode_options(
    options: JsValue,
) -> Result<sidereon_core::astro::propagator::IntegratorOptions, JsValue> {
    reject_integrator_option_keys(&options)?;
    let options: Option<IntegratorOptionsInput> = serde_wasm_bindgen::from_value(options)
        .map_err(|error| type_error(&format!("invalid integrator options: {error}")))?;
    Ok(options.unwrap_or_default().to_core())
}

#[wasm_bindgen(typescript_custom_section)]
const VARIATIONAL_TYPES: &str = r#"
/** Optional numerical integration overrides. */
export interface IntegratorOptions {
    /** Absolute error tolerance. */
    absTol?: number;
    /** Relative error tolerance. */
    relTol?: number;
    /** First step, in seconds. */
    initialStepS?: number;
    /** Shortest step, in seconds. */
    minStepS?: number;
    /** Longest step, in seconds. */
    maxStepS?: number;
    /** Limit on the number of steps. */
    maxSteps?: number;
}
"#;
