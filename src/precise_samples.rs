//! Sample-backed precise-ephemeris source and geometry-only batch range
//! prediction.
//!
//! The canonical precise-ephemeris intermediate representation is a set of
//! per-satellite ECEF position (+ optional clock) samples on a time axis; SP3
//! text is one serialization of it. This module marshals that IR across the JS
//! boundary as plain objects, builds the sample-backed source
//! (`sidereon_core::sp3::PreciseEphemerisSamples`, an
//! `ObservableEphemerisSource`), extracts the samples from a parsed SP3 product,
//! and runs the geometry-only batch predictor
//! (`sidereon_core::observables::predict_ranges`) over either source in one call.
//!
//! Every value delegates to `sidereon-core`; this module only marshals JS input
//! and output. The batch predictor is the serial reference kernel; the binding
//! never spawns the rayon thread pool the parallel variants use.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::astro::time::civil::j2000_seconds_from_split;
use sidereon_core::astro::time::model::{Instant, InstantRepr, JulianDateSplit, TimeScale};
use sidereon_core::constants::{J2000_JD, SECONDS_PER_DAY};
use sidereon_core::ephemeris::{
    sample as core_sample, EphemerisSampleStatus, PreciseEphemerisSample as CoreSample,
    PreciseEphemerisSamples as CoreSamples, PreciseSamplesError,
};
use sidereon_core::observables::{
    predict_ranges as core_predict_ranges, ObservableEphemerisSource, RangePrediction,
    RangePredictionRequest,
};
use sidereon_core::GnssSatelliteId;

use crate::error::{engine_error, range_error, type_error};
use crate::rinex_nav::BroadcastEphemeris;
use crate::sp3::Sp3;

fn parse_sat(token: &str) -> Result<GnssSatelliteId, JsValue> {
    token
        .parse::<GnssSatelliteId>()
        .map_err(|e| type_error(&format!("invalid satellite token {token:?}: {e}")))
}

/// Serialize a value to a plain JS object/array, mapping `None` to `null` and
/// Rust arrays to JS arrays.
fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| engine_error(format!("failed to serialize result: {e}")))
}

/// Convert a parsed SP3 instant to seconds since J2000 in its own time scale,
/// using the same split reduction the SP3 node axis uses.
fn instant_to_j2000_seconds(epoch: &Instant) -> f64 {
    match epoch.repr {
        InstantRepr::JulianDate(jd) => j2000_seconds_from_split(jd.jd_whole, jd.fraction),
        InstantRepr::Nanos(_) => f64::NAN,
    }
}

/// Rebuild a Julian-date split from a J2000 second, keeping the whole-day count
/// separate from the `J2000_JD` anchor so the fraction never absorbs the
/// ~2.45e6 magnitude of the absolute Julian date (the shared idiom with the
/// broadcast comparator).
fn j2000_to_split(t_j2000_s: f64) -> Result<JulianDateSplit, JsValue> {
    if !t_j2000_s.is_finite() {
        return Err(range_error("sample epoch must be a finite number"));
    }
    let days = t_j2000_s / SECONDS_PER_DAY;
    let whole = J2000_JD + days.floor();
    let fraction = days - days.floor();
    JulianDateSplit::new(whole, fraction).map_err(engine_error)
}

/// One precise-ephemeris sample crossing the JS boundary: a satellite's ECEF
/// position (and optional clock) at one epoch, in SI units.
///
/// `epoch` is seconds since J2000 in the source's own time scale (the scale is
/// not carried: the geometry-only prediction and interpolation are scale-free,
/// keyed purely on J2000 seconds, mirroring the SP3 module's epoch numbers).
/// `positionEcefM` is the ITRF/IGS ECEF position `[x, y, z]` in meters, `clockS`
/// the satellite clock offset in seconds (`null` when the source carried none),
/// and `clockEvent` mirrors the SP3 `E` clock-event flag (defaults `false`): a
/// `true` splits the clock interpolation arc at a clock reset.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SampleJs {
    sat: String,
    epoch: f64,
    position_ecef_m: [f64; 3],
    #[serde(default)]
    clock_s: Option<f64>,
    #[serde(default)]
    clock_event: bool,
}

/// Map a sample-source validation failure to the JS exception a caller expects.
fn samples_error(err: PreciseSamplesError) -> JsValue {
    range_error(&err.to_string())
}

/// A precise-ephemeris source built from samples rather than parsed SP3 text.
///
/// Implements the same `ObservableEphemerisSource` contract as a parsed [`Sp3`]
/// product and shares its interpolation substrate, so [`predictRanges`] accepts
/// either handle. Build one with [`preciseEphemerisSamplesFromSamples`].
#[wasm_bindgen]
pub struct PreciseEphemerisSampleSource {
    pub(crate) inner: CoreSamples,
}

