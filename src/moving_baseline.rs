//! Moving-baseline RTK: both receivers move, each epoch carries its own base
//! position (RTKLIB "moving-base").
//!
//! Thin wrapper over `sidereon_core::rtk_filter::moving_baseline`. The double
//! difference cancels the base position, so the float / fixed solvers are the
//! same machinery as the static RTK binding; the only new input is the per-epoch
//! base ECEF position. This module reuses the RTK epoch / measurement-model /
//! solver-option marshalling from [`crate::rtk`], owns the per-epoch base + the
//! shared ambiguity set, builds the borrowed `MovingBaselineEpoch` views, and
//! hands them to `solve_moving_baseline`. No modeling happens here.

use std::collections::BTreeMap;

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use sidereon_core::rtk_filter::{
    solve_moving_baseline as core_solve_moving_baseline, AmbiguityScale, AmbiguitySet, Epoch,
    MovingBaselineEpoch, MovingBaselineEpochSolution, MovingBaselineOpts, MovingBaselineStatus,
};

use crate::error::{engine_error, type_error};
use crate::rtk::{EpochInput, FixedOptionsInput, FloatOptionsInput, MeasModelInput};

/// One moving-baseline epoch: the base receiver's own ECEF position this epoch
/// plus the standard RTK epoch observations.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MovingEpochInput {
    base_position_m: [f64; 3],
    #[serde(flatten)]
    epoch: EpochInput,
}

/// Complete typed input bundle for a moving-baseline solve. The ambiguity set is
/// shared across every epoch (the common moving-base case).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MovingBaselineConfigInput {
    epochs: Vec<MovingEpochInput>,
    ambiguity_ids: Vec<String>,
    ambiguity_satellites: BTreeMap<String, String>,
    wavelengths_m: BTreeMap<String, f64>,
    offsets_m: BTreeMap<String, f64>,
    #[serde(default)]
    float_only_systems: Vec<String>,
    model: MeasModelInput,
    #[serde(default)]
    float_options: FloatOptionsInput,
    #[serde(default)]
    fixed_options: FixedOptionsInput,
    #[serde(default)]
    initial_baseline_m: [f64; 3],
    #[serde(default = "default_true")]
    warm_start: bool,
}

fn default_true() -> bool {
    true
}

fn status_label(status: MovingBaselineStatus) -> String {
    match status {
        MovingBaselineStatus::Fixed => "Fixed".to_string(),
        MovingBaselineStatus::Float => "Float".to_string(),
    }
}

/// Solve a sequence of moving-baseline RTK epochs.
///
/// `config` is a plain object; see the `MovingBaselineConfig` TypeScript type.
/// Each epoch is solved independently against its own `basePositionM`; with
/// `warmStart` (default `true`) each solved baseline seeds the next epoch's
/// linearization point. Returns an array of `MovingBaselineEpochSolution`. Throws
/// a `TypeError` for malformed input and an `Error` if a solve fails (the message
/// names the failing epoch index). Delegates to
/// `sidereon_core::rtk_filter::moving_baseline::solve_moving_baseline`.
#[wasm_bindgen(js_name = solveMovingBaseline)]
pub fn solve_moving_baseline(config: JsValue) -> Result<Vec<MovingBaselineSolution>, JsValue> {
    let cfg: MovingBaselineConfigInput = serde_wasm_bindgen::from_value(config)
        .map_err(|e| type_error(&format!("invalid moving-baseline config: {e}")))?;

    let model = cfg.model.to_core()?;
    let opts = MovingBaselineOpts {
        model,
        float: cfg.float_options.to_core(),
        fixed: cfg.fixed_options.to_core(),
        initial_baseline_m: cfg.initial_baseline_m,
        warm_start: cfg.warm_start,
    };

    // The borrowed `MovingBaselineEpoch` views reference owned data that must
    // outlive the solve: the core epochs and the shared ambiguity set.
    let core_epochs: Vec<Epoch> = cfg.epochs.iter().map(|e| e.epoch.to_core()).collect();
    let ambiguities = AmbiguitySet {
        ids: &cfg.ambiguity_ids,
        satellites: &cfg.ambiguity_satellites,
        scale: AmbiguityScale {
            wavelengths_m: &cfg.wavelengths_m,
            offsets_m: &cfg.offsets_m,
        },
        float_only_systems: &cfg.float_only_systems,
    };
    let moving_epochs: Vec<MovingBaselineEpoch<'_>> = cfg
        .epochs
        .iter()
        .zip(core_epochs.iter())
        .map(|(input, epoch)| MovingBaselineEpoch {
            base_position_m: input.base_position_m,
            epoch,
            ambiguities,
        })
        .collect();

    let solutions = core_solve_moving_baseline(&moving_epochs, opts, None).map_err(engine_error)?;

    Ok(solutions
        .into_iter()
        .map(|inner| MovingBaselineSolution { inner })
        .collect())
}

/// One solved moving-baseline epoch.
#[wasm_bindgen]
pub struct MovingBaselineSolution {
    inner: MovingBaselineEpochSolution,
}

#[wasm_bindgen]
impl MovingBaselineSolution {
    /// Base receiver ECEF position (metres) used for this epoch, `[x, y, z]`.
    #[wasm_bindgen(getter, js_name = basePositionM)]
    pub fn base_position_m(&self) -> Vec<f64> {
        self.inner.base_position_m.to_vec()
    }

    /// Baseline vector `rover - base` (metres) in ECEF, `[dx, dy, dz]`. The
    /// integer-fixed baseline when `status` is `"Fixed"`, else the float baseline.
    #[wasm_bindgen(getter, js_name = baselineM)]
    pub fn baseline_m(&self) -> Vec<f64> {
        self.inner.baseline_m.to_vec()
    }

    /// Euclidean baseline length, metres.
    #[wasm_bindgen(getter, js_name = baselineLengthM)]
    pub fn baseline_length_m(&self) -> f64 {
        self.inner.baseline_length_m
    }

    /// Integer ambiguity verdict for this epoch: `"Fixed"` or `"Float"`.
    #[wasm_bindgen(getter)]
    pub fn status(&self) -> String {
        status_label(self.inner.status)
    }

    /// The float baseline this epoch reduced through, `[dx, dy, dz]`, metres.
    #[wasm_bindgen(getter, js_name = floatBaselineM)]
    pub fn float_baseline_m(&self) -> Vec<f64> {
        self.inner.float.baseline_m.to_vec()
    }

    /// Whether the float baseline solve converged.
    #[wasm_bindgen(getter, js_name = floatConverged)]
    pub fn float_converged(&self) -> bool {
        self.inner.float.converged
    }
}
