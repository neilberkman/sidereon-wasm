//! Sequential RTK baseline arc driver.
//!
//! Thin wrapper over `sidereon_core::rtk_filter::arc::solve_rtk_arc`. The epoch
//! normalization, reference selection, double-difference construction, and the
//! per-epoch Kalman predict/update/search/hold all live in the crate; this
//! module only marshals the raw rover+base arc epochs and the driver config from
//! idiomatic JS objects into the `sidereon-core` input types and packages the
//! per-epoch reported solutions, the per-system references, and the final carried
//! filter state back into one JS object. No filtering logic lives here. The
//! single-epoch batch float/fixed solves are wrapped separately in [`crate::rtk`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::carrier_phase::CycleSlipOptions;
use sidereon_core::rtk::{
    BaselineReferenceSelection, CycleSlipPolicy, CycleSlipReceiver, CycleSlipSplitArc,
};
use sidereon_core::rtk_filter::{
    defaults, fix_wide_lane_rtk_arc, prepare_ionosphere_free_rtk_arc, solve_rtk_arc,
    solve_static_rtk_arc, DynamicsModel, FilterState, FixedBaselineSolution, FloatBaselineSolution,
    FloatResidual, FloatSolveStatus, InnovationScreen, InnovationScreenOpts, IntegerSearchMeta,
    IntegerStatus, ResidualComponentKind, ResidualValidationMeta, ResidualValidationOutlier,
    RtkArcConfig, RtkArcEpoch, RtkArcEpochSolution, RtkArcObservation, RtkArcPreprocessing,
    RtkArcSolution, RtkDualCycleSlipConfig, RtkDualFrequencyArcEpoch, RtkDualFrequencyObservation,
    RtkDualFrequencySatelliteObservation, RtkIonosphereFreeArcConfig, RtkIonosphereFreeArcSolution,
    RtkStaticArcConfig, RtkStaticArcSolution, RtkWideLaneArcConfig, RtkWideLaneArcSolution,
    SearchOpts, UpdateOpts, ValidatedFixedBaselineSolution, ValidatedFixedSolveOpts,
    WideLaneOptions,
};

use crate::error::{engine_error, type_error};
use crate::rtk::{FixedOptionsInput, FloatOptionsInput, MeasModelInput, ResidualOptionsInput};

// --- input objects ----------------------------------------------------------

/// One raw single-frequency code/carrier observation at a receiver.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArcObservationInput {
    satellite_id: String,
    ambiguity_id: String,
    code_m: f64,
    phase_m: f64,
    #[serde(default)]
    lli: Option<i64>,
}

impl ArcObservationInput {
    fn to_core(&self) -> RtkArcObservation {
        RtkArcObservation {
            satellite_id: self.satellite_id.clone(),
            ambiguity_id: self.ambiguity_id.clone(),
            code_m: self.code_m,
            phase_m: self.phase_m,
            lli: self.lli,
        }
    }
}

/// Cycle-slip preprocessing policy. The core owns all slip detection and
/// split/drop behavior; this enum only maps JS string labels onto core variants.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum CycleSlipPolicyInput {
    Error,
    DropSatellite,
    SplitArc,
}

impl CycleSlipPolicyInput {
    fn to_core(&self) -> CycleSlipPolicy {
        match self {
            Self::Error => CycleSlipPolicy::Error,
            Self::DropSatellite => CycleSlipPolicy::DropSatellite,
            Self::SplitArc => CycleSlipPolicy::SplitArc,
        }
    }
}

/// Optional preprocessing chained ahead of the core arc solve.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ArcPreprocessingInput {
    cycle_slip: Option<CycleSlipPolicyInput>,
    hatch_window_cap: Option<usize>,
    elevation_mask_deg: Option<f64>,
}

impl ArcPreprocessingInput {
    fn to_core(&self) -> RtkArcPreprocessing {
        RtkArcPreprocessing {
            cycle_slip: self.cycle_slip.as_ref().map(CycleSlipPolicyInput::to_core),
            hatch_window_cap: self.hatch_window_cap,
            elevation_mask_deg: self.elevation_mask_deg,
        }
    }
}

/// One raw RTK arc epoch: paired base/rover observations and the satellite
/// positions needed to form double differences.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArcEpochInput {
    base: Vec<ArcObservationInput>,
    rover: Vec<ArcObservationInput>,
    satellite_positions_m: BTreeMap<String, [f64; 3]>,
    #[serde(default)]
    base_satellite_positions_m: BTreeMap<String, [f64; 3]>,
    #[serde(default)]
    rover_satellite_positions_m: BTreeMap<String, [f64; 3]>,
    #[serde(default)]
    velocity_mps: Option<[f64; 3]>,
    #[serde(default)]
    prediction_time_s: Option<f64>,
}

impl ArcEpochInput {
    fn to_core(&self) -> RtkArcEpoch {
        RtkArcEpoch {
            base: self.base.iter().map(ArcObservationInput::to_core).collect(),
            rover: self
                .rover
                .iter()
                .map(ArcObservationInput::to_core)
                .collect(),
            satellite_positions_m: self.satellite_positions_m.clone(),
            base_satellite_positions_m: self.base_satellite_positions_m.clone(),
            rover_satellite_positions_m: self.rover_satellite_positions_m.clone(),
            velocity_mps: self.velocity_mps,
            prediction_time_s: self.prediction_time_s,
        }
    }
}

/// Reference-satellite selection policy. `mode` is `"auto"` (default, highest
/// average elevation per constellation), `"satellite"` with `satellite`, or
/// `"perSystem"` with a `references` map (constellation letter -> satellite).
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ReferenceSelectionInput {
    mode: Option<String>,
    satellite: Option<String>,
    references: BTreeMap<String, String>,
}

