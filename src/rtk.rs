//! RTK baseline solving: static float and validated fixed.
//!
//! The RTK epoch input is a structured record (per-satellite base/rover
//! code+phase plus transmit-time satellite positions) that the engine builds
//! from RINEX+SP3 upstream. This binding only MARSHALS that record: idiomatic JS
//! objects are deserialized into mirror structs, rebuilt into the
//! `sidereon-core` input types, and handed to `sidereon::solve_rtk_float` /
//! `sidereon::solve_rtk_fixed`. No modeling happens here.

use std::collections::BTreeMap;

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use sidereon_core::rtk_filter::{
    defaults::{
        AMBIGUITY_TOL_M, MAX_ITERATIONS, PARTIAL_MIN_AMBIGUITIES, POSITION_TOL_M, RATIO_THRESHOLD,
    },
    AmbiguityScale, AmbiguitySet, Epoch, FixedSolveOpts, FloatBaselineSolution, FloatSolveOpts,
    IntegerStatus, MeasModel, ResidualValidationOpts, SatMeas, StochasticModel,
    ValidatedFixedBaselineSolution, ValidatedFixedSolveOpts,
};

use crate::error::{engine_error, type_error};
use crate::geometry_quality::GeometryQuality;

// --- input objects ----------------------------------------------------------

/// One satellite's base/rover measurements for an RTK epoch.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SatMeasInput {
    sat: String,
    sd_ambiguity_id: String,
    base_code_m: f64,
    base_phase_m: f64,
    rover_code_m: f64,
    rover_phase_m: f64,
    base_tx_pos: [f64; 3],
    rover_tx_pos: [f64; 3],
    pos: [f64; 3],
}

impl SatMeasInput {
    pub(crate) fn to_core(&self) -> SatMeas {
        SatMeas {
            sat: self.sat.clone(),
            sd_ambiguity_id: self.sd_ambiguity_id.clone(),
            base_code_m: self.base_code_m,
            base_phase_m: self.base_phase_m,
            rover_code_m: self.rover_code_m,
            rover_phase_m: self.rover_phase_m,
            base_tx_pos: self.base_tx_pos,
            rover_tx_pos: self.rover_tx_pos,
            pos: self.pos,
        }
    }
}

/// One RTK epoch with reference and non-reference satellite rows.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EpochInput {
    references: Vec<SatMeasInput>,
    nonref: Vec<SatMeasInput>,
    dt_s: f64,
    #[serde(default)]
    velocity_mps: Option<[f64; 3]>,
}

impl EpochInput {
    pub(crate) fn to_core(&self) -> Epoch {
        Epoch {
            references: self.references.iter().map(SatMeasInput::to_core).collect(),
            nonref: self.nonref.iter().map(SatMeasInput::to_core).collect(),
            velocity_mps: self.velocity_mps,
            dt_s: self.dt_s,
        }
    }
}

/// RTK measurement weighting and correction model.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MeasModelInput {
    code_sigma_m: f64,
    phase_sigma_m: f64,
    #[serde(default = "default_true")]
    sagnac: bool,
    #[serde(default = "default_stochastic")]
    stochastic: String,
    #[serde(default)]
    elevation_weighting: bool,
}

fn default_true() -> bool {
    true
}
fn default_stochastic() -> String {
    "simple".to_string()
}

impl MeasModelInput {
    pub(crate) fn to_core(&self) -> Result<MeasModel, JsValue> {
        let stochastic = match self.stochastic.as_str() {
            "simple" => StochasticModel::Simple {
                elevation_weighting: self.elevation_weighting,
            },
            "rtklib" => StochasticModel::Rtklib,
            other => {
                return Err(type_error(&format!(
                    "invalid stochastic model {other:?}: expected \"simple\" or \"rtklib\""
                )))
            }
        };
        Ok(MeasModel {
            code_sigma_m: self.code_sigma_m,
            phase_sigma_m: self.phase_sigma_m,
            sagnac: self.sagnac,
            stochastic,
        })
    }
}

/// Iteration controls for an RTK float solve.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct FloatOptionsInput {
    position_tol_m: f64,
    ambiguity_tol_m: f64,
    max_iterations: usize,
}

impl Default for FloatOptionsInput {
    fn default() -> Self {
        Self {
            position_tol_m: POSITION_TOL_M,
            ambiguity_tol_m: AMBIGUITY_TOL_M,
            max_iterations: MAX_ITERATIONS,
        }
    }
}

