use serde::Serialize;
use wasm_bindgen::prelude::*;

use sidereon_core::astro::time::model::{GnssWeekTow, TimeScale};
use sidereon_core::positioning::EphemerisSource;
use sidereon_core::rtcm::{decode_frame, Message, SsrKind, SsrMessage};
use sidereon_core::ssr::{
    MissingCorrectionAction, SsrCorrectedEphemeris, SsrCorrectionStore as CoreSsrCorrectionStore,
    SsrFallbackPolicy, SsrSource as CoreSsrSource,
};
use sidereon_core::GnssSatelliteId;

use crate::error::{engine_error, type_error};
use crate::rinex_nav::BroadcastEphemeris;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CorrectedStateJs {
    position_ecef_m: [f64; 3],
    clock_s: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SsrOrbitJs {
    source: &'static str,
    provider_id: u16,
    solution_id: u8,
    iode: u32,
    iod_ssr: u8,
    radial_m: f64,
    along_m: f64,
    cross_m: f64,
    radial_rate_m_s: f64,
    along_rate_m_s: f64,
    cross_rate_m_s: f64,
    ref_epoch_j2000_s: f64,
    update_interval_s: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SsrClockJs {
    source: &'static str,
    provider_id: u16,
    solution_id: u8,
    iod_ssr: u8,
    c0_m: f64,
    c1_m_s: f64,
    c2_m_s2: f64,
    high_rate_c0_m: Option<f64>,
    ref_epoch_j2000_s: f64,
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
}

impl From<CoreSsrSource> for SsrSource {
    fn from(source: CoreSsrSource) -> Self {
        match source {
            CoreSsrSource::RtcmSsr => Self::RtcmSsr,
            CoreSsrSource::GalileoHas => Self::GalileoHas,
        }
    }
}

impl From<SsrSource> for CoreSsrSource {
    fn from(source: SsrSource) -> Self {
        match source {
            SsrSource::RtcmSsr => Self::RtcmSsr,
            SsrSource::GalileoHas => Self::GalileoHas,
        }
    }
}

fn source_label(source: CoreSsrSource) -> &'static str {
    match source {
        CoreSsrSource::RtcmSsr => "rtcmSsr",
        CoreSsrSource::GalileoHas => "galileoHas",
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
        SsrKind::Vtec => "vtec",
    }
}

fn ssr_to_js(ssr: SsrMessage) -> Result<JsValue, JsValue> {
    let out = SsrMessageJs {
        message_number: ssr.message_number,
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
        }
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
        serde_wasm_bindgen::to_value(&SsrOrbitJs {
            source: source_label(orbit.solution.source),
            provider_id: orbit.solution.provider_id,
            solution_id: orbit.solution.solution_id,
            iode: orbit.iode,
            iod_ssr: orbit.iod_ssr,
            radial_m: orbit.radial_m,
            along_m: orbit.along_m,
            cross_m: orbit.cross_m,
            radial_rate_m_s: orbit.radial_rate_m_s,
            along_rate_m_s: orbit.along_rate_m_s,
            cross_rate_m_s: orbit.cross_rate_m_s,
            ref_epoch_j2000_s: orbit.ref_epoch_j2000_s,
            update_interval_s: orbit.update_interval_s,
        })
        .map_err(|e| type_error(&e.to_string()))
    }

    pub fn clock(&self, sat: &str) -> Result<JsValue, JsValue> {
        let sat = parse_sat(sat)?;
        let Some(clock) = self.inner.clock(sat) else {
            return Ok(JsValue::NULL);
        };
        serde_wasm_bindgen::to_value(&SsrClockJs {
            source: source_label(clock.solution.source),
            provider_id: clock.solution.provider_id,
            solution_id: clock.solution.solution_id,
            iod_ssr: clock.iod_ssr,
            c0_m: clock.c0_m,
            c1_m_s: clock.c1_m_s,
            c2_m_s2: clock.c2_m_s2,
            high_rate_c0_m: clock.high_rate.map(|hr| hr.c0_m),
            ref_epoch_j2000_s: clock.ref_epoch_j2000_s,
            update_interval_s: clock.update_interval_s,
        })
        .map_err(|e| type_error(&e.to_string()))
    }

    #[wasm_bindgen(js_name = uraIndex)]
    pub fn ura_index(&self, sat: &str) -> Result<Option<u8>, JsValue> {
        Ok(self.inner.ura_index(parse_sat(sat)?))
    }
}

impl SsrCorrectionStore {
    pub(crate) fn core(&self) -> &CoreSsrCorrectionStore {
        &self.inner
    }
}

#[wasm_bindgen(js_name = ssrCorrectedState)]
pub fn ssr_corrected_state(
    broadcast: &BroadcastEphemeris,
    store: &SsrCorrectionStore,
    sat: &str,
    t_j2000_s: f64,
    fallback_to_broadcast: Option<bool>,
    allow_regional_provider: Option<u16>,
) -> Result<JsValue, JsValue> {
    let sat = parse_sat(sat)?;
    let fallback = SsrFallbackPolicy {
        on_missing_correction: if fallback_to_broadcast.unwrap_or(false) {
            MissingCorrectionAction::FallBackToBroadcast
        } else {
            MissingCorrectionAction::Decline
        },
        ..Default::default()
    };
    let mut eph =
        SsrCorrectedEphemeris::new(&broadcast.inner, store.core()).with_fallback(fallback);
    if let Some(provider) = allow_regional_provider {
        eph = eph.allow_regional_provider(provider);
    }
    let Some((position_ecef_m, clock_s)) = eph.position_clock_at_j2000_s(sat, t_j2000_s) else {
        return Ok(JsValue::NULL);
    };
    serde_wasm_bindgen::to_value(&CorrectedStateJs {
        position_ecef_m,
        clock_s,
    })
    .map_err(|e| type_error(&e.to_string()))
}
