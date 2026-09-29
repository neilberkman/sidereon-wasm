//! Lossless structured details for SGP4, TLE, look-angle, pass, and fit errors.

use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::astro_error::frame_transform_cause;
use crate::error::error_with_detail;
use sidereon::passes::{LookAngleError, PassError};
use sidereon::sgp4::{
    DecayLatchedError, Error as CoreSgp4Error, Sgp4InputErrorKind, TleFit as CoreTleFit,
    TleRecordIssue,
};
use sidereon::tle::TleError;
use sidereon_core::astro::omm::{
    Omm as CoreOmm, OmmComments, OmmCovariance, OmmEpoch, OmmSpacecraft, OmmUserDefined,
};
use sidereon_core::astro::sgp4::{ElementSet, FitStatistics, JulianDate};
use sidereon_core::astro::time::DegradeReason;
use trust_region_least_squares::trf::{BackendError, TrfError};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorDetail {
    family: &'static str,
    cause: serde_json::Value,
}

fn detail(family: &'static str, message: &str, cause: serde_json::Value) -> JsValue {
    error_with_detail("Error", message, &ErrorDetail { family, cause })
}

fn exact_float(value: f64) -> serde_json::Value {
    serde_json::json!({
        "decimal": value.to_string(),
        "bitsHex": format!("{:016x}", value.to_bits()),
    })
}

fn sgp4_cause(error: &CoreSgp4Error) -> serde_json::Value {
    use CoreSgp4Error as E;
    match error {
        E::InvalidInput { field, kind } => serde_json::json!({
            "kind":"invalidInput", "field":field, "inputKind":input_kind(*kind),
            "reason":kind.to_string(),
        }),
        E::NonFiniteOutput { field } => serde_json::json!({
            "kind":"nonFiniteOutput", "field":field,
        }),
        E::InvalidTle(message) => serde_json::json!({
            "kind":"invalidTle", "message":message,
        }),
        E::Sgp4 { code } => serde_json::json!({"kind":"sgp4", "code":code}),
        E::ResonanceStepBudget { budget } => serde_json::json!({
            "kind":"resonanceStepBudget", "budget":budget.to_string(),
        }),
    }
}

fn input_kind(kind: Sgp4InputErrorKind) -> &'static str {
    match kind {
        Sgp4InputErrorKind::NonFinite => "nonFinite",
        Sgp4InputErrorKind::NotPositive => "notPositive",
        Sgp4InputErrorKind::Negative => "negative",
        Sgp4InputErrorKind::OutOfRange => "outOfRange",
        Sgp4InputErrorKind::Missing => "missing",
        Sgp4InputErrorKind::FloatParse => "floatParse",
        Sgp4InputErrorKind::IntParse => "intParse",
        Sgp4InputErrorKind::InvalidCivilDate => "invalidCivilDate",
        Sgp4InputErrorKind::InvalidCivilTime => "invalidCivilTime",
    }
}

pub(crate) fn sgp4_error(error: CoreSgp4Error) -> JsValue {
    detail("sgp4", &error.to_string(), sgp4_cause(&error))
}

fn tle_cause(error: &TleError) -> serde_json::Value {
    use TleError as E;
    match error {
        E::NonAscii => serde_json::json!({"kind":"nonAscii"}),
        E::Format => serde_json::json!({"kind":"format"}),
        E::SatelliteMismatch => serde_json::json!({"kind":"satelliteMismatch"}),
        E::InvalidCatalogNumber { value, reason } => serde_json::json!({
            "kind":"invalidCatalogNumber", "value":value, "reason":reason,
        }),
        E::CatalogNumberOutOfRange { catalog_number } => serde_json::json!({
            "kind":"catalogNumberOutOfRange", "catalogNumber":catalog_number,
        }),
        E::InvalidField { field, reason } => serde_json::json!({
            "kind":"invalidField", "field":field, "reason":reason,
        }),
        E::Field(value) => serde_json::json!({"kind":"field", "value":value}),
        E::ChecksumMismatch {
            line_label,
            expected,
            computed,
        } => serde_json::json!({
            "kind":"checksumMismatch", "lineLabel":line_label, "expected":expected,
            "computed":computed,
        }),
        E::ChecksumNotDigit {
            line_label,
            found,
            computed,
        } => serde_json::json!({
            "kind":"checksumNotDigit", "lineLabel":line_label, "found":found.to_string(),
            "computed":computed,
        }),
    }
}

