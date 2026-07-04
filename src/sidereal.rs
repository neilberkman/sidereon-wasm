//! Sidereal repeat diagnostics and residual filtering.
//!
//! The exported functions decode plain JS payloads, call
//! `sidereon_core::sidereal`, and serialize the core output without adding
//! filtering logic in the binding.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::astro::time::Duration;
use sidereon_core::sidereal::{
    orbit_repeat_lag as core_orbit_repeat_lag, periodicity_strength_with_sample_interval,
    repeat_period as core_repeat_period, sidereal_filter as core_sidereal_filter,
    SiderealFilterOptions as CoreSiderealFilterOptions,
    SiderealTemplateMethod as CoreSiderealTemplateMethod,
};
use sidereon_core::GnssSatelliteId;

use crate::error::{engine_error, range_error, type_error};
use crate::gnss::GnssSystem;
use crate::rinex_nav::BroadcastEphemeris;

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| engine_error(format!("failed to serialize result: {e}")))
}

fn duration_from_seconds(value: f64, field: &str) -> Result<Duration, JsValue> {
    Duration::from_seconds(value).map_err(|e| range_error(&format!("{field}: {e}")))
}

fn parse_satellite(token: &str) -> Result<GnssSatelliteId, JsValue> {
    token
        .parse::<GnssSatelliteId>()
        .map_err(|e| type_error(&format!("invalid satellite token {token:?}: {e}")))
}

#[derive(Deserialize)]
#[serde(untagged)]
enum TemplateMethodInput {
    Label(String),
    Object(TemplateMethodObjectInput),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TemplateMethodObjectInput {
    method: String,
    #[serde(default)]
    alpha: Option<f64>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct SiderealFilterOptionsInput {
    sample_interval_s: Option<f64>,
    prior_periods: Option<usize>,
    min_coverage: Option<usize>,
    template_method: Option<TemplateMethodInput>,
}

fn template_method(
    input: Option<TemplateMethodInput>,
) -> Result<CoreSiderealTemplateMethod, JsValue> {
    match input {
        None => Ok(CoreSiderealTemplateMethod::Mean),
        Some(TemplateMethodInput::Label(label)) => match label.as_str() {
            "mean" => Ok(CoreSiderealTemplateMethod::Mean),
            "robustMad" | "robust_mad" => Ok(CoreSiderealTemplateMethod::RobustMad),
            other => Err(type_error(&format!(
                "invalid sidereal template method {other:?}: expected \"mean\" or \"robustMad\""
            ))),
        },
        Some(TemplateMethodInput::Object(object)) => match object.method.as_str() {
            "ewma" => Ok(CoreSiderealTemplateMethod::Ewma {
                alpha: object
                    .alpha
                    .ok_or_else(|| type_error("ewma template method requires alpha"))?,
            }),
            "mean" => Ok(CoreSiderealTemplateMethod::Mean),
            "robustMad" | "robust_mad" => Ok(CoreSiderealTemplateMethod::RobustMad),
            other => Err(type_error(&format!(
                "invalid sidereal template method {other:?}: expected \"mean\", \"robustMad\", or \"ewma\""
            ))),
        },
    }
}

fn filter_options(input: SiderealFilterOptionsInput) -> Result<CoreSiderealFilterOptions, JsValue> {
    let defaults = CoreSiderealFilterOptions::default();
    Ok(CoreSiderealFilterOptions {
        sample_interval: match input.sample_interval_s {
            Some(value) => duration_from_seconds(value, "sampleIntervalS")?,
            None => defaults.sample_interval,
        },
        prior_periods: input.prior_periods.unwrap_or(defaults.prior_periods),
        min_coverage: input.min_coverage.unwrap_or(defaults.min_coverage),
        template_method: template_method(input.template_method)?,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SiderealFilterOutputJs {
    filtered: Vec<f64>,
    template: Vec<f64>,
    coverage: Vec<usize>,
    under_covered: Vec<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PeriodicityStrengthJs {
    period_s: f64,
    strength: f64,
}

/// Default ground-track repeat period for a GNSS constellation, in seconds.
#[wasm_bindgen(js_name = repeatPeriod)]
pub fn repeat_period(system: GnssSystem) -> f64 {
    core_repeat_period(system.into()).as_seconds()
}

/// Broadcast-derived per-satellite orbit-repeat lag, in seconds.
///
/// `satellite` is an IGS token such as `"G01"`, and `nearEpochJ2000S` is the
/// broadcast-selection epoch in seconds since J2000.
#[wasm_bindgen(js_name = orbitRepeatLag)]
pub fn orbit_repeat_lag(
    ephemeris: &BroadcastEphemeris,
    satellite: &str,
    near_epoch_j2000_s: f64,
) -> Result<f64, JsValue> {
    let sat = parse_satellite(satellite)?;
    core_orbit_repeat_lag(&ephemeris.inner, sat, near_epoch_j2000_s)
        .map(|duration| duration.as_seconds())
        .map_err(engine_error)
}

/// Apply a sidereal residual filter to a scalar series.
///
/// `series` is a JS number array, `periodS` is seconds, and `options` may set
/// `sampleIntervalS`, `priorPeriods`, `minCoverage`, and `templateMethod`.
/// Returns `{ filtered, template, coverage, underCovered }`.
#[wasm_bindgen(js_name = siderealFilter)]
pub fn sidereal_filter(
    series: JsValue,
    period_s: f64,
    options: JsValue,
) -> Result<JsValue, JsValue> {
    let series: Vec<f64> = serde_wasm_bindgen::from_value(series)
        .map_err(|e| type_error(&format!("invalid sidereal series: {e}")))?;
    let options: SiderealFilterOptionsInput = if options.is_undefined() || options.is_null() {
        SiderealFilterOptionsInput::default()
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid sidereal options: {e}")))?
    };
    let output = core_sidereal_filter(
        &series,
        duration_from_seconds(period_s, "periodS")?,
        filter_options(options)?,
    )
    .map_err(engine_error)?;
    to_js(&SiderealFilterOutputJs {
        filtered: output.filtered,
        template: output.template,
        coverage: output.coverage,
        under_covered: output.under_covered,
    })
}

/// Score repeating components at candidate periods.
///
/// `series` and `candidatePeriodsS` are JS number arrays. `sampleIntervalS`
/// defaults to one second. The result is an array of `{ periodS, strength }`.
#[wasm_bindgen(js_name = periodicityStrength)]
pub fn periodicity_strength(
    series: JsValue,
    candidate_periods_s: JsValue,
    sample_interval_s: Option<f64>,
) -> Result<JsValue, JsValue> {
    let series: Vec<f64> = serde_wasm_bindgen::from_value(series)
        .map_err(|e| type_error(&format!("invalid periodicity series: {e}")))?;
    let candidate_periods_s: Vec<f64> = serde_wasm_bindgen::from_value(candidate_periods_s)
        .map_err(|e| type_error(&format!("invalid candidate periods: {e}")))?;
    let candidate_periods = candidate_periods_s
        .iter()
        .map(|&period_s| duration_from_seconds(period_s, "candidatePeriodsS"))
        .collect::<Result<Vec<_>, _>>()?;
    let sample_interval =
        duration_from_seconds(sample_interval_s.unwrap_or(1.0), "sampleIntervalS")?;
    let scores =
        periodicity_strength_with_sample_interval(&series, &candidate_periods, sample_interval)
            .map_err(engine_error)?;
    let output = scores
        .into_iter()
        .map(|(period, strength)| PeriodicityStrengthJs {
            period_s: period.as_seconds(),
            strength,
        })
        .collect::<Vec<_>>();
    to_js(&output)
}
