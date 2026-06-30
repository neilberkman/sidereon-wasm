//! GNSS constellation identity catalog binding: build normalized satellite
//! identity records from CelesTrak `gps-ops` OMM/JSON and an optional NAVCEN
//! status overlay, then validate and diff them.
//!
//! All catalog logic lives in `sidereon_core::constellation`; this module only
//! marshals the JS strings and plain objects into the core types and packages
//! the records, status rows, validation reports, and diffs back out. The records
//! cross the boundary as plain JSON objects (serde), matching the binding's
//! existing JSON-in / JSON-out surfaces.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::astro::omm::parse_json_array;
use sidereon_core::constellation::{
    changed as core_changed, diff as core_diff, from_celestrak_omm, from_celestrak_omm_lenient,
    glonass_fdma_channel as core_glonass_fdma_channel, gnss_sp3_id as core_gnss_sp3_id,
    is_valid as core_is_valid, merge_navcen, parse_navcen as core_parse_navcen, to_csv,
    validate as core_validate, validate_against_sp3_ids, BoolStyle, CelestrakSource,
    ConstellationError, Diff, FieldChange, NavcenSource, NavcenStatus, Record, RecordSource,
    SkippedOmm, Validation,
};
use sidereon_core::GnssSystem;

use crate::error::{engine_error, type_error};

// ── system <-> label ─────────────────────────────────────────────────────────

fn system_label(system: GnssSystem) -> &'static str {
    match system {
        GnssSystem::Gps => "gps",
        GnssSystem::Glonass => "glonass",
        GnssSystem::Galileo => "galileo",
        GnssSystem::BeiDou => "beidou",
        GnssSystem::Qzss => "qzss",
        GnssSystem::Navic => "navic",
        GnssSystem::Sbas => "sbas",
    }
}

fn system_from_label(label: &str) -> Result<GnssSystem, JsValue> {
    match label {
        "gps" => Ok(GnssSystem::Gps),
        "glonass" => Ok(GnssSystem::Glonass),
        "galileo" => Ok(GnssSystem::Galileo),
        "beidou" => Ok(GnssSystem::BeiDou),
        "qzss" => Ok(GnssSystem::Qzss),
        "navic" => Ok(GnssSystem::Navic),
        "sbas" => Ok(GnssSystem::Sbas),
        other => Err(type_error(&format!("unknown GNSS system label {other:?}"))),
    }
}

// ── serde mirror types ───────────────────────────────────────────────────────

/// CelesTrak `gps-ops` provenance: `{ group, objectName?, objectId?, epoch?, blockType? }`.
#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct CelestrakSourceJs {
    group: String,
    object_name: Option<String>,
    object_id: Option<String>,
    epoch: Option<String>,
    block_type: Option<String>,
}

impl From<&CelestrakSource> for CelestrakSourceJs {
    fn from(c: &CelestrakSource) -> Self {
        CelestrakSourceJs {
            group: c.group.clone(),
            object_name: c.object_name.clone(),
            object_id: c.object_id.clone(),
            epoch: c.epoch.clone(),
            block_type: c.block_type.clone(),
        }
    }
}

impl From<&CelestrakSourceJs> for CelestrakSource {
    fn from(c: &CelestrakSourceJs) -> Self {
        CelestrakSource {
            group: c.group.clone(),
            object_name: c.object_name.clone(),
            object_id: c.object_id.clone(),
            epoch: c.epoch.clone(),
            block_type: c.block_type.clone(),
        }
    }
}

/// NAVCEN provenance: `{ svn?, blockType?, plane?, slot?, clock?, nanuType?, nanuSubject?, activeNanu }`.
#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct NavcenSourceJs {
    svn: Option<u16>,
    block_type: Option<String>,
    plane: Option<String>,
    slot: Option<String>,
    clock: Option<String>,
    nanu_type: Option<String>,
    nanu_subject: Option<String>,
    active_nanu: bool,
}

impl From<&NavcenSource> for NavcenSourceJs {
    fn from(n: &NavcenSource) -> Self {
        NavcenSourceJs {
            svn: n.svn,
            block_type: n.block_type.clone(),
            plane: n.plane.clone(),
            slot: n.slot.clone(),
            clock: n.clock.clone(),
            nanu_type: n.nanu_type.clone(),
            nanu_subject: n.nanu_subject.clone(),
            active_nanu: n.active_nanu,
        }
    }
}

