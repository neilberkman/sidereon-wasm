//! Generic data-driven trust-region least squares binding.
//!
//! Thin wrapper over the `trust-region-least-squares` crate's data path: the JS
//! caller selects a built-in residual kind (`linear`, `polynomial`,
//! `exponential`), hands over the data arrays, and the whole trust-region
//! iteration runs in Rust through [`solve_data_problem`]. The residual and
//! Jacobian for every iteration are evaluated Rust-side, so the call pays one
//! boundary crossing in and one out, not one per function evaluation.
//!
//! Under wasm the default in-crate `NalgebraThinSvd` backend is used (the
//! bit-exact host-LAPACK seam needs `dlopen`, which wasm has no equivalent for),
//! so results are tolerance-close to SciPy rather than bit-for-bit; the
//! Linux-x86_64 native bit-exact bar lives in the core crate's own suite.
//!
//! The leave-one-out helper drives the same trust-region engine **serially**:
//! the core `solve_data_problem_drop_one` fans its re-solves across a rayon
//! pool, which wasm cannot enter, so this binding calls the core's serial twin
//! `solve_data_problem_drop_one_serial`, which runs the identical re-solves one
//! masked row at a time and is byte-identical to the parallel version.

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use trust_region_least_squares::batch::{solve_data_problem_drop_one_serial, DropOneReport};
use trust_region_least_squares::data::{solve_data_problem, BuiltinResidual, DataProblem};
use trust_region_least_squares::loss::Loss;
use trust_region_least_squares::trf::{TrfError, TrfResult, XScale};

use crate::error::{engine_error, range_error, type_error};

/// Map a solver error onto the JS exception class a developer expects: caller
/// shape/budget mistakes become `TypeError`, out-of-domain numbers become
/// `RangeError`, and a genuine solve failure carries the engine message.
fn trf_err(err: TrfError) -> JsValue {
    match err {
        TrfError::EmptyResidual
        | TrfError::EmptyParameters
        | TrfError::InsufficientRows { .. }
        | TrfError::SizeOverflow { .. }
        | TrfError::DegreeOverflow { .. }
        | TrfError::InvalidMaxNfev
        | TrfError::InvalidXScaleLength { .. }
        | TrfError::InvalidSliceLength { .. }
        | TrfError::InvalidJacobianLength { .. }
        | TrfError::InvalidResidualLength { .. } => type_error(&err.to_string()),
        TrfError::NonFiniteParameters
        | TrfError::NonFiniteInitialResidual
        | TrfError::InvalidFScale { .. }
        | TrfError::InvalidXScaleValue { .. } => range_error(&err.to_string()),
        TrfError::InvalidSvdOutput(_) | TrfError::Svd(_) => engine_error(err),
    }
}

/// The data-driven least-squares request: the residual kind, its data, the
/// starting point, and the SciPy `least_squares` options.
///
/// `kind` selects the residual and which data fields are read:
/// - `"linear"`: `a` (row-major `m`-by-`n` design matrix), `b` (length `m`),
///   `m`, `n`. `residual_i = (sum_j a[i*n+j] x[j]) - b[i]`.
/// - `"polynomial"`: `degree`, `t`, `y`. Fits `n = degree + 1` coefficients
///   (lowest order first); `residual_i = horner(x, t_i) - y_i`.
/// - `"exponential"`: `t`, `y`. Three parameters `[amp, rate, offset]`;
///   `residual_i = (x[0] exp(x[1] t_i) + x[2]) - y_i`.
///
/// Options default to the SciPy `least_squares` defaults (`linear` loss,
/// `fScale = 1`, unit `xScale`, `ftol = xtol = 1e-8`, `gtol = 1e-10`,
/// `maxNfev = 100 * n`). `xScale` is `"unit"` or `"jac"`; for per-parameter
/// scales pass a positive `xScaleValues` `Float64Array` of length `n` instead
/// (it takes precedence over `xScale`).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DataProblemInput {
    kind: String,
    x0: Vec<f64>,
    a: Option<Vec<f64>>,
    b: Option<Vec<f64>>,
    m: Option<usize>,
    n: Option<usize>,
    degree: Option<usize>,
    t: Option<Vec<f64>>,
    y: Option<Vec<f64>>,
    loss: Option<String>,
    f_scale: Option<f64>,
    x_scale: Option<String>,
    x_scale_values: Option<Vec<f64>>,
    max_nfev: Option<usize>,
    ftol: Option<f64>,
    xtol: Option<f64>,
    gtol: Option<f64>,
}

