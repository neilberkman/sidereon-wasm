//! RINEX navigation parsing: typed broadcast records, broadcast-orbit/clock
//! evaluation, GLONASS state vectors, header ionosphere coefficients, and the
//! default broadcast ephemeris store. Mirrors the core `rinex::nav` surface.

use wasm_bindgen::prelude::*;

use sidereon_core::ephemeris::{
    is_beidou_geo, satellite_state, BroadcastEphemeris as CoreBroadcastStore, BroadcastRecord,
    ClockPolynomial, GlonassRecord, IonoCorrections, KeplerianElements, KlobucharAlphaBeta,
    SatelliteState,
};
use sidereon_core::rinex::nav::{
    encode_nav, parse_glonass, parse_iono_corrections, parse_leap_seconds, parse_nav,
};

use crate::error::{engine_error, range_error, utf8_text};

/// Which supported RINEX NAV message a broadcast record carries.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NavMessage {
    /// GPS legacy LNAV.
    GpsLnav,
    /// Galileo I/NAV.
    GalileoInav,
    /// Galileo F/NAV.
    GalileoFnav,
    /// BeiDou D1.
    BeidouD1,
    /// BeiDou D2.
    BeidouD2,
}

fn map_nav_message(message: sidereon_core::ephemeris::NavMessage) -> NavMessage {
    use sidereon_core::ephemeris::NavMessage as Core;
    match message {
        Core::GpsLnav => NavMessage::GpsLnav,
        Core::GalileoInav => NavMessage::GalileoInav,
        Core::GalileoFnav => NavMessage::GalileoFnav,
        Core::BeidouD1 => NavMessage::BeidouD1,
        Core::BeidouD2 => NavMessage::BeidouD2,
    }
}

/// Stable lower-case selector for a NAV message, e.g. `"gps_lnav"`.
#[wasm_bindgen(js_name = navMessageLabel)]
pub fn nav_message_label(message: NavMessage) -> String {
    match message {
        NavMessage::GpsLnav => "gps_lnav",
        NavMessage::GalileoInav => "galileo_inav",
        NavMessage::GalileoFnav => "galileo_fnav",
        NavMessage::BeidouD1 => "beidou_d1",
        NavMessage::BeidouD2 => "beidou_d2",
    }
    .to_string()
}

/// Broadcast Keplerian orbital elements. Units are SI: angles in radians,
/// correction terms in radians or metres, `toeSow` in seconds of week.
#[wasm_bindgen]
pub struct KeplerianElementsJs {
    inner: KeplerianElements,
}

#[wasm_bindgen]
impl KeplerianElementsJs {
    #[wasm_bindgen(getter, js_name = sqrtA)]
    pub fn sqrt_a(&self) -> f64 {
        self.inner.sqrt_a
    }
    #[wasm_bindgen(getter)]
    pub fn e(&self) -> f64 {
        self.inner.e
    }
    #[wasm_bindgen(getter)]
    pub fn m0(&self) -> f64 {
        self.inner.m0
    }
    #[wasm_bindgen(getter, js_name = deltaN)]
    pub fn delta_n(&self) -> f64 {
        self.inner.delta_n
    }
    #[wasm_bindgen(getter)]
    pub fn omega0(&self) -> f64 {
        self.inner.omega0
    }
    #[wasm_bindgen(getter)]
    pub fn i0(&self) -> f64 {
        self.inner.i0
    }
    #[wasm_bindgen(getter)]
    pub fn omega(&self) -> f64 {
        self.inner.omega
    }
    #[wasm_bindgen(getter, js_name = omegaDot)]
    pub fn omega_dot(&self) -> f64 {
        self.inner.omega_dot
    }
    #[wasm_bindgen(getter)]
    pub fn idot(&self) -> f64 {
        self.inner.idot
    }
    #[wasm_bindgen(getter)]
    pub fn cuc(&self) -> f64 {
        self.inner.cuc
    }
    #[wasm_bindgen(getter)]
    pub fn cus(&self) -> f64 {
        self.inner.cus
    }
    #[wasm_bindgen(getter)]
    pub fn crc(&self) -> f64 {
        self.inner.crc
    }
    #[wasm_bindgen(getter)]
    pub fn crs(&self) -> f64 {
        self.inner.crs
    }
    #[wasm_bindgen(getter)]
    pub fn cic(&self) -> f64 {
        self.inner.cic
    }
    #[wasm_bindgen(getter)]
    pub fn cis(&self) -> f64 {
        self.inner.cis
    }
    #[wasm_bindgen(getter, js_name = toeSow)]
    pub fn toe_sow(&self) -> f64 {
        self.inner.toe_sow
    }
}

