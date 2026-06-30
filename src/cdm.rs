//! CCSDS CDM binding: typed value objects for the core `CdmKvn` / `CdmObject`
//! plus KVN/XML parse and encode. The grammar and serialization live entirely in
//! `sidereon_core::astro::cdm`; this module marshals strings, optional fields,
//! and flat `Float64Array` vectors.

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use sidereon_core::astro::cdm::{
    encode_kvn, encode_xml, parse_kvn, parse_xml, CdmKvn, CdmObject as CoreCdmObject,
};

use crate::error::{engine_error, type_error};
use crate::marshal::vec3;

fn vec6(name: &str, values: &[f64]) -> Result<[f64; 6], JsValue> {
    if values.len() != 6 {
        return Err(type_error(&format!(
            "{name} must have length 6, got {}",
            values.len()
        )));
    }
    Ok([
        values[0], values[1], values[2], values[3], values[4], values[5],
    ])
}

fn vec15(name: &str, values: &[f64]) -> Result<[f64; 15], JsValue> {
    let array: [f64; 15] = values
        .try_into()
        .map_err(|_| type_error(&format!("{name} must have length 15, got {}", values.len())))?;
    Ok(array)
}

/// Optional CDM object metadata: the full CCSDS 508.0-B-1 metadata block plus the
/// optional RTN velocity-covariance rows. Every string field is the verbatim
/// textual value and absent fields are `None` (not emitted on encode).
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct CdmObjectMeta {
    object_designator: Option<String>,
    catalog_name: Option<String>,
    object_name: Option<String>,
    international_designator: Option<String>,
    object_type: Option<String>,
    operator_contact_position: Option<String>,
    operator_organization: Option<String>,
    operator_phone: Option<String>,
    operator_email: Option<String>,
    ephemeris_name: Option<String>,
    covariance_method: Option<String>,
    maneuverable: Option<String>,
    orbit_center: Option<String>,
    ref_frame: Option<String>,
    gravity_model: Option<String>,
    atmospheric_model: Option<String>,
    n_body_perturbations: Option<String>,
    solar_rad_pressure: Option<String>,
    earth_tides: Option<String>,
    intrack_thrust: Option<String>,
    velocity_covariance_rtn: Option<Vec<f64>>,
}

/// Optional CDM message-level fields.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct CdmMeta {
    creation_date: Option<String>,
    originator: Option<String>,
    message_id: Option<String>,
    tca: Option<String>,
    miss_distance_m: Option<f64>,
    relative_speed_m_s: Option<f64>,
    collision_probability: Option<f64>,
    collision_probability_method: Option<String>,
    hard_body_radius_m: Option<f64>,
}

fn parse_meta<T: Default + for<'de> Deserialize<'de>>(
    value: JsValue,
    label: &str,
) -> Result<T, JsValue> {
    if value.is_undefined() || value.is_null() {
        Ok(T::default())
    } else {
        serde_wasm_bindgen::from_value(value)
            .map_err(|e| type_error(&format!("invalid {label}: {e}")))
    }
}

/// One object's metadata, state vector, and RTN position covariance from a CDM.
#[wasm_bindgen]
#[derive(Clone)]
pub struct CdmObject {
    inner: CoreCdmObject,
}

