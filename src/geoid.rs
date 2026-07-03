//! Geoid undulation binding.
//!
//! Thin wrappers over `sidereon_core::geoid`. The free functions resolve against
//! the COARSE built-in 30-degree global grid; [`GeoidGrid`] wraps a real grid
//! (built from samples or parsed from the documented text format) for
//! survey-grade lookups. Heights and undulations are metres, positions radians
//! or degrees as named.

use wasm_bindgen::prelude::*;

use sidereon_core::geoid::{
    egm96_ellipsoidal_height_m as core_egm96_ellipsoidal_height_m,
    egm96_orthometric_height_m as core_egm96_orthometric_height_m,
    egm96_undulation as core_egm96_undulation, egm96_undulations_deg as core_egm96_undulations_deg,
    egm96_undulations_rad as core_egm96_undulations_rad,
    ellipsoidal_height_m as core_ellipsoidal_height_m, geoid_undulation as core_geoid_undulation,
    geoid_undulations_deg as core_geoid_undulations_deg,
    geoid_undulations_rad as core_geoid_undulations_rad,
    orthometric_height_m as core_orthometric_height_m, GeoidGrid as CoreGeoidGrid,
};

use crate::error::{engine_error, type_error};

fn points_from_flat(name: &str, values: &[f64]) -> Result<Vec<(f64, f64)>, JsValue> {
    if !values.len().is_multiple_of(2) {
        return Err(type_error(&format!(
            "{name} length ({}) must be a multiple of 2 (flat lat/lon pairs)",
            values.len()
        )));
    }
    Ok(values
        .chunks_exact(2)
        .map(|pair| (pair[0], pair[1]))
        .collect())
}

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

/// Geoid undulation `N` (metres above the WGS84 ellipsoid) at a geodetic
/// position in radians, from the embedded GENUINE EGM96 1-degree global grid.
/// Latitude is positive north, longitude positive east. This is the recommended
/// zero-setup default for metre-class datum work (its bilinear lookup agrees
/// with the full 15-arcminute EGM96 grid to ~0.4 m RMS); the coarse
/// [`geoidUndulation`] is only suitable for sanity checks. Delegates to
/// `sidereon_core::geoid::egm96_undulation`.
#[wasm_bindgen(js_name = egm96Undulation)]
pub fn egm96_undulation(lat_rad: f64, lon_rad: f64) -> f64 {
    core_egm96_undulation(lat_rad, lon_rad)
}

/// Batch EGM96 undulation lookup for flat `[latRad, lonRad, ...]` pairs.
#[wasm_bindgen(js_name = egm96UndulationsRad)]
pub fn egm96_undulations_rad(points_rad: &[f64]) -> Result<Vec<f64>, JsValue> {
    Ok(core_egm96_undulations_rad(&points_from_flat(
        "pointsRad",
        points_rad,
    )?))
}

/// Batch EGM96 undulation lookup for flat `[latDeg, lonDeg, ...]` pairs.
#[wasm_bindgen(js_name = egm96UndulationsDeg)]
pub fn egm96_undulations_deg(points_deg: &[f64]) -> Result<Vec<f64>, JsValue> {
    Ok(core_egm96_undulations_deg(&points_from_flat(
        "pointsDeg",
        points_deg,
    )?))
}

/// Batch coarse built-in undulation lookup for flat `[latRad, lonRad, ...]`
/// pairs.
#[wasm_bindgen(js_name = geoidUndulationsRad)]
pub fn geoid_undulations_rad(points_rad: &[f64]) -> Result<Vec<f64>, JsValue> {
    Ok(core_geoid_undulations_rad(&points_from_flat(
        "pointsRad",
        points_rad,
    )?))
}

/// Batch coarse built-in undulation lookup for flat `[latDeg, lonDeg, ...]`
/// pairs.
#[wasm_bindgen(js_name = geoidUndulationsDeg)]
pub fn geoid_undulations_deg(points_deg: &[f64]) -> Result<Vec<f64>, JsValue> {
    Ok(core_geoid_undulations_deg(&points_from_flat(
        "pointsDeg",
        points_deg,
    )?))
}

/// Orthometric height `H = h - N` (metres above mean sea level) from an
/// ellipsoidal height and a geodetic position in radians, using the embedded
/// genuine EGM96 1-degree model. Delegates to
/// `sidereon_core::geoid::egm96_orthometric_height_m`.
#[wasm_bindgen(js_name = egm96OrthometricHeightM)]
pub fn egm96_orthometric_height_m(ellipsoidal_height_m: f64, lat_rad: f64, lon_rad: f64) -> f64 {
    core_egm96_orthometric_height_m(ellipsoidal_height_m, lat_rad, lon_rad)
}

