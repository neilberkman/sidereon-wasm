//! Allan-family receiver clock-stability estimators.
//!
//! The exported functions marshal JS sample arrays into
//! `sidereon_core::clock_stability` and return plain JS result objects. The
//! binding carries no estimator math of its own.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::clock_stability::{
    allan_deviation as core_allan_deviation, compute_allan_deviations as core_compute,
    hadamard_deviation as core_hadamard_deviation, modified_adev as core_modified_adev,
    overlapping_adev as core_overlapping_adev, time_deviation as core_time_deviation,
    AllanDeviationCurves, AllanEstimatorSet, AllanInput, AllanOptions, AllanResult, AllanSeries,
    GapPolicy, TauGrid,
};

use crate::error::{engine_error, range_error, type_error};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AllanSeriesInput {
    kind: String,
    values: Vec<Option<f64>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ComputeInput {
    series: AllanSeriesInput,
    tau0_s: f64,
    #[serde(default)]
    options: AllanOptionsInput,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct AllanOptionsInput {
    estimators: AllanEstimatorSetInput,
    tau_grid: TauGridInput,
    gap_policy: Option<String>,
}

impl Default for AllanOptionsInput {
    fn default() -> Self {
        Self {
            estimators: AllanEstimatorSetInput::default(),
            tau_grid: TauGridInput::Label("octave".to_string()),
            gap_policy: None,
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum AllanEstimatorSetInput {
    Label(String),
    Flags(AllanEstimatorFlagsInput),
}

impl Default for AllanEstimatorSetInput {
    fn default() -> Self {
        Self::Label("standard".to_string())
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct AllanEstimatorFlagsInput {
    adev: Option<bool>,
    overlapping_adev: Option<bool>,
    mdev: Option<bool>,
    hdev: Option<bool>,
    tdev: Option<bool>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum TauGridInput {
    Label(String),
    Object(TauGridObjectInput),
}

impl Default for TauGridInput {
    fn default() -> Self {
        Self::Label("octave".to_string())
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct TauGridObjectInput {
    kind: Option<String>,
    explicit: Option<Vec<usize>>,
    averaging_factors: Option<Vec<usize>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AllanResultJs {
    tau_s: Vec<f64>,
    deviation: Vec<f64>,
    n: Vec<usize>,
}

impl From<AllanResult> for AllanResultJs {
    fn from(result: AllanResult) -> Self {
        Self {
            tau_s: result.tau_s,
            deviation: result.deviation,
            n: result.n,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AllanCurvesJs {
    adev: Option<AllanResultJs>,
    overlapping_adev: Option<AllanResultJs>,
    mdev: Option<AllanResultJs>,
    hdev: Option<AllanResultJs>,
    tdev: Option<AllanResultJs>,
}

impl From<AllanDeviationCurves> for AllanCurvesJs {
    fn from(curves: AllanDeviationCurves) -> Self {
        Self {
            adev: curves.adev.map(AllanResultJs::from),
            overlapping_adev: curves.overlapping_adev.map(AllanResultJs::from),
            mdev: curves.mdev.map(AllanResultJs::from),
            hdev: curves.hdev.map(AllanResultJs::from),
            tdev: curves.tdev.map(AllanResultJs::from),
        }
    }
}

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| engine_error(format!("failed to serialize result: {e}")))
}

fn allan_error<E: core::fmt::Display>(err: E) -> JsValue {
    range_error(&err.to_string())
}

fn contiguous_values(series: &AllanSeriesInput) -> Result<Vec<f64>, JsValue> {
    let mut out = Vec::with_capacity(series.values.len());
    for (index, value) in series.values.iter().enumerate() {
        match value {
            Some(value) => out.push(*value),
            None => {
                return Err(type_error(&format!(
                    "series.values[{index}] is missing; use a WithGaps series kind for nulls"
                )));
            }
        }
    }
    Ok(out)
}

fn with_series<T>(
    input: &AllanSeriesInput,
    f: impl FnOnce(AllanSeries<'_>) -> Result<T, sidereon_core::clock_stability::AllanError>,
) -> Result<T, JsValue> {
    match input.kind.as_str() {
        "phaseSeconds" => {
            let values = contiguous_values(input)?;
            f(AllanSeries::PhaseSeconds(&values)).map_err(allan_error)
        }
        "fractionalFrequency" => {
            let values = contiguous_values(input)?;
            f(AllanSeries::FractionalFrequency(&values)).map_err(allan_error)
        }
        "phaseSecondsWithGaps" => {
            f(AllanSeries::PhaseSecondsWithGaps(&input.values)).map_err(allan_error)
        }
        "fractionalFrequencyWithGaps" => {
            f(AllanSeries::FractionalFrequencyWithGaps(&input.values)).map_err(allan_error)
        }
        other => Err(type_error(&format!("invalid Allan series kind {other:?}"))),
    }
}

fn parse_series(value: JsValue) -> Result<AllanSeriesInput, JsValue> {
    serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid Allan series: {e}")))
}

fn parse_factors(value: JsValue) -> Result<Vec<usize>, JsValue> {
    serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid averaging factors: {e}")))
}

fn estimators(input: AllanEstimatorSetInput) -> Result<AllanEstimatorSet, JsValue> {
    match input {
        AllanEstimatorSetInput::Label(label) if label.is_empty() || label == "standard" => {
            Ok(AllanEstimatorSet::standard())
        }
        AllanEstimatorSetInput::Label(label) if label == "all" => Ok(AllanEstimatorSet::all()),
        AllanEstimatorSetInput::Label(label) if label == "none" => Ok(AllanEstimatorSet::none()),
        AllanEstimatorSetInput::Label(label) => Err(type_error(&format!(
            "invalid Allan estimator set {label:?}"
        ))),
        AllanEstimatorSetInput::Flags(flags) => Ok(AllanEstimatorSet {
            adev: flags.adev.unwrap_or(false),
            overlapping_adev: flags.overlapping_adev.unwrap_or(false),
            mdev: flags.mdev.unwrap_or(false),
            hdev: flags.hdev.unwrap_or(false),
            tdev: flags.tdev.unwrap_or(false),
        }),
    }
}

fn tau_grid(input: TauGridInput) -> Result<TauGrid, JsValue> {
    match input {
        TauGridInput::Label(label) if label == "octave" => Ok(TauGrid::Octave),
        TauGridInput::Label(label) if label == "all" => Ok(TauGrid::All),
        TauGridInput::Label(label) => Err(type_error(&format!("invalid Allan tau grid {label:?}"))),
        TauGridInput::Object(object) => match object.kind.as_deref().unwrap_or("explicit") {
            "octave" => Ok(TauGrid::Octave),
            "all" => Ok(TauGrid::All),
            "explicit" => Ok(TauGrid::Explicit(
                object
                    .explicit
                    .or(object.averaging_factors)
                    .ok_or_else(|| type_error("explicit tau grid needs averagingFactors"))?,
            )),
            other => Err(type_error(&format!(
                "invalid Allan tau grid kind {other:?}"
            ))),
        },
    }
}

fn gap_policy(value: Option<String>) -> Result<GapPolicy, JsValue> {
    match value.as_deref().unwrap_or("reject") {
        "reject" => Ok(GapPolicy::Reject),
        "omitTerms" => Ok(GapPolicy::OmitTerms),
        other => Err(type_error(&format!(
            "invalid Allan gap policy {other:?}: expected \"reject\" or \"omitTerms\""
        ))),
    }
}

fn options(input: AllanOptionsInput) -> Result<AllanOptions, JsValue> {
    Ok(AllanOptions {
        estimators: estimators(input.estimators)?,
        tau_grid: tau_grid(input.tau_grid)?,
        gap_policy: gap_policy(input.gap_policy)?,
    })
}

fn estimate_explicit(
    series: JsValue,
    tau0_s: f64,
    averaging_factors: JsValue,
    f: impl FnOnce(
        AllanSeries<'_>,
        f64,
        &[usize],
    ) -> Result<AllanResult, sidereon_core::clock_stability::AllanError>,
) -> Result<JsValue, JsValue> {
    let series = parse_series(series)?;
    let factors = parse_factors(averaging_factors)?;
    let result = with_series(&series, |series| f(series, tau0_s, &factors))?;
    to_js(&AllanResultJs::from(result))
}

/// Plain non-overlapping Allan deviation for explicit averaging factors.
///
/// `series` is `{ kind, values }`, where `kind` is `"phaseSeconds"` or
/// `"fractionalFrequency"` and `values` are phase seconds or dimensionless
/// fractional-frequency samples. `tau0S` is the sample interval in seconds.
/// `averagingFactors` is an array of positive integer `m` values. Returns
/// `{ tauS, deviation, n }`.
#[wasm_bindgen(js_name = allanDeviation)]
pub fn allan_deviation(
    series: JsValue,
    tau0_s: f64,
    averaging_factors: JsValue,
) -> Result<JsValue, JsValue> {
    estimate_explicit(series, tau0_s, averaging_factors, core_allan_deviation)
}

/// Fully overlapping Allan deviation for explicit averaging factors.
///
/// `series` is `{ kind, values }`, `tau0S` is seconds, and
/// `averagingFactors` contains positive integer `m` values. Returns
/// `{ tauS, deviation, n }`.
#[wasm_bindgen(js_name = overlappingAdev)]
pub fn overlapping_adev(
    series: JsValue,
    tau0_s: f64,
    averaging_factors: JsValue,
) -> Result<JsValue, JsValue> {
    estimate_explicit(series, tau0_s, averaging_factors, core_overlapping_adev)
}

/// Modified Allan deviation for explicit averaging factors.
///
/// `series` is `{ kind, values }`, `tau0S` is seconds, and
/// `averagingFactors` contains positive integer `m` values. Returns
/// `{ tauS, deviation, n }`.
#[wasm_bindgen(js_name = modifiedAdev)]
pub fn modified_adev(
    series: JsValue,
    tau0_s: f64,
    averaging_factors: JsValue,
) -> Result<JsValue, JsValue> {
    estimate_explicit(series, tau0_s, averaging_factors, core_modified_adev)
}

/// Overlapping Hadamard deviation for explicit averaging factors.
///
/// `series` is `{ kind, values }`, `tau0S` is seconds, and
/// `averagingFactors` contains positive integer `m` values. Returns
/// `{ tauS, deviation, n }`.
#[wasm_bindgen(js_name = hadamardDeviation)]
pub fn hadamard_deviation(
    series: JsValue,
    tau0_s: f64,
    averaging_factors: JsValue,
) -> Result<JsValue, JsValue> {
    estimate_explicit(series, tau0_s, averaging_factors, core_hadamard_deviation)
}

/// Time deviation for explicit averaging factors.
///
/// `series` is `{ kind, values }`, `tau0S` is seconds, and
/// `averagingFactors` contains positive integer `m` values. Returns
/// `{ tauS, deviation, n }`.
#[wasm_bindgen(js_name = timeDeviation)]
pub fn time_deviation(
    series: JsValue,
    tau0_s: f64,
    averaging_factors: JsValue,
) -> Result<JsValue, JsValue> {
    estimate_explicit(series, tau0_s, averaging_factors, core_time_deviation)
}

/// Compute one or more Allan-family curves with a selected tau grid and gap
/// policy.
///
/// `input` is `{ series, tau0S, options? }`. `series.kind` may be
/// `"phaseSeconds"`, `"fractionalFrequency"`, `"phaseSecondsWithGaps"`, or
/// `"fractionalFrequencyWithGaps"`; gap series use `null` for missing samples.
/// `options.estimators` is `"standard"`, `"all"`, `"none"`, or a boolean flag
/// object. `options.tauGrid` is `"octave"`, `"all"`, or
/// `{ kind: "explicit", averagingFactors }`. `tau0S` and all returned `tauS`
/// values are seconds.
#[wasm_bindgen(js_name = computeAllanDeviations)]
pub fn compute_allan_deviations(input: JsValue) -> Result<JsValue, JsValue> {
    let input: ComputeInput = serde_wasm_bindgen::from_value(input)
        .map_err(|e| type_error(&format!("invalid Allan input: {e}")))?;
    let opts = options(input.options)?;
    let curves = with_series(&input.series, |series| {
        core_compute(&AllanInput {
            series,
            tau0_s: input.tau0_s,
            options: opts,
        })
    })?;
    to_js(&AllanCurvesJs::from(curves))
}