impl ReferenceSelectionInput {
    fn to_core(&self) -> Result<BaselineReferenceSelection, JsValue> {
        match self.mode.as_deref() {
            None | Some("auto") => Ok(BaselineReferenceSelection::Auto),
            Some("satellite") => {
                let sat = self.satellite.clone().ok_or_else(|| {
                    type_error("reference mode \"satellite\" requires a satellite token")
                })?;
                Ok(BaselineReferenceSelection::Satellite(sat))
            }
            Some("perSystem") => Ok(BaselineReferenceSelection::PerSystem(
                self.references.clone(),
            )),
            Some(other) => Err(type_error(&format!(
                "invalid reference mode {other:?}: expected \"auto\", \"satellite\", or \"perSystem\""
            ))),
        }
    }
}

/// Optional predicted-residual screen: `{ thresholdSigma, minRows }`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InnovationScreenInput {
    threshold_sigma: f64,
    min_rows: usize,
}

impl InnovationScreenInput {
    fn to_core(&self) -> InnovationScreenOpts {
        InnovationScreenOpts {
            threshold_sigma: self.threshold_sigma,
            min_rows: self.min_rows,
        }
    }
}

/// Per-epoch sequential-update controls.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct UpdateOptsInput {
    hold_sigma_m: f64,
    position_tol_m: f64,
    ambiguity_tol_m: f64,
    max_iterations: usize,
    process_noise_baseline_sigma_m: f64,
    dynamics_model: String,
    float_only_systems: Vec<String>,
    innovation_screen: Option<InnovationScreenInput>,
    report_residuals: bool,
    ar_arming_sigma_m: Option<f64>,
    ratio_threshold: f64,
}

impl Default for UpdateOptsInput {
    fn default() -> Self {
        Self {
            hold_sigma_m: 1.0e-4,
            position_tol_m: defaults::POSITION_TOL_M,
            ambiguity_tol_m: defaults::AMBIGUITY_TOL_M,
            max_iterations: defaults::MAX_ITERATIONS,
            process_noise_baseline_sigma_m: 0.0,
            dynamics_model: "constantPosition".to_string(),
            float_only_systems: Vec::new(),
            innovation_screen: None,
            report_residuals: false,
            ar_arming_sigma_m: None,
            ratio_threshold: defaults::RATIO_THRESHOLD,
        }
    }
}

impl UpdateOptsInput {
    fn to_core(&self) -> Result<UpdateOpts, JsValue> {
        let dynamics_model = match self.dynamics_model.as_str() {
            "constantPosition" => DynamicsModel::ConstantPosition,
            "velocityPropagated" => DynamicsModel::VelocityPropagated,
            other => {
                return Err(type_error(&format!(
                    "invalid dynamics model {other:?}: expected \"constantPosition\" or \"velocityPropagated\""
                )))
            }
        };
        Ok(UpdateOpts {
            hold_sigma_m: self.hold_sigma_m,
            position_tol_m: self.position_tol_m,
            ambiguity_tol_m: self.ambiguity_tol_m,
            max_iterations: self.max_iterations,
            process_noise_baseline_sigma_m: self.process_noise_baseline_sigma_m,
            dynamics_model,
            float_only_systems: self.float_only_systems.clone(),
            innovation_screen: self
                .innovation_screen
                .as_ref()
                .map(InnovationScreenInput::to_core),
            report_residuals: self.report_residuals,
            receiver_antenna_corrections: None,
            ar_arming_sigma_m: self.ar_arming_sigma_m,
            search: SearchOpts {
                ratio_threshold: self.ratio_threshold,
            },
        })
    }
}

/// Complete typed configuration for a sequential RTK arc solve.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArcConfigInput {
    base_m: [f64; 3],
    #[serde(default)]
    reference: ReferenceSelectionInput,
    model: MeasModelInput,
    baseline_prior_sigma_m: f64,
    ambiguity_prior_sigma_m: f64,
    #[serde(default)]
    initial_baseline_m: [f64; 3],
    #[serde(default)]
    wavelengths_m: BTreeMap<String, f64>,
    #[serde(default)]
    offsets_m: BTreeMap<String, f64>,
    #[serde(default)]
    update_opts: UpdateOptsInput,
    #[serde(default)]
    preprocessing: ArcPreprocessingInput,
}

impl ArcConfigInput {
    fn to_core(&self) -> Result<RtkArcConfig, JsValue> {
        Ok(RtkArcConfig {
            base_m: self.base_m,
            reference: self.reference.to_core()?,
            model: self.model.to_core()?,
            baseline_prior_sigma_m: self.baseline_prior_sigma_m,
            ambiguity_prior_sigma_m: self.ambiguity_prior_sigma_m,
            initial_baseline_m: self.initial_baseline_m,
            wavelengths_m: self.wavelengths_m.clone(),
            offsets_m: self.offsets_m.clone(),
            update_opts: self.update_opts.to_core()?,
            preprocessing: self.preprocessing.to_core(),
        })
    }
}

/// The three option groups used by the validated fixed solve inside the static
/// arc driver.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ValidatedFixedOptionsInput {
    float: FloatOptionsInput,
    fixed: FixedOptionsInput,
    residual: ResidualOptionsInput,
}

impl ValidatedFixedOptionsInput {
    fn to_core(&self) -> ValidatedFixedSolveOpts {
        ValidatedFixedSolveOpts {
            float: self.float.to_core(),
            fixed: self.fixed.to_core(),
            residual: self.residual.to_core(),
        }
    }
}

