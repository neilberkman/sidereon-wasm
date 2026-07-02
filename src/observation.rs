//! Observational-astronomy geometry binding.
//!
//! Thin wrappers over `sidereon_core::astro::observation`. Each function takes
//! already-resolved geometry (Earth-fixed / inertial vectors, angles in degrees)
//! and returns the core's pure numerical result. Surface points cross to JS as
//! `{ latitudeDeg, longitudeDeg }` plain objects; the engine owns every formula.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon::passes::UtcInstant;
use sidereon_core::astro::bodies::{
    observe as core_observe, observe_spk_body as core_observe_spk_body,
    Observation as CoreObservation, ObserveOptions as CoreObserveOptions,
    Refraction as CoreRefraction, Target,
};
use sidereon_core::astro::frames::transforms::{GeodeticStationKm, PolarMotion};
use sidereon_core::astro::observation::{
    parallactic_angle_deg as core_parallactic_angle_deg,
    satellite_visual_magnitude as core_satellite_visual_magnitude,
    sub_observer_point as core_sub_observer_point, sub_solar_point as core_sub_solar_point,
    terminator_latitude_deg as core_terminator_latitude_deg, SurfacePoint,
};

use crate::error::{engine_error, type_error};
use crate::marshal::vec3_finite;
use crate::spk::Spk;

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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StationInput {
    latitude_deg: f64,
    longitude_deg: f64,
    #[serde(default)]
    altitude_km: f64,
}

