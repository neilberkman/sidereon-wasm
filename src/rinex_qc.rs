//! RINEX lint, repair, and observation QC bindings.

use std::cell::OnceCell;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon::rinex_qc::{
    Finding, FindingRef, LintReport, NavRepair as CoreNavRepair, ObsRepair as CoreObsRepair,
    RepairAction, RepairOptions, Severity,
};
use sidereon_core::observation_qc::{
    observation_qc_with_options, render_html as core_render_html, render_text as core_render_text,
    ClockJump, CycleSlipQc, IntervalSource, MpStats, MultipathReport, ObservationDataGap,
    ObservationQcFinding, ObservationQcNote, ObservationQcOptions,
    ObservationQcReport as CoreObservationQcReport, SatelliteMultipathQc, SatelliteObservationQc,
    SatelliteSignalQc, SnrStats, SsiHistogram, SystemCycleSlipQc, SystemMultipathQc,
    SystemSignalQc,
};
use sidereon_core::rinex::crinex::encode_crinex;
use sidereon_core::rinex::nav::encode_nav;
use sidereon_core::rinex::observations::{ObsEpochTime, PgmRunByDate, RinexObsWriteError};
use sidereon_core::GnssSystem;

use crate::error::{engine_error, type_error, utf8_text};
use crate::label::{upper_snake_variant, Label};
use crate::rinex_nav::{BroadcastRecordJs, IonoCorrectionsJs};
use crate::rinex_obs::{rinex_obs_write_error, RinexObs};

fn to_value<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(value).map_err(|e| type_error(&e.to_string()))
}

fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Fatal => "fatal",
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FindingRefJs {
    epoch_index: Option<usize>,
    satellite: Option<String>,
    field: Option<&'static str>,
}

