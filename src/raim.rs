//! Standalone range RAIM / fault-detection-and-exclusion over a generic
//! linearized measurement set.
//!
//! Thin wrapper over `sidereon_core::quality::raim_fde_design`. The chi-square
//! global test and the leave-one-out exclusion loop live entirely in the crate;
//! this module only marshals the linearized rows (`{ design_row, residual_m,
//! weight }`) and the options from idiomatic JS objects, runs the core, and
//! packages the protected state correction, covariance, global test, exclusion
//! list, and per-measurement diagnostics back into one JS object. No RAIM logic
//! lives here. This is the design-matrix sibling of the SP3-driven
//! [`crate::qc`] FDE that wraps a full SPP solve.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::quality::{
    raim_fde_design, RangeChiSquareTest, RangeFdeOptions, RangeFdeResult, RangeFdeRow,
    RangeMeasurementDiagnostic,
};

use crate::error::{engine_error, type_error};

/// One linearized range measurement: `{ id, residualM, designRow, weight }`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RangeFdeRowInput {
    id: String,
    residual_m: f64,
    design_row: Vec<f64>,
    weight: f64,
}

impl RangeFdeRowInput {
    fn to_core(&self) -> RangeFdeRow {
        RangeFdeRow {
            id: self.id.clone(),
            residual_m: self.residual_m,
            design_row: self.design_row.clone(),
            weight: self.weight,
        }
    }
}

/// RAIM/FDE options. Every field is optional and falls back to the core default
/// (`pFa` the demo5 `1.0e-3`, no exclusion cap, `minRedundancy` 1).
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct RangeFdeOptionsInput {
    p_fa: Option<f64>,
    max_exclusions: Option<usize>,
    min_redundancy: Option<usize>,
}

impl RangeFdeOptionsInput {
    fn to_core(&self) -> RangeFdeOptions {
        let defaults = RangeFdeOptions::default();
        RangeFdeOptions {
            p_fa: self.p_fa.unwrap_or(defaults.p_fa),
            max_exclusions: self.max_exclusions.unwrap_or(defaults.max_exclusions),
            min_redundancy: self.min_redundancy.unwrap_or(defaults.min_redundancy),
        }
    }
}

// --- result mirror objects --------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChiSquareTestObject {
    weighted_sum_squares: f64,
    dof: isize,
    threshold: Option<f64>,
    testable: bool,
    fault_detected: bool,
}

impl From<&RangeChiSquareTest> for ChiSquareTestObject {
    fn from(t: &RangeChiSquareTest) -> Self {
        Self {
            weighted_sum_squares: t.weighted_sum_squares,
            dof: t.dof,
            threshold: t.threshold,
            testable: t.testable,
            fault_detected: t.fault_detected,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticObject {
    id: String,
    excluded: bool,
    post_fit_residual_m: f64,
    normalized_residual: f64,
}

impl From<&RangeMeasurementDiagnostic> for DiagnosticObject {
    fn from(d: &RangeMeasurementDiagnostic) -> Self {
        Self {
            id: d.id.clone(),
            excluded: d.excluded,
            post_fit_residual_m: d.post_fit_residual_m,
            normalized_residual: d.normalized_residual,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RangeFdeResultObject {
    state_correction: Vec<f64>,
    state_covariance: Vec<Vec<f64>>,
    global_test: ChiSquareTestObject,
    excluded: Vec<String>,
    diagnostics: Vec<DiagnosticObject>,
    iterations: usize,
}

impl From<&RangeFdeResult> for RangeFdeResultObject {
    fn from(r: &RangeFdeResult) -> Self {
        Self {
            state_correction: r.state_correction.clone(),
            state_covariance: r.state_covariance.clone(),
            global_test: ChiSquareTestObject::from(&r.global_test),
            excluded: r.excluded.clone(),
            diagnostics: r.diagnostics.iter().map(DiagnosticObject::from).collect(),
            iterations: r.iterations,
        }
    }
}

/// Run standalone range RAIM/FDE over a linearized measurement set.
///
/// `rows` is an array of `{ id, residualM, designRow, weight }` objects (each
/// `designRow` the measurement's design-matrix row, of length equal to the
/// estimated state dimension). `options` is an optional `{ pFa?, maxExclusions?,
/// minRedundancy? }` object (pass `undefined` for the core defaults). Returns the
/// protected state correction, covariance, global chi-square test, the excluded
/// ids, the per-measurement diagnostics, and the exclusion count. Delegates to
/// `sidereon_core::quality::raim_fde_design`. Throws a `TypeError` for malformed
/// input and an `Error` for a rank-deficient or otherwise rejected set.
#[wasm_bindgen(js_name = raimFdeDesign)]
pub fn raim_fde_design_js(rows: JsValue, options: JsValue) -> Result<JsValue, JsValue> {
    let rows: Vec<RangeFdeRowInput> = serde_wasm_bindgen::from_value(rows)
        .map_err(|e| type_error(&format!("invalid RAIM/FDE rows: {e}")))?;
    let options: RangeFdeOptionsInput = if options.is_undefined() || options.is_null() {
        RangeFdeOptionsInput::default()
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid RAIM/FDE options: {e}")))?
    };

    let core_rows: Vec<RangeFdeRow> = rows.iter().map(RangeFdeRowInput::to_core).collect();
    let result = raim_fde_design(&core_rows, &options.to_core()).map_err(engine_error)?;

    RangeFdeResultObject::from(&result)
        .serialize(&serde_wasm_bindgen::Serializer::new())
        .map_err(|e| type_error(&e.to_string()))
}
