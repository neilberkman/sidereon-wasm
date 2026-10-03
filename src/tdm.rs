//! CCSDS TDM binding: parse a Tracking Data Message in KVN form, inspect its
//! canonical blocks, build or replace a segment's metadata from ordered raw
//! fields and positioned comments, and encode it back through the core
//! serializer.
//!
//! Every rule lives in `sidereon_core::astro::tdm`. A metadata block's ordered
//! raw fields and positioned comments are its authority: the participants,
//! paths, mode, timetag reference, time system and range units read from it are
//! derived by the core from those fields whenever a block is built or replaced,
//! and nothing here reads or rewrites a field itself.

use serde::Serialize;
use wasm_bindgen::prelude::*;

use sidereon_core::astro::tdm::{
    encode_kvn, encode_kvn_with_policy, parse_kvn, parse_kvn_with_policy, Tdm as CoreTdm,
    TdmComment as CoreTdmComment, TdmDataRecord as CoreTdmDataRecord,
    TdmDataSection as CoreTdmDataSection, TdmDeparture as CoreTdmDeparture,
    TdmError as CoreTdmError, TdmField as CoreTdmField, TdmInputErrorKind, TdmLeniency,
    TdmMetadata as CoreTdmMetadata, TdmObservable, TdmParticipant as CoreTdmParticipant,
    TdmPath as CoreTdmPath, TdmPolicy, TdmScalar as CoreTdmScalar, TdmSegment as CoreTdmSegment,
    TdmWarning as CoreTdmWarning, TdmWritePolicy,
};

use crate::error::{
    error_with_detail, index_arg, range_error, reject_unknown_keys, result_object, to_plain_js,
    type_error,
};
use crate::label::{upper_snake_variant, Label};

// --- Positioned comments -----------------------------------------------------

/// One comment and the index of the field or record it precedes, as a plain
/// object `{ text, beforeRecord }`.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct TdmCommentJs {
    text: String,
    before_record: usize,
}

fn comments_to_js(comments: &[CoreTdmComment]) -> Result<JsValue, JsValue> {
    let comments: Vec<TdmCommentJs> = comments
        .iter()
        .map(|comment| TdmCommentJs {
            text: comment.text.clone(),
            before_record: comment.before_record,
        })
        .collect();
    to_plain_js(&comments, "TDM comments")
}

/// A string property of a supplied object, read through the prototype chain so
/// a class instance's getter reads as a plain object's own property does. What
/// the accessor throws is re-raised unchanged.
fn string_property(object: &JsValue, key: &str, context: &str) -> Result<String, JsValue> {
    js_sys::Reflect::get(object, &JsValue::from_str(key))?
        .as_string()
        .ok_or_else(|| type_error(&format!("{context}.{key} must be a string")))
}

/// Every property one supplied field may carry.
const TDM_FIELD_KEYS: &[&str] = &["key", "value"];
/// Every property one supplied comment may carry.
const TDM_COMMENT_KEYS: &[&str] = &["text", "beforeRecord"];

/// Ordered raw fields as supplied: an array of `{ key, value }` objects or
/// `TdmField` instances. The order is kept exactly; the core validates the
/// keys, values and their order.
fn fields_from_js(value: &JsValue) -> Result<Vec<CoreTdmField>, JsValue> {
    if !js_sys::Array::is_array(value) {
        return Err(type_error(
            "fields must be an array of { key, value } objects",
        ));
    }
    let array = js_sys::Array::from(value);
    let mut fields = Vec::with_capacity(array.length() as usize);
    for (index, item) in array.iter().enumerate() {
        let context = format!("fields[{index}]");
        reject_unknown_keys(&item, &context, TDM_FIELD_KEYS)?;
        fields.push(CoreTdmField {
            key: string_property(&item, "key", &context)?,
            value: string_property(&item, "value", &context)?,
        });
    }
    Ok(fields)
}

/// Positioned comments as supplied: an array of `{ text, beforeRecord }`
/// objects, or `undefined` / `null` for none. `beforeRecord` is read exactly:
/// a negative, fractional or non-finite position is a `RangeError` rather than
/// a comment moved to another place.
fn comments_from_js(value: &JsValue) -> Result<Vec<CoreTdmComment>, JsValue> {
    if value.is_undefined() || value.is_null() {
        return Ok(Vec::new());
    }
    if !js_sys::Array::is_array(value) {
        return Err(type_error(
            "comments must be an array of { text, beforeRecord } objects",
        ));
    }
    let array = js_sys::Array::from(value);
    let mut comments = Vec::with_capacity(array.length() as usize);
    for (index, item) in array.iter().enumerate() {
        let context = format!("comments[{index}]");
        reject_unknown_keys(&item, &context, TDM_COMMENT_KEYS)?;
        let text = string_property(&item, "text", &context)?;
        let position = js_sys::Reflect::get(&item, &JsValue::from_str("beforeRecord"))?;
        let position = position
            .as_f64()
            .ok_or_else(|| type_error(&format!("{context}.beforeRecord must be a number")))?;
        comments.push(CoreTdmComment {
            text,
            before_record: index_arg(position, &format!("{context}.beforeRecord"))?,
        });
    }
    Ok(comments)
}

// --- Policies ----------------------------------------------------------------

/// Every axis a reader policy object may name.
const TDM_POLICY_KEYS: &[&str] = &[
    "nonPrintable",
    "missingKeywords",
    "longLines",
    "emptyDataSections",
    "recordOrder",
    "duplicateRecords",
    "keywordOrder",
    "finalTerminator",
];

/// Every axis a writer policy object may name: the reader's, and
/// `repeatedKeywords`.
const TDM_WRITE_POLICY_KEYS: &[&str] = &[
    "nonPrintable",
    "missingKeywords",
    "longLines",
    "emptyDataSections",
    "recordOrder",
    "duplicateRecords",
    "keywordOrder",
    "finalTerminator",
    "repeatedKeywords",
];

/// One policy axis: `"strict"`, `"forgive"`, or absent.
fn leniency_property(policy: &JsValue, key: &str) -> Result<Option<TdmLeniency>, JsValue> {
    let value = js_sys::Reflect::get(policy, &JsValue::from_str(key))?;
    if value.is_undefined() || value.is_null() {
        return Ok(None);
    }
    match value.as_string().as_deref() {
        Some("strict") => Ok(Some(TdmLeniency::Strict)),
        Some("forgive") => Ok(Some(TdmLeniency::Forgive)),
        _ => Err(type_error(&format!(
            "policy.{key} must be \"strict\" or \"forgive\""
        ))),
    }
}

/// The preset a policy string names, or `None` for an object.
fn policy_preset(policy: &JsValue) -> Result<Option<bool>, JsValue> {
    if let Some(name) = policy.as_string() {
        return match name.as_str() {
            "strict" => Ok(Some(false)),
            "lenient" => Ok(Some(true)),
            _ => Err(type_error(&format!(
                "unknown TDM policy \"{name}\"; expected \"strict\", \"lenient\" or a policy object"
            ))),
        };
    }
    if !policy.is_object() {
        return Err(type_error(
            "policy must be \"strict\", \"lenient\", a policy object, null or undefined",
        ));
    }
    Ok(None)
}

/// The reader policy a JavaScript value states. `undefined` and `null` are the
/// strict default, which forgives nothing. An axis a policy object leaves out
/// stays strict; an axis it misspells is refused rather than ignored.
fn read_policy(policy: &JsValue) -> Result<TdmPolicy, JsValue> {
    if policy.is_undefined() || policy.is_null() {
        return Ok(TdmPolicy::strict());
    }
    match policy_preset(policy)? {
        Some(true) => return Ok(TdmPolicy::lenient()),
        Some(false) => return Ok(TdmPolicy::strict()),
        None => {}
    }
    reject_unknown_keys(policy, "TDM policy", TDM_POLICY_KEYS)?;
    let mut parsed = TdmPolicy::strict();
    if let Some(value) = leniency_property(policy, "nonPrintable")? {
        parsed = parsed.with_non_printable(value);
    }
    if let Some(value) = leniency_property(policy, "missingKeywords")? {
        parsed = parsed.with_missing_keywords(value);
    }
    if let Some(value) = leniency_property(policy, "longLines")? {
        parsed = parsed.with_long_lines(value);
    }
    if let Some(value) = leniency_property(policy, "emptyDataSections")? {
        parsed = parsed.with_empty_data_sections(value);
    }
    if let Some(value) = leniency_property(policy, "recordOrder")? {
        parsed = parsed.with_record_order(value);
    }
    if let Some(value) = leniency_property(policy, "duplicateRecords")? {
        parsed = parsed.with_duplicate_records(value);
    }
    if let Some(value) = leniency_property(policy, "keywordOrder")? {
        parsed = parsed.with_keyword_order(value);
    }
    if let Some(value) = leniency_property(policy, "finalTerminator")? {
        parsed = parsed.with_final_terminator(value);
    }
    Ok(parsed)
}

