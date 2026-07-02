use serde::Deserialize;
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
pub struct DtedTerrain {
    inner: CoreDtedTerrain,
}

#[wasm_bindgen]
impl DtedTerrain {
    #[wasm_bindgen(constructor)]
    pub fn new(root: &str) -> DtedTerrain {
        DtedTerrain {
            inner: CoreDtedTerrain::new(root),
        }
    }

    #[wasm_bindgen(js_name = heightM)]
    pub fn height_m(&mut self, longitude_deg: f64, latitude_deg: f64) -> Result<f64, JsValue> {
        self.inner
            .height_m(longitude_deg, latitude_deg)
            .map_err(engine_error)
    }

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
}
