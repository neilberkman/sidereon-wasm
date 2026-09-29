//! NMEA 0183 parsing, epoch accumulation, and GGA writing.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::astro::time::civil::j2000_seconds_from_split;
use sidereon_core::nmea::{
    group_epochs as core_group_epochs, parse_nmea as core_parse_nmea, write_gga as core_write_gga,
    Diagnostics, EpochSnapshot, FieldError, Gga, GgaQuality, Gll, Gsa, GsaFixMode,
    GsaSelectionMode, Gst, Gsv, GsvGroup, GsvSatellite, NmeaAccumulator as CoreNmeaAccumulator,
    NmeaBody, NmeaCoordinate, NmeaDate, NmeaLog, NmeaSatNumber, NmeaSentence, NmeaSignalId,
    NmeaTalker, NmeaTime, Rmc, RmcStatus, SkipReason, Vtg, WarningKind, Zda,
};
use sidereon_core::GnssSystem;

use crate::error::{engine_error, type_error};

#[wasm_bindgen(typescript_custom_section)]
const NMEA_DIAGNOSTIC_TYPESCRIPT: &'static str = r#"
export interface NmeaRecordRef {
  line: number | null | undefined;
  recordIndex: number | null | undefined;
  satellite: string | null | undefined;
}

export type NmeaFieldError =
  | { kind: "missing" | "nonFinite" | "notPositive" | "negative"; field: string }
  | { kind: "outOfRange"; field: string; min: number; max: number; upperInclusive: boolean }
  | { kind: "floatParse" | "intParse"; field: string; value: string }
  | { kind: "invalidCivilDate"; field: string; year: string; month: string; day: string }
  | { kind: "invalidCivilTime"; field: string; hour: string; minute: string; second: number };

export type NmeaSkipReason =
  | { reasonKind: "unrepresentableSatellite" }
  | { reasonKind: "unsupportedRecordType"; recordType: string }
  | { reasonKind: "malformedField"; cause: NmeaFieldError }
  | { reasonKind: "outOfRangeEpoch" }
  | { reasonKind: "truncated" }
  | { reasonKind: "unsupportedUnit"; unit: string }
  | { reasonKind: "unknownBlock"; block: string }
  | { reasonKind: "inconsistentRecord"; reasonDetail: string };

export type NmeaSkipDiagnostic = { at: NmeaRecordRef; reason: string } & NmeaSkipReason;
export type NmeaWarningKind = "checksum" | "clamped" | "degraded" | "mismatch" | "overlap" | "missingMetadata";
export interface NmeaWarningDiagnostic {
  at: NmeaRecordRef;
  reason: string;
  kind: NmeaWarningKind;
}
export interface NmeaDiagnostics {
  skipCount: number;
  warningCount: number;
  skips: NmeaSkipDiagnostic[];
  warnings: NmeaWarningDiagnostic[];
}
export interface NmeaAccumulatorOutput {
  sentences: any[];
  epochs: any[];
  diagnostics: NmeaDiagnostics;
  retainedLength: number;
}
"#;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordRefJs {
    line: Option<usize>,
    record_index: Option<usize>,
    satellite: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticEntryJs {
    at: RecordRefJs,
    reason: String,
    #[serde(flatten)]
    reason_detail: SkipReasonJs,
}

