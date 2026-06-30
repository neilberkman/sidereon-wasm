//! RINEX clock parsing and satellite clock-bias interpolation. Mirrors the core
//! `rinex::clock` surface: per-satellite bias series and GPS-time interpolation.

use wasm_bindgen::prelude::*;

use sidereon_core::rinex::clock::{
    civil_to_gps_seconds, ClockEpoch as CoreClockEpoch, ClockPoint as CoreClockPoint,
    RinexClock as CoreRinexClock,
};

use crate::error::{engine_error, range_error, utf8_text};

/// A civil GPS-time epoch for RINEX clock interpolation. Calendar fields are
/// interpreted in GPS time; `gpsSeconds` is seconds since 1980-01-06 GPST.
#[wasm_bindgen]
pub struct ClockEpoch {
    inner: CoreClockEpoch,
    gps_seconds: f64,
}

#[wasm_bindgen]
impl ClockEpoch {
    /// Build a GPS-time civil epoch. Throws a `RangeError` for invalid fields.
    #[wasm_bindgen(constructor)]
    pub fn new(
        year: i32,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: f64,
    ) -> Result<ClockEpoch, JsValue> {
        let gps_seconds = civil_to_gps_seconds(year, month, day, hour, minute, second)
            .ok_or_else(|| range_error("invalid GPS-time clock epoch fields"))?;
        Ok(ClockEpoch {
            inner: CoreClockEpoch {
                year,
                month,
                day,
                hour,
                minute,
                second,
            },
            gps_seconds,
        })
    }

    #[wasm_bindgen(getter)]
    pub fn year(&self) -> i32 {
        self.inner.year
    }
    #[wasm_bindgen(getter)]
    pub fn month(&self) -> u8 {
        self.inner.month
    }
    #[wasm_bindgen(getter)]
    pub fn day(&self) -> u8 {
        self.inner.day
    }
    #[wasm_bindgen(getter)]
    pub fn hour(&self) -> u8 {
        self.inner.hour
    }
    #[wasm_bindgen(getter)]
    pub fn minute(&self) -> u8 {
        self.inner.minute
    }
    #[wasm_bindgen(getter)]
    pub fn second(&self) -> f64 {
        self.inner.second
    }

    /// Seconds since the GPS epoch, 1980-01-06 00:00:00 GPST.
    #[wasm_bindgen(getter, js_name = gpsSeconds)]
    pub fn gps_seconds(&self) -> f64 {
        self.gps_seconds
    }
}

/// Per-satellite RINEX clock-bias samples. `gpsSeconds` and `biasS` are
/// row-aligned `Float64Array`s; epochs are GPS seconds, biases are seconds.
#[wasm_bindgen]
pub struct ClockSeries {
    satellite: String,
    gps_seconds: Vec<f64>,
    bias_s: Vec<f64>,
}

impl ClockSeries {
    fn from_points(satellite: String, points: &[CoreClockPoint]) -> Self {
        let mut gps_seconds = Vec::with_capacity(points.len());
        let mut bias_s = Vec::with_capacity(points.len());
        for point in points {
            if let Some(seconds) = point.gps_seconds() {
                gps_seconds.push(seconds);
                bias_s.push(point.bias_s);
            }
        }
        Self {
            satellite,
            gps_seconds,
            bias_s,
        }
    }
}

#[wasm_bindgen]
impl ClockSeries {
    /// RINEX satellite token such as `"G05"`.
    #[wasm_bindgen(getter)]
    pub fn satellite(&self) -> String {
        self.satellite.clone()
    }

    /// Sample times, GPS seconds, as a `Float64Array`.
    #[wasm_bindgen(getter, js_name = gpsSeconds)]
    pub fn gps_seconds(&self) -> Vec<f64> {
        self.gps_seconds.clone()
    }

    /// Satellite clock-bias samples, seconds, as a `Float64Array`.
    #[wasm_bindgen(getter, js_name = biasS)]
    pub fn bias_s(&self) -> Vec<f64> {
        self.bias_s.clone()
    }

    /// Number of clock samples for this satellite.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> usize {
        self.bias_s.len()
    }
}

/// A parsed RINEX clock product with satellite clock-bias interpolation.
#[wasm_bindgen]
pub struct RinexClock {
    inner: CoreRinexClock,
}

