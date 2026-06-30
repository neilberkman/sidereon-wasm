//! CCSDS OPM binding: the canonical Orbit Parameter Message container and its
//! metadata, state, Keplerian, spacecraft, covariance, and maneuver blocks, plus
//! KVN/XML parse and encode. All grammar and serialization live in
//! `sidereon_core::astro::opm`; this module marshals fields, optional blocks, and
//! the flat 6x6 covariance.

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use sidereon_core::astro::opm::{
    encode_kvn, encode_xml, parse_kvn, parse_xml, Opm as CoreOpm, OpmAnomaly as CoreOpmAnomaly,
    OpmCovariance as CoreOpmCovariance, OpmKeplerian as CoreOpmKeplerian,
    OpmManeuver as CoreOpmManeuver, OpmMetadata as CoreOpmMetadata,
    OpmSpacecraft as CoreOpmSpacecraft, OpmState as CoreOpmState,
};

use crate::error::{engine_error, range_error, type_error};
use crate::marshal::{covariance6_flat, covariance6_from_flat, vec3};

/// Optional OPM header fields, defaulting to the CCSDS-standard values.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct OpmHeaderMeta {
    ccsds_opm_vers: Option<String>,
    creation_date: Option<String>,
    originator: Option<String>,
}

fn parse_header_meta(value: JsValue) -> Result<OpmHeaderMeta, JsValue> {
    if value.is_undefined() || value.is_null() {
        Ok(OpmHeaderMeta::default())
    } else {
        serde_wasm_bindgen::from_value(value)
            .map_err(|e| type_error(&format!("invalid Opm meta: {e}")))
    }
}

fn finite(value: f64, name: &str) -> Result<f64, JsValue> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(range_error(&format!("{name} must be finite")))
    }
}

/// OPM metadata block: object identity, center, reference frame, and time system.
#[wasm_bindgen]
#[derive(Clone)]
pub struct OpmMetadata {
    inner: CoreOpmMetadata,
}

#[wasm_bindgen]
impl OpmMetadata {
    /// Build the OPM metadata block. Every field is mandatory in CCSDS 502.0-B.
    #[wasm_bindgen(constructor)]
    pub fn new(
        object_name: String,
        object_id: String,
        center_name: String,
        ref_frame: String,
        time_system: String,
    ) -> OpmMetadata {
        OpmMetadata {
            inner: CoreOpmMetadata {
                object_name,
                object_id,
                center_name,
                ref_frame,
                time_system,
            },
        }
    }

    /// Object name.
    #[wasm_bindgen(getter, js_name = objectName)]
    pub fn object_name(&self) -> String {
        self.inner.object_name.clone()
    }

    /// Object id (COSPAR international designator).
    #[wasm_bindgen(getter, js_name = objectId)]
    pub fn object_id(&self) -> String {
        self.inner.object_id.clone()
    }

    /// Center name.
    #[wasm_bindgen(getter, js_name = centerName)]
    pub fn center_name(&self) -> String {
        self.inner.center_name.clone()
    }

    /// Reference frame.
    #[wasm_bindgen(getter, js_name = refFrame)]
    pub fn ref_frame(&self) -> String {
        self.inner.ref_frame.clone()
    }

    /// Time system.
    #[wasm_bindgen(getter, js_name = timeSystem)]
    pub fn time_system(&self) -> String {
        self.inner.time_system.clone()
    }
}

/// OPM Cartesian state vector at the message epoch.
#[wasm_bindgen]
#[derive(Clone)]
pub struct OpmState {
    inner: CoreOpmState,
}

#[wasm_bindgen]
impl OpmState {
    /// Build the OPM state. `epoch` is carried as text; `positionKm` and
    /// `velocityKmS` are length-3 `Float64Array`s in kilometres and km/s.
    #[wasm_bindgen(constructor)]
    pub fn new(
        epoch: String,
        position_km: &[f64],
        velocity_km_s: &[f64],
    ) -> Result<OpmState, JsValue> {
        Ok(OpmState {
            inner: CoreOpmState {
                epoch,
                position_km: vec3("positionKm", position_km)?,
                velocity_km_s: vec3("velocityKmS", velocity_km_s)?,
            },
        })
    }

    /// Epoch text.
    #[wasm_bindgen(getter)]
    pub fn epoch(&self) -> String {
        self.inner.epoch.clone()
    }

    /// Position vector, kilometres, length-3 `Float64Array`.
    #[wasm_bindgen(getter, js_name = positionKm)]
    pub fn position_km(&self) -> Vec<f64> {
        self.inner.position_km.to_vec()
    }

