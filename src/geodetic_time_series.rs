//! Geodetic station time-series velocity, trajectory, step, and network tools.
//!
//! This binding marshals plain JS objects into `sidereon_core` time-series
//! inputs and serializes the core result structs. Estimation logic stays in the
//! core crate.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::frame::Wgs84Geodetic;
use sidereon_core::geodetic_time_series::{
    detect_steps as core_detect_steps, fit_trajectory as core_fit_trajectory,
    network_field as core_network_field, velocity_midas as core_velocity_midas, Loss, MidasOptions,
    MotionField, NetworkFrame, NetworkStation, PositionFrame, PositionSample, PositionSeries,
    StepCandidate, StepDetectionHeuristic, StepDetectionOptions, TimeSeriesQuality, Trajectory,
    TrajectoryComponent, TrajectoryFitOptions, TrajectoryModel, TrajectoryTerm, Velocity,
};

use crate::error::{engine_error, range_error, type_error};
use crate::geometry_quality::GeometryQualityJs;

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| engine_error(format!("failed to serialize result: {e}")))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeodeticInput {
    lat_rad: f64,
    lon_rad: f64,
    #[serde(default)]
    height_m: Option<f64>,
}

impl GeodeticInput {
    fn to_core(&self, field: &str) -> Result<Wgs84Geodetic, JsValue> {
        Wgs84Geodetic::new(self.lat_rad, self.lon_rad, self.height_m.unwrap_or(0.0))
            .map_err(|e| range_error(&format!("{field}: {e}")))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PositionSampleInput {
    epoch_year: f64,
    position_m: [f64; 3],
    #[serde(default)]
    covariance_m2: Option<[[f64; 3]; 3]>,
}

impl PositionSampleInput {
    fn to_core(&self) -> PositionSample {
        PositionSample {
            epoch_year: self.epoch_year,
            position_m: self.position_m,
            covariance_m2: self.covariance_m2,
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum FrameInput {
    Label(String),
    Object(FrameObjectInput),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrameObjectInput {
    kind: String,
    #[serde(default)]
    reference: Option<GeodeticInput>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeriesInput {
    samples: Vec<PositionSampleInput>,
    #[serde(default)]
    frame: Option<FrameInput>,
}

fn frame(input: Option<FrameInput>) -> Result<PositionFrame, JsValue> {
    match input {
        None => Ok(PositionFrame::Enu),
        Some(FrameInput::Label(label)) => match label.as_str() {
            "enu" => Ok(PositionFrame::Enu),
            other => Err(type_error(&format!(
                "invalid position frame {other:?}: expected \"enu\" or an object"
            ))),
        },
        Some(FrameInput::Object(object)) => match object.kind.as_str() {
            "enu" => Ok(PositionFrame::Enu),
            "ecef" => Ok(PositionFrame::Ecef {
                reference: object
                    .reference
                    .ok_or_else(|| type_error("ecef frame requires reference"))?
                    .to_core("frame.reference")?,
            }),
            other => Err(type_error(&format!(
                "invalid position frame kind {other:?}: expected \"enu\" or \"ecef\""
            ))),
        },
    }
}

fn decode_series(input: JsValue) -> Result<(Vec<PositionSample>, PositionFrame), JsValue> {
    let input: SeriesInput = serde_wasm_bindgen::from_value(input)
        .map_err(|e| type_error(&format!("invalid position series: {e}")))?;
    let samples = input
        .samples
        .iter()
        .map(PositionSampleInput::to_core)
        .collect();
    Ok((samples, frame(input.frame)?))
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct MidasOptionsInput {
    dominant_period_years: Option<f64>,
    period_tolerance_years: Option<f64>,
    min_pairs: Option<usize>,
}

fn midas_options(input: MidasOptionsInput) -> MidasOptions {
    let defaults = MidasOptions::default();
    MidasOptions {
        dominant_period_years: input
            .dominant_period_years
            .unwrap_or(defaults.dominant_period_years),
        period_tolerance_years: input
            .period_tolerance_years
            .unwrap_or(defaults.period_tolerance_years),
        min_pairs: input.min_pairs.unwrap_or(defaults.min_pairs),
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct TrajectoryModelInput {
    reference_epoch_year: Option<f64>,
    include_annual: Option<bool>,
    include_semiannual: Option<bool>,
    offset_epochs_year: Option<Vec<f64>>,
}

fn trajectory_model(input: TrajectoryModelInput) -> TrajectoryModel {
    let defaults = TrajectoryModel::default();
    TrajectoryModel {
        reference_epoch_year: input.reference_epoch_year,
        include_annual: input.include_annual.unwrap_or(defaults.include_annual),
        include_semiannual: input
            .include_semiannual
            .unwrap_or(defaults.include_semiannual),
        offset_epochs_year: input.offset_epochs_year.unwrap_or_default(),
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct TrajectoryFitOptionsInput {
    loss: Option<String>,
    f_scale_m: Option<f64>,
    max_nfev: Option<usize>,
}

fn loss(label: Option<String>) -> Result<Loss, JsValue> {
    match label.as_deref().unwrap_or("linear") {
        "linear" => Ok(Loss::Linear),
        "softL1" | "soft_l1" => Ok(Loss::SoftL1),
        "huber" => Ok(Loss::Huber),
        "cauchy" => Ok(Loss::Cauchy),
        "arctan" => Ok(Loss::Arctan),
        other => Err(type_error(&format!(
            "invalid trajectory loss {other:?}: expected \"linear\", \"softL1\", \"huber\", \"cauchy\", or \"arctan\""
        ))),
    }
}

fn trajectory_options(input: TrajectoryFitOptionsInput) -> Result<TrajectoryFitOptions, JsValue> {
    let defaults = TrajectoryFitOptions::default();
    Ok(TrajectoryFitOptions {
        loss: loss(input.loss)?,
        f_scale_m: input.f_scale_m.unwrap_or(defaults.f_scale_m),
        max_nfev: input.max_nfev,
    })
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct StepDetectionOptionsInput {
    window_years: Option<f64>,
    score_threshold: Option<f64>,
    min_offset_m: Option<f64>,
    min_samples_each_side: Option<usize>,
    min_separation_years: Option<f64>,
    midas: Option<MidasOptionsInput>,
}

fn step_options(input: StepDetectionOptionsInput) -> StepDetectionOptions {
    let defaults = StepDetectionOptions::default();
    StepDetectionOptions {
        window_years: input.window_years.unwrap_or(defaults.window_years),
        score_threshold: input.score_threshold.unwrap_or(defaults.score_threshold),
        min_offset_m: input.min_offset_m.unwrap_or(defaults.min_offset_m),
        min_samples_each_side: input
            .min_samples_each_side
            .unwrap_or(defaults.min_samples_each_side),
        min_separation_years: input
            .min_separation_years
            .unwrap_or(defaults.min_separation_years),
        midas: input.midas.map(midas_options).unwrap_or(defaults.midas),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NetworkFieldInput {
    frame: NetworkFrameInput,
    stations: Vec<NetworkStationInput>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NetworkFrameInput {
    origin: GeodeticInput,
    #[serde(default)]
    remove_common_mode: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NetworkStationInput {
    id: String,
    reference: GeodeticInput,
    series: SeriesInput,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MidasComponentStatsJs {
    pair_count: usize,
    retained_pair_count: usize,
    slope_sigma_m_per_yr: f64,
    effective_pair_count: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VelocityJs {
    rate_enu_m_per_yr: [f64; 3],
    sigma_enu_m_per_yr: [f64; 3],
    covariance_enu_m2_per_yr2: [[f64; 3]; 3],
    component_stats: [MidasComponentStatsJs; 3],
    sample_count: usize,
    span_years: f64,
    quality: &'static str,
}

impl From<Velocity> for VelocityJs {
    fn from(value: Velocity) -> Self {
        Self {
            rate_enu_m_per_yr: value.rate_enu_m_per_yr,
            sigma_enu_m_per_yr: value.sigma_enu_m_per_yr,
            covariance_enu_m2_per_yr2: value.covariance_enu_m2_per_yr2,
            component_stats: value.component_stats.map(|stats| MidasComponentStatsJs {
                pair_count: stats.pair_count,
                retained_pair_count: stats.retained_pair_count,
                slope_sigma_m_per_yr: stats.slope_sigma_m_per_yr,
                effective_pair_count: stats.effective_pair_count,
            }),
            sample_count: value.sample_count,
            span_years: value.span_years,
            quality: quality_label(value.quality),
        }
    }
}

fn quality_label(value: TimeSeriesQuality) -> &'static str {
    match value {
        TimeSeriesQuality::Nominal => "nominal",
        TimeSeriesQuality::ShortSpan => "shortSpan",
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrajectoryTermJs {
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    epoch_year: Option<f64>,
}

impl From<TrajectoryTerm> for TrajectoryTermJs {
    fn from(value: TrajectoryTerm) -> Self {
        match value {
            TrajectoryTerm::Position => Self {
                kind: "position",
                index: None,
                epoch_year: None,
            },
            TrajectoryTerm::Velocity => Self {
                kind: "velocity",
                index: None,
                epoch_year: None,
            },
            TrajectoryTerm::AnnualSin => Self {
                kind: "annualSin",
                index: None,
                epoch_year: None,
            },
            TrajectoryTerm::AnnualCos => Self {
                kind: "annualCos",
                index: None,
                epoch_year: None,
            },
            TrajectoryTerm::SemiannualSin => Self {
                kind: "semiannualSin",
                index: None,
                epoch_year: None,
            },
            TrajectoryTerm::SemiannualCos => Self {
                kind: "semiannualCos",
                index: None,
                epoch_year: None,
            },
            TrajectoryTerm::Offset { index, epoch_year } => Self {
                kind: "offset",
                index: Some(index),
                epoch_year: Some(epoch_year),
            },
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrajectoryComponentJs {
    position_m: f64,
    velocity_m_per_yr: f64,
    annual_sin_m: Option<f64>,
    annual_cos_m: Option<f64>,
    semiannual_sin_m: Option<f64>,
    semiannual_cos_m: Option<f64>,
    offsets_m: Vec<f64>,
}

impl From<TrajectoryComponent> for TrajectoryComponentJs {
    fn from(value: TrajectoryComponent) -> Self {
        Self {
            position_m: value.position_m,
            velocity_m_per_yr: value.velocity_m_per_yr,
            annual_sin_m: value.annual_sin_m,
            annual_cos_m: value.annual_cos_m,
            semiannual_sin_m: value.semiannual_sin_m,
            semiannual_cos_m: value.semiannual_cos_m,
            offsets_m: value.offsets_m,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrajectoryJs {
    reference_epoch_year: f64,
    terms: Vec<TrajectoryTermJs>,
    components: [TrajectoryComponentJs; 3],
    parameter_covariance: Vec<Vec<f64>>,
    residual_rms_enu_m: [f64; 3],
    geometry_quality: GeometryQualityJs,
    status: i32,
    nfev: usize,
    njev: usize,
    cost: f64,
    optimality: f64,
}

impl From<Trajectory> for TrajectoryJs {
    fn from(value: Trajectory) -> Self {
        Self {
            reference_epoch_year: value.reference_epoch_year,
            terms: value
                .terms
                .into_iter()
                .map(TrajectoryTermJs::from)
                .collect(),
            components: value.components.map(TrajectoryComponentJs::from),
            parameter_covariance: value.parameter_covariance,
            residual_rms_enu_m: value.residual_rms_enu_m,
            geometry_quality: value.geometry_quality.into(),
            status: value.status,
            nfev: value.nfev,
            njev: value.njev,
            cost: value.cost,
            optimality: value.optimality,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StepCandidateJs {
    epoch_year: f64,
    offset_enu_m: [f64; 3],
    score: f64,
    before_count: usize,
    after_count: usize,
    heuristic: &'static str,
}

impl From<StepCandidate> for StepCandidateJs {
    fn from(value: StepCandidate) -> Self {
        Self {
            epoch_year: value.epoch_year,
            offset_enu_m: value.offset_enu_m,
            score: value.score,
            before_count: value.before_count,
            after_count: value.after_count,
            heuristic: match value.heuristic {
                StepDetectionHeuristic::DetrendedSlidingMedian => "detrendedSlidingMedian",
            },
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeodeticJs {
    lat_rad: f64,
    lon_rad: f64,
    height_m: f64,
}

impl From<Wgs84Geodetic> for GeodeticJs {
    fn from(value: Wgs84Geodetic) -> Self {
        Self {
            lat_rad: value.lat_rad,
            lon_rad: value.lon_rad,
            height_m: value.height_m,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NetworkFrameJs {
    origin: GeodeticJs,
    remove_common_mode: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StationMotionJs {
    id: String,
    rate_enu_m_per_yr: [f64; 3],
    raw_rate_enu_m_per_yr: [f64; 3],
    sigma_enu_m_per_yr: [f64; 3],
    local_velocity: VelocityJs,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MotionFieldJs {
    frame: NetworkFrameJs,
    stations: Vec<StationMotionJs>,
    common_mode_enu_m_per_yr: [f64; 3],
}

impl From<MotionField> for MotionFieldJs {
    fn from(value: MotionField) -> Self {
        Self {
            frame: NetworkFrameJs {
                origin: value.frame.origin.into(),
                remove_common_mode: value.frame.remove_common_mode,
            },
            stations: value
                .stations
                .into_iter()
                .map(|station| StationMotionJs {
                    id: station.id,
                    rate_enu_m_per_yr: station.rate_enu_m_per_yr,
                    raw_rate_enu_m_per_yr: station.raw_rate_enu_m_per_yr,
                    sigma_enu_m_per_yr: station.sigma_enu_m_per_yr,
                    local_velocity: station.local_velocity.into(),
                })
                .collect(),
            common_mode_enu_m_per_yr: value.common_mode_enu_m_per_yr,
        }
    }
}

/// Estimate robust station velocity with the MIDAS method.
///
/// `series` is `{ samples, frame? }`, where samples are
/// `{ epochYear, positionM, covarianceM2? }`. The frame is `"enu"` by default
/// or `{ kind: "ecef", reference }`.
#[wasm_bindgen(js_name = velocityMidas)]
pub fn velocity_midas(series: JsValue, options: JsValue) -> Result<JsValue, JsValue> {
    let (samples, frame) = decode_series(series)?;
    let options = if options.is_undefined() || options.is_null() {
        MidasOptionsInput::default()
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid MIDAS options: {e}")))?
    };
    let series = PositionSeries {
        frame,
        samples: &samples,
    };
    let result = core_velocity_midas(&series, midas_options(options)).map_err(engine_error)?;
    to_js(&VelocityJs::from(result))
}

/// Fit a geodetic trajectory model with velocity, seasonal, and step terms.
///
/// `model` may set `referenceEpochYear`, `includeAnnual`,
/// `includeSemiannual`, and `offsetEpochsYear`. `options` may set `loss`,
/// `fScaleM`, and `maxNfev`.
#[wasm_bindgen(js_name = fitTrajectory)]
pub fn fit_trajectory(
    series: JsValue,
    model: JsValue,
    options: JsValue,
) -> Result<JsValue, JsValue> {
    let (samples, frame) = decode_series(series)?;
    let model = if model.is_undefined() || model.is_null() {
        TrajectoryModelInput::default()
    } else {
        serde_wasm_bindgen::from_value(model)
            .map_err(|e| type_error(&format!("invalid trajectory model: {e}")))?
    };
    let options = if options.is_undefined() || options.is_null() {
        TrajectoryFitOptionsInput::default()
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid trajectory options: {e}")))?
    };
    let series = PositionSeries {
        frame,
        samples: &samples,
    };
    let result = core_fit_trajectory(
        &series,
        &trajectory_model(model),
        trajectory_options(options)?,
    )
    .map_err(engine_error)?;
    to_js(&TrajectoryJs::from(result))
}

/// Detect candidate displacement steps in a station position series.
///
/// `options` may set `windowYears`, `scoreThreshold`, `minOffsetM`,
/// `minSamplesEachSide`, `minSeparationYears`, and nested `midas` controls.
#[wasm_bindgen(js_name = detectSteps)]
pub fn detect_steps(series: JsValue, options: JsValue) -> Result<JsValue, JsValue> {
    let (samples, frame) = decode_series(series)?;
    let options = if options.is_undefined() || options.is_null() {
        StepDetectionOptionsInput::default()
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid step-detection options: {e}")))?
    };
    let series = PositionSeries {
        frame,
        samples: &samples,
    };
    let result = core_detect_steps(&series, step_options(options)).map_err(engine_error)?;
    let output = result
        .into_iter()
        .map(StepCandidateJs::from)
        .collect::<Vec<_>>();
    to_js(&output)
}

/// Estimate a station network velocity field in one local ENU frame.
///
/// The input is `{ frame: { origin, removeCommonMode? }, stations }`. Each
/// station has `{ id, reference, series }`, and each series follows
/// `velocityMidas`.
#[wasm_bindgen(js_name = networkField)]
pub fn network_field(input: JsValue) -> Result<JsValue, JsValue> {
    let input: NetworkFieldInput = serde_wasm_bindgen::from_value(input)
        .map_err(|e| type_error(&format!("invalid network field input: {e}")))?;
    let frame = NetworkFrame {
        origin: input.frame.origin.to_core("frame.origin")?,
        remove_common_mode: input.frame.remove_common_mode,
    };

    let mut sample_sets = Vec::with_capacity(input.stations.len());
    let mut frames = Vec::with_capacity(input.stations.len());
    let mut ids = Vec::with_capacity(input.stations.len());
    let mut references = Vec::with_capacity(input.stations.len());
    for station in input.stations {
        sample_sets.push(
            station
                .series
                .samples
                .iter()
                .map(PositionSampleInput::to_core)
                .collect::<Vec<_>>(),
        );
        frames.push(frame_input(station.series.frame)?);
        ids.push(station.id);
        references.push(station.reference.to_core("station.reference")?);
    }

    let series = sample_sets
        .iter()
        .zip(frames.iter())
        .map(|(samples, frame)| PositionSeries {
            frame: *frame,
            samples,
        })
        .collect::<Vec<_>>();
    let stations = series
        .iter()
        .enumerate()
        .map(|(index, series)| NetworkStation {
            id: ids[index].as_str(),
            reference: references[index],
            series: *series,
        })
        .collect::<Vec<_>>();

    let result = core_network_field(&stations, frame).map_err(engine_error)?;
    to_js(&MotionFieldJs::from(result))
}

fn frame_input(input: Option<FrameInput>) -> Result<PositionFrame, JsValue> {
    frame(input)
}
