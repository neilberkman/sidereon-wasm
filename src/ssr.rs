use std::cell::RefCell;

use js_sys::{Array, Object, Reflect};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use sidereon_core::astro::time::model::{GnssWeekTow, TimeScale};
use sidereon_core::astro::time::ValidityMode;
use sidereon_core::positioning::EphemerisSource;
use sidereon_core::rtcm::{decode_frame, Message, SsrKind, SsrMessage};
use sidereon_core::ssr::{
    MissingCorrectionAction, OrbitBasis, SsrCorrectedEphemeris as CoreSsrCorrectedEphemeris,
    SsrCorrectedEphemerisOwned, SsrCorrectionSize,
    SsrCorrectionSizePolicy as CoreCorrectionSizePolicy,
    SsrCorrectionStore as CoreSsrCorrectionStore, SsrFallbackPolicy, SsrNavigationMessage,
    SsrOversizedCorrection, SsrReferencePoint, SsrSatelliteAttitude, SsrSource as CoreSsrSource,
    SsrStateUnavailable,
};
use sidereon_core::staleness::StalenessPolicy;
use sidereon_core::GnssSatelliteId;
use std::sync::Arc;

use crate::error::{engine_error, error_with_detail, type_error};
use crate::frames::ExactEpochQueryValue;
use crate::rinex_nav::BroadcastEphemeris;

#[wasm_bindgen(typescript_custom_section)]
const TS_SSR_SIZE_DEFINITIONS: &str = r#"
export interface SsrCorrectedState {
  positionEcefM: [number, number, number];
  clockS: number;
  ut1Degraded: string | null;
}

export interface SsrCorrectionSize {
  orbitM: number;
  clockM: number;
  orbitExceedsLimit: boolean;
  clockExceedsLimit: boolean;
  exceedsLimit: boolean;
}

export interface SsrOversizedCorrection {
  satellite: string;
  source: "rtcmSsr" | "galileoHas" | "igsSsr";
  providerId: number;
  solutionId: number;
  orbitRefEpochJ2000S: number;
  clockRefEpochJ2000S: number;
  firstQueryEpochJ2000S: number;
  size: SsrCorrectionSize;
}

export interface SsrCorrectionSizeRefusalDetail {
  satellite: string;
  epochJ2000S: number;
  selectionEpochJ2000S: number;
  size: SsrCorrectionSize;
}

export interface SsrIngestRefusal {
  messageNumber: number;
  error: Error & { detail: CoreErrorDetail; cause: CoreErrorDetail };
}

/** Stored clock fields projected from the core correction store. */
export interface SsrClockCorrection {
  source: "rtcmSsr" | "galileoHas" | "igsSsr";
  providerId: number;
  solutionId: number;
  navMessage: "rtcm" | "has" | "igsSsr";
  hasNavMessageIndex: number | undefined;
  iodSsr: number;
  c0M: number;
  c1MS: number;
  c2MS2: number;
  /** Full core high-rate record when one is attached. */
  highRate?: SsrHighRateClock;
  /** Backward-compatible scalar alias for highRate.c0M. */
  highRateC0M: number | undefined;
  refEpochJ2000S: number;
  transmittedEpochJ2000S: number;
  updateIntervalS: number;
}

/** Provider and solution identity copied from a stored SSR correction. */
export interface SsrSolution {
  source: "rtcmSsr" | "galileoHas" | "igsSsr";
  providerId: number;
  solutionId: number;
}

export type SsrStateUnavailableReason =
  | { kind: "excludedByHas" }
  | { kind: "noOrbitCorrection" }
  | { kind: "noClockCorrection" }
  | { kind: "orbitClockMismatch" }
  | { kind: "reservedNavigationMessage"; index: number }
  | { kind: "orbitNotFresh" }
  | { kind: "clockNotFresh" }
  | { kind: "regionalProviderNotAllowed" }
  | { kind: "correctionExceedsLimit"; size: SsrCorrectionSize }
  | { kind: "noBroadcastModel" }
  | { kind: "noMatchingBroadcastRecord"; iode: number }
  | { kind: "invalidBroadcastState" }
  | { kind: "degenerateOrbitFrame" }
  | { kind: "centerOfMassUnresolved" }
  | { kind: "ut1OutsideCoverage"; reason: "beforeCoverage" | "afterCoverage" }
  | { kind: "other" };

export type SsrAppliedOrbitClockStatus =
  | { status: "available"; solution: SsrSolution }
  | { status: "unavailable"; reason: SsrStateUnavailableReason };

export interface SsrCorrectedStateWithGroupDelay {
  positionEcefM: [number, number, number];
  clockS: number;
  groupDelayS: number | undefined;
}

/** Checked exact-epoch state, clock, group delay, and UT1 degradation. */
export interface SsrCorrectedStateWithGroupDelayChecked {
  value: SsrCorrectedStateWithGroupDelay;
  ut1Degraded: "beforeCoverage" | "afterCoverage" | null;
}

/** Stored high-rate clock correction copied from the core record. */
export interface SsrHighRateClock {
  solution: SsrSolution;
  iodSsr: number;
  c0M: number;
  refEpochJ2000S: number;
  transmittedEpochJ2000S: number;
  updateIntervalS: number;
}

/** Stored orbit fields projected from the core correction store. */
export interface SsrOrbitCorrection {
  source: "rtcmSsr" | "galileoHas" | "igsSsr";
  providerId: number;
  solutionId: number;
  navMessage: "rtcm" | "has" | "igsSsr";
  hasNavMessageIndex: number | undefined;
  iode: number;
  /** Native SBAS IOD CRC, when transmitted. */
  iodCrc: number | undefined;
  iodSsr: number;
  /** Core OrbitBasis tag; currently "velocityAligned". */
  basis: "velocityAligned";
  /** Whether the reference datum identifies a regional CRS. */
  crsRegional: boolean;
  /** Stable core tag: 0 is antenna phase center, 1 is center of mass. */
  referencePoint: 0 | 1;
  radialM: number;
  alongM: number;
  crossM: number;
  radialRateMS: number;
  alongRateMS: number;
  crossRateMS: number;
  refEpochJ2000S: number;
  transmittedEpochJ2000S: number;
  updateIntervalS: number;
}

/** Optional constructor tag: 0 is APC and 1 is CoM; other numbers reject. */
export type SsrReferencePointTag = 0 | 1;

/** Configuration for an owned corrected ephemeris source. */
export interface SsrCorrectedEphemerisOptions {
  /** Missing or stale corrections: "decline" (default) or "broadcast". */
  fallback?: "decline" | "broadcast";
  /** Maximum correction age in seconds; finite and non-negative. */
  maxStalenessS?: number;
  /** UT1 policy: "strict" (default) or "permissive". */
  ut1Validity?: "strict" | "permissive";
  /** Correction size policy: "strict" (default) or "lenient". */
  correctionSizePolicy?: "strict" | "lenient";
  /** Satellite attitude for CoM-to-APC conversion. */
  satelliteAttitude?: "unavailable" | "nominalSunFixed";
  /** Providers allowed for regional CRS corrections; each integer is 0..65535. */
  allowRegionalProviders?: number[];
}

