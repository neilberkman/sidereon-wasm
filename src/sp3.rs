//! SP3 precise-ephemeris product: parse, query satellite states by epoch, and
//! feed the SPP solver. Positions cross to JS as `Float64Array`, epochs as plain
//! numbers (seconds since J2000 in the product's own time scale).

use serde::Serialize;
use wasm_bindgen::prelude::*;

use sidereon_core::astro::time::civil::j2000_seconds_from_split;
use sidereon_core::astro::time::{Instant, InstantRepr};
use sidereon_core::data::ProductDate;
use sidereon_core::ephemeris::PositionClockGroupDelay;
use sidereon_core::ephemeris::{
    align_clock_reference as core_align_clock_reference,
    clock_reference_offset as core_clock_reference_offset, parse_exact_sp3 as core_parse_exact_sp3,
    precise_interpolant_store_checksum64 as core_precise_interpolant_store_checksum64,
    validate_exact_sp3 as core_validate_exact_sp3,
    ClockReferenceOffset as CoreClockReferenceOffset, ExactSp3Coverage as CoreExactSp3Coverage,
    ExactSp3Request as CoreExactSp3Request, ExactSp3ValidationError,
    MmapPreciseEphemerisInterpolant as CorePreciseInterpolantArtifact,
    PreciseInterpolantStoreError as CorePreciseInterpolantStoreError, Sp3 as CoreSp3,
    Sp3WriteError as CoreSp3WriteError,
};
use sidereon_core::ephemeris::{
    check_continuity, CellSelection, ContinuityDefect, ContinuityOptionRejection,
    ContinuityOptions, ContinuityOptionsError, EpochWindow, InterpolationNodes, MergeCombine,
    MergeContinuityCell, MergeContinuityCellRole, MergeContinuityReport, MergeContinuityViolation,
    MergeToleranceError, MergeToleranceField, OrbitClass,
    Sp3AccuracyCodeGroup as CoreAccuracyCodeGroup, Sp3AccuracyValue as CoreAccuracyValue,
    Sp3EpochIntervalError, Sp3InterpolationOptions,
    Sp3PositionClockAccuracy as CorePositionClockAccuracy,
    Sp3RawRecordAccuracy as CoreRawRecordAccuracy, Sp3RecordAccuracy as CoreRecordAccuracy,
    Sp3State as CoreState, Sp3VelocityAccuracy as CoreVelocityAccuracy, SpeedBound, StencilExtent,
    UnusableSampleReason, WindowContinuityDecision, WindowContinuityVerdict,
};
use sidereon_core::positioning::{ClockRelativity as CoreClockRelativity, EphemerisSource};
use sidereon_core::DigestProvenance as CoreDigestProvenance;
use sidereon_core::Error as CoreError;
use sidereon_core::GnssSatelliteId;

use crate::data_distribution::GnssProductIdentity;
use crate::error::{
    engine_error, error_with_detail, range_error, safe_integer_number, type_error, u64_bigint,
};
use crate::frames::ExactEpochQueryValue;
use crate::spp::{self, SppSolution};

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum ClockRelativityJs {
    NotApplicable,
    Term { seconds: f64 },
    Unavailable,
}

impl From<CoreClockRelativity> for ClockRelativityJs {
    fn from(value: CoreClockRelativity) -> Self {
        match value {
            CoreClockRelativity::NotApplicable => Self::NotApplicable,
            CoreClockRelativity::Term(seconds) => Self::Term { seconds },
            CoreClockRelativity::Unavailable => Self::Unavailable,
        }
    }
}

