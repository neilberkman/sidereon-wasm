//! Geoid undulation binding.
//!
//! Thin wrappers over `sidereon_core::geoid`. The free functions resolve against
//! the COARSE built-in 30-degree global grid; [`GeoidGrid`] wraps a real grid
//! (built from samples or parsed from the documented text format) for
//! survey-grade lookups. Heights and undulations are metres, positions radians
//! or degrees as named. PROJ EGM96 GTX lookups require an explicit arithmetic
//! recipe because conforming PROJ builds can differ by one ULP.

use serde::Serialize;
use wasm_bindgen::prelude::*;

use sidereon_core::geoid::{
    egm96_ellipsoidal_height_m as core_egm96_ellipsoidal_height_m,
    egm96_orthometric_height_m as core_egm96_orthometric_height_m,
    egm96_undulation as core_egm96_undulation, egm96_undulations_deg as core_egm96_undulations_deg,
    egm96_undulations_rad as core_egm96_undulations_rad,
    ellipsoidal_height_m as core_ellipsoidal_height_m, geoid_undulation as core_geoid_undulation,
    geoid_undulations_deg as core_geoid_undulations_deg,
    geoid_undulations_rad as core_geoid_undulations_rad,
    orthometric_height_m as core_orthometric_height_m,
    Egm2008GridSpacing as CoreEgm2008GridSpacing, Egm2008RasterWindow, GeoidGrid as CoreGeoidGrid,
    ProjVgridshiftArithmetic as CoreProjVgridshiftArithmetic,
    ProjVgridshiftError as CoreProjVgridshiftError,
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

/// EGM2008 raster spacing for NGA row-framed interpolation grids.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Egm2008GridSpacing {
    /// The 1-arcminute EGM2008 grid.
    OneMinute,
    /// The 2.5-arcminute EGM2008 grid.
    TwoPointFiveMinute,
}

impl From<Egm2008GridSpacing> for CoreEgm2008GridSpacing {
    fn from(spacing: Egm2008GridSpacing) -> Self {
        match spacing {
            Egm2008GridSpacing::OneMinute => Self::OneMinute,
            Egm2008GridSpacing::TwoPointFiveMinute => Self::TwoPointFiveMinute,
        }
    }
}

/// Floating-point evaluation recipe for PROJ vertical-grid interpolation.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjVgridshiftArithmetic {
    /// Round every multiplication and addition separately. This matches the
    /// reviewed x86-64 PROJ 9.3.0 build with contraction disabled.
    SeparateMultiplyAdd,
    /// Evaluate each accumulation with a fused multiply-add and one rounding.
    /// This matches the contracted AArch64 PROJ 9.3.0 reference build.
    FusedMultiplyAdd,
}

impl From<ProjVgridshiftArithmetic> for CoreProjVgridshiftArithmetic {
    fn from(arithmetic: ProjVgridshiftArithmetic) -> Self {
        match arithmetic {
            ProjVgridshiftArithmetic::SeparateMultiplyAdd => Self::SeparateMultiplyAdd,
            ProjVgridshiftArithmetic::FusedMultiplyAdd => Self::FusedMultiplyAdd,
        }
    }
}

/// Error category reported by PROJ vertical-grid coordinate lookup.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjVgridshiftError {
    /// Latitude or longitude was not finite.
    NonFiniteCoordinate,
    /// Latitude or longitude was outside the loaded grid.
    CoordinateOutsideGrid,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjVgridshiftErrorDetail {
    name: &'static str,
    message: String,
    coordinate: &'static str,
}

