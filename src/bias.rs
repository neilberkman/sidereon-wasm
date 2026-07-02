use std::io::Read;
use std::str::FromStr;

use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::astro::time::model::{Instant, JulianDateSplit, TimeScale};
use sidereon_core::bias::{BiasRecord, BiasSet as CoreBiasSet, CodeDcbOptions};
use sidereon_core::constants::{J2000_JD, SECONDS_PER_DAY};
use sidereon_core::{GnssSatelliteId, GnssSystem};

use crate::error::{engine_error, type_error};

fn maybe_gzip(bytes: &[u8]) -> Result<Vec<u8>, JsValue> {
    if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut decoder = GzDecoder::new(bytes);
        let mut out = Vec::new();
        decoder
            .read_to_end(&mut out)
            .map_err(|e| engine_error(format!("gzip decode failed: {e}")))?;
        Ok(out)
    } else {
        Ok(bytes.to_vec())
    }
}

fn parse_sat(token: &str) -> Result<GnssSatelliteId, JsValue> {
    GnssSatelliteId::from_str(token)
        .map_err(|e| type_error(&format!("invalid satellite token {token:?}: {e}")))
}

fn parse_system(value: &str) -> Result<GnssSystem, JsValue> {
    match value {
        "gps" => Ok(GnssSystem::Gps),
        "glonass" => Ok(GnssSystem::Glonass),
        "galileo" => Ok(GnssSystem::Galileo),
        "beidou" => Ok(GnssSystem::BeiDou),
        "qzss" => Ok(GnssSystem::Qzss),
        "navic" => Ok(GnssSystem::Navic),
        "sbas" => Ok(GnssSystem::Sbas),
        other => Err(type_error(&format!("invalid GNSS system label {other:?}"))),
    }
}

fn parse_time_scale(value: Option<&str>) -> Result<TimeScale, JsValue> {
    match value.unwrap_or("gpst") {
        "utc" => Ok(TimeScale::Utc),
        "tai" => Ok(TimeScale::Tai),
        "tt" => Ok(TimeScale::Tt),
        "tdb" => Ok(TimeScale::Tdb),
        "gpst" => Ok(TimeScale::Gpst),
        "gst" => Ok(TimeScale::Gst),
        "bdt" => Ok(TimeScale::Bdt),
        "glonasst" => Ok(TimeScale::Glonasst),
        "qzsst" => Ok(TimeScale::Qzsst),
        other => Err(type_error(&format!("invalid time scale {other:?}"))),
    }
}

