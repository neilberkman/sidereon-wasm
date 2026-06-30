//! Broadcast-vs-precise ephemeris accuracy (the orbit/clock pieces of SISRE).
//!
//! `sidereon_core::broadcast_comparison::compare` differences a broadcast
//! navigation product against a precise SP3 product over a window, decomposes the
//! position error into radial/along-track/cross-track, and summarizes RMS/max per
//! satellite and overall. This module owns no statistics: it builds the per-epoch
//! evaluation keys from a J2000-second window (the broadcast query second plus
//! the SP3 split Julian dates for the epoch and its velocity-difference
//! neighbours) and packages the core report for JS.
//!
//! The window is in the precise product's time scale (GPST for IGS/MGEX), used
//! directly for the broadcast query as well; this matches the common case where
//! both products share GPS system time.

use serde::Serialize;
use wasm_bindgen::prelude::*;

use sidereon_core::astro::time::model::JulianDateSplit;
use sidereon_core::broadcast_comparison::{
    compare, compare_window, CompareReport as CoreReport, CompareStats, CompareWindow,
};
use sidereon_core::constants::{J2000_JD, SECONDS_PER_DAY};
use sidereon_core::GnssSatelliteId;

use crate::error::{engine_error, range_error, type_error};
use crate::rinex_nav::BroadcastEphemeris;
use crate::sp3::Sp3;

/// Difference statistics for one satellite (or the overall set), serialized to a
/// plain JS object. Every metre field is `null` when no compared epoch populated
/// it (an empty set, or no clocked epoch).
#[derive(Serialize)]
struct StatsJs {
    count: usize,
    #[serde(rename = "orbit3dRmsM")]
    orbit_3d_rms_m: Option<f64>,
    #[serde(rename = "orbit3dMaxM")]
    orbit_3d_max_m: Option<f64>,
    #[serde(rename = "radialRmsM")]
    radial_rms_m: Option<f64>,
    #[serde(rename = "radialMaxM")]
    radial_max_m: Option<f64>,
    #[serde(rename = "alongRmsM")]
    along_rms_m: Option<f64>,
    #[serde(rename = "alongMaxM")]
    along_max_m: Option<f64>,
    #[serde(rename = "crossRmsM")]
    cross_rms_m: Option<f64>,
    #[serde(rename = "crossMaxM")]
    cross_max_m: Option<f64>,
    #[serde(rename = "clockRmsM")]
    clock_rms_m: Option<f64>,
    #[serde(rename = "clockMaxM")]
    clock_max_m: Option<f64>,
    #[serde(rename = "clockDatumRemovedRmsM")]
    clock_datum_removed_rms_m: Option<f64>,
    #[serde(rename = "clockDatumRemovedMaxM")]
    clock_datum_removed_max_m: Option<f64>,
}