pub(crate) fn tle_error(error: TleError) -> JsValue {
    detail("tle", &error.to_string(), tle_cause(&error))
}

fn decay_cause(error: &DecayLatchedError) -> serde_json::Value {
    match error {
        DecayLatchedError::Decayed {
            first_failing_epoch,
            requested_epoch,
        } => serde_json::json!({
            "kind":"decayed",
            "firstFailingEpochMinutes":exact_float(first_failing_epoch.0),
            "requestedEpochMinutes":exact_float(requested_epoch.0),
        }),
        DecayLatchedError::Propagation(source) => serde_json::json!({
            "kind":"propagation", "message":source.to_string(), "cause":sgp4_cause(source),
        }),
    }
}

pub(crate) fn decay_latched_error(error: DecayLatchedError) -> JsValue {
    detail("decayLatched", &error.to_string(), decay_cause(&error))
}

fn look_angle_cause(error: &LookAngleError) -> serde_json::Value {
    use LookAngleError as E;
    match error {
        E::InvalidInput { field, reason } => serde_json::json!({
            "kind":"invalidInput", "field":field, "reason":reason,
        }),
        E::Init(source) => serde_json::json!({
            "kind":"init", "message":source.to_string(), "cause":sgp4_cause(source),
        }),
        E::Propagate(source) => serde_json::json!({
            "kind":"propagate", "message":source.to_string(), "cause":sgp4_cause(source),
        }),
        E::FrameTransform(source) => serde_json::json!({
            "kind":"frameTransform", "message":source.to_string(),
            "cause":frame_transform_cause(*source),
        }),
    }
}

pub(crate) fn look_angle_error(error: LookAngleError) -> JsValue {
    detail("lookAngle", &error.to_string(), look_angle_cause(&error))
}

fn pass_cause(error: &PassError) -> serde_json::Value {
    match error {
        PassError::InvalidInput { field, reason } => serde_json::json!({
            "kind":"invalidInput", "field":field, "reason":reason,
        }),
        PassError::Ut1OutsideCoverage(reason) => serde_json::json!({
            "kind":"ut1OutsideCoverage",
            "reason":match reason {
                DegradeReason::BeforeCoverage => "beforeCoverage",
                DegradeReason::AfterCoverage => "afterCoverage",
            },
        }),
    }
}

pub(crate) fn pass_error(error: PassError) -> JsValue {
    detail("pass", &error.to_string(), pass_cause(&error))
}

fn trf_cause(error: &TrfError) -> serde_json::Value {
    use TrfError as E;
    match error {
        E::EmptyResidual => serde_json::json!({"kind":"emptyResidual"}),
        E::EmptyParameters => serde_json::json!({"kind":"emptyParameters"}),
        E::NonFiniteParameters => serde_json::json!({"kind":"nonFiniteParameters"}),
        E::NonFiniteInitialResidual => serde_json::json!({"kind":"nonFiniteInitialResidual"}),
        E::InsufficientRows { m, n } => {
            serde_json::json!({"kind":"insufficientRows", "m":m, "n":n})
        }
        E::SizeOverflow { m, n } => serde_json::json!({"kind":"sizeOverflow", "m":m, "n":n}),
        E::DegreeOverflow { degree } => {
            serde_json::json!({"kind":"degreeOverflow", "degree":degree})
        }
        E::InvalidMaxNfev => serde_json::json!({"kind":"invalidMaxNfev"}),
        E::InvalidFScale { f_scale } => {
            serde_json::json!({"kind":"invalidFScale", "fScale":exact_float(*f_scale)})
        }
        E::InvalidXScaleLength { expected, got } => {
            serde_json::json!({"kind":"invalidXScaleLength", "expected":expected, "got":got})
        }
        E::InvalidXScaleValue { index, value } => {
            serde_json::json!({"kind":"invalidXScaleValue", "index":index, "value":exact_float(*value)})
        }
        E::InvalidJacobianLength { expected, got } => {
            serde_json::json!({"kind":"invalidJacobianLength", "expected":expected, "got":got})
        }
        E::InvalidResidualLength { expected, got } => {
            serde_json::json!({"kind":"invalidResidualLength", "expected":expected, "got":got})
        }
        E::InvalidSliceLength {
            what,
            expected,
            got,
        } => {
            serde_json::json!({"kind":"invalidSliceLength", "what":what, "expected":expected, "got":got})
        }
        E::InvalidSvdOutput(message) => {
            serde_json::json!({"kind":"invalidSvdOutput", "message":message})
        }
        E::Backend(source) => serde_json::json!({"kind":"backend", "cause":backend_cause(source)}),
    }
}

