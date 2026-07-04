//! Source localization from arrival times.
//!
//! This module marshals plain JS arrays and objects into
//! `sidereon_core::source_localization`. Sensor and source coordinates are
//! caller-chosen 2D or 3D Cartesian metres, not geodetic coordinates. Arrival
//! times and residuals are seconds, propagation speeds are metres per second,
//! covariance position blocks are square metres, and DOP values multiply timing
//! sigma in seconds to produce position metres. Geometry observability is
//! reported through the shared `GeometryQuality` object.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::dop::{Dop as CoreDop, DopError};
use sidereon_core::source_localization::{
    chan_ho_initial_guess as core_chan_ho_initial_guess, locate_source as core_locate_source,
    source_crlb as core_source_crlb, source_dop as core_source_dop, Loss, Sensor as CoreSensor,
    SourceCovariance as CoreSourceCovariance, SourceCrlb as CoreSourceCrlb,
    SourceInitialGuess as CoreSourceInitialGuess, SourceLocalizationError,
    SourceLocateOptions as CoreSourceLocateOptions, SourceResidual as CoreSourceResidual,
    SourceSensorInfluence as CoreSourceSensorInfluence, SourceSolution as CoreSourceSolution,
    SourceSolveMode as CoreSourceSolveMode,
};

use crate::error::{engine_error, range_error, type_error};
use crate::geometry_quality::GeometryQualityJs;

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| engine_error(format!("failed to serialize result: {e}")))
}

fn from_js<T: for<'de> Deserialize<'de>>(value: JsValue, label: &str) -> Result<T, JsValue> {
    serde_wasm_bindgen::from_value(value).map_err(|e| type_error(&format!("invalid {label}: {e}")))
}