/// Complete typed configuration for a static RTK arc solve.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StaticArcConfigInput {
    arc: ArcConfigInput,
    #[serde(default)]
    opts: ValidatedFixedOptionsInput,
}

impl StaticArcConfigInput {
    fn to_core(&self) -> Result<RtkStaticArcConfig, JsValue> {
        Ok(RtkStaticArcConfig {
            arc: self.arc.to_core()?,
            opts: self.opts.to_core(),
        })
    }
}

/// One receiver's dual-frequency code/carrier observation.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DualFrequencyObservationInput {
    ambiguity_id: String,
    p1_m: f64,
    p2_m: f64,
    phi1_cycles: f64,
    phi2_cycles: f64,
    f1_hz: f64,
    f2_hz: f64,
    #[serde(default)]
    lli1: Option<i64>,
    #[serde(default)]
    lli2: Option<i64>,
}

impl DualFrequencyObservationInput {
    fn to_core(&self) -> RtkDualFrequencyObservation {
        RtkDualFrequencyObservation {
            ambiguity_id: self.ambiguity_id.clone(),
            p1_m: self.p1_m,
            p2_m: self.p2_m,
            phi1_cycles: self.phi1_cycles,
            phi2_cycles: self.phi2_cycles,
            f1_hz: self.f1_hz,
            f2_hz: self.f2_hz,
            lli1: self.lli1,
            lli2: self.lli2,
        }
    }
}

/// Paired base/rover dual-frequency observation for one satellite.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DualFrequencySatelliteObservationInput {
    satellite_id: String,
    base: DualFrequencyObservationInput,
    rover: DualFrequencyObservationInput,
}

impl DualFrequencySatelliteObservationInput {
    fn to_core(&self) -> RtkDualFrequencySatelliteObservation {
        RtkDualFrequencySatelliteObservation {
            satellite_id: self.satellite_id.clone(),
            base: self.base.to_core(),
            rover: self.rover.to_core(),
        }
    }
}

/// One dual-frequency RTK arc epoch.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DualFrequencyArcEpochInput {
    jd_whole: f64,
    jd_fraction: f64,
    #[serde(default)]
    epoch_sort_key: Option<String>,
    #[serde(default)]
    gap_time_s: Option<f64>,
    observations: Vec<DualFrequencySatelliteObservationInput>,
    satellite_positions_m: BTreeMap<String, [f64; 3]>,
    #[serde(default)]
    base_satellite_positions_m: BTreeMap<String, [f64; 3]>,
    #[serde(default)]
    rover_satellite_positions_m: BTreeMap<String, [f64; 3]>,
    #[serde(default)]
    velocity_mps: Option<[f64; 3]>,
    #[serde(default)]
    prediction_time_s: Option<f64>,
}

impl DualFrequencyArcEpochInput {
    fn to_core(&self) -> RtkDualFrequencyArcEpoch {
        RtkDualFrequencyArcEpoch {
            jd_whole: self.jd_whole,
            jd_fraction: self.jd_fraction,
            epoch_sort_key: self.epoch_sort_key.clone(),
            gap_time_s: self.gap_time_s,
            observations: self
                .observations
                .iter()
                .map(DualFrequencySatelliteObservationInput::to_core)
                .collect(),
            satellite_positions_m: self.satellite_positions_m.clone(),
            base_satellite_positions_m: self.base_satellite_positions_m.clone(),
            rover_satellite_positions_m: self.rover_satellite_positions_m.clone(),
            velocity_mps: self.velocity_mps,
            prediction_time_s: self.prediction_time_s,
        }
    }
}

/// Dual-frequency cycle-slip classifier thresholds.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct DualCycleSlipOptionsInput {
    gf_threshold_m: Option<f64>,
    mw_threshold_cycles: Option<f64>,
    min_arc_gap_s: Option<f64>,
}

impl DualCycleSlipOptionsInput {
    fn to_core(&self) -> CycleSlipOptions {
        let defaults = CycleSlipOptions::default();
        CycleSlipOptions {
            gf_threshold_m: self.gf_threshold_m.unwrap_or(defaults.gf_threshold_m),
            mw_threshold_cycles: self
                .mw_threshold_cycles
                .unwrap_or(defaults.mw_threshold_cycles),
            min_arc_gap_s: self.min_arc_gap_s.unwrap_or(defaults.min_arc_gap_s),
        }
    }
}

/// Optional dual-frequency cycle-slip preprocessing for wide-lane fixing.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DualCycleSlipConfigInput {
    policy: CycleSlipPolicyInput,
    #[serde(default)]
    options: DualCycleSlipOptionsInput,
}

impl DualCycleSlipConfigInput {
    fn to_core(&self) -> RtkDualCycleSlipConfig {
        RtkDualCycleSlipConfig {
            policy: self.policy.to_core(),
            options: self.options.to_core(),
        }
    }
}

/// Wide-lane integer estimation controls.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WideLaneOptionsInput {
    min_epochs: usize,
    tolerance_cycles: f64,
    skip_short_fragments: bool,
}

impl WideLaneOptionsInput {
    fn to_core(&self) -> WideLaneOptions {
        WideLaneOptions {
            min_epochs: self.min_epochs,
            tolerance_cycles: self.tolerance_cycles,
            skip_short_fragments: self.skip_short_fragments,
        }
    }
}

/// Complete typed configuration for wide-lane RTK arc fixing.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WideLaneArcConfigInput {
    base_m: [f64; 3],
    #[serde(default)]
    reference: ReferenceSelectionInput,
    options: WideLaneOptionsInput,
    #[serde(default)]
    cycle_slip: Option<DualCycleSlipConfigInput>,
}

