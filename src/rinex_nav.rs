//! RINEX navigation parsing: typed broadcast records, broadcast-orbit/clock
//! evaluation, GLONASS state vectors, header ionosphere coefficients, and the
//! default broadcast ephemeris store. Mirrors the core `rinex::nav` surface.

use wasm_bindgen::prelude::*;

use sidereon_core::ephemeris::{
    cnav_ura_ned_m as core_cnav_ura_ned_m, cnav_ura_nominal_m as core_cnav_ura_nominal_m,
    is_beidou_geo, satellite_state, BroadcastEphemeris as CoreBroadcastStore,
    BroadcastGroupDelayTerm, BroadcastGroupDelays, BroadcastRecord, ClockPolynomial,
    CnavParameters, CnavSignal as CoreCnavSignal, GlonassRecord, IonoCorrections,
    KeplerianElements, KlobucharAlphaBeta, SatelliteState,
};
use sidereon_core::prelude::EphemerisSource;
use sidereon_core::rinex::nav::{
    encode_nav, parse_glonass, parse_iono_corrections, parse_leap_seconds, parse_nav,
    parse_nav_lenient, NavParse as CoreNavParse, SkippedNavBlock as CoreSkippedNavBlock,
};
use sidereon_core::{astro::time::GnssWeekTow, GnssSatelliteId};
use std::{str::FromStr, sync::Arc};

use crate::error::{engine_error, range_error, type_error, utf8_text};
use crate::frames::ExactEpochQueryValue;

/// Which supported RINEX NAV message a broadcast record carries.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NavMessage {
    /// GPS legacy LNAV.
    GpsLnav,
    /// GPS CNAV.
    GpsCnav,
    /// GPS CNAV-2.
    GpsCnav2,
    /// QZSS CNAV.
    QzssCnav,
    /// QZSS CNAV-2.
    QzssCnav2,
    /// QZSS LNAV.
    QzssLnav,
    /// Galileo I/NAV.
    GalileoInav,
    /// Galileo F/NAV.
    GalileoFnav,
    /// BeiDou D1.
    BeidouD1,
    /// BeiDou D2.
    BeidouD2,
    /// NavIC (IRNSS) LNAV.
    NavicLnav,
    /// A Galileo record whose data-source word names no message (RINEX 3.05
    /// Table A8), used with the BGD E5b/E1 as RTKLIB's default selection
    /// uses it.
    GalileoUnclassified,
}

/// GPS/QZSS signal used for CNAV inter-signal correction accessors.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CnavSignal {
    L1Ca,
    L2C,
    L5I5,
    L5Q5,
    L1Cp,
    L1Cd,
}

fn core_cnav_signal(signal: CnavSignal) -> CoreCnavSignal {
    match signal {
        CnavSignal::L1Ca => CoreCnavSignal::L1Ca,
        CnavSignal::L2C => CoreCnavSignal::L2C,
        CnavSignal::L5I5 => CoreCnavSignal::L5I5,
        CnavSignal::L5Q5 => CoreCnavSignal::L5Q5,
        CnavSignal::L1Cp => CoreCnavSignal::L1Cp,
        CnavSignal::L1Cd => CoreCnavSignal::L1Cd,
    }
}

/// Broadcast group-delay term selector.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BroadcastDelayTerm {
    GpsTgd,
    GalileoBgdE5aE1,
    GalileoBgdE5bE1,
    BeidouTgd1,
    BeidouTgd2,
    CnavIscL1Ca,
    CnavIscL2C,
    CnavIscL5I5,
    CnavIscL5Q5,
    CnavIscL1Cd,
    CnavIscL1Cp,
}

fn core_delay_term(term: BroadcastDelayTerm) -> BroadcastGroupDelayTerm {
    match term {
        BroadcastDelayTerm::GpsTgd => BroadcastGroupDelayTerm::GpsTgd,
        BroadcastDelayTerm::GalileoBgdE5aE1 => BroadcastGroupDelayTerm::GalileoBgdE5aE1,
        BroadcastDelayTerm::GalileoBgdE5bE1 => BroadcastGroupDelayTerm::GalileoBgdE5bE1,
        BroadcastDelayTerm::BeidouTgd1 => BroadcastGroupDelayTerm::BeidouTgd1,
        BroadcastDelayTerm::BeidouTgd2 => BroadcastGroupDelayTerm::BeidouTgd2,
        BroadcastDelayTerm::CnavIscL1Ca => BroadcastGroupDelayTerm::CnavIscL1Ca,
        BroadcastDelayTerm::CnavIscL2C => BroadcastGroupDelayTerm::CnavIscL2C,
        BroadcastDelayTerm::CnavIscL5I5 => BroadcastGroupDelayTerm::CnavIscL5I5,
        BroadcastDelayTerm::CnavIscL5Q5 => BroadcastGroupDelayTerm::CnavIscL5Q5,
        BroadcastDelayTerm::CnavIscL1Cd => BroadcastGroupDelayTerm::CnavIscL1Cd,
        BroadcastDelayTerm::CnavIscL1Cp => BroadcastGroupDelayTerm::CnavIscL1Cp,
    }
}

