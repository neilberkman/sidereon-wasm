//! Typed errors for the CCSDS navigation data messages (OMM, OPM, OEM, CDM).
//!
//! Every reader and writer failure of these messages surfaces as an `Error`
//! named `OmmError`, `OpmError`, `OemError` or `CdmError` whose `message` is the
//! engine's text and whose `detail` is an [`NdmErrorDetail`]: a `kind` naming
//! the engine variant and the payload that variant carries.

use serde::Serialize;
use wasm_bindgen::JsValue;

use sidereon_core::astro::cdm::CdmError;
use sidereon_core::astro::oem::OemError;
use sidereon_core::astro::omm::OmmError;
use sidereon_core::astro::omm::TextIssue;
use sidereon_core::astro::opm::OpmError;

use crate::error::error_with_detail;

/// The JS label of a [`TextIssue`].
pub(crate) fn text_issue_label(issue: TextIssue) -> &'static str {
    match issue {
        TextIssue::LineBreak => "lineBreak",
        TextIssue::SurroundingWhitespace => "surroundingWhitespace",
        TextIssue::InteriorWhitespace => "interiorWhitespace",
        TextIssue::KeywordSeparator => "keywordSeparator",
        TextIssue::XmlIllegalCharacter => "xmlIllegalCharacter",
        TextIssue::Empty => "empty",
        TextIssue::DetachedComment => "detachedComment",
        TextIssue::RepeatedParameter => "repeatedParameter",
        TextIssue::CommentNotCarried => "commentNotCarried",
    }
}

/// The typed `detail` of a CCSDS message error. Fields a variant does not
/// carry are `null`.
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NdmErrorDetail {
    /// The engine variant, in upper snake case (`UNWRITABLE_TEXT`).
    kind: &'static str,
    /// The engine's message.
    message: String,
    /// The keyword, element, block or record field the error names.
    field: Option<String>,
    /// The text the error names: a refused value, an unknown object, a
    /// malformed line or comment, or a structural message.
    value: Option<String>,
    /// For `INVALID_FIELD`, the validation category; for `UNWRITABLE_TEXT`,
    /// why the text would not read back (`lineBreak`, ...).
    issue: Option<String>,
    /// One-based line number of a `MALFORMED_LINE`.
    line: Option<usize>,
    /// A stated unit of a `UNIT_MISMATCH`.
    unit: Option<String>,
    /// The unit the table defines for a `UNIT_MISMATCH`, or `null` for a
    /// dimensionless or text keyword.
    expected_unit: Option<&'static str>,
    /// The first and second values of a `DUPLICATE_FIELD`, or the two
    /// parameters of a `CSV_COLUMN_ORDER`.
    first: Option<String>,
    second: Option<String>,
    /// A record, object or message count.
    count: Option<usize>,
    /// The header column count of a `CSV_COLUMN_COUNT`.
    expected_count: Option<usize>,
    /// Zero-based record position of an `IN_RECORD`.
    index: Option<usize>,
    /// The failure of that record.
    source: Option<Box<NdmErrorDetail>>,
}

impl NdmErrorDetail {
    fn new(kind: &'static str, message: String) -> Self {
        Self {
            kind,
            message,
            ..Self::default()
        }
    }
}

fn unit_mismatch(
    message: String,
    field: &str,
    unit: &str,
    expected: Option<&'static str>,
) -> NdmErrorDetail {
    NdmErrorDetail {
        field: Some(field.to_owned()),
        unit: Some(unit.to_owned()),
        expected_unit: expected,
        ..NdmErrorDetail::new("UNIT_MISMATCH", message)
    }
}

fn duplicate(message: String, field: &str, first: &str, second: &str) -> NdmErrorDetail {
    NdmErrorDetail {
        field: Some(field.to_owned()),
        first: Some(first.to_owned()),
        second: Some(second.to_owned()),
        ..NdmErrorDetail::new("DUPLICATE_FIELD", message)
    }
}

