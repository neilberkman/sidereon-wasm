//! Conjunction and covariance binding: encounter-frame geometry, B-plane
//! covariance projection, collision probability, and RTN<->ECI covariance
//! rotation.
//!
//! All math is `sidereon_core::astro::{conjunction, covariance}`. Vectors cross
//! as flat length-3 `Float64Array`s and 3x3 / 2x2 matrices as flat row-major
//! length-9 / 4 `Float64Array`s.

use wasm_bindgen::prelude::*;

use sidereon_core::astro::conjunction::{
    collision_probability as core_collision_probability, encounter_frame as core_encounter_frame,
    encounter_plane_covariance as core_encounter_plane_covariance, CollisionPc, ConjunctionError,
    ConjunctionState as CoreConjunctionState, EncounterFrame as CoreEncounterFrame, PcMethod,
};
use sidereon_core::astro::covariance::{positive_semidefinite, rtn_to_eci, symmetric};

use crate::error::{engine_error, range_error, type_error};
use crate::marshal::{mat2_flat, mat3_flat, mat3_from_flat, vec3};

fn conjunction_err(err: ConjunctionError) -> JsValue {
    match err {
        ConjunctionError::UndefinedFrame => engine_error(err),
        ConjunctionError::NonFinite { .. } | ConjunctionError::NotPositive { .. } => {
            range_error(&err.to_string())
        }
    }
}

fn parse_pc_method(label: Option<String>) -> Result<PcMethod, JsValue> {
    match label.as_deref() {
        None | Some("foster_equal_area") => Ok(PcMethod::FosterEqualArea),
        Some("foster_numerical") => Ok(PcMethod::FosterNumerical),
        Some("alfano_2005") => Ok(PcMethod::Alfano2005),
        Some(other) => Err(type_error(&format!(
            "unknown Pc method {other:?}; expected \"foster_equal_area\", \"foster_numerical\", or \"alfano_2005\""
        ))),
    }
}

/// One object's conjunction state: ECI position (km), velocity (km/s), and 3x3
/// position covariance (km^2, flat row-major length 9).
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct ConjunctionState {
    inner: CoreConjunctionState,
}

#[wasm_bindgen]
impl ConjunctionState {
    /// Build a conjunction state. `positionKm` and `velocityKmS` are length-3
    /// `Float64Array`s; `covarianceKm2` is a flat row-major length-9
    /// `Float64Array`.
    #[wasm_bindgen(constructor)]
    pub fn new(
        position_km: &[f64],
        velocity_km_s: &[f64],
        covariance_km2: &[f64],
    ) -> Result<ConjunctionState, JsValue> {
        Ok(ConjunctionState {
            inner: CoreConjunctionState {
                position_km: vec3("positionKm", position_km)?,
                velocity_km_s: vec3("velocityKmS", velocity_km_s)?,
                covariance_km2: mat3_from_flat("covarianceKm2", covariance_km2)?,
            },
        })
    }

    /// ECI position, kilometres, as a length-3 `Float64Array`.
    #[wasm_bindgen(getter, js_name = positionKm)]
    pub fn position_km(&self) -> Vec<f64> {
        self.inner.position_km.to_vec()
    }

    /// ECI velocity, km/s, as a length-3 `Float64Array`.
    #[wasm_bindgen(getter, js_name = velocityKmS)]
    pub fn velocity_km_s(&self) -> Vec<f64> {
        self.inner.velocity_km_s.to_vec()
    }

    /// Position covariance, km^2, as a flat row-major length-9 `Float64Array`.
    #[wasm_bindgen(getter, js_name = covarianceKm2)]
    pub fn covariance_km2(&self) -> Vec<f64> {
        mat3_flat(&self.inner.covariance_km2)
    }
}

/// Orthonormal encounter frame built from two relative states.
#[wasm_bindgen]
pub struct EncounterFrame {
    inner: CoreEncounterFrame,
}

#[wasm_bindgen]
impl EncounterFrame {
    /// In-plane cross-track unit axis, length-3 `Float64Array`.
    #[wasm_bindgen(getter, js_name = xHat)]
    pub fn x_hat(&self) -> Vec<f64> {
        self.inner.x_hat.to_vec()
    }

    /// Relative-velocity unit axis, length-3 `Float64Array`.
    #[wasm_bindgen(getter, js_name = yHat)]
    pub fn y_hat(&self) -> Vec<f64> {
        self.inner.y_hat.to_vec()
    }

    /// Encounter-plane normal unit axis, length-3 `Float64Array`.
    #[wasm_bindgen(getter, js_name = zHat)]
    pub fn z_hat(&self) -> Vec<f64> {
        self.inner.z_hat.to_vec()
    }

    /// Relative position (object2 minus object1), km, length-3 `Float64Array`.
    #[wasm_bindgen(getter, js_name = relativePositionKm)]
    pub fn relative_position_km(&self) -> Vec<f64> {
        self.inner.relative_position_km.to_vec()
    }

    /// Relative velocity (object2 minus object1), km/s, length-3 `Float64Array`.
    #[wasm_bindgen(getter, js_name = relativeVelocityKmS)]
    pub fn relative_velocity_km_s(&self) -> Vec<f64> {
        self.inner.relative_velocity_km_s.to_vec()
    }

