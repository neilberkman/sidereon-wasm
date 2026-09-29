//! Structured details for errors from orbital, propagation, frame, scenario,
//! and exact-cache core APIs.

use serde::Serialize;
use wasm_bindgen::JsValue;

use crate::error::error_with_detail;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DomainErrorDetail {
    family: &'static str,
    cause: serde_json::Value,
}

fn exact_float(value: f64) -> serde_json::Value {
    serde_json::json!({
        "decimal": value.to_string(),
        "bitsHex": format!("{:016x}", value.to_bits()),
    })
}

fn domain_error(family: &'static str, message: &str, cause: serde_json::Value) -> JsValue {
    error_with_detail("Error", message, &DomainErrorDetail { family, cause })
}

pub(crate) fn anomaly_error(error: sidereon_core::astro::anomaly::AnomalyError) -> JsValue {
    domain_error("anomaly", &error.to_string(), anomaly_cause(error))
}

pub(crate) fn elements_error(error: sidereon_core::astro::elements::ElementsError) -> JsValue {
    domain_error("elements", &error.to_string(), elements_cause(error))
}

fn anomaly_cause(error: sidereon_core::astro::anomaly::AnomalyError) -> serde_json::Value {
    use sidereon_core::astro::anomaly::AnomalyError as E;
    match error {
        E::NonFinite { field } => serde_json::json!({"kind":"nonFinite","field":field}),
        E::NegativeEccentricity => serde_json::json!({"kind":"negativeEccentricity"}),
        E::NonPositiveMu => serde_json::json!({"kind":"nonPositiveMu"}),
        E::NonPositiveSemiLatus => serde_json::json!({"kind":"nonPositiveSemiLatus"}),
        E::BeyondAsymptote { nu, limit } => serde_json::json!({
            "kind":"beyondAsymptote", "nu":exact_float(nu), "limit":exact_float(limit)
        }),
        E::InconsistentElements { field } => {
            serde_json::json!({"kind":"inconsistentElements","field":field})
        }
        E::NonConvergent {
            iterations,
            residual,
        } => serde_json::json!({
            "kind":"nonConvergent", "iterations":iterations, "residual":exact_float(residual)
        }),
    }
}

fn elements_cause(error: sidereon_core::astro::elements::ElementsError) -> serde_json::Value {
    use sidereon_core::astro::elements::ElementsError as E;
    match error {
        E::NonFinite { field } => serde_json::json!({"kind":"nonFinite","field":field}),
        E::NonPositiveMu => serde_json::json!({"kind":"nonPositiveMu"}),
        E::ZeroPosition => serde_json::json!({"kind":"zeroPosition"}),
        E::DegenerateOrbit => serde_json::json!({"kind":"degenerateOrbit"}),
        E::NonPositiveSemiLatus => serde_json::json!({"kind":"nonPositiveSemiLatus"}),
    }
}

pub(crate) fn equinoctial_error(
    error: sidereon_core::astro::equinoctial::EquinoctialError,
) -> JsValue {
    use sidereon_core::astro::equinoctial::EquinoctialError as E;
    let cause = match error {
        E::Elements(inner) => serde_json::json!({"kind":"elements","cause":elements_cause(inner)}),
        E::Anomaly(inner) => serde_json::json!({"kind":"anomaly","cause":anomaly_cause(inner)}),
        E::RetrogradePole => serde_json::json!({"kind":"retrogradePole"}),
        E::ParabolicEquinoctial => serde_json::json!({"kind":"parabolicEquinoctial"}),
    };
    domain_error("equinoctial", &error.to_string(), cause)
}

pub(crate) fn propagation_error(error: sidereon_core::astro::error::PropagationError) -> JsValue {
    use sidereon_core::astro::error::PropagationError as E;
    let cause = match &error {
        E::InvalidInput(message) => serde_json::json!({"kind":"invalidInput","message":message}),
        E::NumericalFailure(message) => {
            serde_json::json!({"kind":"numericalFailure","message":message})
        }
        E::MaxStepsExceeded => serde_json::json!({"kind":"maxStepsExceeded"}),
        E::EventFailure(message) => serde_json::json!({"kind":"eventFailure","message":message}),
        E::ForceModelFailure(message) => {
            serde_json::json!({"kind":"forceModelFailure","message":message})
        }
        E::Ut1OutsideCoverage(reason) => {
            serde_json::json!({"kind":"ut1OutsideCoverage","reason":degrade_reason(*reason)})
        }
    };
    domain_error("propagation", &error.to_string(), cause)
}