/// Broadcast satellite-clock polynomial. `af0`/`af1`/`af2` are s, s/s, s/s^2;
/// `tocSow` is seconds of week.
#[wasm_bindgen]
pub struct ClockPolynomialJs {
    inner: ClockPolynomial,
}

#[wasm_bindgen]
impl ClockPolynomialJs {
    #[wasm_bindgen(getter)]
    pub fn af0(&self) -> f64 {
        self.inner.af0
    }
    #[wasm_bindgen(getter)]
    pub fn af1(&self) -> f64 {
        self.inner.af1
    }
    #[wasm_bindgen(getter)]
    pub fn af2(&self) -> f64 {
        self.inner.af2
    }
    #[wasm_bindgen(getter, js_name = tocSow)]
    pub fn toc_sow(&self) -> f64 {
        self.inner.toc_sow
    }
}

/// Evaluated broadcast orbit and satellite clock at one seconds-of-week epoch.
#[wasm_bindgen]
pub struct BroadcastEvaluation {
    inner: SatelliteState,
    t_sow_s: f64,
}

#[wasm_bindgen]
impl BroadcastEvaluation {
    /// Query epoch, seconds of week in the record's broadcast time scale.
    #[wasm_bindgen(getter, js_name = tSowS)]
    pub fn t_sow_s(&self) -> f64 {
        self.t_sow_s
    }

    /// ITRF/ECEF satellite position `[x, y, z]`, metres.
    #[wasm_bindgen(getter, js_name = positionM)]
    pub fn position_m(&self) -> Result<Vec<f64>, JsValue> {
        let position = self.inner.orbit.position().map_err(engine_error)?;
        Ok(position.as_array().to_vec())
    }

    /// ITRF/ECEF X, metres.
    #[wasm_bindgen(getter, js_name = xM)]
    pub fn x_m(&self) -> f64 {
        self.inner.orbit.x_m
    }

    /// ITRF/ECEF Y, metres.
    #[wasm_bindgen(getter, js_name = yM)]
    pub fn y_m(&self) -> f64 {
        self.inner.orbit.y_m
    }

    /// ITRF/ECEF Z, metres.
    #[wasm_bindgen(getter, js_name = zM)]
    pub fn z_m(&self) -> f64 {
        self.inner.orbit.z_m
    }

    /// Total satellite clock offset, seconds.
    #[wasm_bindgen(getter, js_name = clockS)]
    pub fn clock_s(&self) -> f64 {
        self.inner.clock.dt_clock_total_s
    }

    /// Broadcast clock-polynomial component, seconds.
    #[wasm_bindgen(getter, js_name = clockPolynomialS)]
    pub fn clock_polynomial_s(&self) -> f64 {
        self.inner.clock.dt_clock_poly_s
    }

    /// Relativistic eccentricity clock component, seconds.
    #[wasm_bindgen(getter, js_name = relativisticClockS)]
    pub fn relativistic_clock_s(&self) -> f64 {
        self.inner.clock.dt_rel_s
    }

    /// Broadcast group delay subtracted from the clock offset, seconds.
    #[wasm_bindgen(getter, js_name = groupDelayS)]
    pub fn group_delay_s(&self) -> f64 {
        self.inner.clock.tgd_s
    }

    /// Fixed-point iterations used to solve Kepler's equation.
    #[wasm_bindgen(getter, js_name = keplerIterations)]
    pub fn kepler_iterations(&self) -> usize {
        self.inner.orbit.kepler_iterations
    }
}

/// One GPS, Galileo, or BeiDou broadcast ephemeris record from RINEX NAV.
#[wasm_bindgen]
pub struct BroadcastRecordJs {
    inner: BroadcastRecord,
}

