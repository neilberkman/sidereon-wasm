//! Multi-epoch static positioning binding.
//!
//! Each epoch uses the established SPP request shape, then the binding stacks
//! those epochs into the core `solve_static` entry point. The wrapper returns the
//! shared position, epoch-local clocks, covariance blocks, residuals, influence
//! diagnostics, and solve metadata without changing the core result.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::positioning::{
    solve_static as core_solve_static, RobustConfig as CoreRobustConfig,
    StaticInfluenceStatus as CoreInfluenceStatus, StaticSolution as CoreStaticSolution,
    StaticSolveOptions as CoreStaticSolveOptions, DEFAULT_HUBER_K, DEFAULT_ROBUST_MAX_OUTER,
    DEFAULT_ROBUST_OUTER_TOL_M, DEFAULT_ROBUST_SCALE_FLOOR_M,
};
use sidereon_core::positioning::{EphemerisSource, StaticEpoch as CoreStaticEpoch};
use sidereon_core::{GnssSystem, Wgs84Geodetic};

use crate::error::{engine_error, range_error, type_error};
use crate::geometry_quality::GeometryQuality;
use crate::marshal::mat3_flat;
use crate::sp3::Sp3;
use crate::spp;

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| engine_error(format!("failed to serialize result: {e}")))
}

fn system_label(system: GnssSystem) -> &'static str {
    system.as_str()
}

fn influence_status_label_core(status: CoreInfluenceStatus) -> &'static str {
    match status {
        CoreInfluenceStatus::Solved => "solved",
        CoreInfluenceStatus::TooFewMeasurements => "tooFewMeasurements",
        CoreInfluenceStatus::SingularGeometry => "singularGeometry",
        CoreInfluenceStatus::InvalidInput => "invalidInput",
        CoreInfluenceStatus::EphemerisUnavailable => "ephemerisUnavailable",
        CoreInfluenceStatus::SolveFailed => "solveFailed",
    }
}

fn influence_status_name(status: StaticInfluenceStatus) -> &'static str {
    match status {
        StaticInfluenceStatus::Solved => "solved",
        StaticInfluenceStatus::TooFewMeasurements => "tooFewMeasurements",
        StaticInfluenceStatus::SingularGeometry => "singularGeometry",
        StaticInfluenceStatus::InvalidInput => "invalidInput",
        StaticInfluenceStatus::EphemerisUnavailable => "ephemerisUnavailable",
        StaticInfluenceStatus::SolveFailed => "solveFailed",
    }
}

fn rejection_reason_label(reason: sidereon_core::positioning::RejectionReason) -> &'static str {
    match reason {
        sidereon_core::positioning::RejectionReason::NoEphemeris => "noEphemeris",
        sidereon_core::positioning::RejectionReason::LowElevation => "lowElevation",
        sidereon_core::positioning::RejectionReason::SbasWithdrawn => "sbasWithdrawn",
        sidereon_core::positioning::RejectionReason::SbasIonoUncovered => "sbasIonoUncovered",
    }
}

fn robust_config(input: &RobustInput) -> Result<CoreRobustConfig, JsValue> {
    let huber_k = input.huber_k.unwrap_or(DEFAULT_HUBER_K);
    let scale_floor_m = input.scale_floor_m.unwrap_or(DEFAULT_ROBUST_SCALE_FLOOR_M);
    let outer_tol_m = input.outer_tol_m.unwrap_or(DEFAULT_ROBUST_OUTER_TOL_M);
    let max_outer = input.max_outer.unwrap_or(DEFAULT_ROBUST_MAX_OUTER);
    if !(huber_k.is_finite() && huber_k > 0.0) {
        return Err(range_error(
            "robust.huberK must be a finite positive number",
        ));
    }
    if !(scale_floor_m.is_finite() && scale_floor_m > 0.0) {
        return Err(range_error(
            "robust.scaleFloorM must be a finite positive number",
        ));
    }
    if !(outer_tol_m.is_finite() && outer_tol_m > 0.0) {
        return Err(range_error(
            "robust.outerTolM must be a finite positive number",
        ));
    }
    if max_outer < 1 {
        return Err(range_error("robust.maxOuter must be at least 1"));
    }
    Ok(CoreRobustConfig {
        huber_k,
        scale_floor_m,
        max_outer,
        outer_tol_m,
    })
}

fn decode_epoch_array(epochs: JsValue) -> Result<Vec<JsValue>, JsValue> {
    if !js_sys::Array::is_array(&epochs) {
        return Err(type_error("epochs must be an array"));
    }
    let array = js_sys::Array::from(&epochs);
    Ok((0..array.length()).map(|idx| array.get(idx)).collect())
}