fn parse_loss(value: Option<&str>) -> Result<Loss, JsValue> {
    match value.unwrap_or("linear") {
        "linear" => Ok(Loss::Linear),
        "soft_l1" => Ok(Loss::SoftL1),
        "huber" => Ok(Loss::Huber),
        "cauchy" => Ok(Loss::Cauchy),
        "arctan" => Ok(Loss::Arctan),
        other => Err(type_error(&format!(
            "unknown loss {other:?}; expected one of linear, soft_l1, huber, cauchy, arctan"
        ))),
    }
}

fn parse_x_scale(input: &DataProblemInput) -> Result<XScale, JsValue> {
    if let Some(values) = &input.x_scale_values {
        return Ok(XScale::Values(values.clone()));
    }
    match input.x_scale.as_deref().unwrap_or("unit") {
        "unit" => Ok(XScale::Unit),
        "jac" => Ok(XScale::Jac),
        other => Err(type_error(&format!(
            "unknown xScale {other:?}; expected \"unit\", \"jac\", or an xScaleValues array"
        ))),
    }
}

fn build_residual(input: &DataProblemInput) -> Result<BuiltinResidual, JsValue> {
    match input.kind.as_str() {
        "linear" => {
            let a = input
                .a
                .clone()
                .ok_or_else(|| type_error("linear kind requires a (design matrix)"))?;
            let b = input
                .b
                .clone()
                .ok_or_else(|| type_error("linear kind requires b (right-hand side)"))?;
            let m = input
                .m
                .ok_or_else(|| type_error("linear kind requires m (row count)"))?;
            let n = input
                .n
                .ok_or_else(|| type_error("linear kind requires n (column count)"))?;
            Ok(BuiltinResidual::Linear { a, b, m, n })
        }
        "polynomial" => {
            let degree = input
                .degree
                .ok_or_else(|| type_error("polynomial kind requires degree"))?;
            let t = input
                .t
                .clone()
                .ok_or_else(|| type_error("polynomial kind requires t (sample abscissae)"))?;
            let y = input
                .y
                .clone()
                .ok_or_else(|| type_error("polynomial kind requires y (sample ordinates)"))?;
            Ok(BuiltinResidual::Polynomial { degree, t, y })
        }
        "exponential" => {
            let t = input
                .t
                .clone()
                .ok_or_else(|| type_error("exponential kind requires t (sample abscissae)"))?;
            let y = input
                .y
                .clone()
                .ok_or_else(|| type_error("exponential kind requires y (sample ordinates)"))?;
            Ok(BuiltinResidual::Exponential { t, y })
        }
        other => Err(type_error(&format!(
            "unknown kind {other:?}; expected one of linear, polynomial, exponential"
        ))),
    }
}

fn build_problem(request: JsValue) -> Result<DataProblem, JsValue> {
    let input: DataProblemInput = serde_wasm_bindgen::from_value(request)
        .map_err(|e| type_error(&format!("invalid least-squares request: {e}")))?;
    let kind = build_residual(&input)?;
    let mut problem = DataProblem::new(kind, input.x0.clone());
    problem.loss = parse_loss(input.loss.as_deref())?;
    problem.x_scale = parse_x_scale(&input)?;
    if let Some(f_scale) = input.f_scale {
        problem.f_scale = f_scale;
    }
    if let Some(max_nfev) = input.max_nfev {
        problem.max_nfev = Some(max_nfev);
    }
    if let Some(ftol) = input.ftol {
        problem.ftol = ftol;
    }
    if let Some(xtol) = input.xtol {
        problem.xtol = xtol;
    }
    if let Some(gtol) = input.gtol {
        problem.gtol = gtol;
    }
    Ok(problem)
}

/// The outcome of one trust-region least-squares solve, mirroring the fields of
/// `scipy.optimize.least_squares`'s `OptimizeResult`.
#[wasm_bindgen]
pub struct LeastSquaresResult {
    inner: TrfResult,
}

#[wasm_bindgen]
impl LeastSquaresResult {
    /// Solution parameter vector, `Float64Array` of length `n`.
    #[wasm_bindgen(getter)]
    pub fn x(&self) -> Vec<f64> {
        self.inner.x.clone()
    }

    /// Optimal cost `0.5 * sum(residual^2)` (after robust reweighting when a
    /// non-linear loss is used).
    #[wasm_bindgen(getter)]
    pub fn cost(&self) -> f64 {
        self.inner.cost
    }

    /// Residual vector at the solution, `Float64Array` of length `m`.
    #[wasm_bindgen(getter)]
    pub fn fun(&self) -> Vec<f64> {
        self.inner.fun.clone()
    }

    /// Row-major `m`-by-`n` Jacobian at the solution, flat `Float64Array` of
    /// length `m * n`.
    #[wasm_bindgen(getter)]
    pub fn jac(&self) -> Vec<f64> {
        self.inner.jac.clone()
    }

