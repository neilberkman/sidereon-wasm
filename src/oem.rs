//! CCSDS OEM binding: the canonical Orbit Ephemeris Message container and its
//! segment, metadata, state, and covariance blocks, plus KVN/XML parse and
//! encode. All grammar and serialization live in `sidereon_core::astro::oem`;
//! this module marshals fields, optional blocks, segment arrays, and the flat
//! 6x6 covariance.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::astro::oem::{
    encode_kvn, encode_xml, parse_kvn, parse_xml, Oem as CoreOem, OemComment as CoreOemComment,
    OemCovariance as CoreOemCovariance, OemMetadata as CoreOemMetadata,
    OemSegment as CoreOemSegment, OemSkippedState as CoreOemSkippedState, OemState as CoreOemState,
    OemStateLineError,
};

use crate::error::{reject_unknown_keys, to_plain_js, type_error};
use crate::marshal::{
    covariance6_error, covariance6_flat, lower_triangle21_from_input, lower_triangle21_to_full,
    vec3,
};
use crate::ndm_error::oem_error;

/// Optional OEM header fields. `ccsdsOemVers` defaults to `"2.0"`; every
/// other absent field is absent from the message.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct OemHeaderMeta {
    ccsds_oem_vers: Option<String>,
    classification: Option<String>,
    creation_date: Option<String>,
    originator: Option<String>,
    message_id: Option<String>,
    comments: Vec<String>,
}

const OEM_HEADER_KEYS: &[&str] = &[
    "ccsdsOemVers",
    "classification",
    "creationDate",
    "originator",
    "messageId",
    "comments",
];

/// Optional OEM segment-metadata fields.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct OemMetadataMeta {
    ref_frame_epoch: Option<String>,
    useable_start_time: Option<String>,
    useable_stop_time: Option<String>,
    interpolation: Option<String>,
    interpolation_degree: Option<u32>,
    comments: Vec<String>,
}

const OEM_METADATA_KEYS: &[&str] = &[
    "refFrameEpoch",
    "useableStartTime",
    "useableStopTime",
    "interpolation",
    "interpolationDegree",
    "comments",
];

/// A comment among the ephemeris lines or covariance matrices of a segment:
/// `position` is the number of items of its list that precede it.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OemCommentJs {
    position: usize,
    text: String,
}

impl OemCommentJs {
    fn from_core(value: &CoreOemComment) -> Self {
        Self {
            position: value.position,
            text: value.text.clone(),
        }
    }

    fn to_core(&self) -> CoreOemComment {
        CoreOemComment {
            position: self.position,
            text: self.text.clone(),
        }
    }
}

fn comments_from_js(value: JsValue, label: &str) -> Result<Vec<CoreOemComment>, JsValue> {
    if value.is_undefined() || value.is_null() {
        return Ok(Vec::new());
    }
    let rows: Vec<OemCommentJs> = serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid {label}: {e}")))?;
    Ok(rows.iter().map(OemCommentJs::to_core).collect())
}

fn comments_to_js(rows: &[CoreOemComment], label: &str) -> Result<JsValue, JsValue> {
    let rows: Vec<OemCommentJs> = rows.iter().map(OemCommentJs::from_core).collect();
    to_plain_js(&rows, label)
}

/// A KVN ephemeris data line the forgiving reader skipped.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OemSkippedStateJs {
    line: usize,
    segment: usize,
    text: String,
    /// `"itemCount"` or `"invalidField"`.
    reason: &'static str,
    /// The number of items the line holds, for `"itemCount"`.
    item_count: Option<usize>,
    /// The field that failed, for `"invalidField"`.
    field: Option<&'static str>,
    /// The validation category, for `"invalidField"`.
    issue: Option<String>,
}

impl From<&CoreOemSkippedState> for OemSkippedStateJs {
    fn from(value: &CoreOemSkippedState) -> Self {
        let (reason, item_count, field, issue) = match &value.reason {
            OemStateLineError::ItemCount(count) => ("itemCount", Some(*count), None, None),
            OemStateLineError::InvalidField { field, kind } => {
                ("invalidField", None, Some(*field), Some(kind.to_string()))
            }
        };
        Self {
            line: value.line,
            segment: value.segment,
            text: value.text.clone(),
            reason,
            item_count,
            field,
            issue,
        }
    }
}

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