impl WideLaneArcConfigInput {
    fn to_core(&self) -> Result<RtkWideLaneArcConfig, JsValue> {
        Ok(RtkWideLaneArcConfig {
            base_m: self.base_m,
            reference: self.reference.to_core()?,
            options: self.options.to_core(),
            cycle_slip: self
                .cycle_slip
                .as_ref()
                .map(DualCycleSlipConfigInput::to_core),
        })
    }
}

/// Complete typed configuration for ionosphere-free RTK arc preparation.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IonosphereFreeArcConfigInput {
    base_m: [f64; 3],
    #[serde(default)]
    initial_baseline_m: [f64; 3],
    #[serde(default)]
    reference: ReferenceSelectionInput,
    #[serde(default)]
    apply_troposphere: bool,
}

impl IonosphereFreeArcConfigInput {
    fn to_core(&self) -> Result<RtkIonosphereFreeArcConfig, JsValue> {
        Ok(RtkIonosphereFreeArcConfig {
            base_m: self.base_m,
            initial_baseline_m: self.initial_baseline_m,
            reference: self.reference.to_core()?,
            apply_troposphere: self.apply_troposphere,
        })
    }
}

// --- result mirror objects --------------------------------------------------

fn integer_status_label(status: IntegerStatus) -> &'static str {
    match status {
        IntegerStatus::Fixed => "Fixed",
        IntegerStatus::NotFixed => "NotFixed",
    }
}

fn float_solve_status_label(status: FloatSolveStatus) -> &'static str {
    match status {
        FloatSolveStatus::StateTolerance => "StateTolerance",
        FloatSolveStatus::MaxIterations => "MaxIterations",
    }
}

fn residual_component_label(kind: ResidualComponentKind) -> &'static str {
    match kind {
        ResidualComponentKind::Code => "code",
        ResidualComponentKind::Phase => "phase",
    }
}

fn serialize_to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true))
        .map_err(|e| type_error(&e.to_string()))
}

/// Scalar summary of one epoch's integer search (the heavy LAMBDA covariance
/// matrices are not crossed).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchSummary {
    integer_status: &'static str,
    integer_method: &'static str,
    integer_ratio: Option<f64>,
    integer_best_score: Option<f64>,
    integer_second_best_score: Option<f64>,
    integer_candidates: usize,
    partial_enabled: bool,
    partial_fixed: bool,
}

