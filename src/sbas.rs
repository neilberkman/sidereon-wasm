use serde::Serialize;
use wasm_bindgen::prelude::*;

use sidereon_core::astro::time::model::{GnssWeekTow, TimeScale};
use sidereon_core::frame::Wgs84Geodetic;
use sidereon_core::positioning::EphemerisSource;
use sidereon_core::sbas::message::{
    SbasBlock as CoreSbasBlock, SbasDoNotUse, SbasEncodeError as CoreSbasEncodeError,
    SbasFastCorrections, SbasFastDegradation, SbasGeoAlmanac, SbasGeoNav, SbasIgpMask,
    SbasIntegrity, SbasIonoDelays, SbasLongTermCorrections, SbasLongTermHalf, SbasLongTermRecord,
    SbasMessage, SbasMixedCorrections, SbasNetworkTime, SbasPrnMask, SbasUnsupported, SbasWireForm,
    SpareBits,
};
use sidereon_core::sbas::source::{SbasCorrectedEphemeris, SbasSolveMode};
use sidereon_core::sbas::store::{
    sat_to_sbas_prn as core_sat_to_sbas_prn, sbas_prn_to_sat as core_sbas_prn_to_sat,
    SbasCorrectionStore as CoreSbasCorrectionStore, SbasFastCorrection, SbasGeoState, SbasIgp,
    SbasIonoGrid, SbasLongTermCorrection,
};
use sidereon_core::sbas::{
    parse_ems_lines as core_parse_ems_lines, parse_ems_log as core_parse_ems_log,
    parse_rtklib_lines as core_parse_rtklib_lines, parse_rtklib_log as core_parse_rtklib_log,
    SbasDeparture, SbasIgpUnavailableReason, SbasLineRefusal, SbasLog as CoreSbasLog,
    SbasLogBlock as CoreSbasLogBlock, SbasLogOptions, SbasPolicy, SbasSkippedLineKind,
};
use sidereon_core::staleness::StalenessPolicy;
use sidereon_core::GnssSatelliteId;

use crate::error::{engine_error, error_with_detail, range_error, type_error};
use crate::frames::ExactEpochQueryValue;
use crate::label::{lower_camel_variant, Label};
use crate::rinex_nav::BroadcastEphemeris;
use crate::spp::{self, SppSolution};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SbasMessageJs {
    message_type: u8,
    form: &'static str,
    kind: String,
    message: serde_json::Value,
    /// The six bits that complete the last byte of either wire form, as read.
    pad_bits: u8,
    /// Departures read under the lenient policy.
    departures: Vec<SbasDepartureJs>,
}

/// A departure from the SBAS format, as `{ kind, message, ... }`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SbasDepartureJs {
    /// `"unrecognizedPreamble"`, `"declaredMessageType"`, or, for a departure
    /// this binding does not name yet, the engine variant's name in the same
    /// case.
    kind: Label,
    message: String,
    preamble: Option<u8>,
    declared: Option<u8>,
    carried: Option<u8>,
    /// One-based log line, for a departure read from a log.
    line: Option<usize>,
}

impl From<&SbasDeparture> for SbasDepartureJs {
    fn from(departure: &SbasDeparture) -> Self {
        let (kind, preamble, declared, carried) = match departure {
            SbasDeparture::UnrecognizedPreamble { preamble } => (
                Label::Borrowed("unrecognizedPreamble"),
                Some(*preamble),
                None,
                None,
            ),
            SbasDeparture::DeclaredMessageType { declared, carried } => (
                Label::Borrowed("declaredMessageType"),
                None,
                Some(*declared),
                Some(*carried),
            ),
            other => (lower_camel_variant(other), None, None, None),
        };
        Self {
            kind,
            message: departure.to_string(),
            preamble,
            declared,
            carried,
            line: None,
        }
    }
}

