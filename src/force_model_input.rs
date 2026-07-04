//! Shared decoding for numerical force-model request objects.
//!
//! This module contains no exported WASM surface. It turns the binding's serde
//! request payloads into `sidereon-core` propagator and force-model types so
//! propagation and orbit determination accept the same selectors.

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use sidereon_core::astro::constants::{J2_EARTH, MU_EARTH, RE_EARTH};
use sidereon_core::astro::forces::{
    DragForce as CoreDragForce, DragParameters, SchwarzschildRelativity, SolarRadiationPressure,
    SpaceWeather, ThirdBodyBodies, ThirdBodyGravity, ZonalCoefficients, ZonalDegrees, ZonalGravity,
};
use sidereon_core::astro::propagator::{
    ForceModelComponents, ForceModelKind, IntegratorKind, IntegratorOptions,
};

use crate::error::{engine_error, type_error};

#[derive(Clone, Deserialize)]
#[serde(untagged)]
pub(crate) enum ForceModelInput {
    Label(String),
    Object(ForceModelObject),
}

#[derive(Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct ForceModelObject {
    kind: Option<String>,
    two_body: Option<bool>,
    two_body_mu_km3_s2: Option<f64>,
    mu_km3_s2: Option<f64>,
    re_km: Option<f64>,
    j2: Option<f64>,
    zonal: Option<ComponentInput<ZonalInput>>,
    third_body: Option<ComponentInput<ThirdBodyInput>>,
    solar_radiation_pressure: Option<ComponentInput<SolarRadiationPressureInput>>,
    srp: Option<ComponentInput<SolarRadiationPressureInput>>,
    relativity: Option<ComponentInput<RelativityInput>>,
}

#[derive(Clone, Deserialize)]
#[serde(untagged)]
enum ComponentInput<T> {
    Enabled(bool),
    Label(String),
    Object(T),
}

#[derive(Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct SolarRadiationPressureInput {
    cr: Option<f64>,
    area_to_mass_m2_kg: Option<f64>,
    area_m2: Option<f64>,
    mass_kg: Option<f64>,
    pressure_n_m2: Option<f64>,
    au_km: Option<f64>,
}

#[derive(Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ZonalInput {
    max_degree: Option<u8>,
    j2: Option<bool>,
    j3: Option<bool>,
    j4: Option<bool>,
    j5: Option<bool>,
    j6: Option<bool>,
    mu_km3_s2: Option<f64>,
    re_km: Option<f64>,
    coefficients: Option<ZonalCoefficientsInput>,
}

#[derive(Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ZonalCoefficientsInput {
    j2: Option<f64>,
    j3: Option<f64>,
    j4: Option<f64>,
    j5: Option<f64>,
    j6: Option<f64>,
}

#[derive(Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ThirdBodyInput {
    sun: Option<bool>,
    moon: Option<bool>,
    gm_sun_km3_s2: Option<f64>,
    gm_moon_km3_s2: Option<f64>,
}

#[derive(Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct RelativityInput {
    mu_km3_s2: Option<f64>,
    c_km_s: Option<f64>,
}

#[derive(Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct SpaceWeatherInput {
    f107: Option<f64>,
    f107a: Option<f64>,
    ap: Option<f64>,
}

