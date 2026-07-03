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
    decode_stream as core_decode_stream, derive_lli as core_derive_lli,
    message_number as core_message_number, minimum_lock_time_ms as core_minimum_lock_time_ms,
    msm_epoch_dt_ms as core_msm_epoch_dt_ms, msm_signal_rinex_code as core_msm_signal_rinex_code,
    AntennaDescriptor, FrameScanner as CoreFrameScanner, GlonassEphemeris, GpsEphemeris,
    LockTimeTracker as CoreLockTimeTracker, Message, MsmHeader, MsmKind, MsmMessage, MsmSatellite,
    MsmSignal, PreviousLock, SsrClockRecord, SsrCodeBiasRecord, SsrHeader, SsrKind, SsrMessage,
    SsrOrbitRecord, SsrPhaseBiasRecord, SsrPhaseBiasSignal, StationCoordinates, UnsupportedMessage,
    LLI_HALF_CYCLE, LLI_LOSS_OF_LOCK,
};
use sidereon_core::GnssSystem;

use crate::error::{engine_error, type_error};

fn gnss_system_label(system: GnssSystem) -> &'static str {
    system.as_str()
}

fn msm_kind_label(kind: MsmKind) -> &'static str {
    match kind {
        MsmKind::Msm4 => "msm4",
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
        SsrKind::Vtec => "vtec",
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
    rough_range_ms: u8,
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
    fine_pseudorange: i32,
    fine_phase_range: i32,
    lock_time_indicator: u16,
    half_cycle_ambiguity: bool,
    cnr: u16,
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
    satellites: Vec<MsmSatelliteObject>,
    signals: Vec<MsmSignalObject>,
}

