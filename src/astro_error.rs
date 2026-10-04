//! Lossless structured details for sky, almanac, and SPK core errors.

use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::error::error_with_detail;
use sidereon::almanac::AlmanacError;
use sidereon_core::astro::angles::AngleError;
use sidereon_core::astro::bodies::observe::{BodyObservationError, ObserveError};
use sidereon_core::astro::bodies::sun_moon::SunMoonError;
use sidereon_core::astro::events::EventFinderError;
use sidereon_core::astro::frames::transforms::FrameTransformError;
use sidereon_core::astro::spk::SpkError;
use sidereon_core::astro::time::DegradeReason;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AstroErrorDetail {
    family: &'static str,
    cause: serde_json::Value,
}

fn detail(family: &'static str, message: &str, cause: serde_json::Value) -> JsValue {
    error_with_detail("Error", message, &AstroErrorDetail { family, cause })
}

fn degrade_reason(reason: DegradeReason) -> &'static str {
    match reason {
        DegradeReason::BeforeCoverage => "beforeCoverage",
        DegradeReason::AfterCoverage => "afterCoverage",
    }
}

fn exact_float(value: f64) -> serde_json::Value {
    serde_json::json!({
        "decimal": value.to_string(),
        "bitsHex": format!("{:016x}", value.to_bits()),
    })
}

fn frame_cause(error: FrameTransformError) -> serde_json::Value {
    use FrameTransformError as E;
    match error {
        E::InvalidInput { field, reason } => {
            serde_json::json!({"kind":"invalidInput", "field":field, "reason":reason})
        }
        E::Ut1OutsideCoverage { reason } => serde_json::json!({
            "kind":"ut1OutsideCoverage",
            "reason":degrade_reason(reason),
        }),
    }
}

pub(crate) fn frame_transform_cause(error: FrameTransformError) -> serde_json::Value {
    frame_cause(error)
}

fn sun_moon_cause(error: SunMoonError) -> serde_json::Value {
    use SunMoonError as E;
    match error {
        E::InvalidInput { field, reason } => {
            serde_json::json!({"kind":"invalidInput", "field":field, "reason":reason})
        }
        E::FrameTransform(source) => serde_json::json!({
            "kind":"frameTransform",
            "message":source.to_string(),
            "cause":frame_cause(source),
        }),
    }
}

fn angle_cause(error: AngleError) -> serde_json::Value {
    let AngleError::InvalidInput { field, reason } = error;
    serde_json::json!({"kind":"invalidInput", "field":field, "reason":reason})
}

fn body_observation_cause(error: BodyObservationError) -> serde_json::Value {
    use BodyObservationError as E;
    match error {
        E::Ephemeris(source) => serde_json::json!({
            "kind":"ephemeris",
            "message":source.to_string(),
            "cause":sun_moon_cause(source),
        }),
        E::FrameTransform(source) => serde_json::json!({
            "kind":"frameTransform",
            "message":source.to_string(),
            "cause":frame_cause(source),
        }),
        E::Angle(source) => serde_json::json!({
            "kind":"angle",
            "message":source.to_string(),
            "cause":angle_cause(source),
        }),
    }
}

pub(crate) fn body_observation_error(error: BodyObservationError) -> JsValue {
    detail(
        "bodyObservation",
        &error.to_string(),
        body_observation_cause(error),
    )
}

fn observe_cause(error: ObserveError) -> serde_json::Value {
    use ObserveError as E;
    match error {
        E::Spk(source) => serde_json::json!({
            "kind":"spk", "message":source.to_string(), "cause":spk_cause(&source),
        }),
        E::FrameTransform(source) => serde_json::json!({
            "kind":"frameTransform", "message":source.to_string(), "cause":frame_cause(source),
        }),
        E::SunMoon(source) => serde_json::json!({
            "kind":"sunMoon", "message":source.to_string(), "cause":sun_moon_cause(source),
        }),
        E::Angle(source) => serde_json::json!({
            "kind":"angle", "message":source.to_string(), "cause":angle_cause(source),
        }),
        E::NonFinite => serde_json::json!({"kind":"nonFinite"}),
        E::DegenerateGeometry => serde_json::json!({"kind":"degenerateGeometry"}),
    }
}