    /// Velocity vector, km/s, length-3 `Float64Array`.
    #[wasm_bindgen(getter, js_name = velocityKmS)]
    pub fn velocity_km_s(&self) -> Vec<f64> {
        self.inner.velocity_km_s.to_vec()
    }
}

/// Optional OPM Keplerian-elements block.
#[wasm_bindgen]
#[derive(Clone)]
pub struct OpmKeplerian {
    inner: CoreOpmKeplerian,
}

#[wasm_bindgen]
impl OpmKeplerian {
    /// Build the Keplerian block. Supply exactly one of `trueAnomalyDeg` or
    /// `meanAnomalyDeg`; throws a `TypeError` if neither or both are given.
    #[wasm_bindgen(constructor)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        semi_major_axis_km: f64,
        eccentricity: f64,
        inclination_deg: f64,
        ra_of_asc_node_deg: f64,
        arg_of_pericenter_deg: f64,
        gm_km3_s2: f64,
        true_anomaly_deg: Option<f64>,
        mean_anomaly_deg: Option<f64>,
    ) -> Result<OpmKeplerian, JsValue> {
        let anomaly = match (true_anomaly_deg, mean_anomaly_deg) {
            (Some(value), None) => CoreOpmAnomaly::True(finite(value, "trueAnomalyDeg")?),
            (None, Some(value)) => CoreOpmAnomaly::Mean(finite(value, "meanAnomalyDeg")?),
            (None, None) => {
                return Err(type_error(
                    "Keplerian block requires trueAnomalyDeg or meanAnomalyDeg",
                ))
            }
            (Some(_), Some(_)) => {
                return Err(type_error(
                    "Keplerian block cannot carry both trueAnomalyDeg and meanAnomalyDeg",
                ))
            }
        };
        Ok(OpmKeplerian {
            inner: CoreOpmKeplerian {
                semi_major_axis_km: finite(semi_major_axis_km, "semiMajorAxisKm")?,
                eccentricity: finite(eccentricity, "eccentricity")?,
                inclination_deg: finite(inclination_deg, "inclinationDeg")?,
                ra_of_asc_node_deg: finite(ra_of_asc_node_deg, "raOfAscNodeDeg")?,
                arg_of_pericenter_deg: finite(arg_of_pericenter_deg, "argOfPericenterDeg")?,
                anomaly,
                gm_km3_s2: finite(gm_km3_s2, "gmKm3S2")?,
            },
        })
    }

    /// Semi-major axis, kilometres.
    #[wasm_bindgen(getter, js_name = semiMajorAxisKm)]
    pub fn semi_major_axis_km(&self) -> f64 {
        self.inner.semi_major_axis_km
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

    /// True anomaly, degrees, or `undefined` when a mean anomaly was supplied.
    #[wasm_bindgen(getter, js_name = trueAnomalyDeg)]
    pub fn true_anomaly_deg(&self) -> Option<f64> {
        match self.inner.anomaly {
            CoreOpmAnomaly::True(value) => Some(value),
            CoreOpmAnomaly::Mean(_) => None,
        }
    }

    /// Mean anomaly, degrees, or `undefined` when a true anomaly was supplied.
    #[wasm_bindgen(getter, js_name = meanAnomalyDeg)]
    pub fn mean_anomaly_deg(&self) -> Option<f64> {
        match self.inner.anomaly {
            CoreOpmAnomaly::Mean(value) => Some(value),
            CoreOpmAnomaly::True(_) => None,
        }
    }

    /// Gravitational parameter, km^3/s^2.
    #[wasm_bindgen(getter, js_name = gmKm3S2)]
    pub fn gm_km3_s2(&self) -> f64 {
        self.inner.gm_km3_s2
    }
}

/// Optional OPM spacecraft-parameters block. Every sub-field is individually
/// optional.
#[wasm_bindgen]
#[derive(Clone)]
pub struct OpmSpacecraft {
    inner: CoreOpmSpacecraft,
}

#[wasm_bindgen]
impl OpmSpacecraft {
    /// Build the spacecraft-parameters block. Pass `undefined` for absent fields.
    #[wasm_bindgen(constructor)]
    pub fn new(
        mass_kg: Option<f64>,
        solar_rad_area_m2: Option<f64>,
        solar_rad_coeff: Option<f64>,
        drag_area_m2: Option<f64>,
        drag_coeff: Option<f64>,
    ) -> OpmSpacecraft {
        OpmSpacecraft {
            inner: CoreOpmSpacecraft {
                mass_kg,
                solar_rad_area_m2,
                solar_rad_coeff,
                drag_area_m2,
                drag_coeff,
            },
        }
    }

