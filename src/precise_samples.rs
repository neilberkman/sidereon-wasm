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

use sidereon_core::astro::time::model::{
    Instant, InstantRepr, JulianDateSplit, TimeModelError, TimeScale,
};
use sidereon_core::constants::{J2000_JD, SECONDS_PER_DAY};
use sidereon_core::ephemeris::{
    sample as core_sample, EphemerisSampleStatus, ObservableStateBatch as CoreObservableStateBatch,
    ObservableStateElementStatus as CoreObservableStateElementStatus,
    PreciseEphemerisAccuracySample as CoreAccuracySample,
    PreciseEphemerisInterpolant as CorePreciseEphemerisInterpolant,
    PreciseEphemerisSample as CoreSample, PreciseEphemerisSamples as CoreSamples,
    PreciseSamplesError, Sp3AccuracyValue as CoreAccuracyValue, Sp3InterpolationOptions,
    OBSERVABLE_STATE_MISSING_POSITION_ECEF_M,
};
use sidereon_core::observables::{
    observable_states_at_j2000_s as core_observable_states_at_j2000_s,
    observable_states_at_shared_j2000_s as core_observable_states_at_shared_j2000_s,
    predict_ranges as core_predict_ranges, ObservableEphemerisSource,
    ObservablesError as CoreObservablesError, RangePrediction, RangePredictionRequest,
};
use sidereon_core::GnssSatelliteId;

