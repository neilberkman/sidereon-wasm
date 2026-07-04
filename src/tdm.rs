//! CCSDS TDM binding: parse a Tracking Data Message in KVN form, inspect its
//! canonical blocks, and encode it back through the core serializer.

use wasm_bindgen::prelude::*;

use sidereon_core::astro::tdm::{
    encode_kvn, parse_kvn, Tdm as CoreTdm, TdmDataRecord as CoreTdmDataRecord,
    TdmDataSection as CoreTdmDataSection, TdmField as CoreTdmField, TdmMetadata as CoreTdmMetadata,
    TdmObservable, TdmParticipant as CoreTdmParticipant, TdmPath as CoreTdmPath,
    TdmScalar as CoreTdmScalar, TdmSegment as CoreTdmSegment,
};

use crate::error::engine_error;

/// A KVN key/value field preserved in parse order.
#[wasm_bindgen]
#[derive(Clone)]
pub struct TdmField {
    inner: CoreTdmField,
}

impl From<CoreTdmField> for TdmField {
    fn from(inner: CoreTdmField) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl TdmField {
    /// The KVN keyword.
    #[wasm_bindgen(getter)]
    pub fn key(&self) -> String {
        self.inner.key.clone()
    }

    /// The trimmed KVN value.
    #[wasm_bindgen(getter)]
    pub fn value(&self) -> String {
        self.inner.value.clone()
    }
}

/// One named TDM tracking participant.
#[wasm_bindgen]
#[derive(Clone)]
pub struct TdmParticipant {
    inner: CoreTdmParticipant,
}

impl From<CoreTdmParticipant> for TdmParticipant {
    fn from(inner: CoreTdmParticipant) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl TdmParticipant {
    /// Numeric suffix from `PARTICIPANT_n`.
    #[wasm_bindgen(getter)]
    pub fn index(&self) -> u8 {
        self.inner.index
    }

    /// Participant name.
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.inner.name.clone()
    }
}

/// A parsed TDM signal path from `PATH`, `PATH_1`, or `PATH_2`.
#[wasm_bindgen]
#[derive(Clone)]
pub struct TdmPath {
    inner: CoreTdmPath,
}

impl From<CoreTdmPath> for TdmPath {
    fn from(inner: CoreTdmPath) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl TdmPath {
    /// Original path keyword.
    #[wasm_bindgen(getter)]
    pub fn key(&self) -> String {
        self.inner.key.clone()
    }

    /// Path suffix for `PATH_n`, or `undefined` for unindexed `PATH`.
    #[wasm_bindgen(getter)]
    pub fn index(&self) -> Option<u8> {
        self.inner.index
    }

    /// Participant indices listed in path order.
    #[wasm_bindgen(getter)]
    pub fn participants(&self) -> Vec<u8> {
        self.inner.participants.clone()
    }
}

/// A numeric TDM record value plus its exact decimal token.
#[wasm_bindgen]
#[derive(Clone)]
pub struct TdmScalar {
    inner: CoreTdmScalar,
}

impl From<CoreTdmScalar> for TdmScalar {
    fn from(inner: CoreTdmScalar) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl TdmScalar {
    /// Exact decimal or scientific-notation token read from the message.
    #[wasm_bindgen(getter)]
    pub fn text(&self) -> String {
        self.inner.text.clone()
    }

    /// Parsed finite `f64` value.
    #[wasm_bindgen(getter)]
    pub fn value(&self) -> f64 {
        self.inner.value
    }
}

fn observable_kind(observable: &TdmObservable) -> &'static str {
    match observable {
        TdmObservable::Range => "range",
        TdmObservable::DopplerInstantaneous => "dopplerInstantaneous",
        TdmObservable::DopplerIntegrated => "dopplerIntegrated",
        TdmObservable::ReceiveFreq { .. } => "receiveFreq",
        TdmObservable::TransmitFreq { .. } => "transmitFreq",
        TdmObservable::TransmitFreqRate { .. } => "transmitFreqRate",
        TdmObservable::Angle1 => "angle1",
        TdmObservable::Angle2 => "angle2",
        TdmObservable::Other(_) => "other",
    }
}

fn observable_participant(observable: &TdmObservable) -> Option<u8> {
    match observable {
        TdmObservable::ReceiveFreq { participant }
        | TdmObservable::TransmitFreq { participant }
        | TdmObservable::TransmitFreqRate { participant } => *participant,
        _ => None,
    }
}

fn observable_other(observable: &TdmObservable) -> Option<String> {
    match observable {
        TdmObservable::Other(name) => Some(name.clone()),
        _ => None,
    }
}

/// One time-tagged TDM tracking data record.
#[wasm_bindgen]
#[derive(Clone)]
pub struct TdmDataRecord {
    inner: CoreTdmDataRecord,
}

impl From<CoreTdmDataRecord> for TdmDataRecord {
    fn from(inner: CoreTdmDataRecord) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl TdmDataRecord {
    /// Observable family label such as `"range"` or `"receiveFreq"`.
    #[wasm_bindgen(getter, js_name = observableKind)]
    pub fn observable_kind(&self) -> String {
        observable_kind(&self.inner.observable).to_string()
    }

    /// Participant suffix for frequency records, or `undefined`.
    #[wasm_bindgen(getter, js_name = observableParticipant)]
    pub fn observable_participant(&self) -> Option<u8> {
        observable_participant(&self.inner.observable)
    }

    /// Original name for a table-defined keyword modeled as `other`.
    #[wasm_bindgen(getter, js_name = otherObservable)]
    pub fn other_observable(&self) -> Option<String> {
        observable_other(&self.inner.observable)
    }

    /// Original data keyword.
    #[wasm_bindgen(getter)]
    pub fn keyword(&self) -> String {
        self.inner.keyword.clone()
    }

    /// Raw epoch string.
    #[wasm_bindgen(getter)]
    pub fn epoch(&self) -> String {
        self.inner.epoch.clone()
    }

    /// Parsed numeric observable value.
    #[wasm_bindgen(getter)]
    pub fn value(&self) -> f64 {
        self.inner.value.value
    }

    /// Exact decimal token used for KVN encoding.
    #[wasm_bindgen(getter, js_name = valueText)]
    pub fn value_text(&self) -> String {
        self.inner.value.text.clone()
    }

    /// Numeric value plus exact decimal token.
    #[wasm_bindgen(getter)]
    pub fn scalar(&self) -> TdmScalar {
        self.inner.value.clone().into()
    }

    /// Canonical unit label assigned by CCSDS 503.0-B-2.
    #[wasm_bindgen(getter)]
    pub fn unit(&self) -> String {
        self.inner.unit.as_str().to_string()
    }
}

/// A TDM data block.
#[wasm_bindgen]
#[derive(Clone)]
pub struct TdmDataSection {
    inner: CoreTdmDataSection,
}

impl From<CoreTdmDataSection> for TdmDataSection {
    fn from(inner: CoreTdmDataSection) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl TdmDataSection {
    /// Data-section comments in parse order.
    #[wasm_bindgen(getter)]
    pub fn comments(&self) -> Vec<String> {
        self.inner.comments.clone()
    }

    /// Data records in parse order.
    #[wasm_bindgen(getter)]
    pub fn records(&self) -> Vec<TdmDataRecord> {
        self.inner
            .records
            .iter()
            .cloned()
            .map(TdmDataRecord::from)
            .collect()
    }
}

/// Metadata extracted from a TDM `META_START` / `META_STOP` block.
#[wasm_bindgen]
#[derive(Clone)]
pub struct TdmMetadata {
    inner: CoreTdmMetadata,
}

impl From<CoreTdmMetadata> for TdmMetadata {
    fn from(inner: CoreTdmMetadata) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl TdmMetadata {
    /// Metadata comments in parse order.
    #[wasm_bindgen(getter)]
    pub fn comments(&self) -> Vec<String> {
        self.inner.comments.clone()
    }

    /// Raw metadata fields in parse order.
    #[wasm_bindgen(getter)]
    pub fn fields(&self) -> Vec<TdmField> {
        self.inner
            .fields
            .iter()
            .cloned()
            .map(TdmField::from)
            .collect()
    }

    /// Parsed `PARTICIPANT_n` entries.
    #[wasm_bindgen(getter)]
    pub fn participants(&self) -> Vec<TdmParticipant> {
        self.inner
            .participants
            .iter()
            .cloned()
            .map(TdmParticipant::from)
            .collect()
    }

    /// Optional `MODE` value.
    #[wasm_bindgen(getter)]
    pub fn mode(&self) -> Option<String> {
        self.inner.mode.clone()
    }

    /// Parsed `PATH`, `PATH_1`, and `PATH_2` entries.
    #[wasm_bindgen(getter)]
    pub fn paths(&self) -> Vec<TdmPath> {
        self.inner
            .paths
            .iter()
            .cloned()
            .map(TdmPath::from)
            .collect()
    }

    /// Optional `TIMETAG_REF` value.
    #[wasm_bindgen(getter, js_name = timetagRef)]
    pub fn timetag_ref(&self) -> Option<String> {
        self.inner.timetag_ref.clone()
    }

    /// Optional `TIME_SYSTEM` value.
    #[wasm_bindgen(getter, js_name = timeSystem)]
    pub fn time_system(&self) -> Option<String> {
        self.inner.time_system.clone()
    }

    /// Range unit label used by `RANGE` records.
    #[wasm_bindgen(getter, js_name = rangeUnits)]
    pub fn range_units(&self) -> String {
        self.inner.range_units.as_str().to_string()
    }

    /// Return the last metadata value for `key`, or `undefined`.
    #[wasm_bindgen(js_name = getLast)]
    pub fn get_last(&self, key: &str) -> Option<String> {
        self.inner.get_last(key).map(str::to_owned)
    }
}

/// One TDM segment, consisting of metadata and data blocks.
#[wasm_bindgen]
#[derive(Clone)]
pub struct TdmSegment {
    inner: CoreTdmSegment,
}

impl From<CoreTdmSegment> for TdmSegment {
    fn from(inner: CoreTdmSegment) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl TdmSegment {
    /// Metadata describing this segment's records.
    #[wasm_bindgen(getter)]
    pub fn metadata(&self) -> TdmMetadata {
        self.inner.metadata.clone().into()
    }

    /// Tracking data records for this segment.
    #[wasm_bindgen(getter)]
    pub fn data(&self) -> TdmDataSection {
        self.inner.data.clone().into()
    }
}

/// A parsed CCSDS Tracking Data Message.
#[wasm_bindgen]
#[derive(Clone)]
pub struct Tdm {
    inner: CoreTdm,
}

#[wasm_bindgen]
impl Tdm {
    /// The `CCSDS_TDM_VERS` header value.
    #[wasm_bindgen(getter)]
    pub fn version(&self) -> String {
        self.inner.version.clone()
    }