#[wasm_bindgen]
impl PreciseEphemerisSampleSource {
    /// The satellites this source can interpolate (e.g. `"G01"`), ascending.
    #[wasm_bindgen(getter)]
    pub fn satellites(&self) -> Vec<String> {
        self.inner.satellites().map(|sat| sat.to_string()).collect()
    }

    /// Predict geometric ranges for many requests in one call. See the shared
    /// [`predictRanges`](crate::precise_samples::predict_ranges_over) contract;
    /// this is the sample-source entry point.
    #[wasm_bindgen(js_name = predictRanges)]
    pub fn predict_ranges(&self, requests: JsValue, options: JsValue) -> Result<JsValue, JsValue> {
        predict_ranges_over(&self.inner, requests, options)
    }
}

/// Build a sample-backed precise-ephemeris source from an array of samples.
///
/// `samples` is an array of `{ sat, epoch, positionEcefM, clockS?, clockEvent? }`
/// objects (see the sample field docs). Samples are grouped by satellite in their
/// supplied order; each satellite needs at least two strictly time-increasing
/// samples. Throws a `TypeError` for a malformed object or bad satellite token
/// and a `RangeError` for a non-finite epoch or a source validation failure
/// (empty input, a single-sample satellite, non-monotonic epochs, a non-finite
/// sample). Delegates to `sidereon_core::sp3::PreciseEphemerisSamples::from_samples`.
#[wasm_bindgen(js_name = preciseEphemerisSamplesFromSamples)]
pub fn precise_ephemeris_samples_from_samples(
    samples: JsValue,
) -> Result<PreciseEphemerisSampleSource, JsValue> {
    let samples: Vec<SampleJs> = serde_wasm_bindgen::from_value(samples)
        .map_err(|e| type_error(&format!("invalid samples: {e}")))?;

    let mut core_samples = Vec::with_capacity(samples.len());
    for sample in samples {
        let sat = parse_sat(&sample.sat)?;
        let split = j2000_to_split(sample.epoch)?;
        core_samples.push(CoreSample {
            sat,
            epoch: Instant::from_julian_date(TimeScale::Gpst, split),
            position_ecef_m: sample.position_ecef_m,
            clock_s: sample.clock_s,
            clock_event: sample.clock_event,
        });
    }

    let inner = CoreSamples::from_samples(core_samples).map_err(samples_error)?;
    Ok(PreciseEphemerisSampleSource { inner })
}

/// Extract a parsed SP3 product as the canonical precise-ephemeris samples, one
/// per real position record in ascending epoch order.
///
/// Returns an array of `{ sat, epoch, positionEcefM, clockS, clockEvent }`
/// objects. Round-tripping the result back through
/// [`preciseEphemerisSamplesFromSamples`] rebuilds an interpolatable source that
/// reproduces the SP3-parsed source's interpolated states and predicted ranges
/// to the documented round-trip precision (byte-identical for samples whose
/// meters are the faithful image of the fit nodes; see the core module docs).
/// Delegates to `sidereon_core::sp3::Sp3::precise_ephemeris_samples`.
#[wasm_bindgen(js_name = sp3PreciseEphemerisSamples)]
pub fn sp3_precise_ephemeris_samples(sp3: &Sp3) -> Result<JsValue, JsValue> {
    let samples: Vec<SampleJs> = sp3
        .inner
        .precise_ephemeris_samples()
        .into_iter()
        .map(|s| SampleJs {
            sat: s.sat.to_string(),
            epoch: instant_to_j2000_seconds(&s.epoch),
            position_ecef_m: s.position_ecef_m,
            clock_s: s.clock_s,
            clock_event: s.clock_event,
        })
        .collect();
    to_js(&samples)
}

/// One batch range-prediction request crossing the JS boundary: the satellite
/// token, the static receiver ECEF position `[x, y, z]` in meters, and the
/// receive epoch in seconds since J2000.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RangeRequestJs {
    sat: String,
    receiver_ecef_m: [f64; 3],
    t_rx_j2000_s: f64,
}

/// The geometry-only result of one [`predictRanges`] request.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RangePredictionJs {
    geometric_range_m: f64,
    sat_clock_s: Option<f64>,
    transmit_time_j2000_s: f64,
    sat_pos_ecef_m: [f64; 3],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EphemerisSampleRowJs {
    sat: String,
    epoch_j2000_s: f64,
    status: &'static str,
    position_ecef_m: Option<[f64; 3]>,
    clock_s: Option<f64>,
}

fn sample_status_label(status: EphemerisSampleStatus) -> &'static str {
    match status {
        EphemerisSampleStatus::Valid => "valid",
        EphemerisSampleStatus::Gap => "gap",
    }
}

