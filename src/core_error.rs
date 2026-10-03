//! Structured core and domain error details for WebAssembly bindings.
//!
//! Provides typed, structured error representations across the WASM boundary
//! for `sidereon_core::Error`, `sidereon_core::observables::ObservablesError`,
//! and `sidereon_core::velocity::VelocityError`.

use serde::Serialize;
use wasm_bindgen::JsValue;

use sidereon_core::observables::{ObservablesError, ObservablesInputErrorKind};
use sidereon_core::velocity::VelocityError;
use sidereon_core::Error as CoreError;

use crate::label::upper_snake_variant;

/// Exact JSON-safe representation for an IEEE-754 binary64 value.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExactFloat {
    /// Decimal rendering, including the standard `NaN` and infinity tokens.
    pub decimal: String,
    /// Exact 64-bit IEEE representation as sixteen lowercase hexadecimal digits.
    pub bits_hex: String,
}

fn exact_float(value: f64) -> ExactFloat {
    ExactFloat {
        decimal: value.to_string(),
        bits_hex: format!("{:016x}", value.to_bits()),
    }
}

// `usize` fields in these details remain JSON numbers: the public binding is
// compiled for wasm32, where every `usize` fits in JavaScript's exact integer
// range. Wider native-only test values are not a WebAssembly payload contract.

/// The structured detail of a [`CoreError`].
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind")]
pub enum CoreErrorDetail {
    #[serde(rename = "PARSE", rename_all = "camelCase")]
    Parse { message: String },
    #[serde(rename = "UNKNOWN_SATELLITE", rename_all = "camelCase")]
    UnknownSatellite { satellite_id: String },
    #[serde(rename = "MISSING_GLONASS_CHANNEL", rename_all = "camelCase")]
    MissingGlonassChannel,
    #[serde(rename = "MISSING_TERRAIN_TILE", rename_all = "camelCase")]
    MissingTerrainTile { lat_index: i32, lon_index: i32 },
    #[serde(rename = "UNKNOWN_TERRAIN_ELEVATION", rename_all = "camelCase")]
    UnknownTerrainElevation {
        lat_index: i32,
        lon_index: i32,
        latitude_posting: usize,
        longitude_posting: usize,
    },
    #[serde(rename = "NON_WGS84_TERRAIN_TILE", rename_all = "camelCase")]
    NonWgs84TerrainTile {
        lat_index: i32,
        lon_index: i32,
        datum: String,
    },
    #[serde(rename = "TERRAIN_TILE", rename_all = "camelCase")]
    TerrainTile {
        lat_index: i32,
        lon_index: i32,
        cause: serde_json::Value,
    },
    #[serde(rename = "TERRAIN_TILE_ORIGIN", rename_all = "camelCase")]
    TerrainTileOrigin {
        path: String,
        lat_index: i32,
        lon_index: i32,
        origin_latitude: i32,
        origin_longitude: i32,
    },
    #[serde(rename = "IONEX_OUT_OF_COVERAGE", rename_all = "camelCase")]
    IonexOutOfCoverage {
        cause: crate::ionex::IonexCoverageErrorJs,
    },
    #[serde(rename = "IONEX_NODES_NOT_AVAILABLE", rename_all = "camelCase")]
    IonexNodesNotAvailable { cause: crate::ionex::IonexNodeGapJs },
    #[serde(rename = "IONEX_SLANT_UNAVAILABLE", rename_all = "camelCase")]
    IonexSlantUnavailable {
        cause: crate::ionex::IonexSlantRefusalJs,
    },
    #[serde(rename = "IONEX_EPOCH", rename_all = "camelCase")]
    IonexEpoch { cause: serde_json::Value },
    #[serde(rename = "EPOCH_OUT_OF_RANGE", rename_all = "camelCase")]
    EpochOutOfRange,
    #[serde(rename = "INSUFFICIENT_PRECISE_NODES", rename_all = "camelCase")]
    InsufficientPreciseNodes {
        satellite_id: String,
        nodes: usize,
        required: usize,
    },
    #[serde(rename = "INVALID_INPUT", rename_all = "camelCase")]
    InvalidInput { message: String },
    #[serde(rename = "SP3_EPOCH_INTERVAL", rename_all = "camelCase")]
    Sp3EpochInterval {
        field: String,
        value: ExactFloat,
        reason: String,
    },
    #[serde(rename = "SP3_MERGE_TOLERANCE", rename_all = "camelCase")]
    Sp3MergeTolerance {
        field: String,
        value: ExactFloat,
        reason: String,
    },
    #[serde(rename = "CONTINUITY_OPTIONS", rename_all = "camelCase")]
    ContinuityOptions {
        field: &'static str,
        value: ExactFloat,
        reason: &'static str,
    },
    #[serde(rename = "SBAS_ENCODE", rename_all = "camelCase")]
    SbasEncode { cause: serde_json::Value },
    #[serde(rename = "RTCM_ENCODE", rename_all = "camelCase")]
    RtcmEncode { cause: serde_json::Value },
    #[serde(rename = "RTCM_CONVERSION", rename_all = "camelCase")]
    RtcmConversion { cause: serde_json::Value },
    #[serde(rename = "UT1_OUTSIDE_COVERAGE", rename_all = "camelCase")]
    Ut1OutsideCoverage { reason: &'static str },
    #[serde(rename = "OTHER", rename_all = "camelCase")]
    Other {
        message: String,
        variant: String,
        debug: String,
    },
}

fn dted_tile_error_payload(error: &sidereon_core::terrain::DtedTileError) -> serde_json::Value {
    use sidereon_core::terrain::DtedTileError as E;
    match error {
        E::Io { path, message } => serde_json::json!({
            "kind": "io",
            "path": path,
            "message": message,
        }),
        E::TooShort { path } => serde_json::json!({
            "kind": "tooShort",
            "path": path,
        }),
        E::MissingUhl1 { path } => serde_json::json!({
            "kind": "missingUhl1",
            "path": path,
        }),
        E::InvalidEncoding(message) => serde_json::json!({
            "kind": "invalidEncoding",
            "message": message,
        }),
        E::InvalidField(message) => serde_json::json!({
            "kind": "invalidField",
            "message": message,
        }),
        E::InvalidDimensions {
            path,
            lon_count,
            lat_count,
        } => serde_json::json!({
            "kind": "invalidDimensions",
            "path": path,
            "lonCount": lon_count,
            "latCount": lat_count,
        }),
        E::Truncated {
            path,
            actual,
            expected,
        } => serde_json::json!({
            "kind": "truncated",
            "path": path,
            "actual": actual,
            "expected": expected,
        }),
        E::Outside {
            longitude,
            latitude,
            origin_longitude,
            origin_latitude,
        } => serde_json::json!({
            "kind": "outside",
            "longitude": exact_float(*longitude),
            "latitude": exact_float(*latitude),
            "originLongitude": exact_float(*origin_longitude),
            "originLatitude": exact_float(*origin_latitude),
        }),
        E::PostingIndexOutOfBounds {
            longitude_index,
            latitude_index,
        } => serde_json::json!({
            "kind": "postingIndexOutOfBounds",
            "longitudeIndex": longitude_index,
            "latitudeIndex": latitude_index,
        }),
        E::MissingDataSentinel { longitude_index } => serde_json::json!({
            "kind": "missingDataSentinel",
            "longitudeIndex": longitude_index,
        }),
        E::Checksum {
            longitude_index,
            checksum,
            sum,
        } => serde_json::json!({
            "kind": "checksum",
            "longitudeIndex": longitude_index,
            "checksum": checksum,
            "sum": sum,
        }),
        E::EmptyCoordinate => serde_json::json!({"kind": "emptyCoordinate"}),
        E::InvalidHemisphere { hemisphere } => serde_json::json!({
            "kind": "invalidHemisphere",
            "hemisphere": hemisphere.to_string(),
        }),
        E::NegativePostingIndex { index } => serde_json::json!({
            "kind": "negativePostingIndex",
            "index": index.to_string(),
        }),
        E::CoordinateOutOfRange { field, text } => serde_json::json!({
            "kind": "coordinateOutOfRange",
            "field": field,
            "text": text,
        }),
        E::WrongHemisphere {
            field,
            hemisphere,
            expected,
        } => serde_json::json!({
            "kind": "wrongHemisphere",
            "field": field,
            "hemisphere": hemisphere.to_string(),
            "expected": expected,
        }),
        E::OriginNotWholeDegree { field, text } => serde_json::json!({
            "kind": "originNotWholeDegree",
            "field": field,
            "text": text,
        }),
        E::IntervalCountMismatch {
            field,
            interval_tenths_arcsec,
            count,
        } => serde_json::json!({
            "kind": "intervalCountMismatch",
            "field": field,
            "intervalTenthsArcsec": interval_tenths_arcsec,
            "count": count,
        }),
        E::ProfileLongitudeCountMismatch {
            longitude_index,
            declared,
        } => serde_json::json!({
            "kind": "profileLongitudeCountMismatch",
            "longitudeIndex": longitude_index,
            "declared": declared,
        }),
        E::UnsupportedPartialProfile {
            longitude_index,
            first_latitude_index,
        } => serde_json::json!({
            "kind": "unsupportedPartialProfile",
            "longitudeIndex": longitude_index,
            "firstLatitudeIndex": first_latitude_index,
        }),
        E::NullPosting {
            longitude_index,
            latitude_index,
        } => serde_json::json!({
            "kind": "nullPosting",
            "longitudeIndex": longitude_index,
            "latitudeIndex": latitude_index,
        }),
        other => serde_json::json!({
            "kind": "other",
            "message": other.to_string(),
            "debug": format!("{other:?}"),
        }),
    }
}

fn ionex_epoch_error_payload(
    error: &sidereon_core::atmosphere::IonexEpochError,
) -> serde_json::Value {
    use sidereon_core::atmosphere::IonexEpochError as E;
    match error {
        E::NotWholeSecond { scale } => serde_json::json!({
            "kind": "notWholeSecond",
            "scale": scale.abbrev(),
        }),
        E::FractionalUtcSecond { scale } => serde_json::json!({
            "kind": "fractionalUtcSecond",
            "scale": scale.abbrev(),
        }),
        E::NoExactUtcOffset { scale } => serde_json::json!({
            "kind": "noExactUtcOffset",
            "scale": scale.abbrev(),
        }),
        E::InsertedLeapSecond { scale } => serde_json::json!({
            "kind": "insertedLeapSecond",
            "scale": scale.abbrev(),
        }),
        E::BeforeIntegerLeapSeconds { scale } => serde_json::json!({
            "kind": "beforeIntegerLeapSeconds",
            "scale": scale.abbrev(),
        }),
        E::OutOfRange { scale } => serde_json::json!({
            "kind": "outOfRange",
            "scale": scale.abbrev(),
        }),
        E::YearOutOfField { utc_j2000_s } => serde_json::json!({
            "kind": "yearOutOfField",
            "utcJ2000S": utc_j2000_s.to_string(),
        }),
        other => serde_json::json!({
            "kind": "other",
            "message": other.to_string(),
            "debug": format!("{other:?}"),
        }),
    }
}

fn sbas_encode_error_payload(error: &sidereon_core::sbas::SbasEncodeError) -> serde_json::Value {
    use sidereon_core::sbas::SbasEncodeError as E;
    match error {
        E::FieldOutOfRange {
            message_type,
            field,
            index,
            value,
            width,
            signed,
        } => serde_json::json!({
            "kind": "fieldOutOfRange",
            "messageType": *message_type,
            "field": field,
            "index": index,
            "value": value.to_string(),
            "width": width,
            "signed": signed,
        }),
        E::UnrecognizedPreamble { preamble } => serde_json::json!({
            "kind": "unrecognizedPreamble",
            "preamble": preamble,
        }),
        E::MessageType {
            message_type,
            reason,
        } => serde_json::json!({
            "kind": "messageType",
            "messageType": *message_type,
            "reason": reason,
        }),
        E::RawPayload {
            message_type,
            bytes,
            bits_past_payload,
        } => serde_json::json!({
            "kind": "rawPayload",
            "messageType": *message_type,
            "bytes": bytes,
            "bitsPastPayload": bits_past_payload,
        }),
        E::ReservedLayout {
            message_type,
            part,
            expected,
            found,
        } => serde_json::json!({
            "kind": "reservedLayout",
            "messageType": *message_type,
            "part": part,
            "expected": expected,
            "found": found,
        }),
        E::LongTermRecordCount {
            message_type,
            half,
            velocity_code,
            expected,
            found,
        } => serde_json::json!({
            "kind": "longTermRecordCount",
            "messageType": *message_type,
            "half": half,
            "velocityCode": velocity_code,
            "expected": expected,
            "found": found,
        }),
        E::LongTermFieldNotCarried {
            message_type,
            half,
            record,
            field,
        } => serde_json::json!({
            "kind": "longTermFieldNotCarried",
            "messageType": *message_type,
            "half": half,
            "record": record,
            "field": field,
        }),
        E::LongTermMissingTimeOfDay { message_type, half } => serde_json::json!({
            "kind": "longTermMissingTimeOfDay",
            "messageType": *message_type,
            "half": half,
        }),
        E::PadBits { value } => serde_json::json!({
            "kind": "padBits",
            "value": value,
        }),
        other => serde_json::json!({
            "kind": "other",
            "message": other.to_string(),
            "debug": format!("{other:?}"),
        }),
    }
}

fn msm_kind_label(kind: sidereon_core::rtcm::MsmKind) -> &'static str {
    use sidereon_core::rtcm::MsmKind as M;
    match kind {
        M::Msm1 => "msm1",
        M::Msm2 => "msm2",
        M::Msm3 => "msm3",
        M::Msm4 => "msm4",
        M::Msm5 => "msm5",
        M::Msm6 => "msm6",
        M::Msm7 => "msm7",
    }
}