#[wasm_bindgen]
impl CdmObject {
    /// Build a CDM object. `positionKm` / `velocityKmS` are length-3
    /// `Float64Array`s; `covarianceRtn` is the length-6 RTN position lower triangle
    /// `[CR_R, CT_R, CT_T, CN_R, CN_T, CN_N]`. `meta` carries the optional CCSDS
    /// metadata block (`objectDesignator`, `catalogName`, `objectName`,
    /// `internationalDesignator`, `objectType`, `operatorContactPosition`,
    /// `operatorOrganization`, `operatorPhone`, `operatorEmail`, `ephemerisName`,
    /// `covarianceMethod`, `maneuverable`, `orbitCenter`, `refFrame`,
    /// `gravityModel`, `atmosphericModel`, `nBodyPerturbations`,
    /// `solarRadPressure`, `earthTides`, `intrackThrust`) and the optional
    /// `velocityCovarianceRtn`, a length-15 `Float64Array` of the RTN
    /// velocity-covariance rows that complete the 6x6 matrix.
    #[wasm_bindgen(constructor)]
    pub fn new(
        position_km: &[f64],
        velocity_km_s: &[f64],
        covariance_rtn: &[f64],
        meta: JsValue,
    ) -> Result<CdmObject, JsValue> {
        let p = vec3("positionKm", position_km)?;
        let v = vec3("velocityKmS", velocity_km_s)?;
        let cov = vec6("covarianceRtn", covariance_rtn)?;
        let m: CdmObjectMeta = parse_meta(meta, "CdmObject meta")?;
        let velocity_covariance_rtn = match &m.velocity_covariance_rtn {
            Some(values) => Some(vec15("velocityCovarianceRtn", values)?),
            None => None,
        };
        Ok(CdmObject {
            inner: CoreCdmObject {
                object_designator: m.object_designator,
                catalog_name: m.catalog_name,
                object_name: m.object_name,
                international_designator: m.international_designator,
                object_type: m.object_type,
                operator_contact_position: m.operator_contact_position,
                operator_organization: m.operator_organization,
                operator_phone: m.operator_phone,
                operator_email: m.operator_email,
                ephemeris_name: m.ephemeris_name,
                covariance_method: m.covariance_method,
                maneuverable: m.maneuverable,
                orbit_center: m.orbit_center,
                ref_frame: m.ref_frame,
                gravity_model: m.gravity_model,
                atmospheric_model: m.atmospheric_model,
                n_body_perturbations: m.n_body_perturbations,
                solar_rad_pressure: m.solar_rad_pressure,
                earth_tides: m.earth_tides,
                intrack_thrust: m.intrack_thrust,
                state: ((p[0], p[1], p[2]), (v[0], v[1], v[2])),
                covariance_rtn: cov,
                velocity_covariance_rtn,
            },
        })
    }

    /// Object designator.
    #[wasm_bindgen(getter, js_name = objectDesignator)]
    pub fn object_designator(&self) -> Option<String> {
        self.inner.object_designator.clone()
    }

    /// Catalog name.
    #[wasm_bindgen(getter, js_name = catalogName)]
    pub fn catalog_name(&self) -> Option<String> {
        self.inner.catalog_name.clone()
    }

    /// Object name.
    #[wasm_bindgen(getter, js_name = objectName)]
    pub fn object_name(&self) -> Option<String> {
        self.inner.object_name.clone()
    }

    /// International designator (COSPAR ID).
    #[wasm_bindgen(getter, js_name = internationalDesignator)]
    pub fn international_designator(&self) -> Option<String> {
        self.inner.international_designator.clone()
    }

    /// Object type.
    #[wasm_bindgen(getter, js_name = objectType)]
    pub fn object_type(&self) -> Option<String> {
        self.inner.object_type.clone()
    }

    /// Operator contact position.
    #[wasm_bindgen(getter, js_name = operatorContactPosition)]
    pub fn operator_contact_position(&self) -> Option<String> {
        self.inner.operator_contact_position.clone()
    }

    /// Operator organization.
    #[wasm_bindgen(getter, js_name = operatorOrganization)]
    pub fn operator_organization(&self) -> Option<String> {
        self.inner.operator_organization.clone()
    }

    /// Operator phone.
    #[wasm_bindgen(getter, js_name = operatorPhone)]
    pub fn operator_phone(&self) -> Option<String> {
        self.inner.operator_phone.clone()
    }

    /// Operator email.
    #[wasm_bindgen(getter, js_name = operatorEmail)]
    pub fn operator_email(&self) -> Option<String> {
        self.inner.operator_email.clone()
    }

    /// Ephemeris name.
    #[wasm_bindgen(getter, js_name = ephemerisName)]
    pub fn ephemeris_name(&self) -> Option<String> {
        self.inner.ephemeris_name.clone()
    }

