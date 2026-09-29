//! DTED terrain lookup binding.
//!
//! Heights are orthometric terrain elevations in metres. A lookup that the
//! engine cannot answer with a height (an unknown elevation where a posting
//! holds the DTED null, a tile on a horizontal datum other than WGS84, a
//! coordinate out of range) is refused with a `TerrainLookupError` whose
//! `detail` names the tile and posting, never returned as a number.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::terrain::{
    DtedHorizontalDatum, DtedInterpolation, DtedLookupOptions, DtedTerrain as CoreDtedTerrain,
};

use crate::error::{error_with_detail, type_error};

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct TerrainOptionsInput {
    interpolation: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum TerrainPointInput {
    Pair([f64; 2]),
    Object(TerrainPointObjectInput),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TerrainPointObjectInput {
    longitude_deg: f64,
    latitude_deg: f64,
}

impl TerrainPointInput {
    fn lon_lat(&self) -> (f64, f64) {
        match self {
            Self::Pair([longitude_deg, latitude_deg]) => (*longitude_deg, *latitude_deg),
            Self::Object(point) => (point.longitude_deg, point.latitude_deg),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TerrainBatchResult {
    ok: bool,
    height_m: Option<f64>,
    error: Option<String>,
    detail: Option<TerrainLookupErrorDetailJs>,
}

/// The horizontal datum a DTED tile's DSI record states.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind")]
pub(crate) enum DtedHorizontalDatumJs {
    #[serde(rename = "WGS84")]
    Wgs84,
    #[serde(rename = "WGS72")]
    Wgs72,
    /// The field is blank or zero-filled.
    #[serde(rename = "UNSTATED")]
    Unstated,
    /// Any other field content, as read.
    #[serde(rename = "OTHER")]
    Other { text: String },
}

impl From<&DtedHorizontalDatum> for DtedHorizontalDatumJs {
    fn from(datum: &DtedHorizontalDatum) -> Self {
        match datum {
            DtedHorizontalDatum::Wgs84 => Self::Wgs84,
            DtedHorizontalDatum::Wgs72 => Self::Wgs72,
            DtedHorizontalDatum::Unstated => Self::Unstated,
            DtedHorizontalDatum::Other(text) => Self::Other { text: text.clone() },
            // `DtedHorizontalDatum` is `#[non_exhaustive]`; a datum this
            // binding does not yet name crosses with the engine's own text.
            other => Self::Other {
                text: other.to_string(),
            },
        }
    }
}

/// Why a terrain lookup gave no height, as the `detail` of the thrown
/// `TerrainLookupError` and of a failed batch entry.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind")]
pub(crate) enum TerrainLookupErrorDetailJs {
    /// The lookup gives nonzero weight to a posting holding the DTED null
    /// value, so the query has no height.
    #[serde(rename = "UNKNOWN_TERRAIN_ELEVATION", rename_all = "camelCase")]
    UnknownTerrainElevation {
        lat_index: i32,
        lon_index: i32,
        latitude_posting: usize,
        longitude_posting: usize,
        message: String,
    },
    /// The tile states a horizontal datum other than WGS84.
    #[serde(rename = "NON_WGS84_TERRAIN_TILE", rename_all = "camelCase")]
    NonWgs84TerrainTile {
        lat_index: i32,
        lon_index: i32,
        datum: DtedHorizontalDatumJs,
        message: String,
    },
    /// No tile covers the query.
    #[serde(rename = "MISSING_TERRAIN_TILE", rename_all = "camelCase")]
    MissingTerrainTile {
        lat_index: i32,
        lon_index: i32,
        message: String,
    },
    /// A coordinate out of range or not finite.
    #[serde(rename = "INVALID_INPUT", rename_all = "camelCase")]
    InvalidInput { reason: String, message: String },
    /// Tile or store bytes that do not read.
    #[serde(rename = "PARSE", rename_all = "camelCase")]
    Parse { reason: String, message: String },
    /// Any other engine error, with its message in full.
    #[serde(rename = "UNKNOWN", rename_all = "camelCase")]
    Unknown { message: String },
}

impl TerrainLookupErrorDetailJs {
    pub(crate) fn from_core(err: &sidereon_core::Error) -> Self {
        let message = err.to_string();
        match err {
            sidereon_core::Error::UnknownTerrainElevation {
                lat_index,
                lon_index,
                latitude_posting,
                longitude_posting,
            } => Self::UnknownTerrainElevation {
                lat_index: *lat_index,
                lon_index: *lon_index,
                latitude_posting: *latitude_posting,
                longitude_posting: *longitude_posting,
                message,
            },
            sidereon_core::Error::NonWgs84TerrainTile {
                lat_index,
                lon_index,
                datum,
            } => Self::NonWgs84TerrainTile {
                lat_index: *lat_index,
                lon_index: *lon_index,
                datum: datum.into(),
                message,
            },
            sidereon_core::Error::MissingTerrainTile {
                lat_index,
                lon_index,
            } => Self::MissingTerrainTile {
                lat_index: *lat_index,
                lon_index: *lon_index,
                message,
            },
            sidereon_core::Error::InvalidInput(reason) => Self::InvalidInput {
                reason: reason.clone(),
                message,
            },
            sidereon_core::Error::Parse(reason) => Self::Parse {
                reason: reason.clone(),
                message,
            },
            _ => Self::Unknown { message },
        }
    }
}

/// A terrain lookup failure as an `Error` named `TerrainLookupError` whose
/// `detail` is a `TerrainLookupErrorDetail`.
pub(crate) fn terrain_lookup_error(err: sidereon_core::Error) -> JsValue {
    let detail = TerrainLookupErrorDetailJs::from_core(&err);
    error_with_detail("TerrainLookupError", &err.to_string(), &detail)
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

#[wasm_bindgen]
/// A DTED terrain tile cache rooted at a directory of DTED Level 2 files.
///
/// Heights are ORTHOMETRIC terrain elevations in meters. Point order is always
/// longitude first, then latitude, both in degrees.
pub struct DtedTerrain {
    inner: CoreDtedTerrain,
}

#[wasm_bindgen]
impl DtedTerrain {
    /// Create a DTED terrain reader rooted at `root`.
    ///
    /// The root may contain tile files directly or the nested block layout the
    /// core reader recognizes. Height results are ORTHOMETRIC meters.
    #[wasm_bindgen(constructor)]
    pub fn new(root: &str) -> DtedTerrain {
        DtedTerrain {
            inner: CoreDtedTerrain::new(root),
        }
    }

    /// Terrain height in ORTHOMETRIC meters at `(longitudeDeg, latitudeDeg)`.
    ///
    /// Longitude and latitude are degrees. The lookup uses bilinear
    /// interpolation. A missing tile evaluates to `0.0`, the core DTED
    /// fallback. A lookup that weights a null posting, or a tile whose DSI
    /// states a datum other than WGS84, throws a `TerrainLookupError`.
    #[wasm_bindgen(js_name = heightM)]
    pub fn height_m(&mut self, longitude_deg: f64, latitude_deg: f64) -> Result<f64, JsValue> {
        self.inner
            .height_m(longitude_deg, latitude_deg)
            .map_err(terrain_lookup_error)
    }

    /// Terrain height in ORTHOMETRIC meters at `(longitudeDeg, latitudeDeg)`.
    ///
    /// Longitude and latitude are degrees. `options.interpolation` is
    /// `"bilinear"`, `"nearest"`, or `"nearestPosting"`. A missing tile
    /// evaluates to `0.0`. A null posting given nonzero weight is refused as
    /// `UNKNOWN_TERRAIN_ELEVATION` unless a neighbouring tile knows the height
    /// at the same place; a query exactly on a known posting next to a null
    /// returns that posting.
    #[wasm_bindgen(js_name = heightMWithOptions)]
    pub fn height_m_with_options(
        &mut self,
        longitude_deg: f64,
        latitude_deg: f64,
        options: JsValue,
    ) -> Result<f64, JsValue> {
        let options: TerrainOptionsInput = if options.is_undefined() || options.is_null() {
            TerrainOptionsInput::default()
        } else {
            serde_wasm_bindgen::from_value(options)
                .map_err(|e| type_error(&format!("invalid terrain options: {e}")))?
        };
        let mut lookup_opts = DtedLookupOptions::default();
        lookup_opts.interpolation = interpolation(options.interpolation.as_deref())?;
        self.inner
            .height_m_with_options(longitude_deg, latitude_deg, lookup_opts)
            .map_err(terrain_lookup_error)
    }

    /// Batch terrain heights in ORTHOMETRIC meters for longitude-first points.
    ///
    /// `points` is an array of `[longitudeDeg, latitudeDeg]` pairs or
    /// `{ longitudeDeg, latitudeDeg }` objects. `options.interpolation` is
    /// `"bilinear"`, `"nearest"`, or `"nearestPosting"`. The return value is
    /// index-aligned to `points`; each entry is
    /// `{ ok: true, heightM, error: null, detail: null }` or
    /// `{ ok: false, heightM: null, error, detail }` with `detail` a
    /// `TerrainLookupErrorDetail`. Missing tiles evaluate to `0.0`.
    #[wasm_bindgen(js_name = heightBatch, unchecked_return_type = "TerrainHeightBatchEntry[]")]
    pub fn height_batch(&mut self, points: JsValue, options: JsValue) -> Result<JsValue, JsValue> {
        let points: Vec<TerrainPointInput> = serde_wasm_bindgen::from_value(points)
            .map_err(|e| type_error(&format!("invalid terrain points: {e}")))?;
        let options: TerrainOptionsInput = if options.is_undefined() || options.is_null() {
            TerrainOptionsInput::default()
        } else {
            serde_wasm_bindgen::from_value(options)
                .map_err(|e| type_error(&format!("invalid terrain options: {e}")))?
        };
        let core_points: Vec<(f64, f64)> = points.iter().map(TerrainPointInput::lon_lat).collect();
        let mut lookup_opts = DtedLookupOptions::default();
        lookup_opts.interpolation = interpolation(options.interpolation.as_deref())?;
        let out: Vec<TerrainBatchResult> = self
            .inner
            .height_batch(&core_points, lookup_opts)
            .into_iter()
            .map(|result| match result {
                Ok(height_m) => TerrainBatchResult {
                    ok: true,
                    height_m: Some(height_m),
                    error: None,
                    detail: None,
                },
                Err(err) => TerrainBatchResult {
                    ok: false,
                    height_m: None,
                    error: Some(err.to_string()),
                    detail: Some(TerrainLookupErrorDetailJs::from_core(&err)),
                },
            })
            .collect();
        crate::error::to_plain_js(&out, "terrain batch")
    }
}

// The terrain lookup refusal shapes shared by `DtedTerrain` and `MmapTerrain`.
// `wasm-pack` writes them into both `sidereon.d.ts` targets;
// `types/sidereon-extra.d.ts` re-exports them.
#[wasm_bindgen(typescript_custom_section)]
const TS_TERRAIN_DEFINITIONS: &str = r#"
/** The horizontal datum a DTED tile's DSI record states. */
export type DtedHorizontalDatum =
  | { kind: "WGS84" }
  | { kind: "WGS72" }
  | { kind: "UNSTATED" }
  | { kind: "OTHER"; text: string };

/**
 * Why a terrain lookup gave no height: the `detail` of a thrown
 * `TerrainLookupError` and of a failed batch entry. Posting indices are
 * zero-based within the tile.
 */
export type TerrainLookupErrorDetail =
  | {
      kind: "UNKNOWN_TERRAIN_ELEVATION";
      latIndex: number;
      lonIndex: number;
      latitudePosting: number;
      longitudePosting: number;
      message: string;
    }
  | {
      kind: "NON_WGS84_TERRAIN_TILE";
      latIndex: number;
      lonIndex: number;
      datum: DtedHorizontalDatum;
      message: string;
    }
  | { kind: "MISSING_TERRAIN_TILE"; latIndex: number; lonIndex: number; message: string }
  | { kind: "INVALID_INPUT"; reason: string; message: string }
  | { kind: "PARSE"; reason: string; message: string }
  | { kind: "UNKNOWN"; message: string };

/** One entry of a terrain height batch, index-aligned to the points. */
export type TerrainHeightBatchEntry =
  | { ok: true; heightM: number; error: null; detail: null }
  | { ok: false; heightM: null; error: string; detail: TerrainLookupErrorDetail };
"#;