export interface SsrRtcmIngestReport {
  store: SsrCorrectionStore;
  diagnostics: RtcmStreamDiagnostics;
  trailingPartialFrameLen: number;
  ingestRefusals: SsrIngestRefusal[];
  isComplete: boolean;
}
"#;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CorrectedStateJs {
    position_ecef_m: [f64; 3],
    clock_s: f64,
    /// The UT1 departure accepted under the permissive UT1 policy, or `null`.
    ut1_degraded: Option<&'static str>,
}

/// `("rtcm", None)` for RTCM, `("has", Some(index))` for Galileo HAS, and
/// `("igsSsr", None)` for IGS SSR. The optional index serializes as undefined
/// when the navigation message does not carry one.
fn nav_message_parts(message: SsrNavigationMessage) -> (&'static str, Option<u8>) {
    match message {
        SsrNavigationMessage::Rtcm => ("rtcm", None),
        SsrNavigationMessage::Has(index) => ("has", Some(index)),
        SsrNavigationMessage::IgsSsr => ("igsSsr", None),
    }
}

fn orbit_basis_tag(basis: OrbitBasis) -> &'static str {
    match basis {
        OrbitBasis::VelocityAligned => "velocityAligned",
    }
}

fn reference_point_tag(point: SsrReferencePoint) -> u8 {
    point.tag()
}