use crate::core_error::{core_error_js, observables_error_js, ObservablesErrorDetail};
use crate::error::{engine_error, error_with_detail, range_error, to_plain_js, type_error};
use crate::frames::ExactEpochQueryValue;
use crate::rinex_nav::BroadcastEphemeris;
use crate::sp3::{
    instant_to_j2000_seconds, precise_clock_relativity_at_query, precise_variance_at_queries,
    selected_position_clock_at_queries, sp3_state_from_core, transmit_epoch_clock_at_queries, Sp3,
    Sp3State,
};

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

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum InstantRepresentationJs {
    JulianDate {
        #[serde(rename = "jdWhole")]
        jd_whole: f64,
        fraction: f64,
    },
    Nanos {
        nanos: String,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstantJs {
    scale: String,
    representation: InstantRepresentationJs,
}

fn instant_to_js(epoch: &Instant) -> InstantJs {
    let representation = match epoch.repr {
        InstantRepr::JulianDate(split) => InstantRepresentationJs::JulianDate {
            jd_whole: split.jd_whole,
            fraction: split.fraction,
        },
        InstantRepr::Nanos(nanos) => InstantRepresentationJs::Nanos {
            nanos: nanos.to_string(),
        },
    };
    InstantJs {
        scale: epoch.scale.abbrev().to_owned(),
        representation,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TimeModelErrorDetailJs {
    kind: &'static str,
    field: &'static str,
    reason: &'static str,
    message: String,
}

fn time_model_error_to_js(error: TimeModelError) -> JsValue {
    let (field, reason) = match error {
        TimeModelError::InvalidInput { field, reason } => (field, reason),
    };
    let message = error.to_string();
    let detail = TimeModelErrorDetailJs {
        kind: "INVALID_INPUT",
        field,
        reason,
        message: message.clone(),
    };
    error_with_detail("Error", &message, &detail)
}

fn instant_from_js(value: InstantJs) -> Result<Instant, JsValue> {
    let scale = match value.scale.as_str() {
        "UTC" => TimeScale::Utc,
        "TAI" => TimeScale::Tai,
        "TT" => TimeScale::Tt,
        "TCG" => TimeScale::Tcg,
        "TDB" => TimeScale::Tdb,
        "TCB" => TimeScale::Tcb,
        "GPST" => TimeScale::Gpst,
        "GST" => TimeScale::Gst,
        "BDT" => TimeScale::Bdt,
        "GLONASST" => TimeScale::Glonasst,
        "QZSST" => TimeScale::Qzsst,
        _ => return Err(type_error("instant scale is not recognized")),
    };
    let repr = match value.representation {
        InstantRepresentationJs::JulianDate { jd_whole, fraction } => {
            let split = JulianDateSplit::new(jd_whole, fraction).map_err(time_model_error_to_js)?;
            InstantRepr::JulianDate(split)
        }
        InstantRepresentationJs::Nanos { nanos } => {
            InstantRepr::Nanos(nanos.parse::<i128>().map_err(|_| {
                range_error("instant nanos must be a signed 128-bit decimal integer")
            })?)
        }
    };
    Ok(Instant { scale, repr })
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
    JulianDateSplit::new(whole, fraction).map_err(time_model_error_to_js)
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
    #[serde(default)]
    epoch: Option<f64>,
    #[serde(default)]
    instant: Option<InstantJs>,
    position_ecef_m: [f64; 3],
    #[serde(default)]
    clock_s: Option<f64>,
    #[serde(default)]
    clock_event: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
enum AccuracyValueJs {
    Known(f64),
    Unknown,
    TooLarge,
    InvalidBase,
    Overflow,
}

impl From<AccuracyValueJs> for CoreAccuracyValue {
    fn from(value: AccuracyValueJs) -> Self {
        match value {
            AccuracyValueJs::Known(value) => Self::Known(value),
            AccuracyValueJs::Unknown => Self::Unknown,
            AccuracyValueJs::TooLarge => Self::TooLarge,
            AccuracyValueJs::InvalidBase => Self::InvalidBase,
            AccuracyValueJs::Overflow => Self::Overflow,
        }
    }
}

impl From<CoreAccuracyValue> for AccuracyValueJs {
    fn from(value: CoreAccuracyValue) -> Self {
        match value {
            CoreAccuracyValue::Known(value) => Self::Known(value),
            CoreAccuracyValue::Unknown => Self::Unknown,
            CoreAccuracyValue::TooLarge => Self::TooLarge,
            CoreAccuracyValue::InvalidBase => Self::InvalidBase,
            CoreAccuracyValue::Overflow => Self::Overflow,
            _ => Self::Unknown,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccuracySampleJs {
    sat: String,
    #[serde(default)]
    epoch: Option<f64>,
    #[serde(default)]
    instant: Option<InstantJs>,
    position_variance_m2: [AccuracyValueJs; 3],
    clock_variance_m2: AccuracyValueJs,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AccuracySampleErrorJs {
    kind: &'static str,
    satellite: Option<String>,
    message: String,
}

pub(crate) fn decode_core_samples(samples: JsValue) -> Result<Vec<CoreSample>, JsValue> {
    let samples: Vec<SampleJs> = serde_wasm_bindgen::from_value(samples)
        .map_err(|e| type_error(&format!("invalid samples: {e}")))?;

    let mut core_samples = Vec::with_capacity(samples.len());
    for sample in samples {
        let sat = parse_sat(&sample.sat)?;
        let epoch = match sample.instant {
            Some(instant) => instant_from_js(instant)?,
            None => Instant::from_julian_date(
                TimeScale::Gpst,
                j2000_to_split(sample.epoch.ok_or_else(|| {
                    type_error("sample requires either epoch seconds or an exact instant")
                })?)?,
            ),
        };
        core_samples.push(CoreSample {
            sat,
            epoch,
            position_ecef_m: sample.position_ecef_m,
            clock_s: sample.clock_s,
            clock_event: sample.clock_event,
        });
    }
    Ok(core_samples)
}

fn decode_accuracy_samples(samples: JsValue) -> Result<Vec<CoreAccuracySample>, JsValue> {
    let samples: Vec<AccuracySampleJs> = serde_wasm_bindgen::from_value(samples)
        .map_err(|e| type_error(&format!("invalid accuracy samples: {e}")))?;
    samples
        .into_iter()
        .map(|sample| {
            let sat = parse_sat(&sample.sat)?;
            let epoch = match sample.instant {
                Some(instant) => instant_from_js(instant)?,
                None => Instant::from_julian_date(
                    TimeScale::Gpst,
                    j2000_to_split(sample.epoch.ok_or_else(|| {
                        type_error(
                            "accuracy sample requires either epoch seconds or an exact instant",
                        )
                    })?)?,
                ),
            };
            Ok(CoreAccuracySample::new(
                sat,
                epoch,
                sample.position_variance_m2.map(Into::into),
                sample.clock_variance_m2.into(),
            ))
        })
        .collect()
}

/// Map a sample-source validation failure to the JS exception a caller expects.
fn samples_error(err: PreciseSamplesError) -> JsValue {
    let (kind, satellite, uses_typed_error) = match &err {
        PreciseSamplesError::Empty => ("EMPTY", None, false),
        PreciseSamplesError::SingleSampleSatellite(satellite) => (
            "SINGLE_SAMPLE_SATELLITE",
            Some(satellite.to_string()),
            false,
        ),
        PreciseSamplesError::NonMonotonicEpochs(satellite) => {
            ("NON_MONOTONIC_EPOCHS", Some(satellite.to_string()), false)
        }
        PreciseSamplesError::MixedTimeScales => ("MIXED_TIME_SCALES", None, false),
        PreciseSamplesError::EpochNotRepresentable(satellite) => (
            "EPOCH_NOT_REPRESENTABLE",
            Some(satellite.to_string()),
            false,
        ),
        PreciseSamplesError::NonFiniteSample(satellite) => {
            ("NON_FINITE_SAMPLE", Some(satellite.to_string()), false)
        }
        PreciseSamplesError::AccuracySamplesMismatch => ("ACCURACY_SAMPLES_MISMATCH", None, true),
        PreciseSamplesError::InvalidAccuracyValue(satellite) => {
            ("INVALID_ACCURACY_VALUE", Some(satellite.to_string()), true)
        }
        _ => ("UNKNOWN", None, false),
    };
    let details = AccuracySampleErrorJs {
        kind,
        satellite,
        message: err.to_string(),
    };
    if uses_typed_error {
        return error_with_detail("PreciseSamplesError", &details.message, &details);
    }
    let detail_value = match to_plain_js(&details, "precise sample error detail") {
        Ok(value) => value,
        Err(error) => return error,
    };
    let range_error: JsValue = js_sys::RangeError::new(&details.message).into();
    match js_sys::Reflect::set(&range_error, &JsValue::from_str("detail"), &detail_value) {
        Ok(true) => range_error,
        Ok(false) => engine_error(format!(
            "{} (the typed detail could not be attached to the error)",
            details.message
        )),
        Err(_) => engine_error(format!(
            "{} (attaching the typed detail threw)",
            details.message
        )),
    }
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

    /// Evaluate one satellite state at an exact epoch query.
    #[wasm_bindgen(js_name = stateAtExactQuery)]
    pub fn state_at_exact_query(
        &self,
        satellite: &str,
        query: &ExactEpochQueryValue,
    ) -> Result<Sp3State, JsValue> {
        let satellite = parse_sat(satellite)?;
        CorePreciseEphemerisInterpolant::from_precise_ephemeris_samples(&self.inner)
            .position_at_epoch_query(satellite, &query.core())
            .map(sp3_state_from_core)
            .map_err(|error| core_error_js(&error))
    }

    #[wasm_bindgen(js_name = ephemerisVarianceAtExactQuery)]
    pub fn ephemeris_variance_at_exact_query(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        selection_epoch: &ExactEpochQueryValue,
    ) -> Result<f64, JsValue> {
        let satellite = parse_sat(satellite)?;
        let interpolant =
            CorePreciseEphemerisInterpolant::from_precise_ephemeris_samples(&self.inner);
        Ok(precise_variance_at_queries(
            &interpolant,
            satellite,
            state_epoch,
            selection_epoch,
        ))
    }

    #[wasm_bindgen(js_name = selectedPositionClockAtExactQueries, unchecked_return_type = "Ut1Validated<SelectedPositionClock> | null")]
    pub fn selected_position_clock_at_exact_queries(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        selection_epoch: &ExactEpochQueryValue,
    ) -> Result<JsValue, JsValue> {
        let satellite = parse_sat(satellite)?;
        let interpolant =
            CorePreciseEphemerisInterpolant::from_precise_ephemeris_samples(&self.inner);
        selected_position_clock_at_queries(&interpolant, satellite, state_epoch, selection_epoch)
    }

    #[wasm_bindgen(js_name = transmitEpochClockAtExactQueries, unchecked_return_type = "Ut1Validated<number> | null")]
    pub fn transmit_epoch_clock_at_exact_queries(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        selection_epoch: &ExactEpochQueryValue,
    ) -> Result<JsValue, JsValue> {
        let satellite = parse_sat(satellite)?;
        let interpolant =
            CorePreciseEphemerisInterpolant::from_precise_ephemeris_samples(&self.inner);
        transmit_epoch_clock_at_queries(&interpolant, satellite, state_epoch, selection_epoch)
    }

    #[wasm_bindgen(
        js_name = clockRelativityAtExactQuery,
        unchecked_return_type = "ClockRelativity"
    )]
    pub fn clock_relativity_at_exact_query(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        position_ecef_m: Vec<f64>,
    ) -> Result<JsValue, JsValue> {
        let satellite = parse_sat(satellite)?;
        let position_ecef_m: [f64; 3] = position_ecef_m
            .try_into()
            .map_err(|_| type_error("positionEcefM must contain exactly three coordinates"))?;
        let interpolant =
            CorePreciseEphemerisInterpolant::from_precise_ephemeris_samples(&self.inner);
        precise_clock_relativity_at_query(&interpolant, satellite, state_epoch, position_ecef_m)
    }

    /// Predict geometric ranges for many requests in one call. See the shared
    /// [`predictRanges`](crate::precise_samples::predict_ranges_over) contract;
    /// this is the sample-source entry point.
    #[wasm_bindgen(js_name = predictRanges)]
    pub fn predict_ranges(&self, requests: JsValue, options: JsValue) -> Result<JsValue, JsValue> {
        predict_ranges_over(&self.inner, requests, options)
    }

    /// Query ECEF position and optional clock for parallel satellite and epoch
    /// arrays against this sample-backed precise source.
    ///
    /// `satellites[i]` is evaluated at `epochsJ2000S[i]`, where epochs are
    /// seconds since J2000 on the source time scale. The returned plain object
    /// has aligned `positionsEcefM`, `clocksS`, `statuses`, and
    /// `elementResults` arrays. Failed elements use the core missing-position
    /// sentinel and carry the scalar engine error in `elementResults[i].error`
    /// plus structured detail in `elementResults[i].detail`.
    #[wasm_bindgen(
        js_name = observableStatesAtJ2000S,
        unchecked_return_type = "PreciseObservableStateBatch"
    )]
    pub fn observable_states_at_j2000_s(
        &self,
        satellites: JsValue,
        epochs_j2000_s: JsValue,
    ) -> Result<JsValue, JsValue> {
        observable_states_at_j2000_s_over(&self.inner, satellites, epochs_j2000_s)
    }

    /// Query ECEF position and optional clock for many satellites at one epoch
    /// against this sample-backed precise source.
    ///
    /// `epochJ2000S` is seconds since J2000 on the source time scale. The
    /// returned plain object follows the same contract as
    /// [`observableStatesAtJ2000S`](Self::observable_states_at_j2000_s).
    #[wasm_bindgen(
        js_name = observableStatesAtSharedJ2000S,
        unchecked_return_type = "PreciseObservableStateBatch"
    )]
    pub fn observable_states_at_shared_j2000_s(
        &self,
        satellites: JsValue,
        epoch_j2000_s: f64,
    ) -> Result<JsValue, JsValue> {
        observable_states_at_shared_j2000_s_over(&self.inner, satellites, epoch_j2000_s)
    }
    /// SP3 interpolation gap threshold factor carried by this source.
    #[wasm_bindgen(getter, js_name = gapThresholdFactor)]
    pub fn gap_threshold_factor(&self) -> f64 {
        self.inner.interpolation_options().gap_threshold_factor()
    }

    /// Return a copy of these samples with an explicit gap threshold factor.
    #[wasm_bindgen(js_name = withInterpolationOptions)]
    pub fn with_interpolation_options(
        &self,
        gap_threshold_factor: f64,
    ) -> Result<PreciseEphemerisSampleSource, JsValue> {
        let options = Sp3InterpolationOptions::new(gap_threshold_factor)
            .map_err(|error| core_error_js(&error))?;
        Ok(PreciseEphemerisSampleSource {
            inner: self.inner.clone().with_interpolation_options(options),
        })
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
    #[wasm_bindgen(unchecked_param_type = "Sp3PreciseEphemerisSample[]")] samples: JsValue,
    gap_threshold_factor: Option<f64>,
) -> Result<PreciseEphemerisSampleSource, JsValue> {
    let core_samples = decode_core_samples(samples)?;
    let mut inner = CoreSamples::from_samples(core_samples).map_err(samples_error)?;
    if let Some(factor) = gap_threshold_factor {
        let options =
            Sp3InterpolationOptions::new(factor).map_err(|error| core_error_js(&error))?;
        inner = inner.with_interpolation_options(options);
    }
    Ok(PreciseEphemerisSampleSource { inner })
}

/// Build a sample-backed source with identity-aligned SP3 accuracy sidecars.
#[wasm_bindgen(js_name = preciseEphemerisSamplesFromSamplesWithAccuracy)]
pub fn precise_ephemeris_samples_from_samples_with_accuracy(
    #[wasm_bindgen(unchecked_param_type = "Sp3PreciseEphemerisSample[]")] samples: JsValue,
    #[wasm_bindgen(unchecked_param_type = "Sp3PreciseEphemerisAccuracySample[]")] accuracy: JsValue,
    gap_threshold_factor: Option<f64>,
) -> Result<PreciseEphemerisSampleSource, JsValue> {
    let core_samples = decode_core_samples(samples)?;
    let core_accuracy = decode_accuracy_samples(accuracy)?;
    let mut inner = CoreSamples::from_samples_with_accuracy(core_samples, core_accuracy)
        .map_err(samples_error)?;
    if let Some(factor) = gap_threshold_factor {
        let options =
            Sp3InterpolationOptions::new(factor).map_err(|error| core_error_js(&error))?;
        inner = inner.with_interpolation_options(options);
    }
    Ok(PreciseEphemerisSampleSource { inner })
}

/// A reusable precise-ephemeris interpolant with cached per-satellite nodes.
///
/// Build this handle once from a parsed [`Sp3`] product, raw precise samples, or
/// a [`PreciseEphemerisSampleSource`], then reuse it for many state or range
/// queries. The handle delegates to
/// `sidereon_core::ephemeris::PreciseEphemerisInterpolant`; ECEF positions are
/// metres, clocks are seconds, and query epochs are seconds since J2000 in the
/// source time scale.
#[wasm_bindgen]
pub struct PreciseEphemerisInterpolant {
    inner: CorePreciseEphemerisInterpolant,
}

#[wasm_bindgen]
impl PreciseEphemerisInterpolant {
    /// Build a cached interpolant from a parsed SP3 precise product.
    ///
    /// Nodes are copied from the product's native SP3 records. Query epochs are
    /// seconds since J2000 in the SP3 product time scale.
    #[wasm_bindgen(js_name = fromSp3)]
    pub fn from_sp3(
        sp3: &Sp3,
        gap_threshold_factor: Option<f64>,
    ) -> Result<PreciseEphemerisInterpolant, JsValue> {
        let mut inner = CorePreciseEphemerisInterpolant::from_sp3(&sp3.inner);
        if let Some(factor) = gap_threshold_factor {
            let options =
                Sp3InterpolationOptions::new(factor).map_err(|error| core_error_js(&error))?;
            inner = inner.with_interpolation_options(options);
        }
        Ok(PreciseEphemerisInterpolant { inner })
    }

    /// Build a cached interpolant directly from precise samples.
    ///
    /// `samples` is the same array accepted by
    /// [`preciseEphemerisSamplesFromSamples`]: each item has `{ sat, epoch,
    /// positionEcefM, clockS?, clockEvent? }`, with epochs in seconds since
    /// J2000 and positions in ECEF metres. Throws a `TypeError` for malformed
    /// JS input and a `RangeError` for sample validation failures.
    #[wasm_bindgen(js_name = fromSamples)]
    pub fn from_samples(
        #[wasm_bindgen(unchecked_param_type = "Sp3PreciseEphemerisSample[]")] samples: JsValue,
        gap_threshold_factor: Option<f64>,
    ) -> Result<PreciseEphemerisInterpolant, JsValue> {
        let core_samples = decode_core_samples(samples)?;
        let mut inner = CorePreciseEphemerisInterpolant::from_samples(core_samples).map_err(
            |error| match error {
                sidereon_core::ephemeris::PreciseInterpolantError::Samples(error) => {
                    samples_error(error)
                }
            },
        )?;
        if let Some(factor) = gap_threshold_factor {
            let options =
                Sp3InterpolationOptions::new(factor).map_err(|error| core_error_js(&error))?;
            inner = inner.with_interpolation_options(options);
        }
        Ok(PreciseEphemerisInterpolant { inner })
    }

    /// Build a cached interpolant from samples and aligned accuracy sidecars.
    #[wasm_bindgen(js_name = fromSamplesWithAccuracy)]
    pub fn from_samples_with_accuracy(
        #[wasm_bindgen(unchecked_param_type = "Sp3PreciseEphemerisSample[]")] samples: JsValue,
        #[wasm_bindgen(unchecked_param_type = "Sp3PreciseEphemerisAccuracySample[]")]
        accuracy: JsValue,
        gap_threshold_factor: Option<f64>,
    ) -> Result<PreciseEphemerisInterpolant, JsValue> {
        let core_samples = decode_core_samples(samples)?;
        let core_accuracy = decode_accuracy_samples(accuracy)?;
        let mut inner = CorePreciseEphemerisInterpolant::from_samples_with_accuracy(
            core_samples,
            core_accuracy,
        )
        .map_err(|error| match error {
            sidereon_core::ephemeris::PreciseInterpolantError::Samples(error) => {
                samples_error(error)
            }
        })?;
        if let Some(factor) = gap_threshold_factor {
            let options =
                Sp3InterpolationOptions::new(factor).map_err(|error| core_error_js(&error))?;
            inner = inner.with_interpolation_options(options);
        }
        Ok(PreciseEphemerisInterpolant { inner })
    }

    /// Build a cached interpolant from an existing sample-backed precise source.
    #[wasm_bindgen(js_name = fromPreciseEphemerisSamples)]
    pub fn from_precise_ephemeris_samples(
        source: &PreciseEphemerisSampleSource,
        gap_threshold_factor: Option<f64>,
    ) -> Result<PreciseEphemerisInterpolant, JsValue> {
        let mut inner =
            CorePreciseEphemerisInterpolant::from_precise_ephemeris_samples(&source.inner);
        if let Some(factor) = gap_threshold_factor {
            let options =
                Sp3InterpolationOptions::new(factor).map_err(|error| core_error_js(&error))?;
            inner = inner.with_interpolation_options(options);
        }
        Ok(PreciseEphemerisInterpolant { inner })
    }

    /// SP3 interpolation gap threshold factor carried by this interpolant.
    #[wasm_bindgen(getter, js_name = gapThresholdFactor)]
    pub fn gap_threshold_factor(&self) -> f64 {
        self.inner.interpolation_options().gap_threshold_factor()
    }

    /// Return a copy of this interpolant with an explicit gap threshold factor.
    #[wasm_bindgen(js_name = withInterpolationOptions)]
    pub fn with_interpolation_options(
        &self,
        gap_threshold_factor: f64,
    ) -> Result<PreciseEphemerisInterpolant, JsValue> {
        let options = Sp3InterpolationOptions::new(gap_threshold_factor)
            .map_err(|error| core_error_js(&error))?;
        Ok(PreciseEphemerisInterpolant {
            inner: self.inner.clone().with_interpolation_options(options),
        })
    }

    /// Source time-scale abbreviation used by this handle's J2000-second axis,
    /// such as `"GPST"`.
    #[wasm_bindgen(getter, js_name = timeScale)]
    pub fn time_scale(&self) -> String {
        self.inner.time_scale().abbrev().to_string()
    }

    /// Satellite tokens this handle can interpolate, ascending.
    #[wasm_bindgen(getter)]
    pub fn satellites(&self) -> Vec<String> {
        self.inner.satellites().map(|sat| sat.to_string()).collect()
    }

    /// Evaluate one satellite state at an exact epoch query.
    #[wasm_bindgen(js_name = evaluateExact)]
    pub fn evaluate_exact(
        &self,
        satellite: &str,
        query: &ExactEpochQueryValue,
    ) -> Result<Sp3State, JsValue> {
        let satellite = parse_sat(satellite)?;
        self.inner
            .position_at_epoch_query(satellite, &query.core())
            .map(sp3_state_from_core)
            .map_err(|error| core_error_js(&error))
    }

    #[wasm_bindgen(js_name = ephemerisVarianceAtExactQuery)]
    pub fn ephemeris_variance_at_exact_query(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        selection_epoch: &ExactEpochQueryValue,
    ) -> Result<f64, JsValue> {
        let satellite = parse_sat(satellite)?;
        Ok(precise_variance_at_queries(
            &self.inner,
            satellite,
            state_epoch,
            selection_epoch,
        ))
    }

    #[wasm_bindgen(js_name = selectedPositionClockAtExactQueries, unchecked_return_type = "Ut1Validated<SelectedPositionClock> | null")]
    pub fn selected_position_clock_at_exact_queries(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        selection_epoch: &ExactEpochQueryValue,
    ) -> Result<JsValue, JsValue> {
        let satellite = parse_sat(satellite)?;
        selected_position_clock_at_queries(&self.inner, satellite, state_epoch, selection_epoch)
    }

    #[wasm_bindgen(js_name = transmitEpochClockAtExactQueries, unchecked_return_type = "Ut1Validated<number> | null")]
    pub fn transmit_epoch_clock_at_exact_queries(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        selection_epoch: &ExactEpochQueryValue,
    ) -> Result<JsValue, JsValue> {
        let satellite = parse_sat(satellite)?;
        transmit_epoch_clock_at_queries(&self.inner, satellite, state_epoch, selection_epoch)
    }

    #[wasm_bindgen(
        js_name = clockRelativityAtExactQuery,
        unchecked_return_type = "ClockRelativity"
    )]
    pub fn clock_relativity_at_exact_query(
        &self,
        satellite: &str,
        state_epoch: &ExactEpochQueryValue,
        position_ecef_m: Vec<f64>,
    ) -> Result<JsValue, JsValue> {
        let satellite = parse_sat(satellite)?;
        let position_ecef_m: [f64; 3] = position_ecef_m
            .try_into()
            .map_err(|_| type_error("positionEcefM must contain exactly three coordinates"))?;
        precise_clock_relativity_at_query(&self.inner, satellite, state_epoch, position_ecef_m)
    }

    /// Predict geometric ranges for many `(satellite, receiver, epoch)`
    /// requests using this cached interpolant.
    ///
    /// `requests` is an array of `{ sat, receiverEcefM, tRxJ2000S }` objects.
    /// Positions are ECEF metres and epochs are seconds since J2000 on this
    /// handle's time scale. The output matches [`Sp3.predictRanges`].
    #[wasm_bindgen(js_name = predictRanges)]
    pub fn predict_ranges(&self, requests: JsValue, options: JsValue) -> Result<JsValue, JsValue> {
        predict_ranges_over(&self.inner, requests, options)
    }

    /// Query ECEF position and optional clock for parallel satellite and epoch
    /// arrays using this cached interpolant.
    ///
    /// `satellites[i]` is evaluated at `epochsJ2000S[i]`, where epochs are
    /// seconds since J2000 on this handle's time scale. The returned plain
    /// object has aligned `positionsEcefM`, `clocksS`, `statuses`, and
    /// `elementResults` arrays.
    #[wasm_bindgen(
        js_name = observableStatesAtJ2000S,
        unchecked_return_type = "PreciseObservableStateBatch"
    )]
    pub fn observable_states_at_j2000_s(
        &self,
        satellites: JsValue,
        epochs_j2000_s: JsValue,
    ) -> Result<JsValue, JsValue> {
        observable_states_at_j2000_s_over(&self.inner, satellites, epochs_j2000_s)
    }

    /// Query ECEF position and optional clock for many satellites at one epoch
    /// using this cached interpolant.
    ///
    /// `epochJ2000S` is seconds since J2000 on this handle's time scale. The
    /// returned object follows the same contract as
    /// [`observableStatesAtJ2000S`](Self::observable_states_at_j2000_s).
    #[wasm_bindgen(
        js_name = observableStatesAtSharedJ2000S,
        unchecked_return_type = "PreciseObservableStateBatch"
    )]
    pub fn observable_states_at_shared_j2000_s(
        &self,
        satellites: JsValue,
        epoch_j2000_s: f64,
    ) -> Result<JsValue, JsValue> {
        observable_states_at_shared_j2000_s_over(&self.inner, satellites, epoch_j2000_s)
    }
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
#[wasm_bindgen(js_name = sp3PreciseEphemerisSamples, unchecked_return_type = "Sp3PreciseEphemerisSample[]")]
pub fn sp3_precise_ephemeris_samples(sp3: &Sp3) -> Result<JsValue, JsValue> {
    let samples: Vec<SampleJs> = sp3
        .inner
        .precise_ephemeris_samples()
        .into_iter()
        .map(|s| SampleJs {
            sat: s.sat.to_string(),
            epoch: Some(instant_to_j2000_seconds(&s.epoch)),
            instant: Some(instant_to_js(&s.epoch)),
            position_ecef_m: s.position_ecef_m,
            clock_s: s.clock_s,
            clock_event: s.clock_event,
        })
        .collect();
    to_js(&samples)
}

