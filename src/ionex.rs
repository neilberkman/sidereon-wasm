//! IONEX vertical-TEC grid product, slant ionospheric group delay, policy-aware
//! evaluation, standalone regular grid evaluation, and precision-preserving warnings.
//!
//! All core modeling, validation, interpolation, and mapping logic resides in
//! `sidereon-core::atmosphere`. This module marshals data between JavaScript/TypeScript
//! and the core engine.

use std::cell::RefCell;
use std::f64::consts::PI;
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use sidereon_core::astro::time::civil::{
    j2000_seconds_from_split, split_julian_date_from_j2000_seconds,
};
use sidereon_core::astro::time::model::{Instant, InstantRepr, JulianDateSplit, TimeScale};
use sidereon_core::atmosphere::{
    ionex_slant_delay_results as core_ionex_slant_delay_results,
    ionex_slant_delay_with_policy as core_ionex_slant_delay_with_policy,
    regular_tec_grid_delay_xyz as core_iono_delay_xyz,
    regular_tec_grid_delay_xyz_with_policy as core_iono_delay_xyz_with_policy,
    regular_tec_xyz as core_tec_xyz, regular_tec_xyz_with_policy as core_tec_xyz_with_policy,
    Ionex as CoreIonex, IonexAssumedMapping as CoreIonexAssumedMapping, IonexCoverageError,
    IonexCoveragePolicy, IonexHeader as CoreIonexHeader,
    IonexMappingDeclaration as CoreIonexMappingDeclaration,
    IonexMappingFunction as CoreIonexMappingFunction, IonexMappingPolicy, IonexMissingNodePolicy,
    IonexMissingNodes, IonexNodeGap, IonexSlantDelayEvaluation, IonexSlantDelayStatus,
    IonexSlantPolicy as CoreIonexSlantPolicy, IonexSlantRefusal,
    IonexSlantRequest as CoreIonexSlantRequest, IonexWarning as CoreIonexWarning,
    TecGrid as CoreTecGrid, TecGridEpoch as CoreTecGridEpoch, TecGridError as CoreTecGridError,
    TecGridEvalOptions as CoreTecGridEvalOptions, TecGridSamples as CoreTecGridSamples,
    TecGridShellGeometry as CoreTecGridShellGeometry, TecSample as CoreTecSample, TecSamplesError,
};
use sidereon_core::Wgs84Geodetic;

use crate::error::{engine_error, range_error, require_finite, type_error};

/// pi/180 as a single rounded constant, so boundary conversion is `deg * DEG_TO_RAD`
/// (one multiply, matching the engine's reference bindings).
const DEG_TO_RAD: f64 = PI / 180.0;

/// `Number.MIN_SAFE_INTEGER` in IEEE-754 double precision (-2^53 + 1).
const JS_MIN_SAFE_INTEGER: f64 = -9_007_199_254_740_991.0;
/// `Number.MAX_SAFE_INTEGER` in IEEE-754 double precision (2^53 - 1).
const JS_MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

/// Describe a value JavaScript threw or returned, for an error message.
///
/// A thrown value need not be an `Error`; a string, a number and a plain object
/// all reach here, so the text falls back through the shapes in turn.
fn describe_js(value: &JsValue) -> String {
    if let Some(text) = value.as_string() {
        return text;
    }
    if let Ok(message) = js_sys::Reflect::get(value, &JsValue::from_str("message")) {
        if let Some(text) = message.as_string() {
            return text;
        }
    }
    format!("{value:?}")
}

/// Refuse an unknown own property on a supplied input object.
///
/// serde cannot do this for these inputs, and `deny_unknown_fields` on the DTO
/// would be inert. `serde_wasm_bindgen` 0.6.5 deserializes a struct by asking
/// the object for each *declared* field in turn — `ObjectAccess::next_key_seed`
/// iterates the declared field list and `deserialize_struct` hands it nothing
/// else (its `src/de.rs`) — so a property the struct does not declare is never
/// offered to serde at all and there is nothing for serde to reject. Without
/// this walk, `heightMap` beside `heightMaps` reads as an absent cube and a
/// typo becomes a silently different product.
///
/// `Object::getOwnPropertyNames` lists every own string property name,
/// enumerable or not, and does not invoke an accessor, so a getter that throws
/// still surfaces from the read that follows rather than being swallowed here.
/// Enumerability does not decide whether a name is a typo: `rmsTec` defined
/// non-enumerably drops just as silently as an enumerable one, so both are
/// refused. A name the container declares stays usable however it is defined.
/// An inherited property is deliberately not refused: serde reads these fields
/// through the prototype chain, so an object holding its state there stays a
/// usable input. Own symbol keys are not string field names and are not walked;
/// nothing here validates them. A supplied slant policy is checked by its own
/// walk in `parse_slant_policy`, which reads the same own string names and
/// additionally skips a wasm-bindgen instance's pointer property.
fn reject_unknown_keys(object: &JsValue, container: &str, known: &[&str]) -> Result<(), JsValue> {
    let names = js_sys::Object::get_own_property_names(object.unchecked_ref::<js_sys::Object>());
    for index in 0..names.length() {
        let Some(key) = names.get(index).as_string() else {
            continue;
        };
        if !known.contains(&key.as_str()) {
            return Err(type_error(&format!(
                "unknown {container} property '{key}'; expected one of {}",
                known.join(", ")
            )));
        }
    }
    Ok(())
}

// --- Mapping function and declaration representations -----------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum IonexMappingFunctionJs {
    NoMapping {
        #[serde(default = "default_none_code")]
        code: String,
    },
    Cosz {
        #[serde(default = "default_cosz_code")]
        code: String,
    },
    QFactor {
        #[serde(default = "default_qfac_code")]
        code: String,
    },
    Other {
        code: String,
    },
}

fn default_none_code() -> String {
    "NONE".to_string()
}
fn default_cosz_code() -> String {
    "COSZ".to_string()
}
fn default_qfac_code() -> String {
    "QFAC".to_string()
}

impl IonexMappingFunctionJs {
    pub fn from_core(func: &CoreIonexMappingFunction) -> Self {
        match func {
            CoreIonexMappingFunction::NoMapping => Self::NoMapping {
                code: "NONE".to_string(),
            },
            CoreIonexMappingFunction::CosZ => Self::Cosz {
                code: "COSZ".to_string(),
            },
            CoreIonexMappingFunction::QFactor => Self::QFactor {
                code: "QFAC".to_string(),
            },
            CoreIonexMappingFunction::Other(code) => Self::Other { code: code.clone() },
        }
    }