fn ssr_kind_label(kind: sidereon_core::rtcm::SsrKind) -> &'static str {
    use sidereon_core::rtcm::SsrKind as S;
    match kind {
        S::Orbit => "orbit",
        S::Clock => "clock",
        S::CombinedOrbitClock => "combinedOrbitClock",
        S::CodeBias => "codeBias",
        S::PhaseBias => "phaseBias",
        S::Ura => "ura",
        S::HighRateClock => "highRateClock",
    }
}

fn rtcm_record_kind_value(record: sidereon_core::rtcm::RtcmRecordKind) -> serde_json::Value {
    use sidereon_core::rtcm::RtcmRecordKind as Record;
    match record {
        Record::StationCoordinates => serde_json::json!({"kind": "stationCoordinates"}),
        Record::AntennaDescriptor => serde_json::json!({"kind": "antennaDescriptor"}),
        Record::Msm { system, kind } => serde_json::json!({
            "kind": "msm",
            "system": system.as_str(),
            "messageKind": msm_kind_label(kind)
        }),
        Record::Ssr { system, kind } => serde_json::json!({
            "kind": "ssr",
            "system": system.as_str(),
            "messageKind": ssr_kind_label(kind),
        }),
        Record::LegacyObservations => serde_json::json!({"kind": "legacyObservations"}),
        Record::SystemParameters => serde_json::json!({"kind": "systemParameters"}),
        Record::Text => serde_json::json!({"kind": "text"}),
        Record::Network { family } => serde_json::json!({"kind": "network", "family": family}),
        Record::Transformation { family } => {
            serde_json::json!({"kind": "transformation", "family": family})
        }
        Record::GlonassCodePhaseBiases => serde_json::json!({"kind": "glonassCodePhaseBiases"}),
        Record::SsrVtec { message_number } => {
            serde_json::json!({"kind": "ssrVtec", "messageNumber": message_number})
        }
        _ => serde_json::json!({"kind": "unrecognizedRecord"}),
    }
}

fn rtcm_encode_error_payload(error: &sidereon_core::rtcm::RtcmEncodeError) -> serde_json::Value {
    use sidereon_core::rtcm::{MsmMaskProblem, RtcmEncodeError as Encode};
    match error {
        Encode::FieldOutOfRange {
            message_number,
            field,
            value,
            width,
            encoding,
        } => serde_json::json!({
            "kind": "fieldOutOfRange",
            "messageNumber": message_number,
            "field": field,
            "value": value.to_string(),
            "width": width,
            "encoding": match encoding {
                sidereon_core::rtcm::RtcmFieldEncoding::Unsigned => "unsigned",
                sidereon_core::rtcm::RtcmFieldEncoding::TwosComplement => "twosComplement",
                sidereon_core::rtcm::RtcmFieldEncoding::SignMagnitude => "signMagnitude",
                _ => "unrecognized",
            },
        }),
        Encode::NegativeZeroWithValue {
            message_number,
            field,
            value,
        } => serde_json::json!({
            "kind": "negativeZeroWithValue",
            "messageNumber": message_number,
            "field": field,
            "value": value.to_string(),
        }),
        Encode::NegativeZeroMask {
            message_number,
            mask,
        } => serde_json::json!({
            "kind": "negativeZeroMask",
            "messageNumber": message_number,
            "mask": mask,
        }),
        Encode::MessageNumber {
            message_number,
            record,
        } => serde_json::json!({
            "kind": "messageNumber",
            "messageNumber": message_number,
            "record": rtcm_record_kind_value(*record),
        }),
        Encode::FieldPresence {
            message_number,
            record,
            field,
            carried,
        } => serde_json::json!({
            "kind": "fieldPresence",
            "messageNumber": message_number,
            "record": rtcm_record_kind_value(*record),
            "field": field,
            "carried": carried,
        }),
        Encode::SatelliteFieldPresence {
            message_number,
            record,
            satellite,
            field,
            carried,
        } => serde_json::json!({
            "kind": "satelliteFieldPresence",
            "messageNumber": message_number,
            "record": rtcm_record_kind_value(*record),
            "satellite": satellite,
            "field": field,
            "carried": carried,
        }),
        Encode::CountMismatch {
            message_number,
            field,
            expected,
            actual,
        } => serde_json::json!({
            "kind": "countMismatch",
            "messageNumber": message_number,
            "field": field,
            "expected": expected,
            "actual": actual,
        }),
        Encode::ValueOutOfRange {
            message_number,
            field,
            value,
            minimum,
            maximum,
        } => serde_json::json!({
            "kind": "valueOutOfRange",
            "messageNumber": message_number,
            "field": field,
            "value": value.to_string(),
            "minimum": minimum.to_string(),
            "maximum": maximum.to_string(),
        }),
        Encode::NonLatin1Character { field, character } => serde_json::json!({
            "kind": "nonLatin1Character",
            "field": field,
            "character": character.to_string(),
        }),
        Encode::SatelliteIdOutOfRange {
            message_number,
            field,
            value,
            width,
        } => serde_json::json!({
            "kind": "satelliteIdOutOfRange",
            "messageNumber": message_number,
            "field": field,
            "value": value,
            "width": width,
        }),
        Encode::SsrSatelliteCount {
            message_number,
            declared,
            records,
        } => serde_json::json!({
            "kind": "ssrSatelliteCount",
            "messageNumber": message_number,
            "declared": declared,
            "records": records,
        }),
        Encode::MsmMask {
            message_number,
            problem,
        } => {
            let detail = match problem {
                MsmMaskProblem::SatelliteOutsideMask { satellite } => {
                    serde_json::json!({"kind": "satelliteOutsideMask", "satellite": satellite})
                }
                MsmMaskProblem::SatelliteListedTwice { satellite } => {
                    serde_json::json!({"kind": "satelliteListedTwice", "satellite": satellite})
                }
                MsmMaskProblem::SignalOutsideMask { signal } => {
                    serde_json::json!({"kind": "signalOutsideMask", "signal": signal})
                }
                MsmMaskProblem::SignalNotInMask { signal, mask } => {
                    serde_json::json!({"kind": "signalNotInMask", "signal": signal, "mask": mask})
                }
                MsmMaskProblem::SignalSatelliteNotListed { signal, satellite } => {
                    serde_json::json!({"kind": "signalSatelliteNotListed", "signal": signal, "satellite": satellite})
                }
                MsmMaskProblem::CellListedTwice { satellite, signal } => {
                    serde_json::json!({"kind": "cellListedTwice", "satellite": satellite, "signal": signal})
                }
                _ => serde_json::json!({"kind":"unrecognizedMsmMaskProblem"}),
            };
            serde_json::json!({
                "kind": "msmMask",
                "messageNumber": message_number,
                "problem": detail,
            })
        }
        Encode::SsrSatelliteIdOutOfRange {
            message_number,
            value,
            width,
        } => serde_json::json!({
            "kind": "ssrSatelliteIdOutOfRange", "messageNumber": message_number, "value": value, "width": width,
        }),
        Encode::SsrRecordsNotCarried {
            message_number,
            kind,
            records,
            count,
        } => serde_json::json!({
            "kind": "ssrRecordsNotCarried", "messageNumber": message_number, "ssrKind": ssr_kind_label(*kind), "records": records, "count": count,
        }),
        Encode::SsrCombinedRecordCounts {
            message_number,
            orbit,
            clock,
        } => serde_json::json!({
            "kind": "ssrCombinedRecordCounts", "messageNumber": message_number, "orbit": orbit, "clock": clock,
        }),
        Encode::SsrCombinedSatelliteMismatch {
            message_number,
            index,
            orbit_satellite,
            clock_satellite,
        } => serde_json::json!({
            "kind": "ssrCombinedSatelliteMismatch", "messageNumber": message_number, "index": index, "orbitSatellite": orbit_satellite, "clockSatellite": clock_satellite,
        }),
        Encode::SsrHighRateClockTerms {
            message_number,
            satellite,
            c1,
            c2,
        } => serde_json::json!({
            "kind": "ssrHighRateClockTerms", "messageNumber": message_number, "satellite": satellite, "c1": c1, "c2": c2,
        }),
        Encode::MsmOptional {
            message_number,
            kind,
            satellite,
            signal,
            field,
            problem,
        } => {
            let field = match field {
                sidereon_core::rtcm::MsmOptionalField::ExtendedInfo => "extendedInfo",
                sidereon_core::rtcm::MsmOptionalField::RoughPhaseRangeRate => "roughPhaseRangeRate",
                sidereon_core::rtcm::MsmOptionalField::FinePhaseRangeRate => "finePhaseRangeRate",
                _ => "other",
            };
            let problem = match problem {
                sidereon_core::rtcm::MsmOptionalProblem::Missing => {
                    serde_json::json!({"kind":"missing"})
                }
                sidereon_core::rtcm::MsmOptionalProblem::NotCarried => {
                    serde_json::json!({"kind":"notCarried"})
                }
                sidereon_core::rtcm::MsmOptionalProblem::InvalidValue(value) => {
                    serde_json::json!({"kind":"invalidValue", "value": value.to_string()})
                }
                _ => serde_json::json!({"kind":"unrecognized"}),
            };
            serde_json::json!({"kind":"msmOptional", "messageNumber":message_number, "messageKind":msm_kind_label(*kind), "satellite":satellite, "signal":signal, "field":field, "problem":problem})
        }
        Encode::TrailingZeroBits {
            message_number,
            bits,
        } => serde_json::json!({
            "kind": "trailingZeroBits", "messageNumber": message_number, "bits": bits,
        }),
        Encode::StrictDeparture(departure) => serde_json::json!({
            "kind": "strictDeparture", "departure": rtcm_departure_payload(departure),
        }),
        Encode::UnsupportedBodyTooShort { message_number } => serde_json::json!({
            "kind": "unsupportedBodyTooShort", "messageNumber": message_number,
        }),
        Encode::UnsupportedBodyNumber {
            message_number,
            carried,
        } => serde_json::json!({
            "kind": "unsupportedBodyNumber", "messageNumber": message_number, "carried": carried,
        }),
        Encode::UnsupportedDecodedNumber { message_number } => serde_json::json!({
            "kind": "unsupportedDecodedNumber", "messageNumber": message_number,
        }),
        Encode::FrameBodyTooLong { len } => {
            serde_json::json!({"kind":"frameBodyTooLong", "len":len})
        }
        Encode::FrameReservedOutOfRange { value } => {
            serde_json::json!({"kind":"frameReservedOutOfRange", "value":value})
        }
        _ => serde_json::json!({"kind":"unrecognizedRtcmEncodeError"}),
    }
}