    /// Mass, kilograms.
    #[wasm_bindgen(getter, js_name = massKg)]
    pub fn mass_kg(&self) -> Option<f64> {
        self.inner.mass_kg
    }

    /// Solar-radiation-pressure area, square metres.
    #[wasm_bindgen(getter, js_name = solarRadAreaM2)]
    pub fn solar_rad_area_m2(&self) -> Option<f64> {
        self.inner.solar_rad_area_m2
    }

    /// Solar-radiation-pressure coefficient.
    #[wasm_bindgen(getter, js_name = solarRadCoeff)]
    pub fn solar_rad_coeff(&self) -> Option<f64> {
        self.inner.solar_rad_coeff
    }

    /// Drag area, square metres.
    #[wasm_bindgen(getter, js_name = dragAreaM2)]
    pub fn drag_area_m2(&self) -> Option<f64> {
        self.inner.drag_area_m2
    }

    /// Drag coefficient.
    #[wasm_bindgen(getter, js_name = dragCoeff)]
    pub fn drag_coeff(&self) -> Option<f64> {
        self.inner.drag_coeff
    }
}

/// Optional OPM 6x6 state covariance.
#[wasm_bindgen]
#[derive(Clone)]
pub struct OpmCovariance {
    inner: CoreOpmCovariance,
}

#[wasm_bindgen]
impl OpmCovariance {
    /// Build the covariance block. `matrix` is a length-36 row-major
    /// `Float64Array` for the `[r, v]` state; it must be finite, symmetric, and
    /// positive semidefinite. `covRefFrame` is the optional frame label.
    #[wasm_bindgen(constructor)]
    pub fn new(matrix: &[f64], cov_ref_frame: Option<String>) -> Result<OpmCovariance, JsValue> {
        Ok(OpmCovariance {
            inner: CoreOpmCovariance {
                cov_ref_frame,
                matrix: covariance6_from_flat("matrix", matrix)?,
            },
        })
    }

    /// Covariance reference-frame label, or `undefined`.
    #[wasm_bindgen(getter, js_name = covRefFrame)]
    pub fn cov_ref_frame(&self) -> Option<String> {
        self.inner.cov_ref_frame.clone()
    }

    /// The 6x6 state covariance as a length-36 row-major `Float64Array`.
    #[wasm_bindgen(getter)]
    pub fn matrix(&self) -> Vec<f64> {
        covariance6_flat(&self.inner.matrix)
    }
}

/// One OPM maneuver block. Every field is mandatory in CCSDS 502.0-B when a
/// maneuver is present.
#[wasm_bindgen]
#[derive(Clone)]
pub struct OpmManeuver {
    inner: CoreOpmManeuver,
}

#[wasm_bindgen]
impl OpmManeuver {
    /// Build a maneuver block. `dvKmS` is a length-3 `Float64Array` of the
    /// delta-v in km/s.
    #[wasm_bindgen(constructor)]
    pub fn new(
        epoch_ignition: String,
        duration_s: f64,
        delta_mass_kg: f64,
        ref_frame: String,
        dv_km_s: &[f64],
    ) -> Result<OpmManeuver, JsValue> {
        Ok(OpmManeuver {
            inner: CoreOpmManeuver {
                epoch_ignition,
                duration_s: finite(duration_s, "durationS")?,
                delta_mass_kg: finite(delta_mass_kg, "deltaMassKg")?,
                ref_frame,
                dv_km_s: vec3("dvKmS", dv_km_s)?,
            },
        })
    }

    /// Ignition epoch text.
    #[wasm_bindgen(getter, js_name = epochIgnition)]
    pub fn epoch_ignition(&self) -> String {
        self.inner.epoch_ignition.clone()
    }

    /// Maneuver duration, seconds.
    #[wasm_bindgen(getter, js_name = durationS)]
    pub fn duration_s(&self) -> f64 {
        self.inner.duration_s
    }

    /// Change in mass, kilograms.
    #[wasm_bindgen(getter, js_name = deltaMassKg)]
    pub fn delta_mass_kg(&self) -> f64 {
        self.inner.delta_mass_kg
    }

    /// Maneuver reference frame.
    #[wasm_bindgen(getter, js_name = refFrame)]
    pub fn ref_frame(&self) -> String {
        self.inner.ref_frame.clone()
    }

    /// Delta-v vector, km/s, length-3 `Float64Array`.
    #[wasm_bindgen(getter, js_name = dvKmS)]
    pub fn dv_km_s(&self) -> Vec<f64> {
        self.inner.dv_km_s.to_vec()
    }
}

