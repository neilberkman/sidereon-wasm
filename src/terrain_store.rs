//! Memory-mappable terrain store binding.
//!
//! The store bytes are owned by the WASM handle so browser and Node callers can
//! use the in-memory path even when host mmap is unavailable. Terrain query
//! points are always `(longitudeDeg, latitudeDeg)`. DTED postings and store
//! queries are ORTHOMETRIC heights `H` in metres above the EGM96 mean sea level
//! geoid. Ellipsoidal height is exposed only through explicit `h = H + N`
//! conversion APIs.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::terrain::{DtedInterpolation, DtedLookupOptions};
use sidereon_core::terrain_store::{
    dted_tree_to_mmap_store as core_dted_tree_to_mmap_store,
    terrain_store_checksum64 as core_terrain_store_checksum64,
    write_dted_tree_to_mmap_store as core_write_dted_tree_to_mmap_store,
    Egm96FifteenMinuteGeoid as CoreEgm96FifteenMinuteGeoid,
    EllipsoidalHeightM as CoreEllipsoidalHeightM, MmapTerrain as CoreMmapTerrain,
    OrthometricHeightM as CoreOrthometricHeightM, TerrainDatumError as CoreTerrainDatumError,
    TerrainGeoidModel as CoreTerrainGeoidModel, TerrainStoreError as CoreTerrainStoreError,
    TerrainStoreTileIndex as CoreTerrainStoreTileIndex, VerticalDatum as CoreVerticalDatum,
};

use crate::error::{engine_error, type_error};