pub(crate) fn observe_error(error: ObserveError) -> JsValue {
    let message = error.to_string();
    detail("observation", &message, observe_cause(error))
}

fn event_finder_cause(error: EventFinderError) -> serde_json::Value {
    use EventFinderError as E;
    match error {
        E::InvalidInput { field, reason } => {
            serde_json::json!({"kind":"invalidInput", "field":field, "reason":reason})
        }
        E::Ut1OutsideCoverage(reason) => serde_json::json!({
            "kind":"ut1OutsideCoverage",
            "reason":degrade_reason(reason),
        }),
    }
}

pub(crate) fn event_finder_error(error: EventFinderError) -> JsValue {
    detail("eventFinder", &error.to_string(), event_finder_cause(error))
}

fn spk_cause(error: &SpkError) -> serde_json::Value {
    use SpkError as E;
    match error {
        E::Io { path, message } => {
            serde_json::json!({"kind":"io", "path":path, "message":message})
        }
        E::Truncated {
            context,
            needed,
            actual,
        } => serde_json::json!({
            "kind":"truncated", "context":context, "needed":needed, "actual":actual,
        }),
        E::UnsupportedDafId { id_word } => {
            serde_json::json!({"kind":"unsupportedDafId", "idWord":id_word})
        }
        E::UnsupportedBinaryFormat { binary_format } => serde_json::json!({
            "kind":"unsupportedBinaryFormat", "binaryFormat":binary_format,
        }),
        E::UnsupportedSummaryShape { nd, ni } => {
            serde_json::json!({"kind":"unsupportedSummaryShape", "nd":nd, "ni":ni})
        }
        E::InvalidField { field, value } => {
            serde_json::json!({"kind":"invalidField", "field":field, "value":value})
        }
        E::InvalidDoubleField { field, value } => serde_json::json!({
            "kind":"invalidDoubleField", "field":field, "value":exact_float(*value),
        }),
        E::OutOfCoverage {
            et,
            start_et,
            stop_et,
        } => serde_json::json!({
            "kind":"outOfCoverage",
            "et":exact_float(*et),
            "startEt":exact_float(*start_et),
            "stopEt":exact_float(*stop_et),
        }),
        E::UnsupportedSegmentType { expected, actual } => serde_json::json!({
            "kind":"unsupportedSegmentType", "expected":expected, "actual":actual,
        }),
        E::InvalidSegmentLayout { context } => {
            serde_json::json!({"kind":"invalidSegmentLayout", "context":context})
        }
        E::UnknownBody { body } => serde_json::json!({"kind":"unknownBody", "body":body}),
        E::NoSegmentPath { target, center } => {
            serde_json::json!({"kind":"noSegmentPath", "target":target, "center":center})
        }
        E::CoverageGap { target, center, et } => serde_json::json!({
            "kind":"coverageGap", "target":target, "center":center, "et":exact_float(*et),
        }),
        E::UnsupportedStateSegmentType { data_type } => {
            serde_json::json!({"kind":"unsupportedStateSegmentType", "dataType":data_type})
        }
        E::NonInertialFrameRotation { from, to } => {
            serde_json::json!({"kind":"nonInertialFrameRotation", "from":from, "to":to})
        }
    }
}

pub(crate) fn spk_error(error: SpkError) -> JsValue {
    detail("spk", &error.to_string(), spk_cause(&error))
}

fn almanac_cause(error: &AlmanacError) -> serde_json::Value {
    use AlmanacError as E;
    match error {
        E::Finder(source) => serde_json::json!({
            "kind":"finder",
            "message":source.to_string(),
            "cause":event_finder_cause(*source),
        }),
        E::Spk(source) => serde_json::json!({
            "kind":"spk",
            "message":source.to_string(),
            "cause":spk_cause(source),
        }),
        E::Frame(label) => serde_json::json!({"kind":"frame", "label":label}),
        E::EphemerisRequired => serde_json::json!({"kind":"ephemerisRequired"}),
        E::InferiorPlanetOpposition => {
            serde_json::json!({"kind":"inferiorPlanetOpposition"})
        }
        E::InvalidInput { field, reason } => {
            serde_json::json!({"kind":"invalidInput", "field":field, "reason":reason})
        }
        E::Ut1OutsideCoverage(reason) => serde_json::json!({
            "kind":"ut1OutsideCoverage",
            "reason":degrade_reason(*reason),
        }),
        other => serde_json::json!({"kind":"unknown", "message":other.to_string()}),
    }
}

