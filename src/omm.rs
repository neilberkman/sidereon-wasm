//! CCSDS OMM binding: the canonical OMM container plus KVN/XML/JSON parse and
//! encode. All format grammar and serialization live in
//! `sidereon_core::astro::omm`; this module marshals fields and validates shape.

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use sidereon_core::astro::omm::{
    encode_json, encode_kvn, encode_xml, parse_json, parse_kvn, parse_xml, Omm as CoreOmm,
    OmmEpoch as CoreOmmEpoch,
};

use crate::error::{engine_error, range_error};

fn finite(value: f64, name: &str) -> Result<f64, JsValue> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(range_error(&format!("{name} must be finite")))
    }
}

/// UTC calendar epoch carried by an OMM `EPOCH` field.
#[wasm_bindgen]
#[derive(Clone)]
pub struct OmmEpoch {
    inner: CoreOmmEpoch,
}

impl OmmEpoch {
    fn iso8601_string(&self) -> String {
        let mut text = format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:06}",
            self.inner.year,
            self.inner.month,
            self.inner.day,
            self.inner.hour,
            self.inner.minute,
            self.inner.second,
            self.inner.microsecond
        );
        if self.inner.femtosecond != 0 {
            text.push_str(&format!("{:09}", self.inner.femtosecond));
        }
        text
    }
}

#[wasm_bindgen]
impl OmmEpoch {
    /// Build an OMM epoch from UTC calendar fields. Throws a `RangeError` on an
    /// out-of-range field.
    #[wasm_bindgen(constructor)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
        microsecond: u32,
        femtosecond: Option<u32>,
    ) -> Result<OmmEpoch, JsValue> {
        if !(1..=12).contains(&month) {
            return Err(range_error("month must be in 1..=12"));
        }
        if !(1..=31).contains(&day) {
            return Err(range_error("day must be in 1..=31"));
        }
        if hour > 23 {
            return Err(range_error("hour must be in 0..=23"));
        }
        if minute > 59 {
            return Err(range_error("minute must be in 0..=59"));
        }
        if second > 60 {
            return Err(range_error("second must be in 0..=60"));
        }
        if microsecond > 999_999 {
            return Err(range_error("microsecond must be in 0..=999999"));
        }
        let femtosecond = femtosecond.unwrap_or(0);
        if femtosecond > 999_999_999 {
            return Err(range_error("femtosecond must be in 0..=999999999"));
        }
        Ok(OmmEpoch {
            inner: CoreOmmEpoch {
                year,
                month,
                day,
                hour,
                minute,
                second,
                microsecond,
                femtosecond,
            },
        })
    }

    /// Calendar year.
    #[wasm_bindgen(getter)]
    pub fn year(&self) -> i32 {
        self.inner.year
    }

    /// Calendar month.
    #[wasm_bindgen(getter)]
    pub fn month(&self) -> u32 {
        self.inner.month
    }

    /// Calendar day.
    #[wasm_bindgen(getter)]
    pub fn day(&self) -> u32 {
        self.inner.day
    }

    /// Hour of day.
    #[wasm_bindgen(getter)]
    pub fn hour(&self) -> u32 {
        self.inner.hour
    }

    /// Minute of hour.
    #[wasm_bindgen(getter)]
    pub fn minute(&self) -> u32 {
        self.inner.minute
    }

    /// Second of minute.
    #[wasm_bindgen(getter)]
    pub fn second(&self) -> u32 {
        self.inner.second
    }

    /// Microsecond of second.
    #[wasm_bindgen(getter)]
    pub fn microsecond(&self) -> u32 {
        self.inner.microsecond
    }

    /// Femtosecond remainder within the microsecond.
    #[wasm_bindgen(getter)]
    pub fn femtosecond(&self) -> u32 {
        self.inner.femtosecond
    }

    /// ISO-8601 epoch text with microsecond precision.
    #[wasm_bindgen(getter)]
    pub fn iso8601(&self) -> String {
        self.iso8601_string()
    }
}

/// Optional OMM fields, all defaulting to the CCSDS-standard values.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct OmmMeta {
    ccsds_omm_vers: Option<String>,
    creation_date: Option<String>,
    originator: Option<String>,
    object_name: Option<String>,
    object_id: Option<String>,
    center_name: Option<String>,
    ref_frame: Option<String>,
    time_system: Option<String>,
    mean_element_theory: Option<String>,
    ephemeris_type: Option<i32>,
    classification_type: Option<String>,
    element_set_no: Option<i32>,
    rev_at_epoch: Option<i64>,
    bstar: Option<f64>,
    mean_motion_dot: Option<f64>,
    mean_motion_ddot: Option<f64>,
}

