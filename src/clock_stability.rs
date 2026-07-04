//! Allan-family receiver clock-stability estimators.
//!
//! The exported functions marshal JS sample arrays into
//! `sidereon_core::clock_stability` and return plain JS result objects. The
//! binding carries no estimator math of its own.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::clock_stability::{
    allan_deviation as core_allan_deviation,
    allan_deviation_power_law_slope as core_allan_deviation_power_law_slope,
    allan_variance_power_law_tau_exponent as core_allan_variance_power_law_tau_exponent,
    compute_allan_deviations as core_compute, fit_power_law_noise as core_fit_power_law_noise,
    hadamard_deviation as core_hadamard_deviation, modified_adev as core_modified_adev,
    modified_allan_deviation_power_law_slope as core_modified_allan_deviation_power_law_slope,
    overlapping_adev as core_overlapping_adev, time_deviation as core_time_deviation,
    AllanDeviationCurves, AllanEstimatorSet, AllanInput, AllanOptions, AllanResult, AllanSeries,
    GapPolicy, PowerLawNoiseFit as CorePowerLawNoiseFit,
    PowerLawNoiseOptions as CorePowerLawNoiseOptions,
    PowerLawNoiseRegion as CorePowerLawNoiseRegion, PowerLawNoiseType as CorePowerLawNoiseType,
    PowerLawOctave as CorePowerLawOctave, PowerLawOctaveDominance as CorePowerLawOctaveDominance,
    PowerLawOctaveFlag as CorePowerLawOctaveFlag, TauGrid,
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

/// IEEE 1139 fractional-frequency PSD power-law noise type.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PowerLawNoiseType {
    /// Random-walk frequency modulation, `S_y(f) = h_-2 f^-2`.
    RandomWalkFM,
    /// Flicker frequency modulation, `S_y(f) = h_-1 f^-1`.
    FlickerFM,
    /// White frequency modulation, `S_y(f) = h_0`.
    WhiteFM,
    /// Flicker phase modulation, `S_y(f) = h_1 f`.
    FlickerPM,
    /// White phase modulation, `S_y(f) = h_2 f^2`.
    WhitePM,
}

