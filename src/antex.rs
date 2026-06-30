//! ANTEX antenna-calibration binding: parse an ANTEX 1.4 product and read
//! receiver / satellite phase-center offsets (PCO) and variations (PCV).
//!
//! Parsing and interpolation are entirely `sidereon_core::antex`; this module is
//! a thin value wrapper. PCO/PCV are exposed in metres.

use wasm_bindgen::prelude::*;

use sidereon_core::antex::{
    Antenna as CoreAntenna, AntennaKind, Antex as CoreAntex, AntexDateTime as CoreAntexDateTime,
};

use crate::error::{engine_error, range_error, utf8_text};

fn kind_label(kind: AntennaKind) -> &'static str {
    match kind {
        AntennaKind::Receiver => "receiver",
        AntennaKind::Satellite => "satellite",
    }
}

/// A civil UTC-like ANTEX validity timestamp.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct AntexDateTime {
    inner: CoreAntexDateTime,
}

#[wasm_bindgen]
impl AntexDateTime {
    /// Create an ANTEX validity timestamp. `hour` / `minute` / `second` default
    /// to 0. Throws a `RangeError` on an invalid date.
    #[wasm_bindgen(constructor)]
    pub fn new(
        year: i32,
        month: u8,
        day: u8,
        hour: Option<u8>,
        minute: Option<u8>,
        second: Option<u8>,
    ) -> Result<AntexDateTime, JsValue> {
        CoreAntexDateTime::new(
            year,
            month,
            day,
            hour.unwrap_or(0),
            minute.unwrap_or(0),
            second.unwrap_or(0),
        )
        .map(|inner| AntexDateTime { inner })
        .map_err(|_| range_error("invalid ANTEX datetime"))
    }

    /// Calendar year.
    #[wasm_bindgen(getter)]
    pub fn year(&self) -> i32 {
        self.inner.year
    }

    /// Calendar month, 1..=12.
    #[wasm_bindgen(getter)]
    pub fn month(&self) -> u8 {
        self.inner.month
    }

    /// Calendar day of month.
    #[wasm_bindgen(getter)]
    pub fn day(&self) -> u8 {
        self.inner.day
    }

    /// Hour of day.
    #[wasm_bindgen(getter)]
    pub fn hour(&self) -> u8 {
        self.inner.hour
    }

    /// Minute of hour.
    #[wasm_bindgen(getter)]
    pub fn minute(&self) -> u8 {
        self.inner.minute
    }

    /// Second of minute.
    #[wasm_bindgen(getter)]
    pub fn second(&self) -> u8 {
        self.inner.second
    }
}

/// A receiver or satellite ANTEX antenna calibration block.
#[wasm_bindgen]
pub struct Antenna {
    inner: CoreAntenna,
}

#[wasm_bindgen]
impl Antenna {
    /// ANTEX `TYPE / SERIAL` id.
    #[wasm_bindgen(getter)]
    pub fn id(&self) -> String {
        self.inner.id.clone()
    }

    /// Block role: `"receiver"` or `"satellite"`.
    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> String {
        kind_label(self.inner.kind).to_string()
    }

    /// ANTEX antenna type field.
    #[wasm_bindgen(getter, js_name = antennaType)]
    pub fn antenna_type(&self) -> String {
        self.inner.antenna_type.clone()
    }

    /// ANTEX serial, PRN, or radome field.
    #[wasm_bindgen(getter)]
    pub fn serial(&self) -> String {
        self.inner.serial.clone()
    }

    /// Azimuth grid spacing, degrees.
    #[wasm_bindgen(getter, js_name = daziDeg)]
    pub fn dazi_deg(&self) -> f64 {
        self.inner.dazi_deg
    }

    /// Zenith grid start angle, degrees.
    #[wasm_bindgen(getter, js_name = zenithStartDeg)]
    pub fn zenith_start_deg(&self) -> f64 {
        self.inner.zenith_start_deg
    }

    /// Zenith grid end angle, degrees.
    #[wasm_bindgen(getter, js_name = zenithEndDeg)]
    pub fn zenith_end_deg(&self) -> f64 {
        self.inner.zenith_end_deg
    }

    /// Zenith grid step, degrees.
    #[wasm_bindgen(getter, js_name = zenithStepDeg)]
    pub fn zenith_step_deg(&self) -> f64 {
        self.inner.zenith_step_deg
    }