pub(crate) fn almanac_error(error: AlmanacError) -> JsValue {
    detail("almanac", &error.to_string(), almanac_cause(&error))
}

#[wasm_bindgen(typescript_custom_section)]
const ASTRO_ERROR_TYPES: &str = r#"
export type AstroExactFloat = { decimal: string; bitsHex: string };
export type SpkErrorCause =
  | { kind: "io"; path: string; message: string }
  | { kind: "truncated"; context: string; needed: number; actual: number }
  | { kind: "unsupportedDafId"; idWord: string }
  | { kind: "unsupportedBinaryFormat"; binaryFormat: string }
  | { kind: "unsupportedSummaryShape"; nd: number; ni: number }
  | { kind: "invalidField"; field: string; value: number }
  | { kind: "invalidDoubleField"; field: string; value: AstroExactFloat }
  | { kind: "outOfCoverage"; et: AstroExactFloat; startEt: AstroExactFloat; stopEt: AstroExactFloat }
  | { kind: "unsupportedSegmentType"; expected: number; actual: number }
  | { kind: "invalidSegmentLayout"; context: string }
  | { kind: "unknownBody"; body: number }
  | { kind: "noSegmentPath"; target: number; center: number }
  | { kind: "coverageGap"; target: number; center: number; et: AstroExactFloat }
  | { kind: "unsupportedStateSegmentType"; dataType: number }
  | { kind: "nonInertialFrameRotation"; from: number; to: number };
export type FrameTransformCause =
  | { kind: "invalidInput"; field: string; reason: string }
  | { kind: "ut1OutsideCoverage"; reason: "beforeCoverage" | "afterCoverage" };
export type SunMoonCause =
  | { kind: "invalidInput"; field: string; reason: string }
  | { kind: "frameTransform"; message: string; cause: FrameTransformCause };
export type AngleCause = { kind: "invalidInput"; field: string; reason: string };
export type BodyObservationCause =
  | { kind: "ephemeris"; message: string; cause: SunMoonCause }
  | { kind: "frameTransform"; message: string; cause: FrameTransformCause }
  | { kind: "angle"; message: string; cause: AngleCause };
export type ObserveCause =
  | { kind: "spk"; message: string; cause: SpkErrorCause }
  | { kind: "frameTransform"; message: string; cause: FrameTransformCause }
  | { kind: "sunMoon"; message: string; cause: SunMoonCause }
  | { kind: "angle"; message: string; cause: AngleCause }
  | { kind: "nonFinite" }
  | { kind: "degenerateGeometry" };
export type EventFinderCause =
  | { kind: "invalidInput"; field: string; reason: string }
  | { kind: "ut1OutsideCoverage"; reason: "beforeCoverage" | "afterCoverage" };
export type AlmanacCause =
  | { kind: "finder"; message: string; cause: EventFinderCause }
  | { kind: "spk"; message: string; cause: SpkErrorCause }
  | { kind: "frame"; label: string }
  | { kind: "ephemerisRequired" }
  | { kind: "inferiorPlanetOpposition" }
  | { kind: "invalidInput"; field: string; reason: string }
  | { kind: "ut1OutsideCoverage"; reason: "beforeCoverage" | "afterCoverage" }
  | { kind: "unknown"; message: string };
export type AstroErrorDetail =
  | { family: "bodyObservation"; cause: BodyObservationCause }
  | { family: "observation"; cause: ObserveCause }
  | { family: "eventFinder"; cause: EventFinderCause }
  | { family: "almanac"; cause: AlmanacCause }
  | { family: "spk"; cause: SpkErrorCause };
"#;
