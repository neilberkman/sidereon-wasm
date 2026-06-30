//! Frames + time binding: scale-tagged instants, two-part Julian dates, GNSS
//! week/TOW, leap-second / EOP table provenance, and batched coordinate
//! transforms.
//!
//! Every instant's TT/UT1/TDB scales come from the parity-critical
//! `UtcInstant::time_scales` path, and every transform is the engine's own
//! `*_compute`, so the numbers are bit-identical to what `sidereon-core`
//! produces. The batched transforms take a flat row-major `Float64Array`
//! (`n`-by-3) of states plus a `BigInt64Array` of unix-microsecond epochs and
//! run the per-row loop inside Rust.

use wasm_bindgen::prelude::*;

use sidereon::passes::UtcInstant;
use sidereon_core::astro::frames::nutation::{
    build_skyfield_nutation_matrix, skyfield_iau2000a_radians, skyfield_mean_obliquity_radians,
};
use sidereon_core::astro::frames::precession::compute_skyfield_precession_matrix;
use sidereon_core::astro::frames::transforms::{
    gcrs_to_itrs_compute, geodetic_to_itrs, greenwich_apparent_sidereal_time_radians,
    greenwich_mean_sidereal_time_radians, itrs_to_gcrs_compute, itrs_to_geodetic_compute,
    teme_to_gcrs_compute, TemeStateKm,
};
use sidereon_core::astro::time::civil::{
    civil_from_j2000_seconds, j2000_seconds, j2000_seconds_from_split,
};
use sidereon_core::astro::time::model::Instant as CoreInstant;
use sidereon_core::astro::time::scales::{
    find_leap_seconds, julian_day_number, leap_second_table, ut1_coverage,
};
use sidereon_core::astro::time::{
    timescale_offset_at_s, timescale_offset_s, GnssWeekTow as CoreGnssWeekTow,
    TimeScale as CoreTimeScale, TimeScales,
};

use crate::error::{engine_error, range_error, type_error};
use crate::marshal::{flat3, mat3_flat, rows3, same_len};

const SECONDS_PER_DAY: f64 = 86_400.0;
const MICROSECONDS_PER_SECOND: f64 = 1_000_000.0;

/// A named time scale. The JS value matches the variant order below.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TimeScale {
    /// Coordinated Universal Time.
    Utc,
    /// International Atomic Time.
    Tai,
    /// Terrestrial Time.
    Tt,
    /// Barycentric Dynamical Time.
    Tdb,
    /// GPS time.
    Gpst,
    /// Galileo System Time.
    Gst,
    /// BeiDou Time.
    Bdt,
    /// GLONASS system time (UTC(SU)-based, leap-second carrying).
    Glonasst,
    /// QZSS system time (steered to GPST).
    Qzsst,
}

impl From<TimeScale> for CoreTimeScale {
    fn from(scale: TimeScale) -> Self {
        match scale {
            TimeScale::Utc => CoreTimeScale::Utc,
            TimeScale::Tai => CoreTimeScale::Tai,
            TimeScale::Tt => CoreTimeScale::Tt,
            TimeScale::Tdb => CoreTimeScale::Tdb,
            TimeScale::Gpst => CoreTimeScale::Gpst,
            TimeScale::Gst => CoreTimeScale::Gst,
            TimeScale::Bdt => CoreTimeScale::Bdt,
            TimeScale::Glonasst => CoreTimeScale::Glonasst,
            TimeScale::Qzsst => CoreTimeScale::Qzsst,
        }
    }
}

impl From<CoreTimeScale> for TimeScale {
    fn from(scale: CoreTimeScale) -> Self {
        match scale {
            CoreTimeScale::Utc => TimeScale::Utc,
            CoreTimeScale::Tai => TimeScale::Tai,
            CoreTimeScale::Tt => TimeScale::Tt,
            CoreTimeScale::Tdb => TimeScale::Tdb,
            CoreTimeScale::Gpst => TimeScale::Gpst,
            CoreTimeScale::Gst => TimeScale::Gst,
            CoreTimeScale::Bdt => TimeScale::Bdt,
            CoreTimeScale::Glonasst => TimeScale::Glonasst,
            CoreTimeScale::Qzsst => TimeScale::Qzsst,
        }
    }
}

