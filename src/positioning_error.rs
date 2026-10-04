//! Typed errors for single-point positioning and the static solve.
//!
//! Every failure of an SPP, static, broadcast, fallback, FDE, DGNSS, RTK float,
//! RTK fixed or RINEX SPP solve surfaces as an `Error` named `PositioningError` whose `message` is
//! the engine's text and whose `detail` is a [`PositioningErrorDetail`]: a
//! discriminated union on `kind`, one member per engine variant, with the
//! fields that variant carries. A solve that wraps another solve's failure
//! (an epoch of the static solve, a precise or broadcast leg of the fallback)
//! nests that failure as `cause`.
//!
//! Every engine enum mapped here is exhaustive except `RinexSppError`; a
//! variant a later engine adds to it crosses as `kind: "OTHER"` with the
//! variant's own name in `variant`.

use serde::Serialize;
use wasm_bindgen::JsValue;

use sidereon_core::astro::math::least_squares::SolveError;
use sidereon_core::dgnss::DgnssError;
use sidereon_core::positioning::{
    FallbackError, ReceiverSolution, RinexSppError, SolvePolicyError, SppError, StaticSolveError,
};
use sidereon_core::quality::{
    FdeError, FdeSppError, FdeUnresolvedReason, QualityError, SolutionValidationError,
};

use crate::error::error_with_detail;
use crate::label::{upper_snake_variant, Label};
use crate::spp::degrade_reason_label;

/// The JS error name every positioning failure carries.
pub(crate) const POSITIONING_ERROR: &str = "PositioningError";

/// Why a solution a solve produced was rejected by its validation gate.
#[derive(Serialize, Debug, Clone)]
#[serde(tag = "kind")]
pub(crate) enum SolutionValidationDetail {
    #[serde(rename = "INVALID_OPTIONS", rename_all = "camelCase")]
    InvalidOptions {
        field: &'static str,
        reason: &'static str,
    },
    #[serde(rename = "DEGENERATE_GEOMETRY_RANK_DEFICIENT")]
    DegenerateGeometryRankDeficient,
    #[serde(rename = "DEGENERATE_GEOMETRY_PDOP", rename_all = "camelCase")]
    DegenerateGeometryPdop { pdop: f64 },
    #[serde(rename = "IMPLAUSIBLE_POSITION", rename_all = "camelCase")]
    ImplausiblePosition { radius_m: f64 },
    #[serde(rename = "INVALID_RESIDUALS")]
    InvalidResiduals,
    #[serde(rename = "NO_CONVERGENCE", rename_all = "camelCase")]
    NoConvergence { residual_rms_m: f64 },
}

/// A unit QualityError variant carried by direct RAIM and range-FDE failures.
#[derive(Serialize, Debug, Clone)]
#[serde(tag = "kind")]
pub(crate) enum QualityErrorDetail {
    #[serde(rename = "INVALID_ELEVATION")]
    InvalidElevation,
    #[serde(rename = "MISSING_CN0")]
    MissingCn0,
    #[serde(rename = "INVALID_PARAMETER")]
    InvalidParameter,
    #[serde(rename = "INVALID_PROBABILITY")]
    InvalidProbability,
    #[serde(rename = "INVALID_SYSTEM_COUNT")]
    InvalidSystemCount,
    #[serde(rename = "INVALID_DOF")]
    InvalidDof,
    #[serde(rename = "INVALID_WEIGHT")]
    InvalidWeight,
    #[serde(rename = "INVALID_RELIABILITY_PARAMETER")]
    InvalidReliabilityParameter,
    #[serde(rename = "INVALID_RESIDUALS")]
    InvalidResiduals,
    #[serde(rename = "INVALID_DESIGN")]
    InvalidDesign,
    #[serde(rename = "SINGULAR_GEOMETRY")]
    SingularGeometry,
    #[serde(rename = "MISSING_VARIANCES")]
    MissingVariances,
    #[serde(rename = "INVALID_VARIANCE")]
    InvalidVariance,
}

impl From<QualityError> for QualityErrorDetail {
    fn from(error: QualityError) -> Self {
        match error {
            QualityError::InvalidElevation => Self::InvalidElevation,
            QualityError::MissingCn0 => Self::MissingCn0,
            QualityError::InvalidParameter => Self::InvalidParameter,
            QualityError::InvalidProbability => Self::InvalidProbability,
            QualityError::InvalidSystemCount => Self::InvalidSystemCount,
            QualityError::InvalidDof => Self::InvalidDof,
            QualityError::InvalidWeight => Self::InvalidWeight,
            QualityError::InvalidReliabilityParameter => Self::InvalidReliabilityParameter,
            QualityError::InvalidResiduals => Self::InvalidResiduals,
            QualityError::InvalidDesign => Self::InvalidDesign,
            QualityError::SingularGeometry => Self::SingularGeometry,
            QualityError::MissingVariances => Self::MissingVariances,
            QualityError::InvalidVariance => Self::InvalidVariance,
        }
    }
}

/// Preserve the JavaScript exception family used by the earlier direct quality
/// APIs while attaching the exact core variant as `detail.kind`.
pub(crate) fn quality_error(error: QualityError) -> JsValue {
    let name = match error {
        QualityError::InvalidProbability
        | QualityError::InvalidSystemCount
        | QualityError::InvalidWeight => "RangeError",
        QualityError::InvalidResiduals => "TypeError",
        QualityError::InvalidElevation
        | QualityError::MissingCn0
        | QualityError::InvalidParameter
        | QualityError::InvalidDof
        | QualityError::InvalidReliabilityParameter
        | QualityError::InvalidDesign
        | QualityError::SingularGeometry
        | QualityError::MissingVariances
        | QualityError::InvalidVariance => "Error",
    };
    let message = error.to_string();
    crate::error::error_with_detail(name, &message, &QualityErrorDetail::from(error))
}

impl From<&SolutionValidationError> for SolutionValidationDetail {
    fn from(error: &SolutionValidationError) -> Self {
        match *error {
            SolutionValidationError::InvalidOptions { field, reason } => {
                Self::InvalidOptions { field, reason }
            }
            SolutionValidationError::DegenerateGeometryRankDeficient => {
                Self::DegenerateGeometryRankDeficient
            }
            SolutionValidationError::DegenerateGeometryPdop(pdop) => {
                Self::DegenerateGeometryPdop { pdop }
            }
            SolutionValidationError::ImplausiblePosition(radius_m) => {
                Self::ImplausiblePosition { radius_m }
            }
            SolutionValidationError::InvalidResiduals => Self::InvalidResiduals,
            SolutionValidationError::NoConvergence(residual_rms_m) => {
                Self::NoConvergence { residual_rms_m }
            }
        }
    }
}