/// A canonical, format-agnostic CCSDS Orbit Parameter Message parsed from KVN or
/// XML.
#[wasm_bindgen]
#[derive(Clone)]
pub struct Opm {
    inner: CoreOpm,
}

#[wasm_bindgen]
impl Opm {
    /// Build an OPM from its blocks. `keplerian`, `spacecraft`, and `covariance`
    /// are optional (pass `undefined`); `maneuvers` is an array (possibly empty).
    /// `meta` carries the optional header fields (`ccsdsOpmVers`, `creationDate`,
    /// `originator`).
    #[wasm_bindgen(constructor)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        metadata: &OpmMetadata,
        state: &OpmState,
        keplerian: Option<OpmKeplerian>,
        spacecraft: Option<OpmSpacecraft>,
        covariance: Option<OpmCovariance>,
        maneuvers: Vec<OpmManeuver>,
        meta: JsValue,
    ) -> Result<Opm, JsValue> {
        let header = parse_header_meta(meta)?;
        Ok(Opm {
            inner: CoreOpm {
                ccsds_opm_vers: header.ccsds_opm_vers.unwrap_or_else(|| "2.0".to_string()),
                creation_date: header.creation_date,
                originator: header.originator,
                metadata: metadata.inner.clone(),
                state: state.inner.clone(),
                keplerian: keplerian.map(|k| k.inner),
                spacecraft: spacecraft.map(|s| s.inner),
                covariance: covariance.map(|c| c.inner),
                maneuvers: maneuvers.into_iter().map(|m| m.inner).collect(),
            },
        })
    }

    /// CCSDS OPM version string.
    #[wasm_bindgen(getter, js_name = ccsdsOpmVers)]
    pub fn ccsds_opm_vers(&self) -> String {
        self.inner.ccsds_opm_vers.clone()
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

    /// Metadata block.
    #[wasm_bindgen(getter)]
    pub fn metadata(&self) -> OpmMetadata {
        OpmMetadata {
            inner: self.inner.metadata.clone(),
        }
    }

    /// Cartesian state vector.
    #[wasm_bindgen(getter)]
    pub fn state(&self) -> OpmState {
        OpmState {
            inner: self.inner.state.clone(),
        }
    }

    /// Keplerian-elements block, or `undefined`.
    #[wasm_bindgen(getter)]
    pub fn keplerian(&self) -> Option<OpmKeplerian> {
        self.inner
            .keplerian
            .clone()
            .map(|inner| OpmKeplerian { inner })
    }

    /// Spacecraft-parameters block, or `undefined`.
    #[wasm_bindgen(getter)]
    pub fn spacecraft(&self) -> Option<OpmSpacecraft> {
        self.inner
            .spacecraft
            .clone()
            .map(|inner| OpmSpacecraft { inner })
    }

    /// Covariance block, or `undefined`.
    #[wasm_bindgen(getter)]
    pub fn covariance(&self) -> Option<OpmCovariance> {
        self.inner
            .covariance
            .clone()
            .map(|inner| OpmCovariance { inner })
    }

    /// Maneuver blocks in message order.
    #[wasm_bindgen(getter)]
    pub fn maneuvers(&self) -> Vec<OpmManeuver> {
        self.inner
            .maneuvers
            .iter()
            .cloned()
            .map(|inner| OpmManeuver { inner })
            .collect()
    }

    /// Encode this OPM to CCSDS OPM KVN text.
    #[wasm_bindgen(js_name = toKvnString)]
    pub fn to_kvn_string(&self) -> String {
        encode_kvn(&self.inner)
    }

    /// Encode this OPM to CCSDS OPM XML text.
    #[wasm_bindgen(js_name = toXmlString)]
    pub fn to_xml_string(&self) -> String {
        encode_xml(&self.inner)
    }
}

/// Parse CCSDS OPM KVN text. Throws an `Error` on a parse failure.
#[wasm_bindgen(js_name = parseOpmKvn)]
pub fn parse_opm_kvn(text: &str) -> Result<Opm, JsValue> {
    parse_kvn(text)
        .map(|inner| Opm { inner })
        .map_err(engine_error)
}

/// Parse CCSDS OPM XML text. Throws an `Error` on a parse failure.
#[wasm_bindgen(js_name = parseOpmXml)]
pub fn parse_opm_xml(text: &str) -> Result<Opm, JsValue> {
    parse_xml(text)
        .map(|inner| Opm { inner })
        .map_err(engine_error)
}