/// Short uppercase identifier for a time scale, e.g. `"GPST"`.
#[wasm_bindgen(js_name = timeScaleAbbrev)]
pub fn time_scale_abbrev(scale: TimeScale) -> String {
    CoreTimeScale::from(scale).abbrev().to_string()
}

/// Fixed inter-system offset `to_reading - from_reading`, in seconds, for the
/// same physical instant. Add the result to a `from`-scale reading to get the
/// `to`-scale reading.
///
/// Covers the atomic scales (TAI/TT/GPST/GST/QZSST/BDT), whose mutual offsets
/// are constants fixed by their ICDs. Throws a `RangeError` when either scale is
/// UTC-based (`Utc`/`Glonasst`) — those carry leap seconds, so their offset is
/// epoch-dependent and needs [`timescaleOffsetAtS`] — or for `Tdb` (no fixed
/// offset; resolve it through an `Instant`).
#[wasm_bindgen(js_name = timescaleOffsetS)]
pub fn timescale_offset_s_js(from: TimeScale, to: TimeScale) -> Result<f64, JsValue> {
    timescale_offset_s(from.into(), to.into()).map_err(|e| range_error(&e.to_string()))
}

/// Leap-aware inter-system offset `to_reading - from_reading`, in seconds, at a
/// given UTC instant. Add the result to a `from`-scale reading to get the
/// `to`-scale reading.
///
/// `utcJd` is the UTC Julian date of the instant; it is consulted only to
/// resolve the leap-second count when `from` or `to` is UTC-based
/// (`Utc`/`Glonasst`), and is ignored for purely atomic pairs. Throws a
/// `RangeError` for `Tdb` or a non-finite `utcJd` when a leap count is needed.
#[wasm_bindgen(js_name = timescaleOffsetAtS)]
pub fn timescale_offset_at_s_js(
    from: TimeScale,
    to: TimeScale,
    utc_jd: f64,
) -> Result<f64, JsValue> {
    timescale_offset_at_s(from.into(), to.into(), utc_jd).map_err(|e| range_error(&e.to_string()))
}

/// A two-part Julian date: an integer-day boundary plus a residual fraction,
/// preserving sub-microsecond precision across the Julian-date range.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct JulianDate {
    whole: f64,
    fraction: f64,
}

/// Civil calendar fields from a no-leap core time conversion.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct CivilDateTime {
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
}

#[wasm_bindgen]
impl CivilDateTime {
    /// Civil year.
    #[wasm_bindgen(getter)]
    pub fn year(&self) -> i64 {
        self.year
    }

    /// Civil month, 1-12.
    #[wasm_bindgen(getter)]
    pub fn month(&self) -> i64 {
        self.month
    }

    /// Civil day of month.
    #[wasm_bindgen(getter)]
    pub fn day(&self) -> i64 {
        self.day
    }

    /// Civil hour of day.
    #[wasm_bindgen(getter)]
    pub fn hour(&self) -> i64 {
        self.hour
    }

    /// Civil minute of hour.
    #[wasm_bindgen(getter)]
    pub fn minute(&self) -> i64 {
        self.minute
    }

    /// Whole civil second of minute.
    #[wasm_bindgen(getter)]
    pub fn second(&self) -> i64 {
        self.second
    }
}

/// Continuous seconds since J2000 for a civil instant.
#[wasm_bindgen(js_name = civilToJ2000Seconds)]
pub fn civil_to_j2000_seconds(
    year: i32,
    month: i32,
    day: i32,
    hour: i32,
    minute: i32,
    second: f64,
) -> f64 {
    j2000_seconds(year, month, day, hour, minute, second)
}