impl From<&MsmMessage> for MsmObject {
    fn from(m: &MsmMessage) -> Self {
        Self {
            message_number: m.message_number,
            system: gnss_system_label(m.system),
            kind: msm_kind_label(m.kind),
            header: MsmHeaderObject::from(&m.header),
            satellites: m.satellites.iter().map(MsmSatelliteObject::from).collect(),
            signals: m.signals.iter().map(MsmSignalObject::from).collect(),
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
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamDiagnosticsObject {
    resync_bytes: f64,
    skipped_frames: Vec<FrameSkipObject>,
}

impl From<&sidereon_core::rtcm::StreamDiagnostics> for StreamDiagnosticsObject {
    fn from(diagnostics: &sidereon_core::rtcm::StreamDiagnostics) -> Self {
        Self {
            resync_bytes: diagnostics.resync_bytes as f64,
            skipped_frames: diagnostics
                .skipped_frames
                .iter()
                .map(FrameSkipObject::from)
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
        }
    }
}

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
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UnsupportedObject {
    message_number: u16,
    body: Vec<u8>,
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

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum MessageObject {
    Msm(MsmObject),
    StationCoordinates(StationObject),
    AntennaDescriptor(AntennaObject),
    GpsEphemeris(GpsEphemerisObject),
    GlonassEphemeris(GlonassEphemerisObject),
    Ssr(SsrObject),
    Unsupported(UnsupportedObject),
}

impl From<&Message> for MessageObject {
    fn from(message: &Message) -> Self {
        match message {
            Message::Msm(m) => MessageObject::Msm(MsmObject::from(m)),
            Message::StationCoordinates(s) => {
                MessageObject::StationCoordinates(StationObject::from(s))
            }
            Message::AntennaDescriptor(a) => {
                MessageObject::AntennaDescriptor(AntennaObject::from(a))
            }
            Message::GpsEphemeris(e) => MessageObject::GpsEphemeris(GpsEphemerisObject::from(e)),
            Message::GlonassEphemeris(e) => {
                MessageObject::GlonassEphemeris(GlonassEphemerisObject::from(e))
            }
            Message::Ssr(s) => MessageObject::Ssr(SsrObject::from(s)),
            Message::Unsupported(u) => MessageObject::Unsupported(UnsupportedObject::from(u)),
        }
    }
}

fn serializer() -> serde_wasm_bindgen::Serializer {
    serde_wasm_bindgen::Serializer::new()
        .serialize_maps_as_objects(true)
        .serialize_large_number_types_as_bigints(true)
}

/// Decode every CRC-valid RTCM 3 frame in a byte buffer into the message IR.
///
/// Frames whose CRC fails, or whose body cannot be decoded, are skipped and the
/// scan resynchronizes on the next preamble. Returns an array of message objects,
/// each tagged with a `type` discriminant (`"msm"`, `"stationCoordinates"`,
/// `"antennaDescriptor"`, `"gpsEphemeris"`, `"glonassEphemeris"`,
/// `"unsupported"`). Delegates to `sidereon_core::rtcm::decode_messages`.
#[wasm_bindgen(js_name = decodeRtcm)]
pub fn decode_rtcm(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let objects: Vec<MessageObject> = core_decode_messages(bytes)
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
/// and `diagnostics.skippedFrames` reports CRC-valid frames whose bodies could
/// not be decoded.
#[wasm_bindgen(js_name = decodeRtcmStream)]
pub fn decode_rtcm_stream(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let stream = core_decode_stream(bytes);
    let object = RtcmStreamObject {
        messages: stream.messages.iter().map(MessageObject::from).collect(),
        diagnostics: StreamDiagnosticsObject::from(&stream.diagnostics),
    };
    object
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

/// Minimum continuous-lock time encoded by an MSM4/7 lock-time indicator.
///
/// `kind` is `"msm4"` or `"msm7"`. Reserved or out-of-range indicators return
/// `undefined`.
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
#[wasm_bindgen(js_name = decodeRtcmMessage)]
pub fn decode_rtcm_message(body: &[u8]) -> Result<JsValue, JsValue> {
    let message = Message::decode(body).map_err(engine_error)?;
    MessageObject::from(&message)
        .serialize(&serializer())
        .map_err(|e| type_error(&e.to_string()))
}

/// Decode the single RTCM 3 frame that begins at the start of `bytes`.
///
/// Returns the decoded message object plus the total `frameLen` consumed
/// (preamble, length word, body, CRC). Throws an `Error` if the preamble is
/// missing, the buffer is shorter than the declared frame, or the CRC does not
/// match. Delegates to `sidereon_core::rtcm::decode_frame` +
/// `sidereon_core::rtcm::Message::decode`.
#[wasm_bindgen(js_name = decodeRtcmFrame)]
pub fn decode_rtcm_frame(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let frame = core_decode_frame(bytes).map_err(engine_error)?;
    let message = Message::decode(frame.body).map_err(engine_error)?;

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct FrameObject {
        message: MessageObject,
        // A frame is at most 1029 bytes, so `u32` keeps `frameLen` a plain JS
        // number under the large-number-as-bigint serializer (which the raw
        // 64-bit message fields need).
        frame_len: u32,
    }
    let object = FrameObject {
        message: MessageObject::from(&message),
        frame_len: frame.frame_len as u32,
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
}

struct OwnedFrame {
    body: Vec<u8>,
    frame_len: usize,
}

#[wasm_bindgen]
impl FrameScanner {
    /// Begin scanning `bytes` from the start.
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8]) -> FrameScanner {
        let frames = CoreFrameScanner::new(bytes)
            .map(|frame| OwnedFrame {
                body: frame.body.to_vec(),
                frame_len: frame.frame_len,
            })
            .collect();
        FrameScanner { frames, cursor: 0 }
    }

    /// The total number of CRC-valid frames the scan found.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> usize {
        self.frames.len()
    }

    /// The next CRC-valid frame as `{ body, frameLen }` (`body` a `Uint8Array`,
    /// the message body between the length word and the CRC), or `undefined` when
    /// the scan is exhausted.
    // `next` is the idiomatic JS iterator-step name; this is a wasm-bindgen
    // export, not a Rust `Iterator` impl.
    #[allow(clippy::should_implement_trait)]
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

fn gnss_system_from_label(label: &str) -> Result<GnssSystem, JsValue> {
    Ok(match label {
        "gps" => GnssSystem::Gps,
        "glonass" => GnssSystem::Glonass,
        "galileo" => GnssSystem::Galileo,
        "beidou" => GnssSystem::BeiDou,
        "qzss" => GnssSystem::Qzss,
        "navic" => GnssSystem::Navic,
        "sbas" => GnssSystem::Sbas,
        other => return Err(type_error(&format!("invalid GNSS system label {other:?}"))),
    })
}

fn msm_kind_from_label(label: &str) -> Result<MsmKind, JsValue> {
    Ok(match label {
        "msm4" => MsmKind::Msm4,
        "msm7" => MsmKind::Msm7,
        other => {
            return Err(type_error(&format!(
                "invalid MSM kind label {other:?}: expected \"msm4\" or \"msm7\""
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
        }
    }
}

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
    rough_range_ms: u8,
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
    fine_pseudorange: i32,
    fine_phase_range: i32,
    lock_time_indicator: u16,
    half_cycle_ambiguity: bool,
    cnr: u16,
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
    satellites: Vec<MsmSatelliteInput>,
    signals: Vec<MsmSignalInput>,
}

impl MsmInput {
    fn to_core(&self) -> Result<MsmMessage, JsValue> {
        Ok(MsmMessage {
            message_number: self.message_number,
            system: gnss_system_from_label(&self.system)?,
            kind: msm_kind_from_label(&self.kind)?,
            header: self.header.to_core(),
            satellites: self
                .satellites
                .iter()
                .map(MsmSatelliteInput::to_core)
                .collect(),
            signals: self.signals.iter().map(MsmSignalInput::to_core).collect(),
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

/// Build the core `Message` IR from a `type`-tagged JS object.
fn message_from_value(message: JsValue) -> Result<Message, JsValue> {
    let tag = js_sys::Reflect::get(&message, &JsValue::from_str("type"))
        .ok()
        .and_then(|v| v.as_string())
        .ok_or_else(|| type_error("RTCM message object must carry a string `type` discriminant"))?;
    let built = match tag.as_str() {
        "stationCoordinates" => Message::StationCoordinates(de::<StationInput>(message)?.to_core()),
        "antennaDescriptor" => Message::AntennaDescriptor(de::<AntennaInput>(message)?.to_core()),
        "gpsEphemeris" => Message::GpsEphemeris(de::<GpsEphemerisInput>(message)?.to_core()),
        "glonassEphemeris" => {
            Message::GlonassEphemeris(de::<GlonassEphemerisInput>(message)?.to_core())
        }
        "msm" => Message::Msm(de::<MsmInput>(message)?.to_core()?),
        "unsupported" => Message::Unsupported(de::<UnsupportedInput>(message)?.to_core()),
        other => return Err(type_error(&format!("unknown RTCM message type {other:?}"))),
    };
    Ok(built)
}

/// Encode a constructed RTCM message into a message body (without the transport
/// frame).
///
/// `message` is a `type`-tagged plain object of the same shape [`decodeRtcm`]
/// returns (`"stationCoordinates"`, `"antennaDescriptor"`, `"gpsEphemeris"`,
/// `"glonassEphemeris"`, `"msm"`, `"unsupported"`), carrying the raw transmitted
/// field integers (large fields as `bigint`). Returns the encoded body as a
/// `Uint8Array`. Delegates to `sidereon_core::rtcm::Message::encode`. Throws a
/// `TypeError` for a malformed object or unknown type.
#[wasm_bindgen(js_name = encodeRtcm)]
pub fn encode_rtcm(message: JsValue) -> Result<Vec<u8>, JsValue> {
    Ok(message_from_value(message)?.encode())
}

/// Encode a constructed RTCM message and wrap it in a fresh RTCM transport frame
/// (preamble, length word, body, CRC).
///
/// `message` has the same shape [`encodeRtcm`] takes. Returns the framed bytes as
/// a `Uint8Array`, ready to feed back to [`decodeRtcmFrame`] / [`decodeRtcm`].
/// Delegates to `sidereon_core::rtcm::Message::to_frame`. Throws a `TypeError`
/// for a malformed object and an `Error` if the encoded body exceeds the frame
/// length limit.
#[wasm_bindgen(js_name = encodeRtcmFrame)]
pub fn encode_rtcm_frame(message: JsValue) -> Result<Vec<u8>, JsValue> {
    message_from_value(message)?
        .to_frame()
        .map_err(engine_error)
}