    /// The core mapping function this tagged value names.
    ///
    /// A known tag carries the code the spec fixes for it, so a supplied code
    /// that names something else is a contradiction between the two fields and
    /// is refused rather than dropped. An `OTHER` code is kept as written,
    /// including whitespace: the writer is what decides whether a code can be
    /// put back on a `MAPPING FUNCTION` record.
    pub fn to_core(&self) -> Result<CoreIonexMappingFunction, JsValue> {
        match self {
            Self::NoMapping { code } => {
                if code != "NONE" {
                    return Err(type_error(&format!(
                        "mapping function NO_MAPPING carries the code 'NONE', not '{code}'"
                    )));
                }
                Ok(CoreIonexMappingFunction::NoMapping)
            }
            Self::Cosz { code } => {
                if code != "COSZ" {
                    return Err(type_error(&format!(
                        "mapping function COSZ carries the code 'COSZ', not '{code}'"
                    )));
                }
                Ok(CoreIonexMappingFunction::CosZ)
            }
            Self::QFactor { code } => {
                if code != "QFAC" {
                    return Err(type_error(&format!(
                        "mapping function Q_FACTOR carries the code 'QFAC', not '{code}'"
                    )));
                }
                Ok(CoreIonexMappingFunction::QFactor)
            }
            Self::Other { code } => Ok(CoreIonexMappingFunction::Other(code.clone())),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum IonexMappingDeclarationJs {
    Declared { function: IonexMappingFunctionJs },
    Absent,
}

impl IonexMappingDeclarationJs {
    pub fn from_core(decl: &CoreIonexMappingDeclaration) -> Self {
        match decl {
            CoreIonexMappingDeclaration::Declared(func) => Self::Declared {
                function: IonexMappingFunctionJs::from_core(func),
            },
            CoreIonexMappingDeclaration::Absent => Self::Absent,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IonexAssumedMappingJs {
    pub kind: String,
    pub message: String,
}

impl IonexAssumedMappingJs {
    pub fn from_core(assumed: CoreIonexAssumedMapping) -> Self {
        let (kind, message) = match assumed {
            CoreIonexAssumedMapping::NoMapping => (
                "NO_MAPPING",
                "the product declares NONE; single-layer 1/cos(z') was applied",
            ),
            CoreIonexAssumedMapping::QFactor => (
                "Q_FACTOR",
                "the product declares QFAC; single-layer 1/cos(z') was applied",
            ),
            CoreIonexAssumedMapping::Other => (
                "OTHER",
                "the product declares an unstandardized code; single-layer 1/cos(z') was applied",
            ),
            CoreIonexAssumedMapping::Absent => (
                "ABSENT",
                "the product declares no mapping function; single-layer 1/cos(z') was applied",
            ),
        };
        Self {
            kind: kind.to_string(),
            message: message.to_string(),
        }
    }
}

// --- Header representation --------------------------------------------------

/// The header a file carrying none of the descriptive records reads as.
///
/// `IonexHeader::unstated` is crate-private in the engine and `IonexHeader` is
/// `#[non_exhaustive]`, so the public path is the declaring constructor with
/// the declaration taken back off.
fn unstated_header() -> CoreIonexHeader {
    let mut header = CoreIonexHeader::new(CoreIonexMappingFunction::CosZ);
    header.mapping_function = None;
    header
}

/// The header shape read back out of a product.
///
/// `mappingFunction` is `null` where the product declares none, and
/// `mappingDeclaration` always names the case, so a caller never has to read an
/// absence out of a missing property.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IonexHeaderJs {
    pub version: f64,
    pub satellite_system: String,
    pub program: String,
    pub run_by: String,
    pub date: String,
    pub descriptions: Vec<String>,
    pub comments: Vec<String>,
    pub interval_s: u32,
    pub mapping_function: Option<IonexMappingFunctionJs>,
    pub mapping_declaration: IonexMappingDeclarationJs,
    pub elevation_cutoff_deg: f64,
    pub observables_used: String,
    pub station_count: Option<u32>,
    pub satellite_count: Option<u32>,
    pub maps_in_file: Option<u32>,
}

/// Every header record except the two mapping ones, which are read
/// presence-aware from the JavaScript object rather than through serde.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct IonexHeaderScalarsJs {
    version: f64,
    satellite_system: String,
    program: String,
    run_by: String,
    date: String,
    descriptions: Vec<String>,
    comments: Vec<String>,
    interval_s: u32,
    elevation_cutoff_deg: f64,
    observables_used: String,
    #[serde(default)]
    station_count: Option<u32>,
    #[serde(default)]
    satellite_count: Option<u32>,
    #[serde(default)]
    maps_in_file: Option<u32>,
}

/// Every property an `IonexHeaderInput` may carry, including the two mapping
/// records that are read presence-aware rather than through serde. An emitted
/// `IonexHeader` carries all fifteen, so a header read back out of a product
/// feeds straight back into a constructor.
const IONEX_HEADER_KEYS: &[&str] = &[
    "version",
    "satelliteSystem",
    "program",
    "runBy",
    "date",
    "descriptions",
    "comments",
    "intervalS",
    "mappingFunction",
    "mappingDeclaration",
    "elevationCutoffDeg",
    "observablesUsed",
    "stationCount",
    "satelliteCount",
    "mapsInFile",
];

fn header_to_js(header: &CoreIonexHeader) -> IonexHeaderJs {
    let mapping_function = header
        .mapping_function
        .as_ref()
        .map(IonexMappingFunctionJs::from_core);
    let mapping_declaration = match &mapping_function {
        Some(function) => IonexMappingDeclarationJs::Declared {
            function: function.clone(),
        },
        None => IonexMappingDeclarationJs::Absent,
    };
    IonexHeaderJs {
        version: header.version,
        satellite_system: header.satellite_system.clone(),
        program: header.program.clone(),
        run_by: header.run_by.clone(),
        date: header.date.clone(),
        descriptions: header.descriptions.clone(),
        comments: header.comments.clone(),
        interval_s: header.interval_s,
        mapping_function,
        mapping_declaration,
        elevation_cutoff_deg: header.elevation_cutoff_deg,
        observables_used: header.observables_used.clone(),
        station_count: header.station_count,
        satellite_count: header.satellite_count,
        maps_in_file: header.maps_in_file,
    }
}

/// What a JavaScript object says about one optional property.
///
/// A property the object does not carry, a property explicitly set to `null`,
/// and a property carrying a value are three distinct statements, and the
/// mapping records need all three told apart: `null` beside a `DECLARED`
/// declaration contradicts it, while an absent property says nothing at all.
/// An ordinary `Option`, and an `Option<Option<_>>` behind serde's `default`,
/// collapse the first two into one.
#[derive(Clone, Debug)]
enum Supplied<T> {
    Absent,
    Null,
    Value(T),
}

/// Read one optional property, telling absent from `null` and propagating a
/// throwing accessor rather than reading it as an absence.
///
/// Both reads re-raise what JavaScript threw, unchanged: the presence test
/// (a `Proxy` `has` trap can throw) and the read itself (a getter can throw).
/// The caller's own error class, `message` and custom properties survive, so a
/// `catch` matching on `instanceof` or on a field still matches. Restating the
/// text as a fresh `TypeError` would leave it nothing to match on. A value that
/// is read but does not deserialize, or contradicts another record, is this
/// module's own judgement and stays a `TypeError` written here.
fn supplied_property<T: serde::de::DeserializeOwned>(
    object: &JsValue,
    key: &str,
) -> Result<Supplied<T>, JsValue> {
    let key_js = JsValue::from_str(key);
    if !js_sys::Reflect::has(object, &key_js)? {
        return Ok(Supplied::Absent);
    }
    let value = js_sys::Reflect::get(object, &key_js)?;
    if value.is_null() {
        return Ok(Supplied::Null);
    }
    if value.is_undefined() {
        return Ok(Supplied::Absent);
    }
    let parsed = serde_wasm_bindgen::from_value(value)
        .map_err(|err| type_error(&format!("invalid '{key}': {err}")))?;
    Ok(Supplied::Value(parsed))
}

/// Which mapping function a supplied header states, refusing a `mappingFunction`
/// and a `mappingDeclaration` that contradict each other.
fn header_mapping_from_js(object: &JsValue) -> Result<Option<CoreIonexMappingFunction>, JsValue> {
    let function: Supplied<IonexMappingFunctionJs> = supplied_property(object, "mappingFunction")?;
    let declaration: Supplied<IonexMappingDeclarationJs> =
        supplied_property(object, "mappingDeclaration")?;

    match (function, declaration) {
        // Neither record states anything.
        (Supplied::Absent, Supplied::Absent)
        | (Supplied::Null, Supplied::Absent)
        | (Supplied::Absent, Supplied::Null)
        | (Supplied::Null, Supplied::Null) => Ok(None),

        // One record states it and the other says nothing.
        (Supplied::Value(function), Supplied::Absent) => Ok(Some(function.to_core()?)),
        (Supplied::Absent, Supplied::Value(IonexMappingDeclarationJs::Declared { function })) => {
            Ok(Some(function.to_core()?))
        }
        (Supplied::Absent, Supplied::Value(IonexMappingDeclarationJs::Absent)) => Ok(None),

        // Both records agree that there is no declaration.
        (Supplied::Null, Supplied::Value(IonexMappingDeclarationJs::Absent)) => Ok(None),

        // Both records are supplied and contradict each other.
        (Supplied::Null, Supplied::Value(IonexMappingDeclarationJs::Declared { function })) => {
            Err(type_error(&format!(
                "mappingFunction is null but mappingDeclaration declares {}",
                function.to_core()?.code()
            )))
        }
        (Supplied::Value(function), Supplied::Value(IonexMappingDeclarationJs::Absent)) => {
            Err(type_error(&format!(
                "mappingFunction declares {} but mappingDeclaration is ABSENT",
                function.to_core()?.code()
            )))
        }
        (Supplied::Value(function), Supplied::Null) => Err(type_error(&format!(
            "mappingFunction declares {} but mappingDeclaration is null",
            function.to_core()?.code()
        ))),
        (
            Supplied::Value(function),
            Supplied::Value(IonexMappingDeclarationJs::Declared {
                function: declared_function,
            }),
        ) => {
            let function = function.to_core()?;
            let declared_function = declared_function.to_core()?;
            if function != declared_function {
                return Err(type_error(&format!(
                    "mappingFunction declares {} but mappingDeclaration declares {}",
                    function.code(),
                    declared_function.code()
                )));
            }
            Ok(Some(function))
        }
    }
}

/// Read a complete header from a JavaScript object.
fn header_from_js(value: &JsValue) -> Result<CoreIonexHeader, JsValue> {
    if !value.is_object() {
        return Err(type_error("IONEX header must be an object"));
    }
    reject_unknown_keys(value, "IONEX header", IONEX_HEADER_KEYS)?;
    let scalars: IonexHeaderScalarsJs = serde_wasm_bindgen::from_value(value.clone())
        .map_err(|err| type_error(&format!("invalid IONEX header: {err}")))?;
    let mapping_function = header_mapping_from_js(value)?;

    let mut header = unstated_header();
    header.version = scalars.version;
    header.satellite_system = scalars.satellite_system;
    header.program = scalars.program;
    header.run_by = scalars.run_by;
    header.date = scalars.date;
    header.descriptions = scalars.descriptions;
    header.comments = scalars.comments;
    header.interval_s = scalars.interval_s;
    header.mapping_function = mapping_function;
    header.elevation_cutoff_deg = scalars.elevation_cutoff_deg;
    header.observables_used = scalars.observables_used;
    header.station_count = scalars.station_count;
    header.satellite_count = scalars.satellite_count;
    header.maps_in_file = scalars.maps_in_file;
    Ok(header)
}

// --- Sample structures ------------------------------------------------------

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct TecGridSamplesJs {
    map_epochs_j2000_s: Vec<f64>,
    lat_nodes_deg: Vec<f64>,
    lon_nodes_deg: Vec<f64>,
    dlat_deg: f64,
    dlon_deg: f64,
    shell_height_km: f64,
    base_radius_km: f64,
    exponent: i32,
    tec_maps: Vec<Vec<Vec<Option<f64>>>>,
    rms_maps: Option<Vec<Vec<Vec<Option<f64>>>>>,
    height_maps: Option<Vec<Vec<Vec<Option<f64>>>>>,
    header: IonexHeaderJs,
}

/// Grid samples as supplied from JavaScript. The header is read separately,
/// presence-aware, so its two mapping records can be told apart.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct TecGridSamplesInputJs {
    map_epochs_j2000_s: Vec<f64>,
    lat_nodes_deg: Vec<f64>,
    lon_nodes_deg: Vec<f64>,
    dlat_deg: f64,
    dlon_deg: f64,
    shell_height_km: f64,
    base_radius_km: f64,
    exponent: i32,
    tec_maps: Vec<Vec<Vec<Option<f64>>>>,
    #[serde(default)]
    rms_maps: Option<Vec<Vec<Vec<Option<f64>>>>>,
    #[serde(default)]
    height_maps: Option<Vec<Vec<Vec<Option<f64>>>>>,
}

/// Every property a `TecGridSamplesInput` may carry. `header` is not one of the
/// struct's own fields — it is lifted off the object separately, presence-aware
/// — so it is named here rather than derived from the struct.
const TEC_GRID_SAMPLES_KEYS: &[&str] = &[
    "mapEpochsJ2000S",
    "latNodesDeg",
    "lonNodesDeg",
    "dlatDeg",
    "dlonDeg",
    "shellHeightKm",
    "baseRadiusKm",
    "exponent",
    "tecMaps",
    "rmsMaps",
    "heightMaps",
    "header",
];

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct TecSampleJs {
    epoch_j2000_s: f64,
    lat_deg: f64,
    lon_deg: f64,
    vtec_tecu: Option<f64>,
    #[serde(default)]
    rms_tecu: Option<f64>,
    #[serde(default)]
    height_offset_km: Option<f64>,
}

/// Every property one `TecSample` may carry.
const TEC_SAMPLE_KEYS: &[&str] = &[
    "epochJ2000S",
    "latDeg",
    "lonDeg",
    "vtecTecu",
    "rmsTecu",
    "heightOffsetKm",
];

/// Serialize to plain JavaScript objects and arrays, never `Map`s, so a caller
/// reads a result with ordinary property access.
fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|err| engine_error(format!("failed to serialize IONEX result: {err}")))
}

/// Attach a typed `detail` to a thrown error, or say why it could not be.
///
/// The typed-detail contract is part of the public surface, so a serialization
/// or property-set failure surfaces as its own error rather than a silently
/// bare exception.
fn error_with_detail<T: Serialize>(name: &str, message: &str, detail: &T) -> JsValue {
    let detail_value = match to_js(detail) {
        Ok(value) => value,
        Err(err) => {
            return engine_error(format!(
                "{message} (the typed detail could not be serialized: {})",
                describe_js(&err)
            ))
        }
    };
    let js_error = js_sys::Error::new(message);
    js_error.set_name(name);
    let value: JsValue = js_error.into();
    match js_sys::Reflect::set(&value, &JsValue::from_str("detail"), &detail_value) {
        Ok(true) => value,
        Ok(false) => engine_error(format!(
            "{message} (the typed detail could not be attached to the error)"
        )),
        Err(err) => engine_error(format!(
            "{message} (attaching the typed detail threw: {})",
            describe_js(&err)
        )),
    }
}

fn tec_samples_error(err: TecSamplesError) -> JsValue {
    range_error(&err.to_string())
}

/// An exact integer J2000 second read from a JavaScript number.
///
/// A `number` carries integers exactly only inside the safe-integer range, so a
/// value outside it is refused rather than cast: `i64::MAX as f64` is 2^63, one
/// past the largest `i64`, which makes a bare `> i64::MAX as f64` test accept a
/// value that then wraps.
fn exact_j2000_second(value: f64, field: &str) -> Result<i64, String> {
    if !value.is_finite() {
        return Err(format!("{field} must be a finite number"));
    }
    if value.fract() != 0.0 {
        return Err(format!("{field} must be an integer second"));
    }
    if !(JS_MIN_SAFE_INTEGER..=JS_MAX_SAFE_INTEGER).contains(&value) {
        return Err(format!(
            "{field} must lie within the JavaScript safe integer range \
             [-9007199254740991, 9007199254740991]"
        ));
    }
    Ok(value as i64)
}

pub(crate) fn j2000_seconds_to_instant(epoch_j2000_s: f64) -> Result<Instant, JsValue> {
    let seconds = exact_j2000_second(epoch_j2000_s, "epochJ2000S").map_err(|m| range_error(&m))?;
    let (jd_whole, fraction) = split_julian_date_from_j2000_seconds(seconds);
    let split = JulianDateSplit::new(jd_whole, fraction).map_err(|error| {
        let message = error.to_string();
        crate::tropo::time_model_error_with(error, "Error", &message)
    })?;
    Ok(Instant::from_julian_date(TimeScale::Utc, split))
}

fn instant_to_j2000_seconds(epoch: Instant) -> f64 {
    match epoch.repr {
        InstantRepr::JulianDate(split) => {
            j2000_seconds_from_split(split.jd_whole, split.fraction).round()
        }
        InstantRepr::Nanos(nanos) => (nanos as f64 / 1.0e9).round(),
    }
}

fn grid_samples_to_core(
    samples: TecGridSamplesInputJs,
    header: CoreIonexHeader,
) -> Result<CoreTecGridSamples, JsValue> {
    let map_epochs = samples
        .map_epochs_j2000_s
        .into_iter()
        .map(j2000_seconds_to_instant)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(CoreTecGridSamples {
        map_epochs,
        lat_nodes_deg: samples.lat_nodes_deg,
        lon_nodes_deg: samples.lon_nodes_deg,
        dlat_deg: samples.dlat_deg,
        dlon_deg: samples.dlon_deg,
        shell_height_km: samples.shell_height_km,
        base_radius_km: samples.base_radius_km,
        exponent: samples.exponent,
        tec_maps: samples.tec_maps,
        rms_maps: samples.rms_maps.unwrap_or_default(),
        height_maps: samples.height_maps.unwrap_or_default(),
        header,
    })
}

fn grid_samples_from_core(samples: CoreTecGridSamples) -> TecGridSamplesJs {
    let rms_maps = (!samples.rms_maps.is_empty()).then_some(samples.rms_maps);
    let height_maps = (!samples.height_maps.is_empty()).then_some(samples.height_maps);
    TecGridSamplesJs {
        map_epochs_j2000_s: samples
            .map_epochs
            .into_iter()
            .map(instant_to_j2000_seconds)
            .collect(),
        lat_nodes_deg: samples.lat_nodes_deg,
        lon_nodes_deg: samples.lon_nodes_deg,
        dlat_deg: samples.dlat_deg,
        dlon_deg: samples.dlon_deg,
        shell_height_km: samples.shell_height_km,
        base_radius_km: samples.base_radius_km,
        exponent: samples.exponent,
        tec_maps: samples.tec_maps,
        rms_maps,
        height_maps,
        header: header_to_js(&samples.header),
    }
}

fn node_sample_to_core(sample: TecSampleJs) -> Result<CoreTecSample, JsValue> {
    Ok(CoreTecSample {
        epoch: j2000_seconds_to_instant(sample.epoch_j2000_s)?,
        lat_deg: sample.lat_deg,
        lon_deg: sample.lon_deg,
        vtec_tecu: sample.vtec_tecu,
        rms_tecu: sample.rms_tecu,
        height_offset_km: sample.height_offset_km,
    })
}

fn node_sample_from_core(sample: CoreTecSample) -> TecSampleJs {
    TecSampleJs {
        epoch_j2000_s: instant_to_j2000_seconds(sample.epoch),
        lat_deg: sample.lat_deg,
        lon_deg: sample.lon_deg,
        vtec_tecu: sample.vtec_tecu,
        rms_tecu: sample.rms_tecu,
        height_offset_km: sample.height_offset_km,
    }
}

// --- Policy -----------------------------------------------------------------

/// The three independent axes an IONEX slant-delay evaluation applies.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, Default)]
pub struct IonexSlantPolicy {
    pub(crate) inner: CoreIonexSlantPolicy,
}

fn parse_coverage_policy(value: &str) -> Result<IonexCoveragePolicy, JsValue> {
    match value.trim().to_ascii_lowercase().as_str() {
        "strict" => Ok(IonexCoveragePolicy::Strict),
        "hold" => Ok(IonexCoveragePolicy::Hold),
        other => Err(type_error(&format!(
            "invalid coverage policy '{other}'; expected 'strict' or 'hold'"
        ))),
    }
}

fn parse_missing_node_policy(value: &str) -> Result<IonexMissingNodePolicy, JsValue> {
    match value.trim().to_ascii_lowercase().as_str() {
        "strict" => Ok(IonexMissingNodePolicy::Strict),
        "renormalize" => Ok(IonexMissingNodePolicy::Renormalize),
        other => Err(type_error(&format!(
            "invalid missing-nodes policy '{other}'; expected 'strict' or 'renormalize'"
        ))),
    }
}

fn parse_mapping_policy(value: &str) -> Result<IonexMappingPolicy, JsValue> {
    match value.trim().to_ascii_lowercase().as_str() {
        "declared" => Ok(IonexMappingPolicy::Declared),
        "singlelayer" | "single_layer" => Ok(IonexMappingPolicy::SingleLayer),
        other => Err(type_error(&format!(
            "invalid mapping policy '{other}'; expected 'declared' or 'singleLayer'"
        ))),
    }
}

/// Read one policy axis, propagating a throwing accessor.
///
/// `Reflect::get` walks the prototype chain, so this reads a plain object's own
/// property and an `IonexSlantPolicy` instance's prototype getter the same way.
/// What the accessor threw is re-raised unchanged — same object, same class,
/// same `message` and custom properties — rather than restated as a fresh
/// `TypeError`. A value that is read and is then not a string, or names
/// something outside the axis, is this module's own refusal and stays a
/// `TypeError` written here.
fn policy_property(policy: &JsValue, key: &str) -> Result<Option<String>, JsValue> {
    let value = js_sys::Reflect::get(policy, &JsValue::from_str(key))?;
    if value.is_undefined() || value.is_null() {
        return Ok(None);
    }
    let text = value
        .as_string()
        .ok_or_else(|| type_error(&format!("policy.{key} must be a string")))?;
    Ok(Some(text))
}

/// The policy a JavaScript value states.
///
/// `undefined` and `null` alone select the engine default (strict coverage,
/// strict missing nodes, single-layer mapping). Anything else must be an object
/// naming only the three axes: a supplied policy that cannot be read is refused
/// rather than quietly replaced by the default. Each axis is independent, so an
/// object that names one leaves the other two at their defaults. Where an
/// accessor throws, that value is re-raised as the caller threw it; the
/// refusals written here are this module's own.
fn parse_slant_policy(policy: JsValue) -> Result<CoreIonexSlantPolicy, JsValue> {
    if policy.is_undefined() || policy.is_null() {
        return Ok(CoreIonexSlantPolicy::default());
    }
    if !policy.is_object() {
        return Err(type_error(
            "policy must be an object or an IonexSlantPolicy",
        ));
    }

    // Own string property names, enumerable or not, so an unknown axis on a
    // plain object is refused rather than ignored. Enumerability does not
    // decide whether a name is a typo: `missingNode` defined non-enumerably
    // drops as silently as an enumerable one, and `Object::keys` would not see
    // it. Reading the names does not invoke an accessor, so a throwing getter
    // on a known axis still surfaces from the read below. Own symbol keys are
    // not string field names and are not walked. An inherited property is not
    // refused — a wasm-bindgen class instance carries its pointer under the
    // exact own name `__wbg_ptr` and exposes its axes on the prototype. That
    // generated property alone is skipped; lookalike names remain unknown.
    let keys = js_sys::Object::get_own_property_names(policy.unchecked_ref::<js_sys::Object>());
    for index in 0..keys.length() {
        let Some(key) = keys.get(index).as_string() else {
            continue;
        };
        if key == "__wbg_ptr" {
            continue;
        }
        if !matches!(
            key.as_str(),
            "coverage" | "missingNodes" | "missing_nodes" | "mapping"
        ) {
            return Err(type_error(&format!("unknown policy property '{key}'")));
        }
    }

    let mut parsed = CoreIonexSlantPolicy::default();

    if let Some(coverage) = policy_property(&policy, "coverage")? {
        parsed = parsed.with_coverage(parse_coverage_policy(&coverage)?);
    }

    // `missingNodes` is the spelling; `missing_nodes` is accepted as an alias.
    // Both supplied must agree: choosing one silently would let a caller believe
    // a policy applied that did not.
    let missing_camel = policy_property(&policy, "missingNodes")?;
    let missing_snake = policy_property(&policy, "missing_nodes")?;
    let missing = match (missing_camel, missing_snake) {
        (None, None) => None,
        (Some(value), None) | (None, Some(value)) => Some(value),
        (Some(camel), Some(snake)) => {
            let camel_policy = parse_missing_node_policy(&camel)?;
            let snake_policy = parse_missing_node_policy(&snake)?;
            if camel_policy != snake_policy {
                return Err(type_error(&format!(
                    "policy.missingNodes is '{camel}' but policy.missing_nodes is '{snake}'"
                )));
            }
            Some(camel)
        }
    };
    if let Some(missing) = missing {
        parsed = parsed.with_missing_nodes(parse_missing_node_policy(&missing)?);
    }

    if let Some(mapping) = policy_property(&policy, "mapping")? {
        parsed = parsed.with_mapping(parse_mapping_policy(&mapping)?);
    }

    Ok(parsed)
}

#[wasm_bindgen]
impl IonexSlantPolicy {
    /// A policy naming any of the three axes; an omitted axis keeps its default.
    #[wasm_bindgen(constructor)]
    pub fn new(
        coverage: Option<String>,
        missing_nodes: Option<String>,
        mapping: Option<String>,
    ) -> Result<IonexSlantPolicy, JsValue> {
        let mut inner = CoreIonexSlantPolicy::default();
        if let Some(coverage) = coverage {
            inner = inner.with_coverage(parse_coverage_policy(&coverage)?);
        }
        if let Some(missing_nodes) = missing_nodes {
            inner = inner.with_missing_nodes(parse_missing_node_policy(&missing_nodes)?);
        }
        if let Some(mapping) = mapping {
            inner = inner.with_mapping(parse_mapping_policy(&mapping)?);
        }
        Ok(IonexSlantPolicy { inner })
    }