fn reference_point_from_tag(tag: Option<f64>) -> Result<SsrReferencePoint, JsValue> {
    match tag {
        None | Some(0.0) => Ok(SsrReferencePoint::AntennaPhaseCenter),
        Some(1.0) => Ok(SsrReferencePoint::CenterOfMass),
        Some(_) => Err(type_error(
            "reference point tag must be exactly 0 (antenna phase center) or 1 (center of mass)",
        )),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SsrOrbitJs {
    source: &'static str,
    provider_id: u16,
    solution_id: u8,
    /// `"rtcm"` or `"has"`: the navigation message the correction refers to.
    nav_message: &'static str,
    /// The Galileo HAS navigation-message index as transmitted, or undefined.
    has_nav_message_index: Option<u8>,
    iode: u32,
    iod_crc: Option<u32>,
    iod_ssr: u8,
    basis: &'static str,
    crs_regional: bool,
    reference_point: u8,
    radial_m: f64,
    along_m: f64,
    cross_m: f64,
    radial_rate_m_s: f64,
    along_rate_m_s: f64,
    cross_rate_m_s: f64,
    ref_epoch_j2000_s: f64,
    /// The epoch the correction was transmitted for; for Galileo HAS the TOH
    /// epoch.
    transmitted_epoch_j2000_s: f64,
    update_interval_s: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SsrSolutionJs {
    source: &'static str,
    provider_id: u16,
    solution_id: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
enum SsrStateUnavailableJs {
    ExcludedByHas,
    NoOrbitCorrection,
    NoClockCorrection,
    OrbitClockMismatch,
    ReservedNavigationMessage { index: u8 },
    OrbitNotFresh,
    ClockNotFresh,
    RegionalProviderNotAllowed,
    CorrectionExceedsLimit { size: SsrCorrectionSizeJs },
    NoBroadcastModel,
    NoMatchingBroadcastRecord { iode: u32 },
    InvalidBroadcastState,
    DegenerateOrbitFrame,
    CenterOfMassUnresolved,
    Ut1OutsideCoverage { reason: &'static str },
    Other,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
enum SsrAppliedOrbitClockStatusJs {
    Available { solution: SsrSolutionJs },
    Unavailable { reason: SsrStateUnavailableJs },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CorrectedStateWithGroupDelayJs {
    position_ecef_m: [f64; 3],
    clock_s: f64,
    group_delay_s: Option<f64>,
}

fn ssr_state_unavailable_js(unavailable: SsrStateUnavailable) -> SsrStateUnavailableJs {
    use SsrStateUnavailable as U;
    match unavailable {
        U::ExcludedByHas => SsrStateUnavailableJs::ExcludedByHas,
        U::NoOrbitCorrection => SsrStateUnavailableJs::NoOrbitCorrection,
        U::NoClockCorrection => SsrStateUnavailableJs::NoClockCorrection,
        U::OrbitClockMismatch => SsrStateUnavailableJs::OrbitClockMismatch,
        U::ReservedNavigationMessage { index } => {
            SsrStateUnavailableJs::ReservedNavigationMessage { index }
        }
        U::OrbitNotFresh => SsrStateUnavailableJs::OrbitNotFresh,
        U::ClockNotFresh => SsrStateUnavailableJs::ClockNotFresh,
        U::RegionalProviderNotAllowed => SsrStateUnavailableJs::RegionalProviderNotAllowed,
        U::CorrectionExceedsLimit(size) => {
            SsrStateUnavailableJs::CorrectionExceedsLimit { size: size.into() }
        }
        U::NoBroadcastModel => SsrStateUnavailableJs::NoBroadcastModel,
        U::NoMatchingBroadcastRecord { iode } => {
            SsrStateUnavailableJs::NoMatchingBroadcastRecord { iode }
        }
        U::InvalidBroadcastState => SsrStateUnavailableJs::InvalidBroadcastState,
        U::DegenerateOrbitFrame => SsrStateUnavailableJs::DegenerateOrbitFrame,
        U::CenterOfMassUnresolved => SsrStateUnavailableJs::CenterOfMassUnresolved,
        U::Ut1OutsideCoverage(reason) => SsrStateUnavailableJs::Ut1OutsideCoverage {
            reason: crate::spp::degrade_reason_label(reason),
        },
        _ => SsrStateUnavailableJs::Other,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SsrHighRateClockJs {
    solution: SsrSolutionJs,
    iod_ssr: u8,
    c0_m: f64,
    ref_epoch_j2000_s: f64,
    transmitted_epoch_j2000_s: f64,
    update_interval_s: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SsrClockJs {
    source: &'static str,
    provider_id: u16,
    solution_id: u8,
    nav_message: &'static str,
    has_nav_message_index: Option<u8>,
    iod_ssr: u8,
    c0_m: f64,
    c1_m_s: f64,
    c2_m_s2: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    high_rate: Option<SsrHighRateClockJs>,
    high_rate_c0_m: Option<f64>,
    ref_epoch_j2000_s: f64,
    transmitted_epoch_j2000_s: f64,
    update_interval_s: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SsrHeaderJs {
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SsrOrbitRecordJs {
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SsrClockRecordJs {
    satellite_id: u8,
    c0: i32,
    c1: i32,
    c2: i32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SsrCodeBiasRecordJs {
    satellite_id: u8,
    biases: Vec<SsrCodeBiasSignalJs>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SsrCodeBiasSignalJs {
    signal_id: u8,
    bias: i16,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SsrPhaseBiasRecordJs {
    satellite_id: u8,
    yaw_angle: u16,
    yaw_rate: i8,
    biases: Vec<SsrPhaseBiasSignalJs>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SsrPhaseBiasSignalJs {
    signal_id: u8,
    integer_indicator: u8,
    wide_lane_integer_indicator: u8,
    discontinuity_counter: u8,
    bias: i32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SsrMessageJs {
    message_number: u16,
    igs_ssr_version: Option<u8>,
    system: &'static str,
    kind: &'static str,
    header: SsrHeaderJs,
    orbit: Vec<SsrOrbitRecordJs>,
    clock: Vec<SsrClockRecordJs>,
    code_bias: Vec<SsrCodeBiasRecordJs>,
    phase_bias: Vec<SsrPhaseBiasRecordJs>,
    ura: Vec<(u8, u8)>,
    padding_bit_count: usize,
}

/// Source stream for engineering-unit SSR corrections.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SsrSource {
    /// RTCM SSR messages.
    RtcmSsr,
    /// Galileo High Accuracy Service messages.
    GalileoHas,
    /// IGS SSR messages carried in RTCM 4076.
    IgsSsr,
}

impl From<CoreSsrSource> for SsrSource {
    fn from(source: CoreSsrSource) -> Self {
        match source {
            CoreSsrSource::RtcmSsr => Self::RtcmSsr,
            CoreSsrSource::GalileoHas => Self::GalileoHas,
            CoreSsrSource::IgsSsr => Self::IgsSsr,
        }
    }
}

impl From<SsrSource> for CoreSsrSource {
    fn from(source: SsrSource) -> Self {
        match source {
            SsrSource::RtcmSsr => Self::RtcmSsr,
            SsrSource::GalileoHas => Self::GalileoHas,
            SsrSource::IgsSsr => Self::IgsSsr,
        }
    }
}

fn source_label(source: CoreSsrSource) -> &'static str {
    match source {
        CoreSsrSource::RtcmSsr => "rtcmSsr",
        CoreSsrSource::GalileoHas => "galileoHas",
        CoreSsrSource::IgsSsr => "igsSsr",
    }
}

/// Stable lower-camel-case SSR source token.
#[wasm_bindgen(js_name = ssrSourceLabel)]
pub fn ssr_source_label(source: SsrSource) -> String {
    source_label(source.into()).to_string()
}

fn kind_label(kind: SsrKind) -> &'static str {
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

fn ssr_to_js(ssr: SsrMessage) -> Result<JsValue, JsValue> {
    let out = SsrMessageJs {
        message_number: ssr.message_number,
        igs_ssr_version: ssr.igs_ssr_version,
        system: ssr.system.as_str(),
        kind: kind_label(ssr.kind),
        header: SsrHeaderJs {
            epoch_time_s: ssr.header.epoch_time_s,
            update_interval: ssr.header.update_interval,
            multiple_message: ssr.header.multiple_message,
            iod_ssr: ssr.header.iod_ssr,
            provider_id: ssr.header.provider_id,
            solution_id: ssr.header.solution_id,
            satellite_reference_datum: ssr.header.satellite_reference_datum,
            dispersive_bias_consistency: ssr.header.dispersive_bias_consistency,
            mw_consistency: ssr.header.mw_consistency,
            satellite_count: ssr.header.satellite_count,
        },
        orbit: ssr
            .orbit
            .into_iter()
            .map(|record| SsrOrbitRecordJs {
                satellite_id: record.satellite_id,
                iode: record.iode,
                iod_crc: record.iod_crc,
                delta_radial: record.delta_radial,
                delta_along: record.delta_along,
                delta_cross: record.delta_cross,
                dot_delta_radial: record.dot_delta_radial,
                dot_delta_along: record.dot_delta_along,
                dot_delta_cross: record.dot_delta_cross,
            })
            .collect(),
        clock: ssr
            .clock
            .into_iter()
            .map(|record| SsrClockRecordJs {
                satellite_id: record.satellite_id,
                c0: record.c0,
                c1: record.c1,
                c2: record.c2,
            })
            .collect(),
        code_bias: ssr
            .code_bias
            .into_iter()
            .map(|record| SsrCodeBiasRecordJs {
                satellite_id: record.satellite_id,
                biases: record
                    .biases
                    .into_iter()
                    .map(|(signal_id, bias)| SsrCodeBiasSignalJs { signal_id, bias })
                    .collect(),
            })
            .collect(),
        phase_bias: ssr
            .phase_bias
            .into_iter()
            .map(|record| SsrPhaseBiasRecordJs {
                satellite_id: record.satellite_id,
                yaw_angle: record.yaw_angle,
                yaw_rate: record.yaw_rate,
                biases: record
                    .biases
                    .into_iter()
                    .map(|signal| SsrPhaseBiasSignalJs {
                        signal_id: signal.signal_id,
                        integer_indicator: signal.integer_indicator,
                        wide_lane_integer_indicator: signal.wide_lane_integer_indicator,
                        discontinuity_counter: signal.discontinuity_counter,
                        bias: signal.bias,
                    })
                    .collect(),
            })
            .collect(),
        ura: ssr.ura,
        padding_bit_count: ssr.padding_bits.len(),
    };
    serde_wasm_bindgen::to_value(&out).map_err(|e| type_error(&e.to_string()))
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

fn decode_message(bytes: &[u8], framed: bool) -> Result<Message, JsValue> {
    if framed {
        let frame = decode_frame(bytes).map_err(engine_error)?;
        Message::decode(frame.body).map_err(engine_error)
    } else {
        Message::decode(bytes).map_err(engine_error)
    }
}

#[wasm_bindgen(js_name = decodeSsr)]
pub fn decode_ssr(bytes: &[u8], framed: Option<bool>) -> Result<JsValue, JsValue> {
    match decode_message(bytes, framed.unwrap_or(false))? {
        Message::Ssr(ssr) => ssr_to_js(ssr),
        other => Err(type_error(&format!(
            "RTCM message {} is not an SSR message",
            other.message_number()
        ))),
    }
}

#[wasm_bindgen]
pub struct SsrCorrectionStore {
    inner: CoreSsrCorrectionStore,
    size_policy: SsrCorrectionSizePolicy,
    oversized: RefCell<Vec<SsrOversizedCorrection>>,
}

#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
/// Whether oversized SSR orbit or clock corrections are refused or applied.
pub enum SsrCorrectionSizePolicy {
    /// Refuse the satellite state and do not use broadcast fallback.
    Strict,
    /// Apply oversized corrections and retain a report.
    Lenient,
}

impl From<SsrCorrectionSizePolicy> for CoreCorrectionSizePolicy {
    fn from(policy: SsrCorrectionSizePolicy) -> Self {
        match policy {
            SsrCorrectionSizePolicy::Strict => Self::Strict,
            SsrCorrectionSizePolicy::Lenient => Self::Lenient,
        }
    }
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SsrCorrectionSizeJs {
    orbit_m: f64,
    clock_m: f64,
    orbit_exceeds_limit: bool,
    clock_exceeds_limit: bool,
    exceeds_limit: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SsrCorrectionSizeRefusalJs {
    satellite: String,
    epoch_j2000_s: f64,
    selection_epoch_j2000_s: f64,
    size: SsrCorrectionSizeJs,
}

impl From<SsrCorrectionSize> for SsrCorrectionSizeJs {
    fn from(size: SsrCorrectionSize) -> Self {
        Self {
            orbit_m: size.orbit_m,
            clock_m: size.clock_m,
            orbit_exceeds_limit: size.orbit_exceeds_limit(),
            clock_exceeds_limit: size.clock_exceeds_limit(),
            exceeds_limit: size.exceeds_limit(),
        }
    }
}

fn correction_size_refusal_error(
    satellite: GnssSatelliteId,
    epoch_j2000_s: f64,
    selection_epoch_j2000_s: f64,
    size: SsrCorrectionSize,
) -> JsValue {
    let details = SsrCorrectionSizeRefusalJs {
        satellite: satellite.to_string(),
        epoch_j2000_s,
        selection_epoch_j2000_s,
        size: size.into(),
    };
    error_with_detail(
        "SsrCorrectionSizeRefusal",
        "SSR correction exceeds the configured orbit or clock size limit",
        &details,
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SsrOversizedCorrectionJs {
    satellite: String,
    source: &'static str,
    provider_id: u16,
    solution_id: u8,
    orbit_ref_epoch_j2000_s: f64,
    clock_ref_epoch_j2000_s: f64,
    first_query_epoch_j2000_s: f64,
    size: SsrCorrectionSizeJs,
}

impl From<SsrOversizedCorrection> for SsrOversizedCorrectionJs {
    fn from(correction: SsrOversizedCorrection) -> Self {
        Self {
            satellite: correction.sat.to_string(),
            source: source_label(correction.solution.source),
            provider_id: correction.solution.provider_id,
            solution_id: correction.solution.solution_id,
            orbit_ref_epoch_j2000_s: correction.orbit_ref_epoch_j2000_s,
            clock_ref_epoch_j2000_s: correction.clock_ref_epoch_j2000_s,
            first_query_epoch_j2000_s: correction.t_j2000_s,
            size: correction.size.into(),
        }
    }
}

impl Default for SsrCorrectionStore {
    fn default() -> Self {
        Self {
            inner: CoreSsrCorrectionStore::new(),
            size_policy: SsrCorrectionSizePolicy::Strict,
            oversized: RefCell::new(Vec::new()),
        }
    }
}

#[wasm_bindgen]
impl SsrCorrectionStore {
    /// Optional stable tag: omitted or 0 selects APC; 1 selects CoM.
    #[allow(non_snake_case)]
    #[wasm_bindgen(constructor)]
    pub fn new(referencePointTag: Option<f64>) -> Result<SsrCorrectionStore, JsValue> {
        let reference_point = reference_point_from_tag(referencePointTag)?;
        Ok(SsrCorrectionStore {
            inner: CoreSsrCorrectionStore::new().with_reference_point(reference_point),
            size_policy: SsrCorrectionSizePolicy::Strict,
            oversized: RefCell::new(Vec::new()),
        })
    }

    #[wasm_bindgen(js_name = setCorrectionSizePolicy)]
    pub fn set_correction_size_policy(&mut self, policy: SsrCorrectionSizePolicy) {
        self.size_policy = policy;
    }

    #[wasm_bindgen(getter, js_name = correctionSizePolicy)]
    pub fn correction_size_policy(&self) -> SsrCorrectionSizePolicy {
        self.size_policy
    }

    #[wasm_bindgen(getter, js_name = oversizedCorrections, unchecked_return_type = "SsrOversizedCorrection[]")]
    pub fn oversized_corrections(&self) -> Result<JsValue, JsValue> {
        let reports: Vec<SsrOversizedCorrectionJs> = self
            .oversized
            .borrow()
            .iter()
            .copied()
            .map(Into::into)
            .collect();
        serde_wasm_bindgen::to_value(&reports).map_err(engine_error)
    }

    #[wasm_bindgen(js_name = ingest)]
    pub fn ingest(
        &mut self,
        bytes: &[u8],
        framed: Option<bool>,
        week: u32,
        tow_s: f64,
        time_scale: Option<String>,
    ) -> Result<(), JsValue> {
        let message = decode_message(bytes, framed.unwrap_or(false))?;
        let epoch = GnssWeekTow::new(parse_time_scale(time_scale)?, week, tow_s)
            .and_then(GnssWeekTow::normalized)
            .map_err(engine_error)?;
        self.inner.ingest(&message, epoch).map_err(engine_error)
    }

    #[wasm_bindgen(unchecked_return_type = "SsrOrbitCorrection | null")]
    pub fn orbit(&self, sat: &str) -> Result<JsValue, JsValue> {
        let sat = parse_sat(sat)?;
        let Some(orbit) = self.inner.orbit(sat) else {
            return Ok(JsValue::NULL);
        };
        let (nav_message, has_nav_message_index) = nav_message_parts(orbit.nav_message);
        serde_wasm_bindgen::to_value(&SsrOrbitJs {
            source: source_label(orbit.solution.source),
            provider_id: orbit.solution.provider_id,
            solution_id: orbit.solution.solution_id,
            nav_message,
            has_nav_message_index,
            iode: orbit.iode,
            iod_crc: orbit.iod_crc,
            iod_ssr: orbit.iod_ssr,
            basis: orbit_basis_tag(orbit.basis),
            crs_regional: orbit.crs_regional,
            reference_point: reference_point_tag(orbit.reference_point),
            radial_m: orbit.radial_m,
            along_m: orbit.along_m,
            cross_m: orbit.cross_m,
            radial_rate_m_s: orbit.radial_rate_m_s,
            along_rate_m_s: orbit.along_rate_m_s,
            cross_rate_m_s: orbit.cross_rate_m_s,
            ref_epoch_j2000_s: orbit.ref_epoch_j2000_s,
            transmitted_epoch_j2000_s: orbit.transmitted_epoch_j2000_s,
            update_interval_s: orbit.update_interval_s,
        })
        .map_err(|e| type_error(&e.to_string()))
    }

    #[wasm_bindgen(unchecked_return_type = "SsrClockCorrection | null")]
    pub fn clock(&self, sat: &str) -> Result<JsValue, JsValue> {
        let sat = parse_sat(sat)?;
        let Some(clock) = self.inner.clock(sat) else {
            return Ok(JsValue::NULL);
        };
        let (nav_message, has_nav_message_index) = nav_message_parts(clock.nav_message);
        serde_wasm_bindgen::to_value(&SsrClockJs {
            source: source_label(clock.solution.source),
            provider_id: clock.solution.provider_id,
            solution_id: clock.solution.solution_id,
            nav_message,
            has_nav_message_index,
            iod_ssr: clock.iod_ssr,
            c0_m: clock.c0_m,
            c1_m_s: clock.c1_m_s,
            c2_m_s2: clock.c2_m_s2,
            high_rate: clock.high_rate.map(|hr| SsrHighRateClockJs {
                solution: SsrSolutionJs {
                    source: source_label(hr.solution.source),
                    provider_id: hr.solution.provider_id,
                    solution_id: hr.solution.solution_id,
                },
                iod_ssr: hr.iod_ssr,
                c0_m: hr.c0_m,
                ref_epoch_j2000_s: hr.ref_epoch_j2000_s,
                transmitted_epoch_j2000_s: hr.transmitted_epoch_j2000_s,
                update_interval_s: hr.update_interval_s,
            }),
            high_rate_c0_m: clock.high_rate.map(|hr| hr.c0_m),
            ref_epoch_j2000_s: clock.ref_epoch_j2000_s,
            transmitted_epoch_j2000_s: clock.transmitted_epoch_j2000_s,
            update_interval_s: clock.update_interval_s,
        })
        .map_err(|e| type_error(&e.to_string()))
    }

    #[wasm_bindgen(js_name = uraIndex)]
    pub fn ura_index(&self, sat: &str) -> Result<Option<u8>, JsValue> {
        Ok(self.inner.ura_index(parse_sat(sat)?))
    }
}

#[wasm_bindgen]
pub struct SsrRtcmIngest {
    store: SsrCorrectionStore,
    diagnostics: JsValue,
    trailing_partial_frame_len: usize,
    ingest_refusals: JsValue,
    is_complete: bool,
}

#[wasm_bindgen]
impl SsrRtcmIngest {
    #[wasm_bindgen(getter)]
    pub fn store(&self) -> SsrCorrectionStore {
        SsrCorrectionStore {
            inner: self.store.inner.clone(),
            size_policy: self.store.size_policy,
            oversized: RefCell::new(self.store.oversized.borrow().clone()),
        }
    }

    #[wasm_bindgen(getter, unchecked_return_type = "RtcmStreamDiagnostics")]
    pub fn diagnostics(&self) -> JsValue {
        self.diagnostics.clone()
    }

    #[wasm_bindgen(getter, js_name = trailingPartialFrameLen)]
    pub fn trailing_partial_frame_len(&self) -> usize {
        self.trailing_partial_frame_len
    }

    #[wasm_bindgen(getter, js_name = ingestRefusals, unchecked_return_type = "SsrIngestRefusal[]")]
    pub fn ingest_refusals(&self) -> JsValue {
        self.ingest_refusals.clone()
    }

    #[wasm_bindgen(getter, js_name = isComplete)]
    pub fn is_complete(&self) -> bool {
        self.is_complete
    }
}

fn gnss_week_tow(
    week: u32,
    tow_s: f64,
    time_scale: Option<String>,
) -> Result<GnssWeekTow, JsValue> {
    GnssWeekTow::new(parse_time_scale(time_scale)?, week, tow_s)
        .and_then(GnssWeekTow::normalized)
        .map_err(engine_error)
}

fn ingest_refusals_to_js(
    refusals: impl IntoIterator<Item = sidereon::SsrIngestRefusal>,
) -> Result<JsValue, JsValue> {
    let values = Array::new();
    for refusal in refusals {
        let object = Object::new();
        Reflect::set(
            &object,
            &JsValue::from_str("messageNumber"),
            &JsValue::from_f64(f64::from(refusal.message_number)),
        )?;
        Reflect::set(
            &object,
            &JsValue::from_str("error"),
            &crate::core_error::core_error_js(&refusal.error),
        )?;
        values.push(&object);
    }
    Ok(values.into())
}

#[wasm_bindgen(js_name = ssrStoreFromRtcm)]
pub fn ssr_store_from_rtcm(
    bytes: &[u8],
    week: u32,
    tow_s: f64,
    time_scale: Option<String>,
) -> Result<SsrRtcmIngest, JsValue> {
    let ingest = sidereon::ssr_store_from_rtcm(bytes, gnss_week_tow(week, tow_s, time_scale)?);
    let diagnostics = crate::rtcm::stream_diagnostics_to_js(&ingest.diagnostics)?;
    let is_complete = ingest.is_complete();
    let ingest_refusals = ingest_refusals_to_js(ingest.ingest_refusals)?;
    Ok(SsrRtcmIngest {
        store: SsrCorrectionStore {
            inner: ingest.store,
            size_policy: SsrCorrectionSizePolicy::Strict,
            oversized: RefCell::new(Vec::new()),
        },
        diagnostics,
        trailing_partial_frame_len: ingest.trailing_partial_frame_len,
        ingest_refusals,
        is_complete,
    })
}

#[wasm_bindgen(js_name = ssrStoreFromRtcmStrict)]
pub fn ssr_store_from_rtcm_strict(
    bytes: &[u8],
    week: u32,
    tow_s: f64,
    time_scale: Option<String>,
) -> Result<SsrCorrectionStore, JsValue> {
    let inner =
        sidereon::ssr_store_from_rtcm_strict(bytes, gnss_week_tow(week, tow_s, time_scale)?)
            .map_err(|error| match error {
                sidereon::Error::Ssr(core) => crate::core_error::core_error_js(&core),
                other => engine_error(other),
            })?;
    Ok(SsrCorrectionStore {
        inner,
        size_policy: SsrCorrectionSizePolicy::Strict,
        oversized: RefCell::new(Vec::new()),
    })
}

impl SsrCorrectionStore {
    pub(crate) fn core(&self) -> &CoreSsrCorrectionStore {
        &self.inner
    }

    pub(crate) fn record_oversized(&self, reports: Vec<SsrOversizedCorrection>) {
        let mut stored = self.oversized.borrow_mut();
        for report in reports {
            let already_recorded = stored.iter().any(|previous| {
                previous.sat == report.sat
                    && previous.solution == report.solution
                    && previous.orbit_ref_epoch_j2000_s == report.orbit_ref_epoch_j2000_s
                    && previous.clock_ref_epoch_j2000_s == report.clock_ref_epoch_j2000_s
            });
            if !already_recorded {
                stored.push(report);
            }
        }
    }
}

#[wasm_bindgen(js_name = ssrCorrectedState, unchecked_return_type = "SsrCorrectedState | null")]
pub fn ssr_corrected_state(
    broadcast: &BroadcastEphemeris,
    store: &SsrCorrectionStore,
    sat: &str,
    t_j2000_s: f64,
    fallback_to_broadcast: Option<bool>,
    allow_regional_provider: Option<u16>,
    ut1_validity: Option<String>,
) -> Result<JsValue, JsValue> {
    let sat = parse_sat(sat)?;
    let validity = match ut1_validity.as_deref() {
        None | Some("strict") => ValidityMode::Strict,
        Some("permissive") => ValidityMode::Permissive,
        Some(other) => {
            return Err(type_error(&format!(
                "invalid UT1 validity {other:?}: expected \"strict\" or \"permissive\""
            )))
        }
    };
    let fallback = SsrFallbackPolicy {
        on_missing_correction: if fallback_to_broadcast.unwrap_or(false) {
            MissingCorrectionAction::FallBackToBroadcast
        } else {
            MissingCorrectionAction::Decline
        },
        ..Default::default()
    };
    let mut eph = CoreSsrCorrectedEphemeris::new(&broadcast.inner, store.core())
        .with_fallback(fallback)
        .with_validity(validity)
        .with_correction_size_policy(store.size_policy.into());
    if let Some(provider) = allow_regional_provider {
        eph = eph.allow_regional_provider(provider);
    }
    let size_refusal = eph.correction_size_refusal(sat, t_j2000_s, t_j2000_s);
    // A centre-of-mass orbit converted to the antenna phase centre reads UT1;
    // outside the UT1 table the strict policy refuses the state by name
    // rather than declining the satellite.
    let Some(validated) = eph
        .try_position_clock_at_j2000_s(sat, t_j2000_s)
        .map_err(|error| crate::positioning_error::core_source_error(&error))?
    else {
        store.record_oversized(eph.oversized_corrections());
        if let Some(size) = size_refusal {
            return Err(correction_size_refusal_error(
                sat, t_j2000_s, t_j2000_s, size,
            ));
        }
        return Ok(JsValue::NULL);
    };
    store.record_oversized(eph.oversized_corrections());
    let (position_ecef_m, clock_s) = validated.value;
    serde_wasm_bindgen::to_value(&CorrectedStateJs {
        position_ecef_m,
        clock_s,
        ut1_degraded: validated.degraded.map(crate::spp::degrade_reason_label),
    })
    .map_err(|e| type_error(&e.to_string()))
}

#[allow(clippy::too_many_arguments)]
#[wasm_bindgen(js_name = ssrCorrectedStateExact, unchecked_return_type = "SsrCorrectedState | null")]
pub fn ssr_corrected_state_exact(
    broadcast: &BroadcastEphemeris,
    store: &SsrCorrectionStore,
    sat: &str,
    epoch: &ExactEpochQueryValue,
    selection_epoch: &ExactEpochQueryValue,
    fallback_to_broadcast: Option<bool>,
    allow_regional_provider: Option<u16>,
    ut1_validity: Option<String>,
) -> Result<JsValue, JsValue> {
    let sat = parse_sat(sat)?;
    let validity = match ut1_validity.as_deref() {
        None | Some("strict") => ValidityMode::Strict,
        Some("permissive") => ValidityMode::Permissive,
        Some(other) => {
            return Err(type_error(&format!(
                "invalid UT1 validity {other:?}: expected \"strict\" or \"permissive\""
            )))
        }
    };
    let fallback = SsrFallbackPolicy {
        on_missing_correction: if fallback_to_broadcast.unwrap_or(false) {
            MissingCorrectionAction::FallBackToBroadcast
        } else {
            MissingCorrectionAction::Decline
        },
        ..Default::default()
    };
    let mut eph = CoreSsrCorrectedEphemeris::new(&broadcast.inner, store.core())
        .with_fallback(fallback)
        .with_validity(validity)
        .with_correction_size_policy(store.size_policy.into());
    if let Some(provider) = allow_regional_provider {
        eph = eph.allow_regional_provider(provider);
    }
    let size_refusal =
        eph.correction_size_refusal_at_epoch_query(sat, &epoch.core(), &selection_epoch.core());
    let checked = eph
        .corrected_state_with_group_delay_checked_selected_query(
            sat,
            &epoch.core(),
            &selection_epoch.core(),
        )
        .map_err(|error| crate::positioning_error::core_source_error(&error))?;
    store.record_oversized(eph.oversized_corrections());
    let Some((position_ecef_m, clock_s, _)) = checked.value else {
        if let Some(size) = size_refusal {
            return Err(correction_size_refusal_error(
                sat,
                epoch.j2000_seconds(),
                selection_epoch.j2000_seconds(),
                size,
            ));
        }
        return Ok(JsValue::NULL);
    };
    serde_wasm_bindgen::to_value(&CorrectedStateJs {
        position_ecef_m,
        clock_s,
        ut1_degraded: checked.degraded.map(crate::spp::degrade_reason_label),
    })
    .map_err(engine_error)
}

fn exact_ssr_source<'a>(
    broadcast: &'a BroadcastEphemeris,
    store: &'a SsrCorrectionStore,
    fallback_to_broadcast: Option<bool>,
    allow_regional_provider: Option<u16>,
    validity: Option<String>,
) -> Result<CoreSsrCorrectedEphemeris<'a>, JsValue> {
    let fallback = SsrFallbackPolicy {
        on_missing_correction: if fallback_to_broadcast.unwrap_or(false) {
            MissingCorrectionAction::FallBackToBroadcast
        } else {
            MissingCorrectionAction::Decline
        },
        ..Default::default()
    };
    let mut source = CoreSsrCorrectedEphemeris::new(&broadcast.inner, store.core())
        .with_fallback(fallback)
        .with_validity(crate::error::ut1_validity(validity)?)
        .with_correction_size_policy(store.size_policy.into());
    if let Some(provider) = allow_regional_provider {
        source = source.allow_regional_provider(provider);
    }
    Ok(source)
}

#[wasm_bindgen]
impl BroadcastEphemeris {
    /// Evaluate SSR source variance in square metres at the exact state epoch,
    /// selecting the broadcast record at the distinct exact selection epoch.
    #[allow(clippy::too_many_arguments)]
    #[wasm_bindgen(js_name = ssrEphemerisVarianceAtExactQueries)]
    pub fn ssr_ephemeris_variance_at_exact_queries(
        &self,
        store: &SsrCorrectionStore,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        selection_epoch: &ExactEpochQueryValue,
        fallback_to_broadcast: Option<bool>,
        allow_regional_provider: Option<u16>,
        validity: Option<String>,
    ) -> Result<f64, JsValue> {
        let satellite = parse_sat(satellite)?;
        let source = exact_ssr_source(
            self,
            store,
            fallback_to_broadcast,
            allow_regional_provider,
            validity,
        )?;
        Ok(crate::sp3::precise_variance_at_queries(
            &source,
            satellite,
            state_epoch,
            selection_epoch,
        ))
    }

    /// Evaluate state-dependent SSR clock relativity at the exact state epoch
    /// and supplied satellite position in ECEF metres.
    #[allow(clippy::too_many_arguments)]
    #[wasm_bindgen(js_name = ssrClockRelativityAtExactQuery, unchecked_return_type = "ClockRelativity")]
    pub fn ssr_clock_relativity_at_exact_query(
        &self,
        store: &SsrCorrectionStore,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        position_ecef_m: Vec<f64>,
        fallback_to_broadcast: Option<bool>,
        allow_regional_provider: Option<u16>,
        validity: Option<String>,
    ) -> Result<JsValue, JsValue> {
        let satellite = parse_sat(satellite)?;
        let position_ecef_m: [f64; 3] = position_ecef_m
            .try_into()
            .map_err(|_| type_error("positionEcefM must contain exactly three coordinates"))?;
        let source = exact_ssr_source(
            self,
            store,
            fallback_to_broadcast,
            allow_regional_provider,
            validity,
        )?;
        crate::sp3::precise_clock_relativity_at_query(
            &source,
            satellite,
            state_epoch,
            position_ecef_m,
        )
    }
}

fn source_option(options: &JsValue, key: &str) -> Result<JsValue, JsValue> {
    if options.is_undefined() || options.is_null() {
        return Ok(JsValue::UNDEFINED);
    }
    if !options.is_object() || Array::is_array(options) {
        return Err(type_error("options must be an object"));
    }
    Reflect::get(options, &JsValue::from_str(key))
        .map_err(|_| type_error(&format!("could not read options.{key}")))
}
fn optional_string_option(options: &JsValue, key: &str) -> Result<Option<String>, JsValue> {
    let value = source_option(options, key)?;
    if value.is_undefined() || value.is_null() {
        return Ok(None);
    }
    value
        .as_string()
        .map(Some)
        .ok_or_else(|| type_error(&format!("options.{key} must be a string")))
}
fn regional_providers(options: &JsValue) -> Result<Vec<u16>, JsValue> {
    let value = source_option(options, "allowRegionalProviders")?;
    if value.is_undefined() || value.is_null() {
        return Ok(Vec::new());
    }
    if !Array::is_array(&value) {
        return Err(type_error(
            "options.allowRegionalProviders must be an array",
        ));
    }
    Array::from(&value).iter().enumerate().map(|(index, item)| {
        let Some(provider) = item.as_f64() else { return Err(type_error(&format!("options.allowRegionalProviders[{index}] must be a number"))); };
        if !provider.is_finite() || provider.fract() != 0.0 || !(0.0..=f64::from(u16::MAX)).contains(&provider) {
            return Err(type_error(&format!("options.allowRegionalProviders[{index}] must be an integer from 0 through 65535")));
        }
        Ok(provider as u16)
    }).collect()
}
fn optional_staleness(options: &JsValue) -> Result<Option<StalenessPolicy>, JsValue> {
    let value = source_option(options, "maxStalenessS")?;
    if value.is_undefined() || value.is_null() {
        return Ok(None);
    }
    let Some(seconds) = value.as_f64() else {
        return Err(type_error("options.maxStalenessS must be a number"));
    };
    if !seconds.is_finite() || seconds < 0.0 {
        return Err(type_error(
            "options.maxStalenessS must be finite and non-negative",
        ));
    }
    Ok(Some(StalenessPolicy::seconds(seconds)))
}

/// Owned corrected source. It retains broadcast, ANTEX and policy settings; the
/// SSR store is cloned at construction, so later input-store ingestion is not visible.
#[wasm_bindgen]
pub struct SsrCorrectedEphemeris {
    inner: SsrCorrectedEphemerisOwned,
}

#[wasm_bindgen]
impl SsrCorrectedEphemeris {
    /// Input wrappers may be freed after construction. Options include fallback,
    /// staleness, UT1, size, attitude and regional-provider settings.
    #[wasm_bindgen(constructor)]
    pub fn new(
        broadcast: &BroadcastEphemeris,
        store: &SsrCorrectionStore,
        options: Option<JsValue>,
    ) -> Result<SsrCorrectedEphemeris, JsValue> {
        let options = options.unwrap_or(JsValue::UNDEFINED);
        let fallback_action = match optional_string_option(&options, "fallback")?.as_deref() {
            None | Some("decline") => MissingCorrectionAction::Decline,
            Some("broadcast") => MissingCorrectionAction::FallBackToBroadcast,
            Some(other) => {
                return Err(type_error(&format!(
                    "invalid options.fallback {other:?}: expected decline or broadcast"
                )))
            }
        };
        let validity =
            crate::error::ut1_validity(optional_string_option(&options, "ut1Validity")?)?;
        let size_policy = match optional_string_option(&options, "correctionSizePolicy")?.as_deref()
        {
            None | Some("strict") => CoreCorrectionSizePolicy::Strict,
            Some("lenient") => CoreCorrectionSizePolicy::Lenient,
            Some(other) => {
                return Err(type_error(&format!(
                    "invalid options.correctionSizePolicy {other:?}: expected strict or lenient"
                )))
            }
        };
        let attitude = match optional_string_option(&options, "satelliteAttitude")?.as_deref() {
            None | Some("unavailable") => SsrSatelliteAttitude::Unavailable,
            Some("nominalSunFixed") => SsrSatelliteAttitude::NominalSunFixed,
            Some(other) => return Err(type_error(&format!("invalid options.satelliteAttitude {other:?}: expected unavailable or nominalSunFixed"))),
        };
        let fallback = SsrFallbackPolicy {
            on_missing_correction: fallback_action,
            ..Default::default()
        };
        let providers = regional_providers(&options)?;
        let mut inner = SsrCorrectedEphemerisOwned::new(
            Arc::clone(&broadcast.inner),
            Arc::new(store.inner.clone()),
        )
        .with_fallback(fallback)
        .with_validity(validity)
        .with_correction_size_policy(size_policy)
        .with_satellite_attitude(attitude);
        if let Some(staleness) = optional_staleness(&options)? {
            inner = inner.with_staleness(staleness);
        }
        for provider in providers {
            inner = inner.allow_regional_provider(provider);
        }
        Ok(Self { inner })
    }

    /// Return this source with a cloned ANTEX calibration for CoM-to-APC conversion.
    #[wasm_bindgen(js_name = withSatelliteAntennas)]
    pub fn with_satellite_antennas(mut self, antex: &crate::antex::Antex) -> Self {
        self.inner = self
            .inner
            .with_satellite_antennas(Arc::new(antex.core().clone()));
        self
    }

    /// Velocity of the broadcast record selected by the applied SSR orbit IODE.
    #[wasm_bindgen(js_name = correctedVelocityAtJ2000, unchecked_return_type = "[number, number, number] | null")]
    pub fn corrected_velocity_at_j2000(
        &self,
        satellite: &str,
        epoch_j2000_s: f64,
    ) -> Result<JsValue, JsValue> {
        match self
            .inner
            .corrected_velocity(parse_sat(satellite)?, epoch_j2000_s)
        {
            Some(value) => serde_wasm_bindgen::to_value(&value).map_err(engine_error),
            None => Ok(JsValue::NULL),
        }
    }

    /// Identify the SSR orbit and clock solution applied to the satellite state.
    #[wasm_bindgen(js_name = appliedOrbitClockSolutionAtJ2000, unchecked_return_type = "SsrSolution | null")]
    pub fn applied_orbit_clock_solution_at_j2000(
        &self,
        satellite: &str,
        epoch_j2000_s: f64,
    ) -> Result<JsValue, JsValue> {
        match self
            .inner
            .applied_orbit_clock_solution(parse_sat(satellite)?, epoch_j2000_s)
        {
            Some(solution) => serde_wasm_bindgen::to_value(&SsrSolutionJs {
                source: source_label(solution.source),
                provider_id: solution.provider_id,
                solution_id: solution.solution_id,
            })
            .map_err(engine_error),
            None => Ok(JsValue::NULL),
        }
    }

    /// Return the applied SSR solution or the core reason it is unavailable.
    #[wasm_bindgen(js_name = appliedOrbitClockStatusAtJ2000, unchecked_return_type = "SsrAppliedOrbitClockStatus")]
    pub fn applied_orbit_clock_status_at_j2000(
        &self,
        satellite: &str,
        epoch_j2000_s: f64,
    ) -> Result<JsValue, JsValue> {
        let status = match self
            .inner
            .applied_orbit_clock_status(parse_sat(satellite)?, epoch_j2000_s)
        {
            Ok(solution) => SsrAppliedOrbitClockStatusJs::Available {
                solution: SsrSolutionJs {
                    source: source_label(solution.source),
                    provider_id: solution.provider_id,
                    solution_id: solution.solution_id,
                },
            },
            Err(unavailable) => SsrAppliedOrbitClockStatusJs::Unavailable {
                reason: ssr_state_unavailable_js(unavailable),
            },
        };
        serde_wasm_bindgen::to_value(&status).map_err(engine_error)
    }

    /// Return corrected position, clock and broadcast single-frequency group delay.
    #[wasm_bindgen(js_name = correctedStateWithGroupDelayAtJ2000, unchecked_return_type = "SsrCorrectedStateWithGroupDelay | null")]
    pub fn corrected_state_with_group_delay_at_j2000(
        &self,
        satellite: &str,
        epoch_j2000_s: f64,
    ) -> Result<JsValue, JsValue> {
        let state = self
            .inner
            .corrected_state_with_group_delay(parse_sat(satellite)?, epoch_j2000_s)
            .map(
                |(position_ecef_m, clock_s, group_delay_s)| CorrectedStateWithGroupDelayJs {
                    position_ecef_m,
                    clock_s,
                    group_delay_s,
                },
            );
        match state {
            Some(state) => serde_wasm_bindgen::to_value(&state).map_err(engine_error),
            None => Ok(JsValue::NULL),
        }
    }

    /// Return the single-frequency group delay of the selected broadcast record.
    #[wasm_bindgen(js_name = singleFrequencyGroupDelayAtJ2000, unchecked_return_type = "number | null")]
    pub fn single_frequency_group_delay_at_j2000(
        &self,
        satellite: &str,
        epoch_j2000_s: f64,
    ) -> Result<JsValue, JsValue> {
        match self
            .inner
            .single_frequency_group_delay_s(parse_sat(satellite)?, epoch_j2000_s)
        {
            Some(delay) => serde_wasm_bindgen::to_value(&delay).map_err(engine_error),
            None => Ok(JsValue::NULL),
        }
    }

    #[wasm_bindgen(js_name = correctedStateAtJ2000, unchecked_return_type = "SsrCorrectedState | null")]
    pub fn corrected_state_at_j2000(
        &self,
        satellite: &str,
        epoch_j2000_s: f64,
    ) -> Result<JsValue, JsValue> {
        let satellite = parse_sat(satellite)?;
        let size_refusal =
            self.inner
                .correction_size_refusal(satellite, epoch_j2000_s, epoch_j2000_s);
        let checked = self
            .inner
            .corrected_state_checked(satellite, epoch_j2000_s)
            .map_err(|error| crate::positioning_error::core_source_error(&error))?;
        let Some((position_ecef_m, clock_s)) = checked.value else {
            if self.inner.correction_size_policy() == CoreCorrectionSizePolicy::Strict {
                if let Some(size) = size_refusal {
                    return Err(correction_size_refusal_error(
                        satellite,
                        epoch_j2000_s,
                        epoch_j2000_s,
                        size,
                    ));
                }
            }
            return Ok(JsValue::NULL);
        };
        serde_wasm_bindgen::to_value(&CorrectedStateJs {
            position_ecef_m,
            clock_s,
            ut1_degraded: checked.degraded.map(crate::spp::degrade_reason_label),
        })
        .map_err(engine_error)
    }

    #[wasm_bindgen(js_name = correctedStateAtQueries, unchecked_return_type = "SsrCorrectedState | null")]
    pub fn corrected_state_at_queries(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        selection_epoch: &ExactEpochQueryValue,
    ) -> Result<JsValue, JsValue> {
        let satellite = parse_sat(satellite)?;
        let size_refusal = self.inner.correction_size_refusal_at_epoch_query(
            satellite,
            &state_epoch.core(),
            &selection_epoch.core(),
        );
        let checked = self
            .inner
            .as_borrowed()
            .corrected_state_with_group_delay_checked_selected_query(
                satellite,
                &state_epoch.core(),
                &selection_epoch.core(),
            )
            .map_err(|error| crate::positioning_error::core_source_error(&error))?;
        let Some((position_ecef_m, clock_s, _)) = checked.value else {
            if self.inner.correction_size_policy() == CoreCorrectionSizePolicy::Strict {
                if let Some(size) = size_refusal {
                    return Err(correction_size_refusal_error(
                        satellite,
                        state_epoch.j2000_seconds(),
                        selection_epoch.j2000_seconds(),
                        size,
                    ));
                }
            }
            return Ok(JsValue::NULL);
        };
        serde_wasm_bindgen::to_value(&CorrectedStateJs {
            position_ecef_m,
            clock_s,
            ut1_degraded: checked.degraded.map(crate::spp::degrade_reason_label),
        })
        .map_err(engine_error)
    }

    #[wasm_bindgen(js_name = correctionSizeRefusalAtQueries, unchecked_return_type = "SsrCorrectionSizeRefusalDetail | null")]
    pub fn correction_size_refusal_at_queries(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        selection_epoch: &ExactEpochQueryValue,
    ) -> Result<JsValue, JsValue> {
        let satellite = parse_sat(satellite)?;
        let Some(size) = self.inner.correction_size_refusal_at_epoch_query(
            satellite,
            &state_epoch.core(),
            &selection_epoch.core(),
        ) else {
            return Ok(JsValue::NULL);
        };
        let refusal = SsrCorrectionSizeRefusalJs {
            satellite: satellite.to_string(),
            epoch_j2000_s: state_epoch.j2000_seconds(),
            selection_epoch_j2000_s: selection_epoch.j2000_seconds(),
            size: size.into(),
        };
        serde_wasm_bindgen::to_value(&refusal).map_err(engine_error)
    }

    /// Checked exact-epoch corrected position, clock and group delay.
    #[wasm_bindgen(
        js_name = correctedStateWithGroupDelayAtQueries,
        unchecked_return_type = "SsrCorrectedStateWithGroupDelayChecked | null"
    )]
    pub fn corrected_state_with_group_delay_at_queries(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        selection_epoch: &ExactEpochQueryValue,
    ) -> Result<JsValue, JsValue> {
        crate::sp3::selected_position_clock_at_queries(
            &self.inner,
            parse_sat(satellite)?,
            state_epoch,
            selection_epoch,
        )
    }

    #[wasm_bindgen(js_name = selectedPositionClockAtQueries)]
    pub fn selected_position_clock_at_queries(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        selection_epoch: &ExactEpochQueryValue,
    ) -> Result<JsValue, JsValue> {
        crate::sp3::selected_position_clock_at_queries(
            &self.inner,
            parse_sat(satellite)?,
            state_epoch,
            selection_epoch,
        )
    }
    #[wasm_bindgen(js_name = transmitEpochClockAtQueries)]
    pub fn transmit_epoch_clock_at_queries(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        selection_epoch: &ExactEpochQueryValue,
    ) -> Result<JsValue, JsValue> {
        crate::sp3::transmit_epoch_clock_at_queries(
            &self.inner,
            parse_sat(satellite)?,
            state_epoch,
            selection_epoch,
        )
    }
    #[wasm_bindgen(js_name = ephemerisVarianceAtQueries)]
    pub fn ephemeris_variance_at_queries(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        selection_epoch: &ExactEpochQueryValue,
    ) -> Result<f64, JsValue> {
        Ok(crate::sp3::precise_variance_at_queries(
            &self.inner,
            parse_sat(satellite)?,
            state_epoch,
            selection_epoch,
        ))
    }
    #[wasm_bindgen(js_name = clockRelativityAtQuery)]
    pub fn clock_relativity_at_query(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        position_ecef_m: Vec<f64>,
    ) -> Result<JsValue, JsValue> {
        let position: [f64; 3] = position_ecef_m
            .try_into()
            .map_err(|_| type_error("positionEcefM must contain exactly three coordinates"))?;
        crate::sp3::precise_clock_relativity_at_query(
            &self.inner,
            parse_sat(satellite)?,
            state_epoch,
            position,
        )
    }
    #[wasm_bindgen(getter, js_name = ut1Departure)]
    pub fn ut1_departure(&self) -> Option<String> {
        self.inner
            .ut1_departure()
            .map(crate::spp::degrade_reason_label)
            .map(str::to_owned)
    }
    #[wasm_bindgen(getter, js_name = correctionSizePolicy)]
    pub fn correction_size_policy(&self) -> SsrCorrectionSizePolicy {
        match self.inner.correction_size_policy() {
            CoreCorrectionSizePolicy::Strict => SsrCorrectionSizePolicy::Strict,
            CoreCorrectionSizePolicy::Lenient => SsrCorrectionSizePolicy::Lenient,
        }
    }
    #[wasm_bindgen(getter, js_name = oversizedCorrections, unchecked_return_type = "SsrOversizedCorrection[]")]
    pub fn oversized_corrections(&self) -> Result<JsValue, JsValue> {
        let rows: Vec<SsrOversizedCorrectionJs> = self
            .inner
            .oversized_corrections()
            .into_iter()
            .map(Into::into)
            .collect();
        serde_wasm_bindgen::to_value(&rows).map_err(engine_error)
    }
}