fn unwritable(message: String, field: &str, value: &str, issue: TextIssue) -> NdmErrorDetail {
    NdmErrorDetail {
        field: Some(field.to_owned()),
        value: Some(value.to_owned()),
        issue: Some(text_issue_label(issue).to_owned()),
        ..NdmErrorDetail::new("UNWRITABLE_TEXT", message)
    }
}

fn malformed_line(message: String, line: usize, text: &str) -> NdmErrorDetail {
    NdmErrorDetail {
        line: Some(line),
        value: Some(text.to_owned()),
        ..NdmErrorDetail::new("MALFORMED_LINE", message)
    }
}

fn with_field(kind: &'static str, message: String, field: &str) -> NdmErrorDetail {
    NdmErrorDetail {
        field: Some(field.to_owned()),
        ..NdmErrorDetail::new(kind, message)
    }
}

fn with_value(kind: &'static str, message: String, value: &str) -> NdmErrorDetail {
    NdmErrorDetail {
        value: Some(value.to_owned()),
        ..NdmErrorDetail::new(kind, message)
    }
}

fn with_count(kind: &'static str, message: String, count: usize) -> NdmErrorDetail {
    NdmErrorDetail {
        count: Some(count),
        ..NdmErrorDetail::new(kind, message)
    }
}

fn invalid_field(message: String, field: &str, kind: String) -> NdmErrorDetail {
    NdmErrorDetail {
        field: Some(field.to_owned()),
        issue: Some(kind),
        ..NdmErrorDetail::new("INVALID_FIELD", message)
    }
}

/// The typed detail of an [`OmmError`].
pub(crate) fn omm_detail(err: &OmmError) -> NdmErrorDetail {
    let message = err.to_string();
    match err {
        OmmError::MissingField(field) => with_field("MISSING_FIELD", message, field),
        OmmError::InvalidField { field, kind } => invalid_field(message, field, kind.to_string()),
        OmmError::Field(text) => with_value("FIELD", message, text),
        OmmError::Epoch(text) => with_value("EPOCH", message, text),
        OmmError::DuplicateField {
            field,
            first,
            second,
        } => duplicate(message, field, first, second),
        OmmError::UnknownField(field) => with_field("UNKNOWN_FIELD", message, field),
        OmmError::CsvColumnCount { found, expected } => NdmErrorDetail {
            count: Some(*found),
            expected_count: Some(*expected),
            ..NdmErrorDetail::new("CSV_COLUMN_COUNT", message)
        },
        OmmError::CsvEmptyBlock(block) => with_field("CSV_EMPTY_BLOCK", message, block),
        OmmError::MalformedLine { line, text } => malformed_line(message, *line, text),
        OmmError::UnitMismatch {
            field,
            unit,
            expected,
        } => unit_mismatch(message, field, unit, *expected),
        OmmError::MultipleMessages { count } => with_count("MULTIPLE_MESSAGES", message, *count),
        OmmError::InRecord { index, source } => NdmErrorDetail {
            index: Some(*index),
            source: Some(Box::new(omm_detail(source))),
            ..NdmErrorDetail::new("IN_RECORD", message)
        },
        OmmError::CsvColumnOrder { first, second } => NdmErrorDetail {
            first: Some(first.clone()),
            second: Some(second.clone()),
            ..NdmErrorDetail::new("CSV_COLUMN_ORDER", message)
        },
        OmmError::IncompatibleMetadata { field, value } => NdmErrorDetail {
            field: Some((*field).to_owned()),
            value: Some(value.clone()),
            ..NdmErrorDetail::new("INCOMPATIBLE_METADATA", message)
        },
        OmmError::UnwritableText {
            field,
            value,
            issue,
        } => unwritable(message, field, value, *issue),
    }
}