impl From<&NavcenSourceJs> for NavcenSource {
    fn from(n: &NavcenSourceJs) -> Self {
        NavcenSource {
            svn: n.svn,
            block_type: n.block_type.clone(),
            plane: n.plane.clone(),
            slot: n.slot.clone(),
            clock: n.clock.clone(),
            nanu_type: n.nanu_type.clone(),
            nanu_subject: n.nanu_subject.clone(),
            active_nanu: n.active_nanu,
        }
    }
}

/// Per-source provenance kept on a record.
#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct RecordSourceJs {
    celestrak: Option<CelestrakSourceJs>,
    navcen: Option<NavcenSourceJs>,
    navcen_conflict: Option<NavcenSourceJs>,
}

impl From<&RecordSource> for RecordSourceJs {
    fn from(s: &RecordSource) -> Self {
        RecordSourceJs {
            celestrak: s.celestrak.as_ref().map(CelestrakSourceJs::from),
            navcen: s.navcen.as_ref().map(NavcenSourceJs::from),
            navcen_conflict: s.navcen_conflict.as_ref().map(NavcenSourceJs::from),
        }
    }
}

impl From<&RecordSourceJs> for RecordSource {
    fn from(s: &RecordSourceJs) -> Self {
        RecordSource {
            celestrak: s.celestrak.as_ref().map(CelestrakSource::from),
            navcen: s.navcen.as_ref().map(NavcenSource::from),
            navcen_conflict: s.navcen_conflict.as_ref().map(NavcenSource::from),
        }
    }
}

/// A normalized GNSS satellite identity record.
#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct RecordJs {
    system: String,
    prn: u16,
    svn: Option<u16>,
    norad_id: u32,
    sp3_id: String,
    /// GLONASS FDMA L1/L2 frequency-channel number (`k`, in `-7..=6`); `null`
    /// for the CDMA constellations.
    fdma_channel: Option<i8>,
    active: bool,
    usable: bool,
    source: RecordSourceJs,
}

impl From<&Record> for RecordJs {
    fn from(r: &Record) -> Self {
        RecordJs {
            system: system_label(r.system).to_string(),
            prn: r.prn,
            svn: r.svn,
            norad_id: r.norad_id,
            sp3_id: r.sp3_id.clone(),
            fdma_channel: r.fdma_channel,
            active: r.active,
            usable: r.usable,
            source: RecordSourceJs::from(&r.source),
        }
    }
}

fn record_to_core(r: &RecordJs) -> Result<Record, JsValue> {
    Ok(Record {
        system: system_from_label(&r.system)?,
        prn: r.prn,
        svn: r.svn,
        norad_id: r.norad_id,
        sp3_id: r.sp3_id.clone(),
        fdma_channel: r.fdma_channel,
        active: r.active,
        usable: r.usable,
        source: RecordSource::from(&r.source),
    })
}

/// A parsed NAVCEN status row.
#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct NavcenStatusJs {
    system: String,
    prn: u16,
    svn: Option<u16>,
    usable: bool,
    active_nanu: bool,
    nanu_type: Option<String>,
    nanu_subject: Option<String>,
    plane: Option<String>,
    slot: Option<String>,
    block_type: Option<String>,
    clock: Option<String>,
}

impl From<&NavcenStatus> for NavcenStatusJs {
    fn from(s: &NavcenStatus) -> Self {
        NavcenStatusJs {
            system: system_label(s.system).to_string(),
            prn: s.prn,
            svn: s.svn,
            usable: s.usable,
            active_nanu: s.active_nanu,
            nanu_type: s.nanu_type.clone(),
            nanu_subject: s.nanu_subject.clone(),
            plane: s.plane.clone(),
            slot: s.slot.clone(),
            block_type: s.block_type.clone(),
            clock: s.clock.clone(),
        }
    }
}

fn status_to_core(s: &NavcenStatusJs) -> Result<NavcenStatus, JsValue> {
    Ok(NavcenStatus {
        system: system_from_label(&s.system)?,
        prn: s.prn,
        svn: s.svn,
        usable: s.usable,
        active_nanu: s.active_nanu,
        nanu_type: s.nanu_type.clone(),
        nanu_subject: s.nanu_subject.clone(),
        plane: s.plane.clone(),
        slot: s.slot.clone(),
        block_type: s.block_type.clone(),
        clock: s.clock.clone(),
    })
}