fn sample_over(
    source: &dyn ObservableEphemerisSource,
    satellites: Vec<String>,
    start_j2000_s: f64,
    stop_j2000_s: f64,
    step_s: f64,
) -> Result<JsValue, JsValue> {
    let satellites = satellites
        .iter()
        .map(|sat| parse_sat(sat))
        .collect::<Result<Vec<_>, _>>()?;
    let rows = core_sample(source, &satellites, start_j2000_s, stop_j2000_s, step_s)
        .map_err(engine_error)?;
    let out: Vec<EphemerisSampleRowJs> = rows
        .into_iter()
        .map(|row| EphemerisSampleRowJs {
            sat: row.sat.to_string(),
            epoch_j2000_s: row.epoch_j2000_s,
            status: sample_status_label(row.status),
            position_ecef_m: row.position_ecef_m,
            clock_s: row.clock_s,
        })
        .collect();
    to_js(&out)
}

#[wasm_bindgen(js_name = sampleSp3Ephemeris)]
pub fn sample_sp3_ephemeris(
    sp3: &Sp3,
    satellites: Vec<String>,
    start_j2000_s: f64,
    stop_j2000_s: f64,
    step_s: f64,
) -> Result<JsValue, JsValue> {
    sample_over(&sp3.inner, satellites, start_j2000_s, stop_j2000_s, step_s)
}

#[wasm_bindgen(js_name = sampleBroadcastEphemeris)]
pub fn sample_broadcast_ephemeris(
    broadcast: &BroadcastEphemeris,
    satellites: Vec<String>,
    start_j2000_s: f64,
    stop_j2000_s: f64,
    step_s: f64,
) -> Result<JsValue, JsValue> {
    sample_over(
        &broadcast.inner,
        satellites,
        start_j2000_s,
        stop_j2000_s,
        step_s,
    )
}

/// Predict geometric ranges for many `(satellite, receiver, epoch)` requests in
/// one call, over any `ObservableEphemerisSource`.
///
/// This is the shared kernel behind the `predictRanges` methods on both source
/// handles, so the same call accepts an [`Sp3`] product or a
/// [`PreciseEphemerisSampleSource`] (`source.predictRanges(requests, options)`),
/// mirroring how the observable batch predictors route both an SP3 and a
/// broadcast source through one `&dyn ObservableEphemerisSource` path.
///
/// `requests` is an array of `{ sat, receiverEcefM, tRxJ2000S }` objects and
/// `options` the shared `{ carrierHz?, lightTime?, sagnac? }` predict options
/// (`carrierHz` is unused for ranges; `lightTime` / `sagnac` are honored).
/// Returns an array of `{ geometricRangeM, satClockS, transmitTimeJ2000S,
/// satPosEcefM }`, index-aligned to `requests`. Throws a `TypeError` for a
/// malformed request, a `RangeError` for a non-finite receiver or epoch, and an
/// `Error` if a request has no ephemeris (the first request error aborts the
/// batch). Delegates to the serial reference kernel
/// `sidereon_core::observables::predict_ranges`; the binding never spawns a
/// rayon thread pool.
pub(crate) fn predict_ranges_over(
    source: &dyn ObservableEphemerisSource,
    requests: JsValue,
    options: JsValue,
) -> Result<JsValue, JsValue> {
    let requests: Vec<RangeRequestJs> = serde_wasm_bindgen::from_value(requests)
        .map_err(|e| type_error(&format!("invalid requests: {e}")))?;

    let mut core_requests = Vec::with_capacity(requests.len());
    for (i, request) in requests.iter().enumerate() {
        let sat = parse_sat(&request.sat)?;
        if request.receiver_ecef_m.iter().any(|c| !c.is_finite()) {
            return Err(range_error(&format!(
                "requests[{i}].receiverEcefM must contain only finite values"
            )));
        }
        if !request.t_rx_j2000_s.is_finite() {
            return Err(range_error(&format!(
                "requests[{i}].tRxJ2000S must be a finite number"
            )));
        }
        core_requests.push(RangePredictionRequest {
            sat,
            receiver_ecef_m: request.receiver_ecef_m,
            t_rx_j2000_s: request.t_rx_j2000_s,
        });
    }

    let options = crate::observables::predict_options(options)?;
    let mut out = vec![
        RangePrediction {
            geometric_range_m: 0.0,
            sat_clock_s: None,
            transmit_time_j2000_s: 0.0,
            sat_pos_ecef_m: [0.0; 3],
        };
        core_requests.len()
    ];

    core_predict_ranges(source, &core_requests, options, &mut out).map_err(engine_error)?;

    let results: Vec<RangePredictionJs> = out
        .into_iter()
        .map(|p| RangePredictionJs {
            geometric_range_m: p.geometric_range_m,
            sat_clock_s: p.sat_clock_s,
            transmit_time_j2000_s: p.transmit_time_j2000_s,
            sat_pos_ecef_m: p.sat_pos_ecef_m,
        })
        .collect();
    to_js(&results)
}
