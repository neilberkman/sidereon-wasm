//! Concrete force models and their estimable parameters.

use std::sync::Arc;

use sidereon_core::astro::forces::{self, ForceModel as CoreForceModel};
use sidereon_core::astro::propagator::PropagationContext as CoreContext;
use wasm_bindgen::prelude::*;

use crate::earth_orientation::PropagationContext;
use crate::error::type_error;

pub(crate) fn with_context<T>(
    context: Option<&PropagationContext>,
    evaluate: impl FnOnce(&CoreContext) -> T,
) -> T {
    match context {
        Some(context) => evaluate(&context.inner),
        None => evaluate(&CoreContext::new()),
    }
}

pub(crate) fn flatten_parameter_partials(
    partials: impl IntoIterator<Item = nalgebra::Vector3<f64>>,
) -> Vec<f64> {
    partials.into_iter().flat_map(<[f64; 3]>::from).collect()
}

/// One estimable parameter and its value in the force model's units.
#[wasm_bindgen]
pub struct ForceParameter(pub(crate) forces::ForceParameter);

#[wasm_bindgen]
impl ForceParameter {
    /// Pair a parameter name and value.
    #[wasm_bindgen(constructor)]
    pub fn new(name: String, value: f64) -> Self {
        Self(forces::ForceParameter::new(name, value))
    }

    /// Parameter name.
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.0.name.clone()
    }

    /// Value in the model's parameter units.
    #[wasm_bindgen(getter)]
    pub fn value(&self) -> f64 {
        self.0.value
    }

    /// Compare the name and value.
    pub fn equals(&self, other: &ForceParameter) -> bool {
        self.0 == other.0
    }
}

/// Acceleration derivatives in the inertial frame.
#[wasm_bindgen]
pub struct AccelerationPartials(pub(crate) forces::AccelerationPartials);

fn matrix_flat(matrix: &nalgebra::Matrix3<f64>) -> Vec<f64> {
    crate::marshal::mat3_flat(&std::array::from_fn(|row| {
        std::array::from_fn(|column| matrix[(row, column)])
    }))
}

#[wasm_bindgen]
impl AccelerationPartials {
    /// Flat row-major position derivative in inverse square seconds.
    #[wasm_bindgen(getter, js_name = wrtPosition)]
    pub fn wrt_position(&self) -> Vec<f64> {
        matrix_flat(&self.0.wrt_position)
    }

    /// Flat row-major velocity derivative in inverse seconds.
    #[wasm_bindgen(getter, js_name = wrtVelocity)]
    pub fn wrt_velocity(&self) -> Vec<f64> {
        matrix_flat(&self.0.wrt_velocity)
    }

    /// Two zero derivative matrices.
    pub fn zeros() -> Self {
        Self(forces::AccelerationPartials::zeros())
    }

    /// Construct from a flat row-major position derivative and zero velocity derivative.
    #[wasm_bindgen(js_name = positionOnly)]
    pub fn position_only(wrt_position: &[f64]) -> Result<Self, JsValue> {
        let values: &[f64; 9] = wrt_position
            .try_into()
            .map_err(|_| type_error("wrtPosition must have length 9"))?;
        Ok(Self(forces::AccelerationPartials::position_only(
            nalgebra::Matrix3::from_row_slice(values),
        )))
    }

    /// Compare both derivative matrices.
    pub fn equals(&self, other: &AccelerationPartials) -> bool {
        self.0 == other.0
    }
}

/// A force that can be shared by propagators and estimators.
#[wasm_bindgen]
#[derive(Clone)]
pub struct ForceModel(pub(crate) Arc<dyn CoreForceModel>);