    /// Orthogonal miss distance in the encounter plane, km.
    #[wasm_bindgen(getter, js_name = missKm)]
    pub fn miss_km(&self) -> f64 {
        self.inner.miss_km
    }

    /// Relative speed, km/s.
    #[wasm_bindgen(getter, js_name = relativeSpeedKmS)]
    pub fn relative_speed_km_s(&self) -> f64 {
        self.inner.relative_speed_km_s
    }
}

/// Collision-probability result and encounter-plane summary.
#[wasm_bindgen]
pub struct CollisionProbability {
    inner: CollisionPc,
}

#[wasm_bindgen]
impl CollisionProbability {
    /// Collision probability.
    #[wasm_bindgen(getter)]
    pub fn pc(&self) -> f64 {
        self.inner.pc
    }

    /// Orthogonal miss distance in the encounter plane, km.
    #[wasm_bindgen(getter, js_name = missKm)]
    pub fn miss_km(&self) -> f64 {
        self.inner.miss_km
    }

    /// Relative speed, km/s.
    #[wasm_bindgen(getter, js_name = relativeSpeedKmS)]
    pub fn relative_speed_km_s(&self) -> f64 {
        self.inner.relative_speed_km_s
    }

    /// Principal-axis standard deviation in the encounter plane, km.
    #[wasm_bindgen(getter, js_name = sigmaXKm)]
    pub fn sigma_x_km(&self) -> f64 {
        self.inner.sigma_x_km
    }

    /// Principal-axis standard deviation in the encounter plane, km.
    #[wasm_bindgen(getter, js_name = sigmaZKm)]
    pub fn sigma_z_km(&self) -> f64 {
        self.inner.sigma_z_km
    }
}

/// Build the encounter frame from two position/velocity states (each a length-3
/// `Float64Array`).
#[wasm_bindgen(js_name = encounterFrame)]
pub fn encounter_frame(
    position1_km: &[f64],
    velocity1_km_s: &[f64],
    position2_km: &[f64],
    velocity2_km_s: &[f64],
) -> Result<EncounterFrame, JsValue> {
    let r1 = vec3("position1Km", position1_km)?;
    let v1 = vec3("velocity1KmS", velocity1_km_s)?;
    let r2 = vec3("position2Km", position2_km)?;
    let v2 = vec3("velocity2KmS", velocity2_km_s)?;
    let inner = core_encounter_frame(r1, v1, r2, v2).map_err(conjunction_err)?;
    Ok(EncounterFrame { inner })
}

/// Project a 3x3 ECI covariance into the encounter B-plane `(x, z)`. Returns a
/// flat row-major length-4 (2-by-2) `Float64Array`.
#[wasm_bindgen(js_name = encounterPlaneCovariance)]
pub fn encounter_plane_covariance(
    frame: &EncounterFrame,
    covariance_km2: &[f64],
) -> Result<Vec<f64>, JsValue> {
    let cov = mat3_from_flat("covarianceKm2", covariance_km2)?;
    let projected = core_encounter_plane_covariance(&frame.inner, &cov).map_err(conjunction_err)?;
    Ok(mat2_flat(&projected))
}

/// Compute collision probability from two conjunction states and a hard-body
/// radius. `method` is one of `"foster_equal_area"` (default),
/// `"foster_numerical"`, `"alfano_2005"`.
#[wasm_bindgen(js_name = collisionProbability)]
pub fn collision_probability(
    object1: &ConjunctionState,
    object2: &ConjunctionState,
    hard_body_radius_km: f64,
    method: Option<String>,
) -> Result<CollisionProbability, JsValue> {
    let method = parse_pc_method(method)?;
    let inner =
        core_collision_probability(&object1.inner, &object2.inner, hard_body_radius_km, method)
            .map_err(conjunction_err)?;
    Ok(CollisionProbability { inner })
}

/// Transform a 3x3 RTN covariance (flat row-major length 9) to ECI for the given
/// orbit state. Returns a flat row-major length-9 `Float64Array`.
#[wasm_bindgen(js_name = rtnToEciCovariance)]
pub fn rtn_to_eci_covariance(
    covariance_rtn: &[f64],
    position_km: &[f64],
    velocity_km_s: &[f64],
) -> Result<Vec<f64>, JsValue> {
    let cov = mat3_from_flat("covarianceRtn", covariance_rtn)?;
    let r = vec3("positionKm", position_km)?;
    let v = vec3("velocityKmS", velocity_km_s)?;
    let eci = rtn_to_eci(&cov, r, v).map_err(|e| range_error(e.message()))?;
    Ok(mat3_flat(&eci))
}

/// Whether a 3x3 covariance (flat row-major length 9) is symmetric within the
/// engine tolerance.
#[wasm_bindgen(js_name = covarianceIsSymmetric)]
pub fn covariance_is_symmetric(covariance_km2: &[f64]) -> Result<bool, JsValue> {
    Ok(symmetric(&mat3_from_flat("covarianceKm2", covariance_km2)?))
}

/// Whether a 3x3 covariance (flat row-major length 9) is symmetric positive
/// semidefinite within the engine tolerance.
#[wasm_bindgen(js_name = covarianceIsPositiveSemidefinite)]
pub fn covariance_is_positive_semidefinite(covariance_km2: &[f64]) -> Result<bool, JsValue> {
    Ok(positive_semidefinite(&mat3_from_flat(
        "covarianceKm2",
        covariance_km2,
    )?))
}