#[wasm_bindgen]
impl RinexClock {
    /// Satellite tokens with at least one parsed `AS` clock sample.
    #[wasm_bindgen(getter)]
    pub fn satellites(&self) -> Vec<String> {
        self.inner.series.keys().cloned().collect()
    }

    /// Per-satellite clock-bias series in satellite sort order.
    #[wasm_bindgen(getter)]
    pub fn series(&self) -> Vec<ClockSeries> {
        self.inner
            .series
            .iter()
            .map(|(satellite, points)| ClockSeries::from_points(satellite.clone(), points))
            .collect()
    }

    /// Number of satellites with clock samples.
    #[wasm_bindgen(getter, js_name = satelliteCount)]
    pub fn satellite_count(&self) -> usize {
        self.inner.series.len()
    }

    /// Total number of parsed satellite clock samples.
    #[wasm_bindgen(getter, js_name = sampleCount)]
    pub fn sample_count(&self) -> usize {
        self.inner.series.values().map(Vec::len).sum()
    }

    /// One satellite's clock series, or `undefined` if the satellite is absent.
    #[wasm_bindgen(js_name = seriesFor)]
    pub fn series_for(&self, satellite_id: &str) -> Option<ClockSeries> {
        self.inner
            .series
            .get(satellite_id)
            .map(|points| ClockSeries::from_points(satellite_id.to_string(), points))
    }

    /// Interpolate one satellite clock bias at a GPS-time civil epoch.
    /// Returns `undefined` if the satellite or epoch coverage is absent.
    #[wasm_bindgen(js_name = clockS)]
    pub fn clock_s(&self, satellite_id: &str, epoch: &ClockEpoch) -> Result<Option<f64>, JsValue> {
        self.inner
            .clock_s(satellite_id, epoch.inner)
            .map_err(engine_error)
    }

    /// Interpolate one satellite clock bias at GPS seconds. Throws a
    /// `RangeError` if `gpsSeconds` is non-finite.
    #[wasm_bindgen(js_name = clockSAtGpsSeconds)]
    pub fn clock_s_at_gps_seconds(
        &self,
        satellite_id: &str,
        gps_seconds: f64,
    ) -> Result<Option<f64>, JsValue> {
        if !gps_seconds.is_finite() {
            return Err(range_error("gpsSeconds must be a finite number"));
        }
        self.inner
            .clock_s_at_gps_seconds(satellite_id, gps_seconds)
            .map_err(engine_error)
    }

    /// Serialize to standard RINEX clock text. Deterministic: the same product
    /// always produces byte-identical text, and re-parsing the output reproduces
    /// the same time scale and per-satellite bias series.
    #[wasm_bindgen(js_name = toRinexString)]
    pub fn to_rinex_string(&self) -> String {
        self.inner.to_rinex_string()
    }
}

/// Strictly parse RINEX clock bytes into satellite clock-bias series. Throws a
/// `TypeError` on non-UTF-8 input and an `Error` on a malformed record.
#[wasm_bindgen(js_name = parseRinexClock)]
pub fn parse_rinex_clock(bytes: &[u8]) -> Result<RinexClock, JsValue> {
    let text = utf8_text(bytes, "RINEX clock source")?;
    Ok(RinexClock {
        inner: CoreRinexClock::parse(&text).map_err(engine_error)?,
    })
}

/// Alias of [`parseRinexClock`] for callers that read a file as bytes.
#[wasm_bindgen(js_name = loadRinexClock)]
pub fn load_rinex_clock(bytes: &[u8]) -> Result<RinexClock, JsValue> {
    parse_rinex_clock(bytes)
}

/// Parse RINEX clock bytes, skipping malformed and non-`AS` rows. Throws a
/// `TypeError` only on non-UTF-8 input.
#[wasm_bindgen(js_name = parseRinexClockLossy)]
pub fn parse_rinex_clock_lossy(bytes: &[u8]) -> Result<RinexClock, JsValue> {
    let text = utf8_text(bytes, "RINEX clock source")?;
    Ok(RinexClock {
        inner: CoreRinexClock::parse_lossy(&text),
    })
}

/// Alias of [`parseRinexClockLossy`] for callers that read a file as bytes.
#[wasm_bindgen(js_name = loadRinexClockLossy)]
pub fn load_rinex_clock_lossy(bytes: &[u8]) -> Result<RinexClock, JsValue> {
    parse_rinex_clock_lossy(bytes)
}