impl From<PowerLawNoiseType> for CorePowerLawNoiseType {
    fn from(value: PowerLawNoiseType) -> Self {
        match value {
            PowerLawNoiseType::RandomWalkFM => Self::RandomWalkFM,
            PowerLawNoiseType::FlickerFM => Self::FlickerFM,
            PowerLawNoiseType::WhiteFM => Self::WhiteFM,
            PowerLawNoiseType::FlickerPM => Self::FlickerPM,
            PowerLawNoiseType::WhitePM => Self::WhitePM,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AllanResultInput {
    tau_s: Vec<f64>,
    deviation: Vec<f64>,
    n: Vec<usize>,
}

impl From<AllanResultInput> for AllanResult {
    fn from(value: AllanResultInput) -> Self {
        AllanResult {
            tau_s: value.tau_s,
            deviation: value.deviation,
            n: value.n,
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct PowerLawNoiseOptionsInput {
    min_points_per_octave: Option<usize>,
    slope_tolerance: Option<f64>,
    scatter_tolerance: Option<f64>,
    basic_tau_s: Option<f64>,
    measurement_bandwidth_hz: Option<f64>,
}

fn power_law_options(input: PowerLawNoiseOptionsInput) -> CorePowerLawNoiseOptions {
    let basic_tau_s = input.basic_tau_s.unwrap_or(1.0);
    let mut options = input.measurement_bandwidth_hz.map_or_else(
        || CorePowerLawNoiseOptions::sampled_at_nyquist(basic_tau_s),
        |bandwidth| CorePowerLawNoiseOptions::new(basic_tau_s, bandwidth),
    );
    if let Some(value) = input.min_points_per_octave {
        options.min_points_per_octave = value;
    }
    if let Some(value) = input.slope_tolerance {
        options.slope_tolerance = value;
    }
    if let Some(value) = input.scatter_tolerance {
        options.scatter_tolerance = value;
    }
    options
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PowerLawOctaveDominanceJs {
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    noise_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    flag: Option<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PowerLawOctaveJs {
    tau_start_s: f64,
    tau_end_s: f64,
    point_count: usize,
    adev_slope: Option<f64>,
    mdev_slope: Option<f64>,
    slope_scatter: Option<f64>,
    dominance: PowerLawOctaveDominanceJs,
}

impl From<CorePowerLawOctave> for PowerLawOctaveJs {
    fn from(value: CorePowerLawOctave) -> Self {
        Self {
            tau_start_s: value.tau_start_s,
            tau_end_s: value.tau_end_s,
            point_count: value.point_count,
            adev_slope: value.adev_slope,
            mdev_slope: value.mdev_slope,
            slope_scatter: value.slope_scatter,
            dominance: dominance_js(value.dominance),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PowerLawNoiseRegionJs {
    noise_type: &'static str,
    tau_start_s: f64,
    tau_end_s: f64,
    octave_count: usize,
    point_count: usize,
    mean_slope: f64,
    coefficient: f64,
}

impl From<CorePowerLawNoiseRegion> for PowerLawNoiseRegionJs {
    fn from(value: CorePowerLawNoiseRegion) -> Self {
        Self {
            noise_type: noise_type_label(value.noise_type),
            tau_start_s: value.tau_start_s,
            tau_end_s: value.tau_end_s,
            octave_count: value.octave_count,
            point_count: value.point_count,
            mean_slope: value.mean_slope,
            coefficient: value.coefficient,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PowerLawNoiseFitJs {
    dominant_per_octave: Vec<PowerLawOctaveJs>,
    coefficients: [f64; 5],
    regions: Vec<PowerLawNoiseRegionJs>,
}

impl From<CorePowerLawNoiseFit> for PowerLawNoiseFitJs {
    fn from(value: CorePowerLawNoiseFit) -> Self {
        Self {
            dominant_per_octave: value
                .dominant_per_octave
                .into_iter()
                .map(PowerLawOctaveJs::from)
                .collect(),
            coefficients: value.coefficients,
            regions: value
                .regions
                .into_iter()
                .map(PowerLawNoiseRegionJs::from)
                .collect(),
        }
    }
}

fn dominance_js(value: CorePowerLawOctaveDominance) -> PowerLawOctaveDominanceJs {
    match value {
        CorePowerLawOctaveDominance::Dominant(noise_type) => PowerLawOctaveDominanceJs {
            kind: "dominant",
            noise_type: Some(noise_type_label(noise_type)),
            flag: None,
        },
        CorePowerLawOctaveDominance::Ambiguous => PowerLawOctaveDominanceJs {
            kind: "ambiguous",
            noise_type: None,
            flag: None,
        },
        CorePowerLawOctaveDominance::Flagged(flag) => PowerLawOctaveDominanceJs {
            kind: "flagged",
            noise_type: None,
            flag: Some(octave_flag_label(flag)),
        },
    }
}

fn noise_type_label(value: CorePowerLawNoiseType) -> &'static str {
    match value {
        CorePowerLawNoiseType::RandomWalkFM => "randomWalkFM",
        CorePowerLawNoiseType::FlickerFM => "flickerFM",
        CorePowerLawNoiseType::WhiteFM => "whiteFM",
        CorePowerLawNoiseType::FlickerPM => "flickerPM",
        CorePowerLawNoiseType::WhitePM => "whitePM",
    }
}

fn octave_flag_label(value: CorePowerLawOctaveFlag) -> &'static str {
    match value {
        CorePowerLawOctaveFlag::UnderSampled => "underSampled",
        CorePowerLawOctaveFlag::DegenerateDeviation => "degenerateDeviation",
        CorePowerLawOctaveFlag::MissingModifiedAllan => "missingModifiedAllan",
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

/// Exact ADEV log-log slope for a power-law noise type.
#[wasm_bindgen(js_name = allanDeviationPowerLawSlope)]
pub fn allan_deviation_power_law_slope(noise_type: PowerLawNoiseType) -> f64 {
    core_allan_deviation_power_law_slope(noise_type.into())
}

/// Exact MDEV log-log slope for a power-law noise type.
#[wasm_bindgen(js_name = modifiedAllanDeviationPowerLawSlope)]
pub fn modified_allan_deviation_power_law_slope(noise_type: PowerLawNoiseType) -> f64 {
    core_modified_allan_deviation_power_law_slope(noise_type.into())
}

/// Exact Allan-variance tau exponent for a power-law noise type.
#[wasm_bindgen(js_name = allanVariancePowerLawTauExponent)]
pub fn allan_variance_power_law_tau_exponent(noise_type: PowerLawNoiseType) -> i32 {
    core_allan_variance_power_law_tau_exponent(noise_type.into())
}

/// Identify power-law clock-noise regions from ADEV and MDEV curves.
///
/// `adev` and `mdev` are `{ tauS, deviation, n }` objects, matching the output
/// from the Allan-family functions. `options` may set `basicTauS`,
/// `measurementBandwidthHz`, `minPointsPerOctave`, `slopeTolerance`, and
/// `scatterTolerance`.
#[wasm_bindgen(js_name = fitPowerLawNoise)]
pub fn fit_power_law_noise(
    adev: JsValue,
    mdev: JsValue,
    options: JsValue,
) -> Result<JsValue, JsValue> {
    let adev: AllanResultInput = serde_wasm_bindgen::from_value(adev)
        .map_err(|e| type_error(&format!("invalid ADEV curve: {e}")))?;
    let mdev: AllanResultInput = serde_wasm_bindgen::from_value(mdev)
        .map_err(|e| type_error(&format!("invalid MDEV curve: {e}")))?;
    let options = if options.is_undefined() || options.is_null() {
        PowerLawNoiseOptionsInput::default()
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid power-law options: {e}")))?
    };
    let fit = core_fit_power_law_noise(&adev.into(), &mdev.into(), power_law_options(options))
        .map_err(allan_error)?;
    to_js(&PowerLawNoiseFitJs::from(fit))
}