/// A `(system, prn)` pair: `{ system: "gps", prn: 19 }`. Keying duplicate /
/// inactive findings by system keeps a legitimate multi-system catalog (GPS
/// PRN 1 and Galileo PRN 1) from reading as a false collision.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SystemPrnJs {
    system: String,
    prn: u16,
}

impl SystemPrnJs {
    fn from_pair(pair: &(GnssSystem, u16)) -> Self {
        SystemPrnJs {
            system: system_label(pair.0).to_string(),
            prn: pair.1,
        }
    }

    fn to_pair(&self) -> Result<(GnssSystem, u16), JsValue> {
        Ok((system_from_label(&self.system)?, self.prn))
    }
}

/// A catalog validation report.
#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ValidationJs {
    missing_sp3_ids: Vec<String>,
    duplicate_prns: Vec<SystemPrnJs>,
    duplicate_norad_ids: Vec<u32>,
    inactive_unusable_prns: Vec<SystemPrnJs>,
    extra_sp3_ids: Vec<String>,
}

impl From<&Validation> for ValidationJs {
    fn from(v: &Validation) -> Self {
        ValidationJs {
            missing_sp3_ids: v.missing_sp3_ids.clone(),
            duplicate_prns: v
                .duplicate_prns
                .iter()
                .map(SystemPrnJs::from_pair)
                .collect(),
            duplicate_norad_ids: v.duplicate_norad_ids.clone(),
            inactive_unusable_prns: v
                .inactive_unusable_prns
                .iter()
                .map(SystemPrnJs::from_pair)
                .collect(),
            extra_sp3_ids: v.extra_sp3_ids.clone(),
        }
    }
}

fn validation_to_core(v: &ValidationJs) -> Result<Validation, JsValue> {
    Ok(Validation {
        missing_sp3_ids: v.missing_sp3_ids.clone(),
        duplicate_prns: v
            .duplicate_prns
            .iter()
            .map(SystemPrnJs::to_pair)
            .collect::<Result<Vec<_>, _>>()?,
        duplicate_norad_ids: v.duplicate_norad_ids.clone(),
        inactive_unusable_prns: v
            .inactive_unusable_prns
            .iter()
            .map(SystemPrnJs::to_pair)
            .collect::<Result<Vec<_>, _>>()?,
        extra_sp3_ids: v.extra_sp3_ids.clone(),
    })
}

/// One field change on a PRN held across both diffed snapshots.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FieldChangeJs<T> {
    system: String,
    prn: u16,
    from: T,
    to: T,
}

fn field_change_to_js<T, U, F>(c: &FieldChange<T>, map: F) -> FieldChangeJs<U>
where
    F: Fn(&T) -> U,
{
    FieldChangeJs {
        system: system_label(c.system).to_string(),
        prn: c.prn,
        from: map(&c.from),
        to: map(&c.to),
    }
}

fn field_change_to_core<T, U, F>(c: &FieldChangeJs<T>, map: F) -> Result<FieldChange<U>, JsValue>
where
    F: Fn(&T) -> U,
{
    Ok(FieldChange {
        system: system_from_label(&c.system)?,
        prn: c.prn,
        from: map(&c.from),
        to: map(&c.to),
    })
}

/// A change report between two catalog snapshots.
#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct DiffJs {
    added: Vec<RecordJs>,
    removed: Vec<RecordJs>,
    norad_reassigned: Vec<FieldChangeJs<u32>>,
    sp3_id_changed: Vec<FieldChangeJs<String>>,
    svn_changed: Vec<FieldChangeJs<Option<u16>>>,
    fdma_channel_changed: Vec<FieldChangeJs<Option<i8>>>,
    activity_changed: Vec<FieldChangeJs<bool>>,
    usability_changed: Vec<FieldChangeJs<bool>>,
}

