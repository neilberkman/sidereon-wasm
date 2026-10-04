//! CCSDS OMM binding: the canonical OMM container plus KVN/XML/JSON parse and
//! encode. All format grammar and serialization live in
//! `sidereon_core::astro::omm`; this module marshals fields and validates shape.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::astro::omm::{
    encode_csv, encode_csv_discarding_comments, encode_json, encode_json_discarding_comments,
    encode_kvn, encode_xml, parse_csv, parse_csv_array, parse_json, parse_json_array, parse_kvn,
    parse_xml, parse_xml_all, Omm as CoreOmm, OmmArray as CoreOmmArray,
    OmmComments as CoreOmmComments, OmmCovariance as CoreOmmCovariance, OmmEpoch as CoreOmmEpoch,
    OmmSpacecraft as CoreOmmSpacecraft, OmmUserDefined as CoreOmmUserDefined,
};

use crate::error::{range_error, result_object, to_plain_js, type_error};
use crate::ndm_error::{omm_detail, omm_error, NdmErrorDetail};

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

/// Optional OMM fields. An absent field is absent from the message: no value
/// is filled in for it, so a writer states only what the caller gave.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct OmmMeta {
    ccsds_omm_vers: Option<String>,
    classification: Option<String>,
    creation_date: Option<String>,
    originator: Option<String>,
    message_id: Option<String>,
    object_name: Option<String>,
    object_id: Option<String>,
    center_name: Option<String>,
    ref_frame: Option<String>,
    ref_frame_epoch: Option<String>,
    time_system: Option<String>,
    mean_element_theory: Option<String>,
    semi_major_axis_km: Option<f64>,
    gm_km3_s2: Option<f64>,
    spacecraft: Option<OmmSpacecraftJs>,
    ephemeris_type: Option<i32>,
    classification_type: Option<String>,
    element_set_no: Option<i32>,
    rev_at_epoch: Option<i64>,
    bstar: Option<f64>,
    bterm_m2_kg: Option<f64>,
    mean_motion_dot: Option<f64>,
    mean_motion_ddot: Option<f64>,
    agom_m2_kg: Option<f64>,
    covariance: Option<OmmCovarianceJs>,
    user_defined: Vec<OmmUserDefinedJs>,
    comments: OmmCommentsJs,
}

const OMM_META_KEYS: &[&str] = &[
    "ccsdsOmmVers",
    "classification",
    "creationDate",
    "originator",
    "messageId",
    "objectName",
    "objectId",
    "centerName",
    "refFrame",
    "refFrameEpoch",
    "timeSystem",
    "meanElementTheory",
    "semiMajorAxisKm",
    "gmKm3S2",
    "spacecraft",
    "ephemerisType",
    "classificationType",
    "elementSetNo",
    "revAtEpoch",
    "bstar",
    "btermM2Kg",
    "meanMotionDot",
    "meanMotionDdot",
    "agomM2Kg",
    "covariance",
    "userDefined",
    "comments",
];

/// OMM spacecraft parameters (CCSDS 502.0-B-3 table 4-3).
#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct OmmSpacecraftJs {
    comments: Vec<String>,
    mass_kg: Option<f64>,
    solar_rad_area_m2: Option<f64>,
    solar_rad_coeff: Option<f64>,
    drag_area_m2: Option<f64>,
    drag_coeff: Option<f64>,
}

impl OmmSpacecraftJs {
    fn from_core(value: &CoreOmmSpacecraft) -> Self {
        Self {
            comments: value.comments.clone(),
            mass_kg: value.mass_kg,
            solar_rad_area_m2: value.solar_rad_area_m2,
            solar_rad_coeff: value.solar_rad_coeff,
            drag_area_m2: value.drag_area_m2,
            drag_coeff: value.drag_coeff,
        }
    }

    fn to_core(&self) -> CoreOmmSpacecraft {
        CoreOmmSpacecraft {
            comments: self.comments.clone(),
            mass_kg: self.mass_kg,
            solar_rad_area_m2: self.solar_rad_area_m2,
            solar_rad_coeff: self.solar_rad_coeff,
            drag_area_m2: self.drag_area_m2,
            drag_coeff: self.drag_coeff,
        }
    }
}

/// OMM position/velocity covariance, the 21 lower-triangle values as read.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OmmCovarianceJs {
    #[serde(default)]
    comments: Vec<String>,
    #[serde(default)]
    cov_ref_frame: Option<String>,
    lower_triangle: Vec<f64>,
}