#[wasm_bindgen]
impl BroadcastRecordJs {
    /// RINEX satellite token such as `"G01"`.
    #[wasm_bindgen(getter)]
    pub fn satellite(&self) -> String {
        self.inner.satellite_id.to_string()
    }

    /// Broadcast message type.
    #[wasm_bindgen(getter)]
    pub fn message(&self) -> NavMessage {
        map_nav_message(self.inner.message)
    }

    /// Continuous constellation week number from the broadcast record.
    #[wasm_bindgen(getter)]
    pub fn week(&self) -> u32 {
        self.inner.week
    }

    /// Keplerian orbital elements in SI units.
    #[wasm_bindgen(getter)]
    pub fn elements(&self) -> KeplerianElementsJs {
        KeplerianElementsJs {
            inner: self.inner.elements,
        }
    }

    /// Satellite clock polynomial.
    #[wasm_bindgen(getter)]
    pub fn clock(&self) -> ClockPolynomialJs {
        ClockPolynomialJs {
            inner: self.inner.clock,
        }
    }

    /// Broadcast group delay, seconds.
    #[wasm_bindgen(getter, js_name = groupDelayS)]
    pub fn group_delay_s(&self) -> f64 {
        self.inner.broadcast_clock_group_delay_s()
    }

    /// Satellite health word; 0 is healthy for nominal GPS/Galileo.
    #[wasm_bindgen(getter, js_name = svHealth)]
    pub fn sv_health(&self) -> f64 {
        self.inner.sv_health
    }

    /// Signal-in-space accuracy, metres.
    #[wasm_bindgen(getter, js_name = svAccuracyM)]
    pub fn sv_accuracy_m(&self) -> f64 {
        self.inner.sv_accuracy_m
    }

    /// GPS curve-fit interval, seconds, or `undefined` when not broadcast.
    #[wasm_bindgen(getter, js_name = fitIntervalS)]
    pub fn fit_interval_s(&self) -> Option<f64> {
        self.inner.fit_interval_s
    }

    /// Evaluate the broadcast record at a seconds-of-week epoch. `tSowS` is in
    /// the record's broadcast time scale. Throws a `RangeError` on a non-finite
    /// epoch and an `Error` if the orbit cannot be evaluated.
    pub fn evaluate(&self, t_sow_s: f64) -> Result<BroadcastEvaluation, JsValue> {
        if !t_sow_s.is_finite() {
            return Err(range_error("tSowS must be a finite number"));
        }
        let inner = satellite_state(
            &self.inner.elements,
            &self.inner.clock,
            &self.inner.constants(),
            t_sow_s,
            self.inner.broadcast_clock_group_delay_s(),
            is_beidou_geo(self.inner.satellite_id),
        )
        .map_err(engine_error)?;
        Ok(BroadcastEvaluation { inner, t_sow_s })
    }
}

/// One GLONASS broadcast state-vector record. `toeUtcJ2000S` is UTC seconds past
/// J2000; position is PZ-90.11 ECEF metres, velocity m/s, acceleration m/s^2.
#[wasm_bindgen]
pub struct GlonassRecordJs {
    inner: GlonassRecord,
}

#[wasm_bindgen]
impl GlonassRecordJs {
    /// RINEX satellite token such as `"R10"`.
    #[wasm_bindgen(getter)]
    pub fn satellite(&self) -> String {
        self.inner.satellite_id.to_string()
    }

    /// Reference epoch, UTC seconds past J2000.
    #[wasm_bindgen(getter, js_name = toeUtcJ2000S)]
    pub fn toe_utc_j2000_s(&self) -> f64 {
        self.inner.toe_utc_j2000_s
    }

    /// PZ-90.11 ECEF position `[x, y, z]`, metres.
    #[wasm_bindgen(getter, js_name = positionM)]
    pub fn position_m(&self) -> Vec<f64> {
        self.inner.pos_m.to_vec()
    }

    /// PZ-90.11 ECEF velocity `[vx, vy, vz]`, metres per second.
    #[wasm_bindgen(getter, js_name = velocityMS)]
    pub fn velocity_m_s(&self) -> Vec<f64> {
        self.inner.vel_m_s.to_vec()
    }

