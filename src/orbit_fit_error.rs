//! Structured JavaScript errors for precise-orbit fitting.

use wasm_bindgen::prelude::{wasm_bindgen, JsValue};

use sidereon_core::astro::covariance::RtnFrameError;
use sidereon_core::astro::error::PropagationError;
use sidereon_core::astro::frames::transforms::FrameTransformError;
use sidereon_core::astro::math::least_squares::SolveError;
use sidereon_core::astro::time::{DegradeReason, ValidityMode};
use sidereon_core::orbit_determination::{OrbitFitError, Ut1ProviderRole};

use crate::error::error_with_detail;

fn mode_label(mode: ValidityMode) -> &'static str {
    match mode {
        ValidityMode::Strict => "strict",
        ValidityMode::Permissive => "permissive",
    }
}

fn degrade_label(reason: DegradeReason) -> &'static str {
    match reason {
        DegradeReason::BeforeCoverage => "beforeCoverage",
        DegradeReason::AfterCoverage => "afterCoverage",
    }
}

fn frame_source(error: &FrameTransformError) -> serde_json::Value {
    match error {
        FrameTransformError::InvalidInput { field, reason } => {
            serde_json::json!({"kind":"invalidInput","field":field,"reason":reason})
        }
        FrameTransformError::Ut1OutsideCoverage { reason } => {
            serde_json::json!({"kind":"ut1OutsideCoverage","reason":degrade_label(*reason)})
        }
    }
}

fn propagation_source(error: &PropagationError) -> serde_json::Value {
    match error {
        PropagationError::InvalidInput(message) => {
            serde_json::json!({"kind":"invalidInput","message":message})
        }
        PropagationError::NumericalFailure(message) => {
            serde_json::json!({"kind":"numericalFailure","message":message})
        }
        PropagationError::MaxStepsExceeded => serde_json::json!({"kind":"maxStepsExceeded"}),
        PropagationError::EventFailure(message) => {
            serde_json::json!({"kind":"eventFailure","message":message})
        }
        PropagationError::ForceModelFailure(message) => {
            serde_json::json!({"kind":"forceModelFailure","message":message})
        }
        PropagationError::Ut1OutsideCoverage(reason) => {
            serde_json::json!({"kind":"ut1OutsideCoverage","reason":degrade_label(*reason)})
        }
    }
}

fn solve_source(error: &SolveError) -> serde_json::Value {
    match error {
        SolveError::SingularJacobian => serde_json::json!({"kind":"singularJacobian"}),
        SolveError::InvalidInput { field, reason } => {
            serde_json::json!({"kind":"invalidInput","field":field,"reason":reason})
        }
    }
}

fn rtn_source(error: RtnFrameError) -> serde_json::Value {
    match error {
        RtnFrameError::InvalidInput { field, reason } => {
            serde_json::json!({"kind":"invalidInput","field":field,"reason":reason})
        }
        RtnFrameError::ZeroPosition => serde_json::json!({"kind":"zeroPosition"}),
        RtnFrameError::ParallelPositionVelocity => {
            serde_json::json!({"kind":"parallelPositionVelocity"})
        }
    }
}

fn exact_float(value: f64) -> serde_json::Value {
    serde_json::json!({"decimal":value.to_string(),"bitsHex":format!("{:016x}",value.to_bits())})
}

fn tier_label(tier: sidereon_core::geometry_quality::ObservabilityTier) -> &'static str {
    use sidereon_core::geometry_quality::ObservabilityTier as T;
    match tier {
        T::RankDeficient => "RankDeficient",
        T::ZeroRedundancy => "ZeroRedundancy",
        T::Weak => "Weak",
        T::Nominal => "Nominal",
    }
}