    /// Optional SINEX calibration code.
    #[wasm_bindgen(getter, js_name = sinexCode)]
    pub fn sinex_code(&self) -> Option<String> {
        self.inner.sinex_code.clone()
    }

    /// First valid timestamp, or `undefined` for an open-ended block.
    #[wasm_bindgen(getter, js_name = validFrom)]
    pub fn valid_from(&self) -> Option<AntexDateTime> {
        self.inner.valid_from.map(|inner| AntexDateTime { inner })
    }

    /// Last valid timestamp, or `undefined` for an open-ended block.
    #[wasm_bindgen(getter, js_name = validUntil)]
    pub fn valid_until(&self) -> Option<AntexDateTime> {
        self.inner.valid_until.map(|inner| AntexDateTime { inner })
    }

    /// Available frequency codes, e.g. `"G01"`.
    #[wasm_bindgen(getter)]
    pub fn frequencies(&self) -> Vec<String> {
        self.inner.frequencies.keys().cloned().collect()
    }

    /// Whether this antenna block is valid at `epoch`.
    #[wasm_bindgen(js_name = validAt)]
    pub fn valid_at(&self, epoch: &AntexDateTime) -> bool {
        self.inner.valid_at(epoch.inner)
    }

    /// Frequency-dependent phase-center offset, north/east/up metres, as a
    /// length-3 `Float64Array`.
    pub fn pco(&self, frequency: &str) -> Result<Vec<f64>, JsValue> {
        self.inner
            .pco(frequency)
            .map(|p| p.to_vec())
            .map_err(engine_error)
    }

    /// Frequency-dependent phase-center variation, metres. `azimuthDeg` is
    /// optional; without it the no-azimuth interpolation is used.
    pub fn pcv(
        &self,
        frequency: &str,
        zenith_deg: f64,
        azimuth_deg: Option<f64>,
    ) -> Result<f64, JsValue> {
        if !zenith_deg.is_finite() {
            return Err(range_error("zenith_deg must be finite"));
        }
        if let Some(az) = azimuth_deg {
            if !az.is_finite() {
                return Err(range_error("azimuth_deg must be finite"));
            }
        }
        self.inner
            .pcv(frequency, zenith_deg, azimuth_deg)
            .map_err(engine_error)
    }
}

/// A parsed ANTEX receiver and satellite antenna calibration product.
#[wasm_bindgen]
pub struct Antex {
    inner: CoreAntex,
}

#[wasm_bindgen]
impl Antex {
    /// Number of antenna blocks parsed from the product.
    #[wasm_bindgen(getter, js_name = antennaCount)]
    pub fn antenna_count(&self) -> usize {
        self.inner.antennas.len()
    }

    /// ANTEX `TYPE / SERIAL` ids in deterministic order.
    #[wasm_bindgen(getter, js_name = antennaIds)]
    pub fn antenna_ids(&self) -> Vec<String> {
        self.inner.antennas.keys().cloned().collect()
    }

    /// Return an antenna by exact `TYPE / SERIAL` id, or `undefined`.
    pub fn antenna(&self, id: &str) -> Option<Antenna> {
        self.inner
            .antenna(id)
            .cloned()
            .map(|inner| Antenna { inner })
    }

    /// Return the satellite antenna for `prn` valid at `epoch`, or `undefined`.
    #[wasm_bindgen(js_name = satelliteAntenna)]
    pub fn satellite_antenna(&self, prn: &str, epoch: &AntexDateTime) -> Option<Antenna> {
        self.inner
            .satellite_antenna(prn, epoch.inner)
            .cloned()
            .map(|inner| Antenna { inner })
    }

    /// Serialize to standard ANTEX 1.4 text. Deterministic: the same product
    /// always produces byte-identical text, and re-parsing the output yields an
    /// equal product.
    #[wasm_bindgen(js_name = toAntexString)]
    pub fn to_antex_string(&self) -> String {
        self.inner.encode()
    }
}

/// Parse an ANTEX 1.4 antenna product from in-memory bytes (a `Uint8Array`).
/// Throws an `Error` on a parse failure or non-UTF-8 input.
#[wasm_bindgen(js_name = loadAntex)]
pub fn load_antex(bytes: &[u8]) -> Result<Antex, JsValue> {
    let text = utf8_text(bytes, "ANTEX")?;
    let inner = CoreAntex::parse(&text).map_err(engine_error)?;
    Ok(Antex { inner })
}