/// The writer policy a JavaScript value states, read as [`read_policy`] reads
/// a reader policy, with the writer's own `repeatedKeywords` axis.
fn write_policy(policy: &JsValue) -> Result<TdmWritePolicy, JsValue> {
    if policy.is_undefined() || policy.is_null() {
        return Ok(TdmWritePolicy::strict());
    }
    match policy_preset(policy)? {
        Some(true) => return Ok(TdmWritePolicy::lenient()),
        Some(false) => return Ok(TdmWritePolicy::strict()),
        None => {}
    }
    reject_unknown_keys(policy, "TDM write policy", TDM_WRITE_POLICY_KEYS)?;
    let mut parsed = TdmWritePolicy::strict();
    if let Some(value) = leniency_property(policy, "nonPrintable")? {
        parsed = parsed.with_non_printable(value);
    }
    if let Some(value) = leniency_property(policy, "missingKeywords")? {
        parsed = parsed.with_missing_keywords(value);
    }
    if let Some(value) = leniency_property(policy, "longLines")? {
        parsed = parsed.with_long_lines(value);
    }
    if let Some(value) = leniency_property(policy, "emptyDataSections")? {
        parsed = parsed.with_empty_data_sections(value);
    }
    if let Some(value) = leniency_property(policy, "recordOrder")? {
        parsed = parsed.with_record_order(value);
    }
    if let Some(value) = leniency_property(policy, "duplicateRecords")? {
        parsed = parsed.with_duplicate_records(value);
    }
    if let Some(value) = leniency_property(policy, "keywordOrder")? {
        parsed = parsed.with_keyword_order(value);
    }
    if let Some(value) = leniency_property(policy, "finalTerminator")? {
        parsed = parsed.with_final_terminator(value);
    }
    if let Some(value) = leniency_property(policy, "repeatedKeywords")? {
        parsed = parsed.with_repeated_keywords(value);
    }
    Ok(parsed)
}

// --- Warnings, departures and errors -----------------------------------------

/// A departure the reader forgave under a policy, as a discriminated union on
/// `kind`. A line is one-based; a segment is one-based, or `null` for the
/// header.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind")]
enum TdmWarningJs {
    #[serde(rename = "NON_PRINTABLE_CHARACTER", rename_all = "camelCase")]
    NonPrintableCharacter {
        line: usize,
        keyword: String,
        column: usize,
        character: char,
        message: String,
    },
    #[serde(rename = "LINE_TOO_LONG", rename_all = "camelCase")]
    LineTooLong {
        line: usize,
        keyword: String,
        length: usize,
        message: String,
    },
    #[serde(rename = "REPEATED_KEYWORD", rename_all = "camelCase")]
    RepeatedKeyword {
        line: usize,
        keyword: String,
        section: String,
        message: String,
    },
    #[serde(rename = "MISSING_KEYWORD", rename_all = "camelCase")]
    MissingKeyword {
        keyword: String,
        segment: Option<usize>,
        message: String,
    },
    #[serde(rename = "EMPTY_DATA_SECTION", rename_all = "camelCase")]
    EmptyDataSection { segment: usize, message: String },
    #[serde(rename = "RECORDS_OUT_OF_ORDER", rename_all = "camelCase")]
    RecordsOutOfOrder {
        segment: usize,
        keyword: String,
        epoch: String,
        message: String,
    },
    #[serde(rename = "UNTERMINATED_FINAL_LINE", rename_all = "camelCase")]
    UnterminatedFinalLine { line: usize, message: String },
    #[serde(rename = "KEYWORD_OUT_OF_ORDER", rename_all = "camelCase")]
    KeywordOutOfOrder {
        line: usize,
        keyword: String,
        section: String,
        message: String,
    },
    #[serde(rename = "DUPLICATE_RECORD", rename_all = "camelCase")]
    DuplicateRecord {
        segment: usize,
        keyword: String,
        epoch: String,
        message: String,
    },
    /// A warning this binding does not yet name, with the engine's message.
    #[serde(rename = "UNKNOWN", rename_all = "camelCase")]
    Unknown { message: String },
}

impl TdmWarningJs {
    fn from_core(warning: CoreTdmWarning) -> Self {
        let message = warning.to_string();
        match warning {
            CoreTdmWarning::NonPrintableCharacter {
                line,
                keyword,
                column,
                character,
            } => Self::NonPrintableCharacter {
                line,
                keyword,
                column,
                character,
                message,
            },
            CoreTdmWarning::LineTooLong {
                line,
                keyword,
                length,
            } => Self::LineTooLong {
                line,
                keyword,
                length,
                message,
            },
            CoreTdmWarning::RepeatedKeyword {
                line,
                keyword,
                section,
            } => Self::RepeatedKeyword {
                line,
                keyword,
                section: section.to_string(),
                message,
            },
            CoreTdmWarning::MissingKeyword { keyword, segment } => Self::MissingKeyword {
                keyword,
                segment,
                message,
            },
            CoreTdmWarning::EmptyDataSection { segment } => {
                Self::EmptyDataSection { segment, message }
            }
            CoreTdmWarning::RecordsOutOfOrder {
                segment,
                keyword,
                epoch,
            } => Self::RecordsOutOfOrder {
                segment,
                keyword,
                epoch,
                message,
            },
            CoreTdmWarning::UnterminatedFinalLine { line } => {
                Self::UnterminatedFinalLine { line, message }
            }
            CoreTdmWarning::KeywordOutOfOrder {
                line,
                keyword,
                section,
            } => Self::KeywordOutOfOrder {
                line,
                keyword,
                section: section.to_string(),
                message,
            },
            CoreTdmWarning::DuplicateRecord {
                segment,
                keyword,
                epoch,
            } => Self::DuplicateRecord {
                segment,
                keyword,
                epoch,
                message,
            },
            // `TdmWarning` is `#[non_exhaustive]`.
            _ => Self::Unknown { message },
        }
    }
}

/// A departure from CCSDS 503.0-B-2 the writer emitted under a write policy,
/// as a discriminated union on `kind`.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind")]
enum TdmDepartureJs {
    #[serde(rename = "NON_PRINTABLE_CHARACTER", rename_all = "camelCase")]
    NonPrintableCharacter {
        keyword: String,
        character: char,
        message: String,
    },
    #[serde(rename = "LINE_TOO_LONG", rename_all = "camelCase")]
    LineTooLong {
        keyword: String,
        length: usize,
        message: String,
    },
    #[serde(rename = "MISSING_KEYWORD", rename_all = "camelCase")]
    MissingKeyword {
        keyword: String,
        segment: Option<usize>,
        message: String,
    },
    #[serde(rename = "EMPTY_DATA_SECTION", rename_all = "camelCase")]
    EmptyDataSection { segment: usize, message: String },
    #[serde(rename = "RECORDS_OUT_OF_ORDER", rename_all = "camelCase")]
    RecordsOutOfOrder {
        segment: usize,
        keyword: String,
        epoch: String,
        message: String,
    },
    #[serde(rename = "DUPLICATE_RECORD", rename_all = "camelCase")]
    DuplicateRecord {
        segment: usize,
        keyword: String,
        epoch: String,
        message: String,
    },
    #[serde(rename = "REPEATED_KEYWORD", rename_all = "camelCase")]
    RepeatedKeyword {
        keyword: String,
        section: String,
        message: String,
    },
    #[serde(rename = "KEYWORD_OUT_OF_ORDER", rename_all = "camelCase")]
    KeywordOutOfOrder {
        keyword: String,
        section: String,
        message: String,
    },
    #[serde(rename = "UNTERMINATED_FINAL_LINE", rename_all = "camelCase")]
    UnterminatedFinalLine { message: String },
    /// A departure this binding does not yet name, with the engine's message.
    #[serde(rename = "UNKNOWN", rename_all = "camelCase")]
    Unknown { message: String },
}