impl StationInput {
    fn to_core(&self) -> GeodeticStationKm {
        GeodeticStationKm {
            latitude_deg: self.latitude_deg,
            longitude_deg: self.longitude_deg,
            altitude_km: self.altitude_km,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PolarMotionInput {
    xp_arcsec: f64,
    yp_arcsec: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RefractionInput {
    pressure_mbar: f64,
    temperature_c: f64,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ObserveOptionsInput {
    polar_motion: Option<PolarMotionInput>,
    refraction: Option<RefractionInput>,
    deflection: Option<bool>,
    aberration: Option<bool>,
}

impl ObserveOptionsInput {
    fn to_core(&self) -> Result<CoreObserveOptions, JsValue> {
        let defaults = CoreObserveOptions::default();
        Ok(CoreObserveOptions {
            polar_motion: self
                .polar_motion
                .as_ref()
                .map(|p| PolarMotion::from_arcseconds(p.xp_arcsec, p.yp_arcsec))
                .transpose()
                .map_err(engine_error)?,
            refraction: self.refraction.as_ref().map(|r| CoreRefraction {
                pressure_mbar: r.pressure_mbar,
                temperature_c: r.temperature_c,
            }),
            deflection: self.deflection.unwrap_or(defaults.deflection),
            aberration: self.aberration.unwrap_or(defaults.aberration),
        })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EquatorialJs {
    right_ascension_deg: f64,
    right_ascension_hours: f64,
    declination_deg: f64,
    distance_km: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HorizontalJs {
    azimuth_deg: f64,
    elevation_deg: f64,
    range_km: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EclipticJs {
    longitude_deg: f64,
    latitude_deg: f64,
    distance_km: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ObservationJs {
    astrometric: EquatorialJs,
    apparent_icrs: EquatorialJs,
    apparent: EquatorialJs,
    horizontal: HorizontalJs,
    hour_angle_deg: f64,
    hour_angle_hours: f64,
    ecliptic: EclipticJs,
    reduced: bool,
}

fn equatorial_to_js(value: sidereon_core::astro::bodies::Equatorial) -> EquatorialJs {
    EquatorialJs {
        right_ascension_deg: value.right_ascension_deg,
        right_ascension_hours: value.right_ascension_hours,
        declination_deg: value.declination_deg,
        distance_km: value.distance_km,
    }
}

fn observation_to_js(value: CoreObservation) -> Result<JsValue, JsValue> {
    let object = ObservationJs {
        astrometric: equatorial_to_js(value.astrometric),
        apparent_icrs: equatorial_to_js(value.apparent_icrs),
        apparent: equatorial_to_js(value.apparent),
        horizontal: HorizontalJs {
            azimuth_deg: value.horizontal.azimuth_deg,
            elevation_deg: value.horizontal.elevation_deg,
            range_km: value.horizontal.range_km,
        },
        hour_angle_deg: value.hour_angle_deg,
        hour_angle_hours: value.hour_angle_hours,
        ecliptic: EclipticJs {
            longitude_deg: value.ecliptic.longitude_deg,
            latitude_deg: value.ecliptic.latitude_deg,
            distance_km: value.ecliptic.distance_km,
        },
        reduced: value.reduced,
    };
    serde_wasm_bindgen::to_value(&object).map_err(|e| type_error(&e.to_string()))
}

fn parse_options(options: JsValue) -> Result<CoreObserveOptions, JsValue> {
    if options.is_undefined() || options.is_null() {
        return Ok(CoreObserveOptions::default());
    }
    let input: ObserveOptionsInput = serde_wasm_bindgen::from_value(options)
        .map_err(|e| type_error(&format!("invalid observe options: {e}")))?;
    input.to_core()
}

fn parse_station(station: JsValue) -> Result<GeodeticStationKm, JsValue> {
    let station: StationInput = serde_wasm_bindgen::from_value(station)
        .map_err(|e| type_error(&format!("invalid station: {e}")))?;
    Ok(station.to_core())
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

/// Observe `"sun"` or `"moon"` from a geodetic station at a UTC unix microsecond epoch.
#[wasm_bindgen(js_name = observe)]
pub fn observe(
    station: JsValue,
    epoch_unix_us: i64,
    target: &str,
    options: JsValue,
) -> Result<JsValue, JsValue> {
    let station = parse_station(station)?;
    let target = match target {
        "sun" => Target::Sun,
        "moon" => Target::Moon,
        other => {
            return Err(type_error(&format!(
                "invalid target {other:?}: expected \"sun\" or \"moon\""
            )))
        }
    };
    let observation = core_observe(
        &station,
        UtcInstant::from_unix_microseconds(epoch_unix_us),
        target,
        parse_options(options)?,
    )
    .map_err(engine_error)?;
    observation_to_js(observation)
}

/// Observe an SPK target body by NAIF id using default full-chain options.
#[wasm_bindgen(js_name = observeSpkBody)]
pub fn observe_spk_body(
    station: JsValue,
    epoch_unix_us: i64,
    spk: &Spk,
    naif_id: i32,
) -> Result<JsValue, JsValue> {
    let station = parse_station(station)?;
    let observation = core_observe_spk_body(
        &station,
        UtcInstant::from_unix_microseconds(epoch_unix_us),
        spk.core(),
        naif_id,
    )
    .map_err(engine_error)?;
    observation_to_js(observation)
}

/// Observe a caller-supplied SSB-centered target state using an SPK for Earth and Sun.
#[wasm_bindgen(js_name = observeBarycentricState)]
pub fn observe_barycentric_state(
    station: JsValue,
    epoch_unix_us: i64,
    spk: &Spk,
    position_km: &[f64],
    velocity_km_s: &[f64],
    options: JsValue,
) -> Result<JsValue, JsValue> {
    let station = parse_station(station)?;
    let position_km = vec3_finite("positionKm", position_km)?;
    let velocity_km_s = vec3_finite("velocityKmS", velocity_km_s)?;
    let observation = core_observe(
        &station,
        UtcInstant::from_unix_microseconds(epoch_unix_us),
        Target::BarycentricState {
            kernel: spk.core(),
            position_km,
            velocity_km_s,
        },
        parse_options(options)?,
    )
    .map_err(engine_error)?;
    observation_to_js(observation)
}