#[derive(Serialize)]
#[serde(
    tag = "reasonKind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum SkipReasonJs {
    UnrepresentableSatellite,
    UnsupportedRecordType { record_type: &'static str },
    MalformedField { cause: FieldErrorJs },
    OutOfRangeEpoch,
    Truncated,
    UnsupportedUnit { unit: String },
    UnknownBlock { block: String },
    InconsistentRecord { reason_detail: &'static str },
}

#[derive(Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum FieldErrorJs {
    Missing {
        field: &'static str,
    },
    NonFinite {
        field: &'static str,
    },
    NotPositive {
        field: &'static str,
    },
    Negative {
        field: &'static str,
    },
    OutOfRange {
        field: &'static str,
        min: f64,
        max: f64,
        upper_inclusive: bool,
    },
    FloatParse {
        field: &'static str,
        value: String,
    },
    IntParse {
        field: &'static str,
        value: String,
    },
    InvalidCivilDate {
        field: &'static str,
        year: String,
        month: String,
        day: String,
    },
    InvalidCivilTime {
        field: &'static str,
        hour: String,
        minute: String,
        second: f64,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WarningEntryJs {
    at: RecordRefJs,
    reason: String,
    kind: WarningKindJs,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum WarningKindJs {
    Checksum,
    Clamped,
    Degraded,
    Mismatch,
    Overlap,
    MissingMetadata,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiagnosticsJs {
    skip_count: usize,
    warning_count: usize,
    skips: Vec<DiagnosticEntryJs>,
    warnings: Vec<WarningEntryJs>,
}

fn field_error_js(error: &FieldError) -> FieldErrorJs {
    match error {
        FieldError::Missing { field } => FieldErrorJs::Missing { field },
        FieldError::NonFinite { field } => FieldErrorJs::NonFinite { field },
        FieldError::NotPositive { field } => FieldErrorJs::NotPositive { field },
        FieldError::Negative { field } => FieldErrorJs::Negative { field },
        FieldError::OutOfRange {
            field,
            min,
            max,
            upper_inclusive,
        } => FieldErrorJs::OutOfRange {
            field,
            min: *min,
            max: *max,
            upper_inclusive: *upper_inclusive,
        },
        FieldError::FloatParse { field, value } => FieldErrorJs::FloatParse {
            field,
            value: value.clone(),
        },
        FieldError::IntParse { field, value } => FieldErrorJs::IntParse {
            field,
            value: value.clone(),
        },
        FieldError::InvalidCivilDate {
            field,
            year,
            month,
            day,
        } => FieldErrorJs::InvalidCivilDate {
            field,
            year: year.to_string(),
            month: month.to_string(),
            day: day.to_string(),
        },
        FieldError::InvalidCivilTime {
            field,
            hour,
            minute,
            second,
        } => FieldErrorJs::InvalidCivilTime {
            field,
            hour: hour.to_string(),
            minute: minute.to_string(),
            second: *second,
        },
    }
}

fn skip_reason_js(reason: &SkipReason) -> SkipReasonJs {
    match reason {
        SkipReason::UnrepresentableSatellite => SkipReasonJs::UnrepresentableSatellite,
        SkipReason::UnsupportedRecordType(record_type) => {
            SkipReasonJs::UnsupportedRecordType { record_type }
        }
        SkipReason::MalformedField(error) => SkipReasonJs::MalformedField {
            cause: field_error_js(error),
        },
        SkipReason::OutOfRangeEpoch => SkipReasonJs::OutOfRangeEpoch,
        SkipReason::Truncated => SkipReasonJs::Truncated,
        SkipReason::UnsupportedUnit(unit) => SkipReasonJs::UnsupportedUnit { unit: unit.clone() },
        SkipReason::UnknownBlock(block) => SkipReasonJs::UnknownBlock {
            block: block.clone(),
        },
        SkipReason::InconsistentRecord(reason) => SkipReasonJs::InconsistentRecord {
            reason_detail: reason,
        },
    }
}

fn warning_kind_js(kind: WarningKind) -> WarningKindJs {
    match kind {
        WarningKind::Checksum => WarningKindJs::Checksum,
        WarningKind::Clamped => WarningKindJs::Clamped,
        WarningKind::Degraded => WarningKindJs::Degraded,
        WarningKind::Mismatch => WarningKindJs::Mismatch,
        WarningKind::Overlap => WarningKindJs::Overlap,
        WarningKind::MissingMetadata => WarningKindJs::MissingMetadata,
    }
}

#[cfg(test)]
mod diagnostic_mapping_tests {
    use super::*;

    #[test]
    fn absent_record_reference_components_remain_absent() {
        let record_ref = RecordRefJs {
            line: None,
            record_index: None,
            satellite: None,
        };
        assert!(record_ref.line.is_none());
        assert!(record_ref.record_index.is_none());
        assert!(record_ref.satellite.is_none());
    }

    #[test]
    fn signed_civil_components_are_exported_without_number_precision_loss() {
        let date = field_error_js(&FieldError::InvalidCivilDate {
            field: "date",
            year: i64::MAX,
            month: i64::MIN,
            day: 9_007_199_254_740_993,
        });
        match date {
            FieldErrorJs::InvalidCivilDate {
                year, month, day, ..
            } => {
                assert_eq!(year, i64::MAX.to_string());
                assert_eq!(month, i64::MIN.to_string());
                assert_eq!(day, "9007199254740993");
            }
            _ => panic!("civil-date error retained its variant"),
        }

        let time = field_error_js(&FieldError::InvalidCivilTime {
            field: "time",
            hour: i64::MAX,
            minute: i64::MIN,
            second: f64::INFINITY,
        });
        match time {
            FieldErrorJs::InvalidCivilTime {
                hour,
                minute,
                second,
                ..
            } => {
                assert_eq!(hour, i64::MAX.to_string());
                assert_eq!(minute, i64::MIN.to_string());
                assert!(second.is_infinite());
            }
            _ => panic!("civil-time error retained its variant"),
        }
    }

    #[test]
    fn out_of_range_float_bounds_keep_non_finite_values() {
        let mapped = field_error_js(&FieldError::OutOfRange {
            field: "range",
            min: f64::NAN,
            max: f64::INFINITY,
            upper_inclusive: false,
        });
        match mapped {
            FieldErrorJs::OutOfRange {
                min,
                max,
                upper_inclusive,
                ..
            } => {
                assert!(min.is_nan());
                assert!(max.is_infinite());
                assert!(!upper_inclusive);
            }
            _ => panic!("range error retained its variant"),
        }
    }
}

pub(crate) fn diagnostics_js(diagnostics: &Diagnostics) -> DiagnosticsJs {
    DiagnosticsJs {
        skip_count: diagnostics.skips.len(),
        warning_count: diagnostics.warnings.len(),
        skips: diagnostics
            .skips
            .iter()
            .map(|skip| DiagnosticEntryJs {
                at: RecordRefJs {
                    line: skip.at.line,
                    record_index: skip.at.record_index,
                    satellite: skip.at.satellite.clone(),
                },
                reason: format!("{:?}", skip.reason),
                reason_detail: skip_reason_js(&skip.reason),
            })
            .collect(),
        warnings: diagnostics
            .warnings
            .iter()
            .map(|warning| WarningEntryJs {
                at: RecordRefJs {
                    line: warning.at.line,
                    record_index: warning.at.record_index,
                    satellite: warning.at.satellite.clone(),
                },
                reason: format!("{:?}", warning.kind),
                kind: warning_kind_js(warning.kind),
            })
            .collect(),
    }
}

fn to_value<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true))
        .map_err(|e| type_error(&e.to_string()))
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

fn talker_code(talker: NmeaTalker) -> String {
    talker
        .code()
        .map(|code| String::from_utf8_lossy(&code).into_owned())
        .unwrap_or_else(|_| "??".to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NmeaTimeJs {
    hour: u8,
    minute: u8,
    second: u8,
    nanos: u32,
    decimals: u8,
}

impl From<NmeaTime> for NmeaTimeJs {
    fn from(time: NmeaTime) -> Self {
        Self {
            hour: time.hour,
            minute: time.minute,
            second: time.second,
            nanos: time.nanos,
            decimals: time.decimals,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NmeaDateJs {
    year: u16,
    month: u8,
    day: u8,
}

impl From<NmeaDate> for NmeaDateJs {
    fn from(date: NmeaDate) -> Self {
        Self {
            year: date.year,
            month: date.month,
            day: date.day,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CoordinateJs {
    degrees: f64,
    radians: f64,
}

fn coordinate_js(value: Option<NmeaCoordinate>) -> Option<CoordinateJs> {
    value.map(|coord| CoordinateJs {
        degrees: coord.degrees_f64(),
        radians: coord.radians(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SatNumberJs {
    raw: u16,
    resolved: Option<String>,
}

fn sat_number_js(value: NmeaSatNumber) -> SatNumberJs {
    SatNumberJs {
        raw: value.raw,
        resolved: value.resolved.map(|sat| sat.to_string()),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SignalIdJs {
    system: Option<&'static str>,
    id: u8,
}

fn signal_id_js(value: NmeaSignalId) -> SignalIdJs {
    SignalIdJs {
        system: value.system.map(system_label),
        id: value.id,
    }
}

fn rmc_status_label(status: RmcStatus) -> String {
    match status {
        RmcStatus::Valid => "valid".to_string(),
        RmcStatus::Warning => "warning".to_string(),
        RmcStatus::Other(value) => value.to_string(),
    }
}

fn gsa_selection_label(value: GsaSelectionMode) -> String {
    match value {
        GsaSelectionMode::Manual => "manual".to_string(),
        GsaSelectionMode::Automatic => "automatic".to_string(),
        GsaSelectionMode::Other(value) => value.to_string(),
    }
}

fn gsa_fix_label(value: GsaFixMode) -> String {
    match value {
        GsaFixMode::None => "none".to_string(),
        GsaFixMode::TwoD => "2d".to_string(),
        GsaFixMode::ThreeD => "3d".to_string(),
        GsaFixMode::Other(value) => value.to_string(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GgaJs {
    time: Option<NmeaTimeJs>,
    latitude: Option<CoordinateJs>,
    longitude: Option<CoordinateJs>,
    quality: Option<u8>,
    satellites_used: Option<u8>,
    hdop: Option<f64>,
    altitude_msl_m: Option<f64>,
    geoid_separation_m: Option<f64>,
    differential_age_s: Option<f64>,
    differential_station_id: Option<u16>,
}

fn gga_js(gga: &Gga) -> GgaJs {
    GgaJs {
        time: gga.time.map(NmeaTimeJs::from),
        latitude: coordinate_js(gga.latitude),
        longitude: coordinate_js(gga.longitude),
        quality: gga.quality.map(GgaQuality::value),
        satellites_used: gga.satellites_used,
        hdop: gga.hdop,
        altitude_msl_m: gga.altitude_msl_m,
        geoid_separation_m: gga.geoid_separation_m,
        differential_age_s: gga.differential_age_s,
        differential_station_id: gga.differential_station_id,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RmcJs {
    time: Option<NmeaTimeJs>,
    status: Option<String>,
    latitude: Option<CoordinateJs>,
    longitude: Option<CoordinateJs>,
    speed_over_ground_kn: Option<f64>,
    course_over_ground_deg: Option<f64>,
    date: Option<NmeaDateJs>,
    magnetic_variation_deg: Option<f64>,
    faa_mode: Option<char>,
    navigational_status: Option<char>,
}

fn rmc_js(rmc: &Rmc) -> RmcJs {
    RmcJs {
        time: rmc.time.map(NmeaTimeJs::from),
        status: rmc.status.map(rmc_status_label),
        latitude: coordinate_js(rmc.latitude),
        longitude: coordinate_js(rmc.longitude),
        speed_over_ground_kn: rmc.speed_over_ground_kn,
        course_over_ground_deg: rmc.course_over_ground_deg,
        date: rmc.date.map(NmeaDateJs::from),
        magnetic_variation_deg: rmc.magnetic_variation_deg,
        faa_mode: rmc.faa_mode,
        navigational_status: rmc.navigational_status,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GsaJs {
    selection_mode: Option<String>,
    fix_mode: Option<String>,
    satellites: Vec<SatNumberJs>,
    pdop: Option<f64>,
    hdop: Option<f64>,
    vdop: Option<f64>,
    system_id: Option<u8>,
    system: Option<&'static str>,
}

fn gsa_js(gsa: &Gsa) -> GsaJs {
    GsaJs {
        selection_mode: gsa.selection_mode.map(gsa_selection_label),
        fix_mode: gsa.fix_mode.map(gsa_fix_label),
        satellites: gsa.satellites.iter().copied().map(sat_number_js).collect(),
        pdop: gsa.pdop,
        hdop: gsa.hdop,
        vdop: gsa.vdop,
        system_id: gsa.system_id,
        system: gsa.system.map(system_label),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GsvSatelliteJs {
    sat_number: Option<SatNumberJs>,
    elevation_deg: Option<i16>,
    azimuth_deg: Option<u16>,
    cn0_db_hz: Option<u8>,
}

fn gsv_satellite_js(sat: &GsvSatellite) -> GsvSatelliteJs {
    GsvSatelliteJs {
        sat_number: sat.sat_number.map(sat_number_js),
        elevation_deg: sat.elevation_deg,
        azimuth_deg: sat.azimuth_deg,
        cn0_db_hz: sat.cn0_db_hz,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GsvJs {
    total_messages: u8,
    message_number: u8,
    satellites_in_view: Option<u16>,
    satellites: Vec<GsvSatelliteJs>,
    signal: Option<SignalIdJs>,
}

fn gsv_js(gsv: &Gsv) -> GsvJs {
    GsvJs {
        total_messages: gsv.total_messages,
        message_number: gsv.message_number,
        satellites_in_view: gsv.satellites_in_view,
        satellites: gsv.satellites.iter().map(gsv_satellite_js).collect(),
        signal: gsv.signal.map(signal_id_js),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GstJs {
    time: Option<NmeaTimeJs>,
    rms_range_residual_m: Option<f64>,
    semi_major_error_m: Option<f64>,
    semi_minor_error_m: Option<f64>,
    orientation_deg: Option<f64>,
    latitude_sigma_m: Option<f64>,
    longitude_sigma_m: Option<f64>,
    altitude_sigma_m: Option<f64>,
}

fn gst_js(gst: &Gst) -> GstJs {
    GstJs {
        time: gst.time.map(NmeaTimeJs::from),
        rms_range_residual_m: gst.rms_range_residual_m,
        semi_major_error_m: gst.semi_major_error_m,
        semi_minor_error_m: gst.semi_minor_error_m,
        orientation_deg: gst.orientation_deg,
        latitude_sigma_m: gst.latitude_sigma_m,
        longitude_sigma_m: gst.longitude_sigma_m,
        altitude_sigma_m: gst.altitude_sigma_m,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VtgJs {
    course_true_deg: Option<f64>,
    course_magnetic_deg: Option<f64>,
    speed_kn: Option<f64>,
    speed_kmh: Option<f64>,
    faa_mode: Option<char>,
}

fn vtg_js(vtg: &Vtg) -> VtgJs {
    VtgJs {
        course_true_deg: vtg.course_true_deg,
        course_magnetic_deg: vtg.course_magnetic_deg,
        speed_kn: vtg.speed_kn,
        speed_kmh: vtg.speed_kmh,
        faa_mode: vtg.faa_mode,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GllJs {
    latitude: Option<CoordinateJs>,
    longitude: Option<CoordinateJs>,
    time: Option<NmeaTimeJs>,
    status: Option<String>,
    faa_mode: Option<char>,
}

fn gll_js(gll: &Gll) -> GllJs {
    GllJs {
        latitude: coordinate_js(gll.latitude),
        longitude: coordinate_js(gll.longitude),
        time: gll.time.map(NmeaTimeJs::from),
        status: gll.status.map(rmc_status_label),
        faa_mode: gll.faa_mode,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ZdaJs {
    time: Option<NmeaTimeJs>,
    date: Option<NmeaDateJs>,
    local_zone_hours: Option<i8>,
    local_zone_minutes: Option<u8>,
}

fn zda_js(zda: &Zda) -> ZdaJs {
    ZdaJs {
        time: zda.time.map(NmeaTimeJs::from),
        date: zda.date.map(NmeaDateJs::from),
        local_zone_hours: zda.local_zone_hours,
        local_zone_minutes: zda.local_zone_minutes,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "kind", content = "body")]
enum NmeaBodyJs {
    Gga(GgaJs),
    Rmc(RmcJs),
    Gsa(GsaJs),
    Gsv(GsvJs),
    Gst(GstJs),
    Vtg(VtgJs),
    Gll(GllJs),
    Zda(ZdaJs),
}

fn body_js(body: &NmeaBody) -> NmeaBodyJs {
    match body {
        NmeaBody::Gga(gga) => NmeaBodyJs::Gga(gga_js(gga)),
        NmeaBody::Rmc(rmc) => NmeaBodyJs::Rmc(rmc_js(rmc)),
        NmeaBody::Gsa(gsa) => NmeaBodyJs::Gsa(gsa_js(gsa)),
        NmeaBody::Gsv(gsv) => NmeaBodyJs::Gsv(gsv_js(gsv)),
        NmeaBody::Gst(gst) => NmeaBodyJs::Gst(gst_js(gst)),
        NmeaBody::Vtg(vtg) => NmeaBodyJs::Vtg(vtg_js(vtg)),
        NmeaBody::Gll(gll) => NmeaBodyJs::Gll(gll_js(gll)),
        NmeaBody::Zda(zda) => NmeaBodyJs::Zda(zda_js(zda)),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NmeaSentenceJs {
    talker: String,
    body: NmeaBodyJs,
}

fn sentence_js(sentence: &NmeaSentence) -> NmeaSentenceJs {
    NmeaSentenceJs {
        talker: talker_code(sentence.talker),
        body: body_js(&sentence.body),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PositionJs {
    lat_deg: f64,
    lon_deg: f64,
    height_m: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GsaEntryJs {
    system: Option<&'static str>,
    gsa: GsaJs,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GsvGroupJs {
    talker: String,
    signal: Option<SignalIdJs>,
    claimed_in_view: Option<u16>,
    satellites: Vec<GsvSatelliteJs>,
    complete: bool,
}

fn gsv_group_js(group: &GsvGroup) -> GsvGroupJs {
    GsvGroupJs {
        talker: talker_code(group.talker),
        signal: group.signal.map(signal_id_js),
        claimed_in_view: group.claimed_in_view,
        satellites: group.satellites.iter().map(gsv_satellite_js).collect(),
        complete: group.complete,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EpochSnapshotJs {
    time_of_day: Option<NmeaTimeJs>,
    date: Option<NmeaDateJs>,
    position: Option<PositionJs>,
    instant_utc_j2000_s: Option<f64>,
    pdop: Option<f64>,
    hdop: Option<f64>,
    vdop: Option<f64>,
    used_satellites: Vec<SatNumberJs>,
    satellites_in_view: usize,
    sentence_count: usize,
    diagnostics: DiagnosticsJs,
    gga: Option<GgaJs>,
    rmc: Option<RmcJs>,
    gll: Option<GllJs>,
    gst: Option<GstJs>,
    vtg: Option<VtgJs>,
    zda: Option<ZdaJs>,
    gsa: Vec<GsaEntryJs>,
    gsv: Vec<GsvGroupJs>,
}

fn epoch_js(epoch: &EpochSnapshot) -> EpochSnapshotJs {
    EpochSnapshotJs {
        time_of_day: epoch.time_of_day.map(NmeaTimeJs::from),
        date: epoch.date.map(NmeaDateJs::from),
        position: epoch.position().map(|position| PositionJs {
            lat_deg: position.lat_rad.to_degrees(),
            lon_deg: position.lon_rad.to_degrees(),
            height_m: position.height_m,
        }),
        instant_utc_j2000_s: epoch
            .instant_utc()
            .and_then(|instant| instant.julian_date())
            .map(|jd| j2000_seconds_from_split(jd.jd_whole, jd.fraction)),
        pdop: epoch.pdop(),
        hdop: epoch.hdop(),
        vdop: epoch.vdop(),
        used_satellites: epoch
            .used_satellites()
            .copied()
            .map(sat_number_js)
            .collect(),
        satellites_in_view: epoch.satellites_in_view(),
        sentence_count: epoch.sentence_count,
        diagnostics: diagnostics_js(&epoch.diagnostics),
        gga: epoch.gga.as_ref().map(gga_js),
        rmc: epoch.rmc.as_ref().map(rmc_js),
        gll: epoch.gll.as_ref().map(gll_js),
        gst: epoch.gst.as_ref().map(gst_js),
        vtg: epoch.vtg.as_ref().map(vtg_js),
        zda: epoch.zda.as_ref().map(zda_js),
        gsa: epoch
            .gsa
            .iter()
            .map(|entry| GsaEntryJs {
                system: entry.system.map(system_label),
                gsa: gsa_js(&entry.gsa),
            })
            .collect(),
        gsv: epoch.gsv.iter().map(gsv_group_js).collect(),
    }
}

/// Parsed NMEA log.
#[wasm_bindgen]
pub struct NmeaParseResult {
    log: NmeaLog,
    epochs: Vec<EpochSnapshot>,
    diagnostics: Diagnostics,
}

#[wasm_bindgen]
impl NmeaParseResult {
    #[wasm_bindgen(getter, js_name = sentenceCount)]
    pub fn sentence_count(&self) -> usize {
        self.log.sentences.len()
    }

    #[wasm_bindgen(getter, js_name = epochCount)]
    pub fn epoch_count(&self) -> usize {
        self.epochs.len()
    }

    #[wasm_bindgen(getter)]
    pub fn sentences(&self) -> Result<JsValue, JsValue> {
        let sentences: Vec<_> = self.log.sentences.iter().map(sentence_js).collect();
        to_value(&sentences)
    }

    #[wasm_bindgen(getter)]
    pub fn epochs(&self) -> Result<JsValue, JsValue> {
        let epochs: Vec<_> = self.epochs.iter().map(epoch_js).collect();
        to_value(&epochs)
    }

    #[wasm_bindgen(getter, unchecked_return_type = "NmeaDiagnostics")]
    pub fn diagnostics(&self) -> Result<JsValue, JsValue> {
        to_value(&diagnostics_js(&self.diagnostics))
    }
}

/// Parse NMEA sentences from bytes.
#[wasm_bindgen(js_name = parseNmea)]
pub fn parse_nmea(bytes: &[u8]) -> NmeaParseResult {
    let parsed = core_parse_nmea(bytes);
    let epochs = core_group_epochs(&parsed.value);
    NmeaParseResult {
        log: parsed.value,
        epochs,
        diagnostics: parsed.diagnostics,
    }
}

/// Parse bytes and return grouped epoch snapshots directly.
#[wasm_bindgen(js_name = nmeaEpochs)]
pub fn nmea_epochs(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let parsed = parse_nmea(bytes);
    parsed.epochs()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AccumulatorOutputJs {
    sentences: Vec<NmeaSentenceJs>,
    epochs: Vec<EpochSnapshotJs>,
    diagnostics: DiagnosticsJs,
    retained_length: usize,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct AccumulatorOptions {
    date: Option<DateInput>,
    max_sentences_per_epoch: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DateInput {
    year: u16,
    month: u8,
    day: u8,
}

impl DateInput {
    fn to_core(&self) -> Result<NmeaDate, JsValue> {
        NmeaDate::new(self.year, self.month, self.day).map_err(engine_error)
    }
}

/// Streaming NMEA epoch accumulator.
#[wasm_bindgen]
pub struct NmeaAccumulator {
    inner: CoreNmeaAccumulator,
}

#[wasm_bindgen]
impl NmeaAccumulator {
    #[wasm_bindgen(constructor)]
    pub fn new(options: JsValue) -> Result<NmeaAccumulator, JsValue> {
        let options: AccumulatorOptions = if options.is_null() || options.is_undefined() {
            AccumulatorOptions::default()
        } else {
            serde_wasm_bindgen::from_value(options)
                .map_err(|e| type_error(&format!("invalid NMEA accumulator options: {e}")))?
        };
        let mut inner = match options.date {
            Some(date) => CoreNmeaAccumulator::with_date(date.to_core()?),
            None => CoreNmeaAccumulator::new(),
        };
        if let Some(max) = options.max_sentences_per_epoch {
            inner = inner.with_max_sentences_per_epoch(max);
        }
        Ok(NmeaAccumulator { inner })
    }

    #[wasm_bindgen(getter, js_name = retainedLength)]
    pub fn retained_length(&self) -> usize {
        self.inner.retained_len()
    }

    #[wasm_bindgen(unchecked_return_type = "NmeaAccumulatorOutput")]
    pub fn push(&mut self, bytes: &[u8]) -> Result<JsValue, JsValue> {
        let output = self.inner.push_bytes(bytes);
        let out = AccumulatorOutputJs {
            sentences: output.sentences.iter().map(sentence_js).collect(),
            epochs: output.snapshots.iter().map(epoch_js).collect(),
            diagnostics: diagnostics_js(&output.diagnostics),
            retained_length: self.inner.retained_len(),
        };
        to_value(&out)
    }

    #[wasm_bindgen(unchecked_return_type = "NmeaAccumulatorOutput")]
    pub fn finish(&mut self) -> Result<JsValue, JsValue> {
        let output = self.inner.finish_with_output();
        let out = AccumulatorOutputJs {
            sentences: output.sentences.iter().map(sentence_js).collect(),
            epochs: output.snapshots.iter().map(epoch_js).collect(),
            diagnostics: diagnostics_js(&output.diagnostics),
            retained_length: self.inner.retained_len(),
        };
        to_value(&out)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GgaInput {
    #[serde(default)]
    talker: Option<String>,
    time_seconds_of_day: f64,
    lat_deg: f64,
    lon_deg: f64,
    #[serde(default)]
    coordinate_decimals: Option<u8>,
    #[serde(default)]
    quality: Option<u8>,
    #[serde(default)]
    satellites_used: Option<u8>,
    #[serde(default)]
    hdop: Option<f64>,
    #[serde(default)]
    altitude_msl_m: Option<f64>,
    #[serde(default)]
    geoid_separation_m: Option<f64>,
    #[serde(default)]
    differential_age_s: Option<f64>,
    #[serde(default)]
    differential_station_id: Option<u16>,
}

fn quality(value: u8) -> GgaQuality {
    match value {
        0 => GgaQuality::Invalid,
        1 => GgaQuality::GpsSps,
        2 => GgaQuality::Differential,
        3 => GgaQuality::Pps,
        4 => GgaQuality::RtkFixed,
        5 => GgaQuality::RtkFloat,
        6 => GgaQuality::Estimated,
        7 => GgaQuality::Manual,
        8 => GgaQuality::Simulator,
        other => GgaQuality::Other(other),
    }
}

/// Write a GGA sentence from a JS object.
#[wasm_bindgen(js_name = nmeaWriteGga)]
pub fn nmea_write_gga(request: JsValue) -> Result<String, JsValue> {
    let input: GgaInput = serde_wasm_bindgen::from_value(request)
        .map_err(|e| type_error(&format!("invalid GGA request: {e}")))?;
    let decimals = input.coordinate_decimals.unwrap_or(3);
    let gga = Gga {
        time: Some(
            NmeaTime::from_seconds_of_day_floor_centis(input.time_seconds_of_day)
                .map_err(engine_error)?,
        ),
        latitude: Some(
            NmeaCoordinate::from_degrees(input.lat_deg, true, decimals).map_err(engine_error)?,
        ),
        longitude: Some(
            NmeaCoordinate::from_degrees(input.lon_deg, false, decimals).map_err(engine_error)?,
        ),
        quality: Some(quality(input.quality.unwrap_or(1))),
        satellites_used: input.satellites_used,
        hdop: input.hdop,
        altitude_msl_m: input.altitude_msl_m,
        geoid_separation_m: input.geoid_separation_m,
        differential_age_s: input.differential_age_s,
        differential_station_id: input.differential_station_id,
    };
    let talker = NmeaTalker::parse(input.talker.as_deref().unwrap_or("GP"));
    core_write_gga(talker, &gga).map_err(engine_error)
}
