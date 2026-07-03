//! Space-weather table parsing, lookup, and sourced decay.
//!
//! The binding stores the parsed core table and forwards all lookup and decay
//! work to `sidereon_core`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::astro::forces::SpaceWeatherSource;
use sidereon_core::astro::propagator::decay::{
    estimate_decay_with_source as core_estimate_decay_with_source, DecayConfig,
};
use sidereon_core::astro::propagator::driver::PropagationForceModel;
use sidereon_core::astro::propagator::numerical::IntegratorKind;
use sidereon_core::astro::space_weather::{
    parse as core_parse, parse_csv as core_parse_csv, parse_txt as core_parse_txt, Diagnostics,
    ObservationClass, SpaceWeatherDay, SpaceWeatherPolicy,
    SpaceWeatherTable as CoreSpaceWeatherTable,
};
use sidereon_core::astro::state::CartesianState;

use crate::error::{engine_error, type_error};
use crate::forces::DragForce;
use crate::marshal::vec3_finite;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordRefJs {
    line: Option<usize>,
    record_index: Option<usize>,
    satellite: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticEntryJs {
    at: RecordRefJs,
    reason: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticsJs {
    skip_count: usize,
    warning_count: usize,
    skips: Vec<DiagnosticEntryJs>,
    warnings: Vec<DiagnosticEntryJs>,
}

fn diagnostics_js(diagnostics: &Diagnostics) -> DiagnosticsJs {
    DiagnosticsJs {
        skip_count: diagnostics.skips.len(),
        warning_count: diagnostics.warnings.len(),
        skips: diagnostics
            .skips
            .iter()
            .map(|skip| DiagnosticEntryJs {
                at: RecordRefJs {
                    line: skip.at.line,
                    record_index: skip.at.record_index,
                    satellite: skip.at.satellite.clone(),
                },
                reason: format!("{:?}", skip.reason),
            })
            .collect(),
        warnings: diagnostics
            .warnings
            .iter()
            .map(|warning| DiagnosticEntryJs {
                at: RecordRefJs {
                    line: warning.at.line,
                    record_index: warning.at.record_index,
                    satellite: warning.at.satellite.clone(),
                },
                reason: format!("{:?}", warning.kind),
            })
            .collect(),
    }
}

fn to_value<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(value).map_err(|e| type_error(&e.to_string()))
}

fn class_label(class: ObservationClass) -> &'static str {
    match class {
        ObservationClass::Observed => "observed",
        ObservationClass::Interpolated => "interpolated",
        ObservationClass::DailyPredicted => "dailyPredicted",
        ObservationClass::MonthlyPredicted => "monthlyPredicted",
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SpaceWeatherDayJs {
    year: i32,
    month: u8,
    day: u8,
    class: &'static str,
    bsrn: Option<u16>,
    nd: Option<u8>,
    kp: Vec<Option<f64>>,
    kp_sum: Option<f64>,
    ap: Vec<Option<u16>>,
    ap_avg: Option<u16>,
    cp: Option<f64>,
    c9: Option<u8>,
    isn: Option<u16>,
    flux_qualifier: Option<u8>,
    f107_obs: Option<f64>,
    f107_adj: Option<f64>,
    f107_obs_center81: Option<f64>,
    f107_obs_last81: Option<f64>,
    f107_adj_center81: Option<f64>,
    f107_adj_last81: Option<f64>,
}

impl From<&SpaceWeatherDay> for SpaceWeatherDayJs {
    fn from(day: &SpaceWeatherDay) -> Self {
        Self {
            year: day.year,
            month: day.month,
            day: day.day,
            class: class_label(day.class),
            bsrn: day.bsrn,
            nd: day.nd,
            kp: (0..8).map(|index| day.kp(index)).collect(),
            kp_sum: day.kp_sum_10.map(|value| f64::from(value) / 10.0),
            ap: day.ap.to_vec(),
            ap_avg: day.ap_avg,
            cp: day.cp(),
            c9: day.c9,
            isn: day.isn,
            flux_qualifier: day.flux_qualifier,
            f107_obs: day.f107_obs,
            f107_adj: day.f107_adj,
            f107_obs_center81: day.f107_obs_center81,
            f107_obs_last81: day.f107_obs_last81,
            f107_adj_center81: day.f107_adj_center81,
            f107_adj_last81: day.f107_adj_last81,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CoverageJs {
    first_j2000_s: f64,
    last_observed_j2000_s: Option<f64>,
    last_daily_predicted_j2000_s: Option<f64>,
    end_j2000_s: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SpaceWeatherValuesJs {
    f107: f64,
    f107a: f64,
    ap: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SpaceWeatherSampleJs {
    f107: f64,
    f107a: f64,
    ap: f64,
    class: &'static str,
    ap_defaulted: bool,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct PolicyInput {
    allow_interpolated: Option<bool>,
    allow_daily_predicted: Option<bool>,
    allow_monthly_predicted: Option<bool>,
    require_geomagnetic: Option<bool>,
}

impl PolicyInput {
    fn to_core(&self) -> SpaceWeatherPolicy {
        let defaults = SpaceWeatherPolicy::default();
        SpaceWeatherPolicy {
            allow_interpolated: self
                .allow_interpolated
                .unwrap_or(defaults.allow_interpolated),
            allow_daily_predicted: self
                .allow_daily_predicted
                .unwrap_or(defaults.allow_daily_predicted),
            allow_monthly_predicted: self
                .allow_monthly_predicted
                .unwrap_or(defaults.allow_monthly_predicted),
            require_geomagnetic: self
                .require_geomagnetic
                .unwrap_or(defaults.require_geomagnetic),
        }
    }
}

/// Parsed CelesTrak CSSI space-weather table.
#[wasm_bindgen]
pub struct SpaceWeatherTable {
    inner: Arc<CoreSpaceWeatherTable>,
    diagnostics: Diagnostics,
}

impl SpaceWeatherTable {
    fn new(inner: CoreSpaceWeatherTable, diagnostics: Diagnostics) -> Self {
        Self {
            inner: Arc::new(inner),
            diagnostics,
        }
    }
}

#[wasm_bindgen]
impl SpaceWeatherTable {
    #[wasm_bindgen(getter, js_name = dayCount)]
    pub fn day_count(&self) -> usize {
        self.inner.days().len()
    }

    #[wasm_bindgen(getter, js_name = monthlyCount)]
    pub fn monthly_count(&self) -> usize {
        self.inner.monthly().len()
    }

    #[wasm_bindgen(getter)]
    pub fn diagnostics(&self) -> Result<JsValue, JsValue> {
        to_value(&diagnostics_js(&self.diagnostics))
    }

    #[wasm_bindgen(getter)]
    pub fn coverage(&self) -> Result<JsValue, JsValue> {
        let coverage = self.inner.coverage();
        to_value(&CoverageJs {
            first_j2000_s: coverage.first_j2000_s,
            last_observed_j2000_s: coverage.last_observed_j2000_s,
            last_daily_predicted_j2000_s: coverage.last_daily_predicted_j2000_s,
            end_j2000_s: coverage.end_j2000_s,
        })
    }

    #[wasm_bindgen(js_name = day)]
    pub fn day(&self, year: i32, month: u8, day: u8) -> Result<JsValue, JsValue> {
        self.inner
            .day(year, month, day)
            .map(SpaceWeatherDayJs::from)
            .map_or(Ok(JsValue::UNDEFINED), |value| to_value(&value))
    }

    #[wasm_bindgen(js_name = days)]
    pub fn days(&self) -> Result<JsValue, JsValue> {
        let days: Vec<_> = self
            .inner
            .days()
            .iter()
            .map(SpaceWeatherDayJs::from)
            .collect();
        to_value(&days)
    }

    #[wasm_bindgen(js_name = monthly)]
    pub fn monthly(&self) -> Result<JsValue, JsValue> {
        let days: Vec<_> = self
            .inner
            .monthly()
            .iter()
            .map(SpaceWeatherDayJs::from)
            .collect();
        to_value(&days)
    }

    #[wasm_bindgen(js_name = spaceWeatherAt)]
    pub fn space_weather_at(&self, epoch_j2000_s: f64) -> Result<JsValue, JsValue> {
        let values = self
            .inner
            .space_weather_at(epoch_j2000_s)
            .map_err(engine_error)?;
        to_value(&SpaceWeatherValuesJs {
            f107: values.f107,
            f107a: values.f107a,
            ap: values.ap,
        })
    }

    #[wasm_bindgen(js_name = sampleAt)]
    pub fn sample_at(&self, epoch_j2000_s: f64, policy: JsValue) -> Result<JsValue, JsValue> {
        let policy = if policy.is_null() || policy.is_undefined() {
            SpaceWeatherPolicy::default()
        } else {
            let input: PolicyInput = serde_wasm_bindgen::from_value(policy)
                .map_err(|e| type_error(&format!("invalid space-weather policy: {e}")))?;
            input.to_core()
        };
        let sample = self
            .inner
            .sample_at_with_policy(epoch_j2000_s, policy)
            .map_err(engine_error)?;
        to_value(&SpaceWeatherSampleJs {
            f107: sample.space_weather.f107,
            f107a: sample.space_weather.f107a,
            ap: sample.space_weather.ap,
            class: class_label(sample.class),
            ap_defaulted: sample.ap_defaulted,
        })
    }

    #[wasm_bindgen(js_name = apArrayAt)]
    pub fn ap_array_at(&self, epoch_j2000_s: f64) -> Result<Vec<f64>, JsValue> {
        self.inner
            .ap_array_at(epoch_j2000_s)
            .map(|array| array.to_vec())
            .map_err(engine_error)
    }

    #[wasm_bindgen(js_name = toCsv)]
    pub fn to_csv(&self) -> String {
        sidereon_core::astro::space_weather::encode_csv(&self.inner)
    }

    #[wasm_bindgen(js_name = toTxt)]
    pub fn to_txt(&self) -> String {
        sidereon_core::astro::space_weather::encode_txt(&self.inner)
    }
}

/// Parse either CSV or fixed-width TXT space-weather bytes.
#[wasm_bindgen(js_name = parseSpaceWeather)]
pub fn parse_space_weather(bytes: &[u8]) -> Result<SpaceWeatherTable, JsValue> {
    let parsed = core_parse(bytes).map_err(engine_error)?;
    Ok(SpaceWeatherTable::new(parsed.value, parsed.diagnostics))
}

/// Parse CelesTrak CSSI CSV space-weather text.
#[wasm_bindgen(js_name = parseSpaceWeatherCsv)]
pub fn parse_space_weather_csv(text: &str) -> Result<SpaceWeatherTable, JsValue> {
    let parsed = core_parse_csv(text).map_err(engine_error)?;
    Ok(SpaceWeatherTable::new(parsed.value, parsed.diagnostics))
}

/// Parse CelesTrak CSSI fixed-width TXT space-weather text.
#[wasm_bindgen(js_name = parseSpaceWeatherTxt)]
pub fn parse_space_weather_txt(text: &str) -> Result<SpaceWeatherTable, JsValue> {
    let parsed = core_parse_txt(text).map_err(engine_error)?;
    Ok(SpaceWeatherTable::new(parsed.value, parsed.diagnostics))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DecayRequest {
    epoch_s: f64,
    position_km: Vec<f64>,
    velocity_km_s: Vec<f64>,
    #[serde(default)]
    force_model: Option<String>,
    #[serde(default)]
    integrator: Option<String>,
    #[serde(default)]
    reentry_altitude_km: Option<f64>,
    #[serde(default)]
    scan_step_s: Option<f64>,
    #[serde(default)]
    crossing_tolerance_s: Option<f64>,
    #[serde(default)]
    max_duration_s: Option<f64>,
    #[serde(default)]
    max_scan_samples: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DecayEstimateJs {
    time_to_decay_s: f64,
    reentry_epoch_s: f64,
    reentry_position_km: [f64; 3],
    reentry_velocity_km_s: [f64; 3],
    reentry_altitude_km: f64,
}

fn force_model(label: Option<&str>) -> Result<PropagationForceModel, JsValue> {
    match label.unwrap_or("two_body") {
        "two_body" => Ok(PropagationForceModel::TwoBody),
        "two_body_j2" => Ok(PropagationForceModel::TwoBodyJ2),
        other => Err(type_error(&format!(
            "invalid forceModel {other:?}: expected \"two_body\" or \"two_body_j2\""
        ))),
    }
}

fn integrator(label: Option<&str>) -> Result<IntegratorKind, JsValue> {
    match label.unwrap_or("dp54") {
        "dp54" => Ok(IntegratorKind::Dp54),
        "rk4" => Ok(IntegratorKind::Rk4),
        other => Err(type_error(&format!(
            "invalid integrator {other:?}: expected \"dp54\" or \"rk4\""
        ))),
    }
}

/// Estimate decay using per-epoch values from a parsed space-weather table.
#[wasm_bindgen(js_name = estimateDecayWithSpaceWeather)]
pub fn estimate_decay_with_space_weather(
    drag: &DragForce,
    table: &SpaceWeatherTable,
    request: JsValue,
) -> Result<JsValue, JsValue> {
    let req: DecayRequest = serde_wasm_bindgen::from_value(request)
        .map_err(|e| type_error(&format!("invalid decay request: {e}")))?;
    let position = vec3_finite("positionKm", &req.position_km)?;
    let velocity = vec3_finite("velocityKmS", &req.velocity_km_s)?;
    let mut config = DecayConfig::new(drag.parameters()?)
        .with_force_model(force_model(req.force_model.as_deref())?)
        .with_integrator(integrator(req.integrator.as_deref())?);
    if let Some(value) = req.reentry_altitude_km {
        config = config.with_reentry_altitude_km(value);
    }
    if let Some(value) = req.scan_step_s {
        config = config.with_scan_step_s(value);
    }
    if let Some(value) = req.crossing_tolerance_s {
        config = config.with_crossing_tolerance_s(value);
    }
    if let Some(value) = req.max_duration_s {
        config = config.with_max_duration_s(value);
    }
    if let Some(value) = req.max_scan_samples {
        config = config.with_max_scan_samples(value);
    }

    let source = SpaceWeatherSource::Table(table.inner.clone());
    let estimate = core_estimate_decay_with_source(
        CartesianState::new(req.epoch_s, position, velocity),
        &config,
        &source,
    )
    .map_err(engine_error)?;
    to_value(&DecayEstimateJs {
        time_to_decay_s: estimate.time_to_decay_s,
        reentry_epoch_s: estimate.reentry_state.epoch_tdb_seconds,
        reentry_position_km: estimate.reentry_state.position_array(),
        reentry_velocity_km_s: estimate.reentry_state.velocity_array(),
        reentry_altitude_km: estimate.reentry_altitude_km,
    })
}
