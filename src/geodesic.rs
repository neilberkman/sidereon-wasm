//! WGS84 geodesic direct and inverse binding.
//!
//! The numerical work is delegated to `sidereon_core::geodesic`, which uses
//! Karney's algorithm on WGS84. This module only names the JS fields and maps
//! invalid input to `RangeError`.

use serde::Serialize;
use wasm_bindgen::prelude::*;

use sidereon_core::geodesic::{
    geodesic_direct as core_geodesic_direct, geodesic_inverse as core_geodesic_inverse,
    GeodesicError as CoreGeodesicError,
};

use crate::error::{engine_error, range_error};

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| engine_error(format!("failed to serialize result: {e}")))
}

fn geodesic_error(error: CoreGeodesicError) -> JsValue {
    match error {
        CoreGeodesicError::InvalidInput { .. } => range_error(&error.to_string()),
    }
}

/// WGS84 geodesic input failure class exposed as a stable enum symbol.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GeodesicError {
    /// A latitude, longitude, azimuth, or distance was outside the accepted
    /// numeric domain.
    InvalidInput,
}

/// Stable string label for a [`GeodesicError`] enum value.
#[wasm_bindgen(js_name = geodesicErrorLabel)]
pub fn geodesic_error_label(error: GeodesicError) -> String {
    match error {
        GeodesicError::InvalidInput => "InvalidInput",
    }
    .to_string()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeodesicInverseJs {
    distance_m: f64,
    initial_azimuth_deg: f64,
    final_azimuth_deg: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeodesicDirectJs {
    lat2_deg: f64,
    lon2_deg: f64,
    final_azimuth_deg: f64,
}

/// Solve the WGS84 inverse geodesic problem.
///
/// Inputs are point 1 latitude and longitude followed by point 2 latitude and
/// longitude, all in degrees. Longitudes may be outside `[-180, 180]`. Returns
/// `{ distanceM, initialAzimuthDeg, finalAzimuthDeg }`, with distance in metres
/// and azimuths in degrees. Delegates to
/// `sidereon_core::geodesic::geodesic_inverse`.
#[wasm_bindgen(js_name = geodesicInverse)]
pub fn geodesic_inverse(
    lat1_deg: f64,
    lon1_deg: f64,
    lat2_deg: f64,
    lon2_deg: f64,
) -> Result<JsValue, JsValue> {
    let (distance_m, initial_azimuth_deg, final_azimuth_deg) =
        core_geodesic_inverse(lat1_deg, lon1_deg, lat2_deg, lon2_deg).map_err(geodesic_error)?;
    to_js(&GeodesicInverseJs {
        distance_m,
        initial_azimuth_deg,
        final_azimuth_deg,
    })
}

/// Solve the WGS84 direct geodesic problem.
///
/// Inputs are point 1 latitude, longitude, forward azimuth, and geodesic
/// distance. Angles are degrees and `distanceM` is metres. Returns
/// `{ lat2Deg, lon2Deg, finalAzimuthDeg }`. Delegates to
/// `sidereon_core::geodesic::geodesic_direct`.
#[wasm_bindgen(js_name = geodesicDirect)]
pub fn geodesic_direct(
    lat1_deg: f64,
    lon1_deg: f64,
    initial_azimuth_deg: f64,
    distance_m: f64,
) -> Result<JsValue, JsValue> {
    let (lat2_deg, lon2_deg, final_azimuth_deg) =
        core_geodesic_direct(lat1_deg, lon1_deg, initial_azimuth_deg, distance_m)
            .map_err(geodesic_error)?;
    to_js(&GeodesicDirectJs {
        lat2_deg,
        lon2_deg,
        final_azimuth_deg,
    })
}