    /// Covariance method.
    #[wasm_bindgen(getter, js_name = covarianceMethod)]
    pub fn covariance_method(&self) -> Option<String> {
        self.inner.covariance_method.clone()
    }

    /// Maneuverability indicator.
    #[wasm_bindgen(getter)]
    pub fn maneuverable(&self) -> Option<String> {
        self.inner.maneuverable.clone()
    }

    /// Orbit center.
    #[wasm_bindgen(getter, js_name = orbitCenter)]
    pub fn orbit_center(&self) -> Option<String> {
        self.inner.orbit_center.clone()
    }

    /// Reference frame.
    #[wasm_bindgen(getter, js_name = refFrame)]
    pub fn ref_frame(&self) -> Option<String> {
        self.inner.ref_frame.clone()
    }

    /// Gravity model.
    #[wasm_bindgen(getter, js_name = gravityModel)]
    pub fn gravity_model(&self) -> Option<String> {
        self.inner.gravity_model.clone()
    }

    /// Atmospheric model.
    #[wasm_bindgen(getter, js_name = atmosphericModel)]
    pub fn atmospheric_model(&self) -> Option<String> {
        self.inner.atmospheric_model.clone()
    }

    /// N-body perturbations indicator.
    #[wasm_bindgen(getter, js_name = nBodyPerturbations)]
    pub fn n_body_perturbations(&self) -> Option<String> {
        self.inner.n_body_perturbations.clone()
    }

    /// Solar-radiation-pressure indicator.
    #[wasm_bindgen(getter, js_name = solarRadPressure)]
    pub fn solar_rad_pressure(&self) -> Option<String> {
        self.inner.solar_rad_pressure.clone()
    }

    /// Earth-tides indicator.
    #[wasm_bindgen(getter, js_name = earthTides)]
    pub fn earth_tides(&self) -> Option<String> {
        self.inner.earth_tides.clone()
    }

    /// In-track-thrust indicator.
    #[wasm_bindgen(getter, js_name = intrackThrust)]
    pub fn intrack_thrust(&self) -> Option<String> {
        self.inner.intrack_thrust.clone()
    }

    /// Position vector, kilometres, length-3 `Float64Array`.
    #[wasm_bindgen(getter, js_name = positionKm)]
    pub fn position_km(&self) -> Vec<f64> {
        let ((x, y, z), _) = self.inner.state;
        vec![x, y, z]
    }

    /// Velocity vector, km/s, length-3 `Float64Array`.
    #[wasm_bindgen(getter, js_name = velocityKmS)]
    pub fn velocity_km_s(&self) -> Vec<f64> {
        let (_, (vx, vy, vz)) = self.inner.state;
        vec![vx, vy, vz]
    }

    /// RTN position-covariance lower triangle, length-6 `Float64Array`.
    #[wasm_bindgen(getter, js_name = covarianceRtn)]
    pub fn covariance_rtn(&self) -> Vec<f64> {
        self.inner.covariance_rtn.to_vec()
    }

    /// RTN velocity-covariance rows completing the 6x6 matrix, a length-15
    /// `Float64Array` in CCSDS order (`CRDOT_R` .. `CNDOT_NDOT`), or `undefined`
    /// when the producer carried only the position covariance block.
    #[wasm_bindgen(getter, js_name = velocityCovarianceRtn)]
    pub fn velocity_covariance_rtn(&self) -> Option<Vec<f64>> {
        self.inner.velocity_covariance_rtn.map(|v| v.to_vec())
    }
}

/// A two-object CCSDS Conjunction Data Message parsed from KVN or XML.
#[wasm_bindgen]
#[derive(Clone)]
pub struct Cdm {
    inner: CdmKvn,
}

