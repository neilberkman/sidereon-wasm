//! IONEX vertical-TEC grid product and its slant ionospheric group-delay query.
//! The parse is `Ionex::parse` and the delay is `ionex_slant_delay`, unchanged.

use std::f64::consts::PI;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::astro::time::civil::{
    j2000_seconds_from_split, split_julian_date_from_j2000_seconds,
};
use sidereon_core::astro::time::model::{Instant, InstantRepr, JulianDateSplit, TimeScale};
use sidereon_core::atmosphere::{
    ionex_slant_delay, Ionex as CoreIonex, TecGridSamples as CoreTecGridSamples,
    TecSample as CoreTecSample, TecSamplesError,
};
use sidereon_core::Wgs84Geodetic;

use crate::error::{engine_error, range_error, require_finite, type_error};

/// pi/180 as a single rounded constant, so a degree boundary conversion is one
/// multiply and one rounding (matches the engine's other language bindings).
const DEG_TO_RAD: f64 = PI / 180.0;

#[derive(Serialize, Deserialize)]
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
    tec_maps: Vec<Vec<Vec<f64>>>,
    #[serde(default)]
    rms_maps: Vec<Vec<Vec<f64>>>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TecSampleJs {
    epoch_j2000_s: f64,
    lat_deg: f64,
    lon_deg: f64,
    vtec_tecu: f64,
    #[serde(default)]
    rms_tecu: Option<f64>,
}

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| engine_error(format!("failed to serialize result: {e}")))
}

fn tec_samples_error(err: TecSamplesError) -> JsValue {
    range_error(&err.to_string())
}

fn j2000_seconds_to_instant(epoch_j2000_s: f64) -> Result<Instant, JsValue> {
    if !epoch_j2000_s.is_finite() {
        return Err(range_error("epochJ2000S must be finite"));
    }
    if epoch_j2000_s.fract() != 0.0 {
        return Err(range_error("epochJ2000S must be an integer second"));
    }
    if epoch_j2000_s < i64::MIN as f64 || epoch_j2000_s > i64::MAX as f64 {
        return Err(range_error("epochJ2000S is outside the supported range"));
    }
    let seconds = epoch_j2000_s as i64;
    let (jd_whole, fraction) = split_julian_date_from_j2000_seconds(seconds);
    let split = JulianDateSplit::new(jd_whole, fraction).map_err(engine_error)?;
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

fn grid_samples_to_core(samples: TecGridSamplesJs) -> Result<CoreTecGridSamples, JsValue> {
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
        rms_maps: samples.rms_maps,
    })
}

fn grid_samples_from_core(samples: CoreTecGridSamples) -> TecGridSamplesJs {
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
        rms_maps: samples.rms_maps,
    }
}

fn node_sample_to_core(sample: TecSampleJs) -> Result<CoreTecSample, JsValue> {
    Ok(CoreTecSample {
        epoch: j2000_seconds_to_instant(sample.epoch_j2000_s)?,
        lat_deg: sample.lat_deg,
        lon_deg: sample.lon_deg,
        vtec_tecu: sample.vtec_tecu,
        rms_tecu: sample.rms_tecu,
    })
}

fn node_sample_from_core(sample: CoreTecSample) -> TecSampleJs {
    TecSampleJs {
        epoch_j2000_s: instant_to_j2000_seconds(sample.epoch),
        lat_deg: sample.lat_deg,
        lon_deg: sample.lon_deg,
        vtec_tecu: sample.vtec_tecu,
        rms_tecu: sample.rms_tecu,
    }
}

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

    /// Signed latitude grid step, degrees. Standard IONEX grids are
    /// north-to-south, so this value is usually negative.
    #[wasm_bindgen(getter, js_name = dlatDeg)]
    pub fn dlat_deg(&self) -> f64 {
        self.inner.dlat_deg()
    }

    /// Signed longitude grid step, degrees. Standard IONEX grids are
    /// west-to-east, so this value is usually positive.
    #[wasm_bindgen(getter, js_name = dlonDeg)]
    pub fn dlon_deg(&self) -> f64 {
        self.inner.dlon_deg()
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

    /// Extract the full IONEX vertical-TEC grids as plain sample data.
    ///
    /// Epochs are integer seconds since J2000. Latitude and longitude nodes and
    /// grid steps are degrees. TEC and RMS maps are TECU and indexed
    /// `[map][iLat][iLon]`. The shell height and base radius are kilometers.
    #[wasm_bindgen(js_name = tecGridSamples)]
    pub fn tec_grid_samples(&self) -> Result<JsValue, JsValue> {
        to_js(&grid_samples_from_core(self.inner.tec_grid_samples()))
    }

    /// Extract one IONEX vertical-TEC sample per grid node.
    ///
    /// Each sample is `{ epochJ2000S, latDeg, lonDeg, vtecTecu, rmsTecu }`.
    /// Epochs are integer seconds since J2000, coordinates are degrees, and TEC
    /// and RMS values are TECU.
    #[wasm_bindgen(js_name = tecSamples)]
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

/// Parse an IONEX vertical-TEC product from the full text content (as bytes).
/// Throws an `Error` on malformed input.
#[wasm_bindgen(js_name = loadIonex)]
pub fn load_ionex(bytes: &[u8]) -> Result<Ionex, JsValue> {
    let inner = CoreIonex::parse(bytes).map_err(engine_error)?;
    Ok(Ionex { inner })
}

/// Build an IONEX vertical-TEC product from full-grid samples.
///
/// `samples.mapEpochsJ2000S` are integer seconds since J2000. Latitude and
/// longitude nodes and `dlatDeg` / `dlonDeg` are degrees. `tecMaps` and
/// `rmsMaps` are TECU and indexed `[map][iLat][iLon]`. The shell height and
/// base radius are kilometers. Validation errors from the core sample builder
/// are thrown as `RangeError`.
#[wasm_bindgen(js_name = ionexFromSamples)]
pub fn ionex_from_samples(samples: JsValue) -> Result<Ionex, JsValue> {
    let samples: TecGridSamplesJs = serde_wasm_bindgen::from_value(samples)
        .map_err(|e| type_error(&format!("invalid IONEX TEC grid samples: {e}")))?;
    let inner =
        CoreIonex::from_samples(grid_samples_to_core(samples)?).map_err(tec_samples_error)?;
    Ok(Ionex { inner })
}

/// Build an IONEX vertical-TEC product from flat node samples.
///
/// `samples` is an array of `{ epochJ2000S, latDeg, lonDeg, vtecTecu,
/// rmsTecu? }` objects. Epochs are integer seconds since J2000, coordinates are
/// degrees, and TEC/RMS values are TECU. `shellHeightKm` and `baseRadiusKm` are
/// kilometers. Validation errors from the core sample builder are thrown as
/// `RangeError`.
#[wasm_bindgen(js_name = ionexFromNodeSamples)]
pub fn ionex_from_node_samples(
    samples: JsValue,
    shell_height_km: f64,
    base_radius_km: f64,
    exponent: i32,
) -> Result<Ionex, JsValue> {
    let samples: Vec<TecSampleJs> = serde_wasm_bindgen::from_value(samples)
        .map_err(|e| type_error(&format!("invalid IONEX TEC node samples: {e}")))?;
    let core_samples = samples
        .into_iter()
        .map(node_sample_to_core)
        .collect::<Result<Vec<_>, _>>()?;
    let inner =
        CoreIonex::from_node_samples(core_samples, shell_height_km, base_radius_km, exponent)
            .map_err(tec_samples_error)?;
    Ok(Ionex { inner })
}