fn backend_cause(error: &BackendError) -> serde_json::Value {
    match error {
        BackendError::Failed(message) => serde_json::json!({"kind":"failed", "message":message}),
        BackendError::BadDimensions {
            expected_m,
            expected_n,
            got,
        } => serde_json::json!({
            "kind":"badDimensions", "expectedM":expected_m, "expectedN":expected_n, "got":got,
        }),
    }
}

fn exact_integer(value: impl ToString) -> serde_json::Value {
    serde_json::json!({"decimal":value.to_string()})
}

fn exact_option_float(value: Option<f64>) -> serde_json::Value {
    value.map(exact_float).unwrap_or(serde_json::Value::Null)
}

fn exact_option_integer(value: Option<impl ToString>) -> serde_json::Value {
    value.map(exact_integer).unwrap_or(serde_json::Value::Null)
}

fn exact_julian_date(value: JulianDate) -> serde_json::Value {
    serde_json::json!([exact_float(value.0), exact_float(value.1)])
}

fn exact_elements(value: &ElementSet) -> serde_json::Value {
    serde_json::json!({
        "epoch":exact_julian_date(value.epoch),
        "bstar":exact_float(value.bstar),
        "mean_motion_dot":exact_option_float(value.mean_motion_dot),
        "mean_motion_double_dot":exact_option_float(value.mean_motion_double_dot),
        "eccentricity":exact_float(value.eccentricity),
        "argument_of_perigee_deg":exact_float(value.argument_of_perigee_deg),
        "inclination_deg":exact_float(value.inclination_deg),
        "mean_anomaly_deg":exact_float(value.mean_anomaly_deg),
        "mean_motion_rev_per_day":exact_float(value.mean_motion_rev_per_day),
        "right_ascension_deg":exact_float(value.right_ascension_deg),
        "catalog_number":exact_option_integer(value.catalog_number),
        "omm_epoch_days":exact_option_float(value.omm_epoch_days),
    })
}

fn exact_omm_epoch(value: &OmmEpoch) -> serde_json::Value {
    serde_json::json!({
        "year":exact_integer(value.year),
        "month":exact_integer(value.month),
        "day":exact_integer(value.day),
        "hour":exact_integer(value.hour),
        "minute":exact_integer(value.minute),
        "second":exact_integer(value.second),
        "microsecond":exact_integer(value.microsecond),
        "femtosecond":exact_integer(value.femtosecond),
    })
}

fn exact_omm_comments(value: &OmmComments) -> serde_json::Value {
    serde_json::json!({
        "header":value.header,
        "metadata":value.metadata,
        "mean_elements":value.mean_elements,
        "tle_parameters":value.tle_parameters,
        "user_defined":value.user_defined,
    })
}

fn exact_omm_spacecraft(value: &OmmSpacecraft) -> serde_json::Value {
    serde_json::json!({
        "comments":value.comments,
        "mass_kg":exact_option_float(value.mass_kg),
        "solar_rad_area_m2":exact_option_float(value.solar_rad_area_m2),
        "solar_rad_coeff":exact_option_float(value.solar_rad_coeff),
        "drag_area_m2":exact_option_float(value.drag_area_m2),
        "drag_coeff":exact_option_float(value.drag_coeff),
    })
}