/// A positioning failure, as the `detail` of a thrown `PositioningError`.
#[derive(Serialize, Debug, Clone)]
#[serde(tag = "kind")]
pub(crate) enum PositioningErrorDetail {
    #[serde(rename = "INVALID_INPUT", rename_all = "camelCase")]
    InvalidInput {
        message: String,
        field: &'static str,
        input_kind: Label,
    },
    #[serde(rename = "TOO_FEW_SATELLITES", rename_all = "camelCase")]
    TooFewSatellites {
        message: String,
        used: usize,
        required: usize,
    },
    #[serde(rename = "TOO_FEW_MEASUREMENTS", rename_all = "camelCase")]
    TooFewMeasurements {
        message: String,
        used: usize,
        required: usize,
    },
    #[serde(rename = "SINGULAR", rename_all = "camelCase")]
    Singular {
        message: String,
        /// `"SINGULAR_JACOBIAN"` or `"INVALID_INPUT"`.
        solve_error: &'static str,
        field: Option<&'static str>,
        reason: Option<&'static str>,
    },
    #[serde(rename = "DUPLICATE_OBSERVATION", rename_all = "camelCase")]
    DuplicateObservation {
        message: String,
        satellite_id: String,
        epoch_index: Option<usize>,
    },
    #[serde(rename = "EPHEMERIS_LOST", rename_all = "camelCase")]
    EphemerisLost {
        message: String,
        satellite_id: String,
        epoch_index: Option<usize>,
    },
    #[serde(rename = "SELECTION_UNSETTLED", rename_all = "camelCase")]
    SelectionUnsettled { message: String, passes: usize },
    #[serde(rename = "UT1_OUTSIDE_COVERAGE", rename_all = "camelCase")]
    Ut1OutsideCoverage {
        message: String,
        reason: &'static str,
    },
    #[serde(rename = "SOLUTION_REJECTED", rename_all = "camelCase")]
    SolutionRejected {
        message: String,
        validation: SolutionValidationDetail,
    },
    #[serde(rename = "NO_COARSE_SOLUTION", rename_all = "camelCase")]
    NoCoarseSolution { message: String },
    #[serde(rename = "EMPTY_EPOCHS", rename_all = "camelCase")]
    EmptyEpochs { message: String },
    #[serde(rename = "EPOCH_INPUT", rename_all = "camelCase")]
    EpochInput {
        message: String,
        epoch_index: usize,
        cause: Box<PositioningErrorDetail>,
    },
    #[serde(rename = "PRECISE_SOLVE_FAILED", rename_all = "camelCase")]
    PreciseSolveFailed {
        message: String,
        cause: Box<PositioningErrorDetail>,
    },
    #[serde(rename = "BROADCAST_SOLVE_FAILED", rename_all = "camelCase")]
    BroadcastSolveFailed {
        message: String,
        cause: Box<PositioningErrorDetail>,
    },
    #[serde(rename = "FAULT_UNRESOLVED", rename_all = "camelCase")]
    FaultUnresolved {
        message: String,
        reason: &'static str,
        test_statistic: f64,
        solution: Box<FdeUnresolvedSolutionDetail>,
        excluded: Vec<String>,
        raim: crate::raim::RaimResultObject,
    },
    #[serde(rename = "RAIM_CONFIGURATION", rename_all = "camelCase")]
    RaimConfiguration {
        message: String,
        quality: &'static str,
    },
    #[serde(rename = "OBSERVATION", rename_all = "camelCase")]
    Observation {
        message: String,
        /// The engine error variant, in UPPER_SNAKE_CASE.
        error: Label,
        cause: crate::core_error::CoreErrorDetail,
    },
    #[serde(rename = "MISSING_APPROX_POSITION", rename_all = "camelCase")]
    MissingApproxPosition { message: String },
    #[serde(rename = "DGNSS_INVALID_INPUT", rename_all = "camelCase")]
    DgnssInvalidInput {
        message: String,
        field: &'static str,
        reason: &'static str,
    },
    #[serde(rename = "RTK_FLOAT", rename_all = "camelCase")]
    RtkFloat {
        message: String,
        cause: FloatSolveErrorDetail,
    },
    #[serde(rename = "RTK_FIXED", rename_all = "camelCase")]
    RtkFixed {
        message: String,
        cause: ValidatedFixedErrorDetail,
    },
    /// A variant a later engine adds to a `#[non_exhaustive]` enum, or a
    /// facade error an SPP solve does not produce: the engine variant's name
    /// in UPPER_SNAKE_CASE, with the engine's message.
    #[serde(rename = "OTHER", rename_all = "camelCase")]
    Other { message: String, variant: Label },
}

#[derive(Serialize, Debug, Clone)]
#[serde(
    tag = "kind",
    rename_all = "SCREAMING_SNAKE_CASE",
    rename_all_fields = "camelCase"
)]
pub(crate) enum ValidatedFixedErrorDetail {
    Fixed {
        cause: FixedSolveErrorDetail,
    },
    ResidualValidationFailed {
        outlier: ResidualOutlierDetail,
        exclusions: Vec<ResidualOutlierDetail>,
    },
    DuplicateAmbiguityId {
        ambiguity_id: String,
        first_satellite_id: String,
        second_satellite_id: String,
    },
    Underdetermined {
        row_count: usize,
        unknown_count: usize,
    },
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResidualOutlierDetail {
    epoch_index: usize,
    satellite_id: String,
    reference_satellite_id: String,
    ambiguity_id: String,
    component: &'static str,
    residual_m: f64,
    sigma_m: f64,
    normalized_residual: f64,
    threshold_sigma: f64,
}

impl From<&sidereon_core::rtk_filter::ResidualValidationOutlier> for ResidualOutlierDetail {
    fn from(outlier: &sidereon_core::rtk_filter::ResidualValidationOutlier) -> Self {
        Self {
            epoch_index: outlier.epoch_index,
            satellite_id: outlier.satellite_id.clone(),
            reference_satellite_id: outlier.reference_satellite_id.clone(),
            ambiguity_id: outlier.ambiguity_id.clone(),
            component: match outlier.kind {
                sidereon_core::rtk_filter::ResidualComponentKind::Code => "code",
                sidereon_core::rtk_filter::ResidualComponentKind::Phase => "phase",
            },
            residual_m: outlier.residual_m,
            sigma_m: outlier.sigma_m,
            normalized_residual: outlier.normalized_residual,
            threshold_sigma: outlier.threshold_sigma,
        }
    }
}

#[derive(Serialize, Debug, Clone)]
#[serde(
    tag = "kind",
    rename_all = "SCREAMING_SNAKE_CASE",
    rename_all_fields = "camelCase"
)]
pub(crate) enum FixedSolveErrorDetail {
    Float {
        cause: FloatSolveErrorDetail,
    },
    Ils {
        cause: IlsErrorDetail,
    },
    MissingAmbiguity {
        ambiguity_id: String,
    },
    MissingWavelength {
        ambiguity_id: String,
    },
    MissingOffset {
        ambiguity_id: String,
    },
    InvalidCovarianceDimensions,
    InvalidInput {
        field: &'static str,
        input_kind: &'static str,
    },
    SingularGeometry,
    IncompleteResidualPair,
    ReceiverAntenna {
        cause: ReceiverAntennaErrorDetail,
    },
}

#[derive(Serialize, Debug, Clone)]
#[serde(
    tag = "kind",
    rename_all = "SCREAMING_SNAKE_CASE",
    rename_all_fields = "camelCase"
)]
pub(crate) enum FloatSolveErrorDetail {
    MissingSystemReference {
        system: String,
    },
    MissingAmbiguityColumn {
        ambiguity_id: String,
    },
    InvalidInput {
        field: &'static str,
        input_kind: &'static str,
    },
    SingularGeometry,
    IncompleteResidualPair,
    ReceiverAntenna {
        cause: ReceiverAntennaErrorDetail,
    },
}

#[derive(Serialize, Debug, Clone)]
#[serde(
    tag = "kind",
    rename_all = "SCREAMING_SNAKE_CASE",
    rename_all_fields = "camelCase"
)]
pub(crate) enum IlsErrorDetail {
    Singular,
    NoCandidates {
        evaluated: usize,
    },
    TooManyCandidates {
        evaluated: usize,
        limit: usize,
    },
    InvalidDimensions {
        n: usize,
        rows: usize,
    },
    NonFinite,
    InvalidInput {
        field: &'static str,
        reason: &'static str,
    },
    SearchLimitExceeded,
}

#[derive(Serialize, Debug, Clone)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum ReceiverAntennaErrorDetail {
    MissingPcv,
    InvalidGeometry,
}

/// The input-validation category of an SPP or static `InvalidInput`, in
/// UPPER_SNAKE_CASE (`NON_FINITE`, `OUT_OF_RANGE`, ...). The engine does not
/// export the category's type, so its name is taken from its `Debug` form.
fn input_kind_label<T: std::fmt::Debug>(kind: &T) -> Label {
    upper_snake_variant(kind)
}

pub(crate) fn quality_label(error: QualityError) -> &'static str {
    match error {
        QualityError::InvalidElevation => "INVALID_ELEVATION",
        QualityError::MissingCn0 => "MISSING_CN0",
        QualityError::InvalidParameter => "INVALID_PARAMETER",
        QualityError::InvalidProbability => "INVALID_PROBABILITY",
        QualityError::InvalidSystemCount => "INVALID_SYSTEM_COUNT",
        QualityError::InvalidDof => "INVALID_DOF",
        QualityError::InvalidWeight => "INVALID_WEIGHT",
        QualityError::InvalidReliabilityParameter => "INVALID_RELIABILITY_PARAMETER",
        QualityError::InvalidResiduals => "INVALID_RESIDUALS",
        QualityError::InvalidDesign => "INVALID_DESIGN",
        QualityError::SingularGeometry => "SINGULAR_GEOMETRY",
        QualityError::MissingVariances => "MISSING_VARIANCES",
        QualityError::InvalidVariance => "INVALID_VARIANCE",
    }
}