/// Continuous seconds since J2000 for a split Julian date.
#[wasm_bindgen(js_name = splitJdToJ2000Seconds)]
pub fn split_jd_to_j2000_seconds(jd_whole: f64, jd_fraction: f64) -> f64 {
    j2000_seconds_from_split(jd_whole, jd_fraction)
}

/// Civil calendar fields from whole seconds since J2000.
#[wasm_bindgen(js_name = j2000SecondsToCivil)]
pub fn j2000_seconds_to_civil(seconds: i64) -> CivilDateTime {
    let (year, month, day, hour, minute, second) = civil_from_j2000_seconds(seconds);
    CivilDateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
    }
}

#[wasm_bindgen]
impl JulianDate {
    /// Build the no-leap civil UTC two-part Julian date for calendar fields.
    ///
    /// Delegates to `sidereon_core::astro::time::model::Instant::from_utc_civil`:
    /// `(year, month, day, hour, minute, second)` are marshalled through the
    /// engine's `split_julian_date`, tagged UTC, with no leap second applied
    /// (the civil convention the ionosphere / troposphere dispatchers consume).
    /// This is distinct from the leap-aware `Instant`, whose TT/UT1/TDB scales
    /// run the full UTC conversion. `second` may be fractional. Throws a
    /// `RangeError` on an out-of-day field whose residual leaves the one-day
    /// fraction window.
    #[wasm_bindgen(js_name = fromUtcCivil)]
    pub fn from_utc_civil(
        year: i32,
        month: i32,
        day: i32,
        hour: Option<i32>,
        minute: Option<i32>,
        second: Option<f64>,
    ) -> Result<JulianDate, JsValue> {
        let instant = CoreInstant::from_utc_civil(
            year,
            month,
            day,
            hour.unwrap_or(0),
            minute.unwrap_or(0),
            second.unwrap_or(0.0),
        )
        .map_err(|e| range_error(&e.to_string()))?;
        let split = instant
            .julian_date()
            .ok_or_else(|| engine_error("civil instant did not resolve to a split Julian date"))?;
        Ok(JulianDate {
            whole: split.jd_whole,
            fraction: split.fraction,
        })
    }

    /// Integer-day boundary (typically `*.0` or `*.5`).
    #[wasm_bindgen(getter)]
    pub fn whole(&self) -> f64 {
        self.whole
    }

    /// Residual day fraction relative to `whole`.
    #[wasm_bindgen(getter)]
    pub fn fraction(&self) -> f64 {
        self.fraction
    }

    /// The recombined single-`number` Julian date (`whole + fraction`).
    #[wasm_bindgen(getter)]
    pub fn jd(&self) -> f64 {
        self.whole + self.fraction
    }
}

/// A point in time, tagged UTC, with the precise time scales resolved.
///
/// Construct from a unix-microsecond UTC stamp (a `bigint`) or from UTC calendar
/// fields. The resolved TT/UT1/TDB Julian dates, sidereal time, and
/// precession/nutation are exposed as read-only properties and methods, all from
/// the engine's parity-critical pipeline.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct Instant {
    unix_micros: i64,
}

impl Instant {
    fn time_scales(&self) -> TimeScales {
        UtcInstant::from_unix_microseconds(self.unix_micros).time_scales()
    }
}

#[wasm_bindgen]
impl Instant {
    /// Build an instant from a unix-microsecond UTC stamp.
    #[wasm_bindgen(js_name = fromUnixMicros)]
    pub fn from_unix_micros(unix_micros: i64) -> Instant {
        Instant { unix_micros }
    }

    /// Build an instant from UTC calendar fields. `second` may be fractional and
    /// is held to microsecond resolution. Throws a `RangeError` on an
    /// out-of-range calendar field.
    #[wasm_bindgen(js_name = fromUtc)]
    pub fn from_utc(
        year: i32,
        month: i32,
        day: i32,
        hour: Option<i32>,
        minute: Option<i32>,
        second: Option<f64>,
    ) -> Result<Instant, JsValue> {
        let hour = hour.unwrap_or(0);
        let minute = minute.unwrap_or(0);
        let second = second.unwrap_or(0.0);
        if !second.is_finite() || second < 0.0 {
            return Err(range_error("second must be finite and non-negative"));
        }
        let whole_second = second.trunc() as i32;
        let microsecond = (second.fract() * MICROSECONDS_PER_SECOND).round() as i32;
        let instant =
            UtcInstant::from_utc(year, month, day, hour, minute, whole_second, microsecond)
                .ok_or_else(|| range_error("UTC calendar field out of range"))?;
        Ok(Instant {
            unix_micros: instant.unix_microseconds(),
        })
    }