fn detail(error: &OrbitFitError) -> serde_json::Value {
    use OrbitFitError as E;
    let message = error.to_string();
    match error {
        E::EmptySelection => serde_json::json!({"kind":"EMPTY_SELECTION","message":message}),
        E::InvalidOption { field, reason } => {
            serde_json::json!({"kind":"INVALID_OPTION","message":message,"field":field,"reason":reason})
        }
        E::TooFewSamples {
            satellite,
            got,
            required,
        } => serde_json::json!({
            "kind":"TOO_FEW_SAMPLES","message":message,"satellite":satellite.to_string(),
            "got":got,"required":required
        }),
        E::NonMonotonicEpochs { satellite } => serde_json::json!({
            "kind":"NON_MONOTONIC_EPOCHS","message":message,"satellite":satellite.to_string()
        }),
        E::MixedTimeScales => serde_json::json!({"kind":"MIXED_TIME_SCALES","message":message}),
        E::InvalidEpoch { satellite, reason } => serde_json::json!({
            "kind":"INVALID_EPOCH","message":message,"satellite":satellite.to_string(),"reason":reason
        }),
        E::InvalidObservation { satellite, reason } => serde_json::json!({
            "kind":"INVALID_OBSERVATION","message":message,"satellite":satellite.to_string(),"reason":reason
        }),
        E::Frame { satellite, source } => serde_json::json!({
            "kind":"FRAME","message":message,"satellite":satellite.to_string(),"source":frame_source(source)
        }),
        E::Propagation { satellite, source } => serde_json::json!({
            "kind":"PROPAGATION","message":message,"satellite":satellite.to_string(),
            "source":propagation_source(source)
        }),
        E::LeastSquares { satellite, source } => serde_json::json!({
            "kind":"LEAST_SQUARES","message":message,"satellite":satellite.to_string(),"source":solve_source(source)
        }),
        E::SingularGeometry {
            satellite,
            geometry_quality,
        } => serde_json::json!({
            "kind":"SINGULAR_GEOMETRY","message":message,"satellite":satellite.to_string(),
            "geometryQuality":{
                "tier":tier_label(geometry_quality.tier),
                "redundancy":geometry_quality.redundancy,
                "rank":geometry_quality.rank,
                "conditionNumber":exact_float(geometry_quality.condition_number),
                "gdop":exact_float(geometry_quality.gdop),
                "raimCheckable":geometry_quality.raim_checkable,
                "covarianceValidated":geometry_quality.covariance_validated
            }
        }),
        E::DidNotConverge {
            satellite,
            iterations,
        } => serde_json::json!({
            "kind":"DID_NOT_CONVERGE","message":message,"satellite":satellite.to_string(),"iterations":iterations
        }),
        E::Ut1OutsideCoverage(reason) => serde_json::json!({
            "kind":"UT1_OUTSIDE_COVERAGE","message":message,"reason":degrade_label(*reason)
        }),
        E::Ut1ValidityMismatch {
            fit,
            provider,
            provider_mode,
        } => {
            let provider = match provider {
                Ut1ProviderRole::Orientation => "orientation",
                Ut1ProviderRole::Propagation => "propagation",
            };
            serde_json::json!({
                "kind":"UT1_VALIDITY_MISMATCH","message":message,
                "fit":mode_label(*fit),"provider":provider,"providerMode":mode_label(*provider_mode)
            })
        }
        E::RtnFrame { satellite, reason } => serde_json::json!({
            "kind":"RTN_FRAME","message":message,"satellite":satellite.to_string(),
            "source":rtn_source(*reason)
        }),
    }
}

pub(crate) fn js(error: OrbitFitError) -> JsValue {
    let message = error.to_string();
    let details = detail(&error);
    error_with_detail("OrbitFitError", &message, &details)
}

#[wasm_bindgen(typescript_custom_section)]
const TS_ORBIT_FIT_ERROR: &str = r#"
export type OrbitFitErrorDetail =
  | { kind: "EMPTY_SELECTION"; message: string }
  | { kind: "INVALID_OPTION"; message: string; field: string; reason: string }
  | { kind: "TOO_FEW_SAMPLES"; message: string; satellite: string; got: number; required: number }
  | { kind: "NON_MONOTONIC_EPOCHS"; message: string; satellite: string }
  | { kind: "MIXED_TIME_SCALES"; message: string }
  | { kind: "INVALID_EPOCH"; message: string; satellite: string; reason: string }
  | { kind: "INVALID_OBSERVATION"; message: string; satellite: string; reason: string }
  | { kind: "FRAME"; message: string; satellite: string; source: { kind: string; field?: string; reason?: string } }
  | { kind: "PROPAGATION"; message: string; satellite: string; source: { kind: string; message?: string; reason?: string } }
  | { kind: "LEAST_SQUARES"; message: string; satellite: string; source: { kind: string; field?: string; reason?: string } }
  | { kind: "SINGULAR_GEOMETRY"; message: string; satellite: string; geometryQuality: { tier: string; redundancy: number; rank: number; conditionNumber: { decimal: string; bitsHex: string }; gdop: { decimal: string; bitsHex: string }; raimCheckable: boolean; covarianceValidated: boolean } }
  | { kind: "DID_NOT_CONVERGE"; message: string; satellite: string; iterations: number }
  | { kind: "UT1_OUTSIDE_COVERAGE"; message: string; reason: "beforeCoverage" | "afterCoverage" }
  | { kind: "UT1_VALIDITY_MISMATCH"; message: string; fit: "strict" | "permissive"; provider: "orientation" | "propagation"; providerMode: "strict" | "permissive" }
  | { kind: "RTN_FRAME"; message: string; satellite: string; source: { kind: string; field?: string; reason?: string } };

export type OrbitFitError = Error & { name: "OrbitFitError"; detail: OrbitFitErrorDetail };
"#;
