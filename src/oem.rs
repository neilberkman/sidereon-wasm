//! CCSDS OEM binding: the canonical Orbit Ephemeris Message container and its
//! segment, metadata, state, and covariance blocks, plus KVN/XML parse and
//! encode. All grammar and serialization live in `sidereon_core::astro::oem`;
//! this module marshals fields, optional blocks, segment arrays, and the flat
//! 6x6 covariance.

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use sidereon_core::astro::oem::{
    encode_kvn, encode_xml, parse_kvn, parse_xml, Oem as CoreOem,
    OemCovariance as CoreOemCovariance, OemMetadata as CoreOemMetadata,
    OemSegment as CoreOemSegment, OemState as CoreOemState,
};

use crate::error::{engine_error, type_error};
use crate::marshal::{covariance6_flat, covariance6_from_flat, vec3};

/// Optional OEM header fields, defaulting to the CCSDS-standard values.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct OemHeaderMeta {
    ccsds_oem_vers: Option<String>,
    creation_date: Option<String>,
    originator: Option<String>,
}

/// Optional OEM segment-metadata fields.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct OemMetadataMeta {
    useable_start_time: Option<String>,
    useable_stop_time: Option<String>,
    interpolation: Option<String>,
    interpolation_degree: Option<u32>,
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

/// OEM segment metadata: object identity, frame, time system, and span.
#[wasm_bindgen]
#[derive(Clone)]
pub struct OemMetadata {
    inner: CoreOemMetadata,
}

#[wasm_bindgen]
impl OemMetadata {
    /// Build OEM segment metadata. The seven leading arguments are required;
    /// `meta` carries the optional fields (`useableStartTime`, `useableStopTime`,
    /// `interpolation`, `interpolationDegree`).
    #[wasm_bindgen(constructor)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        object_name: String,
        object_id: String,
        center_name: String,
        ref_frame: String,
        time_system: String,
        start_time: String,
        stop_time: String,
        meta: JsValue,
    ) -> Result<OemMetadata, JsValue> {
        let m: OemMetadataMeta = parse_meta(meta, "OemMetadata meta")?;
        Ok(OemMetadata {
            inner: CoreOemMetadata {
                object_name,
                object_id,
                center_name,
                ref_frame,
                time_system,
                start_time,
                stop_time,
                useable_start_time: m.useable_start_time,
                useable_stop_time: m.useable_stop_time,
                interpolation: m.interpolation,
                interpolation_degree: m.interpolation_degree,
            },
        })
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

    /// Segment start time text.
    #[wasm_bindgen(getter, js_name = startTime)]
    pub fn start_time(&self) -> String {
        self.inner.start_time.clone()
    }

    /// Segment stop time text.
    #[wasm_bindgen(getter, js_name = stopTime)]
    pub fn stop_time(&self) -> String {
        self.inner.stop_time.clone()
    }

    /// Useable start time text, or `undefined`.
    #[wasm_bindgen(getter, js_name = useableStartTime)]
    pub fn useable_start_time(&self) -> Option<String> {
        self.inner.useable_start_time.clone()
    }

    /// Useable stop time text, or `undefined`.
    #[wasm_bindgen(getter, js_name = useableStopTime)]
    pub fn useable_stop_time(&self) -> Option<String> {
        self.inner.useable_stop_time.clone()
    }

    /// Interpolation method label, or `undefined`.
    #[wasm_bindgen(getter)]
    pub fn interpolation(&self) -> Option<String> {
        self.inner.interpolation.clone()
    }

    /// Interpolation polynomial degree, or `undefined`.
    #[wasm_bindgen(getter, js_name = interpolationDegree)]
    pub fn interpolation_degree(&self) -> Option<u32> {
        self.inner.interpolation_degree
    }
}

/// One OEM Cartesian state sample.
#[wasm_bindgen]
#[derive(Clone)]
pub struct OemState {
    inner: CoreOemState,
}