    /// The unix-microsecond UTC stamp backing this instant.
    #[wasm_bindgen(getter, js_name = unixMicros)]
    pub fn unix_micros(&self) -> i64 {
        self.unix_micros
    }

    /// The shared integer Julian-day boundary (TAI-aligned).
    #[wasm_bindgen(getter, js_name = jdWhole)]
    pub fn jd_whole(&self) -> f64 {
        self.time_scales().jd_whole
    }

    /// Full Terrestrial Time (TT) Julian date.
    #[wasm_bindgen(getter, js_name = ttJd)]
    pub fn tt_jd(&self) -> f64 {
        self.time_scales().jd_tt
    }

    /// Full UT1 Julian date.
    #[wasm_bindgen(getter, js_name = ut1Jd)]
    pub fn ut1_jd(&self) -> f64 {
        self.time_scales().jd_ut1
    }

    /// Full Barycentric Dynamical Time (TDB) Julian date.
    #[wasm_bindgen(getter, js_name = tdbJd)]
    pub fn tdb_jd(&self) -> f64 {
        self.time_scales().jd_tdb
    }

    /// TT day fraction relative to `jdWhole`.
    #[wasm_bindgen(getter, js_name = ttFraction)]
    pub fn tt_fraction(&self) -> f64 {
        self.time_scales().tt_fraction
    }

    /// UT1 day fraction relative to `jdWhole`.
    #[wasm_bindgen(getter, js_name = ut1Fraction)]
    pub fn ut1_fraction(&self) -> f64 {
        self.time_scales().ut1_fraction
    }

    /// TDB day fraction relative to `jdWhole`.
    #[wasm_bindgen(getter, js_name = tdbFraction)]
    pub fn tdb_fraction(&self) -> f64 {
        self.time_scales().tdb_fraction
    }

    /// The two-part TT Julian date (`jdWhole`, `ttFraction`).
    #[wasm_bindgen(getter, js_name = ttJdSplit)]
    pub fn tt_jd_split(&self) -> JulianDate {
        let ts = self.time_scales();
        JulianDate {
            whole: ts.jd_whole,
            fraction: ts.tt_fraction,
        }
    }

    /// The two-part UT1 Julian date (`jdWhole`, `ut1Fraction`).
    #[wasm_bindgen(getter, js_name = ut1JdSplit)]
    pub fn ut1_jd_split(&self) -> JulianDate {
        let ts = self.time_scales();
        JulianDate {
            whole: ts.jd_whole,
            fraction: ts.ut1_fraction,
        }
    }

    /// The two-part TDB Julian date (`jdWhole`, `tdbFraction`).
    #[wasm_bindgen(getter, js_name = tdbJdSplit)]
    pub fn tdb_jd_split(&self) -> JulianDate {
        let ts = self.time_scales();
        JulianDate {
            whole: ts.jd_whole,
            fraction: ts.tdb_fraction,
        }
    }

    /// Delta-T (TT minus UT1), seconds.
    #[wasm_bindgen(getter, js_name = deltaTSeconds)]
    pub fn delta_t_seconds(&self) -> f64 {
        let ts = self.time_scales();
        (ts.tt_fraction - ts.ut1_fraction) * SECONDS_PER_DAY
    }

    /// IAU mean obliquity of the ecliptic, radians.
    #[wasm_bindgen(getter, js_name = meanObliquityRadians)]
    pub fn mean_obliquity_radians(&self) -> Result<f64, JsValue> {
        skyfield_mean_obliquity_radians(self.time_scales().jd_tdb).map_err(engine_error)
    }

