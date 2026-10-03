use std::cell::RefCell;

use js_sys::{Array, Object, Reflect};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use sidereon_core::astro::time::model::{GnssWeekTow, TimeScale};
use sidereon_core::astro::time::ValidityMode;
use sidereon_core::positioning::EphemerisSource;
use sidereon_core::rtcm::{decode_frame, Message, SsrKind, SsrMessage};
use sidereon_core::ssr::{
    MissingCorrectionAction, SsrCorrectedEphemeris, SsrCorrectionSize,
    SsrCorrectionSizePolicy as CoreCorrectionSizePolicy,
    SsrCorrectionStore as CoreSsrCorrectionStore, SsrFallbackPolicy, SsrNavigationMessage,
    SsrOversizedCorrection, SsrSource as CoreSsrSource,
};
use sidereon_core::GnssSatelliteId;

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

/// `("rtcm", None)` for an RTCM SSR correction, `("has", Some(index))` for a
/// Galileo HAS correction with its navigation-message index as transmitted.
fn nav_message_parts(message: SsrNavigationMessage) -> (&'static str, Option<u8>) {
    match message {
        SsrNavigationMessage::Rtcm => ("rtcm", None),
        SsrNavigationMessage::Has(index) => ("has", Some(index)),
        SsrNavigationMessage::IgsSsr => ("igsSsr", None),
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
    /// The Galileo HAS navigation-message index as transmitted, or `null`.
    has_nav_message_index: Option<u8>,
    iode: u32,
    iod_ssr: u8,
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
        Self::new()
    }
}

#[wasm_bindgen]
impl SsrCorrectionStore {
    #[wasm_bindgen(constructor)]
    pub fn new() -> SsrCorrectionStore {
        SsrCorrectionStore {
            inner: CoreSsrCorrectionStore::new(),
            size_policy: SsrCorrectionSizePolicy::Strict,
            oversized: RefCell::new(Vec::new()),
        }
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
            iod_ssr: orbit.iod_ssr,
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
    let mut eph = SsrCorrectedEphemeris::new(&broadcast.inner, store.core())
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
    let mut eph = SsrCorrectedEphemeris::new(&broadcast.inner, store.core())
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
) -> Result<SsrCorrectedEphemeris<'a>, JsValue> {
    let fallback = SsrFallbackPolicy {
        on_missing_correction: if fallback_to_broadcast.unwrap_or(false) {
            MissingCorrectionAction::FallBackToBroadcast
        } else {
            MissingCorrectionAction::Decline
        },
        ..Default::default()
    };
    let mut source = SsrCorrectedEphemeris::new(&broadcast.inner, store.core())
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