fn degrade_reason(reason: sidereon_core::astro::time::DegradeReason) -> &'static str {
    match reason {
        sidereon_core::astro::time::DegradeReason::BeforeCoverage => "beforeCoverage",
        sidereon_core::astro::time::DegradeReason::AfterCoverage => "afterCoverage",
    }
}

pub(crate) fn frame_error(
    error: sidereon_core::astro::frames::transforms::FrameTransformError,
) -> JsValue {
    use sidereon_core::astro::frames::transforms::FrameTransformError as E;
    let cause = match error {
        E::InvalidInput { field, reason } => {
            serde_json::json!({"kind":"invalidInput","field":field,"reason":reason})
        }
        E::Ut1OutsideCoverage { reason } => {
            serde_json::json!({"kind":"ut1OutsideCoverage","reason":degrade_reason(reason)})
        }
    };
    domain_error("frameTransform", &error.to_string(), cause)
}

pub(crate) fn nutation_error(
    error: sidereon_core::astro::frames::nutation::NutationError,
) -> JsValue {
    let sidereon_core::astro::frames::nutation::NutationError::InvalidInput { field, reason } =
        error;
    domain_error(
        "frameTransform",
        &error.to_string(),
        serde_json::json!({
            "kind":"nutationInvalidInput", "field":field, "reason":reason
        }),
    )
}

pub(crate) fn precession_error(
    error: sidereon_core::astro::frames::precession::PrecessionError,
) -> JsValue {
    let sidereon_core::astro::frames::precession::PrecessionError::InvalidInput { field, reason } =
        error;
    domain_error(
        "frameTransform",
        &error.to_string(),
        serde_json::json!({
            "kind":"precessionInvalidInput", "field":field, "reason":reason
        }),
    )
}

fn satellite_id(value: sidereon_core::GnssSatelliteId) -> String {
    value.to_string()
}

pub(crate) fn scenario_error(error: sidereon_core::scenario::ScenarioError) -> JsValue {
    match scenario_cause(&error) {
        Ok(cause) => domain_error("scenario", &error.to_string(), cause),
        Err(serialize_error) => domain_error(
            "scenario",
            &format!(
                "{} (failed to serialize typed cause: {serialize_error})",
                error
            ),
            serde_json::json!({"kind":"detailSerializationFailure"}),
        ),
    }
}

fn scenario_cause(
    error: &sidereon_core::scenario::ScenarioError,
) -> Result<serde_json::Value, serde_json::Error> {
    use sidereon_core::scenario::ScenarioError as E;
    let cause = match error {
        E::InvalidInput { field, reason } => {
            serde_json::json!({"kind":"invalidInput","field":field,"reason":reason})
        }
        E::ExternalSourceRequired => serde_json::json!({"kind":"externalSourceRequired"}),
        E::ExternalSourceMismatch {
            field,
            expected,
            actual,
        } => {
            serde_json::json!({"kind":"externalSourceMismatch","field":field,"expected":expected,"actual":actual})
        }
        E::ExternalIonosphereRequired => serde_json::json!({"kind":"externalIonosphereRequired"}),
        E::Ionosphere(message) => serde_json::json!({"kind":"ionosphere","message":message}),
        E::NoEphemeris { satellite } => {
            serde_json::json!({"kind":"noEphemeris","satelliteId":satellite_id(*satellite)})
        }
        E::Ut1OutsideCoverage { satellite, reason } => {
            serde_json::json!({"kind":"ut1OutsideCoverage","satelliteId":satellite_id(*satellite),"reason":degrade_reason(*reason)})
        }
        E::Observable(inner) => {
            let detail = crate::core_error::ObservablesErrorDetail::from(inner);
            serde_json::json!({"kind":"observable","cause":serde_json::to_value(detail)?})
        }
        E::Frame(message) => serde_json::json!({"kind":"frame","message":message}),
    };
    Ok(cause)
}

fn product_date(date: sidereon_core::data::ProductDate) -> serde_json::Value {
    serde_json::json!({"year":date.year,"month":date.month,"day":date.day})
}