impl TdmDepartureJs {
    fn from_core(departure: CoreTdmDeparture) -> Self {
        let message = departure.to_string();
        match departure {
            CoreTdmDeparture::NonPrintableCharacter { keyword, character } => {
                Self::NonPrintableCharacter {
                    keyword,
                    character,
                    message,
                }
            }
            CoreTdmDeparture::LineTooLong { keyword, length } => Self::LineTooLong {
                keyword,
                length,
                message,
            },
            CoreTdmDeparture::MissingKeyword { keyword, segment } => Self::MissingKeyword {
                keyword,
                segment,
                message,
            },
            CoreTdmDeparture::EmptyDataSection { segment } => {
                Self::EmptyDataSection { segment, message }
            }
            CoreTdmDeparture::RecordsOutOfOrder {
                segment,
                keyword,
                epoch,
            } => Self::RecordsOutOfOrder {
                segment,
                keyword,
                epoch,
                message,
            },
            CoreTdmDeparture::DuplicateRecord {
                segment,
                keyword,
                epoch,
            } => Self::DuplicateRecord {
                segment,
                keyword,
                epoch,
                message,
            },
            CoreTdmDeparture::RepeatedKeyword { keyword, section } => Self::RepeatedKeyword {
                keyword,
                section: section.to_string(),
                message,
            },
            CoreTdmDeparture::KeywordOutOfOrder { keyword, section } => Self::KeywordOutOfOrder {
                keyword,
                section: section.to_string(),
                message,
            },
            CoreTdmDeparture::UnterminatedFinalLine => Self::UnterminatedFinalLine { message },
            // `TdmDeparture` is `#[non_exhaustive]`.
            _ => Self::Unknown { message },
        }
    }
}

fn departures_to_js(departures: Vec<CoreTdmDeparture>) -> Result<JsValue, JsValue> {
    let departures: Vec<TdmDepartureJs> = departures
        .into_iter()
        .map(TdmDepartureJs::from_core)
        .collect();
    to_plain_js(&departures, "TDM departures")
}

fn input_error_kind_label(kind: TdmInputErrorKind) -> Label {
    Label::Borrowed(match kind {
        TdmInputErrorKind::Missing => "MISSING",
        TdmInputErrorKind::FloatParse => "FLOAT_PARSE",
        TdmInputErrorKind::NonFinite => "NON_FINITE",
        TdmInputErrorKind::NotPositive => "NOT_POSITIVE",
        TdmInputErrorKind::OutOfRange => "OUT_OF_RANGE",
        TdmInputErrorKind::InvalidIndex => "INVALID_INDEX",
        TdmInputErrorKind::UnknownKeyword => "UNKNOWN_KEYWORD",
        TdmInputErrorKind::UnexpectedUnit => "UNEXPECTED_UNIT",
        TdmInputErrorKind::NonInteger => "NON_INTEGER",
        TdmInputErrorKind::Negative => "NEGATIVE",
        TdmInputErrorKind::NegativeZero => "NEGATIVE_ZERO",
        TdmInputErrorKind::UnitMismatch => "UNIT_MISMATCH",
        TdmInputErrorKind::DecimalMismatch => "DECIMAL_MISMATCH",
        // `TdmInputErrorKind` is `#[non_exhaustive]`; the error's `message`
        // carries the engine's own label.
        other => return upper_snake_variant(&other),
    })
}

/// Why a TDM could not be read, written or built, as the `detail` of a thrown
/// `TdmParseError`, `TdmWriteError` or `TdmValidationError`: a discriminated
/// union on `kind`. A `line` is one-based, and `null` where the writer or a
/// metadata builder raised the failure with no input line to point at.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind")]
enum TdmErrorDetailJs {
    #[serde(rename = "NO_SEGMENTS", rename_all = "camelCase")]
    NoSegments { message: String },
    #[serde(rename = "SECTION", rename_all = "camelCase")]
    Section {
        line: usize,
        detail: String,
        message: String,
    },
    #[serde(rename = "MALFORMED_LINE", rename_all = "camelCase")]
    MalformedLine {
        line: usize,
        text: String,
        message: String,
    },
    #[serde(rename = "NON_PRINTABLE_CHARACTER", rename_all = "camelCase")]
    NonPrintableCharacter {
        line: Option<usize>,
        keyword: String,
        column: usize,
        character: char,
        message: String,
    },
    #[serde(rename = "LINE_TOO_LONG", rename_all = "camelCase")]
    LineTooLong {
        line: Option<usize>,
        keyword: String,
        length: usize,
        message: String,
    },
    #[serde(rename = "MALFORMED_EPOCH", rename_all = "camelCase")]
    MalformedEpoch {
        line: Option<usize>,
        keyword: String,
        text: String,
        message: String,
    },
    #[serde(rename = "RECORDS_OUT_OF_ORDER", rename_all = "camelCase")]
    RecordsOutOfOrder {
        segment: usize,
        keyword: String,
        epoch: String,
        message: String,
    },
    #[serde(rename = "DUPLICATE_RECORD", rename_all = "camelCase")]
    DuplicateRecord {
        segment: usize,
        keyword: String,
        epoch: String,
        message: String,
    },
    #[serde(rename = "UNTERMINATED_FINAL_LINE", rename_all = "camelCase")]
    UnterminatedFinalLine { line: usize, message: String },
    #[serde(rename = "UNWRITABLE", rename_all = "camelCase")]
    Unwritable {
        keyword: String,
        reason: String,
        message: String,
    },
    #[serde(rename = "KEYWORD_OUT_OF_ORDER", rename_all = "camelCase")]
    KeywordOutOfOrder {
        line: Option<usize>,
        keyword: String,
        section: String,
        message: String,
    },
    #[serde(rename = "UNDEFINED_PARTICIPANT", rename_all = "camelCase")]
    UndefinedParticipant {
        segment: usize,
        keyword: String,
        index: u8,
        message: String,
    },
    #[serde(rename = "CONFLICTING_KEYWORD", rename_all = "camelCase")]
    ConflictingKeyword {
        line: Option<usize>,
        keyword: String,
        section: String,
        first: String,
        second: String,
        message: String,
    },
    #[serde(rename = "REPEATED_KEYWORD", rename_all = "camelCase")]
    RepeatedKeyword {
        line: Option<usize>,
        keyword: String,
        section: String,
        message: String,
    },
    #[serde(rename = "UNDEFINED_KEYWORD", rename_all = "camelCase")]
    UndefinedKeyword {
        line: usize,
        keyword: String,
        section: String,
        message: String,
    },
    #[serde(rename = "MISSING_KEYWORD", rename_all = "camelCase")]
    MissingKeyword {
        keyword: String,
        segment: Option<usize>,
        message: String,
    },
    #[serde(rename = "EMPTY_DATA_SECTION", rename_all = "camelCase")]
    EmptyDataSection { segment: usize, message: String },
    #[serde(rename = "EMPTY_VALUE", rename_all = "camelCase")]
    EmptyValue {
        line: Option<usize>,
        keyword: String,
        message: String,
    },
    #[serde(rename = "INVALID_VERSION", rename_all = "camelCase")]
    InvalidVersion {
        line: Option<usize>,
        value: String,
        message: String,
    },
    #[serde(rename = "KEYWORD_NOT_ASSIGNABLE", rename_all = "camelCase")]
    KeywordNotAssignable { keyword: String, message: String },
    #[serde(rename = "MALFORMED_RECORD", rename_all = "camelCase")]
    MalformedRecord {
        line: usize,
        keyword: String,
        message: String,
    },
    #[serde(rename = "INVALID_FIELD", rename_all = "camelCase")]
    InvalidField {
        keyword: String,
        input_error_kind: String,
        message: String,
    },
    /// A failure this binding does not yet name, with the engine's message.
    #[serde(rename = "UNKNOWN", rename_all = "camelCase")]
    Unknown { message: String },
}