macro_rules! force_methods {
    ($model:ident, $member:tt, $acceleration:ident, $js_acceleration:ident) => {
        #[wasm_bindgen::prelude::wasm_bindgen]
        impl $model {
            /// Estimable parameters in evaluation order.
            pub fn parameters(&self) -> Vec<$crate::force_models::ForceParameter> {
                sidereon_core::astro::forces::ForceModel::parameters(&self.$member)
                    .into_iter()
                    .map($crate::force_models::ForceParameter)
                    .collect()
            }

            /// Number of estimable parameters.
            #[wasm_bindgen(js_name = parameterCount)]
            pub fn parameter_count(&self) -> usize {
                sidereon_core::astro::forces::ForceModel::parameter_count(&self.$member)
            }

            /// Acceleration in kilometres per square second.
            #[wasm_bindgen(js_name = $js_acceleration)]
            pub fn $acceleration(
                &self,
                state: &$crate::orbit_state::CartesianState,
                context: &$crate::earth_orientation::PropagationContext,
            ) -> Result<Vec<f64>, wasm_bindgen::JsValue> {
                $crate::force_models::with_context(Some(context), |ctx| {
                    sidereon_core::astro::forces::ForceModel::acceleration(
                        &self.$member,
                        &state.0,
                        ctx,
                    )
                })
                .map(|value| value.as_slice().to_vec())
                .map_err($crate::domain_error::propagation_error)
            }

            /// Acceleration with replacement parameter values in model order.
            #[wasm_bindgen(js_name = accelerationWithParameters)]
            pub fn acceleration_with_parameters(
                &self,
                parameters: &[f64],
                state: &$crate::orbit_state::CartesianState,
                context: &$crate::earth_orientation::PropagationContext,
            ) -> Result<Vec<f64>, wasm_bindgen::JsValue> {
                $crate::force_models::with_context(Some(context), |ctx| {
                    sidereon_core::astro::forces::ForceModel::acceleration_with_parameters(
                        &self.$member,
                        parameters,
                        &state.0,
                        ctx,
                    )
                })
                .map(|value| value.as_slice().to_vec())
                .map_err($crate::domain_error::propagation_error)
            }

            /// Derivatives with respect to the position and velocity.
            #[wasm_bindgen(js_name = accelerationPartials)]
            pub fn acceleration_partials(
                &self,
                parameters: &[f64],
                state: &$crate::orbit_state::CartesianState,
                context: &$crate::earth_orientation::PropagationContext,
            ) -> Result<$crate::force_models::AccelerationPartials, wasm_bindgen::JsValue> {
                $crate::force_models::with_context(Some(context), |ctx| {
                    sidereon_core::astro::forces::ForceModel::acceleration_partials(
                        &self.$member,
                        parameters,
                        &state.0,
                        ctx,
                    )
                })
                .map($crate::force_models::AccelerationPartials)
                .map_err($crate::domain_error::propagation_error)
            }

            /// One acceleration derivative per parameter as flat consecutive three-vectors.
            #[wasm_bindgen(js_name = parameterPartials)]
            pub fn parameter_partials(
                &self,
                parameters: &[f64],
                state: &$crate::orbit_state::CartesianState,
                context: &$crate::earth_orientation::PropagationContext,
            ) -> Result<Vec<f64>, wasm_bindgen::JsValue> {
                $crate::force_models::with_context(Some(context), |ctx| {
                    sidereon_core::astro::forces::ForceModel::parameter_partials(
                        &self.$member,
                        parameters,
                        &state.0,
                        ctx,
                    )
                })
                .map($crate::force_models::flatten_parameter_partials)
                .map_err($crate::domain_error::propagation_error)
            }
        }
    };
}

pub(crate) use force_methods;

force_methods!(ForceModel, 0, acceleration, acceleration);

#[wasm_bindgen]
impl ForceModel {
    /// Build point-mass gravity using the given gravitational parameter.
    #[wasm_bindgen(js_name = twoBody)]
    pub fn two_body(mu_km3_s2: Option<f64>) -> Self {
        let mut model = forces::TwoBodyGravity::default();
        if let Some(mu) = mu_km3_s2 {
            model.mu = mu;
        }
        Self(Arc::new(model))
    }

    /// Build the Earth J2 perturbation using the core constants.
    pub fn j2() -> Self {
        Self(Arc::new(forces::J2Gravity::default()))
    }

    /// Return another handle for the same force configuration.
    #[wasm_bindgen(js_name = clone)]
    pub fn clone_handle(&self) -> Self {
        self.clone()
    }
}

/// Ordered sum of concrete forces.
#[wasm_bindgen]
pub struct CompositeForceModel(Arc<forces::CompositeForceModel>);

#[wasm_bindgen]
impl CompositeForceModel {
    /// Combine force handles in evaluation order. The supplied handles are consumed.
    #[wasm_bindgen(constructor)]
    pub fn new(models: Vec<ForceModel>) -> Self {
        let mut composite = forces::CompositeForceModel::new();
        for model in models {
            composite.add(Box::new(model.0));
        }
        Self(Arc::new(composite))
    }

    /// Return a shared handle usable for propagation and estimation.
    #[wasm_bindgen(js_name = asForceModel)]
    pub fn as_force_model(&self) -> ForceModel {
        ForceModel(self.0.clone())
    }
}

force_methods!(CompositeForceModel, 0, acceleration, acceleration);
