use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::astro::anomaly::{
    eccentric_to_mean as core_eccentric_to_mean, eccentric_to_true as core_eccentric_to_true,
    mean_to_eccentric as core_mean_to_eccentric, mean_to_true as core_mean_to_true,
    propagate_kepler as core_propagate_kepler, solve_kepler as core_solve_kepler,
    true_to_eccentric as core_true_to_eccentric, true_to_mean as core_true_to_mean,
};
use sidereon_core::astro::elements::{ClassicalElements, OrbitType};

use crate::error::{engine_error, type_error};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KeplerSolutionJs {
    anomaly: f64,
    iterations: usize,
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
    let object = ElementsObject {
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
    };
    serde_wasm_bindgen::to_value(&object).map_err(|e| type_error(&e.to_string()))
}

#[wasm_bindgen(js_name = solveKepler)]
pub fn solve_kepler(mean_anomaly_rad: f64, eccentricity: f64) -> Result<JsValue, JsValue> {
    let solution = core_solve_kepler(mean_anomaly_rad, eccentricity).map_err(engine_error)?;
    serde_wasm_bindgen::to_value(&KeplerSolutionJs {
        anomaly: solution.anomaly,
        iterations: solution.iterations,
    })
    .map_err(|e| type_error(&e.to_string()))
}

#[wasm_bindgen(js_name = meanToEccentric)]
pub fn mean_to_eccentric(mean_anomaly_rad: f64, eccentricity: f64) -> Result<f64, JsValue> {
    core_mean_to_eccentric(mean_anomaly_rad, eccentricity).map_err(engine_error)
}

#[wasm_bindgen(js_name = eccentricToMean)]
pub fn eccentric_to_mean(eccentric_anomaly_rad: f64, eccentricity: f64) -> Result<f64, JsValue> {
    core_eccentric_to_mean(eccentric_anomaly_rad, eccentricity).map_err(engine_error)
}

#[wasm_bindgen(js_name = eccentricToTrue)]
pub fn eccentric_to_true(eccentric_anomaly_rad: f64, eccentricity: f64) -> Result<f64, JsValue> {
    core_eccentric_to_true(eccentric_anomaly_rad, eccentricity).map_err(engine_error)
}

#[wasm_bindgen(js_name = trueToEccentric)]
pub fn true_to_eccentric(true_anomaly_rad: f64, eccentricity: f64) -> Result<f64, JsValue> {
    core_true_to_eccentric(true_anomaly_rad, eccentricity).map_err(engine_error)
}

#[wasm_bindgen(js_name = meanToTrue)]
pub fn mean_to_true(mean_anomaly_rad: f64, eccentricity: f64) -> Result<f64, JsValue> {
    core_mean_to_true(mean_anomaly_rad, eccentricity).map_err(engine_error)
}

#[wasm_bindgen(js_name = trueToMean)]
pub fn true_to_mean(true_anomaly_rad: f64, eccentricity: f64) -> Result<f64, JsValue> {
    core_true_to_mean(true_anomaly_rad, eccentricity).map_err(engine_error)
}

#[wasm_bindgen(js_name = propagateKepler)]
pub fn propagate_kepler(coe: JsValue, mu_km3_s2: f64, dt_s: f64) -> Result<JsValue, JsValue> {
    let coe = coe_from_js(coe)?;
    let propagated = core_propagate_kepler(&coe, mu_km3_s2, dt_s).map_err(engine_error)?;
    coe_to_js(propagated)
}
