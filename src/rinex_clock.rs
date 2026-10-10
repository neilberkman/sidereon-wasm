//! RINEX clock products: lossless reading, typed views of every header and
//! data record, editing, writing under a policy, and satellite clock-bias
//! interpolation. Mirrors the core `rinex::clock` surface.
//!
//! A product read from text keeps the text as its authority: every header line
//! and every body line, `AR`, `AS`, `CR`, `DR` and `MS` records, blank lines,
//! and in a lossy read the lines that do not read, so writing an unedited
//! product restates its input byte for byte. Every rule is the engine's; this
//! module carries the views, edits and refusals across the boundary. Absence
//! is never a zero: a getter with no value returns `undefined`, a structured
//! object holds `null`, and a numeric typed array holds `NaN` beside a
//! validity array.

use serde::Serialize;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use sidereon_core::astro::time::model::{Instant, InstantRepr, TimeScale as CoreTimeScale};
use sidereon_core::rinex::clock::{
    civil_to_clock_instant, civil_to_gps_seconds, ClockEpoch as CoreClockEpoch, ClockHeaderField,
    ClockHeaderReading, ClockHeaderRecord, ClockLayout, ClockPoint as CoreClockPoint,
    ClockRecord as CoreClockRecord, ClockRecordReading, ClockRecordType, ClockTimeSystem,
    ClockTimeSystemStatus, ClockWriteDeparture, ClockWriteLeniency, ClockWritePolicy,
    RinexClock as CoreRinexClock, RinexClockError, RinexClockNotice,
};

use crate::error::{
    error_with_detail, index_arg, range_error, reject_unknown_keys, result_object,
    safe_integer_number, to_plain_js, type_error, utf8_text,
};
use crate::frames::TimeScale;
use crate::label::{lower_camel_variant, Label};

// --- Shared representations -------------------------------------------------

/// A civil epoch as its six fields. `second` is the nearest double to the
/// stated second; a record's exact seconds text is in its `sourceLines`.
#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
struct CivilEpochJs {
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: f64,
}

impl From<CoreClockEpoch> for CivilEpochJs {
    fn from(epoch: CoreClockEpoch) -> Self {
        Self {
            year: epoch.year,
            month: epoch.month,
            day: epoch.day,
            hour: epoch.hour,
            minute: epoch.minute,
            second: epoch.second,
        }
    }
}

/// A scale-tagged instant exactly as the engine holds it: the two parts of a
/// split Julian date, or an integer nanosecond count as an exact decimal
/// string, with its GPS seconds where the scale projects onto GPS time.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstantJs {
    scale: &'static str,
    jd_whole: Option<f64>,
    jd_fraction: Option<f64>,
    nanos: Option<String>,
    gps_seconds: Option<f64>,
}

/// GPS seconds of an instant on the GPST timeline (GPST and QZSST), as the
/// engine's clock points project it.
fn instant_gps_seconds(epoch: Instant) -> Option<f64> {
    CoreClockPoint::new(epoch, 0.0, Vec::new()).gps_seconds()
}

impl From<Instant> for InstantJs {
    fn from(epoch: Instant) -> Self {
        let (jd_whole, jd_fraction, nanos) = match epoch.repr {
            InstantRepr::JulianDate(split) => (Some(split.jd_whole), Some(split.fraction), None),
            InstantRepr::Nanos(nanos) => (None, None, Some(nanos.to_string())),
        };
        Self {
            scale: epoch.scale.abbrev(),
            jd_whole,
            jd_fraction,
            nanos,
            gps_seconds: instant_gps_seconds(epoch),
        }
    }
}

fn layout_label(layout: ClockLayout) -> &'static str {
    match layout {
        ClockLayout::V300 => "v300",
        ClockLayout::V304 => "v304",
    }
}

fn record_type_code(record_type: ClockRecordType) -> &'static str {
    record_type.code()
}

#[derive(Serialize)]
#[serde(untagged)]
enum RecordReadingJs {
    Label(Label),
    TrailingText {
        kind: &'static str,
        layout: &'static str,
    },
}

fn record_reading_js(reading: ClockRecordReading) -> RecordReadingJs {
    match reading {
        ClockRecordReading::Columns(ClockLayout::V300) => {
            RecordReadingJs::Label(Label::Borrowed("columnsV300"))
        }
        ClockRecordReading::Columns(ClockLayout::V304) => {
            RecordReadingJs::Label(Label::Borrowed("columnsV304"))
        }
        ClockRecordReading::Whitespace => RecordReadingJs::Label(Label::Borrowed("whitespace")),
        ClockRecordReading::ColumnsTrailingText(layout) => RecordReadingJs::TrailingText {
            kind: "columnsTrailingText",
            layout: layout_label(layout),
        },
        ClockRecordReading::Edited => RecordReadingJs::Label(Label::Borrowed("edited")),
        // `ClockRecordReading` is `#[non_exhaustive]`.
        other => RecordReadingJs::Label(lower_camel_variant(&other)),
    }
}

fn header_reading_label(reading: ClockHeaderReading) -> Label {
    Label::Borrowed(match reading {
        ClockHeaderReading::Columns => "columns",
        ClockHeaderReading::OtherVersionColumns => "otherVersionColumns",
        ClockHeaderReading::Whitespace => "whitespace",
        ClockHeaderReading::Uninterpreted => "uninterpreted",
        ClockHeaderReading::UnknownLabel => "unknownLabel",
        // `ClockHeaderReading` is `#[non_exhaustive]`.
        other => return lower_camel_variant(&other),
    })
}

/// An engine `i64` as an exact decimal string beside a `number` that is
/// `null` where the integer is not exactly representable as one.
fn exact_i64(value: i64) -> (String, Option<f64>) {
    (value.to_string(), safe_integer_number(i128::from(value)))
}

// --- Errors -------------------------------------------------------------------

/// A RINEX clock refusal, as the `detail` of the thrown error and of a lossy
/// read's diagnostics.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind")]
enum RinexClockErrorDetailJs {
    #[serde(rename = "MALFORMED_AS_RECORD", rename_all = "camelCase")]
    MalformedAsRecord {
        line: usize,
        reason: String,
        record: String,
        message: String,
    },
    #[serde(rename = "MISSING_CONTINUATION", rename_all = "camelCase")]
    MissingContinuation {
        line: usize,
        record_type: String,
        message: String,
    },
    #[serde(rename = "MALFORMED_CONTINUATION", rename_all = "camelCase")]
    MalformedContinuation {
        line: usize,
        reason: String,
        record: String,
        message: String,
    },
    #[serde(rename = "BAD_FIELD", rename_all = "camelCase")]
    BadField {
        line: usize,
        field: String,
        value: String,
        message: String,
    },
    #[serde(rename = "INVALID_INPUT", rename_all = "camelCase")]
    InvalidInput {
        field: String,
        reason: String,
        message: String,
    },
    #[serde(rename = "UNSUPPORTED_TIME_SCALE", rename_all = "camelCase")]
    UnsupportedTimeScale { scale: String, message: String },
}

impl From<&RinexClockError> for RinexClockErrorDetailJs {
    fn from(err: &RinexClockError) -> Self {
        let message = err.to_string();
        match err {
            RinexClockError::MalformedAsRecord {
                line,
                reason,
                record,
            } => Self::MalformedAsRecord {
                line: *line,
                reason: (*reason).to_string(),
                record: record.clone(),
                message,
            },
            RinexClockError::MissingContinuation { line, record_type } => {
                Self::MissingContinuation {
                    line: *line,
                    record_type: record_type.clone(),
                    message,
                }
            }
            RinexClockError::MalformedContinuation {
                line,
                reason,
                record,
            } => Self::MalformedContinuation {
                line: *line,
                reason: (*reason).to_string(),
                record: record.clone(),
                message,
            },
            RinexClockError::BadField { line, field, value } => Self::BadField {
                line: *line,
                field: (*field).to_string(),
                value: value.clone(),
                message,
            },
            RinexClockError::InvalidInput { field, reason } => Self::InvalidInput {
                field: (*field).to_string(),
                reason: (*reason).to_string(),
                message,
            },
            RinexClockError::UnsupportedTimeScale { scale } => Self::UnsupportedTimeScale {
                scale: scale.abbrev().to_string(),
                message,
            },
        }
    }
}

/// A RINEX clock failure as an `Error` named `name` whose `detail` is a
/// `RinexClockErrorDetail`.
fn clock_error(name: &str, err: &RinexClockError) -> JsValue {
    error_with_detail(name, &err.to_string(), &RinexClockErrorDetailJs::from(err))
}

fn parse_error(err: RinexClockError) -> JsValue {
    clock_error("RinexClockParseError", &err)
}