/// A canonical, format-agnostic CCSDS Orbit Mean-Elements Message.
#[wasm_bindgen]
#[derive(Clone)]
pub struct Omm {
    inner: CoreOmm,
}

impl Omm {
    pub(crate) fn from_core(inner: CoreOmm) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl Omm {
    /// Build an OMM. The eight leading arguments are required; `meta` carries the
    /// optional fields (`ccsdsOmmVers`, `creationDate`, ... `bstar`,
    /// `meanMotionDot`, `meanMotionDdot`).
    #[wasm_bindgen(constructor)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        epoch: &OmmEpoch,
        mean_motion: f64,
        eccentricity: f64,
        inclination_deg: f64,
        ra_of_asc_node_deg: f64,
        arg_of_pericenter_deg: f64,
        mean_anomaly_deg: f64,
        norad_cat_id: u32,
        meta: JsValue,
    ) -> Result<Omm, JsValue> {
        let m: OmmMeta = if meta.is_undefined() || meta.is_null() {
            OmmMeta::default()
        } else {
            serde_wasm_bindgen::from_value(meta)
                .map_err(|e| range_error(&format!("invalid Omm meta: {e}")))?
        };
        Ok(Omm {
            inner: CoreOmm {
                ccsds_omm_vers: m.ccsds_omm_vers.unwrap_or_else(|| "2.0".to_string()),
                creation_date: m.creation_date,
                originator: m.originator,
                object_name: m.object_name,
                object_id: m.object_id,
                center_name: m.center_name,
                ref_frame: m.ref_frame,
                time_system: m.time_system,
                mean_element_theory: m.mean_element_theory,
                epoch: epoch.inner.clone(),
                mean_motion: finite(mean_motion, "meanMotion")?,
                eccentricity: finite(eccentricity, "eccentricity")?,
                inclination_deg: finite(inclination_deg, "inclinationDeg")?,
                ra_of_asc_node_deg: finite(ra_of_asc_node_deg, "raOfAscNodeDeg")?,
                arg_of_pericenter_deg: finite(arg_of_pericenter_deg, "argOfPericenterDeg")?,
                mean_anomaly_deg: finite(mean_anomaly_deg, "meanAnomalyDeg")?,
                ephemeris_type: m.ephemeris_type.unwrap_or(0),
                classification_type: m.classification_type.unwrap_or_else(|| "U".to_string()),
                norad_cat_id,
                element_set_no: m.element_set_no.unwrap_or(999),
                rev_at_epoch: m.rev_at_epoch.unwrap_or(0),
                bstar: finite(m.bstar.unwrap_or(0.0), "bstar")?,
                mean_motion_dot: finite(m.mean_motion_dot.unwrap_or(0.0), "meanMotionDot")?,
                mean_motion_ddot: finite(m.mean_motion_ddot.unwrap_or(0.0), "meanMotionDdot")?,
                exact_sgp4_epoch: None,
                quantize_tle_derived_fields: true,
            },
        })
    }