    /// Gradient `J^T f` at the solution, `Float64Array` of length `n`.
    #[wasm_bindgen(getter)]
    pub fn grad(&self) -> Vec<f64> {
        self.inner.grad.clone()
    }

    /// First-order optimality `||J^T f||_inf` at the solution.
    #[wasm_bindgen(getter)]
    pub fn optimality(&self) -> f64 {
        self.inner.optimality
    }

    /// Number of residual evaluations.
    #[wasm_bindgen(getter)]
    pub fn nfev(&self) -> usize {
        self.inner.nfev
    }

    /// Number of Jacobian evaluations.
    #[wasm_bindgen(getter)]
    pub fn njev(&self) -> usize {
        self.inner.njev
    }

    /// SciPy-compatible termination status: `0` max evaluations, `1` gtol,
    /// `2` ftol, `3` xtol, `4` both ftol and xtol.
    #[wasm_bindgen(getter)]
    pub fn status(&self) -> i32 {
        self.inner.status
    }

    /// Whether the solve converged (`status > 0`).
    #[wasm_bindgen(getter)]
    pub fn success(&self) -> bool {
        self.inner.success()
    }

    /// Residual row count `m`.
    #[wasm_bindgen(getter)]
    pub fn m(&self) -> usize {
        self.inner.fun.len()
    }

    /// Parameter count `n`.
    #[wasm_bindgen(getter)]
    pub fn n(&self) -> usize {
        self.inner.x.len()
    }
}

/// Solve a generic data-driven least-squares problem.
///
/// `request` is a `DataProblemInput` object: a `kind` (`"linear"`,
/// `"polynomial"`, `"exponential"`) carrying its data arrays, the `x0` starting
/// point, and the SciPy `least_squares` options. The whole trust-region
/// iteration runs in Rust through
/// `trust_region_least_squares::data::solve_data_problem` (the serial entry, the
/// default in-crate SVD backend); no rayon thread pool is entered.
#[wasm_bindgen(js_name = leastSquares)]
pub fn least_squares(request: JsValue) -> Result<LeastSquaresResult, JsValue> {
    let problem = build_problem(request)?;
    let inner = solve_data_problem(&problem).map_err(trf_err)?;
    Ok(LeastSquaresResult { inner })
}

/// A serial leave-one-out sweep: the base solve over all rows plus one re-solve
/// per masked row, with the per-row optimum-cost shift.
#[wasm_bindgen]
pub struct LeastSquaresDropOneReport {
    inner: DropOneReport,
}

#[wasm_bindgen]
impl LeastSquaresDropOneReport {
    /// The solve over the full residual.
    #[wasm_bindgen(getter)]
    pub fn base(&self) -> LeastSquaresResult {
        LeastSquaresResult {
            inner: self.inner.base.clone(),
        }
    }

    /// Number of masked-row re-solves (equals the residual-row count `m`).
    #[wasm_bindgen(getter)]
    pub fn count(&self) -> usize {
        self.inner.drops.len()
    }

    /// The solve with residual row `index` masked out. Throws a `RangeError` for
    /// an out-of-range index.
    #[wasm_bindgen(js_name = dropAt)]
    pub fn drop_at(&self, index: usize) -> Result<LeastSquaresResult, JsValue> {
        self.inner
            .drops
            .get(index)
            .map(|inner| LeastSquaresResult {
                inner: inner.clone(),
            })
            .ok_or_else(|| range_error(&format!("drop index {index} out of range")))
    }

    /// `costDeltas[i] = dropAt(i).cost - base.cost`: how much the optimum cost
    /// moves when row `i` is removed. `Float64Array` of length `count`.
    #[wasm_bindgen(getter, js_name = costDeltas)]
    pub fn cost_deltas(&self) -> Vec<f64> {
        self.inner.cost_delta.clone()
    }
}

/// Serial leave-one-out (jackknife) over a data-driven least-squares problem.
///
/// Delegates to the core's serial leave-one-out entry
/// `trust_region_least_squares::batch::solve_data_problem_drop_one_serial`: the
/// full problem is solved once, then re-solved with each residual row masked in
/// turn. The core's default `solve_data_problem_drop_one` fans these re-solves
/// across a rayon pool that wasm has no threads for, so this binding takes the
/// serial twin, which runs the identical re-solves one row at a time and is
/// byte-identical to the parallel version.
#[wasm_bindgen(js_name = leastSquaresDropOne)]
pub fn least_squares_drop_one(request: JsValue) -> Result<LeastSquaresDropOneReport, JsValue> {
    let problem = build_problem(request)?;
    let inner = solve_data_problem_drop_one_serial(&problem).map_err(trf_err)?;
    Ok(LeastSquaresDropOneReport { inner })
}