fn catalog_error(error: &sidereon_core::data::DataCatalogError) -> serde_json::Value {
    use sidereon_core::data::DataCatalogError as E;
    match error {
        E::UnknownCenter(value) => serde_json::json!({"kind":"unknownCenter","value":value}),
        E::UnknownProductType(value) => {
            serde_json::json!({"kind":"unknownProductType","value":value})
        }
        E::UnsupportedProduct {
            center,
            product_type,
        } => {
            serde_json::json!({"kind":"unsupportedProduct","center":center.code(),"productType":product_type.code()})
        }
        E::UnsupportedDistribution {
            source,
            product_type,
        } => {
            serde_json::json!({"kind":"unsupportedDistribution","source":source.code(),"productType":product_type.code()})
        }
        E::UnsupportedProductEra {
            center,
            product_type,
            date,
        } => {
            serde_json::json!({"kind":"unsupportedProductEra","center":center.code(),"productType":product_type.code(),"date":product_date(*date)})
        }
        E::UnsupportedDistributionEra {
            source,
            center,
            product_type,
            date,
        } => {
            serde_json::json!({"kind":"unsupportedDistributionEra","source":source.code(),"center":center.code(),"productType":product_type.code(),"date":product_date(*date)})
        }
        E::NoDistributionSources => serde_json::json!({"kind":"noDistributionSources"}),
        E::InvalidOfficialFilename(value) => {
            serde_json::json!({"kind":"invalidOfficialFilename","value":value})
        }
        E::InconsistentProductIdentity { field } => {
            serde_json::json!({"kind":"inconsistentProductIdentity","field":field})
        }
        E::NoOpenMirror {
            center,
            product_type,
        } => serde_json::json!({"kind":"noOpenMirror","center":center,"productType":product_type}),
        E::InvalidDate { year, month, day } => {
            serde_json::json!({"kind":"invalidDate","year":year,"month":month,"day":day})
        }
        E::DateOutOfRange => serde_json::json!({"kind":"dateOutOfRange"}),
        E::DateBeforeGpsEpoch(date) => {
            serde_json::json!({"kind":"dateBeforeGpsEpoch","date":product_date(*date)})
        }
        E::InvalidGpsDayOfWeek(day) => serde_json::json!({"kind":"invalidGpsDayOfWeek","day":day}),
        E::InvalidSample(value) => serde_json::json!({"kind":"invalidSample","value":value}),
        E::UnsupportedSample {
            center,
            product_type,
            sample,
        } => {
            serde_json::json!({"kind":"unsupportedSample","center":center.code(),"productType":product_type.code(),"sample":sample})
        }
        E::InvalidSpan(value) => serde_json::json!({"kind":"invalidSpan","value":value}),
        E::InvalidIssue(value) => serde_json::json!({"kind":"invalidIssue","value":value}),
        E::MissingIssue { center } => {
            serde_json::json!({"kind":"missingIssue","center":center.code()})
        }
        E::UnexpectedIssue { center } => {
            serde_json::json!({"kind":"unexpectedIssue","center":center.code()})
        }
        E::UnsupportedIssue { center, issue } => {
            serde_json::json!({"kind":"unsupportedIssue","center":center.code(),"issue":issue})
        }
        E::InvalidDateTime {
            hour,
            minute,
            second,
        } => {
            serde_json::json!({"kind":"invalidDateTime","hour":hour,"minute":minute,"second":second})
        }
        E::NoUltraIssue => serde_json::json!({"kind":"noUltraIssue"}),
        E::NoAvailableUltraIssue => serde_json::json!({"kind":"noAvailableUltraIssue"}),
        E::UnsupportedNominalSchedule {
            center,
            product_type,
        } => {
            serde_json::json!({"kind":"unsupportedNominalSchedule","center":center.code(),"productType":product_type.code()})
        }
        E::UnrecognizedArchiveListing { reason } => {
            serde_json::json!({"kind":"unrecognizedArchiveListing","reason":reason})
        }
        E::InvalidStation(value) => serde_json::json!({"kind":"invalidStation","value":value}),
        E::InvalidCoordinate {
            lat_deg_bits,
            lon_deg_bits,
        } => {
            serde_json::json!({"kind":"invalidCoordinate","latitudeBitsHex":format!("{lat_deg_bits:016x}"),"longitudeBitsHex":format!("{lon_deg_bits:016x}")})
        }
        E::InvalidTileIndex {
            lat_index,
            lon_index,
        } => {
            serde_json::json!({"kind":"invalidTileIndex","latitudeIndex":lat_index,"longitudeIndex":lon_index})
        }
        E::InvalidTileId(value) => serde_json::json!({"kind":"invalidTileId","value":value}),
    }
}

