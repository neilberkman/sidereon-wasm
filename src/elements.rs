//! Classical orbital element conversions.
//!
//! Thin wrappers over `sidereon_core::astro::elements::{rv2coe, coe2rv}`. The
//! two-body Vallado algorithms live in the crate; this layer only reshapes the
//! position/velocity vectors, maps the orbit-type discriminant onto a stable JS
//! string, and marshals the `ClassicalElements` value object to and from an
//! idiomatic plain object. All lengths are kilometres, speeds kilometres per
//! second, and angles radians.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::astro::elements::{
    coe2rv as core_coe2rv, rv2coe as core_rv2coe, ClassicalElements, OrbitType,
};

use crate::error::{engine_error, type_error};
use crate::marshal::vec3_finite;

/// The element set crossing to JS as a plain object. Angles are radians; `a` and
/// `p` are kilometres. The auxiliary angles (`arglat`, `truelon`, `lonper`) and
/// the undefined primary angles are `NaN` per the orbit type, exactly as the
/// core value object reports them.
#[derive(Serialize, Deserialize)]
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

/// Input element set for `coe2rv`. The auxiliary angles and `orbitType` default
/// so an ordinary (elliptical inclined) orbit can be built from the six primary
/// elements alone; round-tripping a `rv2coe` result supplies the full object.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ElementsInput {
    p: f64,
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
        other => Err(type_error(&format!(
            "unknown orbitType {other:?}; expected \"ellipticalInclined\", \
             \"ellipticalEquatorial\", \"circularInclined\", or \"circularEquatorial\""
        ))),
    }
}

fn elements_to_object(coe: &ClassicalElements) -> Result<JsValue, JsValue> {
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

/// Convert an inertial Cartesian state to classical orbital elements.
///
/// `r` is the ECI position (km), `v` the ECI velocity (km/s), and `mu` the
/// gravitational parameter (km^3/s^2). Returns a `ClassicalElements` object;
/// undefined primary angles and the inapplicable auxiliary angles are `NaN` per
/// the orbit type. Delegates to `sidereon_core::astro::elements::rv2coe`. Throws
/// a `TypeError` for a wrong-length vector and an `Error` for a degenerate or
/// non-finite state.
#[wasm_bindgen(js_name = rv2coe)]
pub fn rv2coe(r: &[f64], v: &[f64], mu: f64) -> Result<JsValue, JsValue> {
    let r = vec3_finite("r", r)?;
    let v = vec3_finite("v", v)?;
    let coe = core_rv2coe(r, v, mu).map_err(engine_error)?;
    elements_to_object(&coe)
}

/// Convert classical orbital elements to an inertial Cartesian state.
///
/// `coe` is a `ClassicalElements` object (see `rv2coe`); only `p`, `ecc`,
/// `incl`, `raan`, `argp`, and `nu` are required for an ordinary orbit, with
/// `orbitType` and the auxiliary angles defaulting so the six primary elements
/// suffice. `mu` is the gravitational parameter (km^3/s^2). Returns
/// `{ positionKm, velocityKmS }`, each a length-3 `Float64Array`. Delegates to
/// `sidereon_core::astro::elements::coe2rv`.
#[wasm_bindgen(js_name = coe2rv)]
pub fn coe2rv(coe: JsValue, mu: f64) -> Result<JsValue, JsValue> {
    let input: ElementsInput = serde_wasm_bindgen::from_value(coe)
        .map_err(|e| type_error(&format!("invalid ClassicalElements: {e}")))?;
    let orbit_type = parse_orbit_type(&input.orbit_type)?;
    let mut elements = ClassicalElements::new(
        input.p, input.ecc, input.incl, input.raan, input.argp, input.nu,
    );
    elements.arglat = input.arglat;
    elements.truelon = input.truelon;
    elements.lonper = input.lonper;
    elements.orbit_type = orbit_type;

    let (r, v) = core_coe2rv(&elements, mu).map_err(engine_error)?;

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct State {
        position_km: Vec<f64>,
        velocity_km_s: Vec<f64>,
    }
    serde_wasm_bindgen::to_value(&State {
        position_km: r.to_vec(),
        velocity_km_s: v.to_vec(),
    })
    .map_err(|e| type_error(&e.to_string()))
}