    /// Header comments in parse order.
    #[wasm_bindgen(getter)]
    pub fn comments(&self) -> Vec<String> {
        self.inner.comments.clone()
    }

    /// Optional `CREATION_DATE` header value.
    #[wasm_bindgen(getter, js_name = creationDate)]
    pub fn creation_date(&self) -> Option<String> {
        self.inner.creation_date.clone()
    }

    /// Optional `ORIGINATOR` header value.
    #[wasm_bindgen(getter)]
    pub fn originator(&self) -> Option<String> {
        self.inner.originator.clone()
    }

    /// Optional `MESSAGE_ID` header value.
    #[wasm_bindgen(getter, js_name = messageId)]
    pub fn message_id(&self) -> Option<String> {
        self.inner.message_id.clone()
    }

    /// Header fields not part of the common modeled header.
    #[wasm_bindgen(getter, js_name = headerFields)]
    pub fn header_fields(&self) -> Vec<TdmField> {
        self.inner
            .header_fields
            .iter()
            .cloned()
            .map(TdmField::from)
            .collect()
    }

    /// Metadata/data segments in message order.
    #[wasm_bindgen(getter)]
    pub fn segments(&self) -> Vec<TdmSegment> {
        self.inner
            .segments
            .iter()
            .cloned()
            .map(TdmSegment::from)
            .collect()
    }

    /// Number of segments in the message.
    #[wasm_bindgen(getter, js_name = segmentCount)]
    pub fn segment_count(&self) -> usize {
        self.inner.segments.len()
    }

    /// Encode this TDM as canonical CCSDS KVN text.
    #[wasm_bindgen(js_name = toKvnString)]
    pub fn to_kvn_string(&self) -> Result<String, JsValue> {
        encode_kvn(&self.inner).map_err(engine_error)
    }
}

/// Parse a CCSDS Tracking Data Message in KVN form.
#[wasm_bindgen(js_name = parseTdmKvn)]
pub fn parse_tdm_kvn(text: &str) -> Result<Tdm, JsValue> {
    Ok(Tdm {
        inner: parse_kvn(text).map_err(engine_error)?,
    })
}