pub(crate) fn exact_cache_error(error: sidereon_core::exact_cache::ExactCacheError) -> JsValue {
    use sidereon_core::exact_cache::ExactCacheError as E;
    let cause = match &error {
        E::Identity(inner) => serde_json::json!({"kind":"identity","cause":catalog_error(inner)}),
        E::InvalidEntryId => serde_json::json!({"kind":"invalidEntryId"}),
        E::InvalidCommit(reason) => serde_json::json!({"kind":"invalidCommit","reason":reason}),
        #[cfg(not(target_arch = "wasm32"))]
        E::Io { operation, source } => {
            serde_json::json!({"kind":"io","operation":operation,"message":source.to_string()})
        }
        #[cfg(not(target_arch = "wasm32"))]
        E::LockTimeout => serde_json::json!({"kind":"lockTimeout"}),
        E::SingleFlightTimeout => serde_json::json!({"kind":"singleFlightTimeout"}),
        E::SingleFlightOwnershipLost => serde_json::json!({"kind":"singleFlightOwnershipLost"}),
        E::InvalidSingleFlightOptions => serde_json::json!({"kind":"invalidSingleFlightOptions"}),
        #[cfg(not(target_arch = "wasm32"))]
        E::UnsupportedPlatform => serde_json::json!({"kind":"unsupportedPlatform"}),
    };
    domain_error("exactCache", &error.to_string(), cause)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_float_payload_keeps_exceptional_binary64_values() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.0] {
            let payload = exact_float(value);
            assert_eq!(payload["bitsHex"], format!("{:016x}", value.to_bits()));
            assert_eq!(payload["decimal"], value.to_string());
        }
    }

    #[test]
    fn nested_scenario_observable_cause_is_typed() -> Result<(), serde_json::Error> {
        let error = sidereon_core::scenario::ScenarioError::Observable(
            sidereon_core::observables::ObservablesError::InvalidInput {
                field: "pseudorange_m",
                kind: sidereon_core::observables::ObservablesInputErrorKind::NonFinite,
            },
        );
        let detail = scenario_cause(&error)?;
        assert_eq!(detail["kind"], "observable");
        assert_eq!(detail["cause"]["kind"], "INVALID_INPUT");
        assert_eq!(detail["cause"]["field"], "pseudorange_m");
        assert_eq!(detail["cause"]["reason"], "not finite");
        Ok(())
    }

    #[test]
    fn anomaly_fields_use_exact_binary64_payloads() {
        use sidereon_core::astro::anomaly::AnomalyError as E;
        let asymptote = anomaly_cause(E::BeyondAsymptote {
            nu: -0.0,
            limit: f64::INFINITY,
        });
        assert_eq!(asymptote["kind"], "beyondAsymptote");
        assert_eq!(asymptote["nu"]["bitsHex"], "8000000000000000");
        assert_eq!(asymptote["limit"]["bitsHex"], "7ff0000000000000");

        let residual = anomaly_cause(E::NonConvergent {
            iterations: 23,
            residual: f64::NAN,
        });
        assert_eq!(residual["iterations"], 23);
        assert_eq!(residual["residual"]["bitsHex"], "7ff8000000000000");
    }

    #[test]
    fn element_payload_retains_the_field_or_unit_variant() {
        use sidereon_core::astro::elements::ElementsError as E;
        assert_eq!(elements_cause(E::NonFinite { field: "mu" })["field"], "mu");
        assert_eq!(
            elements_cause(E::DegenerateOrbit)["kind"],
            "degenerateOrbit"
        );
    }

    #[test]
    fn catalog_coordinate_payload_keeps_the_source_bits() {
        let cause = catalog_error(&sidereon_core::data::DataCatalogError::InvalidCoordinate {
            lat_deg_bits: (-0.0_f64).to_bits(),
            lon_deg_bits: f64::INFINITY.to_bits(),
        });
        assert_eq!(cause["kind"], "invalidCoordinate");
        assert_eq!(cause["latitudeBitsHex"], "8000000000000000");
        assert_eq!(cause["longitudeBitsHex"], "7ff0000000000000");
    }
}

#[wasm_bindgen::prelude::wasm_bindgen(typescript_custom_section)]
const TS_DOMAIN_ERRORS: &str = r#"
export type ExactFloatDetail = { decimal: string; bitsHex: string };
export type AnomalyErrorDetail = { family: "anomaly"; cause:
  | { kind: "nonFinite"; field: string } | { kind: "negativeEccentricity" }
  | { kind: "nonPositiveMu" } | { kind: "nonPositiveSemiLatus" }
  | { kind: "beyondAsymptote"; nu: ExactFloatDetail; limit: ExactFloatDetail }
  | { kind: "inconsistentElements"; field: string }
  | { kind: "nonConvergent"; iterations: number; residual: ExactFloatDetail } };