pub(crate) fn precise_variance_at_queries(
    source: &impl EphemerisSource,
    satellite: GnssSatelliteId,
    state_epoch: &ExactEpochQueryValue,
    selection_epoch: &ExactEpochQueryValue,
) -> f64 {
    source.ephemeris_variance_at_epoch_query(
        satellite,
        &state_epoch.core(),
        &selection_epoch.core(),
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SelectedPositionClockJs {
    position_ecef_m: [f64; 3],
    clock_s: f64,
    group_delay_s: Option<f64>,
}

pub(crate) fn selected_position_clock_at_queries(
    source: &impl EphemerisSource,
    satellite: GnssSatelliteId,
    state_epoch: &ExactEpochQueryValue,
    selection_epoch: &ExactEpochQueryValue,
) -> Result<JsValue, JsValue> {
    let checked = source
        .try_position_clock_group_delay_selected_at_epoch_query(
            satellite,
            &state_epoch.core(),
            &selection_epoch.core(),
        )
        .map_err(|error| crate::positioning_error::core_source_error(&error))?;
    let Some(checked) = checked else {
        return Ok(JsValue::NULL);
    };
    let (position_ecef_m, clock_s, group_delay_s): PositionClockGroupDelay = checked.value;
    let value = serde_wasm_bindgen::to_value(&SelectedPositionClockJs {
        position_ecef_m,
        clock_s,
        group_delay_s,
    })
    .map_err(engine_error)?;
    crate::error::validated_object(&value, checked.degraded)
}

pub(crate) fn transmit_epoch_clock_at_queries(
    source: &impl EphemerisSource,
    satellite: GnssSatelliteId,
    state_epoch: &ExactEpochQueryValue,
    selection_epoch: &ExactEpochQueryValue,
) -> Result<JsValue, JsValue> {
    let checked = source
        .try_transmit_epoch_clock_at_epoch_query(
            satellite,
            &state_epoch.core(),
            &selection_epoch.core(),
        )
        .map_err(|error| crate::positioning_error::core_source_error(&error))?;
    let Some(checked) = checked else {
        return Ok(JsValue::NULL);
    };
    let value = JsValue::from_f64(checked.value);
    crate::error::validated_object(&value, checked.degraded)
}

pub(crate) fn precise_clock_relativity_at_query(
    source: &impl EphemerisSource,
    satellite: GnssSatelliteId,
    state_epoch: &ExactEpochQueryValue,
    position_ecef_m: [f64; 3],
) -> Result<JsValue, JsValue> {
    let relativity: ClockRelativityJs = source
        .clock_relativity_for_state_at_epoch_query(satellite, &state_epoch.core(), position_ecef_m)
        .into();
    serde_wasm_bindgen::to_value(&relativity).map_err(engine_error)
}

#[derive(Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
enum Sp3AccuracyValueJs {
    Known(f64),
    Unknown,
    TooLarge,
    InvalidBase,
    Overflow,
}

impl From<CoreAccuracyValue> for Sp3AccuracyValueJs {
    fn from(value: CoreAccuracyValue) -> Self {
        match value {
            CoreAccuracyValue::Known(value) => Self::Known(value),
            CoreAccuracyValue::Unknown => Self::Unknown,
            CoreAccuracyValue::TooLarge => Self::TooLarge,
            CoreAccuracyValue::InvalidBase => Self::InvalidBase,
            CoreAccuracyValue::Overflow => Self::Overflow,
            _ => Self::Unknown,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Sp3AccuracyCodeGroupJs {
    axis_exponents: [Option<i16>; 3],
    clock_exponent: Option<i16>,
    position_velocity_base: Option<f64>,
    clock_rate_base: Option<f64>,
}

impl From<CoreAccuracyCodeGroup> for Sp3AccuracyCodeGroupJs {
    fn from(group: CoreAccuracyCodeGroup) -> Self {
        Self {
            axis_exponents: group.axis_exponents,
            clock_exponent: group.clock_exponent,
            position_velocity_base: group.position_velocity_base,
            clock_rate_base: group.clock_rate_base,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Sp3RawRecordAccuracyJs {
    p: Option<Sp3AccuracyCodeGroupJs>,
    v: Option<Sp3AccuracyCodeGroupJs>,
}

impl From<CoreRawRecordAccuracy> for Sp3RawRecordAccuracyJs {
    fn from(accuracy: CoreRawRecordAccuracy) -> Self {
        Self {
            p: accuracy.p.map(Into::into),
            v: accuracy.v.map(Into::into),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Sp3PositionClockAccuracyJs {
    position_sigma_m: [Sp3AccuracyValueJs; 3],
    clock_sigma_m: Sp3AccuracyValueJs,
    position_variance_m2: [Sp3AccuracyValueJs; 3],
    clock_variance_m2: Sp3AccuracyValueJs,
}

impl From<CorePositionClockAccuracy> for Sp3PositionClockAccuracyJs {
    fn from(accuracy: CorePositionClockAccuracy) -> Self {
        Self {
            position_sigma_m: accuracy.position_sigma_m.map(Into::into),
            clock_sigma_m: accuracy.clock_sigma_m.into(),
            position_variance_m2: accuracy.position_variance_m2().map(Into::into),
            clock_variance_m2: accuracy.clock_variance_m2().into(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Sp3VelocityAccuracyJs {
    velocity_sigma_m_s: [Sp3AccuracyValueJs; 3],
    clock_rate_sigma_m_s: Sp3AccuracyValueJs,
    velocity_variance_m2_s2: [Sp3AccuracyValueJs; 3],
    clock_rate_variance_m2_s2: Sp3AccuracyValueJs,
}

impl From<CoreVelocityAccuracy> for Sp3VelocityAccuracyJs {
    fn from(accuracy: CoreVelocityAccuracy) -> Self {
        Self {
            velocity_sigma_m_s: accuracy.velocity_sigma_m_s.map(Into::into),
            clock_rate_sigma_m_s: accuracy.clock_rate_sigma_m_s.into(),
            velocity_variance_m2_s2: accuracy.velocity_variance_m2_s2().map(Into::into),
            clock_rate_variance_m2_s2: accuracy.clock_rate_variance_m2_s2().into(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Sp3RecordAccuracyJs {
    p: Option<Sp3PositionClockAccuracyJs>,
    v: Option<Sp3VelocityAccuracyJs>,
}

impl From<CoreRecordAccuracy> for Sp3RecordAccuracyJs {
    fn from(accuracy: CoreRecordAccuracy) -> Self {
        Self {
            p: accuracy.p.map(Into::into),
            v: accuracy.v.map(Into::into),
        }
    }
}

pub(crate) fn sp3_state_from_core(state: CoreState) -> Sp3State {
    Sp3State {
        position: state.position.as_array().to_vec(),
        clock_s: state.clock_s,
        velocity: state.velocity.map(|velocity| velocity.as_array().to_vec()),
        clock_event: state.flags.clock_event,
        clock_predicted: state.flags.clock_predicted,
        maneuver: state.flags.maneuver,
        orbit_predicted: state.flags.orbit_predicted,
    }
}

/// Parse a satellite token (e.g. `"G01"`) into a typed id, or a `TypeError`.
pub(crate) fn parse_sat(token: &str) -> Result<GnssSatelliteId, JsValue> {
    token
        .parse::<GnssSatelliteId>()
        .map_err(|e| type_error(&format!("invalid satellite token {token:?}: {e}")))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ContinuityOptionsErrorDetailJs {
    field: &'static str,
    value: String,
    reason: &'static str,
}

pub(crate) fn continuity_options_error_js(error: ContinuityOptionsError) -> JsValue {
    let reason = match error.reason {
        ContinuityOptionRejection::NotFinite => "notFinite",
        ContinuityOptionRejection::Negative => "negative",
        _ => "unknown",
    };
    let detail = ContinuityOptionsErrorDetailJs {
        field: error.field,
        value: error.value.to_string(),
        reason,
    };
    error_with_detail("ContinuityOptionsError", &error.to_string(), &detail)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Sp3PolicyErrorDetailJs {
    field: String,
    value: String,
    reason: String,
}

pub(crate) fn sp3_epoch_interval_error_js(error: Sp3EpochIntervalError) -> JsValue {
    let detail = Sp3PolicyErrorDetailJs {
        field: error.field.to_string(),
        value: error.value.to_string(),
        reason: error.reason.to_string(),
    };
    error_with_detail("Sp3EpochIntervalError", &error.to_string(), &detail)
}

pub(crate) fn merge_tolerance_error_js(error: MergeToleranceError) -> JsValue {
    let field = match error.field {
        MergeToleranceField::Position => "positionToleranceM",
        MergeToleranceField::Clock => "clockToleranceS",
        MergeToleranceField::OutlierPosition => "outlierReject.positionToleranceM",
        MergeToleranceField::OutlierClock => "outlierReject.clockToleranceS",
        _ => "unknown",
    };
    let detail = Sp3PolicyErrorDetailJs {
        field: field.to_string(),
        value: error.value.to_string(),
        reason: "must be finite and nonnegative".to_string(),
    };
    error_with_detail("Sp3MergeToleranceError", &error.to_string(), &detail)
}

pub(crate) fn sp3_core_error_js(error: CoreError) -> JsValue {
    match error {
        CoreError::Sp3EpochInterval(error) => sp3_epoch_interval_error_js(error),
        CoreError::Sp3MergeTolerance(error) => merge_tolerance_error_js(error),
        CoreError::ContinuityOptions(error) => continuity_options_error_js(error),
        other => engine_error(other),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeclaredStartMismatchDetailJs {
    requested_j2000_s: f64,
    declared_j2000_s: f64,
    requested_tick: String,
    declared_tick: Option<String>,
}

fn exact_sp3_validation_error_js(error: ExactSp3ValidationError) -> JsValue {
    if let ExactSp3ValidationError::DeclaredStartMismatch {
        requested_j2000_s,
        declared_j2000_s,
        requested_tick,
        declared_tick,
    } = &error
    {
        let detail = DeclaredStartMismatchDetailJs {
            requested_j2000_s: *requested_j2000_s,
            declared_j2000_s: *declared_j2000_s,
            requested_tick: requested_tick.to_string(),
            declared_tick: declared_tick.map(|tick| tick.to_string()),
        };
        return error_with_detail("ExactSp3ValidationError", &error.to_string(), &detail);
    }
    engine_error(error)
}

/// Seconds since J2000 in the instant's own scale, reduced as the engine's SP3
/// code reduces an instant: a split Julian date as whole days, then the day
/// fraction, each scaled to seconds; an integer-nanosecond count as its whole
/// seconds plus the sub-second remainder, split in `i128`.
pub(crate) fn instant_to_j2000_seconds(epoch: &Instant) -> f64 {
    match epoch.repr {
        InstantRepr::JulianDate(jd) => j2000_seconds_from_split(jd.jd_whole, jd.fraction),
        InstantRepr::Nanos(nanos) => {
            const NANOS_PER_SECOND: i128 = 1_000_000_000;
            nanos.div_euclid(NANOS_PER_SECOND) as f64
                + nanos.rem_euclid(NANOS_PER_SECOND) as f64 / 1.0e9
        }
    }
}

pub(crate) fn continuity_options(
    orbit_class: Option<&str>,
    residual_tolerance_m: Option<f64>,
    gap_threshold_factor: Option<f64>,
) -> Result<ContinuityOptions, JsValue> {
    let speed_bound = match orbit_class {
        None => None,
        Some("meo_gnss") => Some(SpeedBound::OrbitClass(OrbitClass::MeoGnss)),
        Some("geosynchronous") => Some(SpeedBound::OrbitClass(OrbitClass::Geosynchronous)),
        Some("leo") => Some(SpeedBound::OrbitClass(OrbitClass::Leo)),
        Some(other) => {
            return Err(type_error(&format!(
                "unknown orbit class {other:?}: expected \"meo_gnss\", \"geosynchronous\", or \"leo\""
            )));
        }
    };
    let mut options = ContinuityOptions::new(speed_bound, residual_tolerance_m)
        .map_err(continuity_options_error_js)?;
    if let Some(factor) = gap_threshold_factor {
        let interpolation = Sp3InterpolationOptions::new(factor).map_err(engine_error)?;
        options = options.with_interpolation_options(interpolation);
    }
    Ok(options)
}

fn continuity_verdict_options(
    orbit_class: JsValue,
    residual_tolerance_m: JsValue,
    gap_threshold_factor: JsValue,
) -> Result<ContinuityOptions, JsValue> {
    let orbit_class = if orbit_class.is_undefined() {
        Some("meo_gnss".to_string())
    } else if orbit_class.is_null() {
        None
    } else {
        Some(
            orbit_class
                .as_string()
                .ok_or_else(|| type_error("orbitClass must be a string or null"))?,
        )
    };
    let residual_tolerance_m = if residual_tolerance_m.is_undefined() {
        Some(1.0)
    } else if residual_tolerance_m.is_null() {
        None
    } else {
        Some(
            residual_tolerance_m
                .as_f64()
                .ok_or_else(|| type_error("residualToleranceM must be a number or null"))?,
        )
    };
    let gap_threshold_factor =
        if gap_threshold_factor.is_undefined() || gap_threshold_factor.is_null() {
            None
        } else {
            Some(
                gap_threshold_factor
                    .as_f64()
                    .ok_or_else(|| type_error("gapThresholdFactor must be a number or null"))?,
            )
        };
    continuity_options(
        orbit_class.as_deref(),
        residual_tolerance_m,
        gap_threshold_factor,
    )
}

fn attach_detail<T: Serialize>(value: &JsValue, detail: &T) {
    let detail_value =
        serde_wasm_bindgen::to_value(detail).expect("serialize precise artifact error detail");
    js_sys::Reflect::set(value, &JsValue::from_str("detail"), &detail_value)
        .expect("attach precise artifact error detail");
}

fn typed_artifact_error<T: Serialize>(name: &'static str, message: String, detail: &T) -> JsValue {
    let js_error = js_sys::Error::new(&message);
    js_error.set_name(name);
    let value: JsValue = js_error.into();
    js_sys::Reflect::set(&value, &JsValue::from_str("kind"), &JsValue::from_str(name))
        .expect("attach precise artifact error kind");
    attach_detail(&value, detail);
    value
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Sp3EpochPredictionJs {
    epoch_j2000_seconds: f64,
    observed: bool,
    orbit_predicted_satellites: Vec<String>,
    clock_predicted_satellites: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Sp3PredictionSummaryJs {
    epochs: Vec<Sp3EpochPredictionJs>,
    observed_through_j2000_seconds: Option<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreciseInterpolantArtifactErrorDetail {
    name: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tag: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    satellite_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    found: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    claimed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    declared: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    available: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    offset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    len: Option<String>,
}

impl PreciseInterpolantArtifactErrorDetail {
    fn new(name: &'static str, message: String) -> Self {
        Self {
            name,
            message,
            path: None,
            reason: None,
            version: None,
            tag: None,
            satellite_id: None,
            expected: None,
            found: None,
            claimed: None,
            declared: None,
            available: None,
            region: None,
            offset: None,
            len: None,
        }
    }
}

fn hex_u64(value: u64) -> String {
    format!("{value:#x}")
}

fn precise_artifact_error_name(error: PreciseInterpolantArtifactError) -> &'static str {
    match error {
        PreciseInterpolantArtifactError::Io => "Io",
        PreciseInterpolantArtifactError::Parse => "Parse",
        PreciseInterpolantArtifactError::BadMagic => "BadMagic",
        PreciseInterpolantArtifactError::HeaderTruncated => "HeaderTruncated",
        PreciseInterpolantArtifactError::Truncated => "Truncated",
        PreciseInterpolantArtifactError::TrailingBytes => "TrailingBytes",
        PreciseInterpolantArtifactError::RangeOutOfBounds => "RangeOutOfBounds",
        PreciseInterpolantArtifactError::UnsupportedVersion => "UnsupportedVersion",
        PreciseInterpolantArtifactError::UnsupportedTimeScale => "UnsupportedTimeScale",
        PreciseInterpolantArtifactError::UnsupportedSatelliteSystem => "UnsupportedSatelliteSystem",
        PreciseInterpolantArtifactError::DuplicateSatellite => "DuplicateSatellite",
        PreciseInterpolantArtifactError::Checksum => "Checksum",
        PreciseInterpolantArtifactError::SatelliteChecksum => "SatelliteChecksum",
        PreciseInterpolantArtifactError::AttestedChecksumMismatch => "AttestedChecksumMismatch",
        PreciseInterpolantArtifactError::Unknown => "Unknown",
    }
}

fn precise_artifact_error(error: CorePreciseInterpolantStoreError) -> JsValue {
    let kind = PreciseInterpolantArtifactError::from(&error);
    let name = precise_artifact_error_name(kind);
    let mut detail = PreciseInterpolantArtifactErrorDetail::new(name, error.to_string());
    match &error {
        CorePreciseInterpolantStoreError::Io { path, .. } => {
            detail.path = Some(path.display().to_string());
        }
        CorePreciseInterpolantStoreError::Parse { reason } => {
            detail.reason = Some(reason.clone());
        }
        CorePreciseInterpolantStoreError::BadMagic { found } => {
            detail.found = Some(format!("{found:02x?}"));
        }
        CorePreciseInterpolantStoreError::HeaderTruncated { available } => {
            detail.available = Some(available.to_string());
        }
        CorePreciseInterpolantStoreError::Truncated {
            declared,
            available,
        }
        | CorePreciseInterpolantStoreError::TrailingBytes {
            declared,
            available,
        } => {
            detail.declared = Some(declared.to_string());
            detail.available = Some(available.to_string());
        }
        CorePreciseInterpolantStoreError::RangeOutOfBounds {
            region,
            sat,
            offset,
            len,
            available,
        } => {
            detail.region = Some((*region).to_string());
            detail.satellite_id = sat.map(|satellite| satellite.to_string());
            detail.offset = Some(offset.to_string());
            detail.len = Some(len.to_string());
            detail.available = Some(available.to_string());
        }
        CorePreciseInterpolantStoreError::UnsupportedVersion { version } => {
            detail.version = Some(*version);
        }
        CorePreciseInterpolantStoreError::UnsupportedTimeScale { tag }
        | CorePreciseInterpolantStoreError::UnsupportedSatelliteSystem { tag } => {
            detail.tag = Some(*tag);
        }
        CorePreciseInterpolantStoreError::DuplicateSatellite { sat } => {
            detail.satellite_id = Some(sat.to_string());
        }
        CorePreciseInterpolantStoreError::Checksum { expected, found } => {
            detail.expected = Some(hex_u64(*expected));
            detail.found = Some(hex_u64(*found));
        }
        CorePreciseInterpolantStoreError::SatelliteChecksum {
            sat,
            expected,
            found,
        } => {
            detail.satellite_id = Some(sat.to_string());
            detail.expected = Some(hex_u64(*expected));
            detail.found = Some(hex_u64(*found));
        }
        CorePreciseInterpolantStoreError::AttestedChecksumMismatch { claimed, declared } => {
            detail.claimed = Some(hex_u64(*claimed));
            detail.declared = Some(hex_u64(*declared));
        }
        _ => detail.reason = Some(error.to_string()),
    }
    typed_artifact_error(name, detail.message.clone(), &detail)
}

/// Error category for precise-interpolant artifact open or serialization.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreciseInterpolantArtifactError {
    /// File I/O failed in the core artifact API.
    Io,
    /// Artifact bytes could not be parsed.
    Parse,
    /// Artifact magic does not match the mapped-store format.
    BadMagic,
    /// Artifact bytes end before the fixed header is complete.
    HeaderTruncated,
    /// The header declares more bytes than are available.
    Truncated,
    /// Bytes remain after the header-declared artifact length.
    TrailingBytes,
    /// An indexed payload region lies beyond the declared artifact bounds.
    RangeOutOfBounds,
    /// The artifact version tag is unsupported.
    UnsupportedVersion,
    /// The artifact time-scale tag is unsupported.
    UnsupportedTimeScale,
    /// A satellite-system tag is unsupported.
    UnsupportedSatelliteSystem,
    /// A satellite appears more than once in the artifact index.
    DuplicateSatellite,
    /// The artifact file-level checksum did not match.
    Checksum,
    /// A satellite payload checksum did not match its index record.
    SatelliteChecksum,
    /// A caller claim did not match the checksum declared by the header.
    AttestedChecksumMismatch,
    /// A future artifact-store error not yet mapped by this binding.
    Unknown,
}

impl From<&CorePreciseInterpolantStoreError> for PreciseInterpolantArtifactError {
    fn from(value: &CorePreciseInterpolantStoreError) -> Self {
        match value {
            CorePreciseInterpolantStoreError::Io { .. } => Self::Io,
            CorePreciseInterpolantStoreError::Parse { .. } => Self::Parse,
            CorePreciseInterpolantStoreError::BadMagic { .. } => Self::BadMagic,
            CorePreciseInterpolantStoreError::HeaderTruncated { .. } => Self::HeaderTruncated,
            CorePreciseInterpolantStoreError::Truncated { .. } => Self::Truncated,
            CorePreciseInterpolantStoreError::TrailingBytes { .. } => Self::TrailingBytes,
            CorePreciseInterpolantStoreError::RangeOutOfBounds { .. } => Self::RangeOutOfBounds,
            CorePreciseInterpolantStoreError::UnsupportedVersion { .. } => Self::UnsupportedVersion,
            CorePreciseInterpolantStoreError::UnsupportedTimeScale { .. } => {
                Self::UnsupportedTimeScale
            }
            CorePreciseInterpolantStoreError::UnsupportedSatelliteSystem { .. } => {
                Self::UnsupportedSatelliteSystem
            }
            CorePreciseInterpolantStoreError::DuplicateSatellite { .. } => Self::DuplicateSatellite,
            CorePreciseInterpolantStoreError::Checksum { .. } => Self::Checksum,
            CorePreciseInterpolantStoreError::SatelliteChecksum { .. } => Self::SatelliteChecksum,
            CorePreciseInterpolantStoreError::AttestedChecksumMismatch { .. } => {
                Self::AttestedChecksumMismatch
            }
            _ => Self::Unknown,
        }
    }
}

/// Stable string label for a [`PreciseInterpolantArtifactError`] enum value.
#[wasm_bindgen(js_name = preciseInterpolantArtifactErrorLabel)]
pub fn precise_interpolant_artifact_error_label(error: PreciseInterpolantArtifactError) -> String {
    precise_artifact_error_name(error).to_string()
}

/// Accepted boundary convention for a validated exact SP3 product.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExactSp3Coverage {
    /// The declared boundary is excluded (288 five-minute epochs in one day).
    HalfOpen,
    /// The declared boundary epoch is present (289 five-minute epochs).
    Inclusive,
}

impl From<CoreExactSp3Coverage> for ExactSp3Coverage {
    fn from(value: CoreExactSp3Coverage) -> Self {
        match value {
            CoreExactSp3Coverage::HalfOpen => Self::HalfOpen,
            CoreExactSp3Coverage::Inclusive => Self::Inclusive,
        }
    }
}

/// Source-independent exact SP3 content request.
#[wasm_bindgen]
pub struct ExactSp3Request {
    inner: CoreExactSp3Request,
}

#[wasm_bindgen]
impl ExactSp3Request {
    /// Construct an exact request from explicit catalog identity fields.
    #[wasm_bindgen(constructor)]
    pub fn new(
        year: i32,
        month: u8,
        day: u8,
        span: &str,
        sample: &str,
        issue: Option<String>,
    ) -> Result<ExactSp3Request, JsValue> {
        let date = ProductDate::new(year, month, day).map_err(engine_error)?;
        let inner =
            CoreExactSp3Request::new(date, issue.as_deref(), span, sample).map_err(engine_error)?;
        Ok(Self { inner })
    }

    /// Construct an exact request from a complete catalog product identity.
    #[wasm_bindgen(js_name = fromIdentity)]
    pub fn from_identity(identity: &GnssProductIdentity) -> Result<ExactSp3Request, JsValue> {
        CoreExactSp3Request::from_identity(&identity.inner)
            .map(|inner| Self { inner })
            .map_err(engine_error)
    }

    /// Require a particular SP3 line-1 producing-agency code.
    #[wasm_bindgen(js_name = requireAgency)]
    pub fn require_agency(&mut self, agency: &str) -> Result<(), JsValue> {
        self.inner = self
            .inner
            .clone()
            .with_expected_agency(agency)
            .map_err(engine_error)?;
        Ok(())
    }

    #[wasm_bindgen(getter)]
    pub fn year(&self) -> i32 {
        self.inner.date().year
    }

    #[wasm_bindgen(getter)]
    pub fn month(&self) -> u8 {
        self.inner.date().month
    }

    #[wasm_bindgen(getter)]
    pub fn day(&self) -> u8 {
        self.inner.date().day
    }

    #[wasm_bindgen(getter)]
    pub fn issue(&self) -> Option<String> {
        self.inner.issue().map(str::to_owned)
    }

    #[wasm_bindgen(getter)]
    pub fn span(&self) -> String {
        self.inner.span().to_owned()
    }

    #[wasm_bindgen(getter)]
    pub fn sample(&self) -> String {
        self.inner.sample().to_owned()
    }

    #[wasm_bindgen(getter, js_name = formatVersion)]
    pub fn format_version(&self) -> Option<String> {
        self.inner.format_version().map(str::to_owned)
    }

    #[wasm_bindgen(getter, js_name = expectedAgency)]
    pub fn expected_agency(&self) -> Option<String> {
        self.inner.expected_agency().map(str::to_owned)
    }
}

/// Result of parsing and validating exact SP3 bytes in one core call.
#[wasm_bindgen]
pub struct ExactSp3ParseResult {
    product: CoreSp3,
    coverage: ExactSp3Coverage,
}

#[wasm_bindgen]
impl ExactSp3ParseResult {
    #[wasm_bindgen(getter)]
    pub fn coverage(&self) -> ExactSp3Coverage {
        self.coverage
    }

    /// Return the validated parsed product.
    #[wasm_bindgen(getter)]
    pub fn product(&self) -> Sp3 {
        Sp3 {
            inner: self.product.clone(),
        }
    }
}

/// Parse and exact-validate SP3 bytes, returning both product and coverage.
/// A declared-start mismatch throws `ExactSp3ValidationError`; `error.detail`
/// carries `requestedTick` and `declaredTick` as decimal strings so 10 ns tick
/// evidence remains exact in JavaScript.
#[wasm_bindgen(js_name = parseExactSp3)]
pub fn parse_exact_sp3(
    bytes: &[u8],
    request: &ExactSp3Request,
) -> Result<ExactSp3ParseResult, JsValue> {
    let (product, coverage) =
        core_parse_exact_sp3(bytes, &request.inner).map_err(exact_sp3_validation_error_js)?;
    Ok(ExactSp3ParseResult {
        product,
        coverage: coverage.into(),
    })
}

/// Validate an already parsed SP3 product against an exact request. A
/// declared-start mismatch carries the same exact decimal tick strings as
/// [`parse_exact_sp3`].
#[wasm_bindgen(js_name = validateExactSp3)]
pub fn validate_exact_sp3(
    product: &Sp3,
    request: &ExactSp3Request,
) -> Result<ExactSp3Coverage, JsValue> {
    core_validate_exact_sp3(&product.inner, &request.inner)
        .map(Into::into)
        .map_err(exact_sp3_validation_error_js)
}

/// A parsed SP3-c or SP3-d precise-ephemeris product.
///
/// Create with [`load_sp3`]. Query interpolated states with
/// [`Sp3.interpolate`], the exact parsed records with [`Sp3.state`], the node
/// epoch axis with [`Sp3.epochsJ2000Seconds`], run positioning with
/// [`Sp3.solveSpp`], and serialize back with [`Sp3.toSp3String`].
#[wasm_bindgen]
pub struct Sp3 {
    pub(crate) inner: CoreSp3,
}

#[wasm_bindgen]
impl Sp3 {
    /// Number of epochs in the product.
    #[wasm_bindgen(getter, js_name = epochCount)]
    pub fn epoch_count(&self) -> usize {
        self.inner.epoch_count()
    }

    /// Epoch count declared by SP3 header line 1, independent of the parsed
    /// body count used by [`Sp3::epochCount`].
    #[wasm_bindgen(getter, js_name = declaredEpochCount)]
    pub fn declared_epoch_count(&self) -> usize {
        usize::try_from(self.inner.declared_epoch_count()).unwrap_or(usize::MAX)
    }

    /// Start epoch declared by SP3 header line 1, in product-scale seconds
    /// since J2000. Missing or malformed legacy header fields return undefined.
    #[wasm_bindgen(getter, js_name = declaredStartJ2000Seconds)]
    pub fn declared_start_j2000_seconds(&self) -> Option<f64> {
        self.inner.declared_start_j2000_s()
    }

    /// Per-epoch observed/predicted flags and the contiguous observed-through
    /// boundary, derived from the parsed SP3 record flags.
    #[wasm_bindgen(js_name = predictionSummary)]
    pub fn prediction_summary(&self) -> Result<JsValue, JsValue> {
        let summary = self.inner.prediction_summary();
        let value = Sp3PredictionSummaryJs {
            epochs: summary
                .epochs
                .into_iter()
                .map(|epoch| Sp3EpochPredictionJs {
                    epoch_j2000_seconds: instant_to_j2000_seconds(&epoch.epoch),
                    observed: epoch.is_observed(),
                    orbit_predicted_satellites: epoch
                        .orbit_predicted_satellites
                        .into_iter()
                        .map(|satellite| satellite.to_string())
                        .collect(),
                    clock_predicted_satellites: epoch
                        .clock_predicted_satellites
                        .into_iter()
                        .map(|satellite| satellite.to_string())
                        .collect(),
                })
                .collect(),
            observed_through_j2000_seconds: summary
                .observed_through
                .as_ref()
                .map(instant_to_j2000_seconds),
        };
        serde_wasm_bindgen::to_value(&value).map_err(|error| engine_error(error.to_string()))
    }

    /// Satellite tokens present in the product (e.g. `"G01"`), ascending.
    #[wasm_bindgen(getter)]
    pub fn satellites(&self) -> Vec<String> {
        self.inner
            .satellites()
            .iter()
            .map(|sat| sat.to_string())
            .collect()
    }

    /// SP3 interpolation gap threshold factor carried by this product.
    #[wasm_bindgen(getter, js_name = gapThresholdFactor)]
    pub fn gap_threshold_factor(&self) -> f64 {
        self.inner.interpolation_options().gap_threshold_factor()
    }

    /// Return a copy of this product with an explicit gap threshold factor.
    #[wasm_bindgen(js_name = withInterpolationOptions)]
    pub fn with_interpolation_options(&self, gap_threshold_factor: f64) -> Result<Sp3, JsValue> {
        let options = Sp3InterpolationOptions::new(gap_threshold_factor).map_err(engine_error)?;
        Ok(Sp3 {
            inner: self.inner.clone().with_interpolation_options(options),
        })
    }

    /// The product's parsed epochs as seconds since J2000 (the product's own
    /// time scale), ascending. This is the exact axis [`Sp3.interpolate`]
    /// consumes.
    #[wasm_bindgen(js_name = epochsJ2000Seconds)]
    pub fn epochs_j2000_seconds(&self) -> Vec<f64> {
        self.inner.epochs_j2000_seconds()
    }

    /// Time reach of the SP3 position interpolator before and after a query.
    ///
    /// The core derives both values from this product's declared epoch interval
    /// and interpolation-node count. Callers never supply a stencil duration.
    #[wasm_bindgen(js_name = stencilExtent)]
    pub fn stencil_extent(&self) -> Result<JsValue, JsValue> {
        let stencil = StencilExtent::for_sp3(&self.inner).map_err(engine_error)?;
        serde_wasm_bindgen::to_value(&StencilExtentJs {
            before_s: stencil.before_s(),
            after_s: stencil.after_s(),
        })
        .map_err(|error| engine_error(error.to_string()))
    }

    /// Epochs of the position nodes that some interpolation of `satellite` in
    /// the inclusive window selects, ascending, seconds since J2000; empty when
    /// no query in the window is served for it.
    ///
    /// The engine applies the position interpolator's own serving and
    /// node-selection rule to the satellite's node series, under this
    /// product's interpolation options. Merge continuity verdicts read the
    /// merged product's nodes this way.
    #[wasm_bindgen(js_name = selectedNodes)]
    pub fn selected_nodes(
        &self,
        satellite: &str,
        from_j2000_s: f64,
        through_j2000_s: f64,
    ) -> Result<Vec<f64>, JsValue> {
        let sat = parse_sat(satellite)?;
        let window = EpochWindow::new(from_j2000_s, through_j2000_s).map_err(engine_error)?;
        Ok(InterpolationNodes::for_sp3(&self.inner).selected_nodes(sat, window))
    }

    /// Attest that this product is physically continuous, or report each
    /// violation.
    ///
    /// A merged product is assembled per satellite and epoch from several
    /// analysis centers, which is exactly the operation that can splice two
    /// physically inconsistent arcs together while every input stays
    /// individually well-formed. Two checks run, with different jobs: a
    /// physical earth-fixed speed gate whose bound is a true upper bound for
    /// the orbit class, so it cannot false-positive and catches gross
    /// corruption; and a hold-out interpolation residual, which supplies the
    /// sensitivity a speed gate structurally cannot - adjacent GNSS MEO epochs
    /// are hundreds of kilometres apart, so a metre-scale splice moves the
    /// implied speed by a fraction of a percent.
    ///
    /// `orbitClass` is `"meo_gnss"` (default), `"geosynchronous"`, `"leo"`, or
    /// `null` to disable the speed gate. `residualToleranceM` enables the
    /// residual check; `null` disables it. `gapThresholdFactor` configures the
    /// hold-out interpolation policy; `null` leaves the core default (1.5).
    ///
    /// Returns `{ attested, defects, pairsChecked, residualsChecked,
    /// residualsSkipped }`. Reports rather than refuses: whether a product with
    /// defects is acceptable is the caller's decision.
    /// An unusable sample is retained as an `unusable_sample` defect with its
    /// original `sampleIndex`, optional `epochJ2000S`, and reason. Invalid
    /// numeric bounds throw `ContinuityOptionsError` with a `detail` object
    /// naming the field, its decimal-string value, and rejection reason.
    #[wasm_bindgen(js_name = checkContinuity)]
    pub fn check_continuity(
        &self,
        orbit_class: Option<String>,
        residual_tolerance_m: Option<f64>,
        gap_threshold_factor: Option<f64>,
    ) -> Result<JsValue, JsValue> {
        let report = check_continuity(
            &self.inner.precise_ephemeris_samples(),
            &continuity_options(
                orbit_class.as_deref(),
                residual_tolerance_m,
                gap_threshold_factor,
            )?,
        )
        .map_err(continuity_options_error_js)?;

        let defects: Vec<ContinuityDefectJs> = report
            .defects
            .iter()
            .map(ContinuityDefectJs::from)
            .collect();

        let out = ContinuityReportJs {
            attested: report.attested(),
            defects,
            pairs_checked: report.pairs_checked,
            residuals_checked: report.residuals_checked,
            residuals_skipped: report.residuals_skipped,
        };
        serde_wasm_bindgen::to_value(&out).map_err(|error| JsValue::from_str(&error.to_string()))
    }

    /// Decide whether product-wide continuity findings can influence an
    /// inclusive evaluation window through this product's interpolation
    /// stencil.
    ///
    /// Omitted options use the existing defaults (`"meo_gnss"` and 1 metre).
    /// Passing `null` disables the corresponding check. The returned object
    /// retains both the influencing findings and the complete report.
    #[wasm_bindgen(js_name = continuityVerdict)]
    pub fn continuity_verdict(
        &self,
        from_j2000_s: f64,
        through_j2000_s: f64,
        orbit_class: JsValue,
        residual_tolerance_m: JsValue,
        gap_threshold_factor: JsValue,
    ) -> Result<JsValue, JsValue> {
        let window = EpochWindow::new(from_j2000_s, through_j2000_s).map_err(engine_error)?;
        let stencil = StencilExtent::for_sp3(&self.inner).map_err(engine_error)?;
        let report = check_continuity(
            &self.inner.precise_ephemeris_samples(),
            &continuity_verdict_options(orbit_class, residual_tolerance_m, gap_threshold_factor)?,
        )
        .map_err(continuity_options_error_js)?;
        continuity_verdict_to_js(report.verdict_for_window(window, stencil))
    }

    /// Interpolate `satellite`'s position and clock at each query epoch.
    ///
    /// `j2000Seconds` is a `Float64Array` of query times in seconds since J2000,
    /// in the product's own time scale. Throws a `TypeError` if the satellite is
    /// absent or the query array is empty, and an `Error` if a query lies in a
    /// coverage gap (the engine refuses to extrapolate).
    #[wasm_bindgen]
    pub fn interpolate(
        &self,
        satellite: &str,
        j2000_seconds: &[f64],
    ) -> Result<Sp3Interpolation, JsValue> {
        let sat = parse_sat(satellite)?;
        if j2000_seconds.is_empty() {
            return Err(type_error("j2000Seconds array is empty"));
        }

        let mut positions = Vec::with_capacity(j2000_seconds.len() * 3);
        let mut clocks = Vec::with_capacity(j2000_seconds.len());
        for &q in j2000_seconds {
            let state = self
                .inner
                .position_at_j2000_seconds(sat, q)
                .map_err(|e| match e {
                    CoreError::UnknownSatellite(id) => {
                        type_error(&format!("satellite {id} is not in the product"))
                    }
                    other => engine_error(format!("interpolation at j2000 second {q}: {other}")),
                })?;
            let p = state.position.as_array();
            positions.extend_from_slice(&p);
            clocks.push(state.clock_s.unwrap_or(f64::NAN));
        }
        Ok(Sp3Interpolation { positions, clocks })
    }

    /// The exact parsed state of `satellite` at record `epochIndex` (no
    /// interpolation). Throws a `RangeError` past the last epoch and a
    /// `TypeError` if the satellite has no record there.
    #[wasm_bindgen]
    pub fn state(&self, satellite: &str, epoch_index: usize) -> Result<Sp3State, JsValue> {
        let sat = parse_sat(satellite)?;
        let state = self.inner.state(sat, epoch_index).map_err(|e| match e {
            CoreError::EpochOutOfRange => {
                crate::error::range_error(&format!("epoch index {epoch_index} out of range"))
            }
            CoreError::UnknownSatellite(id) => type_error(&format!(
                "satellite {id} has no record at epoch {epoch_index}"
            )),
            other => engine_error(other),
        })?;
        Ok(sp3_state_from_core(state))
    }

    #[wasm_bindgen(js_name = stateAtExactQuery)]
    pub fn state_at_exact_query(
        &self,
        satellite: &str,
        query: &ExactEpochQueryValue,
    ) -> Result<Sp3State, JsValue> {
        let satellite = parse_sat(satellite)?;
        self.inner
            .position_at_epoch_query(satellite, &query.core())
            .map(sp3_state_from_core)
            .map_err(engine_error)
    }

    #[wasm_bindgen(js_name = ephemerisVarianceAtExactQuery)]
    pub fn ephemeris_variance_at_exact_query(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        selection_epoch: &ExactEpochQueryValue,
    ) -> Result<f64, JsValue> {
        let satellite = parse_sat(satellite)?;
        Ok(precise_variance_at_queries(
            &self.inner,
            satellite,
            state_epoch,
            selection_epoch,
        ))
    }

    #[wasm_bindgen(js_name = selectedPositionClockAtExactQueries, unchecked_return_type = "Ut1Validated<SelectedPositionClock> | null")]
    pub fn selected_position_clock_at_exact_queries(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        selection_epoch: &ExactEpochQueryValue,
    ) -> Result<JsValue, JsValue> {
        let satellite = parse_sat(satellite)?;
        selected_position_clock_at_queries(&self.inner, satellite, state_epoch, selection_epoch)
    }

    #[wasm_bindgen(js_name = transmitEpochClockAtExactQueries, unchecked_return_type = "Ut1Validated<number> | null")]
    pub fn transmit_epoch_clock_at_exact_queries(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        selection_epoch: &ExactEpochQueryValue,
    ) -> Result<JsValue, JsValue> {
        let satellite = parse_sat(satellite)?;
        transmit_epoch_clock_at_queries(&self.inner, satellite, state_epoch, selection_epoch)
    }

    #[wasm_bindgen(
        js_name = clockRelativityAtExactQuery,
        unchecked_return_type = "ClockRelativity"
    )]
    pub fn clock_relativity_at_exact_query(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        position_ecef_m: Vec<f64>,
    ) -> Result<JsValue, JsValue> {
        let satellite = parse_sat(satellite)?;
        let position_ecef_m: [f64; 3] = position_ecef_m
            .try_into()
            .map_err(|_| type_error("positionEcefM must contain exactly three coordinates"))?;
        precise_clock_relativity_at_query(&self.inner, satellite, state_epoch, position_ecef_m)
    }

    #[wasm_bindgen(js_name = recordAccuracyCodes, unchecked_return_type = "Sp3RawRecordAccuracy")]
    pub fn record_accuracy_codes(
        &self,
        satellite: &str,
        epoch_index: usize,
    ) -> Result<JsValue, JsValue> {
        let satellite = parse_sat(satellite)?;
        let accuracy = self
            .inner
            .record_accuracy_codes(satellite, epoch_index)
            .map_err(|error| match error {
                CoreError::EpochOutOfRange => {
                    range_error(&format!("epoch index {epoch_index} out of range"))
                }
                CoreError::UnknownSatellite(id) => type_error(&format!(
                    "satellite {id} has no record at epoch {epoch_index}"
                )),
                other => engine_error(other),
            })?;
        serde_wasm_bindgen::to_value(&Sp3RawRecordAccuracyJs::from(accuracy))
            .map_err(|error| engine_error(error.to_string()))
    }

    #[wasm_bindgen(js_name = recordAccuracy, unchecked_return_type = "Sp3RecordAccuracy")]
    pub fn record_accuracy(&self, satellite: &str, epoch_index: usize) -> Result<JsValue, JsValue> {
        let satellite = parse_sat(satellite)?;
        let accuracy = self
            .inner
            .record_accuracy(satellite, epoch_index)
            .map_err(|error| match error {
                CoreError::EpochOutOfRange => {
                    range_error(&format!("epoch index {epoch_index} out of range"))
                }
                CoreError::UnknownSatellite(id) => type_error(&format!(
                    "satellite {id} has no record at epoch {epoch_index}"
                )),
                other => engine_error(other),
            })?;
        serde_wasm_bindgen::to_value(&Sp3RecordAccuracyJs::from(accuracy))
            .map_err(|error| engine_error(error.to_string()))
    }

    /// Run single-point positioning against this ephemeris.
    ///
    /// `request` is a plain object; see the `SppRequest` TypeScript type. Throws
    /// a `TypeError` for malformed input and an `Error` if the solve fails.
    #[wasm_bindgen(js_name = solveSpp)]
    pub fn solve_spp(&self, request: JsValue) -> Result<SppSolution, JsValue> {
        spp::solve(&self.inner, request)
    }

    /// Run SPP and attach a Doppler velocity/clock-drift solve when Doppler rows solve.
    ///
    /// `request` is the normal SPP request object. `dopplerObservations` is an
    /// array of `{ satelliteId, dopplerHz, carrierHz, satClockDriftSS? }`. The
    /// returned receiver solution carries `rxClockDriftSS` when velocity solved.
    #[wasm_bindgen(js_name = solveSppWithDopplerVelocity)]
    pub fn solve_spp_with_doppler_velocity(
        &self,
        request: JsValue,
        doppler_observations: JsValue,
    ) -> Result<crate::spp::SppDopplerSolution, JsValue> {
        spp::solve_with_doppler_velocity(&self.inner, request, doppler_observations)
    }

    /// Solve a batch of independent SPP epochs against this ephemeris in one call.
    ///
    /// `epochs` is an array of SPP request objects (the `SppRequest` shape) and
    /// `options` the shared `{ withGeodetic?, maxPdop?, coarseSearchSeeds? }`
    /// applied to every epoch. Returns an `SppBatchSolution` whose per-epoch
    /// results are index-aligned to `epochs`; each epoch independently converged
    /// or failed. Delegates to the serial reference batch kernel.
    #[wasm_bindgen(js_name = solveSppBatch)]
    pub fn solve_spp_batch(
        &self,
        epochs: JsValue,
        options: JsValue,
    ) -> Result<crate::spp::SppBatchSolution, JsValue> {
        spp::solve_batch(&self.inner, epochs, options)
    }

    /// Solve one static receiver position from multiple SPP-shaped epochs.
    ///
    /// `epochs` is an array of SPP request objects. `options` accepts
    /// `{ initialPositionM?, withGeodetic?, robust?, qzssClock?,
    /// troposphereModel? }` and returns shared
    /// position, per-epoch clocks, covariance, residual, and influence surfaces.
    #[wasm_bindgen(js_name = solveStatic)]
    pub fn solve_static(
        &self,
        epochs: JsValue,
        options: JsValue,
    ) -> Result<crate::static_positioning::StaticSolution, JsValue> {
        crate::static_positioning::solve_static_sp3(&self.inner, epochs, options)
    }

    /// Compute DGNSS pseudorange corrections from a surveyed base station.
    ///
    /// `request` is `{ basePositionM, baseObservations, tRxJ2000S }`; returns a
    /// `{ satelliteId, correctionM }[]` array sorted by satellite token.
    #[wasm_bindgen(js_name = dgnssCorrections)]
    pub fn dgnss_corrections(&self, request: JsValue) -> Result<JsValue, JsValue> {
        crate::dgnss::corrections(&self.inner, request)
    }

    /// Solve a DGNSS rover position: compute base corrections, apply them to the
    /// rover, and run the corrected SPP. `request` carries the base + rover
    /// observations and the receive-time scalars; see the `DgnssSolveRequest`
    /// TypeScript type. Returns the corrected solution and the base baseline.
    #[wasm_bindgen(js_name = dgnssSolve)]
    pub fn dgnss_solve(&self, request: JsValue) -> Result<crate::dgnss::DgnssSolution, JsValue> {
        crate::dgnss::solve(&self.inner, request)
    }

    /// Run fault detection and exclusion (FDE) against this ephemeris.
    ///
    /// `request` is the SPP solve request plus RAIM/exclusion options. Omitted
    /// weights use the solve's pseudorange variances; unit and per-satellite
    /// modes are also available. The default exclusion budget is one and the
    /// candidate residual RMS cap is 100 m. The core loop returns the surviving
    /// solution, exclusions, and its detection result; unresolved faults throw a
    /// `PositioningError` with the last solution, exclusions, reason, and RAIM
    /// result.
    #[wasm_bindgen(js_name = fde)]
    pub fn fde(&self, request: JsValue) -> Result<crate::qc::FdeSolution, JsValue> {
        crate::qc::fde(&self.inner, request)
    }

    /// Run the core robust-reweighted SPP driver under the RAIM/FDE exclusion loop.
    ///
    /// `request` is the FDE request with a `robust` object. The implementation
    /// delegates to `sidereon_core::quality::spp_robust_fde_driver`.
    #[wasm_bindgen(js_name = sppRobustFdeDriver)]
    pub fn spp_robust_fde_driver(
        &self,
        request: JsValue,
    ) -> Result<crate::qc::FdeSolution, JsValue> {
        crate::qc::fde(&self.inner, request)
    }

    /// Estimate `other`'s per-epoch common clock offset relative to this product.
    ///
    /// The result is one row per matched epoch with enough common clocked
    /// satellites. Subtract `offsetS` from `other` clocks to put them on this
    /// product's clock datum. Delegates to
    /// `sidereon_core::ephemeris::clock_reference_offset`.
    #[wasm_bindgen(js_name = clockReferenceOffset)]
    pub fn clock_reference_offset(
        &self,
        other: &Sp3,
        min_common: Option<usize>,
    ) -> Result<Vec<Sp3ClockReferenceOffset>, JsValue> {
        let min_common = min_common.unwrap_or(5);
        if min_common == 0 {
            return Err(range_error("minCommon must be at least 1"));
        }
        Ok(
            core_clock_reference_offset(&self.inner, &other.inner, min_common)
                .into_iter()
                .map(Into::into)
                .collect(),
        )
    }

    /// Return a copy of `other` with its clocks shifted onto this product's
    /// clock datum. Epochs without an offset estimate are left unchanged.
    /// Delegates to `sidereon_core::ephemeris::align_clock_reference`.
    #[wasm_bindgen(js_name = alignClockReference)]
    pub fn align_clock_reference(
        &self,
        other: &Sp3,
        min_common: Option<usize>,
    ) -> Result<Sp3, JsValue> {
        let min_common = min_common.unwrap_or(5);
        if min_common == 0 {
            return Err(range_error("minCommon must be at least 1"));
        }
        Ok(Sp3 {
            inner: core_align_clock_reference(&self.inner, &other.inner, min_common),
        })
    }

    /// Serialize to standard SP3 text (the version named by the header, `c` or
    /// `d`). Deterministic: the same product always produces byte-identical text.
    ///
    /// Every field is written only when reading its columns back gives the
    /// value the product holds. Otherwise this throws an `Sp3WriteError` whose
    /// `detail` names the field, the value and, for a record, the satellite and
    /// epoch, as an `Sp3WriteErrorDetail`; nothing is rounded, shifted or
    /// dropped to make the product fit.
    #[wasm_bindgen(js_name = toSp3String)]
    pub fn to_sp3_string(&self) -> Result<String, JsValue> {
        self.inner.to_sp3_string().map_err(sp3_write_error)
    }

    /// Build deterministic precise-interpolant artifact bytes from this SP3 product.
    ///
    /// `gapThresholdFactor` optionally overrides the product's interpolation policy;
    /// `null` or omitted retains this product's active policy.
    #[wasm_bindgen(js_name = preciseInterpolantArtifactBytes)]
    pub fn precise_interpolant_artifact_bytes(
        &self,
        gap_threshold_factor: Option<f64>,
    ) -> Result<Vec<u8>, JsValue> {
        let mut product = self.inner.clone();
        if let Some(factor) = gap_threshold_factor {
            let options = Sp3InterpolationOptions::new(factor).map_err(engine_error)?;
            product = product.with_interpolation_options(options);
        }
        product
            .precise_interpolant_store_bytes()
            .map_err(precise_artifact_error)
    }

    /// Predict geometric ranges for many `(satellite, receiver, epoch)` requests
    /// against this ephemeris in one call. `requests` is an array of
    /// `{ sat, receiverEcefM, tRxJ2000S }`; returns an array of
    /// `{ geometricRangeM, satClockS, transmitTimeJ2000S, satPosEcefM }`
    /// index-aligned to `requests`. The same call shape works on a
    /// `PreciseEphemerisSampleSource`. Delegates to the serial reference kernel
    /// `sidereon_core::observables::predict_ranges`.
    #[wasm_bindgen(js_name = predictRanges)]
    pub fn predict_ranges(&self, requests: JsValue, options: JsValue) -> Result<JsValue, JsValue> {
        crate::precise_samples::predict_ranges_over(&self.inner, requests, options)
    }

    /// Evaluate emission-time state and media corrections for index-aligned satellites.
    ///
    /// `satellites` and `emissionEpochsJ2000S` share a row count. `receiverEcefM`
    /// is `[x, y, z]` metres. Without an IONEX product this can still request
    /// troposphere corrections by passing `{ troposphere: true }`.
    #[wasm_bindgen(js_name = emissionMediaBatch)]
    pub fn emission_media_batch(
        &self,
        satellites: Vec<String>,
        emission_epochs_j2000_s: &[f64],
        receiver_ecef_m: &[f64],
        options: JsValue,
    ) -> Result<crate::emission_media::EmissionMediaBatch, JsValue> {
        crate::emission_media::emission_media_batch_sp3(
            &self.inner,
            satellites,
            emission_epochs_j2000_s,
            receiver_ecef_m,
            options,
        )
    }

    /// Evaluate emission-time state plus IONEX/troposphere media corrections.
    ///
    /// `options.ionosphere` defaults to `true` on this IONEX-bearing path.
    /// `options.troposphere` defaults to `false`.
    #[wasm_bindgen(js_name = emissionMediaBatchIonex)]
    pub fn emission_media_batch_ionex(
        &self,
        ionex: &crate::ionex::Ionex,
        satellites: Vec<String>,
        emission_epochs_j2000_s: &[f64],
        receiver_ecef_m: &[f64],
        options: JsValue,
    ) -> Result<crate::emission_media::EmissionMediaBatch, JsValue> {
        crate::emission_media::emission_media_batch_sp3_ionex(
            &self.inner,
            ionex,
            satellites,
            emission_epochs_j2000_s,
            receiver_ecef_m,
            options,
        )
    }
}

/// Why an SP3 product could not be written, as the `detail` of the thrown
/// `Sp3WriteError`: a discriminated union on `kind`, each variant carrying only
/// its own payload and the engine's message.
///
/// An engine `u64` or `i64` crosses as an exact decimal string beside a
/// `number` that is `null` where the integer is not exactly representable as
/// one. A satellite is its RINEX token (`"G01"`), a time scale its short
/// identifier (`"GPST"`), and an SP3 time system its three-character label.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind")]
enum Sp3WriteErrorDetailJs {
    #[serde(rename = "ACCURACY_NOT_REPRESENTABLE", rename_all = "camelCase")]
    AccuracyNotRepresentable {
        satellite: String,
        epoch_index: usize,
        component: String,
        exponent: Option<i16>,
        message: String,
    },
    #[serde(rename = "ACCURACY_RECORD_MISMATCH", rename_all = "camelCase")]
    AccuracyRecordMismatch {
        satellite: String,
        epoch_index: usize,
        message: String,
    },
    #[serde(rename = "ACCURACY_BASIS_MISSING", rename_all = "camelCase")]
    AccuracyBasisMissing {
        satellite: String,
        epoch_index: usize,
        message: String,
    },
    #[serde(rename = "TEXT_NOT_COLUMN_SAFE", rename_all = "camelCase")]
    TextNotColumnSafe {
        field: String,
        value: String,
        message: String,
    },
    #[serde(rename = "TEXT_NOT_COLUMN_STABLE", rename_all = "camelCase")]
    TextNotColumnStable {
        field: String,
        value: String,
        message: String,
    },
    #[serde(rename = "BLANK_DESCRIPTOR", rename_all = "camelCase")]
    BlankDescriptor {
        field: String,
        value: String,
        message: String,
    },
    #[serde(rename = "EMPTY_COMMENT", rename_all = "camelCase")]
    EmptyComment {
        index: usize,
        value: String,
        message: String,
    },
    #[serde(rename = "TEXT_TOO_WIDE", rename_all = "camelCase")]
    TextTooWide {
        field: String,
        columns: usize,
        value: String,
        message: String,
    },
    #[serde(rename = "INTEGER_TOO_WIDE", rename_all = "camelCase")]
    IntegerTooWide {
        field: String,
        columns: usize,
        value: String,
        value_number: Option<f64>,
        message: String,
    },
    #[serde(rename = "NON_FINITE", rename_all = "camelCase")]
    NonFinite { field: String, message: String },
    #[serde(rename = "NUMBER_TOO_WIDE", rename_all = "camelCase")]
    NumberTooWide {
        field: String,
        columns: usize,
        decimals: usize,
        value: f64,
        message: String,
    },
    #[serde(rename = "PRECISION_NOT_REPRESENTABLE", rename_all = "camelCase")]
    PrecisionNotRepresentable {
        field: String,
        columns: usize,
        decimals: usize,
        value: f64,
        message: String,
    },
    #[serde(rename = "YEAR_NOT_REPRESENTABLE", rename_all = "camelCase")]
    YearNotRepresentable {
        epoch_index: usize,
        year: String,
        year_number: Option<f64>,
        message: String,
    },
    #[serde(rename = "EPOCH_NOT_RESTATABLE", rename_all = "camelCase")]
    EpochNotRestatable {
        epoch_index: usize,
        field_seconds: f64,
        /// `null` where no candidate record could be read back at all, which
        /// the engine marks with NaN.
        residual_s: Option<f64>,
        message: String,
    },
    #[serde(rename = "EPOCH_TIME_SCALE_MISMATCH", rename_all = "camelCase")]
    EpochTimeScaleMismatch {
        epoch_index: usize,
        epoch_scale: String,
        header_scale: String,
        message: String,
    },
    #[serde(rename = "HEADER_TIME_SCALE_MISMATCH", rename_all = "camelCase")]
    HeaderTimeScaleMismatch {
        time_system: String,
        time_scale: String,
        message: String,
    },
    #[serde(rename = "EPOCH_COUNT_MISMATCH", rename_all = "camelCase")]
    EpochCountMismatch {
        declared: String,
        declared_number: Option<f64>,
        epochs: usize,
        message: String,
    },
    #[serde(rename = "ACCURACY_CODE_COUNT_MISMATCH", rename_all = "camelCase")]
    AccuracyCodeCountMismatch {
        satellites: usize,
        codes: usize,
        message: String,
    },
    #[serde(rename = "DUPLICATE_SATELLITE", rename_all = "camelCase")]
    DuplicateSatellite { satellite: String, message: String },
    /// A header satellite with no `01`..`99` token that reads back as itself.
    /// The satellite crosses as its system letter and number, since it has no
    /// token.
    #[serde(rename = "SATELLITE_NOT_REPRESENTABLE", rename_all = "camelCase")]
    SatelliteNotRepresentable {
        system: String,
        prn: u8,
        message: String,
    },
    #[serde(rename = "EPOCH_ARRAY_LENGTH_MISMATCH", rename_all = "camelCase")]
    EpochArrayLengthMismatch {
        field: String,
        epochs: usize,
        entries: usize,
        message: String,
    },
    #[serde(rename = "UNDECLARED_SATELLITE_RECORD", rename_all = "camelCase")]
    UndeclaredSatelliteRecord {
        satellite: String,
        epoch_index: usize,
        message: String,
    },
    #[serde(rename = "CONFLICTING_RECORDS", rename_all = "camelCase")]
    ConflictingRecords {
        satellite: String,
        epoch_index: usize,
        message: String,
    },
    #[serde(
        rename = "VELOCITY_STATE_IN_POSITION_PRODUCT",
        rename_all = "camelCase"
    )]
    VelocityStateInPositionProduct {
        field: String,
        satellite: String,
        epoch_index: usize,
        message: String,
    },
    #[serde(rename = "RECORD_VALUE_NON_FINITE", rename_all = "camelCase")]
    RecordValueNonFinite {
        field: String,
        satellite: String,
        epoch_index: usize,
        message: String,
    },
    #[serde(rename = "RECORD_VALUE_TOO_WIDE", rename_all = "camelCase")]
    RecordValueTooWide {
        field: String,
        satellite: String,
        epoch_index: usize,
        columns: usize,
        decimals: usize,
        column_value: f64,
        message: String,
    },
    #[serde(rename = "RECORD_VALUE_NOT_REPRESENTABLE", rename_all = "camelCase")]
    RecordValueNotRepresentable {
        field: String,
        satellite: String,
        epoch_index: usize,
        columns: usize,
        decimals: usize,
        stored: f64,
        column_value: f64,
        message: String,
    },
    #[serde(rename = "RECORD_READS_AS_ABSENT", rename_all = "camelCase")]
    RecordReadsAsAbsent {
        field: String,
        satellite: String,
        epoch_index: usize,
        column_value: f64,
        message: String,
    },
    #[serde(rename = "RECORD_FIELDS_DISAGREE", rename_all = "camelCase")]
    RecordFieldsDisagree {
        field: String,
        satellite: String,
        epoch_index: usize,
        stored: Option<f64>,
        native: Option<f64>,
        message: String,
    },
    /// A refusal this binding does not yet name, carrying the engine's own
    /// message in full rather than being folded into a known kind.
    #[serde(rename = "UNKNOWN", rename_all = "camelCase")]
    Unknown { message: String },
}

impl Sp3WriteErrorDetailJs {
    fn from_core(err: &CoreSp3WriteError) -> Self {
        let message = err.to_string();
        match err {
            CoreSp3WriteError::AccuracyNotRepresentable {
                sat,
                epoch_index,
                component,
                exponent,
            } => Self::AccuracyNotRepresentable {
                satellite: sat.to_string(),
                epoch_index: *epoch_index,
                component: (*component).to_string(),
                exponent: *exponent,
                message,
            },
            CoreSp3WriteError::AccuracyRecordMismatch { sat, epoch_index } => {
                Self::AccuracyRecordMismatch {
                    satellite: sat.to_string(),
                    epoch_index: *epoch_index,
                    message,
                }
            }
            CoreSp3WriteError::AccuracyBasisMissing { sat, epoch_index } => {
                Self::AccuracyBasisMissing {
                    satellite: sat.to_string(),
                    epoch_index: *epoch_index,
                    message,
                }
            }
            CoreSp3WriteError::TextNotColumnSafe { field, value } => Self::TextNotColumnSafe {
                field: (*field).to_string(),
                value: value.clone(),
                message,
            },
            CoreSp3WriteError::TextNotColumnStable { field, value } => Self::TextNotColumnStable {
                field: (*field).to_string(),
                value: value.clone(),
                message,
            },
            CoreSp3WriteError::BlankDescriptor { field, value } => Self::BlankDescriptor {
                field: (*field).to_string(),
                value: value.clone(),
                message,
            },
            CoreSp3WriteError::EmptyComment { index, value } => Self::EmptyComment {
                index: *index,
                value: value.clone(),
                message,
            },
            CoreSp3WriteError::TextTooWide {
                field,
                columns,
                value,
            } => Self::TextTooWide {
                field: (*field).to_string(),
                columns: *columns,
                value: value.clone(),
                message,
            },
            CoreSp3WriteError::IntegerTooWide {
                field,
                columns,
                value,
            } => Self::IntegerTooWide {
                field: (*field).to_string(),
                columns: *columns,
                value: value.to_string(),
                value_number: safe_integer_number(i128::from(*value)),
                message,
            },
            CoreSp3WriteError::NonFinite { field } => Self::NonFinite {
                field: (*field).to_string(),
                message,
            },
            CoreSp3WriteError::NumberTooWide {
                field,
                columns,
                decimals,
                value,
            } => Self::NumberTooWide {
                field: (*field).to_string(),
                columns: *columns,
                decimals: *decimals,
                value: *value,
                message,
            },
            CoreSp3WriteError::PrecisionNotRepresentable {
                field,
                columns,
                decimals,
                value,
            } => Self::PrecisionNotRepresentable {
                field: (*field).to_string(),
                columns: *columns,
                decimals: *decimals,
                value: *value,
                message,
            },
            CoreSp3WriteError::YearNotRepresentable { epoch_index, year } => {
                Self::YearNotRepresentable {
                    epoch_index: *epoch_index,
                    year: year.to_string(),
                    year_number: safe_integer_number(i128::from(*year)),
                    message,
                }
            }
            CoreSp3WriteError::EpochNotRestatable {
                epoch_index,
                field_seconds,
                residual_s,
            } => Self::EpochNotRestatable {
                epoch_index: *epoch_index,
                field_seconds: *field_seconds,
                residual_s: (!residual_s.is_nan()).then_some(*residual_s),
                message,
            },
            CoreSp3WriteError::EpochTimeScaleMismatch {
                epoch_index,
                epoch_scale,
                header_scale,
            } => Self::EpochTimeScaleMismatch {
                epoch_index: *epoch_index,
                epoch_scale: epoch_scale.abbrev().to_string(),
                header_scale: header_scale.abbrev().to_string(),
                message,
            },
            CoreSp3WriteError::HeaderTimeScaleMismatch {
                time_system,
                time_scale,
            } => Self::HeaderTimeScaleMismatch {
                time_system: time_system.label().to_string(),
                time_scale: time_scale.abbrev().to_string(),
                message,
            },
            CoreSp3WriteError::EpochCountMismatch { declared, epochs } => {
                Self::EpochCountMismatch {
                    declared: declared.to_string(),
                    declared_number: safe_integer_number(i128::from(*declared)),
                    epochs: *epochs,
                    message,
                }
            }
            CoreSp3WriteError::AccuracyCodeCountMismatch { satellites, codes } => {
                Self::AccuracyCodeCountMismatch {
                    satellites: *satellites,
                    codes: *codes,
                    message,
                }
            }
            CoreSp3WriteError::DuplicateSatellite { sat } => Self::DuplicateSatellite {
                satellite: sat.to_string(),
                message,
            },
            CoreSp3WriteError::SatelliteNotRepresentable { sat } => {
                Self::SatelliteNotRepresentable {
                    system: sat.system.letter().to_string(),
                    prn: sat.prn,
                    message,
                }
            }
            CoreSp3WriteError::EpochArrayLengthMismatch {
                field,
                epochs,
                entries,
            } => Self::EpochArrayLengthMismatch {
                field: (*field).to_string(),
                epochs: *epochs,
                entries: *entries,
                message,
            },
            CoreSp3WriteError::UndeclaredSatelliteRecord { sat, epoch_index } => {
                Self::UndeclaredSatelliteRecord {
                    satellite: sat.to_string(),
                    epoch_index: *epoch_index,
                    message,
                }
            }
            CoreSp3WriteError::ConflictingRecords { sat, epoch_index } => {
                Self::ConflictingRecords {
                    satellite: sat.to_string(),
                    epoch_index: *epoch_index,
                    message,
                }
            }
            CoreSp3WriteError::VelocityStateInPositionProduct {
                field,
                sat,
                epoch_index,
            } => Self::VelocityStateInPositionProduct {
                field: (*field).to_string(),
                satellite: sat.to_string(),
                epoch_index: *epoch_index,
                message,
            },
            CoreSp3WriteError::RecordValueNonFinite {
                field,
                sat,
                epoch_index,
            } => Self::RecordValueNonFinite {
                field: (*field).to_string(),
                satellite: sat.to_string(),
                epoch_index: *epoch_index,
                message,
            },
            CoreSp3WriteError::RecordValueTooWide {
                field,
                sat,
                epoch_index,
                columns,
                decimals,
                column_value,
            } => Self::RecordValueTooWide {
                field: (*field).to_string(),
                satellite: sat.to_string(),
                epoch_index: *epoch_index,
                columns: *columns,
                decimals: *decimals,
                column_value: *column_value,
                message,
            },
            CoreSp3WriteError::RecordValueNotRepresentable {
                field,
                sat,
                epoch_index,
                columns,
                decimals,
                stored,
                column_value,
            } => Self::RecordValueNotRepresentable {
                field: (*field).to_string(),
                satellite: sat.to_string(),
                epoch_index: *epoch_index,
                columns: *columns,
                decimals: *decimals,
                stored: *stored,
                column_value: *column_value,
                message,
            },
            CoreSp3WriteError::RecordReadsAsAbsent {
                field,
                sat,
                epoch_index,
                column_value,
            } => Self::RecordReadsAsAbsent {
                field: (*field).to_string(),
                satellite: sat.to_string(),
                epoch_index: *epoch_index,
                column_value: *column_value,
                message,
            },
            CoreSp3WriteError::RecordFieldsDisagree {
                field,
                sat,
                epoch_index,
                stored,
                native,
            } => Self::RecordFieldsDisagree {
                field: (*field).to_string(),
                satellite: sat.to_string(),
                epoch_index: *epoch_index,
                stored: *stored,
                native: *native,
                message,
            },
            // `Sp3WriteError` is `#[non_exhaustive]`: a variant added later
            // reaches here and keeps its full message.
            _ => Self::Unknown { message },
        }
    }
}

fn sp3_write_error(err: CoreSp3WriteError) -> JsValue {
    let detail = Sp3WriteErrorDetailJs::from_core(&err);
    error_with_detail("Sp3WriteError", &err.to_string(), &detail)
}

/// Parse an SP3-c or SP3-d byte buffer (the full, already-decompressed file)
/// into a precise-ephemeris product. Throws an `Error` on malformed input.
///
/// `gapThresholdFactor` optionally configures the product-carried SP3 coverage-gap
/// interpolation policy; omitted or `null` leaves the core default (1.5).
#[wasm_bindgen(js_name = loadSp3)]
pub fn load_sp3(bytes: &[u8], gap_threshold_factor: Option<f64>) -> Result<Sp3, JsValue> {
    let mut inner = sidereon::load_sp3(bytes).map_err(engine_error)?;
    if let Some(factor) = gap_threshold_factor {
        let options = Sp3InterpolationOptions::new(factor).map_err(engine_error)?;
        inner = inner.with_interpolation_options(options);
    }
    Ok(Sp3 { inner })
}

/// Compute the precise-interpolant artifact file-level checksum for byte content.
#[wasm_bindgen(js_name = preciseInterpolantArtifactChecksum64)]
pub fn precise_interpolant_artifact_checksum64(bytes: &[u8]) -> u64 {
    core_precise_interpolant_store_checksum64(bytes)
}

/// Open precise-interpolant artifact bytes as an evaluable in-memory product.
///
/// The returned handle owns its byte buffer because JS byte slices cannot be
/// borrowed across calls by this class boundary.
#[wasm_bindgen(js_name = openPreciseInterpolantArtifact)]
pub fn open_precise_interpolant_artifact(
    bytes: &[u8],
) -> Result<PreciseInterpolantArtifact, JsValue> {
    let inner =
        CorePreciseInterpolantArtifact::from_vec(bytes.to_vec()).map_err(precise_artifact_error)?;
    Ok(PreciseInterpolantArtifact { inner })
}

/// Evaluable precise-interpolant artifact opened from canonical store bytes.
#[wasm_bindgen]
pub struct PreciseInterpolantArtifact {
    inner: CorePreciseInterpolantArtifact<'static>,
}

#[wasm_bindgen]
impl PreciseInterpolantArtifact {
    /// Read a precise-interpolant artifact from host I/O and open it in memory.
    ///
    /// Browser runtimes should use [`openPreciseInterpolantArtifact`] with
    /// fetched bytes.
    #[wasm_bindgen(js_name = fromPath)]
    pub fn from_path(path: &str) -> Result<PreciseInterpolantArtifact, JsValue> {
        let inner =
            CorePreciseInterpolantArtifact::from_path(path).map_err(precise_artifact_error)?;
        Ok(Self { inner })
    }

    /// Read a precise-interpolant artifact using a caller-attested checksum.
    ///
    /// `claimedChecksum64` is an exact JavaScript `bigint` in the unsigned
    /// 64-bit range. It must equal the checksum declared by the artifact
    /// header; a mismatch fails immediately without hashing the payload.
    #[wasm_bindgen(js_name = fromPathAttested)]
    pub fn from_path_attested(
        path: &str,
        claimed_checksum64: JsValue,
    ) -> Result<PreciseInterpolantArtifact, JsValue> {
        let claimed_checksum64 = u64_bigint(claimed_checksum64, "claimedChecksum64")?;
        let inner = CorePreciseInterpolantArtifact::from_path_attested(path, claimed_checksum64)
            .map_err(precise_artifact_error)?;
        Ok(Self { inner })
    }

    /// Internal byte bridge used by the generated Node path adapter.
    #[doc(hidden)]
    #[wasm_bindgen(js_name = __fromBytesAttested)]
    pub fn from_bytes_attested(
        bytes: &[u8],
        claimed_checksum64: JsValue,
    ) -> Result<PreciseInterpolantArtifact, JsValue> {
        let claimed_checksum64 = u64_bigint(claimed_checksum64, "claimedChecksum64")?;
        let inner =
            CorePreciseInterpolantArtifact::from_vec_attested(bytes.to_vec(), claimed_checksum64)
                .map_err(precise_artifact_error)?;
        Ok(Self { inner })
    }

    /// Number of bytes retained by this artifact handle.
    #[wasm_bindgen(getter, js_name = byteLength)]
    pub fn byte_length(&self) -> usize {
        self.inner.as_bytes().len()
    }

    /// File-level artifact checksum as a JavaScript `bigint`. An attested
    /// handle returns its caller-supplied claim without hashing.
    #[wasm_bindgen(getter)]
    pub fn checksum64(&self) -> u64 {
        self.inner.checksum64()
    }

    /// Checksum provenance: `"verified"` or `"attested"`.
    #[wasm_bindgen(getter, js_name = digestProvenance)]
    pub fn digest_provenance(&self) -> String {
        match self.inner.digest_provenance() {
            CoreDigestProvenance::Verified => "verified",
            CoreDigestProvenance::Attested => "attested",
        }
        .to_string()
    }

    /// SP3 interpolation gap threshold factor read from the artifact header.
    #[wasm_bindgen(getter, js_name = gapThresholdFactor)]
    pub fn gap_threshold_factor(&self) -> f64 {
        self.inner.interpolation_options().gap_threshold_factor()
    }

    /// Recompute and verify the file-level and per-satellite checksums.
    ///
    /// Success changes [`PreciseInterpolantArtifact.digestProvenance`] to
    /// `"verified"`.
    pub fn verify(&mut self) -> Result<(), JsValue> {
        self.inner.verify().map_err(precise_artifact_error)
    }

    /// Artifact time scale label from the stored epoch axis.
    #[wasm_bindgen(getter, js_name = timeScale)]
    pub fn time_scale(&self) -> String {
        format!("{:?}", self.inner.time_scale())
    }

    /// Satellite tokens present in the artifact, ascending.
    #[wasm_bindgen(getter)]
    pub fn satellites(&self) -> Vec<String> {
        self.inner
            .satellites()
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    /// Evaluate one satellite state at a J2000-second epoch.
    pub fn evaluate(&self, satellite: &str, j2000_seconds: f64) -> Result<Sp3State, JsValue> {
        let sat = parse_sat(satellite)?;
        let state = self
            .inner
            .position_at_j2000_seconds(sat, j2000_seconds)
            .map_err(engine_error)?;
        Ok(sp3_state_from_core(state))
    }

    #[wasm_bindgen(js_name = evaluateExact)]
    pub fn evaluate_exact(
        &self,
        satellite: &str,
        query: &ExactEpochQueryValue,
    ) -> Result<Sp3State, JsValue> {
        let satellite = parse_sat(satellite)?;
        self.inner
            .position_at_epoch_query(satellite, &query.core())
            .map(sp3_state_from_core)
            .map_err(engine_error)
    }

    #[wasm_bindgen(js_name = ephemerisVarianceAtExactQuery)]
    pub fn ephemeris_variance_at_exact_query(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        selection_epoch: &ExactEpochQueryValue,
    ) -> Result<f64, JsValue> {
        let satellite = parse_sat(satellite)?;
        Ok(precise_variance_at_queries(
            &self.inner,
            satellite,
            state_epoch,
            selection_epoch,
        ))
    }

    #[wasm_bindgen(js_name = selectedPositionClockAtExactQueries, unchecked_return_type = "Ut1Validated<SelectedPositionClock> | null")]
    pub fn selected_position_clock_at_exact_queries(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        selection_epoch: &ExactEpochQueryValue,
    ) -> Result<JsValue, JsValue> {
        let satellite = parse_sat(satellite)?;
        selected_position_clock_at_queries(&self.inner, satellite, state_epoch, selection_epoch)
    }

    #[wasm_bindgen(js_name = transmitEpochClockAtExactQueries, unchecked_return_type = "Ut1Validated<number> | null")]
    pub fn transmit_epoch_clock_at_exact_queries(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        selection_epoch: &ExactEpochQueryValue,
    ) -> Result<JsValue, JsValue> {
        let satellite = parse_sat(satellite)?;
        transmit_epoch_clock_at_queries(&self.inner, satellite, state_epoch, selection_epoch)
    }

    #[wasm_bindgen(
        js_name = clockRelativityAtExactQuery,
        unchecked_return_type = "ClockRelativity"
    )]
    pub fn clock_relativity_at_exact_query(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        position_ecef_m: Vec<f64>,
    ) -> Result<JsValue, JsValue> {
        let satellite = parse_sat(satellite)?;
        let position_ecef_m: [f64; 3] = position_ecef_m
            .try_into()
            .map_err(|_| type_error("positionEcefM must contain exactly three coordinates"))?;
        precise_clock_relativity_at_query(&self.inner, satellite, state_epoch, position_ecef_m)
    }
}

/// One epoch's common clock offset between two SP3 products.
#[wasm_bindgen]
#[derive(Clone)]
pub struct Sp3ClockReferenceOffset {
    epoch_j2000_seconds: f64,
    offset_s: f64,
    satellites: usize,
}

#[wasm_bindgen]
impl Sp3ClockReferenceOffset {
    /// Matched epoch as seconds since J2000 in the product time scale.
    #[wasm_bindgen(getter, js_name = epochJ2000Seconds)]
    pub fn epoch_j2000_seconds(&self) -> f64 {
        self.epoch_j2000_seconds
    }

    /// `other - reference` clock datum, seconds.
    #[wasm_bindgen(getter, js_name = offsetS)]
    pub fn offset_s(&self) -> f64 {
        self.offset_s
    }

    /// Number of satellites used in the median offset estimate.
    #[wasm_bindgen(getter)]
    pub fn satellites(&self) -> usize {
        self.satellites
    }
}

impl From<CoreClockReferenceOffset> for Sp3ClockReferenceOffset {
    fn from(value: CoreClockReferenceOffset) -> Self {
        Self {
            epoch_j2000_seconds: instant_to_j2000_seconds(&value.epoch),
            offset_s: value.offset_s,
            satellites: value.satellites,
        }
    }
}

/// A batch of interpolated SP3 states.
#[wasm_bindgen]
pub struct Sp3Interpolation {
    positions: Vec<f64>,
    clocks: Vec<f64>,
}

#[wasm_bindgen]
impl Sp3Interpolation {
    /// Interpolated ECEF positions, metres, as a flat row-major `Float64Array`
    /// of length `3 * epochCount` (`[x0, y0, z0, x1, y1, z1, ...]`).
    #[wasm_bindgen(getter, js_name = positionM)]
    pub fn position_m(&self) -> Vec<f64> {
        self.positions.clone()
    }

    /// Interpolated clock offsets, seconds, as a `Float64Array` (NaN where the
    /// satellite has no clock estimate at that epoch).
    #[wasm_bindgen(getter, js_name = clockS)]
    pub fn clock_s(&self) -> Vec<f64> {
        self.clocks.clone()
    }

    /// Number of query epochs in the batch.
    #[wasm_bindgen(getter, js_name = epochCount)]
    pub fn epoch_count(&self) -> usize {
        self.clocks.len()
    }
}

/// The exact parsed state of one satellite at one SP3 epoch.
#[wasm_bindgen]
pub struct Sp3State {
    position: Vec<f64>,
    clock_s: Option<f64>,
    velocity: Option<Vec<f64>>,
    clock_event: bool,
    clock_predicted: bool,
    maneuver: bool,
    orbit_predicted: bool,
}

#[wasm_bindgen]
impl Sp3State {
    /// ECEF position as a `Float64Array` `[x, y, z]`, metres.
    #[wasm_bindgen(getter, js_name = positionM)]
    pub fn position_m(&self) -> Vec<f64> {
        self.position.clone()
    }

    /// Clock offset in seconds, or `undefined` for the bad-clock sentinel.
    #[wasm_bindgen(getter, js_name = clockS)]
    pub fn clock_s(&self) -> Option<f64> {
        self.clock_s
    }

    /// ECEF velocity as a `Float64Array` `[vx, vy, vz]`, metres per second, or
    /// `undefined` for a position-only product.
    #[wasm_bindgen(getter, js_name = velocityMS)]
    pub fn velocity_m_s(&self) -> Option<Vec<f64>> {
        self.velocity.clone()
    }

    /// Clock discontinuity (`E`) flagged at this epoch.
    #[wasm_bindgen(getter, js_name = clockEvent)]
    pub fn clock_event(&self) -> bool {
        self.clock_event
    }

    /// The clock is predicted, not fitted.
    #[wasm_bindgen(getter, js_name = clockPredicted)]
    pub fn clock_predicted(&self) -> bool {
        self.clock_predicted
    }

    /// The satellite was being maneuvered at this epoch.
    #[wasm_bindgen(getter)]
    pub fn maneuver(&self) -> bool {
        self.maneuver
    }

    /// The orbit is predicted, not fitted.
    #[wasm_bindgen(getter, js_name = orbitPredicted)]
    pub fn orbit_predicted(&self) -> bool {
        self.orbit_predicted
    }
}

/// One continuity defect, as returned by [`Sp3.checkContinuity`]: the
/// summary fields every kind fills where it has them, and every field of its
/// kind under the engine's name.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ContinuityDefectJs {
    kind: String,
    satellite: String,
    from_j2000_s: Option<f64>,
    to_j2000_s: Option<f64>,
    magnitude: Option<f64>,
    bound: Option<f64>,
    epoch_j2000_s: Option<f64>,
    sample_index: Option<usize>,
    reason: Option<String>,
    occurrences: Option<usize>,
    interval_s: Option<f64>,
    displacement_m: Option<f64>,
    implied_speed_m_s: Option<f64>,
    bound_m_s: Option<f64>,
    preceding_j2000_s: Option<f64>,
    residual_m: Option<f64>,
    tolerance_m: Option<f64>,
    node_epochs_j2000_s: Option<Vec<f64>>,
}

#[cfg(test)]
mod continuity_defect_tests {
    use super::*;

    #[test]
    fn unusable_sample_detail_preserves_index_and_reason() {
        let satellite = "G01".parse().expect("satellite");
        let defect = ContinuityDefect::UnusableSample {
            sat: satellite,
            sample_index: 7,
            epoch_j2000_s: None,
            reason: UnusableSampleReason::EpochNotPlaced,
        };
        let output = ContinuityDefectJs::from(&defect);
        assert_eq!(output.kind, "unusable_sample");
        assert_eq!(output.sample_index, Some(7));
        assert_eq!(output.epoch_j2000_s, None);
        assert_eq!(output.reason.as_deref(), Some("epochNotPlaced"));
    }
}

impl From<&ContinuityDefect> for ContinuityDefectJs {
    fn from(defect: &ContinuityDefect) -> Self {
        let mut out = Self {
            kind: String::new(),
            satellite: defect.satellite().to_string(),
            from_j2000_s: None,
            to_j2000_s: None,
            magnitude: None,
            bound: None,
            epoch_j2000_s: None,
            sample_index: None,
            reason: None,
            occurrences: None,
            interval_s: None,
            displacement_m: None,
            implied_speed_m_s: None,
            bound_m_s: None,
            preceding_j2000_s: None,
            residual_m: None,
            tolerance_m: None,
            node_epochs_j2000_s: None,
        };
        match defect {
            ContinuityDefect::DuplicateEpoch {
                epoch_j2000_s,
                occurrences,
                ..
            } => {
                out.kind = "duplicate_epoch".to_string();
                out.from_j2000_s = Some(*epoch_j2000_s);
                out.to_j2000_s = Some(*epoch_j2000_s);
                out.magnitude = Some(*occurrences as f64);
                out.epoch_j2000_s = Some(*epoch_j2000_s);
                out.occurrences = Some(*occurrences);
            }
            ContinuityDefect::SingleSampleSeries { .. } => {
                out.kind = "single_sample_series".to_string();
            }
            ContinuityDefect::UnusableSample {
                sample_index,
                epoch_j2000_s,
                reason,
                ..
            } => {
                out.kind = "unusable_sample".to_string();
                out.from_j2000_s = *epoch_j2000_s;
                out.to_j2000_s = *epoch_j2000_s;
                out.epoch_j2000_s = *epoch_j2000_s;
                out.sample_index = Some(*sample_index);
                out.reason = Some(match reason {
                    UnusableSampleReason::EpochNotPlaced => "epochNotPlaced".to_string(),
                    UnusableSampleReason::NonFinitePosition => "nonFinitePosition".to_string(),
                    _ => "unknown".to_string(),
                });
            }
            ContinuityDefect::SpeedBound {
                from_j2000_s,
                to_j2000_s,
                interval_s,
                displacement_m,
                implied_speed_m_s,
                bound_m_s,
                ..
            } => {
                out.kind = "speed_bound".to_string();
                out.from_j2000_s = Some(*from_j2000_s);
                out.to_j2000_s = Some(*to_j2000_s);
                out.magnitude = Some(*implied_speed_m_s);
                out.bound = Some(*bound_m_s);
                out.interval_s = Some(*interval_s);
                out.displacement_m = Some(*displacement_m);
                out.implied_speed_m_s = Some(*implied_speed_m_s);
                out.bound_m_s = Some(*bound_m_s);
            }
            ContinuityDefect::HoldOutResidual {
                preceding_j2000_s,
                epoch_j2000_s,
                residual_m,
                tolerance_m,
                node_epochs_j2000_s,
                ..
            } => {
                out.kind = "hold_out_residual".to_string();
                out.from_j2000_s = Some(*preceding_j2000_s);
                out.to_j2000_s = Some(*epoch_j2000_s);
                out.magnitude = Some(*residual_m);
                out.bound = Some(*tolerance_m);
                out.epoch_j2000_s = Some(*epoch_j2000_s);
                out.preceding_j2000_s = Some(*preceding_j2000_s);
                out.residual_m = Some(*residual_m);
                out.tolerance_m = Some(*tolerance_m);
                out.node_epochs_j2000_s = Some(node_epochs_j2000_s.clone());
            }
        }
        out
    }
}

/// How the merge arrived at the value it wrote for one channel of one cell.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum CellSelectionJs {
    SingleSource {
        source: usize,
    },
    Precedence {
        source: usize,
        members: Vec<usize>,
    },
    Combined {
        rule: &'static str,
        members: Vec<usize>,
    },
}

impl From<&CellSelection> for CellSelectionJs {
    fn from(selection: &CellSelection) -> Self {
        match selection {
            CellSelection::SingleSource { source } => Self::SingleSource { source: *source },
            CellSelection::Precedence { source, members } => Self::Precedence {
                source: *source,
                members: members.clone(),
            },
            CellSelection::Combined { rule, members } => Self::Combined {
                rule: match rule {
                    MergeCombine::Mean => "mean",
                    MergeCombine::Median => "median",
                    MergeCombine::Precedence => "precedence",
                },
                members: members.clone(),
            },
        }
    }
}

/// One merged cell a continuity finding rests on.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MergeContinuityCellJs {
    epoch_j2000_s: f64,
    role: &'static str,
    selection: Option<CellSelectionJs>,
}

impl From<&MergeContinuityCell> for MergeContinuityCellJs {
    fn from(cell: &MergeContinuityCell) -> Self {
        Self {
            epoch_j2000_s: cell.epoch_j2000_s,
            role: match cell.role {
                MergeContinuityCellRole::HeldOut => "held_out",
                MergeContinuityCellRole::InterpolationNode => "interpolation_node",
                MergeContinuityCellRole::PairEnd => "pair_end",
                MergeContinuityCellRole::RepeatedEpoch => "repeated_epoch",
            },
            selection: cell.selection.as_ref().map(CellSelectionJs::from),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MergeContinuityViolationJs {
    defect: ContinuityDefectJs,
    from_sources: Vec<usize>,
    to_sources: Vec<usize>,
    cells: Vec<MergeContinuityCellJs>,
    sources: Vec<usize>,
    crosses_contributors: bool,
}

impl From<&MergeContinuityViolation> for MergeContinuityViolationJs {
    fn from(violation: &MergeContinuityViolation) -> Self {
        Self {
            defect: (&violation.defect).into(),
            from_sources: violation.from_sources.clone(),
            to_sources: violation.to_sources.clone(),
            cells: violation
                .cells
                .iter()
                .map(MergeContinuityCellJs::from)
                .collect(),
            sources: violation.sources.clone(),
            crosses_contributors: violation.crosses_contributors,
        }
    }
}

/// Continuity verification of a merged product, as a merge post-condition.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MergeContinuityReportJs {
    attested: bool,
    defects: Vec<ContinuityDefectJs>,
    pairs_checked: usize,
    residuals_checked: usize,
    residuals_skipped: usize,
    violations: Vec<MergeContinuityViolationJs>,
    splices: Vec<MergeContinuityViolationJs>,
}

pub(crate) fn merge_continuity_report_to_js(
    report: &MergeContinuityReport,
) -> Result<JsValue, JsValue> {
    let out = MergeContinuityReportJs {
        attested: report.attested(),
        defects: report
            .report
            .defects
            .iter()
            .map(ContinuityDefectJs::from)
            .collect(),
        pairs_checked: report.report.pairs_checked,
        residuals_checked: report.report.residuals_checked,
        residuals_skipped: report.report.residuals_skipped,
        violations: report
            .violations
            .iter()
            .map(MergeContinuityViolationJs::from)
            .collect(),
        splices: report
            .splices()
            .map(MergeContinuityViolationJs::from)
            .collect(),
    };
    serde_wasm_bindgen::to_value(&out).map_err(|error| engine_error(error.to_string()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WindowContinuityVerdictJs {
    decision: &'static str,
    accepted: bool,
    influencing_defects: Vec<ContinuityDefectJs>,
    influencing_splices: Vec<MergeContinuityViolationJs>,
    all_defects: Vec<ContinuityDefectJs>,
    all_splices: Vec<MergeContinuityViolationJs>,
}

pub(crate) fn continuity_verdict_to_js(
    verdict: WindowContinuityVerdict<'_>,
) -> Result<JsValue, JsValue> {
    let out = WindowContinuityVerdictJs {
        decision: match verdict.decision {
            WindowContinuityDecision::Accept => "accept",
            WindowContinuityDecision::Refuse => "refuse",
        },
        accepted: verdict.accepted(),
        influencing_defects: verdict
            .influencing_defects
            .into_iter()
            .map(ContinuityDefectJs::from)
            .collect(),
        influencing_splices: verdict
            .influencing_splices
            .into_iter()
            .map(MergeContinuityViolationJs::from)
            .collect(),
        all_defects: verdict
            .all_defects
            .iter()
            .map(ContinuityDefectJs::from)
            .collect(),
        all_splices: verdict
            .all_splices
            .into_iter()
            .map(MergeContinuityViolationJs::from)
            .collect(),
    };
    serde_wasm_bindgen::to_value(&out).map_err(|error| engine_error(error.to_string()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StencilExtentJs {
    before_s: f64,
    after_s: f64,
}

/// Continuity verdict for one product.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ContinuityReportJs {
    attested: bool,
    defects: Vec<ContinuityDefectJs>,
    pairs_checked: usize,
    residuals_checked: usize,
    residuals_skipped: usize,
}

// The `detail` of a thrown `Sp3WriteError`. `wasm-pack` writes this into both
// `sidereon.d.ts` targets; `types/sidereon-extra.d.ts` re-exports it.
#[wasm_bindgen(typescript_custom_section)]
const TS_SP3_WRITE_DEFINITIONS: &str = r#"
export type Sp3PreciseEphemerisAccuracySample = {
  sat: string;
  epoch: number;
  instant: Sp3SampleInstant;
  positionVarianceM2: [Sp3AccuracyValue, Sp3AccuracyValue, Sp3AccuracyValue];
  clockVarianceM2: Sp3AccuracyValue;
};

export interface Sp3PreciseEphemerisSample {
  sat: string;
  epoch: number;
  instant?: Sp3SampleInstant;
  positionEcefM: [number, number, number];
  clockS: number | null;
  clockEvent: boolean;
}

export type Sp3SampleInstant = {
  scale: "UTC" | "TAI" | "TT" | "TCG" | "TDB" | "TCB" | "GPST" | "GST" | "BDT" | "GLONASST" | "QZSST";
  representation:
    | { kind: "julianDate"; jdWhole: number; fraction: number }
    | { kind: "nanos"; nanos: string };
};

export interface PreciseSamplesErrorDetail {
  kind:
    | "EMPTY"
    | "SINGLE_SAMPLE_SATELLITE"
    | "NON_MONOTONIC_EPOCHS"
    | "MIXED_TIME_SCALES"
    | "EPOCH_NOT_REPRESENTABLE"
    | "NON_FINITE_SAMPLE"
    | "ACCURACY_SAMPLES_MISMATCH"
    | "INVALID_ACCURACY_VALUE"
    | "UNKNOWN";
  satellite: string | null;
  message: string;
}

export interface PreciseInterpolantArtifactErrorDetail {
  name: string;
  message: string;
  path?: string;
  reason?: string;
  version?: number;
  tag?: number;
  satelliteId?: string;
  expected?: string;
  found?: string;
  claimed?: string;
  declared?: string;
  available?: string;
  region?: string;
  offset?: string;
  len?: string;
}

export type ClockRelativity =
  | { kind: "notApplicable" }
  | { kind: "term"; seconds: number }
  | { kind: "unavailable" };

export interface Ut1Validated<T> { value: T; ut1Degraded: "beforeCoverage" | "afterCoverage" | null; }
export interface SelectedPositionClock { positionEcefM: [number, number, number]; clockS: number; groupDelayS: number | null; }

export type Sp3AccuracyValue =
  | { kind: "known"; value: number }
  | { kind: "unknown" }
  | { kind: "tooLarge" }
  | { kind: "invalidBase" }
  | { kind: "overflow" };

export interface Sp3AccuracyCodeGroup {
  axisExponents: [number | null, number | null, number | null];
  clockExponent: number | null;
  positionVelocityBase: number | null;
  clockRateBase: number | null;
}

export interface Sp3RawRecordAccuracy {
  p: Sp3AccuracyCodeGroup | null;
  v: Sp3AccuracyCodeGroup | null;
}

export interface Sp3PositionClockAccuracy {
  positionSigmaM: [Sp3AccuracyValue, Sp3AccuracyValue, Sp3AccuracyValue];
  clockSigmaM: Sp3AccuracyValue;
  positionVarianceM2: [Sp3AccuracyValue, Sp3AccuracyValue, Sp3AccuracyValue];
  clockVarianceM2: Sp3AccuracyValue;
}

export interface Sp3VelocityAccuracy {
  velocitySigmaMS: [Sp3AccuracyValue, Sp3AccuracyValue, Sp3AccuracyValue];
  clockRateSigmaMS: Sp3AccuracyValue;
  velocityVarianceM2S2: [Sp3AccuracyValue, Sp3AccuracyValue, Sp3AccuracyValue];
  clockRateVarianceM2S2: Sp3AccuracyValue;
}

export interface Sp3RecordAccuracy {
  p: Sp3PositionClockAccuracy | null;
  v: Sp3VelocityAccuracy | null;
}

export type Sp3WriteErrorDetail =
  | { kind: "ACCURACY_NOT_REPRESENTABLE"; satellite: string; epochIndex: number; component: string; exponent: number | null; message: string }
  | { kind: "ACCURACY_RECORD_MISMATCH"; satellite: string; epochIndex: number; message: string }
  | { kind: "ACCURACY_BASIS_MISSING"; satellite: string; epochIndex: number; message: string }
  | { kind: "TEXT_NOT_COLUMN_SAFE"; field: string; value: string; message: string }
  | { kind: "TEXT_NOT_COLUMN_STABLE"; field: string; value: string; message: string }
  | { kind: "BLANK_DESCRIPTOR"; field: string; value: string; message: string }
  | { kind: "EMPTY_COMMENT"; index: number; value: string; message: string }
  | { kind: "TEXT_TOO_WIDE"; field: string; columns: number; value: string; message: string }
  | {
      kind: "INTEGER_TOO_WIDE";
      field: string;
      columns: number;
      value: string;
      valueNumber: number | null;
      message: string;
    }
  | { kind: "NON_FINITE"; field: string; message: string }
  | {
      kind: "NUMBER_TOO_WIDE";
      field: string;
      columns: number;
      decimals: number;
      value: number;
      message: string;
    }
  | {
      kind: "PRECISION_NOT_REPRESENTABLE";
      field: string;
      columns: number;
      decimals: number;
      value: number;
      message: string;
    }
  | {
      kind: "YEAR_NOT_REPRESENTABLE";
      epochIndex: number;
      year: string;
      yearNumber: number | null;
      message: string;
    }
  | {
      kind: "EPOCH_NOT_RESTATABLE";
      epochIndex: number;
      fieldSeconds: number;
      residualS: number | null;
      message: string;
    }
  | {
      kind: "EPOCH_TIME_SCALE_MISMATCH";
      epochIndex: number;
      epochScale: string;
      headerScale: string;
      message: string;
    }
  | { kind: "HEADER_TIME_SCALE_MISMATCH"; timeSystem: string; timeScale: string; message: string }
  | {
      kind: "EPOCH_COUNT_MISMATCH";
      declared: string;
      declaredNumber: number | null;
      epochs: number;
      message: string;
    }
  | { kind: "ACCURACY_CODE_COUNT_MISMATCH"; satellites: number; codes: number; message: string }
  | { kind: "DUPLICATE_SATELLITE"; satellite: string; message: string }
  | { kind: "SATELLITE_NOT_REPRESENTABLE"; system: string; prn: number; message: string }
  | {
      kind: "EPOCH_ARRAY_LENGTH_MISMATCH";
      field: string;
      epochs: number;
      entries: number;
      message: string;
    }
  | { kind: "UNDECLARED_SATELLITE_RECORD"; satellite: string; epochIndex: number; message: string }
  | { kind: "CONFLICTING_RECORDS"; satellite: string; epochIndex: number; message: string }
  | {
      kind: "VELOCITY_STATE_IN_POSITION_PRODUCT";
      field: string;
      satellite: string;
      epochIndex: number;
      message: string;
    }
  | {
      kind: "RECORD_VALUE_NON_FINITE";
      field: string;
      satellite: string;
      epochIndex: number;
      message: string;
    }
  | {
      kind: "RECORD_VALUE_TOO_WIDE";
      field: string;
      satellite: string;
      epochIndex: number;
      columns: number;
      decimals: number;
      columnValue: number;
      message: string;
    }
  | {
      kind: "RECORD_VALUE_NOT_REPRESENTABLE";
      field: string;
      satellite: string;
      epochIndex: number;
      columns: number;
      decimals: number;
      stored: number;
      columnValue: number;
      message: string;
    }
  | {
      kind: "RECORD_READS_AS_ABSENT";
      field: string;
      satellite: string;
      epochIndex: number;
      columnValue: number;
      message: string;
    }
  | {
      kind: "RECORD_FIELDS_DISAGREE";
      field: string;
      satellite: string;
      epochIndex: number;
      stored: number | null;
      native: number | null;
      message: string;
    }
  | { kind: "UNKNOWN"; message: string };
"#;

#[cfg(test)]
mod sp3_writer_detail_mapping_tests {
    use super::*;

    #[test]
    fn current_epoch_and_accuracy_variants_keep_typed_payloads() {
        let satellite = "G07".parse().expect("valid test satellite");
        for (error, expected) in [
            (
                CoreSp3WriteError::AccuracyNotRepresentable {
                    sat: satellite,
                    epoch_index: 5,
                    component: "position",
                    exponent: Some(12),
                },
                serde_json::json!({"kind":"ACCURACY_NOT_REPRESENTABLE","satellite":"G07","epochIndex":5,"component":"position","exponent":12,"message":CoreSp3WriteError::AccuracyNotRepresentable { sat: satellite, epoch_index: 5, component: "position", exponent: Some(12) }.to_string()}),
            ),
            (
                CoreSp3WriteError::AccuracyNotRepresentable {
                    sat: satellite,
                    epoch_index: 9,
                    component: "clock",
                    exponent: None,
                },
                serde_json::json!({"kind":"ACCURACY_NOT_REPRESENTABLE","satellite":"G07","epochIndex":9,"component":"clock","exponent":null,"message":CoreSp3WriteError::AccuracyNotRepresentable { sat: satellite, epoch_index: 9, component: "clock", exponent: None }.to_string()}),
            ),
            (
                CoreSp3WriteError::AccuracyRecordMismatch {
                    sat: satellite,
                    epoch_index: 6,
                },
                serde_json::json!({"kind":"ACCURACY_RECORD_MISMATCH","satellite":"G07","epochIndex":6,"message":CoreSp3WriteError::AccuracyRecordMismatch { sat: satellite, epoch_index: 6 }.to_string()}),
            ),
            (
                CoreSp3WriteError::AccuracyBasisMissing {
                    sat: satellite,
                    epoch_index: 8,
                },
                serde_json::json!({"kind":"ACCURACY_BASIS_MISSING","satellite":"G07","epochIndex":8,"message":CoreSp3WriteError::AccuracyBasisMissing { sat: satellite, epoch_index: 8 }.to_string()}),
            ),
        ] {
            assert_eq!(
                serde_json::to_value(Sp3WriteErrorDetailJs::from_core(&error)).unwrap(),
                expected
            );
        }
    }
}
