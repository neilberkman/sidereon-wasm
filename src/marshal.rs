//! Small marshalling helpers shared by the astro/frames/events bindings.
//!
//! The numpy-shaped batch APIs in the Python binding cross to JS as flat
//! `Float64Array`s: an `(n, 3)` matrix is a length-`3n` row-major array, a `(3,)`
//! vector is length 3, and a `(3, 3)` / `(2, 2)` matrix is a flat length-9 / 4
//! row-major array. These helpers validate and reshape those buffers and surface
//! the same shape/finite rejections the Python layer raises, as JS `TypeError` /
//! `RangeError`.

use wasm_bindgen::prelude::*;

use sidereon::passes::UtcInstant;
use sidereon_core::astro::covariance::{Covariance6, Covariance6Error};

use crate::error::{range_error, type_error};

/// Build TEME instants from unix-microsecond epochs (a `BigInt64Array`).
pub fn instants(epochs_unix_us: &[i64]) -> Vec<UtcInstant> {
    epochs_unix_us
        .iter()
        .map(|&us| UtcInstant::from_unix_microseconds(us))
        .collect()
}

/// Read a length-3 vector, rejecting a wrong length (`TypeError`).
pub fn vec3(name: &str, values: &[f64]) -> Result<[f64; 3], JsValue> {
    if values.len() != 3 {
        return Err(type_error(&format!(
            "{name} must have length 3, got {}",
            values.len()
        )));
    }
    Ok([values[0], values[1], values[2]])
}

/// Read a length-3 vector and require every element finite (`RangeError`).
pub fn vec3_finite(name: &str, values: &[f64]) -> Result<[f64; 3], JsValue> {
    let v = vec3(name, values)?;
    for (i, x) in v.iter().enumerate() {
        if !x.is_finite() {
            return Err(range_error(&format!("{name}[{i}] must be a finite number")));
        }
    }
    Ok(v)
}

/// Reshape a flat row-major `(n, 3)` buffer into rows, rejecting a length that is
/// not a multiple of 3 (`TypeError`) and, when `require_finite`, any non-finite
/// element (`RangeError`).
pub fn rows3(name: &str, values: &[f64], require_finite: bool) -> Result<Vec<[f64; 3]>, JsValue> {
    if !values.len().is_multiple_of(3) {
        return Err(type_error(&format!(
            "{name} length ({}) must be a multiple of 3 (flat row-major n-by-3)",
            values.len()
        )));
    }
    let mut rows = Vec::with_capacity(values.len() / 3);
    for chunk in values.chunks_exact(3) {
        if require_finite && chunk.iter().any(|x| !x.is_finite()) {
            return Err(range_error(&format!(
                "{name} must contain only finite values"
            )));
        }
        rows.push([chunk[0], chunk[1], chunk[2]]);
    }
    Ok(rows)
}

/// Require a non-empty flat batch (`TypeError`).
pub fn reject_empty(name: &str, rows: &[[f64; 3]]) -> Result<(), JsValue> {
    if rows.is_empty() {
        return Err(type_error(&format!("{name} must not be empty")));
    }
    Ok(())
}

/// Require two batches share the same row count (`TypeError`).
pub fn same_len(a_name: &str, a: usize, b_name: &str, b: usize) -> Result<(), JsValue> {
    if a != b {
        return Err(type_error(&format!(
            "{a_name} ({a}) and {b_name} ({b}) must have the same length"
        )));
    }
    Ok(())
}

/// Flatten rows into a row-major `Float64Array`.
pub fn flat3(rows: &[[f64; 3]]) -> Vec<f64> {
    let mut out = Vec::with_capacity(rows.len() * 3);
    for r in rows {
        out.extend_from_slice(r);
    }
    out
}

/// Flatten a 3x3 matrix into a length-9 row-major `Float64Array`.
pub fn mat3_flat(m: &[[f64; 3]; 3]) -> Vec<f64> {
    let mut out = Vec::with_capacity(9);
    for row in m {
        out.extend_from_slice(row);
    }
    out
}

/// Read a length-9 row-major buffer as a 3x3 matrix (`TypeError` on bad length).
pub fn mat3_from_flat(name: &str, values: &[f64]) -> Result<[[f64; 3]; 3], JsValue> {
    if values.len() != 9 {
        return Err(type_error(&format!(
            "{name} must have length 9 (flat row-major 3-by-3), got {}",
            values.len()
        )));
    }
    Ok([
        [values[0], values[1], values[2]],
        [values[3], values[4], values[5]],
        [values[6], values[7], values[8]],
    ])
}

/// Flatten a 2x2 matrix into a length-4 row-major `Float64Array`.
pub fn mat2_flat(m: &[[f64; 2]; 2]) -> Vec<f64> {
    vec![m[0][0], m[0][1], m[1][0], m[1][1]]
}

/// Flatten a typed 6x6 state covariance into a length-36 row-major
/// `Float64Array`, the shape the CCSDS OEM/OPM covariance getters cross on.
pub fn covariance6_flat(cov: &Covariance6) -> Vec<f64> {
    let mut out = Vec::with_capacity(36);
    for row in cov.as_matrix() {
        out.extend_from_slice(row);
    }
    out
}

/// Read a length-36 row-major buffer as a validated 6x6 state covariance.
/// Rejects a wrong length (`TypeError`) and a non-finite, asymmetric, or
/// non-positive-semidefinite matrix (`RangeError`).
pub fn covariance6_from_flat(name: &str, values: &[f64]) -> Result<Covariance6, JsValue> {
    if values.len() != 36 {
        return Err(type_error(&format!(
            "{name} must have length 36 (flat row-major 6-by-6), got {}",
            values.len()
        )));
    }
    let mut matrix = [[0.0_f64; 6]; 6];
    for (i, row) in matrix.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = values[i * 6 + j];
        }
    }
    Covariance6::try_from_matrix(matrix)
        .map_err(|e| range_error(&format!("{name} {}", covariance6_error_message(e))))
}

fn covariance6_error_message(error: Covariance6Error) -> &'static str {
    match error {
        Covariance6Error::NonFinite => "must contain only finite values",
        Covariance6Error::Asymmetric => "must be symmetric",
        Covariance6Error::NotPositiveSemidefinite => "must be positive semidefinite",
    }
}
