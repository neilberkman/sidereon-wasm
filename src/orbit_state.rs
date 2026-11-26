//! Cartesian state values for orbit and force models.

use sidereon_core::astro::state::CartesianState as CoreState;
use wasm_bindgen::prelude::*;

use crate::marshal::vec3;

/// Cartesian position and velocity at TDB seconds from J2000.
#[wasm_bindgen]
pub struct CartesianState(pub(crate) CoreState);

#[wasm_bindgen]
impl CartesianState {
    /// Construct from a TDB epoch, position in kilometres and velocity in kilometres per second.
    #[wasm_bindgen(constructor)]
    pub fn new(
        epoch_tdb_seconds: f64,
        position_km: &[f64],
        velocity_km_s: &[f64],
    ) -> Result<CartesianState, JsValue> {
        Ok(Self(CoreState::new(
            epoch_tdb_seconds,
            vec3("positionKm", position_km)?,
            vec3("velocityKmS", velocity_km_s)?,
        )))
    }

    /// The epoch in TDB seconds from J2000.
    #[wasm_bindgen(getter, js_name = epochTdbSeconds)]
    pub fn epoch_tdb_seconds(&self) -> f64 {
        self.0.epoch_tdb_seconds
    }

    /// Position in kilometres.
    #[wasm_bindgen(getter, js_name = positionKm)]
    pub fn position_km(&self) -> Vec<f64> {
        self.0.position_array().to_vec()
    }

    /// Velocity in kilometres per second.
    #[wasm_bindgen(getter, js_name = velocityKmS)]
    pub fn velocity_km_s(&self) -> Vec<f64> {
        self.0.velocity_array().to_vec()
    }

    /// Compare the epoch, position and velocity by value.
    pub fn equals(&self, other: &CartesianState) -> bool {
        self.0 == other.0
    }
}

/// Cartesian vectors without an epoch.
#[wasm_bindgen]
pub struct CartesianVectors {
    pub(crate) position: [f64; 3],
    pub(crate) velocity: [f64; 3],
}

#[wasm_bindgen]
impl CartesianVectors {
    /// Position in kilometres.
    #[wasm_bindgen(getter, js_name = positionKm)]
    pub fn position_km(&self) -> Vec<f64> {
        self.position.to_vec()
    }

    /// Velocity in kilometres per second.
    #[wasm_bindgen(getter, js_name = velocityKmS)]
    pub fn velocity_km_s(&self) -> Vec<f64> {
        self.velocity.to_vec()
    }
}