fn proj_vgridshift_error(error: CoreProjVgridshiftError) -> JsValue {
    let (kind, coordinate) = match error {
        CoreProjVgridshiftError::NonFiniteCoordinate { field } => {
            (ProjVgridshiftError::NonFiniteCoordinate, field)
        }
        CoreProjVgridshiftError::CoordinateOutsideGrid { field } => {
            (ProjVgridshiftError::CoordinateOutsideGrid, field)
        }
    };
    let name = match kind {
        ProjVgridshiftError::NonFiniteCoordinate => "NonFiniteCoordinate",
        ProjVgridshiftError::CoordinateOutsideGrid => "CoordinateOutsideGrid",
    };
    let detail = ProjVgridshiftErrorDetail {
        name,
        message: error.to_string(),
        coordinate,
    };
    let js_error = js_sys::RangeError::new(&detail.message);
    js_error.set_name(name);
    let value: JsValue = js_error.into();
    js_sys::Reflect::set(&value, &JsValue::from_str("kind"), &JsValue::from_str(name))
        .expect("attach PROJ vertical-grid error kind");
    js_sys::Reflect::set(
        &value,
        &JsValue::from_str("coordinate"),
        &JsValue::from_str(coordinate),
    )
    .expect("attach PROJ vertical-grid error coordinate");
    let detail_value = serde_wasm_bindgen::to_value(&detail)
        .expect("serialize typed PROJ vertical-grid error detail");
    js_sys::Reflect::set(&value, &JsValue::from_str("detail"), &detail_value)
        .expect("attach typed PROJ vertical-grid error detail");
    value
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

    /// Parse PROJ's public EGM96 15-arcminute `egm96_15.gtx` byte stream.
    /// Use `undulationProjRad` with an explicit arithmetic recipe.
    #[wasm_bindgen(js_name = fromProjEgm96Gtx)]
    pub fn from_proj_egm96_gtx(bytes: &[u8]) -> Result<GeoidGrid, JsValue> {
        let inner = CoreGeoidGrid::from_proj_egm96_gtx(bytes).map_err(engine_error)?;
        Ok(GeoidGrid { inner })
    }

    /// Parse a full NGA EGM2008 row-framed interpolation raster.
    ///
    /// The byte stream must contain one north-to-south Fortran sequential record
    /// per latitude row at the requested spacing. Use
    /// [`fromEgm2008RasterWindow`] for small cropped fixtures or partial loads.
    #[wasm_bindgen(js_name = fromEgm2008Raster)]
    pub fn from_egm2008_raster(
        bytes: &[u8],
        spacing: Egm2008GridSpacing,
    ) -> Result<GeoidGrid, JsValue> {
        let inner =
            CoreGeoidGrid::from_egm2008_raster(bytes, spacing.into()).map_err(engine_error)?;
        Ok(GeoidGrid { inner })
    }

    /// Parse a cropped NGA EGM2008 row-framed interpolation raster window.
    ///
    /// `latMinDeg` and `lonMinDeg` are the southwest node of the loaded grid.
    /// `nLat` and `nLon` are the latitude and longitude node counts in the byte
    /// stream. Queries use the same undulation methods as every other
    /// [`GeoidGrid`].
    #[wasm_bindgen(js_name = fromEgm2008RasterWindow)]
    pub fn from_egm2008_raster_window(
        bytes: &[u8],
        spacing: Egm2008GridSpacing,
        lat_min_deg: f64,
        lon_min_deg: f64,
        n_lat: usize,
        n_lon: usize,
    ) -> Result<GeoidGrid, JsValue> {
        let window =
            Egm2008RasterWindow::new(spacing.into(), lat_min_deg, lon_min_deg, n_lat, n_lon)
                .map_err(engine_error)?;
        let inner =
            CoreGeoidGrid::from_egm2008_raster_window(bytes, window).map_err(engine_error)?;
        Ok(GeoidGrid { inner })
    }

    /// Bilinearly interpolated undulation `N` (metres) at a geodetic position in
    /// radians (latitude positive north, longitude positive east).
    #[wasm_bindgen(js_name = undulationRad)]
    pub fn undulation_rad(&self, lat_rad: f64, lon_rad: f64) -> f64 {
        self.inner.undulation_rad(lat_rad, lon_rad)
    }

    /// Evaluate PROJ 9.3.0-compatible vertical-grid interpolation at geodetic
    /// radians. The caller must select separate or fused multiply/add behavior;
    /// no platform-dependent default is inferred. Invalid coordinates throw a
    /// `RangeError` with machine-readable `kind`, `coordinate`, and `detail`
    /// properties.
    #[wasm_bindgen(js_name = undulationProjRad)]
    pub fn undulation_proj_rad(
        &self,
        lat_rad: f64,
        lon_rad: f64,
        arithmetic: ProjVgridshiftArithmetic,
    ) -> Result<f64, JsValue> {
        self.inner
            .undulation_proj_rad(lat_rad, lon_rad, arithmetic.into())
            .map_err(proj_vgridshift_error)
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
