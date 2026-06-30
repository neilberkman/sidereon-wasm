//! IONEX vertical-TEC grid product and its slant ionospheric group-delay query.
//! The parse is `Ionex::parse` and the delay is `ionex_slant_delay`, unchanged.

use std::f64::consts::PI;

use wasm_bindgen::prelude::*;

use sidereon_core::atmosphere::{ionex_slant_delay, Ionex as CoreIonex};
use sidereon_core::Wgs84Geodetic;

use crate::error::{engine_error, range_error, require_finite};

/// pi/180 as a single rounded constant, so a degree boundary conversion is one
/// multiply and one rounding (matches the engine's other language bindings).
const DEG_TO_RAD: f64 = PI / 180.0;

/// A parsed IONEX vertical-TEC product. Create with [`load_ionex`].
#[wasm_bindgen]
pub struct Ionex {
    pub(crate) inner: CoreIonex,
}

/// IONEX slant ionospheric group delay from degree-valued geometry, shared by
/// [`Ionex.slantDelay`] and the staleness-selected `IonexSelection.slantDelay`
/// so a selected product evaluates bit-for-bit identically to the product the
/// caller parsed. Delegates to the reference `ionex_slant_delay`.
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
    if frequency_hz <= 0.0 {
        return Err(range_error("frequencyHz must be positive"));
    }

    let receiver = Wgs84Geodetic::new(lat_deg * DEG_TO_RAD, lon_deg * DEG_TO_RAD, 0.0)
        .map_err(|e| range_error(&e.to_string()))?;
    ionex_slant_delay(
        inner,
        receiver,
        elevation_deg * DEG_TO_RAD,
        azimuth_deg * DEG_TO_RAD,
        epoch_j2000_s as i64,
        frequency_hz,
    )
    .map_err(engine_error)
}

#[wasm_bindgen]
impl Ionex {
    /// Latitude node values, degrees, as a `Float64Array`, descending
    /// (north-to-south).
    #[wasm_bindgen(getter, js_name = latNodesDeg)]
    pub fn lat_nodes_deg(&self) -> Vec<f64> {
        self.inner.lat_nodes_deg().to_vec()
    }

    /// Longitude node values, degrees, as a `Float64Array`, ascending
    /// (west-to-east).
    #[wasm_bindgen(getter, js_name = lonNodesDeg)]
    pub fn lon_nodes_deg(&self) -> Vec<f64> {
        self.inner.lon_nodes_deg().to_vec()
    }

    /// Single-layer shell height, kilometres.
    #[wasm_bindgen(getter, js_name = shellHeightKm)]
    pub fn shell_height_km(&self) -> f64 {
        self.inner.shell_height_km()
    }

    /// Mean Earth radius used by the geometry, kilometres.
    #[wasm_bindgen(getter, js_name = baseRadiusKm)]
    pub fn base_radius_km(&self) -> f64 {
        self.inner.base_radius_km()
    }

    /// The IONEX `EXPONENT` header field; the TEC scale is `10^exponent`.
    #[wasm_bindgen(getter)]
    pub fn exponent(&self) -> i32 {
        self.inner.exponent()
    }

    /// Map epochs as seconds since J2000, ascending, as a `Float64Array`. This
    /// is the exact axis [`Ionex.slantDelay`] brackets against.
    #[wasm_bindgen(getter, js_name = mapEpochsJ2000S)]
    pub fn map_epochs_j2000_s(&self) -> Vec<f64> {
        self.inner
            .map_epochs_s()
            .into_iter()
            .map(|s| s as f64)
            .collect()
    }

    /// IONEX slant ionospheric group delay, positive metres.
    ///
    /// Receiver latitude/longitude and satellite azimuth/elevation are degrees
    /// (latitude positive north, longitude positive east, azimuth clockwise
    /// from north). The pierce point rides on the IONEX shell, so no receiver
    /// height enters. `epochJ2000S` is an integer number of seconds since J2000.
    /// `frequencyHz` is the carrier the dispersive delay is reported on. Throws
    /// a `RangeError` on non-finite input and an `Error` on out-of-range input.
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

    /// Serialize to standard IONEX text. Deterministic: the same product always
    /// produces byte-identical text, and re-parsing the output yields an equal
    /// product (the canonical node axes, geometry, exponent, map epochs, and
    /// every TEC/RMS value).
    #[wasm_bindgen(js_name = toIonexString)]
    pub fn to_ionex_string(&self) -> String {
        self.inner.to_ionex_string()
    }
}

/// Parse an IONEX vertical-TEC product from the full text content (as bytes).
/// Throws an `Error` on malformed input.
#[wasm_bindgen(js_name = loadIonex)]
pub fn load_ionex(bytes: &[u8]) -> Result<Ionex, JsValue> {
    let inner = CoreIonex::parse(bytes).map_err(engine_error)?;
    Ok(Ionex { inner })
}