#[wasm_bindgen]
impl OemState {
    /// Build an OEM state sample. `epoch` is carried as text; `positionKm` and
    /// `velocityKmS` are length-3 `Float64Array`s. `accelerationKmS2` is an
    /// optional length-3 `Float64Array` (pass `undefined` for a position/velocity
    /// sample).
    #[wasm_bindgen(constructor)]
    pub fn new(
        epoch: String,
        position_km: &[f64],
        velocity_km_s: &[f64],
        acceleration_km_s2: Option<Vec<f64>>,
    ) -> Result<OemState, JsValue> {
        let acceleration_km_s2 = match acceleration_km_s2 {
            Some(values) => Some(vec3("accelerationKmS2", &values)?),
            None => None,
        };
        Ok(OemState {
            inner: CoreOemState {
                epoch,
                position_km: vec3("positionKm", position_km)?,
                velocity_km_s: vec3("velocityKmS", velocity_km_s)?,
                acceleration_km_s2,
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

    /// Acceleration vector, km/s^2, length-3 `Float64Array`, or `undefined`.
    #[wasm_bindgen(getter, js_name = accelerationKmS2)]
    pub fn acceleration_km_s2(&self) -> Option<Vec<f64>> {
        self.inner.acceleration_km_s2.map(|a| a.to_vec())
    }
}

/// One OEM covariance block.
#[wasm_bindgen]
#[derive(Clone)]
pub struct OemCovariance {
    inner: CoreOemCovariance,
}

#[wasm_bindgen]
impl OemCovariance {
    /// Build an OEM covariance block. `matrix` is a length-36 row-major
    /// `Float64Array` for the `[r, v]` state; it must be finite, symmetric, and
    /// positive semidefinite. `covRefFrame` is the optional frame label.
    #[wasm_bindgen(constructor)]
    pub fn new(
        epoch: String,
        matrix: &[f64],
        cov_ref_frame: Option<String>,
    ) -> Result<OemCovariance, JsValue> {
        Ok(OemCovariance {
            inner: CoreOemCovariance {
                epoch,
                cov_ref_frame,
                matrix: covariance6_from_flat("matrix", matrix)?,
            },
        })
    }

    /// Epoch text.
    #[wasm_bindgen(getter)]
    pub fn epoch(&self) -> String {
        self.inner.epoch.clone()
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

/// One OEM metadata/data segment.
#[wasm_bindgen]
#[derive(Clone)]
pub struct OemSegment {
    inner: CoreOemSegment,
}

#[wasm_bindgen]
impl OemSegment {
    /// Build an OEM segment from its metadata, state samples, and (possibly
    /// empty) covariance blocks.
    #[wasm_bindgen(constructor)]
    pub fn new(
        metadata: &OemMetadata,
        states: Vec<OemState>,
        covariances: Vec<OemCovariance>,
    ) -> OemSegment {
        OemSegment {
            inner: CoreOemSegment {
                metadata: metadata.inner.clone(),
                states: states.into_iter().map(|s| s.inner).collect(),
                covariances: covariances.into_iter().map(|c| c.inner).collect(),
            },
        }
    }

    /// Segment metadata.
    #[wasm_bindgen(getter)]
    pub fn metadata(&self) -> OemMetadata {
        OemMetadata {
            inner: self.inner.metadata.clone(),
        }
    }

    /// State samples in segment order.
    #[wasm_bindgen(getter)]
    pub fn states(&self) -> Vec<OemState> {
        self.inner
            .states
            .iter()
            .cloned()
            .map(|inner| OemState { inner })
            .collect()
    }

    /// Covariance blocks in segment order.
    #[wasm_bindgen(getter)]
    pub fn covariances(&self) -> Vec<OemCovariance> {
        self.inner
            .covariances
            .iter()
            .cloned()
            .map(|inner| OemCovariance { inner })
            .collect()
    }
}

/// A canonical, format-agnostic CCSDS Orbit Ephemeris Message parsed from KVN or
/// XML.
#[wasm_bindgen]
#[derive(Clone)]
pub struct Oem {
    inner: CoreOem,
}

#[wasm_bindgen]
impl Oem {
    /// Build an OEM from one or more segments. `meta` carries the optional header
    /// fields (`ccsdsOemVers`, `creationDate`, `originator`).
    #[wasm_bindgen(constructor)]
    pub fn new(segments: Vec<OemSegment>, meta: JsValue) -> Result<Oem, JsValue> {
        if segments.is_empty() {
            return Err(type_error("Oem requires at least one segment"));
        }
        let header: OemHeaderMeta = parse_meta(meta, "Oem meta")?;
        Ok(Oem {
            inner: CoreOem {
                ccsds_oem_vers: header.ccsds_oem_vers.unwrap_or_else(|| "2.0".to_string()),
                creation_date: header.creation_date,
                originator: header.originator,
                segments: segments.into_iter().map(|s| s.inner).collect(),
                skipped_states: 0,
            },
        })
    }

    /// CCSDS OEM version string.
    #[wasm_bindgen(getter, js_name = ccsdsOemVers)]
    pub fn ccsds_oem_vers(&self) -> String {
        self.inner.ccsds_oem_vers.clone()
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

    /// Metadata/data segments in message order.
    #[wasm_bindgen(getter)]
    pub fn segments(&self) -> Vec<OemSegment> {
        self.inner
            .segments
            .iter()
            .cloned()
            .map(|inner| OemSegment { inner })
            .collect()
    }

    /// Number of segments.
    #[wasm_bindgen(getter, js_name = segmentCount)]
    pub fn segment_count(&self) -> usize {
        self.inner.segments.len()
    }

    /// Forgiving-parse count of ephemeris data lines skipped as malformed (KVN
    /// only; 0 for a constructed or XML-parsed message).
    #[wasm_bindgen(getter, js_name = skippedStates)]
    pub fn skipped_states(&self) -> usize {
        self.inner.skipped_states
    }

    /// Encode this OEM to CCSDS OEM KVN text.
    #[wasm_bindgen(js_name = toKvnString)]
    pub fn to_kvn_string(&self) -> String {
        encode_kvn(&self.inner)
    }

    /// Encode this OEM to CCSDS OEM XML text.
    #[wasm_bindgen(js_name = toXmlString)]
    pub fn to_xml_string(&self) -> String {
        encode_xml(&self.inner)
    }
}

/// Parse CCSDS OEM KVN text. The KVN reader is forgiving: malformed ephemeris
/// lines are skipped and counted in `skippedStates`. Throws an `Error` on a
/// structural failure.
#[wasm_bindgen(js_name = parseOemKvn)]
pub fn parse_oem_kvn(text: &str) -> Result<Oem, JsValue> {
    parse_kvn(text)
        .map(|inner| Oem { inner })
        .map_err(engine_error)
}

/// Parse CCSDS OEM XML text. Throws an `Error` on a parse failure.
#[wasm_bindgen(js_name = parseOemXml)]
pub fn parse_oem_xml(text: &str) -> Result<Oem, JsValue> {
    parse_xml(text)
        .map(|inner| Oem { inner })
        .map_err(engine_error)
}
