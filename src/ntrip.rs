//! NTRIP request and stream state machine bindings.
//!
//! This is a sans-IO wrapper: callers provide bytes from their transport and
//! receive protocol events and payload bytes.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::ntrip::{
    parse_sourcetable as core_parse_sourcetable, CasRecord, Field, GgaPosition, NetRecord,
    NtripClientMachine as CoreNtripClientMachine, NtripConfig, NtripCredentials, NtripEvent,
    NtripRejection, NtripState as CoreNtripState, NtripVersion as CoreNtripVersion, OtherRecord,
    Sourcetable, SourcetableRecord, StrAuth, StrRecord,
};

use crate::error::{engine_error, type_error, utf8_text};

#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NtripVersion {
    Rev1,
    Rev2,
}

#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NtripState {
    Idle,
    AwaitingStatus,
    AwaitingHeaders,
    Streaming,
    Sourcetable,
    Closed,
}

fn version_label(version: CoreNtripVersion) -> &'static str {
    match version {
        CoreNtripVersion::Rev1 => "rev1",
        CoreNtripVersion::Rev2 => "rev2",
    }
}

fn parse_version(label: Option<&str>) -> Result<CoreNtripVersion, JsValue> {
    match label.unwrap_or("rev2") {
        "rev1" => Ok(CoreNtripVersion::Rev1),
        "rev2" => Ok(CoreNtripVersion::Rev2),
        other => Err(type_error(&format!(
            "invalid NTRIP version {other:?}: expected \"rev1\" or \"rev2\""
        ))),
    }
}

fn state_label(state: CoreNtripState) -> &'static str {
    match state {
        CoreNtripState::Idle => "idle",
        CoreNtripState::AwaitingStatus => "awaitingStatus",
        CoreNtripState::AwaitingHeaders => "awaitingHeaders",
        CoreNtripState::Streaming => "streaming",
        CoreNtripState::Sourcetable => "sourcetable",
        CoreNtripState::Closed => "closed",
    }
}