impl TdmErrorDetailJs {
    fn from_core(err: CoreTdmError) -> Self {
        let message = err.to_string();
        match err {
            CoreTdmError::NoSegments => Self::NoSegments { message },
            CoreTdmError::Section { line, detail } => Self::Section {
                line,
                detail: detail.to_string(),
                message,
            },
            CoreTdmError::MalformedLine { line, text } => Self::MalformedLine {
                line,
                text,
                message,
            },
            CoreTdmError::NonPrintableCharacter {
                line,
                keyword,
                column,
                character,
            } => Self::NonPrintableCharacter {
                line,
                keyword,
                column,
                character,
                message,
            },
            CoreTdmError::LineTooLong {
                line,
                keyword,
                length,
            } => Self::LineTooLong {
                line,
                keyword,
                length,
                message,
            },
            CoreTdmError::MalformedEpoch {
                line,
                keyword,
                text,
            } => Self::MalformedEpoch {
                line,
                keyword,
                text,
                message,
            },
            CoreTdmError::RecordsOutOfOrder {
                segment,
                keyword,
                epoch,
            } => Self::RecordsOutOfOrder {
                segment,
                keyword,
                epoch,
                message,
            },
            CoreTdmError::DuplicateRecord {
                segment,
                keyword,
                epoch,
            } => Self::DuplicateRecord {
                segment,
                keyword,
                epoch,
                message,
            },
            CoreTdmError::UnterminatedFinalLine { line } => {
                Self::UnterminatedFinalLine { line, message }
            }
            CoreTdmError::Unwritable { keyword, reason } => Self::Unwritable {
                keyword,
                reason: reason.to_string(),
                message,
            },
            CoreTdmError::KeywordOutOfOrder {
                line,
                keyword,
                section,
            } => Self::KeywordOutOfOrder {
                line,
                keyword,
                section: section.to_string(),
                message,
            },
            CoreTdmError::UndefinedParticipant {
                segment,
                keyword,
                index,
            } => Self::UndefinedParticipant {
                segment,
                keyword,
                index,
                message,
            },
            CoreTdmError::ConflictingKeyword {
                line,
                keyword,
                section,
                first,
                second,
            } => Self::ConflictingKeyword {
                line,
                keyword,
                section: section.to_string(),
                first,
                second,
                message,
            },
            CoreTdmError::RepeatedKeyword {
                line,
                keyword,
                section,
            } => Self::RepeatedKeyword {
                line,
                keyword,
                section: section.to_string(),
                message,
            },
            CoreTdmError::UndefinedKeyword {
                line,
                keyword,
                section,
            } => Self::UndefinedKeyword {
                line,
                keyword,
                section: section.to_string(),
                message,
            },
            CoreTdmError::MissingKeyword { keyword, segment } => Self::MissingKeyword {
                keyword,
                segment,
                message,
            },
            CoreTdmError::EmptyDataSection { segment } => {
                Self::EmptyDataSection { segment, message }
            }
            CoreTdmError::EmptyValue { line, keyword } => Self::EmptyValue {
                line,
                keyword,
                message,
            },
            CoreTdmError::InvalidVersion { line, value } => Self::InvalidVersion {
                line,
                value,
                message,
            },
            CoreTdmError::KeywordNotAssignable { keyword } => {
                Self::KeywordNotAssignable { keyword, message }
            }
            CoreTdmError::MalformedRecord { line, keyword } => Self::MalformedRecord {
                line,
                keyword,
                message,
            },
            CoreTdmError::InvalidField { keyword, kind } => Self::InvalidField {
                keyword,
                input_error_kind: input_error_kind_label(kind).into_owned(),
                message,
            },
            // `TdmError` is `#[non_exhaustive]`.
            _ => Self::Unknown { message },
        }
    }
}

/// A thrown TDM error: `name` says which operation refused (`TdmParseError`,
/// `TdmWriteError` or `TdmValidationError`) and `detail` says why.
fn tdm_error(name: &str, err: CoreTdmError) -> JsValue {
    let message = err.to_string();
    error_with_detail(name, &message, &TdmErrorDetailJs::from_core(err))
}

/// A KVN key/value field preserved in parse order.
#[wasm_bindgen]
#[derive(Clone)]
pub struct TdmField {
    inner: CoreTdmField,
}