/// Extract identity-aligned position and clock variance sidecars from SP3.
#[wasm_bindgen(js_name = sp3PreciseEphemerisAccuracySamples, unchecked_return_type = "Sp3PreciseEphemerisAccuracySample[]")]
pub fn sp3_precise_ephemeris_accuracy_samples(sp3: &Sp3) -> Result<JsValue, JsValue> {
    let samples: Vec<AccuracySampleJs> = sp3
        .inner
        .precise_ephemeris_accuracy_samples()
        .into_iter()
        .map(|sample| AccuracySampleJs {
            sat: sample.sat.to_string(),
            epoch: Some(instant_to_j2000_seconds(&sample.epoch)),
            instant: Some(instant_to_js(&sample.epoch)),
            position_variance_m2: sample.position_variance_m2.map(Into::into),
            clock_variance_m2: sample.clock_variance_m2.into(),
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

fn observable_state_status_label(status: CoreObservableStateElementStatus) -> &'static str {
    match status {
        CoreObservableStateElementStatus::Valid => "valid",
        CoreObservableStateElementStatus::Gap => "gap",
        CoreObservableStateElementStatus::Error => "error",
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ObservableStateElementResultJs {
    ok: bool,
    error: Option<String>,
    detail: Option<ObservablesErrorDetail>,
}

fn observables_error_to_js(error: CoreObservablesError) -> JsValue {
    let detail = ObservablesErrorDetail::from(&error);
    observables_error_js(&detail)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ObservableStateBatchJs {
    count: usize,
    positions_ecef_m: Vec<[f64; 3]>,
    clocks_s: Vec<Option<f64>>,
    statuses: Vec<&'static str>,
    element_results: Vec<ObservableStateElementResultJs>,
}

fn parse_satellites_value(value: JsValue) -> Result<Vec<GnssSatelliteId>, JsValue> {
    let tokens: Vec<String> = serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid satellites: {e}")))?;
    tokens
        .iter()
        .map(|sat| parse_sat(sat))
        .collect::<Result<Vec<_>, _>>()
}

fn parse_epochs_value(value: JsValue) -> Result<Vec<f64>, JsValue> {
    serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid epochsJ2000S: {e}")))
}

fn observable_state_batch_to_js(batch: CoreObservableStateBatch) -> Result<JsValue, JsValue> {
    let statuses = (0..batch.len())
        .map(|index| {
            observable_state_status_label(
                batch
                    .element_status(index)
                    .unwrap_or(CoreObservableStateElementStatus::Error),
            )
        })
        .collect::<Vec<_>>();
    let element_results = batch
        .element_results
        .iter()
        .map(|result| match result {
            Ok(()) => ObservableStateElementResultJs {
                ok: true,
                error: None,
                detail: None,
            },
            Err(error) => {
                let detail = ObservablesErrorDetail::from(error);
                ObservableStateElementResultJs {
                    ok: false,
                    error: Some(detail.message().to_owned()),
                    detail: Some(detail),
                }
            }
        })
        .collect::<Vec<_>>();
    to_js(&ObservableStateBatchJs {
        count: batch.len(),
        positions_ecef_m: batch.positions_ecef_m,
        clocks_s: batch.clocks_s,
        statuses,
        element_results,
    })
}

#[wasm_bindgen(typescript_custom_section)]
const TS_PRECISE_OBSERVABLE_STATE_BATCH: &str = r#"
export interface PreciseObservableStateElementResult {
  ok: boolean;
  error: string | null;
  detail: ObservablesErrorDetail | null;
}

export interface TimeModelErrorDetail {
  kind: "INVALID_INPUT";
  field: string;
  reason: string;
  message: string;
}

export interface PreciseObservableStateBatch {
  count: number;
  positionsEcefM: [number, number, number][];
  clocksS: (number | null)[];
  statuses: ("valid" | "gap" | "error")[];
  elementResults: PreciseObservableStateElementResult[];
}
"#;

fn observable_states_at_j2000_s_over(
    source: &dyn ObservableEphemerisSource,
    satellites: JsValue,
    epochs_j2000_s: JsValue,
) -> Result<JsValue, JsValue> {
    let satellites = parse_satellites_value(satellites)?;
    let epochs_j2000_s = parse_epochs_value(epochs_j2000_s)?;
    if satellites.len() != epochs_j2000_s.len() {
        return Err(type_error(&format!(
            "satellites ({}) and epochsJ2000S ({}) must have the same length",
            satellites.len(),
            epochs_j2000_s.len()
        )));
    }
    let batch = core_observable_states_at_j2000_s(source, &satellites, &epochs_j2000_s)
        .map_err(observables_error_to_js)?;
    observable_state_batch_to_js(batch)
}

fn observable_states_at_shared_j2000_s_over(
    source: &dyn ObservableEphemerisSource,
    satellites: JsValue,
    epoch_j2000_s: f64,
) -> Result<JsValue, JsValue> {
    let satellites = parse_satellites_value(satellites)?;
    let batch = core_observable_states_at_shared_j2000_s(source, &satellites, epoch_j2000_s);
    observable_state_batch_to_js(batch)
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
        .map_err(observables_error_to_js)?;
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

/// Missing-position sentinel used in failed observable-state batch elements.
///
/// The returned JS value is `[NaN, NaN, NaN]`, matching
/// `sidereon_core::ephemeris::OBSERVABLE_STATE_MISSING_POSITION_ECEF_M`. Always
/// check `elementResults[i].ok` or `statuses[i]` before using
/// `positionsEcefM[i]`.
#[wasm_bindgen(js_name = observableStateMissingPositionEcefM)]
pub fn observable_state_missing_position_ecef_m() -> Result<JsValue, JsValue> {
    to_js(&OBSERVABLE_STATE_MISSING_POSITION_ECEF_M)
}

#[wasm_bindgen]
impl Sp3 {
    /// Query ECEF position and optional clock for parallel satellite and epoch
    /// arrays against this parsed SP3 product.
    ///
    /// `satellites[i]` is evaluated at `epochsJ2000S[i]`, where epochs are
    /// seconds since J2000 in the product time scale. The returned plain object
    /// has aligned `positionsEcefM`, `clocksS`, `statuses`, and
    /// `elementResults` arrays. Failed elements use the core missing-position
    /// sentinel and carry the scalar engine error in `elementResults[i].error`
    /// plus structured detail in `elementResults[i].detail`.
    #[wasm_bindgen(
        js_name = observableStatesAtJ2000S,
        unchecked_return_type = "PreciseObservableStateBatch"
    )]
    pub fn observable_states_at_j2000_s(
        &self,
        satellites: JsValue,
        epochs_j2000_s: JsValue,
    ) -> Result<JsValue, JsValue> {
        observable_states_at_j2000_s_over(&self.inner, satellites, epochs_j2000_s)
    }

    /// Query ECEF position and optional clock for many satellites at one epoch
    /// against this parsed SP3 product.
    ///
    /// `epochJ2000S` is seconds since J2000 in the product time scale. The
    /// returned object follows the same contract as
    /// [`observableStatesAtJ2000S`](Self::observable_states_at_j2000_s).
    #[wasm_bindgen(
        js_name = observableStatesAtSharedJ2000S,
        unchecked_return_type = "PreciseObservableStateBatch"
    )]
    pub fn observable_states_at_shared_j2000_s(
        &self,
        satellites: JsValue,
        epoch_j2000_s: f64,
    ) -> Result<JsValue, JsValue> {
        observable_states_at_shared_j2000_s_over(&self.inner, satellites, epoch_j2000_s)
    }
}

#[wasm_bindgen]
impl BroadcastEphemeris {
    /// Query ECEF position and clock for parallel satellite and epoch arrays
    /// against this parsed broadcast ephemeris store.
    ///
    /// `satellites[i]` is evaluated at `epochsJ2000S[i]`, where epochs are
    /// seconds since J2000 in GPST for GNSS broadcast records. The returned
    /// plain object has aligned `positionsEcefM`, `clocksS`, `statuses`, and
    /// `elementResults` arrays.
    #[wasm_bindgen(
        js_name = observableStatesAtJ2000S,
        unchecked_return_type = "PreciseObservableStateBatch"
    )]
    pub fn observable_states_at_j2000_s(
        &self,
        satellites: JsValue,
        epochs_j2000_s: JsValue,
    ) -> Result<JsValue, JsValue> {
        observable_states_at_j2000_s_over(&self.inner, satellites, epochs_j2000_s)
    }

    /// Query ECEF position and clock for many satellites at one epoch against
    /// this parsed broadcast ephemeris store.
    ///
    /// `epochJ2000S` is seconds since J2000 in GPST. The returned object follows
    /// the same contract as
    /// [`observableStatesAtJ2000S`](Self::observable_states_at_j2000_s).
    #[wasm_bindgen(
        js_name = observableStatesAtSharedJ2000S,
        unchecked_return_type = "PreciseObservableStateBatch"
    )]
    pub fn observable_states_at_shared_j2000_s(
        &self,
        satellites: JsValue,
        epoch_j2000_s: f64,
    ) -> Result<JsValue, JsValue> {
        observable_states_at_shared_j2000_s_over(&self.inner, satellites, epoch_j2000_s)
    }
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
        core_requests.push(RangePredictionRequest::new(
            sat,
            request.receiver_ecef_m,
            request.t_rx_j2000_s,
        ));
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

    core_predict_ranges(source, &core_requests, options, &mut out)
        .map_err(observables_error_to_js)?;

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