    /// Strict coverage, strict missing nodes, single-layer mapping.
    #[wasm_bindgen(js_name = defaultPolicy)]
    pub fn default_policy() -> IonexSlantPolicy {
        IonexSlantPolicy {
            inner: CoreIonexSlantPolicy::default(),
        }
    }

    /// The default policy with strict coverage; the other axes stay at default.
    #[wasm_bindgen(js_name = coverageStrict)]
    pub fn coverage_strict() -> IonexSlantPolicy {
        IonexSlantPolicy {
            inner: CoreIonexSlantPolicy::default().with_coverage(IonexCoveragePolicy::Strict),
        }
    }

    /// The default policy holding a query outside coverage at the nearest edge.
    #[wasm_bindgen(js_name = coverageHold)]
    pub fn coverage_hold() -> IonexSlantPolicy {
        IonexSlantPolicy {
            inner: CoreIonexSlantPolicy::default().with_coverage(IonexCoveragePolicy::Hold),
        }
    }

    /// The default policy refusing a query that weights a non-available node.
    #[wasm_bindgen(js_name = missingStrict)]
    pub fn missing_strict() -> IonexSlantPolicy {
        IonexSlantPolicy {
            inner: CoreIonexSlantPolicy::default()
                .with_missing_nodes(IonexMissingNodePolicy::Strict),
        }
    }

    /// The default policy interpolating around non-available nodes.
    #[wasm_bindgen(js_name = missingRenormalize)]
    pub fn missing_renormalize() -> IonexSlantPolicy {
        IonexSlantPolicy {
            inner: CoreIonexSlantPolicy::default()
                .with_missing_nodes(IonexMissingNodePolicy::Renormalize),
        }
    }

    /// The default policy applying the factor the product declares.
    #[wasm_bindgen(js_name = mappingDeclared)]
    pub fn mapping_declared() -> IonexSlantPolicy {
        IonexSlantPolicy {
            inner: CoreIonexSlantPolicy::default().with_mapping(IonexMappingPolicy::Declared),
        }
    }

    /// The default policy applying the single-layer `1/cos(z')`.
    #[wasm_bindgen(js_name = mappingSingleLayer)]
    pub fn mapping_single_layer() -> IonexSlantPolicy {
        IonexSlantPolicy {
            inner: CoreIonexSlantPolicy::default().with_mapping(IonexMappingPolicy::SingleLayer),
        }
    }

    /// This policy with another coverage axis; the other two are carried over.
    #[wasm_bindgen(js_name = withCoverage)]
    pub fn with_coverage(&self, coverage: &str) -> Result<IonexSlantPolicy, JsValue> {
        Ok(IonexSlantPolicy {
            inner: self.inner.with_coverage(parse_coverage_policy(coverage)?),
        })
    }

    /// This policy with strict coverage; the other two axes are carried over.
    #[wasm_bindgen(js_name = withCoverageStrict)]
    pub fn with_coverage_strict(&self) -> IonexSlantPolicy {
        IonexSlantPolicy {
            inner: self.inner.with_coverage(IonexCoveragePolicy::Strict),
        }
    }

    /// This policy holding outside coverage; the other two axes are carried over.
    #[wasm_bindgen(js_name = withCoverageHold)]
    pub fn with_coverage_hold(&self) -> IonexSlantPolicy {
        IonexSlantPolicy {
            inner: self.inner.with_coverage(IonexCoveragePolicy::Hold),
        }
    }

    /// This policy with another missing-nodes axis; the other two are carried over.
    #[wasm_bindgen(js_name = withMissingNodes)]
    pub fn with_missing_nodes(&self, missing_nodes: &str) -> Result<IonexSlantPolicy, JsValue> {
        Ok(IonexSlantPolicy {
            inner: self
                .inner
                .with_missing_nodes(parse_missing_node_policy(missing_nodes)?),
        })
    }

    /// This policy refusing non-available nodes; the other two axes are carried over.
    #[wasm_bindgen(js_name = withMissingStrict)]
    pub fn with_missing_strict(&self) -> IonexSlantPolicy {
        IonexSlantPolicy {
            inner: self
                .inner
                .with_missing_nodes(IonexMissingNodePolicy::Strict),
        }
    }

    /// This policy interpolating around non-available nodes; the other two axes
    /// are carried over.
    #[wasm_bindgen(js_name = withMissingRenormalize)]
    pub fn with_missing_renormalize(&self) -> IonexSlantPolicy {
        IonexSlantPolicy {
            inner: self
                .inner
                .with_missing_nodes(IonexMissingNodePolicy::Renormalize),
        }
    }

    /// This policy with another mapping axis; the other two are carried over.
    #[wasm_bindgen(js_name = withMapping)]
    pub fn with_mapping(&self, mapping: &str) -> Result<IonexSlantPolicy, JsValue> {
        Ok(IonexSlantPolicy {
            inner: self.inner.with_mapping(parse_mapping_policy(mapping)?),
        })
    }

    /// This policy applying the declared factor; the other two axes are carried over.
    #[wasm_bindgen(js_name = withMappingDeclared)]
    pub fn with_mapping_declared(&self) -> IonexSlantPolicy {
        IonexSlantPolicy {
            inner: self.inner.with_mapping(IonexMappingPolicy::Declared),
        }
    }

    /// This policy applying the single-layer factor; the other two axes are
    /// carried over.
    #[wasm_bindgen(js_name = withMappingSingleLayer)]
    pub fn with_mapping_single_layer(&self) -> IonexSlantPolicy {
        IonexSlantPolicy {
            inner: self.inner.with_mapping(IonexMappingPolicy::SingleLayer),
        }
    }

    /// The coverage axis.
    #[wasm_bindgen(getter, unchecked_return_type = "IonexCoveragePolicy")]
    pub fn coverage(&self) -> String {
        match self.inner.coverage {
            IonexCoveragePolicy::Strict => "strict".to_string(),
            IonexCoveragePolicy::Hold => "hold".to_string(),
        }
    }

    /// The missing-nodes axis.
    #[wasm_bindgen(
        getter,
        js_name = missingNodes,
        unchecked_return_type = "IonexMissingNodePolicy"
    )]
    pub fn missing_nodes(&self) -> String {
        match self.inner.missing_nodes {
            IonexMissingNodePolicy::Strict => "strict".to_string(),
            IonexMissingNodePolicy::Renormalize => "renormalize".to_string(),
        }
    }

    /// The mapping axis.
    #[wasm_bindgen(getter, unchecked_return_type = "IonexMappingPolicy")]
    pub fn mapping(&self) -> String {
        match self.inner.mapping {
            IonexMappingPolicy::Declared => "declared".to_string(),
            IonexMappingPolicy::SingleLayer => "singleLayer".to_string(),
        }
    }
}

// --- Status and refusal representations -------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IonexCoverageErrorJs {
    pub kind: String,
    pub message: String,
}

