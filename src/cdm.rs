//! CCSDS CDM binding: typed value objects for the core `CdmKvn` / `CdmObject`
//! plus KVN/XML parse and encode. The grammar and serialization live entirely in
//! `sidereon_core::astro::cdm`; this module marshals strings, optional fields,
//! and flat `Float64Array` vectors.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::astro::cdm::{
    encode_kvn, encode_xml, parse_kvn, parse_xml, CdmAdditionalParameters, CdmKvn,
    CdmObject as CoreCdmObject, CdmOdParameters,
};

use crate::error::{reject_unknown_keys, to_plain_js, type_error};
use crate::marshal::vec3;
use crate::ndm_error::cdm_error;

fn fixed<const N: usize>(name: &str, values: &[f64]) -> Result<[f64; N], JsValue> {
    values.try_into().map_err(|_| {
        type_error(&format!(
            "{name} must have length {N}, got {}",
            values.len()
        ))
    })
}

/// CDM orbit-determination parameters (CCSDS 508.0-B-1 table 3-4).
#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct CdmOdParametersJs {
    comments: Vec<String>,
    time_lastob_start: Option<String>,
    time_lastob_end: Option<String>,
    recommended_od_span_d: Option<f64>,
    actual_od_span_d: Option<f64>,
    obs_available: Option<u64>,
    obs_used: Option<u64>,
    tracks_available: Option<u64>,
    tracks_used: Option<u64>,
    residuals_accepted_pct: Option<f64>,
    weighted_rms: Option<f64>,
}

impl CdmOdParametersJs {
    fn from_core(v: &CdmOdParameters) -> Self {
        Self {
            comments: v.comments.clone(),
            time_lastob_start: v.time_lastob_start.clone(),
            time_lastob_end: v.time_lastob_end.clone(),
            recommended_od_span_d: v.recommended_od_span_d,
            actual_od_span_d: v.actual_od_span_d,
            obs_available: v.obs_available,
            obs_used: v.obs_used,
            tracks_available: v.tracks_available,
            tracks_used: v.tracks_used,
            residuals_accepted_pct: v.residuals_accepted_pct,
            weighted_rms: v.weighted_rms,
        }
    }

    fn to_core(&self) -> CdmOdParameters {
        CdmOdParameters {
            comments: self.comments.clone(),
            time_lastob_start: self.time_lastob_start.clone(),
            time_lastob_end: self.time_lastob_end.clone(),
            recommended_od_span_d: self.recommended_od_span_d,
            actual_od_span_d: self.actual_od_span_d,
            obs_available: self.obs_available,
            obs_used: self.obs_used,
            tracks_available: self.tracks_available,
            tracks_used: self.tracks_used,
            residuals_accepted_pct: self.residuals_accepted_pct,
            weighted_rms: self.weighted_rms,
        }
    }
}

/// CDM additional parameters (CCSDS 508.0-B-1 table 3-4).
#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct CdmAdditionalParametersJs {
    comments: Vec<String>,
    area_pc_m2: Option<f64>,
    area_drg_m2: Option<f64>,
    area_srp_m2: Option<f64>,
    mass_kg: Option<f64>,
    cd_area_over_mass_m2_kg: Option<f64>,
    cr_area_over_mass_m2_kg: Option<f64>,
    thrust_acceleration_m_s2: Option<f64>,
    sedr_w_kg: Option<f64>,
}

impl CdmAdditionalParametersJs {
    fn from_core(v: &CdmAdditionalParameters) -> Self {
        Self {
            comments: v.comments.clone(),
            area_pc_m2: v.area_pc_m2,
            area_drg_m2: v.area_drg_m2,
            area_srp_m2: v.area_srp_m2,
            mass_kg: v.mass_kg,
            cd_area_over_mass_m2_kg: v.cd_area_over_mass_m2_kg,
            cr_area_over_mass_m2_kg: v.cr_area_over_mass_m2_kg,
            thrust_acceleration_m_s2: v.thrust_acceleration_m_s2,
            sedr_w_kg: v.sedr_w_kg,
        }
    }