impl From<&FindingRef> for FindingRefJs {
    fn from(value: &FindingRef) -> Self {
        Self {
            epoch_index: value.epoch_index,
            satellite: value.satellite.clone(),
            field: value.field,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FindingJs {
    code: &'static str,
    severity: &'static str,
    spec_ref: &'static str,
    repairable: bool,
    at: FindingRefJs,
    detail: FindingDetailJs,
}

fn finding_js(finding: &Finding) -> FindingJs {
    FindingJs {
        code: finding.code(),
        severity: severity_label(finding.severity()),
        spec_ref: finding.spec_ref(),
        repairable: finding.is_repairable(),
        at: FindingRefJs::from(finding.at()),
        detail: FindingDetailJs::from(finding),
    }
}

/// What a lint finding states, as a union on `kind` (the engine variant in
/// UPPER_SNAKE_CASE) with the fields that variant carries. A satellite is its
/// token (`"G05"`), a system its name (`"GPS"`), a time scale its label
/// (`"gpst"`). A variant a later engine adds crosses as `OTHER` with its own
/// name in `variant` and the engine's text in `text`.
#[derive(Serialize)]
#[serde(tag = "kind")]
enum FindingDetailJs {
    #[serde(rename = "OBS_FATAL_PARSE", rename_all = "camelCase")]
    ObsFatalParse { message: String },
    #[serde(rename = "OBS_UNPUBLISHED_VERSION", rename_all = "camelCase")]
    ObsUnpublishedVersion { version: f64 },
    #[serde(rename = "OBS_MISSING_HEADER", rename_all = "camelCase")]
    ObsMissingHeader { label: &'static str },
    #[serde(rename = "OBS_MISSING_OBS_TYPES")]
    ObsMissingObsTypes,
    #[serde(rename = "OBS_INVALID_OBS_CODE", rename_all = "camelCase")]
    ObsInvalidObsCode { system: &'static str, code: String },
    #[serde(rename = "OBS_DUPLICATE_OBS_CODE", rename_all = "camelCase")]
    ObsDuplicateObsCode { system: &'static str, code: String },
    #[serde(rename = "OBS_TIME_OF_FIRST_MISMATCH", rename_all = "camelCase")]
    ObsTimeOfFirstMismatch {
        declared: ObsEpochTimeJs,
        declared_scale: &'static str,
        observed: ObsEpochTimeJs,
        observed_scale: &'static str,
    },
    #[serde(rename = "OBS_TIME_OF_LAST_MISMATCH", rename_all = "camelCase")]
    ObsTimeOfLastMismatch {
        declared: ObsEpochTimeJs,
        declared_scale: &'static str,
        observed: ObsEpochTimeJs,
        observed_scale: &'static str,
    },
    #[serde(rename = "OBS_INTERVAL_MISMATCH", rename_all = "camelCase")]
    ObsIntervalMismatch { declared_s: f64, observed_s: f64 },
    #[serde(rename = "OBS_SATELLITE_COUNT_MISMATCH", rename_all = "camelCase")]
    ObsSatelliteCountMismatch { declared: usize, observed: usize },
    #[serde(rename = "OBS_PRN_OBS_COUNT_MISMATCH", rename_all = "camelCase")]
    ObsPrnObsCountMismatch {
        satellite: String,
        code: String,
        declared: Option<usize>,
        observed: usize,
    },
    #[serde(rename = "OBS_GLONASS_SLOT_ISSUE", rename_all = "camelCase")]
    ObsGlonassSlotIssue {
        satellite: String,
        issue: &'static str,
    },
    #[serde(rename = "OBS_PHASE_SHIFT_UNDECLARED_CODE", rename_all = "camelCase")]
    ObsPhaseShiftUndeclaredCode { system: &'static str, code: String },
    #[serde(rename = "OBS_SCALE_FACTOR_ISSUE", rename_all = "camelCase")]
    ObsScaleFactorIssue {
        system: &'static str,
        code: Option<String>,
    },
    #[serde(rename = "OBS_MARKER_TYPE_ISSUE", rename_all = "camelCase")]
    ObsMarkerTypeIssue { marker_type: String },
    #[serde(rename = "OBS_IDENTITY_FIELD_ISSUE", rename_all = "camelCase")]
    ObsIdentityFieldIssue { label: &'static str, value: String },
    #[serde(rename = "OBS_IMPLAUSIBLE_APPROX_POSITION", rename_all = "camelCase")]
    ObsImplausibleApproxPosition { radius_m: f64 },
    #[serde(rename = "OBS_IMPLAUSIBLE_ANTENNA_DELTA", rename_all = "camelCase")]
    ObsImplausibleAntennaDelta { component: usize, value_m: f64 },
    #[serde(rename = "OBS_EPOCH_ORDER", rename_all = "camelCase")]
    ObsEpochOrder {
        previous: ObsEpochTimeJs,
        current: ObsEpochTimeJs,
    },
    #[serde(rename = "OBS_DUPLICATE_EPOCH", rename_all = "camelCase")]
    ObsDuplicateEpoch { epoch: ObsEpochTimeJs },
    #[serde(rename = "OBS_SKIPPED_RECORDS", rename_all = "camelCase")]
    ObsSkippedRecords { count: usize },
    #[serde(rename = "OBS_EPOCH_SAT_COUNT_MISMATCH", rename_all = "camelCase")]
    ObsEpochSatCountMismatch { declared: usize, retained: usize },
    #[serde(rename = "OBS_UNRETAINED_HEADER", rename_all = "camelCase")]
    ObsUnretainedHeader { label: String },
    #[serde(rename = "OBS_PSEUDORANGE_OUT_OF_RANGE", rename_all = "camelCase")]
    ObsPseudorangeOutOfRange { code: String, value_m: f64 },
    #[serde(rename = "OBS_LOSS_OF_LOCK_OUT_OF_RANGE", rename_all = "camelCase")]
    ObsLossOfLockOutOfRange { code: String, lli: u8 },
    #[serde(rename = "OBS_EVENT_HEADER_UNREADABLE", rename_all = "camelCase")]
    ObsEventHeaderUnreadable { message: String },
    #[serde(rename = "OBS_EVENT_EPOCH", rename_all = "camelCase")]
    ObsEventEpoch { flag: u8 },
    #[serde(rename = "OBS_EMPTY_SATELLITE_RECORD")]
    ObsEmptySatelliteRecord,
    #[serde(rename = "OBS_EPOCH_GAP", rename_all = "camelCase")]
    ObsEpochGap { gap_s: f64, interval_s: f64 },
    #[serde(rename = "OBS_INTERVAL_UNAVAILABLE")]
    ObsIntervalUnavailable,
    #[serde(rename = "OBS_INVALID_INTERVAL", rename_all = "camelCase")]
    ObsInvalidInterval { declared_s: f64 },
    #[serde(rename = "NAV_FATAL_PARSE", rename_all = "camelCase")]
    NavFatalParse { message: String },
    #[serde(rename = "NAV_LEAP_SECONDS_ABSENT")]
    NavLeapSecondsAbsent,
    #[serde(rename = "NAV_IONO_MALFORMED", rename_all = "camelCase")]
    NavIonoMalformed { message: String },
    #[serde(rename = "NAV_DROPPED_BLOCK", rename_all = "camelCase")]
    NavDroppedBlock { satellite: String, message: String },
    #[serde(rename = "NAV_DUPLICATE_RECORD", rename_all = "camelCase")]
    NavDuplicateRecord {
        satellite: String,
        same_payload: bool,
    },
    #[serde(rename = "NAV_UNSORTED_RECORDS")]
    NavUnsortedRecords,
    #[serde(rename = "NAV_IMPLAUSIBLE_RECORD", rename_all = "camelCase")]
    NavImplausibleRecord {
        satellite: String,
        field: &'static str,
        value: f64,
    },
    #[serde(rename = "NAV_UNHEALTHY_RECORDS", rename_all = "camelCase")]
    NavUnhealthyRecords { system: &'static str, count: usize },
    #[serde(rename = "NAV_OUT_OF_SCOPE_RECORDS", rename_all = "camelCase")]
    NavOutOfScopeRecords { class: String, count: usize },
    #[serde(rename = "OTHER", rename_all = "camelCase")]
    Other { variant: Label, text: String },
}

impl From<&Finding> for FindingDetailJs {
    fn from(finding: &Finding) -> Self {
        use crate::bias::time_scale_label as scale;
        match finding {
            Finding::ObsFatalParse { message, .. } => Self::ObsFatalParse {
                message: message.clone(),
            },
            Finding::ObsUnpublishedVersion { version, .. } => {
                Self::ObsUnpublishedVersion { version: *version }
            }
            Finding::ObsMissingHeader { label, .. } => Self::ObsMissingHeader { label },
            Finding::ObsMissingObsTypes { .. } => Self::ObsMissingObsTypes,
            Finding::ObsInvalidObsCode { system, code, .. } => Self::ObsInvalidObsCode {
                system: system_label(*system),
                code: code.clone(),
            },
            Finding::ObsDuplicateObsCode { system, code, .. } => Self::ObsDuplicateObsCode {
                system: system_label(*system),
                code: code.clone(),
            },
            Finding::ObsTimeOfFirstMismatch {
                declared,
                declared_scale,
                observed,
                observed_scale,
                ..
            } => Self::ObsTimeOfFirstMismatch {
                declared: epoch_time_js(*declared),
                declared_scale: scale(*declared_scale),
                observed: epoch_time_js(*observed),
                observed_scale: scale(*observed_scale),
            },
            Finding::ObsTimeOfLastMismatch {
                declared,
                declared_scale,
                observed,
                observed_scale,
                ..
            } => Self::ObsTimeOfLastMismatch {
                declared: epoch_time_js(*declared),
                declared_scale: scale(*declared_scale),
                observed: epoch_time_js(*observed),
                observed_scale: scale(*observed_scale),
            },
            Finding::ObsIntervalMismatch {
                declared_s,
                observed_s,
                ..
            } => Self::ObsIntervalMismatch {
                declared_s: *declared_s,
                observed_s: *observed_s,
            },
            Finding::ObsSatelliteCountMismatch {
                declared, observed, ..
            } => Self::ObsSatelliteCountMismatch {
                declared: *declared,
                observed: *observed,
            },
            Finding::ObsPrnObsCountMismatch {
                satellite,
                code,
                declared,
                observed,
                ..
            } => Self::ObsPrnObsCountMismatch {
                satellite: satellite.to_string(),
                code: code.clone(),
                declared: *declared,
                observed: *observed,
            },
            Finding::ObsGlonassSlotIssue {
                satellite, issue, ..
            } => Self::ObsGlonassSlotIssue {
                satellite: satellite.to_string(),
                issue,
            },
            Finding::ObsPhaseShiftUndeclaredCode { system, code, .. } => {
                Self::ObsPhaseShiftUndeclaredCode {
                    system: system_label(*system),
                    code: code.clone(),
                }
            }
            Finding::ObsScaleFactorIssue { system, code, .. } => Self::ObsScaleFactorIssue {
                system: system_label(*system),
                code: code.clone(),
            },
            Finding::ObsMarkerTypeIssue { marker_type, .. } => Self::ObsMarkerTypeIssue {
                marker_type: marker_type.clone(),
            },
            Finding::ObsIdentityFieldIssue { label, value, .. } => Self::ObsIdentityFieldIssue {
                label,
                value: value.clone(),
            },
            Finding::ObsImplausibleApproxPosition { radius_m, .. } => {
                Self::ObsImplausibleApproxPosition {
                    radius_m: *radius_m,
                }
            }
            Finding::ObsImplausibleAntennaDelta {
                component, value_m, ..
            } => Self::ObsImplausibleAntennaDelta {
                component: *component,
                value_m: *value_m,
            },
            Finding::ObsEpochOrder {
                previous, current, ..
            } => Self::ObsEpochOrder {
                previous: epoch_time_js(*previous),
                current: epoch_time_js(*current),
            },
            Finding::ObsDuplicateEpoch { epoch, .. } => Self::ObsDuplicateEpoch {
                epoch: epoch_time_js(*epoch),
            },
            Finding::ObsSkippedRecords { count, .. } => Self::ObsSkippedRecords { count: *count },
            Finding::ObsEpochSatCountMismatch {
                declared, retained, ..
            } => Self::ObsEpochSatCountMismatch {
                declared: *declared,
                retained: *retained,
            },
            Finding::ObsUnretainedHeader { label, .. } => Self::ObsUnretainedHeader {
                label: label.clone(),
            },
            Finding::ObsPseudorangeOutOfRange { code, value_m, .. } => {
                Self::ObsPseudorangeOutOfRange {
                    code: code.clone(),
                    value_m: *value_m,
                }
            }
            Finding::ObsLossOfLockOutOfRange { code, lli, .. } => Self::ObsLossOfLockOutOfRange {
                code: code.clone(),
                lli: *lli,
            },
            Finding::ObsEventHeaderUnreadable { message, .. } => Self::ObsEventHeaderUnreadable {
                message: message.clone(),
            },
            Finding::ObsEventEpoch { flag, .. } => Self::ObsEventEpoch { flag: *flag },
            Finding::ObsEmptySatelliteRecord { .. } => Self::ObsEmptySatelliteRecord,
            Finding::ObsEpochGap {
                gap_s, interval_s, ..
            } => Self::ObsEpochGap {
                gap_s: *gap_s,
                interval_s: *interval_s,
            },
            Finding::ObsIntervalUnavailable { .. } => Self::ObsIntervalUnavailable,
            Finding::ObsInvalidInterval { declared_s, .. } => Self::ObsInvalidInterval {
                declared_s: *declared_s,
            },
            Finding::NavFatalParse { message, .. } => Self::NavFatalParse {
                message: message.clone(),
            },
            Finding::NavLeapSecondsAbsent { .. } => Self::NavLeapSecondsAbsent,
            Finding::NavIonoMalformed { message, .. } => Self::NavIonoMalformed {
                message: message.clone(),
            },
            Finding::NavDroppedBlock {
                satellite, message, ..
            } => Self::NavDroppedBlock {
                satellite: satellite.clone(),
                message: message.clone(),
            },
            Finding::NavDuplicateRecord {
                satellite,
                same_payload,
                ..
            } => Self::NavDuplicateRecord {
                satellite: satellite.to_string(),
                same_payload: *same_payload,
            },
            Finding::NavUnsortedRecords { .. } => Self::NavUnsortedRecords,
            Finding::NavImplausibleRecord {
                satellite,
                field,
                value,
                ..
            } => Self::NavImplausibleRecord {
                satellite: satellite.to_string(),
                field,
                value: *value,
            },
            Finding::NavUnhealthyRecords { system, count, .. } => Self::NavUnhealthyRecords {
                system: system_label(*system),
                count: *count,
            },
            Finding::NavOutOfScopeRecords { class, count, .. } => Self::NavOutOfScopeRecords {
                class: class.clone(),
                count: *count,
            },
            other => Self::Other {
                variant: upper_snake_variant(other),
                text: format!("{other:?}"),
            },
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SeverityCountsJs {
    fatal: usize,
    error: usize,
    warning: usize,
    info: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LintReportJs {
    clean: bool,
    decoded_from_crinex: bool,
    finding_count: usize,
    counts: SeverityCountsJs,
    findings: Vec<FindingJs>,
}

fn lint_report_js(report: &LintReport) -> LintReportJs {
    LintReportJs {
        clean: report.is_clean(),
        decoded_from_crinex: report.decoded_from_crinex,
        finding_count: report.findings.len(),
        counts: SeverityCountsJs {
            fatal: report.count(Severity::Fatal),
            error: report.count(Severity::Error),
            warning: report.count(Severity::Warning),
            info: report.count(Severity::Info),
        },
        findings: report.findings.iter().map(finding_js).collect(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RepairActionJs {
    id: &'static str,
    message: String,
}

fn action_js(action: &RepairAction) -> RepairActionJs {
    RepairActionJs {
        id: action.id,
        message: action.message.clone(),
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct FileStampInput {
    program: String,
    run_by: String,
    date: String,
}

impl FileStampInput {
    fn to_core(&self) -> PgmRunByDate {
        PgmRunByDate {
            program: self.program.clone(),
            run_by: self.run_by.clone(),
            date: self.date.clone(),
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct RepairOptionsInput {
    file_stamp: Option<FileStampInput>,
    set_interval: Option<bool>,
    set_time_of_last_obs: Option<bool>,
    set_obs_counts: Option<bool>,
    drop_empty_records: Option<bool>,
    sort_records: Option<bool>,
    drop_unsupported: Option<bool>,
}

fn repair_options(value: JsValue) -> Result<RepairOptions, JsValue> {
    if value.is_null() || value.is_undefined() {
        return Ok(RepairOptions::default());
    }
    let input: RepairOptionsInput = serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid RINEX repair options: {e}")))?;
    let defaults = RepairOptions::default();
    let mut options = RepairOptions::default();
    options.file_stamp = input.file_stamp.map(|stamp| stamp.to_core());
    options.set_interval = input.set_interval.unwrap_or(defaults.set_interval);
    options.set_time_of_last_obs = input
        .set_time_of_last_obs
        .unwrap_or(defaults.set_time_of_last_obs);
    options.set_obs_counts = input.set_obs_counts.unwrap_or(defaults.set_obs_counts);
    options.drop_empty_records = input
        .drop_empty_records
        .unwrap_or(defaults.drop_empty_records);
    options.sort_records = input.sort_records.unwrap_or(defaults.sort_records);
    options.drop_unsupported = input.drop_unsupported.unwrap_or(defaults.drop_unsupported);
    Ok(options)
}

/// Lint RINEX observation text.
#[wasm_bindgen(js_name = lintRinexObs, unchecked_return_type = "RinexLintReport")]
pub fn lint_rinex_obs(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let text = utf8_text(bytes, "RINEX OBS source")?;
    to_value(&lint_report_js(&sidereon::lint_rinex_obs(&text)))
}

/// Lint RINEX navigation text.
#[wasm_bindgen(js_name = lintRinexNav, unchecked_return_type = "RinexLintReport")]
pub fn lint_rinex_nav(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let text = utf8_text(bytes, "RINEX NAV source")?;
    to_value(&lint_report_js(&sidereon::lint_rinex_nav(&text)))
}

/// Observation repair result.
///
/// The repaired product, the actions and the remaining lint report are
/// available whether or not the product can be written. The text is written
/// when first asked for, and a product the strict writer refuses throws the
/// same typed `RinexObsWriteError` from `repairedText` and `toCrinexString`
/// that `RinexObs.toRinexString` throws.
#[wasm_bindgen]
pub struct RinexObsRepair {
    inner: CoreObsRepair,
    repaired_text: OnceCell<Result<String, RinexObsWriteError>>,
}

impl RinexObsRepair {
    /// The repaired product written by the strict writer, once.
    fn written(&self) -> &Result<String, RinexObsWriteError> {
        self.repaired_text
            .get_or_init(|| self.inner.repaired.to_rinex_string())
    }
}

#[wasm_bindgen]
impl RinexObsRepair {
    #[wasm_bindgen(getter)]
    pub fn repaired(&self) -> RinexObs {
        RinexObs::from_core(self.inner.repaired.clone())
    }

    /// The repaired product as RINEX observation text. Throws a
    /// `RinexObsWriteError` whose `detail` names what the text could not
    /// carry, rather than returning text that reads back as something else.
    #[wasm_bindgen(getter, js_name = repairedText)]
    pub fn repaired_text(&self) -> Result<String, JsValue> {
        self.written().clone().map_err(rinex_obs_write_error)
    }

    #[wasm_bindgen(getter)]
    pub fn actions(&self) -> Result<JsValue, JsValue> {
        let actions: Vec<_> = self.inner.actions.iter().map(action_js).collect();
        to_value(&actions)
    }

    #[wasm_bindgen(getter, unchecked_return_type = "RinexLintReport")]
    pub fn remaining(&self) -> Result<JsValue, JsValue> {
        to_value(&lint_report_js(&self.inner.remaining))
    }

    #[wasm_bindgen(getter, js_name = decodedFromCrinex)]
    pub fn decoded_from_crinex(&self) -> bool {
        self.inner.decoded_from_crinex
    }

    /// The repaired product as CRINEX text: the strict RINEX writer's text
    /// compressed by the CRINEX encoder, as `repair_obs_to_crinex_string`
    /// composes them. A writer refusal throws the typed `RinexObsWriteError`
    /// `repairedText` throws; an encoder refusal throws an `Error` with the
    /// encoder's message.
    #[wasm_bindgen(js_name = toCrinexString)]
    pub fn to_crinex_string(&self) -> Result<String, JsValue> {
        let text = self.written().clone().map_err(rinex_obs_write_error)?;
        encode_crinex(&text).map_err(engine_error)
    }
}

/// Repair RINEX observation text.
///
/// The repair is returned whether or not its product can be written; writing
/// happens in `repairedText` and `toCrinexString`, which throw the typed
/// writer refusal.
#[wasm_bindgen(js_name = repairRinexObs)]
pub fn repair_rinex_obs(bytes: &[u8], options: JsValue) -> Result<RinexObsRepair, JsValue> {
    let text = utf8_text(bytes, "RINEX OBS source")?;
    let inner =
        sidereon::repair_rinex_obs(&text, &repair_options(options)?).map_err(engine_error)?;
    Ok(RinexObsRepair {
        inner,
        repaired_text: OnceCell::new(),
    })
}

/// Navigation repair result.
#[wasm_bindgen]
pub struct RinexNavRepair {
    inner: CoreNavRepair,
    repaired_text: String,
}

#[wasm_bindgen]
impl RinexNavRepair {
    #[wasm_bindgen(getter)]
    pub fn records(&self) -> Vec<BroadcastRecordJs> {
        self.inner
            .records
            .iter()
            .cloned()
            .map(BroadcastRecordJs::from_core)
            .collect()
    }

    #[wasm_bindgen(getter, js_name = repairedText)]
    pub fn repaired_text(&self) -> String {
        self.repaired_text.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn actions(&self) -> Result<JsValue, JsValue> {
        let actions: Vec<_> = self.inner.actions.iter().map(action_js).collect();
        to_value(&actions)
    }

    #[wasm_bindgen(getter, unchecked_return_type = "RinexLintReport")]
    pub fn remaining(&self) -> Result<JsValue, JsValue> {
        to_value(&lint_report_js(&self.inner.remaining))
    }

    #[wasm_bindgen(getter)]
    pub fn iono(&self) -> Option<IonoCorrectionsJs> {
        self.inner.iono.map(IonoCorrectionsJs::from_core)
    }

    #[wasm_bindgen(getter, js_name = leapSeconds)]
    pub fn leap_seconds(&self) -> Option<f64> {
        self.inner.leap_seconds
    }
}

/// Repair RINEX navigation text.
#[wasm_bindgen(js_name = repairRinexNav)]
pub fn repair_rinex_nav(bytes: &[u8], options: JsValue) -> Result<RinexNavRepair, JsValue> {
    let text = utf8_text(bytes, "RINEX NAV source")?;
    let inner =
        sidereon::repair_rinex_nav(&text, &repair_options(options)?).map_err(engine_error)?;
    let repaired_text = encode_nav(&inner.records).map_err(engine_error)?;
    Ok(RinexNavRepair {
        inner,
        repaired_text,
    })
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ObservationQcOptionsInput {
    interval_override_s: Option<f64>,
    gap_factor: Option<f64>,
    clock_jump_threshold_s: Option<f64>,
}

fn observation_qc_options(value: JsValue) -> Result<ObservationQcOptions, JsValue> {
    if value.is_null() || value.is_undefined() {
        return Ok(ObservationQcOptions::default());
    }
    let input: ObservationQcOptionsInput = serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid observation QC options: {e}")))?;
    let defaults = ObservationQcOptions::default();
    let mut options = ObservationQcOptions::default();
    options.interval_override_s = input.interval_override_s;
    options.gap_factor = input.gap_factor.unwrap_or(defaults.gap_factor);
    options.clock_jump_threshold_s = input
        .clock_jump_threshold_s
        .unwrap_or(defaults.clock_jump_threshold_s);
    Ok(options)
}

fn interval_source_label(source: IntervalSource) -> &'static str {
    match source {
        IntervalSource::Override => "override",
        IntervalSource::Header => "header",
        IntervalSource::Inferred => "inferred",
        IntervalSource::Unresolved => "unresolved",
    }
}

fn epoch_time_js(epoch: ObsEpochTime) -> ObsEpochTimeJs {
    ObsEpochTimeJs {
        year: epoch.year,
        month: epoch.month,
        day: epoch.day,
        hour: epoch.hour,
        minute: epoch.minute,
        second: epoch.second,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ObsEpochTimeJs {
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ObservationDataGapJs {
    start_epoch: ObsEpochTimeJs,
    end_epoch: ObsEpochTimeJs,
    nominal_interval_s: f64,
    observed_delta_s: f64,
    missing_epochs: usize,
}

fn data_gap_js(gap: &ObservationDataGap) -> ObservationDataGapJs {
    ObservationDataGapJs {
        start_epoch: epoch_time_js(gap.start_epoch),
        end_epoch: epoch_time_js(gap.end_epoch),
        nominal_interval_s: gap.nominal_interval_s,
        observed_delta_s: gap.observed_delta_s,
        missing_epochs: gap.missing_epochs,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClockJumpJs {
    epoch_index: usize,
    delta_s: f64,
}

fn clock_jump_js(jump: &ClockJump) -> ClockJumpJs {
    ClockJumpJs {
        epoch_index: jump.epoch_index,
        delta_s: jump.delta_s,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SatelliteObservationQcJs {
    satellite: String,
    epochs_with_observations: usize,
    value_observations: usize,
}

fn satellite_qc_js(value: &SatelliteObservationQc) -> SatelliteObservationQcJs {
    SatelliteObservationQcJs {
        satellite: value.satellite.to_string(),
        epochs_with_observations: value.epochs_with_observations,
        value_observations: value.value_observations,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SsiHistogramJs {
    counts: [u64; 10],
}

fn ssi_js(value: SsiHistogram) -> SsiHistogramJs {
    SsiHistogramJs {
        counts: value.counts,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SnrStatsJs {
    n: usize,
    mean: f64,
    min: f64,
    max: f64,
    std: Option<f64>,
}

fn snr_js(value: SnrStats) -> SnrStatsJs {
    SnrStatsJs {
        n: value.n,
        mean: value.mean,
        min: value.min,
        max: value.max,
        std: value.std,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SatelliteSignalQcJs {
    satellite: String,
    code: String,
    value_observations: usize,
    ssi: Option<SsiHistogramJs>,
    snr: Option<SnrStatsJs>,
}

fn satellite_signal_qc_js(value: &SatelliteSignalQc) -> SatelliteSignalQcJs {
    SatelliteSignalQcJs {
        satellite: value.satellite.to_string(),
        code: value.code.clone(),
        value_observations: value.value_observations,
        ssi: value.ssi.map(ssi_js),
        snr: value.snr.map(snr_js),
    }
}

fn system_label(system: GnssSystem) -> &'static str {
    match system {
        GnssSystem::Gps => "GPS",
        GnssSystem::Glonass => "GLONASS",
        GnssSystem::Galileo => "Galileo",
        GnssSystem::BeiDou => "BeiDou",
        GnssSystem::Qzss => "QZSS",
        GnssSystem::Sbas => "SBAS",
        GnssSystem::Navic => "NavIC",
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemSignalQcJs {
    system: &'static str,
    code: String,
    value_observations: usize,
    ssi: Option<SsiHistogramJs>,
    snr: Option<SnrStatsJs>,
}

fn system_signal_qc_js(value: &SystemSignalQc) -> SystemSignalQcJs {
    SystemSignalQcJs {
        system: system_label(value.system),
        code: value.code.clone(),
        value_observations: value.value_observations,
        ssi: value.ssi.map(ssi_js),
        snr: value.snr.map(snr_js),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemCycleSlipQcJs {
    system: &'static str,
    observations: usize,
    slips: usize,
    observations_per_slip: Option<f64>,
}

fn system_cycle_slip_qc_js(value: &SystemCycleSlipQc) -> SystemCycleSlipQcJs {
    SystemCycleSlipQcJs {
        system: system_label(value.system),
        observations: value.observations,
        slips: value.slips,
        observations_per_slip: value.observations_per_slip,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CycleSlipQcJs {
    observations: usize,
    total_slips: usize,
    observations_per_slip: Option<f64>,
    by_system: Vec<SystemCycleSlipQcJs>,
}

fn cycle_slip_qc_js(value: &CycleSlipQc) -> CycleSlipQcJs {
    CycleSlipQcJs {
        observations: value.observations,
        total_slips: value.total_slips,
        observations_per_slip: value.observations_per_slip,
        by_system: value
            .by_system
            .iter()
            .map(system_cycle_slip_qc_js)
            .collect(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MpStatsJs {
    n: usize,
    rms_m: f64,
}

fn mp_stats_js(value: MpStats) -> MpStatsJs {
    MpStatsJs {
        n: value.n,
        rms_m: value.rms_m,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SatelliteMultipathQcJs {
    satellite: String,
    mp1: Option<MpStatsJs>,
    mp2: Option<MpStatsJs>,
}

fn satellite_multipath_qc_js(value: &SatelliteMultipathQc) -> SatelliteMultipathQcJs {
    SatelliteMultipathQcJs {
        satellite: value.satellite.to_string(),
        mp1: value.mp1.map(mp_stats_js),
        mp2: value.mp2.map(mp_stats_js),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemMultipathQcJs {
    system: &'static str,
    mp1: Option<MpStatsJs>,
    mp2: Option<MpStatsJs>,
}

fn system_multipath_qc_js(value: &SystemMultipathQc) -> SystemMultipathQcJs {
    SystemMultipathQcJs {
        system: system_label(value.system),
        mp1: value.mp1.map(mp_stats_js),
        mp2: value.mp2.map(mp_stats_js),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MultipathReportJs {
    satellites: Vec<SatelliteMultipathQcJs>,
    systems: Vec<SystemMultipathQcJs>,
}

fn multipath_report_js(value: &MultipathReport) -> MultipathReportJs {
    MultipathReportJs {
        satellites: value
            .satellites
            .iter()
            .map(satellite_multipath_qc_js)
            .collect(),
        systems: value.systems.iter().map(system_multipath_qc_js).collect(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ObservationQcNoteJs {
    kind: &'static str,
    epoch_index: Option<usize>,
}

fn note_js(note: ObservationQcNote) -> ObservationQcNoteJs {
    match note {
        ObservationQcNote::NonMonotonicEpoch { epoch_index } => ObservationQcNoteJs {
            kind: "nonMonotonicEpoch",
            epoch_index: Some(epoch_index),
        },
        ObservationQcNote::IntervalUnresolved => ObservationQcNoteJs {
            kind: "intervalUnresolved",
            epoch_index: None,
        },
        // An event's header records did not read, so every epoch was taken
        // with the file header. A product read from text always reads, so only
        // a product built or changed in memory reaches this.
        ObservationQcNote::EventHeaderRecordsUnread => ObservationQcNoteJs {
            kind: "eventHeaderRecordsUnread",
            epoch_index: None,
        },
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ObservationQcReceiverJs {
    number: String,
    receiver_type: String,
    version: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ObservationQcAntennaJs {
    number: String,
    antenna_type: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ObservationQcTimeJs {
    epoch: ObsEpochTimeJs,
    time_scale: Option<String>,
}

fn qc_time_js(time: &sidereon_core::observation_qc::ObservationQcTime) -> ObservationQcTimeJs {
    ObservationQcTimeJs {
        epoch: epoch_time_js(time.epoch),
        time_scale: time.time_scale.clone(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ObservationQcHeaderJs {
    marker_name: Option<String>,
    marker_number: Option<String>,
    marker_type: Option<String>,
    receiver: Option<ObservationQcReceiverJs>,
    antenna: Option<ObservationQcAntennaJs>,
    approx_position_m: Option<[f64; 3]>,
    antenna_delta_hen_m: Option<[f64; 3]>,
    time_of_first_obs: Option<ObservationQcTimeJs>,
    time_of_last_obs: Option<ObservationQcTimeJs>,
    duration_s: Option<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemObservationQcJs {
    system: &'static str,
    satellites_seen: usize,
    epochs_with_observations: usize,
    value_observations: usize,
    expected_observations: usize,
    completeness_ratio: Option<f64>,
    gap_count: usize,
    total_gap_s: f64,
}

fn qc_header_js(h: &sidereon_core::observation_qc::ObservationQcHeader) -> ObservationQcHeaderJs {
    ObservationQcHeaderJs {
        marker_name: h.marker_name.clone(),
        marker_number: h.marker_number.clone(),
        marker_type: h.marker_type.clone(),
        receiver: h.receiver.as_ref().map(|r| ObservationQcReceiverJs {
            number: r.number.clone(),
            receiver_type: r.receiver_type.clone(),
            version: r.version.clone(),
        }),
        antenna: h.antenna.as_ref().map(|a| ObservationQcAntennaJs {
            number: a.number.clone(),
            antenna_type: a.antenna_type.clone(),
        }),
        approx_position_m: h.approx_position_m,
        antenna_delta_hen_m: h.antenna_delta_hen_m,
        time_of_first_obs: h.time_of_first_obs.as_ref().map(qc_time_js),
        time_of_last_obs: h.time_of_last_obs.as_ref().map(qc_time_js),
        duration_s: h.duration_s,
    }
}

fn system_qc_js(row: &sidereon_core::observation_qc::SystemObservationQc) -> SystemObservationQcJs {
    SystemObservationQcJs {
        system: system_label(row.system),
        satellites_seen: row.satellites_seen,
        epochs_with_observations: row.epochs_with_observations,
        value_observations: row.value_observations,
        expected_observations: row.expected_observations,
        completeness_ratio: row.completeness_ratio,
        gap_count: row.gap_count,
        total_gap_s: row.total_gap_s,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ObservationQcFindingJs {
    code: String,
    severity: &'static str,
    spec_ref: String,
}

fn observation_qc_finding_js(finding: &ObservationQcFinding) -> ObservationQcFindingJs {
    ObservationQcFindingJs {
        code: finding.code.clone(),
        severity: severity_label(finding.severity),
        spec_ref: finding.spec_ref.clone(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ObservationQcReportJs {
    header: ObservationQcHeaderJs,
    total_epoch_records: usize,
    observation_epochs: usize,
    event_records: usize,
    power_failure_epochs: usize,
    skipped_records: usize,
    interval_s: Option<f64>,
    interval_source: &'static str,
    missing_epochs: usize,
    data_gaps: Vec<ObservationDataGapJs>,
    clock_jumps: Vec<ClockJumpJs>,
    cycle_slips: CycleSlipQcJs,
    multipath: MultipathReportJs,
    satellites: Vec<SatelliteObservationQcJs>,
    satellite_signals: Vec<SatelliteSignalQcJs>,
    system_signals: Vec<SystemSignalQcJs>,
    lint_findings: Vec<ObservationQcFindingJs>,
    systems: Vec<SystemObservationQcJs>,
    notes: Vec<ObservationQcNoteJs>,
}

fn observation_qc_report_js(report: &CoreObservationQcReport) -> ObservationQcReportJs {
    ObservationQcReportJs {
        header: qc_header_js(&report.header),
        total_epoch_records: report.total_epoch_records,
        observation_epochs: report.observation_epochs,
        event_records: report.event_records,
        power_failure_epochs: report.power_failure_epochs,
        skipped_records: report.skipped_records,
        interval_s: report.interval_s,
        interval_source: interval_source_label(report.interval_source),
        missing_epochs: report.missing_epochs,
        data_gaps: report.data_gaps.iter().map(data_gap_js).collect(),
        clock_jumps: report.clock_jumps.iter().map(clock_jump_js).collect(),
        cycle_slips: cycle_slip_qc_js(&report.cycle_slips),
        multipath: multipath_report_js(&report.multipath),
        satellites: report.satellites.iter().map(satellite_qc_js).collect(),
        satellite_signals: report
            .satellite_signals
            .iter()
            .map(satellite_signal_qc_js)
            .collect(),
        system_signals: report
            .system_signals
            .iter()
            .map(system_signal_qc_js)
            .collect(),
        lint_findings: report
            .lint_findings
            .iter()
            .map(observation_qc_finding_js)
            .collect(),
        systems: report.systems.iter().map(system_qc_js).collect(),
        notes: report.notes.iter().copied().map(note_js).collect(),
    }
}

/// Aggregate observation QC report.
#[wasm_bindgen]
pub struct ObservationQcReport {
    inner: CoreObservationQcReport,
}

#[wasm_bindgen]
impl ObservationQcReport {
    /// The header fields the report was built from: marker, receiver,
    /// antenna, approximate position, antenna delta, first and last
    /// observation times and the duration between them. A field the header
    /// does not state is `null`.
    #[wasm_bindgen(getter, unchecked_return_type = "ObservationQcHeader")]
    pub fn header(&self) -> Result<JsValue, JsValue> {
        crate::error::to_plain_js(&qc_header_js(&self.inner.header), "observation QC header")
    }

    /// Per-system completeness, in the order the report lists the systems.
    #[wasm_bindgen(getter, unchecked_return_type = "SystemObservationQc[]")]
    pub fn systems(&self) -> Result<JsValue, JsValue> {
        let rows: Vec<SystemObservationQcJs> =
            self.inner.systems.iter().map(system_qc_js).collect();
        crate::error::to_plain_js(&rows, "observation QC systems")
    }

    #[wasm_bindgen(getter, js_name = totalEpochRecords)]
    pub fn total_epoch_records(&self) -> usize {
        self.inner.total_epoch_records
    }

    #[wasm_bindgen(getter, js_name = observationEpochs)]
    pub fn observation_epochs(&self) -> usize {
        self.inner.observation_epochs
    }

    #[wasm_bindgen(getter, js_name = eventRecords)]
    pub fn event_records(&self) -> usize {
        self.inner.event_records
    }

    #[wasm_bindgen(getter, js_name = powerFailureEpochs)]
    pub fn power_failure_epochs(&self) -> usize {
        self.inner.power_failure_epochs
    }

    #[wasm_bindgen(getter, js_name = skippedRecords)]
    pub fn skipped_records(&self) -> usize {
        self.inner.skipped_records
    }

    #[wasm_bindgen(getter, js_name = intervalS)]
    pub fn interval_s(&self) -> Option<f64> {
        self.inner.interval_s
    }

    #[wasm_bindgen(getter, js_name = intervalSource)]
    pub fn interval_source(&self) -> String {
        interval_source_label(self.inner.interval_source).to_string()
    }

    #[wasm_bindgen(getter, js_name = missingEpochs)]
    pub fn missing_epochs(&self) -> usize {
        self.inner.missing_epochs
    }

    #[wasm_bindgen(getter, js_name = dataGaps)]
    pub fn data_gaps(&self) -> Result<JsValue, JsValue> {
        let gaps: Vec<_> = self.inner.data_gaps.iter().map(data_gap_js).collect();
        to_value(&gaps)
    }

    #[wasm_bindgen(getter, js_name = clockJumps)]
    pub fn clock_jumps(&self) -> Result<JsValue, JsValue> {
        let jumps: Vec<_> = self.inner.clock_jumps.iter().map(clock_jump_js).collect();
        to_value(&jumps)
    }

    #[wasm_bindgen(getter, js_name = cycleSlips)]
    pub fn cycle_slips(&self) -> Result<JsValue, JsValue> {
        to_value(&cycle_slip_qc_js(&self.inner.cycle_slips))
    }

    #[wasm_bindgen(getter)]
    pub fn multipath(&self) -> Result<JsValue, JsValue> {
        to_value(&multipath_report_js(&self.inner.multipath))
    }

    #[wasm_bindgen(getter)]
    pub fn satellites(&self) -> Result<JsValue, JsValue> {
        let satellites: Vec<_> = self.inner.satellites.iter().map(satellite_qc_js).collect();
        to_value(&satellites)
    }

    #[wasm_bindgen(getter, js_name = satelliteSignals)]
    pub fn satellite_signals(&self) -> Result<JsValue, JsValue> {
        let signals: Vec<_> = self
            .inner
            .satellite_signals
            .iter()
            .map(satellite_signal_qc_js)
            .collect();
        to_value(&signals)
    }

    #[wasm_bindgen(getter, js_name = systemSignals)]
    pub fn system_signals(&self) -> Result<JsValue, JsValue> {
        let signals: Vec<_> = self
            .inner
            .system_signals
            .iter()
            .map(system_signal_qc_js)
            .collect();
        to_value(&signals)
    }

    #[wasm_bindgen(getter, js_name = lintFindings)]
    pub fn lint_findings(&self) -> Result<JsValue, JsValue> {
        let findings: Vec<_> = self
            .inner
            .lint_findings
            .iter()
            .map(observation_qc_finding_js)
            .collect();
        to_value(&findings)
    }

    #[wasm_bindgen(getter)]
    pub fn notes(&self) -> Result<JsValue, JsValue> {
        let notes: Vec<_> = self.inner.notes.iter().copied().map(note_js).collect();
        to_value(&notes)
    }

    #[wasm_bindgen(js_name = renderText)]
    pub fn render_text(&self) -> String {
        core_render_text(&self.inner)
    }

    #[wasm_bindgen(js_name = renderHtml)]
    pub fn render_html(&self) -> String {
        core_render_html(&self.inner)
    }

    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&observation_qc_report_js(&self.inner))
            .map_err(|e| type_error(&e.to_string()))
    }
}

/// Aggregate observation QC for a parsed RINEX OBS product.
#[wasm_bindgen(js_name = observationQc)]
pub fn observation_qc(obs: &RinexObs, options: JsValue) -> Result<ObservationQcReport, JsValue> {
    let inner = observation_qc_with_options(&obs.inner, observation_qc_options(options)?)
        .map_err(engine_error)?;
    Ok(ObservationQcReport { inner })
}

#[wasm_bindgen(typescript_custom_section)]
const TS_RINEX_LINT: &str = r#"
/** A header time of an observation QC report, with the time system the header names. */
export interface ObservationQcTime {
    epoch: RinexLintEpochTime;
    timeScale: string | null;
}

/** The header fields an observation QC report was built from; null where the header states none. */
export interface ObservationQcHeader {
    markerName: string | null;
    markerNumber: string | null;
    markerType: string | null;
    receiver: { number: string; receiverType: string; version: string } | null;
    antenna: { number: string; antennaType: string } | null;
    approxPositionM: [number, number, number] | null;
    antennaDeltaHenM: [number, number, number] | null;
    timeOfFirstObs: ObservationQcTime | null;
    timeOfLastObs: ObservationQcTime | null;
    durationS: number | null;
}

/** Per-system completeness of an observation QC report. */
export interface SystemObservationQc {
    system: RinexLintSystem;
    satellitesSeen: number;
    epochsWithObservations: number;
    valueObservations: number;
    expectedObservations: number;
    completenessRatio: number | null;
    gapCount: number;
    totalGapS: number;
}

/** The GNSS system a lint finding names. */
export type RinexLintSystem = "GPS" | "GLONASS" | "Galileo" | "BeiDou" | "QZSS" | "SBAS" | "NavIC";

/** The time scale a lint finding names. */
export type RinexLintTimeScale = "utc" | "tai" | "tt" | "tdb" | "gpst" | "gst" | "bdt" | "glonasst" | "qzsst" | "tcg" | "tcb";

/** A civil epoch of a lint finding, in the file's own time scale. */
export interface RinexLintEpochTime {
    year: number;
    month: number;
    day: number;
    hour: number;
    minute: number;
    second: number;
}

/**
 * What a lint finding states, as a union on kind (the engine variant in
 * UPPER_SNAKE_CASE) with its fields. A variant a later engine adds crosses as
 * OTHER with its own name in variant.
 */
export type RinexLintFindingDetail =
    | { kind: "OBS_FATAL_PARSE"; message: string }
    | { kind: "OBS_UNPUBLISHED_VERSION"; version: number }
    | { kind: "OBS_MISSING_HEADER"; label: string }
    | { kind: "OBS_MISSING_OBS_TYPES" }
    | { kind: "OBS_INVALID_OBS_CODE"; system: RinexLintSystem; code: string }
    | { kind: "OBS_DUPLICATE_OBS_CODE"; system: RinexLintSystem; code: string }
    | { kind: "OBS_TIME_OF_FIRST_MISMATCH"; declared: RinexLintEpochTime; declaredScale: RinexLintTimeScale; observed: RinexLintEpochTime; observedScale: RinexLintTimeScale }
    | { kind: "OBS_TIME_OF_LAST_MISMATCH"; declared: RinexLintEpochTime; declaredScale: RinexLintTimeScale; observed: RinexLintEpochTime; observedScale: RinexLintTimeScale }
    | { kind: "OBS_INTERVAL_MISMATCH"; declaredS: number; observedS: number }
    | { kind: "OBS_SATELLITE_COUNT_MISMATCH"; declared: number; observed: number }
    | { kind: "OBS_PRN_OBS_COUNT_MISMATCH"; satellite: string; code: string; declared: number | undefined; observed: number }
    | { kind: "OBS_GLONASS_SLOT_ISSUE"; satellite: string; issue: string }
    | { kind: "OBS_PHASE_SHIFT_UNDECLARED_CODE"; system: RinexLintSystem; code: string }
    | { kind: "OBS_SCALE_FACTOR_ISSUE"; system: RinexLintSystem; code: string | undefined }
    | { kind: "OBS_MARKER_TYPE_ISSUE"; markerType: string }
    | { kind: "OBS_IDENTITY_FIELD_ISSUE"; label: string; value: string }
    | { kind: "OBS_IMPLAUSIBLE_APPROX_POSITION"; radiusM: number }
    | { kind: "OBS_IMPLAUSIBLE_ANTENNA_DELTA"; component: number; valueM: number }
    | { kind: "OBS_EPOCH_ORDER"; previous: RinexLintEpochTime; current: RinexLintEpochTime }
    | { kind: "OBS_DUPLICATE_EPOCH"; epoch: RinexLintEpochTime }
    | { kind: "OBS_SKIPPED_RECORDS"; count: number }
    | { kind: "OBS_EPOCH_SAT_COUNT_MISMATCH"; declared: number; retained: number }
    | { kind: "OBS_UNRETAINED_HEADER"; label: string }
    | { kind: "OBS_PSEUDORANGE_OUT_OF_RANGE"; code: string; valueM: number }
    | { kind: "OBS_LOSS_OF_LOCK_OUT_OF_RANGE"; code: string; lli: number }
    | { kind: "OBS_EVENT_HEADER_UNREADABLE"; message: string }
    | { kind: "OBS_EVENT_EPOCH"; flag: number }
    | { kind: "OBS_EMPTY_SATELLITE_RECORD" }
    | { kind: "OBS_EPOCH_GAP"; gapS: number; intervalS: number }
    | { kind: "OBS_INTERVAL_UNAVAILABLE" }
    | { kind: "OBS_INVALID_INTERVAL"; declaredS: number }
    | { kind: "NAV_FATAL_PARSE"; message: string }
    | { kind: "NAV_LEAP_SECONDS_ABSENT" }
    | { kind: "NAV_IONO_MALFORMED"; message: string }
    | { kind: "NAV_DROPPED_BLOCK"; satellite: string; message: string }
    | { kind: "NAV_DUPLICATE_RECORD"; satellite: string; samePayload: boolean }
    | { kind: "NAV_UNSORTED_RECORDS" }
    | { kind: "NAV_IMPLAUSIBLE_RECORD"; satellite: string; field: string; value: number }
    | { kind: "NAV_UNHEALTHY_RECORDS"; system: RinexLintSystem; count: number }
    | { kind: "NAV_OUT_OF_SCOPE_RECORDS"; class: string; count: number }
    | { kind: "OTHER"; variant: string; text: string };

/** One lint finding. */
export interface RinexLintFinding {
    code: string;
    severity: "fatal" | "error" | "warning" | "info";
    specRef: string;
    repairable: boolean;
    at: {
        epochIndex: number | undefined;
        satellite: string | undefined;
        field: string | undefined;
    };
    detail: RinexLintFindingDetail;
}

/** The result of lintRinexObs and lintRinexNav. */
export interface RinexLintReport {
    clean: boolean;
    decodedFromCrinex: boolean;
    findingCount: number;
    counts: { fatal: number; error: number; warning: number; info: number };
    findings: RinexLintFinding[];
}
"#;