impl IonexCoverageErrorJs {
    pub fn from_core(coverage: IonexCoverageError) -> Self {
        let kind = match coverage {
            IonexCoverageError::EpochBeforeFirstMap => "EPOCH_BEFORE_FIRST_MAP",
            IonexCoverageError::EpochAfterLastMap => "EPOCH_AFTER_LAST_MAP",
            IonexCoverageError::LatitudeOutOfRange => "LATITUDE_OUT_OF_RANGE",
            IonexCoverageError::LongitudeOutOfRange => "LONGITUDE_OUT_OF_RANGE",
        };
        Self {
            kind: kind.to_string(),
            message: coverage.to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IonexMissingNodesJs {
    pub map_number: usize,
    pub lat_index: usize,
    pub lon_index: usize,
    pub lon_index_next: usize,
    pub missing: [bool; 4],
}

impl IonexMissingNodesJs {
    pub fn from_core(nodes: IonexMissingNodes) -> Self {
        Self {
            map_number: nodes.map_number,
            lat_index: nodes.lat_index,
            lon_index: nodes.lon_index,
            lon_index_next: nodes.lon_index_next,
            missing: nodes.missing,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IonexNodeGapJs {
    pub earlier: Option<IonexMissingNodesJs>,
    pub later: Option<IonexMissingNodesJs>,
}

impl IonexNodeGapJs {
    pub fn from_core(gap: IonexNodeGap) -> Self {
        Self {
            earlier: gap.earlier.map(IonexMissingNodesJs::from_core),
            later: gap.later.map(IonexMissingNodesJs::from_core),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IonexSlantDelayStatusJs {
    pub held: Option<IonexCoverageErrorJs>,
    pub degraded: Option<IonexNodeGapJs>,
    pub assumed_mapping: Option<IonexAssumedMappingJs>,
    pub is_valid: bool,
    pub is_nominal: bool,
    pub is_held: bool,
    pub is_degraded: bool,
    pub is_assumed_mapping: bool,
}

impl IonexSlantDelayStatusJs {
    pub fn from_core(status: IonexSlantDelayStatus) -> Self {
        Self {
            held: status.held.map(IonexCoverageErrorJs::from_core),
            degraded: status.degraded.map(IonexNodeGapJs::from_core),
            assumed_mapping: status.assumed_mapping.map(IonexAssumedMappingJs::from_core),
            // The engine's own rule: an assumed mapping is a nominal result and
            // leaves the value valid; only a held coverage miss or a degraded
            // interpolation does not.
            is_valid: status.is_valid(),
            is_nominal: status == IonexSlantDelayStatus::VALID,
            is_held: status.held.is_some(),
            is_degraded: status.degraded.is_some(),
            is_assumed_mapping: status.assumed_mapping.is_some(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IonexSlantDelayEvaluationJs {
    pub delay_m: f64,
    pub status: IonexSlantDelayStatusJs,
}

impl IonexSlantDelayEvaluationJs {
    pub fn from_core(evaluation: IonexSlantDelayEvaluation) -> Self {
        Self {
            delay_m: evaluation.delay_m,
            status: IonexSlantDelayStatusJs::from_core(evaluation.status),
        }
    }
}

/// Why a query gave no slant delay, as a discriminated union: each variant
/// carries only the fields that variant has.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind")]
pub enum IonexSlantRefusalJs {
    #[serde(rename = "COVERAGE", rename_all = "camelCase")]
    Coverage {
        coverage_error: IonexCoverageErrorJs,
        message: String,
    },
    #[serde(rename = "MISSING_NODES", rename_all = "camelCase")]
    MissingNodes {
        node_gap: IonexNodeGapJs,
        message: String,
    },
    #[serde(rename = "VARYING_HEIGHTS", rename_all = "camelCase")]
    VaryingHeights {
        map_number: usize,
        lat_index: usize,
        lon_index: usize,
        message: String,
    },
    #[serde(rename = "HEIGHT_NOT_AVAILABLE", rename_all = "camelCase")]
    HeightNotAvailable {
        map_number: usize,
        lat_index: usize,
        lon_index: usize,
        message: String,
    },
    #[serde(rename = "MAPPING_FUNCTION", rename_all = "camelCase")]
    MappingFunction {
        mapping_declaration: IonexMappingDeclarationJs,
        message: String,
    },
    #[serde(rename = "INVALID_INPUT", rename_all = "camelCase")]
    InvalidInput { message: String },
    /// A refusal this binding does not yet name, carrying the engine's own
    /// message in full rather than being folded into a known category.
    #[serde(rename = "UNKNOWN", rename_all = "camelCase")]
    Unknown { message: String },
}

impl IonexSlantRefusalJs {
    pub fn from_core_error(err: &sidereon_core::Error) -> Self {
        match err {
            sidereon_core::Error::IonexOutOfCoverage(coverage) => Self::Coverage {
                coverage_error: IonexCoverageErrorJs::from_core(*coverage),
                message: err.to_string(),
            },
            sidereon_core::Error::IonexNodesNotAvailable(gap) => Self::MissingNodes {
                node_gap: IonexNodeGapJs::from_core(**gap),
                message: err.to_string(),
            },
            sidereon_core::Error::IonexSlantUnavailable(refusal) => {
                let message = refusal.to_string();
                match refusal {
                    IonexSlantRefusal::VaryingHeights {
                        map_number,
                        lat_index,
                        lon_index,
                    } => Self::VaryingHeights {
                        map_number: *map_number,
                        lat_index: *lat_index,
                        lon_index: *lon_index,
                        message,
                    },
                    IonexSlantRefusal::HeightNotAvailable {
                        map_number,
                        lat_index,
                        lon_index,
                    } => Self::HeightNotAvailable {
                        map_number: *map_number,
                        lat_index: *lat_index,
                        lon_index: *lon_index,
                        message,
                    },
                    IonexSlantRefusal::MappingFunction(declaration) => Self::MappingFunction {
                        mapping_declaration: IonexMappingDeclarationJs::from_core(declaration),
                        message,
                    },
                    // `IonexSlantRefusal` is `#[non_exhaustive]`: a variant added
                    // later reaches here and keeps its full message.
                    _ => Self::Unknown { message },
                }
            }
            sidereon_core::Error::InvalidInput(_) => Self::InvalidInput {
                message: err.to_string(),
            },
            // `sidereon_core::Error` is `#[non_exhaustive]`.
            other => Self::Unknown {
                message: other.to_string(),
            },
        }
    }

    fn invalid_input(message: String) -> Self {
        Self::InvalidInput { message }
    }

    /// The exception class name a scalar query throws for this refusal.
    fn error_name(&self) -> &'static str {
        match self {
            Self::Coverage { .. } => "IonexCoverageError",
            Self::MissingNodes { .. } => "IonexMissingNodesError",
            Self::VaryingHeights { .. } => "IonexVaryingHeightsError",
            Self::HeightNotAvailable { .. } => "IonexHeightNotAvailableError",
            Self::MappingFunction { .. } => "IonexMappingFunctionError",
            Self::InvalidInput { .. } => "IonexInvalidInputError",
            Self::Unknown { .. } => "IonexSlantError",
        }
    }
}

fn ionex_core_error_to_js(err: sidereon_core::Error) -> JsValue {
    let refusal = IonexSlantRefusalJs::from_core_error(&err);
    error_with_detail(refusal.error_name(), &err.to_string(), &refusal)
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IonexSlantRequestJs {
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub azimuth_deg: f64,
    pub elevation_deg: f64,
    pub epoch_j2000_s: f64,
    pub frequency_hz: f64,
}

/// One row of a batch, carrying its own request index so a caller never has to
/// infer position from array length.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IonexSlantBatchResultJs {
    pub index: usize,
    pub is_ok: bool,
    pub evaluation: Option<IonexSlantDelayEvaluationJs>,
    pub refusal: Option<IonexSlantRefusalJs>,
}

impl IonexSlantBatchResultJs {
    fn ok(index: usize, evaluation: IonexSlantDelayEvaluationJs) -> Self {
        Self {
            index,
            is_ok: true,
            evaluation: Some(evaluation),
            refusal: None,
        }
    }

    fn refused(index: usize, refusal: IonexSlantRefusalJs) -> Self {
        Self {
            index,
            is_ok: false,
            evaluation: None,
            refusal: Some(refusal),
        }
    }
}

// --- Diagnostic epoch and warning representations ---------------------------

/// A diagnostic epoch at the precision the engine holds it.
///
/// A warning epoch is not forced through an integer constructor: the split
/// Julian date is carried as it stands, and an integer-second projection is
/// given both as an exact decimal string and as a `number` that is `null` where
/// the value does not fit a JavaScript `number` exactly.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IonexDiagnosticEpochJs {
    pub scale: String,
    pub jd_whole: Option<f64>,
    pub fraction: Option<f64>,
    pub jd: Option<f64>,
    /// Exact integer nanoseconds, as a decimal string, where the engine holds
    /// this epoch in that representation.
    pub nanos: Option<String>,
    /// Exact whole J2000 seconds as a decimal string, where this epoch falls on
    /// a whole second.
    pub j2000_seconds: Option<String>,
    /// The same value as a `number`, or `null` where it is not exactly
    /// representable as one.
    pub j2000_seconds_number: Option<f64>,
    /// The floating-point J2000-second projection, which a fractional epoch has
    /// and a whole-second one agrees with.
    pub j2000_seconds_f64: f64,
}

/// A `number` for an exact integer, or `None` where the integer would not
/// survive the conversion.
fn safe_integer_number(value: i128) -> Option<f64> {
    let candidate = value as f64;
    (JS_MIN_SAFE_INTEGER..=JS_MAX_SAFE_INTEGER)
        .contains(&candidate)
        .then_some(candidate)
}

impl IonexDiagnosticEpochJs {
    pub fn from_instant(epoch: Instant) -> Self {
        let scale = epoch.scale.abbrev().to_string();
        let (jd_whole, fraction, jd) = match epoch.julian_date() {
            Some(split) => (
                Some(split.jd_whole),
                Some(split.fraction),
                Some(split.to_jd()),
            ),
            None => (None, None, None),
        };
        let (nanos, whole_seconds, seconds_f64) = match epoch.repr {
            InstantRepr::JulianDate(split) => {
                let seconds_f64 = j2000_seconds_from_split(split.jd_whole, split.fraction);
                let whole = (seconds_f64.fract() == 0.0
                    && (JS_MIN_SAFE_INTEGER..=JS_MAX_SAFE_INTEGER).contains(&seconds_f64))
                .then_some(seconds_f64 as i128);
                (None, whole, seconds_f64)
            }
            InstantRepr::Nanos(nanos) => {
                let seconds_f64 = nanos as f64 / 1.0e9;
                let whole = (nanos % 1_000_000_000 == 0).then_some(nanos / 1_000_000_000);
                (Some(nanos.to_string()), whole, seconds_f64)
            }
        };

        Self {
            scale,
            jd_whole,
            fraction,
            jd,
            nanos,
            j2000_seconds: whole_seconds.map(|seconds| seconds.to_string()),
            j2000_seconds_number: whole_seconds.and_then(safe_integer_number),
            j2000_seconds_f64: seconds_f64,
        }
    }
}

/// A finding the reader reports without refusing the file, as a discriminated
/// union: each variant carries only its own payload, so no two variants give
/// one property two different types.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind")]
pub enum IonexWarningJs {
    #[serde(rename = "MISSING_RECORD", rename_all = "camelCase")]
    MissingRecord { label: String, message: String },
    #[serde(rename = "VERSION_RECORD_NOT_FIRST", rename_all = "camelCase")]
    VersionRecordNotFirst { line: usize, message: String },
    #[serde(rename = "EPOCH_MISMATCH", rename_all = "camelCase")]
    EpochMismatch {
        label: String,
        line: usize,
        declared_epoch: Box<IonexDiagnosticEpochJs>,
        maps_epoch: Box<IonexDiagnosticEpochJs>,
        message: String,
    },
    #[serde(rename = "MAP_COUNT_MISMATCH", rename_all = "camelCase")]
    MapCountMismatch {
        line: usize,
        /// The count the record gives, as an exact decimal string. The engine
        /// carries it as a `u64`, which a `number` holds exactly only below
        /// 2^53.
        declared_count: String,
        /// The same count as a `number`, or `null` where it is not exactly
        /// representable as one.
        declared_count_number: Option<f64>,
        tec_maps: usize,
        all_maps: usize,
        message: String,
    },
    #[serde(rename = "NOT_A_NUMBER_VALUE", rename_all = "camelCase")]
    NotANumberValue {
        data_kind: String,
        map_number: usize,
        line: usize,
        lat_deg: f64,
        lon_deg: f64,
        message: String,
    },
    #[serde(rename = "INTERVAL_MISMATCH", rename_all = "camelCase")]
    IntervalMismatch {
        line: usize,
        declared_s: u32,
        map_number: usize,
        /// The pair's spacing in seconds, as an exact decimal string. The engine
        /// carries it as an `i64`.
        spacing_s: String,
        /// The same spacing as a `number`, or `null` where it is not exactly
        /// representable as one.
        spacing_s_number: Option<f64>,
        message: String,
    },
    #[serde(rename = "EXPONENT_CARRIED_INTO_MAP", rename_all = "camelCase")]
    ExponentCarriedIntoMap {
        data_kind: String,
        map_number: usize,
        line: usize,
        exponent: i32,
        set_by_line: usize,
        message: String,
    },
    /// A warning this binding does not yet name, carrying the engine's own
    /// message in full.
    #[serde(rename = "UNKNOWN", rename_all = "camelCase")]
    Unknown { message: String },
}

impl IonexWarningJs {
    pub fn from_core(warning: CoreIonexWarning) -> Self {
        let message = warning.to_string();
        match warning {
            CoreIonexWarning::MissingRecord(label) => Self::MissingRecord {
                label: label.to_string(),
                message,
            },
            CoreIonexWarning::VersionRecordNotFirst { line } => {
                Self::VersionRecordNotFirst { line, message }
            }
            CoreIonexWarning::EpochMismatch {
                label,
                line,
                declared,
                maps,
            } => Self::EpochMismatch {
                label: label.to_string(),
                line,
                declared_epoch: Box::new(IonexDiagnosticEpochJs::from_instant(declared)),
                maps_epoch: Box::new(IonexDiagnosticEpochJs::from_instant(maps)),
                message,
            },
            CoreIonexWarning::MapCountMismatch {
                line,
                declared,
                tec_maps,
                all_maps,
            } => Self::MapCountMismatch {
                line,
                declared_count: declared.to_string(),
                declared_count_number: safe_integer_number(i128::from(declared)),
                tec_maps,
                all_maps,
                message,
            },
            CoreIonexWarning::NotANumberValue {
                kind,
                map_number,
                line,
                lat_deg,
                lon_deg,
            } => Self::NotANumberValue {
                data_kind: kind.to_string(),
                map_number,
                line,
                lat_deg,
                lon_deg,
                message,
            },
            CoreIonexWarning::IntervalMismatch {
                line,
                declared_s,
                map_number,
                spacing_s,
            } => Self::IntervalMismatch {
                line,
                declared_s,
                map_number,
                spacing_s: spacing_s.to_string(),
                spacing_s_number: safe_integer_number(i128::from(spacing_s)),
                message,
            },
            CoreIonexWarning::ExponentCarriedIntoMap {
                kind,
                map_number,
                line,
                exponent,
                set_by_line,
            } => Self::ExponentCarriedIntoMap {
                data_kind: kind.to_string(),
                map_number,
                line,
                exponent,
                set_by_line,
                message,
            },
            // `IonexWarning` is `#[non_exhaustive]`.
            _ => Self::Unknown { message },
        }
    }
}

fn make_parse_result(
    inner: CoreIonex,
    warnings: Vec<CoreIonexWarning>,
) -> Result<JsValue, JsValue> {
    let ionex_value: JsValue = Ionex { inner }.into();
    let warnings_js: Vec<IonexWarningJs> = warnings
        .into_iter()
        .map(IonexWarningJs::from_core)
        .collect();
    let warnings_value = to_js(&warnings_js)?;
    let result = js_sys::Object::new();
    for (key, value) in [
        ("ionex", &ionex_value),
        ("value", &ionex_value),
        ("warnings", &warnings_value),
    ] {
        match js_sys::Reflect::set(&result, &JsValue::from_str(key), value) {
            Ok(true) => {}
            Ok(false) => {
                return Err(engine_error(format!(
                    "failed to set '{key}' on the IONEX parse result"
                )))
            }
            Err(err) => {
                return Err(engine_error(format!(
                    "setting '{key}' on the IONEX parse result threw: {}",
                    describe_js(&err)
                )))
            }
        }
    }
    Ok(result.into())
}

// --- Main Ionex WASM handle -------------------------------------------------

/// A parsed IONEX vertical-TEC product. Create with `loadIonex` or `Ionex.parse`.
#[wasm_bindgen]
pub struct Ionex {
    pub(crate) inner: CoreIonex,
}

/// Validate the degree-valued ray geometry a slant-delay query carries.
///
/// Shared by the scalar entries and the batch so every path applies one rule.
/// Returns the receiver and the exact integer query second.
fn slant_query_inputs(
    lat_deg: f64,
    lon_deg: f64,
    azimuth_deg: f64,
    elevation_deg: f64,
    epoch_j2000_s: f64,
    frequency_hz: f64,
) -> Result<(Wgs84Geodetic, Instant), String> {
    for (value, field) in [
        (lat_deg, "latDeg"),
        (lon_deg, "lonDeg"),
        (azimuth_deg, "azimuthDeg"),
        (elevation_deg, "elevationDeg"),
        (frequency_hz, "frequencyHz"),
    ] {
        if !value.is_finite() {
            return Err(format!("{field} must be a finite number"));
        }
    }
    if frequency_hz <= 0.0 {
        return Err("frequencyHz must be positive".to_string());
    }
    let epoch = j2000_seconds_to_instant(epoch_j2000_s).map_err(|error| describe_js(&error))?;
    let receiver = Wgs84Geodetic::new(lat_deg * DEG_TO_RAD, lon_deg * DEG_TO_RAD, 0.0)
        .map_err(|err| format!("invalid receiver coordinates: {err}"))?;
    Ok((receiver, epoch))
}

/// IONEX slant ionospheric group delay from degree-valued geometry, shared by
/// `Ionex.slantDelay` and the staleness-selected `IonexSelection.slantDelay`
/// so a selected product evaluates bit-for-bit identically to the product the
/// caller parsed. Delegates to `ionex_slant_delay_with_policy`.
pub(crate) fn slant_delay_deg(
    inner: &CoreIonex,
    lat_deg: f64,
    lon_deg: f64,
    azimuth_deg: f64,
    elevation_deg: f64,
    epoch_j2000_s: f64,
    frequency_hz: f64,
) -> Result<f64, JsValue> {
    require_finite(lat_deg, "latDeg")?;
    require_finite(lon_deg, "lonDeg")?;
    require_finite(azimuth_deg, "azimuthDeg")?;
    require_finite(elevation_deg, "elevationDeg")?;
    require_finite(epoch_j2000_s, "epochJ2000S")?;
    require_finite(frequency_hz, "frequencyHz")?;
    let (receiver, epoch) = slant_query_inputs(
        lat_deg,
        lon_deg,
        azimuth_deg,
        elevation_deg,
        epoch_j2000_s,
        frequency_hz,
    )
    .map_err(|message| range_error(&message))?;

    core_ionex_slant_delay_with_policy(
        inner,
        receiver,
        elevation_deg * DEG_TO_RAD,
        azimuth_deg * DEG_TO_RAD,
        epoch,
        frequency_hz,
        CoreIonexSlantPolicy::default(),
    )
    .map(|evaluation| evaluation.delay_m)
    .map_err(ionex_core_error_to_js)
}

#[wasm_bindgen]
impl Ionex {
    /// Parse an IONEX vertical-TEC product from bytes.
    #[wasm_bindgen(js_name = parse)]
    pub fn parse(bytes: &[u8]) -> Result<Ionex, JsValue> {
        let inner = CoreIonex::parse(bytes).map_err(engine_error)?;
        Ok(Ionex { inner })
    }

    /// Parse an IONEX vertical-TEC product from UTF-8 text.
    #[wasm_bindgen(js_name = parseStr)]
    pub fn parse_str(text: &str) -> Result<Ionex, JsValue> {
        let inner = CoreIonex::parse_str(text).map_err(engine_error)?;
        Ok(Ionex { inner })
    }

    /// Parse from bytes, returning the product and the reader's ordered warnings.
    #[wasm_bindgen(js_name = parseWithWarnings, unchecked_return_type = "IonexParseResult")]
    pub fn parse_with_warnings(bytes: &[u8]) -> Result<JsValue, JsValue> {
        let (inner, warnings) = CoreIonex::parse_with_warnings(bytes).map_err(engine_error)?;
        make_parse_result(inner, warnings)
    }

    /// Parse from text, returning the product and the reader's ordered warnings.
    #[wasm_bindgen(js_name = parseStrWithWarnings, unchecked_return_type = "IonexParseResult")]
    pub fn parse_str_with_warnings(text: &str) -> Result<JsValue, JsValue> {
        let (inner, warnings) = CoreIonex::parse_str_with_warnings(text).map_err(engine_error)?;
        make_parse_result(inner, warnings)
    }

    /// Descriptive header records this product carries.
    #[wasm_bindgen(getter, unchecked_return_type = "IonexHeader")]
    pub fn header(&self) -> Result<JsValue, JsValue> {
        to_js(&header_to_js(self.inner.header()))
    }

    /// Latitude node values in degrees, in the order the file writes them.
    #[wasm_bindgen(getter, js_name = latNodesDeg)]
    pub fn lat_nodes_deg(&self) -> Vec<f64> {
        self.inner.lat_nodes_deg().to_vec()
    }

    /// Longitude node values in degrees, in the order the file writes them.
    #[wasm_bindgen(getter, js_name = lonNodesDeg)]
    pub fn lon_nodes_deg(&self) -> Vec<f64> {
        self.inner.lon_nodes_deg().to_vec()
    }

    /// Single-layer shell height, kilometres.
    #[wasm_bindgen(getter, js_name = shellHeightKm)]
    pub fn shell_height_km(&self) -> f64 {
        self.inner.shell_height_km()
    }

    /// Mean Earth radius the pierce-point geometry uses, kilometres.
    #[wasm_bindgen(getter, js_name = baseRadiusKm)]
    pub fn base_radius_km(&self) -> f64 {
        self.inner.base_radius_km()
    }

    /// The `EXPONENT` header field; the TEC scale is `10^exponent`.
    #[wasm_bindgen(getter)]
    pub fn exponent(&self) -> i32 {
        self.inner.exponent()
    }

    /// Signed latitude grid step, degrees. Most files run north to south, so
    /// this is usually negative.
    #[wasm_bindgen(getter, js_name = dlatDeg)]
    pub fn dlat_deg(&self) -> f64 {
        self.inner.dlat_deg()
    }

    /// Signed longitude grid step, degrees.
    #[wasm_bindgen(getter, js_name = dlonDeg)]
    pub fn dlon_deg(&self) -> f64 {
        self.inner.dlon_deg()
    }

    /// Map epochs as whole seconds since J2000, ascending.
    #[wasm_bindgen(getter, js_name = mapEpochsJ2000S)]
    pub fn map_epochs_j2000_s(&self) -> Vec<f64> {
        self.inner
            .map_epochs_s()
            .into_iter()
            .map(|seconds| seconds as f64)
            .collect()
    }

    /// Per-map vertical-TEC grids, indexed `[map][iLat][iLon]` (TECU).
    ///
    /// A node the product gives as non-available is `null`, never `0`.
    #[wasm_bindgen(getter, js_name = tecMaps, unchecked_return_type = "IonexMapCube")]
    pub fn tec_maps(&self) -> Result<JsValue, JsValue> {
        to_js(&self.inner.tec_maps())
    }

    /// Per-map RMS grids, indexed `[map][iLat][iLon]` (TECU), or `null` where
    /// the product carries none.
    #[wasm_bindgen(getter, js_name = rmsMaps, unchecked_return_type = "IonexMapCube | null")]
    pub fn rms_maps(&self) -> Result<JsValue, JsValue> {
        if self.inner.rms_maps().is_empty() {
            Ok(JsValue::NULL)
        } else {
            to_js(&self.inner.rms_maps())
        }
    }

    /// Per-map single-layer height grids, indexed `[map][iLat][iLon]` (km), or
    /// `null` where the product carries none.
    #[wasm_bindgen(getter, js_name = heightMaps, unchecked_return_type = "IonexMapCube | null")]
    pub fn height_maps(&self) -> Result<JsValue, JsValue> {
        if self.inner.height_maps().is_empty() {
            Ok(JsValue::NULL)
        } else {
            to_js(&self.inner.height_maps())
        }
    }

    /// Whether the product carries RMS maps.
    #[wasm_bindgen(getter, js_name = hasRms)]
    pub fn has_rms(&self) -> bool {
        !self.inner.rms_maps().is_empty()
    }

    /// Whether the product carries height maps.
    #[wasm_bindgen(getter, js_name = hasHeight)]
    pub fn has_height(&self) -> bool {
        !self.inner.height_maps().is_empty()
    }

    /// Records skipped during a forgiving parse.
    #[wasm_bindgen(getter, js_name = skippedRecords)]
    pub fn skipped_records(&self) -> usize {
        self.inner.skipped_records()
    }

    /// The `MAPPING FUNCTION` the header declares, or `null` where it gives none.
    #[wasm_bindgen(
        getter,
        js_name = mappingFunction,
        unchecked_return_type = "IonexMappingFunction | null"
    )]
    pub fn mapping_function(&self) -> Result<JsValue, JsValue> {
        match &self.inner.header().mapping_function {
            Some(function) => to_js(&IonexMappingFunctionJs::from_core(function)),
            None => Ok(JsValue::NULL),
        }
    }

    /// What the header says about its mapping function, declared or absent.
    #[wasm_bindgen(
        getter,
        js_name = mappingDeclaration,
        unchecked_return_type = "IonexMappingDeclaration"
    )]
    pub fn mapping_declaration(&self) -> Result<JsValue, JsValue> {
        let declaration = match &self.inner.header().mapping_function {
            Some(function) => IonexMappingDeclarationJs::Declared {
                function: IonexMappingFunctionJs::from_core(function),
            },
            None => IonexMappingDeclarationJs::Absent,
        };
        to_js(&declaration)
    }

    /// IONEX slant ionospheric group delay, positive metres.
    ///
    /// Evaluates under the engine default policy (strict coverage, strict
    /// missing nodes, single-layer mapping). Throws a `RangeError` for input
    /// outside its domain and a typed error carrying `.detail` for a refusal.
    #[wasm_bindgen(js_name = slantDelay)]
    #[allow(clippy::too_many_arguments)]
    pub fn slant_delay(
        &self,
        lat_deg: f64,
        lon_deg: f64,
        azimuth_deg: f64,
        elevation_deg: f64,
        epoch_j2000_s: f64,
        frequency_hz: f64,
    ) -> Result<f64, JsValue> {
        slant_delay_deg(
            &self.inner,
            lat_deg,
            lon_deg,
            azimuth_deg,
            elevation_deg,
            epoch_j2000_s,
            frequency_hz,
        )
    }

    /// Policy-aware slant-delay query returning the delay with its full status.
    #[wasm_bindgen(
        js_name = slantDelayWithPolicy,
        unchecked_return_type = "IonexSlantDelayEvaluation"
    )]
    #[allow(clippy::too_many_arguments)]
    pub fn slant_delay_with_policy(
        &self,
        lat_deg: f64,
        lon_deg: f64,
        azimuth_deg: f64,
        elevation_deg: f64,
        epoch_j2000_s: f64,
        frequency_hz: f64,
        #[wasm_bindgen(unchecked_optional_param_type = "IonexSlantPolicyLike")] policy: JsValue,
    ) -> Result<JsValue, JsValue> {
        let (receiver, epoch) = slant_query_inputs(
            lat_deg,
            lon_deg,
            azimuth_deg,
            elevation_deg,
            epoch_j2000_s,
            frequency_hz,
        )
        .map_err(|message| range_error(&message))?;
        let policy = parse_slant_policy(policy)?;

        let evaluation = core_ionex_slant_delay_with_policy(
            &self.inner,
            receiver,
            elevation_deg * DEG_TO_RAD,
            azimuth_deg * DEG_TO_RAD,
            epoch,
            frequency_hz,
            policy,
        )
        .map_err(ionex_core_error_to_js)?;

        to_js(&IonexSlantDelayEvaluationJs::from_core(evaluation))
    }

    /// Evaluate a batch of queries, one typed row per request in request order.
    ///
    /// A malformed row is refused in its own row and the rows after it are still
    /// evaluated; a request container that is not an array is refused outright.
    #[wasm_bindgen(
        js_name = slantDelaysBatchResults,
        unchecked_return_type = "IonexSlantBatchResult[]"
    )]
    pub fn slant_delays_batch_results(
        &self,
        #[wasm_bindgen(unchecked_param_type = "IonexSlantRequest[]")] requests: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "IonexSlantPolicyLike")] policy: JsValue,
    ) -> Result<JsValue, JsValue> {
        if !js_sys::Array::is_array(&requests) {
            return Err(type_error(
                "IONEX slant requests must be an array of request objects",
            ));
        }
        let requests = requests.unchecked_into::<js_sys::Array>();
        let policy = parse_slant_policy(policy)?;

        let count = requests.length() as usize;
        let mut rows: Vec<Option<IonexSlantBatchResultJs>> = vec![None; count];
        let mut valid_requests = Vec::with_capacity(count);
        let mut valid_indices = Vec::with_capacity(count);

        for (index, slot) in rows.iter_mut().enumerate() {
            let row = requests.get(index as u32);
            let request: IonexSlantRequestJs = match serde_wasm_bindgen::from_value(row) {
                Ok(request) => request,
                Err(err) => {
                    *slot = Some(IonexSlantBatchResultJs::refused(
                        index,
                        IonexSlantRefusalJs::invalid_input(format!(
                            "invalid IONEX slant request: {err}"
                        )),
                    ));
                    continue;
                }
            };

            match slant_query_inputs(
                request.lat_deg,
                request.lon_deg,
                request.azimuth_deg,
                request.elevation_deg,
                request.epoch_j2000_s,
                request.frequency_hz,
            ) {
                Ok((receiver, epoch)) => {
                    valid_requests.push(CoreIonexSlantRequest::new(
                        receiver,
                        request.elevation_deg * DEG_TO_RAD,
                        request.azimuth_deg * DEG_TO_RAD,
                        epoch,
                        request.frequency_hz,
                    ));
                    valid_indices.push(index);
                }
                Err(message) => {
                    *slot = Some(IonexSlantBatchResultJs::refused(
                        index,
                        IonexSlantRefusalJs::invalid_input(message),
                    ));
                }
            }
        }

        if !valid_requests.is_empty() {
            let evaluated = core_ionex_slant_delay_results(&self.inner, &valid_requests, policy);
            for (index, result) in valid_indices.into_iter().zip(evaluated) {
                rows[index] = Some(match result {
                    Ok(evaluation) => IonexSlantBatchResultJs::ok(
                        index,
                        IonexSlantDelayEvaluationJs::from_core(evaluation),
                    ),
                    Err(err) => IonexSlantBatchResultJs::refused(
                        index,
                        IonexSlantRefusalJs::from_core_error(&err),
                    ),
                });
            }
        }

        // Every index was written above; a hole would be a binding defect, and
        // it is reported rather than dropped by a filtering collect.
        let results = rows
            .into_iter()
            .enumerate()
            .map(|(index, row)| {
                row.ok_or_else(|| {
                    engine_error(format!("IONEX batch row {index} was not evaluated"))
                })
            })
            .collect::<Result<Vec<_>, JsValue>>()?;
        to_js(&results)
    }

    /// Write this product back as standard IONEX text.
    ///
    /// Throws an `IonexWriterError` carrying `.detail` of
    /// `{ kind: "UNWRITABLE", message }` where a value or a code cannot be put
    /// on a standard record as it stands.
    #[wasm_bindgen(js_name = toIonexString)]
    pub fn to_ionex_string(&self) -> Result<String, JsValue> {
        self.inner.to_ionex_string().map_err(|err| {
            let message = err.to_string();
            let detail = IonexWriterDetailJs {
                kind: "UNWRITABLE",
                message: message.clone(),
            };
            error_with_detail("IonexWriterError", &message, &detail)
        })
    }

    /// Extract the full vertical-TEC grids as plain sample data.
    ///
    /// A non-available node is `null`. An optional map cube the product does not
    /// carry is `null`, which stays distinct from a cube present with every node
    /// `null`.
    #[wasm_bindgen(js_name = tecGridSamples, unchecked_return_type = "TecGridSamples")]
    pub fn tec_grid_samples(&self) -> Result<JsValue, JsValue> {
        to_js(&grid_samples_from_core(self.inner.tec_grid_samples()))
    }

    /// Extract one sample per grid node, every record retained.
    #[wasm_bindgen(js_name = tecSamples, unchecked_return_type = "TecSample[]")]
    pub fn tec_samples(&self) -> Result<JsValue, JsValue> {
        let samples: Vec<TecSampleJs> = self
            .inner
            .tec_samples()
            .into_iter()
            .map(node_sample_from_core)
            .collect();
        to_js(&samples)
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
struct IonexWriterDetailJs {
    kind: &'static str,
    message: String,
}

// --- Top-level IONEX conveniences -------------------------------------------

/// Parse an IONEX vertical-TEC product from bytes.
#[wasm_bindgen(js_name = loadIonex)]
pub fn load_ionex(bytes: &[u8]) -> Result<Ionex, JsValue> {
    Ionex::parse(bytes)
}

/// Parse from bytes, returning the product and the reader's ordered warnings.
#[wasm_bindgen(js_name = loadIonexWithWarnings, unchecked_return_type = "IonexParseResult")]
pub fn load_ionex_with_warnings(bytes: &[u8]) -> Result<JsValue, JsValue> {
    Ionex::parse_with_warnings(bytes)
}

/// Build an IONEX vertical-TEC product from whole-grid samples.
#[wasm_bindgen(js_name = ionexFromSamples)]
pub fn ionex_from_samples(
    #[wasm_bindgen(unchecked_param_type = "TecGridSamplesInput")] samples: JsValue,
) -> Result<Ionex, JsValue> {
    if !samples.is_object() {
        return Err(type_error("IONEX TEC grid samples must be an object"));
    }
    reject_unknown_keys(&samples, "IONEX TEC grid samples", TEC_GRID_SAMPLES_KEYS)?;
    // A throwing `header` accessor re-raises the value it threw, unchanged: the
    // caller's own error class, its properties and its identity all survive.
    // Rebuilding it as a `TypeError` carrying its text would leave a caller
    // matching on `instanceof` or on a field with nothing to match.
    let header_value = js_sys::Reflect::get(&samples, &JsValue::from_str("header"))?;
    let header = if header_value.is_null() || header_value.is_undefined() {
        unstated_header()
    } else {
        header_from_js(&header_value)?
    };
    let parsed: TecGridSamplesInputJs = serde_wasm_bindgen::from_value(samples)
        .map_err(|err| type_error(&format!("invalid IONEX TEC grid samples: {err}")))?;
    let inner = CoreIonex::from_samples(grid_samples_to_core(parsed, header)?)
        .map_err(tec_samples_error)?;
    Ok(Ionex { inner })
}

/// Build an IONEX vertical-TEC product from a flat stream of node samples.
///
/// An omitted `header` reads as the header a file carrying none of the
/// descriptive records reads as: no mapping function is invented for it.
#[wasm_bindgen(js_name = ionexFromNodeSamples)]
pub fn ionex_from_node_samples(
    #[wasm_bindgen(unchecked_param_type = "TecSample[]")] samples: JsValue,
    shell_height_km: f64,
    base_radius_km: f64,
    exponent: i32,
    #[wasm_bindgen(unchecked_optional_param_type = "IonexHeaderInput | null")] header: Option<
        JsValue,
    >,
) -> Result<Ionex, JsValue> {
    // Each row is checked and converted on its own, so an unknown property can
    // name the sample it is on. `rmsTecu` and `heightOffsetKm` are optional, and
    // a typo of either would otherwise read as a node carrying neither.
    if !js_sys::Array::is_array(&samples) {
        return Err(type_error(
            "IONEX TEC node samples must be an array of sample objects",
        ));
    }
    let samples = samples.unchecked_into::<js_sys::Array>();
    // The length is caller-controlled and need not describe any real rows:
    // `new Array(100_000_000)` states a hundred million and holds nothing. A
    // reservation made from it would ask the allocator for gigabytes before the
    // first row is looked at, and a wasm32 allocation failure aborts rather than
    // throwing something a caller can catch. Growing on demand keeps the first
    // malformed row's `TypeError` reachable, and imposes no ceiling on a long
    // array that really does carry that many samples.
    let mut core_samples = Vec::new();
    for index in 0..samples.length() {
        let row = samples.get(index);
        if !row.is_object() {
            return Err(type_error(&format!(
                "IONEX TEC node sample {index} must be an object"
            )));
        }
        reject_unknown_keys(
            &row,
            &format!("IONEX TEC node sample {index}"),
            TEC_SAMPLE_KEYS,
        )?;
        let sample: TecSampleJs = serde_wasm_bindgen::from_value(row)
            .map_err(|err| type_error(&format!("invalid IONEX TEC node sample {index}: {err}")))?;
        core_samples.push(node_sample_to_core(sample)?);
    }
    let header = match header {
        Some(value) if !value.is_null() && !value.is_undefined() => header_from_js(&value)?,
        _ => unstated_header(),
    };
    let inner = CoreIonex::from_node_samples(
        core_samples,
        shell_height_km,
        base_radius_km,
        exponent,
        header,
    )
    .map_err(tec_samples_error)?;
    Ok(Ionex { inner })
}

/// Scalar IONEX slant-delay convenience.
#[wasm_bindgen(js_name = ionexSlantDelay)]
#[allow(clippy::too_many_arguments)]
pub fn ionex_slant_delay(
    ionex: &Ionex,
    lat_deg: f64,
    lon_deg: f64,
    azimuth_deg: f64,
    elevation_deg: f64,
    epoch_j2000_s: f64,
    frequency_hz: f64,
) -> Result<f64, JsValue> {
    ionex.slant_delay(
        lat_deg,
        lon_deg,
        azimuth_deg,
        elevation_deg,
        epoch_j2000_s,
        frequency_hz,
    )
}

/// Policy-aware IONEX slant-delay convenience.
#[wasm_bindgen(
    js_name = ionexSlantDelayWithPolicy,
    unchecked_return_type = "IonexSlantDelayEvaluation"
)]
#[allow(clippy::too_many_arguments)]
pub fn ionex_slant_delay_with_policy(
    ionex: &Ionex,
    lat_deg: f64,
    lon_deg: f64,
    azimuth_deg: f64,
    elevation_deg: f64,
    epoch_j2000_s: f64,
    frequency_hz: f64,
    #[wasm_bindgen(unchecked_optional_param_type = "IonexSlantPolicyLike")] policy: JsValue,
) -> Result<JsValue, JsValue> {
    ionex.slant_delay_with_policy(
        lat_deg,
        lon_deg,
        azimuth_deg,
        elevation_deg,
        epoch_j2000_s,
        frequency_hz,
        policy,
    )
}

/// Batch IONEX slant-delay convenience, one typed row per request.
#[wasm_bindgen(
    js_name = ionexSlantDelayResults,
    unchecked_return_type = "IonexSlantBatchResult[]"
)]
pub fn ionex_slant_delay_results(
    ionex: &Ionex,
    #[wasm_bindgen(unchecked_param_type = "IonexSlantRequest[]")] requests: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "IonexSlantPolicyLike")] policy: JsValue,
) -> Result<JsValue, JsValue> {
    ionex.slant_delays_batch_results(requests, policy)
}

// --- Standalone TecGrid, shell geometry, and XYZ evaluators ------------------

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TecGridEvaluationJs<T> {
    pub value: T,
    pub degraded: Option<IonexNodeGapJs>,
}

/// A regular-grid failure as a discriminated union.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind")]
pub enum TecGridErrorJs {
    #[serde(rename = "AXES_TOO_SHORT", rename_all = "camelCase")]
    AxesTooShort { message: String },
    #[serde(rename = "AXES_NOT_INCREASING", rename_all = "camelCase")]
    AxesNotIncreasing { message: String },
    #[serde(rename = "DIMENSIONS_OVERFLOW", rename_all = "camelCase")]
    DimensionsOverflow { message: String },
    #[serde(rename = "VALUE_COUNT_MISMATCH", rename_all = "camelCase")]
    ValueCountMismatch {
        actual: usize,
        expected: usize,
        message: String,
    },
    #[serde(rename = "INVALID_FIELD", rename_all = "camelCase")]
    InvalidField {
        field: String,
        reason: String,
        message: String,
    },
    #[serde(rename = "NODES_NOT_AVAILABLE", rename_all = "camelCase")]
    NodesNotAvailable {
        node_gap: IonexNodeGapJs,
        message: String,
    },
    #[serde(rename = "OUT_OF_BOUNDS", rename_all = "camelCase")]
    OutOfBounds {
        name: String,
        value: f64,
        message: String,
    },
    /// A failure this binding does not yet name, carrying the engine's own
    /// message in full.
    #[serde(rename = "UNKNOWN", rename_all = "camelCase")]
    Unknown { message: String },
}

impl TecGridErrorJs {
    fn from_core(err: &CoreTecGridError) -> Self {
        let message = err.to_string();
        match err {
            CoreTecGridError::AxesTooShort => Self::AxesTooShort { message },
            CoreTecGridError::AxesNotIncreasing => Self::AxesNotIncreasing { message },
            CoreTecGridError::DimensionsOverflow => Self::DimensionsOverflow { message },
            CoreTecGridError::ValueCountMismatch { actual, expected } => Self::ValueCountMismatch {
                actual: *actual,
                expected: *expected,
                message,
            },
            CoreTecGridError::InvalidField { field, reason } => Self::InvalidField {
                field: (*field).to_string(),
                reason: (*reason).to_string(),
                message,
            },
            CoreTecGridError::NodesNotAvailable(gap) => Self::NodesNotAvailable {
                node_gap: IonexNodeGapJs::from_core(*gap),
                message,
            },
            CoreTecGridError::OutOfBounds { name, value } => Self::OutOfBounds {
                name: (*name).to_string(),
                value: *value,
                message,
            },
            // `TecGridError` is `#[non_exhaustive]`.
            _ => Self::Unknown { message },
        }
    }
}

fn tec_grid_error_to_js(err: CoreTecGridError) -> JsValue {
    let detail = TecGridErrorJs::from_core(&err);
    error_with_detail("TecGridError", &err.to_string(), &detail)
}

/// A regular-grid vertical-TEC source.
///
/// The coordinate axes are `f64`, which is the engine's own contract for this
/// grid: an epoch coordinate is `f64` Unix nanoseconds, so adjacent nanoseconds
/// are not distinguishable at present-day magnitudes. `TecGridEpoch` is a
/// separate, exact `i64` nanosecond value, and a query converts it to the axis
/// coordinate on the way in.
#[wasm_bindgen]
pub struct TecGrid {
    inner: CoreTecGrid,
}

#[wasm_bindgen]
impl TecGrid {
    /// Build a grid from its three strictly increasing axes and flat values in
    /// epoch-latitude-longitude order, `null` marking a node without a value.
    #[wasm_bindgen(constructor)]
    pub fn new(
        #[wasm_bindgen(unchecked_param_type = "number[] | Float64Array")] epochs_ns: Vec<f64>,
        #[wasm_bindgen(unchecked_param_type = "number[] | Float64Array")] latitudes_deg: Vec<f64>,
        #[wasm_bindgen(unchecked_param_type = "number[] | Float64Array")] longitudes_deg: Vec<f64>,
        #[wasm_bindgen(unchecked_param_type = "(number | null)[]")] values: JsValue,
    ) -> Result<TecGrid, JsValue> {
        let values: Vec<Option<f64>> = serde_wasm_bindgen::from_value(values)
            .map_err(|err| type_error(&format!("invalid TEC grid values: {err}")))?;
        let inner = CoreTecGrid::new(epochs_ns, latitudes_deg, longitudes_deg, values)
            .map_err(tec_grid_error_to_js)?;
        Ok(TecGrid { inner })
    }