const MISSING_EGM96_DAC_REMEDIATION: &str =
    "load WW15MGH.DAC with Egm96FifteenMinuteGeoid.fromWw15mghDacBytes or use fromWw15mghDacPath where host I/O is available";

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct TerrainStoreOptionsInput {
    interpolation: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum TerrainStorePointInput {
    Pair([f64; 2]),
    Object(TerrainStorePointObjectInput),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TerrainStorePointObjectInput {
    longitude_deg: f64,
    latitude_deg: f64,
}

impl TerrainStorePointInput {
    fn lon_lat(&self) -> (f64, f64) {
        match self {
            Self::Pair([longitude_deg, latitude_deg]) => (*longitude_deg, *latitude_deg),
            Self::Object(point) => (point.longitude_deg, point.latitude_deg),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TerrainStoreHeightBatchResult {
    ok: bool,
    height_m: Option<f64>,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OrthometricHeightObject {
    value_m: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TerrainStoreOrthometricBatchResult {
    ok: bool,
    orthometric_height_m: Option<OrthometricHeightObject>,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TerrainStoreErrorDetail {
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
    lat_index: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lon_index: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    found: Option<u64>,
}

impl TerrainStoreErrorDetail {
    fn new(name: &'static str, message: String) -> Self {
        Self {
            name,
            message,
            path: None,
            reason: None,
            version: None,
            tag: None,
            lat_index: None,
            lon_index: None,
            expected: None,
            found: None,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TerrainDatumErrorDetail {
    name: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    remediation: Option<&'static str>,
}

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| engine_error(format!("failed to serialize result: {e}")))
}

fn interpolation(value: Option<&str>) -> Result<DtedInterpolation, JsValue> {
    match value.unwrap_or("bilinear") {
        "nearest" | "nearestPosting" => Ok(DtedInterpolation::NearestPosting),
        "bilinear" => Ok(DtedInterpolation::Bilinear),
        other => Err(type_error(&format!(
            "invalid interpolation {other:?}: expected \"nearest\" or \"bilinear\""
        ))),
    }
}

fn lookup_options(value: JsValue) -> Result<DtedLookupOptions, JsValue> {
    let options: TerrainStoreOptionsInput = if value.is_undefined() || value.is_null() {
        TerrainStoreOptionsInput::default()
    } else {
        serde_wasm_bindgen::from_value(value)
            .map_err(|e| type_error(&format!("invalid terrain store options: {e}")))?
    };
    Ok(DtedLookupOptions {
        interpolation: interpolation(options.interpolation.as_deref())?,
    })
}

fn parse_points(value: JsValue) -> Result<Vec<(f64, f64)>, JsValue> {
    let points: Vec<TerrainStorePointInput> = serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid terrain store points: {e}")))?;
    Ok(points.iter().map(TerrainStorePointInput::lon_lat).collect())
}

fn datum_from_core(value: CoreVerticalDatum) -> VerticalDatum {
    match value {
        CoreVerticalDatum::Egm96MslOrthometric => VerticalDatum::Egm96MslOrthometric,
    }
}

fn attach_detail<T: Serialize>(value: &JsValue, detail: &T) {
    let detail_value =
        serde_wasm_bindgen::to_value(detail).expect("serialize typed terrain error detail");
    js_sys::Reflect::set(value, &JsValue::from_str("detail"), &detail_value)
        .expect("attach typed terrain error detail");
}

fn typed_error<T: Serialize>(name: &'static str, message: String, detail: &T) -> JsValue {
    let js_error = js_sys::Error::new(&message);
    js_error.set_name(name);
    let value: JsValue = js_error.into();
    js_sys::Reflect::set(&value, &JsValue::from_str("kind"), &JsValue::from_str(name))
        .expect("attach typed terrain error kind");
    attach_detail(&value, detail);
    value
}

fn missing_egm96_dac_error(path: String) -> JsValue {
    let detail = TerrainDatumErrorDetail {
        name: "MissingEgm96Dac",
        message: format!("{path} is missing; {MISSING_EGM96_DAC_REMEDIATION}"),
        path: Some(path),
        remediation: Some(MISSING_EGM96_DAC_REMEDIATION),
    };
    let value = typed_error(detail.name, detail.message.clone(), &detail);
    if let Some(path) = &detail.path {
        js_sys::Reflect::set(&value, &JsValue::from_str("path"), &JsValue::from_str(path))
            .expect("attach terrain datum error path");
    }
    js_sys::Reflect::set(
        &value,
        &JsValue::from_str("remediation"),
        &JsValue::from_str(MISSING_EGM96_DAC_REMEDIATION),
    )
    .expect("attach terrain datum error remediation");
    value
}

fn terrain_store_error(error: CoreTerrainStoreError) -> JsValue {
    let message = error.to_string();
    let mut detail = match error {
        CoreTerrainStoreError::Io { path, message: _ } => {
            let mut detail = TerrainStoreErrorDetail::new("Io", message.clone());
            detail.path = Some(path.display().to_string());
            detail
        }
        CoreTerrainStoreError::Parse { reason } => {
            let mut detail = TerrainStoreErrorDetail::new("Parse", message.clone());
            detail.reason = Some(reason);
            detail
        }
        CoreTerrainStoreError::UnsupportedVersion { version } => {
            let mut detail = TerrainStoreErrorDetail::new("UnsupportedVersion", message.clone());
            detail.version = Some(version);
            detail
        }
        CoreTerrainStoreError::TileIdMismatch { path, .. } => {
            let mut detail = TerrainStoreErrorDetail::new("TileIdMismatch", message.clone());
            detail.path = Some(path.display().to_string());
            detail
        }
        CoreTerrainStoreError::UnsupportedDatum { tag } => {
            let mut detail = TerrainStoreErrorDetail::new("UnsupportedDatum", message.clone());
            detail.tag = Some(tag);
            detail
        }
        CoreTerrainStoreError::DuplicateTile {
            lat_index,
            lon_index,
        } => {
            let mut detail = TerrainStoreErrorDetail::new("DuplicateTile", message.clone());
            detail.lat_index = Some(lat_index);
            detail.lon_index = Some(lon_index);
            detail
        }
        CoreTerrainStoreError::Checksum {
            lat_index,
            lon_index,
            expected,
            found,
        } => {
            let mut detail = TerrainStoreErrorDetail::new("Checksum", message.clone());
            detail.lat_index = Some(lat_index);
            detail.lon_index = Some(lon_index);
            detail.expected = Some(expected);
            detail.found = Some(found);
            detail
        }
    };
    detail.message = message.clone();
    typed_error(detail.name, message, &detail)
}

fn terrain_datum_error(error: CoreTerrainDatumError) -> JsValue {
    let message = error.to_string();
    let detail = match error {
        CoreTerrainDatumError::Terrain(_) => TerrainDatumErrorDetail {
            name: "Terrain",
            message,
            path: None,
            remediation: None,
        },
        CoreTerrainDatumError::Geoid(_) => TerrainDatumErrorDetail {
            name: "Geoid",
            message,
            path: None,
            remediation: None,
        },
        CoreTerrainDatumError::Io { path, message: _ } => TerrainDatumErrorDetail {
            name: "Io",
            message,
            path: Some(path.display().to_string()),
            remediation: None,
        },
        CoreTerrainDatumError::MissingEgm96Dac { path, remediation } => TerrainDatumErrorDetail {
            name: "MissingEgm96Dac",
            message,
            path: Some(path.display().to_string()),
            remediation: Some(remediation),
        },
    };
    let value = typed_error(detail.name, detail.message.clone(), &detail);
    if let Some(path) = &detail.path {
        js_sys::Reflect::set(&value, &JsValue::from_str("path"), &JsValue::from_str(path))
            .expect("attach terrain datum error path");
    }
    if let Some(remediation) = detail.remediation {
        js_sys::Reflect::set(
            &value,
            &JsValue::from_str("remediation"),
            &JsValue::from_str(remediation),
        )
        .expect("attach terrain datum error remediation");
    }
    value
}

/// Vertical datum carried by a terrain store.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerticalDatum {
    /// Orthometric height `H` in metres above the EGM96 mean sea level geoid.
    Egm96MslOrthometric,
}

/// Terrain store conversion, serialization, and parsing error variants.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainStoreError {
    /// File or directory I/O failed.
    Io,
    /// DTED or terrain store bytes could not be parsed.
    Parse,
    /// The terrain store version is not supported.
    UnsupportedVersion,
    /// The terrain store datum tag is not supported.
    UnsupportedDatum,
    /// Two input DTED files resolved to the same integer tile id.
    DuplicateTile,
    /// A tile payload checksum did not match its index record.
    Checksum,
}

/// Terrain datum conversion and optional geoid-grid loading error variants.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainDatumError {
    /// Terrain lookup failed before datum conversion.
    Terrain,
    /// A geoid grid could not be parsed.
    Geoid,
    /// Reading a geoid grid failed for a reason other than absence.
    Io,
    /// The EGM96 15-arcminute `WW15MGH.DAC` grid was requested but is absent.
    MissingEgm96Dac,
}

/// Orthometric terrain height `H` in metres above the EGM96 mean sea level
/// geoid.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct OrthometricHeightM {
    inner: CoreOrthometricHeightM,
}

#[wasm_bindgen]
impl OrthometricHeightM {
    /// Build an orthometric height `H` in metres.
    #[wasm_bindgen(constructor)]
    pub fn new(value_m: f64) -> OrthometricHeightM {
        Self {
            inner: CoreOrthometricHeightM::new(value_m),
        }
    }

    /// Orthometric height `H`, metres above the EGM96 mean sea level geoid.
    #[wasm_bindgen(getter, js_name = valueM)]
    pub fn value_m(&self) -> f64 {
        self.inner.metres()
    }

    /// Convert to ellipsoidal height `h = H + N` using degree inputs in geoid
    /// order `(latitudeDeg, longitudeDeg)` and an explicit geoid model.
    #[wasm_bindgen(js_name = toEllipsoidalHeightDeg)]
    pub fn to_ellipsoidal_height_deg(
        &self,
        latitude_deg: f64,
        longitude_deg: f64,
        geoid: &TerrainGeoidModel,
    ) -> Result<EllipsoidalHeightM, JsValue> {
        let inner = self
            .inner
            .to_ellipsoidal_height_deg(latitude_deg, longitude_deg, geoid.as_core())
            .map_err(terrain_datum_error)?;
        Ok(EllipsoidalHeightM { inner })
    }

    /// Convert to ellipsoidal height `h = H + N` using radian inputs in geoid
    /// order `(latitudeRad, longitudeRad)` and an explicit geoid model.
    #[wasm_bindgen(js_name = toEllipsoidalHeightRad)]
    pub fn to_ellipsoidal_height_rad(
        &self,
        latitude_rad: f64,
        longitude_rad: f64,
        geoid: &TerrainGeoidModel,
    ) -> Result<EllipsoidalHeightM, JsValue> {
        let inner = self
            .inner
            .to_ellipsoidal_height_rad(latitude_rad, longitude_rad, geoid.as_core())
            .map_err(terrain_datum_error)?;
        Ok(EllipsoidalHeightM { inner })
    }
}

/// Ellipsoidal height `h` in metres above the WGS84 reference ellipsoid.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct EllipsoidalHeightM {
    inner: CoreEllipsoidalHeightM,
}

#[wasm_bindgen]
impl EllipsoidalHeightM {
    /// Build an ellipsoidal height `h` in metres.
    #[wasm_bindgen(constructor)]
    pub fn new(value_m: f64) -> EllipsoidalHeightM {
        Self {
            inner: CoreEllipsoidalHeightM::new(value_m),
        }
    }

    /// Ellipsoidal height `h`, metres above the WGS84 reference ellipsoid.
    #[wasm_bindgen(getter, js_name = valueM)]
    pub fn value_m(&self) -> f64 {
        self.inner.metres()
    }
}

/// Loaded EGM96 15-arcminute geoid grid for explicit terrain datum conversion.
#[wasm_bindgen]
#[derive(Clone)]
pub struct Egm96FifteenMinuteGeoid {
    inner: CoreEgm96FifteenMinuteGeoid,
}

#[wasm_bindgen]
impl Egm96FifteenMinuteGeoid {
    /// Load `WW15MGH.DAC` bytes as an EGM96 15-arcminute geoid grid.
    #[wasm_bindgen(js_name = fromWw15mghDacBytes)]
    pub fn from_ww15mgh_dac_bytes(bytes: &[u8]) -> Result<Egm96FifteenMinuteGeoid, JsValue> {
        let inner = CoreEgm96FifteenMinuteGeoid::from_ww15mgh_dac_bytes(bytes)
            .map_err(terrain_datum_error)?;
        Ok(Self { inner })
    }

    /// Read and load `WW15MGH.DAC` from disk. A missing file throws a typed
    /// `MissingEgm96Dac` error with `path` and `remediation` fields.
    #[wasm_bindgen(js_name = fromWw15mghDacPath)]
    pub fn from_ww15mgh_dac_path(path: &str) -> Result<Egm96FifteenMinuteGeoid, JsValue> {
        let inner =
            CoreEgm96FifteenMinuteGeoid::from_ww15mgh_dac_path(path).map_err(|err| match err {
                CoreTerrainDatumError::Io { path, message }
                    if message.contains("operation not supported") =>
                {
                    missing_egm96_dac_error(path.display().to_string())
                }
                other => terrain_datum_error(other),
            })?;
        Ok(Self { inner })
    }
}

#[derive(Clone)]
enum TerrainGeoidModelInner {
    Egm96OneDegree,
    Egm96FifteenMinute(CoreEgm96FifteenMinuteGeoid),
}

/// Geoid model used to convert terrain orthometric height `H` to ellipsoidal
/// height `h`.
#[wasm_bindgen]
#[derive(Clone)]
pub struct TerrainGeoidModel {
    inner: TerrainGeoidModelInner,
}

impl TerrainGeoidModel {
    fn as_core(&self) -> CoreTerrainGeoidModel<'_> {
        match &self.inner {
            TerrainGeoidModelInner::Egm96OneDegree => CoreTerrainGeoidModel::Egm96OneDegree,
            TerrainGeoidModelInner::Egm96FifteenMinute(geoid) => {
                CoreTerrainGeoidModel::Egm96FifteenMinute(geoid)
            }
        }
    }
}

#[wasm_bindgen]
impl TerrainGeoidModel {
    /// Use the embedded EGM96 1-degree geoid grid for `h = H + N`.
    #[wasm_bindgen(js_name = egm96OneDegree)]
    pub fn egm96_one_degree() -> TerrainGeoidModel {
        Self {
            inner: TerrainGeoidModelInner::Egm96OneDegree,
        }
    }

    /// Use a caller-supplied EGM96 15-arcminute geoid grid for `h = H + N`.
    #[wasm_bindgen(js_name = egm96FifteenMinute)]
    pub fn egm96_fifteen_minute(geoid: &Egm96FifteenMinuteGeoid) -> TerrainGeoidModel {
        Self {
            inner: TerrainGeoidModelInner::Egm96FifteenMinute(geoid.inner.clone()),
        }
    }

    /// Model discriminator: `"egm96OneDegree"` or `"egm96FifteenMinute"`.
    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> String {
        match self.inner {
            TerrainGeoidModelInner::Egm96OneDegree => "egm96OneDegree",
            TerrainGeoidModelInner::Egm96FifteenMinute(_) => "egm96FifteenMinute",
        }
        .to_string()
    }
}

/// Metadata for one tile index record in a memory-mappable terrain store.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct TerrainStoreTileIndex {
    inner: CoreTerrainStoreTileIndex,
}

#[wasm_bindgen]
impl TerrainStoreTileIndex {
    /// Integer latitude tile id, for example `36` for a tile covering
    /// `36..37` degrees.
    #[wasm_bindgen(getter, js_name = latIndex)]
    pub fn lat_index(&self) -> i32 {
        self.inner.lat_index
    }

    /// Integer longitude tile id, for example `-107` for a tile covering
    /// `-107..-106` degrees.
    #[wasm_bindgen(getter, js_name = lonIndex)]
    pub fn lon_index(&self) -> i32 {
        self.inner.lon_index
    }

    /// Western edge longitude, degrees.
    #[wasm_bindgen(getter, js_name = minLongitudeDeg)]
    pub fn min_longitude_deg(&self) -> f64 {
        self.inner.min_longitude_deg
    }

    /// Southern edge latitude, degrees.
    #[wasm_bindgen(getter, js_name = minLatitudeDeg)]
    pub fn min_latitude_deg(&self) -> f64 {
        self.inner.min_latitude_deg
    }

    /// Eastern edge longitude, degrees.
    #[wasm_bindgen(getter, js_name = maxLongitudeDeg)]
    pub fn max_longitude_deg(&self) -> f64 {
        self.inner.max_longitude_deg
    }

    /// Northern edge latitude, degrees.
    #[wasm_bindgen(getter, js_name = maxLatitudeDeg)]
    pub fn max_latitude_deg(&self) -> f64 {
        self.inner.max_latitude_deg
    }

    /// Number of longitude postings.
    #[wasm_bindgen(getter, js_name = lonCount)]
    pub fn lon_count(&self) -> u32 {
        self.inner.lon_count
    }

    /// Number of latitude postings.
    #[wasm_bindgen(getter, js_name = latCount)]
    pub fn lat_count(&self) -> u32 {
        self.inner.lat_count
    }

    /// Byte offset of this tile's posting payload in the store.
    #[wasm_bindgen(getter, js_name = dataOffset)]
    pub fn data_offset(&self) -> u64 {
        self.inner.data_offset
    }

    /// Byte length of this tile's posting payload in the store.
    #[wasm_bindgen(getter, js_name = dataLen)]
    pub fn data_len(&self) -> u64 {
        self.inner.data_len
    }

    /// FNV-1a checksum of this tile's posting payload bytes.
    #[wasm_bindgen(getter)]
    pub fn checksum64(&self) -> u64 {
        self.inner.checksum64
    }

    /// Vertical datum for this tile's orthometric posting payload.
    #[wasm_bindgen(getter, js_name = verticalDatum)]
    pub fn vertical_datum(&self) -> VerticalDatum {
        datum_from_core(self.inner.vertical_datum)
    }
}

/// Convert a DTED tile tree into canonical memory-mappable terrain store bytes.
///
/// The returned `Uint8Array` can be passed to [`MmapTerrain.fromBytes`] or
/// [`MmapTerrain.fromVec`]. Posting payloads are decoded orthometric metres.
#[wasm_bindgen(js_name = dtedTreeToMmapStore)]
pub fn dted_tree_to_mmap_store(root: &str) -> Result<Vec<u8>, JsValue> {
    core_dted_tree_to_mmap_store(root).map_err(terrain_store_error)
}

/// Convert a DTED tile tree and write canonical terrain store bytes to a path.
#[wasm_bindgen(js_name = writeDtedTreeToMmapStore)]
pub fn write_dted_tree_to_mmap_store(root: &str, out_path: &str) -> Result<(), JsValue> {
    core_write_dted_tree_to_mmap_store(root, out_path).map_err(terrain_store_error)
}

/// Return an FNV-1a checksum for terrain store bytes.
#[wasm_bindgen(js_name = terrainStoreChecksum64)]
pub fn terrain_store_checksum64(bytes: &[u8]) -> u64 {
    core_terrain_store_checksum64(bytes)
}

/// In-memory reader for memory-mappable terrain store bytes.
///
/// Query results are orthometric terrain heights `H` in metres. Use the
/// ellipsoidal methods only when a geoid model is deliberately selected.
#[wasm_bindgen]
#[derive(Clone)]
pub struct MmapTerrain {
    inner: CoreMmapTerrain<'static>,
}

#[wasm_bindgen]
impl MmapTerrain {
    /// Parse terrain store bytes from a `Uint8Array`.
    #[wasm_bindgen(js_name = fromBytes)]
    pub fn from_bytes(bytes: &[u8]) -> Result<MmapTerrain, JsValue> {
        Self::from_vec(bytes.to_vec())
    }

    /// Parse terrain store bytes from an owned `Uint8Array` copy.
    #[wasm_bindgen(js_name = fromVec)]
    pub fn from_vec(bytes: Vec<u8>) -> Result<MmapTerrain, JsValue> {
        let inner = CoreMmapTerrain::from_vec(bytes).map_err(terrain_store_error)?;
        Ok(Self { inner })
    }

    /// Read a terrain store file from host I/O and parse it into memory.
    ///
    /// Browser runtimes should use [`MmapTerrain.fromBytes`] with fetched bytes.
    #[wasm_bindgen(js_name = fromPath)]
    pub fn from_path(path: &str) -> Result<MmapTerrain, JsValue> {
        let inner = CoreMmapTerrain::from_path(path).map_err(terrain_store_error)?;
        Ok(Self { inner })
    }

    /// Terrain height in ORTHOMETRIC metres at `(longitudeDeg, latitudeDeg)`.
    ///
    /// Longitude and latitude are degrees. The lookup uses bilinear
    /// interpolation. Missing tiles evaluate to `0.0`.
    #[wasm_bindgen(js_name = heightM)]
    pub fn height_m(&mut self, longitude_deg: f64, latitude_deg: f64) -> Result<f64, JsValue> {
        self.inner
            .height_m(longitude_deg, latitude_deg)
            .map_err(engine_error)
    }

    /// Terrain height in ORTHOMETRIC metres at `(longitudeDeg, latitudeDeg)`.
    ///
    /// `options.interpolation` is `"bilinear"`, `"nearest"`, or
    /// `"nearestPosting"`.
    #[wasm_bindgen(js_name = heightMWithOptions)]
    pub fn height_m_with_options(
        &mut self,
        longitude_deg: f64,
        latitude_deg: f64,
        options: JsValue,
    ) -> Result<f64, JsValue> {
        let options = lookup_options(options)?;
        self.inner
            .height_m_with_options(longitude_deg, latitude_deg, options)
            .map_err(engine_error)
    }

    /// Typed ORTHOMETRIC terrain height `H` at `(longitudeDeg, latitudeDeg)`.
    #[wasm_bindgen(js_name = orthometricHeightM)]
    pub fn orthometric_height_m(
        &self,
        longitude_deg: f64,
        latitude_deg: f64,
    ) -> Result<OrthometricHeightM, JsValue> {
        let inner = self
            .inner
            .orthometric_height_m(longitude_deg, latitude_deg)
            .map_err(engine_error)?;
        Ok(OrthometricHeightM { inner })
    }

    /// Typed ORTHOMETRIC terrain height `H` at `(longitudeDeg, latitudeDeg)`
    /// with explicit lookup options.
    #[wasm_bindgen(js_name = orthometricHeightMWithOptions)]
    pub fn orthometric_height_m_with_options(
        &self,
        longitude_deg: f64,
        latitude_deg: f64,
        options: JsValue,
    ) -> Result<OrthometricHeightM, JsValue> {
        let options = lookup_options(options)?;
        let inner = self
            .inner
            .orthometric_height_m_with_options(longitude_deg, latitude_deg, options)
            .map_err(engine_error)?;
        Ok(OrthometricHeightM { inner })
    }

    /// Batch ORTHOMETRIC terrain heights for longitude-first points.
    ///
    /// `points` is an array of `[longitudeDeg, latitudeDeg]` pairs or
    /// `{ longitudeDeg, latitudeDeg }` objects. Each entry is
    /// `{ ok: true, heightM }` or `{ ok: false, error }`.
    #[wasm_bindgen(js_name = heightBatch)]
    pub fn height_batch(&mut self, points: JsValue, options: JsValue) -> Result<JsValue, JsValue> {
        let points = parse_points(points)?;
        let options = lookup_options(options)?;
        let out: Vec<TerrainStoreHeightBatchResult> = self
            .inner
            .height_batch(&points, options)
            .into_iter()
            .map(|result| match result {
                Ok(height_m) => TerrainStoreHeightBatchResult {
                    ok: true,
                    height_m: Some(height_m),
                    error: None,
                },
                Err(err) => TerrainStoreHeightBatchResult {
                    ok: false,
                    height_m: None,
                    error: Some(err.to_string()),
                },
            })
            .collect();
        to_js(&out)
    }

    /// Batch typed ORTHOMETRIC terrain heights for longitude-first points.
    ///
    /// Each entry is `{ ok: true, orthometricHeightM: { valueM } }` or
    /// `{ ok: false, error }`.
    #[wasm_bindgen(js_name = orthometricHeightBatch)]
    pub fn orthometric_height_batch(
        &self,
        points: JsValue,
        options: JsValue,
    ) -> Result<JsValue, JsValue> {
        let points = parse_points(points)?;
        let options = lookup_options(options)?;
        let out: Vec<TerrainStoreOrthometricBatchResult> = self
            .inner
            .orthometric_height_batch(&points, options)
            .into_iter()
            .map(|result| match result {
                Ok(height) => TerrainStoreOrthometricBatchResult {
                    ok: true,
                    orthometric_height_m: Some(OrthometricHeightObject {
                        value_m: height.metres(),
                    }),
                    error: None,
                },
                Err(err) => TerrainStoreOrthometricBatchResult {
                    ok: false,
                    orthometric_height_m: None,
                    error: Some(err.to_string()),
                },
            })
            .collect();
        to_js(&out)
    }

    /// Ellipsoidal height `h = H + N` in metres at `(longitudeDeg, latitudeDeg)`
    /// using the embedded EGM96 1-degree geoid grid.
    #[wasm_bindgen(js_name = ellipsoidalHeightM)]
    pub fn ellipsoidal_height_m(
        &self,
        longitude_deg: f64,
        latitude_deg: f64,
    ) -> Result<EllipsoidalHeightM, JsValue> {
        let inner = self
            .inner
            .ellipsoidal_height_m(longitude_deg, latitude_deg)
            .map_err(terrain_datum_error)?;
        Ok(EllipsoidalHeightM { inner })
    }

    /// Ellipsoidal height `h = H + N` in metres using embedded EGM96 1-degree
    /// geoid conversion and explicit terrain lookup options.
    #[wasm_bindgen(js_name = ellipsoidalHeightMWithOptions)]
    pub fn ellipsoidal_height_m_with_options(
        &self,
        longitude_deg: f64,
        latitude_deg: f64,
        options: JsValue,
    ) -> Result<EllipsoidalHeightM, JsValue> {
        let options = lookup_options(options)?;
        let inner = self
            .inner
            .ellipsoidal_height_m_with_options(longitude_deg, latitude_deg, options)
            .map_err(terrain_datum_error)?;
        Ok(EllipsoidalHeightM { inner })
    }

    /// Ellipsoidal height `h = H + N` in metres using an explicit geoid model.
    ///
    /// The terrain lookup input order is `(longitudeDeg, latitudeDeg)`. Choosing
    /// the EGM96 15-arcminute model requires a loaded `WW15MGH.DAC` grid and
    /// does not fall back to the embedded 1-degree grid.
    #[wasm_bindgen(js_name = ellipsoidalHeightMWithModel)]
    pub fn ellipsoidal_height_m_with_model(
        &self,
        longitude_deg: f64,
        latitude_deg: f64,
        options: JsValue,
        geoid: &TerrainGeoidModel,
    ) -> Result<EllipsoidalHeightM, JsValue> {
        let options = lookup_options(options)?;
        let inner = self
            .inner
            .ellipsoidal_height_m_with_model(longitude_deg, latitude_deg, options, geoid.as_core())
            .map_err(terrain_datum_error)?;
        Ok(EllipsoidalHeightM { inner })
    }

    /// Parsed tile index records in store order.
    #[wasm_bindgen(js_name = tileIndex)]
    pub fn tile_index(&self) -> Vec<TerrainStoreTileIndex> {
        self.inner
            .tile_index()
            .iter()
            .copied()
            .map(|inner| TerrainStoreTileIndex { inner })
            .collect()
    }

    /// File-level vertical datum for the store's orthometric posting payloads.
    #[wasm_bindgen(getter, js_name = verticalDatum)]
    pub fn vertical_datum(&self) -> VerticalDatum {
        datum_from_core(self.inner.vertical_datum())
    }

    /// FNV-1a checksum of the full terrain store byte span.
    #[wasm_bindgen]
    pub fn checksum64(&self) -> u64 {
        self.inner.checksum64()
    }

    /// Canonical store bytes for this parsed terrain store.
    #[wasm_bindgen(js_name = toBytes)]
    pub fn to_bytes(&self) -> Vec<u8> {
        self.inner.to_bytes()
    }
}
