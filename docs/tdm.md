# CCSDS TDM surface reference

The TDM bindings read and write CCSDS 503.0-B-2 Tracking Data Messages in KVN
form through `sidereon-core`. Every rule the reader and writer apply is the
engine's; this module converts representations, and returns each departure a
policy forgave or emitted.

This document describes the settled 3.0 surface. It does not move the package
version pin.

## 1. Positioned comments

`Tdm.comments`, `TdmMetadata.comments` and `TdmDataSection.comments` are
`TdmComment[]`, each `{ text, beforeRecord }`: the comment text after the
`COMMENT` keyword and its required space, and the zero-based index of the field
or record it precedes. A comment after every field or record has `beforeRecord`
equal to their count. A conforming header comment follows `CCSDS_TDM_VERS` and
has `beforeRecord` 1; conforming metadata and data comments have 0.

The writer puts each comment back at its position. A position it cannot emit
unchanged is refused rather than moved: past the end of its block, out of
ascending order, or, under the strict policy, away from the start of its
section (CCSDS 503.0-B-2 4.5.2).

## 2. Metadata from raw fields

A metadata block's ordered raw fields and positioned comments are its
authority. `participants`, `paths`, `mode`, `timetagRef`, `timeSystem` and
`rangeUnits` are derived from the fields by the engine whenever a block is
built or replaced; nothing in this module reads or rewrites a field.

| Entry point | Result |
| --- | --- |
| `TdmMetadata.fromRaw(fields, comments?)` | A `TdmMetadata`, validated under the strict write policy. |
| `TdmMetadata.fromRawWithPolicy(fields, comments, policy?)` | `{ metadata, value, departures }`; `value` is the same instance. |
| `metadata.replaceRaw(fields, comments?)` | Replaces the fields, the comments and every derived property together. |
| `metadata.replaceRawWithPolicy(fields, comments, policy?)` | The same, returning `TdmDeparture[]`. |
| `tdm.setSegmentMetadata(segmentIndex, metadata)` | Writes a copy of `metadata` into segment `segmentIndex`. |

`fields` is an array of `{ key, value }` objects or `TdmField` instances
(`new TdmField(key, value)`), kept in the order given. `comments` is an array
of `{ text, beforeRecord }`, or `undefined` / `null` for none.

Replacement is atomic: a candidate the engine refuses throws a
`TdmValidationError` and leaves the metadata exactly as it was. The engine
checks keyword membership and table order, repeated and conflicting keywords,
the mandatory `TIME_SYSTEM` and `PARTICIPANT_n`, path participants and comment
positions; a segment-specific failure names segment 1.

`segments` and `metadata` return copies, so a block changed with `replaceRaw` is
written back with `setSegmentMetadata`. The message as a whole is validated
when it is written.

The binding checks the shape of what it is given before the engine sees it: an
own property other than `key` and `value` on a field, or `text` and
`beforeRecord` on a comment, is a `TypeError` naming it; a `beforeRecord` that
is not a number is a `TypeError`, and one that is negative, fractional or
non-finite is a `RangeError`. A `segmentIndex` is read the same way, and one
past the last segment is a `RangeError`.

## 3. Policies

A reader policy is `"strict"`, `"lenient"`, an object naming axes, or omitted
for strict. Each axis is `"strict"` or `"forgive"`; an axis the object leaves
out stays strict, and an axis it misspells is a `TypeError`.

| Axis | CCSDS 503.0-B-2 |
| --- | --- |
| `nonPrintable` | 4.2.1 printable ASCII |
| `missingKeywords` | tables 3-2 and 3-3 mandatory keywords |
| `longLines` | 4.2.1 line length |
| `emptyDataSections` | 3.1.3 at least one record |
| `recordOrder` | 3.4.10 chronological records |
| `duplicateRecords` | 3.4.11 unique keyword and timetag |
| `keywordOrder` | tables 3-2 and 3-3 order, 4.5.2 comment placement |
| `finalTerminator` | 4.2.11 terminated last line |

A write policy takes the same axes and `repeatedKeywords`, a keyword written
twice in one block with the same value.

## 4. Reading and writing

| Entry point | Result |
| --- | --- |
| `parseTdmKvn(text)` | A `Tdm` under the strict policy. |
| `parseTdmKvnWithPolicy(text, policy?)` | `{ tdm, value, warnings }`, every departure the policy forgave as a `TdmWarning`. |
| `tdm.toKvnString()` | KVN text under the strict write policy. |
| `tdm.toKvnStringWithPolicy(policy?)` | `{ text, value, departures }`, every departure the writer emitted as a `TdmDeparture`. |

The writer terminates every line, the last one included, unless
`finalTerminator` is forgiven.

A refusal is an `Error` named `TdmParseError`, `TdmWriteError` or
`TdmValidationError` for the operation that refused, whose `detail` is a
`TdmErrorDetail`: a union on `kind` (`NO_SEGMENTS`, `SECTION`,
`MALFORMED_LINE`, `NON_PRINTABLE_CHARACTER`, `LINE_TOO_LONG`,
`MALFORMED_EPOCH`, `RECORDS_OUT_OF_ORDER`, `DUPLICATE_RECORD`,
`UNTERMINATED_FINAL_LINE`, `UNWRITABLE`, `KEYWORD_OUT_OF_ORDER`,
`UNDEFINED_PARTICIPANT`, `CONFLICTING_KEYWORD`, `REPEATED_KEYWORD`,
`UNDEFINED_KEYWORD`, `MISSING_KEYWORD`, `EMPTY_DATA_SECTION`, `EMPTY_VALUE`,
`INVALID_VERSION`, `KEYWORD_NOT_ASSIGNABLE`, `MALFORMED_RECORD`,
`INVALID_FIELD`, and `UNKNOWN` for a variant this binding does not yet name)
with the engine's payload and `message`. A `line` is one-based, and `null`
where the writer or a metadata builder raised the failure with no input line.
`INVALID_FIELD` carries the engine's input error kind as `inputErrorKind`.

`TdmWarning` and `TdmDeparture` are unions on `kind` in the same vocabulary,
each with the engine's `message`.

## 5. TypeScript

The shapes are declared in the `typescript_custom_section` of `src/tdm.rs` and
re-exported from `@neilberkman/sidereon/types`: `TdmComment`, `TdmFieldInput`,
`TdmLeniency`, `TdmPolicyInput`, `TdmPolicyLike`, `TdmWritePolicyInput`,
`TdmWritePolicyLike`, `TdmWarning`, `TdmDeparture`, `TdmInputErrorKind`,
`TdmErrorDetail`, `TdmParseResult`, `TdmWriteResult` and `TdmMetadataResult`.

## 6. Not bound here

Header and data comments are read-only: a message's header fields and data
records are not built from JavaScript, so only a metadata block can be replaced.
Per-keyword metadata setters are not bound; a block changes through
`replaceRaw`.
