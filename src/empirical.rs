//! Empirical acceleration terms in the satellite's radial, along-track and cross-track frame.

use std::sync::Arc;

use sidereon_core::astro::forces;
use wasm_bindgen::prelude::*;

use crate::domain_error::propagation_error;
use crate::error::{index_arg, range_error, type_error};
use crate::force_models::{force_methods, ForceModel};

/// An axis of the orbital RTN frame.
#[wasm_bindgen]
pub enum RtnAxis {
    /// Along the position vector.
    Radial,
    /// Perpendicular to radial within the orbit plane.
    AlongTrack,
    /// Along the orbital angular momentum.
    CrossTrack,
}

/// Dependence of an empirical term on argument of latitude.
#[wasm_bindgen]
pub enum EmpiricalHarmonic {
    /// Constant.
    Constant,
    /// Proportional to the cosine.
    Cosine,
    /// Proportional to the sine.
    Sine,
}

/// All RTN axes in parameter order.
#[wasm_bindgen(js_name = rtnAxisAll, unchecked_return_type = "RtnAxis[]")]
pub fn rtn_axis_all() -> JsValue {
    forces::RtnAxis::ALL
        .into_iter()
        .map(|axis| JsValue::from(axis as u32))
        .collect::<js_sys::Array>()
        .into()
}

/// All empirical harmonic types in parameter order.
#[wasm_bindgen(js_name = empiricalHarmonicAll, unchecked_return_type = "EmpiricalHarmonic[]")]
pub fn empirical_harmonic_all() -> JsValue {
    forces::EmpiricalHarmonic::ALL
        .into_iter()
        .map(|harmonic| JsValue::from(harmonic as u32))
        .collect::<js_sys::Array>()
        .into()
}

fn axis(value: f64) -> Result<forces::RtnAxis, JsValue> {
    forces::RtnAxis::ALL
        .get(index_arg(value, "axis")?)
        .copied()
        .ok_or_else(|| range_error("axis must be an RtnAxis value"))
}

fn harmonic(value: f64) -> Result<forces::EmpiricalHarmonic, JsValue> {
    forces::EmpiricalHarmonic::ALL
        .get(index_arg(value, "harmonic")?)
        .copied()
        .ok_or_else(|| range_error("harmonic must be an EmpiricalHarmonic value"))
}

/// Constant and periodic acceleration terms along the three orbital axes.
#[wasm_bindgen]
#[derive(Default)]
pub struct EmpiricalRtnAcceleration(forces::EmpiricalRtnAcceleration);

#[wasm_bindgen]
impl EmpiricalRtnAcceleration {
    /// Build an empty empirical acceleration.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable three constant terms with zero values.
    #[wasm_bindgen(js_name = constantTerms)]
    pub fn constant_terms() -> Self {
        Self(forces::EmpiricalRtnAcceleration::constant_terms())
    }

    /// Enable all nine terms with zero values.
    #[wasm_bindgen(js_name = allTerms)]
    pub fn all_terms() -> Self {
        Self(forces::EmpiricalRtnAcceleration::all_terms())
    }

    /// Enable the specified terms with zero values.
    #[wasm_bindgen(js_name = withZeroTerms)]
    pub fn with_zero_terms(
        #[wasm_bindgen(unchecked_param_type = "[RtnAxis, EmpiricalHarmonic][]")] terms: JsValue,
    ) -> Result<Self, JsValue> {
        let terms: Vec<(f64, f64)> = serde_wasm_bindgen::from_value(terms)
            .map_err(|error| type_error(&format!("invalid empirical terms: {error}")))?;
        let terms = terms
            .into_iter()
            .map(|(a, h)| Ok((axis(a)?, harmonic(h)?)))
            .collect::<Result<Vec<_>, JsValue>>()?;
        Ok(Self(forces::EmpiricalRtnAcceleration::with_zero_terms(
            &terms,
        )))
    }

    /// Enable one term with the supplied coefficient in kilometres per square second.
    #[wasm_bindgen(js_name = withTerm)]
    pub fn with_term(
        &self,
        #[wasm_bindgen(unchecked_param_type = "RtnAxis")] axis_value: f64,
        #[wasm_bindgen(unchecked_param_type = "EmpiricalHarmonic")] harmonic_value: f64,
        coefficient_km_s2: f64,
    ) -> Result<Self, JsValue> {
        self.0
            .with_term(
                axis(axis_value)?,
                harmonic(harmonic_value)?,
                coefficient_km_s2,
            )
            .map(Self)
            .map_err(propagation_error)
    }

    /// Coefficient of one term if enabled, in kilometres per square second.
    #[wasm_bindgen(js_name = coefficientKmS2)]
    pub fn coefficient_km_s2(
        &self,
        #[wasm_bindgen(unchecked_param_type = "RtnAxis")] axis_value: f64,
        #[wasm_bindgen(unchecked_param_type = "EmpiricalHarmonic")] harmonic_value: f64,
    ) -> Result<Option<f64>, JsValue> {
        Ok(self
            .0
            .coefficient_km_s2(axis(axis_value)?, harmonic(harmonic_value)?))
    }

    /// The enabled terms in model parameter order.
    #[wasm_bindgen(unchecked_return_type = "[RtnAxis, EmpiricalHarmonic, number][]")]
    pub fn terms(&self) -> Result<JsValue, JsValue> {
        let terms = self
            .0
            .terms()
            .into_iter()
            .map(|(axis, harmonic, value)| (axis as u32, harmonic as u32, value))
            .collect::<Vec<_>>();
        crate::error::to_plain_js(&terms, "empirical terms")
    }

    /// Stable estimable parameter name for this combination of axis and harmonic.
    #[wasm_bindgen(js_name = parameterName)]
    pub fn parameter_name(
        #[wasm_bindgen(unchecked_param_type = "RtnAxis")] axis_value: f64,
        #[wasm_bindgen(unchecked_param_type = "EmpiricalHarmonic")] harmonic_value: f64,
    ) -> Result<String, JsValue> {
        Ok(forces::EmpiricalRtnAcceleration::parameter_name(
            axis(axis_value)?,
            harmonic(harmonic_value)?,
        )
        .to_owned())
    }

    /// Compare the enabled terms and their coefficients.
    pub fn equals(&self, other: &EmpiricalRtnAcceleration) -> bool {
        self.0 == other.0
    }

    /// Return a shared handle usable for propagation and estimation.
    #[wasm_bindgen(js_name = asForceModel)]
    pub fn as_force_model(&self) -> ForceModel {
        ForceModel(Arc::new(self.0))
    }
}

force_methods!(EmpiricalRtnAcceleration, 0, acceleration, acceleration);

/// Compare two RtnAxis values in the core order.
#[wasm_bindgen(js_name = rtnAxisCompare)]
pub fn rtn_axis_compare(
    #[wasm_bindgen(unchecked_param_type = "RtnAxis")] left: f64,
    #[wasm_bindgen(unchecked_param_type = "RtnAxis")] right: f64,
) -> Result<i32, JsValue> {
    Ok(axis(left)?.cmp(&axis(right)?) as i32)
}

/// Compare two EmpiricalHarmonic values in the core order.
#[wasm_bindgen(js_name = empiricalHarmonicCompare)]
pub fn empirical_harmonic_compare(
    #[wasm_bindgen(unchecked_param_type = "EmpiricalHarmonic")] left: f64,
    #[wasm_bindgen(unchecked_param_type = "EmpiricalHarmonic")] right: f64,
) -> Result<i32, JsValue> {
    Ok(harmonic(left)?.cmp(&harmonic(right)?) as i32)
}