fn rtcm_conversion_error_payload(
    error: &sidereon_core::rtcm::RtcmConversionError,
) -> serde_json::Value {
    use sidereon_core::rtcm::RtcmConversionError as Convert;
    match error {
        Convert::SatelliteIdOutOfRange {
            message_number,
            field,
            value,
            width,
        } => serde_json::json!({
            "kind": "satelliteIdOutOfRange",
            "messageNumber": message_number,
            "field": field,
            "value": value,
            "width": width,
        }),
        Convert::InvalidSatellite {
            message_number,
            field,
            value,
            error,
        } => {
            let err_json = match error {
                sidereon_core::SatelliteIdError::InvalidInput { field, reason } => {
                    serde_json::json!({"kind": "invalidInput", "field": field, "reason": reason})
                }
            };
            serde_json::json!({
                "kind": "invalidSatellite",
                "messageNumber": message_number,
                "field": field,
                "value": value,
                "error": err_json,
            })
        }
        Convert::SbasPrnOutsideWindow {
            value,
            broadcast_prn,
        } => serde_json::json!({
            "kind": "sbasPrnOutsideWindow",
            "value": value,
            "broadcastPrn": broadcast_prn,
        }),
        Convert::NoLnavRecord { value, satellite } => serde_json::json!({
            "kind": "noLnavRecord",
            "value": value,
            "satellite": satellite.to_string(),
        }),
        Convert::WeekMismatch {
            message_number,
            full_week,
            week,
        } => serde_json::json!({
            "kind": "weekMismatch",
            "messageNumber": message_number,
            "fullWeek": full_week,
            "week": week,
        }),
        Convert::NavicWeekMismatch { full_week, week } => serde_json::json!({
            "kind": "navicWeekMismatch", "fullWeek": full_week, "week": week,
        }),
        Convert::TimeNotRepresentable { field } => {
            serde_json::json!({"kind":"timeNotRepresentable", "field":field})
        }
        Convert::GalileoWeekOverflow => serde_json::json!({"kind":"galileoWeekOverflow"}),
        Convert::SisaSpare { index } => serde_json::json!({"kind":"sisaSpare", "index":index}),
        Convert::SisaNoPrediction => serde_json::json!({"kind":"sisaNoPrediction"}),
        Convert::UraOutOfRange { system, index } => {
            serde_json::json!({"kind":"uraOutOfRange", "system":system.as_str(), "index":index})
        }
        Convert::UraNoPrediction { system, index } => {
            serde_json::json!({"kind":"uraNoPrediction", "system":system.as_str(), "index":index})
        }
        Convert::FitInterval(error) => {
            use sidereon_core::ephemeris::LnavRecordError;
            let error = match error {
                LnavRecordError::NotGps(satellite) => {
                    serde_json::json!({"kind":"notGps", "satellite":satellite.to_string()})
                }
                LnavRecordError::InvalidEpoch(field) => {
                    serde_json::json!({"kind":"invalidEpoch", "field":field})
                }
                LnavRecordError::WeekMismatch {
                    full_week,
                    decoded_week,
                } => {
                    serde_json::json!({"kind":"weekMismatch", "fullWeek":full_week, "decodedWeek":decoded_week.to_string()})
                }
                LnavRecordError::NoUraPrediction(index) => {
                    serde_json::json!({"kind":"noUraPrediction", "index":index.to_string()})
                }
                LnavRecordError::FitIntervalUnsupported {
                    fit_interval_flag,
                    iode,
                    iodc,
                } => {
                    serde_json::json!({"kind":"fitIntervalUnsupported", "fitIntervalFlag":fit_interval_flag.to_string(), "iode":iode.to_string(), "iodc":iodc.to_string()})
                }
            };
            serde_json::json!({"kind":"fitInterval", "error":error})
        }
        Convert::VtecEvaluation(problem) => {
            serde_json::json!({"kind":"vtecEvaluation", "problem":vtec_problem_payload(problem)})
        }
        _ => serde_json::json!({"kind":"unrecognizedRtcmConversionError"}),
    }
}

fn rtcm_departure_payload(departure: &sidereon_core::rtcm::RtcmDeparture) -> serde_json::Value {
    use sidereon_core::rtcm::RtcmDeparture as D;
    match departure {
        D::FrameReservedBits { reserved } => {
            serde_json::json!({"kind":"frameReservedBits", "reserved":reserved, "message":departure.to_string()})
        }
        D::TrailingBits {
            message_number,
            bits,
        } => {
            serde_json::json!({"kind":"trailingBits", "messageNumber":message_number, "bits":bits, "message":departure.to_string()})
        }
        D::MsmCellMaskOver64 {
            message_number,
            cells,
        } => {
            serde_json::json!({"kind":"msmCellMaskOver64", "messageNumber":message_number, "cells":cells, "message":departure.to_string()})
        }
        D::OrderExceedsDegree {
            message_number,
            layer_index,
            degree,
            order,
        } => {
            serde_json::json!({"kind":"orderExceedsDegree", "messageNumber":message_number, "layerIndex":layer_index, "degree":degree, "order":order, "message":departure.to_string()})
        }
        D::SsrRecordsShort {
            message_number,
            declared,
            read,
        } => {
            serde_json::json!({"kind":"ssrRecordsShort", "messageNumber":message_number, "declared":declared, "read":read, "message":departure.to_string()})
        }
        D::RecordsShort {
            message_number,
            declared,
            read,
        } => {
            serde_json::json!({"kind":"recordsShort", "messageNumber":message_number, "declared":declared, "read":read, "message":departure.to_string()})
        }
        _ => serde_json::json!({"kind":"unrecognizedDeparture", "message":departure.to_string()}),
    }
}

fn vtec_problem_payload(problem: &sidereon_core::rtcm::VtecEvaluationProblem) -> serde_json::Value {
    use sidereon_core::rtcm::VtecEvaluationProblem as P;
    match problem {
        P::ComputationTime => serde_json::json!({"kind":"computationTime"}),
        P::Frequency => serde_json::json!({"kind":"frequency"}),
        P::NonFiniteCoordinates => serde_json::json!({"kind":"nonFiniteCoordinates"}),
        P::MessageIdentity { message_number } => {
            serde_json::json!({"kind":"messageIdentity", "messageNumber":message_number})
        }
        P::LayerCount { layers } => serde_json::json!({"kind":"layerCount", "layers":layers}),
        P::InvalidGeometry => serde_json::json!({"kind":"invalidGeometry"}),
        P::BelowHorizon => serde_json::json!({"kind":"belowHorizon"}),
        P::LayerDegreeOrder {
            layer_index,
            degree,
            order,
        } => {
            serde_json::json!({"kind":"layerDegreeOrder", "layerIndex":layer_index, "degree":degree, "order":order})
        }
        P::CoefficientCounts {
            layer_index,
            cosine_expected,
            cosine_actual,
            sine_expected,
            sine_actual,
        } => {
            serde_json::json!({"kind":"coefficientCounts", "layerIndex":layer_index, "cosineExpected":cosine_expected, "cosineActual":cosine_actual, "sineExpected":sine_expected, "sineActual":sine_actual})
        }
        P::UnavailableCoefficient { layer_index } => {
            serde_json::json!({"kind":"unavailableCoefficient", "layerIndex":layer_index})
        }
        P::ShellNotAboveReceiver { layer_index } => {
            serde_json::json!({"kind":"shellNotAboveReceiver", "layerIndex":layer_index})
        }
        P::MissingCoefficient {
            layer_index,
            field,
            index,
        } => {
            serde_json::json!({"kind":"missingCoefficient", "layerIndex":layer_index, "field":field, "index":index})
        }
        P::InvalidMappingFactor { layer_index } => {
            serde_json::json!({"kind":"invalidMappingFactor", "layerIndex":layer_index})
        }
        P::PhysicalResultOutOfRange { field } => {
            serde_json::json!({"kind":"physicalResultOutOfRange", "field":field})
        }
        _ => serde_json::json!({"kind":"unrecognizedVtecProblem"}),
    }
}

