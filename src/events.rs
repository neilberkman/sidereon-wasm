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

/// Lighting-ratio model for solar radiation pressure.
#[wasm_bindgen]
pub enum ShadowFunction {
    /// Apparent-disc occultation.
    ApparentDisc,
    /// Earth-only linear conical geometry.
    Conical,
}

/// IAU nominal solar radius in kilometres.
#[wasm_bindgen(js_name = IAU_2015_SOLAR_RADIUS_KM)]
pub fn iau_2015_solar_radius_km() -> f64 {
    sidereon_core::astro::constants::astro::IAU_2015_SOLAR_RADIUS_KM
}

fn eclipse_detail(error: sidereon_core::astro::events::eclipse::EclipseError) -> serde_json::Value {
    use sidereon_core::astro::events::eclipse::EclipseError;
    match error {
        EclipseError::InvalidInput { field, reason } => {
            serde_json::json!({"kind":"invalidInput","field":field,"reason":reason})
        }
    }
}

fn eclipse_error(error: sidereon_core::astro::events::eclipse::EclipseError) -> JsValue {
    crate::error::typed_error("EclipseError", &error.to_string(), eclipse_detail(error))
}

fn shadow_gradient_error(
    error: sidereon_core::astro::events::eclipse::ShadowGradientError,
) -> JsValue {
    use sidereon_core::astro::events::eclipse::ShadowGradientError;
    let detail = match error {
        ShadowGradientError::Eclipse(source) => {
            serde_json::json!({"kind":"eclipse","cause":eclipse_detail(source)})
        }
        ShadowGradientError::NumericalFailure => serde_json::json!({"kind":"numericalFailure"}),
        _ => serde_json::json!({"kind":"unknown"}),
    };
    crate::error::typed_error("ShadowGradientError", &error.to_string(), detail)
}

/// Shadow fraction and derivative with respect to satellite position.
#[wasm_bindgen]
pub struct ShadowFractionGradient {
    fraction: f64,
    gradient: [f64; 3],
}

#[wasm_bindgen]
impl ShadowFractionGradient {
    /// Shadow fraction.
    #[wasm_bindgen(getter)]
    pub fn fraction(&self) -> f64 {
        self.fraction
    }

    /// Derivative with respect to each satellite position component, per kilometre.
    #[wasm_bindgen(getter, js_name = gradientPerKm)]
    pub fn gradient_per_km(&self) -> Vec<f64> {
        self.gradient.to_vec()
    }
}

pub(crate) fn earth_shadow_model(model: f64) -> Result<CoreEarthShadowModel, JsValue> {
    match model {
        x if x == f64::from(EarthShadowModel::Spherical as u32) => {
            Ok(CoreEarthShadowModel::Spherical)
        }
        x if x == f64::from(EarthShadowModel::Wgs84Oblate as u32) => {
            Ok(CoreEarthShadowModel::Wgs84Oblate)
        }
        _ => Err(crate::error::range_error(
            "model must be an EarthShadowModel value",
        )),
    }
}

#[wasm_bindgen(typescript_custom_section)]
const ECLIPSE_TYPES: &str = r#"
/** Invalid position input for eclipse geometry. */
export interface EclipseErrorDetail {
    kind: "invalidInput";
    field: string;
    reason: string;
}

/** Eclipse evaluation error. */
export interface EclipseError extends Error {
    readonly name: "EclipseError";
    readonly kind: EclipseErrorDetail["kind"];
    readonly detail: EclipseErrorDetail;
}

/** Failure to evaluate a shadow fraction or its gradient. */
export type ShadowGradientErrorDetail =
    | { kind: "eclipse"; cause: EclipseErrorDetail }
    | { kind: "numericalFailure" }
    | { kind: "unknown" };

/** Shadow fraction gradient evaluation error. */
export interface ShadowGradientError extends Error {
    readonly name: "ShadowGradientError";
    readonly kind: ShadowGradientErrorDetail["kind"];
    readonly detail: ShadowGradientErrorDetail;
}
"#;

