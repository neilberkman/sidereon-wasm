//! Jacobian-derived geometry binding: parameter covariance and the
//! Gauss-Newton Hessian trace.
//!
//! Thin wrappers over `sidereon_core::astro::math::least_squares`. Each
//! Jacobian crosses from JS as a flat row-major `(m, n)` `Float64Array`; this
//! layer reshapes it into the core's `nalgebra` matrix, calls the engine, and
//! flattens the returned covariance back to a row-major `Float64Array`. The
//! linear algebra (the thin SVD path that avoids squaring the condition number)
//! is entirely the core's.

use nalgebra::DMatrix;
use wasm_bindgen::prelude::*;

use sidereon_core::astro::math::least_squares::{
    covariance_from_jacobian as core_covariance_from_jacobian, hessian_trace as core_hessian_trace,
    normal_covariance as core_normal_covariance, SolveError,
};

use crate::error::{engine_error, range_error, type_error};

/// Reshape a flat row-major `(m, n)` buffer into the core's column-major
/// `DMatrix`, rejecting a length mismatch (`TypeError`) and any non-finite entry
/// (`RangeError`).
fn matrix_from_flat(
    name: &str,
    values: &[f64],
    m: usize,
    n: usize,
) -> Result<DMatrix<f64>, JsValue> {
    let expected = m
        .checked_mul(n)
        .ok_or_else(|| range_error(&format!("{name} dimensions m*n overflow")))?;
    if values.len() != expected {
        return Err(type_error(&format!(
            "{name} length ({}) must equal m*n ({expected}) for a row-major {m}-by-{n} matrix",
            values.len()
        )));
    }
    for (i, &v) in values.iter().enumerate() {
        if !v.is_finite() {
            return Err(range_error(&format!("{name}[{i}] must be finite")));
        }
    }
    // `values` is row-major; `DMatrix::from_row_slice` reads it as such.
    Ok(DMatrix::from_row_slice(m, n, values))
}

/// Flatten an `n`-by-`n` `DMatrix` into a row-major `Float64Array`.
fn matrix_to_flat(matrix: &DMatrix<f64>) -> Vec<f64> {
    let (rows, cols) = (matrix.nrows(), matrix.ncols());
    let mut out = Vec::with_capacity(rows * cols);
    for i in 0..rows {
        for j in 0..cols {
            out.push(matrix[(i, j)]);
        }
    }
    out
}

fn solve_err(err: SolveError) -> JsValue {
    match err {
        SolveError::InvalidInput { .. } => range_error(&err.to_string()),
        SolveError::SingularJacobian => engine_error(err),
    }
}

/// Parameter covariance from a design (Jacobian) matrix via the Gauss-Newton
/// normal equations `varianceScale * (J^T J)^-1`, formed from the thin SVD of
/// `J` (not by inverting `J^T J`, so the condition number is not squared).
///
/// `jacobian` is a flat row-major `(m, n)` `Float64Array` with `m >= n`;
/// `varianceScale` (`sigma^2`, non-negative) scales the bare cofactor (pass the
/// post-fit reduced chi-square for the fitted covariance, or `1.0` for
/// `(J^T J)^-1`). Returns the `n`-by-`n` covariance as a flat row-major
/// `Float64Array` of length `n * n`. Throws an `Error` for a rank-deficient
/// Jacobian. Delegates to
/// `sidereon_core::astro::math::least_squares::normal_covariance`.
#[wasm_bindgen(js_name = normalCovariance)]
pub fn normal_covariance(
    jacobian: &[f64],
    m: usize,
    n: usize,
    variance_scale: f64,
) -> Result<Vec<f64>, JsValue> {
    let jac = matrix_from_flat("jacobian", jacobian, m, n)?;
    let cov = core_normal_covariance(&jac, variance_scale).map_err(solve_err)?;
    Ok(matrix_to_flat(&cov))
}

/// Trace of the Gauss-Newton Hessian approximation `J^T J`: the sum of the
/// squared column norms of the Jacobian, with no inverse formed.
///
/// `jacobian` is a flat row-major `(m, n)` `Float64Array`. Delegates to
/// `sidereon_core::astro::math::least_squares::hessian_trace`.
#[wasm_bindgen(js_name = hessianTrace)]
pub fn hessian_trace(jacobian: &[f64], m: usize, n: usize) -> Result<f64, JsValue> {
    let jac = matrix_from_flat("jacobian", jacobian, m, n)?;
    Ok(core_hessian_trace(&jac))
}

/// Fitted parameter covariance from a converged solve, scaling `(J^T J)^-1` by
/// the post-fit reduced chi-square `s_sq = 2 * cost / (m - n)` (the same scale
/// `scipy.optimize.curve_fit` applies to its `pcov`).
///
/// `jacobian` is a flat row-major `(m, n)` `Float64Array` (the Jacobian at the
/// solution), and `cost` is the solve's optimal cost `0.5 * sum(residual^2)`;
/// the degrees of freedom `m - n` (taken from the Jacobian's own shape) must be
/// positive. Pairs naturally with a [`crate::LeastSquaresResult`] (`result.jac`,
/// `result.m`, `result.n`, `result.cost`). Returns the `n`-by-`n` covariance as
/// a flat row-major `Float64Array`. Delegates to
/// `sidereon_core::astro::math::least_squares::covariance_from_jacobian`.
#[wasm_bindgen(js_name = covarianceFromJacobian)]
pub fn covariance_from_jacobian(
    jacobian: &[f64],
    m: usize,
    n: usize,
    cost: f64,
) -> Result<Vec<f64>, JsValue> {
    if !cost.is_finite() {
        return Err(range_error("cost must be finite"));
    }
    let jac = matrix_from_flat("jacobian", jacobian, m, n)?;
    let cov = core_covariance_from_jacobian(&jac, cost).map_err(solve_err)?;
    Ok(matrix_to_flat(&cov))
}