    /// The epoch axis, `f64` Unix nanoseconds.
    #[wasm_bindgen(getter, js_name = epochsNs)]
    pub fn epochs_ns(&self) -> Vec<f64> {
        self.inner.epochs_ns().to_vec()
    }

    /// The latitude axis, degrees.
    #[wasm_bindgen(getter, js_name = latitudesDeg)]
    pub fn latitudes_deg(&self) -> Vec<f64> {
        self.inner.latitudes_deg().to_vec()
    }

    /// The longitude axis, degrees.
    #[wasm_bindgen(getter, js_name = longitudesDeg)]
    pub fn longitudes_deg(&self) -> Vec<f64> {
        self.inner.longitudes_deg().to_vec()
    }

    /// The flat TECU values in epoch-latitude-longitude order, longitude
    /// varying fastest; `null` marks a node without a value and `0` is a value.
    #[wasm_bindgen(getter, unchecked_return_type = "(number | null)[]")]
    pub fn values(&self) -> Result<JsValue, JsValue> {
        to_js(&self.inner.values())
    }

    /// VTEC at a pierce point, refusing a query that weights a node holding no
    /// value.
    #[wasm_bindgen(js_name = vtecAtPiercePoint)]
    pub fn vtec_at_pierce_point(
        &self,
        epoch: &TecGridEpoch,
        longitude_deg: f64,
        latitude_deg: f64,
    ) -> Result<f64, JsValue> {
        self.inner
            .vtec_at_pierce_point(epoch.inner, longitude_deg, latitude_deg)
            .map_err(tec_grid_error_to_js)
    }