fn singular(message: String, error: &SolveError) -> PositioningErrorDetail {
    match error {
        SolveError::SingularJacobian => PositioningErrorDetail::Singular {
            message,
            solve_error: "SINGULAR_JACOBIAN",
            field: None,
            reason: None,
        },
        SolveError::InvalidInput { field, reason } => PositioningErrorDetail::Singular {
            message,
            solve_error: "INVALID_INPUT",
            field: Some(field),
            reason: Some(reason),
        },
    }
}

/// The detail of an [`SppError`].
pub(crate) fn spp_detail(error: &SppError) -> PositioningErrorDetail {
    let message = error.to_string();
    match error {
        SppError::InvalidInput { field, kind } => PositioningErrorDetail::InvalidInput {
            message,
            field,
            input_kind: input_kind_label(kind),
        },
        SppError::TooFewSatellites { used, required } => PositioningErrorDetail::TooFewSatellites {
            message,
            used: *used,
            required: *required,
        },
        SppError::Singular(inner) => singular(message, inner),
        SppError::DuplicateObservation { satellite } => {
            PositioningErrorDetail::DuplicateObservation {
                message,
                satellite_id: satellite.to_string(),
                epoch_index: None,
            }
        }
        SppError::EphemerisLost { satellite } => PositioningErrorDetail::EphemerisLost {
            message,
            satellite_id: satellite.to_string(),
            epoch_index: None,
        },
        SppError::SelectionUnsettled { passes } => PositioningErrorDetail::SelectionUnsettled {
            message,
            passes: *passes,
        },
        SppError::Ut1OutsideCoverage(reason) => PositioningErrorDetail::Ut1OutsideCoverage {
            message,
            reason: degrade_reason_label(*reason),
        },
    }
}

fn rtk_input_kind(kind: sidereon_core::rtk_filter::RtkInputErrorKind) -> &'static str {
    use sidereon_core::rtk_filter::RtkInputErrorKind as K;
    match kind {
        K::NonFinite => "non_finite",
        K::NotPositive => "not_positive",
        K::Negative => "negative",
        K::OutOfRange => "out_of_range",
        K::Missing => "missing",
        K::FloatParse => "float_parse",
        K::IntParse => "int_parse",
        K::InvalidCivilDate => "invalid_civil_date",
        K::InvalidCivilTime => "invalid_civil_time",
    }
}

fn antenna_error_detail(
    error: sidereon_core::rtk_filter::ReceiverAntennaError,
) -> ReceiverAntennaErrorDetail {
    use sidereon_core::rtk_filter::ReceiverAntennaError as E;
    match error {
        E::MissingPcv => ReceiverAntennaErrorDetail::MissingPcv,
        E::InvalidGeometry => ReceiverAntennaErrorDetail::InvalidGeometry,
    }
}

fn ils_error_detail(error: &sidereon_core::ils::IlsError) -> IlsErrorDetail {
    use sidereon_core::ils::IlsError as E;
    match error {
        E::Singular => IlsErrorDetail::Singular,
        E::NoCandidates(evaluated) => IlsErrorDetail::NoCandidates {
            evaluated: *evaluated,
        },
        E::TooManyCandidates { evaluated, limit } => IlsErrorDetail::TooManyCandidates {
            evaluated: *evaluated,
            limit: *limit,
        },
        E::InvalidDimensions { n, rows } => {
            IlsErrorDetail::InvalidDimensions { n: *n, rows: *rows }
        }
        E::NonFinite => IlsErrorDetail::NonFinite,
        E::InvalidInput { field, reason } => IlsErrorDetail::InvalidInput { field, reason },
        E::SearchLimitExceeded => IlsErrorDetail::SearchLimitExceeded,
    }
}

fn float_solve_error_detail(
    error: &sidereon_core::rtk_filter::FloatSolveError,
) -> FloatSolveErrorDetail {
    use sidereon_core::rtk_filter::FloatSolveError as E;
    match error {
        E::MissingSystemReference(system) => FloatSolveErrorDetail::MissingSystemReference {
            system: system.clone(),
        },
        E::MissingAmbiguityColumn(id) => FloatSolveErrorDetail::MissingAmbiguityColumn {
            ambiguity_id: id.clone(),
        },
        E::InvalidInput { field, kind } => FloatSolveErrorDetail::InvalidInput {
            field,
            input_kind: rtk_input_kind(*kind),
        },
        E::SingularGeometry => FloatSolveErrorDetail::SingularGeometry,
        E::IncompleteResidualPair => FloatSolveErrorDetail::IncompleteResidualPair,
        E::ReceiverAntenna(source) => FloatSolveErrorDetail::ReceiverAntenna {
            cause: antenna_error_detail(*source),
        },
    }
}

fn fixed_solve_error_detail(
    error: &sidereon_core::rtk_filter::FixedSolveError,
) -> FixedSolveErrorDetail {
    use sidereon_core::rtk_filter::FixedSolveError as E;
    match error {
        E::Float(source) => FixedSolveErrorDetail::Float {
            cause: float_solve_error_detail(source),
        },
        E::Ils(source) => FixedSolveErrorDetail::Ils {
            cause: ils_error_detail(source),
        },
        E::MissingAmbiguity(id) => FixedSolveErrorDetail::MissingAmbiguity {
            ambiguity_id: id.clone(),
        },
        E::MissingWavelength(id) => FixedSolveErrorDetail::MissingWavelength {
            ambiguity_id: id.clone(),
        },
        E::MissingOffset(id) => FixedSolveErrorDetail::MissingOffset {
            ambiguity_id: id.clone(),
        },
        E::InvalidCovarianceDimensions => FixedSolveErrorDetail::InvalidCovarianceDimensions,
        E::InvalidInput { field, kind } => FixedSolveErrorDetail::InvalidInput {
            field,
            input_kind: rtk_input_kind(*kind),
        },
        E::SingularGeometry => FixedSolveErrorDetail::SingularGeometry,
        E::IncompleteResidualPair => FixedSolveErrorDetail::IncompleteResidualPair,
        E::ReceiverAntenna(source) => FixedSolveErrorDetail::ReceiverAntenna {
            cause: antenna_error_detail(*source),
        },
    }
}

fn validated_fixed_error_detail(
    error: &sidereon_core::rtk_filter::ValidatedFixedSolveError,
) -> ValidatedFixedErrorDetail {
    use sidereon_core::rtk_filter::ValidatedFixedSolveError as E;
    match error {
        E::Fixed(source) => ValidatedFixedErrorDetail::Fixed {
            cause: fixed_solve_error_detail(source),
        },
        E::ResidualValidationFailed {
            outlier,
            exclusions,
        } => ValidatedFixedErrorDetail::ResidualValidationFailed {
            outlier: ResidualOutlierDetail::from(outlier.as_ref()),
            exclusions: exclusions.iter().map(ResidualOutlierDetail::from).collect(),
        },
        E::DuplicateAmbiguityId {
            ambiguity_id,
            first_satellite_id,
            second_satellite_id,
        } => ValidatedFixedErrorDetail::DuplicateAmbiguityId {
            ambiguity_id: ambiguity_id.clone(),
            first_satellite_id: first_satellite_id.clone(),
            second_satellite_id: second_satellite_id.clone(),
        },
        E::Underdetermined {
            row_count,
            unknown_count,
        } => ValidatedFixedErrorDetail::Underdetermined {
            row_count: *row_count,
            unknown_count: *unknown_count,
        },
    }
}

pub(crate) fn rtk_float_error(error: &sidereon_core::rtk_filter::FloatSolveError) -> JsValue {
    let message = error.to_string();
    let detail = PositioningErrorDetail::RtkFloat {
        message: message.clone(),
        cause: float_solve_error_detail(error),
    };
    error_with_detail(POSITIONING_ERROR, &message, &detail)
}

pub(crate) fn rtk_fixed_error(
    error: &sidereon_core::rtk_filter::ValidatedFixedSolveError,
) -> JsValue {
    let message = error.to_string();
    let detail = PositioningErrorDetail::RtkFixed {
        message: message.clone(),
        cause: validated_fixed_error_detail(error),
    };
    error_with_detail(POSITIONING_ERROR, &message, &detail)
}

