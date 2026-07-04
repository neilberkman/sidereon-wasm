//! Events + body-angle geometry: eclipse shadow / status and the Sun/Moon angle
//! helpers, batched over position grids.
//!
//! Every output is delegated to `sidereon-core` (`events::eclipse`,
//! `astro::angles`); this module only reshapes flat row-major `(n, 3)`
//! `Float64Array` batches and surfaces the engine numbers. Each helper crosses
//! the FFI boundary once with the per-row loop in Rust.

use wasm_bindgen::prelude::*;

use sidereon_core::astro::angles::{
    angular_separation as core_angular_separation,
    angular_separation_coords as core_angular_separation_coords, beta_angle as core_beta_angle,
    beta_angle_from_state as core_beta_angle_from_state,
    earth_angular_radius as core_earth_angular_radius, moon_angle as core_moon_angle,
    phase_angle as core_phase_angle, position_angle as core_position_angle,
    sun_angle as core_sun_angle, sun_elevation as core_sun_elevation,
};
use sidereon_core::astro::events::eclipse::{
    shadow_fraction as core_shadow_fraction,
    shadow_fraction_with_model as core_shadow_fraction_with_model, status as core_status,
    EarthShadowModel as CoreEarthShadowModel, EclipseStatus,
};

use crate::error::engine_error;
use crate::marshal::{reject_empty, rows3, same_len};

fn status_label(status: EclipseStatus) -> &'static str {
    match status {
        EclipseStatus::Sunlit => "sunlit",
        EclipseStatus::Penumbra => "penumbra",
        EclipseStatus::Umbra => "umbra",
    }
}

/// Earth figure used for conical eclipse shadow geometry.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EarthShadowModel {
    /// Spherical Earth using the core mean Earth radius.
    Spherical,
    /// WGS84 oblate Earth approximation by polar-axis scaling.
    Wgs84Oblate,
}

impl From<EarthShadowModel> for CoreEarthShadowModel {
    fn from(model: EarthShadowModel) -> Self {
        match model {
            EarthShadowModel::Spherical => Self::Spherical,
            EarthShadowModel::Wgs84Oblate => Self::Wgs84Oblate,
        }
    }
}

/// Shadow fraction in `[0, 1]` for satellite and Sun position batches, km.
///
/// `satellitePositionKm` and `sunPositionKm` are flat row-major `(n, 3)`
/// `Float64Array`s. Returns a `Float64Array` of length `n`.
#[wasm_bindgen(js_name = shadowFraction)]
pub fn shadow_fraction(
    satellite_position_km: &[f64],
    sun_position_km: &[f64],
) -> Result<Vec<f64>, JsValue> {
    let sats = rows3("satellitePositionKm", satellite_position_km, true)?;
    let suns = rows3("sunPositionKm", sun_position_km, true)?;
    reject_empty("satellitePositionKm", &sats)?;
    same_len(
        "satellitePositionKm",
        sats.len(),
        "sunPositionKm",
        suns.len(),
    )?;
    sats.iter()
        .zip(suns.iter())
        .map(|(&sat, &sun)| core_shadow_fraction(sat, sun).map_err(engine_error))
        .collect()
}

/// Shadow fraction in `[0, 1]` for position batches with an explicit Earth
/// shadow model.
///
/// `satellitePositionKm` and `sunPositionKm` are flat row-major `(n, 3)`
/// `Float64Array`s. `EarthShadowModel.Spherical` is bit-identical to
/// [`shadowFraction`].
#[wasm_bindgen(js_name = shadowFractionWithModel)]
pub fn shadow_fraction_with_model(
    satellite_position_km: &[f64],
    sun_position_km: &[f64],
    model: EarthShadowModel,
) -> Result<Vec<f64>, JsValue> {
    let sats = rows3("satellitePositionKm", satellite_position_km, true)?;
    let suns = rows3("sunPositionKm", sun_position_km, true)?;
    reject_empty("satellitePositionKm", &sats)?;
    same_len(
        "satellitePositionKm",
        sats.len(),
        "sunPositionKm",
        suns.len(),
    )?;
    let model = model.into();
    sats.iter()
        .zip(suns.iter())
        .map(|(&sat, &sun)| core_shadow_fraction_with_model(sat, sun, model).map_err(engine_error))
        .collect()
}

/// Eclipse status for satellite and Sun position batches, km. Returns a
/// `string[]` of `"sunlit"` / `"penumbra"` / `"umbra"`.
#[wasm_bindgen(js_name = eclipseStatus)]
pub fn eclipse_status(
    satellite_position_km: &[f64],
    sun_position_km: &[f64],
) -> Result<Vec<String>, JsValue> {
    let sats = rows3("satellitePositionKm", satellite_position_km, true)?;
    let suns = rows3("sunPositionKm", sun_position_km, true)?;
    reject_empty("satellitePositionKm", &sats)?;
    same_len(
        "satellitePositionKm",
        sats.len(),
        "sunPositionKm",
        suns.len(),
    )?;
    sats.iter()
        .zip(suns.iter())
        .map(|(&sat, &sun)| {
            core_status(sat, sun)
                .map(|s| status_label(s).to_string())
                .map_err(engine_error)
        })
        .collect()
}

/// Angle in degrees between satellite nadir and the Sun direction.
#[wasm_bindgen(js_name = sunAngle)]
pub fn sun_angle(
    satellite_position_km: &[f64],
    sun_position_km: &[f64],
) -> Result<Vec<f64>, JsValue> {
    let sats = rows3("satellitePositionKm", satellite_position_km, true)?;
    let suns = rows3("sunPositionKm", sun_position_km, true)?;
    reject_empty("satellitePositionKm", &sats)?;
    same_len(
        "satellitePositionKm",
        sats.len(),
        "sunPositionKm",
        suns.len(),
    )?;
    sats.iter()
        .zip(suns.iter())
        .map(|(&sat, &sun)| core_sun_angle(sat, sun).map_err(engine_error))
        .collect()
}

