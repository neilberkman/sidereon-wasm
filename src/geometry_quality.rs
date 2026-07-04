//! Shared geometry observability diagnostics.
//!
//! The core solver returns these diagnostics on solution structs so callers can
//! tell whether the final design had residual degrees of freedom, whether RAIM
//! can check residuals, and whether covariance bounds were validated. This module
//! only maps that public core value across the WASM boundary.

use serde::Serialize;
use wasm_bindgen::prelude::*;

use sidereon_core::geometry_quality::{
    GeometryQuality as CoreGeometryQuality, ObservabilityTier as CoreObservabilityTier,
};

/// Observability and validation tier for an estimation design.
///
/// `ZeroRedundancy` means the design is full rank but has no residual degrees of
/// freedom, so snapshot-solve covariance bounds are unvalidated unless a
/// propagated prior is present. `Weak` means the design exceeded the configured
/// condition-number or GDOP cutoff; the returned bounds are reported as computed
/// and are not clamped.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ObservabilityTier {
    /// At least one estimated parameter is not observable.
    RankDeficient,
    /// Full rank with no residual degrees of freedom.
    ZeroRedundancy,
    /// Full rank with residual degrees of freedom, but above a cutoff.
    Weak,
    /// Full rank and within the configured cutoffs.
    Nominal,
}

impl From<CoreObservabilityTier> for ObservabilityTier {
    fn from(value: CoreObservabilityTier) -> Self {
        match value {
            CoreObservabilityTier::RankDeficient => Self::RankDeficient,
            CoreObservabilityTier::ZeroRedundancy => Self::ZeroRedundancy,
            CoreObservabilityTier::Weak => Self::Weak,
            CoreObservabilityTier::Nominal => Self::Nominal,
        }
    }
}

fn tier_label_core(value: CoreObservabilityTier) -> &'static str {
    match value {
        CoreObservabilityTier::RankDeficient => "RankDeficient",
        CoreObservabilityTier::ZeroRedundancy => "ZeroRedundancy",
        CoreObservabilityTier::Weak => "Weak",
        CoreObservabilityTier::Nominal => "Nominal",
    }
}

fn tier_label(value: ObservabilityTier) -> &'static str {
    match value {
        ObservabilityTier::RankDeficient => "RankDeficient",
        ObservabilityTier::ZeroRedundancy => "ZeroRedundancy",
        ObservabilityTier::Weak => "Weak",
        ObservabilityTier::Nominal => "Nominal",
    }
}

/// Stable string label for an [`ObservabilityTier`] enum value.
#[wasm_bindgen(js_name = observabilityTierLabel)]
pub fn observability_tier_label(tier: ObservabilityTier) -> String {
    tier_label(tier).to_string()
}

/// Geometry observability and covariance-validation diagnostics.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct GeometryQuality {
    inner: CoreGeometryQuality,
}

impl From<CoreGeometryQuality> for GeometryQuality {
    fn from(inner: CoreGeometryQuality) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl GeometryQuality {
    /// Observability and validation tier.
    #[wasm_bindgen(getter)]
    pub fn tier(&self) -> ObservabilityTier {
        self.inner.tier.into()
    }

    /// Observation redundancy, `nObs - nParams`.
    #[wasm_bindgen(getter)]
    pub fn redundancy(&self) -> i32 {
        self.inner.redundancy
    }

    /// Rank of the design matrix used by the solve.
    #[wasm_bindgen(getter)]
    pub fn rank(&self) -> usize {
        self.inner.rank
    }

    /// Singular-value condition number of the design matrix.
    #[wasm_bindgen(getter, js_name = conditionNumber)]
    pub fn condition_number(&self) -> f64 {
        self.inner.condition_number
    }

    /// Geometric dilution of precision for the solved state.
    #[wasm_bindgen(getter)]
    pub fn gdop(&self) -> f64 {
        self.inner.gdop
    }

    /// Whether residual-based RAIM can test the solve.
    #[wasm_bindgen(getter, js_name = raimCheckable)]
    pub fn raim_checkable(&self) -> bool {
        self.inner.raim_checkable
    }

    /// Whether residuals or a propagated prior validated the covariance bound.
    #[wasm_bindgen(getter, js_name = covarianceValidated)]
    pub fn covariance_validated(&self) -> bool {
        self.inner.covariance_validated
    }
}

/// Plain-object form used by serde-returning APIs in this binding.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeometryQualityJs {
    tier: &'static str,
    redundancy: i32,
    rank: usize,
    condition_number: f64,
    gdop: f64,
    raim_checkable: bool,
    covariance_validated: bool,
}

impl From<CoreGeometryQuality> for GeometryQualityJs {
    fn from(value: CoreGeometryQuality) -> Self {
        Self {
            tier: tier_label_core(value.tier),
            redundancy: value.redundancy,
            rank: value.rank,
            condition_number: value.condition_number,
            gdop: value.gdop,
            raim_checkable: value.raim_checkable,
            covariance_validated: value.covariance_validated,
        }
    }
}