fn solution_rejected(message: String, error: &SolutionValidationError) -> PositioningErrorDetail {
    PositioningErrorDetail::SolutionRejected {
        message,
        validation: SolutionValidationDetail::from(error),
    }
}

/// The detail of a [`SolvePolicyError`] (an SPP solve under a policy).
pub(crate) fn solve_policy_detail(error: &SolvePolicyError) -> PositioningErrorDetail {
    match error {
        SolvePolicyError::Solve(inner) => spp_detail(inner).with_message(error.to_string()),
        SolvePolicyError::Validation(inner) => solution_rejected(error.to_string(), inner),
        SolvePolicyError::NoCoarseSolution => PositioningErrorDetail::NoCoarseSolution {
            message: error.to_string(),
        },
    }
}

/// The detail of a [`StaticSolveError`].
pub(crate) fn static_detail(error: &StaticSolveError) -> PositioningErrorDetail {
    let message = error.to_string();
    match error {
        StaticSolveError::EmptyEpochs => PositioningErrorDetail::EmptyEpochs { message },
        StaticSolveError::InvalidInput { field, kind } => PositioningErrorDetail::InvalidInput {
            message,
            field,
            input_kind: input_kind_label(kind),
        },
        StaticSolveError::EpochInput {
            epoch_index,
            source,
        } => PositioningErrorDetail::EpochInput {
            message,
            epoch_index: *epoch_index,
            cause: Box::new(spp_detail(source)),
        },
        StaticSolveError::DuplicateObservation {
            epoch_index,
            satellite,
        } => PositioningErrorDetail::DuplicateObservation {
            message,
            satellite_id: satellite.to_string(),
            epoch_index: Some(*epoch_index),
        },
        StaticSolveError::TooFewMeasurements { used, required } => {
            PositioningErrorDetail::TooFewMeasurements {
                message,
                used: *used,
                required: *required,
            }
        }
        StaticSolveError::EphemerisLost {
            epoch_index,
            satellite,
        } => PositioningErrorDetail::EphemerisLost {
            message,
            satellite_id: satellite.to_string(),
            epoch_index: Some(*epoch_index),
        },
        StaticSolveError::Singular(inner) => singular(message, inner),
        StaticSolveError::SelectionUnsettled { passes } => {
            PositioningErrorDetail::SelectionUnsettled {
                message,
                passes: *passes,
            }
        }
        StaticSolveError::Ut1OutsideCoverage(reason) => {
            PositioningErrorDetail::Ut1OutsideCoverage {
                message,
                reason: degrade_reason_label(*reason),
            }
        }
    }
}

/// The detail of a [`FallbackError`].
pub(crate) fn fallback_detail(error: &FallbackError) -> PositioningErrorDetail {
    let message = error.to_string();
    match error {
        FallbackError::Precise(inner) => PositioningErrorDetail::PreciseSolveFailed {
            message,
            cause: Box::new(spp_detail(inner)),
        },
        FallbackError::Broadcast(inner) => PositioningErrorDetail::BroadcastSolveFailed {
            message,
            cause: Box::new(spp_detail(inner)),
        },
    }
}

/// The detail of an FDE failure over SPP solves.
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FdeUnresolvedSolutionDetail {
    position_m: [f64; 3],
    rx_clock_s: f64,
    rx_clock_drift_s_s: Option<f64>,
    geodetic: Option<[f64; 3]>,
    position_covariance_ecef_m2: Vec<f64>,
    position_covariance_enu_m2: Vec<f64>,
    system_clocks_s: Vec<(String, f64)>,
    used_sats: Vec<String>,
    residuals_m: Vec<f64>,
    pseudorange_variances_m2: Vec<f64>,
    weights: Vec<f64>,
    rejected_sats: Vec<crate::spp::RejectedSatJs>,
    geometry_quality: crate::geometry_quality::GeometryQualityJs,
    redundancy: isize,
    raim_checkable: bool,
    dop: Option<[f64; 5]>,
    system_tdops: Vec<(String, f64)>,
    iterations: usize,
    converged: bool,
    status: &'static str,
    outer_iterations: usize,
    final_robust_scale_m: Option<f64>,
    used_count: usize,
    systems: Vec<String>,
    ionosphere_applied: bool,
    troposphere_applied: bool,
    ut1_degraded: Option<String>,
}

impl From<&ReceiverSolution> for FdeUnresolvedSolutionDetail {
    fn from(solution: &ReceiverSolution) -> Self {
        let geodetic = solution
            .geodetic
            .map(|value| [value.lat_rad, value.lon_rad, value.height_m]);
        let flatten = |matrix: &[[f64; 3]; 3]| matrix.iter().flatten().copied().collect();
        Self {
            position_m: [
                solution.position.x_m,
                solution.position.y_m,
                solution.position.z_m,
            ],
            rx_clock_s: solution.rx_clock_s,
            rx_clock_drift_s_s: solution.rx_clock_drift_s_s,
            geodetic,
            position_covariance_ecef_m2: flatten(&solution.position_covariance.ecef_m2),
            position_covariance_enu_m2: flatten(&solution.position_covariance.enu_m2),
            system_clocks_s: solution
                .system_clocks_s
                .iter()
                .map(|(system, clock)| (format!("{system:?}"), *clock))
                .collect(),
            used_sats: solution.used_sats.iter().map(ToString::to_string).collect(),
            residuals_m: solution.residuals_m.clone(),
            pseudorange_variances_m2: solution.pseudorange_variances_m2.clone(),
            weights: solution.weights.clone(),
            rejected_sats: solution
                .rejected_sats
                .iter()
                .map(crate::spp::RejectedSatJs::from)
                .collect(),
            geometry_quality: crate::geometry_quality::GeometryQualityJs::from(
                solution.geometry_quality,
            ),
            redundancy: solution.metadata.redundancy,
            raim_checkable: solution.metadata.raim_checkable,
            dop: solution
                .dop
                .as_ref()
                .map(|dop| [dop.gdop, dop.pdop, dop.hdop, dop.vdop, dop.tdop]),
            system_tdops: solution
                .system_tdops
                .iter()
                .map(|(system, tdop)| (format!("{system:?}"), *tdop))
                .collect(),
            iterations: solution.metadata.iterations,
            converged: solution.metadata.converged,
            status: crate::spp::solve_status_label(solution.metadata.status),
            outer_iterations: solution.metadata.outer_iterations,
            final_robust_scale_m: solution.metadata.final_robust_scale_m,
            used_count: solution.metadata.used_count,
            systems: solution
                .metadata
                .systems
                .iter()
                .map(|system| system.as_str().to_owned())
                .collect(),
            ionosphere_applied: solution.metadata.ionosphere_applied,
            troposphere_applied: solution.metadata.troposphere_applied,
            ut1_degraded: solution
                .metadata
                .ut1_degraded
                .map(|reason| degrade_reason_label(reason).to_owned()),
        }
    }
}

pub(crate) fn fde_detail(
    error: &FdeError<ReceiverSolution, FdeSppError>,
) -> PositioningErrorDetail {
    match error {
        FdeError::FaultUnresolved(unresolved) => {
            let reason = match unresolved.reason {
                FdeUnresolvedReason::ExclusionBudgetExhausted => "EXCLUSION_BUDGET_EXHAUSTED",
                FdeUnresolvedReason::NoAdmissibleExclusion => "NO_ADMISSIBLE_EXCLUSION",
                _ => "UNKNOWN",
            };
            let message = match unresolved.reason {
                FdeUnresolvedReason::ExclusionBudgetExhausted => format!(
                    "RAIM fault unresolved after the exclusion budget, test statistic {}",
                    unresolved.raim.test_statistic
                ),
                FdeUnresolvedReason::NoAdmissibleExclusion => format!(
                    "RAIM fault unresolved because no exclusion was admissible, test statistic {}",
                    unresolved.raim.test_statistic
                ),
                _ => format!(
                    "RAIM fault unresolved ({:?}), test statistic {}",
                    unresolved.reason, unresolved.raim.test_statistic
                ),
            };
            let raim_input = sidereon_core::quality::RaimInput {
                used_sats: unresolved
                    .solution
                    .used_sats
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
                residuals_m: unresolved.solution.residuals_m.clone(),
                variances_m2: Some(unresolved.solution.pseudorange_variances_m2.clone()),
            };
            PositioningErrorDetail::FaultUnresolved {
                message,
                reason,
                test_statistic: unresolved.raim.test_statistic,
                solution: Box::new(FdeUnresolvedSolutionDetail::from(&unresolved.solution)),
                excluded: unresolved.excluded.clone(),
                raim: crate::raim::result_from_core(unresolved.raim.clone(), &raim_input),
            }
        }
        FdeError::Solve(solve @ FdeSppError::Spp(inner)) => {
            spp_detail(inner).with_message(solve.to_string())
        }
        FdeError::Solve(solve @ FdeSppError::Validation(inner)) => {
            solution_rejected(solve.to_string(), inner)
        }
        FdeError::Raim(inner) => PositioningErrorDetail::RaimConfiguration {
            message: format!("RAIM configuration rejected: {inner}"),
            quality: quality_label(*inner),
        },
    }
}