/// Angle in degrees between satellite nadir and the Moon direction.
#[wasm_bindgen(js_name = moonAngle)]
pub fn moon_angle(
    satellite_position_km: &[f64],
    moon_position_km: &[f64],
) -> Result<Vec<f64>, JsValue> {
    let sats = rows3("satellitePositionKm", satellite_position_km, true)?;
    let moons = rows3("moonPositionKm", moon_position_km, true)?;
    reject_empty("satellitePositionKm", &sats)?;
    same_len(
        "satellitePositionKm",
        sats.len(),
        "moonPositionKm",
        moons.len(),
    )?;
    sats.iter()
        .zip(moons.iter())
        .map(|(&sat, &moon)| core_moon_angle(sat, moon).map_err(engine_error))
        .collect()
}

/// Sun elevation in degrees above the satellite local horizontal plane.
#[wasm_bindgen(js_name = sunElevation)]
pub fn sun_elevation(
    satellite_position_km: &[f64],
    sun_position_km: &[f64],
) -> Result<Vec<f64>, JsValue> {
    let sats = rows3("satellitePositionKm", satellite_position_km, true)?;
    let suns = rows3("sunPositionKm", sun_position_km, true)?;
    reject_empty("satellitePositionKm", &sats)?;
    same_len(
        "satellitePositionKm",
        sats.len(),
        "sunPositionKm",
        suns.len(),
    )?;
    sats.iter()
        .zip(suns.iter())
        .map(|(&sat, &sun)| core_sun_elevation(sat, sun).map_err(engine_error))
        .collect()
}

/// Sun-satellite-observer phase angle in degrees.
#[wasm_bindgen(js_name = phaseAngle)]
pub fn phase_angle(
    satellite_position_km: &[f64],
    sun_position_km: &[f64],
    observer_position_km: &[f64],
) -> Result<Vec<f64>, JsValue> {
    let sats = rows3("satellitePositionKm", satellite_position_km, true)?;
    let suns = rows3("sunPositionKm", sun_position_km, true)?;
    let observers = rows3("observerPositionKm", observer_position_km, true)?;
    reject_empty("satellitePositionKm", &sats)?;
    same_len(
        "satellitePositionKm",
        sats.len(),
        "sunPositionKm",
        suns.len(),
    )?;
    same_len(
        "satellitePositionKm",
        sats.len(),
        "observerPositionKm",
        observers.len(),
    )?;
    sats.iter()
        .zip(suns.iter())
        .zip(observers.iter())
        .map(|((&sat, &sun), &obs)| core_phase_angle(sat, sun, obs).map_err(engine_error))
        .collect()
}

/// Angular radius in degrees of Earth as seen from each satellite position.
#[wasm_bindgen(js_name = earthAngularRadius)]
pub fn earth_angular_radius(satellite_position_km: &[f64]) -> Result<Vec<f64>, JsValue> {
    let sats = rows3("satellitePositionKm", satellite_position_km, true)?;
    reject_empty("satellitePositionKm", &sats)?;
    sats.iter()
        .map(|&sat| core_earth_angular_radius(sat).map_err(engine_error))
        .collect()
}

/// On-sky angle in degrees between two direction vectors.
#[wasm_bindgen(js_name = angularSeparation)]
pub fn angular_separation(a: &[f64], b: &[f64]) -> Result<f64, JsValue> {
    let a = crate::marshal::vec3_finite("a", a)?;
    let b = crate::marshal::vec3_finite("b", b)?;
    core_angular_separation(a, b).map_err(engine_error)
}

/// On-sky angle in degrees between two `(lonDeg, latDeg)` direction pairs.
#[wasm_bindgen(js_name = angularSeparationCoords)]
pub fn angular_separation_coords(
    a_lon_deg: f64,
    a_lat_deg: f64,
    b_lon_deg: f64,
    b_lat_deg: f64,
) -> Result<f64, JsValue> {
    core_angular_separation_coords((a_lon_deg, a_lat_deg), (b_lon_deg, b_lat_deg))
        .map_err(engine_error)
}

/// Position angle in degrees from North through East.
#[wasm_bindgen(js_name = positionAngle)]
pub fn position_angle(
    from_lon_deg: f64,
    from_lat_deg: f64,
    to_lon_deg: f64,
    to_lat_deg: f64,
) -> Result<f64, JsValue> {
    core_position_angle((from_lon_deg, from_lat_deg), (to_lon_deg, to_lat_deg))
        .map_err(engine_error)
}

/// Solar beta angle in degrees from orbit normal and Sun vectors.
#[wasm_bindgen(js_name = betaAngle)]
pub fn beta_angle(orbit_normal: &[f64], sun: &[f64]) -> Result<f64, JsValue> {
    let orbit_normal = crate::marshal::vec3_finite("orbitNormal", orbit_normal)?;
    let sun = crate::marshal::vec3_finite("sun", sun)?;
    core_beta_angle(orbit_normal, sun).map_err(engine_error)
}

/// Solar beta angle in degrees from an inertial state and Sun vector.
#[wasm_bindgen(js_name = betaAngleFromState)]
pub fn beta_angle_from_state(r: &[f64], v: &[f64], sun: &[f64]) -> Result<f64, JsValue> {
    let r = crate::marshal::vec3_finite("r", r)?;
    let v = crate::marshal::vec3_finite("v", v)?;
    let sun = crate::marshal::vec3_finite("sun", sun)?;
    core_beta_angle_from_state(r, v, sun).map_err(engine_error)
}
