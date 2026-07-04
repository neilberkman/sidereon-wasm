use serde::Serialize;
use wasm_bindgen::prelude::*;

use sidereon_core::astro::time::model::{GnssWeekTow, TimeScale};
use sidereon_core::frame::Wgs84Geodetic;
use sidereon_core::positioning::EphemerisSource;
use sidereon_core::sbas::message::{
    SbasBlock as CoreSbasBlock, SbasDoNotUse, SbasFastCorrections, SbasFastDegradation,
    SbasGeoAlmanac, SbasGeoNav, SbasIgpMask, SbasIntegrity, SbasIonoDelays,
    SbasLongTermCorrections, SbasLongTermHalf, SbasLongTermRecord, SbasMessage,
    SbasMixedCorrections, SbasNetworkTime, SbasPrnMask, SbasUnsupported, SbasWireForm, SpareBits,
};
use sidereon_core::sbas::source::{SbasCorrectedEphemeris, SbasSolveMode};
use sidereon_core::sbas::store::{
    sat_to_sbas_prn as core_sat_to_sbas_prn, sbas_prn_to_sat as core_sbas_prn_to_sat,
    SbasCorrectionStore as CoreSbasCorrectionStore, SbasFastCorrection, SbasGeoState, SbasIgp,
    SbasIonoGrid, SbasLongTermCorrection,
};
use sidereon_core::staleness::StalenessPolicy;
use sidereon_core::GnssSatelliteId;

use crate::error::{engine_error, range_error, type_error};
use crate::rinex_nav::BroadcastEphemeris;
use crate::spp::{self, SppSolution};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SbasMessageJs {
    message_type: u8,
    form: &'static str,
    kind: String,
    message: serde_json::Value,
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

/// Decode a raw SBAS message.
///
/// `form` is `"framed250"` for a 32-byte message with CRC or `"body226"` for a
/// 29-byte body. The result contains `messageType`, `form`, legacy debug
/// `kind`, and `message`, a structured decoded payload. Parse failures are
/// thrown as `Error`.
#[wasm_bindgen(js_name = decodeSbasMessage)]
pub fn decode_sbas_message(bytes: &[u8], form: Option<String>) -> Result<JsValue, JsValue> {
    let block = decode_block(bytes, form)?;
    let out = SbasMessageJs {
        message_type: block.message.message_type(),
        form: form_label(block.form),
        kind: format!("{:?}", block.message),
        message: message_payload(&block.message),
    };
    to_js(&out)
}

#[wasm_bindgen]
/// Mutable SBAS correction store.
///
/// Ingest raw SBAS messages with a source GEO and GNSS time, then query decoded
/// fast, long-term, ionospheric, and GEO navigation correction records.
pub struct SbasCorrectionStore {
    inner: CoreSbasCorrectionStore,
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

/// Convert an SBAS broadcast PRN number such as `129` to an SBAS satellite
/// token such as `"S29"`. Returns `null` when the PRN is outside the SBAS range.
#[wasm_bindgen(js_name = sbasPrnToSat)]
pub fn sbas_prn_to_sat(broadcast_prn: u16) -> JsValue {
    core_sbas_prn_to_sat(broadcast_prn)
        .map(|sat| JsValue::from_str(&sat.to_string()))
        .unwrap_or(JsValue::NULL)
}

/// Convert an SBAS satellite token such as `"S29"` to broadcast PRN number
/// such as `129`. Returns `null` for non-SBAS satellites.
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
    .map_err(engine_error)?;
    Ok(SppSolution { inner: solution })
}