impl FloatOptionsInput {
    pub(crate) fn to_core(&self) -> FloatSolveOpts {
        FloatSolveOpts {
            position_tol_m: self.position_tol_m,
            ambiguity_tol_m: self.ambiguity_tol_m,
            max_iterations: self.max_iterations,
        }
    }
}

/// Iteration and integer-search controls for RTK fixed solving.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct FixedOptionsInput {
    position_tol_m: f64,
    ambiguity_tol_m: f64,
    max_iterations: usize,
    ratio_threshold: f64,
    partial_ambiguity_resolution: bool,
    partial_min_ambiguities: usize,
}

impl Default for FixedOptionsInput {
    fn default() -> Self {
        Self {
            position_tol_m: POSITION_TOL_M,
            ambiguity_tol_m: AMBIGUITY_TOL_M,
            max_iterations: MAX_ITERATIONS,
            ratio_threshold: RATIO_THRESHOLD,
            partial_ambiguity_resolution: false,
            partial_min_ambiguities: PARTIAL_MIN_AMBIGUITIES,
        }
    }
}

impl FixedOptionsInput {
    pub(crate) fn to_core(&self) -> FixedSolveOpts {
        FixedSolveOpts {
            position_tol_m: self.position_tol_m,
            ambiguity_tol_m: self.ambiguity_tol_m,
            max_iterations: self.max_iterations,
            ratio_threshold: self.ratio_threshold,
            partial_ambiguity_resolution: self.partial_ambiguity_resolution,
            partial_min_ambiguities: self.partial_min_ambiguities,
        }
    }
}

/// Residual validation controls for RTK fixed solving.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct ResidualOptionsInput {
    threshold_sigma: Option<f64>,
    max_exclusions: usize,
}

impl ResidualOptionsInput {
    pub(crate) fn to_core(&self) -> ResidualValidationOpts {
        ResidualValidationOpts {
            threshold_sigma: self.threshold_sigma,
            max_exclusions: self.max_exclusions,
        }
    }
}

/// Complete typed input bundle for an RTK float solve.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FloatConfigInput {
    epochs: Vec<EpochInput>,
    base: [f64; 3],
    ambiguity_ids: Vec<String>,
    model: MeasModelInput,
    #[serde(default)]
    initial_baseline_m: [f64; 3],
    #[serde(default)]
    options: FloatOptionsInput,
}

/// Complete typed input bundle for an RTK fixed solve.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixedConfigInput {
    epochs: Vec<EpochInput>,
    base: [f64; 3],
    ambiguity_ids: Vec<String>,
    ambiguity_satellites: BTreeMap<String, String>,
    wavelengths_m: BTreeMap<String, f64>,
    offsets_m: BTreeMap<String, f64>,
    model: MeasModelInput,
    #[serde(default)]
    float_options: FloatOptionsInput,
    #[serde(default)]
    fixed_options: FixedOptionsInput,
    #[serde(default)]
    residual_options: ResidualOptionsInput,
    #[serde(default)]
    float_only_systems: Vec<String>,
    #[serde(default)]
    initial_baseline_m: [f64; 3],
}

// --- solve entry points -----------------------------------------------------

/// Solve a static float RTK baseline.
///
/// `config` is a plain object; see the `RtkFloatConfig` TypeScript type. Throws
/// a `TypeError` for malformed input and an `Error` if the solve fails.
#[wasm_bindgen(js_name = solveRtkFloat)]
pub fn solve_rtk_float(config: JsValue) -> Result<RtkFloatSolution, JsValue> {
    let cfg: FloatConfigInput = serde_wasm_bindgen::from_value(config)
        .map_err(|e| type_error(&format!("invalid RTK float config: {e}")))?;

    let epochs: Vec<Epoch> = cfg.epochs.iter().map(EpochInput::to_core).collect();
    let model = cfg.model.to_core()?;

    let inner = sidereon::solve_rtk_float(
        &epochs,
        cfg.base,
        &cfg.ambiguity_ids,
        cfg.initial_baseline_m,
        &model,
        cfg.options.to_core(),
        None,
    )
    .map_err(engine_error)?;

    Ok(RtkFloatSolution { inner })
}