/// Shadow fraction from lunar occultation.
/// Each position is an inertial three-vector in kilometres.
#[wasm_bindgen(js_name = moonShadowFraction)]
pub fn moon_shadow_fraction(
    satellite_position_km: &[f64],
    sun_position_km: &[f64],
    moon_position_km: &[f64],
) -> Result<f64, JsValue> {
    sidereon_core::astro::events::eclipse::moon_shadow_fraction(
        crate::marshal::vec3("satellitePositionKm", satellite_position_km)?,
        crate::marshal::vec3("sunPositionKm", sun_position_km)?,
        crate::marshal::vec3("moonPositionKm", moon_position_km)?,
    )
    .map_err(eclipse_error)
}

/// Combined shadow fraction from spherical Earth and Moon occultation.
/// Each position is an inertial three-vector in kilometres.
#[wasm_bindgen(js_name = shadowFractionWithMoon)]
pub fn shadow_fraction_with_moon(
    satellite_position_km: &[f64],
    sun_position_km: &[f64],
    moon_position_km: &[f64],
) -> Result<f64, JsValue> {
    sidereon_core::astro::events::eclipse::shadow_fraction_with_moon(
        crate::marshal::vec3("satellitePositionKm", satellite_position_km)?,
        crate::marshal::vec3("sunPositionKm", sun_position_km)?,
        crate::marshal::vec3("moonPositionKm", moon_position_km)?,
    )
    .map_err(eclipse_error)
}

/// Combined shadow fraction with the selected Earth figure and Moon occultation.
/// Each position is an inertial three-vector in kilometres.
#[wasm_bindgen(js_name = shadowFractionWithModelAndMoon)]
pub fn shadow_fraction_with_model_and_moon(
    satellite_position_km: &[f64],
    sun_position_km: &[f64],
    moon_position_km: &[f64],
    #[wasm_bindgen(unchecked_param_type = "EarthShadowModel")] model: f64,
) -> Result<f64, JsValue> {
    sidereon_core::astro::events::eclipse::shadow_fraction_with_model_and_moon(
        crate::marshal::vec3("satellitePositionKm", satellite_position_km)?,
        crate::marshal::vec3("sunPositionKm", sun_position_km)?,
        crate::marshal::vec3("moonPositionKm", moon_position_km)?,
        earth_shadow_model(model)?,
    )
    .map_err(eclipse_error)
}

/// Eclipse status from spherical Earth and Moon occultation.
/// Each position is an inertial three-vector in kilometres.
#[wasm_bindgen(js_name = statusWithMoon, unchecked_return_type = "\"sunlit\" | \"penumbra\" | \"umbra\"")]
pub fn status_with_moon(
    satellite_position_km: &[f64],
    sun_position_km: &[f64],
    moon_position_km: &[f64],
) -> Result<String, JsValue> {
    sidereon_core::astro::events::eclipse::status_with_moon(
        crate::marshal::vec3("satellitePositionKm", satellite_position_km)?,
        crate::marshal::vec3("sunPositionKm", sun_position_km)?,
        crate::marshal::vec3("moonPositionKm", moon_position_km)?,
    )
    .map(|status| status_label(status).to_owned())
    .map_err(eclipse_error)
}

/// Eclipse status with the selected Earth figure and Moon occultation.
/// Each position is an inertial three-vector in kilometres.
#[wasm_bindgen(js_name = statusWithModelAndMoon, unchecked_return_type = "\"sunlit\" | \"penumbra\" | \"umbra\"")]
pub fn status_with_model_and_moon(
    satellite_position_km: &[f64],
    sun_position_km: &[f64],
    moon_position_km: &[f64],
    #[wasm_bindgen(unchecked_param_type = "EarthShadowModel")] model: f64,
) -> Result<String, JsValue> {
    sidereon_core::astro::events::eclipse::status_with_model_and_moon(
        crate::marshal::vec3("satellitePositionKm", satellite_position_km)?,
        crate::marshal::vec3("sunPositionKm", sun_position_km)?,
        crate::marshal::vec3("moonPositionKm", moon_position_km)?,
        earth_shadow_model(model)?,
    )
    .map(|status| status_label(status).to_owned())
    .map_err(eclipse_error)
}