impl From<&CoreError> for CoreErrorDetail {
    fn from(error: &CoreError) -> Self {
        match error {
            CoreError::Parse(msg) => Self::Parse {
                message: msg.clone(),
            },
            CoreError::UnknownSatellite(sat) => Self::UnknownSatellite {
                satellite_id: sat.to_string(),
            },
            CoreError::MissingGlonassChannel => Self::MissingGlonassChannel,
            CoreError::MissingTerrainTile {
                lat_index,
                lon_index,
            } => Self::MissingTerrainTile {
                lat_index: *lat_index,
                lon_index: *lon_index,
            },
            CoreError::UnknownTerrainElevation {
                lat_index,
                lon_index,
                latitude_posting,
                longitude_posting,
            } => Self::UnknownTerrainElevation {
                lat_index: *lat_index,
                lon_index: *lon_index,
                latitude_posting: *latitude_posting,
                longitude_posting: *longitude_posting,
            },
            CoreError::NonWgs84TerrainTile {
                lat_index,
                lon_index,
                datum,
            } => Self::NonWgs84TerrainTile {
                lat_index: *lat_index,
                lon_index: *lon_index,
                datum: datum.to_string(),
            },
            CoreError::TerrainTile {
                lat_index,
                lon_index,
                error,
            } => Self::TerrainTile {
                lat_index: *lat_index,
                lon_index: *lon_index,
                cause: dted_tile_error_payload(error),
            },
            CoreError::TerrainTileOrigin {
                path,
                lat_index,
                lon_index,
                origin_latitude,
                origin_longitude,
            } => Self::TerrainTileOrigin {
                path: path.display().to_string(),
                lat_index: *lat_index,
                lon_index: *lon_index,
                origin_latitude: *origin_latitude,
                origin_longitude: *origin_longitude,
            },
            CoreError::IonexOutOfCoverage(err) => Self::IonexOutOfCoverage {
                cause: crate::ionex::IonexCoverageErrorJs::from_core(*err),
            },
            CoreError::IonexNodesNotAvailable(err) => Self::IonexNodesNotAvailable {
                cause: crate::ionex::IonexNodeGapJs::from_core(**err),
            },
            CoreError::IonexSlantUnavailable(_) => Self::IonexSlantUnavailable {
                cause: crate::ionex::IonexSlantRefusalJs::from_core_error(error),
            },
            CoreError::IonexEpoch(err) => Self::IonexEpoch {
                cause: ionex_epoch_error_payload(err),
            },
            CoreError::EpochOutOfRange => Self::EpochOutOfRange,
            CoreError::InsufficientPreciseNodes {
                sat,
                nodes,
                required,
            } => Self::InsufficientPreciseNodes {
                satellite_id: sat.to_string(),
                nodes: *nodes,
                required: *required,
            },
            CoreError::InvalidInput(msg) => Self::InvalidInput {
                message: msg.clone(),
            },
            CoreError::Sp3EpochInterval(err) => Self::Sp3EpochInterval {
                field: err.field.to_string(),
                value: exact_float(err.value),
                reason: err.reason.to_string(),
            },
            CoreError::Sp3MergeTolerance(err) => {
                let field = match err.field {
                    sidereon_core::ephemeris::MergeToleranceField::Position => "positionToleranceM",
                    sidereon_core::ephemeris::MergeToleranceField::Clock => "clockToleranceS",
                    sidereon_core::ephemeris::MergeToleranceField::OutlierPosition => {
                        "outlierReject.positionToleranceM"
                    }
                    sidereon_core::ephemeris::MergeToleranceField::OutlierClock => {
                        "outlierReject.clockToleranceS"
                    }
                    _ => {
                        return Self::Other {
                            message: error.to_string(),
                            variant: upper_snake_variant(error).into_owned(),
                            debug: format!("{error:?}"),
                        };
                    }
                };
                Self::Sp3MergeTolerance {
                    field: field.to_string(),
                    value: exact_float(err.value),
                    reason: "must be finite and nonnegative".to_string(),
                }
            }
            CoreError::ContinuityOptions(err) => {
                let reason = match err.reason {
                    sidereon_core::ephemeris::ContinuityOptionRejection::NotFinite => "not_finite",
                    sidereon_core::ephemeris::ContinuityOptionRejection::Negative => "negative",
                    _ => {
                        return Self::Other {
                            message: error.to_string(),
                            variant: upper_snake_variant(error).into_owned(),
                            debug: format!("{error:?}"),
                        };
                    }
                };
                Self::ContinuityOptions {
                    field: err.field,
                    value: exact_float(err.value),
                    reason,
                }
            }
            CoreError::SbasEncode(err) => Self::SbasEncode {
                cause: sbas_encode_error_payload(err),
            },
            CoreError::RtcmEncode(err) => Self::RtcmEncode {
                cause: rtcm_encode_error_payload(err),
            },
            CoreError::RtcmConversion(err) => Self::RtcmConversion {
                cause: rtcm_conversion_error_payload(err),
            },
            CoreError::Ut1OutsideCoverage(reason) => Self::Ut1OutsideCoverage {
                reason: match reason {
                    sidereon_core::astro::time::DegradeReason::BeforeCoverage => "beforeCoverage",
                    sidereon_core::astro::time::DegradeReason::AfterCoverage => "afterCoverage",
                },
            },
            other => Self::Other {
                message: other.to_string(),
                variant: upper_snake_variant(other).into_owned(),
                debug: format!("{other:?}"),
            },
        }
    }
}

/// The structured detail of an [`ObservablesError`].
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind")]
pub enum ObservablesErrorDetail {
    #[serde(rename = "INVALID_INPUT", rename_all = "camelCase")]
    InvalidInput {
        message: String,
        field: &'static str,
        reason: &'static str,
    },
    #[serde(rename = "NO_EPHEMERIS", rename_all = "camelCase")]
    NoEphemeris { message: String },
    #[serde(rename = "EPHEMERIS", rename_all = "camelCase")]
    Ephemeris {
        message: String,
        cause: CoreErrorDetail,
    },
    #[serde(rename = "MEDIA", rename_all = "camelCase")]
    Media {
        message: String,
        cause: CoreErrorDetail,
    },
}

impl ObservablesErrorDetail {
    /// Return the error message text.
    pub fn message(&self) -> &str {
        match self {
            Self::InvalidInput { message, .. } => message,
            Self::NoEphemeris { message } => message,
            Self::Ephemeris { message, .. } => message,
            Self::Media { message, .. } => message,
        }
    }
}

fn observables_input_reason_label(kind: ObservablesInputErrorKind) -> &'static str {
    match kind {
        ObservablesInputErrorKind::NonFinite => "not finite",
        ObservablesInputErrorKind::NotPositive => "not positive",
        ObservablesInputErrorKind::Negative => "negative",
        ObservablesInputErrorKind::OutOfRange => "out of range",
        ObservablesInputErrorKind::Missing => "missing",
        ObservablesInputErrorKind::FloatParse => "invalid float",
        ObservablesInputErrorKind::IntParse => "invalid integer",
        ObservablesInputErrorKind::InvalidCivilDate => "invalid civil date",
        ObservablesInputErrorKind::InvalidCivilTime => "invalid civil time",
    }
}

impl From<&ObservablesError> for ObservablesErrorDetail {
    fn from(error: &ObservablesError) -> Self {
        let message = error.to_string();
        match error {
            ObservablesError::InvalidInput { field, kind } => Self::InvalidInput {
                message,
                field,
                reason: observables_input_reason_label(*kind),
            },
            ObservablesError::NoEphemeris => Self::NoEphemeris { message },
            ObservablesError::Ephemeris(err) => Self::Ephemeris {
                message,
                cause: CoreErrorDetail::from(err),
            },
            ObservablesError::Media(err) => Self::Media {
                message,
                cause: CoreErrorDetail::from(err),
            },
        }
    }
}

/// The structured detail of a [`VelocityError`].
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind")]
pub enum VelocityErrorDetail {
    #[serde(rename = "NO_OBSERVATIONS", rename_all = "camelCase")]
    NoObservations { message: String },
    #[serde(rename = "TOO_FEW_SATELLITES", rename_all = "camelCase")]
    TooFewSatellites {
        message: String,
        used: usize,
        required: usize,
    },
    #[serde(rename = "SINGULAR_GEOMETRY", rename_all = "camelCase")]
    SingularGeometry { message: String },
    #[serde(rename = "DUPLICATE_OBSERVATION", rename_all = "camelCase")]
    DuplicateObservation {
        message: String,
        satellite_id: String,
    },
    #[serde(rename = "INVALID_CARRIER", rename_all = "camelCase")]
    InvalidCarrier {
        message: String,
        satellite_id: String,
    },
    #[serde(rename = "INVALID_INPUT", rename_all = "camelCase")]
    InvalidInput {
        message: String,
        field: &'static str,
        reason: &'static str,
    },
    #[serde(rename = "INVALID_OBSERVATION", rename_all = "camelCase")]
    InvalidObservation {
        message: String,
        satellite_id: String,
    },
    #[serde(rename = "INVALID_RECEIVER_STATE", rename_all = "camelCase")]
    InvalidReceiverState { message: String },
}

impl VelocityErrorDetail {
    /// Return the error message text.
    pub fn message(&self) -> &str {
        match self {
            Self::NoObservations { message } => message,
            Self::TooFewSatellites { message, .. } => message,
            Self::SingularGeometry { message } => message,
            Self::DuplicateObservation { message, .. } => message,
            Self::InvalidCarrier { message, .. } => message,
            Self::InvalidInput { message, .. } => message,
            Self::InvalidObservation { message, .. } => message,
            Self::InvalidReceiverState { message } => message,
        }
    }
}

impl From<&VelocityError> for VelocityErrorDetail {
    fn from(error: &VelocityError) -> Self {
        let message = error.to_string();
        match error {
            VelocityError::NoObservations => Self::NoObservations { message },
            VelocityError::TooFewSatellites { used, required } => Self::TooFewSatellites {
                message,
                used: *used,
                required: *required,
            },
            VelocityError::SingularGeometry => Self::SingularGeometry { message },
            VelocityError::DuplicateObservation { satellite_id } => Self::DuplicateObservation {
                message,
                satellite_id: satellite_id.to_string(),
            },
            VelocityError::InvalidCarrier { satellite_id } => Self::InvalidCarrier {
                message,
                satellite_id: satellite_id.to_string(),
            },
            VelocityError::InvalidInput { field, reason } => Self::InvalidInput {
                message,
                field,
                reason,
            },
            VelocityError::InvalidObservation { satellite_id } => Self::InvalidObservation {
                message,
                satellite_id: satellite_id.to_string(),
            },
            VelocityError::InvalidReceiverState => Self::InvalidReceiverState { message },
        }
    }
}

/// Create a JavaScript `Error` with `name = "Error"`, carrying message, `.detail`, and `.cause`.
pub fn observables_error_js(detail: &ObservablesErrorDetail) -> JsValue {
    error_with_typed_cause(detail.message(), detail, "observables error detail")
}

/// Create a JavaScript `Error` with `name = "Error"`, carrying message, `.detail`, and `.cause`.
pub fn velocity_error_js(error: &VelocityError) -> JsValue {
    let detail = VelocityErrorDetail::from(error);
    error_with_typed_cause(detail.message(), &detail, "velocity error detail")
}

/// Create an `Error` carrying a core error's typed detail as both `.detail`
/// and `.cause`, refusing to return it bare if either attachment fails.
pub(crate) fn core_error_js(error: &CoreError) -> JsValue {
    let detail = CoreErrorDetail::from(error);
    error_with_typed_cause(&error.to_string(), &detail, "core error detail")
}

fn error_with_typed_cause<T: Serialize>(message: &str, detail: &T, what: &str) -> JsValue {
    let error = crate::error::error_with_detail("Error", message, detail);
    let detail_js = match js_sys::Reflect::get(&error, &JsValue::from_str("detail")) {
        Ok(value) if !value.is_undefined() => value,
        Ok(_) => {
            return crate::error::engine_error(format!(
                "{message} (the typed detail for {what} was not attached: {})",
                crate::error::describe_js(&error)
            ));
        }
        Err(cause) => {
            return crate::error::engine_error(format!(
                "{message} (reading the typed detail for {what} threw: {})",
                crate::error::describe_js(&cause)
            ));
        }
    };
    match js_sys::Reflect::set(&error, &JsValue::from_str("cause"), &detail_js) {
        Ok(true) => error,
        Ok(false) => crate::error::engine_error(format!(
            "{message} (the typed cause could not be attached for {what})"
        )),
        Err(cause) => crate::error::engine_error(format!(
            "{message} (attaching the typed cause for {what} threw: {})",
            crate::error::describe_js(&cause)
        )),
    }
}

#[wasm_bindgen::prelude::wasm_bindgen(typescript_custom_section)]
const TS_CORE_ERROR: &str = r#"
export type ObservablesErrorDetail =
  | { kind: "INVALID_INPUT"; message: string; field: string; reason: string }
  | { kind: "NO_EPHEMERIS"; message: string }
  | { kind: "EPHEMERIS"; message: string; cause: CoreErrorDetail }
  | { kind: "MEDIA"; message: string; cause: CoreErrorDetail };

export type VelocityErrorDetail =
  | { kind: "NO_OBSERVATIONS"; message: string }
  | { kind: "TOO_FEW_SATELLITES"; message: string; used: number; required: number }
  | { kind: "SINGULAR_GEOMETRY"; message: string }
  | { kind: "DUPLICATE_OBSERVATION"; message: string; satelliteId: string }
  | { kind: "INVALID_CARRIER"; message: string; satelliteId: string }
  | { kind: "INVALID_INPUT"; message: string; field: string; reason: string }
  | { kind: "INVALID_OBSERVATION"; message: string; satelliteId: string }
  | { kind: "INVALID_RECEIVER_STATE"; message: string };

