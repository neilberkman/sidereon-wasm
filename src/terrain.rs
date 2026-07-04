use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::terrain::{
    DtedInterpolation, DtedLookupOptions, DtedTerrain as CoreDtedTerrain,
};

use crate::error::{engine_error, type_error};

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
    /// interpolation. Missing tiles evaluate to `0.0`, matching the core DTED
    /// fallback.
    #[wasm_bindgen(js_name = heightM)]
    pub fn height_m(&mut self, longitude_deg: f64, latitude_deg: f64) -> Result<f64, JsValue> {
        self.inner
            .height_m(longitude_deg, latitude_deg)
            .map_err(engine_error)
    }

    /// Terrain height in ORTHOMETRIC meters at `(longitudeDeg, latitudeDeg)`.
    ///
    /// Longitude and latitude are degrees. `options.interpolation` is
    /// `"bilinear"`, `"nearest"`, or `"nearestPosting"`. Missing tiles evaluate
    /// to `0.0`, matching the core DTED fallback.
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
        self.inner
            .height_m_with_options(
                longitude_deg,
                latitude_deg,
                DtedLookupOptions {
                    interpolation: interpolation(options.interpolation.as_deref())?,
                },
            )
            .map_err(engine_error)
    }

    /// Batch terrain heights in ORTHOMETRIC meters for longitude-first points.
    ///
    /// `points` is an array of `[longitudeDeg, latitudeDeg]` pairs or
    /// `{ longitudeDeg, latitudeDeg }` objects. `options.interpolation` is
    /// `"bilinear"`, `"nearest"`, or `"nearestPosting"`. The return value is
    /// index-aligned to `points`; each entry is `{ ok: true, heightM }` or
    /// `{ ok: false, error }`. Missing tiles evaluate to `0.0`.
    #[wasm_bindgen(js_name = heightBatch)]
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
        let out: Vec<TerrainBatchResult> = self
            .inner
            .height_batch(
                &core_points,
                DtedLookupOptions {
                    interpolation: interpolation(options.interpolation.as_deref())?,
                },
            )
            .into_iter()
            .map(|result| match result {
                Ok(height_m) => TerrainBatchResult {
                    ok: true,
                    height_m: Some(height_m),
                    error: None,
                },
                Err(err) => TerrainBatchResult {
                    ok: false,
                    height_m: None,
                    error: Some(err.to_string()),
                },
            })
            .collect();
        out.serialize(&serde_wasm_bindgen::Serializer::json_compatible())
            .map_err(|e| engine_error(format!("failed to serialize terrain batch: {e}")))
    }
}