/// The detail of a [`DgnssError`].
pub(crate) fn dgnss_detail(error: &DgnssError) -> PositioningErrorDetail {
    let message = error.to_string();
    match error {
        DgnssError::InvalidInput { field, reason } => PositioningErrorDetail::DgnssInvalidInput {
            message,
            field,
            reason,
        },
        DgnssError::Spp(inner) => spp_detail(inner).with_message(message),
        DgnssError::Ut1OutsideCoverage(reason) => PositioningErrorDetail::Ut1OutsideCoverage {
            message,
            reason: degrade_reason_label(*reason),
        },
    }
}

/// The detail of a [`RinexSppError`] (`#[non_exhaustive]`).
pub(crate) fn rinex_spp_detail(error: &RinexSppError) -> PositioningErrorDetail {
    let message = error.to_string();
    match error {
        RinexSppError::Observation(inner) => PositioningErrorDetail::Observation {
            message,
            error: upper_snake_variant(inner),
            cause: crate::core_error::CoreErrorDetail::from(inner),
        },
        RinexSppError::MissingApproxPosition => {
            PositioningErrorDetail::MissingApproxPosition { message }
        }
        other => PositioningErrorDetail::Other {
            message,
            variant: upper_snake_variant(other),
        },
    }
}

/// The detail of a failure of the facade's `solve_spp`. Only its `Spp`
/// variant is reachable from an SPP solve; any other facade error keeps the
/// engine's message under the facade variant's name.
pub(crate) fn facade_detail(error: &sidereon::Error) -> PositioningErrorDetail {
    match error {
        sidereon::Error::Spp(inner) => solve_policy_detail(inner).with_message(error.to_string()),
        other => PositioningErrorDetail::Other {
            message: other.to_string(),
            variant: upper_snake_variant(other),
        },
    }
}

impl PositioningErrorDetail {
    /// The engine's message the detail carries.
    fn message(&self) -> &str {
        match self {
            Self::InvalidInput { message, .. }
            | Self::TooFewSatellites { message, .. }
            | Self::TooFewMeasurements { message, .. }
            | Self::Singular { message, .. }
            | Self::DuplicateObservation { message, .. }
            | Self::EphemerisLost { message, .. }
            | Self::SelectionUnsettled { message, .. }
            | Self::Ut1OutsideCoverage { message, .. }
            | Self::SolutionRejected { message, .. }
            | Self::NoCoarseSolution { message }
            | Self::EmptyEpochs { message }
            | Self::EpochInput { message, .. }
            | Self::PreciseSolveFailed { message, .. }
            | Self::BroadcastSolveFailed { message, .. }
            | Self::FaultUnresolved { message, .. }
            | Self::RaimConfiguration { message, .. }
            | Self::Observation { message, .. }
            | Self::MissingApproxPosition { message }
            | Self::DgnssInvalidInput { message, .. }
            | Self::RtkFloat { message, .. }
            | Self::RtkFixed { message, .. }
            | Self::Other { message, .. } => message,
        }
    }

    /// The detail with `message` in place of the one it carries: a wrapper's
    /// failure keeps the typed detail of the variant it wraps and states the
    /// wrapper's own message, the text the engine reports for the whole
    /// failure.
    fn with_message(mut self, message_text: String) -> Self {
        match &mut self {
            Self::InvalidInput { message, .. }
            | Self::TooFewSatellites { message, .. }
            | Self::TooFewMeasurements { message, .. }
            | Self::Singular { message, .. }
            | Self::DuplicateObservation { message, .. }
            | Self::EphemerisLost { message, .. }
            | Self::SelectionUnsettled { message, .. }
            | Self::Ut1OutsideCoverage { message, .. }
            | Self::SolutionRejected { message, .. }
            | Self::NoCoarseSolution { message }
            | Self::EmptyEpochs { message }
            | Self::EpochInput { message, .. }
            | Self::PreciseSolveFailed { message, .. }
            | Self::BroadcastSolveFailed { message, .. }
            | Self::FaultUnresolved { message, .. }
            | Self::RaimConfiguration { message, .. }
            | Self::Observation { message, .. }
            | Self::MissingApproxPosition { message }
            | Self::DgnssInvalidInput { message, .. }
            | Self::RtkFloat { message, .. }
            | Self::RtkFixed { message, .. }
            | Self::Other { message, .. } => *message = message_text,
        }
        self
    }
}

/// A thrown `PositioningError` carrying `detail`.
pub(crate) fn positioning_error(detail: &PositioningErrorDetail) -> JsValue {
    error_with_detail(POSITIONING_ERROR, detail.message(), detail)
}

/// `detail` as a plain JS object, for a getter that reports a failure without
/// throwing it.
pub(crate) fn detail_to_js(detail: &PositioningErrorDetail) -> Result<JsValue, JsValue> {
    crate::error::to_plain_js(detail, "positioning error detail")
}

/// A thrown `PositioningError` for an [`SppError`].
pub(crate) fn spp_error(error: &SppError) -> JsValue {
    positioning_error(&spp_detail(error))
}

/// A thrown `PositioningError` for a failure of the facade's `solve_spp`.
pub(crate) fn facade_error(error: &sidereon::Error) -> JsValue {
    positioning_error(&facade_detail(error))
}

pub(crate) fn core_source_error(error: &sidereon_core::Error) -> JsValue {
    match error {
        sidereon_core::Error::Ut1OutsideCoverage(reason) => {
            ut1_refusal_error(*reason, error.to_string())
        }
        other => crate::core_error::core_error_js(other),
    }
}

pub(crate) fn ut1_refusal_error(
    reason: sidereon_core::astro::time::DegradeReason,
    message: String,
) -> JsValue {
    positioning_error(&PositioningErrorDetail::Ut1OutsideCoverage {
        message,
        reason: degrade_reason_label(reason),
    })
}

#[cfg(test)]
mod rinex_spp_detail_tests {
    use super::rinex_spp_detail;
    use sidereon_core::positioning::RinexSppError;
    use sidereon_core::Error as CoreError;

    #[test]
    fn observation_parse_error_keeps_complete_nested_detail_and_messages() {
        let error = RinexSppError::Observation(CoreError::Parse(
            "malformed event record at epoch 1".into(),
        ));
        let detail = rinex_spp_detail(&error);

        assert_eq!(
            serde_json::to_value(detail).expect("serializable positioning detail"),
            serde_json::json!({
                "kind": "OBSERVATION",
                "message": "RINEX SPP observation assembly failed: parse error: malformed event record at epoch 1",
                "error": "PARSE",
                "cause": {
                    "kind": "PARSE",
                    "message": "malformed event record at epoch 1"
                }
            })
        );
        assert_eq!(
            error.to_string(),
            "RINEX SPP observation assembly failed: parse error: malformed event record at epoch 1"
        );
    }
}

#[cfg(test)]
mod rtk_detail_serialization_tests {
    use super::*;
    use serde_json::{json, Value};

    fn serialized<T: Serialize>(value: &T) -> Value {
        serde_json::to_value(value).expect("RTK detail serializes")
    }