impl From<&IntegerSearchMeta> for SearchSummary {
    fn from(m: &IntegerSearchMeta) -> Self {
        Self {
            integer_status: integer_status_label(m.integer_status),
            integer_method: m.integer_method,
            integer_ratio: m.integer_ratio,
            integer_best_score: m.integer_best_score,
            integer_second_best_score: m.integer_second_best_score,
            integer_candidates: m.integer_candidates,
            partial_enabled: m.partial.enabled,
            partial_fixed: m.partial.fixed,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResidualObject {
    epoch_index: usize,
    satellite_id: String,
    reference_satellite_id: String,
    ambiguity_id: String,
    code_m: f64,
    phase_m: f64,
    code_sigma_m: f64,
    phase_sigma_m: f64,
    code_normalized: f64,
    phase_normalized: f64,
}

/// Per-epoch predicted-residual (innovation) screen outcome, present only when
/// the screen was enabled for the arc via `updateOpts.innovationScreen`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InnovationScreenObject {
    threshold_sigma: f64,
    min_rows: usize,
    input_rows: usize,
    accepted_rows: usize,
    rejected_rows: usize,
    rejected_code_rows: usize,
    rejected_phase_rows: usize,
    max_abs_normalized_innovation: Option<f64>,
    max_rejected_abs_normalized_innovation: Option<f64>,
    coasted: bool,
}

impl From<&InnovationScreen> for InnovationScreenObject {
    fn from(s: &InnovationScreen) -> Self {
        Self {
            threshold_sigma: s.threshold_sigma,
            min_rows: s.min_rows,
            input_rows: s.input_rows,
            accepted_rows: s.accepted_rows,
            rejected_rows: s.rejected_rows,
            rejected_code_rows: s.rejected_code_rows,
            rejected_phase_rows: s.rejected_phase_rows,
            max_abs_normalized_innovation: s.max_abs_normalized_innovation,
            max_rejected_abs_normalized_innovation: s.max_rejected_abs_normalized_innovation,
            coasted: s.coasted,
        }
    }
}

fn cycle_slip_receiver_label(receiver: CycleSlipReceiver) -> &'static str {
    match receiver {
        CycleSlipReceiver::Base => "base",
        CycleSlipReceiver::Rover => "rover",
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CycleSlipSplitArcObject {
    receiver: &'static str,
    satellite_id: String,
    ambiguity_id: String,
    start_epoch_index: usize,
    end_epoch_index: usize,
    n_epochs: usize,
}

impl From<&CycleSlipSplitArc> for CycleSlipSplitArcObject {
    fn from(arc: &CycleSlipSplitArc) -> Self {
        Self {
            receiver: cycle_slip_receiver_label(arc.receiver),
            satellite_id: arc.satellite_id.clone(),
            ambiguity_id: arc.ambiguity_id.clone(),
            start_epoch_index: arc.start_epoch_index,
            end_epoch_index: arc.end_epoch_index,
            n_epochs: arc.n_epochs,
        }
    }
}

impl From<&FloatResidual> for ResidualObject {
    fn from(r: &FloatResidual) -> Self {
        Self {
            epoch_index: r.epoch_index,
            satellite_id: r.satellite_id.clone(),
            reference_satellite_id: r.reference_satellite_id.clone(),
            ambiguity_id: r.ambiguity_id.clone(),
            code_m: r.code_m,
            phase_m: r.phase_m,
            code_sigma_m: r.code_sigma_m,
            phase_sigma_m: r.phase_sigma_m,
            code_normalized: r.code_normalized,
            phase_normalized: r.phase_normalized,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FloatSolutionObject {
    baseline_m: [f64; 3],
    ambiguities_m: BTreeMap<String, f64>,
    ambiguity_covariance_m: Vec<f64>,
    ambiguity_covariance_inverse_m: Vec<f64>,
    residuals: Vec<ResidualObject>,
    iterations: usize,
    converged: bool,
    status: &'static str,
    code_rms_m: f64,
    phase_rms_m: f64,
    weighted_rms_m: f64,
    n_observations: usize,
}

impl From<&FloatBaselineSolution> for FloatSolutionObject {
    fn from(s: &FloatBaselineSolution) -> Self {
        Self {
            baseline_m: s.baseline_m,
            ambiguities_m: s.ambiguities_m.iter().cloned().collect(),
            ambiguity_covariance_m: s.ambiguity_covariance_m.clone(),
            ambiguity_covariance_inverse_m: s.ambiguity_covariance_inverse_m.clone(),
            residuals: s.residuals.iter().map(ResidualObject::from).collect(),
            iterations: s.iterations,
            converged: s.converged,
            status: float_solve_status_label(s.status),
            code_rms_m: s.code_rms_m,
            phase_rms_m: s.phase_rms_m,
            weighted_rms_m: s.weighted_rms_m,
            n_observations: s.n_observations,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FixedSolutionObject {
    baseline_m: [f64; 3],
    free_ambiguities_m: BTreeMap<String, f64>,
    fixed_ambiguities_cycles: BTreeMap<String, i64>,
    fixed_ambiguities_m: BTreeMap<String, f64>,
    residuals: Vec<ResidualObject>,
    search: SearchSummary,
    iterations: usize,
    converged: bool,
    status: &'static str,
    code_rms_m: f64,
    phase_rms_m: f64,
    weighted_rms_m: f64,
    n_observations: usize,
}

impl From<&FixedBaselineSolution> for FixedSolutionObject {
    fn from(s: &FixedBaselineSolution) -> Self {
        Self {
            baseline_m: s.baseline_m,
            free_ambiguities_m: s.free_ambiguities_m.iter().cloned().collect(),
            fixed_ambiguities_cycles: s.fixed_ambiguities_cycles.iter().cloned().collect(),
            fixed_ambiguities_m: s.fixed_ambiguities_m.iter().cloned().collect(),
            residuals: s.residuals.iter().map(ResidualObject::from).collect(),
            search: SearchSummary::from(&s.search),
            iterations: s.iterations,
            converged: s.converged,
            status: float_solve_status_label(s.status),
            code_rms_m: s.code_rms_m,
            phase_rms_m: s.phase_rms_m,
            weighted_rms_m: s.weighted_rms_m,
            n_observations: s.n_observations,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResidualValidationOutlierObject {
    epoch_index: usize,
    satellite_id: String,
    reference_satellite_id: String,
    ambiguity_id: String,
    kind: &'static str,
    residual_m: f64,
    sigma_m: f64,
    normalized_residual: f64,
    threshold_sigma: f64,
}

impl From<&ResidualValidationOutlier> for ResidualValidationOutlierObject {
    fn from(o: &ResidualValidationOutlier) -> Self {
        Self {
            epoch_index: o.epoch_index,
            satellite_id: o.satellite_id.clone(),
            reference_satellite_id: o.reference_satellite_id.clone(),
            ambiguity_id: o.ambiguity_id.clone(),
            kind: residual_component_label(o.kind),
            residual_m: o.residual_m,
            sigma_m: o.sigma_m,
            normalized_residual: o.normalized_residual,
            threshold_sigma: o.threshold_sigma,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResidualValidationObject {
    threshold_sigma: f64,
    max_exclusions: usize,
    excluded_sats: Vec<String>,
    exclusions: Vec<ResidualValidationOutlierObject>,
}

impl From<&ResidualValidationMeta> for ResidualValidationObject {
    fn from(m: &ResidualValidationMeta) -> Self {
        Self {
            threshold_sigma: m.threshold_sigma,
            max_exclusions: m.max_exclusions,
            excluded_sats: m.excluded_sats.clone(),
            exclusions: m
                .exclusions
                .iter()
                .map(ResidualValidationOutlierObject::from)
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ValidatedFixedSolutionObject {
    float_solution: FloatSolutionObject,
    fixed_solution: FixedSolutionObject,
    residual_validation: Option<ResidualValidationObject>,
    ambiguity_ids: Vec<String>,
    ambiguity_satellites: BTreeMap<String, String>,
}

impl From<&ValidatedFixedBaselineSolution> for ValidatedFixedSolutionObject {
    fn from(s: &ValidatedFixedBaselineSolution) -> Self {
        Self {
            float_solution: FloatSolutionObject::from(&s.float_solution),
            fixed_solution: FixedSolutionObject::from(&s.fixed_solution),
            residual_validation: s
                .residual_validation
                .as_ref()
                .map(ResidualValidationObject::from),
            ambiguity_ids: s.ambiguity_ids.clone(),
            ambiguity_satellites: s.ambiguity_satellites.clone(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StaticArcSolutionObject {
    references: BTreeMap<String, String>,
    ambiguity_ids: Vec<String>,
    ambiguity_satellites: BTreeMap<String, String>,
    float_solution: FloatSolutionObject,
    fixed_solution: ValidatedFixedSolutionObject,
    dropped_sats: Vec<String>,
    split_cycle_slip_arcs: Vec<CycleSlipSplitArcObject>,
    elevation_masked_sats: Vec<String>,
}

impl From<&RtkStaticArcSolution> for StaticArcSolutionObject {
    fn from(s: &RtkStaticArcSolution) -> Self {
        Self {
            references: s.references.clone(),
            ambiguity_ids: s.ambiguity_ids.clone(),
            ambiguity_satellites: s.ambiguity_satellites.clone(),
            float_solution: FloatSolutionObject::from(&s.float_solution),
            fixed_solution: ValidatedFixedSolutionObject::from(&s.fixed_solution),
            dropped_sats: s.dropped_sats.clone(),
            split_cycle_slip_arcs: s
                .split_cycle_slip_arcs
                .iter()
                .map(CycleSlipSplitArcObject::from)
                .collect(),
            elevation_masked_sats: s.elevation_masked_sats.clone(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EpochSolutionObject {
    reported_baseline_m: [f64; 3],
    float_baseline_m: [f64; 3],
    integer_fixed: bool,
    integer_ratio: f64,
    newly_fixed: Vec<String>,
    fixed_ids: Vec<String>,
    sd_ambiguities_m: BTreeMap<String, f64>,
    fixed_double_difference_ids: Vec<String>,
    used_satellite_ids: Vec<String>,
    search: Option<SearchSummary>,
    residuals: Vec<ResidualObject>,
    innovation_screen: Option<InnovationScreenObject>,
}

impl From<&RtkArcEpochSolution> for EpochSolutionObject {
    fn from(e: &RtkArcEpochSolution) -> Self {
        Self {
            reported_baseline_m: e.reported_baseline_m,
            float_baseline_m: e.float_baseline_m,
            integer_fixed: e.integer_fixed,
            integer_ratio: e.integer_ratio,
            newly_fixed: e.newly_fixed.clone(),
            fixed_ids: e.fixed_ids.clone(),
            sd_ambiguities_m: e.sd_ambiguities_m.iter().cloned().collect(),
            fixed_double_difference_ids: e.fixed_double_difference_ids.clone(),
            used_satellite_ids: e.used_satellite_ids.clone(),
            search: e.search.as_ref().map(SearchSummary::from),
            residuals: e.residuals.iter().map(ResidualObject::from).collect(),
            innovation_screen: e
                .innovation_screen
                .as_ref()
                .map(InnovationScreenObject::from),
        }
    }
}

/// The final carried filter state (the serializable streaming ABI).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FilterStateObject {
    version: u16,
    references: BTreeMap<String, String>,
    sd_ambiguity_ids: Vec<String>,
    baseline_m: [f64; 3],
    sd_ambiguities_m: Vec<f64>,
    information: Vec<f64>,
    ambiguity_prior_sigma_m: f64,
    epoch_count: usize,
    fixed_cycles: BTreeMap<String, i64>,
    fixed_m: BTreeMap<String, f64>,
}

impl From<&FilterState> for FilterStateObject {
    fn from(s: &FilterState) -> Self {
        Self {
            version: s.version,
            references: s.references.clone(),
            sd_ambiguity_ids: s.sd_ambiguity_ids.clone(),
            baseline_m: s.baseline_m,
            sd_ambiguities_m: s.sd_ambiguities_m.clone(),
            information: s.information.clone(),
            ambiguity_prior_sigma_m: s.ambiguity_prior_sigma_m,
            epoch_count: s.epoch_count,
            fixed_cycles: s.fixed_cycles.clone(),
            fixed_m: s.fixed_m.clone(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ArcObservationObject {
    satellite_id: String,
    ambiguity_id: String,
    code_m: f64,
    phase_m: f64,
    lli: Option<i64>,
}

impl From<&RtkArcObservation> for ArcObservationObject {
    fn from(o: &RtkArcObservation) -> Self {
        Self {
            satellite_id: o.satellite_id.clone(),
            ambiguity_id: o.ambiguity_id.clone(),
            code_m: o.code_m,
            phase_m: o.phase_m,
            lli: o.lli,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ArcEpochObject {
    base: Vec<ArcObservationObject>,
    rover: Vec<ArcObservationObject>,
    satellite_positions_m: BTreeMap<String, [f64; 3]>,
    base_satellite_positions_m: BTreeMap<String, [f64; 3]>,
    rover_satellite_positions_m: BTreeMap<String, [f64; 3]>,
    velocity_mps: Option<[f64; 3]>,
    prediction_time_s: Option<f64>,
}

impl From<&RtkArcEpoch> for ArcEpochObject {
    fn from(e: &RtkArcEpoch) -> Self {
        Self {
            base: e.base.iter().map(ArcObservationObject::from).collect(),
            rover: e.rover.iter().map(ArcObservationObject::from).collect(),
            satellite_positions_m: e.satellite_positions_m.clone(),
            base_satellite_positions_m: e.base_satellite_positions_m.clone(),
            rover_satellite_positions_m: e.rover_satellite_positions_m.clone(),
            velocity_mps: e.velocity_mps,
            prediction_time_s: e.prediction_time_s,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DualFrequencyObservationObject {
    ambiguity_id: String,
    p1_m: f64,
    p2_m: f64,
    phi1_cycles: f64,
    phi2_cycles: f64,
    f1_hz: f64,
    f2_hz: f64,
    lli1: Option<i64>,
    lli2: Option<i64>,
}

impl From<&RtkDualFrequencyObservation> for DualFrequencyObservationObject {
    fn from(o: &RtkDualFrequencyObservation) -> Self {
        Self {
            ambiguity_id: o.ambiguity_id.clone(),
            p1_m: o.p1_m,
            p2_m: o.p2_m,
            phi1_cycles: o.phi1_cycles,
            phi2_cycles: o.phi2_cycles,
            f1_hz: o.f1_hz,
            f2_hz: o.f2_hz,
            lli1: o.lli1,
            lli2: o.lli2,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DualFrequencySatelliteObservationObject {
    satellite_id: String,
    base: DualFrequencyObservationObject,
    rover: DualFrequencyObservationObject,
}

impl From<&RtkDualFrequencySatelliteObservation> for DualFrequencySatelliteObservationObject {
    fn from(o: &RtkDualFrequencySatelliteObservation) -> Self {
        Self {
            satellite_id: o.satellite_id.clone(),
            base: DualFrequencyObservationObject::from(&o.base),
            rover: DualFrequencyObservationObject::from(&o.rover),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DualFrequencyArcEpochObject {
    jd_whole: f64,
    jd_fraction: f64,
    epoch_sort_key: Option<String>,
    gap_time_s: Option<f64>,
    observations: Vec<DualFrequencySatelliteObservationObject>,
    satellite_positions_m: BTreeMap<String, [f64; 3]>,
    base_satellite_positions_m: BTreeMap<String, [f64; 3]>,
    rover_satellite_positions_m: BTreeMap<String, [f64; 3]>,
    velocity_mps: Option<[f64; 3]>,
    prediction_time_s: Option<f64>,
}

impl From<&RtkDualFrequencyArcEpoch> for DualFrequencyArcEpochObject {
    fn from(e: &RtkDualFrequencyArcEpoch) -> Self {
        Self {
            jd_whole: e.jd_whole,
            jd_fraction: e.jd_fraction,
            epoch_sort_key: e.epoch_sort_key.clone(),
            gap_time_s: e.gap_time_s,
            observations: e
                .observations
                .iter()
                .map(DualFrequencySatelliteObservationObject::from)
                .collect(),
            satellite_positions_m: e.satellite_positions_m.clone(),
            base_satellite_positions_m: e.base_satellite_positions_m.clone(),
            rover_satellite_positions_m: e.rover_satellite_positions_m.clone(),
            velocity_mps: e.velocity_mps,
            prediction_time_s: e.prediction_time_s,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WideLaneArcSolutionObject {
    references: BTreeMap<String, String>,
    wide_lane_cycles: BTreeMap<String, i64>,
    epochs: Vec<DualFrequencyArcEpochObject>,
    dropped_sats: Vec<String>,
    split_cycle_slip_arcs: Vec<CycleSlipSplitArcObject>,
}

impl From<&RtkWideLaneArcSolution> for WideLaneArcSolutionObject {
    fn from(s: &RtkWideLaneArcSolution) -> Self {
        Self {
            references: s.references.clone(),
            wide_lane_cycles: s.wide_lane_cycles.clone(),
            epochs: s
                .epochs
                .iter()
                .map(DualFrequencyArcEpochObject::from)
                .collect(),
            dropped_sats: s.dropped_sats.clone(),
            split_cycle_slip_arcs: s
                .split_cycle_slip_arcs
                .iter()
                .map(CycleSlipSplitArcObject::from)
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IonosphereFreeArcSolutionObject {
    references: BTreeMap<String, String>,
    epochs: Vec<ArcEpochObject>,
    wavelengths_m: BTreeMap<String, f64>,
    offsets_m: BTreeMap<String, f64>,
}

impl From<&RtkIonosphereFreeArcSolution> for IonosphereFreeArcSolutionObject {
    fn from(s: &RtkIonosphereFreeArcSolution) -> Self {
        Self {
            references: s.references.clone(),
            epochs: s.epochs.iter().map(ArcEpochObject::from).collect(),
            wavelengths_m: s.wavelengths_m.clone(),
            offsets_m: s.offsets_m.clone(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ArcSolutionObject {
    references: BTreeMap<String, String>,
    epochs: Vec<EpochSolutionObject>,
    final_state: FilterStateObject,
    dropped_sats: Vec<String>,
    split_cycle_slip_arcs: Vec<CycleSlipSplitArcObject>,
    elevation_masked_sats: Vec<String>,
    measurement_covariance: Vec<f64>,
}

impl From<&RtkArcSolution> for ArcSolutionObject {
    fn from(s: &RtkArcSolution) -> Self {
        Self {
            references: s.references.clone(),
            epochs: s.epochs.iter().map(EpochSolutionObject::from).collect(),
            final_state: FilterStateObject::from(&s.final_state),
            dropped_sats: s.dropped_sats.clone(),
            split_cycle_slip_arcs: s
                .split_cycle_slip_arcs
                .iter()
                .map(CycleSlipSplitArcObject::from)
                .collect(),
            elevation_masked_sats: s.elevation_masked_sats.clone(),
            measurement_covariance: s.measurement_covariance.clone(),
        }
    }
}

/// Solve a sequential RTK baseline arc from raw rover+base epochs.
///
/// `epochs` is an array of `RtkArcEpoch` objects and `config` an `RtkArcConfig`
/// object (see the TypeScript types). Returns `{ references, epochs, finalState }`:
/// one reported baseline/ambiguity solution per input epoch, the per-system
/// reference satellites selected once for the whole arc, and the final carried
/// filter state. Delegates to `sidereon_core::rtk_filter::arc::solve_rtk_arc`.
/// Throws a `TypeError` for malformed input and an `Error` if the solve fails.
#[wasm_bindgen(js_name = solveRtkArc)]
pub fn solve_rtk_arc_js(epochs: JsValue, config: JsValue) -> Result<JsValue, JsValue> {
    let epochs: Vec<ArcEpochInput> = serde_wasm_bindgen::from_value(epochs)
        .map_err(|e| type_error(&format!("invalid RTK arc epochs: {e}")))?;
    let cfg: ArcConfigInput = serde_wasm_bindgen::from_value(config)
        .map_err(|e| type_error(&format!("invalid RTK arc config: {e}")))?;

    let core_epochs: Vec<RtkArcEpoch> = epochs.iter().map(ArcEpochInput::to_core).collect();
    let solution = solve_rtk_arc(&core_epochs, &cfg.to_core()?).map_err(engine_error)?;

    serialize_to_js(&ArcSolutionObject::from(&solution))
}

/// Solve a static RTK arc with one batch float solution and one validated fixed
/// solution over the whole arc.
///
/// `epochs` is an array of `RtkArcEpoch` objects and `config` a
/// `RtkStaticArcConfig` object. Delegates to
/// `sidereon_core::rtk_filter::arc::solve_static_rtk_arc`.
#[wasm_bindgen(js_name = solveStaticRtkArc)]
pub fn solve_static_rtk_arc_js(epochs: JsValue, config: JsValue) -> Result<JsValue, JsValue> {
    let epochs: Vec<ArcEpochInput> = serde_wasm_bindgen::from_value(epochs)
        .map_err(|e| type_error(&format!("invalid static RTK arc epochs: {e}")))?;
    let cfg: StaticArcConfigInput = serde_wasm_bindgen::from_value(config)
        .map_err(|e| type_error(&format!("invalid static RTK arc config: {e}")))?;

    let core_epochs: Vec<RtkArcEpoch> = epochs.iter().map(ArcEpochInput::to_core).collect();
    let solution = solve_static_rtk_arc(&core_epochs, &cfg.to_core()?).map_err(engine_error)?;

    serialize_to_js(&StaticArcSolutionObject::from(&solution))
}

/// Fix Melbourne-Wubbena wide-lane ambiguities over a dual-frequency RTK arc.
///
/// `epochs` is an array of `RtkDualFrequencyArcEpoch` objects and `config` a
/// `RtkWideLaneArcConfig` object. Delegates to
/// `sidereon_core::rtk_filter::arc::fix_wide_lane_rtk_arc`.
#[wasm_bindgen(js_name = fixWideLaneRtkArc)]
pub fn fix_wide_lane_rtk_arc_js(epochs: JsValue, config: JsValue) -> Result<JsValue, JsValue> {
    let epochs: Vec<DualFrequencyArcEpochInput> = serde_wasm_bindgen::from_value(epochs)
        .map_err(|e| type_error(&format!("invalid wide-lane RTK arc epochs: {e}")))?;
    let cfg: WideLaneArcConfigInput = serde_wasm_bindgen::from_value(config)
        .map_err(|e| type_error(&format!("invalid wide-lane RTK arc config: {e}")))?;

    let core_epochs: Vec<RtkDualFrequencyArcEpoch> = epochs
        .iter()
        .map(DualFrequencyArcEpochInput::to_core)
        .collect();
    let solution = fix_wide_lane_rtk_arc(&core_epochs, &cfg.to_core()?).map_err(engine_error)?;

    serialize_to_js(&WideLaneArcSolutionObject::from(&solution))
}

/// Prepare ionosphere-free single-frequency RTK arc inputs from a
/// dual-frequency arc and fixed wide-lane ambiguities.
///
/// `epochs` is an array of `RtkDualFrequencyArcEpoch` objects, `wideLaneCycles`
/// is an id-keyed integer object, and `config` is an
/// `RtkIonosphereFreeArcConfig` object. Delegates to
/// `sidereon_core::rtk_filter::arc::prepare_ionosphere_free_rtk_arc`.
#[wasm_bindgen(js_name = prepareIonosphereFreeRtkArc)]
pub fn prepare_ionosphere_free_rtk_arc_js(
    epochs: JsValue,
    wide_lane_cycles: JsValue,
    config: JsValue,
) -> Result<JsValue, JsValue> {
    let epochs: Vec<DualFrequencyArcEpochInput> = serde_wasm_bindgen::from_value(epochs)
        .map_err(|e| type_error(&format!("invalid ionosphere-free RTK arc epochs: {e}")))?;
    let wide_lane_cycles: BTreeMap<String, i64> = serde_wasm_bindgen::from_value(wide_lane_cycles)
        .map_err(|e| type_error(&format!("invalid RTK wide-lane cycles: {e}")))?;
    let cfg: IonosphereFreeArcConfigInput = serde_wasm_bindgen::from_value(config)
        .map_err(|e| type_error(&format!("invalid ionosphere-free RTK arc config: {e}")))?;

    let core_epochs: Vec<RtkDualFrequencyArcEpoch> = epochs
        .iter()
        .map(DualFrequencyArcEpochInput::to_core)
        .collect();
    let solution =
        prepare_ionosphere_free_rtk_arc(&core_epochs, &wide_lane_cycles, &cfg.to_core()?)
            .map_err(engine_error)?;

    serialize_to_js(&IonosphereFreeArcSolutionObject::from(&solution))
}

#[cfg(test)]
mod drift_tests {
    //! The arc update defaults track the canonical core RTK constants rather than
    //! literals duplicated in this binding.
    use super::*;

    #[test]
    fn update_opts_defaults_track_core() {
        let d = UpdateOptsInput::default();
        assert_eq!(d.position_tol_m, defaults::POSITION_TOL_M);
        assert_eq!(d.ambiguity_tol_m, defaults::AMBIGUITY_TOL_M);
        assert_eq!(d.max_iterations, defaults::MAX_ITERATIONS);
        assert_eq!(d.ratio_threshold, defaults::RATIO_THRESHOLD);
    }
}