impl From<&Diff> for DiffJs {
    fn from(d: &Diff) -> Self {
        DiffJs {
            added: d.added.iter().map(RecordJs::from).collect(),
            removed: d.removed.iter().map(RecordJs::from).collect(),
            norad_reassigned: d
                .norad_reassigned
                .iter()
                .map(|c| field_change_to_js(c, |v| *v))
                .collect(),
            sp3_id_changed: d
                .sp3_id_changed
                .iter()
                .map(|c| field_change_to_js(c, Clone::clone))
                .collect(),
            svn_changed: d
                .svn_changed
                .iter()
                .map(|c| field_change_to_js(c, |v| *v))
                .collect(),
            fdma_channel_changed: d
                .fdma_channel_changed
                .iter()
                .map(|c| field_change_to_js(c, |v| *v))
                .collect(),
            activity_changed: d
                .activity_changed
                .iter()
                .map(|c| field_change_to_js(c, |v| *v))
                .collect(),
            usability_changed: d
                .usability_changed
                .iter()
                .map(|c| field_change_to_js(c, |v| *v))
                .collect(),
        }
    }
}

fn diff_to_core(d: &DiffJs) -> Result<Diff, JsValue> {
    Ok(Diff {
        added: d
            .added
            .iter()
            .map(record_to_core)
            .collect::<Result<Vec<_>, _>>()?,
        removed: d
            .removed
            .iter()
            .map(record_to_core)
            .collect::<Result<Vec<_>, _>>()?,
        norad_reassigned: d
            .norad_reassigned
            .iter()
            .map(|c| field_change_to_core(c, |v| *v))
            .collect::<Result<Vec<_>, _>>()?,
        sp3_id_changed: d
            .sp3_id_changed
            .iter()
            .map(|c| field_change_to_core(c, Clone::clone))
            .collect::<Result<Vec<_>, _>>()?,
        svn_changed: d
            .svn_changed
            .iter()
            .map(|c| field_change_to_core(c, |v| *v))
            .collect::<Result<Vec<_>, _>>()?,
        fdma_channel_changed: d
            .fdma_channel_changed
            .iter()
            .map(|c| field_change_to_core(c, |v| *v))
            .collect::<Result<Vec<_>, _>>()?,
        activity_changed: d
            .activity_changed
            .iter()
            .map(|c| field_change_to_core(c, |v| *v))
            .collect::<Result<Vec<_>, _>>()?,
        usability_changed: d
            .usability_changed
            .iter()
            .map(|c| field_change_to_core(c, |v| *v))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

// ── marshalling helpers ──────────────────────────────────────────────────────

fn const_error(err: ConstellationError) -> JsValue {
    engine_error(err)
}

fn records_to_js(records: &[Record]) -> Result<JsValue, JsValue> {
    let out: Vec<RecordJs> = records.iter().map(RecordJs::from).collect();
    serde_wasm_bindgen::to_value(&out).map_err(|e| engine_error(e.to_string()))
}

fn records_from_js(value: JsValue, label: &str) -> Result<Vec<Record>, JsValue> {
    let parsed: Vec<RecordJs> = serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid {label}: {e}")))?;
    parsed.iter().map(record_to_core).collect()
}

// ── exports ──────────────────────────────────────────────────────────────────

/// Build constellation identity records from a CelesTrak OMM JSON-array text.
///
/// `system` is the constellation label (`"gps"`, `"glonass"`, `"galileo"`,
/// `"beidou"`, `"qzss"`); it dispatches the per-system `OBJECT_NAME` identity
/// adapter and defaults to `"gps"`. Parses the CelesTrak JSON array into OMM
/// records (skipping array elements that are not parseable OMM objects), then
/// derives normalized `Record` rows sorted by `(system, prn)`. Returns a
/// `Record[]`. Throws an `Error` on a JSON parse failure or a CelesTrak object
/// name with no PRN resolvable for `system`.
#[wasm_bindgen(js_name = fromCelestrakJson)]
pub fn from_celestrak_json(json: &str, system: Option<String>) -> Result<JsValue, JsValue> {
    let system = match system {
        Some(label) => system_from_label(&label)?,
        None => GnssSystem::Gps,
    };
    let parsed = parse_json_array(json).map_err(engine_error)?;
    let records = from_celestrak_omm(system, &parsed.omms).map_err(const_error)?;
    records_to_js(&records)
}

/// An OMM entry a lenient catalog build could not resolve to a `Record` for the
/// requested system: `{ objectName?, noradId }`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SkippedOmmJs {
    object_name: Option<String>,
    norad_id: u32,
}

impl From<&SkippedOmm> for SkippedOmmJs {
    fn from(s: &SkippedOmm) -> Self {
        SkippedOmmJs {
            object_name: s.object_name.clone(),
            norad_id: s.norad_id,
        }
    }
}

/// The lenient catalog result: `{ records, skipped }`.
#[derive(Serialize)]
struct CatalogJs {
    records: Vec<RecordJs>,
    skipped: Vec<SkippedOmmJs>,
}

/// Build constellation identity records from a CelesTrak OMM JSON-array text,
/// skipping (rather than throwing on) entries that do not resolve.
///
/// The lenient sibling of [`fromCelestrakJson`]: feed it a raw combined CelesTrak
/// `gnss` feed and it returns `{ records, skipped }` — `records` is the `Record[]`
/// for `system` (the same shape `fromCelestrakJson` returns, sorted by
/// `(system, prn)`), and `skipped` is a `SkippedOmm[]` of the entries whose
/// `OBJECT_NAME` did not resolve to a PRN for `system` (another constellation's
/// satellite in the combined feed, or a freshly launched satellite not yet in the
/// published slot/SVID table), each carrying its `{ objectName?, noradId }` so the
/// caller can triage. `system` defaults to `"gps"`. Throws an `Error` only on a
/// JSON parse failure.
#[wasm_bindgen(js_name = fromCelestrakJsonLenient)]
pub fn from_celestrak_json_lenient(json: &str, system: Option<String>) -> Result<JsValue, JsValue> {
    let system = match system {
        Some(label) => system_from_label(&label)?,
        None => GnssSystem::Gps,
    };
    let parsed = parse_json_array(json).map_err(engine_error)?;
    let catalog = from_celestrak_omm_lenient(system, &parsed.omms);
    let out = CatalogJs {
        records: catalog.records.iter().map(RecordJs::from).collect(),
        skipped: catalog.skipped.iter().map(SkippedOmmJs::from).collect(),
    };
    serde_wasm_bindgen::to_value(&out).map_err(|e| engine_error(e.to_string()))
}

/// Render the canonical SP3/RINEX satellite token for a constellation + PRN
/// (`("gps", 7)` -> `"G07"`, `("glonass", 13)` -> `"R13"`).
#[wasm_bindgen(js_name = gnssSp3Id)]
pub fn gnss_sp3_id_js(system: &str, prn: f64) -> Result<String, JsValue> {
    // Take the PRN as an f64 and validate it here: a `u16` parameter would let
    // wasm-bindgen silently coerce an out-of-range or fractional JS number
    // (-1 -> 65535, 1.5 -> 1) into a bogus identifier before this code runs.
    let system = system_from_label(system)?;
    if !prn.is_finite() || prn.fract() != 0.0 || prn < 1.0 || prn > f64::from(u16::MAX) {
        return Err(type_error(&format!(
            "invalid PRN {prn}: expected an integer in 1..={}",
            u16::MAX
        )));
    }
    Ok(core_gnss_sp3_id(system, prn as u16))
}

/// GLONASS FDMA L1/L2 frequency-channel number (`k`, in `-7..=6`) for an
/// orbital slot, from the published IGS/MCC slot-channel table. Returns `null`
/// for a slot with no tabulated channel.
#[wasm_bindgen(js_name = glonassFdmaChannel)]
pub fn glonass_fdma_channel_js(slot: u16) -> Option<i8> {
    core_glonass_fdma_channel(slot)
}

/// Parse NAVCEN's GPS constellation status HTML into status rows.
///
/// Returns a `NavcenStatus[]` sorted by PRN. Throws an `Error` when the HTML
/// carries no GPS rows or a required integer cell fails to parse.
#[wasm_bindgen(js_name = parseNavcen)]
pub fn parse_navcen(html: &str) -> Result<JsValue, JsValue> {
    let statuses = core_parse_navcen(html.as_bytes()).map_err(const_error)?;
    let out: Vec<NavcenStatusJs> = statuses.iter().map(NavcenStatusJs::from).collect();
    serde_wasm_bindgen::to_value(&out).map_err(|e| engine_error(e.to_string()))
}

/// Merge NAVCEN status rows into records by PRN.
///
/// `records` is a `Record[]` (from [`fromCelestrakJson`]); `statuses` is a
/// `NavcenStatus[]` (from [`parseNavcen`]). Returns the merged `Record[]` sorted
/// by PRN, filling SVN/usability and NAVCEN provenance on compatible matches.
#[wasm_bindgen(js_name = mergeNavcen)]
pub fn merge_navcen_js(records: JsValue, statuses: JsValue) -> Result<JsValue, JsValue> {
    let records = records_from_js(records, "records")?;
    let status_js: Vec<NavcenStatusJs> = serde_wasm_bindgen::from_value(statuses)
        .map_err(|e| type_error(&format!("invalid statuses: {e}")))?;
    let statuses = status_js
        .iter()
        .map(status_to_core)
        .collect::<Result<Vec<_>, _>>()?;
    let merged = merge_navcen(&records, &statuses);
    records_to_js(&merged)
}

/// Export records as the compact mapping CSV (`prn,norad_cat_id,active,sp3_id`).
///
/// `booleans` selects the `active` column rendering: `"lower"` (default) for
/// `true`/`false`, `"title"` for `True`/`False`.
#[wasm_bindgen(js_name = toCsv)]
pub fn to_csv_js(records: JsValue, booleans: Option<String>) -> Result<String, JsValue> {
    let records = records_from_js(records, "records")?;
    let style = match booleans.as_deref() {
        None | Some("lower") => BoolStyle::Lower,
        Some("title") => BoolStyle::Title,
        Some(other) => {
            return Err(type_error(&format!(
                "booleans must be \"lower\" or \"title\", got {other:?}"
            )))
        }
    };
    Ok(to_csv(&records, style))
}

/// Validate catalog identity without an SP3 product.
///
/// Returns a `Validation` reporting duplicate PRNs, duplicate NORAD ids, and
/// inactive/unusable PRNs.
#[wasm_bindgen(js_name = validate)]
pub fn validate_js(records: JsValue) -> Result<JsValue, JsValue> {
    let records = records_from_js(records, "records")?;
    let report = core_validate(&records);
    serde_wasm_bindgen::to_value(&ValidationJs::from(&report))
        .map_err(|e| engine_error(e.to_string()))
}

/// Validate catalog identity against a list of SP3/RINEX satellite tokens.
///
/// Returns a `Validation` that additionally reports active+usable catalog ids
/// missing from `ids` and GPS ids in `ids` absent from the active+usable catalog.
#[wasm_bindgen(js_name = validateAgainstSp3Ids)]
pub fn validate_against_sp3_ids_js(records: JsValue, ids: JsValue) -> Result<JsValue, JsValue> {
    let records = records_from_js(records, "records")?;
    let id_strings: Vec<String> = serde_wasm_bindgen::from_value(ids)
        .map_err(|e| type_error(&format!("invalid ids: {e}")))?;
    let id_refs: Vec<&str> = id_strings.iter().map(String::as_str).collect();
    let report = validate_against_sp3_ids(&records, &id_refs);
    serde_wasm_bindgen::to_value(&ValidationJs::from(&report))
        .map_err(|e| engine_error(e.to_string()))
}

/// Returns `true` when a validation report has no findings.
#[wasm_bindgen(js_name = isValid)]
pub fn is_valid_js(validation: JsValue) -> Result<bool, JsValue> {
    let report: ValidationJs = serde_wasm_bindgen::from_value(validation)
        .map_err(|e| type_error(&format!("invalid validation: {e}")))?;
    Ok(core_is_valid(&validation_to_core(&report)?))
}

/// Compare two catalog snapshots by `(system, prn)` identity.
///
/// `previous` and `current` are each a `Record[]`. Returns a `Diff` of added,
/// removed, and per-field changes on held PRNs.
#[wasm_bindgen(js_name = diff)]
pub fn diff_js(previous: JsValue, current: JsValue) -> Result<JsValue, JsValue> {
    let previous = records_from_js(previous, "previous records")?;
    let current = records_from_js(current, "current records")?;
    let report = core_diff(&previous, &current);
    serde_wasm_bindgen::to_value(&DiffJs::from(&report)).map_err(|e| engine_error(e.to_string()))
}

/// Returns `true` when a diff has any findings.
#[wasm_bindgen(js_name = changed)]
pub fn changed_js(diff: JsValue) -> Result<bool, JsValue> {
    let report: DiffJs = serde_wasm_bindgen::from_value(diff)
        .map_err(|e| type_error(&format!("invalid diff: {e}")))?;
    Ok(core_changed(&diff_to_core(&report)?))
}