fn query_error(err: RinexClockError) -> JsValue {
    clock_error("RinexClockQueryError", &err)
}

fn write_error(err: RinexClockError) -> JsValue {
    clock_error("RinexClockWriteError", &err)
}

fn edit_error(err: RinexClockError) -> JsValue {
    clock_error("RinexClockEditError", &err)
}

fn build_error(err: RinexClockError) -> JsValue {
    clock_error("RinexClockBuildError", &err)
}

// --- Header, records, findings --------------------------------------------------

#[derive(Serialize)]
#[serde(tag = "kind")]
enum HeaderFieldJs {
    #[serde(rename = "VERSION_TYPE", rename_all = "camelCase")]
    VersionType {
        version: f64,
        file_type: String,
        satellite_system: String,
    },
    #[serde(rename = "PROGRAM_RUN_BY_DATE", rename_all = "camelCase")]
    ProgramRunByDate {
        program: String,
        run_by: String,
        date: String,
    },
    #[serde(rename = "COMMENT", rename_all = "camelCase")]
    Comment { text: String },
    /// `system` and `count` are `null` on a continuation line.
    #[serde(rename = "OBSERVATION_TYPES", rename_all = "camelCase")]
    ObservationTypes {
        system: Option<String>,
        count: Option<usize>,
        descriptors: Vec<String>,
    },
    #[serde(rename = "TIME_SYSTEM", rename_all = "camelCase")]
    TimeSystem { label: String },
    #[serde(rename = "LEAP_SECONDS", rename_all = "camelCase")]
    LeapSeconds {
        seconds: String,
        seconds_number: Option<f64>,
    },
    #[serde(rename = "LEAP_SECONDS_GNSS", rename_all = "camelCase")]
    LeapSecondsGnss {
        seconds: String,
        seconds_number: Option<f64>,
    },
    #[serde(rename = "DCBS_APPLIED", rename_all = "camelCase")]
    DcbsApplied {
        system: String,
        program: String,
        source: String,
    },
    #[serde(rename = "PCVS_APPLIED", rename_all = "camelCase")]
    PcvsApplied {
        system: String,
        program: String,
        source: String,
    },
    #[serde(rename = "TYPES_OF_DATA", rename_all = "camelCase")]
    TypesOfData { count: usize, types: Vec<String> },
    #[serde(rename = "STATION_NAME_NUM", rename_all = "camelCase")]
    StationNameNum { name: String, identifier: String },
    #[serde(rename = "STATION_CLOCK_REF", rename_all = "camelCase")]
    StationClockRef { text: String },
    #[serde(rename = "ANALYSIS_CENTER", rename_all = "camelCase")]
    AnalysisCenter { designator: String, name: String },
    #[serde(rename = "CLOCK_REF_COUNT", rename_all = "camelCase")]
    ClockRefCount {
        count: usize,
        start: Option<CivilEpochJs>,
        stop: Option<CivilEpochJs>,
    },
    #[serde(rename = "ANALYSIS_CLOCK_REF", rename_all = "camelCase")]
    AnalysisClockRef {
        name: String,
        identifier: String,
        constraint_s: Option<f64>,
    },
    #[serde(rename = "SOLUTION_STATION_COUNT", rename_all = "camelCase")]
    SolutionStationCount { count: usize, frame: String },
    /// Geocentric X, Y, Z in millimetres, each an exact decimal string beside
    /// a `number` that is `null` where it would not survive the conversion.
    #[serde(rename = "SOLUTION_STATION", rename_all = "camelCase")]
    SolutionStation {
        name: String,
        identifier: String,
        xyz_mm: [String; 3],
        xyz_mm_number: [Option<f64>; 3],
    },
    #[serde(rename = "SOLUTION_SATELLITE_COUNT", rename_all = "camelCase")]
    SolutionSatelliteCount { count: usize },
    #[serde(rename = "PRN_LIST", rename_all = "camelCase")]
    PrnList { prns: Vec<String> },
    #[serde(rename = "END_OF_HEADER")]
    EndOfHeader,
    /// A header field this binding does not yet name, described by the
    /// engine.
    #[serde(rename = "UNKNOWN", rename_all = "camelCase")]
    Unknown { message: String },
}