impl From<CoreTdmField> for TdmField {
    fn from(inner: CoreTdmField) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl TdmField {
    /// A raw field to pass to `TdmMetadata.fromRaw` or `replaceRaw`. Nothing is
    /// checked here; the metadata builder validates the key, the value and the
    /// field's place in the block.
    #[wasm_bindgen(constructor)]
    pub fn new(key: String, value: String) -> TdmField {
        TdmField {
            inner: CoreTdmField { key, value },
        }
    }

    /// The KVN keyword.
    #[wasm_bindgen(getter)]
    pub fn key(&self) -> String {
        self.inner.key.clone()
    }

    /// The trimmed KVN value.
    #[wasm_bindgen(getter)]
    pub fn value(&self) -> String {
        self.inner.value.clone()
    }
}

/// One named TDM tracking participant.
#[wasm_bindgen]
#[derive(Clone)]
pub struct TdmParticipant {
    inner: CoreTdmParticipant,
}

impl From<CoreTdmParticipant> for TdmParticipant {
    fn from(inner: CoreTdmParticipant) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl TdmParticipant {
    /// Numeric suffix from `PARTICIPANT_n`.
    #[wasm_bindgen(getter)]
    pub fn index(&self) -> u8 {
        self.inner.index
    }

    /// Participant name.
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.inner.name.clone()
    }
}

/// A parsed TDM signal path from `PATH`, `PATH_1`, or `PATH_2`.
#[wasm_bindgen]
#[derive(Clone)]
pub struct TdmPath {
    inner: CoreTdmPath,
}

impl From<CoreTdmPath> for TdmPath {
    fn from(inner: CoreTdmPath) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl TdmPath {
    /// Original path keyword.
    #[wasm_bindgen(getter)]
    pub fn key(&self) -> String {
        self.inner.key.clone()
    }

    /// Path suffix for `PATH_n`, or `undefined` for unindexed `PATH`.
    #[wasm_bindgen(getter)]
    pub fn index(&self) -> Option<u8> {
        self.inner.index
    }

    /// Participant indices listed in path order.
    #[wasm_bindgen(getter)]
    pub fn participants(&self) -> Vec<u8> {
        self.inner.participants.clone()
    }
}

/// A numeric TDM record value plus its exact decimal token.
#[wasm_bindgen]
#[derive(Clone)]
pub struct TdmScalar {
    inner: CoreTdmScalar,
}

impl From<CoreTdmScalar> for TdmScalar {
    fn from(inner: CoreTdmScalar) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl TdmScalar {
    /// Exact decimal or scientific-notation token read from the message.
    #[wasm_bindgen(getter)]
    pub fn text(&self) -> String {
        self.inner.text.clone()
    }

    /// Parsed finite `f64` value.
    #[wasm_bindgen(getter)]
    pub fn value(&self) -> f64 {
        self.inner.value
    }
}

fn observable_kind(observable: &TdmObservable) -> &'static str {
    match observable {
        TdmObservable::Range => "range",
        TdmObservable::DopplerInstantaneous => "dopplerInstantaneous",
        TdmObservable::DopplerIntegrated => "dopplerIntegrated",
        TdmObservable::ReceiveFreq { .. } => "receiveFreq",
        TdmObservable::TransmitFreq { .. } => "transmitFreq",
        TdmObservable::TransmitFreqRate { .. } => "transmitFreqRate",
        TdmObservable::Angle1 => "angle1",
        TdmObservable::Angle2 => "angle2",
        TdmObservable::Other(_) => "other",
    }
}

fn observable_participant(observable: &TdmObservable) -> Option<u8> {
    match observable {
        TdmObservable::ReceiveFreq { participant }
        | TdmObservable::TransmitFreq { participant }
        | TdmObservable::TransmitFreqRate { participant } => *participant,
        _ => None,
    }
}

fn observable_other(observable: &TdmObservable) -> Option<String> {
    match observable {
        TdmObservable::Other(name) => Some(name.clone()),
        _ => None,
    }
}

/// One time-tagged TDM tracking data record.
#[wasm_bindgen]
#[derive(Clone)]
pub struct TdmDataRecord {
    inner: CoreTdmDataRecord,
}

impl From<CoreTdmDataRecord> for TdmDataRecord {
    fn from(inner: CoreTdmDataRecord) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl TdmDataRecord {
    /// Observable family label such as `"range"` or `"receiveFreq"`.
    #[wasm_bindgen(getter, js_name = observableKind)]
    pub fn observable_kind(&self) -> String {
        observable_kind(&self.inner.observable).to_string()
    }

    /// Participant suffix for frequency records, or `undefined`.
    #[wasm_bindgen(getter, js_name = observableParticipant)]
    pub fn observable_participant(&self) -> Option<u8> {
        observable_participant(&self.inner.observable)
    }

    /// Original name for a table-defined keyword modeled as `other`.
    #[wasm_bindgen(getter, js_name = otherObservable)]
    pub fn other_observable(&self) -> Option<String> {
        observable_other(&self.inner.observable)
    }

    /// Original data keyword.
    #[wasm_bindgen(getter)]
    pub fn keyword(&self) -> String {
        self.inner.keyword.clone()
    }

    /// Raw epoch string.
    #[wasm_bindgen(getter)]
    pub fn epoch(&self) -> String {
        self.inner.epoch.clone()
    }

    /// Parsed numeric observable value.
    #[wasm_bindgen(getter)]
    pub fn value(&self) -> f64 {
        self.inner.value.value
    }

    /// Exact decimal token used for KVN encoding.
    #[wasm_bindgen(getter, js_name = valueText)]
    pub fn value_text(&self) -> String {
        self.inner.value.text.clone()
    }

    /// Numeric value plus exact decimal token.
    #[wasm_bindgen(getter)]
    pub fn scalar(&self) -> TdmScalar {
        self.inner.value.clone().into()
    }

    /// Canonical unit label assigned by CCSDS 503.0-B-2.
    #[wasm_bindgen(getter)]
    pub fn unit(&self) -> String {
        self.inner.unit.as_str().to_string()
    }
}

/// A TDM data block.
#[wasm_bindgen]
#[derive(Clone)]
pub struct TdmDataSection {
    inner: CoreTdmDataSection,
}

impl From<CoreTdmDataSection> for TdmDataSection {
    fn from(inner: CoreTdmDataSection) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl TdmDataSection {
    /// Data-section comments in parse order, each `{ text, beforeRecord }`
    /// with the index of the record it precedes. `toKvnString` writes each one
    /// back at that position.
    #[wasm_bindgen(getter, unchecked_return_type = "TdmComment[]")]
    pub fn comments(&self) -> Result<JsValue, JsValue> {
        comments_to_js(&self.inner.comments)
    }

    /// Data records in parse order.
    #[wasm_bindgen(getter)]
    pub fn records(&self) -> Vec<TdmDataRecord> {
        self.inner
            .records
            .iter()
            .cloned()
            .map(TdmDataRecord::from)
            .collect()
    }
}

/// Metadata extracted from a TDM `META_START` / `META_STOP` block.
#[wasm_bindgen]
#[derive(Clone)]
pub struct TdmMetadata {
    inner: CoreTdmMetadata,
}

impl From<CoreTdmMetadata> for TdmMetadata {
    fn from(inner: CoreTdmMetadata) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl TdmMetadata {
    /// Build metadata from ordered raw fields and positioned comments under the
    /// strict write policy.
    ///
    /// `fields` is an array of `{ key, value }` objects or `TdmField`
    /// instances, kept in the order given; `comments` is an array of
    /// `{ text, beforeRecord }`, or omitted for none. The core validates the
    /// block as the writer would - keyword membership and table order,
    /// repeated and conflicting keywords, the mandatory `TIME_SYSTEM` and
    /// `PARTICIPANT_n`, path participants and comment positions - and derives
    /// every convenience property from the fields. A block that fails is
    /// refused with a `TdmValidationError` whose `detail` is a
    /// `TdmErrorDetail`; a segment-specific failure names segment 1.
    #[wasm_bindgen(js_name = fromRaw)]
    pub fn from_raw(
        #[wasm_bindgen(unchecked_param_type = "TdmFieldInput[]")] fields: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "TdmComment[] | null")] comments: JsValue,
    ) -> Result<TdmMetadata, JsValue> {
        let fields = fields_from_js(&fields)?;
        let comments = comments_from_js(&comments)?;
        CoreTdmMetadata::from_raw(fields, comments)
            .map(TdmMetadata::from)
            .map_err(|err| tdm_error("TdmValidationError", err))
    }

    /// Build metadata under an explicit write policy, returning it with every
    /// departure the policy forgave, as `{ metadata, value, departures }`.
    /// `value` is the same instance as `metadata`. A departure no policy
    /// forgives is refused as `fromRaw` refuses it.
    #[wasm_bindgen(js_name = fromRawWithPolicy, unchecked_return_type = "TdmMetadataResult")]
    pub fn from_raw_with_policy(
        #[wasm_bindgen(unchecked_param_type = "TdmFieldInput[]")] fields: JsValue,
        #[wasm_bindgen(unchecked_param_type = "TdmComment[] | null | undefined")] comments: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "TdmWritePolicyLike")] policy: JsValue,
    ) -> Result<JsValue, JsValue> {
        let fields = fields_from_js(&fields)?;
        let comments = comments_from_js(&comments)?;
        let policy = write_policy(&policy)?;
        let (metadata, departures) =
            CoreTdmMetadata::from_raw_with_policy(fields, comments, policy)
                .map_err(|err| tdm_error("TdmValidationError", err))?;
        let metadata: JsValue = TdmMetadata::from(metadata).into();
        let departures = departures_to_js(departures)?;
        result_object(
            &[
                ("metadata", &metadata),
                ("value", &metadata),
                ("departures", &departures),
            ],
            "TDM metadata result",
        )
    }

    /// Replace this block's raw fields and comments under the strict write
    /// policy, re-deriving every convenience property from the new fields.
    ///
    /// Atomic: a candidate that fails validation throws a `TdmValidationError`
    /// and leaves this metadata exactly as it was.
    #[wasm_bindgen(js_name = replaceRaw)]
    pub fn replace_raw(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "TdmFieldInput[]")] fields: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "TdmComment[] | null")] comments: JsValue,
    ) -> Result<(), JsValue> {
        let fields = fields_from_js(&fields)?;
        let comments = comments_from_js(&comments)?;
        self.inner
            .replace_raw(fields, comments)
            .map_err(|err| tdm_error("TdmValidationError", err))
    }

