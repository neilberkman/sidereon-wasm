//! RTCM 3.x differential-GNSS stream decoding.
//!
//! Thin wrappers over `sidereon_core::rtcm`. The codec, framing, and per-message
//! grammar live entirely in the crate; this module only marshals the decoded
//! canonical IR into idiomatic JS objects. Each [`Message`] variant crosses as a
//! plain object tagged with a `type` discriminant, carrying the raw transmitted
//! field integers exactly as the IR stores them (large fields cross as `bigint`
//! to preserve precision). [`FrameScanner`] wraps the forgiving stream scanner.

use serde::Serialize;
use wasm_bindgen::prelude::*;

use sidereon_core::rtcm::{
    decode_frame as core_decode_frame, decode_messages as core_decode_messages,
    decode_stream_with_policy as core_decode_stream_with_policy, derive_lli as core_derive_lli,
    encode_frame_with_reserved as core_encode_frame_with_reserved,
    message_number as core_message_number, minimum_lock_time_ms as core_minimum_lock_time_ms,
    msm_epoch_dt_ms as core_msm_epoch_dt_ms, msm_signal_mask as core_msm_signal_mask,
    msm_signal_rinex_code as core_msm_signal_rinex_code, AntennaDescriptor, BeidouEphemeris,
    FkpGradient, FkpGradients, FrameScanner as CoreFrameScanner, GalileoFnavEphemeris,
    GalileoInavEphemeris, GlonassCodePhaseBiases, GlonassEphemeris, GpsEphemeris, GridResidual,
    HelmertTransformation, LegacyL1, LegacyL2, LegacyObservations, LegacySatellite,
    LockTimeTracker as CoreLockTimeTracker, Message, MessageAnnouncement, MsmHeader, MsmKind,
    MsmMessage, MsmSatellite, MsmSignal, NavicEphemeris, NetworkAuxiliaryStation,
    NetworkCorrectionDifference, NetworkCorrectionDifferences, NetworkResidual, NetworkResiduals,
    PhysicalReferenceStation, PreviousLock, Projection, ProjectionParameters, QzssEphemeris,
    ResidualGrid, RotationPoint, RtcmDeparture, RtcmPolicy, SsrClockRecord, SsrCodeBiasRecord,
    SsrHeader, SsrKind, SsrMessage, SsrOrbitRecord, SsrPhaseBiasRecord, SsrPhaseBiasSignal,
    SsrVtecLayer, SsrVtecMessage, StationCoordinates, SystemParameters, TextMessage,
    UnsupportedMessage, LLI_HALF_CYCLE, LLI_LOSS_OF_LOCK,
};
use sidereon_core::GnssSystem;

use crate::error::{engine_error, error_with_detail, type_error};
use crate::label::{lower_camel_variant, Label};

/// Read an RTCM policy: `"strict"` (the default) refuses a departure from RTCM
/// 3 by name; `"lenient"` reads or writes it and reports it.
fn rtcm_policy(label: Option<String>) -> Result<RtcmPolicy, JsValue> {
    match label.as_deref() {
        None | Some("strict") => Ok(RtcmPolicy::Strict),
        Some("lenient") => Ok(RtcmPolicy::Lenient),
        Some(other) => Err(type_error(&format!(
            "invalid RTCM policy {other:?}: expected \"strict\" or \"lenient\""
        ))),
    }
}

#[cfg(test)]
mod rtcm_encode_error_contract_tests {
    use super::rtcm_encode_error_payload;
    use sidereon_core::rtcm::{
        MsmKind, MsmMaskProblem, MsmOptionalField, MsmOptionalProblem, RtcmDeparture,
        RtcmEncodeError as Encode, RtcmFieldEncoding, RtcmRecordKind, SsrKind,
    };

    #[test]
    fn every_rtcm_encode_variant_and_field_has_the_documented_js_detail() {
        let departure = RtcmDeparture::FrameReservedBits { reserved: 5 };
        let departure_message = departure.to_string();
        let cases = vec![
            (
                Encode::FieldOutOfRange {
                    message_number: 1005,
                    field: "ecef_x".into(),
                    value: -2,
                    width: 38,
                    encoding: RtcmFieldEncoding::TwosComplement,
                },
                serde_json::json!({"kind":"fieldOutOfRange","messageNumber":1005,"field":"ecef_x","value":"-2","width":38,"encoding":"twosComplement"}),
            ),
            (
                Encode::NegativeZeroWithValue {
                    message_number: 1020,
                    field: "tau_n".into(),
                    value: 7,
                },
                serde_json::json!({"kind":"negativeZeroWithValue","messageNumber":1020,"field":"tau_n","value":7}),
            ),
            (
                Encode::NegativeZeroMask {
                    message_number: 1020,
                    mask: 9,
                },
                serde_json::json!({"kind":"negativeZeroMask","messageNumber":1020,"mask":9}),
            ),
            (
                Encode::MessageNumber {
                    message_number: 999,
                    record: RtcmRecordKind::StationCoordinates,
                },
                serde_json::json!({"kind":"messageNumber","messageNumber":999,"record":{"kind":"stationCoordinates"}}),
            ),
            (
                Encode::FieldPresence {
                    message_number: 1005,
                    record: RtcmRecordKind::StationCoordinates,
                    field: "antenna_height",
                    carried: false,
                },
                serde_json::json!({"kind":"fieldPresence","messageNumber":1005,"record":{"kind":"stationCoordinates"},"field":"antenna_height","carried":false}),
            ),
            (
                Encode::SatelliteFieldPresence {
                    message_number: 1074,
                    record: RtcmRecordKind::Msm {
                        system: sidereon_core::GnssSystem::Gps,
                        kind: MsmKind::Msm4,
                    },
                    satellite: 7,
                    field: "extended_info",
                    carried: false,
                },
                serde_json::json!({"kind":"satelliteFieldPresence","messageNumber":1074,"record":{"kind":"msm","system":"GPS","messageKind":"msm4"},"satellite":7,"field":"extended_info","carried":false}),
            ),
            (
                Encode::CountMismatch {
                    message_number: 1015,
                    field: "satellites",
                    expected: 2,
                    actual: 1,
                },
                serde_json::json!({"kind":"countMismatch","messageNumber":1015,"field":"satellites","expected":2,"actual":1}),
            ),
            (
                Encode::ValueOutOfRange {
                    message_number: 1005,
                    field: "itrf".into(),
                    value: 64,
                    minimum: 0,
                    maximum: 63,
                },
                serde_json::json!({"kind":"valueOutOfRange","messageNumber":1005,"field":"itrf","value":"64","minimum":"0","maximum":"63"}),
            ),
            (
                Encode::NonLatin1Character {
                    field: "descriptor".into(),
                    character: 'λ',
                },
                serde_json::json!({"kind":"nonLatin1Character","field":"descriptor","character":"λ"}),
            ),
            (
                Encode::SatelliteIdOutOfRange {
                    message_number: 1019,
                    field: "GPS PRN",
                    value: 64,
                    width: 6,
                },
                serde_json::json!({"kind":"satelliteIdOutOfRange","messageNumber":1019,"field":"GPS PRN","value":64,"width":6}),
            ),
            (
                Encode::SsrSatelliteIdOutOfRange {
                    message_number: 1057,
                    value: 64,
                    width: 6,
                },
                serde_json::json!({"kind":"ssrSatelliteIdOutOfRange","messageNumber":1057,"value":64,"width":6}),
            ),
            (
                Encode::SsrRecordsNotCarried {
                    message_number: 1058,
                    kind: SsrKind::Clock,
                    records: "orbit",
                    count: 2,
                },
                serde_json::json!({"kind":"ssrRecordsNotCarried","messageNumber":1058,"ssrKind":"clock","records":"orbit","count":2}),
            ),
            (
                Encode::SsrCombinedRecordCounts {
                    message_number: 1060,
                    orbit: 2,
                    clock: 1,
                },
                serde_json::json!({"kind":"ssrCombinedRecordCounts","messageNumber":1060,"orbit":2,"clock":1}),
            ),
            (
                Encode::SsrCombinedSatelliteMismatch {
                    message_number: 1060,
                    index: 1,
                    orbit_satellite: 4,
                    clock_satellite: 5,
                },
                serde_json::json!({"kind":"ssrCombinedSatelliteMismatch","messageNumber":1060,"index":1,"orbitSatellite":4,"clockSatellite":5}),
            ),
            (
                Encode::SsrHighRateClockTerms {
                    message_number: 1062,
                    satellite: 3,
                    c1: -4,
                    c2: 5,
                },
                serde_json::json!({"kind":"ssrHighRateClockTerms","messageNumber":1062,"satellite":3,"c1":-4,"c2":5}),
            ),
            (
                Encode::SsrSatelliteCount {
                    message_number: 1057,
                    declared: 2,
                    records: 1,
                },
                serde_json::json!({"kind":"ssrSatelliteCount","messageNumber":1057,"declared":2,"records":1}),
            ),
            (
                Encode::MsmMask {
                    message_number: 1074,
                    problem: MsmMaskProblem::SignalNotInMask { signal: 3, mask: 5 },
                },
                serde_json::json!({"kind":"msmMask","messageNumber":1074,"problem":{"kind":"signalNotInMask","signal":3,"mask":5}}),
            ),
            (
                Encode::MsmOptional {
                    message_number: 1077,
                    kind: MsmKind::Msm7,
                    satellite: 4,
                    signal: Some(6),
                    field: MsmOptionalField::FinePhaseRangeRate,
                    problem: MsmOptionalProblem::InvalidValue(-16384),
                },
                serde_json::json!({"kind":"msmOptional","messageNumber":1077,"messageKind":"msm7","satellite":4,"signal":6,"field":"finePhaseRangeRate","problem":{"kind":"invalidValue","value":-16384}}),
            ),
            (
                Encode::TrailingZeroBits {
                    message_number: 1006,
                    bits: 3,
                },
                serde_json::json!({"kind":"trailingZeroBits","messageNumber":1006,"bits":3}),
            ),
            (
                Encode::StrictDeparture(departure),
                serde_json::json!({"kind":"strictDeparture","departure":{"kind":"frameReservedBits","message":departure_message,"reserved":5}}),
            ),
            (
                Encode::UnsupportedBodyTooShort {
                    message_number: 4090,
                },
                serde_json::json!({"kind":"unsupportedBodyTooShort","messageNumber":4090}),
            ),
            (
                Encode::UnsupportedBodyNumber {
                    message_number: 4090,
                    carried: 4089,
                },
                serde_json::json!({"kind":"unsupportedBodyNumber","messageNumber":4090,"carried":4089}),
            ),
            (
                Encode::UnsupportedDecodedNumber {
                    message_number: 4090,
                },
                serde_json::json!({"kind":"unsupportedDecodedNumber","messageNumber":4090}),
            ),
            (
                Encode::FrameBodyTooLong { len: 1024 },
                serde_json::json!({"kind":"frameBodyTooLong","len":1024}),
            ),
            (
                Encode::FrameReservedOutOfRange { value: 64 },
                serde_json::json!({"kind":"frameReservedOutOfRange","value":64}),
            ),
        ];

        assert_eq!(cases.len(), 25);
        for (error, expected) in cases {
            assert_eq!(rtcm_encode_error_payload(&error), expected);
            assert!(!error.to_string().is_empty());
        }
    }
}

/// A departure from RTCM 3, as `{ kind, message, messageNumber?, ... }`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DepartureObject {
    /// `"frameReservedBits"`, `"trailingBits"`, `"msmCellMaskOver64"`,
    /// `"ssrRecordsShort"`, or, for one this binding does not name yet, the
    /// engine variant's name in the same case.
    kind: Label,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_number: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reserved: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bits: Option<Vec<bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cells: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    declared: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    read: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    offset: Option<f64>,
}

impl From<&RtcmDeparture> for DepartureObject {
    fn from(departure: &RtcmDeparture) -> Self {
        let mut out = DepartureObject {
            kind: lower_camel_variant(departure),
            message: departure.to_string(),
            message_number: None,
            reserved: None,
            bits: None,
            cells: None,
            declared: None,
            read: None,
            offset: None,
        };
        match departure {
            RtcmDeparture::FrameReservedBits { reserved } => {
                out.kind = Label::Borrowed("frameReservedBits");
                out.reserved = Some(*reserved);
            }
            RtcmDeparture::TrailingBits {
                message_number,
                bits,
            } => {
                out.kind = Label::Borrowed("trailingBits");
                out.message_number = Some(*message_number);
                out.bits = Some(bits.clone());
            }
            RtcmDeparture::MsmCellMaskOver64 {
                message_number,
                cells,
            } => {
                out.kind = Label::Borrowed("msmCellMaskOver64");
                out.message_number = Some(*message_number);
                out.cells = Some(*cells as u32);
            }
            RtcmDeparture::SsrRecordsShort {
                message_number,
                declared,
                read,
            } => {
                out.kind = Label::Borrowed("ssrRecordsShort");
                out.message_number = Some(*message_number);
                out.declared = Some(*declared as u32);
                out.read = Some(*read as u32);
            }
            _ => {}
        }
        out
    }
}

fn gnss_system_label(system: GnssSystem) -> &'static str {
    system.as_str()
}

fn msm_kind_label(kind: MsmKind) -> &'static str {
    match kind {
        MsmKind::Msm1 => "msm1",
        MsmKind::Msm2 => "msm2",
        MsmKind::Msm3 => "msm3",
        MsmKind::Msm4 => "msm4",
        MsmKind::Msm5 => "msm5",
        MsmKind::Msm6 => "msm6",
        MsmKind::Msm7 => "msm7",
    }
}

fn ssr_kind_label(kind: SsrKind) -> &'static str {
    match kind {
        SsrKind::Orbit => "orbit",
        SsrKind::Clock => "clock",
        SsrKind::CombinedOrbitClock => "combinedOrbitClock",
        SsrKind::CodeBias => "codeBias",
        SsrKind::PhaseBias => "phaseBias",
        SsrKind::Ura => "ura",
        SsrKind::HighRateClock => "highRateClock",
    }
}

// --- mirror structs (camelCase JS objects) ----------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StationObject {
    message_number: u16,
    reference_station_id: u16,
    itrf_realization_year: u8,
    gps_indicator: bool,
    glonass_indicator: bool,
    galileo_indicator: bool,
    reference_station_indicator: bool,
    ecef_x: i64,
    single_receiver_oscillator: bool,
    reserved: bool,
    ecef_y: i64,
    quarter_cycle_indicator: u8,
    ecef_z: i64,
    antenna_height: Option<u16>,
    trailing_bits: Vec<bool>,
    x_m: f64,
    y_m: f64,
    z_m: f64,
    antenna_height_m: Option<f64>,
}

