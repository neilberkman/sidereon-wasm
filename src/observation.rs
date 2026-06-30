//! Observational-astronomy geometry binding.
//!
//! Thin wrappers over `sidereon_core::astro::observation`. Each function takes
//! already-resolved geometry (Earth-fixed / inertial vectors, angles in degrees)
//! and returns the core's pure numerical result. Surface points cross to JS as
//! `{ latitudeDeg, longitudeDeg }` plain objects; the engine owns every formula.

use serde::Serialize;
use wasm_bindgen::prelude::*;

use sidereon_core::astro::observation::{
    parallactic_angle_deg as core_parallactic_angle_deg,
    satellite_visual_magnitude as core_satellite_visual_magnitude,
    sub_observer_point as core_sub_observer_point, sub_solar_point as core_sub_solar_point,
    terminator_latitude_deg as core_terminator_latitude_deg, SurfacePoint,
};

use crate::error::{engine_error, type_error};
use crate::marshal::vec3_finite;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SurfacePointObject {
    latitude_deg: f64,
    longitude_deg: f64,
}

fn surface_point_to_object(point: SurfacePoint) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(&SurfacePointObject {
        latitude_deg: point.latitude_deg,
        longitude_deg: point.longitude_deg,
    })
    .map_err(|e| type_error(&e.to_string()))
}

/// Sub-solar point: the geographic point where the Sun is at the zenith.
///
/// `sunEcef` is the geocentric Sun position (length-3 `Float64Array`) in an
/// Earth-fixed (ITRS/ECEF) frame; only its direction matters. Returns
/// `{ latitudeDeg, longitudeDeg }` (geocentric, degrees). Delegates to
/// `sidereon_core::astro::observation::sub_solar_point`.
#[wasm_bindgen(js_name = subSolarPoint)]
pub fn sub_solar_point(sun_ecef: &[f64]) -> Result<JsValue, JsValue> {
    let sun = vec3_finite("sunEcef", sun_ecef)?;
    let point = core_sub_solar_point(sun).map_err(engine_error)?;
    surface_point_to_object(point)
}

/// Latitude (degrees) of the day-night terminator at a query longitude.
///
/// `subSolarLatitudeDeg` / `subSolarLongitudeDeg` are the sub-solar point (see
/// `subSolarPoint`); `longitudeDeg` is the query longitude. Delegates to
/// `sidereon_core::astro::observation::terminator_latitude_deg`.
#[wasm_bindgen(js_name = terminatorLatitudeDeg)]
pub fn terminator_latitude_deg(
    sub_solar_latitude_deg: f64,
    sub_solar_longitude_deg: f64,
    longitude_deg: f64,
) -> Result<f64, JsValue> {
    let sub_solar = SurfacePoint {
        latitude_deg: sub_solar_latitude_deg,
        longitude_deg: sub_solar_longitude_deg,
    };
    core_terminator_latitude_deg(sub_solar, longitude_deg).map_err(engine_error)
}

/// Parallactic angle (degrees) of a target at a station.
///
/// `observerLatitudeDeg` is the observer geodetic latitude, `hourAngleDeg` the
/// local hour angle (positive west of the meridian), and `declinationDeg` the
/// target declination. The result is on `(-180, 180]`. Delegates to
/// `sidereon_core::astro::observation::parallactic_angle_deg`.
#[wasm_bindgen(js_name = parallacticAngleDeg)]
pub fn parallactic_angle_deg(
    observer_latitude_deg: f64,
    hour_angle_deg: f64,
    declination_deg: f64,
) -> Result<f64, JsValue> {
    core_parallactic_angle_deg(observer_latitude_deg, hour_angle_deg, declination_deg)
        .map_err(engine_error)
}

/// Apparent visual magnitude of a sunlit body from a diffuse-sphere phase law.
///
/// `rangeKm` and `referenceRangeKm` (both positive) are the observation range
/// and the reference range at which `standardMagnitude` is defined;
/// `phaseAngleDeg` is the solar phase angle (Sun-body-observer), clamped to
/// `[0, 180]`. Delegates to
/// `sidereon_core::astro::observation::satellite_visual_magnitude`.
#[wasm_bindgen(js_name = satelliteVisualMagnitude)]
pub fn satellite_visual_magnitude(
    range_km: f64,
    phase_angle_deg: f64,
    standard_magnitude: f64,
    reference_range_km: f64,
) -> Result<f64, JsValue> {
    core_satellite_visual_magnitude(
        range_km,
        phase_angle_deg,
        standard_magnitude,
        reference_range_km,
    )
    .map_err(engine_error)
}

/// Sub-observer point (planetary central meridian) on a rotating body.
///
/// `observerFromBody` is the observer position relative to the body center
/// (length-3 `Float64Array`) in an inertial (ICRF/J2000 equatorial) frame, and
/// `poleRaDeg` / `poleDecDeg` / `primeMeridianDeg` are the IAU WGCCRE pole right
/// ascension, declination, and prime-meridian angle. Returns the body-fixed
/// `{ latitudeDeg, longitudeDeg }` (planetocentric, longitude on `(-180, 180]`).
/// Delegates to `sidereon_core::astro::observation::sub_observer_point`.
#[wasm_bindgen(js_name = subObserverPoint)]
pub fn sub_observer_point(
    observer_from_body: &[f64],
    pole_ra_deg: f64,
    pole_dec_deg: f64,
    prime_meridian_deg: f64,
) -> Result<JsValue, JsValue> {
    let observer = vec3_finite("observerFromBody", observer_from_body)?;
    let point = core_sub_observer_point(observer, pole_ra_deg, pole_dec_deg, prime_meridian_deg)
        .map_err(engine_error)?;
    surface_point_to_object(point)
}