    /// Replace this block's raw fields and comments under an explicit write
    /// policy, returning every departure the policy forgave. Atomic as
    /// `replaceRaw` is.
    #[wasm_bindgen(js_name = replaceRawWithPolicy, unchecked_return_type = "TdmDeparture[]")]
    pub fn replace_raw_with_policy(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "TdmFieldInput[]")] fields: JsValue,
        #[wasm_bindgen(unchecked_param_type = "TdmComment[] | null | undefined")] comments: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "TdmWritePolicyLike")] policy: JsValue,
    ) -> Result<JsValue, JsValue> {
        let fields = fields_from_js(&fields)?;
        let comments = comments_from_js(&comments)?;
        let policy = write_policy(&policy)?;
        let departures = self
            .inner
            .replace_raw_with_policy(fields, comments, policy)
            .map_err(|err| tdm_error("TdmValidationError", err))?;
        departures_to_js(departures)
    }

    /// Set keyword `key` to `value` under the strict write policy. The last
    /// occurrence of a keyword the block states takes the new value in place;
    /// a keyword it does not state is inserted at the first position the
    /// strict validator accepts, which is where the keyword order of CCSDS
    /// 503.0-B-2 tables 3-2 and 3-3 puts it. Every comment keeps the field it
    /// precedes, and every convenience property is re-derived.
    ///
    /// Atomic: a value or keyword the block cannot hold throws a
    /// `TdmValidationError` (for an absent keyword, the refusal of appending
    /// it) and leaves this metadata exactly as it was.
    #[wasm_bindgen(js_name = setField)]
    pub fn set_field(&mut self, key: &str, value: &str) -> Result<(), JsValue> {
        let mut fields = self.inner.fields.clone();
        let comments = self.inner.comments.clone();
        if let Some(index) = fields.iter().rposition(|field| field.key == key) {
            fields[index].value = value.to_owned();
            return self
                .inner
                .replace_raw(fields, comments)
                .map_err(|err| tdm_error("TdmValidationError", err));
        }
        let mut refusal = None;
        for position in 0..=fields.len() {
            let mut candidate = fields.clone();
            candidate.insert(
                position,
                CoreTdmField {
                    key: key.to_owned(),
                    value: value.to_owned(),
                },
            );
            let shifted = comments
                .iter()
                .map(|comment| CoreTdmComment {
                    text: comment.text.clone(),
                    before_record: if comment.before_record >= position {
                        comment.before_record + 1
                    } else {
                        comment.before_record
                    },
                })
                .collect();
            match self.inner.replace_raw(candidate, shifted) {
                Ok(()) => return Ok(()),
                Err(err) => refusal = Some(err),
            }
        }
        // The last candidate appended the keyword; its refusal is reported.
        Err(refusal.map_or_else(
            || type_error("no position was tried for the keyword"),
            |err| tdm_error("TdmValidationError", err),
        ))
    }

    /// Remove every occurrence of keyword `key` under the strict write
    /// policy. A comment that preceded a removed field precedes the field that
    /// followed it. Removing a keyword the block does not state changes
    /// nothing. Atomic as `setField` is: a block the removal leaves invalid
    /// (without its `TIME_SYSTEM`, say) throws and is left as it was.
    #[wasm_bindgen(js_name = removeField)]
    pub fn remove_field(&mut self, key: &str) -> Result<(), JsValue> {
        let mut fields = self.inner.fields.clone();
        let mut comments = self.inner.comments.clone();
        let mut index = fields.len();
        let mut removed = false;
        while index > 0 {
            index -= 1;
            if fields[index].key == key {
                fields.remove(index);
                removed = true;
                for comment in &mut comments {
                    if comment.before_record > index {
                        comment.before_record -= 1;
                    }
                }
            }
        }
        if !removed {
            return Ok(());
        }
        self.inner
            .replace_raw(fields, comments)
            .map_err(|err| tdm_error("TdmValidationError", err))
    }

    /// Metadata comments in parse order, each `{ text, beforeRecord }` with the
    /// index of the raw field it precedes; a comment after every field has
    /// `beforeRecord` equal to the field count. `toKvnString` writes each one
    /// back at that position.
    #[wasm_bindgen(getter, unchecked_return_type = "TdmComment[]")]
    pub fn comments(&self) -> Result<JsValue, JsValue> {
        comments_to_js(&self.inner.comments)
    }

    /// Raw metadata fields in parse order.
    #[wasm_bindgen(getter)]
    pub fn fields(&self) -> Vec<TdmField> {
        self.inner
            .fields
            .iter()
            .cloned()
            .map(TdmField::from)
            .collect()
    }

    /// Parsed `PARTICIPANT_n` entries.
    #[wasm_bindgen(getter)]
    pub fn participants(&self) -> Vec<TdmParticipant> {
        self.inner
            .participants
            .iter()
            .cloned()
            .map(TdmParticipant::from)
            .collect()
    }

    /// Optional `MODE` value.
    #[wasm_bindgen(getter)]
    pub fn mode(&self) -> Option<String> {
        self.inner.mode.clone()
    }

    /// Parsed `PATH`, `PATH_1`, and `PATH_2` entries.
    #[wasm_bindgen(getter)]
    pub fn paths(&self) -> Vec<TdmPath> {
        self.inner
            .paths
            .iter()
            .cloned()
            .map(TdmPath::from)
            .collect()
    }

    /// Optional `TIMETAG_REF` value.
    #[wasm_bindgen(getter, js_name = timetagRef)]
    pub fn timetag_ref(&self) -> Option<String> {
        self.inner.timetag_ref.clone()
    }

    /// Optional `TIME_SYSTEM` value.
    #[wasm_bindgen(getter, js_name = timeSystem)]
    pub fn time_system(&self) -> Option<String> {
        self.inner.time_system.clone()
    }

    /// Range unit label used by `RANGE` records.
    #[wasm_bindgen(getter, js_name = rangeUnits)]
    pub fn range_units(&self) -> String {
        self.inner.range_units.as_str().to_string()
    }

    /// Return the last metadata value for `key`, or `undefined`.
    #[wasm_bindgen(js_name = getLast)]
    pub fn get_last(&self, key: &str) -> Option<String> {
        self.inner.get_last(key).map(str::to_owned)
    }
}

/// One TDM segment, consisting of metadata and data blocks.
#[wasm_bindgen]
#[derive(Clone)]
pub struct TdmSegment {
    inner: CoreTdmSegment,
}

impl From<CoreTdmSegment> for TdmSegment {
    fn from(inner: CoreTdmSegment) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl TdmSegment {
    /// Metadata describing this segment's records.
    #[wasm_bindgen(getter)]
    pub fn metadata(&self) -> TdmMetadata {
        self.inner.metadata.clone().into()
    }

    /// Tracking data records for this segment.
    #[wasm_bindgen(getter)]
    pub fn data(&self) -> TdmDataSection {
        self.inner.data.clone().into()
    }
}

/// A parsed CCSDS Tracking Data Message.
#[wasm_bindgen]
#[derive(Clone)]
pub struct Tdm {
    inner: CoreTdm,
}

#[wasm_bindgen]
impl Tdm {
    /// The `CCSDS_TDM_VERS` header value.
    #[wasm_bindgen(getter)]
    pub fn version(&self) -> String {
        self.inner.version.clone()
    }

    /// Header comments in parse order, each `{ text, beforeRecord }`. A
    /// conforming header comment follows `CCSDS_TDM_VERS` and has
    /// `beforeRecord` 1. `toKvnString` writes each one back at that position.
    #[wasm_bindgen(getter, unchecked_return_type = "TdmComment[]")]
    pub fn comments(&self) -> Result<JsValue, JsValue> {
        comments_to_js(&self.inner.comments)
    }

    /// Optional `CREATION_DATE` header value.
    #[wasm_bindgen(getter, js_name = creationDate)]
    pub fn creation_date(&self) -> Option<String> {
        self.inner.creation_date.clone()
    }

    /// Optional `ORIGINATOR` header value.
    #[wasm_bindgen(getter)]
    pub fn originator(&self) -> Option<String> {
        self.inner.originator.clone()
    }

    /// Optional `MESSAGE_ID` header value.
    #[wasm_bindgen(getter, js_name = messageId)]
    pub fn message_id(&self) -> Option<String> {
        self.inner.message_id.clone()
    }

    /// Header fields not part of the common modeled header.
    #[wasm_bindgen(getter, js_name = headerFields)]
    pub fn header_fields(&self) -> Vec<TdmField> {
        self.inner
            .header_fields
            .iter()
            .cloned()
            .map(TdmField::from)
            .collect()
    }

    /// Metadata/data segments in message order.
    #[wasm_bindgen(getter)]
    pub fn segments(&self) -> Vec<TdmSegment> {
        self.inner
            .segments
            .iter()
            .cloned()
            .map(TdmSegment::from)
            .collect()
    }

    /// Number of segments in the message.
    #[wasm_bindgen(getter, js_name = segmentCount)]
    pub fn segment_count(&self) -> usize {
        self.inner.segments.len()
    }

    /// Replace one segment's metadata with a copy of `metadata`.
    ///
    /// `segments` and `metadata` return copies, so a block changed with
    /// `replaceRaw` is written back into the message here. `metadata` was
    /// validated when it was built or replaced; the message as a whole is
    /// validated when it is written. `segmentIndex` is zero-based, and one that
    /// is negative, fractional or past the last segment is a `RangeError`.
    #[wasm_bindgen(js_name = setSegmentMetadata)]
    pub fn set_segment_metadata(
        &mut self,
        segment_index: f64,
        metadata: &TdmMetadata,
    ) -> Result<(), JsValue> {
        let index = index_arg(segment_index, "segmentIndex")?;
        let count = self.inner.segments.len();
        let segment = self.inner.segments.get_mut(index).ok_or_else(|| {
            range_error(&format!(
                "segmentIndex {index} is past the message's {count} segments"
            ))
        })?;
        segment.metadata = metadata.inner.clone();
        Ok(())
    }

    /// Encode this TDM as canonical CCSDS KVN text under the strict write
    /// policy, with every comment at the position it holds.
    ///
    /// A message the writer cannot state conformingly is refused with a
    /// `TdmWriteError` whose `detail` is a `TdmErrorDetail`, never moved,
    /// repaired or dropped to fit.
    #[wasm_bindgen(js_name = toKvnString)]
    pub fn to_kvn_string(&self) -> Result<String, JsValue> {
        encode_kvn(&self.inner).map_err(|err| tdm_error("TdmWriteError", err))
    }