    /// VTEC at a pierce point with an explicit missing-node policy.
    #[wasm_bindgen(
        js_name = vtecAtPiercePointWithPolicy,
        unchecked_return_type = "TecGridEvaluation<number>"
    )]
    pub fn vtec_at_pierce_point_with_policy(
        &self,
        epoch: &TecGridEpoch,
        longitude_deg: f64,
        latitude_deg: f64,
        #[wasm_bindgen(unchecked_optional_param_type = "IonexMissingNodePolicy | null")]
        policy: Option<String>,
    ) -> Result<JsValue, JsValue> {
        let policy = match policy.as_deref() {
            Some(policy) => parse_missing_node_policy(policy)?,
            None => IonexMissingNodePolicy::Strict,
        };
        let evaluation = self
            .inner
            .vtec_at_pierce_point_with_policy(epoch.inner, longitude_deg, latitude_deg, policy)
            .map_err(tec_grid_error_to_js)?;
        to_js(&TecGridEvaluationJs {
            value: evaluation.value,
            degraded: evaluation.degraded.map(IonexNodeGapJs::from_core),
        })
    }
}

/// Read an exact `i64` nanosecond count from a `bigint`, a decimal string, or a
/// `number` inside the safe-integer range.
///
/// A `number` cannot carry a present-day nanosecond timestamp exactly: 2026 is
/// about 1.8e18 ns, two hundred times past `Number.MAX_SAFE_INTEGER`. A
/// `bigint` is read through its own decimal text, so nothing is narrowed on the
/// way in and a value outside `i64` is refused rather than wrapped.
fn exact_i64_nanos(value: &JsValue, field: &str) -> Result<i64, JsValue> {
    let text = if value.is_bigint() {
        let big = value.clone().unchecked_into::<js_sys::BigInt>();
        let string = big.to_string(10).map_err(|err| {
            range_error(&format!("{field}: {}", describe_js(&JsValue::from(err))))
        })?;
        String::from(string)
    } else if let Some(text) = value.as_string() {
        text
    } else if let Some(number) = value.as_f64() {
        if !number.is_finite() {
            return Err(range_error(&format!("{field} must be a finite number")));
        }
        if number.fract() != 0.0 {
            return Err(range_error(&format!(
                "{field} must be a whole number of nanoseconds"
            )));
        }
        if !(JS_MIN_SAFE_INTEGER..=JS_MAX_SAFE_INTEGER).contains(&number) {
            return Err(range_error(&format!(
                "{field} is outside the JavaScript safe integer range; pass a bigint or a \
                 decimal string to keep every nanosecond"
            )));
        }
        return Ok(number as i64);
    } else {
        return Err(type_error(&format!(
            "{field} must be a bigint, a decimal string, or a safe-integer number"
        )));
    };

    text.trim().parse::<i64>().map_err(|_| {
        range_error(&format!(
            "{field} must be a whole number of nanoseconds within \
             [-9223372036854775808, 9223372036854775807]"
        ))
    })
}