/// The typed detail of an [`OpmError`].
pub(crate) fn opm_detail(err: &OpmError) -> NdmErrorDetail {
    let message = err.to_string();
    match err {
        OpmError::MissingField(field) => with_field("MISSING_FIELD", message, field),
        OpmError::InvalidField { field, kind } => invalid_field(message, field, kind.to_string()),
        OpmError::Field(text) => with_value("FIELD", message, text),
        OpmError::DuplicateField {
            field,
            first,
            second,
        } => duplicate(message, field, first, second),
        OpmError::UnitMismatch {
            field,
            unit,
            expected,
        } => unit_mismatch(message, field, unit, *expected),
        OpmError::MultipleMessages { count } => with_count("MULTIPLE_MESSAGES", message, *count),
        OpmError::UnknownField(field) => with_field("UNKNOWN_FIELD", message, field),
        OpmError::MalformedLine { line, text } => malformed_line(message, *line, text),
        OpmError::UnwritableText {
            field,
            value,
            issue,
        } => unwritable(message, field, value, *issue),
    }
}

/// The typed detail of an [`OemError`].
pub(crate) fn oem_detail(err: &OemError) -> NdmErrorDetail {
    let message = err.to_string();
    match err {
        OemError::MissingField(field) => with_field("MISSING_FIELD", message, field),
        OemError::InvalidField { field, kind } => invalid_field(message, field, kind.to_string()),
        OemError::Field(text) => with_value("FIELD", message, text),
        OemError::DuplicateField {
            field,
            first,
            second,
        } => duplicate(message, field, first, second),
        OemError::UnitMismatch {
            field,
            unit,
            expected,
        } => unit_mismatch(message, field, unit, *expected),
        OemError::MultipleMessages { count } => with_count("MULTIPLE_MESSAGES", message, *count),
        OemError::UnknownField(field) => with_field("UNKNOWN_FIELD", message, field),
        OemError::MalformedLine { line, text } => malformed_line(message, *line, text),
        OemError::UnwritableText {
            field,
            value,
            issue,
        } => unwritable(message, field, value, *issue),
    }
}

/// The typed detail of a [`CdmError`].
pub(crate) fn cdm_detail(err: &CdmError) -> NdmErrorDetail {
    let message = err.to_string();
    match err {
        CdmError::IncompleteStateVector => NdmErrorDetail::new("INCOMPLETE_STATE_VECTOR", message),
        CdmError::InvalidField { field, kind } => invalid_field(message, field, kind.to_string()),
        CdmError::MalformedXml(text) => with_value("MALFORMED_XML", message, text),
        CdmError::DuplicateField {
            field,
            first,
            second,
        } => duplicate(message, field, first, second),
        CdmError::UnitMismatch {
            field,
            unit,
            expected,
        } => unit_mismatch(message, field, unit, *expected),
        CdmError::UnexpectedObjectCount(count) => {
            with_count("UNEXPECTED_OBJECT_COUNT", message, *count)
        }
        CdmError::MultipleMessages { count } => with_count("MULTIPLE_MESSAGES", message, *count),
        CdmError::UnknownField(field) => with_field("UNKNOWN_FIELD", message, field),
        CdmError::MalformedLine { line, text } => malformed_line(message, *line, text),
        CdmError::UnknownObject(value) => with_value("UNKNOWN_OBJECT", message, value),
        CdmError::RepeatedObject(value) => with_value("REPEATED_OBJECT", message, value),
        CdmError::UnwritableText {
            field,
            value,
            issue,
        } => unwritable(message, field, value, *issue),
        CdmError::HardBodyRadiusComment { comment } => {
            with_value("HARD_BODY_RADIUS_COMMENT", message, comment)
        }
    }
}

/// An `OmmError` exception for `err`.
pub(crate) fn omm_error(err: OmmError) -> JsValue {
    error_with_detail("OmmError", &err.to_string(), &omm_detail(&err))
}

/// An `OpmError` exception for `err`.
pub(crate) fn opm_error(err: OpmError) -> JsValue {
    error_with_detail("OpmError", &err.to_string(), &opm_detail(&err))
}

/// An `OemError` exception for `err`.
pub(crate) fn oem_error(err: OemError) -> JsValue {
    error_with_detail("OemError", &err.to_string(), &oem_detail(&err))
}

/// A `CdmError` exception for `err`.
pub(crate) fn cdm_error(err: CdmError) -> JsValue {
    error_with_detail("CdmError", &err.to_string(), &cdm_detail(&err))
}