    /// Greenwich Mean Sidereal Time, radians in `[0, 2pi)`.
    #[wasm_bindgen(js_name = gmstRadians)]
    pub fn gmst_radians(&self) -> Result<f64, JsValue> {
        greenwich_mean_sidereal_time_radians(&self.time_scales()).map_err(engine_error)
    }

    /// Greenwich Apparent Sidereal Time, radians in `[0, 2pi)`.
    #[wasm_bindgen(js_name = gastRadians)]
    pub fn gast_radians(&self) -> Result<f64, JsValue> {
        greenwich_apparent_sidereal_time_radians(&self.time_scales()).map_err(engine_error)
    }

    /// IAU 2000A nutation in longitude and obliquity `[dpsi, deps]`, radians,
    /// as a `Float64Array` of length 2.
    #[wasm_bindgen(js_name = nutationAngles)]
    pub fn nutation_angles(&self) -> Result<Vec<f64>, JsValue> {
        let (dpsi, deps) =
            skyfield_iau2000a_radians(self.time_scales().jd_tt).map_err(engine_error)?;
        Ok(vec![dpsi, deps])
    }

    /// IAU 2006 precession rotation matrix as a flat row-major `Float64Array` of
    /// length 9 (3-by-3).
    #[wasm_bindgen(js_name = precessionMatrix)]
    pub fn precession_matrix(&self) -> Result<Vec<f64>, JsValue> {
        let m =
            compute_skyfield_precession_matrix(self.time_scales().jd_tdb).map_err(engine_error)?;
        Ok(mat3_flat(&m))
    }

    /// IAU 2000A nutation rotation matrix as a flat row-major `Float64Array` of
    /// length 9 (3-by-3).
    #[wasm_bindgen(js_name = nutationMatrix)]
    pub fn nutation_matrix(&self) -> Result<Vec<f64>, JsValue> {
        let ts = self.time_scales();
        let (dpsi, deps) = skyfield_iau2000a_radians(ts.jd_tt).map_err(engine_error)?;
        let mean_ob = skyfield_mean_obliquity_radians(ts.jd_tdb).map_err(engine_error)?;
        let m =
            build_skyfield_nutation_matrix(mean_ob, mean_ob + deps, dpsi).map_err(engine_error)?;
        Ok(mat3_flat(&m))
    }
}

/// A GNSS week number plus time-of-week, tagged by constellation.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct GnssWeekTow {
    inner: CoreGnssWeekTow,
}

#[wasm_bindgen]
impl GnssWeekTow {
    /// Create a week/TOW. Throws a `RangeError` on an invalid value.
    #[wasm_bindgen(constructor)]
    pub fn new(system: TimeScale, week: u32, tow_s: f64) -> Result<GnssWeekTow, JsValue> {
        Ok(GnssWeekTow {
            inner: CoreGnssWeekTow::new(system.into(), week, tow_s)
                .map_err(|e| range_error(&e.to_string()))?,
        })
    }

    /// The constellation/system whose week/TOW convention this uses.
    #[wasm_bindgen(getter)]
    pub fn system(&self) -> TimeScale {
        self.inner.system.into()
    }

    /// Week number (constellation-native, may have rolled over).
    #[wasm_bindgen(getter)]
    pub fn week(&self) -> u32 {
        self.inner.week
    }

    /// Time of week, seconds, nominally `[0, 604800)`.
    #[wasm_bindgen(getter, js_name = towS)]
    pub fn tow_s(&self) -> f64 {
        self.inner.tow_s
    }

    /// Normalize so `towS` lands in `[0, 604800)`, carrying whole weeks into
    /// `week`. Negative `towS` borrows from the week count.
    pub fn normalized(&self) -> Result<GnssWeekTow, JsValue> {
        Ok(GnssWeekTow {
            inner: self
                .inner
                .normalized()
                .map_err(|e| range_error(&e.to_string()))?,
        })
    }