export type ScalarObservableErrorDetail =
  | { family: "IonosphereFreeError"; kind: "unknown_system"; message: string; system: string }
  | { family: "IonosphereFreeError"; kind: "unknown_band"; message: string; system: string; band: string }
  | { family: "IonosphereFreeError"; kind: "equal_frequencies" | "invalid_frequency" | "invalid_observation"; message: string }
  | { family: "CarrierPhaseError"; kind: "equal_frequencies" | "invalid_frequency" | "invalid_observation" | "invalid_threshold"; message: string }
  | { family: "SignalError"; kind: "unsupported_prn"; message: string; prn: string }
  | { family: "SignalError"; kind: "invalid_input"; message: string; field: string; reason: string }
  | { family: "SignalError"; kind: "empty_samples" | "too_short"; message: string };

export type VelocityRangeErrorDetail = VelocityErrorDetail & { family: "VelocityError" };
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use sidereon_core::astro::time::TimeScale;
    use sidereon_core::atmosphere::{IonexCoverageError, IonexEpochError};
    use sidereon_core::ephemeris::{
        ContinuityOptionRejection, ContinuityOptionsError, MergeToleranceError,
        MergeToleranceField, Sp3EpochIntervalError, Sp3EpochIntervalRejection,
    };
    use sidereon_core::terrain::DtedHorizontalDatum;
    use sidereon_core::GnssSatelliteId;

    #[test]
    fn dted_payload_table_preserves_every_current_variant_field() {
        use sidereon_core::terrain::DtedTileError as E;
        let cases = [
            (
                E::Io {
                    path: "tile".into(),
                    message: "io".into(),
                },
                serde_json::json!({"kind":"io","path":"tile","message":"io"}),
            ),
            (
                E::TooShort {
                    path: "tile".into(),
                },
                serde_json::json!({"kind":"tooShort","path":"tile"}),
            ),
            (
                E::MissingUhl1 {
                    path: "tile".into(),
                },
                serde_json::json!({"kind":"missingUhl1","path":"tile"}),
            ),
            (
                E::InvalidEncoding("bytes".into()),
                serde_json::json!({"kind":"invalidEncoding","message":"bytes"}),
            ),
            (
                E::InvalidField("number".into()),
                serde_json::json!({"kind":"invalidField","message":"number"}),
            ),
            (
                E::InvalidDimensions {
                    path: "tile".into(),
                    lon_count: 2,
                    lat_count: 3,
                },
                serde_json::json!({"kind":"invalidDimensions","path":"tile","lonCount":2,"latCount":3}),
            ),
            (
                E::Truncated {
                    path: "tile".into(),
                    actual: 4,
                    expected: 8,
                },
                serde_json::json!({"kind":"truncated","path":"tile","actual":4,"expected":8}),
            ),
            (
                E::Outside {
                    longitude: -0.0,
                    latitude: f64::from_bits(0x7ff8_0000_0000_0001),
                    origin_longitude: 4.0,
                    origin_latitude: -5.0,
                },
                serde_json::json!({"kind":"outside","longitude":exact_float(-0.0),"latitude":exact_float(f64::from_bits(0x7ff8_0000_0000_0001)),"originLongitude":exact_float(4.0),"originLatitude":exact_float(-5.0)}),
            ),
            (
                E::PostingIndexOutOfBounds {
                    longitude_index: 5,
                    latitude_index: 6,
                },
                serde_json::json!({"kind":"postingIndexOutOfBounds","longitudeIndex":5,"latitudeIndex":6}),
            ),
            (
                E::MissingDataSentinel { longitude_index: 7 },
                serde_json::json!({"kind":"missingDataSentinel","longitudeIndex":7}),
            ),
            (
                E::Checksum {
                    longitude_index: 8,
                    checksum: -9,
                    sum: 10,
                },
                serde_json::json!({"kind":"checksum","longitudeIndex":8,"checksum":-9,"sum":10}),
            ),
            (
                E::EmptyCoordinate,
                serde_json::json!({"kind":"emptyCoordinate"}),
            ),
            (
                E::InvalidHemisphere { hemisphere: 'Q' },
                serde_json::json!({"kind":"invalidHemisphere","hemisphere":"Q"}),
            ),
            (
                E::NegativePostingIndex { index: -11 },
                serde_json::json!({"kind":"negativePostingIndex","index":"-11"}),
            ),
            (
                E::CoordinateOutOfRange {
                    field: "latitude",
                    text: "9001N".into(),
                },
                serde_json::json!({"kind":"coordinateOutOfRange","field":"latitude","text":"9001N"}),
            ),
            (
                E::WrongHemisphere {
                    field: "latitude",
                    hemisphere: 'E',
                    expected: "N or S",
                },
                serde_json::json!({"kind":"wrongHemisphere","field":"latitude","hemisphere":"E","expected":"N or S"}),
            ),
            (
                E::OriginNotWholeDegree {
                    field: "longitude",
                    text: "001230E".into(),
                },
                serde_json::json!({"kind":"originNotWholeDegree","field":"longitude","text":"001230E"}),
            ),
            (
                E::IntervalCountMismatch {
                    field: "longitude",
                    interval_tenths_arcsec: 11,
                    count: 12,
                },
                serde_json::json!({"kind":"intervalCountMismatch","field":"longitude","intervalTenthsArcsec":11,"count":12}),
            ),
            (
                E::ProfileLongitudeCountMismatch {
                    longitude_index: 13,
                    declared: -14,
                },
                serde_json::json!({"kind":"profileLongitudeCountMismatch","longitudeIndex":13,"declared":-14}),
            ),
            (
                E::UnsupportedPartialProfile {
                    longitude_index: 15,
                    first_latitude_index: -16,
                },
                serde_json::json!({"kind":"unsupportedPartialProfile","longitudeIndex":15,"firstLatitudeIndex":-16}),
            ),
            (
                E::NullPosting {
                    longitude_index: 17,
                    latitude_index: 18,
                },
                serde_json::json!({"kind":"nullPosting","longitudeIndex":17,"latitudeIndex":18}),
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(dted_tile_error_payload(&error), expected);
        }
    }

    #[test]
    fn sbas_payload_table_preserves_every_current_variant_field() {
        use sidereon_core::sbas::SbasEncodeError as E;
        let cases = [
            (
                E::FieldOutOfRange {
                    message_type: 1,
                    field: "f",
                    index: Some(2),
                    value: i128::MIN,
                    width: 3,
                    signed: true,
                },
                serde_json::json!({"kind":"fieldOutOfRange","messageType":1,"field":"f","index":2,"value":i128::MIN.to_string(),"width":3,"signed":true}),
            ),
            (
                E::UnrecognizedPreamble { preamble: 0x53 },
                serde_json::json!({"kind":"unrecognizedPreamble","preamble":0x53}),
            ),
            (
                E::MessageType {
                    message_type: 4,
                    reason: "reason",
                },
                serde_json::json!({"kind":"messageType","messageType":4,"reason":"reason"}),
            ),
            (
                E::RawPayload {
                    message_type: 5,
                    bytes: 27,
                    bits_past_payload: true,
                },
                serde_json::json!({"kind":"rawPayload","messageType":5,"bytes":27,"bitsPastPayload":true}),
            ),
            (
                E::ReservedLayout {
                    message_type: 6,
                    part: "part",
                    expected: vec![1, 2],
                    found: vec![3],
                },
                serde_json::json!({"kind":"reservedLayout","messageType":6,"part":"part","expected":[1,2],"found":[3]}),
            ),
            (
                E::LongTermRecordCount {
                    message_type: 7,
                    half: 1,
                    velocity_code: false,
                    expected: 2,
                    found: 1,
                },
                serde_json::json!({"kind":"longTermRecordCount","messageType":7,"half":1,"velocityCode":false,"expected":2,"found":1}),
            ),
            (
                E::LongTermFieldNotCarried {
                    message_type: 8,
                    half: 2,
                    record: 3,
                    field: "rate",
                },
                serde_json::json!({"kind":"longTermFieldNotCarried","messageType":8,"half":2,"record":3,"field":"rate"}),
            ),
            (
                E::LongTermMissingTimeOfDay {
                    message_type: 9,
                    half: 4,
                },
                serde_json::json!({"kind":"longTermMissingTimeOfDay","messageType":9,"half":4}),
            ),
            (
                E::PadBits { value: 0x2a },
                serde_json::json!({"kind":"padBits","value":0x2a}),
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(sbas_encode_error_payload(&error), expected);
        }
    }

    #[test]
    fn rtcm_encode_payload_table_preserves_every_current_variant_field() {
        use sidereon_core::rtcm::{
            MsmKind, MsmMaskProblem, MsmOptionalField, MsmOptionalProblem, RtcmDeparture,
            RtcmEncodeError as E, RtcmFieldEncoding, RtcmRecordKind, SsrKind,
        };
        let record = RtcmRecordKind::StationCoordinates;
        let cases = [
            (
                E::FieldOutOfRange {
                    message_number: 1005,
                    field: "x".into(),
                    value: i128::MIN,
                    width: 5,
                    encoding: RtcmFieldEncoding::Unsigned,
                },
                serde_json::json!({"kind":"fieldOutOfRange","messageNumber":1005,"field":"x","value":i128::MIN.to_string(),"width":5,"encoding":"unsigned"}),
            ),
            (
                E::NegativeZeroWithValue {
                    message_number: 1006,
                    field: "height".into(),
                    value: i64::MIN,
                },
                serde_json::json!({"kind":"negativeZeroWithValue","messageNumber":1006,"field":"height","value":i64::MIN.to_string()}),
            ),
            (
                E::NegativeZeroMask {
                    message_number: 1020,
                    mask: 9,
                },
                serde_json::json!({"kind":"negativeZeroMask","messageNumber":1020,"mask":9}),
            ),
            (
                E::MessageNumber {
                    message_number: 1007,
                    record,
                },
                serde_json::json!({"kind":"messageNumber","messageNumber":1007,"record":{"kind":"stationCoordinates"}}),
            ),
            (
                E::FieldPresence {
                    message_number: 1006,
                    record,
                    field: "height",
                    carried: true,
                },
                serde_json::json!({"kind":"fieldPresence","messageNumber":1006,"record":{"kind":"stationCoordinates"},"field":"height","carried":true}),
            ),
            (
                E::SatelliteFieldPresence {
                    message_number: 1002,
                    record: RtcmRecordKind::LegacyObservations,
                    satellite: 3,
                    field: "carrier",
                    carried: false,
                },
                serde_json::json!({"kind":"satelliteFieldPresence","messageNumber":1002,"record":{"kind":"legacyObservations"},"satellite":3,"field":"carrier","carried":false}),
            ),
            (
                E::CountMismatch {
                    message_number: 1013,
                    field: "count",
                    expected: 4,
                    actual: 5,
                },
                serde_json::json!({"kind":"countMismatch","messageNumber":1013,"field":"count","expected":4,"actual":5}),
            ),
            (
                E::ValueOutOfRange {
                    message_number: 1005,
                    field: "x".into(),
                    value: i128::MIN,
                    minimum: -4,
                    maximum: 5,
                },
                serde_json::json!({"kind":"valueOutOfRange","messageNumber":1005,"field":"x","value":i128::MIN.to_string(),"minimum":"-4","maximum":"5"}),
            ),
            (
                E::NonLatin1Character {
                    field: "text".into(),
                    character: 'λ',
                },
                serde_json::json!({"kind":"nonLatin1Character","field":"text","character":"λ"}),
            ),
            (
                E::SatelliteIdOutOfRange {
                    message_number: 1019,
                    field: "GPS PRN",
                    value: 99,
                    width: 6,
                },
                serde_json::json!({"kind":"satelliteIdOutOfRange","messageNumber":1019,"field":"GPS PRN","value":99,"width":6}),
            ),
            (
                E::SsrSatelliteIdOutOfRange {
                    message_number: 1057,
                    value: 88,
                    width: 6,
                },
                serde_json::json!({"kind":"ssrSatelliteIdOutOfRange","messageNumber":1057,"value":88,"width":6}),
            ),
            (
                E::SsrRecordsNotCarried {
                    message_number: 1059,
                    kind: SsrKind::Orbit,
                    records: "clock",
                    count: 2,
                },
                serde_json::json!({"kind":"ssrRecordsNotCarried","messageNumber":1059,"ssrKind":"orbit","records":"clock","count":2}),
            ),
            (
                E::SsrCombinedRecordCounts {
                    message_number: 1060,
                    orbit: 2,
                    clock: 3,
                },
                serde_json::json!({"kind":"ssrCombinedRecordCounts","messageNumber":1060,"orbit":2,"clock":3}),
            ),
            (
                E::SsrCombinedSatelliteMismatch {
                    message_number: 1060,
                    index: 4,
                    orbit_satellite: 5,
                    clock_satellite: 6,
                },
                serde_json::json!({"kind":"ssrCombinedSatelliteMismatch","messageNumber":1060,"index":4,"orbitSatellite":5,"clockSatellite":6}),
            ),
            (
                E::SsrHighRateClockTerms {
                    message_number: 1062,
                    satellite: 7,
                    c1: -8,
                    c2: 9,
                },
                serde_json::json!({"kind":"ssrHighRateClockTerms","messageNumber":1062,"satellite":7,"c1":-8,"c2":9}),
            ),
            (
                E::SsrSatelliteCount {
                    message_number: 1057,
                    declared: 10,
                    records: 11,
                },
                serde_json::json!({"kind":"ssrSatelliteCount","messageNumber":1057,"declared":10,"records":11}),
            ),
            (
                E::MsmMask {
                    message_number: 1074,
                    problem: MsmMaskProblem::SignalNotInMask {
                        signal: 12,
                        mask: 13,
                    },
                },
                serde_json::json!({"kind":"msmMask","messageNumber":1074,"problem":{"kind":"signalNotInMask","signal":12,"mask":13}}),
            ),
            (
                E::MsmOptional {
                    message_number: 1077,
                    kind: MsmKind::Msm7,
                    satellite: 14,
                    signal: Some(15),
                    field: MsmOptionalField::FinePhaseRangeRate,
                    problem: MsmOptionalProblem::InvalidValue(i64::MIN),
                },
                serde_json::json!({"kind":"msmOptional","messageNumber":1077,"messageKind":"msm7","satellite":14,"signal":15,"field":"finePhaseRangeRate","problem":{"kind":"invalidValue","value":i64::MIN.to_string()}}),
            ),
            (
                E::TrailingZeroBits {
                    message_number: 1029,
                    bits: 3,
                },
                serde_json::json!({"kind":"trailingZeroBits","messageNumber":1029,"bits":3}),
            ),
            (
                E::StrictDeparture(RtcmDeparture::FrameReservedBits { reserved: 4 }),
                serde_json::json!({"kind":"strictDeparture","departure":{"kind":"frameReservedBits","reserved":4,"message":"RTCM frame reserved bits are 0x04, not zero"}}),
            ),
            (
                E::UnsupportedBodyTooShort {
                    message_number: 1200,
                },
                serde_json::json!({"kind":"unsupportedBodyTooShort","messageNumber":1200}),
            ),
            (
                E::UnsupportedBodyNumber {
                    message_number: 1201,
                    carried: 1202,
                },
                serde_json::json!({"kind":"unsupportedBodyNumber","messageNumber":1201,"carried":1202}),
            ),
            (
                E::UnsupportedDecodedNumber {
                    message_number: 1005,
                },
                serde_json::json!({"kind":"unsupportedDecodedNumber","messageNumber":1005}),
            ),
            (
                E::FrameBodyTooLong { len: 1024 },
                serde_json::json!({"kind":"frameBodyTooLong","len":1024}),
            ),
            (
                E::FrameReservedOutOfRange { value: 64 },
                serde_json::json!({"kind":"frameReservedOutOfRange","value":64}),
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(rtcm_encode_error_payload(&error), expected);
        }

        let mask_cases = [
            (
                MsmMaskProblem::SatelliteOutsideMask { satellite: 1 },
                serde_json::json!({"kind":"satelliteOutsideMask","satellite":1}),
            ),
            (
                MsmMaskProblem::SatelliteListedTwice { satellite: 2 },
                serde_json::json!({"kind":"satelliteListedTwice","satellite":2}),
            ),
            (
                MsmMaskProblem::SignalOutsideMask { signal: 3 },
                serde_json::json!({"kind":"signalOutsideMask","signal":3}),
            ),
            (
                MsmMaskProblem::SignalNotInMask { signal: 4, mask: 5 },
                serde_json::json!({"kind":"signalNotInMask","signal":4,"mask":5}),
            ),
            (
                MsmMaskProblem::SignalSatelliteNotListed {
                    signal: 6,
                    satellite: 7,
                },
                serde_json::json!({"kind":"signalSatelliteNotListed","signal":6,"satellite":7}),
            ),
            (
                MsmMaskProblem::CellListedTwice {
                    satellite: 8,
                    signal: 9,
                },
                serde_json::json!({"kind":"cellListedTwice","satellite":8,"signal":9}),
            ),
        ];
        for (problem, expected) in mask_cases {
            let detail = rtcm_encode_error_payload(&E::MsmMask {
                message_number: 1074,
                problem,
            });
            assert_eq!(detail["problem"], expected);
        }

        let optional_cases = [
            (
                MsmOptionalField::ExtendedInfo,
                MsmOptionalProblem::Missing,
                "extendedInfo",
                serde_json::json!({"kind":"missing"}),
            ),
            (
                MsmOptionalField::RoughPhaseRangeRate,
                MsmOptionalProblem::NotCarried,
                "roughPhaseRangeRate",
                serde_json::json!({"kind":"notCarried"}),
            ),
            (
                MsmOptionalField::FinePhaseRangeRate,
                MsmOptionalProblem::InvalidValue(i64::MIN),
                "finePhaseRangeRate",
                serde_json::json!({"kind":"invalidValue","value":i64::MIN.to_string()}),
            ),
        ];
        for (field, problem, expected_field, expected_problem) in optional_cases {
            let detail = rtcm_encode_error_payload(&E::MsmOptional {
                message_number: 1077,
                kind: MsmKind::Msm7,
                satellite: 2,
                signal: None,
                field,
                problem,
            });
            assert_eq!(detail["field"], expected_field);
            assert_eq!(detail["problem"], expected_problem);
            assert_eq!(detail["signal"], serde_json::Value::Null);
        }
    }

    #[test]
    fn rtcm_departure_payload_table_preserves_every_current_field() {
        use sidereon_core::rtcm::RtcmDeparture as D;
        let departures = [
            (
                D::FrameReservedBits { reserved: 1 },
                "frameReservedBits",
                serde_json::json!({"reserved":1}),
            ),
            (
                D::TrailingBits {
                    message_number: 1029,
                    bits: vec![true, false],
                },
                "trailingBits",
                serde_json::json!({"messageNumber":1029,"bits":[true,false]}),
            ),
            (
                D::MsmCellMaskOver64 {
                    message_number: 1077,
                    cells: 65,
                },
                "msmCellMaskOver64",
                serde_json::json!({"messageNumber":1077,"cells":65}),
            ),
            (
                D::OrderExceedsDegree {
                    message_number: 1264,
                    layer_index: 2,
                    degree: 3,
                    order: 4,
                },
                "orderExceedsDegree",
                serde_json::json!({"messageNumber":1264,"layerIndex":2,"degree":3,"order":4}),
            ),
            (
                D::SsrRecordsShort {
                    message_number: 1057,
                    declared: 5,
                    read: 4,
                },
                "ssrRecordsShort",
                serde_json::json!({"messageNumber":1057,"declared":5,"read":4}),
            ),
            (
                D::RecordsShort {
                    message_number: 1002,
                    declared: 7,
                    read: 6,
                },
                "recordsShort",
                serde_json::json!({"messageNumber":1002,"declared":7,"read":6}),
            ),
        ];
        for (departure, kind, expected_fields) in departures {
            let payload = rtcm_departure_payload(&departure);
            assert_eq!(payload["kind"], kind);
            assert_eq!(payload["message"], departure.to_string());
            for (key, value) in expected_fields.as_object().expect("object fields") {
                assert_eq!(&payload[key], value, "departure field {key}");
            }
        }
    }

    #[test]
    fn rtcm_record_kind_payload_table_preserves_current_variants() {
        use sidereon_core::rtcm::{MsmKind, RtcmRecordKind as R, SsrKind};
        use sidereon_core::GnssSystem;
        let cases = [
            (
                R::StationCoordinates,
                serde_json::json!({"kind":"stationCoordinates"}),
            ),
            (
                R::AntennaDescriptor,
                serde_json::json!({"kind":"antennaDescriptor"}),
            ),
            (
                R::Msm {
                    system: GnssSystem::BeiDou,
                    kind: MsmKind::Msm6,
                },
                serde_json::json!({"kind":"msm","system":"BeiDou","messageKind":"msm6"}),
            ),
            (
                R::Ssr {
                    system: GnssSystem::Glonass,
                    kind: SsrKind::CombinedOrbitClock,
                },
                serde_json::json!({"kind":"ssr","system":"GLONASS","messageKind":"combinedOrbitClock"}),
            ),
            (
                R::LegacyObservations,
                serde_json::json!({"kind":"legacyObservations"}),
            ),
            (
                R::SystemParameters,
                serde_json::json!({"kind":"systemParameters"}),
            ),
            (R::Text, serde_json::json!({"kind":"text"})),
            (
                R::Network { family: "network" },
                serde_json::json!({"kind":"network","family":"network"}),
            ),
            (
                R::Transformation {
                    family: "transformation",
                },
                serde_json::json!({"kind":"transformation","family":"transformation"}),
            ),
            (
                R::GlonassCodePhaseBiases,
                serde_json::json!({"kind":"glonassCodePhaseBiases"}),
            ),
            (
                R::SsrVtec {
                    message_number: 4076,
                },
                serde_json::json!({"kind":"ssrVtec","messageNumber":4076}),
            ),
        ];
        for (record, expected) in cases {
            assert_eq!(rtcm_record_kind_value(record), expected);
        }
    }

    #[test]
    fn ionex_epoch_payload_table_preserves_every_current_field() {
        use sidereon_core::atmosphere::IonexEpochError as E;
        let cases = [
            (
                E::NotWholeSecond {
                    scale: TimeScale::Utc,
                },
                serde_json::json!({"kind":"notWholeSecond","scale":"UTC"}),
            ),
            (
                E::FractionalUtcSecond {
                    scale: TimeScale::Tai,
                },
                serde_json::json!({"kind":"fractionalUtcSecond","scale":"TAI"}),
            ),
            (
                E::NoExactUtcOffset {
                    scale: TimeScale::Tcg,
                },
                serde_json::json!({"kind":"noExactUtcOffset","scale":"TCG"}),
            ),
            (
                E::InsertedLeapSecond {
                    scale: TimeScale::Utc,
                },
                serde_json::json!({"kind":"insertedLeapSecond","scale":"UTC"}),
            ),
            (
                E::BeforeIntegerLeapSeconds {
                    scale: TimeScale::Utc,
                },
                serde_json::json!({"kind":"beforeIntegerLeapSeconds","scale":"UTC"}),
            ),
            (
                E::OutOfRange {
                    scale: TimeScale::Tdb,
                },
                serde_json::json!({"kind":"outOfRange","scale":"TDB"}),
            ),
            (
                E::YearOutOfField {
                    utc_j2000_s: i64::MIN,
                },
                serde_json::json!({"kind":"yearOutOfField","utcJ2000S":i64::MIN.to_string()}),
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(ionex_epoch_error_payload(&error), expected);
        }
    }

    #[test]
    fn rtcm_conversion_payload_table_preserves_current_variants_and_causes() {
        use sidereon_core::ephemeris::LnavRecordError;
        use sidereon_core::rtcm::{RtcmConversionError as E, VtecEvaluationProblem as P};
        use sidereon_core::{GnssSatelliteId, GnssSystem};
        let sat = "E03".parse::<GnssSatelliteId>().expect("valid satellite");
        let cases = [
            (
                E::SatelliteIdOutOfRange {
                    message_number: 1019,
                    field: "prn",
                    value: 255,
                    width: 6,
                },
                serde_json::json!({"kind":"satelliteIdOutOfRange","messageNumber":1019,"field":"prn","value":255,"width":6}),
            ),
            (
                E::InvalidSatellite {
                    message_number: 1019,
                    field: "prn",
                    value: 0,
                    error: sidereon_core::SatelliteIdError::InvalidInput {
                        field: "prn",
                        reason: "outside range",
                    },
                },
                serde_json::json!({"kind":"invalidSatellite","messageNumber":1019,"field":"prn","value":0,"error":{"kind":"invalidInput","field":"prn","reason":"outside range"}}),
            ),
            (
                E::SbasPrnOutsideWindow {
                    value: 63,
                    broadcast_prn: 210,
                },
                serde_json::json!({"kind":"sbasPrnOutsideWindow","value":63,"broadcastPrn":210}),
            ),
            (
                E::NoLnavRecord {
                    value: 40,
                    satellite: sat,
                },
                serde_json::json!({"kind":"noLnavRecord","value":40,"satellite":"E03"}),
            ),
            (
                E::WeekMismatch {
                    message_number: 1019,
                    full_week: 2049,
                    week: 1,
                },
                serde_json::json!({"kind":"weekMismatch","messageNumber":1019,"fullWeek":2049,"week":1}),
            ),
            (
                E::NavicWeekMismatch {
                    full_week: 2050,
                    week: 2,
                },
                serde_json::json!({"kind":"navicWeekMismatch","fullWeek":2050,"week":2}),
            ),
            (
                E::TimeNotRepresentable { field: "toe" },
                serde_json::json!({"kind":"timeNotRepresentable","field":"toe"}),
            ),
            (
                E::GalileoWeekOverflow,
                serde_json::json!({"kind":"galileoWeekOverflow"}),
            ),
            (
                E::SisaSpare { index: 126 },
                serde_json::json!({"kind":"sisaSpare","index":126}),
            ),
            (
                E::SisaNoPrediction,
                serde_json::json!({"kind":"sisaNoPrediction"}),
            ),
            (
                E::UraOutOfRange {
                    system: GnssSystem::Navic,
                    index: 16,
                },
                serde_json::json!({"kind":"uraOutOfRange","system":"NavIC","index":16}),
            ),
            (
                E::UraNoPrediction {
                    system: GnssSystem::Gps,
                    index: 15,
                },
                serde_json::json!({"kind":"uraNoPrediction","system":"GPS","index":15}),
            ),
            (
                E::FitInterval(LnavRecordError::WeekMismatch {
                    full_week: u32::MAX,
                    decoded_week: i64::MIN,
                }),
                serde_json::json!({"kind":"fitInterval","error":{"kind":"weekMismatch","fullWeek":u32::MAX,"decodedWeek":i64::MIN.to_string()}}),
            ),
            (
                E::VtecEvaluation(P::CoefficientCounts {
                    layer_index: 2,
                    cosine_expected: 3,
                    cosine_actual: 4,
                    sine_expected: 5,
                    sine_actual: 6,
                }),
                serde_json::json!({"kind":"vtecEvaluation","problem":{"kind":"coefficientCounts","layerIndex":2,"cosineExpected":3,"cosineActual":4,"sineExpected":5,"sineActual":6}}),
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(rtcm_conversion_error_payload(&error), expected);
        }

        let lnav_cases = [
            (
                LnavRecordError::NotGps(sat),
                serde_json::json!({"kind":"notGps","satellite":"E03"}),
            ),
            (
                LnavRecordError::InvalidEpoch("toe"),
                serde_json::json!({"kind":"invalidEpoch","field":"toe"}),
            ),
            (
                LnavRecordError::NoUraPrediction(i64::MIN),
                serde_json::json!({"kind":"noUraPrediction","index":i64::MIN.to_string()}),
            ),
            (
                LnavRecordError::FitIntervalUnsupported {
                    fit_interval_flag: i64::MIN,
                    iode: -2,
                    iodc: 3,
                },
                serde_json::json!({"kind":"fitIntervalUnsupported","fitIntervalFlag":i64::MIN.to_string(),"iode":"-2","iodc":"3"}),
            ),
        ];
        for (error, expected) in lnav_cases {
            let detail = rtcm_conversion_error_payload(&E::FitInterval(error));
            assert_eq!(detail["error"], expected);
        }

        let problems = [
            (
                P::ComputationTime,
                serde_json::json!({"kind":"computationTime"}),
            ),
            (P::Frequency, serde_json::json!({"kind":"frequency"})),
            (
                P::NonFiniteCoordinates,
                serde_json::json!({"kind":"nonFiniteCoordinates"}),
            ),
            (
                P::MessageIdentity {
                    message_number: 4076,
                },
                serde_json::json!({"kind":"messageIdentity","messageNumber":4076}),
            ),
            (
                P::LayerCount { layers: 5 },
                serde_json::json!({"kind":"layerCount","layers":5}),
            ),
            (
                P::InvalidGeometry,
                serde_json::json!({"kind":"invalidGeometry"}),
            ),
            (P::BelowHorizon, serde_json::json!({"kind":"belowHorizon"})),
            (
                P::LayerDegreeOrder {
                    layer_index: 1,
                    degree: 2,
                    order: 3,
                },
                serde_json::json!({"kind":"layerDegreeOrder","layerIndex":1,"degree":2,"order":3}),
            ),
            (
                P::UnavailableCoefficient { layer_index: 2 },
                serde_json::json!({"kind":"unavailableCoefficient","layerIndex":2}),
            ),
            (
                P::ShellNotAboveReceiver { layer_index: 3 },
                serde_json::json!({"kind":"shellNotAboveReceiver","layerIndex":3}),
            ),
            (
                P::MissingCoefficient {
                    layer_index: 4,
                    field: "cosine",
                    index: 5,
                },
                serde_json::json!({"kind":"missingCoefficient","layerIndex":4,"field":"cosine","index":5}),
            ),
            (
                P::InvalidMappingFactor { layer_index: 6 },
                serde_json::json!({"kind":"invalidMappingFactor","layerIndex":6}),
            ),
            (
                P::PhysicalResultOutOfRange { field: "delay" },
                serde_json::json!({"kind":"physicalResultOutOfRange","field":"delay"}),
            ),
        ];
        for (problem, expected) in problems {
            assert_eq!(vtec_problem_payload(&problem), expected);
        }
    }

    #[test]
    fn test_core_error_all_variants_mapped() {
        use sidereon_core::atmosphere::ionosphere::{
            IonexMappingDeclaration, IonexNodeGap, IonexSlantRefusal,
        };
        use sidereon_core::rinex::observations::RinexObsWriteError;
        use sidereon_core::rtcm::{RtcmConversionError, RtcmEncodeError};
        use sidereon_core::sbas::SbasEncodeError;
        use sidereon_core::terrain::DtedTileError;

        let sat = "G01".parse::<GnssSatelliteId>().expect("valid sat");

        // 1. Parse
        let e = CoreError::Parse("bad line".into());
        let d = CoreErrorDetail::from(&e);
        assert_eq!(
            d,
            CoreErrorDetail::Parse {
                message: "bad line".into()
            }
        );

        // 2. UnknownSatellite
        let e = CoreError::UnknownSatellite(sat);
        let d = CoreErrorDetail::from(&e);
        assert_eq!(
            d,
            CoreErrorDetail::UnknownSatellite {
                satellite_id: "G01".into()
            }
        );

        // 3. MissingGlonassChannel
        let e = CoreError::MissingGlonassChannel;
        let d = CoreErrorDetail::from(&e);
        assert_eq!(d, CoreErrorDetail::MissingGlonassChannel);

        // 4. MissingTerrainTile
        let e = CoreError::MissingTerrainTile {
            lat_index: 36,
            lon_index: -107,
        };
        let d = CoreErrorDetail::from(&e);
        assert_eq!(
            d,
            CoreErrorDetail::MissingTerrainTile {
                lat_index: 36,
                lon_index: -107
            }
        );

        // 5. UnknownTerrainElevation
        let e = CoreError::UnknownTerrainElevation {
            lat_index: 36,
            lon_index: -107,
            latitude_posting: 12,
            longitude_posting: 34,
        };
        let d = CoreErrorDetail::from(&e);
        assert_eq!(
            d,
            CoreErrorDetail::UnknownTerrainElevation {
                lat_index: 36,
                lon_index: -107,
                latitude_posting: 12,
                longitude_posting: 34
            }
        );

        // 6. NonWgs84TerrainTile
        let e = CoreError::NonWgs84TerrainTile {
            lat_index: 36,
            lon_index: -107,
            datum: DtedHorizontalDatum::Other("Tokyo".into()),
        };
        let d = CoreErrorDetail::from(&e);
        assert_eq!(
            d,
            CoreErrorDetail::NonWgs84TerrainTile {
                lat_index: 36,
                lon_index: -107,
                datum: "\"Tokyo\"".into()
            }
        );

        // 7. TerrainTileOrigin
        let e = CoreError::TerrainTileOrigin {
            path: std::path::PathBuf::from("n36.dt2"),
            lat_index: 36,
            lon_index: -107,
            origin_latitude: 36,
            origin_longitude: -107,
        };
        let d = CoreErrorDetail::from(&e);
        assert_eq!(
            d,
            CoreErrorDetail::TerrainTileOrigin {
                path: "n36.dt2".into(),
                lat_index: 36,
                lon_index: -107,
                origin_latitude: 36,
                origin_longitude: -107
            }
        );

        let e = CoreError::TerrainTile {
            lat_index: 36,
            lon_index: -107,
            error: Box::new(DtedTileError::InvalidField("bad field".into())),
        };
        assert_eq!(
            CoreErrorDetail::from(&e),
            CoreErrorDetail::TerrainTile {
                lat_index: 36,
                lon_index: -107,
                cause: serde_json::json!({"kind":"invalidField","message":"bad field"}),
            }
        );

        // 8. IonexOutOfCoverage
        let e = CoreError::IonexOutOfCoverage(IonexCoverageError::EpochBeforeFirstMap);
        let d = CoreErrorDetail::from(&e);
        assert_eq!(
            d,
            CoreErrorDetail::IonexOutOfCoverage {
                cause: crate::ionex::IonexCoverageErrorJs::from_core(
                    IonexCoverageError::EpochBeforeFirstMap
                )
            }
        );

        let e = CoreError::IonexNodesNotAvailable(Box::new(IonexNodeGap {
            earlier: None,
            later: None,
        }));
        if let CoreErrorDetail::IonexNodesNotAvailable { cause } = CoreErrorDetail::from(&e) {
            let value = serde_json::to_value(cause).unwrap();
            assert_eq!(value["earlier"], serde_json::Value::Null);
            assert_eq!(value["later"], serde_json::Value::Null);
        } else {
            panic!("expected typed IONEX node-gap detail");
        }

        let e = CoreError::IonexSlantUnavailable(IonexSlantRefusal::MappingFunction(
            IonexMappingDeclaration::Absent,
        ));
        if let CoreErrorDetail::IonexSlantUnavailable { cause } = CoreErrorDetail::from(&e) {
            let value = serde_json::to_value(cause).unwrap();
            assert_eq!(value["kind"], "MAPPING_FUNCTION");
        } else {
            panic!("expected typed IONEX slant-refusal detail");
        }

        // 9. IonexEpoch
        let e = CoreError::IonexEpoch(IonexEpochError::NotWholeSecond {
            scale: TimeScale::Utc,
        });
        let d = CoreErrorDetail::from(&e);
        if let CoreErrorDetail::IonexEpoch { cause } = d {
            assert_eq!(cause["kind"], "notWholeSecond");
            assert_eq!(cause["scale"], "UTC");
        } else {
            panic!("expected IonexEpoch");
        }

        // 10. EpochOutOfRange
        let e = CoreError::EpochOutOfRange;
        let d = CoreErrorDetail::from(&e);
        assert_eq!(d, CoreErrorDetail::EpochOutOfRange);

        // 11. InsufficientPreciseNodes
        let e = CoreError::InsufficientPreciseNodes {
            sat,
            nodes: 3,
            required: 4,
        };
        let d = CoreErrorDetail::from(&e);
        assert_eq!(
            d,
            CoreErrorDetail::InsufficientPreciseNodes {
                satellite_id: "G01".into(),
                nodes: 3,
                required: 4
            }
        );

        // 12. InvalidInput
        let e = CoreError::InvalidInput("bad input".into());
        let d = CoreErrorDetail::from(&e);
        assert_eq!(
            d,
            CoreErrorDetail::InvalidInput {
                message: "bad input".into()
            }
        );

        // 13. Sp3EpochInterval
        let e = CoreError::Sp3EpochInterval(Sp3EpochIntervalError {
            field: "interval",
            value: 0.0,
            reason: Sp3EpochIntervalRejection::NotPositive,
        });
        let d = CoreErrorDetail::from(&e);
        assert_eq!(
            d,
            CoreErrorDetail::Sp3EpochInterval {
                field: "interval".into(),
                value: exact_float(0.0),
                reason: "it is not positive".into()
            }
        );

        // 14. Sp3MergeTolerance
        let e = CoreError::Sp3MergeTolerance(MergeToleranceError {
            field: MergeToleranceField::Position,
            value: -1.0,
        });
        let d = CoreErrorDetail::from(&e);
        assert_eq!(
            d,
            CoreErrorDetail::Sp3MergeTolerance {
                field: "positionToleranceM".into(),
                value: exact_float(-1.0),
                reason: "must be finite and nonnegative".into()
            }
        );

        // 15. ContinuityOptions
        let e = CoreError::ContinuityOptions(ContinuityOptionsError {
            field: "speedBound",
            value: -1.0,
            reason: ContinuityOptionRejection::Negative,
        });
        let d = CoreErrorDetail::from(&e);
        assert_eq!(
            d,
            CoreErrorDetail::ContinuityOptions {
                field: "speedBound",
                value: exact_float(-1.0),
                reason: "negative"
            }
        );

        // 16. Ut1OutsideCoverage
        let e = CoreError::Ut1OutsideCoverage(
            sidereon_core::astro::time::DegradeReason::BeforeCoverage,
        );
        let d = CoreErrorDetail::from(&e);
        assert_eq!(
            d,
            CoreErrorDetail::Ut1OutsideCoverage {
                reason: "beforeCoverage"
            }
        );

        let e = CoreError::SbasEncode(Box::new(SbasEncodeError::UnrecognizedPreamble {
            preamble: 0x42,
        }));
        assert_eq!(
            CoreErrorDetail::from(&e),
            CoreErrorDetail::SbasEncode {
                cause: serde_json::json!({"kind":"unrecognizedPreamble","preamble":0x42}),
            }
        );

        let e = CoreError::RtcmEncode(Box::new(RtcmEncodeError::NegativeZeroWithValue {
            message_number: 1020,
            field: "df001".into(),
            value: 42,
        }));
        if let CoreErrorDetail::RtcmEncode { cause } = CoreErrorDetail::from(&e) {
            assert_eq!(cause["kind"], "negativeZeroWithValue");
            assert_eq!(cause["messageNumber"], 1020);
            assert_eq!(cause["field"], "df001");
            assert_eq!(cause["value"], "42");
        } else {
            panic!("expected typed RTCM encode detail");
        }

        let e = CoreError::RtcmConversion(Box::new(RtcmConversionError::GalileoWeekOverflow));
        assert_eq!(
            CoreErrorDetail::from(&e),
            CoreErrorDetail::RtcmConversion {
                cause: serde_json::json!({"kind":"galileoWeekOverflow"}),
            }
        );

        assert_eq!(
            CoreError::Parse("bad line".into()).to_string(),
            "parse error: bad line"
        );

        let rinex_source = RinexObsWriteError::NotVersionTwo { version: 3.0 };
        let rinex_message = rinex_source.to_string();
        assert_eq!(
            CoreError::from(rinex_source),
            CoreError::InvalidInput(rinex_message)
        );

        let rtcm_encode_source = RtcmEncodeError::NegativeZeroWithValue {
            message_number: 1020,
            field: "df001".into(),
            value: 42,
        };
        let rtcm_encode_expected = rtcm_encode_source.clone();
        assert_eq!(
            CoreError::from(rtcm_encode_source),
            CoreError::RtcmEncode(Box::new(rtcm_encode_expected))
        );

        let rtcm_conversion_source = RtcmConversionError::GalileoWeekOverflow;
        let rtcm_conversion_expected = rtcm_conversion_source.clone();
        assert_eq!(
            CoreError::from(rtcm_conversion_source),
            CoreError::RtcmConversion(Box::new(rtcm_conversion_expected))
        );

        let sbas_source = SbasEncodeError::UnrecognizedPreamble { preamble: 0x42 };
        let sbas_expected = sbas_source.clone();
        assert_eq!(
            CoreError::from(sbas_source),
            CoreError::SbasEncode(Box::new(sbas_expected))
        );

        let truncated = sidereon_core::rtcm::Message::decode(&[0x3e, 0xd0])
            .expect_err("recognized RTCM 1005 body must be truncated");
        match truncated {
            CoreError::Parse(message) => assert!(message.contains("RTCM body truncated")),
            other => panic!("expected parse error from truncated RTCM body, got {other:?}"),
        }
    }

    #[test]
    fn test_observables_error_mapping() {
        let e = ObservablesError::InvalidInput {
            field: "satellite",
            kind: ObservablesInputErrorKind::Missing,
        };
        let d = ObservablesErrorDetail::from(&e);
        assert_eq!(
            d,
            ObservablesErrorDetail::InvalidInput {
                message: e.to_string(),
                field: "satellite",
                reason: "missing"
            }
        );

        let e = ObservablesError::NoEphemeris;
        let d = ObservablesErrorDetail::from(&e);
        assert_eq!(
            d,
            ObservablesErrorDetail::NoEphemeris {
                message: e.to_string()
            }
        );

        let core_e = CoreError::EpochOutOfRange;
        let e = ObservablesError::Ephemeris(core_e);
        let d = ObservablesErrorDetail::from(&e);
        assert_eq!(
            d,
            ObservablesErrorDetail::Ephemeris {
                message: e.to_string(),
                cause: CoreErrorDetail::EpochOutOfRange
            }
        );
    }

    #[test]
    fn test_velocity_error_mapping() {
        let sat = "G02".parse::<GnssSatelliteId>().expect("valid sat");
        let e = VelocityError::TooFewSatellites {
            used: 2,
            required: 4,
        };
        let d = VelocityErrorDetail::from(&e);
        assert_eq!(
            d,
            VelocityErrorDetail::TooFewSatellites {
                message: e.to_string(),
                used: 2,
                required: 4
            }
        );

        let e = VelocityError::DuplicateObservation { satellite_id: sat };
        let d = VelocityErrorDetail::from(&e);
        assert_eq!(
            d,
            VelocityErrorDetail::DuplicateObservation {
                message: e.to_string(),
                satellite_id: "G02".into()
            }
        );

        let e = VelocityError::InvalidCarrier { satellite_id: sat };
        let d = VelocityErrorDetail::from(&e);
        assert_eq!(
            d,
            VelocityErrorDetail::InvalidCarrier {
                message: e.to_string(),
                satellite_id: "G02".into()
            }
        );

        let e = VelocityError::InvalidInput {
            field: "carrier_hz",
            reason: "not positive",
        };
        let d = VelocityErrorDetail::from(&e);
        assert_eq!(
            d,
            VelocityErrorDetail::InvalidInput {
                message: e.to_string(),
                field: "carrier_hz",
                reason: "not positive"
            }
        );
    }

    #[test]
    fn exact_error_numbers_keep_nan_payload_and_signed_zero_bits() {
        let nan = f64::from_bits(0x7ff8_0000_0000_0042);
        let d = CoreErrorDetail::from(&CoreError::Sp3EpochInterval(Sp3EpochIntervalError {
            field: "interval",
            value: nan,
            reason: Sp3EpochIntervalRejection::NotFinite,
        }));
        let json = serde_json::to_value(d).expect("exact finite JSON-safe detail");
        assert_eq!(json["value"]["decimal"], "NaN");
        assert_eq!(json["value"]["bitsHex"], "7ff8000000000042");

        let negative_zero = exact_float(-0.0);
        assert_eq!(negative_zero.decimal, "-0");
        assert_eq!(negative_zero.bits_hex, "8000000000000000");
        let positive_infinity = exact_float(f64::INFINITY);
        assert_eq!(positive_infinity.decimal, "inf");
        assert_eq!(positive_infinity.bits_hex, "7ff0000000000000");

        let merge = CoreErrorDetail::from(&CoreError::Sp3MergeTolerance(MergeToleranceError {
            field: MergeToleranceField::OutlierClock,
            value: nan,
        }));
        let merge = serde_json::to_value(merge).expect("exact merge value");
        assert_eq!(merge["field"], "outlierReject.clockToleranceS");
        assert_eq!(merge["value"]["bitsHex"], "7ff8000000000042");

        let continuity =
            CoreErrorDetail::from(&CoreError::ContinuityOptions(ContinuityOptionsError {
                field: "residual_tolerance_m",
                value: -0.0,
                reason: ContinuityOptionRejection::Negative,
            }));
        let continuity = serde_json::to_value(continuity).expect("exact continuity value");
        assert_eq!(continuity["value"]["bitsHex"], "8000000000000000");
    }
}