    #[test]
    fn rtk_error_detail_variants_and_multiword_fields_are_camel_case() {
        let antenna = [
            ReceiverAntennaErrorDetail::MissingPcv,
            ReceiverAntennaErrorDetail::InvalidGeometry,
        ];
        let ils = [
            IlsErrorDetail::Singular,
            IlsErrorDetail::NoCandidates { evaluated: 1 },
            IlsErrorDetail::TooManyCandidates {
                evaluated: 2,
                limit: 3,
            },
            IlsErrorDetail::InvalidDimensions { n: 4, rows: 5 },
            IlsErrorDetail::NonFinite,
            IlsErrorDetail::InvalidInput {
                field: "scale",
                reason: "invalid",
            },
            IlsErrorDetail::SearchLimitExceeded,
        ];
        let float = [
            FloatSolveErrorDetail::MissingSystemReference {
                system: "GPS".into(),
            },
            FloatSolveErrorDetail::MissingAmbiguityColumn {
                ambiguity_id: "G02".into(),
            },
            FloatSolveErrorDetail::InvalidInput {
                field: "sigma",
                input_kind: "not_positive",
            },
            FloatSolveErrorDetail::SingularGeometry,
            FloatSolveErrorDetail::IncompleteResidualPair,
            FloatSolveErrorDetail::ReceiverAntenna {
                cause: antenna[0].clone(),
            },
        ];
        let fixed = [
            FixedSolveErrorDetail::Float {
                cause: float[0].clone(),
            },
            FixedSolveErrorDetail::Ils {
                cause: ils[0].clone(),
            },
            FixedSolveErrorDetail::MissingAmbiguity {
                ambiguity_id: "G02".into(),
            },
            FixedSolveErrorDetail::MissingWavelength {
                ambiguity_id: "G02".into(),
            },
            FixedSolveErrorDetail::MissingOffset {
                ambiguity_id: "G02".into(),
            },
            FixedSolveErrorDetail::InvalidCovarianceDimensions,
            FixedSolveErrorDetail::InvalidInput {
                field: "maxIterations",
                input_kind: "out_of_range",
            },
            FixedSolveErrorDetail::SingularGeometry,
            FixedSolveErrorDetail::IncompleteResidualPair,
            FixedSolveErrorDetail::ReceiverAntenna {
                cause: antenna[1].clone(),
            },
        ];
        let outlier = ResidualOutlierDetail {
            epoch_index: 2,
            satellite_id: "G05".into(),
            reference_satellite_id: "G01".into(),
            ambiguity_id: "G05".into(),
            component: "code",
            residual_m: 12.0,
            sigma_m: 2.0,
            normalized_residual: 6.0,
            threshold_sigma: 5.0,
        };
        let validated = [
            ValidatedFixedErrorDetail::Fixed {
                cause: fixed[0].clone(),
            },
            ValidatedFixedErrorDetail::ResidualValidationFailed {
                outlier: outlier.clone(),
                exclusions: vec![outlier.clone()],
            },
            ValidatedFixedErrorDetail::DuplicateAmbiguityId {
                ambiguity_id: "G02".into(),
                first_satellite_id: "G02".into(),
                second_satellite_id: "G02".into(),
            },
            ValidatedFixedErrorDetail::Underdetermined {
                row_count: 1,
                unknown_count: 2,
            },
        ];

        fn assert_kinds(values: &[Value], expected: &[&str]) {
            let actual = values
                .iter()
                .map(|value| value["kind"].as_str().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(actual.as_slice(), expected);
        }
        let ils_values = ils.iter().map(serialized).collect::<Vec<_>>();
        assert_kinds(
            &ils_values,
            &[
                "SINGULAR",
                "NO_CANDIDATES",
                "TOO_MANY_CANDIDATES",
                "INVALID_DIMENSIONS",
                "NON_FINITE",
                "INVALID_INPUT",
                "SEARCH_LIMIT_EXCEEDED",
            ],
        );
        let float_values = float.iter().map(serialized).collect::<Vec<_>>();
        assert_kinds(
            &float_values,
            &[
                "MISSING_SYSTEM_REFERENCE",
                "MISSING_AMBIGUITY_COLUMN",
                "INVALID_INPUT",
                "SINGULAR_GEOMETRY",
                "INCOMPLETE_RESIDUAL_PAIR",
                "RECEIVER_ANTENNA",
            ],
        );
        let fixed_values = fixed.iter().map(serialized).collect::<Vec<_>>();
        assert_kinds(
            &fixed_values,
            &[
                "FLOAT",
                "ILS",
                "MISSING_AMBIGUITY",
                "MISSING_WAVELENGTH",
                "MISSING_OFFSET",
                "INVALID_COVARIANCE_DIMENSIONS",
                "INVALID_INPUT",
                "SINGULAR_GEOMETRY",
                "INCOMPLETE_RESIDUAL_PAIR",
                "RECEIVER_ANTENNA",
            ],
        );
        let validated_values = validated.iter().map(serialized).collect::<Vec<_>>();
        assert_kinds(
            &validated_values,
            &[
                "FIXED",
                "RESIDUAL_VALIDATION_FAILED",
                "DUPLICATE_AMBIGUITY_ID",
                "UNDERDETERMINED",
            ],
        );

        let duplicate = serialized(&validated[2]);
        assert_eq!(duplicate["ambiguityId"], "G02");
        assert_eq!(duplicate["firstSatelliteId"], "G02");
        assert_eq!(duplicate["secondSatelliteId"], "G02");
        let underdetermined = serialized(&validated[3]);
        assert_eq!(
            underdetermined,
            json!({"kind":"UNDERDETERMINED", "rowCount":1, "unknownCount":2})
        );
        let fixed_invalid = serialized(&fixed[6]);
        assert_eq!(fixed_invalid["field"], "maxIterations");
        assert_eq!(fixed_invalid["inputKind"], "out_of_range");
        let fixed_missing = serialized(&fixed[2]);
        assert_eq!(fixed_missing["ambiguityId"], "G02");
        assert_eq!(
            serialized(&fixed[0])["cause"]["kind"],
            "MISSING_SYSTEM_REFERENCE"
        );
        assert_eq!(serialized(&fixed[1])["cause"]["kind"], "SINGULAR");
        assert_eq!(serialized(&fixed[9])["cause"]["kind"], "INVALID_GEOMETRY");
        let float_missing = serialized(&float[1]);
        assert_eq!(float_missing["ambiguityId"], "G02");
        let float_invalid = serialized(&float[2]);
        assert_eq!(float_invalid["inputKind"], "not_positive");
        assert_eq!(serialized(&ils[1])["evaluated"], 1);
        assert_eq!(serialized(&ils[2])["limit"], 3);
        assert_eq!(serialized(&ils[3])["rows"], 5);
        assert_eq!(serialized(&ils[5])["field"], "scale");
        assert_eq!(serialized(&ils[5])["reason"], "invalid");
        assert_eq!(serialized(&validated[0])["cause"]["kind"], "FLOAT");

        let outlier_value = serialized(&outlier);
        assert_eq!(outlier_value["epochIndex"], 2);
        assert_eq!(outlier_value["referenceSatelliteId"], "G01");
        assert_eq!(outlier_value["residualM"], 12.0);
        assert_eq!(outlier_value["sigmaM"], 2.0);
        assert_eq!(outlier_value["normalizedResidual"], 6.0);
        assert_eq!(outlier_value["thresholdSigma"], 5.0);
        let residual = serialized(&validated[1]);
        assert_eq!(residual["outlier"], outlier_value);
        assert_eq!(residual["exclusions"][0], outlier_value);
        let float_error = serialized(&PositioningErrorDetail::RtkFloat {
            message: "RTK float geometry is singular".into(),
            cause: FloatSolveErrorDetail::InvalidInput {
                field: "maxIterations",
                input_kind: "out_of_range",
            },
        });
        assert_eq!(float_error["kind"], "RTK_FLOAT");
        assert_eq!(float_error["cause"]["inputKind"], "out_of_range");
    }
}

#[wasm_bindgen::prelude::wasm_bindgen(typescript_custom_section)]
const TS_POSITIONING_ERROR: &str = r#"
/** Why a solution a solve produced was rejected by its validation gate. */
export type SolutionValidationDetail =
  | { kind: "INVALID_OPTIONS"; field: string; reason: string }
  | { kind: "DEGENERATE_GEOMETRY_RANK_DEFICIENT" }
  | { kind: "DEGENERATE_GEOMETRY_PDOP"; pdop: number }
  | { kind: "IMPLAUSIBLE_POSITION"; radiusM: number }
  | { kind: "INVALID_RESIDUALS" }
  | { kind: "NO_CONVERGENCE"; residualRmsM: number };

export type QualityErrorKind =
  | "INVALID_ELEVATION"
  | "MISSING_CN0"
  | "INVALID_PARAMETER"
  | "INVALID_PROBABILITY"
  | "INVALID_SYSTEM_COUNT"
  | "INVALID_DOF"
  | "INVALID_WEIGHT"
  | "INVALID_RELIABILITY_PARAMETER"
  | "INVALID_RESIDUALS"
  | "INVALID_DESIGN"
  | "SINGULAR_GEOMETRY"
  | "MISSING_VARIANCES"
  | "INVALID_VARIANCE";

export interface QualityErrorDetail {
  kind: QualityErrorKind;
}

/** A direct quality failure; its detail preserves the exact core error kind. */
export interface QualityError extends Error {
  name: "Error" | "RangeError" | "TypeError";
  detail: QualityErrorDetail;
}

/**
 * The detail of a thrown PositioningError: the failure of an SPP, static,
 * broadcast, fallback, FDE, DGNSS, RTK float, RTK fixed or RINEX SPP solve, one member per engine
 * variant. A failure that wraps another solve's failure nests it as cause.
 */
export type PositioningErrorDetail =
  | {
      kind: "INVALID_INPUT";
      message: string;
      field: string;
      inputKind:
        | "NON_FINITE"
        | "NOT_POSITIVE"
        | "NEGATIVE"
        | "OUT_OF_RANGE"
        | "MISSING"
        | "FLOAT_PARSE"
        | "INT_PARSE"
        | "INVALID_CIVIL_DATE"
        | "INVALID_CIVIL_TIME"
        | (string & {});
    }
  | { kind: "TOO_FEW_SATELLITES"; message: string; used: number; required: number }
  | { kind: "TOO_FEW_MEASUREMENTS"; message: string; used: number; required: number }
  | {
      kind: "SINGULAR";
      message: string;
      solveError: "SINGULAR_JACOBIAN" | "INVALID_INPUT";
      field: string | null;
      reason: string | null;
    }
  | { kind: "DUPLICATE_OBSERVATION"; message: string; satelliteId: string; epochIndex: number | null }
  | { kind: "EPHEMERIS_LOST"; message: string; satelliteId: string; epochIndex: number | null }
  | { kind: "SELECTION_UNSETTLED"; message: string; passes: number }
  | {
      kind: "UT1_OUTSIDE_COVERAGE";
      message: string;
      reason: "beforeCoverage" | "afterCoverage";
    }
  | { kind: "SOLUTION_REJECTED"; message: string; validation: SolutionValidationDetail }
  | { kind: "NO_COARSE_SOLUTION"; message: string }
  | { kind: "EMPTY_EPOCHS"; message: string }
  | { kind: "EPOCH_INPUT"; message: string; epochIndex: number; cause: PositioningErrorDetail }
  | { kind: "PRECISE_SOLVE_FAILED"; message: string; cause: PositioningErrorDetail }
  | { kind: "BROADCAST_SOLVE_FAILED"; message: string; cause: PositioningErrorDetail }
  | {
      kind: "FAULT_UNRESOLVED";
      message: string;
      reason: "EXCLUSION_BUDGET_EXHAUSTED" | "NO_ADMISSIBLE_EXCLUSION" | "UNKNOWN";
      testStatistic: number;
      solution: FdeUnresolvedSolution;
      excluded: string[];
      raim: FdeRaimResult;
    }
  | {
      kind: "RAIM_CONFIGURATION";
      message: string;
      quality:
        | "INVALID_ELEVATION"
        | "MISSING_CN0"
        | "INVALID_PARAMETER"
        | "INVALID_PROBABILITY"
        | "INVALID_SYSTEM_COUNT"
        | "INVALID_DOF"
        | "INVALID_WEIGHT"
        | "INVALID_RELIABILITY_PARAMETER"
        | "INVALID_RESIDUALS"
        | "INVALID_DESIGN"
        | "SINGULAR_GEOMETRY"
        | "MISSING_VARIANCES"
        | "INVALID_VARIANCE";
    }
  | { kind: "OBSERVATION"; message: string; error: string; cause: CoreErrorDetail }
  | { kind: "MISSING_APPROX_POSITION"; message: string }
  | { kind: "DGNSS_INVALID_INPUT"; message: string; field: string; reason: string }
  | { kind: "RTK_FLOAT"; message: string; cause: FloatSolveErrorDetail }
  | { kind: "RTK_FIXED"; message: string; cause: ValidatedFixedSolveErrorDetail }
  | {
      /** A variant a later engine adds, under its engine name in UPPER_SNAKE_CASE. */
      kind: "OTHER";
      message: string;
      variant: string;
    };

export interface ResidualValidationOutlierDetail {
  epochIndex: number;
  satelliteId: string;
  referenceSatelliteId: string;
  ambiguityId: string;
  component: "code" | "phase";
  residualM: number;
  sigmaM: number;
  normalizedResidual: number;
  thresholdSigma: number;
}

export type ReceiverAntennaErrorDetail =
  | { kind: "MISSING_PCV" }
  | { kind: "INVALID_GEOMETRY" };

export type IlsErrorDetail =
  | { kind: "SINGULAR" }
  | { kind: "NO_CANDIDATES"; evaluated: number }
  | { kind: "TOO_MANY_CANDIDATES"; evaluated: number; limit: number }
  | { kind: "INVALID_DIMENSIONS"; n: number; rows: number }
  | { kind: "NON_FINITE" }
  | { kind: "INVALID_INPUT"; field: string; reason: string }
  | { kind: "SEARCH_LIMIT_EXCEEDED" };

export type RtkInputErrorKind =
  | "non_finite"
  | "not_positive"
  | "negative"
  | "out_of_range"
  | "missing"
  | "float_parse"
  | "int_parse"
  | "invalid_civil_date"
  | "invalid_civil_time";

export type FloatSolveErrorDetail =
  | { kind: "MISSING_SYSTEM_REFERENCE"; system: string }
  | { kind: "MISSING_AMBIGUITY_COLUMN"; ambiguityId: string }
  | { kind: "INVALID_INPUT"; field: string; inputKind: RtkInputErrorKind }
  | { kind: "SINGULAR_GEOMETRY" }
  | { kind: "INCOMPLETE_RESIDUAL_PAIR" }
  | { kind: "RECEIVER_ANTENNA"; cause: ReceiverAntennaErrorDetail };

export type FixedSolveErrorDetail =
  | { kind: "FLOAT"; cause: FloatSolveErrorDetail }
  | { kind: "ILS"; cause: IlsErrorDetail }
  | { kind: "MISSING_AMBIGUITY"; ambiguityId: string }
  | { kind: "MISSING_WAVELENGTH"; ambiguityId: string }
  | { kind: "MISSING_OFFSET"; ambiguityId: string }
  | { kind: "INVALID_COVARIANCE_DIMENSIONS" }
  | { kind: "INVALID_INPUT"; field: string; inputKind: RtkInputErrorKind }
  | { kind: "SINGULAR_GEOMETRY" }
  | { kind: "INCOMPLETE_RESIDUAL_PAIR" }
  | { kind: "RECEIVER_ANTENNA"; cause: ReceiverAntennaErrorDetail };

export type ValidatedFixedSolveErrorDetail =
  | { kind: "FIXED"; cause: FixedSolveErrorDetail }
  | {
      kind: "RESIDUAL_VALIDATION_FAILED";
      outlier: ResidualValidationOutlierDetail;
      exclusions: ResidualValidationOutlierDetail[];
    }
  | {
      kind: "DUPLICATE_AMBIGUITY_ID";
      ambiguityId: string;
      firstSatelliteId: string;
      secondSatelliteId: string;
    }
  | { kind: "UNDERDETERMINED"; rowCount: number; unknownCount: number };

/** A thrown positioning failure: name "PositioningError", detail typed. */
export interface PositioningError extends Error {
  name: "PositioningError";
  detail: PositioningErrorDetail;
}

/** Plain geometry diagnostics preserved on unresolved solutions. */
export interface FdeGeometryQuality {
  tier: "RankDeficient" | "ZeroRedundancy" | "Weak" | "Nominal";
  redundancy: number;
  rank: number;
  conditionNumber: number;
  gdop: number;
  raimCheckable: boolean;
  covarianceValidated: boolean;
}

/** The complete last receiver solution retained by an unresolved FDE error. */
export interface FdeUnresolvedSolution {
  positionM: [number, number, number];
  rxClockS: number;
  rxClockDriftSS: number | null;
  geodetic: [number, number, number] | null;
  positionCovarianceEcefM2: number[];
  positionCovarianceEnuM2: number[];
  systemClocksS: [string, number][];
  usedSats: string[];
  residualsM: number[];
  pseudorangeVariancesM2: number[];
  weights: number[];
  rejectedSats: SppRejectedSatellite[];
  geometryQuality: FdeGeometryQuality;
  redundancy: number;
  raimCheckable: boolean;
  dop: [number, number, number, number, number] | null;
  systemTdops: [string, number][];
  iterations: number;
  converged: boolean;
  status: SolveStatus;
  outerIterations: number;
  finalRobustScaleM: number | null;
  systems: string[];
  usedCount: number;
  ionosphereApplied: boolean;
  troposphereApplied: boolean;
  ut1Degraded: "beforeCoverage" | "afterCoverage" | null;
}

/** The exact RAIM detection result associated with the retained FDE solution. */
export interface FdeRaimResult {
  faultDetected: boolean;
  testStatistic: number;
  threshold: number | null;
  testable: boolean;
  worstSat: string | null;
  reducedChiSquare: number | null;
  normalizedResiduals: Record<string, number>;
  rmsM: number;
  dof: number;
}

/** Structured core error details from sidereon-core. */
export interface CoreErrorExactFloat {
  decimal: string;
  bitsHex: string;
}

export type CoreDtedTileErrorDetail =
  | { kind: "io"; path: string; message: string }
  | { kind: "tooShort" | "missingUhl1"; path: string }
  | { kind: "invalidEncoding" | "invalidField"; message: string }
  | { kind: "invalidDimensions"; path: string; lonCount: number; latCount: number }
  | { kind: "truncated"; path: string; actual: number; expected: number }
  | {
      kind: "outside";
      longitude: CoreErrorExactFloat;
      latitude: CoreErrorExactFloat;
      originLongitude: CoreErrorExactFloat;
      originLatitude: CoreErrorExactFloat;
    }
  | { kind: "postingIndexOutOfBounds"; longitudeIndex: number; latitudeIndex: number }
  | { kind: "missingDataSentinel"; longitudeIndex: number }
  | { kind: "checksum"; longitudeIndex: number; checksum: number; sum: number }
  | { kind: "emptyCoordinate" }
  | { kind: "invalidHemisphere"; hemisphere: string }
  | { kind: "negativePostingIndex"; index: string }
  | { kind: "coordinateOutOfRange" | "originNotWholeDegree"; field: string; text: string }
  | { kind: "wrongHemisphere"; field: string; hemisphere: string; expected: string }
  | { kind: "intervalCountMismatch"; field: string; intervalTenthsArcsec: number; count: number }
  | { kind: "profileLongitudeCountMismatch"; longitudeIndex: number; declared: number }
  | { kind: "unsupportedPartialProfile"; longitudeIndex: number; firstLatitudeIndex: number }
  | { kind: "nullPosting"; longitudeIndex: number; latitudeIndex: number }
  | { kind: "other"; message: string; debug: string };

export type CoreIonexEpochErrorDetail =
  | { kind: "notWholeSecond" | "fractionalUtcSecond" | "noExactUtcOffset" | "insertedLeapSecond" | "beforeIntegerLeapSeconds" | "outOfRange"; scale: string }
  | { kind: "yearOutOfField"; utcJ2000S: string }
  | { kind: "other"; message: string; debug: string };

export type CoreSbasEncodeCause =
  | Exclude<SbasEncodeCoreDetail, { kind: "unrecognizedSbasEncodeError" }>
  | { kind: "other"; message: string; debug: string };

export type CoreRtcmMsmOptionalProblem =
  | Exclude<RtcmMsmOptionalProblem, { kind: "invalidValue" }>
  | { kind: "invalidValue"; value: string };

export type CoreRtcmDepartureDetail =
  | { kind: "frameReservedBits"; reserved: number; message: string }
  | { kind: "trailingBits"; messageNumber: number; bits: boolean[]; message: string }
  | { kind: "msmCellMaskOver64"; messageNumber: number; cells: number; message: string }
  | { kind: "orderExceedsDegree"; messageNumber: number; layerIndex: number; degree: number; order: number; message: string }
  | { kind: "ssrRecordsShort"; messageNumber: number; declared: number; read: number; message: string }
  | { kind: "recordsShort"; messageNumber: number; declared: number; read: number; message: string }
  | { kind: "unrecognizedDeparture"; message: string };

export type CoreRtcmEncodeCause =
  | Exclude<RtcmEncodeCoreDetail, { kind: "negativeZeroWithValue" | "msmOptional" | "strictDeparture" }>
  | { kind: "negativeZeroWithValue"; messageNumber: number; field: string; value: string }
  | (Omit<Extract<RtcmEncodeCoreDetail, { kind: "msmOptional" }>, "problem"> & { problem: CoreRtcmMsmOptionalProblem })
  | { kind: "strictDeparture"; departure: CoreRtcmDepartureDetail };

export type CoreRtcmLnavRecordError =
  | { kind: "notGps"; satellite: string }
  | { kind: "invalidEpoch"; field: string }
  | { kind: "weekMismatch"; fullWeek: number; decodedWeek: string }
  | { kind: "noUraPrediction"; index: string }
  | { kind: "fitIntervalUnsupported"; fitIntervalFlag: string; iode: string; iodc: string };

export type CoreRtcmConversionCause =
  | Exclude<RtcmConversionCoreDetail, { kind: "fitInterval" }>
  | { kind: "fitInterval"; error: CoreRtcmLnavRecordError };

export type CoreErrorDetail =
  | { kind: "PARSE"; message: string }
  | { kind: "UNKNOWN_SATELLITE"; satelliteId: string }
  | { kind: "MISSING_GLONASS_CHANNEL" }
  | { kind: "MISSING_TERRAIN_TILE"; latIndex: number; lonIndex: number }
  | { kind: "UNKNOWN_TERRAIN_ELEVATION"; latIndex: number; lonIndex: number; latitudePosting: number; longitudePosting: number }
  | { kind: "NON_WGS84_TERRAIN_TILE"; latIndex: number; lonIndex: number; datum: string }
  | { kind: "TERRAIN_TILE"; latIndex: number; lonIndex: number; cause: CoreDtedTileErrorDetail }
  | { kind: "TERRAIN_TILE_ORIGIN"; path: string; latIndex: number; lonIndex: number; originLatitude: number; originLongitude: number }
  | { kind: "IONEX_OUT_OF_COVERAGE"; cause: IonexCoverageError }
  | { kind: "IONEX_NODES_NOT_AVAILABLE"; cause: IonexNodeGap }
  | { kind: "IONEX_SLANT_UNAVAILABLE"; cause: IonexSlantRefusal }
  | { kind: "IONEX_EPOCH"; cause: CoreIonexEpochErrorDetail }
  | { kind: "EPOCH_OUT_OF_RANGE" }
  | { kind: "INSUFFICIENT_PRECISE_NODES"; satelliteId: string; nodes: number; required: number }
  | { kind: "INVALID_INPUT"; message: string }
  | { kind: "SP3_EPOCH_INTERVAL"; field: string; value: CoreErrorExactFloat; reason: string }
  | { kind: "SP3_MERGE_TOLERANCE"; field: string; value: CoreErrorExactFloat; reason: string }
  | { kind: "CONTINUITY_OPTIONS"; field: string; value: CoreErrorExactFloat; reason: string }
  | { kind: "SBAS_ENCODE"; cause: CoreSbasEncodeCause }
  | { kind: "RTCM_ENCODE"; cause: CoreRtcmEncodeCause }
  | { kind: "RTCM_CONVERSION"; cause: CoreRtcmConversionCause }
  | { kind: "UT1_OUTSIDE_COVERAGE"; reason: "beforeCoverage" | "afterCoverage" }
  | { kind: "OTHER"; message: string; variant: string; debug: string };
"#;