fn source_error(err: SourceLocalizationError) -> JsValue {
    match err {
        SourceLocalizationError::InvalidInput { .. } => range_error(&err.to_string()),
        SourceLocalizationError::TooFewSensors { .. } => type_error(&err.to_string()),
        SourceLocalizationError::Geometry(DopError::InvalidInput { .. }) => {
            range_error(&err.to_string())
        }
        SourceLocalizationError::Geometry(DopError::TooFewSatellites) => {
            type_error(&err.to_string())
        }
        SourceLocalizationError::InitializerSingular
        | SourceLocalizationError::Geometry(DopError::Singular)
        | SourceLocalizationError::Solver(_)
        | SourceLocalizationError::DidNotConverge { .. } => engine_error(err),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SensorJs {
    position_m: Vec<f64>,
    #[serde(default)]
    propagation_speed_m_s: Option<f64>,
}

impl From<SensorJs> for CoreSensor {
    fn from(value: SensorJs) -> Self {
        Self {
            position_m: value.position_m,
            propagation_speed_m_s: value.propagation_speed_m_s,
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct SourceLocateOptionsJs {
    mode: Option<String>,
    reference_sensor: Option<usize>,
    timing_sigma_s: Option<f64>,
    loss: Option<String>,
    f_scale_s: Option<f64>,
    ftol: Option<f64>,
    xtol: Option<f64>,
    gtol: Option<f64>,
    max_nfev: Option<usize>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct SourceSolveModeObjectJs {
    mode: Option<String>,
    reference_sensor: Option<usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceSolveModeTdoaJs {
    mode: &'static str,
    reference_sensor: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DopSystemTdopJs {
    system: String,
    tdop: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DopJs {
    gdop: f64,
    pdop: f64,
    hdop: f64,
    vdop: f64,
    tdop: f64,
    system_tdops: Vec<DopSystemTdopJs>,
}

impl From<CoreDop> for DopJs {
    fn from(value: CoreDop) -> Self {
        Self {
            gdop: value.gdop,
            pdop: value.pdop,
            hdop: value.hdop,
            vdop: value.vdop,
            tdop: value.tdop,
            system_tdops: value
                .system_tdops
                .into_iter()
                .map(|(system, tdop)| DopSystemTdopJs {
                    system: system.as_str().to_string(),
                    tdop,
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceInitialGuessJs {
    position_m: Vec<f64>,
    origin_time_s: Option<f64>,
    residual_rms_s: f64,
}

impl From<CoreSourceInitialGuess> for SourceInitialGuessJs {
    fn from(value: CoreSourceInitialGuess) -> Self {
        Self {
            position_m: value.position_m,
            origin_time_s: value.origin_time_s,
            residual_rms_s: value.residual_rms_s,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceResidualJs {
    sensor_index: usize,
    reference_sensor_index: Option<usize>,
    residual_s: f64,
}

impl From<CoreSourceResidual> for SourceResidualJs {
    fn from(value: CoreSourceResidual) -> Self {
        Self {
            sensor_index: value.sensor_index,
            reference_sensor_index: value.reference_sensor_index,
            residual_s: value.residual_s,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceSensorInfluenceJs {
    sensor_index: usize,
    residual_s: f64,
    leave_one_out_residual_s: Option<f64>,
    position_delta_m: Option<f64>,
    origin_time_delta_s: Option<f64>,
    loss_weight: f64,
    score: f64,
}

impl From<CoreSourceSensorInfluence> for SourceSensorInfluenceJs {
    fn from(value: CoreSourceSensorInfluence) -> Self {
        Self {
            sensor_index: value.sensor_index,
            residual_s: value.residual_s,
            leave_one_out_residual_s: value.leave_one_out_residual_s,
            position_delta_m: value.position_delta_m,
            origin_time_delta_s: value.origin_time_delta_s,
            loss_weight: value.loss_weight,
            score: value.score,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceCovarianceJs {
    state: Vec<Vec<f64>>,
    position_m2: Vec<Vec<f64>>,
    origin_time_s2: Option<f64>,
    timing_sigma_s: f64,
}

impl From<CoreSourceCovariance> for SourceCovarianceJs {
    fn from(value: CoreSourceCovariance) -> Self {
        Self {
            state: value.state,
            position_m2: value.position_m2,
            origin_time_s2: value.origin_time_s2,
            timing_sigma_s: value.timing_sigma_s,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceSolutionJs {
    position_m: Vec<f64>,
    origin_time_s: Option<f64>,
    covariance: Option<SourceCovarianceJs>,
    residuals: Vec<SourceResidualJs>,
    per_sensor_influence: Vec<SourceSensorInfluenceJs>,
    geometry_quality: GeometryQualityJs,
    initial_guess: SourceInitialGuessJs,
    status: i32,
    nfev: usize,
    njev: usize,
    cost: f64,
    optimality: f64,
}

impl From<CoreSourceSolution> for SourceSolutionJs {
    fn from(value: CoreSourceSolution) -> Self {
        Self {
            position_m: value.position_m,
            origin_time_s: value.origin_time_s,
            covariance: value.covariance.map(Into::into),
            residuals: value.residuals.into_iter().map(Into::into).collect(),
            per_sensor_influence: value
                .per_sensor_influence
                .into_iter()
                .map(Into::into)
                .collect(),
            geometry_quality: value.geometry_quality.into(),
            initial_guess: value.initial_guess.into(),
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
struct SourceCrlbJs {
    dop: DopJs,
    covariance: SourceCovarianceJs,
}

impl From<CoreSourceCrlb> for SourceCrlbJs {
    fn from(value: CoreSourceCrlb) -> Self {
        Self {
            dop: value.dop.into(),
            covariance: value.covariance.into(),
        }
    }
}

fn parse_sensors(value: JsValue) -> Result<Vec<CoreSensor>, JsValue> {
    let sensors: Vec<SensorJs> = from_js(value, "sensors")?;
    Ok(sensors.into_iter().map(Into::into).collect())
}

fn parse_f64_array(value: JsValue, label: &str) -> Result<Vec<f64>, JsValue> {
    from_js(value, label)
}

fn parse_loss(value: Option<&str>) -> Result<Loss, JsValue> {
    match value.unwrap_or("linear") {
        "linear" => Ok(Loss::Linear),
        "softL1" | "soft_l1" => Ok(Loss::SoftL1),
        "huber" => Ok(Loss::Huber),
        "cauchy" => Ok(Loss::Cauchy),
        "arctan" => Ok(Loss::Arctan),
        other => Err(type_error(&format!(
            "unknown loss {other:?}; expected linear, softL1, huber, cauchy, or arctan"
        ))),
    }
}

fn parse_mode_parts(
    mode: Option<&str>,
    reference_sensor: Option<usize>,
) -> Result<CoreSourceSolveMode, JsValue> {
    match mode.unwrap_or("toa") {
        "toa" | "ToA" | "TOA" => Ok(CoreSourceSolveMode::Toa),
        "tdoa" | "TDOA" => Ok(CoreSourceSolveMode::Tdoa {
            reference_sensor: reference_sensor.unwrap_or(0),
        }),
        other => Err(type_error(&format!(
            "unknown source solve mode {other:?}; expected toa or tdoa"
        ))),
    }
}

fn parse_mode_value(value: JsValue) -> Result<CoreSourceSolveMode, JsValue> {
    if value.is_undefined() || value.is_null() {
        return Ok(CoreSourceSolveMode::Toa);
    }
    if let Some(label) = value.as_string() {
        return parse_mode_parts(Some(&label), None);
    }
    let mode: SourceSolveModeObjectJs = from_js(value, "source solve mode")?;
    parse_mode_parts(mode.mode.as_deref(), mode.reference_sensor)
}

fn parse_options(value: JsValue) -> Result<CoreSourceLocateOptions, JsValue> {
    let input = if value.is_undefined() || value.is_null() {
        SourceLocateOptionsJs::default()
    } else {
        from_js(value, "source locate options")?
    };
    let mut options = CoreSourceLocateOptions::default();
    options.mode = parse_mode_parts(input.mode.as_deref(), input.reference_sensor)?;
    if let Some(timing_sigma_s) = input.timing_sigma_s {
        options.timing_sigma_s = timing_sigma_s;
    }
    options.loss = parse_loss(input.loss.as_deref())?;
    if let Some(f_scale_s) = input.f_scale_s {
        options.f_scale_s = f_scale_s;
    }
    options.ftol = input.ftol;
    options.xtol = input.xtol;
    options.gtol = input.gtol;
    options.max_nfev = input.max_nfev;
    Ok(options)
}

/// Return the plain mode value for absolute time-of-arrival solves.
#[wasm_bindgen(js_name = sourceSolveModeToa)]
pub fn source_solve_mode_toa() -> String {
    "toa".to_string()
}

/// Return the plain mode object for TDOA solves against `referenceSensor`.
#[wasm_bindgen(js_name = sourceSolveModeTdoa)]
pub fn source_solve_mode_tdoa(reference_sensor: usize) -> Result<JsValue, JsValue> {
    to_js(&SourceSolveModeTdoaJs {
        mode: "tdoa",
        reference_sensor,
    })
}

/// Locate a source from sensor arrival times.
///
/// `sensors` is an array of `{ positionM, propagationSpeedMS? }`; each
/// `positionM` is a 2D or 3D Cartesian metre vector and all sensors must share
/// the same dimension. `arrivalTimesS` is an aligned seconds array,
/// `propagationSpeedMS` is the call-level speed in metres per second, and
/// `options` may include `mode`, `referenceSensor`, `timingSigmaS`, `loss`,
/// `fScaleS`, `ftol`, `xtol`, `gtol`, and `maxNfev`.
#[wasm_bindgen(js_name = locateSource)]
pub fn locate_source(
    sensors: JsValue,
    arrival_times_s: JsValue,
    propagation_speed_m_s: f64,
    options: JsValue,
) -> Result<JsValue, JsValue> {
    let sensors = parse_sensors(sensors)?;
    let arrival_times_s = parse_f64_array(arrival_times_s, "arrivalTimesS")?;
    let options = parse_options(options)?;
    let solution = core_locate_source(&sensors, &arrival_times_s, propagation_speed_m_s, &options)
        .map_err(source_error)?;
    to_js(&SourceSolutionJs::from(solution))
}

/// Compute the closed-form Chan-Ho seed used by [`locateSource`].
///
/// `mode` is `"toa"`, `"tdoa"`, or `{ mode: "tdoa", referenceSensor }`.
/// Per-sensor speed overrides are not used by the closed-form equations, but
/// they are used by [`locateSource`] during iterative refinement.
#[wasm_bindgen(js_name = chanHoInitialGuess)]
pub fn chan_ho_initial_guess(
    sensors: JsValue,
    arrival_times_s: JsValue,
    propagation_speed_m_s: f64,
    mode: JsValue,
) -> Result<JsValue, JsValue> {
    let sensors = parse_sensors(sensors)?;
    let arrival_times_s = parse_f64_array(arrival_times_s, "arrivalTimesS")?;
    let mode = parse_mode_value(mode)?;
    let guess = core_chan_ho_initial_guess(&sensors, &arrival_times_s, propagation_speed_m_s, mode)
        .map_err(source_error)?;
    to_js(&SourceInitialGuessJs::from(guess))
}

/// Compute timing DOP for a proposed source position.
///
/// `sourcePositionM` is a 2D or 3D Cartesian metre vector in the same frame as
/// the sensors. The returned DOP values multiply timing sigma in seconds to
/// produce metres in the caller's Cartesian axes.
#[wasm_bindgen(js_name = sourceDop)]
pub fn source_dop(
    sensors: JsValue,
    source_position_m: JsValue,
    propagation_speed_m_s: f64,
) -> Result<JsValue, JsValue> {
    let sensors = parse_sensors(sensors)?;
    let source_position_m = parse_f64_array(source_position_m, "sourcePositionM")?;
    let dop = core_source_dop(&sensors, &source_position_m, propagation_speed_m_s)
        .map_err(source_error)?;
    to_js(&DopJs::from(dop))
}

/// Compute the timing Cramer-Rao lower bound for a proposed source position.
///
/// The covariance is `(H^T H)^-1 * timingSigmaS^2`; position blocks are square
/// metres and origin-time variance is square seconds.
#[wasm_bindgen(js_name = sourceCrlb)]
pub fn source_crlb(
    sensors: JsValue,
    source_position_m: JsValue,
    propagation_speed_m_s: f64,
    timing_sigma_s: f64,
) -> Result<JsValue, JsValue> {
    let sensors = parse_sensors(sensors)?;
    let source_position_m = parse_f64_array(source_position_m, "sourcePositionM")?;
    let crlb = core_source_crlb(
        &sensors,
        &source_position_m,
        propagation_speed_m_s,
        timing_sigma_s,
    )
    .map_err(source_error)?;
    to_js(&SourceCrlbJs::from(crlb))
}