    /// Apply a 1024-week rollover count to recover the continuous week number.
    #[wasm_bindgen(js_name = unrolledWeek)]
    pub fn unrolled_week(&self, rollovers: u32) -> Result<u32, JsValue> {
        self.inner
            .unrolled_week(rollovers)
            .map_err(|e| range_error(&e.to_string()))
    }
}

/// Provenance and coverage of the embedded IERS leap-second (TAI-UTC) table.
#[wasm_bindgen]
pub struct LeapSecondTable {
    source: String,
    first_mjd: i32,
    last_mjd: i32,
    entries: usize,
}

#[wasm_bindgen]
impl LeapSecondTable {
    /// Human-readable provenance string for the table.
    #[wasm_bindgen(getter)]
    pub fn source(&self) -> String {
        self.source.clone()
    }

    /// Modified Julian date of the first table entry.
    #[wasm_bindgen(getter, js_name = firstMjd)]
    pub fn first_mjd(&self) -> i32 {
        self.first_mjd
    }

    /// Modified Julian date of the last table entry.
    #[wasm_bindgen(getter, js_name = lastMjd)]
    pub fn last_mjd(&self) -> i32 {
        self.last_mjd
    }

    /// Number of entries in the table.
    #[wasm_bindgen(getter)]
    pub fn entries(&self) -> usize {
        self.entries
    }
}

/// Provenance and coverage of the embedded UT1-UTC / delta-T (EOP) table.
#[wasm_bindgen]
pub struct Ut1Coverage {
    source: String,
    first_mjd: i32,
    last_mjd: i32,
    first_jd_tt: f64,
    last_jd_tt: f64,
    entries: usize,
}

#[wasm_bindgen]
impl Ut1Coverage {
    /// Human-readable provenance string for the table.
    #[wasm_bindgen(getter)]
    pub fn source(&self) -> String {
        self.source.clone()
    }

    /// Modified Julian date of the first table entry.
    #[wasm_bindgen(getter, js_name = firstMjd)]
    pub fn first_mjd(&self) -> i32 {
        self.first_mjd
    }

    /// Modified Julian date of the last table entry.
    #[wasm_bindgen(getter, js_name = lastMjd)]
    pub fn last_mjd(&self) -> i32 {
        self.last_mjd
    }

    /// TT Julian date of the first table entry (coverage lower bound).
    #[wasm_bindgen(getter, js_name = firstJdTt)]
    pub fn first_jd_tt(&self) -> f64 {
        self.first_jd_tt
    }

    /// TT Julian date of the last table entry (coverage upper bound).
    #[wasm_bindgen(getter, js_name = lastJdTt)]
    pub fn last_jd_tt(&self) -> f64 {
        self.last_jd_tt
    }

    /// Number of entries in the table.
    #[wasm_bindgen(getter)]
    pub fn entries(&self) -> usize {
        self.entries
    }
}

/// TAI-UTC (cumulative leap seconds) in effect on a UTC calendar date.
#[wasm_bindgen(js_name = leapSeconds)]
pub fn leap_seconds(year: i32, month: i32, day: i32) -> f64 {
    let jd_utc_midnight = julian_day_number(year, month, day) as f64 - 0.5;
    find_leap_seconds(jd_utc_midnight)
}

/// Provenance and coverage of the embedded leap-second (TAI-UTC) table.
#[wasm_bindgen(js_name = leapSecondTableInfo)]
pub fn leap_second_table_info() -> LeapSecondTable {
    let table = leap_second_table();
    LeapSecondTable {
        source: table.source.to_string(),
        first_mjd: table.first_mjd,
        last_mjd: table.last_mjd,
        entries: table.entries,
    }
}

/// Provenance and coverage of the embedded UT1-UTC / delta-T (EOP) table.
#[wasm_bindgen(js_name = ut1CoverageInfo)]
pub fn ut1_coverage_info() -> Ut1Coverage {
    let prov = ut1_coverage();
    Ut1Coverage {
        source: prov.source.to_string(),
        first_mjd: prov.first_mjd,
        last_mjd: prov.last_mjd,
        first_jd_tt: prov.first_jd_tt,
        last_jd_tt: prov.last_jd_tt,
        entries: prov.entries,
    }
}