fn exact_omm_covariance(value: &OmmCovariance) -> serde_json::Value {
    serde_json::json!({
        "comments":value.comments,
        "cov_ref_frame":value.cov_ref_frame,
        "lower_triangle":value.lower_triangle.map(exact_float),
    })
}

fn exact_omm_user_defined(value: &OmmUserDefined) -> serde_json::Value {
    serde_json::json!({"parameter":value.parameter, "value":value.value})
}

fn exact_omm(value: &CoreOmm) -> serde_json::Value {
    serde_json::json!({
        "ccsds_omm_vers":value.ccsds_omm_vers,
        "classification":value.classification,
        "creation_date":value.creation_date,
        "originator":value.originator,
        "message_id":value.message_id,
        "object_name":value.object_name,
        "object_id":value.object_id,
        "center_name":value.center_name,
        "ref_frame":value.ref_frame,
        "ref_frame_epoch":value.ref_frame_epoch,
        "time_system":value.time_system,
        "mean_element_theory":value.mean_element_theory,
        "epoch":exact_omm_epoch(&value.epoch),
        "mean_motion":exact_option_float(value.mean_motion),
        "semi_major_axis_km":exact_option_float(value.semi_major_axis_km),
        "eccentricity":exact_float(value.eccentricity),
        "inclination_deg":exact_float(value.inclination_deg),
        "ra_of_asc_node_deg":exact_float(value.ra_of_asc_node_deg),
        "arg_of_pericenter_deg":exact_float(value.arg_of_pericenter_deg),
        "mean_anomaly_deg":exact_float(value.mean_anomaly_deg),
        "gm_km3_s2":exact_option_float(value.gm_km3_s2),
        "spacecraft":value.spacecraft.as_ref().map(exact_omm_spacecraft),
        "ephemeris_type":exact_option_integer(value.ephemeris_type),
        "classification_type":value.classification_type,
        "norad_cat_id":exact_option_integer(value.norad_cat_id),
        "element_set_no":exact_option_integer(value.element_set_no),
        "rev_at_epoch":exact_option_integer(value.rev_at_epoch),
        "bstar":exact_option_float(value.bstar),
        "bterm_m2_kg":exact_option_float(value.bterm_m2_kg),
        "mean_motion_dot":exact_option_float(value.mean_motion_dot),
        "mean_motion_ddot":exact_option_float(value.mean_motion_ddot),
        "agom_m2_kg":exact_option_float(value.agom_m2_kg),
        "covariance":value.covariance.as_ref().map(exact_omm_covariance),
        "user_defined":value.user_defined.iter().map(exact_omm_user_defined).collect::<Vec<_>>(),
        "comments":exact_omm_comments(&value.comments),
        "exact_sgp4_epoch":value.exact_sgp4_epoch.map(exact_julian_date),
        "quantize_tle_derived_fields":value.quantize_tle_derived_fields,
    })
}

fn exact_stats(value: &FitStatistics) -> serde_json::Value {
    serde_json::json!({
        "rms_position_km":exact_float(value.rms_position_km),
        "max_position_km":exact_float(value.max_position_km),
        "rms_position_axes_km":value.rms_position_axes_km.map(exact_float),
        "rms_velocity_km_s":exact_option_float(value.rms_velocity_km_s),
        "tle_rms_position_km":exact_float(value.tle_rms_position_km),
        "status":exact_integer(value.status),
        "nfev":exact_integer(value.nfev),
        "njev":exact_integer(value.njev),
        "cost":exact_float(value.cost),
        "optimality":exact_float(value.optimality),
        "bstar_observable":value.bstar_observable,
        "seed_refine_passes":exact_integer(value.seed_refine_passes),
    })
}

fn fit_result_value(result: &CoreTleFit) -> serde_json::Value {
    serde_json::json!({
        "elements":exact_elements(&result.elements),
        "line1":result.line1,
        "line2":result.line2,
        "omm":exact_omm(&result.omm),
        "stats":exact_stats(&result.stats),
    })
}