    /// Encode under an explicit write policy, returning the text with every
    /// departure from CCSDS 503.0-B-2 the policy let the writer emit, as
    /// `{ text, value, departures }`. `value` is the same string as `text`.
    #[wasm_bindgen(js_name = toKvnStringWithPolicy, unchecked_return_type = "TdmWriteResult")]
    pub fn to_kvn_string_with_policy(
        &self,
        #[wasm_bindgen(unchecked_optional_param_type = "TdmWritePolicyLike")] policy: JsValue,
    ) -> Result<JsValue, JsValue> {
        let policy = write_policy(&policy)?;
        let (text, departures) = encode_kvn_with_policy(&self.inner, policy)
            .map_err(|err| tdm_error("TdmWriteError", err))?;
        let text = JsValue::from_str(&text);
        let departures = departures_to_js(departures)?;
        result_object(
            &[
                ("text", &text),
                ("value", &text),
                ("departures", &departures),
            ],
            "TDM write result",
        )
    }
}

/// Parse a CCSDS Tracking Data Message in KVN form under the strict policy.
///
/// A message that departs from CCSDS 503.0-B-2 is refused with a
/// `TdmParseError` whose `detail` is a `TdmErrorDetail`.
#[wasm_bindgen(js_name = parseTdmKvn)]
pub fn parse_tdm_kvn(text: &str) -> Result<Tdm, JsValue> {
    Ok(Tdm {
        inner: parse_kvn(text).map_err(|err| tdm_error("TdmParseError", err))?,
    })
}

/// Parse a TDM under a reader policy, returning the message with every
/// departure the policy forgave, as `{ tdm, value, warnings }`. `value` is the
/// same instance as `tdm`.
///
/// `policy` is `"strict"`, `"lenient"`, an object naming axes as `"strict"` or
/// `"forgive"`, or omitted for strict. What no policy forgives is refused with
/// a `TdmParseError`.
#[wasm_bindgen(js_name = parseTdmKvnWithPolicy, unchecked_return_type = "TdmParseResult")]
pub fn parse_tdm_kvn_with_policy(
    text: &str,
    #[wasm_bindgen(unchecked_optional_param_type = "TdmPolicyLike")] policy: JsValue,
) -> Result<JsValue, JsValue> {
    let policy = read_policy(&policy)?;
    let (inner, warnings) =
        parse_kvn_with_policy(text, policy).map_err(|err| tdm_error("TdmParseError", err))?;
    let tdm: JsValue = Tdm { inner }.into();
    let warnings: Vec<TdmWarningJs> = warnings.into_iter().map(TdmWarningJs::from_core).collect();
    let warnings = to_plain_js(&warnings, "TDM warnings")?;
    result_object(
        &[("tdm", &tdm), ("value", &tdm), ("warnings", &warnings)],
        "TDM parse result",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tdm_error_detail_all_variants_and_payloads_are_exact() {
        let cases: Vec<(CoreTdmError, &str, serde_json::Value)> = vec![
            (
                CoreTdmError::NoSegments,
                "NO_SEGMENTS",
                serde_json::json!({}),
            ),
            (
                CoreTdmError::Section {
                    line: 5,
                    detail: "unexpected section",
                },
                "SECTION",
                serde_json::json!({"line":5,"detail":"unexpected section"}),
            ),
            (
                CoreTdmError::MalformedLine {
                    line: 12,
                    text: "NOT A LINE".into(),
                },
                "MALFORMED_LINE",
                serde_json::json!({"line":12,"text":"NOT A LINE"}),
            ),
            (
                CoreTdmError::NonPrintableCharacter {
                    line: Some(8),
                    keyword: "COMMENT".into(),
                    column: 4,
                    character: '\x07',
                },
                "NON_PRINTABLE_CHARACTER",
                serde_json::json!({"line":8,"keyword":"COMMENT","column":4,"character":"\u{7}"}),
            ),
            (
                CoreTdmError::LineTooLong {
                    line: None,
                    keyword: "DATA".into(),
                    length: 255,
                },
                "LINE_TOO_LONG",
                serde_json::json!({"line":null,"keyword":"DATA","length":255}),
            ),
            (
                CoreTdmError::MalformedEpoch {
                    line: Some(10),
                    keyword: "RECEIVE_FREQ".into(),
                    text: "bad-epoch".into(),
                },
                "MALFORMED_EPOCH",
                serde_json::json!({"line":10,"keyword":"RECEIVE_FREQ","text":"bad-epoch"}),
            ),
            (
                CoreTdmError::RecordsOutOfOrder {
                    segment: 1,
                    keyword: "RANGE".into(),
                    epoch: "2026-01-01T00:00:00".into(),
                },
                "RECORDS_OUT_OF_ORDER",
                serde_json::json!({"segment":1,"keyword":"RANGE","epoch":"2026-01-01T00:00:00"}),
            ),
            (
                CoreTdmError::DuplicateRecord {
                    segment: 2,
                    keyword: "DOPPLER".into(),
                    epoch: "2026-01-01T00:00:00".into(),
                },
                "DUPLICATE_RECORD",
                serde_json::json!({"segment":2,"keyword":"DOPPLER","epoch":"2026-01-01T00:00:00"}),
            ),
            (
                CoreTdmError::UnterminatedFinalLine { line: 99 },
                "UNTERMINATED_FINAL_LINE",
                serde_json::json!({"line":99}),
            ),
            (
                CoreTdmError::Unwritable {
                    keyword: "COMMENT".into(),
                    reason: "non-ascii",
                },
                "UNWRITABLE",
                serde_json::json!({"keyword":"COMMENT","reason":"non-ascii"}),
            ),
            (
                CoreTdmError::KeywordOutOfOrder {
                    line: Some(14),
                    keyword: "MODE".into(),
                    section: "metadata",
                },
                "KEYWORD_OUT_OF_ORDER",
                serde_json::json!({"line":14,"keyword":"MODE","section":"metadata"}),
            ),
            (
                CoreTdmError::UndefinedParticipant {
                    segment: 1,
                    keyword: "PATH".into(),
                    index: 3,
                },
                "UNDEFINED_PARTICIPANT",
                serde_json::json!({"segment":1,"keyword":"PATH","index":3}),
            ),
            (
                CoreTdmError::ConflictingKeyword {
                    line: Some(6),
                    keyword: "TIME_SYSTEM".into(),
                    section: "metadata",
                    first: "UTC".into(),
                    second: "TAI".into(),
                },
                "CONFLICTING_KEYWORD",
                serde_json::json!({"line":6,"keyword":"TIME_SYSTEM","section":"metadata","first":"UTC","second":"TAI"}),
            ),
            (
                CoreTdmError::RepeatedKeyword {
                    line: None,
                    keyword: "START_TIME".into(),
                    section: "metadata",
                },
                "REPEATED_KEYWORD",
                serde_json::json!({"line":null,"keyword":"START_TIME","section":"metadata"}),
            ),
            (
                CoreTdmError::UndefinedKeyword {
                    line: 22,
                    keyword: "UNKNOWN_KW".into(),
                    section: "header",
                },
                "UNDEFINED_KEYWORD",
                serde_json::json!({"line":22,"keyword":"UNKNOWN_KW","section":"header"}),
            ),
            (
                CoreTdmError::MissingKeyword {
                    keyword: "TIME_SYSTEM".into(),
                    segment: Some(1),
                },
                "MISSING_KEYWORD",
                serde_json::json!({"keyword":"TIME_SYSTEM","segment":1}),
            ),
            (
                CoreTdmError::EmptyDataSection { segment: 1 },
                "EMPTY_DATA_SECTION",
                serde_json::json!({"segment":1}),
            ),
            (
                CoreTdmError::EmptyValue {
                    line: None,
                    keyword: "PARTICIPANT_2".into(),
                },
                "EMPTY_VALUE",
                serde_json::json!({"line":null,"keyword":"PARTICIPANT_2"}),
            ),
            (
                CoreTdmError::InvalidVersion {
                    line: Some(1),
                    value: "3.0".into(),
                },
                "INVALID_VERSION",
                serde_json::json!({"line":1,"value":"3.0"}),
            ),
            (
                CoreTdmError::KeywordNotAssignable {
                    keyword: "DATA_START".into(),
                },
                "KEYWORD_NOT_ASSIGNABLE",
                serde_json::json!({"keyword":"DATA_START"}),
            ),
            (
                CoreTdmError::MalformedRecord {
                    line: 33,
                    keyword: "RECEIVE_FREQ".into(),
                },
                "MALFORMED_RECORD",
                serde_json::json!({"line":33,"keyword":"RECEIVE_FREQ"}),
            ),
            (
                CoreTdmError::InvalidField {
                    keyword: "TRANSMIT_FREQ".into(),
                    kind: TdmInputErrorKind::NotPositive,
                },
                "INVALID_FIELD",
                serde_json::json!({"keyword":"TRANSMIT_FREQ","inputErrorKind":"NOT_POSITIVE"}),
            ),
        ];

        assert_eq!(cases.len(), 22);
        for (error, expected_kind, expected_fields) in cases {
            let expected_message = error.to_string();
            let detail = serde_json::to_value(TdmErrorDetailJs::from_core(error)).unwrap();
            assert_eq!(detail["kind"], expected_kind);
            assert_eq!(detail["message"], expected_message);
            for (field, expected) in expected_fields.as_object().unwrap() {
                assert_eq!(&detail[field], expected, "field {field} of {expected_kind}");
            }
        }
    }
}

// --- TypeScript declarations -------------------------------------------------

// The plain-object shapes the TDM entry points take and return, and the names
// their `unchecked_*_type` attributes resolve against. `wasm-pack` writes them
// into both `sidereon.d.ts` targets; `types/sidereon-extra.d.ts` re-exports them.
#[wasm_bindgen(typescript_custom_section)]
const TS_TDM_DEFINITIONS: &str = r#"
/**
 * A comment and the zero-based index of the field or record it precedes. A
 * comment after every field or record has `beforeRecord` equal to their count.
 */
export interface TdmComment {
  text: string;
  beforeRecord: number;
}

/**
 * A raw metadata field as supplied to `TdmMetadata.fromRaw` or `replaceRaw`. A
 * `TdmField` instance is accepted too. Any other own property is a `TypeError`.
 */
export interface TdmFieldInput {
  key: string;
  value: string;
}

export type TdmLeniency = "strict" | "forgive";

/**
 * Reader policy axes; an omitted axis stays strict. Any other own property is a
 * `TypeError` naming it.
 */
export interface TdmPolicyInput {
  nonPrintable?: TdmLeniency | null;
  missingKeywords?: TdmLeniency | null;
  longLines?: TdmLeniency | null;
  emptyDataSections?: TdmLeniency | null;
  recordOrder?: TdmLeniency | null;
  duplicateRecords?: TdmLeniency | null;
  keywordOrder?: TdmLeniency | null;
  finalTerminator?: TdmLeniency | null;
}

export type TdmPolicyLike = "strict" | "lenient" | TdmPolicyInput | null | undefined;

/** Writer policy axes: the reader's, and `repeatedKeywords`. */
export interface TdmWritePolicyInput extends TdmPolicyInput {
  repeatedKeywords?: TdmLeniency | null;
}

export type TdmWritePolicyLike =
  | "strict"
  | "lenient"
  | TdmWritePolicyInput
  | null
  | undefined;

export type TdmWarning =
  | {
      kind: "NON_PRINTABLE_CHARACTER";
      line: number;
      keyword: string;
      column: number;
      character: string;
      message: string;
    }
  | { kind: "LINE_TOO_LONG"; line: number; keyword: string; length: number; message: string }
  | { kind: "REPEATED_KEYWORD"; line: number; keyword: string; section: string; message: string }
  | { kind: "MISSING_KEYWORD"; keyword: string; segment: number | null; message: string }
  | { kind: "EMPTY_DATA_SECTION"; segment: number; message: string }
  | {
      kind: "RECORDS_OUT_OF_ORDER";
      segment: number;
      keyword: string;
      epoch: string;
      message: string;
    }
  | { kind: "UNTERMINATED_FINAL_LINE"; line: number; message: string }
  | {
      kind: "KEYWORD_OUT_OF_ORDER";
      line: number;
      keyword: string;
      section: string;
      message: string;
    }
  | { kind: "DUPLICATE_RECORD"; segment: number; keyword: string; epoch: string; message: string }
  | { kind: "UNKNOWN"; message: string };

export type TdmDeparture =
  | { kind: "NON_PRINTABLE_CHARACTER"; keyword: string; character: string; message: string }
  | { kind: "LINE_TOO_LONG"; keyword: string; length: number; message: string }
  | { kind: "MISSING_KEYWORD"; keyword: string; segment: number | null; message: string }
  | { kind: "EMPTY_DATA_SECTION"; segment: number; message: string }
  | {
      kind: "RECORDS_OUT_OF_ORDER";
      segment: number;
      keyword: string;
      epoch: string;
      message: string;
    }
  | { kind: "DUPLICATE_RECORD"; segment: number; keyword: string; epoch: string; message: string }
  | { kind: "REPEATED_KEYWORD"; keyword: string; section: string; message: string }
  | { kind: "KEYWORD_OUT_OF_ORDER"; keyword: string; section: string; message: string }
  | { kind: "UNTERMINATED_FINAL_LINE"; message: string }
  | { kind: "UNKNOWN"; message: string };

export type TdmInputErrorKind =
  | "MISSING"
  | "FLOAT_PARSE"
  | "NON_FINITE"
  | "NOT_POSITIVE"
  | "OUT_OF_RANGE"
  | "INVALID_INDEX"
  | "UNKNOWN_KEYWORD"
  | "UNEXPECTED_UNIT"
  | "NON_INTEGER"
  | "NEGATIVE"
  | "NEGATIVE_ZERO"
  | "UNIT_MISMATCH"
  | "DECIMAL_MISMATCH"
  | (string & {});

/**
 * The `detail` of a thrown `TdmParseError`, `TdmWriteError` or
 * `TdmValidationError`. A `line` is one-based, and `null` where the failure has
 * no input line.
 */
export type TdmErrorDetail =
  | { kind: "NO_SEGMENTS"; message: string }
  | { kind: "SECTION"; line: number; detail: string; message: string }
  | { kind: "MALFORMED_LINE"; line: number; text: string; message: string }
  | {
      kind: "NON_PRINTABLE_CHARACTER";
      line: number | null;
      keyword: string;
      column: number;
      character: string;
      message: string;
    }
  | {
      kind: "LINE_TOO_LONG";
      line: number | null;
      keyword: string;
      length: number;
      message: string;
    }
  | {
      kind: "MALFORMED_EPOCH";
      line: number | null;
      keyword: string;
      text: string;
      message: string;
    }
  | {
      kind: "RECORDS_OUT_OF_ORDER";
      segment: number;
      keyword: string;
      epoch: string;
      message: string;
    }
  | { kind: "DUPLICATE_RECORD"; segment: number; keyword: string; epoch: string; message: string }
  | { kind: "UNTERMINATED_FINAL_LINE"; line: number; message: string }
  | { kind: "UNWRITABLE"; keyword: string; reason: string; message: string }
  | {
      kind: "KEYWORD_OUT_OF_ORDER";
      line: number | null;
      keyword: string;
      section: string;
      message: string;
    }
  | {
      kind: "UNDEFINED_PARTICIPANT";
      segment: number;
      keyword: string;
      index: number;
      message: string;
    }
  | {
      kind: "CONFLICTING_KEYWORD";
      line: number | null;
      keyword: string;
      section: string;
      first: string;
      second: string;
      message: string;
    }
  | {
      kind: "REPEATED_KEYWORD";
      line: number | null;
      keyword: string;
      section: string;
      message: string;
    }
  | { kind: "UNDEFINED_KEYWORD"; line: number; keyword: string; section: string; message: string }
  | { kind: "MISSING_KEYWORD"; keyword: string; segment: number | null; message: string }
  | { kind: "EMPTY_DATA_SECTION"; segment: number; message: string }
  | { kind: "EMPTY_VALUE"; line: number | null; keyword: string; message: string }
  | { kind: "INVALID_VERSION"; line: number | null; value: string; message: string }
  | { kind: "KEYWORD_NOT_ASSIGNABLE"; keyword: string; message: string }
  | { kind: "MALFORMED_RECORD"; line: number; keyword: string; message: string }
  | {
      kind: "INVALID_FIELD";
      keyword: string;
      inputErrorKind: TdmInputErrorKind;
      message: string;
    }
  | { kind: "UNKNOWN"; message: string };

export interface TdmParseResult {
  tdm: Tdm;
  value: Tdm;
  warnings: TdmWarning[];
}

export interface TdmWriteResult {
  text: string;
  value: string;
  departures: TdmDeparture[];
}

export interface TdmMetadataResult {
  metadata: TdmMetadata;
  value: TdmMetadata;
  departures: TdmDeparture[];
}
"#;