/// Solve a static fixed RTK baseline with residual validation / FDE.
///
/// `config` is a plain object; see the `RtkFixedConfig` TypeScript type. Throws
/// a `TypeError` for malformed input and an `Error` if the solve fails.
#[wasm_bindgen(js_name = solveRtkFixed)]
pub fn solve_rtk_fixed(config: JsValue) -> Result<RtkFixedSolution, JsValue> {
    let cfg: FixedConfigInput = serde_wasm_bindgen::from_value(config)
        .map_err(|e| type_error(&format!("invalid RTK fixed config: {e}")))?;

    let epochs: Vec<Epoch> = cfg.epochs.iter().map(EpochInput::to_core).collect();
    let model = cfg.model.to_core()?;

    let ambiguities = AmbiguitySet {
        ids: &cfg.ambiguity_ids,
        satellites: &cfg.ambiguity_satellites,
        scale: AmbiguityScale {
            wavelengths_m: &cfg.wavelengths_m,
            offsets_m: &cfg.offsets_m,
        },
        float_only_systems: &cfg.float_only_systems,
    };

    let opts = ValidatedFixedSolveOpts {
        float: cfg.float_options.to_core(),
        fixed: cfg.fixed_options.to_core(),
        residual: cfg.residual_options.to_core(),
    };

    let inner = sidereon::solve_rtk_fixed(
        &epochs,
        cfg.base,
        ambiguities,
        cfg.initial_baseline_m,
        &model,
        opts,
        None,
    )
    .map_err(engine_error)?;

    Ok(RtkFixedSolution { inner })
}

/// Map the core integer-status enum to a stable JS string.
fn integer_status_label(status: IntegerStatus) -> String {
    match status {
        IntegerStatus::Fixed => "Fixed".to_string(),
        IntegerStatus::NotFixed => "NotFixed".to_string(),
    }
}

/// Serialize an id-keyed metres map to a plain JS object (not a JS `Map`).
fn ambiguity_object<I>(entries: I) -> JsValue
where
    I: IntoIterator<Item = (String, f64)>,
{
    use serde::Serialize;
    let map: BTreeMap<String, f64> = entries.into_iter().collect();
    let serializer = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
    map.serialize(&serializer).unwrap_or(JsValue::UNDEFINED)
}

// --- result objects ---------------------------------------------------------

/// Static float RTK baseline solution.
#[wasm_bindgen]
pub struct RtkFloatSolution {
    inner: FloatBaselineSolution,
}

#[wasm_bindgen]
impl RtkFloatSolution {
    /// Baseline (rover minus base) as a `Float64Array` `[dx, dy, dz]`, metres.
    #[wasm_bindgen(getter, js_name = baselineM)]
    pub fn baseline_m(&self) -> Vec<f64> {
        self.inner.baseline_m.to_vec()
    }

    /// Float single-difference ambiguities, metres, as an id-keyed object.
    #[wasm_bindgen(getter, js_name = ambiguitiesM)]
    pub fn ambiguities_m(&self) -> JsValue {
        ambiguity_object(self.inner.ambiguities_m.iter().cloned())
    }

    #[wasm_bindgen(getter, js_name = codeRmsM)]
    pub fn code_rms_m(&self) -> f64 {
        self.inner.code_rms_m
    }

    #[wasm_bindgen(getter, js_name = phaseRmsM)]
    pub fn phase_rms_m(&self) -> f64 {
        self.inner.phase_rms_m
    }

    #[wasm_bindgen(getter, js_name = weightedRmsM)]
    pub fn weighted_rms_m(&self) -> f64 {
        self.inner.weighted_rms_m
    }

    #[wasm_bindgen(getter)]
    pub fn converged(&self) -> bool {
        self.inner.converged
    }

    #[wasm_bindgen(getter)]
    pub fn iterations(&self) -> usize {
        self.inner.iterations
    }

    #[wasm_bindgen(getter, js_name = nObservations)]
    pub fn n_observations(&self) -> usize {
        self.inner.n_observations
    }

    /// Geometry observability and covariance-validation diagnostics for the
    /// final double-difference design. `ZeroRedundancy` bounds are unvalidated
    /// for snapshot solves, `Weak` bounds are unclamped, and rank-deficient
    /// designs are returned as a singular-geometry `Error`.
    #[wasm_bindgen(getter, js_name = geometryQuality)]
    pub fn geometry_quality(&self) -> GeometryQuality {
        self.inner.geometry_quality.into()
    }

    /// Observation redundancy, `nObs - nParams`, for the float design.
    #[wasm_bindgen(getter)]
    pub fn redundancy(&self) -> i32 {
        self.inner.geometry_quality.redundancy
    }