fn fit_cause(error: &sidereon::sgp4::TleFitError) -> serde_json::Value {
    use sidereon::sgp4::TleFitError as E;
    match error {
        E::ArcTooShort { samples, needed } => {
            serde_json::json!({"kind":"arcTooShort", "samples":samples, "needed":needed})
        }
        E::InvalidInput { field, reason } => {
            serde_json::json!({"kind":"invalidInput", "field":field, "reason":reason})
        }
        E::EpochsNotIncreasing { index } => {
            serde_json::json!({"kind":"epochsNotIncreasing", "index":index})
        }
        E::EpochOutsideArc => serde_json::json!({"kind":"epochOutsideArc"}),
        E::MixedVelocityPresence => serde_json::json!({"kind":"mixedVelocityPresence"}),
        E::NotElliptical => serde_json::json!({"kind":"notElliptical"}),
        E::InclinationNearRetrograde { inclination_deg } => {
            serde_json::json!({"kind":"inclinationNearRetrograde", "inclinationDeg":exact_float(*inclination_deg)})
        }
        E::SeedPropagation {
            epoch_index,
            source,
        } => {
            serde_json::json!({"kind":"seedPropagation", "epochIndex":epoch_index, "message":source.to_string(), "cause":sgp4_cause(source)})
        }
        E::Solver(source) => {
            serde_json::json!({"kind":"solver", "message":source.to_string(), "cause":trf_cause(source)})
        }
        E::SolutionInfeasible => serde_json::json!({"kind":"solutionInfeasible"}),
        E::DidNotConverge { result } => {
            serde_json::json!({"kind":"didNotConverge", "bestEffortFit":fit_result_value(result)})
        }
        E::FinalElements(source) => {
            serde_json::json!({"kind":"finalElements", "message":source.to_string(), "cause":sgp4_cause(source)})
        }
        E::TleEncode(source) => {
            serde_json::json!({"kind":"tleEncode", "message":source.to_string(), "cause":tle_cause(source)})
        }
    }
}

pub(crate) fn fit_error(error: sidereon::sgp4::TleFitError) -> JsValue {
    detail("tleFit", &error.to_string(), fit_cause(&error))
}

pub(crate) fn indexed_sgp4_error(index: usize, error: CoreSgp4Error) -> JsValue {
    detail(
        "sgp4Batch",
        &format!("satellite {index}: {error}"),
        serde_json::json!({
            "kind":"satellitePropagation",
            "satelliteIndex":index,
            "message":error.to_string(),
            "cause":sgp4_cause(&error),
        }),
    )
}

pub(crate) fn record_issue_cause(issue: &TleRecordIssue) -> serde_json::Value {
    match issue {
        TleRecordIssue::Invalid(source) => serde_json::json!({
            "kind":"invalid", "message":source.to_string(), "cause":sgp4_cause(source),
        }),
        TleRecordIssue::MissingLine2 => serde_json::json!({"kind":"missingLine2"}),
        TleRecordIssue::OrphanLine2 => serde_json::json!({"kind":"orphanLine2"}),
        TleRecordIssue::OrphanName => serde_json::json!({"kind":"orphanName"}),
    }
}

#[cfg(test)]
mod tests {
    use super::{exact_float, exact_integer};

    #[test]
    fn exact_fit_detail_primitives_keep_non_finite_bits_and_integer_ranges() {
        let nan = f64::from_bits(0x7ff8_0000_0000_0042);
        assert_eq!(
            exact_float(nan),
            serde_json::json!({"decimal":"NaN", "bitsHex":"7ff8000000000042"})
        );
        assert_eq!(
            exact_float(f64::INFINITY),
            serde_json::json!({"decimal":"inf", "bitsHex":"7ff0000000000000"})
        );
        assert_eq!(
            exact_integer(i64::MIN),
            serde_json::json!({"decimal":"-9223372036854775808"})
        );
        assert_eq!(
            exact_integer(i64::MAX),
            serde_json::json!({"decimal":"9223372036854775807"})
        );
        assert_eq!(
            exact_integer(u64::MAX),
            serde_json::json!({"decimal":"18446744073709551615"})
        );
    }
}