/// OEM segment metadata: object identity, frame, time system, and span.
#[wasm_bindgen]
#[derive(Clone)]
pub struct OemMetadata {
    inner: CoreOemMetadata,
}

#[wasm_bindgen]
impl OemMetadata {
    /// Build OEM segment metadata. The seven leading arguments are required;
    /// `meta` carries the optional fields (`refFrameEpoch`, `useableStartTime`,
    /// `useableStopTime`, `interpolation`, `interpolationDegree`, and the
    /// block `comments`).
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
        #[wasm_bindgen(unchecked_param_type = "OemMetadataMeta | undefined | null")] meta: JsValue,
    ) -> Result<OemMetadata, JsValue> {
        let m: OemMetadataMeta = parse_meta(meta, "OemMetadata meta", OEM_METADATA_KEYS)?;
        Ok(OemMetadata {
            inner: CoreOemMetadata {
                comments: m.comments,
                object_name,
                object_id,
                center_name,
                ref_frame,
                ref_frame_epoch: m.ref_frame_epoch,
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

    /// `REF_FRAME_EPOCH` as written, or `undefined`.
    #[wasm_bindgen(getter, js_name = refFrameEpoch)]
    pub fn ref_frame_epoch(&self) -> Option<String> {
        self.inner.ref_frame_epoch.clone()
    }

    /// Metadata comments, in source order.
    #[wasm_bindgen(getter)]
    pub fn comments(&self) -> Vec<String> {
        self.inner.comments.clone()
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
    /// Build an OEM covariance block from the 21 lower-triangle values
    /// (`CX_X`, `CY_X`, `CY_Y`, ...) or a length-36 row-major symmetric
    /// matrix for the `[r, v]` state. The values are kept as given; no
    /// definiteness check is applied, as a message holds them as stated.
    /// `covRefFrame` is the optional frame label.
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
                lower_triangle: lower_triangle21_from_input("matrix", matrix)?,
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

    /// The 21 lower-triangle values exactly as read, row by row.
    #[wasm_bindgen(getter, js_name = lowerTriangle)]
    pub fn lower_triangle(&self) -> Vec<f64> {
        self.inner.lower_triangle.to_vec()
    }

    /// The stated values as a length-36 row-major symmetric matrix, without
    /// validation.
    #[wasm_bindgen(getter)]
    pub fn matrix(&self) -> Vec<f64> {
        lower_triangle21_to_full(&self.inner.lower_triangle)
    }

    /// The matrix validated as a state covariance (finite and positive
    /// semidefinite within the covariance tolerance), as a length-36 row-major
    /// `Float64Array`. Throws a `RangeError` when it is not one.
    #[wasm_bindgen(js_name = toValidatedMatrix)]
    pub fn to_validated_matrix(&self) -> Result<Vec<f64>, JsValue> {
        self.inner
            .to_covariance6()
            .map(|covariance| covariance6_flat(&covariance))
            .map_err(|error| covariance6_error("covariance", error))
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
    /// empty) covariance blocks. `dataComments` and `covarianceComments` are
    /// optional `{ position, text }` arrays placing each comment after
    /// `position` state lines or covariance matrices.
    #[wasm_bindgen(constructor)]
    pub fn new(
        metadata: &OemMetadata,
        states: Vec<OemState>,
        covariances: Vec<OemCovariance>,
        #[wasm_bindgen(unchecked_param_type = "OemComment[] | undefined | null")]
        data_comments: JsValue,
        #[wasm_bindgen(unchecked_param_type = "OemComment[] | undefined | null")]
        covariance_comments: JsValue,
    ) -> Result<OemSegment, JsValue> {
        Ok(OemSegment {
            inner: CoreOemSegment {
                metadata: metadata.inner.clone(),
                data_comments: comments_from_js(data_comments, "dataComments")?,
                states: states.into_iter().map(|s| s.inner).collect(),
                covariance_comments: comments_from_js(covariance_comments, "covarianceComments")?,
                covariances: covariances.into_iter().map(|c| c.inner).collect(),
            },
        })
    }

    /// Comments among the ephemeris lines, as `{ position, text }`.
    #[wasm_bindgen(getter, js_name = dataComments, unchecked_return_type = "OemComment[]")]
    pub fn data_comments(&self) -> Result<JsValue, JsValue> {
        comments_to_js(&self.inner.data_comments, "OEM data comments")
    }

    /// Comments among the covariance matrices, as `{ position, text }`.
    #[wasm_bindgen(getter, js_name = covarianceComments, unchecked_return_type = "OemComment[]")]
    pub fn covariance_comments(&self) -> Result<JsValue, JsValue> {
        comments_to_js(&self.inner.covariance_comments, "OEM covariance comments")
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
    /// fields (`ccsdsOemVers`, `classification`, `creationDate`, `originator`,
    /// `messageId`, header `comments`).
    #[wasm_bindgen(constructor)]
    pub fn new(
        segments: Vec<OemSegment>,
        #[wasm_bindgen(unchecked_param_type = "OemMeta | undefined | null")] meta: JsValue,
    ) -> Result<Oem, JsValue> {
        if segments.is_empty() {
            return Err(type_error("Oem requires at least one segment"));
        }
        let header: OemHeaderMeta = parse_meta(meta, "Oem meta", OEM_HEADER_KEYS)?;
        Ok(Oem {
            inner: CoreOem {
                ccsds_oem_vers: header.ccsds_oem_vers.unwrap_or_else(|| "2.0".to_string()),
                comments: header.comments,
                classification: header.classification,
                creation_date: header.creation_date,
                originator: header.originator,
                message_id: header.message_id,
                segments: segments.into_iter().map(|s| s.inner).collect(),
                skipped_states: Vec::new(),
            },
        })
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

    /// Header comments, in source order.
    #[wasm_bindgen(getter)]
    pub fn comments(&self) -> Vec<String> {
        self.inner.comments.clone()
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

    /// KVN ephemeris data lines the forgiving reader skipped, in input order,
    /// as `{ line, segment, text, reason, itemCount, field, issue }`: `line`
    /// is one-based, `segment` zero-based, and `reason` is `"itemCount"` (with
    /// the number of items the line holds) or `"invalidField"` (with the field
    /// and validation category). Empty for a constructed or XML-parsed message.
    #[wasm_bindgen(getter, js_name = skippedStates, unchecked_return_type = "OemSkippedState[]")]
    pub fn skipped_states(&self) -> Result<JsValue, JsValue> {
        let rows: Vec<OemSkippedStateJs> = self
            .inner
            .skipped_states
            .iter()
            .map(OemSkippedStateJs::from)
            .collect();
        to_plain_js(&rows, "skipped OEM states")
    }

    /// Number of entries in `skippedStates`.
    #[wasm_bindgen(getter, js_name = skippedStateCount)]
    pub fn skipped_state_count(&self) -> usize {
        self.inner.skipped_states.len()
    }

    /// Encode this OEM to CCSDS OEM KVN text. Throws an `OemError` for what
    /// the reader would not return unchanged (`UNWRITABLE_TEXT`) and for a
    /// non-finite number (`INVALID_FIELD`).
    #[wasm_bindgen(js_name = toKvnString)]
    pub fn to_kvn_string(&self) -> Result<String, JsValue> {
        encode_kvn(&self.inner).map_err(oem_error)
    }

    /// Encode this OEM to CCSDS OEM XML text. Throws an `OemError` as
    /// `toKvnString` does.
    #[wasm_bindgen(js_name = toXmlString)]
    pub fn to_xml_string(&self) -> Result<String, JsValue> {
        encode_xml(&self.inner).map_err(oem_error)
    }
}

/// Parse CCSDS OEM KVN text. The KVN reader is forgiving: malformed ephemeris
/// lines are skipped and reported in `skippedStates`. Throws an `OemError` on
/// a structural failure.
#[wasm_bindgen(js_name = parseOemKvn)]
pub fn parse_oem_kvn(text: &str) -> Result<Oem, JsValue> {
    parse_kvn(text)
        .map(|inner| Oem { inner })
        .map_err(oem_error)
}

/// Parse CCSDS OEM XML text. Throws an `OemError` on a parse failure.
#[wasm_bindgen(js_name = parseOemXml)]
pub fn parse_oem_xml(text: &str) -> Result<Oem, JsValue> {
    parse_xml(text)
        .map(|inner| Oem { inner })
        .map_err(oem_error)
}