    /// Whether residual-based RAIM can test the float design.
    #[wasm_bindgen(getter, js_name = raimCheckable)]
    pub fn raim_checkable(&self) -> bool {
        self.inner.geometry_quality.raim_checkable
    }
}

/// Validated fixed RTK baseline solution.
#[wasm_bindgen]
pub struct RtkFixedSolution {
    inner: ValidatedFixedBaselineSolution,
}

#[wasm_bindgen]
impl RtkFixedSolution {
    /// Fixed (integer-resolved) baseline as a `Float64Array` `[dx, dy, dz]`, metres.
    #[wasm_bindgen(getter, js_name = fixedBaselineM)]
    pub fn fixed_baseline_m(&self) -> Vec<f64> {
        self.inner.fixed_solution.baseline_m.to_vec()
    }

    /// The underlying float baseline as a `Float64Array` `[dx, dy, dz]`, metres.
    #[wasm_bindgen(getter, js_name = floatBaselineM)]
    pub fn float_baseline_m(&self) -> Vec<f64> {
        self.inner.float_solution.baseline_m.to_vec()
    }

    /// Integer ambiguity-fix status: `"Fixed"` or `"NotFixed"`.
    #[wasm_bindgen(getter, js_name = integerStatus)]
    pub fn integer_status(&self) -> String {
        integer_status_label(self.inner.fixed_solution.search.integer_status)
    }

    /// Integer ambiguity ratio, or `undefined` when no ratio was computed.
    #[wasm_bindgen(getter, js_name = integerRatio)]
    pub fn integer_ratio(&self) -> Option<f64> {
        self.inner.fixed_solution.search.integer_ratio
    }

    #[wasm_bindgen(getter, js_name = integerCandidates)]
    pub fn integer_candidates(&self) -> usize {
        self.inner.fixed_solution.search.integer_candidates
    }

    #[wasm_bindgen(getter)]
    pub fn converged(&self) -> bool {
        self.inner.fixed_solution.converged
    }

    /// Geometry observability and covariance-validation diagnostics for the
    /// float design used by the integer-fixed solve.
    #[wasm_bindgen(getter, js_name = geometryQuality)]
    pub fn geometry_quality(&self) -> GeometryQuality {
        self.inner.float_solution.geometry_quality.into()
    }

    /// Observation redundancy, `nObs - nParams`, for the float design used by
    /// the integer-fixed solve.
    #[wasm_bindgen(getter)]
    pub fn redundancy(&self) -> i32 {
        self.inner.float_solution.geometry_quality.redundancy
    }

    /// Whether residual-based RAIM can test the float design used by the
    /// integer-fixed solve.
    #[wasm_bindgen(getter, js_name = raimCheckable)]
    pub fn raim_checkable(&self) -> bool {
        self.inner.float_solution.geometry_quality.raim_checkable
    }
}

#[cfg(test)]
mod drift_tests {
    //! The float and fixed iteration defaults track the canonical core constant
    //! rather than a literal duplicated in this binding.
    use super::*;

    #[test]
    fn float_max_iterations_tracks_core() {
        assert_eq!(FloatOptionsInput::default().max_iterations, MAX_ITERATIONS);
    }

    #[test]
    fn fixed_max_iterations_tracks_core() {
        assert_eq!(FixedOptionsInput::default().max_iterations, MAX_ITERATIONS);
    }

    #[test]
    fn float_tolerances_track_core() {
        let d = FloatOptionsInput::default();
        assert_eq!(d.position_tol_m, POSITION_TOL_M);
        assert_eq!(d.ambiguity_tol_m, AMBIGUITY_TOL_M);
    }

    #[test]
    fn fixed_defaults_track_core() {
        let d = FixedOptionsInput::default();
        assert_eq!(d.position_tol_m, POSITION_TOL_M);
        assert_eq!(d.ambiguity_tol_m, AMBIGUITY_TOL_M);
        assert_eq!(d.ratio_threshold, RATIO_THRESHOLD);
        assert_eq!(d.partial_min_ambiguities, PARTIAL_MIN_AMBIGUITIES);
    }

    #[test]
    fn core_filter_constants_pinned() {
        assert_eq!(POSITION_TOL_M, 1.0e-4);
        assert_eq!(AMBIGUITY_TOL_M, 1.0e-4);
        assert_eq!(RATIO_THRESHOLD, 3.0);
        assert_eq!(PARTIAL_MIN_AMBIGUITIES, 4);
    }
}
