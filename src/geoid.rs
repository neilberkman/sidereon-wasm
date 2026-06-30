//! Geoid undulation binding.
//!
//! Thin wrappers over `sidereon_core::geoid`. The free functions resolve against
//! the COARSE built-in 30-degree global grid; [`GeoidGrid`] wraps a real grid
//! (built from samples or parsed from the documented text format) for
//! survey-grade lookups. Heights and undulations are metres, positions radians
//! or degrees as named.

use wasm_bindgen::prelude::*;

use sidereon_core::geoid::{
    ellipsoidal_height_m as core_ellipsoidal_height_m, geoid_undulation as core_geoid_undulation,
    orthometric_height_m as core_orthometric_height_m, GeoidGrid as CoreGeoidGrid,
};

use crate::error::engine_error;

/// Geoid undulation `N` (metres above the WGS84 ellipsoid) at a geodetic
/// position in radians, from the COARSE built-in global grid. Latitude is
/// positive north, longitude positive east. Delegates to
/// `sidereon_core::geoid::geoid_undulation`.
#[wasm_bindgen(js_name = geoidUndulation)]
pub fn geoid_undulation(lat_rad: f64, lon_rad: f64) -> f64 {
    core_geoid_undulation(lat_rad, lon_rad)
}

/// Orthometric height `H = h - N` (metres above mean sea level) from an
/// ellipsoidal height and a geodetic position in radians, using the built-in
/// grid's undulation. Delegates to
/// `sidereon_core::geoid::orthometric_height_m`.
#[wasm_bindgen(js_name = orthometricHeightM)]
pub fn orthometric_height_m(ellipsoidal_height_m: f64, lat_rad: f64, lon_rad: f64) -> f64 {
    core_orthometric_height_m(ellipsoidal_height_m, lat_rad, lon_rad)
}

/// Ellipsoidal height `h = H + N` (metres above the WGS84 ellipsoid) from an
/// orthometric height and a geodetic position in radians, using the built-in
/// grid's undulation. Delegates to
/// `sidereon_core::geoid::ellipsoidal_height_m`.
#[wasm_bindgen(js_name = ellipsoidalHeightM)]
pub fn ellipsoidal_height_m(orthometric_height_m: f64, lat_rad: f64, lon_rad: f64) -> f64 {
    core_ellipsoidal_height_m(orthometric_height_m, lat_rad, lon_rad)
}

/// A regular latitude/longitude grid of geoid undulation samples with bilinear
/// interpolation, wrapping a real (loaded) geoid model.
#[wasm_bindgen]
pub struct GeoidGrid {
    inner: CoreGeoidGrid,
}

#[wasm_bindgen]
impl GeoidGrid {
    /// Build a grid from its origin, spacing, dimensions, and row-major samples
    /// (metres, latitude ascending outer, longitude ascending inner). Throws an
    /// `Error` when a dimension is zero, the sample count is not `nLat * nLon`, a
    /// spacing/origin is non-finite or a spacing is non-positive, or a sample is
    /// non-finite. Delegates to `sidereon_core::geoid::GeoidGrid::new`.
    #[wasm_bindgen(constructor)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        lat_min_deg: f64,
        lon_min_deg: f64,
        dlat_deg: f64,
        dlon_deg: f64,
        n_lat: usize,
        n_lon: usize,
        values_m: Vec<f64>,
    ) -> Result<GeoidGrid, JsValue> {
        let inner = CoreGeoidGrid::new(
            lat_min_deg,
            lon_min_deg,
            dlat_deg,
            dlon_deg,
            n_lat,
            n_lon,
            values_m,
        )
        .map_err(engine_error)?;
        Ok(GeoidGrid { inner })
    }

    /// Parse a grid from the documented whitespace-delimited text format (a
    /// six-field header `lat_min lon_min dlat dlon n_lat n_lon` followed by
    /// `n_lat * n_lon` samples in metres). Throws an `Error` on a malformed
    /// header or sample. Delegates to
    /// `sidereon_core::geoid::GeoidGrid::from_text`.
    #[wasm_bindgen(js_name = fromText)]
    pub fn from_text(text: &str) -> Result<GeoidGrid, JsValue> {
        let inner = CoreGeoidGrid::from_text(text).map_err(engine_error)?;
        Ok(GeoidGrid { inner })
    }

    /// Bilinearly interpolated undulation `N` (metres) at a geodetic position in
    /// radians (latitude positive north, longitude positive east).
    #[wasm_bindgen(js_name = undulationRad)]
    pub fn undulation_rad(&self, lat_rad: f64, lon_rad: f64) -> f64 {
        self.inner.undulation_rad(lat_rad, lon_rad)
    }

    /// Bilinearly interpolated undulation `N` (metres) at a geodetic position in
    /// degrees (latitude positive north, longitude positive east).
    #[wasm_bindgen(js_name = undulationDeg)]
    pub fn undulation_deg(&self, lat_deg: f64, lon_deg: f64) -> f64 {
        self.inner.undulation_deg(lat_deg, lon_deg)
    }
}