fn state_enum(state: CoreNtripState) -> NtripState {
    match state {
        CoreNtripState::Idle => NtripState::Idle,
        CoreNtripState::AwaitingStatus => NtripState::AwaitingStatus,
        CoreNtripState::AwaitingHeaders => NtripState::AwaitingHeaders,
        CoreNtripState::Streaming => NtripState::Streaming,
        CoreNtripState::Sourcetable => NtripState::Sourcetable,
        CoreNtripState::Closed => NtripState::Closed,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CredentialsInput {
    username: String,
    password: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigInput {
    host: String,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    mountpoint: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    credentials: Option<CredentialsInput>,
    #[serde(default)]
    user_agent_product: Option<String>,
    #[serde(default)]
    gga_interval_s: Option<f64>,
}

fn config_from_js(value: JsValue) -> Result<NtripConfig, JsValue> {
    let input: ConfigInput = serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid NTRIP config: {e}")))?;
    let mut config = NtripConfig::default();
    config.host = input.host;
    config.port = input.port.unwrap_or(config.port);
    config.mountpoint = input.mountpoint.unwrap_or_default();
    config.version = parse_version(input.version.as_deref())?;
    config.credentials = input.credentials.map(|credentials| NtripCredentials {
        username: credentials.username,
        password: credentials.password,
    });
    if let Some(product) = input.user_agent_product {
        config.user_agent_product = product;
    }
    config.gga_interval_s = input.gga_interval_s;
    Ok(config)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HeaderJs {
    name: String,
    value: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RejectionJs {
    kind: &'static str,
    reason: Option<String>,
    status: Option<u16>,
    content_type: Option<String>,
    prefix: Option<Vec<u8>>,
}

fn rejection_js(rejection: NtripRejection) -> RejectionJs {
    match rejection {
        NtripRejection::Unauthorized => RejectionJs {
            kind: "unauthorized",
            reason: None,
            status: None,
            content_type: None,
            prefix: None,
        },
        NtripRejection::MountpointNotFound => RejectionJs {
            kind: "mountpointNotFound",
            reason: None,
            status: None,
            content_type: None,
            prefix: None,
        },
        NtripRejection::DigestRequired => RejectionJs {
            kind: "digestRequired",
            reason: None,
            status: None,
            content_type: None,
            prefix: None,
        },
        NtripRejection::CasterError { reason } => RejectionJs {
            kind: "casterError",
            reason: Some(reason),
            status: None,
            content_type: None,
            prefix: None,
        },
        NtripRejection::UnexpectedContentType { content_type } => RejectionJs {
            kind: "unexpectedContentType",
            reason: None,
            status: None,
            content_type: Some(content_type),
            prefix: None,
        },
        NtripRejection::HttpError { status, reason } => RejectionJs {
            kind: "httpError",
            reason: Some(reason),
            status: Some(status),
            content_type: None,
            prefix: None,
        },
        NtripRejection::MalformedHandshake { prefix } => RejectionJs {
            kind: "malformedHandshake",
            reason: None,
            status: None,
            content_type: None,
            prefix: Some(prefix),
        },
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NtripEventJs {
    kind: &'static str,
    version: Option<&'static str>,
    chunked: Option<bool>,
    headers: Option<Vec<HeaderJs>>,
    payload: Option<Vec<u8>>,
    sourcetable: Option<SourcetableJs>,
    rejection: Option<RejectionJs>,
    detail: Option<String>,
}

fn event_js(event: NtripEvent) -> Result<NtripEventJs, JsValue> {
    Ok(match event {
        NtripEvent::Connected(handshake) => NtripEventJs {
            kind: "connected",
            version: Some(version_label(handshake.version)),
            chunked: Some(handshake.chunked),
            headers: Some(
                handshake
                    .headers
                    .into_iter()
                    .map(|(name, value)| HeaderJs { name, value })
                    .collect(),
            ),
            payload: None,
            sourcetable: None,
            rejection: None,
            detail: None,
        },
        NtripEvent::Payload(payload) => NtripEventJs {
            kind: "payload",
            version: None,
            chunked: None,
            headers: None,
            payload: Some(payload),
            sourcetable: None,
            rejection: None,
            detail: None,
        },
        NtripEvent::Sourcetable(table) => NtripEventJs {
            kind: "sourcetable",
            version: None,
            chunked: None,
            headers: None,
            payload: None,
            sourcetable: Some(sourcetable_js(&table)?),
            rejection: None,
            detail: None,
        },
        NtripEvent::Rejected(rejection) => NtripEventJs {
            kind: "rejected",
            version: None,
            chunked: None,
            headers: None,
            payload: None,
            sourcetable: None,
            rejection: Some(rejection_js(rejection)),
            detail: None,
        },
        NtripEvent::StreamCorrupted { detail } => NtripEventJs {
            kind: "streamCorrupted",
            version: None,
            chunked: None,
            headers: None,
            payload: None,
            sourcetable: None,
            rejection: None,
            detail: Some(detail),
        },
        NtripEvent::StreamEnded => NtripEventJs {
            kind: "streamEnded",
            version: None,
            chunked: None,
            headers: None,
            payload: None,
            sourcetable: None,
            rejection: None,
            detail: None,
        },
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FieldJs<T: Serialize> {
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    raw: Option<String>,
}

fn field_js<T>(field: &Field<T>) -> FieldJs<T>
where
    T: Clone + Serialize,
{
    match field {
        Field::Parsed(value) => FieldJs {
            kind: "parsed",
            value: Some(value.clone()),
            raw: None,
        },
        Field::Empty => FieldJs {
            kind: "empty",
            value: None,
            raw: None,
        },
        Field::Raw(raw) => FieldJs {
            kind: "raw",
            value: None,
            raw: Some(raw.clone()),
        },
    }
}

fn auth_label(auth: &StrAuth) -> String {
    match auth {
        StrAuth::None => "none".to_string(),
        StrAuth::Basic => "basic".to_string(),
        StrAuth::Digest => "digest".to_string(),
        StrAuth::Other(value) => value.clone(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StrRecordJs {
    type_tag: &'static str,
    mountpoint: String,
    identifier: String,
    format: String,
    format_details: String,
    carrier: FieldJs<u8>,
    nav_system: String,
    network: String,
    country: String,
    lat_deg: FieldJs<f64>,
    lon_deg: FieldJs<f64>,
    nmea_required: FieldJs<bool>,
    network_solution: FieldJs<bool>,
    generator: String,
    compression: String,
    authentication: String,
    fee: FieldJs<bool>,
    bitrate: FieldJs<u32>,
    misc: String,
}

fn str_record_js(record: &StrRecord) -> StrRecordJs {
    StrRecordJs {
        type_tag: "STR",
        mountpoint: record.mountpoint.clone(),
        identifier: record.identifier.clone(),
        format: record.format.clone(),
        format_details: record.format_details.clone(),
        carrier: field_js(&record.carrier),
        nav_system: record.nav_system.clone(),
        network: record.network.clone(),
        country: record.country.clone(),
        lat_deg: field_js(&record.lat_deg),
        lon_deg: field_js(&record.lon_deg),
        nmea_required: field_js(&record.nmea_required),
        network_solution: field_js(&record.network_solution),
        generator: record.generator.clone(),
        compression: record.compression.clone(),
        authentication: auth_label(&record.authentication),
        fee: field_js(&record.fee),
        bitrate: field_js(&record.bitrate),
        misc: record.misc.clone(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CasRecordJs {
    type_tag: &'static str,
    host: String,
    port: FieldJs<u16>,
    identifier: String,
    operator: String,
    nmea_required: FieldJs<bool>,
    country: String,
    lat_deg: FieldJs<f64>,
    lon_deg: FieldJs<f64>,
    fallback_host: String,
    fallback_port: FieldJs<u16>,
    misc: String,
}

fn cas_record_js(record: &CasRecord) -> CasRecordJs {
    CasRecordJs {
        type_tag: "CAS",
        host: record.host.clone(),
        port: field_js(&record.port),
        identifier: record.identifier.clone(),
        operator: record.operator.clone(),
        nmea_required: field_js(&record.nmea_required),
        country: record.country.clone(),
        lat_deg: field_js(&record.lat_deg),
        lon_deg: field_js(&record.lon_deg),
        fallback_host: record.fallback_host.clone(),
        fallback_port: field_js(&record.fallback_port),
        misc: record.misc.clone(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NetRecordJs {
    type_tag: &'static str,
    identifier: String,
    operator: String,
    authentication: String,
    fee: FieldJs<bool>,
    web_net: String,
    web_str: String,
    web_reg: String,
    misc: String,
}

fn net_record_js(record: &NetRecord) -> NetRecordJs {
    NetRecordJs {
        type_tag: "NET",
        identifier: record.identifier.clone(),
        operator: record.operator.clone(),
        authentication: auth_label(&record.authentication),
        fee: field_js(&record.fee),
        web_net: record.web_net.clone(),
        web_str: record.web_str.clone(),
        web_reg: record.web_reg.clone(),
        misc: record.misc.clone(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OtherRecordJs {
    type_tag: String,
    fields: Vec<String>,
}

fn other_record_js(record: &OtherRecord) -> OtherRecordJs {
    OtherRecordJs {
        type_tag: record.type_tag.clone(),
        fields: record.fields.clone(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "kind", content = "record")]
enum SourcetableRecordJs {
    Str(Box<StrRecordJs>),
    Cas(Box<CasRecordJs>),
    Net(Box<NetRecordJs>),
    Other(Box<OtherRecordJs>),
}

fn record_js(record: &SourcetableRecord) -> SourcetableRecordJs {
    match record {
        SourcetableRecord::Str(record) => SourcetableRecordJs::Str(Box::new(str_record_js(record))),
        SourcetableRecord::Cas(record) => SourcetableRecordJs::Cas(Box::new(cas_record_js(record))),
        SourcetableRecord::Net(record) => SourcetableRecordJs::Net(Box::new(net_record_js(record))),
        SourcetableRecord::Other(record) => {
            SourcetableRecordJs::Other(Box::new(other_record_js(record)))
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourcetableJs {
    record_count: usize,
    stream_count: usize,
    records: Vec<SourcetableRecordJs>,
    streams: Vec<StrRecordJs>,
    text: String,
}

fn sourcetable_js(table: &Sourcetable) -> Result<SourcetableJs, JsValue> {
    Ok(SourcetableJs {
        record_count: table.records.len(),
        stream_count: table.streams().count(),
        records: table.records.iter().map(record_js).collect(),
        streams: table.streams().map(str_record_js).collect(),
        text: table.to_text().map_err(engine_error)?,
    })
}

fn to_value<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(value).map_err(|e| type_error(&e.to_string()))
}

/// Build the NTRIP connection request bytes for a config object.
#[wasm_bindgen(js_name = ntripRequestBytes)]
pub fn ntrip_request_bytes(config: JsValue) -> Result<Vec<u8>, JsValue> {
    config_from_js(config)?
        .request_bytes()
        .map_err(engine_error)
}

/// Parse NTRIP sourcetable text from UTF-8 bytes.
#[wasm_bindgen(js_name = parseNtripSourcetable)]
pub fn parse_ntrip_sourcetable(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let text = utf8_text(bytes, "NTRIP sourcetable")?;
    let table = core_parse_sourcetable(&text).map_err(engine_error)?;
    to_value(&sourcetable_js(&table)?)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GgaPositionInput {
    lat_deg: f64,
    lon_deg: f64,
    height_m: f64,
    #[serde(default)]
    fix_quality: Option<u8>,
    #[serde(default)]
    num_satellites: Option<u8>,
    #[serde(default)]
    hdop: Option<f64>,
}

impl GgaPositionInput {
    fn to_core(&self) -> GgaPosition {
        let defaults = GgaPosition::default();
        GgaPosition {
            lat_deg: self.lat_deg,
            lon_deg: self.lon_deg,
            height_m: self.height_m,
            fix_quality: self.fix_quality.unwrap_or(defaults.fix_quality),
            num_satellites: self.num_satellites.unwrap_or(defaults.num_satellites),
            hdop: self.hdop.unwrap_or(defaults.hdop),
        }
    }
}

/// Sans-IO NTRIP client state machine.
#[wasm_bindgen]
pub struct NtripClientMachine {
    inner: CoreNtripClientMachine,
}

#[wasm_bindgen]
impl NtripClientMachine {
    #[wasm_bindgen(constructor)]
    pub fn new(config: JsValue) -> Result<NtripClientMachine, JsValue> {
        Ok(NtripClientMachine {
            inner: CoreNtripClientMachine::new(config_from_js(config)?),
        })
    }

    #[wasm_bindgen(getter)]
    pub fn state(&self) -> NtripState {
        state_enum(self.inner.state())
    }

    #[wasm_bindgen(getter, js_name = stateLabel)]
    pub fn state_label(&self) -> String {
        state_label(self.inner.state()).to_string()
    }

    #[wasm_bindgen(js_name = connectionRequest)]
    pub fn connection_request(&mut self) -> Result<Vec<u8>, JsValue> {
        self.inner.connection_request().map_err(engine_error)
    }

    #[wasm_bindgen]
    pub fn push(&mut self, bytes: &[u8]) -> Result<JsValue, JsValue> {
        let events: Vec<_> = self
            .inner
            .push(bytes)
            .into_iter()
            .map(event_js)
            .collect::<Result<_, _>>()?;
        to_value(&events)
    }

    #[wasm_bindgen]
    pub fn finish(&mut self) -> Result<JsValue, JsValue> {
        let events: Vec<_> = self
            .inner
            .finish()
            .into_iter()
            .map(event_js)
            .collect::<Result<_, _>>()?;
        to_value(&events)
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }

    #[wasm_bindgen(js_name = ggaMessage)]
    pub fn gga_message(
        &mut self,
        now_s: f64,
        position: JsValue,
        utc_seconds_of_day: f64,
    ) -> Result<JsValue, JsValue> {
        let position: GgaPositionInput = serde_wasm_bindgen::from_value(position)
            .map_err(|e| type_error(&format!("invalid NTRIP GGA position: {e}")))?;
        match self
            .inner
            .try_gga_message(now_s, &position.to_core(), utc_seconds_of_day)
            .map_err(engine_error)?
        {
            Some(bytes) => Ok(js_sys::Uint8Array::from(bytes.as_slice()).into()),
            None => Ok(JsValue::UNDEFINED),
        }
    }
}
