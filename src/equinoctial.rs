use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::astro::elements::{ClassicalElements, OrbitType};
use sidereon_core::astro::equinoctial::{
    coe2eq as core_coe2eq, coe2mee as core_coe2mee, eq2coe as core_eq2coe, eq2rv as core_eq2rv,
    mee2coe as core_mee2coe, mee2rv as core_mee2rv, rv2eq as core_rv2eq, rv2mee as core_rv2mee,
    EquinoctialElements as CoreEq, ModifiedEquinoctialElements as CoreMee,
    RetrogradeFactor as CoreRetrogradeFactor,
};

use crate::error::{engine_error, type_error};
use crate::marshal::vec3_finite;

#[wasm_bindgen]
#[derive(Clone, Copy)]
pub enum RetrogradeFactor {
    Prograde,
    Retrograde,
}

impl From<RetrogradeFactor> for CoreRetrogradeFactor {
    fn from(value: RetrogradeFactor) -> Self {
        match value {
            RetrogradeFactor::Prograde => CoreRetrogradeFactor::Prograde,
            RetrogradeFactor::Retrograde => CoreRetrogradeFactor::Retrograde,
        }
    }
}

impl From<CoreRetrogradeFactor> for RetrogradeFactor {
    fn from(value: CoreRetrogradeFactor) -> Self {
        match value {
            CoreRetrogradeFactor::Prograde => RetrogradeFactor::Prograde,
            CoreRetrogradeFactor::Retrograde => RetrogradeFactor::Retrograde,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EquinoctialObject {
    a: f64,
    h: f64,
    k: f64,
    p: f64,
    q: f64,
    lambda: f64,
    retrograde: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModifiedEquinoctialObject {
    p: f64,
    f: f64,
    g: f64,
    h: f64,
    k: f64,
    l: f64,
    retrograde: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ElementsObject {
    p: f64,
    a: f64,
    ecc: f64,
    incl: f64,
    raan: f64,
    argp: f64,
    nu: f64,
    arglat: f64,
    truelon: f64,
    lonper: f64,
    orbit_type: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ElementsInput {
    p: f64,
    a: Option<f64>,
    ecc: f64,
    incl: f64,
    raan: f64,
    argp: f64,
    nu: f64,
    arglat: f64,
    truelon: f64,
    lonper: f64,
    orbit_type: String,
}

impl Default for ElementsInput {
    fn default() -> Self {
        Self {
            p: f64::NAN,
            a: None,
            ecc: f64::NAN,
            incl: f64::NAN,
            raan: f64::NAN,
            argp: f64::NAN,
            nu: f64::NAN,
            arglat: f64::NAN,
            truelon: f64::NAN,
            lonper: f64::NAN,
            orbit_type: "ellipticalInclined".to_string(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StateObject {
    position_km: [f64; 3],
    velocity_km_s: [f64; 3],
}

fn factor_or_default(value: Option<RetrogradeFactor>) -> CoreRetrogradeFactor {
    value.unwrap_or(RetrogradeFactor::Prograde).into()
}

fn factor_label(value: CoreRetrogradeFactor) -> &'static str {
    match value {
        CoreRetrogradeFactor::Prograde => "prograde",
        CoreRetrogradeFactor::Retrograde => "retrograde",
    }
}

fn parse_factor(value: &str) -> Result<CoreRetrogradeFactor, JsValue> {
    match value {
        "prograde" => Ok(CoreRetrogradeFactor::Prograde),
        "retrograde" => Ok(CoreRetrogradeFactor::Retrograde),
        other => Err(type_error(&format!(
            "unknown retrograde factor {other:?}; expected \"prograde\" or \"retrograde\""
        ))),
    }
}

fn orbit_type_label(orbit_type: OrbitType) -> &'static str {
    match orbit_type {
        OrbitType::EllipticalInclined => "ellipticalInclined",
        OrbitType::EllipticalEquatorial => "ellipticalEquatorial",
        OrbitType::CircularInclined => "circularInclined",
        OrbitType::CircularEquatorial => "circularEquatorial",
    }
}

fn parse_orbit_type(value: &str) -> Result<OrbitType, JsValue> {
    match value {
        "ellipticalInclined" => Ok(OrbitType::EllipticalInclined),
        "ellipticalEquatorial" => Ok(OrbitType::EllipticalEquatorial),
        "circularInclined" => Ok(OrbitType::CircularInclined),
        "circularEquatorial" => Ok(OrbitType::CircularEquatorial),
        other => Err(type_error(&format!("unknown orbitType {other:?}"))),
    }
}

fn coe_from_js(value: JsValue) -> Result<ClassicalElements, JsValue> {
    let input: ElementsInput = serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid ClassicalElements: {e}")))?;
    let mut coe = ClassicalElements::new(
        input.p, input.ecc, input.incl, input.raan, input.argp, input.nu,
    );
    if let Some(a) = input.a {
        coe.a = a;
    }
    coe.arglat = input.arglat;
    coe.truelon = input.truelon;
    coe.lonper = input.lonper;
    coe.orbit_type = parse_orbit_type(&input.orbit_type)?;
    Ok(coe)
}

fn coe_to_js(coe: ClassicalElements) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(&ElementsObject {
        p: coe.p,
        a: coe.a,
        ecc: coe.ecc,
        incl: coe.incl,
        raan: coe.raan,
        argp: coe.argp,
        nu: coe.nu,
        arglat: coe.arglat,
        truelon: coe.truelon,
        lonper: coe.lonper,
        orbit_type: orbit_type_label(coe.orbit_type).to_string(),
    })
    .map_err(|e| type_error(&e.to_string()))
}

fn eq_to_js(eq: CoreEq) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(&EquinoctialObject {
        a: eq.a,
        h: eq.h,
        k: eq.k,
        p: eq.p,
        q: eq.q,
        lambda: eq.lambda,
        retrograde: factor_label(eq.retrograde).to_string(),
    })
    .map_err(|e| type_error(&e.to_string()))
}

fn eq_from_js(value: JsValue) -> Result<CoreEq, JsValue> {
    let input: EquinoctialObject = serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid EquinoctialElements: {e}")))?;
    Ok(CoreEq {
        a: input.a,
        h: input.h,
        k: input.k,
        p: input.p,
        q: input.q,
        lambda: input.lambda,
        retrograde: parse_factor(&input.retrograde)?,
    })
}

fn mee_to_js(mee: CoreMee) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(&ModifiedEquinoctialObject {
        p: mee.p,
        f: mee.f,
        g: mee.g,
        h: mee.h,
        k: mee.k,
        l: mee.l,
        retrograde: factor_label(mee.retrograde).to_string(),
    })
    .map_err(|e| type_error(&e.to_string()))
}

fn mee_from_js(value: JsValue) -> Result<CoreMee, JsValue> {
    let input: ModifiedEquinoctialObject = serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid ModifiedEquinoctialElements: {e}")))?;
    Ok(CoreMee {
        p: input.p,
        f: input.f,
        g: input.g,
        h: input.h,
        k: input.k,
        l: input.l,
        retrograde: parse_factor(&input.retrograde)?,
    })
}

fn state_to_js(r: [f64; 3], v: [f64; 3]) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(&StateObject {
        position_km: r,
        velocity_km_s: v,
    })
    .map_err(|e| type_error(&e.to_string()))
}

#[wasm_bindgen(js_name = coe2eq)]
pub fn coe2eq(coe: JsValue, factor: Option<RetrogradeFactor>) -> Result<JsValue, JsValue> {
    let coe = coe_from_js(coe)?;
    eq_to_js(core_coe2eq(&coe, factor_or_default(factor)).map_err(engine_error)?)
}

#[wasm_bindgen(js_name = eq2coe)]
pub fn eq2coe(eq: JsValue) -> Result<JsValue, JsValue> {
    coe_to_js(core_eq2coe(&eq_from_js(eq)?).map_err(engine_error)?)
}

#[wasm_bindgen(js_name = coe2mee)]
pub fn coe2mee(coe: JsValue, factor: Option<RetrogradeFactor>) -> Result<JsValue, JsValue> {
    let coe = coe_from_js(coe)?;
    mee_to_js(core_coe2mee(&coe, factor_or_default(factor)).map_err(engine_error)?)
}

#[wasm_bindgen(js_name = mee2coe)]
pub fn mee2coe(mee: JsValue) -> Result<JsValue, JsValue> {
    coe_to_js(core_mee2coe(&mee_from_js(mee)?).map_err(engine_error)?)
}

#[wasm_bindgen(js_name = rv2eq)]
pub fn rv2eq(
    r: &[f64],
    v: &[f64],
    mu_km3_s2: f64,
    factor: Option<RetrogradeFactor>,
) -> Result<JsValue, JsValue> {
    let r = vec3_finite("r", r)?;
    let v = vec3_finite("v", v)?;
    eq_to_js(core_rv2eq(r, v, mu_km3_s2, factor_or_default(factor)).map_err(engine_error)?)
}

#[wasm_bindgen(js_name = eq2rv)]
pub fn eq2rv(eq: JsValue, mu_km3_s2: f64) -> Result<JsValue, JsValue> {
    let (r, v) = core_eq2rv(&eq_from_js(eq)?, mu_km3_s2).map_err(engine_error)?;
    state_to_js(r, v)
}

#[wasm_bindgen(js_name = rv2mee)]
pub fn rv2mee(
    r: &[f64],
    v: &[f64],
    mu_km3_s2: f64,
    factor: Option<RetrogradeFactor>,
) -> Result<JsValue, JsValue> {
    let r = vec3_finite("r", r)?;
    let v = vec3_finite("v", v)?;
    mee_to_js(core_rv2mee(r, v, mu_km3_s2, factor_or_default(factor)).map_err(engine_error)?)
}

#[wasm_bindgen(js_name = mee2rv)]
pub fn mee2rv(mee: JsValue, mu_km3_s2: f64) -> Result<JsValue, JsValue> {
    let (r, v) = core_mee2rv(&mee_from_js(mee)?, mu_km3_s2).map_err(engine_error)?;
    state_to_js(r, v)
}