fn static_options(
    options: JsValue,
    first_initial_guess: Option<[f64; 4]>,
    first_robust: Option<CoreRobustConfig>,
) -> Result<CoreStaticSolveOptions, JsValue> {
    let input: StaticSolveOptionsInput = if options.is_undefined() || options.is_null() {
        StaticSolveOptionsInput::default()
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid static solve options: {e}")))?
    };
    let initial_position_m = input
        .initial_position_m
        .or_else(|| first_initial_guess.map(|guess| [guess[0], guess[1], guess[2]]))
        .unwrap_or([0.0; 3]);
    let robust = match input.robust {
        Some(robust) => Some(robust_config(&robust)?),
        None => first_robust,
    };
    Ok(CoreStaticSolveOptions {
        initial_position_m,
        with_geodetic: input.with_geodetic.unwrap_or(true),
        robust,
    })
}

fn solve_over<E>(eph: &E, epochs: JsValue, options: JsValue) -> Result<StaticSolution, JsValue>
where
    E: EphemerisSource,
{
    let epoch_values = decode_epoch_array(epochs)?;
    let mut core_epochs = Vec::with_capacity(epoch_values.len());
    let mut first_initial_guess = None;
    let mut first_robust = None;

    for value in epoch_values {
        let (inputs, _with_geodetic) = spp::build_inputs(value)?;
        if first_initial_guess.is_none() {
            first_initial_guess = Some(inputs.initial_guess);
            first_robust = inputs.robust;
        }
        core_epochs.push(CoreStaticEpoch::from_solve_inputs(inputs));
    }

    let options = static_options(options, first_initial_guess, first_robust)?;
    let inner = core_solve_static(eph, &core_epochs, options).map_err(engine_error)?;
    Ok(StaticSolution { inner })
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct StaticSolveOptionsInput {
    initial_position_m: Option<[f64; 3]>,
    with_geodetic: Option<bool>,
    robust: Option<RobustInput>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct RobustInput {
    huber_k: Option<f64>,
    scale_floor_m: Option<f64>,
    max_outer: Option<usize>,
    outer_tol_m: Option<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StaticClockBiasJs {
    epoch_index: usize,
    system: String,
    clock_s: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StaticResidualJs {
    epoch_index: usize,
    satellite_id: String,
    residual_m: f64,
    base_weight: f64,
    effective_weight: f64,
    robust_weight_ratio: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RejectedSatJs {
    satellite_id: String,
    reason: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StaticEpochInfluenceJs {
    epoch_index: usize,
    omitted_measurements: usize,
    status: &'static str,
    position_delta_m: Option<[f64; 3]>,
    position_delta_norm_m: Option<f64>,
    residual_rms_m: Option<f64>,
    min_robust_weight_ratio: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StaticSatelliteInfluenceJs {
    epoch_index: usize,
    satellite_id: String,
    status: &'static str,
    position_delta_m: Option<[f64; 3]>,
    position_delta_norm_m: Option<f64>,
    residual_rms_m: Option<f64>,
    residual_m: f64,
    base_weight: f64,
    effective_weight: f64,
    robust_weight_ratio: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StaticSatelliteBatchInfluenceJs {
    satellite_id: String,
    omitted_measurements: usize,
    status: &'static str,
    position_delta_m: Option<[f64; 3]>,
    position_delta_norm_m: Option<f64>,
    residual_rms_m: Option<f64>,
    min_robust_weight_ratio: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StaticMetadataJs {
    iterations: usize,
    converged: bool,
    status: String,
    outer_iterations: usize,
    final_robust_scale_m: Option<f64>,
    used_measurements: usize,
    n_parameters: usize,
    redundancy: isize,
}

/// Status for a leave-one-out static positioning diagnostic.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaticInfluenceStatus {
    /// The diagnostic solve completed.
    Solved,
    /// The omitted subset left too few measurements.
    TooFewMeasurements,
    /// The diagnostic geometry was singular.
    SingularGeometry,
    /// Input validation failed for the diagnostic subset.
    InvalidInput,
    /// Ephemeris was unavailable for the diagnostic subset.
    EphemerisUnavailable,
    /// The diagnostic subset failed for another reason.
    SolveFailed,
}

impl From<CoreInfluenceStatus> for StaticInfluenceStatus {
    fn from(value: CoreInfluenceStatus) -> Self {
        match value {
            CoreInfluenceStatus::Solved => Self::Solved,
            CoreInfluenceStatus::TooFewMeasurements => Self::TooFewMeasurements,
            CoreInfluenceStatus::SingularGeometry => Self::SingularGeometry,
            CoreInfluenceStatus::InvalidInput => Self::InvalidInput,
            CoreInfluenceStatus::EphemerisUnavailable => Self::EphemerisUnavailable,
            CoreInfluenceStatus::SolveFailed => Self::SolveFailed,
        }
    }
}

/// Stable string label for a [`StaticInfluenceStatus`] enum value.
#[wasm_bindgen(js_name = staticInfluenceStatusLabel)]
pub fn static_influence_status_label(status: StaticInfluenceStatus) -> String {
    influence_status_name(status).to_string()
}

/// Multi-epoch static receiver solution.
#[wasm_bindgen]
pub struct StaticSolution {
    inner: CoreStaticSolution,
}

#[wasm_bindgen]
impl StaticSolution {
    /// ECEF position as `[x, y, z]`, metres.
    #[wasm_bindgen(getter, js_name = positionM)]
    pub fn position_m(&self) -> Vec<f64> {
        self.inner.position.as_array().to_vec()
    }

    /// ECEF X, metres.
    #[wasm_bindgen(getter, js_name = xM)]
    pub fn x_m(&self) -> f64 {
        self.inner.position.x_m
    }

    /// ECEF Y, metres.
    #[wasm_bindgen(getter, js_name = yM)]
    pub fn y_m(&self) -> f64 {
        self.inner.position.y_m
    }

    /// ECEF Z, metres.
    #[wasm_bindgen(getter, js_name = zM)]
    pub fn z_m(&self) -> f64 {
        self.inner.position.z_m
    }

    /// `[latRad, lonRad, heightM]` when geodetic output was requested.
    #[wasm_bindgen(getter)]
    pub fn geodetic(&self) -> Option<Vec<f64>> {
        self.inner.geodetic.map(
            |Wgs84Geodetic {
                 lat_rad,
                 lon_rad,
                 height_m,
             }| vec![lat_rad, lon_rad, height_m],
        )
    }

    /// Epoch-local receiver clocks as `{ epochIndex, system, clockS }[]`.
    #[wasm_bindgen(getter, js_name = perEpochClocks)]
    pub fn per_epoch_clocks(&self) -> Result<JsValue, JsValue> {
        let clocks = self
            .inner
            .per_epoch_clock
            .iter()
            .map(|clock| StaticClockBiasJs {
                epoch_index: clock.epoch_index,
                system: system_label(clock.system).to_string(),
                clock_s: clock.clock_s,
            })
            .collect::<Vec<_>>();
        to_js(&clocks)
    }

    /// Full state covariance, flat row-major square matrix in square metres.
    #[wasm_bindgen(getter, js_name = stateCovarianceM2)]
    pub fn state_covariance_m2(&self) -> Vec<f64> {
        self.inner
            .covariance
            .state_m2
            .iter()
            .flatten()
            .copied()
            .collect()
    }

    /// State covariance matrix dimension.
    #[wasm_bindgen(getter, js_name = stateParameterCount)]
    pub fn state_parameter_count(&self) -> usize {
        self.inner.covariance.state_m2.len()
    }

    /// ECEF position covariance, flat row-major 3-by-3 in square metres.
    #[wasm_bindgen(getter, js_name = positionCovarianceEcefM2)]
    pub fn position_covariance_ecef_m2(&self) -> Vec<f64> {
        mat3_flat(&self.inner.covariance.position_ecef_m2)
    }

    /// ENU position covariance, flat row-major 3-by-3 in square metres.
    #[wasm_bindgen(getter, js_name = positionCovarianceEnuM2)]
    pub fn position_covariance_enu_m2(&self) -> Vec<f64> {
        mat3_flat(&self.inner.covariance.position_enu_m2)
    }

    /// Post-fit residuals as `{ epochIndex, satelliteId, residualM, ... }[]`.
    #[wasm_bindgen(getter)]
    pub fn residuals(&self) -> Result<JsValue, JsValue> {
        let residuals = self
            .inner
            .residuals_m
            .iter()
            .map(|row| StaticResidualJs {
                epoch_index: row.epoch_index,
                satellite_id: row.satellite_id.to_string(),
                residual_m: row.residual_m,
                base_weight: row.base_weight,
                effective_weight: row.effective_weight,
                robust_weight_ratio: row.robust_weight_ratio,
            })
            .collect::<Vec<_>>();
        to_js(&residuals)
    }

    /// Root-mean-square of the unweighted post-fit residuals, metres.
    #[wasm_bindgen(getter, js_name = residualRmsM)]
    pub fn residual_rms_m(&self) -> f64 {
        self.inner.residual_rms_m()
    }

    /// Used satellite tokens grouped by input epoch.
    #[wasm_bindgen(getter, js_name = usedSats)]
    pub fn used_sats(&self) -> Result<JsValue, JsValue> {
        let used = self
            .inner
            .used_sats
            .iter()
            .map(|epoch| epoch.iter().map(ToString::to_string).collect::<Vec<_>>())
            .collect::<Vec<_>>();
        to_js(&used)
    }

    /// Rejected satellites grouped by input epoch.
    #[wasm_bindgen(getter, js_name = rejectedSats)]
    pub fn rejected_sats(&self) -> Result<JsValue, JsValue> {
        let rejected = self
            .inner
            .rejected_sats
            .iter()
            .map(|epoch| {
                epoch
                    .iter()
                    .map(|row| RejectedSatJs {
                        satellite_id: row.satellite_id.to_string(),
                        reason: rejection_reason_label(row.reason),
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        to_js(&rejected)
    }

    /// Geometry observability and covariance-validation diagnostics.
    #[wasm_bindgen(getter, js_name = geometryQuality)]
    pub fn geometry_quality(&self) -> GeometryQuality {
        self.inner.geometry_quality.into()
    }

    /// Solver iteration, convergence, and redundancy metadata.
    #[wasm_bindgen(getter)]
    pub fn metadata(&self) -> Result<JsValue, JsValue> {
        to_js(&StaticMetadataJs {
            iterations: self.inner.metadata.iterations,
            converged: self.inner.metadata.converged,
            status: format!("{:?}", self.inner.metadata.status),
            outer_iterations: self.inner.metadata.outer_iterations,
            final_robust_scale_m: self.inner.metadata.final_robust_scale_m,
            used_measurements: self.inner.metadata.used_measurements,
            n_parameters: self.inner.metadata.n_parameters,
            redundancy: self.inner.metadata.redundancy,
        })
    }

    /// Leave-one-epoch-out diagnostics.
    #[wasm_bindgen(getter, js_name = perEpochInfluence)]
    pub fn per_epoch_influence(&self) -> Result<JsValue, JsValue> {
        let values = self
            .inner
            .per_epoch_influence
            .iter()
            .map(|row| StaticEpochInfluenceJs {
                epoch_index: row.epoch_index,
                omitted_measurements: row.omitted_measurements,
                status: influence_status_label_core(row.status),
                position_delta_m: row.position_delta_m,
                position_delta_norm_m: row.position_delta_norm_m,
                residual_rms_m: row.residual_rms_m,
                min_robust_weight_ratio: row.min_robust_weight_ratio,
            })
            .collect::<Vec<_>>();
        to_js(&values)
    }

    /// Leave-one-satellite-out diagnostics per epoch.
    #[wasm_bindgen(getter, js_name = perSatelliteInfluence)]
    pub fn per_satellite_influence(&self) -> Result<JsValue, JsValue> {
        let values = self
            .inner
            .per_satellite_influence
            .iter()
            .map(|row| StaticSatelliteInfluenceJs {
                epoch_index: row.epoch_index,
                satellite_id: row.satellite_id.to_string(),
                status: influence_status_label_core(row.status),
                position_delta_m: row.position_delta_m,
                position_delta_norm_m: row.position_delta_norm_m,
                residual_rms_m: row.residual_rms_m,
                residual_m: row.residual_m,
                base_weight: row.base_weight,
                effective_weight: row.effective_weight,
                robust_weight_ratio: row.robust_weight_ratio,
            })
            .collect::<Vec<_>>();
        to_js(&values)
    }

    /// Leave-one-satellite-out diagnostics across every epoch.
    #[wasm_bindgen(getter, js_name = perSatelliteBatchInfluence)]
    pub fn per_satellite_batch_influence(&self) -> Result<JsValue, JsValue> {
        let values = self
            .inner
            .per_satellite_batch_influence
            .iter()
            .map(|row| StaticSatelliteBatchInfluenceJs {
                satellite_id: row.satellite_id.to_string(),
                omitted_measurements: row.omitted_measurements,
                status: influence_status_label_core(row.status),
                position_delta_m: row.position_delta_m,
                position_delta_norm_m: row.position_delta_norm_m,
                residual_rms_m: row.residual_rms_m,
                min_robust_weight_ratio: row.min_robust_weight_ratio,
            })
            .collect::<Vec<_>>();
        to_js(&values)
    }
}

/// Solve one static receiver position from multiple SPP-shaped epochs over SP3.
#[wasm_bindgen(js_name = solveStatic)]
pub fn solve_static(
    sp3: &Sp3,
    epochs: JsValue,
    options: JsValue,
) -> Result<StaticSolution, JsValue> {
    solve_over(&sp3.inner, epochs, options)
}

pub(crate) fn solve_static_sp3(
    sp3: &sidereon_core::ephemeris::Sp3,
    epochs: JsValue,
    options: JsValue,
) -> Result<StaticSolution, JsValue> {
    solve_over(sp3, epochs, options)
}