    /// Lunisolar acceleration `[ax, ay, az]`, metres per second squared.
    #[wasm_bindgen(getter, js_name = accelerationMS2)]
    pub fn acceleration_m_s2(&self) -> Vec<f64> {
        self.inner.acc_m_s2.to_vec()
    }

    /// Broadcast clock bias, seconds.
    #[wasm_bindgen(getter, js_name = clockBiasS)]
    pub fn clock_bias_s(&self) -> f64 {
        self.inner.clk_bias
    }

    /// Relative frequency offset.
    #[wasm_bindgen(getter, js_name = gammaN)]
    pub fn gamma_n(&self) -> f64 {
        self.inner.gamma_n
    }

    /// Satellite health; 0 is healthy.
    #[wasm_bindgen(getter, js_name = svHealth)]
    pub fn sv_health(&self) -> f64 {
        self.inner.sv_health
    }

    /// FDMA frequency-channel number.
    #[wasm_bindgen(getter, js_name = freqChannel)]
    pub fn freq_channel(&self) -> i32 {
        self.inner.freq_channel
    }
}

/// Klobuchar alpha and beta ionosphere coefficients (each a 4-element array).
#[wasm_bindgen]
pub struct KlobucharAlphaBetaJs {
    inner: KlobucharAlphaBeta,
}

#[wasm_bindgen]
impl KlobucharAlphaBetaJs {
    /// Alpha coefficients as a `Float64Array` of length 4.
    #[wasm_bindgen(getter)]
    pub fn alpha(&self) -> Vec<f64> {
        self.inner.alpha.to_vec()
    }

    /// Beta coefficients as a `Float64Array` of length 4.
    #[wasm_bindgen(getter)]
    pub fn beta(&self) -> Vec<f64> {
        self.inner.beta.to_vec()
    }
}

/// Broadcast ionosphere coefficients parsed from a RINEX NAV header.
#[wasm_bindgen]
pub struct IonoCorrectionsJs {
    inner: IonoCorrections,
}

#[wasm_bindgen]
impl IonoCorrectionsJs {
    /// GPS Klobuchar coefficients, if the header has GPSA and GPSB.
    #[wasm_bindgen(getter)]
    pub fn gps(&self) -> Option<KlobucharAlphaBetaJs> {
        self.inner.gps.map(|inner| KlobucharAlphaBetaJs { inner })
    }

    /// BeiDou Klobuchar coefficients, if the header has BDSA and BDSB.
    #[wasm_bindgen(getter)]
    pub fn beidou(&self) -> Option<KlobucharAlphaBetaJs> {
        self.inner
            .beidou
            .map(|inner| KlobucharAlphaBetaJs { inner })
    }
}

/// A parsed broadcast ephemeris store from a RINEX NAV file. `records` are the
/// usable GPS/Galileo/BeiDou records selected by the core default SPP policy.
#[wasm_bindgen]
pub struct BroadcastEphemeris {
    pub(crate) inner: CoreBroadcastStore,
    leap_seconds: Option<f64>,
}

#[wasm_bindgen]
impl BroadcastEphemeris {
    /// Usable GPS, Galileo, and BeiDou broadcast records in file order.
    #[wasm_bindgen(getter)]
    pub fn records(&self) -> Vec<BroadcastRecordJs> {
        self.inner
            .records()
            .iter()
            .copied()
            .map(|inner| BroadcastRecordJs { inner })
            .collect()
    }

    /// Healthy GLONASS broadcast records in file order.
    #[wasm_bindgen(getter, js_name = glonassRecords)]
    pub fn glonass_records(&self) -> Vec<GlonassRecordJs> {
        self.inner
            .glonass_records()
            .iter()
            .copied()
            .map(|inner| GlonassRecordJs { inner })
            .collect()
    }

    /// Broadcast ionosphere coefficients parsed from the NAV header.
    #[wasm_bindgen(getter, js_name = ionoCorrections)]
    pub fn iono_corrections(&self) -> IonoCorrectionsJs {
        IonoCorrectionsJs {
            inner: self.inner.iono_corrections(),
        }
    }