    fn to_core(&self) -> CdmAdditionalParameters {
        CdmAdditionalParameters {
            comments: self.comments.clone(),
            area_pc_m2: self.area_pc_m2,
            area_drg_m2: self.area_drg_m2,
            area_srp_m2: self.area_srp_m2,
            mass_kg: self.mass_kg,
            cd_area_over_mass_m2_kg: self.cd_area_over_mass_m2_kg,
            cr_area_over_mass_m2_kg: self.cr_area_over_mass_m2_kg,
            thrust_acceleration_m_s2: self.thrust_acceleration_m_s2,
            sedr_w_kg: self.sedr_w_kg,
        }
    }
}

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
    drag_covariance_rtn: Option<Vec<f64>>,
    srp_covariance_rtn: Option<Vec<f64>>,
    thrust_covariance_rtn: Option<Vec<f64>>,
    metadata_comments: Vec<String>,
    od_parameters: CdmOdParametersJs,
    additional_parameters: CdmAdditionalParametersJs,
    state_comments: Vec<String>,
    covariance_comments: Vec<String>,
}

const CDM_OBJECT_META_KEYS: &[&str] = &[
    "objectDesignator",
    "catalogName",
    "objectName",
    "internationalDesignator",
    "objectType",
    "operatorContactPosition",
    "operatorOrganization",
    "operatorPhone",
    "operatorEmail",
    "ephemerisName",
    "covarianceMethod",
    "maneuverable",
    "orbitCenter",
    "refFrame",
    "gravityModel",
    "atmosphericModel",
    "nBodyPerturbations",
    "solarRadPressure",
    "earthTides",
    "intrackThrust",
    "velocityCovarianceRtn",
    "dragCovarianceRtn",
    "srpCovarianceRtn",
    "thrustCovarianceRtn",
    "metadataComments",
    "odParameters",
    "additionalParameters",
    "stateComments",
    "covarianceComments",
];

/// Optional CDM message-level fields: the header, the relative
/// metadata/data block and the screening volume. Absent fields are absent
/// from the message.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct CdmMeta {
    ccsds_cdm_vers: Option<String>,
    comments: Vec<String>,
    creation_date: Option<String>,
    originator: Option<String>,
    message_for: Option<String>,
    message_id: Option<String>,
    relative_comments: Vec<String>,
    tca: Option<String>,
    miss_distance_m: Option<f64>,
    relative_speed_m_s: Option<f64>,
    relative_position_rtn_m: [Option<f64>; 3],
    relative_velocity_rtn_m_s: [Option<f64>; 3],
    start_screen_period: Option<String>,
    stop_screen_period: Option<String>,
    screen_volume_frame: Option<String>,
    screen_volume_shape: Option<String>,
    screen_volume_m: [Option<f64>; 3],
    screen_entry_time: Option<String>,
    screen_exit_time: Option<String>,
    collision_probability: Option<f64>,
    collision_probability_method: Option<String>,
    hard_body_radius_m: Option<f64>,
}

const CDM_META_KEYS: &[&str] = &[
    "ccsdsCdmVers",
    "comments",
    "creationDate",
    "originator",
    "messageFor",
    "messageId",
    "relativeComments",
    "tca",
    "missDistanceM",
    "relativeSpeedMS",
    "relativePositionRtnM",
    "relativeVelocityRtnMS",
    "startScreenPeriod",
    "stopScreenPeriod",
    "screenVolumeFrame",
    "screenVolumeShape",
    "screenVolumeM",
    "screenEntryTime",
    "screenExitTime",
    "collisionProbability",
    "collisionProbabilityMethod",
    "hardBodyRadiusM",
];