/// Nominal GPS/QZSS CNAV URA meters for an ED/NED0 index.
#[wasm_bindgen(js_name = cnavUraNominalM)]
pub fn cnav_ura_nominal_m(index: i8) -> Option<f64> {
    core_cnav_ura_nominal_m(index)
}

fn map_nav_message(message: sidereon_core::ephemeris::NavMessage) -> NavMessage {
    use sidereon_core::ephemeris::NavMessage as Core;
    match message {
        Core::GpsLnav => NavMessage::GpsLnav,
        Core::GpsCnav => NavMessage::GpsCnav,
        Core::GpsCnav2 => NavMessage::GpsCnav2,
        Core::QzssCnav => NavMessage::QzssCnav,
        Core::QzssCnav2 => NavMessage::QzssCnav2,
        Core::QzssLnav => NavMessage::QzssLnav,
        Core::GalileoInav => NavMessage::GalileoInav,
        Core::GalileoFnav => NavMessage::GalileoFnav,
        Core::BeidouD1 => NavMessage::BeidouD1,
        Core::BeidouD2 => NavMessage::BeidouD2,
        Core::NavicLnav => NavMessage::NavicLnav,
        Core::GalileoUnclassified => NavMessage::GalileoUnclassified,
    }
}

/// Stable lower-case selector for a NAV message, e.g. `"gps_lnav"`.
#[wasm_bindgen(js_name = navMessageLabel)]
pub fn nav_message_label(message: NavMessage) -> String {
    match message {
        NavMessage::GpsLnav => "gps_lnav",
        NavMessage::GpsCnav => "gps_cnav",
        NavMessage::GpsCnav2 => "gps_cnav2",
        NavMessage::QzssCnav => "qzss_cnav",
        NavMessage::QzssCnav2 => "qzss_cnav2",
        NavMessage::QzssLnav => "qzss_lnav",
        NavMessage::GalileoInav => "galileo_inav",
        NavMessage::GalileoFnav => "galileo_fnav",
        NavMessage::BeidouD1 => "beidou_d1",
        NavMessage::BeidouD2 => "beidou_d2",
        NavMessage::NavicLnav => "navic_lnav",
        NavMessage::GalileoUnclassified => "galileo_unclassified",
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
    pub(crate) inner: BroadcastRecord,
}

impl BroadcastRecordJs {
    pub(crate) fn from_core(inner: BroadcastRecord) -> Self {
        Self { inner }
    }
}

/// CNAV/CNAV-2 fields with no legacy LNAV counterpart.
#[wasm_bindgen]
pub struct CnavParametersJs {
    inner: CnavParameters,
}

#[wasm_bindgen]
impl CnavParametersJs {
    #[wasm_bindgen(getter, js_name = adotMS)]
    pub fn adot_m_s(&self) -> f64 {
        self.inner.adot_m_s
    }

    #[wasm_bindgen(getter, js_name = deltaN0DotRadS2)]
    pub fn delta_n0_dot_rad_s2(&self) -> f64 {
        self.inner.delta_n0_dot_rad_s2
    }

    #[wasm_bindgen(getter, js_name = topWeek)]
    pub fn top_week(&self) -> u32 {
        self.inner.top.week
    }

    #[wasm_bindgen(getter, js_name = topTowS)]
    pub fn top_tow_s(&self) -> f64 {
        self.inner.top.tow_s
    }

    #[wasm_bindgen(getter, js_name = uraEdIndex)]
    pub fn ura_ed_index(&self) -> i8 {
        self.inner.ura_ed_index
    }

    #[wasm_bindgen(getter, js_name = uraNed0Index)]
    pub fn ura_ned0_index(&self) -> i8 {
        self.inner.ura_ned0_index
    }

    #[wasm_bindgen(getter, js_name = uraNed1Index)]
    pub fn ura_ned1_index(&self) -> u8 {
        self.inner.ura_ned1_index
    }

    #[wasm_bindgen(getter, js_name = uraNed2Index)]
    pub fn ura_ned2_index(&self) -> u8 {
        self.inner.ura_ned2_index
    }

    #[wasm_bindgen(getter, js_name = transmissionTimeSow)]
    pub fn transmission_time_sow(&self) -> f64 {
        self.inner.transmission_time_sow
    }

    #[wasm_bindgen(getter)]
    pub fn flags(&self) -> Option<u32> {
        self.inner.flags
    }

    #[wasm_bindgen(js_name = uraNedM)]
    pub fn ura_ned_m(&self, week: u32, tow_s: f64) -> Result<Option<f64>, JsValue> {
        let t = GnssWeekTow::new(self.inner.top.system, week, tow_s).map_err(engine_error)?;
        Ok(core_cnav_ura_ned_m(&self.inner, t))
    }
}

/// Per-signal broadcast group-delay terms retained from a NAV record.
#[wasm_bindgen]
pub struct BroadcastGroupDelaysJs {
    inner: BroadcastGroupDelays,
}

#[wasm_bindgen]
impl BroadcastGroupDelaysJs {
    #[wasm_bindgen(js_name = get)]
    pub fn get(&self, term: BroadcastDelayTerm) -> Option<f64> {
        self.inner.get(core_delay_term(term))
    }

    #[wasm_bindgen(js_name = cnavSingleFrequencyCorrectionS)]
    pub fn cnav_single_frequency_correction_s(&self, signal: CnavSignal) -> Option<f64> {
        self.inner
            .cnav_single_frequency_correction_s(core_cnav_signal(signal))
    }
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

    /// Native issue-of-data value, or `undefined` for a RINEX 4 CNAV or
    /// CNAV-2 record, which states none.
    #[wasm_bindgen(getter)]
    pub fn issue(&self) -> Option<u32> {
        self.inner.issue_of_data.map(|issue| issue.issue)
    }

    /// Message family attached to the issue-of-data value, or `undefined`
    /// when the record states no issue of data.
    #[wasm_bindgen(getter, js_name = issueMessage)]
    pub fn issue_message(&self) -> Option<NavMessage> {
        self.inner
            .issue_of_data
            .map(|issue| map_nav_message(issue.message))
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

    /// Signal-in-space accuracy, metres, or `undefined` when the record
    /// states none: a blank or unreadable field, or a CNAV URA_ED index of no
    /// prediction.
    #[wasm_bindgen(getter, js_name = svAccuracyM)]
    pub fn sv_accuracy_m(&self) -> Option<f64> {
        self.inner.sv_accuracy_m
    }

    /// GPS and QZSS IODC as the record states it, or `undefined`.
    #[wasm_bindgen(getter)]
    pub fn iodc(&self) -> Option<f64> {
        self.inner.iodc()
    }

    /// GPS "codes on L2" as stated, or `undefined`.
    #[wasm_bindgen(getter, js_name = l2Codes)]
    pub fn l2_codes(&self) -> Option<f64> {
        self.inner.l2_codes()
    }

    /// GPS "L2 P data flag" as stated, or `undefined`.
    #[wasm_bindgen(getter, js_name = l2pDataFlag)]
    pub fn l2p_data_flag(&self) -> Option<f64> {
        self.inner.l2p_data_flag()
    }

    /// Galileo data-source word as the nearest integer, or `undefined`.
    #[wasm_bindgen(getter, js_name = galileoDataSources)]
    pub fn galileo_data_sources(&self) -> Option<u32> {
        self.inner.galileo_data_sources()
    }

    /// BeiDou AODC as stated, or `undefined`.
    #[wasm_bindgen(getter, js_name = beidouAodc)]
    pub fn beidou_aodc(&self) -> Option<f64> {
        self.inner.beidou_aodc()
    }

    /// Transmission time of message, seconds of week, as stated, or
    /// `undefined`.
    #[wasm_bindgen(getter, js_name = transmissionTimeSow)]
    pub fn transmission_time_sow(&self) -> Option<f64> {
        self.inner.transmission_time_sow()
    }

    /// GPS curve-fit interval, seconds, or `undefined` when not broadcast.
    #[wasm_bindgen(getter, js_name = fitIntervalS)]
    pub fn fit_interval_s(&self) -> Option<f64> {
        self.inner.fit_interval_s
    }

    /// Broadcast group-delay terms carried by this record.
    #[wasm_bindgen(getter, js_name = groupDelays)]
    pub fn group_delays(&self) -> BroadcastGroupDelaysJs {
        BroadcastGroupDelaysJs {
            inner: self.inner.group_delays,
        }
    }

    /// CNAV-family extension fields, or `undefined` for legacy records.
    #[wasm_bindgen(getter)]
    pub fn cnav(&self) -> Option<CnavParametersJs> {
        self.inner.cnav.map(|inner| CnavParametersJs { inner })
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

    /// FDMA frequency-channel number. A stated channel above 128 reads as that
    /// value less 256, as RTKLIB reads it.
    #[wasm_bindgen(getter, js_name = freqChannel)]
    pub fn freq_channel(&self) -> i32 {
        self.inner.freq_channel
    }

    /// The frequency channel as the record states it.
    #[wasm_bindgen(getter, js_name = statedFreqChannel)]
    pub fn stated_freq_channel(&self) -> i32 {
        self.inner.stated_freq_channel
    }

    /// The record epoch as stated, UTC seconds past J2000 (`toeUtcJ2000S` is
    /// that epoch rounded to the 15-minute grid, as RTKLIB rounds `tb`).
    #[wasm_bindgen(getter, js_name = epochUtcJ2000S)]
    pub fn epoch_utc_j2000_s(&self) -> f64 {
        self.inner.epoch_utc_j2000_s
    }

    /// The reference epoch in GPS time, seconds past J2000, converted with the
    /// leap-second table at that instant, as RTKLIB `utc2gpst` converts it.
    #[wasm_bindgen(getter, js_name = toeGpstJ2000S)]
    pub fn toe_gpst_j2000_s(&self) -> f64 {
        self.inner.toe_gpst_j2000_s()
    }

    /// Message frame time, seconds, as stated, or `undefined`.
    #[wasm_bindgen(getter, js_name = messageFrameTimeS)]
    pub fn message_frame_time_s(&self) -> Option<f64> {
        self.inner.message_frame_time_s
    }

    /// Age of operation information, days, or `undefined`.
    #[wasm_bindgen(getter, js_name = ageDays)]
    pub fn age_days(&self) -> Option<f64> {
        self.inner.age_days
    }

    /// Status flags (RINEX 3.05 fourth orbit line) as stated, or `undefined`.
    #[wasm_bindgen(getter, js_name = statusFlags)]
    pub fn status_flags(&self) -> Option<f64> {
        self.inner.status_flags
    }

    /// L1/L2 group delay difference field as stated, seconds, or `undefined`.
    #[wasm_bindgen(getter, js_name = l1L2GroupDelayFieldS)]
    pub fn l1_l2_group_delay_field_s(&self) -> Option<f64> {
        self.inner.l1_l2_group_delay_field_s
    }

    /// Raw accuracy index URAI as stated, or `undefined`.
    #[wasm_bindgen(getter)]
    pub fn urai(&self) -> Option<f64> {
        self.inner.urai
    }

    /// Health flags as stated, or `undefined`.
    #[wasm_bindgen(getter, js_name = healthFlags)]
    pub fn health_flags(&self) -> Option<f64> {
        self.inner.health_flags
    }

    /// The L1/L2 group delay difference `ΔτN`, seconds, or `undefined` when
    /// blank or stated as not known.
    #[wasm_bindgen(getter, js_name = l1L2GroupDelayS)]
    pub fn l1_l2_group_delay_s(&self) -> Option<f64> {
        self.inner.l1_l2_group_delay_s()
    }

    /// The status flags as a word, or `undefined`.
    #[wasm_bindgen(getter, js_name = statusFlagsWord)]
    pub fn status_flags_word(&self) -> Option<u32> {
        self.inner.status_flags_word()
    }

    /// The health flags as a word, or `undefined`.
    #[wasm_bindgen(getter, js_name = healthFlagsWord)]
    pub fn health_flags_word(&self) -> Option<u32> {
        self.inner.health_flags_word()
    }

    /// Whether the record reports the satellite healthy by the RINEX 3.05
    /// Table A10 health fields.
    #[wasm_bindgen(getter, js_name = isHealthy)]
    pub fn is_healthy(&self) -> bool {
        self.inner.is_healthy()
    }

    /// RTKLIB `prange`'s G1 single-frequency group delay `-ΔτN / (γ - 1)`,
    /// seconds, or `undefined` when `ΔτN` is not known.
    #[wasm_bindgen(getter, js_name = singleFrequencyGroupDelayS)]
    pub fn single_frequency_group_delay_s(&self) -> Option<f64> {
        self.inner.single_frequency_group_delay_s()
    }

    /// RTKLIB `geph2clk`: the clock bias at a satellite clock time, GPST
    /// seconds past J2000.
    #[wasm_bindgen(js_name = clockBiasAtS)]
    pub fn clock_bias_at_s(&self, t_sv_gpst_j2000_s: f64) -> f64 {
        self.inner.clock_bias_s(t_sv_gpst_j2000_s)
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

impl IonoCorrectionsJs {
    pub(crate) fn from_core(inner: IonoCorrections) -> Self {
        Self { inner }
    }
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

    /// QZSS Klobuchar coefficients (`QZSA`/`QZSB` or a RINEX 4 frame).
    #[wasm_bindgen(getter)]
    pub fn qzss(&self) -> Option<KlobucharAlphaBetaJs> {
        self.inner.qzss.map(|inner| KlobucharAlphaBetaJs { inner })
    }

    /// NavIC Klobuchar coefficients (`IRNA`/`IRNB` or a RINEX 4 frame).
    #[wasm_bindgen(getter)]
    pub fn navic(&self) -> Option<KlobucharAlphaBetaJs> {
        self.inner.navic.map(|inner| KlobucharAlphaBetaJs { inner })
    }

    /// Galileo NeQuick G coefficients `[ai0, ai1, ai2]`, or `undefined`.
    #[wasm_bindgen(getter, js_name = galileoNequick)]
    pub fn galileo_nequick(&self) -> Option<Vec<f64>> {
        self.inner
            .galileo
            .map(|coeffs| vec![coeffs.ai0, coeffs.ai1, coeffs.ai2])
    }

    /// Galileo ionospheric disturbance flags word as stated, or `undefined`.
    #[wasm_bindgen(getter, js_name = galileoDisturbanceFlags)]
    pub fn galileo_disturbance_flags(&self) -> Option<f64> {
        self.inner.galileo_disturbance_flags
    }

    /// BeiDou BDGIM coefficients (nine), or `undefined`.
    #[wasm_bindgen(getter, js_name = beidouBdgim)]
    pub fn beidou_bdgim(&self) -> Option<Vec<f64>> {
        self.inner.beidou_bdgim.map(|coeffs| coeffs.to_vec())
    }
}

/// A parsed broadcast ephemeris store from a RINEX NAV file. `records` are the
/// usable GPS/Galileo/BeiDou records selected by the core default SPP policy.
#[wasm_bindgen]
pub struct BroadcastEphemeris {
    pub(crate) inner: Arc<CoreBroadcastStore>,
    leap_seconds: Option<f64>,
}

/// One supported RINEX NAV block that the core parser could not represent.
///
/// The diagnostic is retained by [`parseRinexNavLenient`] instead of turning a
/// malformed supported block into a fatal whole-file error.
#[wasm_bindgen]
pub struct SkippedNavBlock {
    inner: CoreSkippedNavBlock,
}

#[wasm_bindgen]
impl SkippedNavBlock {
    /// Satellite token from the skipped block, such as `"G01"`.
    #[wasm_bindgen(getter)]
    pub fn satellite(&self) -> String {
        self.inner.satellite.clone()
    }

    /// Core parser diagnostic explaining why the block was skipped.
    #[wasm_bindgen(getter)]
    pub fn message(&self) -> String {
        self.inner.message.clone()
    }

    /// One-based line number of the block's first line.
    #[wasm_bindgen(getter)]
    pub fn line(&self) -> usize {
        self.inner.line
    }
}

/// A departure from the format read through, with its line.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct NavDiagnosticJs {
    line: usize,
    satellite: String,
    message: String,
}

fn diagnostics_to_js(
    diagnostics: &[sidereon_core::rinex::nav::NavDiagnostic],
) -> Result<JsValue, JsValue> {
    let rows: Vec<NavDiagnosticJs> = diagnostics
        .iter()
        .map(|diagnostic| NavDiagnosticJs {
            line: diagnostic.line,
            satellite: diagnostic.satellite.clone(),
            message: diagnostic.error.to_string(),
        })
        .collect();
    crate::error::to_plain_js(&rows, "RINEX NAV departures")
}

/// A navigation block of a kind `parseRinexNavLenient` does not return as a
/// broadcast record: GLONASS, SBAS, system time offset, Earth orientation,
/// ionosphere, and the BeiDou CNAV-1/2/3 and NavIC L1 frames.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct OtherNavBlockJs {
    line: usize,
    satellite: String,
    message_token: Option<String>,
    kind: String,
}

/// Result of lenient RINEX NAV parsing.
///
/// `records` contains the supported records in file order and `skipped`
/// contains malformed supported blocks reported by the core parser.
#[wasm_bindgen]
pub struct RinexNavParse {
    inner: CoreNavParse,
}

#[wasm_bindgen]
impl RinexNavParse {
    /// Supported broadcast records in file order.
    #[wasm_bindgen(getter)]
    pub fn records(&self) -> Vec<BroadcastRecordJs> {
        self.inner
            .records
            .iter()
            .cloned()
            .map(BroadcastRecordJs::from_core)
            .collect()
    }

    /// Supported blocks skipped by the lenient core parser, in file order.
    #[wasm_bindgen(getter)]
    pub fn skipped(&self) -> Vec<SkippedNavBlock> {
        self.inner
            .skipped
            .iter()
            .cloned()
            .map(|inner| SkippedNavBlock { inner })
            .collect()
    }

    /// Number of supported records returned by the parser.
    #[wasm_bindgen(getter, js_name = recordCount)]
    pub fn record_count(&self) -> usize {
        self.inner.records.len()
    }

    /// Number of supported blocks skipped by the parser.
    #[wasm_bindgen(getter, js_name = skippedCount)]
    pub fn skipped_count(&self) -> usize {
        self.inner.skipped.len()
    }

    /// Departures from the format read through, as `{ line, satellite,
    /// message }`.
    #[wasm_bindgen(getter, unchecked_return_type = "RinexNavDiagnostic[]")]
    pub fn departures(&self) -> Result<JsValue, JsValue> {
        diagnostics_to_js(&self.inner.departures)
    }

    /// Blocks of kinds this reader does not return as broadcast records, as
    /// `{ line, satellite, messageToken, kind }`.
    #[wasm_bindgen(getter, unchecked_return_type = "RinexNavOtherBlock[]")]
    pub fn other(&self) -> Result<JsValue, JsValue> {
        let rows: Vec<OtherNavBlockJs> = self
            .inner
            .other
            .iter()
            .map(|block| OtherNavBlockJs {
                line: block.line,
                satellite: block.satellite.clone(),
                message_token: block.message_token.clone(),
                kind: format!("{:?}", block.kind),
            })
            .collect();
        crate::error::to_plain_js(&rows, "other RINEX NAV blocks")
    }
}

/// Store-level broadcast ephemeris evaluation at a J2000 query epoch.
#[wasm_bindgen]
pub struct BroadcastStoreEvaluation {
    satellite: String,
    t_j2000_s: f64,
    position_m: [f64; 3],
    clock_s: f64,
}

#[wasm_bindgen]
impl BroadcastStoreEvaluation {
    #[wasm_bindgen(getter)]
    pub fn satellite(&self) -> String {
        self.satellite.clone()
    }

    #[wasm_bindgen(getter, js_name = tJ2000S)]
    pub fn t_j2000_s(&self) -> f64 {
        self.t_j2000_s
    }

    #[wasm_bindgen(getter, js_name = positionM)]
    pub fn position_m(&self) -> Vec<f64> {
        self.position_m.to_vec()
    }

    #[wasm_bindgen(getter, js_name = clockS)]
    pub fn clock_s(&self) -> f64 {
        self.clock_s
    }
}

#[wasm_bindgen]
impl BroadcastEphemeris {
    #[wasm_bindgen(js_name = selectedPositionClockAtExactQueries, unchecked_return_type = "Ut1Validated<SelectedPositionClock> | null")]
    pub fn selected_position_clock_at_exact_queries(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        selection_epoch: &ExactEpochQueryValue,
    ) -> Result<JsValue, JsValue> {
        let satellite = GnssSatelliteId::from_str(satellite)
            .map_err(|_| type_error("invalid satellite token"))?;
        crate::sp3::selected_position_clock_at_queries(
            self.inner.as_ref(),
            satellite,
            state_epoch,
            selection_epoch,
        )
    }

    #[wasm_bindgen(js_name = transmitEpochClockAtExactQueries, unchecked_return_type = "Ut1Validated<number> | null")]
    pub fn transmit_epoch_clock_at_exact_queries(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        selection_epoch: &ExactEpochQueryValue,
    ) -> Result<JsValue, JsValue> {
        let satellite = GnssSatelliteId::from_str(satellite)
            .map_err(|_| type_error("invalid satellite token"))?;
        crate::sp3::transmit_epoch_clock_at_queries(
            self.inner.as_ref(),
            satellite,
            state_epoch,
            selection_epoch,
        )
    }

    #[wasm_bindgen(js_name = ephemerisVarianceAtExactQuery)]
    pub fn ephemeris_variance_at_exact_query(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        selection_epoch: &ExactEpochQueryValue,
    ) -> Result<f64, JsValue> {
        let satellite = GnssSatelliteId::from_str(satellite)
            .map_err(|_| type_error("invalid satellite token"))?;
        Ok(crate::sp3::precise_variance_at_queries(
            self.inner.as_ref(),
            satellite,
            state_epoch,
            selection_epoch,
        ))
    }

    #[wasm_bindgen(js_name = clockRelativityAtExactQuery, unchecked_return_type = "ClockRelativity")]
    pub fn clock_relativity_at_exact_query(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        position_ecef_m: Vec<f64>,
    ) -> Result<JsValue, JsValue> {
        let satellite = GnssSatelliteId::from_str(satellite)
            .map_err(|_| type_error("invalid satellite token"))?;
        let position_ecef_m: [f64; 3] = position_ecef_m
            .try_into()
            .map_err(|_| type_error("positionEcefM must contain exactly three coordinates"))?;
        crate::sp3::precise_clock_relativity_at_query(
            self.inner.as_ref(),
            satellite,
            state_epoch,
            position_ecef_m,
        )
    }

    /// Usable GPS, Galileo, and BeiDou broadcast records in file order.
    #[wasm_bindgen(getter)]
    pub fn records(&self) -> Vec<BroadcastRecordJs> {
        self.inner
            .records()
            .iter()
            .cloned()
            .map(BroadcastRecordJs::from_core)
            .collect()
    }

    /// GLONASS broadcast records in file order, unhealthy ones included; a
    /// query applies the health rule to the record it selects.
    #[wasm_bindgen(getter, js_name = glonassRecords)]
    pub fn glonass_records(&self) -> Vec<GlonassRecordJs> {
        self.inner
            .glonass_records()
            .iter()
            .cloned()
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

    /// Number of GLONASS records.
    #[wasm_bindgen(getter, js_name = glonassRecordCount)]
    pub fn glonass_record_count(&self) -> usize {
        self.inner.glonass_records().len()
    }

    /// Blocks the store could not read, each with its line; the rest of the
    /// file was read.
    #[wasm_bindgen(getter)]
    pub fn skipped(&self) -> Vec<SkippedNavBlock> {
        self.inner
            .skipped()
            .iter()
            .cloned()
            .map(|inner| SkippedNavBlock { inner })
            .collect()
    }

    /// Departures from the format read through, as `{ line, satellite,
    /// message }`, including a malformed optional header row whose value is
    /// then absent.
    #[wasm_bindgen(getter, unchecked_return_type = "RinexNavDiagnostic[]")]
    pub fn departures(&self) -> Result<JsValue, JsValue> {
        diagnostics_to_js(self.inner.departures())
    }

    /// The broadcast ionosphere coefficients in effect at `tJ2000S`: each
    /// header set replaced by the frame of its system and model transmitted
    /// latest at or before the epoch.
    #[wasm_bindgen(js_name = ionoCorrectionsAt)]
    pub fn iono_corrections_at(&self, t_j2000_s: f64) -> Result<IonoCorrectionsJs, JsValue> {
        if !t_j2000_s.is_finite() {
            return Err(range_error("tJ2000S must be a finite number"));
        }
        Ok(IonoCorrectionsJs {
            inner: self.inner.iono_corrections_at(t_j2000_s),
        })
    }

    /// Evaluate the store-selected broadcast state for `satellite` at GPST-like
    /// J2000 seconds. Returns `undefined` when no usable record covers the query.
    #[wasm_bindgen]
    pub fn evaluate(
        &self,
        satellite: &str,
        t_j2000_s: f64,
    ) -> Result<Option<BroadcastStoreEvaluation>, JsValue> {
        if !t_j2000_s.is_finite() {
            return Err(range_error("tJ2000S must be a finite number"));
        }
        let sat = GnssSatelliteId::from_str(satellite).map_err(|_| {
            crate::error::type_error(&format!("invalid satellite token: {satellite}"))
        })?;
        Ok(self
            .inner
            .position_clock_at_j2000_s(sat, t_j2000_s)
            .map(|(position_m, clock_s)| BroadcastStoreEvaluation {
                satellite: sat.to_string(),
                t_j2000_s,
                position_m,
                clock_s,
            }))
    }

    /// Serialize the Keplerian broadcast records to RINEX navigation text,
    /// with a `PGM / RUN BY / DATE` record. Deterministic: the same record set
    /// always produces byte-identical text. GLONASS state-vector records are
    /// not part of the Keplerian broadcast-orbit grammar this writer emits and
    /// are therefore not included; `parseRinexNavFile` keeps and restates a
    /// whole file. Throws an `Error` for a set holding both a CNAV-family
    /// record (RINEX 4 only) and an unclassified Galileo record (RINEX 3
    /// only), or a CNAV-family record without CNAV parameters.
    #[wasm_bindgen(js_name = toRinexString)]
    pub fn to_rinex_string(&self) -> Result<String, JsValue> {
        encode_nav(self.inner.records()).map_err(engine_error)
    }
}

/// Parse RINEX NAV bytes into the default broadcast ephemeris store. Throws a
/// `TypeError` on non-UTF-8 input and an `Error` on a parse failure.
#[wasm_bindgen(js_name = parseRinexNav)]
pub fn parse_rinex_nav(bytes: &[u8]) -> Result<BroadcastEphemeris, JsValue> {
    let text = utf8_text(bytes, "RINEX NAV source")?;
    let inner = Arc::new(CoreBroadcastStore::from_nav(&text).map_err(engine_error)?);
    // A malformed LEAP SECONDS row leaves the value absent, as the store
    // leaves every malformed optional header row, and is reported in the
    // store's `departures`.
    let leap_seconds = parse_leap_seconds(&text).ok().flatten();
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
        .map(BroadcastRecordJs::from_core)
        .collect())
}

/// Parse RINEX NAV bytes leniently through the core parser.
///
/// Header failures remain fatal. Malformed supported record blocks are
/// returned in `skipped` with their satellite token and core diagnostic;
/// unsupported systems and message rosters retain the core parser's skip
/// policy. Non-UTF-8 input throws a `TypeError`.
#[wasm_bindgen(js_name = parseRinexNavLenient)]
pub fn parse_rinex_nav_lenient(bytes: &[u8]) -> Result<RinexNavParse, JsValue> {
    let text = utf8_text(bytes, "RINEX NAV source")?;
    Ok(RinexNavParse {
        inner: parse_nav_lenient(&text).map_err(engine_error)?,
    })
}

/// Encode an arbitrary caller-supplied list of broadcast records as RINEX NAV
/// text. The list is consumed by the WASM boundary and the bytes are produced
/// by the core's deterministic encoder. Throws an `Error` for a record set the
/// writer refuses (see `BroadcastEphemeris.toRinexString`).
#[wasm_bindgen(js_name = encodeRinexNav)]
pub fn encode_rinex_nav(records: Vec<BroadcastRecordJs>) -> Result<String, JsValue> {
    let records: Vec<BroadcastRecord> = records.into_iter().map(|record| record.inner).collect();
    encode_nav(&records).map_err(engine_error)
}

/// A whole RINEX navigation file: every header record and every block,
/// decoded or not, with its text and line number.
#[wasm_bindgen]
pub struct RinexNavFile {
    inner: sidereon_core::rinex::nav::NavFile,
}

#[wasm_bindgen]
impl RinexNavFile {
    /// Number of body blocks, in file order.
    #[wasm_bindgen(getter, js_name = entryCount)]
    pub fn entry_count(&self) -> usize {
        self.inner.entries.len()
    }

    /// Header and body departures from the format read through, as
    /// `{ line, satellite, message }`.
    #[wasm_bindgen(getter, unchecked_return_type = "RinexNavDiagnostic[]")]
    pub fn departures(&self) -> Result<JsValue, JsValue> {
        diagnostics_to_js(&self.inner.departures())
    }

    /// Write the file back. A file read and written unchanged comes back byte
    /// for byte. Throws an `Error` when the writer refuses the content.
    #[wasm_bindgen(js_name = toRinexString)]
    pub fn to_rinex_string(&self) -> Result<String, JsValue> {
        sidereon_core::rinex::nav::encode_nav_file(&self.inner).map_err(engine_error)
    }
}

/// Read a whole RINEX 2, 3 or 4 navigation file, keeping every header record
/// and every block, decoded or not, so `toRinexString` restates it. Throws a
/// `TypeError` on non-UTF-8 input and an `Error` on a header failure.
#[wasm_bindgen(js_name = parseRinexNavFile)]
pub fn parse_rinex_nav_file(bytes: &[u8]) -> Result<RinexNavFile, JsValue> {
    let text = utf8_text(bytes, "RINEX NAV source")?;
    Ok(RinexNavFile {
        inner: sidereon_core::rinex::nav::parse_nav_file(&text).map_err(engine_error)?,
    })
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