impl OmmCovarianceJs {
    fn from_core(value: &CoreOmmCovariance) -> Self {
        Self {
            comments: value.comments.clone(),
            cov_ref_frame: value.cov_ref_frame.clone(),
            lower_triangle: value.lower_triangle.to_vec(),
        }
    }

    fn to_core(&self) -> Result<CoreOmmCovariance, JsValue> {
        let lower_triangle: [f64; 21] =
            self.lower_triangle.as_slice().try_into().map_err(|_| {
                range_error(&format!(
                    "covariance.lowerTriangle must hold 21 values, got {}",
                    self.lower_triangle.len()
                ))
            })?;
        Ok(CoreOmmCovariance {
            comments: self.comments.clone(),
            cov_ref_frame: self.cov_ref_frame.clone(),
            lower_triangle,
        })
    }
}

/// One `USER_DEFINED_*` parameter, verbatim.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OmmUserDefinedJs {
    parameter: String,
    value: String,
}

/// OMM block comments, each in source order.
#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct OmmCommentsJs {
    header: Vec<String>,
    metadata: Vec<String>,
    mean_elements: Vec<String>,
    tle_parameters: Vec<String>,
    user_defined: Vec<String>,
}

impl OmmCommentsJs {
    fn from_core(value: &CoreOmmComments) -> Self {
        Self {
            header: value.header.clone(),
            metadata: value.metadata.clone(),
            mean_elements: value.mean_elements.clone(),
            tle_parameters: value.tle_parameters.clone(),
            user_defined: value.user_defined.clone(),
        }
    }

    fn to_core(&self) -> CoreOmmComments {
        CoreOmmComments {
            header: self.header.clone(),
            metadata: self.metadata.clone(),
            mean_elements: self.mean_elements.clone(),
            tle_parameters: self.tle_parameters.clone(),
            user_defined: self.user_defined.clone(),
        }
    }
}

fn finite_opt(value: Option<f64>, name: &str) -> Result<Option<f64>, JsValue> {
    value.map(|value| finite(value, name)).transpose()
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

    pub(crate) fn core(&self) -> &CoreOmm {
        &self.inner
    }
}