/// A batch of transformed states from [`temeToGcrs`]: flat row-major
/// `positionKm` and `velocityKmS` `Float64Array`s, each length `3 * epochCount`.
#[wasm_bindgen]
pub struct FrameStates {
    positions: Vec<f64>,
    velocities: Vec<f64>,
}

#[wasm_bindgen]
impl FrameStates {
    /// Transformed positions, kilometres, flat row-major `(n, 3)`.
    #[wasm_bindgen(getter, js_name = positionKm)]
    pub fn position_km(&self) -> Vec<f64> {
        self.positions.clone()
    }

    /// Transformed velocities, km/s, flat row-major `(n, 3)`.
    #[wasm_bindgen(getter, js_name = velocityKmS)]
    pub fn velocity_km_s(&self) -> Vec<f64> {
        self.velocities.clone()
    }

    /// Number of states in the batch.
    #[wasm_bindgen(getter, js_name = epochCount)]
    pub fn epoch_count(&self) -> usize {
        self.positions.len() / 3
    }
}

fn scales_from_epochs(epochs_unix_us: &[i64]) -> Result<Vec<TimeScales>, JsValue> {
    if epochs_unix_us.is_empty() {
        return Err(type_error("epochsUnixUs must not be empty"));
    }
    Ok(epochs_unix_us
        .iter()
        .map(|&us| UtcInstant::from_unix_microseconds(us).time_scales())
        .collect())
}

/// Transform a batch of TEME states to GCRS, each at its own epoch.
///
/// `positionKm` and `velocityKmS` are flat row-major `(n, 3)` `Float64Array`s;
/// `epochsUnixUs` is a `BigInt64Array` of unix-microsecond UTC stamps.
/// `skyfieldCompat` (default true) selects the AU-scaled Skyfield-parity path.
#[wasm_bindgen(js_name = temeToGcrs)]
pub fn teme_to_gcrs(
    position_km: &[f64],
    velocity_km_s: &[f64],
    epochs_unix_us: &[i64],
    skyfield_compat: Option<bool>,
) -> Result<FrameStates, JsValue> {
    let compat = skyfield_compat.unwrap_or(true);
    let positions = rows3("positionKm", position_km, false)?;
    let velocities = rows3("velocityKmS", velocity_km_s, false)?;
    let scales = scales_from_epochs(epochs_unix_us)?;
    same_len("positionKm rows", positions.len(), "epochs", scales.len())?;
    same_len("velocityKmS rows", velocities.len(), "epochs", scales.len())?;

    let mut out_pos = Vec::with_capacity(scales.len());
    let mut out_vel = Vec::with_capacity(scales.len());
    for ((pos, vel), ts) in positions.iter().zip(velocities.iter()).zip(scales.iter()) {
        let (p, v) = teme_to_gcrs_compute(
            &TemeStateKm {
                position_km: *pos,
                velocity_km_s: *vel,
            },
            ts,
            compat,
        )
        .map_err(engine_error)?;
        out_pos.push([p.0, p.1, p.2]);
        out_vel.push([v.0, v.1, v.2]);
    }
    Ok(FrameStates {
        positions: flat3(&out_pos),
        velocities: flat3(&out_vel),
    })
}

/// Transform a batch of GCRS positions to ITRS (Earth-fixed / ECEF), each at its
/// own epoch. Returns a flat row-major `(n, 3)` `Float64Array`.
#[wasm_bindgen(js_name = gcrsToItrs)]
pub fn gcrs_to_itrs(
    position_km: &[f64],
    epochs_unix_us: &[i64],
    skyfield_compat: Option<bool>,
) -> Result<Vec<f64>, JsValue> {
    let compat = skyfield_compat.unwrap_or(true);
    let positions = rows3("positionKm", position_km, false)?;
    let scales = scales_from_epochs(epochs_unix_us)?;
    same_len("positionKm rows", positions.len(), "epochs", scales.len())?;
    let out: Vec<[f64; 3]> = positions
        .iter()
        .zip(scales.iter())
        .map(|(p, ts)| {
            gcrs_to_itrs_compute(p[0], p[1], p[2], ts, compat)
                .map(|(x, y, z)| [x, y, z])
                .map_err(engine_error)
        })
        .collect::<Result<Vec<_>, JsValue>>()?;
    Ok(flat3(&out))
}