    /// CCSDS OMM version string.
    #[wasm_bindgen(getter, js_name = ccsdsOmmVers)]
    pub fn ccsds_omm_vers(&self) -> String {
        self.inner.ccsds_omm_vers.clone()
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

    /// Object name.
    #[wasm_bindgen(getter, js_name = objectName)]
    pub fn object_name(&self) -> Option<String> {
        self.inner.object_name.clone()
    }

    /// Object id.
    #[wasm_bindgen(getter, js_name = objectId)]
    pub fn object_id(&self) -> Option<String> {
        self.inner.object_id.clone()
    }

    /// Center name.
    #[wasm_bindgen(getter, js_name = centerName)]
    pub fn center_name(&self) -> Option<String> {
        self.inner.center_name.clone()
    }

    /// Reference frame.
    #[wasm_bindgen(getter, js_name = refFrame)]
    pub fn ref_frame(&self) -> Option<String> {
        self.inner.ref_frame.clone()
    }

    /// Time system.
    #[wasm_bindgen(getter, js_name = timeSystem)]
    pub fn time_system(&self) -> Option<String> {
        self.inner.time_system.clone()
    }

    /// Mean-element theory.
    #[wasm_bindgen(getter, js_name = meanElementTheory)]
    pub fn mean_element_theory(&self) -> Option<String> {
        self.inner.mean_element_theory.clone()
    }

    /// The epoch.
    #[wasm_bindgen(getter)]
    pub fn epoch(&self) -> OmmEpoch {
        OmmEpoch {
            inner: self.inner.epoch.clone(),
        }
    }

    /// Mean motion, rev/day.
    #[wasm_bindgen(getter, js_name = meanMotion)]
    pub fn mean_motion(&self) -> f64 {
        self.inner.mean_motion
    }

    /// Eccentricity.
    #[wasm_bindgen(getter)]
    pub fn eccentricity(&self) -> f64 {
        self.inner.eccentricity
    }

    /// Inclination, degrees.
    #[wasm_bindgen(getter, js_name = inclinationDeg)]
    pub fn inclination_deg(&self) -> f64 {
        self.inner.inclination_deg
    }

    /// Right ascension of the ascending node, degrees.
    #[wasm_bindgen(getter, js_name = raOfAscNodeDeg)]
    pub fn ra_of_asc_node_deg(&self) -> f64 {
        self.inner.ra_of_asc_node_deg
    }

    /// Argument of pericenter, degrees.
    #[wasm_bindgen(getter, js_name = argOfPericenterDeg)]
    pub fn arg_of_pericenter_deg(&self) -> f64 {
        self.inner.arg_of_pericenter_deg
    }

    /// Mean anomaly, degrees.
    #[wasm_bindgen(getter, js_name = meanAnomalyDeg)]
    pub fn mean_anomaly_deg(&self) -> f64 {
        self.inner.mean_anomaly_deg
    }

    /// SGP4 ephemeris type.
    #[wasm_bindgen(getter, js_name = ephemerisType)]
    pub fn ephemeris_type(&self) -> i32 {
        self.inner.ephemeris_type
    }

    /// Classification type.
    #[wasm_bindgen(getter, js_name = classificationType)]
    pub fn classification_type(&self) -> String {
        self.inner.classification_type.clone()
    }

    /// NORAD catalog number.
    #[wasm_bindgen(getter, js_name = noradCatId)]
    pub fn norad_cat_id(&self) -> u32 {
        self.inner.norad_cat_id
    }

    /// Element set number.
    #[wasm_bindgen(getter, js_name = elementSetNo)]
    pub fn element_set_no(&self) -> i32 {
        self.inner.element_set_no
    }

    /// Revolution number at epoch.
    #[wasm_bindgen(getter, js_name = revAtEpoch)]
    pub fn rev_at_epoch(&self) -> i64 {
        self.inner.rev_at_epoch
    }

    /// B* drag term.
    #[wasm_bindgen(getter)]
    pub fn bstar(&self) -> f64 {
        self.inner.bstar
    }

    /// First derivative of mean motion.
    #[wasm_bindgen(getter, js_name = meanMotionDot)]
    pub fn mean_motion_dot(&self) -> f64 {
        self.inner.mean_motion_dot
    }

    /// Second derivative of mean motion.
    #[wasm_bindgen(getter, js_name = meanMotionDdot)]
    pub fn mean_motion_ddot(&self) -> f64 {
        self.inner.mean_motion_ddot
    }

    /// Encode this OMM to CCSDS OMM KVN text.
    #[wasm_bindgen(js_name = toKvnString)]
    pub fn to_kvn_string(&self) -> String {
        encode_kvn(&self.inner)
    }

    /// Encode this OMM to CCSDS OMM XML text.
    #[wasm_bindgen(js_name = toXmlString)]
    pub fn to_xml_string(&self) -> String {
        encode_xml(&self.inner)
    }

    /// Encode this OMM to CCSDS/CelesTrak JSON text.
    #[wasm_bindgen(js_name = toJsonString)]
    pub fn to_json_string(&self) -> String {
        encode_json(&self.inner)
    }
}

/// Parse CCSDS OMM KVN text. Throws an `Error` on a parse failure.
#[wasm_bindgen(js_name = parseOmmKvn)]
pub fn parse_omm_kvn(text: &str) -> Result<Omm, JsValue> {
    parse_kvn(text)
        .map(|inner| Omm { inner })
        .map_err(engine_error)
}

/// Parse CCSDS OMM XML text. Throws an `Error` on a parse failure.
#[wasm_bindgen(js_name = parseOmmXml)]
pub fn parse_omm_xml(text: &str) -> Result<Omm, JsValue> {
    parse_xml(text)
        .map(|inner| Omm { inner })
        .map_err(engine_error)
}

/// Parse CCSDS/CelesTrak OMM JSON text. Throws an `Error` on a parse failure.
#[wasm_bindgen(js_name = parseOmmJson)]
pub fn parse_omm_json(text: &str) -> Result<Omm, JsValue> {
    parse_json(text)
        .map(|inner| Omm { inner })
        .map_err(engine_error)
}