#[wasm_bindgen]
impl Omm {
    /// Build an OMM. `meanMotion` and `noradCatId` may be `undefined`: an OMM
    /// states `MEAN_MOTION` or `SEMI_MAJOR_AXIS` (`meta.semiMajorAxisKm`), and
    /// the TLE related parameters only for SGP/SGP4 theories. `meta` carries
    /// the other optional fields (`ccsdsOmmVers`, `classification`, ...,
    /// `spacecraft`, `covariance`, `userDefined`, `comments`); a field left out
    /// is absent from the message, with no default filled in. Throws a
    /// `TypeError` for an unknown `meta` key and a `RangeError` for a
    /// non-finite number.
    #[wasm_bindgen(constructor)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        epoch: &OmmEpoch,
        mean_motion: Option<f64>,
        eccentricity: f64,
        inclination_deg: f64,
        ra_of_asc_node_deg: f64,
        arg_of_pericenter_deg: f64,
        mean_anomaly_deg: f64,
        norad_cat_id: Option<u32>,
        #[wasm_bindgen(unchecked_param_type = "OmmMeta | undefined | null")] meta: JsValue,
    ) -> Result<Omm, JsValue> {
        let m: OmmMeta = if meta.is_undefined() || meta.is_null() {
            OmmMeta::default()
        } else {
            crate::error::reject_unknown_keys(&meta, "Omm meta", OMM_META_KEYS)?;
            serde_wasm_bindgen::from_value(meta)
                .map_err(|e| type_error(&format!("invalid Omm meta: {e}")))?
        };
        Ok(Omm {
            inner: CoreOmm {
                ccsds_omm_vers: m.ccsds_omm_vers,
                classification: m.classification,
                creation_date: m.creation_date,
                originator: m.originator,
                message_id: m.message_id,
                object_name: m.object_name,
                object_id: m.object_id,
                center_name: m.center_name,
                ref_frame: m.ref_frame,
                ref_frame_epoch: m.ref_frame_epoch,
                time_system: m.time_system,
                mean_element_theory: m.mean_element_theory,
                epoch: epoch.inner.clone(),
                mean_motion: finite_opt(mean_motion, "meanMotion")?,
                semi_major_axis_km: finite_opt(m.semi_major_axis_km, "semiMajorAxisKm")?,
                eccentricity: finite(eccentricity, "eccentricity")?,
                inclination_deg: finite(inclination_deg, "inclinationDeg")?,
                ra_of_asc_node_deg: finite(ra_of_asc_node_deg, "raOfAscNodeDeg")?,
                arg_of_pericenter_deg: finite(arg_of_pericenter_deg, "argOfPericenterDeg")?,
                mean_anomaly_deg: finite(mean_anomaly_deg, "meanAnomalyDeg")?,
                gm_km3_s2: finite_opt(m.gm_km3_s2, "gmKm3S2")?,
                spacecraft: m.spacecraft.as_ref().map(OmmSpacecraftJs::to_core),
                ephemeris_type: m.ephemeris_type,
                classification_type: m.classification_type,
                norad_cat_id,
                element_set_no: m.element_set_no,
                rev_at_epoch: m.rev_at_epoch,
                bstar: finite_opt(m.bstar, "bstar")?,
                bterm_m2_kg: finite_opt(m.bterm_m2_kg, "btermM2Kg")?,
                mean_motion_dot: finite_opt(m.mean_motion_dot, "meanMotionDot")?,
                mean_motion_ddot: finite_opt(m.mean_motion_ddot, "meanMotionDdot")?,
                agom_m2_kg: finite_opt(m.agom_m2_kg, "agomM2Kg")?,
                covariance: m
                    .covariance
                    .as_ref()
                    .map(OmmCovarianceJs::to_core)
                    .transpose()?,
                user_defined: m
                    .user_defined
                    .into_iter()
                    .map(|entry| CoreOmmUserDefined {
                        parameter: entry.parameter,
                        value: entry.value,
                    })
                    .collect(),
                comments: m.comments.to_core(),
                exact_sgp4_epoch: None,
                quantize_tle_derived_fields: true,
            },
        })
    }

    /// CCSDS OMM version as the message states it, or `undefined` when it
    /// states none (as CelesTrak GP JSON and CSV do).
    #[wasm_bindgen(getter, js_name = ccsdsOmmVers)]
    pub fn ccsds_omm_vers(&self) -> Option<String> {
        self.inner.ccsds_omm_vers.clone()
    }

    /// Header `CLASSIFICATION`.
    #[wasm_bindgen(getter)]
    pub fn classification(&self) -> Option<String> {
        self.inner.classification.clone()
    }

    /// Header `MESSAGE_ID`.
    #[wasm_bindgen(getter, js_name = messageId)]
    pub fn message_id(&self) -> Option<String> {
        self.inner.message_id.clone()
    }

    /// Metadata `REF_FRAME_EPOCH`, as written.
    #[wasm_bindgen(getter, js_name = refFrameEpoch)]
    pub fn ref_frame_epoch(&self) -> Option<String> {
        self.inner.ref_frame_epoch.clone()
    }

    /// `SEMI_MAJOR_AXIS`, km: the table 4-3 alternative to mean motion.
    #[wasm_bindgen(getter, js_name = semiMajorAxisKm)]
    pub fn semi_major_axis_km(&self) -> Option<f64> {
        self.inner.semi_major_axis_km
    }

    /// `GM`, km^3/s^2.
    #[wasm_bindgen(getter, js_name = gmKm3S2)]
    pub fn gm_km3_s2(&self) -> Option<f64> {
        self.inner.gm_km3_s2
    }

    /// SGP4-XP `BTERM`, m^2/kg.
    #[wasm_bindgen(getter, js_name = btermM2Kg)]
    pub fn bterm_m2_kg(&self) -> Option<f64> {
        self.inner.bterm_m2_kg
    }

    /// SGP4-XP `AGOM`, m^2/kg.
    #[wasm_bindgen(getter, js_name = agomM2Kg)]
    pub fn agom_m2_kg(&self) -> Option<f64> {
        self.inner.agom_m2_kg
    }

    /// Spacecraft parameters, or `undefined` when the message has none.
    #[wasm_bindgen(getter, unchecked_return_type = "OmmSpacecraft | undefined")]
    pub fn spacecraft(&self) -> Result<JsValue, JsValue> {
        match &self.inner.spacecraft {
            Some(value) => to_plain_js(&OmmSpacecraftJs::from_core(value), "OMM spacecraft"),
            None => Ok(JsValue::UNDEFINED),
        }
    }

    /// Covariance with its 21 lower-triangle values as read, or `undefined`.
    #[wasm_bindgen(getter, unchecked_return_type = "OmmCovariance | undefined")]
    pub fn covariance(&self) -> Result<JsValue, JsValue> {
        match &self.inner.covariance {
            Some(value) => to_plain_js(&OmmCovarianceJs::from_core(value), "OMM covariance"),
            None => Ok(JsValue::UNDEFINED),
        }
    }

    /// `USER_DEFINED_*` parameters as `{ parameter, value }`, verbatim, in
    /// source order.
    #[wasm_bindgen(getter, js_name = userDefined, unchecked_return_type = "OmmUserDefined[]")]
    pub fn user_defined(&self) -> Result<JsValue, JsValue> {
        let rows: Vec<OmmUserDefinedJs> = self
            .inner
            .user_defined
            .iter()
            .map(|entry| OmmUserDefinedJs {
                parameter: entry.parameter.clone(),
                value: entry.value.clone(),
            })
            .collect();
        to_plain_js(&rows, "OMM user-defined parameters")
    }

    /// Block comments, each list in source order.
    #[wasm_bindgen(getter, unchecked_return_type = "OmmComments")]
    pub fn comments(&self) -> Result<JsValue, JsValue> {
        to_plain_js(
            &OmmCommentsJs::from_core(&self.inner.comments),
            "OMM comments",
        )
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

    /// Mean motion, rev/day, or `undefined` for a message that states
    /// `SEMI_MAJOR_AXIS` instead.
    #[wasm_bindgen(getter, js_name = meanMotion)]
    pub fn mean_motion(&self) -> Option<f64> {
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
    pub fn ephemeris_type(&self) -> Option<i32> {
        self.inner.ephemeris_type
    }

    /// Classification type.
    #[wasm_bindgen(getter, js_name = classificationType)]
    pub fn classification_type(&self) -> Option<String> {
        self.inner.classification_type.clone()
    }

    /// NORAD catalog number.
    #[wasm_bindgen(getter, js_name = noradCatId)]
    pub fn norad_cat_id(&self) -> Option<u32> {
        self.inner.norad_cat_id
    }

    /// Element set number.
    #[wasm_bindgen(getter, js_name = elementSetNo)]
    pub fn element_set_no(&self) -> Option<i32> {
        self.inner.element_set_no
    }

    /// Revolution number at epoch.
    #[wasm_bindgen(getter, js_name = revAtEpoch)]
    pub fn rev_at_epoch(&self) -> Option<i64> {
        self.inner.rev_at_epoch
    }

    /// B* drag term.
    #[wasm_bindgen(getter)]
    pub fn bstar(&self) -> Option<f64> {
        self.inner.bstar
    }

    /// First derivative of mean motion.
    #[wasm_bindgen(getter, js_name = meanMotionDot)]
    pub fn mean_motion_dot(&self) -> Option<f64> {
        self.inner.mean_motion_dot
    }

    /// Second derivative of mean motion.
    #[wasm_bindgen(getter, js_name = meanMotionDdot)]
    pub fn mean_motion_ddot(&self) -> Option<f64> {
        self.inner.mean_motion_ddot
    }

    /// Encode this OMM to CCSDS OMM KVN text. Throws an `OmmError` for text
    /// that would not read back unchanged (`UNWRITABLE_TEXT`), a non-finite
    /// number or an epoch the reader refuses (`INVALID_FIELD`).
    #[wasm_bindgen(js_name = toKvnString)]
    pub fn to_kvn_string(&self) -> Result<String, JsValue> {
        encode_kvn(&self.inner).map_err(omm_error)
    }

    /// Encode this OMM to CCSDS OMM XML text. Throws an `OmmError` for what
    /// the XML reader would not return unchanged.
    #[wasm_bindgen(js_name = toXmlString)]
    pub fn to_xml_string(&self) -> Result<String, JsValue> {
        encode_xml(&self.inner).map_err(omm_error)
    }

    /// Encode this OMM to GP JSON text. GP JSON carries one comment, the
    /// header `COMMENT`; a record holding any other comment is refused with an
    /// `OmmError` (`UNWRITABLE_TEXT`, issue `commentNotCarried`), as is a
    /// non-finite number.
    #[wasm_bindgen(js_name = toJsonString)]
    pub fn to_json_string(&self) -> Result<String, JsValue> {
        encode_json(&self.inner).map_err(omm_error)
    }

    /// Encode this OMM to GP JSON text without the comments GP JSON cannot
    /// carry, for a caller that accepts that loss.
    #[wasm_bindgen(js_name = toJsonStringDiscardingComments)]
    pub fn to_json_string_discarding_comments(&self) -> Result<String, JsValue> {
        encode_json_discarding_comments(&self.inner).map_err(omm_error)
    }

    /// Encode this OMM as a one-record GP CSV table. Throws an `OmmError` for
    /// a record `parseOmmCsvArray` would not return unchanged, named with
    /// `IN_RECORD`.
    #[wasm_bindgen(js_name = toCsvString)]
    pub fn to_csv_string(&self) -> Result<String, JsValue> {
        encode_csv(core::slice::from_ref(&self.inner)).map_err(omm_error)
    }

    /// Encode this OMM as a one-record GP CSV table without the comments, and
    /// without a spacecraft-parameters block that held nothing else, that GP
    /// CSV cannot carry.
    #[wasm_bindgen(js_name = toCsvStringDiscardingComments)]
    pub fn to_csv_string_discarding_comments(&self) -> Result<String, JsValue> {
        encode_csv_discarding_comments(core::slice::from_ref(&self.inner)).map_err(omm_error)
    }
}

/// Parse CCSDS OMM KVN text. Throws an `OmmError` on a parse failure.
#[wasm_bindgen(js_name = parseOmmKvn)]
pub fn parse_omm_kvn(text: &str) -> Result<Omm, JsValue> {
    parse_kvn(text)
        .map(|inner| Omm { inner })
        .map_err(omm_error)
}

/// Parse CCSDS OMM XML text holding one OMM. Throws an `OmmError` on a parse
/// failure, including `MULTIPLE_MESSAGES` for a document holding several;
/// `parseOmmXmlAll` reads all of them.
#[wasm_bindgen(js_name = parseOmmXml)]
pub fn parse_omm_xml(text: &str) -> Result<Omm, JsValue> {
    parse_xml(text)
        .map(|inner| Omm { inner })
        .map_err(omm_error)
}

/// Parse CCSDS/CelesTrak OMM JSON text holding one record (an object, or an
/// array of one). Throws an `OmmError` on a parse failure, including
/// `MULTIPLE_MESSAGES` for an array of several; `parseOmmJsonArray` reads all
/// of them.
#[wasm_bindgen(js_name = parseOmmJson)]
pub fn parse_omm_json(text: &str) -> Result<Omm, JsValue> {
    parse_json(text)
        .map(|inner| Omm { inner })
        .map_err(omm_error)
}

/// Parse GP CSV text holding one record. Throws an `OmmError` on a parse
/// failure, including `MULTIPLE_MESSAGES` for several records.
#[wasm_bindgen(js_name = parseOmmCsv)]
pub fn parse_omm_csv(text: &str) -> Result<Omm, JsValue> {
    parse_csv(text)
        .map(|inner| Omm { inner })
        .map_err(omm_error)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OmmSkippedRecordJs {
    index: usize,
    reason: NdmErrorDetail,
}

/// `{ omms, skipped }`: every OMM read, and each record skipped with its
/// zero-based `index` and the `reason` it was not read.
fn omm_array_js(array: CoreOmmArray) -> Result<JsValue, JsValue> {
    let omms = js_sys::Array::new();
    for inner in array.omms {
        omms.push(&JsValue::from(Omm { inner }));
    }
    let skipped: Vec<OmmSkippedRecordJs> = array
        .skipped
        .iter()
        .map(|record| OmmSkippedRecordJs {
            index: record.index,
            reason: omm_detail(&record.reason),
        })
        .collect();
    let skipped = to_plain_js(&skipped, "skipped OMM records")?;
    result_object(
        &[("omms", &omms.into()), ("skipped", &skipped)],
        "OMM array",
    )
}

/// Parse every OMM of an XML document: a single message or an NDM combined
/// instantiation. A message that cannot be read is reported in `skipped`.
#[wasm_bindgen(js_name = parseOmmXmlAll, unchecked_return_type = "OmmArray")]
pub fn parse_omm_xml_all(text: &str) -> Result<JsValue, JsValue> {
    omm_array_js(parse_xml_all(text).map_err(omm_error)?)
}

/// Parse every record of a GP JSON array (a lone object reads as an array of
/// one). An element that is not a valid OMM is reported in `skipped`.
#[wasm_bindgen(js_name = parseOmmJsonArray, unchecked_return_type = "OmmArray")]
pub fn parse_omm_json_array(text: &str) -> Result<JsValue, JsValue> {
    omm_array_js(parse_json_array(text).map_err(omm_error)?)
}

/// Parse every record of a GP CSV table. A row that cannot be read is
/// reported in `skipped`.
#[wasm_bindgen(js_name = parseOmmCsvArray, unchecked_return_type = "OmmArray")]
pub fn parse_omm_csv_array(text: &str) -> Result<JsValue, JsValue> {
    omm_array_js(parse_csv_array(text).map_err(omm_error)?)
}
