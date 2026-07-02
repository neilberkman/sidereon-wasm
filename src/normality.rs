//! Residual-distribution diagnostics binding: sample moments and normality
//! tests over a residual set.
//!
//! Thin wrappers over `sidereon_core::quality::normality`. The residual set
//! crosses as a `Float64Array`; every statistic (the moment definitions follow
//! `scipy.stats`, the Shapiro-Wilk path is Royston's AS R94) is the core's.
//! Aggregate results cross back as plain `{ ... }` objects.

use serde::Serialize;
use wasm_bindgen::prelude::*;

use sidereon_core::quality::normality::{
    jarque_bera as core_jarque_bera, kurtosis as core_kurtosis, moments as core_moments,
    shapiro_wilk as core_shapiro_wilk, skewness as core_skewness, NormalityError,
};

use crate::error::{range_error, type_error};

fn normality_err(err: NormalityError) -> JsValue {
    match err {
        NormalityError::InsufficientData { .. } => type_error(&err.to_string()),
        NormalityError::NonFinite | NormalityError::ZeroVariance | NormalityError::ZeroRange => {
            range_error(&err.to_string())
        }
    }
}

fn to_object<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(value).map_err(|e| type_error(&e.to_string()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MomentStatsObject {
    mean: f64,
    variance: f64,
    skewness: f64,
    kurtosis_excess: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JarqueBeraObject {
    statistic: f64,
    p_value: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ShapiroWilkObject {
    w: f64,
    p_value: f64,
}

/// Sample skewness of a residual set.
///
/// `x` is a `Float64Array`. `bias` (default `true`) selects the Fisher-Pearson
/// coefficient `g1 = m3 / m2^(3/2)` (`scipy.stats.skew`); `false` applies the
/// sample correction (`scipy.stats.skew(bias=False)`, needs at least three
/// values). Delegates to `sidereon_core::quality::normality::skewness`.
#[wasm_bindgen]
pub fn skewness(x: &[f64], bias: Option<bool>) -> Result<f64, JsValue> {
    core_skewness(x, bias.unwrap_or(true)).map_err(normality_err)
}

/// Sample kurtosis of a residual set.
///
/// `x` is a `Float64Array`. `fisher` (default `true`) returns the excess
/// kurtosis `m4 / m2^2 - 3` (Gaussian -> 0, `scipy.stats.kurtosis`); `false`
/// returns the Pearson kurtosis (Gaussian -> 3). `bias` (default `true`); pass
/// `false` for the sample correction (needs at least four values). Delegates to
/// `sidereon_core::quality::normality::kurtosis`.
#[wasm_bindgen]
pub fn kurtosis(x: &[f64], fisher: Option<bool>, bias: Option<bool>) -> Result<f64, JsValue> {
    core_kurtosis(x, fisher.unwrap_or(true), bias.unwrap_or(true)).map_err(normality_err)
}

/// Mean, variance, skewness, and excess kurtosis of a residual set in one pass.
///
/// `x` is a `Float64Array`; `fisher` and `bias` select the kurtosis convention
/// and the bias correction exactly as in [`skewness`] / [`kurtosis`] (both
/// default `true`). Returns `{ mean, variance, skewness, kurtosisExcess }`; the
/// variance is the biased second central moment. Delegates to
/// `sidereon_core::quality::normality::moments`.
#[wasm_bindgen]
pub fn moments(x: &[f64], fisher: Option<bool>, bias: Option<bool>) -> Result<JsValue, JsValue> {
    let stats =
        core_moments(x, fisher.unwrap_or(true), bias.unwrap_or(true)).map_err(normality_err)?;
    to_object(&MomentStatsObject {
        mean: stats.mean,
        variance: stats.variance,
        skewness: stats.skewness,
        kurtosis_excess: stats.kurtosis_excess,
    })
}

/// Jarque-Bera normality test on a residual set.
///
/// `x` is a `Float64Array` (at least two values). Returns
/// `{ statistic, pValue }` with `statistic = n/6 (S^2 + K^2/4)` (biased moments)
/// and the chi-square(2) survival `pValue = exp(-statistic/2)`, matching
/// `scipy.stats.jarque_bera`. Delegates to
/// `sidereon_core::quality::normality::jarque_bera`.
#[wasm_bindgen(js_name = jarqueBera)]
pub fn jarque_bera(x: &[f64]) -> Result<JsValue, JsValue> {
    let jb = core_jarque_bera(x).map_err(normality_err)?;
    to_object(&JarqueBeraObject {
        statistic: jb.statistic,
        p_value: jb.p_value,
    })
}

/// Shapiro-Wilk normality test on a residual set.
///
/// `x` is a `Float64Array` (at least three values). Returns `{ w, pValue }`,
/// Royston's AS R94 port that `scipy.stats.shapiro` uses; `w` is in `(0, 1]`
/// (closer to one is more Gaussian). Delegates to
/// `sidereon_core::quality::normality::shapiro_wilk`.
#[wasm_bindgen(js_name = shapiroWilk)]
pub fn shapiro_wilk(x: &[f64]) -> Result<JsValue, JsValue> {
    let sw = core_shapiro_wilk(x).map_err(normality_err)?;
    to_object(&ShapiroWilkObject {
        w: sw.w,
        p_value: sw.p_value,
    })
}