impl From<&CompareStats> for StatsJs {
    fn from(s: &CompareStats) -> Self {
        Self {
            count: s.count,
            orbit_3d_rms_m: s.orbit_3d_rms_m,
            orbit_3d_max_m: s.orbit_3d_max_m,
            radial_rms_m: s.radial_rms_m,
            radial_max_m: s.radial_max_m,
            along_rms_m: s.along_rms_m,
            along_max_m: s.along_max_m,
            cross_rms_m: s.cross_rms_m,
            cross_max_m: s.cross_max_m,
            clock_rms_m: s.clock_rms_m,
            clock_max_m: s.clock_max_m,
            clock_datum_removed_rms_m: s.clock_datum_removed_rms_m,
            clock_datum_removed_max_m: s.clock_datum_removed_max_m,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SatStatsJs {
    satellite_id: String,
    stats: StatsJs,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MissingJs {
    satellite_id: String,
    count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportJs {
    overall: StatsJs,
    per_satellite: Vec<SatStatsJs>,
    missing: Vec<MissingJs>,
}

impl From<&CoreReport> for ReportJs {
    fn from(r: &CoreReport) -> Self {
        Self {
            overall: StatsJs::from(&r.overall),
            per_satellite: r
                .per_satellite
                .iter()
                .map(|(sat, stats)| SatStatsJs {
                    satellite_id: sat.to_string(),
                    stats: StatsJs::from(stats),
                })
                .collect(),
            missing: r
                .missing
                .iter()
                .map(|(sat, count)| MissingJs {
                    satellite_id: sat.to_string(),
                    count: *count,
                })
                .collect(),
        }
    }
}

/// Split a J2000 second into a Julian-date `(whole, fraction)` pair. The
/// whole-day count is kept separate from the integer `J2000_JD` anchor so the
/// fraction never absorbs the ~2.45e6 magnitude of the absolute Julian date (a
/// single combined `J2000_JD + days` would shed the low bits before the floor).
fn j2000_to_split(t_j2000_s: f64) -> Result<JulianDateSplit, JsValue> {
    let days = t_j2000_s / SECONDS_PER_DAY;
    let whole = J2000_JD + days.floor();
    let fraction = days - days.floor();
    JulianDateSplit::new(whole, fraction).map_err(engine_error)
}

#[wasm_bindgen]
impl BroadcastEphemeris {
    /// Compare this broadcast product against a precise SP3 product over a
    /// J2000-second window `[fromJ2000S, toJ2000S]` at `stepS`.
    ///
    /// `satellites` is the list of tokens (e.g. `["G01", "G11"]`) to compare. The
    /// per-epoch keys are built here: the broadcast query second is the epoch
    /// itself, and the precise queries use the SP3 split Julian dates at the
    /// epoch and at `epoch +/- round(stepS / 2)` for the centered velocity
    /// difference. Returns a `{ overall, perSatellite, missing }` report; metre
    /// fields are `null` where no compared epoch populated them.
    #[wasm_bindgen(js_name = compareToSp3)]
    pub fn compare_to_sp3(
        &self,
        precise: &Sp3,
        satellites: Vec<String>,
        from_j2000_s: f64,
        to_j2000_s: f64,
        step_s: f64,
    ) -> Result<JsValue, JsValue> {
        if !step_s.is_finite() || step_s <= 0.0 {
            return Err(range_error("stepS must be a positive number"));
        }
        if !from_j2000_s.is_finite() || !to_j2000_s.is_finite() {
            return Err(range_error("fromJ2000S and toJ2000S must be finite"));
        }
        if to_j2000_s < from_j2000_s {
            return Err(range_error("toJ2000S must be >= fromJ2000S"));
        }

        let sats = satellites
            .iter()
            .map(|token| {
                token
                    .parse::<GnssSatelliteId>()
                    .map_err(|e| type_error(&format!("invalid satellite token {token:?}: {e}")))
            })
            .collect::<Result<Vec<_>, JsValue>>()?;

        let half_s = (step_s / 2.0).round();

        // Reject (rather than silently truncate) a window/step that would
        // generate an unreasonable number of epochs. Surfacing the bad request
        // is the contract; quietly comparing only a prefix would hide it.
        let max_epochs = 1_000_000usize;
        let epoch_count = ((to_j2000_s - from_j2000_s) / step_s).floor() + 1.0;
        if !epoch_count.is_finite() || epoch_count > max_epochs as f64 {
            return Err(range_error(&format!(
                "window/step would produce more than {max_epochs} epochs; widen stepS or shorten the window"
            )));
        }

        let mut epochs = Vec::with_capacity(epoch_count as usize);
        let mut t = from_j2000_s;
        while t <= to_j2000_s + 1.0e-6 {
            epochs.push(sidereon_core::broadcast_comparison::EpochInputs {
                broadcast_t_j2000_s: t,
                precise: j2000_to_split(t)?,
                precise_plus: j2000_to_split(t + half_s)?,
                precise_minus: j2000_to_split(t - half_s)?,
            });
            t += step_s;
        }

        let report =
            compare(&self.inner, &precise.inner, &sats, &epochs, half_s).map_err(engine_error)?;

        serde_wasm_bindgen::to_value(&ReportJs::from(&report))
            .map_err(|e| engine_error(e.to_string()))
    }

    /// Compare this broadcast product against a precise SP3 product over a regular
    /// sampling window, letting the core window driver build the per-epoch grid.
    ///
    /// The window-form sibling of [`compareToSp3`]: instead of building the
    /// per-epoch keys here, this hands the core
    /// `sidereon_core::broadcast_comparison::compare_window` a `CompareWindow`
    /// (the inclusive `[fromJ2000S, toJ2000S]` broadcast span, the precise split
    /// Julian-date anchor for the window start, the `stepS` sampling step, and the
    /// velocity finite-difference half step) so the grid sampling, the final snap
    /// to the window end, and the lockstep precise-date advance all run in core.
    /// The precise anchor is derived from `fromJ2000S` (both products share GPS
    /// system time, as in [`compareToSp3`]), and `velocityHalfS` defaults to
    /// `round(stepS / 2)`. Returns the same `{ overall, perSatellite, missing }`
    /// report shape.
    #[wasm_bindgen(js_name = compareWindowToSp3)]
    pub fn compare_window_to_sp3(
        &self,
        precise: &Sp3,
        satellites: Vec<String>,
        from_j2000_s: f64,
        to_j2000_s: f64,
        step_s: f64,
        velocity_half_s: Option<f64>,
    ) -> Result<JsValue, JsValue> {
        if !step_s.is_finite() || step_s <= 0.0 {
            return Err(range_error("stepS must be a positive number"));
        }
        if !from_j2000_s.is_finite() || !to_j2000_s.is_finite() {
            return Err(range_error("fromJ2000S and toJ2000S must be finite"));
        }

        let half_s = velocity_half_s.unwrap_or_else(|| (step_s / 2.0).round());
        if !half_s.is_finite() || half_s <= 0.0 {
            return Err(range_error("velocityHalfS must be a positive number"));
        }

        let sats = satellites
            .iter()
            .map(|token| {
                token
                    .parse::<GnssSatelliteId>()
                    .map_err(|e| type_error(&format!("invalid satellite token {token:?}: {e}")))
            })
            .collect::<Result<Vec<_>, JsValue>>()?;

        let window = CompareWindow {
            broadcast_window_j2000_s: (from_j2000_s, to_j2000_s),
            precise_start: j2000_to_split(from_j2000_s)?,
            step_s,
            velocity_half_s: half_s,
        };

        let report =
            compare_window(&self.inner, &precise.inner, &sats, &window).map_err(engine_error)?;

        serde_wasm_bindgen::to_value(&ReportJs::from(&report))
            .map_err(|e| engine_error(e.to_string()))
    }
}