/// Transform a batch of ITRS (ECEF) positions to GCRS, each at its own epoch.
/// Returns a flat row-major `(n, 3)` `Float64Array`.
#[wasm_bindgen(js_name = itrsToGcrs)]
pub fn itrs_to_gcrs(position_km: &[f64], epochs_unix_us: &[i64]) -> Result<Vec<f64>, JsValue> {
    let positions = rows3("positionKm", position_km, false)?;
    let scales = scales_from_epochs(epochs_unix_us)?;
    same_len("positionKm rows", positions.len(), "epochs", scales.len())?;
    let out: Vec<[f64; 3]> = positions
        .iter()
        .zip(scales.iter())
        .map(|(p, ts)| {
            itrs_to_gcrs_compute(p[0], p[1], p[2], ts)
                .map(|(x, y, z)| [x, y, z])
                .map_err(engine_error)
        })
        .collect::<Result<Vec<_>, JsValue>>()?;
    Ok(flat3(&out))
}

/// Convert a batch of geodetic coordinates to ITRS (ECEF).
///
/// `geodetic` is a flat row-major `(n, 3)` `Float64Array` whose columns are
/// `[latitudeDeg, longitudeDeg, altitudeKm]` (WGS84). Returns a flat `(n, 3)`
/// ITRS `Float64Array` in kilometres. Time-independent.
#[wasm_bindgen(js_name = geodeticToEcef)]
pub fn geodetic_to_ecef(geodetic: &[f64]) -> Result<Vec<f64>, JsValue> {
    let rows = rows3("geodetic", geodetic, false)?;
    let out: Vec<[f64; 3]> = rows
        .iter()
        .map(|g| {
            geodetic_to_itrs(g[0], g[1], g[2])
                .map(|(x, y, z)| [x, y, z])
                .map_err(engine_error)
        })
        .collect::<Result<Vec<_>, JsValue>>()?;
    Ok(flat3(&out))
}

/// Convert a batch of ITRS (ECEF) positions to geodetic coordinates.
///
/// `positionKm` is a flat row-major `(n, 3)` `Float64Array` in kilometres.
/// Returns a flat `(n, 3)` `Float64Array` whose columns are
/// `[latitudeDeg, longitudeDeg, altitudeKm]` (WGS84). Time-independent.
#[wasm_bindgen(js_name = ecefToGeodetic)]
pub fn ecef_to_geodetic(position_km: &[f64]) -> Result<Vec<f64>, JsValue> {
    let rows = rows3("positionKm", position_km, false)?;
    let out: Vec<[f64; 3]> = rows
        .iter()
        .map(|p| {
            itrs_to_geodetic_compute(p[0], p[1], p[2])
                .map(|(lat, lon, alt)| [lat, lon, alt])
                .map_err(engine_error)
        })
        .collect::<Result<Vec<_>, JsValue>>()?;
    Ok(flat3(&out))
}

/// Leap seconds for a batch of UTC dates, as a `Float64Array`. `dates` is a flat
/// row-major `(n, 3)` `Int32Array` (or `number[]`) of `[year, month, day]`.
#[wasm_bindgen(js_name = leapSecondsBatch)]
pub fn leap_seconds_batch(dates: &[i32]) -> Result<Vec<f64>, JsValue> {
    if !dates.len().is_multiple_of(3) {
        return Err(type_error(
            "dates length must be a multiple of 3 (year, month, day)",
        ));
    }
    Ok(dates
        .chunks_exact(3)
        .map(|d| leap_seconds(d[0], d[1], d[2]))
        .collect())
}