impl From<&StationCoordinates> for StationObject {
    fn from(s: &StationCoordinates) -> Self {
        Self {
            message_number: s.message_number,
            reference_station_id: s.reference_station_id,
            itrf_realization_year: s.itrf_realization_year,
            gps_indicator: s.gps_indicator,
            glonass_indicator: s.glonass_indicator,
            galileo_indicator: s.galileo_indicator,
            reference_station_indicator: s.reference_station_indicator,
            ecef_x: s.ecef_x,
            single_receiver_oscillator: s.single_receiver_oscillator,
            reserved: s.reserved,
            ecef_y: s.ecef_y,
            quarter_cycle_indicator: s.quarter_cycle_indicator,
            ecef_z: s.ecef_z,
            antenna_height: s.antenna_height,
            trailing_bits: s.trailing_bits.clone(),
            x_m: s.x_m(),
            y_m: s.y_m(),
            z_m: s.z_m(),
            antenna_height_m: s.antenna_height_m(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AntennaObject {
    message_number: u16,
    reference_station_id: u16,
    antenna_descriptor: String,
    antenna_setup_id: u8,
    antenna_serial_number: Option<String>,
    receiver_type: Option<String>,
    receiver_firmware_version: Option<String>,
    receiver_serial_number: Option<String>,
    trailing_bits: Vec<bool>,
}

impl From<&AntennaDescriptor> for AntennaObject {
    fn from(a: &AntennaDescriptor) -> Self {
        Self {
            message_number: a.message_number,
            reference_station_id: a.reference_station_id,
            antenna_descriptor: a.antenna_descriptor.clone(),
            antenna_setup_id: a.antenna_setup_id,
            antenna_serial_number: a.antenna_serial_number.clone(),
            receiver_type: a.receiver_type.clone(),
            receiver_firmware_version: a.receiver_firmware_version.clone(),
            receiver_serial_number: a.receiver_serial_number.clone(),
            trailing_bits: a.trailing_bits.clone(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MsmHeaderObject {
    reference_station_id: u16,
    epoch_time: u32,
    multiple_message: bool,
    iods: u8,
    reserved: u8,
    clock_steering: u8,
    external_clock: u8,
    divergence_free_smoothing: bool,
    smoothing_interval: u8,
}

impl From<&MsmHeader> for MsmHeaderObject {
    fn from(h: &MsmHeader) -> Self {
        Self {
            reference_station_id: h.reference_station_id,
            epoch_time: h.epoch_time,
            multiple_message: h.multiple_message,
            iods: h.iods,
            reserved: h.reserved,
            clock_steering: h.clock_steering,
            external_clock: h.external_clock,
            divergence_free_smoothing: h.divergence_free_smoothing,
            smoothing_interval: h.smoothing_interval,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MsmSatelliteObject {
    id: u8,
    rough_range_ms: Option<u8>,
    rough_range_mod1: u16,
    extended_info: Option<u8>,
    rough_phase_range_rate_m_s: Option<i16>,
}

impl From<&MsmSatellite> for MsmSatelliteObject {
    fn from(s: &MsmSatellite) -> Self {
        Self {
            id: s.id,
            rough_range_ms: s.rough_range_ms,
            rough_range_mod1: s.rough_range_mod1,
            extended_info: s.extended_info,
            rough_phase_range_rate_m_s: s.rough_phase_range_rate_m_s,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MsmSignalObject {
    satellite_id: u8,
    signal_id: u8,
    fine_pseudorange: Option<i32>,
    fine_phase_range: Option<i32>,
    lock_time_indicator: Option<u16>,
    half_cycle_ambiguity: Option<bool>,
    cnr: Option<u16>,
    fine_phase_range_rate: Option<i16>,
}

impl From<&MsmSignal> for MsmSignalObject {
    fn from(s: &MsmSignal) -> Self {
        Self {
            satellite_id: s.satellite_id,
            signal_id: s.signal_id,
            fine_pseudorange: s.fine_pseudorange,
            fine_phase_range: s.fine_phase_range,
            lock_time_indicator: s.lock_time_indicator,
            half_cycle_ambiguity: s.half_cycle_ambiguity,
            cnr: s.cnr,
            fine_phase_range_rate: s.fine_phase_range_rate,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MsmObject {
    message_number: u16,
    system: &'static str,
    kind: &'static str,
    header: MsmHeaderObject,
    signal_mask: u32,
    satellites: Vec<MsmSatelliteObject>,
    signals: Vec<MsmSignalObject>,
    trailing_bits: Vec<bool>,
}

impl From<&MsmMessage> for MsmObject {
    fn from(m: &MsmMessage) -> Self {
        Self {
            message_number: m.message_number,
            system: gnss_system_label(m.system),
            kind: msm_kind_label(m.kind),
            header: MsmHeaderObject::from(&m.header),
            signal_mask: m.signal_mask,
            satellites: m.satellites.iter().map(MsmSatelliteObject::from).collect(),
            signals: m.signals.iter().map(MsmSignalObject::from).collect(),
            trailing_bits: m.trailing_bits.clone(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CellLliObject {
    satellite_id: u8,
    signal_id: u8,
    lli: u8,
    min_lock_time_ms: Option<u32>,
}

impl From<&sidereon_core::rtcm::CellLli> for CellLliObject {
    fn from(cell: &sidereon_core::rtcm::CellLli) -> Self {
        Self {
            satellite_id: cell.satellite_id,
            signal_id: cell.signal_id,
            lli: cell.lli,
            min_lock_time_ms: cell.min_lock_time_ms,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FrameSkipObject {
    offset: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_number: Option<u16>,
    reason: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

impl From<&sidereon_core::rtcm::FrameSkip> for FrameSkipObject {
    fn from(skip: &sidereon_core::rtcm::FrameSkip) -> Self {
        match &skip.reason {
            sidereon_core::rtcm::FrameSkipReason::Truncated => Self {
                offset: skip.offset as f64,
                message_number: skip.message_number,
                reason: "truncated",
                message: None,
            },
            sidereon_core::rtcm::FrameSkipReason::Malformed(message) => Self {
                offset: skip.offset as f64,
                message_number: skip.message_number,
                reason: "malformed",
                message: Some(message.clone()),
            },
            sidereon_core::rtcm::FrameSkipReason::Departure(departure) => Self {
                offset: skip.offset as f64,
                message_number: skip.message_number,
                reason: "departure",
                message: Some(departure.to_string()),
            },
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamDiagnosticsObject {
    resync_bytes: f64,
    crc_failures: f64,
    skipped_frames: Vec<FrameSkipObject>,
    departures: Vec<DepartureObject>,
}

impl From<&sidereon_core::rtcm::StreamDiagnostics> for StreamDiagnosticsObject {
    fn from(diagnostics: &sidereon_core::rtcm::StreamDiagnostics) -> Self {
        Self {
            resync_bytes: diagnostics.resync_bytes as f64,
            crc_failures: diagnostics.crc_failures as f64,
            skipped_frames: diagnostics
                .skipped_frames
                .iter()
                .map(FrameSkipObject::from)
                .collect(),
            departures: diagnostics
                .departures
                .iter()
                .map(|entry| DepartureObject {
                    offset: Some(entry.offset as f64),
                    ..DepartureObject::from(&entry.departure)
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RtcmStreamObject {
    messages: Vec<MessageObject>,
    diagnostics: StreamDiagnosticsObject,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GpsEphemerisObject {
    message_number: u16,
    satellite_id: u8,
    week_number: u16,
    sv_accuracy: u8,
    code_on_l2: u8,
    idot: i32,
    iode: u8,
    t_oc: u16,
    a_f2: i16,
    a_f1: i32,
    a_f0: i32,
    iodc: u16,
    c_rs: i32,
    delta_n: i32,
    m0: i64,
    c_uc: i32,
    eccentricity: u64,
    c_us: i32,
    sqrt_a: u64,
    t_oe: u16,
    c_ic: i32,
    omega0: i64,
    c_is: i32,
    i0: i64,
    c_rc: i32,
    omega: i64,
    omega_dot: i32,
    t_gd: i16,
    sv_health: u8,
    l2_p_data_flag: bool,
    fit_interval: bool,
    trailing_bits: Vec<bool>,
}

impl From<&GpsEphemeris> for GpsEphemerisObject {
    fn from(e: &GpsEphemeris) -> Self {
        Self {
            message_number: 1019,
            satellite_id: e.satellite_id,
            week_number: e.week_number,
            sv_accuracy: e.sv_accuracy,
            code_on_l2: e.code_on_l2,
            idot: e.idot,
            iode: e.iode,
            t_oc: e.t_oc,
            a_f2: e.a_f2,
            a_f1: e.a_f1,
            a_f0: e.a_f0,
            iodc: e.iodc,
            c_rs: e.c_rs,
            delta_n: e.delta_n,
            m0: e.m0,
            c_uc: e.c_uc,
            eccentricity: e.eccentricity,
            c_us: e.c_us,
            sqrt_a: e.sqrt_a,
            t_oe: e.t_oe,
            c_ic: e.c_ic,
            omega0: e.omega0,
            c_is: e.c_is,
            i0: e.i0,
            c_rc: e.c_rc,
            omega: e.omega,
            omega_dot: e.omega_dot,
            t_gd: e.t_gd,
            sv_health: e.sv_health,
            l2_p_data_flag: e.l2_p_data_flag,
            fit_interval: e.fit_interval,
            trailing_bits: e.trailing_bits.clone(),
        }
    }
}

macro_rules! rtcm_ephemeris_object {
    ($object:ident, $core:ident, $message_number:literal, { $($field:ident : $ty:ty),+ $(,)? }) => {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct $object {
            message_number: u16,
            $($field: $ty,)+
            trailing_bits: Vec<bool>,
        }

        impl From<&$core> for $object {
            fn from(e: &$core) -> Self {
                Self {
                    message_number: $message_number,
                    $($field: e.$field,)+
                    trailing_bits: e.trailing_bits.clone(),
                }
            }
        }
    };
}

rtcm_ephemeris_object!(GalileoFnavEphemerisObject, GalileoFnavEphemeris, 1045, {
    satellite_id: u8,
    week_number: u16,
    iod_nav: u16,
    sisa: u8,
    idot: i32,
    t_oc: u16,
    a_f2: i16,
    a_f1: i32,
    a_f0: i64,
    c_rs: i32,
    delta_n: i32,
    m0: i64,
    c_uc: i32,
    eccentricity: u64,
    c_us: i32,
    sqrt_a: u64,
    t_oe: u16,
    c_ic: i32,
    omega0: i64,
    c_is: i32,
    i0: i64,
    c_rc: i32,
    omega: i64,
    omega_dot: i32,
    bgd_e5a_e1: i16,
    e5a_signal_health: u8,
    e5a_data_validity: bool,
    reserved: u8,
});

rtcm_ephemeris_object!(GalileoInavEphemerisObject, GalileoInavEphemeris, 1046, {
    satellite_id: u8,
    week_number: u16,
    iod_nav: u16,
    sisa_index: u8,
    idot: i32,
    t_oc: u16,
    a_f2: i16,
    a_f1: i32,
    a_f0: i64,
    c_rs: i32,
    delta_n: i32,
    m0: i64,
    c_uc: i32,
    eccentricity: u64,
    c_us: i32,
    sqrt_a: u64,
    t_oe: u16,
    c_ic: i32,
    omega0: i64,
    c_is: i32,
    i0: i64,
    c_rc: i32,
    omega: i64,
    omega_dot: i32,
    bgd_e5a_e1: i16,
    bgd_e5b_e1: i16,
    e5b_signal_health: u8,
    e5b_data_validity: bool,
    e1b_signal_health: u8,
    e1b_data_validity: bool,
    reserved: u8,
});

rtcm_ephemeris_object!(BeidouEphemerisObject, BeidouEphemeris, 1042, {
    satellite_id: u8,
    week_number: u16,
    sv_urai: u8,
    idot: i32,
    aode: u8,
    t_oc: u32,
    a_f2: i16,
    a_f1: i32,
    a_f0: i32,
    aodc: u8,
    c_rs: i32,
    delta_n: i32,
    m0: i64,
    c_uc: i32,
    eccentricity: u64,
    c_us: i32,
    sqrt_a: u64,
    t_oe: u32,
    c_ic: i32,
    omega0: i64,
    c_is: i32,
    i0: i64,
    c_rc: i32,
    omega: i64,
    omega_dot: i32,
    t_gd1: i16,
    t_gd2: i16,
    sv_health: bool,
});

rtcm_ephemeris_object!(NavicEphemerisObject, NavicEphemeris, 1041, {
    satellite_id: u8,
    week_number: u16,
    a_f0: i32,
    a_f1: i32,
    a_f2: i16,
    ura: u8,
    t_oc: u16,
    t_gd: i16,
    delta_n: i32,
    iodec: u8,
    reserved: u16,
    l5_flag: bool,
    s_flag: bool,
    c_uc: i32,
    c_us: i32,
    c_ic: i32,
    c_is: i32,
    c_rc: i32,
    c_rs: i32,
    idot: i32,
    m0: i64,
    t_oe: u16,
    eccentricity: u64,
    sqrt_a: u64,
    omega0: i64,
    omega: i64,
    omega_dot: i32,
    i0: i64,
    spare_df544: u8,
    spare_df545: u8,
});

rtcm_ephemeris_object!(QzssEphemerisObject, QzssEphemeris, 1044, {
    satellite_id: u8,
    t_oc: u16,
    a_f2: i16,
    a_f1: i32,
    a_f0: i32,
    iode: u8,
    c_rs: i32,
    delta_n: i32,
    m0: i64,
    c_uc: i32,
    eccentricity: u64,
    c_us: i32,
    sqrt_a: u64,
    t_oe: u16,
    c_ic: i32,
    omega0: i64,
    c_is: i32,
    i0: i64,
    c_rc: i32,
    omega: i64,
    omega_dot: i32,
    idot: i32,
    codes_on_l2: u8,
    week_number: u16,
    ura: u8,
    sv_health: u8,
    t_gd: i16,
    iodc: u16,
    fit_interval: bool,
});

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GlonassEphemerisObject {
    message_number: u16,
    satellite_id: u8,
    frequency_channel: u8,
    almanac_health: bool,
    almanac_health_availability: bool,
    p1: u8,
    t_k: u16,
    b_n_msb: bool,
    p2: bool,
    t_b: u8,
    xn_dot: i32,
    xn: i32,
    xn_dot_dot: i8,
    yn_dot: i32,
    yn: i32,
    yn_dot_dot: i8,
    zn_dot: i32,
    zn: i32,
    zn_dot_dot: i8,
    p3: bool,
    gamma_n: i16,
    m_p: u8,
    m_l_n_third: bool,
    tau_n: i32,
    delta_tau_n: i8,
    e_n: u8,
    m_p4: bool,
    m_f_t: u8,
    m_n_t: u16,
    m_m: u8,
    additional_data_available: bool,
    n_a: u16,
    tau_c: i64,
    m_n4: u8,
    m_tau_gps: i32,
    m_l_n_fifth: bool,
    reserved: u8,
    negative_zero: u16,
    trailing_bits: Vec<bool>,
}

impl From<&GlonassEphemeris> for GlonassEphemerisObject {
    fn from(e: &GlonassEphemeris) -> Self {
        Self {
            message_number: 1020,
            satellite_id: e.satellite_id,
            frequency_channel: e.frequency_channel,
            almanac_health: e.almanac_health,
            almanac_health_availability: e.almanac_health_availability,
            p1: e.p1,
            t_k: e.t_k,
            b_n_msb: e.b_n_msb,
            p2: e.p2,
            t_b: e.t_b,
            xn_dot: e.xn_dot,
            xn: e.xn,
            xn_dot_dot: e.xn_dot_dot,
            yn_dot: e.yn_dot,
            yn: e.yn,
            yn_dot_dot: e.yn_dot_dot,
            zn_dot: e.zn_dot,
            zn: e.zn,
            zn_dot_dot: e.zn_dot_dot,
            p3: e.p3,
            gamma_n: e.gamma_n,
            m_p: e.m_p,
            m_l_n_third: e.m_l_n_third,
            tau_n: e.tau_n,
            delta_tau_n: e.delta_tau_n,
            e_n: e.e_n,
            m_p4: e.m_p4,
            m_f_t: e.m_f_t,
            m_n_t: e.m_n_t,
            m_m: e.m_m,
            additional_data_available: e.additional_data_available,
            n_a: e.n_a,
            tau_c: e.tau_c,
            m_n4: e.m_n4,
            m_tau_gps: e.m_tau_gps,
            m_l_n_fifth: e.m_l_n_fifth,
            reserved: e.reserved,
            negative_zero: e.negative_zero,
            trailing_bits: e.trailing_bits.clone(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UnsupportedObject {
    message_number: u16,
    body: Vec<u8>,
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyL1Value {
    code_indicator: bool,
    pseudorange: u32,
    phase_range_minus_pseudorange: i32,
    lock_time_indicator: u8,
    pseudorange_modulus_ambiguity: Option<u8>,
    cnr: Option<u8>,
}

impl From<LegacyL1> for LegacyL1Value {
    fn from(value: LegacyL1) -> Self {
        Self {
            code_indicator: value.code_indicator,
            pseudorange: value.pseudorange,
            phase_range_minus_pseudorange: value.phase_range_minus_pseudorange,
            lock_time_indicator: value.lock_time_indicator,
            pseudorange_modulus_ambiguity: value.pseudorange_modulus_ambiguity,
            cnr: value.cnr,
        }
    }
}

impl From<LegacyL1Value> for LegacyL1 {
    fn from(value: LegacyL1Value) -> Self {
        Self {
            code_indicator: value.code_indicator,
            pseudorange: value.pseudorange,
            phase_range_minus_pseudorange: value.phase_range_minus_pseudorange,
            lock_time_indicator: value.lock_time_indicator,
            pseudorange_modulus_ambiguity: value.pseudorange_modulus_ambiguity,
            cnr: value.cnr,
        }
    }
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyL2Value {
    code_indicator: u8,
    pseudorange_difference: i16,
    phase_range_minus_l1_pseudorange: i32,
    lock_time_indicator: u8,
    cnr: Option<u8>,
}

impl From<LegacyL2> for LegacyL2Value {
    fn from(value: LegacyL2) -> Self {
        Self {
            code_indicator: value.code_indicator,
            pseudorange_difference: value.pseudorange_difference,
            phase_range_minus_l1_pseudorange: value.phase_range_minus_l1_pseudorange,
            lock_time_indicator: value.lock_time_indicator,
            cnr: value.cnr,
        }
    }
}

impl From<LegacyL2Value> for LegacyL2 {
    fn from(value: LegacyL2Value) -> Self {
        Self {
            code_indicator: value.code_indicator,
            pseudorange_difference: value.pseudorange_difference,
            phase_range_minus_l1_pseudorange: value.phase_range_minus_l1_pseudorange,
            lock_time_indicator: value.lock_time_indicator,
            cnr: value.cnr,
        }
    }
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacySatelliteValue {
    satellite_id: u8,
    frequency_channel: Option<u8>,
    l1: LegacyL1Value,
    l2: Option<LegacyL2Value>,
}

impl From<&LegacySatellite> for LegacySatelliteValue {
    fn from(value: &LegacySatellite) -> Self {
        Self {
            satellite_id: value.satellite_id,
            frequency_channel: value.frequency_channel,
            l1: value.l1.into(),
            l2: value.l2.map(Into::into),
        }
    }
}

impl From<LegacySatelliteValue> for LegacySatellite {
    fn from(value: LegacySatelliteValue) -> Self {
        Self {
            satellite_id: value.satellite_id,
            frequency_channel: value.frequency_channel,
            l1: value.l1.into(),
            l2: value.l2.map(Into::into),
        }
    }
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyObservationsValue {
    message_number: u16,
    reference_station_id: u16,
    epoch_time: u32,
    synchronous_gnss: bool,
    satellite_count: u8,
    divergence_free_smoothing: bool,
    smoothing_interval: u8,
    satellites: Vec<LegacySatelliteValue>,
    #[serde(default)]
    trailing_bits: Vec<bool>,
}

impl From<&LegacyObservations> for LegacyObservationsValue {
    fn from(value: &LegacyObservations) -> Self {
        Self {
            message_number: value.message_number,
            reference_station_id: value.reference_station_id,
            epoch_time: value.epoch_time,
            synchronous_gnss: value.synchronous_gnss,
            satellite_count: value.satellite_count,
            divergence_free_smoothing: value.divergence_free_smoothing,
            smoothing_interval: value.smoothing_interval,
            satellites: value.satellites.iter().map(Into::into).collect(),
            trailing_bits: value.trailing_bits.clone(),
        }
    }
}

impl From<LegacyObservationsValue> for LegacyObservations {
    fn from(value: LegacyObservationsValue) -> Self {
        Self {
            message_number: value.message_number,
            reference_station_id: value.reference_station_id,
            epoch_time: value.epoch_time,
            synchronous_gnss: value.synchronous_gnss,
            satellite_count: value.satellite_count,
            divergence_free_smoothing: value.divergence_free_smoothing,
            smoothing_interval: value.smoothing_interval,
            satellites: value.satellites.into_iter().map(Into::into).collect(),
            trailing_bits: value.trailing_bits,
        }
    }
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct MessageAnnouncementValue {
    message_number: u16,
    synchronous: bool,
    interval: u16,
}

impl From<&MessageAnnouncement> for MessageAnnouncementValue {
    fn from(value: &MessageAnnouncement) -> Self {
        Self {
            message_number: value.message_number,
            synchronous: value.synchronous,
            interval: value.interval,
        }
    }
}

impl From<MessageAnnouncementValue> for MessageAnnouncement {
    fn from(value: MessageAnnouncementValue) -> Self {
        Self {
            message_number: value.message_number,
            synchronous: value.synchronous,
            interval: value.interval,
        }
    }
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SystemParametersValue {
    reference_station_id: u16,
    mjd: u16,
    seconds_of_day: u32,
    announcement_count: u8,
    leap_seconds: u8,
    announcements: Vec<MessageAnnouncementValue>,
    #[serde(default)]
    trailing_bits: Vec<bool>,
}

impl From<&SystemParameters> for SystemParametersValue {
    fn from(value: &SystemParameters) -> Self {
        Self {
            reference_station_id: value.reference_station_id,
            mjd: value.mjd,
            seconds_of_day: value.seconds_of_day,
            announcement_count: value.announcement_count,
            leap_seconds: value.leap_seconds,
            announcements: value.announcements.iter().map(Into::into).collect(),
            trailing_bits: value.trailing_bits.clone(),
        }
    }
}

impl From<SystemParametersValue> for SystemParameters {
    fn from(value: SystemParametersValue) -> Self {
        Self {
            reference_station_id: value.reference_station_id,
            mjd: value.mjd,
            seconds_of_day: value.seconds_of_day,
            announcement_count: value.announcement_count,
            leap_seconds: value.leap_seconds,
            announcements: value.announcements.into_iter().map(Into::into).collect(),
            trailing_bits: value.trailing_bits,
        }
    }
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextMessageValue {
    reference_station_id: u16,
    mjd: u16,
    seconds_of_day: u32,
    character_count: u8,
    code_units: Vec<u8>,
    #[serde(default)]
    trailing_bits: Vec<bool>,
}

impl From<&TextMessage> for TextMessageValue {
    fn from(value: &TextMessage) -> Self {
        Self {
            reference_station_id: value.reference_station_id,
            mjd: value.mjd,
            seconds_of_day: value.seconds_of_day,
            character_count: value.character_count,
            code_units: value.code_units.clone(),
            trailing_bits: value.trailing_bits.clone(),
        }
    }
}

impl From<TextMessageValue> for TextMessage {
    fn from(value: TextMessageValue) -> Self {
        Self {
            reference_station_id: value.reference_station_id,
            mjd: value.mjd,
            seconds_of_day: value.seconds_of_day,
            character_count: value.character_count,
            code_units: value.code_units,
            trailing_bits: value.trailing_bits,
        }
    }
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GlonassCodePhaseBiasesValue {
    reference_station_id: u16,
    aligned: bool,
    reserved: u8,
    l1_ca: Option<i16>,
    l1_p: Option<i16>,
    l2_ca: Option<i16>,
    l2_p: Option<i16>,
    #[serde(default)]
    trailing_bits: Vec<bool>,
}

impl From<&GlonassCodePhaseBiases> for GlonassCodePhaseBiasesValue {
    fn from(value: &GlonassCodePhaseBiases) -> Self {
        Self {
            reference_station_id: value.reference_station_id,
            aligned: value.aligned,
            reserved: value.reserved,
            l1_ca: value.l1_ca,
            l1_p: value.l1_p,
            l2_ca: value.l2_ca,
            l2_p: value.l2_p,
            trailing_bits: value.trailing_bits.clone(),
        }
    }
}

impl From<GlonassCodePhaseBiasesValue> for GlonassCodePhaseBiases {
    fn from(value: GlonassCodePhaseBiasesValue) -> Self {
        Self {
            reference_station_id: value.reference_station_id,
            aligned: value.aligned,
            reserved: value.reserved,
            l1_ca: value.l1_ca,
            l1_p: value.l1_p,
            l2_ca: value.l2_ca,
            l2_p: value.l2_p,
            trailing_bits: value.trailing_bits,
        }
    }
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SsrVtecLayerValue {
    height: u8,
    degree: u8,
    order: u8,
    cosine: Vec<i16>,
    sine: Vec<i16>,
}

impl From<&SsrVtecLayer> for SsrVtecLayerValue {
    fn from(value: &SsrVtecLayer) -> Self {
        Self {
            height: value.height,
            degree: value.degree,
            order: value.order,
            cosine: value.cosine.clone(),
            sine: value.sine.clone(),
        }
    }
}

impl From<SsrVtecLayerValue> for SsrVtecLayer {
    fn from(value: SsrVtecLayerValue) -> Self {
        Self {
            height: value.height,
            degree: value.degree,
            order: value.order,
            cosine: value.cosine,
            sine: value.sine,
        }
    }
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SsrVtecValue {
    message_number: u16,
    igs_ssr_version: Option<u8>,
    epoch_time_s: u32,
    update_interval: u8,
    multiple_message: bool,
    iod_ssr: u8,
    provider_id: u16,
    solution_id: u8,
    quality_indicator: u16,
    layers: Vec<SsrVtecLayerValue>,
    #[serde(default)]
    trailing_bits: Vec<bool>,
}

impl From<&SsrVtecMessage> for SsrVtecValue {
    fn from(value: &SsrVtecMessage) -> Self {
        Self {
            message_number: value.message_number,
            igs_ssr_version: value.igs_ssr_version,
            epoch_time_s: value.epoch_time_s,
            update_interval: value.update_interval,
            multiple_message: value.multiple_message,
            iod_ssr: value.iod_ssr,
            provider_id: value.provider_id,
            solution_id: value.solution_id,
            quality_indicator: value.quality_indicator,
            layers: value.layers.iter().map(Into::into).collect(),
            trailing_bits: value.trailing_bits.clone(),
        }
    }
}

impl From<SsrVtecValue> for SsrVtecMessage {
    fn from(value: SsrVtecValue) -> Self {
        Self {
            message_number: value.message_number,
            igs_ssr_version: value.igs_ssr_version,
            epoch_time_s: value.epoch_time_s,
            update_interval: value.update_interval,
            multiple_message: value.multiple_message,
            iod_ssr: value.iod_ssr,
            provider_id: value.provider_id,
            solution_id: value.solution_id,
            quality_indicator: value.quality_indicator,
            layers: value.layers.into_iter().map(Into::into).collect(),
            trailing_bits: value.trailing_bits,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SsrHeaderObject {
    epoch_time_s: u32,
    update_interval: u8,
    multiple_message: bool,
    iod_ssr: u8,
    provider_id: u16,
    solution_id: u8,
    satellite_reference_datum: Option<bool>,
    dispersive_bias_consistency: Option<bool>,
    mw_consistency: Option<bool>,
    satellite_count: u8,
}

impl From<&SsrHeader> for SsrHeaderObject {
    fn from(h: &SsrHeader) -> Self {
        Self {
            epoch_time_s: h.epoch_time_s,
            update_interval: h.update_interval,
            multiple_message: h.multiple_message,
            iod_ssr: h.iod_ssr,
            provider_id: h.provider_id,
            solution_id: h.solution_id,
            satellite_reference_datum: h.satellite_reference_datum,
            dispersive_bias_consistency: h.dispersive_bias_consistency,
            mw_consistency: h.mw_consistency,
            satellite_count: h.satellite_count,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SsrOrbitObject {
    satellite_id: u8,
    iode: u32,
    iod_crc: Option<u32>,
    delta_radial: i32,
    delta_along: i32,
    delta_cross: i32,
    dot_delta_radial: i32,
    dot_delta_along: i32,
    dot_delta_cross: i32,
}

impl From<&SsrOrbitRecord> for SsrOrbitObject {
    fn from(r: &SsrOrbitRecord) -> Self {
        Self {
            satellite_id: r.satellite_id,
            iode: r.iode,
            iod_crc: r.iod_crc,
            delta_radial: r.delta_radial,
            delta_along: r.delta_along,
            delta_cross: r.delta_cross,
            dot_delta_radial: r.dot_delta_radial,
            dot_delta_along: r.dot_delta_along,
            dot_delta_cross: r.dot_delta_cross,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SsrClockObject {
    satellite_id: u8,
    c0: i32,
    c1: i32,
    c2: i32,
}

impl From<&SsrClockRecord> for SsrClockObject {
    fn from(r: &SsrClockRecord) -> Self {
        Self {
            satellite_id: r.satellite_id,
            c0: r.c0,
            c1: r.c1,
            c2: r.c2,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SsrCodeBiasObject {
    satellite_id: u8,
    biases: Vec<(u8, i16)>,
}

impl From<&SsrCodeBiasRecord> for SsrCodeBiasObject {
    fn from(r: &SsrCodeBiasRecord) -> Self {
        Self {
            satellite_id: r.satellite_id,
            biases: r.biases.clone(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SsrPhaseBiasSignalObject {
    signal_id: u8,
    integer_indicator: u8,
    wide_lane_integer_indicator: u8,
    discontinuity_counter: u8,
    bias: i32,
}

impl From<&SsrPhaseBiasSignal> for SsrPhaseBiasSignalObject {
    fn from(r: &SsrPhaseBiasSignal) -> Self {
        Self {
            signal_id: r.signal_id,
            integer_indicator: r.integer_indicator,
            wide_lane_integer_indicator: r.wide_lane_integer_indicator,
            discontinuity_counter: r.discontinuity_counter,
            bias: r.bias,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SsrPhaseBiasObject {
    satellite_id: u8,
    yaw_angle: u16,
    yaw_rate: i8,
    biases: Vec<SsrPhaseBiasSignalObject>,
}

impl From<&SsrPhaseBiasRecord> for SsrPhaseBiasObject {
    fn from(r: &SsrPhaseBiasRecord) -> Self {
        Self {
            satellite_id: r.satellite_id,
            yaw_angle: r.yaw_angle,
            yaw_rate: r.yaw_rate,
            biases: r
                .biases
                .iter()
                .map(SsrPhaseBiasSignalObject::from)
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SsrObject {
    message_number: u16,
    igs_ssr_version: Option<u8>,
    system: &'static str,
    kind: &'static str,
    header: SsrHeaderObject,
    orbit: Vec<SsrOrbitObject>,
    clock: Vec<SsrClockObject>,
    code_bias: Vec<SsrCodeBiasObject>,
    phase_bias: Vec<SsrPhaseBiasObject>,
    ura: Vec<(u8, u8)>,
    padding_bits: Vec<bool>,
}

impl From<&SsrMessage> for SsrObject {
    fn from(m: &SsrMessage) -> Self {
        Self {
            message_number: m.message_number,
            igs_ssr_version: m.igs_ssr_version,
            system: gnss_system_label(m.system),
            kind: ssr_kind_label(m.kind),
            header: SsrHeaderObject::from(&m.header),
            orbit: m.orbit.iter().map(SsrOrbitObject::from).collect(),
            clock: m.clock.iter().map(SsrClockObject::from).collect(),
            code_bias: m.code_bias.iter().map(SsrCodeBiasObject::from).collect(),
            phase_bias: m.phase_bias.iter().map(SsrPhaseBiasObject::from).collect(),
            ura: m.ura.clone(),
            padding_bits: m.padding_bits.clone(),
        }
    }
}

impl From<&UnsupportedMessage> for UnsupportedObject {
    fn from(u: &UnsupportedMessage) -> Self {
        Self {
            message_number: u.message_number,
            body: u.body.clone(),
        }
    }
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct NetworkAuxiliaryStationValue {
    network_id: u8,
    subnetwork_id: u8,
    auxiliary_station_count: u8,
    master_station_id: u16,
    auxiliary_station_id: u16,
    delta_latitude: i32,
    delta_longitude: i32,
    delta_height: i32,
    #[serde(default)]
    trailing_bits: Vec<bool>,
}

impl From<&NetworkAuxiliaryStation> for NetworkAuxiliaryStationValue {
    fn from(value: &NetworkAuxiliaryStation) -> Self {
        Self {
            network_id: value.network_id,
            subnetwork_id: value.subnetwork_id,
            auxiliary_station_count: value.auxiliary_station_count,
            master_station_id: value.master_station_id,
            auxiliary_station_id: value.auxiliary_station_id,
            delta_latitude: value.delta_latitude,
            delta_longitude: value.delta_longitude,
            delta_height: value.delta_height,
            trailing_bits: value.trailing_bits.clone(),
        }
    }
}

impl From<NetworkAuxiliaryStationValue> for NetworkAuxiliaryStation {
    fn from(value: NetworkAuxiliaryStationValue) -> Self {
        Self {
            network_id: value.network_id,
            subnetwork_id: value.subnetwork_id,
            auxiliary_station_count: value.auxiliary_station_count,
            master_station_id: value.master_station_id,
            auxiliary_station_id: value.auxiliary_station_id,
            delta_latitude: value.delta_latitude,
            delta_longitude: value.delta_longitude,
            delta_height: value.delta_height,
            trailing_bits: value.trailing_bits,
        }
    }
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct NetworkCorrectionDifferenceValue {
    satellite_id: u8,
    ambiguity_status: u8,
    non_sync_count: u8,
    geometric: Option<i32>,
    iod: Option<u8>,
    ionospheric: Option<i32>,
}

impl From<NetworkCorrectionDifference> for NetworkCorrectionDifferenceValue {
    fn from(value: NetworkCorrectionDifference) -> Self {
        Self {
            satellite_id: value.satellite_id,
            ambiguity_status: value.ambiguity_status,
            non_sync_count: value.non_sync_count,
            geometric: value.geometric,
            iod: value.iod,
            ionospheric: value.ionospheric,
        }
    }
}

impl From<NetworkCorrectionDifferenceValue> for NetworkCorrectionDifference {
    fn from(value: NetworkCorrectionDifferenceValue) -> Self {
        Self {
            satellite_id: value.satellite_id,
            ambiguity_status: value.ambiguity_status,
            non_sync_count: value.non_sync_count,
            geometric: value.geometric,
            iod: value.iod,
            ionospheric: value.ionospheric,
        }
    }
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct NetworkCorrectionDifferencesValue {
    message_number: u16,
    network_id: u8,
    subnetwork_id: u8,
    epoch_time: u32,
    multiple_message: bool,
    master_station_id: u16,
    auxiliary_station_id: u16,
    satellite_count: u8,
    satellites: Vec<NetworkCorrectionDifferenceValue>,
    #[serde(default)]
    trailing_bits: Vec<bool>,
}

impl From<&NetworkCorrectionDifferences> for NetworkCorrectionDifferencesValue {
    fn from(value: &NetworkCorrectionDifferences) -> Self {
        Self {
            message_number: value.message_number,
            network_id: value.network_id,
            subnetwork_id: value.subnetwork_id,
            epoch_time: value.epoch_time,
            multiple_message: value.multiple_message,
            master_station_id: value.master_station_id,
            auxiliary_station_id: value.auxiliary_station_id,
            satellite_count: value.satellite_count,
            satellites: value.satellites.iter().copied().map(Into::into).collect(),
            trailing_bits: value.trailing_bits.clone(),
        }
    }
}

impl From<NetworkCorrectionDifferencesValue> for NetworkCorrectionDifferences {
    fn from(value: NetworkCorrectionDifferencesValue) -> Self {
        Self {
            message_number: value.message_number,
            network_id: value.network_id,
            subnetwork_id: value.subnetwork_id,
            epoch_time: value.epoch_time,
            multiple_message: value.multiple_message,
            master_station_id: value.master_station_id,
            auxiliary_station_id: value.auxiliary_station_id,
            satellite_count: value.satellite_count,
            satellites: value.satellites.into_iter().map(Into::into).collect(),
            trailing_bits: value.trailing_bits,
        }
    }
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct NetworkResidualValue {
    satellite_id: u8,
    s_oc: u8,
    s_od: u16,
    s_oh: u8,
    s_lc: u16,
    s_ld: u16,
}

impl From<NetworkResidual> for NetworkResidualValue {
    fn from(value: NetworkResidual) -> Self {
        Self {
            satellite_id: value.satellite_id,
            s_oc: value.s_oc,
            s_od: value.s_od,
            s_oh: value.s_oh,
            s_lc: value.s_lc,
            s_ld: value.s_ld,
        }
    }
}

impl From<NetworkResidualValue> for NetworkResidual {
    fn from(value: NetworkResidualValue) -> Self {
        Self {
            satellite_id: value.satellite_id,
            s_oc: value.s_oc,
            s_od: value.s_od,
            s_oh: value.s_oh,
            s_lc: value.s_lc,
            s_ld: value.s_ld,
        }
    }
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct NetworkResidualsValue {
    message_number: u16,
    epoch_time: u32,
    reference_station_id: u16,
    reference_station_count: u8,
    satellite_count: u8,
    satellites: Vec<NetworkResidualValue>,
    #[serde(default)]
    trailing_bits: Vec<bool>,
}

impl From<&NetworkResiduals> for NetworkResidualsValue {
    fn from(value: &NetworkResiduals) -> Self {
        Self {
            message_number: value.message_number,
            epoch_time: value.epoch_time,
            reference_station_id: value.reference_station_id,
            reference_station_count: value.reference_station_count,
            satellite_count: value.satellite_count,
            satellites: value.satellites.iter().copied().map(Into::into).collect(),
            trailing_bits: value.trailing_bits.clone(),
        }
    }
}

impl From<NetworkResidualsValue> for NetworkResiduals {
    fn from(value: NetworkResidualsValue) -> Self {
        Self {
            message_number: value.message_number,
            epoch_time: value.epoch_time,
            reference_station_id: value.reference_station_id,
            reference_station_count: value.reference_station_count,
            satellite_count: value.satellite_count,
            satellites: value.satellites.into_iter().map(Into::into).collect(),
            trailing_bits: value.trailing_bits,
        }
    }
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PhysicalReferenceStationValue {
    non_physical_station_id: u16,
    physical_station_id: u16,
    itrf_realization_year: u8,
    ecef_x: i64,
    ecef_y: i64,
    ecef_z: i64,
    #[serde(default)]
    trailing_bits: Vec<bool>,
}

impl From<&PhysicalReferenceStation> for PhysicalReferenceStationValue {
    fn from(value: &PhysicalReferenceStation) -> Self {
        Self {
            non_physical_station_id: value.non_physical_station_id,
            physical_station_id: value.physical_station_id,
            itrf_realization_year: value.itrf_realization_year,
            ecef_x: value.ecef_x,
            ecef_y: value.ecef_y,
            ecef_z: value.ecef_z,
            trailing_bits: value.trailing_bits.clone(),
        }
    }
}

impl From<PhysicalReferenceStationValue> for PhysicalReferenceStation {
    fn from(value: PhysicalReferenceStationValue) -> Self {
        Self {
            non_physical_station_id: value.non_physical_station_id,
            physical_station_id: value.physical_station_id,
            itrf_realization_year: value.itrf_realization_year,
            ecef_x: value.ecef_x,
            ecef_y: value.ecef_y,
            ecef_z: value.ecef_z,
            trailing_bits: value.trailing_bits,
        }
    }
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct FkpGradientValue {
    satellite_id: u8,
    iod: u8,
    geometric_north: i16,
    geometric_east: i16,
    ionospheric_north: i16,
    ionospheric_east: i16,
}

impl From<FkpGradient> for FkpGradientValue {
    fn from(value: FkpGradient) -> Self {
        Self {
            satellite_id: value.satellite_id,
            iod: value.iod,
            geometric_north: value.geometric_north,
            geometric_east: value.geometric_east,
            ionospheric_north: value.ionospheric_north,
            ionospheric_east: value.ionospheric_east,
        }
    }
}

impl From<FkpGradientValue> for FkpGradient {
    fn from(value: FkpGradientValue) -> Self {
        Self {
            satellite_id: value.satellite_id,
            iod: value.iod,
            geometric_north: value.geometric_north,
            geometric_east: value.geometric_east,
            ionospheric_north: value.ionospheric_north,
            ionospheric_east: value.ionospheric_east,
        }
    }
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct FkpGradientsValue {
    message_number: u16,
    reference_station_id: u16,
    epoch_time: u32,
    satellite_count: u8,
    satellites: Vec<FkpGradientValue>,
    #[serde(default)]
    trailing_bits: Vec<bool>,
}

impl From<&FkpGradients> for FkpGradientsValue {
    fn from(value: &FkpGradients) -> Self {
        Self {
            message_number: value.message_number,
            reference_station_id: value.reference_station_id,
            epoch_time: value.epoch_time,
            satellite_count: value.satellite_count,
            satellites: value.satellites.iter().copied().map(Into::into).collect(),
            trailing_bits: value.trailing_bits.clone(),
        }
    }
}

impl From<FkpGradientsValue> for FkpGradients {
    fn from(value: FkpGradientsValue) -> Self {
        Self {
            message_number: value.message_number,
            reference_station_id: value.reference_station_id,
            epoch_time: value.epoch_time,
            satellite_count: value.satellite_count,
            satellites: value.satellites.into_iter().map(Into::into).collect(),
            trailing_bits: value.trailing_bits,
        }
    }
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RotationPointValue {
    x: i64,
    y: i64,
    z: i64,
}

impl From<RotationPoint> for RotationPointValue {
    fn from(value: RotationPoint) -> Self {
        Self {
            x: value.x,
            y: value.y,
            z: value.z,
        }
    }
}

impl From<RotationPointValue> for RotationPoint {
    fn from(value: RotationPointValue) -> Self {
        Self {
            x: value.x,
            y: value.y,
            z: value.z,
        }
    }
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct HelmertValue {
    message_number: u16,
    source_name: String,
    target_name: String,
    system_id: u8,
    utilized_messages: u16,
    plate_number: u8,
    computation_indicator: u8,
    height_indicator: u8,
    validity_latitude: i32,
    validity_longitude: i32,
    validity_extension_latitude: u16,
    validity_extension_longitude: u16,
    dx: i32,
    dy: i32,
    dz: i32,
    r1: i32,
    r2: i32,
    r3: i32,
    ds: i32,
    rotation_point: Option<RotationPointValue>,
    add_as: u32,
    add_bs: u32,
    add_at: u32,
    add_bt: u32,
    horizontal_quality: u8,
    vertical_quality: u8,
    #[serde(default)]
    trailing_bits: Vec<bool>,
}

impl From<&HelmertTransformation> for HelmertValue {
    fn from(value: &HelmertTransformation) -> Self {
        Self {
            message_number: value.message_number,
            source_name: value.source_name.clone(),
            target_name: value.target_name.clone(),
            system_id: value.system_id,
            utilized_messages: value.utilized_messages,
            plate_number: value.plate_number,
            computation_indicator: value.computation_indicator,
            height_indicator: value.height_indicator,
            validity_latitude: value.validity_latitude,
            validity_longitude: value.validity_longitude,
            validity_extension_latitude: value.validity_extension_latitude,
            validity_extension_longitude: value.validity_extension_longitude,
            dx: value.dx,
            dy: value.dy,
            dz: value.dz,
            r1: value.r1,
            r2: value.r2,
            r3: value.r3,
            ds: value.ds,
            rotation_point: value.rotation_point.map(Into::into),
            add_as: value.add_as,
            add_bs: value.add_bs,
            add_at: value.add_at,
            add_bt: value.add_bt,
            horizontal_quality: value.horizontal_quality,
            vertical_quality: value.vertical_quality,
            trailing_bits: value.trailing_bits.clone(),
        }
    }
}

impl From<HelmertValue> for HelmertTransformation {
    fn from(value: HelmertValue) -> Self {
        Self {
            message_number: value.message_number,
            source_name: value.source_name,
            target_name: value.target_name,
            system_id: value.system_id,
            utilized_messages: value.utilized_messages,
            plate_number: value.plate_number,
            computation_indicator: value.computation_indicator,
            height_indicator: value.height_indicator,
            validity_latitude: value.validity_latitude,
            validity_longitude: value.validity_longitude,
            validity_extension_latitude: value.validity_extension_latitude,
            validity_extension_longitude: value.validity_extension_longitude,
            dx: value.dx,
            dy: value.dy,
            dz: value.dz,
            r1: value.r1,
            r2: value.r2,
            r3: value.r3,
            ds: value.ds,
            rotation_point: value.rotation_point.map(Into::into),
            add_as: value.add_as,
            add_bs: value.add_bs,
            add_at: value.add_at,
            add_bt: value.add_bt,
            horizontal_quality: value.horizontal_quality,
            vertical_quality: value.vertical_quality,
            trailing_bits: value.trailing_bits,
        }
    }
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GridResidualValue {
    horizontal_1: i16,
    horizontal_2: i16,
    height: i16,
}

impl From<GridResidual> for GridResidualValue {
    fn from(value: GridResidual) -> Self {
        Self {
            horizontal_1: value.horizontal_1,
            horizontal_2: value.horizontal_2,
            height: value.height,
        }
    }
}

impl From<GridResidualValue> for GridResidual {
    fn from(value: GridResidualValue) -> Self {
        Self {
            horizontal_1: value.horizontal_1,
            horizontal_2: value.horizontal_2,
            height: value.height,
        }
    }
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResidualGridValue {
    message_number: u16,
    system_id: u8,
    horizontal_shift: bool,
    vertical_shift: bool,
    origin_1: i32,
    origin_2: i32,
    extension_1: u16,
    extension_2: u16,
    mean_offset_1: i16,
    mean_offset_2: i16,
    mean_height_offset: i16,
    residuals: [GridResidualValue; 16],
    horizontal_interpolation: u8,
    vertical_interpolation: u8,
    horizontal_quality: u8,
    vertical_quality: u8,
    mjd: u16,
    #[serde(default)]
    trailing_bits: Vec<bool>,
}

impl From<&ResidualGrid> for ResidualGridValue {
    fn from(value: &ResidualGrid) -> Self {
        Self {
            message_number: value.message_number,
            system_id: value.system_id,
            horizontal_shift: value.horizontal_shift,
            vertical_shift: value.vertical_shift,
            origin_1: value.origin_1,
            origin_2: value.origin_2,
            extension_1: value.extension_1,
            extension_2: value.extension_2,
            mean_offset_1: value.mean_offset_1,
            mean_offset_2: value.mean_offset_2,
            mean_height_offset: value.mean_height_offset,
            residuals: value.residuals.map(Into::into),
            horizontal_interpolation: value.horizontal_interpolation,
            vertical_interpolation: value.vertical_interpolation,
            horizontal_quality: value.horizontal_quality,
            vertical_quality: value.vertical_quality,
            mjd: value.mjd,
            trailing_bits: value.trailing_bits.clone(),
        }
    }
}

impl From<ResidualGridValue> for ResidualGrid {
    fn from(value: ResidualGridValue) -> Self {
        Self {
            message_number: value.message_number,
            system_id: value.system_id,
            horizontal_shift: value.horizontal_shift,
            vertical_shift: value.vertical_shift,
            origin_1: value.origin_1,
            origin_2: value.origin_2,
            extension_1: value.extension_1,
            extension_2: value.extension_2,
            mean_offset_1: value.mean_offset_1,
            mean_offset_2: value.mean_offset_2,
            mean_height_offset: value.mean_height_offset,
            residuals: value.residuals.map(Into::into),
            horizontal_interpolation: value.horizontal_interpolation,
            vertical_interpolation: value.vertical_interpolation,
            horizontal_quality: value.horizontal_quality,
            vertical_quality: value.vertical_quality,
            mjd: value.mjd,
            trailing_bits: value.trailing_bits,
        }
    }
}

#[derive(Serialize, serde::Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum ProjectionParametersValue {
    NaturalOrigin {
        latitude: i64,
        longitude: i64,
        add_scale: u32,
        false_easting: u64,
        false_northing: i64,
    },
    LambertConicConformal {
        latitude: i64,
        longitude: i64,
        standard_parallel_1: i64,
        standard_parallel_2: i64,
        false_easting: u64,
        false_northing: i64,
    },
    ObliqueMercator {
        rectification: bool,
        latitude: i64,
        longitude: i64,
        azimuth: u64,
        rectified_to_skew: i32,
        add_scale: u32,
        easting: u64,
        northing: i64,
    },
}

impl From<ProjectionParameters> for ProjectionParametersValue {
    fn from(value: ProjectionParameters) -> Self {
        match value {
            ProjectionParameters::NaturalOrigin {
                latitude,
                longitude,
                add_scale,
                false_easting,
                false_northing,
            } => Self::NaturalOrigin {
                latitude,
                longitude,
                add_scale,
                false_easting,
                false_northing,
            },
            ProjectionParameters::LambertConicConformal {
                latitude,
                longitude,
                standard_parallel_1,
                standard_parallel_2,
                false_easting,
                false_northing,
            } => Self::LambertConicConformal {
                latitude,
                longitude,
                standard_parallel_1,
                standard_parallel_2,
                false_easting,
                false_northing,
            },
            ProjectionParameters::ObliqueMercator {
                rectification,
                latitude,
                longitude,
                azimuth,
                rectified_to_skew,
                add_scale,
                easting,
                northing,
            } => Self::ObliqueMercator {
                rectification,
                latitude,
                longitude,
                azimuth,
                rectified_to_skew,
                add_scale,
                easting,
                northing,
            },
        }
    }
}

impl From<ProjectionParametersValue> for ProjectionParameters {
    fn from(value: ProjectionParametersValue) -> Self {
        match value {
            ProjectionParametersValue::NaturalOrigin {
                latitude,
                longitude,
                add_scale,
                false_easting,
                false_northing,
            } => Self::NaturalOrigin {
                latitude,
                longitude,
                add_scale,
                false_easting,
                false_northing,
            },
            ProjectionParametersValue::LambertConicConformal {
                latitude,
                longitude,
                standard_parallel_1,
                standard_parallel_2,
                false_easting,
                false_northing,
            } => Self::LambertConicConformal {
                latitude,
                longitude,
                standard_parallel_1,
                standard_parallel_2,
                false_easting,
                false_northing,
            },
            ProjectionParametersValue::ObliqueMercator {
                rectification,
                latitude,
                longitude,
                azimuth,
                rectified_to_skew,
                add_scale,
                easting,
                northing,
            } => Self::ObliqueMercator {
                rectification,
                latitude,
                longitude,
                azimuth,
                rectified_to_skew,
                add_scale,
                easting,
                northing,
            },
        }
    }
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectionValue {
    system_id: u8,
    projection_type: u8,
    parameters: ProjectionParametersValue,
    #[serde(default)]
    trailing_bits: Vec<bool>,
}

impl From<&Projection> for ProjectionValue {
    fn from(value: &Projection) -> Self {
        Self {
            system_id: value.system_id,
            projection_type: value.projection_type,
            parameters: value.parameters.into(),
            trailing_bits: value.trailing_bits.clone(),
        }
    }
}

impl From<ProjectionValue> for Projection {
    fn from(value: ProjectionValue) -> Self {
        Self {
            system_id: value.system_id,
            projection_type: value.projection_type,
            parameters: value.parameters.into(),
            trailing_bits: value.trailing_bits,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum MessageObject {
    Msm(MsmObject),
    LegacyObservations(LegacyObservationsValue),
    NetworkAuxiliaryStation(NetworkAuxiliaryStationValue),
    NetworkCorrectionDifferences(NetworkCorrectionDifferencesValue),
    NetworkResiduals(NetworkResidualsValue),
    PhysicalReferenceStation(PhysicalReferenceStationValue),
    FkpGradients(FkpGradientsValue),
    StationCoordinates(StationObject),
    AntennaDescriptor(AntennaObject),
    SystemParameters(SystemParametersValue),
    Text(TextMessageValue),
    GlonassCodePhaseBiases(GlonassCodePhaseBiasesValue),
    GpsEphemeris(GpsEphemerisObject),
    GlonassEphemeris(GlonassEphemerisObject),
    BeidouEphemeris(BeidouEphemerisObject),
    NavicEphemeris(NavicEphemerisObject),
    QzssEphemeris(QzssEphemerisObject),
    GalileoFnavEphemeris(GalileoFnavEphemerisObject),
    GalileoInavEphemeris(GalileoInavEphemerisObject),
    Ssr(SsrObject),
    SsrVtec(SsrVtecValue),
    HelmertTransformation(HelmertValue),
    ResidualGrid(ResidualGridValue),
    Projection(ProjectionValue),
    Unsupported(UnsupportedObject),
}

impl From<&Message> for MessageObject {
    fn from(message: &Message) -> Self {
        match message {
            Message::Msm(m) => MessageObject::Msm(MsmObject::from(m)),
            Message::LegacyObservations(value) => MessageObject::LegacyObservations(value.into()),
            Message::NetworkAuxiliaryStation(value) => {
                MessageObject::NetworkAuxiliaryStation(value.into())
            }
            Message::NetworkCorrectionDifferences(value) => {
                MessageObject::NetworkCorrectionDifferences(value.into())
            }
            Message::NetworkResiduals(value) => MessageObject::NetworkResiduals(value.into()),
            Message::PhysicalReferenceStation(value) => {
                MessageObject::PhysicalReferenceStation(value.into())
            }
            Message::FkpGradients(value) => MessageObject::FkpGradients(value.into()),
            Message::StationCoordinates(s) => {
                MessageObject::StationCoordinates(StationObject::from(s))
            }
            Message::AntennaDescriptor(a) => {
                MessageObject::AntennaDescriptor(AntennaObject::from(a))
            }
            Message::SystemParameters(value) => MessageObject::SystemParameters(value.into()),
            Message::Text(value) => MessageObject::Text(value.into()),
            Message::GlonassCodePhaseBiases(value) => {
                MessageObject::GlonassCodePhaseBiases(value.into())
            }
            Message::GpsEphemeris(e) => MessageObject::GpsEphemeris(GpsEphemerisObject::from(e)),
            Message::GlonassEphemeris(e) => {
                MessageObject::GlonassEphemeris(GlonassEphemerisObject::from(e))
            }
            Message::BeidouEphemeris(e) => {
                MessageObject::BeidouEphemeris(BeidouEphemerisObject::from(e))
            }
            Message::NavicEphemeris(e) => {
                MessageObject::NavicEphemeris(NavicEphemerisObject::from(e))
            }
            Message::QzssEphemeris(e) => MessageObject::QzssEphemeris(QzssEphemerisObject::from(e)),
            Message::GalileoFnavEphemeris(e) => {
                MessageObject::GalileoFnavEphemeris(GalileoFnavEphemerisObject::from(e))
            }
            Message::GalileoInavEphemeris(e) => {
                MessageObject::GalileoInavEphemeris(GalileoInavEphemerisObject::from(e))
            }
            Message::Ssr(s) => MessageObject::Ssr(SsrObject::from(s)),
            Message::SsrVtec(value) => MessageObject::SsrVtec(value.into()),
            Message::HelmertTransformation(value) => {
                MessageObject::HelmertTransformation(value.into())
            }
            Message::ResidualGrid(value) => MessageObject::ResidualGrid(value.into()),
            Message::Projection(value) => MessageObject::Projection(value.into()),
            Message::Unsupported(u) => MessageObject::Unsupported(UnsupportedObject::from(u)),
        }
    }
}

fn serializer() -> serde_wasm_bindgen::Serializer {
    serde_wasm_bindgen::Serializer::new()
        .serialize_maps_as_objects(true)
        .serialize_large_number_types_as_bigints(true)
}

/// Decode a complete RTCM 3 byte buffer into the message IR, reading it in full
/// or refusing it by name: a byte outside a CRC-valid frame (a stray byte, a
/// CRC-24Q failure or a trailing partial frame) or a frame whose body does not
/// decode, or departs from RTCM 3, throws an `Error`. `decodeRtcmStream` reads
/// a noisy stream frame by frame and reports each skip. Returns an array of
/// message objects,
/// each tagged with a `type` discriminant (`"msm"`, `"stationCoordinates"`,
/// `"antennaDescriptor"`, `"gpsEphemeris"`, `"glonassEphemeris"`,
/// `"beidouEphemeris"`, `"qzssEphemeris"`, `"galileoFnavEphemeris"`,
/// `"galileoInavEphemeris"`, `"unsupported"`). Delegates to
/// `sidereon_core::rtcm::decode_messages`.
#[wasm_bindgen(js_name = decodeRtcm, unchecked_return_type = "RtcmMessage[]")]
pub fn decode_rtcm(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let objects: Vec<MessageObject> = core_decode_messages(bytes)
        .map_err(engine_error)?
        .iter()
        .map(MessageObject::from)
        .collect();
    objects
        .serialize(&serializer())
        .map_err(|e| type_error(&e.to_string()))
}

/// Decode an RTCM 3 byte stream into messages plus stream diagnostics.
///
/// The `messages` array has the same object form as [`decodeRtcm`].
/// `diagnostics.resyncBytes` counts skipped bytes while finding valid frames,
/// `diagnostics.crcFailures` the preambles whose declared frame failed its
/// CRC-24Q, `diagnostics.skippedFrames` reports CRC-valid frames whose bodies
/// could not be decoded or, under the strict policy, departed from RTCM 3, and
/// `diagnostics.departures` the departures read under the lenient policy, each
/// with its frame `offset`. `policy` is `"strict"` (the default) or
/// `"lenient"`.
#[wasm_bindgen(js_name = decodeRtcmStream, unchecked_return_type = "RtcmStream")]
pub fn decode_rtcm_stream(bytes: &[u8], policy: Option<String>) -> Result<JsValue, JsValue> {
    let stream = core_decode_stream_with_policy(bytes, rtcm_policy(policy)?);
    let object = RtcmStreamObject {
        messages: stream.messages.iter().map(MessageObject::from).collect(),
        diagnostics: StreamDiagnosticsObject::from(&stream.diagnostics),
    };
    object
        .serialize(&serializer())
        .map_err(|e| type_error(&e.to_string()))
}

pub(crate) fn stream_diagnostics_to_js(
    diagnostics: &sidereon_core::rtcm::StreamDiagnostics,
) -> Result<JsValue, JsValue> {
    StreamDiagnosticsObject::from(diagnostics)
        .serialize(&serializer())
        .map_err(|e| type_error(&e.to_string()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LliBitsObject {
    loss_of_lock: u8,
    half_cycle: u8,
}

/// RINEX LLI bit constants used by the RTCM MSM LLI helpers.
#[wasm_bindgen(js_name = rtcmLliBits)]
pub fn rtcm_lli_bits() -> Result<JsValue, JsValue> {
    LliBitsObject {
        loss_of_lock: LLI_LOSS_OF_LOCK,
        half_cycle: LLI_HALF_CYCLE,
    }
    .serialize(&serializer())
    .map_err(|e| type_error(&e.to_string()))
}

fn optional_number(value: Option<u32>) -> JsValue {
    value
        .map(|v| JsValue::from_f64(f64::from(v)))
        .unwrap_or(JsValue::UNDEFINED)
}

fn optional_string(value: Option<&str>) -> JsValue {
    value.map(JsValue::from_str).unwrap_or(JsValue::UNDEFINED)
}

fn optional_u32_from_value(value: JsValue, name: &str) -> Result<Option<u32>, JsValue> {
    if value.is_null() || value.is_undefined() {
        Ok(None)
    } else {
        serde_wasm_bindgen::from_value(value)
            .map(Some)
            .map_err(|e| type_error(&format!("{name} must be an unsigned 32-bit integer: {e}")))
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviousLockInput {
    #[serde(default)]
    min_lock_time_ms: Option<u32>,
    elapsed_ms: u64,
}

impl From<PreviousLockInput> for PreviousLock {
    fn from(previous: PreviousLockInput) -> Self {
        Self {
            min_lock_time_ms: previous.min_lock_time_ms,
            elapsed_ms: previous.elapsed_ms,
        }
    }
}

fn optional_previous_lock(value: JsValue) -> Result<Option<PreviousLock>, JsValue> {
    if value.is_null() || value.is_undefined() {
        Ok(None)
    } else {
        serde_wasm_bindgen::from_value::<PreviousLockInput>(value)
            .map(|previous| Some(previous.into()))
            .map_err(|e| {
                type_error(&format!(
                    "previous must be null/undefined or {{ elapsedMs, minLockTimeMs? }}: {e}"
                ))
            })
    }
}

/// Minimum continuous-lock time encoded by an MSM lock-time indicator.
///
/// `kind` is `"msm1"` through `"msm7"`. Kinds without a lock-time field and
/// reserved or out-of-range indicators return `undefined`.
#[wasm_bindgen(js_name = rtcmMinimumLockTimeMs)]
pub fn rtcm_minimum_lock_time_ms(kind: &str, indicator: u16) -> Result<JsValue, JsValue> {
    Ok(optional_number(core_minimum_lock_time_ms(
        msm_kind_from_label(kind)?,
        indicator,
    )))
}

/// Derive a RINEX LLI value for one MSM signal cell.
///
/// `previous` is `null`/`undefined` or `{ elapsedMs, minLockTimeMs? }`.
/// `currentMinLockTimeMs` is a number or `null`/`undefined` for reserved current
/// indicators.
#[wasm_bindgen(js_name = rtcmDeriveLli)]
pub fn rtcm_derive_lli(
    previous: JsValue,
    current_min_lock_time_ms: JsValue,
    half_cycle_ambiguity: bool,
) -> Result<u8, JsValue> {
    Ok(core_derive_lli(
        optional_previous_lock(previous)?,
        optional_u32_from_value(current_min_lock_time_ms, "currentMinLockTimeMs")?,
        half_cycle_ambiguity,
    ))
}

/// Elapsed milliseconds between two raw MSM epoch-time fields.
#[wasm_bindgen(js_name = rtcmMsmEpochDtMs)]
pub fn rtcm_msm_epoch_dt_ms(
    system: &str,
    previous_epoch_time: u32,
    current_epoch_time: u32,
) -> Result<f64, JsValue> {
    Ok(core_msm_epoch_dt_ms(
        gnss_system_from_label(system)?,
        previous_epoch_time,
        current_epoch_time,
    ) as f64)
}

/// RINEX 3 observation-code suffix for an MSM signal id, or `undefined` for
/// reserved ids.
#[wasm_bindgen(js_name = rtcmMsmSignalRinexCode)]
pub fn rtcm_msm_signal_rinex_code(system: &str, signal_id: u8) -> Result<JsValue, JsValue> {
    Ok(optional_string(core_msm_signal_rinex_code(
        gnss_system_from_label(system)?,
        signal_id,
    )))
}

/// Stateful MSM lock-time tracker for deriving RINEX LLI continuity bits.
#[wasm_bindgen]
pub struct RtcmLockTimeTracker {
    inner: CoreLockTimeTracker,
}

impl Default for RtcmLockTimeTracker {
    fn default() -> Self {
        Self {
            inner: CoreLockTimeTracker::new(),
        }
    }
}

#[wasm_bindgen]
impl RtcmLockTimeTracker {
    /// Build an empty tracker.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop all per-cell lock history.
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    /// Derive LLI rows for one decoded MSM message object and advance state.
    pub fn observe(&mut self, message: JsValue) -> Result<JsValue, JsValue> {
        let message = message_from_value(message)?;
        let Message::Msm(msm) = message else {
            return Err(type_error(
                "RtcmLockTimeTracker.observe expects an MSM message",
            ));
        };
        let rows: Vec<CellLliObject> = self
            .inner
            .observe(&msm)
            .iter()
            .map(CellLliObject::from)
            .collect();
        rows.serialize(&serializer())
            .map_err(|e| type_error(&e.to_string()))
    }
}

/// Read the 12-bit RTCM message number from a message body.
///
/// `body` is the bytes between the frame length word and CRC. Delegates to
/// `sidereon_core::rtcm::message_number`.
#[wasm_bindgen(js_name = rtcmMessageNumber)]
pub fn rtcm_message_number(body: &[u8]) -> Result<u16, JsValue> {
    core_message_number(body).map_err(engine_error)
}

/// Decode a single RTCM 3 message body without the transport frame.
///
/// `body` is the bytes between the frame length word and CRC. Delegates to
/// `sidereon_core::rtcm::Message::decode`.
#[wasm_bindgen(js_name = decodeRtcmMessage, unchecked_return_type = "RtcmMessage")]
pub fn decode_rtcm_message(body: &[u8]) -> Result<JsValue, JsValue> {
    let message = Message::decode(body).map_err(engine_error)?;
    MessageObject::from(&message)
        .serialize(&serializer())
        .map_err(|e| type_error(&e.to_string()))
}

/// Decode the single RTCM 3 frame that begins at the start of `bytes`.
///
/// Returns the decoded message object, the total `frameLen` consumed
/// (preamble, length word, body, CRC), the six `reserved` header bits as read,
/// and the `departures` read under the lenient policy. Throws an `Error` if
/// the preamble is missing, the buffer is shorter than the declared frame, the
/// CRC does not match, or, under the strict policy (the default), the frame
/// departs from RTCM 3. Delegates to `sidereon_core::rtcm::decode_frame` +
/// `sidereon_core::rtcm::Message::decode_with_policy`.
#[wasm_bindgen(js_name = decodeRtcmFrame, unchecked_return_type = "RtcmFrame")]
pub fn decode_rtcm_frame(bytes: &[u8], policy: Option<String>) -> Result<JsValue, JsValue> {
    let policy = rtcm_policy(policy)?;
    let frame = core_decode_frame(bytes).map_err(engine_error)?;
    let mut departures = Vec::new();
    if frame.reserved != 0 {
        let departure = RtcmDeparture::FrameReservedBits {
            reserved: frame.reserved,
        };
        if policy == RtcmPolicy::Strict {
            return Err(engine_error(departure));
        }
        departures.push(DepartureObject::from(&departure));
    }
    let (message, body_departures) =
        Message::decode_with_policy(frame.body, policy).map_err(engine_error)?;
    departures.extend(body_departures.iter().map(DepartureObject::from));

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct FrameObject {
        message: MessageObject,
        // A frame is at most 1029 bytes, so `u32` keeps `frameLen` a plain JS
        // number under the large-number-as-bigint serializer (which the raw
        // 64-bit message fields need).
        frame_len: u32,
        reserved: u8,
        departures: Vec<DepartureObject>,
    }
    let object = FrameObject {
        message: MessageObject::from(&message),
        frame_len: frame.frame_len as u32,
        reserved: frame.reserved,
        departures,
    };
    object
        .serialize(&serializer())
        .map_err(|e| type_error(&e.to_string()))
}

/// A forgiving RTCM 3 stream scanner: slides over a byte buffer, resynchronizes
/// on the next `0xD3` preamble whenever the length overruns or the CRC fails, and
/// yields only frames whose CRC verifies, exactly as a receiver locks onto a
/// serial feed.
///
/// Wraps `sidereon_core::rtcm::FrameScanner`: construction runs the scan to
/// completion (the core iterator owns the scanning logic) and `next()` walks the
/// yielded frames.
#[wasm_bindgen]
pub struct FrameScanner {
    frames: Vec<OwnedFrame>,
    cursor: usize,
    resync_bytes: usize,
    crc_failures: usize,
}

struct OwnedFrame {
    body: Vec<u8>,
    frame_len: usize,
    reserved: u8,
}

#[wasm_bindgen]
impl FrameScanner {
    /// Begin scanning `bytes` from the start.
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8]) -> FrameScanner {
        let mut scanner = CoreFrameScanner::new(bytes);
        let frames = scanner
            .by_ref()
            .map(|frame| OwnedFrame {
                body: frame.body.to_vec(),
                frame_len: frame.frame_len,
                reserved: frame.reserved,
            })
            .collect();
        FrameScanner {
            frames,
            cursor: 0,
            resync_bytes: scanner.resync_bytes(),
            crc_failures: scanner.crc_failures(),
        }
    }

    /// Bytes the scan passed over that did not belong to a yielded frame.
    #[wasm_bindgen(getter, js_name = resyncBytes)]
    pub fn resync_bytes(&self) -> usize {
        self.resync_bytes
    }

    /// Preambles whose declared frame lay wholly within the buffer but failed
    /// its CRC-24Q.
    #[wasm_bindgen(getter, js_name = crcFailures)]
    pub fn crc_failures(&self) -> usize {
        self.crc_failures
    }

    /// The total number of CRC-valid frames the scan found.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> usize {
        self.frames.len()
    }

    /// The next CRC-valid frame as `{ body, frameLen, reserved }` (`body` a
    /// `Uint8Array`, the message body between the length word and the CRC;
    /// `reserved` the six header bits as read), or `undefined` when the scan is
    /// exhausted.
    // `next` is the idiomatic JS iterator-step name; this is a wasm-bindgen
    // export, not a Rust `Iterator` impl.
    #[allow(clippy::should_implement_trait)]
    #[wasm_bindgen(unchecked_return_type = "RtcmScannedFrame | undefined")]
    pub fn next(&mut self) -> JsValue {
        let Some(frame) = self.frames.get(self.cursor) else {
            return JsValue::UNDEFINED;
        };
        self.cursor += 1;
        let object = js_sys::Object::new();
        let body = js_sys::Uint8Array::from(frame.body.as_slice());
        let _ = js_sys::Reflect::set(&object, &JsValue::from_str("body"), &body);
        let _ = js_sys::Reflect::set(
            &object,
            &JsValue::from_str("frameLen"),
            &JsValue::from_f64(frame.frame_len as f64),
        );
        let _ = js_sys::Reflect::set(
            &object,
            &JsValue::from_str("reserved"),
            &JsValue::from_f64(f64::from(frame.reserved)),
        );
        object.into()
    }
}

// --- construction from JS objects (encode path) -----------------------------
//
// The reverse of the decode mirrors above: an idiomatic JS object tagged with
// the same `type` discriminant is deserialized into a mirror input struct that
// carries the raw transmitted field integers, rebuilt into the `sidereon_core`
// IR, and handed to `Message::encode` / `Message::to_frame`. Every supported
// message family (1005/1006, 1007/1008/1033, 1019, 1020, MSM4/7) can be built
// from scratch. The codec, framing, and per-type grammar still live entirely in
// the crate.

/// Read a GNSS system label: the lower-case form (`"gps"`) or the form the
/// decoder writes (`"GPS"`, `"GLONASS"`, `"Galileo"`, `"BeiDou"`, `"QZSS"`,
/// `"NavIC"`, `"SBAS"`), so a decoded message re-encodes as it was read.
fn gnss_system_from_label(label: &str) -> Result<GnssSystem, JsValue> {
    Ok(match label {
        "gps" | "GPS" => GnssSystem::Gps,
        "glonass" | "GLONASS" => GnssSystem::Glonass,
        "galileo" | "Galileo" => GnssSystem::Galileo,
        "beidou" | "BeiDou" => GnssSystem::BeiDou,
        "qzss" | "QZSS" => GnssSystem::Qzss,
        "navic" | "NavIC" => GnssSystem::Navic,
        "sbas" | "SBAS" => GnssSystem::Sbas,
        other => return Err(type_error(&format!("invalid GNSS system label {other:?}"))),
    })
}

fn msm_kind_from_label(label: &str) -> Result<MsmKind, JsValue> {
    Ok(match label {
        "msm1" => MsmKind::Msm1,
        "msm2" => MsmKind::Msm2,
        "msm3" => MsmKind::Msm3,
        "msm4" => MsmKind::Msm4,
        "msm5" => MsmKind::Msm5,
        "msm6" => MsmKind::Msm6,
        "msm7" => MsmKind::Msm7,
        other => {
            return Err(type_error(&format!(
                "invalid MSM kind label {other:?}: expected \"msm1\" through \"msm7\""
            )))
        }
    })
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct StationInput {
    message_number: u16,
    reference_station_id: u16,
    itrf_realization_year: u8,
    gps_indicator: bool,
    glonass_indicator: bool,
    galileo_indicator: bool,
    reference_station_indicator: bool,
    ecef_x: i64,
    single_receiver_oscillator: bool,
    reserved: bool,
    ecef_y: i64,
    quarter_cycle_indicator: u8,
    ecef_z: i64,
    #[serde(default)]
    antenna_height: Option<u16>,
    #[serde(default)]
    trailing_bits: Vec<bool>,
}

impl StationInput {
    fn to_core(&self) -> StationCoordinates {
        StationCoordinates {
            message_number: self.message_number,
            reference_station_id: self.reference_station_id,
            itrf_realization_year: self.itrf_realization_year,
            gps_indicator: self.gps_indicator,
            glonass_indicator: self.glonass_indicator,
            galileo_indicator: self.galileo_indicator,
            reference_station_indicator: self.reference_station_indicator,
            ecef_x: self.ecef_x,
            single_receiver_oscillator: self.single_receiver_oscillator,
            reserved: self.reserved,
            ecef_y: self.ecef_y,
            quarter_cycle_indicator: self.quarter_cycle_indicator,
            ecef_z: self.ecef_z,
            antenna_height: self.antenna_height,
            trailing_bits: self.trailing_bits.clone(),
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct AntennaInput {
    message_number: u16,
    reference_station_id: u16,
    antenna_descriptor: String,
    antenna_setup_id: u8,
    #[serde(default)]
    antenna_serial_number: Option<String>,
    #[serde(default)]
    receiver_type: Option<String>,
    #[serde(default)]
    receiver_firmware_version: Option<String>,
    #[serde(default)]
    receiver_serial_number: Option<String>,
    #[serde(default)]
    trailing_bits: Vec<bool>,
}

impl AntennaInput {
    fn to_core(&self) -> AntennaDescriptor {
        AntennaDescriptor {
            message_number: self.message_number,
            reference_station_id: self.reference_station_id,
            antenna_descriptor: self.antenna_descriptor.clone(),
            antenna_setup_id: self.antenna_setup_id,
            antenna_serial_number: self.antenna_serial_number.clone(),
            receiver_type: self.receiver_type.clone(),
            receiver_firmware_version: self.receiver_firmware_version.clone(),
            receiver_serial_number: self.receiver_serial_number.clone(),
            trailing_bits: self.trailing_bits.clone(),
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GpsEphemerisInput {
    satellite_id: u8,
    week_number: u16,
    sv_accuracy: u8,
    code_on_l2: u8,
    idot: i32,
    iode: u8,
    t_oc: u16,
    a_f2: i16,
    a_f1: i32,
    a_f0: i32,
    iodc: u16,
    c_rs: i32,
    delta_n: i32,
    m0: i64,
    c_uc: i32,
    eccentricity: u64,
    c_us: i32,
    sqrt_a: u64,
    t_oe: u16,
    c_ic: i32,
    omega0: i64,
    c_is: i32,
    i0: i64,
    c_rc: i32,
    omega: i64,
    omega_dot: i32,
    t_gd: i16,
    sv_health: u8,
    l2_p_data_flag: bool,
    fit_interval: bool,
    #[serde(default)]
    trailing_bits: Vec<bool>,
}

impl GpsEphemerisInput {
    fn to_core(&self) -> GpsEphemeris {
        GpsEphemeris {
            satellite_id: self.satellite_id,
            week_number: self.week_number,
            sv_accuracy: self.sv_accuracy,
            code_on_l2: self.code_on_l2,
            idot: self.idot,
            iode: self.iode,
            t_oc: self.t_oc,
            a_f2: self.a_f2,
            a_f1: self.a_f1,
            a_f0: self.a_f0,
            iodc: self.iodc,
            c_rs: self.c_rs,
            delta_n: self.delta_n,
            m0: self.m0,
            c_uc: self.c_uc,
            eccentricity: self.eccentricity,
            c_us: self.c_us,
            sqrt_a: self.sqrt_a,
            t_oe: self.t_oe,
            c_ic: self.c_ic,
            omega0: self.omega0,
            c_is: self.c_is,
            i0: self.i0,
            c_rc: self.c_rc,
            omega: self.omega,
            omega_dot: self.omega_dot,
            t_gd: self.t_gd,
            sv_health: self.sv_health,
            l2_p_data_flag: self.l2_p_data_flag,
            fit_interval: self.fit_interval,
            trailing_bits: self.trailing_bits.clone(),
        }
    }
}

macro_rules! rtcm_ephemeris_input {
    ($input:ident, $core:ident, { $($field:ident : $ty:ty),+ $(,)? }) => {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct $input {
            $($field: $ty,)+
            #[serde(default)]
            trailing_bits: Vec<bool>,
        }

        impl $input {
            fn to_core(&self) -> $core {
                $core {
                    $($field: self.$field,)+
                    trailing_bits: self.trailing_bits.clone(),
                }
            }
        }
    };
}

rtcm_ephemeris_input!(GalileoFnavEphemerisInput, GalileoFnavEphemeris, {
    satellite_id: u8,
    week_number: u16,
    iod_nav: u16,
    sisa: u8,
    idot: i32,
    t_oc: u16,
    a_f2: i16,
    a_f1: i32,
    a_f0: i64,
    c_rs: i32,
    delta_n: i32,
    m0: i64,
    c_uc: i32,
    eccentricity: u64,
    c_us: i32,
    sqrt_a: u64,
    t_oe: u16,
    c_ic: i32,
    omega0: i64,
    c_is: i32,
    i0: i64,
    c_rc: i32,
    omega: i64,
    omega_dot: i32,
    bgd_e5a_e1: i16,
    e5a_signal_health: u8,
    e5a_data_validity: bool,
    reserved: u8,
});

rtcm_ephemeris_input!(GalileoInavEphemerisInput, GalileoInavEphemeris, {
    satellite_id: u8,
    week_number: u16,
    iod_nav: u16,
    sisa_index: u8,
    idot: i32,
    t_oc: u16,
    a_f2: i16,
    a_f1: i32,
    a_f0: i64,
    c_rs: i32,
    delta_n: i32,
    m0: i64,
    c_uc: i32,
    eccentricity: u64,
    c_us: i32,
    sqrt_a: u64,
    t_oe: u16,
    c_ic: i32,
    omega0: i64,
    c_is: i32,
    i0: i64,
    c_rc: i32,
    omega: i64,
    omega_dot: i32,
    bgd_e5a_e1: i16,
    bgd_e5b_e1: i16,
    e5b_signal_health: u8,
    e5b_data_validity: bool,
    e1b_signal_health: u8,
    e1b_data_validity: bool,
    reserved: u8,
});

rtcm_ephemeris_input!(BeidouEphemerisInput, BeidouEphemeris, {
    satellite_id: u8,
    week_number: u16,
    sv_urai: u8,
    idot: i32,
    aode: u8,
    t_oc: u32,
    a_f2: i16,
    a_f1: i32,
    a_f0: i32,
    aodc: u8,
    c_rs: i32,
    delta_n: i32,
    m0: i64,
    c_uc: i32,
    eccentricity: u64,
    c_us: i32,
    sqrt_a: u64,
    t_oe: u32,
    c_ic: i32,
    omega0: i64,
    c_is: i32,
    i0: i64,
    c_rc: i32,
    omega: i64,
    omega_dot: i32,
    t_gd1: i16,
    t_gd2: i16,
    sv_health: bool,
});

rtcm_ephemeris_input!(NavicEphemerisInput, NavicEphemeris, {
    satellite_id: u8,
    week_number: u16,
    a_f0: i32,
    a_f1: i32,
    a_f2: i16,
    ura: u8,
    t_oc: u16,
    t_gd: i16,
    delta_n: i32,
    iodec: u8,
    reserved: u16,
    l5_flag: bool,
    s_flag: bool,
    c_uc: i32,
    c_us: i32,
    c_ic: i32,
    c_is: i32,
    c_rc: i32,
    c_rs: i32,
    idot: i32,
    m0: i64,
    t_oe: u16,
    eccentricity: u64,
    sqrt_a: u64,
    omega0: i64,
    omega: i64,
    omega_dot: i32,
    i0: i64,
    spare_df544: u8,
    spare_df545: u8,
});

rtcm_ephemeris_input!(QzssEphemerisInput, QzssEphemeris, {
    satellite_id: u8,
    t_oc: u16,
    a_f2: i16,
    a_f1: i32,
    a_f0: i32,
    iode: u8,
    c_rs: i32,
    delta_n: i32,
    m0: i64,
    c_uc: i32,
    eccentricity: u64,
    c_us: i32,
    sqrt_a: u64,
    t_oe: u16,
    c_ic: i32,
    omega0: i64,
    c_is: i32,
    i0: i64,
    c_rc: i32,
    omega: i64,
    omega_dot: i32,
    idot: i32,
    codes_on_l2: u8,
    week_number: u16,
    ura: u8,
    sv_health: u8,
    t_gd: i16,
    iodc: u16,
    fit_interval: bool,
});

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GlonassEphemerisInput {
    satellite_id: u8,
    frequency_channel: u8,
    almanac_health: bool,
    almanac_health_availability: bool,
    p1: u8,
    t_k: u16,
    b_n_msb: bool,
    p2: bool,
    t_b: u8,
    xn_dot: i32,
    xn: i32,
    xn_dot_dot: i8,
    yn_dot: i32,
    yn: i32,
    yn_dot_dot: i8,
    zn_dot: i32,
    zn: i32,
    zn_dot_dot: i8,
    p3: bool,
    gamma_n: i16,
    m_p: u8,
    m_l_n_third: bool,
    tau_n: i32,
    delta_tau_n: i8,
    e_n: u8,
    m_p4: bool,
    m_f_t: u8,
    m_n_t: u16,
    m_m: u8,
    additional_data_available: bool,
    n_a: u16,
    tau_c: i64,
    m_n4: u8,
    m_tau_gps: i32,
    m_l_n_fifth: bool,
    reserved: u8,
    #[serde(default)]
    negative_zero: u16,
    #[serde(default)]
    trailing_bits: Vec<bool>,
}

impl GlonassEphemerisInput {
    fn to_core(&self) -> GlonassEphemeris {
        GlonassEphemeris {
            satellite_id: self.satellite_id,
            frequency_channel: self.frequency_channel,
            almanac_health: self.almanac_health,
            almanac_health_availability: self.almanac_health_availability,
            p1: self.p1,
            t_k: self.t_k,
            b_n_msb: self.b_n_msb,
            p2: self.p2,
            t_b: self.t_b,
            xn_dot: self.xn_dot,
            xn: self.xn,
            xn_dot_dot: self.xn_dot_dot,
            yn_dot: self.yn_dot,
            yn: self.yn,
            yn_dot_dot: self.yn_dot_dot,
            zn_dot: self.zn_dot,
            zn: self.zn,
            zn_dot_dot: self.zn_dot_dot,
            p3: self.p3,
            gamma_n: self.gamma_n,
            m_p: self.m_p,
            m_l_n_third: self.m_l_n_third,
            tau_n: self.tau_n,
            delta_tau_n: self.delta_tau_n,
            e_n: self.e_n,
            m_p4: self.m_p4,
            m_f_t: self.m_f_t,
            m_n_t: self.m_n_t,
            m_m: self.m_m,
            additional_data_available: self.additional_data_available,
            n_a: self.n_a,
            tau_c: self.tau_c,
            m_n4: self.m_n4,
            m_tau_gps: self.m_tau_gps,
            m_l_n_fifth: self.m_l_n_fifth,
            reserved: self.reserved,
            negative_zero: self.negative_zero,
            trailing_bits: self.trailing_bits.clone(),
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct MsmHeaderInput {
    reference_station_id: u16,
    epoch_time: u32,
    multiple_message: bool,
    iods: u8,
    reserved: u8,
    clock_steering: u8,
    external_clock: u8,
    divergence_free_smoothing: bool,
    smoothing_interval: u8,
}

impl MsmHeaderInput {
    fn to_core(&self) -> MsmHeader {
        MsmHeader {
            reference_station_id: self.reference_station_id,
            epoch_time: self.epoch_time,
            multiple_message: self.multiple_message,
            iods: self.iods,
            reserved: self.reserved,
            clock_steering: self.clock_steering,
            external_clock: self.external_clock,
            divergence_free_smoothing: self.divergence_free_smoothing,
            smoothing_interval: self.smoothing_interval,
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct MsmSatelliteInput {
    id: u8,
    #[serde(default)]
    rough_range_ms: Option<u8>,
    rough_range_mod1: u16,
    #[serde(default)]
    extended_info: Option<u8>,
    #[serde(default)]
    rough_phase_range_rate_m_s: Option<i16>,
}

impl MsmSatelliteInput {
    fn to_core(&self) -> MsmSatellite {
        MsmSatellite {
            id: self.id,
            rough_range_ms: self.rough_range_ms,
            rough_range_mod1: self.rough_range_mod1,
            extended_info: self.extended_info,
            rough_phase_range_rate_m_s: self.rough_phase_range_rate_m_s,
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct MsmSignalInput {
    satellite_id: u8,
    signal_id: u8,
    #[serde(default)]
    fine_pseudorange: Option<i32>,
    #[serde(default)]
    fine_phase_range: Option<i32>,
    #[serde(default)]
    lock_time_indicator: Option<u16>,
    #[serde(default)]
    half_cycle_ambiguity: Option<bool>,
    #[serde(default)]
    cnr: Option<u16>,
    #[serde(default)]
    fine_phase_range_rate: Option<i16>,
}

impl MsmSignalInput {
    fn to_core(&self) -> MsmSignal {
        MsmSignal {
            satellite_id: self.satellite_id,
            signal_id: self.signal_id,
            fine_pseudorange: self.fine_pseudorange,
            fine_phase_range: self.fine_phase_range,
            lock_time_indicator: self.lock_time_indicator,
            half_cycle_ambiguity: self.half_cycle_ambiguity,
            cnr: self.cnr,
            fine_phase_range_rate: self.fine_phase_range_rate,
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct MsmInput {
    message_number: u16,
    system: String,
    kind: String,
    header: MsmHeaderInput,
    #[serde(default)]
    signal_mask: Option<u32>,
    satellites: Vec<MsmSatelliteInput>,
    signals: Vec<MsmSignalInput>,
    #[serde(default)]
    trailing_bits: Vec<bool>,
}

impl MsmInput {
    fn to_core(&self) -> Result<MsmMessage, JsValue> {
        let signals: Vec<MsmSignal> = self.signals.iter().map(MsmSignalInput::to_core).collect();
        Ok(MsmMessage {
            message_number: self.message_number,
            system: gnss_system_from_label(&self.system)?,
            kind: msm_kind_from_label(&self.kind)?,
            header: self.header.to_core(),
            // An absent mask is built from the signal list, one bit per listed
            // signal id; a decoded message carries the mask as transmitted.
            signal_mask: self
                .signal_mask
                .unwrap_or_else(|| core_msm_signal_mask(&signals)),
            satellites: self
                .satellites
                .iter()
                .map(MsmSatelliteInput::to_core)
                .collect(),
            signals,
            trailing_bits: self.trailing_bits.clone(),
        })
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct UnsupportedInput {
    message_number: u16,
    body: Vec<u8>,
}

impl UnsupportedInput {
    fn to_core(&self) -> UnsupportedMessage {
        UnsupportedMessage {
            message_number: self.message_number,
            body: self.body.clone(),
        }
    }
}

fn de<T: serde::de::DeserializeOwned>(value: JsValue) -> Result<T, JsValue> {
    serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid RTCM message: {e}")))
}

fn ssr_kind_from_label(label: &str) -> Result<SsrKind, JsValue> {
    Ok(match label {
        "orbit" => SsrKind::Orbit,
        "clock" => SsrKind::Clock,
        "combinedOrbitClock" => SsrKind::CombinedOrbitClock,
        "codeBias" => SsrKind::CodeBias,
        "phaseBias" => SsrKind::PhaseBias,
        "ura" => SsrKind::Ura,
        "highRateClock" => SsrKind::HighRateClock,
        other => {
            return Err(type_error(&format!(
                "invalid SSR kind label {other:?}: expected \"orbit\", \"clock\", \
                 \"combinedOrbitClock\", \"codeBias\", \"phaseBias\", \"ura\", \
                 \"highRateClock\" or \"vtec\""
            )))
        }
    })
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SsrHeaderInput {
    epoch_time_s: u32,
    update_interval: u8,
    multiple_message: bool,
    iod_ssr: u8,
    provider_id: u16,
    solution_id: u8,
    #[serde(default)]
    satellite_reference_datum: Option<bool>,
    #[serde(default)]
    dispersive_bias_consistency: Option<bool>,
    #[serde(default)]
    mw_consistency: Option<bool>,
    satellite_count: u8,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SsrOrbitInput {
    satellite_id: u8,
    iode: u32,
    #[serde(default)]
    iod_crc: Option<u32>,
    delta_radial: i32,
    delta_along: i32,
    delta_cross: i32,
    dot_delta_radial: i32,
    dot_delta_along: i32,
    dot_delta_cross: i32,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SsrClockInput {
    satellite_id: u8,
    c0: i32,
    c1: i32,
    c2: i32,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SsrCodeBiasInput {
    satellite_id: u8,
    biases: Vec<(u8, i16)>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SsrPhaseBiasSignalInput {
    signal_id: u8,
    integer_indicator: u8,
    wide_lane_integer_indicator: u8,
    discontinuity_counter: u8,
    bias: i32,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SsrPhaseBiasInput {
    satellite_id: u8,
    yaw_angle: u16,
    yaw_rate: i8,
    biases: Vec<SsrPhaseBiasSignalInput>,
}

/// An RTCM SSR message to encode: the raw transmitted fields the decoder
/// returns, so a decoded SSR message re-encodes as it was read.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SsrInput {
    message_number: u16,
    #[serde(default)]
    igs_ssr_version: Option<u8>,
    system: String,
    kind: String,
    header: SsrHeaderInput,
    #[serde(default)]
    orbit: Vec<SsrOrbitInput>,
    #[serde(default)]
    clock: Vec<SsrClockInput>,
    #[serde(default)]
    code_bias: Vec<SsrCodeBiasInput>,
    #[serde(default)]
    phase_bias: Vec<SsrPhaseBiasInput>,
    #[serde(default)]
    ura: Vec<(u8, u8)>,
    #[serde(default)]
    padding_bits: Vec<bool>,
}

impl SsrInput {
    fn into_core(self) -> Result<SsrMessage, JsValue> {
        let h = self.header;
        Ok(SsrMessage {
            message_number: self.message_number,
            igs_ssr_version: self.igs_ssr_version,
            system: gnss_system_from_label(&self.system)?,
            kind: ssr_kind_from_label(&self.kind)?,
            header: SsrHeader {
                epoch_time_s: h.epoch_time_s,
                update_interval: h.update_interval,
                multiple_message: h.multiple_message,
                iod_ssr: h.iod_ssr,
                provider_id: h.provider_id,
                solution_id: h.solution_id,
                satellite_reference_datum: h.satellite_reference_datum,
                dispersive_bias_consistency: h.dispersive_bias_consistency,
                mw_consistency: h.mw_consistency,
                satellite_count: h.satellite_count,
            },
            orbit: self
                .orbit
                .into_iter()
                .map(|r| SsrOrbitRecord {
                    satellite_id: r.satellite_id,
                    iode: r.iode,
                    iod_crc: r.iod_crc,
                    delta_radial: r.delta_radial,
                    delta_along: r.delta_along,
                    delta_cross: r.delta_cross,
                    dot_delta_radial: r.dot_delta_radial,
                    dot_delta_along: r.dot_delta_along,
                    dot_delta_cross: r.dot_delta_cross,
                })
                .collect(),
            clock: self
                .clock
                .into_iter()
                .map(|r| SsrClockRecord {
                    satellite_id: r.satellite_id,
                    c0: r.c0,
                    c1: r.c1,
                    c2: r.c2,
                })
                .collect(),
            code_bias: self
                .code_bias
                .into_iter()
                .map(|r| SsrCodeBiasRecord {
                    satellite_id: r.satellite_id,
                    biases: r.biases,
                })
                .collect(),
            phase_bias: self
                .phase_bias
                .into_iter()
                .map(|r| SsrPhaseBiasRecord {
                    satellite_id: r.satellite_id,
                    yaw_angle: r.yaw_angle,
                    yaw_rate: r.yaw_rate,
                    biases: r
                        .biases
                        .into_iter()
                        .map(|b| SsrPhaseBiasSignal {
                            signal_id: b.signal_id,
                            integer_indicator: b.integer_indicator,
                            wide_lane_integer_indicator: b.wide_lane_integer_indicator,
                            discontinuity_counter: b.discontinuity_counter,
                            bias: b.bias,
                        })
                        .collect(),
                })
                .collect(),
            ura: self.ura,
            padding_bits: self.padding_bits,
        })
    }
}

/// Build the core `Message` IR from a `type`-tagged JS object.
fn message_from_value(message: JsValue) -> Result<Message, JsValue> {
    let tag = js_sys::Reflect::get(&message, &JsValue::from_str("type"))
        .ok()
        .and_then(|v| v.as_string())
        .ok_or_else(|| type_error("RTCM message object must carry a string `type` discriminant"))?;
    let built = match tag.as_str() {
        "legacyObservations" => {
            Message::LegacyObservations(de::<LegacyObservationsValue>(message)?.into())
        }
        "networkAuxiliaryStation" => {
            Message::NetworkAuxiliaryStation(de::<NetworkAuxiliaryStationValue>(message)?.into())
        }
        "networkCorrectionDifferences" => Message::NetworkCorrectionDifferences(
            de::<NetworkCorrectionDifferencesValue>(message)?.into(),
        ),
        "networkResiduals" => {
            Message::NetworkResiduals(de::<NetworkResidualsValue>(message)?.into())
        }
        "physicalReferenceStation" => {
            Message::PhysicalReferenceStation(de::<PhysicalReferenceStationValue>(message)?.into())
        }
        "fkpGradients" => Message::FkpGradients(de::<FkpGradientsValue>(message)?.into()),
        "stationCoordinates" => Message::StationCoordinates(de::<StationInput>(message)?.to_core()),
        "antennaDescriptor" => Message::AntennaDescriptor(de::<AntennaInput>(message)?.to_core()),
        "systemParameters" => {
            Message::SystemParameters(de::<SystemParametersValue>(message)?.into())
        }
        "text" => Message::Text(de::<TextMessageValue>(message)?.into()),
        "glonassCodePhaseBiases" => {
            Message::GlonassCodePhaseBiases(de::<GlonassCodePhaseBiasesValue>(message)?.into())
        }
        "gpsEphemeris" => Message::GpsEphemeris(de::<GpsEphemerisInput>(message)?.to_core()),
        "glonassEphemeris" => {
            Message::GlonassEphemeris(de::<GlonassEphemerisInput>(message)?.to_core())
        }
        "beidouEphemeris" => {
            Message::BeidouEphemeris(de::<BeidouEphemerisInput>(message)?.to_core())
        }
        "navicEphemeris" => Message::NavicEphemeris(de::<NavicEphemerisInput>(message)?.to_core()),
        "qzssEphemeris" => Message::QzssEphemeris(de::<QzssEphemerisInput>(message)?.to_core()),
        "galileoFnavEphemeris" => {
            Message::GalileoFnavEphemeris(de::<GalileoFnavEphemerisInput>(message)?.to_core())
        }
        "galileoInavEphemeris" => {
            Message::GalileoInavEphemeris(de::<GalileoInavEphemerisInput>(message)?.to_core())
        }
        "msm" => Message::Msm(de::<MsmInput>(message)?.to_core()?),
        "unsupported" => Message::Unsupported(de::<UnsupportedInput>(message)?.to_core()),
        "ssr" => Message::Ssr(de::<SsrInput>(message)?.into_core()?),
        "ssrVtec" => Message::SsrVtec(de::<SsrVtecValue>(message)?.into()),
        "helmertTransformation" => {
            Message::HelmertTransformation(de::<HelmertValue>(message)?.into())
        }
        "residualGrid" => Message::ResidualGrid(de::<ResidualGridValue>(message)?.into()),
        "projection" => Message::Projection(de::<ProjectionValue>(message)?.into()),
        other => return Err(type_error(&format!("unknown RTCM message type {other:?}"))),
    };
    Ok(built)
}

/// Why the encoder refused a message, as the `detail` of the thrown
/// `RtcmEncodeError`.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind")]
enum RtcmEncodeErrorDetailJs {
    /// An engine input refusal outside the codec's typed errors, with its
    /// reason. Wire-layout refusals (an MSM list its masks cannot hold, an
    /// ephemeris satellite id wider than its field) are `RTCM_ENCODE`.
    #[serde(rename = "INVALID_INPUT", rename_all = "camelCase")]
    InvalidInput { reason: String, message: String },
    /// A typed core codec refusal, retaining its concrete variant and fields.
    #[serde(rename = "RTCM_ENCODE", rename_all = "camelCase")]
    CoreEncode {
        core: serde_json::Value,
        message: String,
    },
    /// A typed core conversion refusal, retaining its concrete variant and fields.
    #[serde(rename = "RTCM_CONVERSION", rename_all = "camelCase")]
    CoreConversion {
        core: serde_json::Value,
        message: String,
    },
    /// An engine error outside the RTCM codec's typed encode and conversion
    /// errors, with its message in full.
    #[serde(rename = "UNKNOWN", rename_all = "camelCase")]
    Unknown { message: String },
}

fn rtcm_encode_error(err: sidereon_core::Error) -> JsValue {
    let message = err.to_string();
    let detail = match err {
        sidereon_core::Error::InvalidInput(reason) => RtcmEncodeErrorDetailJs::InvalidInput {
            reason,
            message: message.clone(),
        },
        sidereon_core::Error::RtcmEncode(error) => RtcmEncodeErrorDetailJs::CoreEncode {
            core: rtcm_encode_error_payload(&error),
            message: message.clone(),
        },
        sidereon_core::Error::RtcmConversion(error) => RtcmEncodeErrorDetailJs::CoreConversion {
            core: rtcm_conversion_error_payload(&error),
            message: message.clone(),
        },
        _ => RtcmEncodeErrorDetailJs::Unknown {
            message: message.clone(),
        },
    };
    error_with_detail("RtcmEncodeError", &message, &detail)
}

fn record_kind_value(record: sidereon_core::rtcm::RtcmRecordKind) -> serde_json::Value {
    use sidereon_core::rtcm::RtcmRecordKind as Record;
    match record {
        Record::StationCoordinates => serde_json::json!({"kind":"stationCoordinates"}),
        Record::AntennaDescriptor => serde_json::json!({"kind":"antennaDescriptor"}),
        Record::Msm { system, kind } => {
            serde_json::json!({"kind":"msm","system":system.as_str(),"messageKind":msm_kind_label(kind)})
        }
        Record::Ssr { system, kind } => {
            serde_json::json!({"kind":"ssr","system":system.as_str(),"messageKind":ssr_kind_label(kind)})
        }
        Record::LegacyObservations => serde_json::json!({"kind":"legacyObservations"}),
        Record::SystemParameters => serde_json::json!({"kind":"systemParameters"}),
        Record::Text => serde_json::json!({"kind":"text"}),
        Record::Network { family } => serde_json::json!({"kind":"network","family":family}),
        Record::Transformation { family } => {
            serde_json::json!({"kind":"transformation","family":family})
        }
        Record::GlonassCodePhaseBiases => serde_json::json!({"kind":"glonassCodePhaseBiases"}),
        Record::SsrVtec { message_number } => {
            serde_json::json!({"kind":"ssrVtec","messageNumber":message_number})
        }
        _ => serde_json::json!({"kind":"unrecognizedRecord"}),
    }
}

fn rtcm_encode_error_payload(error: &sidereon_core::rtcm::RtcmEncodeError) -> serde_json::Value {
    use sidereon_core::rtcm::{
        MsmMaskProblem, MsmOptionalField, MsmOptionalProblem, RtcmEncodeError as Encode,
    };
    match error {
        Encode::FieldOutOfRange {
            message_number,
            field,
            value,
            width,
            encoding,
        } => {
            serde_json::json!({"kind":"fieldOutOfRange","messageNumber":message_number,"field":field,"value":value.to_string(),"width":width,"encoding":match encoding { sidereon_core::rtcm::RtcmFieldEncoding::Unsigned => "unsigned", sidereon_core::rtcm::RtcmFieldEncoding::TwosComplement => "twosComplement", sidereon_core::rtcm::RtcmFieldEncoding::SignMagnitude => "signMagnitude", _ => "unrecognized" }})
        }
        Encode::NegativeZeroWithValue {
            message_number,
            field,
            value,
        } => {
            serde_json::json!({"kind":"negativeZeroWithValue","messageNumber":message_number,"field":field,"value":value})
        }
        Encode::NegativeZeroMask {
            message_number,
            mask,
        } => {
            serde_json::json!({"kind":"negativeZeroMask","messageNumber":message_number,"mask":mask})
        }
        Encode::MessageNumber {
            message_number,
            record,
        } => {
            serde_json::json!({"kind":"messageNumber","messageNumber":message_number,"record":record_kind_value(*record)})
        }
        Encode::FieldPresence {
            message_number,
            record,
            field,
            carried,
        } => {
            serde_json::json!({"kind":"fieldPresence","messageNumber":message_number,"record":record_kind_value(*record),"field":field,"carried":carried})
        }
        Encode::SatelliteFieldPresence {
            message_number,
            record,
            satellite,
            field,
            carried,
        } => {
            serde_json::json!({"kind":"satelliteFieldPresence","messageNumber":message_number,"record":record_kind_value(*record),"satellite":satellite,"field":field,"carried":carried})
        }
        Encode::CountMismatch {
            message_number,
            field,
            expected,
            actual,
        } => {
            serde_json::json!({"kind":"countMismatch","messageNumber":message_number,"field":field,"expected":expected,"actual":actual})
        }
        Encode::ValueOutOfRange {
            message_number,
            field,
            value,
            minimum,
            maximum,
        } => {
            serde_json::json!({"kind":"valueOutOfRange","messageNumber":message_number,"field":field,"value":value.to_string(),"minimum":minimum.to_string(),"maximum":maximum.to_string()})
        }
        Encode::NonLatin1Character { field, character } => {
            serde_json::json!({"kind":"nonLatin1Character","field":field,"character":character.to_string()})
        }
        Encode::SatelliteIdOutOfRange {
            message_number,
            field,
            value,
            width,
        } => {
            serde_json::json!({"kind":"satelliteIdOutOfRange","messageNumber":message_number,"field":field,"value":value,"width":width})
        }
        Encode::SsrSatelliteIdOutOfRange {
            message_number,
            value,
            width,
        } => {
            serde_json::json!({"kind":"ssrSatelliteIdOutOfRange","messageNumber":message_number,"value":value,"width":width})
        }
        Encode::SsrRecordsNotCarried {
            message_number,
            kind,
            records,
            count,
        } => {
            serde_json::json!({"kind":"ssrRecordsNotCarried","messageNumber":message_number,"ssrKind":ssr_kind_label(*kind),"records":records,"count":count})
        }
        Encode::SsrCombinedRecordCounts {
            message_number,
            orbit,
            clock,
        } => {
            serde_json::json!({"kind":"ssrCombinedRecordCounts","messageNumber":message_number,"orbit":orbit,"clock":clock})
        }
        Encode::SsrCombinedSatelliteMismatch {
            message_number,
            index,
            orbit_satellite,
            clock_satellite,
        } => {
            serde_json::json!({"kind":"ssrCombinedSatelliteMismatch","messageNumber":message_number,"index":index,"orbitSatellite":orbit_satellite,"clockSatellite":clock_satellite})
        }
        Encode::SsrHighRateClockTerms {
            message_number,
            satellite,
            c1,
            c2,
        } => {
            serde_json::json!({"kind":"ssrHighRateClockTerms","messageNumber":message_number,"satellite":satellite,"c1":c1,"c2":c2})
        }
        Encode::SsrSatelliteCount {
            message_number,
            declared,
            records,
        } => {
            serde_json::json!({"kind":"ssrSatelliteCount","messageNumber":message_number,"declared":declared,"records":records})
        }
        Encode::MsmMask {
            message_number,
            problem,
        } => {
            let detail = match problem {
                MsmMaskProblem::SatelliteOutsideMask { satellite } => {
                    serde_json::json!({"kind":"satelliteOutsideMask","satellite":satellite})
                }
                MsmMaskProblem::SatelliteListedTwice { satellite } => {
                    serde_json::json!({"kind":"satelliteListedTwice","satellite":satellite})
                }
                MsmMaskProblem::SignalOutsideMask { signal } => {
                    serde_json::json!({"kind":"signalOutsideMask","signal":signal})
                }
                MsmMaskProblem::SignalNotInMask { signal, mask } => {
                    serde_json::json!({"kind":"signalNotInMask","signal":signal,"mask":mask})
                }
                MsmMaskProblem::SignalSatelliteNotListed { signal, satellite } => {
                    serde_json::json!({"kind":"signalSatelliteNotListed","signal":signal,"satellite":satellite})
                }
                MsmMaskProblem::CellListedTwice { satellite, signal } => {
                    serde_json::json!({"kind":"cellListedTwice","satellite":satellite,"signal":signal})
                }
                _ => serde_json::json!({"kind":"unrecognizedMsmMaskProblem"}),
            };
            serde_json::json!({"kind":"msmMask","messageNumber":message_number,"problem":detail})
        }
        Encode::MsmOptional {
            message_number,
            kind,
            satellite,
            signal,
            field,
            problem,
        } => {
            let field = match field {
                MsmOptionalField::ExtendedInfo => "extendedInfo",
                MsmOptionalField::RoughPhaseRangeRate => "roughPhaseRangeRate",
                MsmOptionalField::FinePhaseRangeRate => "finePhaseRangeRate",
                _ => "unrecognized",
            };
            let problem = match problem {
                MsmOptionalProblem::Missing => serde_json::json!({"kind":"missing"}),
                MsmOptionalProblem::NotCarried => serde_json::json!({"kind":"notCarried"}),
                MsmOptionalProblem::InvalidValue(value) => {
                    serde_json::json!({"kind":"invalidValue","value":value})
                }
                _ => serde_json::json!({"kind":"unrecognized"}),
            };
            serde_json::json!({"kind":"msmOptional","messageNumber":message_number,"messageKind":msm_kind_label(*kind),"satellite":satellite,"signal":signal,"field":field,"problem":problem})
        }
        Encode::TrailingZeroBits {
            message_number,
            bits,
        } => {
            serde_json::json!({"kind":"trailingZeroBits","messageNumber":message_number,"bits":bits})
        }
        Encode::StrictDeparture(departure) => {
            serde_json::json!({"kind":"strictDeparture","departure":DepartureObject::from(departure)})
        }
        Encode::UnsupportedBodyTooShort { message_number } => {
            serde_json::json!({"kind":"unsupportedBodyTooShort","messageNumber":message_number})
        }
        Encode::UnsupportedBodyNumber {
            message_number,
            carried,
        } => {
            serde_json::json!({"kind":"unsupportedBodyNumber","messageNumber":message_number,"carried":carried})
        }
        Encode::UnsupportedDecodedNumber { message_number } => {
            serde_json::json!({"kind":"unsupportedDecodedNumber","messageNumber":message_number})
        }
        Encode::FrameBodyTooLong { len } => {
            serde_json::json!({"kind":"frameBodyTooLong","len":len})
        }
        Encode::FrameReservedOutOfRange { value } => {
            serde_json::json!({"kind":"frameReservedOutOfRange","value":value})
        }
        _ => serde_json::json!({"kind":"unrecognizedRtcmEncodeError"}),
    }
}

fn rtcm_conversion_error_payload(
    error: &sidereon_core::rtcm::RtcmConversionError,
) -> serde_json::Value {
    use sidereon_core::rtcm::RtcmConversionError as Convert;
    match error {
        Convert::SatelliteIdOutOfRange {
            message_number,
            field,
            value,
            width,
        } => {
            serde_json::json!({"kind":"satelliteIdOutOfRange","messageNumber":message_number,"field":field,"value":value,"width":width})
        }
        Convert::InvalidSatellite {
            message_number,
            field,
            value,
            error,
        } => {
            let error = match error {
                sidereon_core::SatelliteIdError::InvalidInput { field, reason } => {
                    serde_json::json!({"kind":"invalidInput","field":field,"reason":reason})
                }
            };
            serde_json::json!({"kind":"invalidSatellite","messageNumber":message_number,"field":field,"value":value,"error":error})
        }
        Convert::SbasPrnOutsideWindow {
            value,
            broadcast_prn,
        } => {
            serde_json::json!({"kind":"sbasPrnOutsideWindow","value":value,"broadcastPrn":broadcast_prn})
        }
        Convert::NoLnavRecord { value, satellite } => {
            serde_json::json!({"kind":"noLnavRecord","value":value,"satellite":satellite.to_string()})
        }
        Convert::WeekMismatch {
            message_number,
            full_week,
            week,
        } => {
            serde_json::json!({"kind":"weekMismatch","messageNumber":message_number,"fullWeek":full_week,"week":week})
        }
        Convert::NavicWeekMismatch { full_week, week } => {
            serde_json::json!({"kind":"navicWeekMismatch","fullWeek":full_week,"week":week})
        }
        Convert::TimeNotRepresentable { field } => {
            serde_json::json!({"kind":"timeNotRepresentable","field":field})
        }
        Convert::GalileoWeekOverflow => serde_json::json!({"kind":"galileoWeekOverflow"}),
        Convert::SisaSpare { index } => serde_json::json!({"kind":"sisaSpare","index":index}),
        Convert::SisaNoPrediction => serde_json::json!({"kind":"sisaNoPrediction"}),
        Convert::UraOutOfRange { system, index } => {
            serde_json::json!({"kind":"uraOutOfRange","system":system.as_str(),"index":index})
        }
        Convert::UraNoPrediction { system, index } => {
            serde_json::json!({"kind":"uraNoPrediction","system":system.as_str(),"index":index})
        }
        Convert::FitInterval(error) => {
            use sidereon_core::ephemeris::LnavRecordError;
            let detail = match error {
                LnavRecordError::NotGps(satellite) => {
                    serde_json::json!({"kind":"notGps","satellite":satellite.to_string()})
                }
                LnavRecordError::InvalidEpoch(field) => {
                    serde_json::json!({"kind":"invalidEpoch","field":field})
                }
                LnavRecordError::WeekMismatch {
                    full_week,
                    decoded_week,
                } => {
                    serde_json::json!({"kind":"weekMismatch","fullWeek":full_week,"decodedWeek":decoded_week})
                }
                LnavRecordError::NoUraPrediction(index) => {
                    serde_json::json!({"kind":"noUraPrediction","index":index})
                }
                LnavRecordError::FitIntervalUnsupported {
                    fit_interval_flag,
                    iode,
                    iodc,
                } => {
                    serde_json::json!({"kind":"fitIntervalUnsupported","fitIntervalFlag":fit_interval_flag,"iode":iode,"iodc":iodc})
                }
            };
            serde_json::json!({"kind":"fitInterval","error":detail})
        }
        Convert::VtecEvaluation(problem) => {
            serde_json::json!({"kind":"vtecEvaluation","problem":vtec_problem_payload(problem)})
        }
        _ => serde_json::json!({"kind":"unrecognizedRtcmConversionError"}),
    }
}

fn vtec_problem_payload(problem: &sidereon_core::rtcm::VtecEvaluationProblem) -> serde_json::Value {
    use sidereon_core::rtcm::VtecEvaluationProblem as Problem;
    match problem {
        Problem::ComputationTime => serde_json::json!({"kind":"computationTime"}),
        Problem::Frequency => serde_json::json!({"kind":"frequency"}),
        Problem::NonFiniteCoordinates => serde_json::json!({"kind":"nonFiniteCoordinates"}),
        Problem::MessageIdentity { message_number } => {
            serde_json::json!({"kind":"messageIdentity","messageNumber":message_number})
        }
        Problem::LayerCount { layers } => serde_json::json!({"kind":"layerCount","layers":layers}),
        Problem::InvalidGeometry => serde_json::json!({"kind":"invalidGeometry"}),
        Problem::BelowHorizon => serde_json::json!({"kind":"belowHorizon"}),
        Problem::LayerDegreeOrder {
            layer_index,
            degree,
            order,
        } => {
            serde_json::json!({"kind":"layerDegreeOrder","layerIndex":layer_index,"degree":degree,"order":order})
        }
        Problem::CoefficientCounts {
            layer_index,
            cosine_expected,
            cosine_actual,
            sine_expected,
            sine_actual,
        } => {
            serde_json::json!({"kind":"coefficientCounts","layerIndex":layer_index,"cosineExpected":cosine_expected,"cosineActual":cosine_actual,"sineExpected":sine_expected,"sineActual":sine_actual})
        }
        Problem::UnavailableCoefficient { layer_index } => {
            serde_json::json!({"kind":"unavailableCoefficient","layerIndex":layer_index})
        }
        Problem::ShellNotAboveReceiver { layer_index } => {
            serde_json::json!({"kind":"shellNotAboveReceiver","layerIndex":layer_index})
        }
        Problem::MissingCoefficient {
            layer_index,
            field,
            index,
        } => {
            serde_json::json!({"kind":"missingCoefficient","layerIndex":layer_index,"field":field,"index":index})
        }
        Problem::InvalidMappingFactor { layer_index } => {
            serde_json::json!({"kind":"invalidMappingFactor","layerIndex":layer_index})
        }
        Problem::PhysicalResultOutOfRange { field } => {
            serde_json::json!({"kind":"physicalResultOutOfRange","field":field})
        }
        _ => serde_json::json!({"kind":"unrecognizedVtecProblem"}),
    }
}

/// Encode a constructed RTCM message into a message body (without the transport
/// frame).
///
/// `message` is a `type`-tagged plain object of the same shape [`decodeRtcm`]
/// returns (`"stationCoordinates"`, `"antennaDescriptor"`, `"gpsEphemeris"`,
/// `"glonassEphemeris"`, `"beidouEphemeris"`, `"qzssEphemeris"`,
/// `"galileoFnavEphemeris"`, `"galileoInavEphemeris"`, `"msm"`,
/// `"unsupported"`), carrying the raw transmitted field integers (large fields
/// as `bigint`). Returns the encoded body as a `Uint8Array`. Delegates to
/// `sidereon_core::rtcm::Message::encode`. Throws a `TypeError` for a malformed
/// object or unknown type.
///
/// A message whose fields the wire layout cannot state is refused, never
/// written as a different mask or satellite: an MSM satellite id outside
/// `1..=64`, a signal id outside `1..=32`, a satellite or satellite/signal
/// cell listed twice, a signal whose satellite is not listed, or an ephemeris
/// satellite id wider than the message's field (four bits for QZSS 1044, six
/// otherwise). The refusal is an `Error` named `RtcmEncodeError` whose `detail`
/// is an `RtcmEncodeErrorDetail`.
///
/// `policy` is `"strict"` (the default), which refuses a nonempty
/// `trailingBits` by name, or `"lenient"`, which writes the trailing bits back
/// after the last field so a body decoded under the lenient policy
/// re-encodes byte for byte.
#[wasm_bindgen(js_name = encodeRtcm)]
pub fn encode_rtcm(
    #[wasm_bindgen(unchecked_param_type = "RtcmMessageInput")] message: JsValue,
    policy: Option<String>,
) -> Result<Vec<u8>, JsValue> {
    let policy = rtcm_policy(policy)?;
    message_from_value(message)?
        .encode_with_policy(policy)
        .map(|(body, _)| body)
        .map_err(rtcm_encode_error)
}

/// Encode a constructed RTCM message and wrap it in a fresh RTCM transport frame
/// (preamble, length word, body, CRC).
///
/// `message` has the same shape [`encodeRtcm`] takes. Returns the framed bytes as
/// a `Uint8Array`, ready to feed back to [`decodeRtcmFrame`] / [`decodeRtcm`].
/// Delegates to `sidereon_core::rtcm::Message::encode_with_policy` and
/// `encode_frame_with_reserved`. `policy` is as for [`encodeRtcm`];
/// `reserved` (default 0) is the six reserved header bits, so a frame read
/// with nonzero reserved bits is written back as read. Throws a `TypeError`
/// for a malformed object, and an `RtcmEncodeError` for a message
/// [`encodeRtcm`] refuses, a body past the frame length limit, or `reserved`
/// wider than six bits.
#[wasm_bindgen(js_name = encodeRtcmFrame)]
pub fn encode_rtcm_frame(
    #[wasm_bindgen(unchecked_param_type = "RtcmMessageInput")] message: JsValue,
    policy: Option<String>,
    reserved: Option<u8>,
) -> Result<Vec<u8>, JsValue> {
    let policy = rtcm_policy(policy)?;
    let (body, _) = message_from_value(message)?
        .encode_with_policy(policy)
        .map_err(rtcm_encode_error)?;
    core_encode_frame_with_reserved(&body, reserved.unwrap_or(0)).map_err(rtcm_encode_error)
}

// The `detail` of a thrown `RtcmEncodeError`. `wasm-pack` writes this into both
// `sidereon.d.ts` targets; `types/sidereon-extra.d.ts` re-exports it.
#[wasm_bindgen(typescript_custom_section)]
const TS_RTCM_DEFINITIONS: &str = r#"
export type RtcmEncodeErrorDetail =
  | { kind: "INVALID_INPUT"; reason: string; message: string }
  | { kind: "RTCM_ENCODE"; core: RtcmEncodeCoreDetail; message: string }
  | { kind: "RTCM_CONVERSION"; core: RtcmConversionCoreDetail; message: string }
  | { kind: "UNKNOWN"; message: string };

export type RtcmEncodeCoreDetail =
  | { kind: "fieldOutOfRange"; messageNumber: number; field: string; value: string; width: number; encoding: "unsigned" | "twosComplement" | "signMagnitude" }
  | { kind: "negativeZeroWithValue"; messageNumber: number; field: string; value: number }
  | { kind: "negativeZeroMask"; messageNumber: number; mask: number }
  | { kind: "messageNumber"; messageNumber: number; record: RtcmRecordKindDetail }
  | { kind: "fieldPresence"; messageNumber: number; record: RtcmRecordKindDetail; field: string; carried: boolean }
  | { kind: "satelliteFieldPresence"; messageNumber: number; record: RtcmRecordKindDetail; satellite: number; field: string; carried: boolean }
  | { kind: "countMismatch"; messageNumber: number; field: string; expected: number; actual: number }
  | { kind: "valueOutOfRange"; messageNumber: number; field: string; value: string; minimum: string; maximum: string }
  | { kind: "nonLatin1Character"; field: string; character: string }
  | { kind: "satelliteIdOutOfRange"; messageNumber: number; field: string; value: number; width: number }
  | { kind: "ssrSatelliteIdOutOfRange"; messageNumber: number; value: number; width: number }
  | { kind: "ssrRecordsNotCarried"; messageNumber: number; ssrKind: string; records: string; count: number }
  | { kind: "ssrCombinedRecordCounts"; messageNumber: number; orbit: number; clock: number }
  | { kind: "ssrCombinedSatelliteMismatch"; messageNumber: number; index: number; orbitSatellite: number; clockSatellite: number }
  | { kind: "ssrHighRateClockTerms"; messageNumber: number; satellite: number; c1: number; c2: number }
  | { kind: "ssrSatelliteCount"; messageNumber: number; declared: number; records: number }
  | { kind: "msmMask"; messageNumber: number; problem: RtcmMsmMaskProblem }
  | { kind: "msmOptional"; messageNumber: number; messageKind: string; satellite: number; signal: number | null; field: string; problem: RtcmMsmOptionalProblem }
  | { kind: "trailingZeroBits"; messageNumber: number; bits: number }
  | { kind: "strictDeparture"; departure: RtcmDeparture }
  | { kind: "unsupportedBodyTooShort" | "unsupportedDecodedNumber"; messageNumber: number }
  | { kind: "unsupportedBodyNumber"; messageNumber: number; carried: number }
  | { kind: "frameBodyTooLong"; len: number }
  | { kind: "frameReservedOutOfRange"; value: number }
  | { kind: "unrecognizedRtcmEncodeError" };

export type RtcmRecordKindDetail =
  | { kind: "stationCoordinates" | "antennaDescriptor" | "legacyObservations" | "systemParameters" | "text" | "glonassCodePhaseBiases" }
  | { kind: "msm" | "ssr"; system: RtcmGnssSystem; messageKind: string }
  | { kind: "network" | "transformation"; family: string }
  | { kind: "ssrVtec"; messageNumber: number }
  | { kind: "unrecognizedRecord" };

export type RtcmMsmMaskProblem =
  | { kind: "satelliteOutsideMask" | "satelliteListedTwice"; satellite: number }
  | { kind: "signalOutsideMask"; signal: number }
  | { kind: "signalNotInMask"; signal: number; mask: number }
  | { kind: "signalSatelliteNotListed"; signal: number; satellite: number }
  | { kind: "cellListedTwice"; satellite: number; signal: number }
  | { kind: "unrecognizedMsmMaskProblem" };

export type RtcmMsmOptionalProblem =
  | { kind: "missing" | "notCarried" }
  | { kind: "invalidValue"; value: number }
  | { kind: "unrecognized" };

export type RtcmConversionCoreDetail =
  | { kind: "satelliteIdOutOfRange"; messageNumber: number; field: string; value: number; width: number }
  | { kind: "invalidSatellite"; messageNumber: number; field: string; value: number; error: { kind: "invalidInput"; field: string; reason: string } | { kind: "unrecognizedSatelliteIdError" } }
  | { kind: "sbasPrnOutsideWindow"; value: number; broadcastPrn: number }
  | { kind: "noLnavRecord"; value: number; satellite: string }
  | { kind: "weekMismatch"; messageNumber: number; fullWeek: number; week: number }
  | { kind: "navicWeekMismatch"; fullWeek: number; week: number }
  | { kind: "timeNotRepresentable"; field: string }
  | { kind: "galileoWeekOverflow" | "sisaNoPrediction" }
  | { kind: "sisaSpare"; index: number }
  | { kind: "uraOutOfRange" | "uraNoPrediction"; system: RtcmGnssSystem; index: number }
  | { kind: "fitInterval"; error: RtcmLnavRecordError }
  | { kind: "vtecEvaluation"; problem: RtcmVtecProblem }
  | { kind: "unrecognizedRtcmConversionError" };

export type RtcmLnavRecordError =
  | { kind: "notGps"; satellite: string }
  | { kind: "invalidEpoch"; field: string }
  | { kind: "weekMismatch"; fullWeek: number; decodedWeek: bigint }
  | { kind: "noUraPrediction"; index: bigint }
  | { kind: "fitIntervalUnsupported"; fitIntervalFlag: bigint; iode: bigint; iodc: bigint }
  | { kind: "unrecognizedLnavRecordError" };

export type RtcmVtecProblem =
  | { kind: "computationTime" | "frequency" | "nonFiniteCoordinates" | "invalidGeometry" | "belowHorizon" }
  | { kind: "messageIdentity"; messageNumber: number }
  | { kind: "layerCount"; layers: number }
  | { kind: "layerDegreeOrder"; layerIndex: number; degree: number; order: number }
  | { kind: "coefficientCounts"; layerIndex: number; cosineExpected: number; cosineActual: number; sineExpected: number; sineActual: number }
  | { kind: "unavailableCoefficient" | "shellNotAboveReceiver" | "invalidMappingFactor"; layerIndex: number }
  | { kind: "missingCoefficient"; layerIndex: number; field: string; index: number }
  | { kind: "physicalResultOutOfRange"; field: string }
  | { kind: "unrecognizedVtecProblem" };

/** The GNSS system of an RTCM MSM or SSR message, as the decoder writes it. The encoder also reads the lower-case spelling. */

export type RtcmGnssSystem = "GPS" | "GLONASS" | "Galileo" | "BeiDou" | "QZSS" | "NavIC" | "SBAS" | "gps" | "glonass" | "galileo" | "beidou" | "qzss" | "navic" | "sbas";

export interface RtcmMsmHeader {
    referenceStationId: number;
    epochTime: number;
    multipleMessage: boolean;
    iods: number;
    reserved: number;
    clockSteering: number;
    externalClock: number;
    divergenceFreeSmoothing: boolean;
    smoothingInterval: number;
}

export interface RtcmMsmSatellite {
    id: number;
    roughRangeMs: number | undefined;
    roughRangeMod1: number;
    extendedInfo: number | undefined;
    roughPhaseRangeRateMS: number | undefined;
}

export interface RtcmMsmSignal {
    satelliteId: number;
    signalId: number;
    finePseudorange: number | undefined;
    finePhaseRange: number | undefined;
    lockTimeIndicator: number | undefined;
    halfCycleAmbiguity: boolean | undefined;
    cnr: number | undefined;
    finePhaseRangeRate: number | undefined;
}

export interface RtcmSsrHeader {
    epochTimeS: number;
    updateInterval: number;
    multipleMessage: boolean;
    iodSsr: number;
    providerId: number;
    solutionId: number;
    satelliteReferenceDatum: boolean | undefined;
    dispersiveBiasConsistency: boolean | undefined;
    mwConsistency: boolean | undefined;
    satelliteCount: number;
}

export interface RtcmSsrOrbit {
    satelliteId: number;
    iode: number;
    iodCrc: number | null;
    deltaRadial: number;
    deltaAlong: number;
    deltaCross: number;
    dotDeltaRadial: number;
    dotDeltaAlong: number;
    dotDeltaCross: number;
}

export interface RtcmSsrClock {
    satelliteId: number;
    c0: number;
    c1: number;
    c2: number;
}

export interface RtcmSsrCodeBias {
    satelliteId: number;
    biases: [number, number][];
}

export interface RtcmSsrPhaseBiasSignal {
    signalId: number;
    integerIndicator: number;
    wideLaneIntegerIndicator: number;
    discontinuityCounter: number;
    bias: number;
}

export interface RtcmSsrPhaseBias {
    satelliteId: number;
    yawAngle: number;
    yawRate: number;
    biases: RtcmSsrPhaseBiasSignal[];
}

export interface RtcmMsmMessage {
    type: "msm";
    messageNumber: number;
    system: RtcmGnssSystem;
    kind: "msm4" | "msm7";
    header: RtcmMsmHeader;
    signalMask: number;
    satellites: RtcmMsmSatellite[];
    signals: RtcmMsmSignal[];
    trailingBits: boolean[];
}

export interface RtcmStationCoordinates {
    type: "stationCoordinates";
    messageNumber: number;
    referenceStationId: number;
    itrfRealizationYear: number;
    gpsIndicator: boolean;
    glonassIndicator: boolean;
    galileoIndicator: boolean;
    referenceStationIndicator: boolean;
    ecefX: bigint;
    singleReceiverOscillator: boolean;
    reserved: boolean;
    ecefY: bigint;
    quarterCycleIndicator: number;
    ecefZ: bigint;
    antennaHeight: number | undefined;
    trailingBits: boolean[];
    xM: number;
    yM: number;
    zM: number;
    antennaHeightM: number | undefined;
}

export interface RtcmAntennaDescriptor {
    type: "antennaDescriptor";
    messageNumber: number;
    referenceStationId: number;
    antennaDescriptor: string;
    antennaSetupId: number;
    antennaSerialNumber: string | undefined;
    receiverType: string | undefined;
    receiverFirmwareVersion: string | undefined;
    receiverSerialNumber: string | undefined;
    trailingBits: boolean[];
}

export interface RtcmGpsEphemeris {
    type: "gpsEphemeris";
    messageNumber: number;
    satelliteId: number;
    weekNumber: number;
    svAccuracy: number;
    codeOnL2: number;
    idot: number;
    iode: number;
    tOc: number;
    aF2: number;
    aF1: number;
    aF0: number;
    iodc: number;
    cRs: number;
    deltaN: number;
    m0: bigint;
    cUc: number;
    eccentricity: bigint;
    cUs: number;
    sqrtA: bigint;
    tOe: number;
    cIc: number;
    omega0: bigint;
    cIs: number;
    i0: bigint;
    cRc: number;
    omega: bigint;
    omegaDot: number;
    tGd: number;
    svHealth: number;
    l2PDataFlag: boolean;
    fitInterval: boolean;
    trailingBits: boolean[];
}

export interface RtcmGlonassEphemeris {
    type: "glonassEphemeris";
    messageNumber: number;
    satelliteId: number;
    frequencyChannel: number;
    almanacHealth: boolean;
    almanacHealthAvailability: boolean;
    p1: number;
    tK: number;
    bNMsb: boolean;
    p2: boolean;
    tB: number;
    xnDot: number;
    xn: number;
    xnDotDot: number;
    ynDot: number;
    yn: number;
    ynDotDot: number;
    znDot: number;
    zn: number;
    znDotDot: number;
    p3: boolean;
    gammaN: number;
    mP: number;
    mLNThird: boolean;
    tauN: number;
    deltaTauN: number;
    eN: number;
    mP4: boolean;
    mFT: number;
    mNT: number;
    mM: number;
    additionalDataAvailable: boolean;
    nA: number;
    tauC: bigint;
    mN4: number;
    mTauGps: number;
    mLNFifth: boolean;
    reserved: number;
    negativeZero: number;
    trailingBits: boolean[];
}

export interface RtcmBeidouEphemeris {
    type: "beidouEphemeris";
    messageNumber: number;
    satelliteId: number;
    weekNumber: number;
    svUrai: number;
    idot: number;
    aode: number;
    tOc: number;
    aF2: number;
    aF1: number;
    aF0: number;
    aodc: number;
    cRs: number;
    deltaN: number;
    m0: bigint;
    cUc: number;
    eccentricity: bigint;
    cUs: number;
    sqrtA: bigint;
    tOe: number;
    cIc: number;
    omega0: bigint;
    cIs: number;
    i0: bigint;
    cRc: number;
    omega: bigint;
    omegaDot: number;
    tGd1: number;
    tGd2: number;
    svHealth: boolean;
    trailingBits: boolean[];
}

export interface RtcmNavicEphemeris {
    type: "navicEphemeris";
    messageNumber: number;
    satelliteId: number;
    weekNumber: number;
    aF0: number;
    aF1: number;
    aF2: number;
    ura: number;
    tOc: number;
    tGd: number;
    deltaN: number;
    iodec: number;
    reserved: number;
    l5Flag: boolean;
    sFlag: boolean;
    cUc: number;
    cUs: number;
    cIc: number;
    cIs: number;
    cRc: number;
    cRs: number;
    idot: number;
    m0: bigint;
    tOe: number;
    eccentricity: bigint;
    sqrtA: bigint;
    omega0: bigint;
    omega: bigint;
    omegaDot: number;
    i0: bigint;
    spareDf544: number;
    spareDf545: number;
    trailingBits: boolean[];
}

export interface RtcmQzssEphemeris {
    type: "qzssEphemeris";
    messageNumber: number;
    satelliteId: number;
    tOc: number;
    aF2: number;
    aF1: number;
    aF0: number;
    iode: number;
    cRs: number;
    deltaN: number;
    m0: bigint;
    cUc: number;
    eccentricity: bigint;
    cUs: number;
    sqrtA: bigint;
    tOe: number;
    cIc: number;
    omega0: bigint;
    cIs: number;
    i0: bigint;
    cRc: number;
    omega: bigint;
    omegaDot: number;
    idot: number;
    codesOnL2: number;
    weekNumber: number;
    ura: number;
    svHealth: number;
    tGd: number;
    iodc: number;
    fitInterval: boolean;
    trailingBits: boolean[];
}

export interface RtcmGalileoFnavEphemeris {
    type: "galileoFnavEphemeris";
    messageNumber: number;
    satelliteId: number;
    weekNumber: number;
    iodNav: number;
    sisa: number;
    idot: number;
    tOc: number;
    aF2: number;
    aF1: number;
    aF0: bigint;
    cRs: number;
    deltaN: number;
    m0: bigint;
    cUc: number;
    eccentricity: bigint;
    cUs: number;
    sqrtA: bigint;
    tOe: number;
    cIc: number;
    omega0: bigint;
    cIs: number;
    i0: bigint;
    cRc: number;
    omega: bigint;
    omegaDot: number;
    bgdE5aE1: number;
    e5aSignalHealth: number;
    e5aDataValidity: boolean;
    reserved: number;
    trailingBits: boolean[];
}

export interface RtcmGalileoInavEphemeris {
    type: "galileoInavEphemeris";
    messageNumber: number;
    satelliteId: number;
    weekNumber: number;
    iodNav: number;
    sisaIndex: number;
    idot: number;
    tOc: number;
    aF2: number;
    aF1: number;
    aF0: bigint;
    cRs: number;
    deltaN: number;
    m0: bigint;
    cUc: number;
    eccentricity: bigint;
    cUs: number;
    sqrtA: bigint;
    tOe: number;
    cIc: number;
    omega0: bigint;
    cIs: number;
    i0: bigint;
    cRc: number;
    omega: bigint;
    omegaDot: number;
    bgdE5aE1: number;
    bgdE5bE1: number;
    e5bSignalHealth: number;
    e5bDataValidity: boolean;
    e1bSignalHealth: number;
    e1bDataValidity: boolean;
    reserved: number;
    trailingBits: boolean[];
}

export interface RtcmSsrMessage {
    type: "ssr";
    messageNumber: number;
    system: RtcmGnssSystem;
    kind: "orbit" | "clock" | "combinedOrbitClock" | "codeBias" | "phaseBias" | "ura" | "highRateClock";
    igsSsrVersion: number | null;
    header: RtcmSsrHeader;
    orbit: RtcmSsrOrbit[];
    clock: RtcmSsrClock[];
    codeBias: RtcmSsrCodeBias[];
    phaseBias: RtcmSsrPhaseBias[];
    ura: [number, number][];
    paddingBits: boolean[];
}

export interface RtcmLegacyL1 {
    codeIndicator: boolean;
    pseudorange: number;
    phaseRangeMinusPseudorange: number;
    lockTimeIndicator: number;
    pseudorangeModulusAmbiguity: number | undefined;
    cnr: number | undefined;
}

export interface RtcmNetworkAuxiliaryStation {
    type: "networkAuxiliaryStation";
    networkId: number;
    subnetworkId: number;
    auxiliaryStationCount: number;
    masterStationId: number;
    auxiliaryStationId: number;
    deltaLatitude: number;
    deltaLongitude: number;
    deltaHeight: number;
    trailingBits: boolean[];
}

export interface RtcmNetworkCorrectionDifference {
    satelliteId: number;
    ambiguityStatus: number;
    nonSyncCount: number;
    geometric: number | undefined;
    iod: number | undefined;
    ionospheric: number | undefined;
}

export interface RtcmNetworkCorrectionDifferences {
    type: "networkCorrectionDifferences";
    messageNumber: number;
    networkId: number;
    subnetworkId: number;
    epochTime: number;
    multipleMessage: boolean;
    masterStationId: number;
    auxiliaryStationId: number;
    satelliteCount: number;
    satellites: RtcmNetworkCorrectionDifference[];
    trailingBits: boolean[];
}

export interface RtcmNetworkResidual {
    satelliteId: number;
    sOc: number;
    sOd: number;
    sOh: number;
    sLc: number;
    sLd: number;
}

export interface RtcmNetworkResiduals {
    type: "networkResiduals";
    messageNumber: number;
    epochTime: number;
    referenceStationId: number;
    referenceStationCount: number;
    satelliteCount: number;
    satellites: RtcmNetworkResidual[];
    trailingBits: boolean[];
}

export interface RtcmPhysicalReferenceStation {
    type: "physicalReferenceStation";
    nonPhysicalStationId: number;
    physicalStationId: number;
    itrfRealizationYear: number;
    ecefX: bigint;
    ecefY: bigint;
    ecefZ: bigint;
    trailingBits: boolean[];
}

export interface RtcmFkpGradient {
    satelliteId: number;
    iod: number;
    geometricNorth: number;
    geometricEast: number;
    ionosphericNorth: number;
    ionosphericEast: number;
}

export interface RtcmFkpGradients {
    type: "fkpGradients";
    messageNumber: number;
    referenceStationId: number;
    epochTime: number;
    satelliteCount: number;
    satellites: RtcmFkpGradient[];
    trailingBits: boolean[];
}

export interface RtcmLegacyL2 {
    codeIndicator: number;
    pseudorangeDifference: number;
    phaseRangeMinusL1Pseudorange: number;
    lockTimeIndicator: number;
    cnr: number | undefined;
}

export interface RtcmLegacySatellite {
    satelliteId: number;
    frequencyChannel: number | undefined;
    l1: RtcmLegacyL1;
    l2: RtcmLegacyL2 | undefined;
}

export interface RtcmLegacyObservations {
    type: "legacyObservations";
    messageNumber: number;
    referenceStationId: number;
    epochTime: number;
    synchronousGnss: boolean;
    satelliteCount: number;
    divergenceFreeSmoothing: boolean;
    smoothingInterval: number;
    satellites: RtcmLegacySatellite[];
    trailingBits: boolean[];
}

export interface RtcmMessageAnnouncement {
    messageNumber: number;
    synchronous: boolean;
    interval: number;
}

export interface RtcmSystemParameters {
    type: "systemParameters";
    referenceStationId: number;
    mjd: number;
    secondsOfDay: number;
    announcementCount: number;
    leapSeconds: number;
    announcements: RtcmMessageAnnouncement[];
    trailingBits: boolean[];
}

export interface RtcmTextMessage {
    type: "text";
    referenceStationId: number;
    mjd: number;
    secondsOfDay: number;
    characterCount: number;
    codeUnits: number[];
    trailingBits: boolean[];
}

export interface RtcmGlonassCodePhaseBiases {
    type: "glonassCodePhaseBiases";
    referenceStationId: number;
    aligned: boolean;
    reserved: number;
    l1Ca: number | undefined;
    l1P: number | undefined;
    l2Ca: number | undefined;
    l2P: number | undefined;
    trailingBits: boolean[];
}

export interface RtcmSsrVtecLayer {
    height: number;
    degree: number;
    order: number;
    cosine: number[];
    sine: number[];
}

export interface RtcmSsrVtecMessage {
    type: "ssrVtec";
    messageNumber: number;
    igsSsrVersion: number | undefined;
    epochTimeS: number;
    updateInterval: number;
    multipleMessage: boolean;
    iodSsr: number;
    providerId: number;
    solutionId: number;
    qualityIndicator: number;
    layers: RtcmSsrVtecLayer[];
    trailingBits: boolean[];
}

export interface RtcmHelmertTransformation {
    type: "helmertTransformation";
    messageNumber: number;
    sourceName: string;
    targetName: string;
    systemId: number;
    utilizedMessages: number;
    plateNumber: number;
    computationIndicator: number;
    heightIndicator: number;
    validityLatitude: number;
    validityLongitude: number;
    validityExtensionLatitude: number;
    validityExtensionLongitude: number;
    dx: number; dy: number; dz: number; r1: number; r2: number; r3: number; ds: number;
    rotationPoint: { x: number; y: number; z: number } | null;
    addAs: number; addBs: number; addAt: number; addBt: number;
    horizontalQuality: number; verticalQuality: number; trailingBits: boolean[];
}

export interface RtcmResidualGrid {
    type: "residualGrid";
    messageNumber: number; systemId: number; horizontalShift: boolean; verticalShift: boolean;
    origin1: number; origin2: number; extension1: number; extension2: number;
    meanOffset1: number; meanOffset2: number; meanHeightOffset: number;
    residuals: { horizontal1: number; horizontal2: number; height: number }[];
    horizontalInterpolation: number; verticalInterpolation: number;
    horizontalQuality: number; verticalQuality: number; mjd: number; trailingBits: boolean[];
}

export type RtcmProjectionParameters =
    | { kind: "naturalOrigin"; latitude: bigint; longitude: bigint; addScale: number; falseEasting: bigint; falseNorthing: bigint }
    | { kind: "lambertConicConformal"; latitude: bigint; longitude: bigint; standardParallel1: bigint; standardParallel2: bigint; falseEasting: bigint; falseNorthing: bigint }
    | { kind: "obliqueMercator"; rectification: boolean; latitude: bigint; longitude: bigint; azimuth: bigint; rectifiedToSkew: number; addScale: number; easting: bigint; northing: bigint };

export interface RtcmProjection {
    type: "projection"; systemId: number; projectionType: number;
    parameters: RtcmProjectionParameters; trailingBits: boolean[];
}

export interface RtcmUnsupportedMessage {
    type: "unsupported";
    messageNumber: number;
    body: number[];
}

/** A decoded RTCM 3 message, tagged by `type`. */
export type RtcmMessage =
    | RtcmMsmMessage
    | RtcmLegacyObservations
    | RtcmNetworkAuxiliaryStation
    | RtcmNetworkCorrectionDifferences
    | RtcmNetworkResiduals
    | RtcmPhysicalReferenceStation
    | RtcmFkpGradients
    | RtcmStationCoordinates
    | RtcmAntennaDescriptor
    | RtcmSystemParameters
    | RtcmTextMessage
    | RtcmGlonassCodePhaseBiases
    | RtcmGpsEphemeris
    | RtcmGlonassEphemeris
    | RtcmBeidouEphemeris
    | RtcmNavicEphemeris
    | RtcmQzssEphemeris
    | RtcmGalileoFnavEphemeris
    | RtcmGalileoInavEphemeris
    | RtcmSsrMessage
    | RtcmSsrVtecMessage
    | RtcmHelmertTransformation
    | RtcmResidualGrid
    | RtcmProjection
    | RtcmUnsupportedMessage;

/** A departure from RTCM 3 the lenient policy read. `kind` is "frameReservedBits", "trailingBits", "msmCellMaskOver64" or "ssrRecordsShort", or, for one this binding does not name yet, the engine variant's name in lowerCamelCase. */
export interface RtcmDeparture {
    kind: "frameReservedBits" | "trailingBits" | "msmCellMaskOver64" | "ssrRecordsShort" | (string & {});
    message: string;
    messageNumber?: number;
    reserved?: number;
    bits?: boolean[];
    cells?: number;
    declared?: number;
    read?: number;
    offset?: number;
}

/** A CRC-valid frame the stream reader skipped, with its byte offset. */
export interface RtcmFrameSkip {
    offset: number;
    messageNumber?: number;
    reason: "truncated" | "malformed" | "departure";
    message?: string;
}

export interface RtcmStreamDiagnostics {
    resyncBytes: number;
    crcFailures: number;
    skippedFrames: RtcmFrameSkip[];
    departures: RtcmDeparture[];
}

/** The result of decodeRtcmStream. */
export interface RtcmStream {
    messages: RtcmMessage[];
    diagnostics: RtcmStreamDiagnostics;
}

/** The result of decodeRtcmFrame. */
export interface RtcmFrame {
    message: RtcmMessage;
    frameLen: number;
    reserved: number;
    departures: RtcmDeparture[];
}

/** One CRC-valid frame from FrameScanner.next(). */
export interface RtcmScannedFrame {
    body: Uint8Array;
    frameLen: number;
    reserved: number;
}


export interface RtcmMsmSatelliteInput {
    id: number;
    roughRangeMs?: number | null;
    roughRangeMod1: number;
    extendedInfo?: number;
    roughPhaseRangeRateMS?: number;
}

export interface RtcmMsmSignalInput {
    satelliteId: number;
    signalId: number;
    finePseudorange?: number | null;
    finePhaseRange?: number | null;
    lockTimeIndicator?: number | null;
    halfCycleAmbiguity?: boolean | null;
    cnr?: number | null;
    finePhaseRangeRate?: number;
}

export interface RtcmMsmMessageInput {
    type: "msm";
    messageNumber: number;
    system: RtcmGnssSystem;
    kind: "msm1" | "msm2" | "msm3" | "msm4" | "msm5" | "msm6" | "msm7";
    header: RtcmMsmHeader;
    signalMask?: number;
    satellites: RtcmMsmSatelliteInput[];
    signals: RtcmMsmSignalInput[];
    trailingBits?: boolean[];
}

export interface RtcmStationCoordinatesInput {
    type: "stationCoordinates";
    messageNumber: number;
    referenceStationId: number;
    itrfRealizationYear: number;
    gpsIndicator: boolean;
    glonassIndicator: boolean;
    galileoIndicator: boolean;
    referenceStationIndicator: boolean;
    ecefX: bigint | number;
    singleReceiverOscillator: boolean;
    reserved: boolean;
    ecefY: bigint | number;
    quarterCycleIndicator: number;
    ecefZ: bigint | number;
    antennaHeight?: number;
    trailingBits?: boolean[];
}

export interface RtcmAntennaDescriptorInput {
    type: "antennaDescriptor";
    messageNumber: number;
    referenceStationId: number;
    antennaDescriptor: string;
    antennaSetupId: number;
    antennaSerialNumber?: string;
    receiverType?: string;
    receiverFirmwareVersion?: string;
    receiverSerialNumber?: string;
    trailingBits?: boolean[];
}

export interface RtcmGpsEphemerisInput {
    type: "gpsEphemeris";
    satelliteId: number;
    weekNumber: number;
    svAccuracy: number;
    codeOnL2: number;
    idot: number;
    iode: number;
    tOc: number;
    aF2: number;
    aF1: number;
    aF0: number;
    iodc: number;
    cRs: number;
    deltaN: number;
    m0: bigint | number;
    cUc: number;
    eccentricity: bigint | number;
    cUs: number;
    sqrtA: bigint | number;
    tOe: number;
    cIc: number;
    omega0: bigint | number;
    cIs: number;
    i0: bigint | number;
    cRc: number;
    omega: bigint | number;
    omegaDot: number;
    tGd: number;
    svHealth: number;
    l2PDataFlag: boolean;
    fitInterval: boolean;
    trailingBits?: boolean[];
}

export interface RtcmGlonassEphemerisInput {
    type: "glonassEphemeris";
    satelliteId: number;
    frequencyChannel: number;
    almanacHealth: boolean;
    almanacHealthAvailability: boolean;
    p1: number;
    tK: number;
    bNMsb: boolean;
    p2: boolean;
    tB: number;
    xnDot: number;
    xn: number;
    xnDotDot: number;
    ynDot: number;
    yn: number;
    ynDotDot: number;
    znDot: number;
    zn: number;
    znDotDot: number;
    p3: boolean;
    gammaN: number;
    mP: number;
    mLNThird: boolean;
    tauN: number;
    deltaTauN: number;
    eN: number;
    mP4: boolean;
    mFT: number;
    mNT: number;
    mM: number;
    additionalDataAvailable: boolean;
    nA: number;
    tauC: bigint | number;
    mN4: number;
    mTauGps: number;
    mLNFifth: boolean;
    reserved: number;
    negativeZero?: number;
    trailingBits?: boolean[];
}

export interface RtcmBeidouEphemerisInput {
    type: "beidouEphemeris";
    satelliteId: number;
    weekNumber: number;
    svUrai: number;
    idot: number;
    aode: number;
    tOc: number;
    aF2: number;
    aF1: number;
    aF0: number;
    aodc: number;
    cRs: number;
    deltaN: number;
    m0: bigint | number;
    cUc: number;
    eccentricity: bigint | number;
    cUs: number;
    sqrtA: bigint | number;
    tOe: number;
    cIc: number;
    omega0: bigint | number;
    cIs: number;
    i0: bigint | number;
    cRc: number;
    omega: bigint | number;
    omegaDot: number;
    tGd1: number;
    tGd2: number;
    svHealth: boolean;
    trailingBits?: boolean[];
}

export interface RtcmNavicEphemerisInput {
    type: "navicEphemeris";
    satelliteId: number;
    weekNumber: number;
    aF0: number;
    aF1: number;
    aF2: number;
    ura: number;
    tOc: number;
    tGd: number;
    deltaN: number;
    iodec: number;
    reserved: number;
    l5Flag: boolean;
    sFlag: boolean;
    cUc: number;
    cUs: number;
    cIc: number;
    cIs: number;
    cRc: number;
    cRs: number;
    idot: number;
    m0: bigint | number;
    tOe: number;
    eccentricity: bigint | number;
    sqrtA: bigint | number;
    omega0: bigint | number;
    omega: bigint | number;
    omegaDot: number;
    i0: bigint | number;
    spareDf544: number;
    spareDf545: number;
    trailingBits?: boolean[];
}

export interface RtcmQzssEphemerisInput {
    type: "qzssEphemeris";
    satelliteId: number;
    tOc: number;
    aF2: number;
    aF1: number;
    aF0: number;
    iode: number;
    cRs: number;
    deltaN: number;
    m0: bigint | number;
    cUc: number;
    eccentricity: bigint | number;
    cUs: number;
    sqrtA: bigint | number;
    tOe: number;
    cIc: number;
    omega0: bigint | number;
    cIs: number;
    i0: bigint | number;
    cRc: number;
    omega: bigint | number;
    omegaDot: number;
    idot: number;
    codesOnL2: number;
    weekNumber: number;
    ura: number;
    svHealth: number;
    tGd: number;
    iodc: number;
    fitInterval: boolean;
    trailingBits?: boolean[];
}

export interface RtcmGalileoFnavEphemerisInput {
    type: "galileoFnavEphemeris";
    satelliteId: number;
    weekNumber: number;
    iodNav: number;
    sisa: number;
    idot: number;
    tOc: number;
    aF2: number;
    aF1: number;
    aF0: bigint | number;
    cRs: number;
    deltaN: number;
    m0: bigint | number;
    cUc: number;
    eccentricity: bigint | number;
    cUs: number;
    sqrtA: bigint | number;
    tOe: number;
    cIc: number;
    omega0: bigint | number;
    cIs: number;
    i0: bigint | number;
    cRc: number;
    omega: bigint | number;
    omegaDot: number;
    bgdE5aE1: number;
    e5aSignalHealth: number;
    e5aDataValidity: boolean;
    reserved: number;
    trailingBits?: boolean[];
}

export interface RtcmGalileoInavEphemerisInput {
    type: "galileoInavEphemeris";
    satelliteId: number;
    weekNumber: number;
    iodNav: number;
    sisaIndex: number;
    idot: number;
    tOc: number;
    aF2: number;
    aF1: number;
    aF0: bigint | number;
    cRs: number;
    deltaN: number;
    m0: bigint | number;
    cUc: number;
    eccentricity: bigint | number;
    cUs: number;
    sqrtA: bigint | number;
    tOe: number;
    cIc: number;
    omega0: bigint | number;
    cIs: number;
    i0: bigint | number;
    cRc: number;
    omega: bigint | number;
    omegaDot: number;
    bgdE5aE1: number;
    bgdE5bE1: number;
    e5bSignalHealth: number;
    e5bDataValidity: boolean;
    e1bSignalHealth: number;
    e1bDataValidity: boolean;
    reserved: number;
    trailingBits?: boolean[];
}

export interface RtcmUnsupportedMessageInput {
    type: "unsupported";
    messageNumber: number;
    body: number[];
}

/**
 * An RTCM 3 message to encode, tagged by `type`. A decoded RtcmMessage of
 * these types is accepted as it was read; omitted optional fields take their
 * defaults (no trailing bits, the signal mask the signals imply).
 */
export type RtcmMessageInput =
    | RtcmMsmMessageInput
    | RtcmLegacyObservationsInput
    | RtcmNetworkAuxiliaryStationInput
    | RtcmNetworkCorrectionDifferencesInput
    | RtcmNetworkResidualsInput
    | RtcmPhysicalReferenceStationInput
    | RtcmFkpGradientsInput
    | RtcmStationCoordinatesInput
    | RtcmAntennaDescriptorInput
    | RtcmSystemParametersInput
    | RtcmTextMessageInput
    | RtcmGlonassCodePhaseBiasesInput
    | RtcmGpsEphemerisInput
    | RtcmGlonassEphemerisInput
    | RtcmBeidouEphemerisInput
    | RtcmNavicEphemerisInput
    | RtcmQzssEphemerisInput
    | RtcmGalileoFnavEphemerisInput
    | RtcmGalileoInavEphemerisInput
    | RtcmSsrMessageInput
    | RtcmSsrVtecMessageInput
    | RtcmHelmertTransformationInput
    | RtcmResidualGridInput
    | RtcmProjectionInput
    | RtcmUnsupportedMessageInput;

export type RtcmLegacyObservationsInput = Omit<RtcmLegacyObservations, "trailingBits"> & {
    trailingBits?: boolean[];
};
export type RtcmSystemParametersInput = Omit<RtcmSystemParameters, "trailingBits"> & {
    trailingBits?: boolean[];
};
export type RtcmTextMessageInput = Omit<RtcmTextMessage, "trailingBits"> & {
    trailingBits?: boolean[];
};
export type RtcmGlonassCodePhaseBiasesInput = Omit<RtcmGlonassCodePhaseBiases, "trailingBits"> & {
    trailingBits?: boolean[];
};
export type RtcmSsrVtecMessageInput = Omit<RtcmSsrVtecMessage, "trailingBits"> & {
    trailingBits?: boolean[];
};
export type RtcmHelmertTransformationInput = Omit<RtcmHelmertTransformation, "trailingBits"> & { trailingBits?: boolean[] };
export type RtcmResidualGridInput = Omit<RtcmResidualGrid, "trailingBits"> & { trailingBits?: boolean[] };
export type RtcmProjectionInput = Omit<RtcmProjection, "trailingBits"> & { trailingBits?: boolean[] };
export type RtcmNetworkAuxiliaryStationInput = Omit<RtcmNetworkAuxiliaryStation, "type" | "trailingBits"> & {
    type: "networkAuxiliaryStation";
    trailingBits?: boolean[];
};
export type RtcmNetworkCorrectionDifferencesInput = Omit<RtcmNetworkCorrectionDifferences, "trailingBits"> & {
    trailingBits?: boolean[];
};
export type RtcmNetworkResidualsInput = Omit<RtcmNetworkResiduals, "trailingBits"> & {
    trailingBits?: boolean[];
};
export type RtcmPhysicalReferenceStationInput = Omit<RtcmPhysicalReferenceStation, "trailingBits"> & {
    trailingBits?: boolean[];
};
export type RtcmFkpGradientsInput = Omit<RtcmFkpGradients, "trailingBits"> & {
    trailingBits?: boolean[];
};

/** An RTCM SSR message to encode: the raw transmitted fields decodeRtcm returns. */
export interface RtcmSsrMessageInput {
    type: "ssr";
    messageNumber: number;
    igsSsrVersion?: number | null;
    system: RtcmGnssSystem;
    kind: RtcmSsrMessage["kind"];
    header: {
        epochTimeS: number;
        updateInterval: number;
        multipleMessage: boolean;
        iodSsr: number;
        providerId: number;
        solutionId: number;
        satelliteReferenceDatum?: boolean;
        dispersiveBiasConsistency?: boolean;
        mwConsistency?: boolean;
        satelliteCount: number;
    };
    orbit?: Array<Omit<RtcmSsrOrbit, "iodCrc"> & { iodCrc?: number | null }>;
    clock?: RtcmSsrClock[];
    codeBias?: RtcmSsrCodeBias[];
    phaseBias?: RtcmSsrPhaseBias[];
    ura?: [number, number][];
    paddingBits?: boolean[];
}

"#;
