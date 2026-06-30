//! Initial orbit determination (IOD): Gibbs, Herrick-Gibbs, and Gauss
//! angles-only.
//!
//! Thin wrapper over `sidereon_core::astro::iod`. The classical Vallado
//! algorithms (Gibbs, Herrick-Gibbs, Gauss) live in the crate; this layer only
//! reshapes the position / angle vectors and re-encodes the velocity and orbit
//! state. All vectors are kilometres / kilometres-per-second.

use wasm_bindgen::prelude::*;

use sidereon_core::astro::iod::{gauss_angles, gibbs, hgibbs};

use crate::error::engine_error;
use crate::marshal::{mat3_from_flat, vec3_finite};

/// A Gibbs / Herrick-Gibbs velocity solve: the velocity at the middle position
/// and the geometry diagnostics.
#[wasm_bindgen]
pub struct IodVelocity {
    velocity_km_s: Vec<f64>,
    theta12_rad: f64,
    theta23_rad: f64,
    coplanarity_rad: f64,
}

#[wasm_bindgen]
impl IodVelocity {
    /// Velocity at the middle position `[vx, vy, vz]`, kilometres per second.
    #[wasm_bindgen(getter, js_name = velocityKmS)]
    pub fn velocity_km_s(&self) -> Vec<f64> {
        self.velocity_km_s.clone()
    }

    /// Angle between the first and second position vectors, radians.
    #[wasm_bindgen(getter, js_name = theta12Rad)]
    pub fn theta12_rad(&self) -> f64 {
        self.theta12_rad
    }

    /// Angle between the second and third position vectors, radians.
    #[wasm_bindgen(getter, js_name = theta23Rad)]
    pub fn theta23_rad(&self) -> f64 {
        self.theta23_rad
    }

    /// Coplanarity angle of the three position vectors, radians.
    #[wasm_bindgen(getter, js_name = coplanarityRad)]
    pub fn coplanarity_rad(&self) -> f64 {
        self.coplanarity_rad
    }
}

/// A determined orbit state: position and velocity at one epoch.
#[wasm_bindgen]
pub struct IodState {
    position_km: Vec<f64>,
    velocity_km_s: Vec<f64>,
}

#[wasm_bindgen]
impl IodState {
    /// Position `[x, y, z]`, kilometres.
    #[wasm_bindgen(getter, js_name = positionKm)]
    pub fn position_km(&self) -> Vec<f64> {
        self.position_km.clone()
    }

    /// Velocity `[vx, vy, vz]`, kilometres per second.
    #[wasm_bindgen(getter, js_name = velocityKmS)]
    pub fn velocity_km_s(&self) -> Vec<f64> {
        self.velocity_km_s.clone()
    }
}

/// Gibbs three-position velocity solve.
///
/// `r1`, `r2`, `r3` are length-3 coplanar geocentric position `Float64Array`s
/// (km). Returns the velocity at `r2` plus the inter-vector and coplanarity
/// angles. Delegates to `sidereon_core::astro::iod::gibbs`.
#[wasm_bindgen(js_name = iodGibbs)]
pub fn iod_gibbs(r1: &[f64], r2: &[f64], r3: &[f64]) -> Result<IodVelocity, JsValue> {
    let r1 = vec3_finite("r1", r1)?;
    let r2 = vec3_finite("r2", r2)?;
    let r3 = vec3_finite("r3", r3)?;
    let (v2, theta12, theta23, copa) = gibbs(&r1, &r2, &r3).map_err(engine_error)?;
    Ok(IodVelocity {
        velocity_km_s: v2.to_vec(),
        theta12_rad: theta12,
        theta23_rad: theta23,
        coplanarity_rad: copa,
    })
}

/// Herrick-Gibbs three-position velocity solve.
///
/// `r1`, `r2`, `r3` are length-3 closely-spaced geocentric position
/// `Float64Array`s (km) and `jd1`/`jd2`/`jd3` their Julian-date epochs (days).
/// Returns the velocity at `r2` plus the inter-vector and coplanarity angles.
/// Delegates to `sidereon_core::astro::iod::hgibbs`.
#[wasm_bindgen(js_name = iodHerrickGibbs)]
#[allow(clippy::too_many_arguments)]
pub fn iod_herrick_gibbs(
    r1: &[f64],
    r2: &[f64],
    r3: &[f64],
    jd1: f64,
    jd2: f64,
    jd3: f64,
) -> Result<IodVelocity, JsValue> {
    let r1 = vec3_finite("r1", r1)?;
    let r2 = vec3_finite("r2", r2)?;
    let r3 = vec3_finite("r3", r3)?;
    let (v2, theta12, theta23, copa) =
        hgibbs(&r1, &r2, &r3, jd1, jd2, jd3).map_err(engine_error)?;
    Ok(IodVelocity {
        velocity_km_s: v2.to_vec(),
        theta12_rad: theta12,
        theta23_rad: theta23,
        coplanarity_rad: copa,
    })
}

/// Gauss angles-only orbit determination.
///
/// `decl` and `rtasc` are length-3 declination / right-ascension
/// `Float64Array`s (radians), `jd` and `jdf` length-3 split Julian dates (whole
/// part and fraction, days), and `rseci` a flat row-major `(3, 3)`
/// `Float64Array` of observer-site ECI positions (km), one row per epoch.
/// Returns the orbit state at the middle observation. Delegates to
/// `sidereon_core::astro::iod::gauss_angles`.
#[wasm_bindgen(js_name = iodGaussAngles)]
pub fn iod_gauss_angles(
    decl: &[f64],
    rtasc: &[f64],
    jd: &[f64],
    jdf: &[f64],
    rseci: &[f64],
) -> Result<IodState, JsValue> {
    let decl = vec3_finite("decl", decl)?;
    let rtasc = vec3_finite("rtasc", rtasc)?;
    let jd = vec3_finite("jd", jd)?;
    let jdf = vec3_finite("jdf", jdf)?;
    let rseci = mat3_from_flat("rseci", rseci)?;
    let (r2, v2) = gauss_angles(&decl, &rtasc, &jd, &jdf, &rseci).map_err(engine_error)?;
    Ok(IodState {
        position_km: r2.to_vec(),
        velocity_km_s: v2.to_vec(),
    })
}