impl From<&ClockHeaderField> for HeaderFieldJs {
    fn from(field: &ClockHeaderField) -> Self {
        match field {
            ClockHeaderField::VersionType {
                version,
                file_type,
                satellite_system,
            } => Self::VersionType {
                version: *version,
                file_type: file_type.clone(),
                satellite_system: satellite_system.clone(),
            },
            ClockHeaderField::ProgramRunByDate {
                program,
                run_by,
                date,
            } => Self::ProgramRunByDate {
                program: program.clone(),
                run_by: run_by.clone(),
                date: date.clone(),
            },
            ClockHeaderField::Comment(text) => Self::Comment { text: text.clone() },
            ClockHeaderField::ObservationTypes {
                system,
                count,
                descriptors,
            } => Self::ObservationTypes {
                system: system.map(String::from),
                count: *count,
                descriptors: descriptors.clone(),
            },
            ClockHeaderField::TimeSystem { label } => Self::TimeSystem {
                label: label.clone(),
            },
            ClockHeaderField::LeapSeconds(value) => {
                let (seconds, seconds_number) = exact_i64(*value);
                Self::LeapSeconds {
                    seconds,
                    seconds_number,
                }
            }
            ClockHeaderField::LeapSecondsGnss(value) => {
                let (seconds, seconds_number) = exact_i64(*value);
                Self::LeapSecondsGnss {
                    seconds,
                    seconds_number,
                }
            }
            ClockHeaderField::DcbsApplied {
                system,
                program,
                source,
            } => Self::DcbsApplied {
                system: system.clone(),
                program: program.clone(),
                source: source.clone(),
            },
            ClockHeaderField::PcvsApplied {
                system,
                program,
                source,
            } => Self::PcvsApplied {
                system: system.clone(),
                program: program.clone(),
                source: source.clone(),
            },
            ClockHeaderField::TypesOfData { count, types } => Self::TypesOfData {
                count: *count,
                types: types.clone(),
            },
            ClockHeaderField::StationNameNum { name, identifier } => Self::StationNameNum {
                name: name.clone(),
                identifier: identifier.clone(),
            },
            ClockHeaderField::StationClockRef(text) => Self::StationClockRef { text: text.clone() },
            ClockHeaderField::AnalysisCenter { designator, name } => Self::AnalysisCenter {
                designator: designator.clone(),
                name: name.clone(),
            },
            ClockHeaderField::ClockRefCount { count, start, stop } => Self::ClockRefCount {
                count: *count,
                start: start.map(CivilEpochJs::from),
                stop: stop.map(CivilEpochJs::from),
            },
            ClockHeaderField::AnalysisClockRef {
                name,
                identifier,
                constraint_s,
            } => Self::AnalysisClockRef {
                name: name.clone(),
                identifier: identifier.clone(),
                constraint_s: *constraint_s,
            },
            ClockHeaderField::SolutionStationCount { count, frame } => Self::SolutionStationCount {
                count: *count,
                frame: frame.clone(),
            },
            ClockHeaderField::SolutionStation {
                name,
                identifier,
                xyz_mm,
            } => {
                let [x, y, z] = xyz_mm.map(exact_i64);
                Self::SolutionStation {
                    name: name.clone(),
                    identifier: identifier.clone(),
                    xyz_mm: [x.0, y.0, z.0],
                    xyz_mm_number: [x.1, y.1, z.1],
                }
            }
            ClockHeaderField::SolutionSatelliteCount(count) => {
                Self::SolutionSatelliteCount { count: *count }
            }
            ClockHeaderField::PrnList(prns) => Self::PrnList { prns: prns.clone() },
            ClockHeaderField::EndOfHeader => Self::EndOfHeader,
            // `ClockHeaderField` is `#[non_exhaustive]`.
            other => Self::Unknown {
                message: format!("{other:?}"),
            },
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HeaderRecordJs {
    line: Option<usize>,
    text: String,
    label: String,
    label_column: usize,
    payload: String,
    reading: Label,
    field: Option<HeaderFieldJs>,
}

impl From<&ClockHeaderRecord> for HeaderRecordJs {
    fn from(record: &ClockHeaderRecord) -> Self {
        Self {
            line: record.line(),
            text: record.text().to_string(),
            label: record.label().to_string(),
            label_column: record.label_column(),
            payload: record.payload().to_string(),
            reading: header_reading_label(record.reading()),
            field: record.field().map(HeaderFieldJs::from),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SurplusValueJs {
    position: usize,
    value: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordJs {
    index: usize,
    record_type: &'static str,
    name: String,
    satellite: Option<String>,
    civil_epoch: CivilEpochJs,
    epoch: Option<InstantJs>,
    declared_count: usize,
    values: Vec<f64>,
    surplus_values: Vec<SurplusValueJs>,
    line: Option<usize>,
    line_count: usize,
    reading: RecordReadingJs,
    continuation_reading: Option<RecordReadingJs>,
    source_lines: Vec<String>,
}

fn record_js(clock: &CoreRinexClock, index: usize, record: &CoreClockRecord) -> RecordJs {
    let source_lines = match record.line() {
        Some(first) => (first..first + record.line_count())
            .filter_map(|line| clock.source_line(line).map(str::to_string))
            .collect(),
        None => Vec::new(),
    };
    RecordJs {
        index,
        record_type: record_type_code(record.record_type()),
        name: record.name().to_string(),
        satellite: record.satellite().map(str::to_string),
        civil_epoch: record.civil_epoch().into(),
        epoch: record.epoch().map(InstantJs::from),
        declared_count: record.declared_count(),
        values: record.values().to_vec(),
        surplus_values: record
            .surplus_values()
            .iter()
            .map(|surplus| SurplusValueJs {
                position: surplus.position,
                value: surplus.value,
            })
            .collect(),
        line: record.line(),
        line_count: record.line_count(),
        reading: record_reading_js(record.reading()),
        continuation_reading: record.continuation_reading().map(record_reading_js),
        source_lines,
    }
}

#[derive(Serialize)]
#[serde(tag = "kind")]
enum TimeSystemStatusJs {
    #[serde(rename = "DECLARED")]
    Declared,
    #[serde(rename = "DEFAULTED")]
    Defaulted,
    #[serde(rename = "UNRECOGNIZED", rename_all = "camelCase")]
    Unrecognized { label: String },
    #[serde(rename = "CONFLICTING", rename_all = "camelCase")]
    Conflicting { labels: Vec<String> },
    #[serde(rename = "CONSTRUCTED")]
    Constructed,
    #[serde(rename = "UNKNOWN", rename_all = "camelCase")]
    Unknown { message: String },
}

impl From<&ClockTimeSystemStatus> for TimeSystemStatusJs {
    fn from(status: &ClockTimeSystemStatus) -> Self {
        match status {
            ClockTimeSystemStatus::Declared => Self::Declared,
            ClockTimeSystemStatus::Defaulted => Self::Defaulted,
            ClockTimeSystemStatus::Unrecognized { label } => Self::Unrecognized {
                label: label.clone(),
            },
            ClockTimeSystemStatus::Conflicting { labels } => Self::Conflicting {
                labels: labels.clone(),
            },
            ClockTimeSystemStatus::Constructed => Self::Constructed,
            // `ClockTimeSystemStatus` is `#[non_exhaustive]`.
            other => Self::Unknown {
                message: format!("{other:?}"),
            },
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind")]
enum NoticeJs {
    #[serde(rename = "TIME_SYSTEM_DEFAULTED", rename_all = "camelCase")]
    TimeSystemDefaulted { system: String, message: String },
    #[serde(rename = "TIME_SYSTEM_MISSING", rename_all = "camelCase")]
    TimeSystemMissing { message: String },
    #[serde(rename = "TIME_SYSTEM_WITHOUT_SCALE", rename_all = "camelCase")]
    TimeSystemWithoutScale { system: String, message: String },
    #[serde(rename = "HEADER_RECORD_NONCONFORMING", rename_all = "camelCase")]
    HeaderRecordNonconforming { line: usize, message: String },
    #[serde(rename = "HEADER_RECORD_UNINTERPRETED", rename_all = "camelCase")]
    HeaderRecordUninterpreted { line: usize, message: String },
    #[serde(rename = "HEADER_RECORD_UNKNOWN_LABEL", rename_all = "camelCase")]
    HeaderRecordUnknownLabel { line: usize, message: String },
    #[serde(rename = "SURPLUS_VALUES", rename_all = "camelCase")]
    SurplusValues {
        records: usize,
        first_line: usize,
        message: String,
    },
    #[serde(rename = "OTHER_LAYOUT_RECORDS", rename_all = "camelCase")]
    OtherLayoutRecords {
        records: usize,
        first_line: usize,
        message: String,
    },
    #[serde(rename = "WHITESPACE_RECORDS", rename_all = "camelCase")]
    WhitespaceRecords {
        records: usize,
        first_line: usize,
        message: String,
    },
    #[serde(rename = "TRAILING_TEXT_RECORDS", rename_all = "camelCase")]
    TrailingTextRecords {
        records: usize,
        first_line: usize,
        message: String,
    },
    #[serde(rename = "UNKNOWN", rename_all = "camelCase")]
    Unknown { message: String },
}

impl From<&RinexClockNotice> for NoticeJs {
    fn from(notice: &RinexClockNotice) -> Self {
        let message = notice.to_string();
        match notice {
            RinexClockNotice::TimeSystemDefaulted { system } => Self::TimeSystemDefaulted {
                system: system.label().to_string(),
                message,
            },
            RinexClockNotice::TimeSystemMissing => Self::TimeSystemMissing { message },
            RinexClockNotice::TimeSystemWithoutScale { system } => Self::TimeSystemWithoutScale {
                system: system.label().to_string(),
                message,
            },
            RinexClockNotice::HeaderRecordNonconforming { line } => {
                Self::HeaderRecordNonconforming {
                    line: *line,
                    message,
                }
            }
            RinexClockNotice::HeaderRecordUninterpreted { line } => {
                Self::HeaderRecordUninterpreted {
                    line: *line,
                    message,
                }
            }
            RinexClockNotice::HeaderRecordUnknownLabel { line } => Self::HeaderRecordUnknownLabel {
                line: *line,
                message,
            },
            RinexClockNotice::SurplusValues {
                records,
                first_line,
            } => Self::SurplusValues {
                records: *records,
                first_line: *first_line,
                message,
            },
            RinexClockNotice::OtherLayoutRecords {
                records,
                first_line,
            } => Self::OtherLayoutRecords {
                records: *records,
                first_line: *first_line,
                message,
            },
            RinexClockNotice::WhitespaceRecords {
                records,
                first_line,
            } => Self::WhitespaceRecords {
                records: *records,
                first_line: *first_line,
                message,
            },
            RinexClockNotice::TrailingTextRecords {
                records,
                first_line,
            } => Self::TrailingTextRecords {
                records: *records,
                first_line: *first_line,
                message,
            },
            // `RinexClockNotice` is `#[non_exhaustive]`.
            _ => Self::Unknown { message },
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SkipJs {
    line: usize,
    record_type: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticJs {
    line: usize,
    error: RinexClockErrorDetailJs,
}

#[derive(Serialize)]
#[serde(tag = "kind")]
enum DepartureJs {
    #[serde(rename = "EPOCH_AT_NEAREST_MICROSECOND", rename_all = "camelCase")]
    EpochAtNearestMicrosecond {
        record: usize,
        name: String,
        epoch: Option<InstantJs>,
        written: String,
        message: String,
    },
    #[serde(rename = "UNKNOWN", rename_all = "camelCase")]
    Unknown { message: String },
}

impl From<&ClockWriteDeparture> for DepartureJs {
    fn from(departure: &ClockWriteDeparture) -> Self {
        let message = departure.to_string();
        match departure {
            ClockWriteDeparture::EpochAtNearestMicrosecond {
                record,
                name,
                epoch,
                written,
            } => Self::EpochAtNearestMicrosecond {
                record: *record,
                name: name.clone(),
                epoch: epoch.map(InstantJs::from),
                written: written.clone(),
                message,
            },
            // `ClockWriteDeparture` is `#[non_exhaustive]`.
            _ => Self::Unknown { message },
        }
    }
}

// --- Inputs ------------------------------------------------------------------

fn time_system_from_label(label: &str) -> Result<ClockTimeSystem, JsValue> {
    ClockTimeSystem::from_label(label).ok_or_else(|| {
        type_error(&format!(
            "invalid RINEX clock time system {label:?}: expected GPS, GLO, GAL, QZS, BDS, BDT, IRN, UTC or TAI"
        ))
    })
}

fn record_type_from_code(code: &str) -> Result<ClockRecordType, JsValue> {
    ClockRecordType::from_code(code).ok_or_else(|| {
        type_error(&format!(
            "invalid RINEX clock record type {code:?}: expected AR, AS, CR, DR or MS"
        ))
    })
}

fn get_property(object: &JsValue, key: &str) -> Result<JsValue, JsValue> {
    js_sys::Reflect::get(object, &JsValue::from_str(key))
}

/// Read an integer property within `[min, max]`: a missing or non-number
/// value is a `TypeError`, a fractional or out-of-range one a `RangeError`.
fn integer_property(
    object: &JsValue,
    path: &str,
    key: &str,
    min: f64,
    max: f64,
) -> Result<f64, JsValue> {
    let value = get_property(object, key)?
        .as_f64()
        .ok_or_else(|| type_error(&format!("{path}.{key} must be a number")))?;
    if !value.is_finite() || value.fract() != 0.0 || value < min || value > max {
        return Err(range_error(&format!(
            "{path}.{key} must be an integer in [{min}, {max}]"
        )));
    }
    Ok(value)
}

/// Read a civil epoch from a `ClockEpoch` or any object with the six fields.
/// The fields' ranges are checked here; whether they name an epoch is the
/// engine's to decide in the relevant time scale.
fn civil_epoch_from_js(value: &JsValue, path: &str) -> Result<CoreClockEpoch, JsValue> {
    if !value.is_object() {
        return Err(type_error(&format!(
            "{path} must be a ClockEpoch or an object with year, month, day, hour, minute and second"
        )));
    }
    let second = get_property(value, "second")?
        .as_f64()
        .ok_or_else(|| type_error(&format!("{path}.second must be a number")))?;
    Ok(CoreClockEpoch {
        year: integer_property(
            value,
            path,
            "year",
            f64::from(i32::MIN),
            f64::from(i32::MAX),
        )? as i32,
        month: integer_property(value, path, "month", 0.0, 255.0)? as u8,
        day: integer_property(value, path, "day", 0.0, 255.0)? as u8,
        hour: integer_property(value, path, "hour", 0.0, 255.0)? as u8,
        minute: integer_property(value, path, "minute", 0.0, 255.0)? as u8,
        second,
    })
}

/// Read an array or `Float64Array` of numbers.
fn numbers_from_js(value: &JsValue, path: &str) -> Result<Vec<f64>, JsValue> {
    if let Some(array) = value.dyn_ref::<js_sys::Float64Array>() {
        return Ok(array.to_vec());
    }
    if !js_sys::Array::is_array(value) {
        return Err(type_error(&format!(
            "{path} must be an array or Float64Array of numbers"
        )));
    }
    js_sys::Array::from(value)
        .iter()
        .enumerate()
        .map(|(index, item)| {
            item.as_f64()
                .ok_or_else(|| type_error(&format!("{path}[{index}] must be a number")))
        })
        .collect()
}

fn array_from_js(value: &JsValue, path: &str) -> Result<js_sys::Array, JsValue> {
    if !js_sys::Array::is_array(value) {
        return Err(type_error(&format!("{path} must be an array")));
    }
    Ok(js_sys::Array::from(value))
}

fn string_property(object: &JsValue, path: &str, key: &str) -> Result<String, JsValue> {
    get_property(object, key)?
        .as_string()
        .ok_or_else(|| type_error(&format!("{path}.{key} must be a string")))
}

/// The write policy a JavaScript value states: `"strict"` or omitted allows
/// no departure, `"lenient"` allows every one, and an object names
/// `nearestMicrosecondEpochs` as `"strict"` or `"allow"`.
fn write_policy_from_js(value: &JsValue) -> Result<ClockWritePolicy, JsValue> {
    if value.is_undefined() || value.is_null() {
        return Ok(ClockWritePolicy::strict());
    }
    if let Some(text) = value.as_string() {
        return match text.as_str() {
            "strict" => Ok(ClockWritePolicy::strict()),
            "lenient" => Ok(ClockWritePolicy::lenient()),
            other => Err(type_error(&format!(
                "invalid RINEX clock write policy {other:?}: expected \"strict\", \"lenient\" or an object"
            ))),
        };
    }
    reject_unknown_keys(
        value,
        "RINEX clock write policy",
        &["nearestMicrosecondEpochs"],
    )?;
    let axis = get_property(value, "nearestMicrosecondEpochs")?;
    let leniency = if axis.is_undefined() || axis.is_null() {
        ClockWriteLeniency::Strict
    } else {
        match axis.as_string().as_deref() {
            Some("strict") => ClockWriteLeniency::Strict,
            Some("allow") => ClockWriteLeniency::Allow,
            _ => {
                return Err(type_error(
                    "policy.nearestMicrosecondEpochs must be \"strict\" or \"allow\"",
                ))
            }
        }
    };
    Ok(ClockWritePolicy::strict().with_nearest_microsecond_epochs(leniency))
}

// --- ClockEpoch ------------------------------------------------------------------

/// A civil clock epoch: calendar fields and a second, interpreted in the time
/// scale of the product it is used with. A UTC product accepts `60.x` on a day
/// that ends with a positive leap second.
#[wasm_bindgen]
pub struct ClockEpoch {
    inner: CoreClockEpoch,
}

#[wasm_bindgen]
impl ClockEpoch {
    /// Build a civil clock epoch. Throws a `RangeError` for fields that name
    /// no civil epoch in any time scale; a `23:59:60` label is accepted on a
    /// day that ends with a positive leap second, and a product in a
    /// continuous scale refuses it when queried. The second is read as the
    /// shortest decimal of the number given, with every digit kept.
    #[wasm_bindgen(constructor)]
    pub fn new(
        year: i32,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: f64,
    ) -> Result<ClockEpoch, JsValue> {
        // UTC accepts every label a continuous scale accepts and the leap
        // second labels besides, so it is the widest civil check.
        civil_to_clock_instant(CoreTimeScale::Utc, year, month, day, hour, minute, second)
            .ok_or_else(|| range_error("invalid clock epoch fields"))?;
        Ok(ClockEpoch {
            inner: CoreClockEpoch {
                year,
                month,
                day,
                hour,
                minute,
                second,
            },
        })
    }

    #[wasm_bindgen(getter)]
    pub fn year(&self) -> i32 {
        self.inner.year
    }
    #[wasm_bindgen(getter)]
    pub fn month(&self) -> u8 {
        self.inner.month
    }
    #[wasm_bindgen(getter)]
    pub fn day(&self) -> u8 {
        self.inner.day
    }
    #[wasm_bindgen(getter)]
    pub fn hour(&self) -> u8 {
        self.inner.hour
    }
    #[wasm_bindgen(getter)]
    pub fn minute(&self) -> u8 {
        self.inner.minute
    }
    #[wasm_bindgen(getter)]
    pub fn second(&self) -> f64 {
        self.inner.second
    }

    /// Seconds since the GPS epoch, 1980-01-06 00:00:00 GPST, reading the
    /// fields in GPS time; `undefined` for a label GPS time does not have
    /// (a `60` second).
    #[wasm_bindgen(getter, js_name = gpsSeconds)]
    pub fn gps_seconds(&self) -> Option<f64> {
        civil_to_gps_seconds(
            self.inner.year,
            self.inner.month,
            self.inner.day,
            self.inner.hour,
            self.inner.minute,
            self.inner.second,
        )
    }
}

// --- ClockSeries -----------------------------------------------------------------

/// One satellite's clock-bias samples, derived from the `AS` records whose
/// epoch resolves to an instant, strictly time-ordered. Every array is
/// index-aligned to the samples.
#[wasm_bindgen]
pub struct ClockSeries {
    satellite: String,
    points: Vec<CoreClockPoint>,
}

#[wasm_bindgen]
impl ClockSeries {
    /// RINEX satellite token such as `"G05"`.
    #[wasm_bindgen(getter)]
    pub fn satellite(&self) -> String {
        self.satellite.clone()
    }

    /// Number of clock samples for this satellite.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> usize {
        self.points.len()
    }

    /// Time scale of the samples, or `undefined` for an empty series.
    #[wasm_bindgen(getter, js_name = timeScale)]
    pub fn time_scale(&self) -> Option<TimeScale> {
        self.points.first().map(|point| point.epoch.scale.into())
    }

    /// Sample times as GPS seconds, a `Float64Array` holding `NaN` where a
    /// sample's scale does not project onto GPS time (`hasGpsSeconds` is 0);
    /// GPST and QZSST samples project.
    #[wasm_bindgen(getter, js_name = gpsSeconds)]
    pub fn gps_seconds(&self) -> Vec<f64> {
        self.points
            .iter()
            .map(|point| point.gps_seconds().unwrap_or(f64::NAN))
            .collect()
    }

    /// 1 where `gpsSeconds` holds the sample's GPS seconds, 0 where it holds
    /// `NaN`.
    #[wasm_bindgen(getter, js_name = hasGpsSeconds)]
    pub fn has_gps_seconds(&self) -> Vec<u8> {
        self.points
            .iter()
            .map(|point| u8::from(point.gps_seconds().is_some()))
            .collect()
    }

    /// Whole part of each sample's split Julian date in its own scale, a
    /// `Float64Array`; `NaN` for an instant held as a nanosecond count, which
    /// `epochs` states exactly.
    #[wasm_bindgen(getter, js_name = jdWhole)]
    pub fn jd_whole(&self) -> Vec<f64> {
        self.points
            .iter()
            .map(|point| point.epoch.julian_date().map_or(f64::NAN, |jd| jd.jd_whole))
            .collect()
    }

    /// Day fraction of each sample's split Julian date, relative to `jdWhole`.
    #[wasm_bindgen(getter, js_name = jdFraction)]
    pub fn jd_fraction(&self) -> Vec<f64> {
        self.points
            .iter()
            .map(|point| point.epoch.julian_date().map_or(f64::NAN, |jd| jd.fraction))
            .collect()
    }

    /// Each sample's instant exactly as the engine holds it.
    #[wasm_bindgen(getter, unchecked_return_type = "RinexClockInstant[]")]
    pub fn epochs(&self) -> Result<JsValue, JsValue> {
        let epochs: Vec<InstantJs> = self
            .points
            .iter()
            .map(|point| InstantJs::from(point.epoch))
            .collect();
        to_plain_js(&epochs, "RINEX clock series epochs")
    }

    /// Satellite clock-bias samples, seconds, as a `Float64Array`.
    #[wasm_bindgen(getter, js_name = biasS)]
    pub fn bias_s(&self) -> Vec<f64> {
        self.points.iter().map(|point| point.bias_s).collect()
    }

    /// The declared values after the bias for each sample, in RINEX order:
    /// bias sigma, rate, rate sigma, acceleration, acceleration sigma. A
    /// record's values beyond its declared count are in `records()`.
    #[wasm_bindgen(getter, js_name = additionalValues, unchecked_return_type = "number[][]")]
    pub fn additional_values(&self) -> Result<JsValue, JsValue> {
        let values: Vec<&[f64]> = self
            .points
            .iter()
            .map(|point| point.additional_values.as_slice())
            .collect();
        to_plain_js(&values, "RINEX clock additional values")
    }
}

// --- RinexClock ------------------------------------------------------------------

/// A RINEX clock product.
#[wasm_bindgen]
pub struct RinexClock {
    inner: CoreRinexClock,
}

impl RinexClock {
    fn record_objects(&self) -> Vec<RecordJs> {
        self.inner
            .records()
            .enumerate()
            .map(|(index, record)| record_js(&self.inner, index, &record))
            .collect()
    }
}

#[wasm_bindgen]
impl RinexClock {
    /// Declared format version; for a product built from rows, the version it
    /// is written in. `undefined` when none is declared.
    #[wasm_bindgen(getter)]
    pub fn version(&self) -> Option<f64> {
        self.inner.version()
    }

    /// Column layout records are read and written in: `"v300"` (the 80-column
    /// layout before 3.04) or `"v304"` (85 columns); `undefined` when the text
    /// declares no version, in which case records are read at the 3.00
    /// columns first and written in them.
    #[wasm_bindgen(getter, unchecked_return_type = "\"v300\" | \"v304\" | undefined")]
    pub fn layout(&self) -> JsValue {
        self.inner.layout().map_or(JsValue::UNDEFINED, |layout| {
            JsValue::from_str(layout_label(layout))
        })
    }

    /// Satellite system code of `RINEX VERSION / TYPE`, or `undefined`.
    #[wasm_bindgen(getter, js_name = satelliteSystem)]
    pub fn satellite_system(&self) -> Option<String> {
        self.inner.satellite_system().map(String::from)
    }

    /// The time system label (`"GPS"`, `"GLO"`, `"GAL"`, `"QZS"`, `"BDS"`,
    /// `"IRN"`, `"UTC"`, `"TAI"`) when one is declared, defaulted or built in.
    #[wasm_bindgen(getter, js_name = timeSystem)]
    pub fn time_system(&self) -> Option<String> {
        self.inner
            .time_system()
            .map(|system| system.label().to_string())
    }

    /// How the time system was established: `{ kind: "DECLARED" }`,
    /// `"DEFAULTED"` (no `TIME SYSTEM ID`; the 3.00 default applied),
    /// `{ kind: "UNRECOGNIZED", label }`, `{ kind: "CONFLICTING", labels }`,
    /// or `"CONSTRUCTED"` for a product built from rows.
    #[wasm_bindgen(getter, js_name = timeSystemStatus, unchecked_return_type = "RinexClockTimeSystemStatus")]
    pub fn time_system_status(&self) -> Result<JsValue, JsValue> {
        to_plain_js(
            &TimeSystemStatusJs::from(self.inner.time_system_status()),
            "RINEX clock time system status",
        )
    }

    /// The time scale record epochs are interpreted in; `undefined` when the
    /// time system is missing, unrecognised, conflicting or has no core scale
    /// (`IRN`). `GLO` epochs are UTC.
    #[wasm_bindgen(getter, js_name = timeScale)]
    pub fn time_scale(&self) -> Option<TimeScale> {
        self.inner.time_scale().map(TimeScale::from)
    }

    /// Every header line in order with its exact text and typed reading. A
    /// product built from rows has none; its header is written from its time
    /// scale.
    #[wasm_bindgen(js_name = headerRecords, unchecked_return_type = "RinexClockHeaderRecord[]")]
    pub fn header_records(&self) -> Result<JsValue, JsValue> {
        let records: Vec<HeaderRecordJs> = self
            .inner
            .header_records()
            .iter()
            .map(HeaderRecordJs::from)
            .collect();
        to_plain_js(&records, "RINEX clock header records")
    }

    /// Every data record in order, `AR`, `AS`, `CR`, `DR` and `MS` alike,
    /// including records repeated for one name and epoch. `index` is the
    /// position the edit methods take. `sourceLines` holds the record's lines
    /// exactly as read, the seconds text included; it is empty for a record
    /// built or edited through this API.
    #[wasm_bindgen(unchecked_return_type = "RinexClockRecord[]")]
    pub fn records(&self) -> Result<JsValue, JsValue> {
        to_plain_js(&self.record_objects(), "RINEX clock records")
    }

    /// Number of data records.
    #[wasm_bindgen(getter, js_name = recordCount)]
    pub fn record_count(&self) -> usize {
        self.inner.record_count()
    }

    /// One line of the text the product was read from, by one-based line
    /// number, without its terminator; `undefined` past the last line.
    #[wasm_bindgen(js_name = sourceLine)]
    pub fn source_line(&self, line: f64) -> Result<Option<String>, JsValue> {
        let line = index_arg(line, "line")?;
        Ok(self.inner.source_line(line).map(str::to_string))
    }

    /// Satellite tokens with at least one clock sample.
    #[wasm_bindgen(getter)]
    pub fn satellites(&self) -> Vec<String> {
        self.inner.series().keys().cloned().collect()
    }

    /// Per-satellite clock-bias series in satellite sort order, in every time
    /// scale: a sample off the GPS timeline is kept with `NaN` GPS seconds.
    #[wasm_bindgen(getter)]
    pub fn series(&self) -> Vec<ClockSeries> {
        self.inner
            .series()
            .iter()
            .map(|(satellite, points)| ClockSeries {
                satellite: satellite.clone(),
                points: points.clone(),
            })
            .collect()
    }

    /// Number of satellites with clock samples.
    #[wasm_bindgen(getter, js_name = satelliteCount)]
    pub fn satellite_count(&self) -> usize {
        self.inner.series().len()
    }

    /// Total number of satellite clock samples.
    #[wasm_bindgen(getter, js_name = sampleCount)]
    pub fn sample_count(&self) -> usize {
        self.inner.series().values().map(Vec::len).sum()
    }

    /// One satellite's clock series, or `undefined` if the satellite is absent.
    #[wasm_bindgen(js_name = seriesFor)]
    pub fn series_for(&self, satellite_id: &str) -> Option<ClockSeries> {
        self.inner
            .series()
            .get(satellite_id)
            .map(|points| ClockSeries {
                satellite: satellite_id.to_string(),
                points: points.clone(),
            })
    }

    /// Records read from the source that are not in the satellite series
    /// (`AR`, `CR`, `DR`, `MS`, and `AS` records without an instant), each
    /// `{ line, recordType }`. The records themselves are in `records()`.
    #[wasm_bindgen(getter, js_name = skippedRecords, unchecked_return_type = "RinexClockSkip[]")]
    pub fn skipped_records(&self) -> Result<JsValue, JsValue> {
        let rows: Vec<SkipJs> = self
            .inner
            .skipped_records()
            .iter()
            .map(|skip| SkipJs {
                line: skip.line,
                record_type: skip.record_type.clone(),
            })
            .collect();
        to_plain_js(&rows, "RINEX clock skipped records")
    }

    /// Lines a lossy read kept without reading them as records, and header
    /// time-system errors, each `{ line, error }` with `error` a
    /// `RinexClockErrorDetail`. A lossy read writes these lines back
    /// unchanged.
    #[wasm_bindgen(getter, unchecked_return_type = "RinexClockDiagnostic[]")]
    pub fn diagnostics(&self) -> Result<JsValue, JsValue> {
        let rows: Vec<DiagnosticJs> = self
            .inner
            .diagnostics()
            .iter()
            .map(|diagnostic| DiagnosticJs {
                line: diagnostic.line,
                error: RinexClockErrorDetailJs::from(&diagnostic.error),
            })
            .collect();
        to_plain_js(&rows, "RINEX clock diagnostics")
    }

    /// Findings about how the product was read that do not stop it being
    /// read, as `RinexClockNotice` objects with the engine's `message`.
    #[wasm_bindgen(getter, unchecked_return_type = "RinexClockNotice[]")]
    pub fn notices(&self) -> Result<JsValue, JsValue> {
        let rows: Vec<NoticeJs> = self.inner.notices().iter().map(NoticeJs::from).collect();
        to_plain_js(&rows, "RINEX clock notices")
    }

    /// Interpolate one satellite clock bias at a civil epoch in this product's
    /// time scale. Returns `undefined` if the satellite or epoch coverage is
    /// absent. On a UTC product a `23:59:60.x` label on a leap-second day is a
    /// valid query. A product whose time system resolves to no scale, or an
    /// epoch the scale does not have, throws a `RinexClockQueryError`.
    #[wasm_bindgen(js_name = clockS)]
    pub fn clock_s(&self, satellite_id: &str, epoch: &ClockEpoch) -> Result<Option<f64>, JsValue> {
        self.inner
            .clock_s(satellite_id, epoch.inner)
            .map_err(query_error)
    }

    /// Interpolate one satellite clock bias at GPS seconds; GPST and QZSST
    /// series answer. Throws a `RangeError` if `gpsSeconds` is non-finite and
    /// a `RinexClockQueryError` outside the civil years 1 through 9999.
    #[wasm_bindgen(js_name = clockSAtGpsSeconds)]
    pub fn clock_s_at_gps_seconds(
        &self,
        satellite_id: &str,
        gps_seconds: f64,
    ) -> Result<Option<f64>, JsValue> {
        if !gps_seconds.is_finite() {
            return Err(range_error("gpsSeconds must be a finite number"));
        }
        self.inner
            .clock_s_at_gps_seconds(satellite_id, gps_seconds)
            .map_err(query_error)
    }

    /// Write the product as RINEX clock text under the strict policy.
    ///
    /// An unedited product read from text is restated byte for byte, line
    /// terminators included. Records built or edited through this API are
    /// written in the product's layout; a value no 19-column field states
    /// exactly, or an epoch no microsecond seconds text states exactly, is
    /// refused with a `RinexClockWriteError`, never rounded.
    #[wasm_bindgen(js_name = toRinexString)]
    pub fn to_rinex_string(&self) -> Result<String, JsValue> {
        self.inner.to_rinex_string().map_err(write_error)
    }

    /// Write under a policy, returning `{ text, value, departures }` with
    /// every departure the policy let the writer emit; `value` is the same
    /// string as `text`. `policy` is `"strict"`, `"lenient"`, or
    /// `{ nearestMicrosecondEpochs: "strict" | "allow" }`, or omitted for
    /// strict. With `nearestMicrosecondEpochs` allowed, an epoch no microsecond
    /// text states exactly is written at the nearest microsecond and reported
    /// as `EPOCH_AT_NEAREST_MICROSECOND` with the record index, name, epoch and
    /// the text written. Values are never approximated under any policy.
    #[wasm_bindgen(js_name = toRinexStringWithPolicy, unchecked_return_type = "RinexClockWriteResult")]
    pub fn to_rinex_string_with_policy(
        &self,
        #[wasm_bindgen(unchecked_optional_param_type = "RinexClockWritePolicyLike")]
        policy: JsValue,
    ) -> Result<JsValue, JsValue> {
        let policy = write_policy_from_js(&policy)?;
        let (text, departures) = self
            .inner
            .to_rinex_string_with_policy(policy)
            .map_err(write_error)?;
        let text = JsValue::from_str(&text);
        let departures: Vec<DepartureJs> = departures.iter().map(DepartureJs::from).collect();
        let departures = to_plain_js(&departures, "RINEX clock write departures")?;
        result_object(
            &[
                ("text", &text),
                ("value", &text),
                ("departures", &departures),
            ],
            "RINEX clock write result",
        )
    }

    /// Declare the product's time system by label (`"GPS"`, `"GLO"`,
    /// `"GAL"`, `"QZS"`, `"BDS"` or `"BDT"`, `"IRN"`, `"UTC"`, `"TAI"`).
    ///
    /// Replaces every `TIME SYSTEM ID` record with one written at the columns
    /// of the product's layout, or inserts one where Table A15 orders it. Every
    /// record epoch is checked in the new system first; if one does not
    /// convert, nothing changes and a `RinexClockEditError` is thrown. A
    /// product with no header section, or built from rows, is refused. An
    /// unknown label is a `TypeError`.
    #[wasm_bindgen(js_name = setTimeSystem)]
    pub fn set_time_system(&mut self, label: &str) -> Result<(), JsValue> {
        let system = time_system_from_label(label)?;
        self.inner.set_time_system(system).map_err(edit_error)
    }

    /// Replace the declared values, bias first, of the record at `index` in
    /// `records()` order. The record keeps its type, name and epoch, including
    /// the exact text of its seconds field. An edit the writer would refuse,
    /// or one that would drop values the record carries beyond its declared
    /// count, throws a `RinexClockEditError` and changes nothing.
    #[wasm_bindgen(js_name = setRecordValues)]
    pub fn set_record_values(
        &mut self,
        index: f64,
        #[wasm_bindgen(unchecked_param_type = "ArrayLike<number>")] values: JsValue,
    ) -> Result<(), JsValue> {
        let index = index_arg(index, "index")?;
        let values = numbers_from_js(&values, "values")?;
        self.inner
            .set_record_values(index, values)
            .map_err(edit_error)
    }

    /// Insert a record before the record at `index` in `records()` order, or
    /// after the last when `index` equals `recordCount`.
    ///
    /// `record` is `{ recordType, name, epoch, values }`: `recordType` is
    /// `"AR"`, `"AS"`, `"CR"`, `"DR"` or `"MS"`; `name` is a satellite token
    /// for `AS` and a receiver name of at most nine characters otherwise;
    /// `epoch` is a `ClockEpoch` or an object with its six fields, in the
    /// product's time scale; `values` holds the bias and up to five further
    /// values in RINEX order. A record the product could not write, or an
    /// epoch its time scale does not have, throws a `RinexClockEditError` and
    /// changes nothing.
    #[wasm_bindgen(js_name = insertRecord)]
    pub fn insert_record(
        &mut self,
        index: f64,
        #[wasm_bindgen(unchecked_param_type = "RinexClockRecordInput")] record: JsValue,
    ) -> Result<(), JsValue> {
        let index = index_arg(index, "index")?;
        reject_unknown_keys(
            &record,
            "RINEX clock record",
            &["recordType", "name", "epoch", "values"],
        )?;
        let record_type =
            record_type_from_code(&string_property(&record, "record", "recordType")?)?;
        let name = string_property(&record, "record", "name")?;
        let epoch = civil_epoch_from_js(&get_property(&record, "epoch")?, "record.epoch")?;
        let values = numbers_from_js(&get_property(&record, "values")?, "record.values")?;
        let record = CoreClockRecord::new(record_type, &name, epoch, values).map_err(edit_error)?;
        self.inner.insert_record(index, record).map_err(edit_error)
    }

    /// Remove the record at `index` in `records()` order with every line it
    /// spans, returning it as a `RinexClockRecord`.
    #[wasm_bindgen(js_name = removeRecord, unchecked_return_type = "RinexClockRecord")]
    pub fn remove_record(&mut self, index: f64) -> Result<JsValue, JsValue> {
        let index = index_arg(index, "index")?;
        let record = self.inner.remove_record(index).map_err(edit_error)?;
        to_plain_js(
            &record_js(&self.inner, index, &record),
            "removed RINEX clock record",
        )
    }

    /// Keep the records `keep` returns a truthy value for and remove every
    /// other, with every line it spans; returns the number removed. `keep`
    /// receives each `RinexClockRecord` in order. Blank and unread lines stay.
    /// If `keep` throws, nothing changes and its exception is rethrown.
    #[wasm_bindgen(js_name = retainRecords)]
    pub fn retain_records(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "(record: RinexClockRecord) => unknown")]
        keep: &js_sys::Function,
    ) -> Result<usize, JsValue> {
        let mut decisions = Vec::with_capacity(self.inner.record_count());
        for record in self.record_objects() {
            let record = to_plain_js(&record, "RINEX clock record")?;
            decisions.push(keep.call1(&JsValue::NULL, &record)?.is_truthy());
        }
        let mut next = decisions.into_iter();
        Ok(self.inner.retain_records(|_| next.next().unwrap_or(true)))
    }

    /// Replace the declared values of every record for which `edit` returns
    /// new values (an array or `Float64Array`, bias first); `undefined` or
    /// `null` leaves a record as it is. Returns the number edited. The whole
    /// batch is checked before anything changes: a refused edit throws a
    /// `RinexClockEditError` and none is applied. If `edit` throws, nothing
    /// changes and its exception is rethrown.
    #[wasm_bindgen(js_name = editRecords)]
    pub fn edit_records(
        &mut self,
        #[wasm_bindgen(
            unchecked_param_type = "(record: RinexClockRecord) => ArrayLike<number> | null | undefined"
        )]
        edit: &js_sys::Function,
    ) -> Result<usize, JsValue> {
        let mut edits = Vec::with_capacity(self.inner.record_count());
        for record in self.record_objects() {
            let index = record.index;
            let record = to_plain_js(&record, "RINEX clock record")?;
            let answer = edit.call1(&JsValue::NULL, &record)?;
            edits.push(if answer.is_undefined() || answer.is_null() {
                None
            } else {
                Some(numbers_from_js(
                    &answer,
                    &format!("the values returned for record {index}"),
                )?)
            });
        }
        let mut next = edits.into_iter();
        self.inner
            .edit_records(|_| next.next().flatten())
            .map_err(edit_error)
    }

    /// Build a product from per-satellite clock points in `timeScale`.
    ///
    /// `rows` is an array of `{ satellite, points }`, each point
    /// `{ epoch, biasS, additionalValues? }` with `epoch` a `ClockEpoch` or an
    /// object with its six fields, read in `timeScale`, and `additionalValues`
    /// the declared values after the bias. Each satellite's points must be
    /// strictly increasing. The product is written with a header stating its
    /// version, time system and data types; a scale no RINEX clock time system
    /// names (GLONASS system time among them, since `GLO` names UTC hours)
    /// throws a `RinexClockWriteError` when written. An epoch the scale does
    /// not have is a `RangeError`; a point the engine refuses throws a
    /// `RinexClockBuildError`.
    #[wasm_bindgen(js_name = fromClockPoints)]
    pub fn from_clock_points(
        time_scale: TimeScale,
        #[wasm_bindgen(unchecked_param_type = "RinexClockPointRowInput[]")] rows: JsValue,
    ) -> Result<RinexClock, JsValue> {
        let scale: CoreTimeScale = time_scale.into();
        let rows = array_from_js(&rows, "rows")?;
        let mut core_rows = Vec::with_capacity(rows.length() as usize);
        for (row_index, row) in rows.iter().enumerate() {
            let path = format!("rows[{row_index}]");
            reject_unknown_keys(&row, &path, &["satellite", "points"])?;
            let satellite = string_property(&row, &path, "satellite")?;
            let points = array_from_js(&get_property(&row, "points")?, &format!("{path}.points"))?;
            let mut core_points = Vec::with_capacity(points.length() as usize);
            for (point_index, point) in points.iter().enumerate() {
                let point_path = format!("{path}.points[{point_index}]");
                reject_unknown_keys(&point, &point_path, &["epoch", "biasS", "additionalValues"])?;
                let civil = civil_epoch_from_js(
                    &get_property(&point, "epoch")?,
                    &format!("{point_path}.epoch"),
                )?;
                let epoch = civil_to_clock_instant(
                    scale,
                    civil.year,
                    civil.month,
                    civil.day,
                    civil.hour,
                    civil.minute,
                    civil.second,
                )
                .ok_or_else(|| {
                    range_error(&format!(
                        "{point_path}.epoch names no epoch in {}",
                        scale.abbrev()
                    ))
                })?;
                let bias_s = get_property(&point, "biasS")?
                    .as_f64()
                    .ok_or_else(|| type_error(&format!("{point_path}.biasS must be a number")))?;
                let additional = get_property(&point, "additionalValues")?;
                let additional_values = if additional.is_undefined() || additional.is_null() {
                    Vec::new()
                } else {
                    numbers_from_js(&additional, &format!("{point_path}.additionalValues"))?
                };
                core_points.push(CoreClockPoint::new(epoch, bias_s, additional_values));
            }
            core_rows.push((satellite, core_points));
        }
        CoreRinexClock::from_clock_points(scale, core_rows)
            .map(|inner| RinexClock { inner })
            .map_err(build_error)
    }

    /// Build a GPST product from GPS-second rows: an array of
    /// `{ satellite, gpsSeconds, biasS }` with index-aligned arrays, the form
    /// `seriesFor(...).gpsSeconds` and `.biasS` export. Seconds must be
    /// strictly increasing within a row; a refused row throws a
    /// `RinexClockBuildError`.
    #[wasm_bindgen(js_name = fromSeriesRows)]
    pub fn from_series_rows(
        #[wasm_bindgen(unchecked_param_type = "RinexClockSeriesRowInput[]")] rows: JsValue,
    ) -> Result<RinexClock, JsValue> {
        let rows = array_from_js(&rows, "rows")?;
        let mut core_rows = Vec::with_capacity(rows.length() as usize);
        for (row_index, row) in rows.iter().enumerate() {
            let path = format!("rows[{row_index}]");
            reject_unknown_keys(&row, &path, &["satellite", "gpsSeconds", "biasS"])?;
            let satellite = string_property(&row, &path, "satellite")?;
            let seconds = numbers_from_js(
                &get_property(&row, "gpsSeconds")?,
                &format!("{path}.gpsSeconds"),
            )?;
            let biases = numbers_from_js(&get_property(&row, "biasS")?, &format!("{path}.biasS"))?;
            if seconds.len() != biases.len() {
                return Err(type_error(&format!(
                    "{path}.gpsSeconds has {} values but {path}.biasS has {}",
                    seconds.len(),
                    biases.len()
                )));
            }
            core_rows.push((satellite, seconds.into_iter().zip(biases).collect()));
        }
        CoreRinexClock::from_series_rows(core_rows)
            .map(|inner| RinexClock { inner })
            .map_err(build_error)
    }
}

/// Strictly parse RINEX clock bytes. Throws a `TypeError` on non-UTF-8 input
/// and a `RinexClockParseError` whose `detail` is a `RinexClockErrorDetail` on
/// the first line that does not read.
#[wasm_bindgen(js_name = parseRinexClock)]
pub fn parse_rinex_clock(bytes: &[u8]) -> Result<RinexClock, JsValue> {
    let text = utf8_text(bytes, "RINEX clock source")?;
    Ok(RinexClock {
        inner: CoreRinexClock::parse(&text).map_err(parse_error)?,
    })
}

/// Alias of [`parseRinexClock`] for callers that read a file as bytes.
#[wasm_bindgen(js_name = loadRinexClock)]
pub fn load_rinex_clock(bytes: &[u8]) -> Result<RinexClock, JsValue> {
    parse_rinex_clock(bytes)
}

/// Parse RINEX clock bytes, keeping lines that do not read verbatim with a
/// diagnostic in `diagnostics`. Nothing is dropped: `toRinexString()` on the
/// result restates the input exactly. Throws a `TypeError` only on non-UTF-8
/// input.
#[wasm_bindgen(js_name = parseRinexClockLossy)]
pub fn parse_rinex_clock_lossy(bytes: &[u8]) -> Result<RinexClock, JsValue> {
    let text = utf8_text(bytes, "RINEX clock source")?;
    Ok(RinexClock {
        inner: CoreRinexClock::parse_lossy(&text),
    })
}

/// Alias of [`parseRinexClockLossy`] for callers that read a file as bytes.
#[wasm_bindgen(js_name = loadRinexClockLossy)]
pub fn load_rinex_clock_lossy(bytes: &[u8]) -> Result<RinexClock, JsValue> {
    parse_rinex_clock_lossy(bytes)
}

// --- TypeScript declarations -------------------------------------------------

// The plain-object shapes the RINEX clock entry points take and return, and
// the names their `unchecked_*_type` attributes resolve against. `wasm-pack`
// writes them into both `sidereon.d.ts` targets; `types/sidereon-extra.d.ts`
// re-exports them.
#[wasm_bindgen(typescript_custom_section)]
const TS_RINEX_CLOCK_DEFINITIONS: &str = r#"
/**
 * A civil epoch as its six fields. `second` is the nearest double to the
 * stated second; a record's exact seconds text is in its `sourceLines`.
 */
export interface RinexClockCivilEpoch {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
  second: number;
}

/**
 * A scale-tagged instant exactly as the engine holds it: a split Julian date
 * (`jdWhole` + `jdFraction`), or a nanosecond count as an exact decimal
 * string in `nanos`. `gpsSeconds` is null off the GPS timeline (GPST and
 * QZSST project onto it).
 */
export interface RinexClockInstant {
  scale: string;
  jdWhole: number | null;
  jdFraction: number | null;
  nanos: string | null;
  gpsSeconds: number | null;
}

export type RinexClockTimeSystemStatus =
  | { kind: "DECLARED" }
  | { kind: "DEFAULTED" }
  | { kind: "UNRECOGNIZED"; label: string }
  | { kind: "CONFLICTING"; labels: string[] }
  | { kind: "CONSTRUCTED" }
  | { kind: "UNKNOWN"; message: string };

/**
 * A typed header field. An engine `i64` is an exact decimal string beside a
 * `number` that is null where it is not exactly representable as one.
 */
export type RinexClockHeaderField =
  | { kind: "VERSION_TYPE"; version: number; fileType: string; satelliteSystem: string }
  | { kind: "PROGRAM_RUN_BY_DATE"; program: string; runBy: string; date: string }
  | { kind: "COMMENT"; text: string }
  | {
      kind: "OBSERVATION_TYPES";
      system: string | null;
      count: number | null;
      descriptors: string[];
    }
  | { kind: "TIME_SYSTEM"; label: string }
  | { kind: "LEAP_SECONDS"; seconds: string; secondsNumber: number | null }
  | { kind: "LEAP_SECONDS_GNSS"; seconds: string; secondsNumber: number | null }
  | { kind: "DCBS_APPLIED"; system: string; program: string; source: string }
  | { kind: "PCVS_APPLIED"; system: string; program: string; source: string }
  | { kind: "TYPES_OF_DATA"; count: number; types: string[] }
  | { kind: "STATION_NAME_NUM"; name: string; identifier: string }
  | { kind: "STATION_CLOCK_REF"; text: string }
  | { kind: "ANALYSIS_CENTER"; designator: string; name: string }
  | {
      kind: "CLOCK_REF_COUNT";
      count: number;
      start: RinexClockCivilEpoch | null;
      stop: RinexClockCivilEpoch | null;
    }
  | { kind: "ANALYSIS_CLOCK_REF"; name: string; identifier: string; constraintS: number | null }
  | { kind: "SOLUTION_STATION_COUNT"; count: number; frame: string }
  | {
      kind: "SOLUTION_STATION";
      name: string;
      identifier: string;
      xyzMm: [string, string, string];
      xyzMmNumber: [number | null, number | null, number | null];
    }
  | { kind: "SOLUTION_SATELLITE_COUNT"; count: number }
  | { kind: "PRN_LIST"; prns: string[] }
  | { kind: "END_OF_HEADER" }
  | { kind: "UNKNOWN"; message: string };

/**
 * One header line: its exact text, label and typed reading. `line` is null
 * for a line written by an edit; `field` is null when the fields do not read.
 */
export interface RinexClockHeaderRecord {
  line: number | null;
  text: string;
  label: string;
  labelColumn: number;
  payload: string;
  reading:
    | "columns"
    | "otherVersionColumns"
    | "whitespace"
    | "uninterpreted"
    | "unknownLabel"
    | (string & {});
  field: RinexClockHeaderField | null;
}

export type RinexClockRecordReading =
  | "columnsV300"
  | "columnsV304"
  | "whitespace"
  | "edited"
  | { kind: "columnsTrailingText"; layout: "v300" | "v304" }
  | (string & {});

/**
 * One data record. `values` are the declared values, bias first;
 * `surplusValues` are values beyond the declared count, each at its position
 * in the value sequence (0 bias, 1 bias sigma, ...). `epoch` is null when the
 * product's time system resolves to no scale. `line` and `sourceLines` are
 * null and empty for a record built or edited through the API.
 */
export interface RinexClockRecord {
  index: number;
  recordType: "AR" | "AS" | "CR" | "DR" | "MS";
  name: string;
  satellite: string | null;
  civilEpoch: RinexClockCivilEpoch;
  epoch: RinexClockInstant | null;
  declaredCount: number;
  values: number[];
  surplusValues: { position: number; value: number }[];
  line: number | null;
  lineCount: number;
  reading: RinexClockRecordReading;
  continuationReading: RinexClockRecordReading | null;
  sourceLines: string[];
}

/** A record read from the source that is not in the satellite series. */
export interface RinexClockSkip {
  line: number;
  recordType: string;
}

/**
 * The `detail` of a thrown `RinexClockParseError`, `RinexClockQueryError`,
 * `RinexClockWriteError`, `RinexClockEditError` or `RinexClockBuildError`.
 * A `line` is one-based.
 */
export type RinexClockErrorDetail =
  | { kind: "MALFORMED_AS_RECORD"; line: number; reason: string; record: string; message: string }
  | { kind: "MISSING_CONTINUATION"; line: number; recordType: string; message: string }
  | {
      kind: "MALFORMED_CONTINUATION";
      line: number;
      reason: string;
      record: string;
      message: string;
    }
  | { kind: "BAD_FIELD"; line: number; field: string; value: string; message: string }
  | { kind: "INVALID_INPUT"; field: string; reason: string; message: string }
  | { kind: "UNSUPPORTED_TIME_SCALE"; scale: string; message: string };

/** A line a lossy read kept without reading it, or a header time-system error. */
export interface RinexClockDiagnostic {
  line: number;
  error: RinexClockErrorDetail;
}

/** A finding about how a product was read that does not stop it being read. */
export type RinexClockNotice =
  | { kind: "TIME_SYSTEM_DEFAULTED"; system: string; message: string }
  | { kind: "TIME_SYSTEM_MISSING"; message: string }
  | { kind: "TIME_SYSTEM_WITHOUT_SCALE"; system: string; message: string }
  | { kind: "HEADER_RECORD_NONCONFORMING"; line: number; message: string }
  | { kind: "HEADER_RECORD_UNINTERPRETED"; line: number; message: string }
  | { kind: "HEADER_RECORD_UNKNOWN_LABEL"; line: number; message: string }
  | { kind: "SURPLUS_VALUES"; records: number; firstLine: number; message: string }
  | { kind: "OTHER_LAYOUT_RECORDS"; records: number; firstLine: number; message: string }
  | { kind: "WHITESPACE_RECORDS"; records: number; firstLine: number; message: string }
  | { kind: "TRAILING_TEXT_RECORDS"; records: number; firstLine: number; message: string }
  | { kind: "UNKNOWN"; message: string };

/** A departure the writer emitted under a write policy. */
export type RinexClockWriteDeparture =
  | {
      kind: "EPOCH_AT_NEAREST_MICROSECOND";
      record: number;
      name: string;
      epoch: RinexClockInstant | null;
      written: string;
      message: string;
    }
  | { kind: "UNKNOWN"; message: string };

export type RinexClockLeniency = "strict" | "allow";

export interface RinexClockWritePolicyInput {
  nearestMicrosecondEpochs?: RinexClockLeniency;
}

export type RinexClockWritePolicyLike = "strict" | "lenient" | RinexClockWritePolicyInput;

export interface RinexClockWriteResult {
  text: string;
  value: string;
  departures: RinexClockWriteDeparture[];
}

/** A record to insert; `epoch` is a `ClockEpoch` or its six fields. */
export interface RinexClockRecordInput {
  recordType: "AR" | "AS" | "CR" | "DR" | "MS";
  name: string;
  epoch: RinexClockCivilEpoch;
  values: ArrayLike<number>;
}

export interface RinexClockPointInput {
  epoch: RinexClockCivilEpoch;
  biasS: number;
  additionalValues?: ArrayLike<number> | null;
}

export interface RinexClockPointRowInput {
  satellite: string;
  points: RinexClockPointInput[];
}

export interface RinexClockSeriesRowInput {
  satellite: string;
  gpsSeconds: ArrayLike<number>;
  biasS: ArrayLike<number>;
}
"#;