/// Spherical Earth conical shadow fraction and position derivative.
/// Each position is an inertial three-vector in kilometres.
#[wasm_bindgen(js_name = shadowFractionWithGradient)]
pub fn shadow_fraction_with_gradient(
    satellite_position_km: &[f64],
    sun_position_km: &[f64],
) -> Result<ShadowFractionGradient, JsValue> {
    sidereon_core::astro::events::eclipse::shadow_fraction_with_gradient(
        crate::marshal::vec3("satellitePositionKm", satellite_position_km)?,
        crate::marshal::vec3("sunPositionKm", sun_position_km)?,
    )
    .map(|(fraction, gradient)| ShadowFractionGradient { fraction, gradient })
    .map_err(shadow_gradient_error)
}

/// Conical shadow fraction and position derivative with the selected Earth figure.
/// Each position is an inertial three-vector in kilometres.
#[wasm_bindgen(js_name = shadowFractionWithModelAndGradient)]
pub fn shadow_fraction_with_model_and_gradient(
    satellite_position_km: &[f64],
    sun_position_km: &[f64],
    #[wasm_bindgen(unchecked_param_type = "EarthShadowModel")] model: f64,
) -> Result<ShadowFractionGradient, JsValue> {
    sidereon_core::astro::events::eclipse::shadow_fraction_with_model_and_gradient(
        crate::marshal::vec3("satellitePositionKm", satellite_position_km)?,
        crate::marshal::vec3("sunPositionKm", sun_position_km)?,
        earth_shadow_model(model)?,
    )
    .map(|(fraction, gradient)| ShadowFractionGradient { fraction, gradient })
    .map_err(shadow_gradient_error)
}

/// Combined Earth and Moon shadow fraction and position derivative.
/// Each position is an inertial three-vector in kilometres.
#[wasm_bindgen(js_name = shadowFractionWithModelMoonAndGradient)]
pub fn shadow_fraction_with_model_moon_and_gradient(
    satellite_position_km: &[f64],
    sun_position_km: &[f64],
    moon_position_km: &[f64],
    #[wasm_bindgen(unchecked_param_type = "EarthShadowModel")] model: f64,
) -> Result<ShadowFractionGradient, JsValue> {
    sidereon_core::astro::events::eclipse::shadow_fraction_with_model_moon_and_gradient(
        crate::marshal::vec3("satellitePositionKm", satellite_position_km)?,
        crate::marshal::vec3("sunPositionKm", sun_position_km)?,
        crate::marshal::vec3("moonPositionKm", moon_position_km)?,
        earth_shadow_model(model)?,
    )
    .map(|(fraction, gradient)| ShadowFractionGradient { fraction, gradient })
    .map_err(shadow_gradient_error)
}

impl From<CoreEarthShadowModel> for EarthShadowModel {
    fn from(model: CoreEarthShadowModel) -> Self {
        match model {
            CoreEarthShadowModel::Spherical => Self::Spherical,
            CoreEarthShadowModel::Wgs84Oblate => Self::Wgs84Oblate,
        }
    }
}

pub(crate) fn shadow_function(
    value: f64,
) -> Result<sidereon_core::astro::events::eclipse::ShadowFunction, JsValue> {
    use sidereon_core::astro::events::eclipse::ShadowFunction as CoreShadowFunction;
    match value {
        x if x == f64::from(ShadowFunction::ApparentDisc as u32) => {
            Ok(CoreShadowFunction::ApparentDisc)
        }
        x if x == f64::from(ShadowFunction::Conical as u32) => Ok(CoreShadowFunction::Conical),
        _ => Err(crate::error::range_error(
            "function must be a ShadowFunction value",
        )),
    }
}

pub(crate) fn shadow_function_label(
    value: sidereon_core::astro::events::eclipse::ShadowFunction,
) -> Result<ShadowFunction, JsValue> {
    use sidereon_core::astro::events::eclipse::ShadowFunction as CoreShadowFunction;
    match value {
        CoreShadowFunction::ApparentDisc => Ok(ShadowFunction::ApparentDisc),
        CoreShadowFunction::Conical => Ok(ShadowFunction::Conical),
        _ => Err(crate::error::range_error(
            "unsupported ShadowFunction value",
        )),
    }
}
