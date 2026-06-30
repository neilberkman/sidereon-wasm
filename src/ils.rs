//! Integer-ambiguity resolution kernels: standalone LAMBDA and bounded
//! integer-least-squares search.
//!
//! Thin marshaling over [`sidereon_core::ils`]. The float ambiguities cross as a
//! `Float64Array`, the covariance as a row-major `number[][]`, and the result as
//! a plain `IlsResult` object (serde). All search math — Gaussian elimination,
//! LtDL decorrelation, the MLAMBDA depth-first search, scoring and the ratio test
//! — lives in `sidereon-core`; this module adds no modeling of its own. The fixed
//! integer vector is a small-magnitude cycle count, so it crosses as `number[]`
//! (exact in f64), not a BigInt array.

use serde::Serialize;
use wasm_bindgen::prelude::*;

use sidereon_core::ils::{
    bounded_ils_search, lambda_ils_search, IlsError, IlsResult as CoreIlsResult,
};

use crate::error::{engine_error, range_error, type_error};

fn ils_err(err: IlsError) -> JsValue {
    match err {
        IlsError::InvalidDimensions { .. } => type_error(&err.to_string()),
        IlsError::NonFinite => range_error(&err.to_string()),
        IlsError::InvalidInput { .. } => range_error(&err.to_string()),
        IlsError::Singular
        | IlsError::NoCandidates(_)
        | IlsError::TooManyCandidates { .. }
        | IlsError::SearchLimitExceeded => engine_error(err),
    }
}

/// Plain `IlsResult` object returned by both search kernels.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IlsResultJs {
    /// Best integer vector, parallel to the input `floatCycles`.
    fixed: Vec<i64>,
    /// Whether the ratio test passes at the requested threshold.
    fixed_status: bool,
    /// Runner-up / best score ratio.
    ratio: f64,
    /// Best (lowest) quadratic score.
    best_score: f64,
    /// Runner-up score, present only when a second lattice point exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    second_best_score: Option<f64>,
    /// Number of lattice points evaluated.
    candidates_evaluated: usize,
    /// Symmetrized covariance actually used (`number[][]`).
    covariance: Vec<Vec<f64>>,
    /// Symmetrized inverse covariance (`number[][]`).
    covariance_inverse: Vec<Vec<f64>>,
}

impl From<CoreIlsResult> for IlsResultJs {
    fn from(r: CoreIlsResult) -> Self {
        Self {
            fixed: r.fixed,
            fixed_status: r.fixed_status,
            ratio: r.ratio,
            best_score: r.best_score,
            second_best_score: r.second_best_score,
            candidates_evaluated: r.candidates_evaluated,
            covariance: r.covariance,
            covariance_inverse: r.covariance_inverse,
        }
    }
}

fn covariance_from_js(value: JsValue) -> Result<Vec<Vec<f64>>, JsValue> {
    serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid covariance (expected number[][]): {e}")))
}

/// Read a non-negative integer count from a JS number, throwing a `RangeError`
/// rather than letting a negative value wrap through the wasm integer ABI into a
/// huge unsigned bound.
fn nonneg_int(value: f64, name: &str) -> Result<u64, JsValue> {
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > u64::MAX as f64 {
        return Err(range_error(&format!(
            "{name} must be a non-negative integer"
        )));
    }
    Ok(value as u64)
}

fn result_to_js(result: CoreIlsResult) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(&IlsResultJs::from(result))
        .map_err(|e| engine_error(e.to_string()))
}

/// Resolve integer ambiguities with the LAMBDA method (RTKLIB `lambda()` port).
///
/// `floatCycles` is the real-valued ambiguity vector (`Float64Array`),
/// `covariance` is its `n x n` covariance as a row-major `number[][]`, and
/// `ratioThreshold` is the ratio-test acceptance threshold (RTKLIB's default is
/// `3.0`). Finds the true integer-least-squares optimum and runner-up for any
/// positive-definite covariance — no search box, no combinatorial blow-up.
/// Returns an `IlsResult`. Throws a `TypeError` on a malformed shape, a
/// `RangeError` on a non-finite or out-of-domain input, and an `Error` on a
/// singular covariance or a non-converging search.
#[wasm_bindgen(js_name = lambdaIlsSearch)]
pub fn lambda_ils_search_js(
    float_cycles: &[f64],
    covariance: JsValue,
    ratio_threshold: f64,
) -> Result<JsValue, JsValue> {
    let covariance = covariance_from_js(covariance)?;
    let result = lambda_ils_search(float_cycles, &covariance, ratio_threshold).map_err(ils_err)?;
    result_to_js(result)
}

/// Resolve integer ambiguities with a bounded lattice search.
///
/// `floatCycles` and `covariance` match [`lambdaIlsSearch`]. `radius` is the
/// per-ambiguity integer search half-width (the lattice spans `radius` integers
/// either side of each rounded float), `candidateLimit` caps the number of
/// lattice points evaluated before the search aborts, and `ratioThreshold` is the
/// ratio-test acceptance threshold. Correct in the weakly-correlated regime and a
/// drop-in match for `lambdaIlsSearch` there; on strongly-correlated geometry
/// prefer `lambdaIlsSearch`. Returns an `IlsResult`. Throws a `TypeError` on a
/// malformed shape, a `RangeError` on a non-finite or out-of-domain input, and an
/// `Error` on a singular covariance or a lattice that exceeds `candidateLimit`.
#[wasm_bindgen(js_name = boundedIlsSearch)]
pub fn bounded_ils_search_js(
    float_cycles: &[f64],
    covariance: JsValue,
    radius: f64,
    candidate_limit: f64,
    ratio_threshold: f64,
) -> Result<JsValue, JsValue> {
    let covariance = covariance_from_js(covariance)?;
    // Take the integer bounds as JS numbers and validate here: a wasm `u32`/`usize`
    // argument would silently wrap a negative value into a huge bound instead of
    // throwing.
    let radius = i64::try_from(nonneg_int(radius, "radius")?)
        .map_err(|_| range_error("radius is too large"))?;
    let candidate_limit = usize::try_from(nonneg_int(candidate_limit, "candidateLimit")?)
        .map_err(|_| range_error("candidateLimit is too large"))?;
    let result = bounded_ils_search(
        float_cycles,
        &covariance,
        radius,
        candidate_limit,
        ratio_threshold,
    )
    .map_err(ils_err)?;
    result_to_js(result)
}