/// Read an SBAS policy: `"strict"` (the default) refuses a preamble other
/// than `0x53`, `0x9A` and `0xC6` and a declared message type that differs
/// from the one the message carries; `"lenient"` reads them and reports each.
fn sbas_policy(value: Option<&str>) -> Result<SbasPolicy, JsValue> {
    match value {
        None | Some("strict") => Ok(SbasPolicy::Strict),
        Some("lenient") => Ok(SbasPolicy::Lenient),
        Some(other) => Err(type_error(&format!(
            "invalid SBAS policy {other:?}: expected \"strict\" or \"lenient\""
        ))),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CorrectedStateJs {
    position_ecef_m: [f64; 3],
    clock_s: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReservedBitsJs {
    value: u64,
    width: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FastCorrectionJs {
    prc_m: f64,
    rrc_m_s: f64,
    udrei: u8,
    t_of_j2000_s: f64,
    iodf: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LongTermCorrectionJs {
    iode: u8,
    delta_ecef_m: [f64; 3],
    delta_ecef_rate_m_s: [f64; 3],
    delta_af0_s: f64,
    delta_af1_s_s: f64,
    t0_j2000_s: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IgpJs {
    lat_deg: f64,
    lon_deg: f64,
    vertical_delay_m: f64,
    give_variance_m2: Option<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IonoGridJs {
    iodi: u8,
    igps: Vec<IgpJs>,
    unavailable_igps: Vec<UnavailableIgpJs>,
}

/// A grid point whose latest entry DO-229 marks unavailable.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UnavailableIgpJs {
    lat_deg: f64,
    lon_deg: f64,
    vertical_delay: u16,
    givei: u8,
    /// `"doNotUse"` (vertical delay 511) or `"notMonitored"` (GIVEI 15).
    reason: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeoStateJs {
    position_ecef_m: [f64; 3],
    velocity_ecef_m_s: [f64; 3],
    acceleration_ecef_m_s2: [f64; 3],
    clock_offset_s: f64,
    clock_drift_s_s: f64,
    t0_j2000_s: f64,
}

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| engine_error(format!("failed to serialize result: {e}")))
}

fn parse_form(value: Option<String>) -> Result<SbasWireForm, JsValue> {
    match value.as_deref().unwrap_or("framed250") {
        "framed250" => Ok(SbasWireForm::Framed250),
        "body226" => Ok(SbasWireForm::Body226),
        other => Err(type_error(&format!(
            "invalid SBAS wire form {other:?}: expected \"framed250\" or \"body226\""
        ))),
    }
}

fn form_label(value: SbasWireForm) -> &'static str {
    match value {
        SbasWireForm::Framed250 => "framed250",
        SbasWireForm::Body226 => "body226",
    }
}

fn parse_sat(token: &str) -> Result<GnssSatelliteId, JsValue> {
    token
        .parse::<GnssSatelliteId>()
        .map_err(|e| type_error(&format!("invalid satellite token {token:?}: {e}")))
}

fn parse_time_scale(value: Option<String>) -> Result<TimeScale, JsValue> {
    match value.as_deref().unwrap_or("gpst") {
        "gpst" => Ok(TimeScale::Gpst),
        "gst" => Ok(TimeScale::Gst),
        "bdt" => Ok(TimeScale::Bdt),
        other => Err(type_error(&format!("invalid GNSS time scale {other:?}"))),
    }
}

fn parse_mode(value: Option<String>) -> Result<SbasSolveMode, JsValue> {
    match value.as_deref().unwrap_or("mixedAugmentation") {
        "mixedAugmentation" => Ok(SbasSolveMode::MixedAugmentation),
        "sbasOnly" => Ok(SbasSolveMode::SbasOnly),
        other => Err(type_error(&format!(
            "invalid SBAS solve mode {other:?}: expected \"mixedAugmentation\" or \"sbasOnly\""
        ))),
    }
}

fn decode_block(bytes: &[u8], form: Option<String>) -> Result<CoreSbasBlock, JsValue> {
    CoreSbasBlock::decode(bytes, parse_form(form)?).map_err(engine_error)
}

fn reserved_bits(bits: &SpareBits) -> Vec<ReservedBitsJs> {
    bits.0
        .iter()
        .map(|&(value, width)| ReservedBitsJs { value, width })
        .collect()
}

fn raw_message(kind: &str, preamble: u8, data: &[u8]) -> serde_json::Value {
    serde_json::json!({
        "kind": kind,
        "preamble": preamble,
        "data": data,
    })
}

fn long_record_message(record: &SbasLongTermRecord) -> serde_json::Value {
    serde_json::json!({
        "monitoredIndex": record.monitored_index,
        "iode": record.iode,
        "deltaX": record.delta_x,
        "deltaY": record.delta_y,
        "deltaZ": record.delta_z,
        "deltaXRate": record.delta_x_rate,
        "deltaYRate": record.delta_y_rate,
        "deltaZRate": record.delta_z_rate,
        "deltaAF0": record.delta_a_f0,
        "deltaAF1": record.delta_a_f1,
        "timeOfDayS": record.time_of_day_s,
    })
}

fn long_half_message(half: &SbasLongTermHalf) -> serde_json::Value {
    serde_json::json!({
        "velocityCode": half.velocity_code,
        "iodp": half.iodp,
        "records": half.records.iter().map(long_record_message).collect::<Vec<_>>(),
        "reserved": reserved_bits(&half.reserved),
    })
}

fn message_payload(message: &SbasMessage) -> serde_json::Value {
    match message {
        SbasMessage::DoNotUse(SbasDoNotUse { preamble, data }) => {
            raw_message("doNotUse", *preamble, data)
        }
        SbasMessage::PrnMask(SbasPrnMask {
            preamble,
            iodp,
            mask,
            reserved,
        }) => serde_json::json!({
            "kind": "prnMask",
            "preamble": preamble,
            "iodp": iodp,
            "mask": mask.to_vec(),
            "reserved": reserved_bits(reserved),
        }),
        SbasMessage::FastCorrections(SbasFastCorrections {
            preamble,
            message_type,
            iodf,
            iodp,
            prc,
            udrei,
            reserved,
        }) => serde_json::json!({
            "kind": "fastCorrections",
            "preamble": preamble,
            "messageType": message_type,
            "iodf": iodf,
            "iodp": iodp,
            "prc": prc.to_vec(),
            "udrei": udrei.to_vec(),
            "reserved": reserved_bits(reserved),
        }),
        SbasMessage::Integrity(SbasIntegrity {
            preamble,
            iodf,
            udrei,
            reserved,
        }) => serde_json::json!({
            "kind": "integrity",
            "preamble": preamble,
            "iodf": iodf.to_vec(),
            "udrei": udrei.to_vec(),
            "reserved": reserved_bits(reserved),
        }),
        SbasMessage::FastDegradation(SbasFastDegradation {
            preamble,
            system_latency_s,
            iodp,
            ai,
            reserved,
        }) => serde_json::json!({
            "kind": "fastDegradation",
            "preamble": preamble,
            "systemLatencyS": system_latency_s,
            "iodp": iodp,
            "ai": ai.to_vec(),
            "reserved": reserved_bits(reserved),
        }),
        SbasMessage::GeoNav(SbasGeoNav {
            preamble,
            time_of_day_s,
            ura,
            x_m,
            y_m,
            z_m,
            x_rate_m_s,
            y_rate_m_s,
            z_rate_m_s,
            x_accel_m_s2,
            y_accel_m_s2,
            z_accel_m_s2,
            a_gf0_s,
            a_gf1_s_s,
            reserved,
        }) => serde_json::json!({
            "kind": "geoNav",
            "preamble": preamble,
            "timeOfDayS": time_of_day_s,
            "ura": ura,
            "xM": x_m,
            "yM": y_m,
            "zM": z_m,
            "xRateMS": x_rate_m_s,
            "yRateMS": y_rate_m_s,
            "zRateMS": z_rate_m_s,
            "xAccelMS2": x_accel_m_s2,
            "yAccelMS2": y_accel_m_s2,
            "zAccelMS2": z_accel_m_s2,
            "aGf0S": a_gf0_s,
            "aGf1SS": a_gf1_s_s,
            "reserved": reserved_bits(reserved),
        }),
        SbasMessage::NetworkTime(SbasNetworkTime { preamble, data }) => {
            raw_message("networkTime", *preamble, data)
        }
        SbasMessage::GeoAlmanac(SbasGeoAlmanac { preamble, data }) => {
            raw_message("geoAlmanac", *preamble, data)
        }
        SbasMessage::MixedCorrections(SbasMixedCorrections {
            preamble,
            fast,
            long_term,
        }) => serde_json::json!({
            "kind": "mixedCorrections",
            "preamble": preamble,
            "fast": {
                "iodf": fast.iodf,
                "iodp": fast.iodp,
                "blockId": fast.block_id,
                "prc": fast.prc.to_vec(),
                "udrei": fast.udrei.to_vec(),
                "reserved": reserved_bits(&fast.reserved),
            },
            "longTerm": long_half_message(long_term),
        }),
        SbasMessage::LongTermCorrections(SbasLongTermCorrections { preamble, halves }) => {
            serde_json::json!({
                "kind": "longTermCorrections",
                "preamble": preamble,
                "halves": halves.iter().map(long_half_message).collect::<Vec<_>>(),
            })
        }
        SbasMessage::IgpMask(SbasIgpMask {
            preamble,
            band_number,
            iodi,
            mask,
            reserved,
        }) => serde_json::json!({
            "kind": "igpMask",
            "preamble": preamble,
            "bandNumber": band_number,
            "iodi": iodi,
            "mask": mask.to_vec(),
            "reserved": reserved_bits(reserved),
        }),
        SbasMessage::IonoDelays(SbasIonoDelays {
            preamble,
            band_number,
            block_id,
            iodi,
            entries,
            reserved,
        }) => serde_json::json!({
            "kind": "ionoDelays",
            "preamble": preamble,
            "bandNumber": band_number,
            "blockId": block_id,
            "iodi": iodi,
            "entries": entries.iter().map(|entry| serde_json::json!({
                "verticalDelay": entry.vertical_delay,
                "givei": entry.givei,
            })).collect::<Vec<_>>(),
            "reserved": reserved_bits(reserved),
        }),
        SbasMessage::Unsupported(SbasUnsupported {
            preamble,
            message_type,
            data,
        }) => serde_json::json!({
            "kind": "unsupported",
            "preamble": preamble,
            "messageType": message_type,
            "data": data,
        }),
    }
}

fn fast_correction(value: &SbasFastCorrection) -> FastCorrectionJs {
    FastCorrectionJs {
        prc_m: value.prc_m,
        rrc_m_s: value.rrc_m_s,
        udrei: value.udrei,
        t_of_j2000_s: value.t_of_j2000_s,
        iodf: value.iodf,
    }
}

fn long_term_correction(value: &SbasLongTermCorrection) -> LongTermCorrectionJs {
    LongTermCorrectionJs {
        iode: value.iode,
        delta_ecef_m: value.delta_ecef_m,
        delta_ecef_rate_m_s: value.delta_ecef_rate_m_s,
        delta_af0_s: value.delta_af0_s,
        delta_af1_s_s: value.delta_af1_s_s,
        t0_j2000_s: value.t0_j2000_s,
    }
}

fn igp(value: &SbasIgp) -> IgpJs {
    IgpJs {
        lat_deg: value.lat_deg,
        lon_deg: value.lon_deg,
        vertical_delay_m: value.vertical_delay_m,
        give_variance_m2: value.give_variance_m2,
    }
}

fn iono_grid(value: &SbasIonoGrid) -> IonoGridJs {
    IonoGridJs {
        iodi: value.iodi,
        igps: value.igps().iter().map(igp).collect(),
        unavailable_igps: value
            .unavailable_igps()
            .iter()
            .map(|point| UnavailableIgpJs {
                lat_deg: point.lat_deg,
                lon_deg: point.lon_deg,
                vertical_delay: point.vertical_delay,
                givei: point.givei,
                reason: match point.reason {
                    SbasIgpUnavailableReason::DoNotUse => "doNotUse",
                    SbasIgpUnavailableReason::NotMonitored => "notMonitored",
                },
            })
            .collect(),
    }
}

fn geo_state(value: &SbasGeoState) -> GeoStateJs {
    GeoStateJs {
        position_ecef_m: value.position_ecef_m,
        velocity_ecef_m_s: value.velocity_ecef_m_s,
        acceleration_ecef_m_s2: value.acceleration_ecef_m_s2,
        clock_offset_s: value.clock_offset_s,
        clock_drift_s_s: value.clock_drift_s_s,
        t0_j2000_s: value.t0_j2000_s,
    }
}

fn nullable<T: Serialize>(value: Option<T>) -> Result<JsValue, JsValue> {
    match value {
        Some(value) => to_js(&value),
        None => Ok(JsValue::NULL),
    }
}

/// One timestamped SBAS message row parsed from an EMS or RTKLIB text log.
#[wasm_bindgen]
pub struct SbasLogBlock {
    inner: CoreSbasLogBlock,
}

impl From<CoreSbasLogBlock> for SbasLogBlock {
    fn from(inner: CoreSbasLogBlock) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl SbasLogBlock {
    /// SBAS satellite token, such as `"S20"`.
    #[wasm_bindgen(getter)]
    pub fn satellite(&self) -> String {
        self.inner.satellite_id.to_string()
    }

    /// SBAS satellite token, such as `"S20"`.
    #[wasm_bindgen(getter, js_name = satelliteId)]
    pub fn satellite_id(&self) -> String {
        self.inner.satellite_id.to_string()
    }

    /// GPS week of the parsed message epoch.
    #[wasm_bindgen(getter)]
    pub fn week(&self) -> u32 {
        self.inner.epoch.week
    }

    /// Seconds of GPS week of the parsed message epoch.
    #[wasm_bindgen(getter, js_name = towS)]
    pub fn tow_s(&self) -> f64 {
        self.inner.epoch.tow_s
    }

    /// Core-selected wire form: `"framed250"` or `"body226"`.
    #[wasm_bindgen(getter)]
    pub fn form(&self) -> String {
        form_label(self.inner.form).to_string()
    }

    /// Raw SBAS bytes in the form selected by the core parser.
    #[wasm_bindgen(getter)]
    pub fn bytes(&self) -> Vec<u8> {
        self.inner.bytes.clone()
    }

    /// The message type the record's own field states (the EMS message-type
    /// field or RTKLIB's fourth header field), or `undefined` for an
    /// eight-field comma line, which carries none.
    #[wasm_bindgen(getter, js_name = declaredMessageType)]
    pub fn declared_message_type(&self) -> Option<u8> {
        self.inner.declared_message_type
    }

    /// The six-bit message type the bytes carry, or `undefined` when they are
    /// shorter than two bytes.
    #[wasm_bindgen(getter, js_name = messageType)]
    pub fn message_type(&self) -> Option<u8> {
        self.inner.message_type()
    }

    /// Decode this raw message using its parsed wire form, under `policy`
    /// (`"strict"` by default, or `"lenient"`).
    pub fn decode(&self, policy: Option<String>) -> Result<JsValue, JsValue> {
        let (block, departures) = CoreSbasBlock::decode_with_policy(
            &self.inner.bytes,
            self.inner.form,
            sbas_policy(policy.as_deref())?,
        )
        .map_err(engine_error)?;
        decoded_sbas_block(block, &departures)
    }
}

fn decoded_sbas_block(
    block: CoreSbasBlock,
    departures: &[SbasDeparture],
) -> Result<JsValue, JsValue> {
    let out = SbasMessageJs {
        message_type: block.message.message_type(),
        form: form_label(block.form),
        kind: format!("{:?}", block.message),
        message: message_payload(&block.message),
        pad_bits: block.pad_bits,
        departures: departures.iter().map(SbasDepartureJs::from).collect(),
    };
    to_js(&out)
}

#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct SbasLogOptionsInput {
    policy: Option<String>,
    reference_week: Option<u32>,
}

fn log_options(options: JsValue) -> Result<SbasLogOptions, JsValue> {
    let input: SbasLogOptionsInput = if options.is_undefined() || options.is_null() {
        SbasLogOptionsInput::default()
    } else {
        crate::error::reject_unknown_keys(
            &options,
            "SBAS log options",
            &["policy", "referenceWeek"],
        )?;
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid SBAS log options: {e}")))?
    };
    let mut out = SbasLogOptions::default().with_policy(sbas_policy(input.policy.as_deref())?);
    if let Some(week) = input.reference_week {
        out = out.with_reference_week(week);
    }
    Ok(out)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SbasSkippedLineJs {
    line: usize,
    /// `"blank"`, `"comment"` or `"nonRecord"`.
    kind: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SbasRefusedLineJs {
    line: usize,
    /// `"ambiguousWeek"`, `"checksumMismatch"`, or, for a reason this binding
    /// does not name yet, the engine variant's name in the same case.
    reason: Label,
    week: Option<u32>,
    written: Option<u32>,
    computed: Option<u32>,
}

/// Everything a log reader read: `blocks`, `skippedLines` (read as no record),
/// `refusedLines` (record lines left unread) and `departures`.
#[wasm_bindgen]
pub struct SbasLog {
    inner: CoreSbasLog,
}

#[wasm_bindgen]
impl SbasLog {
    /// Records in input order.
    #[wasm_bindgen(getter)]
    pub fn blocks(&self) -> Vec<SbasLogBlock> {
        self.inner.blocks.iter().cloned().map(Into::into).collect()
    }

    /// Lines read as no record, as `{ line, kind }`.
    #[wasm_bindgen(getter, js_name = skippedLines, unchecked_return_type = "SbasSkippedLine[]")]
    pub fn skipped_lines(&self) -> Result<JsValue, JsValue> {
        let rows: Vec<SbasSkippedLineJs> = self
            .inner
            .skipped_lines
            .iter()
            .map(|line| SbasSkippedLineJs {
                line: line.line,
                kind: match line.kind {
                    SbasSkippedLineKind::Blank => "blank",
                    SbasSkippedLineKind::Comment => "comment",
                    SbasSkippedLineKind::NonRecord => "nonRecord",
                },
            })
            .collect();
        to_js(&rows)
    }

    /// Record lines left unread while the rest of the log was read, as
    /// `{ line, reason, week, written, computed }`.
    #[wasm_bindgen(getter, js_name = refusedLines, unchecked_return_type = "SbasRefusedLine[]")]
    pub fn refused_lines(&self) -> Result<JsValue, JsValue> {
        let rows: Vec<SbasRefusedLineJs> = self
            .inner
            .refused_lines
            .iter()
            .map(|line| {
                let mut row = SbasRefusedLineJs {
                    line: line.line,
                    reason: lower_camel_variant(&line.reason),
                    week: None,
                    written: None,
                    computed: None,
                };
                match line.reason {
                    SbasLineRefusal::AmbiguousWeek { week } => {
                        row.reason = Label::Borrowed("ambiguousWeek");
                        row.week = Some(week);
                    }
                    SbasLineRefusal::ChecksumMismatch { written, computed } => {
                        row.reason = Label::Borrowed("checksumMismatch");
                        row.written = written;
                        row.computed = Some(computed);
                    }
                    _ => {}
                }
                row
            })
            .collect();
        to_js(&rows)
    }

    /// Departures read under the lenient policy, each with its `line`.
    #[wasm_bindgen(getter, unchecked_return_type = "SbasDeparture[]")]
    pub fn departures(&self) -> Result<JsValue, JsValue> {
        let rows: Vec<SbasDepartureJs> = self
            .inner
            .departures
            .iter()
            .map(|entry| SbasDepartureJs {
                line: Some(entry.line),
                ..SbasDepartureJs::from(&entry.departure)
            })
            .collect();
        to_js(&rows)
    }
}

/// Decode a raw SBAS message.
///
/// `form` is `"framed250"` for a 32-byte message with CRC or `"body226"` for a
/// 29-byte body. The result contains `messageType`, `form`, legacy debug
/// `kind`, and `message`, a structured decoded payload. Parse failures are
/// thrown as `Error`. `policy` is `"strict"` (the default), which refuses a
/// preamble other than `0x53`, `0x9A` and `0xC6`, or `"lenient"`, which reads
/// it and reports it in `departures`. A framed block whose CRC does not match
/// is refused under both.
#[wasm_bindgen(js_name = decodeSbasMessage)]
pub fn decode_sbas_message(
    bytes: &[u8],
    form: Option<String>,
    policy: Option<String>,
) -> Result<JsValue, JsValue> {
    let (block, departures) = CoreSbasBlock::decode_with_policy(
        bytes,
        parse_form(form)?,
        sbas_policy(policy.as_deref())?,
    )
    .map_err(engine_error)?;
    decoded_sbas_block(block, &departures)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SbasReencodedMessageJs {
    bytes: Vec<u8>,
    departures: Vec<SbasDepartureJs>,
}

/// Re-encode an SBAS body or framed block under `policy` (`"strict"` by
/// default, or `"lenient"`). The input is decoded leniently so an unknown
/// preamble is refused by the encoder with a typed `SbasEncodeError` in strict
/// mode, or preserved and reported in lenient mode. Returns `{ bytes,
/// departures }`.
#[wasm_bindgen(
    js_name = encodeSbasMessage,
    unchecked_return_type = "SbasReencodedMessage"
)]
pub fn encode_sbas_message(
    bytes: &[u8],
    form: Option<String>,
    policy: Option<String>,
) -> Result<JsValue, JsValue> {
    let form = parse_form(form)?;
    let policy = sbas_policy(policy.as_deref())?;
    let (block, _) = CoreSbasBlock::decode_with_policy(bytes, form, SbasPolicy::Lenient)
        .map_err(engine_error)?;
    let (bytes, departures) = block
        .encode_with_policy(policy)
        .map_err(sbas_encode_error)?;
    to_js(&SbasReencodedMessageJs {
        bytes,
        departures: departures.iter().map(SbasDepartureJs::from).collect(),
    })
}

fn sbas_encode_error(error: sidereon_core::Error) -> JsValue {
    let message = error.to_string();
    let detail = match error {
        sidereon_core::Error::SbasEncode(error) => serde_json::json!({
            "kind": "SBAS_ENCODE",
            "core": sbas_encode_error_payload(&error),
            "message": message.clone(),
        }),
        _ => serde_json::json!({ "kind": "UNKNOWN", "message": message.clone() }),
    };
    error_with_detail("SbasEncodeError", &message, &detail)
}

fn sbas_encode_error_payload(error: &CoreSbasEncodeError) -> serde_json::Value {
    use CoreSbasEncodeError as EncodeError;
    match error {
        EncodeError::FieldOutOfRange {
            message_type,
            field,
            index,
            value,
            width,
            signed,
        } => serde_json::json!({
            "kind": "fieldOutOfRange",
            "messageType": *message_type,
            "field": field,
            "index": index,
            "value": value.to_string(),
            "width": width,
            "signed": signed,
        }),
        EncodeError::UnrecognizedPreamble { preamble } => serde_json::json!({
            "kind": "unrecognizedPreamble",
            "preamble": preamble,
        }),
        EncodeError::MessageType {
            message_type,
            reason,
        } => serde_json::json!({
            "kind": "messageType",
            "messageType": *message_type,
            "reason": reason,
        }),
        EncodeError::RawPayload {
            message_type,
            bytes,
            bits_past_payload,
        } => serde_json::json!({
            "kind": "rawPayload",
            "messageType": *message_type,
            "bytes": bytes,
            "bitsPastPayload": bits_past_payload,
        }),
        EncodeError::ReservedLayout {
            message_type,
            part,
            expected,
            found,
        } => serde_json::json!({
            "kind": "reservedLayout",
            "messageType": *message_type,
            "part": part,
            "expected": expected,
            "found": found,
        }),
        EncodeError::LongTermRecordCount {
            message_type,
            half,
            velocity_code,
            expected,
            found,
        } => serde_json::json!({
            "kind": "longTermRecordCount",
            "messageType": *message_type,
            "half": half,
            "velocityCode": velocity_code,
            "expected": expected,
            "found": found,
        }),
        EncodeError::LongTermFieldNotCarried {
            message_type,
            half,
            record,
            field,
        } => serde_json::json!({
            "kind": "longTermFieldNotCarried",
            "messageType": *message_type,
            "half": half,
            "record": record,
            "field": field,
        }),
        EncodeError::LongTermMissingTimeOfDay { message_type, half } => serde_json::json!({
            "kind": "longTermMissingTimeOfDay",
            "messageType": *message_type,
            "half": half,
        }),
        EncodeError::PadBits { value } => serde_json::json!({
            "kind": "padBits",
            "value": value,
        }),
        other => serde_json::json!({
            "kind": "unrecognizedSbasEncodeError",
            "message": other.to_string(),
        }),
    }
}

#[wasm_bindgen(typescript_custom_section)]
const TS_SBAS_ENCODE_ERROR: &str = r#"
export type SbasEncodeErrorDetail =
  | { kind: "SBAS_ENCODE"; core: SbasEncodeCoreDetail; message: string }
  | { kind: "UNKNOWN"; message: string };

export interface SbasReencodedMessage {
  bytes: number[];
  departures: SbasDeparture[];
}

export type SbasEncodeCoreDetail =
  | { kind: "fieldOutOfRange"; messageType: number; field: string; index: number | null; value: string; width: number; signed: boolean }
  | { kind: "unrecognizedPreamble"; preamble: number }
  | { kind: "messageType"; messageType: number; reason: string }
  | { kind: "rawPayload"; messageType: number; bytes: number; bitsPastPayload: boolean }
  | { kind: "reservedLayout"; messageType: number; part: string; expected: number[]; found: number[] }
  | { kind: "longTermRecordCount"; messageType: number; half: number; velocityCode: boolean; expected: number; found: number }
  | { kind: "longTermFieldNotCarried"; messageType: number; half: number; record: number; field: string }
  | { kind: "longTermMissingTimeOfDay"; messageType: number; half: number }
  | { kind: "padBits"; value: number }
  | { kind: "unrecognizedSbasEncodeError"; message: string };
"#;

/// Read an EMS log under `options` (`{ policy?, referenceWeek? }`): the
/// records, every line read as no record, every record line left unread and
/// every departure read under the lenient policy. NovAtel OEM4
/// `#RAWWAASFRAMEA` and OEM3 `$FRMA` lines are read as RTKLIB `readmsgs`
/// reads them; `referenceWeek` resolves an OEM3 10-bit week.
#[wasm_bindgen(js_name = parseSbasEmsLog)]
pub fn parse_sbas_ems_log(
    text: &str,
    #[wasm_bindgen(unchecked_optional_param_type = "SbasLogOptions")] options: JsValue,
) -> Result<SbasLog, JsValue> {
    Ok(SbasLog {
        inner: core_parse_ems_log(text, log_options(options)?).map_err(engine_error)?,
    })
}

/// Read an RTKLIB SBAS log under `options`, as `parseSbasEmsLog` does.
#[wasm_bindgen(js_name = parseSbasRtklibLog)]
pub fn parse_sbas_rtklib_log(
    text: &str,
    #[wasm_bindgen(unchecked_optional_param_type = "SbasLogOptions")] options: JsValue,
) -> Result<SbasLog, JsValue> {
    Ok(SbasLog {
        inner: core_parse_rtklib_log(text, log_options(options)?).map_err(engine_error)?,
    })
}

/// Parse timestamped SBAS EMS log lines into raw message blocks.
#[wasm_bindgen(js_name = parseSbasEmsLines)]
pub fn parse_sbas_ems_lines(text: &str) -> Result<Vec<SbasLogBlock>, JsValue> {
    core_parse_ems_lines(text)
        .map(|blocks| blocks.into_iter().map(Into::into).collect())
        .map_err(engine_error)
}

/// Parse timestamped RTKLIB SBAS log lines into raw message blocks.
#[wasm_bindgen(js_name = parseSbasRtklibLines)]
pub fn parse_sbas_rtklib_lines(text: &str) -> Result<Vec<SbasLogBlock>, JsValue> {
    core_parse_rtklib_lines(text)
        .map(|blocks| blocks.into_iter().map(Into::into).collect())
        .map_err(engine_error)
}

#[wasm_bindgen]
/// Mutable SBAS correction store.
///
/// Ingest raw SBAS messages with a source GEO and GNSS time, then query decoded
/// fast, long-term, ionospheric, and GEO navigation correction records.
pub struct SbasCorrectionStore {
    pub(crate) inner: CoreSbasCorrectionStore,
}

impl Default for SbasCorrectionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl SbasCorrectionStore {
    /// Create an empty SBAS correction store.
    #[wasm_bindgen(constructor)]
    pub fn new() -> SbasCorrectionStore {
        SbasCorrectionStore {
            inner: CoreSbasCorrectionStore::new(),
        }
    }

    /// Ingest one decoded SBAS message into the correction store.
    ///
    /// `geo` is the SBAS source satellite token such as `"S29"`. `week` and
    /// `towS` are in the selected GNSS time scale. `form` is `"framed250"` or
    /// `"body226"`.
    #[wasm_bindgen(js_name = ingest)]
    pub fn ingest(
        &mut self,
        bytes: &[u8],
        form: Option<String>,
        geo: &str,
        week: u32,
        tow_s: f64,
        time_scale: Option<String>,
    ) -> Result<(), JsValue> {
        let block = decode_block(bytes, form)?;
        let geo = parse_sat(geo)?;
        let epoch = GnssWeekTow::new(parse_time_scale(time_scale)?, week, tow_s)
            .and_then(GnssWeekTow::normalized)
            .map_err(engine_error)?;
        self.inner
            .ingest(&block.message, geo, epoch)
            .map_err(engine_error)
    }

    /// Ready SBAS GEO source satellites at `tJ2000S`.
    ///
    /// The time is seconds since J2000. Returned tokens are strings such as
    /// `"S29"`, sorted by most recent update first.
    #[wasm_bindgen(js_name = readyGeos)]
    pub fn ready_geos(&self, t_j2000_s: f64) -> Vec<String> {
        self.inner
            .ready_geos(t_j2000_s)
            .into_iter()
            .map(|sat| sat.to_string())
            .collect()
    }

    /// Set the maximum staleness for fresh SBAS corrections, in seconds.
    #[wasm_bindgen(js_name = setStalenessSeconds)]
    pub fn set_staleness_seconds(&mut self, seconds: f64) -> Result<(), JsValue> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(range_error("seconds must be finite and non-negative"));
        }
        let inner = std::mem::replace(&mut self.inner, CoreSbasCorrectionStore::new())
            .with_policy(StalenessPolicy::seconds(seconds));
        self.inner = inner;
        Ok(())
    }

    /// Allow or disallow partial SBAS corrections when building corrected
    /// ephemeris states.
    #[wasm_bindgen(js_name = setAllowPartial)]
    pub fn set_allow_partial(&mut self, yes: bool) {
        let inner =
            std::mem::replace(&mut self.inner, CoreSbasCorrectionStore::new()).allow_partial(yes);
        self.inner = inner;
    }

    /// Fast pseudorange correction for `(geo, sat)`, or `null`.
    ///
    /// `prcM` is meters, `rrcMS` is meters per second, and `tOfJ2000S` is
    /// seconds since J2000.
    #[wasm_bindgen(js_name = fastCorrection)]
    pub fn fast_correction(&self, geo: &str, sat: &str) -> Result<JsValue, JsValue> {
        let geo = parse_sat(geo)?;
        let sat = parse_sat(sat)?;
        nullable(self.inner.fast(geo, sat).map(fast_correction))
    }

    /// Long-term orbit and clock correction for `(geo, sat)`, or `null`.
    ///
    /// Position deltas are ECEF meters, rates are ECEF meters per second, clock
    /// deltas are seconds and seconds per second, and `t0J2000S` is seconds
    /// since J2000.
    #[wasm_bindgen(js_name = longTermCorrection)]
    pub fn long_term_correction(&self, geo: &str, sat: &str) -> Result<JsValue, JsValue> {
        let geo = parse_sat(geo)?;
        let sat = parse_sat(sat)?;
        nullable(self.inner.long_term(geo, sat).map(long_term_correction))
    }

    /// SBAS ionospheric grid for `geo`, or `null`.
    ///
    /// Grid point latitudes and longitudes are degrees, vertical delays are
    /// meters, and GIVE variances are square meters when present.
    #[wasm_bindgen(js_name = ionoGrid)]
    pub fn iono_grid(&self, geo: &str) -> Result<JsValue, JsValue> {
        let geo = parse_sat(geo)?;
        nullable(self.inner.iono_grid(geo).map(iono_grid))
    }

    /// SBAS GEO navigation state for `geo`, or `null`.
    ///
    /// Positions are ECEF meters, velocities are ECEF meters per second,
    /// accelerations are ECEF meters per second squared, clock fields are
    /// seconds and seconds per second, and `t0J2000S` is seconds since J2000.
    #[wasm_bindgen(js_name = geoNavState)]
    pub fn geo_nav_state(&self, geo: &str) -> Result<JsValue, JsValue> {
        let geo = parse_sat(geo)?;
        nullable(self.inner.geo_nav(geo).map(geo_state))
    }

    /// Corrections a source GEO addressed to active PRN-mask bits that name no
    /// satellite held here, per 1-based PRN mask number, or `null` when the
    /// GEO has no mask partition.
    ///
    /// The mask follows the RTCA DO-229 layout: numbers 1..37 are GPS, 38..61
    /// GLONASS slots 1..24, 120..158 SBAS, and the rest name no satellite. An
    /// unassigned bit keeps its place among the active bits, so the corrections
    /// after it still reach their own satellites; the corrections addressed to
    /// it are applied to no satellite and counted here. Each entry is
    /// `{ maskNumber, count }` in ascending `maskNumber`, with `count` an exact
    /// `bigint`.
    #[wasm_bindgen(
        js_name = unassignedMaskCorrections,
        unchecked_return_type = "SbasUnassignedMaskCorrections[] | null"
    )]
    pub fn unassigned_mask_corrections(&self, geo: &str) -> Result<JsValue, JsValue> {
        let geo = parse_sat(geo)?;
        let Some(counts) = self.inner.unassigned_mask_corrections(geo) else {
            return Ok(JsValue::NULL);
        };
        let out = js_sys::Array::new();
        for (&mask_number, &count) in counts {
            let entry = js_sys::Object::new();
            js_sys::Reflect::set(
                &entry,
                &JsValue::from_str("maskNumber"),
                &JsValue::from_f64(f64::from(mask_number)),
            )?;
            js_sys::Reflect::set(
                &entry,
                &JsValue::from_str("count"),
                &js_sys::BigInt::from(count).into(),
            )?;
            out.push(&entry);
        }
        Ok(out.into())
    }

    /// SBAS ionospheric slant delay in meters, or `null`.
    ///
    /// Receiver latitude, longitude, elevation, and azimuth are radians.
    /// `frequencyHz` is the carrier frequency for the reported group delay.
    #[wasm_bindgen(js_name = ionoSlantDelayM)]
    #[allow(clippy::too_many_arguments)]
    pub fn iono_slant_delay_m(
        &self,
        geo: &str,
        receiver_lat_rad: f64,
        receiver_lon_rad: f64,
        receiver_height_m: f64,
        elevation_rad: f64,
        azimuth_rad: f64,
        frequency_hz: f64,
    ) -> Result<Option<f64>, JsValue> {
        let geo = parse_sat(geo)?;
        let receiver = Wgs84Geodetic {
            lat_rad: receiver_lat_rad,
            lon_rad: receiver_lon_rad,
            height_m: receiver_height_m,
        };
        Ok(self.inner.iono_grid(geo).and_then(|grid| {
            grid.slant_delay_m(receiver, elevation_rad, azimuth_rad, frequency_hz)
        }))
    }
}

impl SbasCorrectionStore {
    pub(crate) fn core(&self) -> &CoreSbasCorrectionStore {
        &self.inner
    }
}

#[wasm_bindgen]
impl BroadcastEphemeris {
    /// Evaluate SBAS-selected broadcast state using distinct exact state and
    /// record-selection epochs.
    #[wasm_bindgen(js_name = sbasPositionClockAtExactQueries, unchecked_return_type = "Ut1Validated<SelectedPositionClock> | null")]
    pub fn sbas_position_clock_at_exact_queries(
        &self,
        store: &SbasCorrectionStore,
        geo: &str,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        selection_epoch: &ExactEpochQueryValue,
        mode: Option<String>,
    ) -> Result<JsValue, JsValue> {
        let geo = parse_sat(geo)?;
        let satellite = parse_sat(satellite)?;
        let source = SbasCorrectedEphemeris::new(&self.inner, store.core(), geo)
            .with_mode(parse_mode(mode)?);
        crate::sp3::selected_position_clock_at_queries(
            &source,
            satellite,
            state_epoch,
            selection_epoch,
        )
    }

    /// Evaluate the SBAS-selected satellite clock at an exact transmission
    /// epoch while selecting the broadcast record at a separate exact epoch.
    #[wasm_bindgen(js_name = sbasTransmitEpochClockAtExactQueries, unchecked_return_type = "Ut1Validated<number> | null")]
    pub fn sbas_transmit_epoch_clock_at_exact_queries(
        &self,
        store: &SbasCorrectionStore,
        geo: &str,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        selection_epoch: &ExactEpochQueryValue,
        mode: Option<String>,
    ) -> Result<JsValue, JsValue> {
        let geo = parse_sat(geo)?;
        let satellite = parse_sat(satellite)?;
        let source = SbasCorrectedEphemeris::new(&self.inner, store.core(), geo)
            .with_mode(parse_mode(mode)?);
        crate::sp3::transmit_epoch_clock_at_queries(
            &source,
            satellite,
            state_epoch,
            selection_epoch,
        )
    }

    /// Return SBAS source variance in square metres at an exact state epoch,
    /// using the exact selection epoch for broadcast-record selection.
    #[wasm_bindgen(js_name = sbasEphemerisVarianceAtExactQueries)]
    pub fn sbas_ephemeris_variance_at_exact_queries(
        &self,
        store: &SbasCorrectionStore,
        geo: &str,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        selection_epoch: &ExactEpochQueryValue,
        mode: Option<String>,
    ) -> Result<f64, JsValue> {
        let geo = parse_sat(geo)?;
        let satellite = parse_sat(satellite)?;
        let source = SbasCorrectedEphemeris::new(&self.inner, store.core(), geo)
            .with_mode(parse_mode(mode)?);
        Ok(crate::sp3::precise_variance_at_queries(
            &source,
            satellite,
            state_epoch,
            selection_epoch,
        ))
    }

    /// Evaluate the state-dependent SBAS clock relativity term at an exact
    /// epoch and supplied satellite position in ECEF metres.
    #[wasm_bindgen(js_name = sbasClockRelativityAtExactQuery, unchecked_return_type = "ClockRelativity")]
    pub fn sbas_clock_relativity_at_exact_query(
        &self,
        store: &SbasCorrectionStore,
        geo: &str,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        position_ecef_m: Vec<f64>,
        mode: Option<String>,
    ) -> Result<JsValue, JsValue> {
        let geo = parse_sat(geo)?;
        let satellite = parse_sat(satellite)?;
        let position_ecef_m: [f64; 3] = position_ecef_m
            .try_into()
            .map_err(|_| type_error("positionEcefM must contain exactly three coordinates"))?;
        let source = SbasCorrectedEphemeris::new(&self.inner, store.core(), geo)
            .with_mode(parse_mode(mode)?);
        crate::sp3::precise_clock_relativity_at_query(
            &source,
            satellite,
            state_epoch,
            position_ecef_m,
        )
    }
}

/// Convert an SBAS broadcast PRN number such as `129` to an SBAS satellite
/// token such as `"S29"`. Returns `null` when the PRN is outside the SBAS range.
#[wasm_bindgen(js_name = sbasPrnToSat)]
pub fn sbas_prn_to_sat(broadcast_prn: u16) -> JsValue {
    core_sbas_prn_to_sat(broadcast_prn)
        .map(|sat| JsValue::from_str(&sat.to_string()))
        .unwrap_or(JsValue::NULL)
}

/// Convert an SBAS satellite token such as `"S29"` to broadcast PRN number
/// such as `129`. Only the slots a broadcast PRN exists for convert, `S20`
/// through `S58` (PRN 120 through 158); returns `null` for any other SBAS slot
/// and for a satellite of another constellation.
#[wasm_bindgen(js_name = satToSbasPrn)]
pub fn sat_to_sbas_prn(sat: &str) -> Result<JsValue, JsValue> {
    let sat = parse_sat(sat)?;
    Ok(core_sat_to_sbas_prn(sat)
        .map(|prn| JsValue::from_f64(f64::from(prn)))
        .unwrap_or(JsValue::NULL))
}

/// SBAS-corrected broadcast satellite position and clock.
///
/// Position is ECEF meters and clock is seconds at `tJ2000S`, seconds since
/// J2000. Returns `null` when the selected SBAS mode cannot provide a state.
#[wasm_bindgen(js_name = sbasCorrectedState)]
pub fn sbas_corrected_state(
    broadcast: &BroadcastEphemeris,
    store: &SbasCorrectionStore,
    geo: &str,
    sat: &str,
    t_j2000_s: f64,
    mode: Option<String>,
) -> Result<JsValue, JsValue> {
    let geo = parse_sat(geo)?;
    let sat = parse_sat(sat)?;
    let eph = SbasCorrectedEphemeris::new(&broadcast.inner, store.core(), geo)
        .with_mode(parse_mode(mode)?);
    let Some((position_ecef_m, clock_s)) = eph.position_clock_at_j2000_s(sat, t_j2000_s) else {
        return Ok(JsValue::NULL);
    };
    serde_wasm_bindgen::to_value(&CorrectedStateJs {
        position_ecef_m,
        clock_s,
    })
    .map_err(|e| type_error(&e.to_string()))
}

/// Solve SPP using an SBAS-corrected broadcast source and optional SBAS iono.
///
/// The returned solution uses the same units as `solveSpp`: ECEF meters,
/// receiver clock seconds, residual meters, and optional geodetic radians plus
/// ellipsoidal height meters.
#[wasm_bindgen(js_name = solveSppSbas)]
pub fn solve_spp_sbas(
    broadcast: &BroadcastEphemeris,
    store: &SbasCorrectionStore,
    geo: &str,
    request: JsValue,
    mode: Option<String>,
) -> Result<SppSolution, JsValue> {
    let geo = parse_sat(geo)?;
    let (mut inputs, with_geodetic) = spp::build_inputs(request)?;
    let eph = SbasCorrectedEphemeris::new(&broadcast.inner, store.core(), geo)
        .with_mode(parse_mode(mode)?);
    inputs.sbas_iono = eph.iono_grid().cloned();
    let solution = sidereon::solve_spp(
        &eph,
        &inputs,
        with_geodetic,
        sidereon_core::positioning::SolvePolicy::default(),
    )
    .map_err(|e| crate::positioning_error::facade_error(&e))?;
    Ok(SppSolution { inner: solution })
}

// The unassigned PRN-mask correction counts `SbasCorrectionStore` reports.
// `wasm-pack` writes this into both `sidereon.d.ts` targets;
// `types/sidereon-extra.d.ts` re-exports it.
#[wasm_bindgen(typescript_custom_section)]
const TS_SBAS_DEFINITIONS: &str = r#"
/**
 * Corrections a GEO addressed to one PRN-mask number that names no satellite
 * held here. `count` is exact.
 */
export interface SbasUnassignedMaskCorrections {
  maskNumber: number;
  count: bigint;
}
"#;