export type ElementsErrorDetail = { family: "elements"; cause:
  | { kind: "nonFinite"; field: string } | { kind: "nonPositiveMu" }
  | { kind: "zeroPosition" } | { kind: "degenerateOrbit" } | { kind: "nonPositiveSemiLatus" } };
export type EquinoctialErrorDetail = { family: "equinoctial"; cause:
  | { kind: "elements"; cause: ElementsErrorDetail["cause"] }
  | { kind: "anomaly"; cause: AnomalyErrorDetail["cause"] }
  | { kind: "retrogradePole" } | { kind: "parabolicEquinoctial" } };
export type PropagationErrorDetail = { family: "propagation"; cause:
  | { kind: "invalidInput" | "numericalFailure" | "eventFailure" | "forceModelFailure"; message: string }
  | { kind: "maxStepsExceeded" }
  | { kind: "ut1OutsideCoverage"; reason: "beforeCoverage" | "afterCoverage" } };
export type FrameDomainErrorDetail = { family: "frameTransform"; cause:
  | { kind: "invalidInput"; field: string; reason: string }
  | { kind: "ut1OutsideCoverage"; reason: "beforeCoverage" | "afterCoverage" }
  | { kind: "nutationInvalidInput" | "precessionInvalidInput"; field: string; reason: string } };
export type ScenarioErrorDetail = { family: "scenario"; cause:
  | { kind: "invalidInput"; field: string; reason: string }
  | { kind: "externalSourceRequired" } | { kind: "externalSourceMismatch"; field: string; expected: string; actual: string }
  | { kind: "externalIonosphereRequired" } | { kind: "ionosphere" | "frame"; message: string }
  | { kind: "noEphemeris"; satelliteId: string }
  | { kind: "ut1OutsideCoverage"; satelliteId: string; reason: "beforeCoverage" | "afterCoverage" }
  | { kind: "observable"; cause: ObservablesErrorDetail } };
export type ExactCacheErrorDetail = { family: "exactCache"; cause:
  | { kind: "identity"; cause: DataCatalogErrorDetail }
  | { kind: "invalidEntryId" } | { kind: "invalidCommit"; reason: string }
  | { kind: "io"; operation: string; message: string } | { kind: "lockTimeout" }
  | { kind: "singleFlightTimeout" } | { kind: "singleFlightOwnershipLost" }
  | { kind: "invalidSingleFlightOptions" } | { kind: "unsupportedPlatform" } };
export type DataCatalogErrorDetail =
  | { kind: "unknownCenter" | "unknownProductType" | "invalidOfficialFilename" | "invalidSample" | "invalidSpan" | "invalidIssue" | "invalidStation" | "invalidTileId"; value: string }
  | { kind: "unsupportedProduct"; center: string; productType: string }
  | { kind: "unsupportedDistribution"; source: string; productType: string }
  | { kind: "unsupportedProductEra"; center: string; productType: string; date: { year: number; month: number; day: number } }
  | { kind: "unsupportedDistributionEra"; source: string; center: string; productType: string; date: { year: number; month: number; day: number } }
  | { kind: "noDistributionSources" } | { kind: "inconsistentProductIdentity"; field: string }
  | { kind: "noOpenMirror"; center: string; productType: string }
  | { kind: "invalidDate"; year: number; month: number; day: number }
  | { kind: "dateOutOfRange" } | { kind: "dateBeforeGpsEpoch"; date: { year: number; month: number; day: number } }
  | { kind: "invalidGpsDayOfWeek"; day: number }
  | { kind: "unsupportedSample"; center: string; productType: string; sample: string }
  | { kind: "missingIssue" | "unexpectedIssue"; center: string }
  | { kind: "unsupportedIssue"; center: string; issue: string }
  | { kind: "invalidDateTime"; hour: number; minute: number; second: number }
  | { kind: "noUltraIssue" | "noAvailableUltraIssue" }
  | { kind: "unsupportedNominalSchedule"; center: string; productType: string }
  | { kind: "unrecognizedArchiveListing"; reason: string }
  | { kind: "invalidCoordinate"; latitudeBitsHex: string; longitudeBitsHex: string }
  | { kind: "invalidTileIndex"; latitudeIndex: number; longitudeIndex: number };
export type DomainErrorDetail = AnomalyErrorDetail | ElementsErrorDetail | EquinoctialErrorDetail | PropagationErrorDetail | FrameDomainErrorDetail | ScenarioErrorDetail | ExactCacheErrorDetail;
"#;