impl SpaceWeatherInput {
    pub(crate) fn to_core(self) -> SpaceWeather {
        let defaults = SpaceWeather::default();
        SpaceWeather {
            f107: self.f107.unwrap_or(defaults.f107),
            f107a: self.f107a.unwrap_or(defaults.f107a),
            ap: self.ap.unwrap_or(defaults.ap),
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DragInput {
    #[serde(default)]
    bc_factor_m2_kg: Option<f64>,
    #[serde(default)]
    ballistic_coefficient_kg_m2: Option<f64>,
    #[serde(default)]
    cd: Option<f64>,
    #[serde(default)]
    area_m2: Option<f64>,
    #[serde(default)]
    mass_kg: Option<f64>,
    #[serde(default)]
    cutoff_altitude_km: Option<f64>,
    #[serde(default)]
    space_weather: SpaceWeatherInput,
}

impl DragInput {
    pub(crate) fn to_core(&self) -> Result<DragParameters, JsValue> {
        let cutoff = self
            .cutoff_altitude_km
            .unwrap_or(CoreDragForce::DEFAULT_REENTRY_ALTITUDE_KM);
        let sw = self.space_weather.to_core();
        if let Some(bc_factor) = self.bc_factor_m2_kg {
            return DragParameters::from_bc_factor_m2_kg(bc_factor, sw, cutoff)
                .map_err(engine_error);
        }
        if let Some(bc) = self.ballistic_coefficient_kg_m2 {
            return DragParameters::from_ballistic_coefficient(bc, sw, cutoff)
                .map_err(engine_error);
        }
        match (self.cd, self.area_m2, self.mass_kg) {
            (Some(cd), Some(area_m2), Some(mass_kg)) => {
                DragParameters::from_area_mass(cd, area_m2, mass_kg, sw, cutoff)
                    .map_err(engine_error)
            }
            _ => Err(type_error(
                "drag requires bcFactorM2Kg, ballisticCoefficientKgM2, or cd/areaM2/massKg",
            )),
        }
    }
}

#[derive(Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct IntegratorOptionsInput {
    pub(crate) abs_tol: Option<f64>,
    pub(crate) rel_tol: Option<f64>,
    pub(crate) initial_step_s: Option<f64>,
    pub(crate) min_step_s: Option<f64>,
    pub(crate) max_step_s: Option<f64>,
    pub(crate) max_steps: Option<u32>,
}

impl IntegratorOptionsInput {
    pub(crate) fn to_core(self) -> IntegratorOptions {
        let defaults = IntegratorOptions::default();
        IntegratorOptions {
            abs_tol: self.abs_tol.unwrap_or(defaults.abs_tol),
            rel_tol: self.rel_tol.unwrap_or(defaults.rel_tol),
            initial_step: self.initial_step_s.unwrap_or(defaults.initial_step),
            min_step: self.min_step_s.unwrap_or(defaults.min_step),
            max_step: self.max_step_s.unwrap_or(defaults.max_step),
            max_steps: self.max_steps.unwrap_or(defaults.max_steps),
            dense_output: false,
        }
    }
}

pub(crate) fn integrator_kind(label: Option<&str>) -> Result<IntegratorKind, JsValue> {
    match label.unwrap_or("dp54") {
        "dp54" => Ok(IntegratorKind::Dp54),
        "rk4" => Ok(IntegratorKind::Rk4),
        other => Err(type_error(&format!(
            "invalid integrator {other:?}: expected \"dp54\" or \"rk4\""
        ))),
    }
}

pub(crate) fn force_model_kind(
    input: Option<&ForceModelInput>,
    mu_override_km3_s2: Option<f64>,
) -> Result<ForceModelKind, JsValue> {
    match input {
        None => Ok(ForceModelKind::TwoBody {
            mu_km3_s2: mu_override_km3_s2.unwrap_or(MU_EARTH),
        }),
        Some(ForceModelInput::Label(label)) => force_model_label(label, mu_override_km3_s2),
        Some(ForceModelInput::Object(object)) => force_model_object(object, mu_override_km3_s2),
    }
}

fn force_model_label(
    label: &str,
    mu_override_km3_s2: Option<f64>,
) -> Result<ForceModelKind, JsValue> {
    let mu = mu_override_km3_s2.unwrap_or(MU_EARTH);
    match label {
        "two_body" => Ok(ForceModelKind::TwoBody { mu_km3_s2: mu }),
        "two_body_j2" => Ok(ForceModelKind::TwoBodyJ2 {
            mu_km3_s2: mu,
            re_km: RE_EARTH,
            j2: J2_EARTH,
        }),
        "earth_phase_a" | "earthPhaseA" => {
            Ok(ForceModelKind::earth_phase_a(None))
        }
        "composite" => Ok(ForceModelKind::composite(
            ForceModelComponents::earth_two_body().with_two_body_mu(mu),
        )),
        other => Err(type_error(&format!(
            "invalid forceModel {other:?}: expected \"two_body\", \"two_body_j2\", \"composite\", or \"earth_phase_a\""
        ))),
    }
}

fn force_model_object(
    object: &ForceModelObject,
    mu_override_km3_s2: Option<f64>,
) -> Result<ForceModelKind, JsValue> {
    let kind = object.kind.as_deref().unwrap_or("composite");
    let mu = object
        .two_body_mu_km3_s2
        .or(object.mu_km3_s2)
        .or(mu_override_km3_s2)
        .unwrap_or(MU_EARTH);
    match kind {
        "two_body" => Ok(ForceModelKind::TwoBody { mu_km3_s2: mu }),
        "two_body_j2" => Ok(ForceModelKind::TwoBodyJ2 {
            mu_km3_s2: mu,
            re_km: object.re_km.unwrap_or(RE_EARTH),
            j2: object.j2.unwrap_or(J2_EARTH),
        }),
        "earth_phase_a" | "earthPhaseA" => {
            let srp = match object
                .solar_radiation_pressure
                .as_ref()
                .or(object.srp.as_ref())
            {
                None => None,
                Some(input) => component_srp(input)?,
            };
            Ok(ForceModelKind::earth_phase_a(srp))
        }
        "composite" => composite_object(object, mu),
        other => Err(type_error(&format!(
            "invalid forceModel.kind {other:?}: expected \"two_body\", \"two_body_j2\", \"composite\", or \"earth_phase_a\""
        ))),
    }
}

fn composite_object(object: &ForceModelObject, mu: f64) -> Result<ForceModelKind, JsValue> {
    let include_two_body = object.two_body.unwrap_or(true);
    let zonal = object.zonal.as_ref().map(component_zonal).transpose()?;
    let third_body = object
        .third_body
        .as_ref()
        .map(component_third_body)
        .transpose()?
        .flatten();
    let srp = object
        .solar_radiation_pressure
        .as_ref()
        .or(object.srp.as_ref())
        .map(component_srp)
        .transpose()?
        .flatten();
    let relativity = object
        .relativity
        .as_ref()
        .map(component_relativity)
        .transpose()?
        .flatten();

    if third_body.is_none() && srp.is_none() && relativity.is_none() {
        if include_two_body && zonal.is_none() {
            return Ok(ForceModelKind::TwoBody { mu_km3_s2: mu });
        }
        if include_two_body && zonal.as_ref().is_some_and(|z| is_default_j2_only(z, mu)) {
            let zonal = zonal.expect("checked");
            return Ok(ForceModelKind::TwoBodyJ2 {
                mu_km3_s2: mu,
                re_km: zonal.re_km,
                j2: zonal.coefficients.j2,
            });
        }
    }

    let mut components = ForceModelComponents::EMPTY;
    if include_two_body {
        components = components.with_two_body_mu(mu);
    }
    if let Some(zonal) = zonal {
        components = components.with_zonal(zonal);
    }
    if let Some(third_body) = third_body {
        components = components.with_third_body(third_body);
    }
    if let Some(srp) = srp {
        components = components.with_solar_radiation_pressure(srp);
    }
    if let Some(relativity) = relativity {
        components = components.with_relativity(relativity);
    }
    Ok(ForceModelKind::composite(components))
}

fn is_default_j2_only(zonal: &ZonalGravity, mu: f64) -> bool {
    zonal.degrees == ZonalDegrees::J2_ONLY
        && zonal.mu_km3_s2.to_bits() == mu.to_bits()
        && zonal.re_km.to_bits() == RE_EARTH.to_bits()
        && zonal.coefficients.j2.to_bits() == J2_EARTH.to_bits()
}

fn component_zonal(input: &ComponentInput<ZonalInput>) -> Result<ZonalGravity, JsValue> {
    match input {
        ComponentInput::Enabled(false) => Ok(ZonalGravity {
            degrees: ZonalDegrees::NONE,
            ..ZonalGravity::default()
        }),
        ComponentInput::Enabled(true) => Ok(ZonalGravity::earth_j2_through_j6()),
        ComponentInput::Label(label) => match label.as_str() {
            "none" => Ok(ZonalGravity {
                degrees: ZonalDegrees::NONE,
                ..ZonalGravity::default()
            }),
            "j2" | "J2" => Ok(ZonalGravity {
                degrees: ZonalDegrees::J2_ONLY,
                ..ZonalGravity::default()
            }),
            "j2_j6" | "j2ThroughJ6" | "J2ThroughJ6" => Ok(ZonalGravity::earth_j2_through_j6()),
            other => Err(type_error(&format!(
                "invalid zonal selector {other:?}: expected \"none\", \"j2\", or \"j2_j6\""
            ))),
        },
        ComponentInput::Object(input) => zonal_from_object(input),
    }
}

fn zonal_from_object(input: &ZonalInput) -> Result<ZonalGravity, JsValue> {
    let degrees = if input.j2.is_some()
        || input.j3.is_some()
        || input.j4.is_some()
        || input.j5.is_some()
        || input.j6.is_some()
    {
        ZonalDegrees {
            j2: input.j2.unwrap_or(false),
            j3: input.j3.unwrap_or(false),
            j4: input.j4.unwrap_or(false),
            j5: input.j5.unwrap_or(false),
            j6: input.j6.unwrap_or(false),
        }
    } else if let Some(max_degree) = input.max_degree {
        ZonalDegrees::through(max_degree).map_err(engine_error)?
    } else {
        ZonalDegrees::J2_THROUGH_J6
    };
    let defaults = ZonalCoefficients::default();
    let coefficients =
        input
            .coefficients
            .clone()
            .map_or(defaults, |coefficients| ZonalCoefficients {
                j2: coefficients.j2.unwrap_or(defaults.j2),
                j3: coefficients.j3.unwrap_or(defaults.j3),
                j4: coefficients.j4.unwrap_or(defaults.j4),
                j5: coefficients.j5.unwrap_or(defaults.j5),
                j6: coefficients.j6.unwrap_or(defaults.j6),
            });
    Ok(ZonalGravity::new(
        input.mu_km3_s2.unwrap_or(MU_EARTH),
        input.re_km.unwrap_or(RE_EARTH),
        degrees,
        coefficients,
    ))
}

fn component_third_body(
    input: &ComponentInput<ThirdBodyInput>,
) -> Result<Option<ThirdBodyGravity>, JsValue> {
    match input {
        ComponentInput::Enabled(false) => Ok(None),
        ComponentInput::Enabled(true) => Ok(Some(ThirdBodyGravity::default())),
        ComponentInput::Label(label) => match label.as_str() {
            "none" => Ok(None),
            "sun" => Ok(Some(ThirdBodyGravity::sun())),
            "moon" => Ok(Some(ThirdBodyGravity::moon())),
            "sun_moon" | "sunMoon" | "sun_and_moon" => Ok(Some(ThirdBodyGravity::default())),
            other => Err(type_error(&format!(
                "invalid thirdBody selector {other:?}: expected \"none\", \"sun\", \"moon\", or \"sun_moon\""
            ))),
        },
        ComponentInput::Object(input) => {
            let defaults = ThirdBodyGravity::default();
            let bodies = ThirdBodyBodies {
                sun: input.sun.unwrap_or(true),
                moon: input.moon.unwrap_or(true),
            };
            if !bodies.sun && !bodies.moon {
                return Ok(None);
            }
            Ok(Some(ThirdBodyGravity::new(
                bodies,
                input.gm_sun_km3_s2.unwrap_or(defaults.gm_sun_km3_s2),
                input.gm_moon_km3_s2.unwrap_or(defaults.gm_moon_km3_s2),
            )))
        }
    }
}

fn component_srp(
    input: &ComponentInput<SolarRadiationPressureInput>,
) -> Result<Option<SolarRadiationPressure>, JsValue> {
    match input {
        ComponentInput::Enabled(false) => Ok(None),
        ComponentInput::Enabled(true) => Err(type_error(
            "solarRadiationPressure requires an object with cr and areaToMassM2Kg or areaM2/massKg",
        )),
        ComponentInput::Label(label) if label == "none" => Ok(None),
        ComponentInput::Label(label) => Err(type_error(&format!(
            "invalid solarRadiationPressure selector {label:?}: expected \"none\" or an object"
        ))),
        ComponentInput::Object(input) => Ok(Some(srp_from_object(input)?)),
    }
}

pub(crate) fn srp_from_object(
    input: &SolarRadiationPressureInput,
) -> Result<SolarRadiationPressure, JsValue> {
    let cr = input
        .cr
        .ok_or_else(|| type_error("solarRadiationPressure requires cr"))?;
    let area_to_mass = match (input.area_to_mass_m2_kg, input.area_m2, input.mass_kg) {
        (Some(value), _, _) => value,
        (None, Some(area_m2), Some(mass_kg)) => area_m2 / mass_kg,
        _ => {
            return Err(type_error(
                "solarRadiationPressure requires areaToMassM2Kg or areaM2/massKg",
            ));
        }
    };
    match (input.pressure_n_m2, input.au_km) {
        (None, None) => SolarRadiationPressure::new(cr, area_to_mass).map_err(engine_error),
        _ => SolarRadiationPressure::with_pressure(
            cr,
            area_to_mass,
            input.pressure_n_m2.unwrap_or(4.56e-6),
            input
                .au_km
                .unwrap_or(sidereon_core::astro::constants::astro::AU_KM),
        )
        .map_err(engine_error),
    }
}

fn component_relativity(
    input: &ComponentInput<RelativityInput>,
) -> Result<Option<SchwarzschildRelativity>, JsValue> {
    match input {
        ComponentInput::Enabled(false) => Ok(None),
        ComponentInput::Enabled(true) => Ok(Some(SchwarzschildRelativity::default())),
        ComponentInput::Label(label) if label == "none" => Ok(None),
        ComponentInput::Label(label) if label == "schwarzschild" => {
            Ok(Some(SchwarzschildRelativity::default()))
        }
        ComponentInput::Label(label) => Err(type_error(&format!(
            "invalid relativity selector {label:?}: expected \"none\" or \"schwarzschild\""
        ))),
        ComponentInput::Object(input) => {
            let defaults = SchwarzschildRelativity::default();
            Ok(Some(SchwarzschildRelativity::new(
                input.mu_km3_s2.unwrap_or(defaults.mu_km3_s2),
                input.c_km_s.unwrap_or(defaults.c_km_s),
            )))
        }
    }
}