/// Read a day-of-year in the domain the engine's `u16` field holds.
///
/// A `number` reaching a `u16` parameter through the generated glue is narrowed
/// by truncation, which turns 65536 into 0 and -1 into 65535; the value is
/// checked here instead, before it can be narrowed.
fn exact_day_of_year(value: &JsValue) -> Result<u16, JsValue> {
    let number = value
        .as_f64()
        .ok_or_else(|| type_error("dayOfYear must be a number"))?;
    if !number.is_finite() {
        return Err(range_error("dayOfYear must be a finite number"));
    }
    if number.fract() != 0.0 {
        return Err(range_error("dayOfYear must be a whole number"));
    }
    if !(0.0..=65535.0).contains(&number) {
        return Err(range_error("dayOfYear must be in [0, 65535]"));
    }
    Ok(number as u16)
}

/// A regular TEC grid timestamp, exact to the nanosecond.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TecGridEpoch {
    pub(crate) inner: CoreTecGridEpoch,
}

#[wasm_bindgen]
impl TecGridEpoch {
    /// Build an epoch from exact Unix nanoseconds and a day-of-year companion.
    ///
    /// `unixNanos` takes a `bigint`, a decimal string, or a `number` that is a
    /// safe integer. A `number` past `Number.MAX_SAFE_INTEGER` is refused rather
    /// than silently rounded, which every present-day nanosecond timestamp is.
    #[wasm_bindgen(constructor)]
    pub fn new(
        #[wasm_bindgen(unchecked_param_type = "bigint | string | number")] unix_nanos: JsValue,
        #[wasm_bindgen(unchecked_param_type = "number")] day_of_year: JsValue,
    ) -> Result<TecGridEpoch, JsValue> {
        let unix_nanos = exact_i64_nanos(&unix_nanos, "unixNanos")?;
        let day_of_year = exact_day_of_year(&day_of_year)?;
        Ok(TecGridEpoch {
            inner: CoreTecGridEpoch::new(unix_nanos, day_of_year),
        })
    }

    /// The exact Unix nanosecond timestamp.
    #[wasm_bindgen(getter, js_name = unixNanos)]
    pub fn unix_nanos(&self) -> js_sys::BigInt {
        js_sys::BigInt::from(self.inner.unix_nanos)
    }

    /// The same timestamp as an exact decimal string.
    #[wasm_bindgen(getter, js_name = unixNanosString)]
    pub fn unix_nanos_string(&self) -> String {
        self.inner.unix_nanos.to_string()
    }

    /// The day-of-year carried beside the timestamp. The regular-grid
    /// interpolation does not read it.
    #[wasm_bindgen(getter, js_name = dayOfYear)]
    pub fn day_of_year(&self) -> u16 {
        self.inner.day_of_year
    }
}

/// Earth radius and shell height for the thin-shell pierce-point geometry.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TecGridShellGeometry {
    pub(crate) inner: CoreTecGridShellGeometry,
}

#[wasm_bindgen]
impl TecGridShellGeometry {
    #[wasm_bindgen(constructor)]
    pub fn new(earth_radius_m: f64, shell_height_m: f64) -> Result<TecGridShellGeometry, JsValue> {
        require_finite(earth_radius_m, "earthRadiusM")?;
        require_finite(shell_height_m, "shellHeightM")?;
        Ok(TecGridShellGeometry {
            inner: CoreTecGridShellGeometry::new(earth_radius_m, shell_height_m),
        })
    }

    /// The engine's own Earth radius and ionospheric shell height.
    #[wasm_bindgen(js_name = defaultShell)]
    pub fn default_shell() -> TecGridShellGeometry {
        TecGridShellGeometry {
            inner: CoreTecGridShellGeometry::default_shell(),
        }
    }

    #[wasm_bindgen(getter, js_name = earthRadiusM)]
    pub fn earth_radius_m(&self) -> f64 {
        self.inner.earth_radius_m
    }

    #[wasm_bindgen(getter, js_name = shellHeightM)]
    pub fn shell_height_m(&self) -> f64 {
        self.inner.shell_height_m
    }

    /// `earthRadiusM + shellHeightM`.
    #[wasm_bindgen(js_name = shellRadiusM)]
    pub fn shell_radius_m(&self) -> f64 {
        self.inner.shell_radius_m()
    }
}

/// Options a regular-grid ECEF query evaluates under.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TecGridEvalOptions {
    pub(crate) inner: CoreTecGridEvalOptions,
}

#[wasm_bindgen]
impl TecGridEvalOptions {
    #[wasm_bindgen(constructor)]
    pub fn new(epoch: &TecGridEpoch, frequency_hz: f64) -> Result<TecGridEvalOptions, JsValue> {
        require_finite(frequency_hz, "frequencyHz")?;
        Ok(TecGridEvalOptions {
            inner: CoreTecGridEvalOptions::new(epoch.inner, frequency_hz),
        })
    }

    /// Options for the canonical GPS L1 carrier.
    #[wasm_bindgen(js_name = l1)]
    pub fn l1(epoch: &TecGridEpoch) -> TecGridEvalOptions {
        TecGridEvalOptions {
            inner: CoreTecGridEvalOptions::l1(epoch.inner),
        }
    }

    #[wasm_bindgen(getter)]
    pub fn epoch(&self) -> TecGridEpoch {
        TecGridEpoch {
            inner: self.inner.epoch,
        }
    }

    #[wasm_bindgen(getter, js_name = minElevationRad)]
    pub fn min_elevation_rad(&self) -> f64 {
        self.inner.min_elevation_rad
    }

    #[wasm_bindgen(setter, js_name = minElevationRad)]
    pub fn set_min_elevation_rad(&mut self, value: f64) {
        self.inner.min_elevation_rad = value;
    }

    #[wasm_bindgen(getter, js_name = nanPiercePointHeightM)]
    pub fn nan_pierce_point_height_m(&self) -> f64 {
        self.inner.nan_pierce_point_height_m
    }

    #[wasm_bindgen(setter, js_name = nanPiercePointHeightM)]
    pub fn set_nan_pierce_point_height_m(&mut self, value: f64) {
        self.inner.nan_pierce_point_height_m = value;
    }

    #[wasm_bindgen(getter, js_name = frequencyHz)]
    pub fn frequency_hz(&self) -> f64 {
        self.inner.frequency_hz
    }

    #[wasm_bindgen(setter, js_name = frequencyHz)]
    pub fn set_frequency_hz(&mut self, value: f64) {
        self.inner.frequency_hz = value;
    }

    #[wasm_bindgen(getter, js_name = shellGeometry)]
    pub fn shell_geometry(&self) -> TecGridShellGeometry {
        TecGridShellGeometry {
            inner: self.inner.shell_geometry,
        }
    }

    #[wasm_bindgen(setter, js_name = shellGeometry)]
    pub fn set_shell_geometry(&mut self, geometry: &TecGridShellGeometry) {
        self.inner.shell_geometry = geometry.inner;
    }
}

fn parse_vec3(value: &[f64], name: &str) -> Result<[f64; 3], JsValue> {
    if value.len() < 3 {
        return Err(range_error(&format!(
            "{name} must contain at least 3 elements"
        )));
    }
    require_finite(value[0], name)?;
    require_finite(value[1], name)?;
    require_finite(value[2], name)?;
    Ok([value[0], value[1], value[2]])
}

/// The first value a coordinate callback threw, or the first way its return
/// value was malformed, kept so the original can be re-raised.
type CallbackFailure = Rc<RefCell<Option<JsValue>>>;

/// Keep the first failure a coordinate callback produced, so the value the
/// caller actually saw is the one re-raised.
fn record_failure(slot: &CallbackFailure, err: JsValue) {
    let mut held = slot.borrow_mut();
    if held.is_none() {
        *held = Some(err);
    }
}

/// The coordinate conversion handed to the engine, borrowing the caller's
/// function for as long as the evaluation runs.
type EcefCallback<'a> = Box<dyn Fn(&[f64; 3]) -> [f64; 3] + 'a>;

/// The ECEF-to-geodetic conversion a regular-grid query uses, with the slot a
/// failure is recorded in.
///
/// The engine's callback signature cannot fail, and it reads a returned NaN as
/// a deliberate marker, falling back to the receiver position. A JavaScript
/// exception is not that marker: it is recorded here and re-raised after the
/// engine call, so a thrown error never comes back as a delay.
fn make_ecef_callback(
    ecef_to_lla: Option<&js_sys::Function>,
) -> (EcefCallback<'_>, CallbackFailure) {
    const NAN3: [f64; 3] = [f64::NAN, f64::NAN, f64::NAN];
    let failure: CallbackFailure = Rc::new(RefCell::new(None));

    match ecef_to_lla {
        Some(func) => {
            let slot = Rc::clone(&failure);
            let callback = Box::new(move |xyz: &[f64; 3]| -> [f64; 3] {
                let arguments = js_sys::Array::new();
                for component in xyz {
                    arguments.push(&JsValue::from_f64(*component));
                }
                let returned = match func.call1(&JsValue::NULL, &arguments) {
                    Ok(value) => value,
                    Err(thrown) => {
                        // The thrown value itself, not a description of it, so a
                        // caller can match on the very error it threw.
                        record_failure(&slot, thrown);
                        return NAN3;
                    }
                };
                let Some(array) = returned.dyn_ref::<js_sys::Array>() else {
                    record_failure(
                        &slot,
                        type_error(
                            "ecefToLla must return an array of three numbers \
                             [longitudeDeg, latitudeDeg, altitudeM]",
                        ),
                    );
                    return NAN3;
                };
                if array.length() != 3 {
                    record_failure(
                        &slot,
                        type_error(&format!(
                            "ecefToLla must return exactly 3 numbers, got {}",
                            array.length()
                        )),
                    );
                    return NAN3;
                }
                let mut out = [0.0f64; 3];
                for (index, slot_out) in out.iter_mut().enumerate() {
                    // `as_f64` accepts NaN, which the engine reads as the
                    // deliberate "no pierce point" marker it documents.
                    match array.get(index as u32).as_f64() {
                        Some(value) => *slot_out = value,
                        None => {
                            record_failure(
                                &slot,
                                type_error(&format!(
                                    "ecefToLla returned a non-numeric component at index {index}"
                                )),
                            );
                            return NAN3;
                        }
                    }
                }
                out
            });
            (callback, failure)
        }
        None => {
            let slot = Rc::clone(&failure);
            let callback = Box::new(move |xyz: &[f64; 3]| -> [f64; 3] {
                // The engine's own WGS84 conversion, which takes kilometres and
                // returns (latitudeDeg, longitudeDeg, altitudeKm).
                match sidereon_core::astro::frames::transforms::itrs_to_geodetic_compute(
                    xyz[0] / 1000.0,
                    xyz[1] / 1000.0,
                    xyz[2] / 1000.0,
                ) {
                    Ok((lat_deg, lon_deg, alt_km)) => [lon_deg, lat_deg, alt_km * 1000.0],
                    Err(err) => {
                        record_failure(&slot, engine_error(err));
                        NAN3
                    }
                }
            });
            (callback, failure)
        }
    }
}

/// Take the callback failure, if any, so it is raised ahead of whatever the
/// engine returned from a fabricated coordinate.
fn callback_failure(failure: &CallbackFailure) -> Option<JsValue> {
    failure.borrow_mut().take()
}

/// Regular-grid ionospheric group delay for an ECEF satellite/receiver pair.
#[wasm_bindgen(js_name = ionoDelayXyz)]
pub fn iono_delay_xyz(
    grid: &TecGrid,
    options: &TecGridEvalOptions,
    #[wasm_bindgen(unchecked_param_type = "number[] | Float64Array")] sat_xyz: &[f64],
    #[wasm_bindgen(unchecked_param_type = "number[] | Float64Array")] receiver_xyz: &[f64],
    #[wasm_bindgen(unchecked_optional_param_type = "EcefToLla | null")] ecef_to_lla: Option<
        js_sys::Function,
    >,
) -> Result<f64, JsValue> {
    let satellite = parse_vec3(sat_xyz, "satXyz")?;
    let receiver = parse_vec3(receiver_xyz, "receiverXyz")?;
    let (callback, failure) = make_ecef_callback(ecef_to_lla.as_ref());
    let result = core_iono_delay_xyz(&grid.inner, options.inner, &satellite, &receiver, callback);
    if let Some(thrown) = callback_failure(&failure) {
        return Err(thrown);
    }
    result.map_err(tec_grid_error_to_js)
}