/// Ellipsoidal height `h = H + N` (metres above the WGS84 ellipsoid) from an
/// orthometric height and a geodetic position in radians, using the embedded
/// genuine EGM96 1-degree model. Delegates to
/// `sidereon_core::geoid::egm96_ellipsoidal_height_m`.
#[wasm_bindgen(js_name = egm96EllipsoidalHeightM)]
pub fn egm96_ellipsoidal_height_m(orthometric_height_m: f64, lat_rad: f64, lon_rad: f64) -> f64 {
    core_egm96_ellipsoidal_height_m(orthometric_height_m, lat_rad, lon_rad)
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

    /// Parse an EGM96 `WW15MGH.DAC` byte buffer into a full-resolution grid.
    #[wasm_bindgen(js_name = fromEgm96Dac)]
    pub fn from_egm96_dac(bytes: &[u8]) -> Result<GeoidGrid, JsValue> {
        let inner = CoreGeoidGrid::from_egm96_dac(bytes).map_err(engine_error)?;
        Ok(GeoidGrid { inner })
    }

    /// Bilinearly interpolated undulation `N` (metres) at a geodetic position in
    /// radians (latitude positive north, longitude positive east).
    #[wasm_bindgen(js_name = undulationRad)]
    pub fn undulation_rad(&self, lat_rad: f64, lon_rad: f64) -> f64 {
        self.inner.undulation_rad(lat_rad, lon_rad)
    }

    /// Batch undulation lookup for flat `[latRad, lonRad, ...]` pairs.
    #[wasm_bindgen(js_name = undulationsRad)]
    pub fn undulations_rad(&self, points_rad: &[f64]) -> Result<Vec<f64>, JsValue> {
        Ok(self
            .inner
            .undulations_rad(&points_from_flat("pointsRad", points_rad)?))
    }

    /// Bilinearly interpolated undulation `N` (metres) at a geodetic position in
    /// degrees (latitude positive north, longitude positive east).
    #[wasm_bindgen(js_name = undulationDeg)]
    pub fn undulation_deg(&self, lat_deg: f64, lon_deg: f64) -> f64 {
        self.inner.undulation_deg(lat_deg, lon_deg)
    }

    /// Batch undulation lookup for flat `[latDeg, lonDeg, ...]` pairs.
    #[wasm_bindgen(js_name = undulationsDeg)]
    pub fn undulations_deg(&self, points_deg: &[f64]) -> Result<Vec<f64>, JsValue> {
        Ok(self
            .inner
            .undulations_deg(&points_from_flat("pointsDeg", points_deg)?))
    }

    /// Orthometric height from ellipsoidal height and radians.
    #[wasm_bindgen(js_name = orthometricHeightRad)]
    pub fn orthometric_height_rad(
        &self,
        ellipsoidal_height_m: f64,
        lat_rad: f64,
        lon_rad: f64,
    ) -> f64 {
        self.inner
            .orthometric_height_rad(ellipsoidal_height_m, lat_rad, lon_rad)
    }

    /// Ellipsoidal height from orthometric height and radians.
    #[wasm_bindgen(js_name = ellipsoidalHeightRad)]
    pub fn ellipsoidal_height_rad(
        &self,
        orthometric_height_m: f64,
        lat_rad: f64,
        lon_rad: f64,
    ) -> f64 {
        self.inner
            .ellipsoidal_height_rad(orthometric_height_m, lat_rad, lon_rad)
    }

    /// Orthometric height from ellipsoidal height and degrees.
    #[wasm_bindgen(js_name = orthometricHeightDeg)]
    pub fn orthometric_height_deg(
        &self,
        ellipsoidal_height_m: f64,
        lat_deg: f64,
        lon_deg: f64,
    ) -> f64 {
        self.inner
            .orthometric_height_deg(ellipsoidal_height_m, lat_deg, lon_deg)
    }

    /// Ellipsoidal height from orthometric height and degrees.
    #[wasm_bindgen(js_name = ellipsoidalHeightDeg)]
    pub fn ellipsoidal_height_deg(
        &self,
        orthometric_height_m: f64,
        lat_deg: f64,
        lon_deg: f64,
    ) -> f64 {
        self.inner
            .ellipsoidal_height_deg(orthometric_height_m, lat_deg, lon_deg)
    }
}