    /// GPS minus UTC leap seconds from the NAV header, or `undefined`.
    #[wasm_bindgen(getter, js_name = leapSeconds)]
    pub fn leap_seconds(&self) -> Option<f64> {
        self.leap_seconds
    }

    /// Number of usable GPS, Galileo, and BeiDou records.
    #[wasm_bindgen(getter, js_name = recordCount)]
    pub fn record_count(&self) -> usize {
        self.inner.records().len()
    }

    /// Number of usable GLONASS records.
    #[wasm_bindgen(getter, js_name = glonassRecordCount)]
    pub fn glonass_record_count(&self) -> usize {
        self.inner.glonass_records().len()
    }

    /// Serialize the usable GPS, Galileo, and BeiDou broadcast records to
    /// standard RINEX 3 navigation text. Deterministic: the same record set
    /// always produces byte-identical text, and re-parsing the output yields the
    /// same records. GLONASS state-vector records are not part of the Keplerian
    /// broadcast-orbit grammar this writer emits and are therefore not included.
    #[wasm_bindgen(js_name = toRinexString)]
    pub fn to_rinex_string(&self) -> String {
        encode_nav(self.inner.records())
    }
}

/// Parse RINEX NAV bytes into the default broadcast ephemeris store. Throws a
/// `TypeError` on non-UTF-8 input and an `Error` on a parse failure.
#[wasm_bindgen(js_name = parseRinexNav)]
pub fn parse_rinex_nav(bytes: &[u8]) -> Result<BroadcastEphemeris, JsValue> {
    let text = utf8_text(bytes, "RINEX NAV source")?;
    let inner = CoreBroadcastStore::from_nav(&text).map_err(engine_error)?;
    let leap_seconds = parse_leap_seconds(&text).map_err(engine_error)?;
    Ok(BroadcastEphemeris {
        inner,
        leap_seconds,
    })
}

/// Alias of [`parseRinexNav`] for callers that read a file as bytes.
#[wasm_bindgen(js_name = loadRinexNav)]
pub fn load_rinex_nav(bytes: &[u8]) -> Result<BroadcastEphemeris, JsValue> {
    parse_rinex_nav(bytes)
}

/// Parse all supported GPS, Galileo, and BeiDou broadcast records from NAV
/// bytes, before the store's default health and message-policy filter.
#[wasm_bindgen(js_name = parseRinexNavRecords)]
pub fn parse_rinex_nav_records(bytes: &[u8]) -> Result<Vec<BroadcastRecordJs>, JsValue> {
    let text = utf8_text(bytes, "RINEX NAV source")?;
    Ok(parse_nav(&text)
        .map_err(engine_error)?
        .into_iter()
        .map(|inner| BroadcastRecordJs { inner })
        .collect())
}

/// Parse all GLONASS state-vector records from RINEX NAV bytes.
#[wasm_bindgen(js_name = parseRinexGlonassRecords)]
pub fn parse_rinex_glonass_records(bytes: &[u8]) -> Result<Vec<GlonassRecordJs>, JsValue> {
    let text = utf8_text(bytes, "RINEX NAV source")?;
    Ok(parse_glonass(&text)
        .map_err(engine_error)?
        .into_iter()
        .map(|inner| GlonassRecordJs { inner })
        .collect())
}

/// Parse GPS and BeiDou Klobuchar coefficients from a RINEX NAV header.
#[wasm_bindgen(js_name = parseRinexIonoCorrections)]
pub fn parse_rinex_iono_corrections(bytes: &[u8]) -> Result<IonoCorrectionsJs, JsValue> {
    let text = utf8_text(bytes, "RINEX NAV source")?;
    Ok(IonoCorrectionsJs {
        inner: parse_iono_corrections(&text).map_err(engine_error)?,
    })
}

/// Parse the NAV header GPS minus UTC leap seconds, or `undefined`.
#[wasm_bindgen(js_name = parseRinexLeapSeconds)]
pub fn parse_rinex_leap_seconds(bytes: &[u8]) -> Result<Option<f64>, JsValue> {
    let text = utf8_text(bytes, "RINEX NAV source")?;
    parse_leap_seconds(&text).map_err(engine_error)
}