fn parse_meta<T: Default + for<'de> Deserialize<'de>>(
    value: JsValue,
    label: &str,
    known: &[&str],
) -> Result<T, JsValue> {
    if value.is_undefined() || value.is_null() {
        Ok(T::default())
    } else {
        reject_unknown_keys(&value, label, known)?;
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
    /// `solarRadPressure`, `earthTides`, `intrackThrust`), the block comments
    /// (`metadataComments`, `stateComments`, `covarianceComments`), the
    /// `odParameters` and `additionalParameters` blocks, and the optional
    /// covariance rows 4 to 9: `velocityCovarianceRtn` (15 values, rows 4-6),
    /// `dragCovarianceRtn` (7, row 7), `srpCovarianceRtn` (8, row 8) and
    /// `thrustCovarianceRtn` (9, row 9).
    #[wasm_bindgen(constructor)]
    pub fn new(
        position_km: &[f64],
        velocity_km_s: &[f64],
        covariance_rtn: &[f64],
        #[wasm_bindgen(unchecked_param_type = "CdmObjectMeta | undefined | null")] meta: JsValue,
    ) -> Result<CdmObject, JsValue> {
        let p = vec3("positionKm", position_km)?;
        let v = vec3("velocityKmS", velocity_km_s)?;
        let cov = vec6("covarianceRtn", covariance_rtn)?;
        let m: CdmObjectMeta = parse_meta(meta, "CdmObject meta", CDM_OBJECT_META_KEYS)?;
        let velocity_covariance_rtn = match &m.velocity_covariance_rtn {
            Some(values) => Some(vec15("velocityCovarianceRtn", values)?),
            None => None,
        };
        let drag_covariance_rtn = match &m.drag_covariance_rtn {
            Some(values) => Some(fixed::<7>("dragCovarianceRtn", values)?),
            None => None,
        };
        let srp_covariance_rtn = match &m.srp_covariance_rtn {
            Some(values) => Some(fixed::<8>("srpCovarianceRtn", values)?),
            None => None,
        };
        let thrust_covariance_rtn = match &m.thrust_covariance_rtn {
            Some(values) => Some(fixed::<9>("thrustCovarianceRtn", values)?),
            None => None,
        };
        Ok(CdmObject {
            inner: CoreCdmObject {
                metadata_comments: m.metadata_comments,
                od_parameters: m.od_parameters.to_core(),
                additional_parameters: m.additional_parameters.to_core(),
                state_comments: m.state_comments,
                covariance_comments: m.covariance_comments,
                drag_covariance_rtn,
                srp_covariance_rtn,
                thrust_covariance_rtn,
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

    /// Covariance row 7 (`CDRG_R` .. `CDRG_DRG`), 7 values, or `undefined`.
    #[wasm_bindgen(getter, js_name = dragCovarianceRtn)]
    pub fn drag_covariance_rtn(&self) -> Option<Vec<f64>> {
        self.inner.drag_covariance_rtn.map(|v| v.to_vec())
    }

    /// Covariance row 8 (`CSRP_R` .. `CSRP_SRP`), 8 values, or `undefined`.
    #[wasm_bindgen(getter, js_name = srpCovarianceRtn)]
    pub fn srp_covariance_rtn(&self) -> Option<Vec<f64>> {
        self.inner.srp_covariance_rtn.map(|v| v.to_vec())
    }

    /// Covariance row 9 (`CTHR_R` .. `CTHR_THR`), 9 values, or `undefined`.
    #[wasm_bindgen(getter, js_name = thrustCovarianceRtn)]
    pub fn thrust_covariance_rtn(&self) -> Option<Vec<f64>> {
        self.inner.thrust_covariance_rtn.map(|v| v.to_vec())
    }

    /// Comments of the metadata block, in source order.
    #[wasm_bindgen(getter, js_name = metadataComments)]
    pub fn metadata_comments(&self) -> Vec<String> {
        self.inner.metadata_comments.clone()
    }

    /// Comments of the state vector block, in source order.
    #[wasm_bindgen(getter, js_name = stateComments)]
    pub fn state_comments(&self) -> Vec<String> {
        self.inner.state_comments.clone()
    }

    /// Comments of the covariance block, in source order.
    #[wasm_bindgen(getter, js_name = covarianceComments)]
    pub fn covariance_comments(&self) -> Vec<String> {
        self.inner.covariance_comments.clone()
    }

    /// The orbit-determination parameters block.
    #[wasm_bindgen(getter, js_name = odParameters, unchecked_return_type = "CdmOdParameters")]
    pub fn od_parameters(&self) -> Result<JsValue, JsValue> {
        to_plain_js(
            &CdmOdParametersJs::from_core(&self.inner.od_parameters),
            "CDM OD parameters",
        )
    }

    /// The additional parameters block.
    #[wasm_bindgen(
        getter,
        js_name = additionalParameters,
        unchecked_return_type = "CdmAdditionalParameters"
    )]
    pub fn additional_parameters(&self) -> Result<JsValue, JsValue> {
        to_plain_js(
            &CdmAdditionalParametersJs::from_core(&self.inner.additional_parameters),
            "CDM additional parameters",
        )
    }

    /// The symmetric RTN covariance of the rows the object holds, 3x3 to 9x9,
    /// as an array of rows, validated positive semidefinite. Throws a
    /// `CdmError` (`INVALID_FIELD` for `covariance_rtn`) when it is not, or
    /// when a row is given while an earlier row is absent.
    #[wasm_bindgen(js_name = toCovarianceRtn, unchecked_return_type = "number[][]")]
    pub fn to_covariance_rtn(&self) -> Result<JsValue, JsValue> {
        let rows = self.inner.to_covariance_rtn().map_err(cdm_error)?;
        to_plain_js(&rows, "CDM RTN covariance")
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
    /// fields: the header (`ccsdsCdmVers`, `comments`, `creationDate`,
    /// `originator`, `messageFor`, `messageId`), the relative metadata/data
    /// (`relativeComments`, `tca`, `missDistanceM`, `relativeSpeedMS`,
    /// `relativePositionRtnM`, `relativeVelocityRtnMS`, the screening period,
    /// volume frame, shape, size `screenVolumeM` and entry and exit times) and
    /// the collision probability and hard-body radius.
    #[wasm_bindgen(constructor)]
    pub fn new(
        object1: &CdmObject,
        object2: &CdmObject,
        #[wasm_bindgen(unchecked_param_type = "CdmMeta | undefined | null")] meta: JsValue,
    ) -> Result<Cdm, JsValue> {
        let m: CdmMeta = parse_meta(meta, "Cdm meta", CDM_META_KEYS)?;
        Ok(Cdm {
            inner: CdmKvn {
                ccsds_cdm_vers: m.ccsds_cdm_vers,
                comments: m.comments,
                message_for: m.message_for,
                relative_comments: m.relative_comments,
                relative_position_rtn_m: m.relative_position_rtn_m,
                relative_velocity_rtn_m_s: m.relative_velocity_rtn_m_s,
                start_screen_period: m.start_screen_period,
                stop_screen_period: m.stop_screen_period,
                screen_volume_frame: m.screen_volume_frame,
                screen_volume_shape: m.screen_volume_shape,
                screen_volume_m: m.screen_volume_m,
                screen_entry_time: m.screen_entry_time,
                screen_exit_time: m.screen_exit_time,
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

    /// `CCSDS_CDM_VERS` as the message states it, or `undefined`.
    #[wasm_bindgen(getter, js_name = ccsdsCdmVers)]
    pub fn ccsds_cdm_vers(&self) -> Option<String> {
        self.inner.ccsds_cdm_vers.clone()
    }

    /// Header comments, in source order.
    #[wasm_bindgen(getter)]
    pub fn comments(&self) -> Vec<String> {
        self.inner.comments.clone()
    }

    /// `MESSAGE_FOR`.
    #[wasm_bindgen(getter, js_name = messageFor)]
    pub fn message_for(&self) -> Option<String> {
        self.inner.message_for.clone()
    }

    /// Comments of the relative metadata/data block, in source order.
    #[wasm_bindgen(getter, js_name = relativeComments)]
    pub fn relative_comments(&self) -> Vec<String> {
        self.inner.relative_comments.clone()
    }

    /// Relative position `[R, T, N]`, metres, each `null` when absent.
    #[wasm_bindgen(getter, js_name = relativePositionRtnM, unchecked_return_type = "Array<number | null>")]
    pub fn relative_position_rtn_m(&self) -> Result<JsValue, JsValue> {
        to_plain_js(&self.inner.relative_position_rtn_m, "CDM relative position")
    }

    /// Relative velocity `[R, T, N]`, m/s, each `null` when absent.
    #[wasm_bindgen(getter, js_name = relativeVelocityRtnMS, unchecked_return_type = "Array<number | null>")]
    pub fn relative_velocity_rtn_m_s(&self) -> Result<JsValue, JsValue> {
        to_plain_js(
            &self.inner.relative_velocity_rtn_m_s,
            "CDM relative velocity",
        )
    }

    /// `START_SCREEN_PERIOD`.
    #[wasm_bindgen(getter, js_name = startScreenPeriod)]
    pub fn start_screen_period(&self) -> Option<String> {
        self.inner.start_screen_period.clone()
    }

    /// `STOP_SCREEN_PERIOD`.
    #[wasm_bindgen(getter, js_name = stopScreenPeriod)]
    pub fn stop_screen_period(&self) -> Option<String> {
        self.inner.stop_screen_period.clone()
    }

    /// `SCREEN_VOLUME_FRAME`.
    #[wasm_bindgen(getter, js_name = screenVolumeFrame)]
    pub fn screen_volume_frame(&self) -> Option<String> {
        self.inner.screen_volume_frame.clone()
    }

    /// `SCREEN_VOLUME_SHAPE`.
    #[wasm_bindgen(getter, js_name = screenVolumeShape)]
    pub fn screen_volume_shape(&self) -> Option<String> {
        self.inner.screen_volume_shape.clone()
    }

    /// Screening volume size `[X, Y, Z]`, metres, each `null` when absent.
    #[wasm_bindgen(getter, js_name = screenVolumeM, unchecked_return_type = "Array<number | null>")]
    pub fn screen_volume_m(&self) -> Result<JsValue, JsValue> {
        to_plain_js(&self.inner.screen_volume_m, "CDM screening volume")
    }

    /// `SCREEN_ENTRY_TIME`.
    #[wasm_bindgen(getter, js_name = screenEntryTime)]
    pub fn screen_entry_time(&self) -> Option<String> {
        self.inner.screen_entry_time.clone()
    }

    /// `SCREEN_EXIT_TIME`.
    #[wasm_bindgen(getter, js_name = screenExitTime)]
    pub fn screen_exit_time(&self) -> Option<String> {
        self.inner.screen_exit_time.clone()
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

    /// Encode this message to CCSDS CDM KVN text. Throws a `CdmError` for what
    /// the reader would not return unchanged (`UNWRITABLE_TEXT`,
    /// `HARD_BODY_RADIUS_COMMENT`) and for a non-finite number.
    #[wasm_bindgen(js_name = toKvnString)]
    pub fn to_kvn_string(&self) -> Result<String, JsValue> {
        encode_kvn(&self.inner).map_err(cdm_error)
    }

    /// Encode this message to CCSDS CDM XML text. Throws a `CdmError` as
    /// `toKvnString` does.
    #[wasm_bindgen(js_name = toXmlString)]
    pub fn to_xml_string(&self) -> Result<String, JsValue> {
        encode_xml(&self.inner).map_err(cdm_error)
    }
}

/// Parse CCSDS CDM KVN text. Throws a `CdmError` on a parse failure.
#[wasm_bindgen(js_name = parseCdmKvn)]
pub fn parse_cdm_kvn(text: &str) -> Result<Cdm, JsValue> {
    parse_kvn(text)
        .map(|inner| Cdm { inner })
        .map_err(cdm_error)
}

/// Parse CCSDS CDM XML text. Throws a `CdmError` on a parse failure.
#[wasm_bindgen(js_name = parseCdmXml)]
pub fn parse_cdm_xml(text: &str) -> Result<Cdm, JsValue> {
    parse_xml(text)
        .map(|inner| Cdm { inner })
        .map_err(cdm_error)
}