fn epoch_from_j2000(epoch_j2000_s: f64, scale: TimeScale) -> Result<Instant, JsValue> {
    let days = epoch_j2000_s / SECONDS_PER_DAY;
    let whole = J2000_JD + days.floor();
    let fraction = days - days.floor();
    let split = JulianDateSplit::new(whole, fraction).map_err(engine_error)?;
    Ok(Instant::from_julian_date(scale, split))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeDcbOptionsInput {
    obs1: String,
    obs2: String,
    year: i32,
    month: u8,
    #[serde(default)]
    time_scale: Option<String>,
    #[serde(default)]
    receiver_system: Option<String>,
}

impl CodeDcbOptionsInput {
    fn to_core(&self) -> Result<CodeDcbOptions, JsValue> {
        Ok(CodeDcbOptions {
            pair: (self.obs1.clone(), self.obs2.clone()),
            year: self.year,
            month: self.month,
            time_scale: parse_time_scale(self.time_scale.as_deref())?,
            receiver_system: self
                .receiver_system
                .as_deref()
                .map(parse_system)
                .transpose()?,
        })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BiasRecordJs {
    kind: String,
    target: String,
    obs1: String,
    obs2: Option<String>,
    value: f64,
    sigma: Option<f64>,
    slope: Option<f64>,
    is_phase: bool,
}

fn record_to_js(record: &BiasRecord) -> BiasRecordJs {
    BiasRecordJs {
        kind: format!("{:?}", record.kind).to_ascii_lowercase(),
        target: format!("{:?}", record.target),
        obs1: record.obs1.clone(),
        obs2: record.obs2.clone(),
        value: record.value,
        sigma: record.sigma,
        slope: record.slope,
        is_phase: record.is_phase,
    }
}

#[wasm_bindgen]
pub struct BiasSet {
    inner: CoreBiasSet,
}

impl BiasSet {
    pub(crate) fn core(&self) -> CoreBiasSet {
        self.inner.clone()
    }
}

#[wasm_bindgen]
impl BiasSet {
    #[wasm_bindgen(getter, js_name = recordCount)]
    pub fn record_count(&self) -> usize {
        self.inner.records().len()
    }

    #[wasm_bindgen(getter, js_name = skippedRecords)]
    pub fn skipped_records(&self) -> usize {
        self.inner.skipped_records()
    }

    #[wasm_bindgen(getter)]
    pub fn records(&self) -> Result<JsValue, JsValue> {
        let records: Vec<BiasRecordJs> = self.inner.records().iter().map(record_to_js).collect();
        serde_wasm_bindgen::to_value(&records).map_err(|e| type_error(&e.to_string()))
    }

    #[wasm_bindgen(js_name = codeOsbSeconds)]
    pub fn code_osb_seconds(
        &self,
        sat: &str,
        obs: &str,
        epoch_j2000_s: f64,
        time_scale: Option<String>,
    ) -> Result<Option<f64>, JsValue> {
        let sat = parse_sat(sat)?;
        let epoch = epoch_from_j2000(epoch_j2000_s, parse_time_scale(time_scale.as_deref())?)?;
        Ok(self.inner.code_osb_seconds(sat, obs, epoch))
    }

    #[wasm_bindgen(js_name = phaseOsbCycles)]
    pub fn phase_osb_cycles(
        &self,
        sat: &str,
        obs: &str,
        epoch_j2000_s: f64,
        time_scale: Option<String>,
    ) -> Result<Option<f64>, JsValue> {
        let sat = parse_sat(sat)?;
        let epoch = epoch_from_j2000(epoch_j2000_s, parse_time_scale(time_scale.as_deref())?)?;
        Ok(self.inner.phase_osb_cycles(sat, obs, epoch))
    }

    #[wasm_bindgen(js_name = codeDsbSeconds)]
    pub fn code_dsb_seconds(
        &self,
        sat: &str,
        obs1: &str,
        obs2: &str,
        epoch_j2000_s: f64,
        time_scale: Option<String>,
    ) -> Result<Option<f64>, JsValue> {
        let sat = parse_sat(sat)?;
        let epoch = epoch_from_j2000(epoch_j2000_s, parse_time_scale(time_scale.as_deref())?)?;
        Ok(self.inner.code_dsb_seconds(sat, obs1, obs2, epoch))
    }

    #[wasm_bindgen(js_name = codeBiasModelM)]
    #[allow(clippy::too_many_arguments)]
    pub fn code_bias_model_m(
        &self,
        sat: &str,
        used_obs1: &str,
        used_obs2: &str,
        freq1_hz: f64,
        freq2_hz: f64,
        glonass_channel: Option<i8>,
        clock_ref_obs1: &str,
        clock_ref_obs2: &str,
        epoch_j2000_s: f64,
        time_scale: Option<String>,
    ) -> Result<Option<f64>, JsValue> {
        let sat = parse_sat(sat)?;
        let epoch = epoch_from_j2000(epoch_j2000_s, parse_time_scale(time_scale.as_deref())?)?;
        Ok(self.inner.code_bias_model_m(
            sat,
            (used_obs1, used_obs2),
            (freq1_hz, freq2_hz),
            glonass_channel,
            (clock_ref_obs1, clock_ref_obs2),
            epoch,
        ))
    }
}

fn parse_sinex(bytes: &[u8]) -> Result<BiasSet, JsValue> {
    let bytes = maybe_gzip(bytes)?;
    let parsed = CoreBiasSet::parse_bias_sinex(&bytes).map_err(engine_error)?;
    Ok(BiasSet {
        inner: parsed.value,
    })
}

#[wasm_bindgen(js_name = loadBiasSinex)]
pub fn load_bias_sinex(bytes: &[u8]) -> Result<BiasSet, JsValue> {
    parse_sinex(bytes)
}

#[wasm_bindgen(js_name = loadBiasSinexLossy)]
pub fn load_bias_sinex_lossy(bytes: &[u8]) -> Result<BiasSet, JsValue> {
    parse_sinex(bytes)
}

fn parse_dcb(bytes: &[u8], options: JsValue) -> Result<BiasSet, JsValue> {
    let bytes = maybe_gzip(bytes)?;
    let options = if options.is_undefined() || options.is_null() {
        None
    } else {
        let input: CodeDcbOptionsInput = serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid CODE DCB options: {e}")))?;
        Some(input.to_core()?)
    };
    let parsed = CoreBiasSet::parse_code_dcb(&bytes, options).map_err(engine_error)?;
    Ok(BiasSet {
        inner: parsed.value,
    })
}

#[wasm_bindgen(js_name = loadCodeDcb)]
pub fn load_code_dcb(bytes: &[u8], options: JsValue) -> Result<BiasSet, JsValue> {
    parse_dcb(bytes, options)
}

#[wasm_bindgen(js_name = loadCodeDcbLossy)]
pub fn load_code_dcb_lossy(bytes: &[u8], options: JsValue) -> Result<BiasSet, JsValue> {
    parse_dcb(bytes, options)
}