/// Regular-grid ionospheric group delay with an explicit missing-node policy.
#[wasm_bindgen(
    js_name = ionoDelayXyzWithPolicy,
    unchecked_return_type = "TecGridEvaluation<number>"
)]
pub fn iono_delay_xyz_with_policy(
    grid: &TecGrid,
    options: &TecGridEvalOptions,
    #[wasm_bindgen(unchecked_param_type = "number[] | Float64Array")] sat_xyz: &[f64],
    #[wasm_bindgen(unchecked_param_type = "number[] | Float64Array")] receiver_xyz: &[f64],
    #[wasm_bindgen(unchecked_param_type = "IonexMissingNodePolicy | null")] policy: Option<String>,
    #[wasm_bindgen(unchecked_optional_param_type = "EcefToLla | null")] ecef_to_lla: Option<
        js_sys::Function,
    >,
) -> Result<JsValue, JsValue> {
    let satellite = parse_vec3(sat_xyz, "satXyz")?;
    let receiver = parse_vec3(receiver_xyz, "receiverXyz")?;
    let policy = match policy.as_deref() {
        Some(policy) => parse_missing_node_policy(policy)?,
        None => IonexMissingNodePolicy::Strict,
    };
    let (callback, failure) = make_ecef_callback(ecef_to_lla.as_ref());
    let result = core_iono_delay_xyz_with_policy(
        &grid.inner,
        options.inner,
        &satellite,
        &receiver,
        callback,
        policy,
    );
    if let Some(thrown) = callback_failure(&failure) {
        return Err(thrown);
    }
    let evaluation = result.map_err(tec_grid_error_to_js)?;
    to_js(&TecGridEvaluationJs {
        value: evaluation.value,
        degraded: evaluation.degraded.map(IonexNodeGapJs::from_core),
    })
}

/// Vertical and slant TEC for an ECEF satellite/receiver pair, `[vtec, stec]`.
#[wasm_bindgen(js_name = tecXyz)]
pub fn tec_xyz(
    grid: &TecGrid,
    options: &TecGridEvalOptions,
    #[wasm_bindgen(unchecked_param_type = "number[] | Float64Array")] sat_xyz: &[f64],
    #[wasm_bindgen(unchecked_param_type = "number[] | Float64Array")] receiver_xyz: &[f64],
    #[wasm_bindgen(unchecked_optional_param_type = "EcefToLla | null")] ecef_to_lla: Option<
        js_sys::Function,
    >,
) -> Result<Vec<f64>, JsValue> {
    let satellite = parse_vec3(sat_xyz, "satXyz")?;
    let receiver = parse_vec3(receiver_xyz, "receiverXyz")?;
    let (callback, failure) = make_ecef_callback(ecef_to_lla.as_ref());
    let result = core_tec_xyz(&grid.inner, options.inner, &satellite, &receiver, callback);
    if let Some(thrown) = callback_failure(&failure) {
        return Err(thrown);
    }
    let (vtec, stec) = result.map_err(tec_grid_error_to_js)?;
    Ok(vec![vtec, stec])
}

/// Vertical and slant TEC with an explicit missing-node policy.
#[wasm_bindgen(
    js_name = tecXyzWithPolicy,
    unchecked_return_type = "TecGridEvaluation<[number, number]>"
)]
pub fn tec_xyz_with_policy(
    grid: &TecGrid,
    options: &TecGridEvalOptions,
    #[wasm_bindgen(unchecked_param_type = "number[] | Float64Array")] sat_xyz: &[f64],
    #[wasm_bindgen(unchecked_param_type = "number[] | Float64Array")] receiver_xyz: &[f64],
    #[wasm_bindgen(unchecked_param_type = "IonexMissingNodePolicy | null")] policy: Option<String>,
    #[wasm_bindgen(unchecked_optional_param_type = "EcefToLla | null")] ecef_to_lla: Option<
        js_sys::Function,
    >,
) -> Result<JsValue, JsValue> {
    let satellite = parse_vec3(sat_xyz, "satXyz")?;
    let receiver = parse_vec3(receiver_xyz, "receiverXyz")?;
    let policy = match policy.as_deref() {
        Some(policy) => parse_missing_node_policy(policy)?,
        None => IonexMissingNodePolicy::Strict,
    };
    let (callback, failure) = make_ecef_callback(ecef_to_lla.as_ref());
    let result = core_tec_xyz_with_policy(
        &grid.inner,
        options.inner,
        &satellite,
        &receiver,
        callback,
        policy,
    );
    if let Some(thrown) = callback_failure(&failure) {
        return Err(thrown);
    }
    let evaluation = result.map_err(tec_grid_error_to_js)?;
    to_js(&TecGridEvaluationJs {
        value: vec![evaluation.value.0, evaluation.value.1],
        degraded: evaluation.degraded.map(IonexNodeGapJs::from_core),
    })
}

// --- TypeScript declarations ------------------------------------------------

// These are the declarations `wasm-pack` emits into both `sidereon.d.ts`
// targets, and the names the `unchecked_return_type` / `unchecked_param_type`
// attributes above resolve against. `types/sidereon-extra.d.ts` re-exports them
// rather than restating them, so the two never drift.
#[wasm_bindgen(typescript_custom_section)]
const TS_IONEX_DEFINITIONS: &str = r#"
export type IonexMappingFunction =
  | { kind: "NO_MAPPING"; code: "NONE" }
  | { kind: "COSZ"; code: "COSZ" }
  | { kind: "Q_FACTOR"; code: "QFAC" }
  | { kind: "OTHER"; code: string };

export type IonexMappingDeclaration =
  | { kind: "DECLARED"; function: IonexMappingFunction }
  | { kind: "ABSENT" };

export type IonexAssumedMappingKind = "NO_MAPPING" | "Q_FACTOR" | "OTHER" | "ABSENT";

export interface IonexAssumedMapping {
  kind: IonexAssumedMappingKind;
  message: string;
}

export type IonexCoverageErrorKind =
  | "EPOCH_BEFORE_FIRST_MAP"
  | "EPOCH_AFTER_LAST_MAP"
  | "LATITUDE_OUT_OF_RANGE"
  | "LONGITUDE_OUT_OF_RANGE";

export interface IonexCoverageError {
  kind: IonexCoverageErrorKind;
  message: string;
}

export interface IonexMissingNodes {
  mapNumber: number;
  latIndex: number;
  lonIndex: number;
  lonIndexNext: number;
  missing: [boolean, boolean, boolean, boolean];
}

export interface IonexNodeGap {
  earlier: IonexMissingNodes | null;
  later: IonexMissingNodes | null;
}

export interface IonexSlantDelayStatus {
  held: IonexCoverageError | null;
  degraded: IonexNodeGap | null;
  assumedMapping: IonexAssumedMapping | null;
  isValid: boolean;
  isNominal: boolean;
  isHeld: boolean;
  isDegraded: boolean;
  isAssumedMapping: boolean;
}

export interface IonexSlantDelayEvaluation {
  delayM: number;
  status: IonexSlantDelayStatus;
}

export type IonexSlantRefusal =
  | { kind: "COVERAGE"; coverageError: IonexCoverageError; message: string }
  | { kind: "MISSING_NODES"; nodeGap: IonexNodeGap; message: string }
  | {
      kind: "VARYING_HEIGHTS";
      mapNumber: number;
      latIndex: number;
      lonIndex: number;
      message: string;
    }
  | {
      kind: "HEIGHT_NOT_AVAILABLE";
      mapNumber: number;
      latIndex: number;
      lonIndex: number;
      message: string;
    }
  | { kind: "MAPPING_FUNCTION"; mappingDeclaration: IonexMappingDeclaration; message: string }
  | { kind: "INVALID_INPUT"; message: string }
  | { kind: "UNKNOWN"; message: string };

export type IonexSlantBatchResult =
  | { index: number; isOk: true; evaluation: IonexSlantDelayEvaluation; refusal: null }
  | { index: number; isOk: false; evaluation: null; refusal: IonexSlantRefusal };

export interface IonexDiagnosticEpoch {
  scale: string;
  jdWhole: number | null;
  fraction: number | null;
  jd: number | null;
  nanos: string | null;
  j2000Seconds: string | null;
  j2000SecondsNumber: number | null;
  j2000SecondsF64: number;
}

export type IonexWarning =
  | { kind: "MISSING_RECORD"; label: string; message: string }
  | { kind: "VERSION_RECORD_NOT_FIRST"; line: number; message: string }
  | {
      kind: "EPOCH_MISMATCH";
      label: string;
      line: number;
      declaredEpoch: IonexDiagnosticEpoch;
      mapsEpoch: IonexDiagnosticEpoch;
      message: string;
    }
  | {
      kind: "MAP_COUNT_MISMATCH";
      line: number;
      declaredCount: string;
      declaredCountNumber: number | null;
      tecMaps: number;
      allMaps: number;
      message: string;
    }
  | {
      kind: "NOT_A_NUMBER_VALUE";
      dataKind: string;
      mapNumber: number;
      line: number;
      latDeg: number;
      lonDeg: number;
      message: string;
    }
  | {
      kind: "INTERVAL_MISMATCH";
      line: number;
      declaredS: number;
      mapNumber: number;
      spacingS: string;
      spacingSNumber: number | null;
      message: string;
    }
  | {
      kind: "EXPONENT_CARRIED_INTO_MAP";
      dataKind: string;
      mapNumber: number;
      line: number;
      exponent: number;
      setByLine: number;
      message: string;
    }
  | { kind: "UNKNOWN"; message: string };

export interface IonexParseResult {
  ionex: Ionex;
  value: Ionex;
  warnings: IonexWarning[];
}

export interface IonexHeader {
  version: number;
  satelliteSystem: string;
  program: string;
  runBy: string;
  date: string;
  descriptions: string[];
  comments: string[];
  intervalS: number;
  mappingFunction: IonexMappingFunction | null;
  mappingDeclaration: IonexMappingDeclaration;
  elevationCutoffDeg: number;
  observablesUsed: string;
  stationCount: number | null;
  satelliteCount: number | null;
  mapsInFile: number | null;
}

/**
 * A header as supplied to a constructor. `mappingFunction` and
 * `mappingDeclaration` are read presence-aware: omitting both states nothing,
 * and supplying both means they must agree, `null` beside a `DECLARED`
 * declaration included.
 *
 * A property named here that the object does not carry is an absence. Any other
 * own property, enumerable or not, is a `TypeError` naming it, so `stationCounts` is
 * refused rather than read as an absent `stationCount`.
 */
export interface IonexHeaderInput {
  version: number;
  satelliteSystem: string;
  program: string;
  runBy: string;
  date: string;
  descriptions: string[];
  comments: string[];
  intervalS: number;
  mappingFunction?: IonexMappingFunction | null;
  mappingDeclaration?: IonexMappingDeclaration | null;
  elevationCutoffDeg: number;
  observablesUsed: string;
  stationCount?: number | null;
  satelliteCount?: number | null;
  mapsInFile?: number | null;
}

/** Per-map grids indexed `[map][iLat][iLon]`; `null` marks a node with no value. */
export type IonexMapCube = (number | null)[][][];

export interface TecGridSamples {
  mapEpochsJ2000S: number[];
  latNodesDeg: number[];
  lonNodesDeg: number[];
  dlatDeg: number;
  dlonDeg: number;
  shellHeightKm: number;
  baseRadiusKm: number;
  exponent: number;
  tecMaps: IonexMapCube;
  rmsMaps: IonexMapCube | null;
  heightMaps: IonexMapCube | null;
  header: IonexHeader;
}

/**
 * Whole-grid samples as supplied to `ionexFromSamples`. Any own property,
 * enumerable or not, other than the ones named here is a `TypeError` naming it, so
 * `heightMap` is refused rather than read as an absent `heightMaps` cube.
 */
export interface TecGridSamplesInput {
  mapEpochsJ2000S: number[];
  latNodesDeg: number[];
  lonNodesDeg: number[];
  dlatDeg: number;
  dlonDeg: number;
  shellHeightKm: number;
  baseRadiusKm: number;
  exponent: number;
  tecMaps: IonexMapCube;
  rmsMaps?: IonexMapCube | null;
  heightMaps?: IonexMapCube | null;
  header?: IonexHeaderInput | null;
}

/**
 * One grid node. Supplied to `ionexFromNodeSamples`, any own property,
 * enumerable or not, other than the ones named here is a `TypeError` naming both the
 * property and the index of the sample carrying it, so `rmsTec` is refused
 * rather than read as a node with no RMS.
 */
export interface TecSample {
  epochJ2000S: number;
  latDeg: number;
  lonDeg: number;
  vtecTecu: number | null;
  rmsTecu?: number | null;
  heightOffsetKm?: number | null;
}

export interface IonexSlantRequest {
  latDeg: number;
  lonDeg: number;
  azimuthDeg: number;
  elevationDeg: number;
  epochJ2000S: number;
  frequencyHz: number;
}

export type IonexCoveragePolicy = "strict" | "hold";
export type IonexMissingNodePolicy = "strict" | "renormalize";
export type IonexMappingPolicy = "declared" | "singleLayer";

/** Each axis is independent; an omitted axis keeps its default. */
export interface IonexSlantPolicyInput {
  coverage?: IonexCoveragePolicy | null;
  missingNodes?: IonexMissingNodePolicy | null;
  /** Accepted alias of `missingNodes`; supplying both means they must agree. */
  missing_nodes?: IonexMissingNodePolicy | null;
  mapping?: IonexMappingPolicy | "single_layer" | null;
}

/** An `IonexSlantPolicy` instance, a plain policy object, or nothing at all. */
export type IonexSlantPolicyLike =
  | IonexSlantPolicy
  | IonexSlantPolicyInput
  | null
  | undefined;

export interface TecGridEvaluation<T> {
  value: T;
  degraded: IonexNodeGap | null;
}

export type TecGridErrorDetail =
  | { kind: "AXES_TOO_SHORT"; message: string }
  | { kind: "AXES_NOT_INCREASING"; message: string }
  | { kind: "DIMENSIONS_OVERFLOW"; message: string }
  | { kind: "VALUE_COUNT_MISMATCH"; actual: number; expected: number; message: string }
  | { kind: "INVALID_FIELD"; field: string; reason: string; message: string }
  | { kind: "NODES_NOT_AVAILABLE"; nodeGap: IonexNodeGap; message: string }
  | { kind: "OUT_OF_BOUNDS"; name: string; value: number; message: string }
  | { kind: "UNKNOWN"; message: string };

export interface IonexWriterErrorDetail {
  kind: "UNWRITABLE";
  message: string;
}

/**
 * Converts an ECEF pierce point in metres to
 * `[longitudeDeg, latitudeDeg, altitudeM]`.
 *
 * A returned NaN component is the engine's deliberate marker for "no pierce
 * point" and falls back to the receiver position. A thrown value is not that
 * marker: it is re-raised unchanged, so a thrown error never comes back as a
 * delay.
 */
export type EcefToLla = (xyz: number[]) => [number, number, number];
"#;