#[wasm_bindgen]
impl Cdm {
    /// Build a CDM from two objects. `meta` carries the optional message-level
    /// fields.
    #[wasm_bindgen(constructor)]
    pub fn new(object1: &CdmObject, object2: &CdmObject, meta: JsValue) -> Result<Cdm, JsValue> {
        let m: CdmMeta = parse_meta(meta, "Cdm meta")?;
        Ok(Cdm {
            inner: CdmKvn {
                creation_date: m.creation_date,
                originator: m.originator,
                message_id: m.message_id,
                tca: m.tca,
                miss_distance_m: m.miss_distance_m,
                relative_speed_m_s: m.relative_speed_m_s,
                collision_probability: m.collision_probability,
                collision_probability_method: m.collision_probability_method,
                hard_body_radius_m: m.hard_body_radius_m,
                object1: object1.inner.clone(),
                object2: object2.inner.clone(),
            },
        })
    }

    /// Creation date.
    #[wasm_bindgen(getter, js_name = creationDate)]
    pub fn creation_date(&self) -> Option<String> {
        self.inner.creation_date.clone()
    }

    /// Originator.
    #[wasm_bindgen(getter)]
    pub fn originator(&self) -> Option<String> {
        self.inner.originator.clone()
    }

    /// Message id.
    #[wasm_bindgen(getter, js_name = messageId)]
    pub fn message_id(&self) -> Option<String> {
        self.inner.message_id.clone()
    }

    /// Time of closest approach (TCA).
    #[wasm_bindgen(getter)]
    pub fn tca(&self) -> Option<String> {
        self.inner.tca.clone()
    }

    /// Miss distance, metres.
    #[wasm_bindgen(getter, js_name = missDistanceM)]
    pub fn miss_distance_m(&self) -> Option<f64> {
        self.inner.miss_distance_m
    }

    /// Relative speed, m/s.
    #[wasm_bindgen(getter, js_name = relativeSpeedMS)]
    pub fn relative_speed_m_s(&self) -> Option<f64> {
        self.inner.relative_speed_m_s
    }

    /// Collision probability.
    #[wasm_bindgen(getter, js_name = collisionProbability)]
    pub fn collision_probability(&self) -> Option<f64> {
        self.inner.collision_probability
    }

    /// Collision-probability method label.
    #[wasm_bindgen(getter, js_name = collisionProbabilityMethod)]
    pub fn collision_probability_method(&self) -> Option<String> {
        self.inner.collision_probability_method.clone()
    }

    /// Hard-body radius, metres.
    #[wasm_bindgen(getter, js_name = hardBodyRadiusM)]
    pub fn hard_body_radius_m(&self) -> Option<f64> {
        self.inner.hard_body_radius_m
    }

    /// First object.
    #[wasm_bindgen(getter)]
    pub fn object1(&self) -> CdmObject {
        CdmObject {
            inner: self.inner.object1.clone(),
        }
    }

    /// Second object.
    #[wasm_bindgen(getter)]
    pub fn object2(&self) -> CdmObject {
        CdmObject {
            inner: self.inner.object2.clone(),
        }
    }

    /// Encode this message to CCSDS CDM KVN text.
    #[wasm_bindgen(js_name = toKvnString)]
    pub fn to_kvn_string(&self) -> Result<String, JsValue> {
        encode_kvn(&self.inner).map_err(engine_error)
    }

    /// Encode this message to CCSDS CDM XML text.
    #[wasm_bindgen(js_name = toXmlString)]
    pub fn to_xml_string(&self) -> Result<String, JsValue> {
        encode_xml(&self.inner).map_err(engine_error)
    }
}

/// Parse CCSDS CDM KVN text. Throws an `Error` on a parse failure.
#[wasm_bindgen(js_name = parseCdmKvn)]
pub fn parse_cdm_kvn(text: &str) -> Result<Cdm, JsValue> {
    parse_kvn(text)
        .map(|inner| Cdm { inner })
        .map_err(engine_error)
}

/// Parse CCSDS CDM XML text. Throws an `Error` on a parse failure.
#[wasm_bindgen(js_name = parseCdmXml)]
pub fn parse_cdm_xml(text: &str) -> Result<Cdm, JsValue> {
    parse_xml(text)
        .map(|inner| Cdm { inner })
        .map_err(engine_error)
}
