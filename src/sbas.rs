use serde::Serialize;
use wasm_bindgen::prelude::*;

use sidereon_core::astro::time::model::{GnssWeekTow, TimeScale};
use sidereon_core::frame::Wgs84Geodetic;
use sidereon_core::positioning::EphemerisSource;
use sidereon_core::sbas::message::{SbasBlock as CoreSbasBlock, SbasWireForm};
use sidereon_core::sbas::source::{SbasCorrectedEphemeris, SbasSolveMode};
use sidereon_core::sbas::store::SbasCorrectionStore as CoreSbasCorrectionStore;
use sidereon_core::GnssSatelliteId;

use crate::error::{engine_error, type_error};
use crate::rinex_nav::BroadcastEphemeris;
use crate::spp::{self, SppSolution};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SbasMessageJs {
    message_type: u8,
    form: &'static str,
    kind: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CorrectedStateJs {
    position_ecef_m: [f64; 3],
    clock_s: f64,
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

#[wasm_bindgen(js_name = decodeSbasMessage)]
pub fn decode_sbas_message(bytes: &[u8], form: Option<String>) -> Result<JsValue, JsValue> {
    let block = decode_block(bytes, form)?;
    let out = SbasMessageJs {
        message_type: block.message.message_type(),
        form: form_label(block.form),
        kind: format!("{:?}", block.message),
    };
    serde_wasm_bindgen::to_value(&out).map_err(|e| type_error(&e.to_string()))
}

#[wasm_bindgen]
pub struct SbasCorrectionStore {
    inner: CoreSbasCorrectionStore,
}

#[wasm_bindgen]
impl SbasCorrectionStore {
    #[wasm_bindgen(constructor)]
    pub fn new() -> SbasCorrectionStore {
        SbasCorrectionStore {
            inner: CoreSbasCorrectionStore::new(),
        }
    }

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

    #[wasm_bindgen(js_name = readyGeos)]
    pub fn ready_geos(&self, t_j2000_s: f64) -> Vec<String> {
        self.inner
            .ready_geos(t_j2000_s)
            .into_iter()
            .map(|sat| sat.to_string())
            .collect()
    }

    #[wasm_bindgen(js_name = ionoSlantDelayM)]
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
