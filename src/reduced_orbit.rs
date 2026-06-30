//! Compact reduced-orbit (mean-element) model: fit, evaluate, and drift.
//!
//! Thin wrapper over the `sidereon_core::orbit` public API. The plane fit,
//! secular element refinement, frame transforms, and drift evaluation all live
//! in the crate; this layer only decodes the JS sample objects and calendar
//! epochs, calls the core entry points, and re-encodes the results. Mirrors the
//! Elixir `Sidereon.ReducedOrbit` fit/eval/drift surface.

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use sidereon_core::astro::time::model::TimeScale as CoreTimeScale;
use sidereon_core::orbit::{
    self as orbit, CalendarEpoch, DriftReport, EcefSample, Elements, Frame, Model,
    PiecewiseOrbit as CorePiecewiseOrbit, PiecewiseOrbitError, PiecewiseOrbitSourceFitOptions,
    ReducedOrbit as CoreReducedOrbit, ReducedOrbitSource, ReducedOrbitSourceDriftOptions,
    ReducedOrbitSourceFitOptions, ReducedOrbitSourceSampling,
};
use sidereon_core::GnssSatelliteId;

use crate::error::{engine_error, type_error};
use crate::frames::TimeScale;
use crate::sgp4::Tle;
use crate::sp3::Sp3;

/// A civil calendar epoch `{ year, month, day, hour, minute, second }`,
/// interpreted in the model's time scale.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CalendarEpochInput {
    year: i32,
    month: i32,
    day: i32,
    hour: i32,
    minute: i32,
    second: f64,
}

impl CalendarEpochInput {
    fn to_core(&self) -> CalendarEpoch {
        CalendarEpoch::new(
            self.year,
            self.month,
            self.day,
            self.hour,
            self.minute,
            self.second,
        )
    }
}

/// One ECEF position sample `{ epoch, xM, yM, zM }`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EcefSampleInput {
    epoch: CalendarEpochInput,
    x_m: f64,
    y_m: f64,
    z_m: f64,
}

impl EcefSampleInput {
    fn to_core(&self) -> EcefSample {
        EcefSample::new(self.epoch.to_core(), self.x_m, self.y_m, self.z_m)
    }
}

fn decode_samples(samples: JsValue) -> Result<Vec<EcefSample>, JsValue> {
    let input: Vec<EcefSampleInput> = serde_wasm_bindgen::from_value(samples)
        .map_err(|e| type_error(&format!("invalid samples: {e}")))?;
    Ok(input.iter().map(EcefSampleInput::to_core).collect())
}

fn parse_satellite(token: &str) -> Result<GnssSatelliteId, JsValue> {
    token
        .parse::<GnssSatelliteId>()
        .map_err(|e| type_error(&format!("invalid satellite token {token:?}: {e}")))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceFitOptionsInput {
    t0: CalendarEpochInput,
    t1: CalendarEpochInput,
    cadence_s: f64,
    model: String,
}

impl SourceFitOptionsInput {
    fn sampling(&self) -> ReducedOrbitSourceSampling {
        ReducedOrbitSourceSampling::new(self.t0.to_core(), self.t1.to_core(), self.cadence_s)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PiecewiseSourceFitOptionsInput {
    t0: CalendarEpochInput,
    t1: CalendarEpochInput,
    cadence_s: f64,
    model: String,
    segment_seconds: f64,
}

impl PiecewiseSourceFitOptionsInput {
    fn sampling(&self) -> ReducedOrbitSourceSampling {
        ReducedOrbitSourceSampling::new(self.t0.to_core(), self.t1.to_core(), self.cadence_s)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceDriftOptionsInput {
    t0: CalendarEpochInput,
    t1: CalendarEpochInput,
    cadence_s: f64,
    threshold_m: f64,
}

impl SourceDriftOptionsInput {
    fn to_core(&self) -> ReducedOrbitSourceDriftOptions {
        ReducedOrbitSourceDriftOptions {
            sampling: ReducedOrbitSourceSampling::new(
                self.t0.to_core(),
                self.t1.to_core(),
                self.cadence_s,
            ),
            threshold_m: self.threshold_m,
        }
    }
}

fn decode_source_fit_options(options: JsValue) -> Result<SourceFitOptionsInput, JsValue> {
    serde_wasm_bindgen::from_value(options)
        .map_err(|e| type_error(&format!("invalid source fit options: {e}")))
}

fn decode_piecewise_source_fit_options(
    options: JsValue,
) -> Result<PiecewiseSourceFitOptionsInput, JsValue> {
    serde_wasm_bindgen::from_value(options)
        .map_err(|e| type_error(&format!("invalid piecewise source fit options: {e}")))
}

fn decode_source_drift_options(options: JsValue) -> Result<SourceDriftOptionsInput, JsValue> {
    serde_wasm_bindgen::from_value(options)
        .map_err(|e| type_error(&format!("invalid source drift options: {e}")))
}

fn drift_from_report(report: DriftReport, requested_samples: usize) -> ReducedOrbitDrift {
    let errors_m: Vec<f64> = report.per_epoch.iter().map(|d| d.error_m).collect();
    let threshold_index = report.threshold_index.map_or(-1, |i| i as i64);
    ReducedOrbitDrift {
        errors_m,
        max_m: report.max_m,
        rms_m: report.rms_m,
        threshold_index,
        requested_samples,
    }
}

fn model_from_str(model: &str) -> Result<Model, JsValue> {
    match model {
        "circular_secular" => Ok(Model::CircularSecular),
        "eccentric_secular" => Ok(Model::EccentricSecular),
        other => Err(type_error(&format!(
            "invalid model {other:?}: expected \"circular_secular\" or \"eccentric_secular\""
        ))),
    }
}

fn frame_from_str(frame: &str) -> Result<Frame, JsValue> {
    match frame {
        "ecef" => Ok(Frame::Ecef),
        "gcrs" => Ok(Frame::Gcrs),
        other => Err(type_error(&format!(
            "invalid frame {other:?}: expected \"ecef\" or \"gcrs\""
        ))),
    }
}

fn model_label(model: Model) -> &'static str {
    match model {
        Model::CircularSecular => "circular_secular",
        Model::EccentricSecular => "eccentric_secular",
    }
}

fn elements_vec(e: &Elements) -> Vec<f64> {
    let mut out = vec![
        e.a_m,
        e.e,
        e.i_rad,
        e.raan_rad,
        e.raan_rate_rad_s,
        e.raan_rate_j2_rad_s,
        e.arg_lat_rad,
        e.mean_motion_rad_s,
    ];
    if matches!(e.model, Model::EccentricSecular) {
        out.push(e.h);
        out.push(e.k);
        out.push(e.arg_perigee_rad);
    }
    out
}

/// Receiver model position and velocity from `positionVelocity`.
#[wasm_bindgen]
pub struct ReducedOrbitState {
    position_m: Vec<f64>,
    velocity_m_s: Vec<f64>,
}

#[wasm_bindgen]
impl ReducedOrbitState {
    /// Model position `[x, y, z]`, metres.
    #[wasm_bindgen(getter, js_name = positionM)]
    pub fn position_m(&self) -> Vec<f64> {
        self.position_m.clone()
    }

    /// Model velocity `[vx, vy, vz]`, metres per second.
    #[wasm_bindgen(getter, js_name = velocityMS)]
    pub fn velocity_m_s(&self) -> Vec<f64> {
        self.velocity_m_s.clone()
    }
}

/// Model-vs-truth drift report from `drift`.
#[wasm_bindgen]
pub struct ReducedOrbitDrift {
    errors_m: Vec<f64>,
    max_m: f64,
    rms_m: f64,
    threshold_index: i64,
    requested_samples: usize,
}

#[wasm_bindgen]
impl ReducedOrbitDrift {
    /// Per-epoch position error magnitudes, metres, in input order.
    #[wasm_bindgen(getter, js_name = errorsM)]
    pub fn errors_m(&self) -> Vec<f64> {
        self.errors_m.clone()
    }

    /// Maximum error over the horizon, metres.
    #[wasm_bindgen(getter, js_name = maxM)]
    pub fn max_m(&self) -> f64 {
        self.max_m
    }

    /// Root-mean-square error over the horizon, metres.
    #[wasm_bindgen(getter, js_name = rmsM)]
    pub fn rms_m(&self) -> f64 {
        self.rms_m
    }

    /// Index of the first epoch whose error exceeds the threshold, or `-1`.
    #[wasm_bindgen(getter, js_name = thresholdIndex)]
    pub fn threshold_index(&self) -> i64 {
        self.threshold_index
    }

    /// Number of samples requested from the source or supplied by the caller.
    #[wasm_bindgen(getter, js_name = requestedSamples)]
    pub fn requested_samples(&self) -> usize {
        self.requested_samples
    }

    /// Number of samples evaluated in the drift report.
    #[wasm_bindgen(getter, js_name = usedSamples)]
    pub fn used_samples(&self) -> usize {
        self.errors_m.len()
    }
}

/// A fitted compact reduced-orbit model. Carries the fitted elements and the
/// time scale they were fitted in; evaluate and drift queries reuse that scale.
#[wasm_bindgen]
pub struct ReducedOrbit {
    inner: CoreReducedOrbit,
    scale: CoreTimeScale,
}

#[wasm_bindgen]
impl ReducedOrbit {
    /// Model label, `"circular_secular"` or `"eccentric_secular"`.
    #[wasm_bindgen(getter)]
    pub fn model(&self) -> String {
        model_label(self.inner.elements.model).to_string()
    }

    /// Fitted mean elements as a `Float64Array`:
    /// `[a_m, e, i, raan, raanRate, raanRateJ2, argLat, n]` for the circular
    /// model, with `[h, k, argPerigee]` appended for the eccentric model.
    #[wasm_bindgen(getter)]
    pub fn elements(&self) -> Vec<f64> {
        elements_vec(&self.inner.elements)
    }

    /// Fit residual RMS over the samples, metres.
    #[wasm_bindgen(getter, js_name = rmsM)]
    pub fn rms_m(&self) -> f64 {
        self.inner.stats.rms_m
    }

    /// Fit residual maximum over the samples, metres.
    #[wasm_bindgen(getter, js_name = maxM)]
    pub fn max_m(&self) -> f64 {
        self.inner.stats.max_m
    }

    /// Number of samples used in the fit.
    #[wasm_bindgen(getter, js_name = nSamples)]
    pub fn n_samples(&self) -> usize {
        self.inner.stats.n_samples
    }

    /// Evaluate the model position at `query` in `frame` (`"ecef"` or
    /// `"gcrs"`). Delegates to `sidereon_core::orbit::position`.
    pub fn position(&self, query: JsValue, frame: &str) -> Result<Vec<f64>, JsValue> {
        let query: CalendarEpochInput = serde_wasm_bindgen::from_value(query)
            .map_err(|e| type_error(&format!("invalid query epoch: {e}")))?;
        let frame = frame_from_str(frame)?;
        let r = orbit::position(&self.inner.elements, query.to_core(), self.scale, frame)
            .map_err(engine_error)?;
        Ok(r.to_vec())
    }

    /// Evaluate the model position and velocity at `query` in `frame`.
    /// Delegates to `sidereon_core::orbit::position_velocity`.
    #[wasm_bindgen(js_name = positionVelocity)]
    pub fn position_velocity(
        &self,
        query: JsValue,
        frame: &str,
    ) -> Result<ReducedOrbitState, JsValue> {
        let query: CalendarEpochInput = serde_wasm_bindgen::from_value(query)
            .map_err(|e| type_error(&format!("invalid query epoch: {e}")))?;
        let frame = frame_from_str(frame)?;
        let (r, v) =
            orbit::position_velocity(&self.inner.elements, query.to_core(), self.scale, frame)
                .map_err(engine_error)?;
        Ok(ReducedOrbitState {
            position_m: r.to_vec(),
            velocity_m_s: v.to_vec(),
        })
    }

    /// Evaluate the model against `truth` ECEF samples, flagging the first epoch
    /// whose error exceeds `thresholdM`. Delegates to
    /// `sidereon_core::orbit::drift`.
    pub fn drift(&self, truth: JsValue, threshold_m: f64) -> Result<ReducedOrbitDrift, JsValue> {
        let truth = decode_samples(truth)?;
        let report = orbit::drift(&self.inner.elements, &truth, self.scale, threshold_m)
            .map_err(engine_error)?;
        Ok(drift_from_report(report, truth.len()))
    }

    /// Sample an SP3 product and evaluate this model against those positions.
    /// Delegates to `sidereon_core::orbit::drift_reduced_orbit_source`.
    #[wasm_bindgen(js_name = driftSp3)]
    pub fn drift_sp3(
        &self,
        sp3: &Sp3,
        satellite: &str,
        options: JsValue,
    ) -> Result<ReducedOrbitDrift, JsValue> {
        let options = decode_source_drift_options(options)?;
        let source = ReducedOrbitSource::Sp3 {
            product: &sp3.inner,
            satellite: parse_satellite(satellite)?,
        };
        let result =
            orbit::drift_reduced_orbit_source(&self.inner.elements, source, options.to_core())
                .map_err(engine_error)?;
        Ok(drift_from_report(result.report, result.requested_samples))
    }

    /// Sample a TLE/SGP4 source in UTC and evaluate this model against those
    /// positions. Delegates to
    /// `sidereon_core::orbit::drift_reduced_orbit_source`.
    #[wasm_bindgen(js_name = driftTle)]
    pub fn drift_tle(&self, tle: &Tle, options: JsValue) -> Result<ReducedOrbitDrift, JsValue> {
        let options = decode_source_drift_options(options)?;
        let source = ReducedOrbitSource::Sgp4 {
            satellite: tle.core_satellite(),
        };
        let result =
            orbit::drift_reduced_orbit_source(&self.inner.elements, source, options.to_core())
                .map_err(engine_error)?;
        Ok(drift_from_report(result.report, result.requested_samples))
    }
}

/// Fit a reduced-orbit model to ECEF position samples.
///
/// `samples` is an array of `{ epoch, xM, yM, zM }` (ECEF metres), `scale` the
/// time scale the epochs are expressed in, and `model` is `"circular_secular"`
/// or `"eccentric_secular"`. Delegates to `sidereon_core::orbit::fit_with_model`.
#[wasm_bindgen(js_name = fitReducedOrbit)]
pub fn fit_reduced_orbit(
    samples: JsValue,
    scale: TimeScale,
    model: &str,
) -> Result<ReducedOrbit, JsValue> {
    let samples = decode_samples(samples)?;
    let model = model_from_str(model)?;
    let scale: CoreTimeScale = scale.into();
    let inner = orbit::fit_with_model(&samples, scale, model).map_err(engine_error)?;
    Ok(ReducedOrbit { inner, scale })
}

/// Source-backed reduced-orbit fit result.
#[wasm_bindgen]
pub struct ReducedOrbitSourceFit {
    inner: CoreReducedOrbit,
    scale: CoreTimeScale,
    requested_samples: usize,
}

#[wasm_bindgen]
impl ReducedOrbitSourceFit {
    /// The fitted reduced-orbit model.
    #[wasm_bindgen(getter)]
    pub fn orbit(&self) -> ReducedOrbit {
        ReducedOrbit {
            inner: self.inner,
            scale: self.scale,
        }
    }

    /// Number of samples requested from the source.
    #[wasm_bindgen(getter, js_name = requestedSamples)]
    pub fn requested_samples(&self) -> usize {
        self.requested_samples
    }

    /// Number of source samples used by the fit.
    #[wasm_bindgen(getter, js_name = usedSamples)]
    pub fn used_samples(&self) -> usize {
        self.inner.stats.n_samples
    }
}

fn fit_source(
    source: ReducedOrbitSource<'_>,
    scale: CoreTimeScale,
    options: SourceFitOptionsInput,
) -> Result<ReducedOrbitSourceFit, JsValue> {
    let model = model_from_str(&options.model)?;
    let result = orbit::fit_reduced_orbit_source(
        source,
        ReducedOrbitSourceFitOptions {
            sampling: options.sampling(),
            model,
        },
    )
    .map_err(engine_error)?;
    Ok(ReducedOrbitSourceFit {
        inner: result.orbit,
        scale,
        requested_samples: result.requested_samples,
    })
}

/// Sample an SP3 product and fit a reduced orbit.
///
/// `options` is `{ t0, t1, cadenceS, model }`. Epochs are interpreted in the
/// SP3 product time scale. Delegates to
/// `sidereon_core::orbit::fit_reduced_orbit_source`.
#[wasm_bindgen(js_name = fitReducedOrbitSp3)]
pub fn fit_reduced_orbit_sp3(
    sp3: &Sp3,
    satellite: &str,
    options: JsValue,
) -> Result<ReducedOrbitSourceFit, JsValue> {
    let options = decode_source_fit_options(options)?;
    let source = ReducedOrbitSource::Sp3 {
        product: &sp3.inner,
        satellite: parse_satellite(satellite)?,
    };
    fit_source(source, sp3.inner.header.time_scale, options)
}

/// Sample a TLE/SGP4 source in UTC and fit a reduced orbit.
///
/// `options` is `{ t0, t1, cadenceS, model }`. Delegates to
/// `sidereon_core::orbit::fit_reduced_orbit_source`.
#[wasm_bindgen(js_name = fitReducedOrbitTle)]
pub fn fit_reduced_orbit_tle(
    tle: &Tle,
    options: JsValue,
) -> Result<ReducedOrbitSourceFit, JsValue> {
    let options = decode_source_fit_options(options)?;
    let source = ReducedOrbitSource::Sgp4 {
        satellite: tle.core_satellite(),
    };
    fit_source(source, CoreTimeScale::Utc, options)
}

/// Map the (non-`Display`) piecewise error onto a thrown JS `Error`, reusing the
/// single-segment error's own message for the wrapped case.
fn piecewise_err(error: PiecewiseOrbitError) -> JsValue {
    let message = match error {
        PiecewiseOrbitError::InvalidSegment => {
            "piecewise segment length is missing, non-positive, or rounds below one second"
                .to_string()
        }
        PiecewiseOrbitError::OutOfRange => {
            "query epoch is outside the piecewise model coverage".to_string()
        }
        PiecewiseOrbitError::TooFewSamples { got, required } => {
            format!("only {got} usable samples; need at least {required}")
        }
        PiecewiseOrbitError::Reduced(inner) => inner.to_string(),
    };
    engine_error(message)
}

/// A long span represented by contiguous independently-fitted reduced-orbit
/// segments. Carries the fitted segments and the time scale they were fitted in;
/// evaluate and drift queries reuse that scale.
#[wasm_bindgen]
pub struct PiecewiseOrbit {
    inner: CorePiecewiseOrbit,
    scale: CoreTimeScale,
}

#[wasm_bindgen]
impl PiecewiseOrbit {
    /// Model label, `"circular_secular"` or `"eccentric_secular"`.
    #[wasm_bindgen(getter)]
    pub fn model(&self) -> String {
        model_label(self.inner.model).to_string()
    }

    /// Rounded segment length used to tile the requested window, seconds.
    #[wasm_bindgen(getter, js_name = segmentSeconds)]
    pub fn segment_seconds(&self) -> i64 {
        self.inner.segment_s
    }

    /// Number of contiguous fitted segments.
    #[wasm_bindgen(getter, js_name = segmentCount)]
    pub fn segment_count(&self) -> usize {
        self.inner.segments.len()
    }

    /// Zero-based index of the segment covering `query`. Delegates to
    /// `sidereon_core::orbit::select_piecewise_segment`. Throws an `Error` when
    /// the epoch is outside coverage.
    #[wasm_bindgen(js_name = segmentIndexAt)]
    pub fn segment_index_at(&self, query: JsValue) -> Result<usize, JsValue> {
        let query: CalendarEpochInput = serde_wasm_bindgen::from_value(query)
            .map_err(|e| type_error(&format!("invalid query epoch: {e}")))?;
        let seg =
            orbit::select_piecewise_segment(&self.inner, query.to_core()).map_err(piecewise_err)?;
        Ok(self
            .inner
            .segments
            .iter()
            .position(|s| std::ptr::eq(s, seg))
            .expect("selected segment belongs to this piecewise model"))
    }

    /// Evaluate the piecewise model position at `query` in `frame` (`"ecef"` or
    /// `"gcrs"`). Delegates to `sidereon_core::orbit::piecewise_position`.
    pub fn position(&self, query: JsValue, frame: &str) -> Result<Vec<f64>, JsValue> {
        let query: CalendarEpochInput = serde_wasm_bindgen::from_value(query)
            .map_err(|e| type_error(&format!("invalid query epoch: {e}")))?;
        let frame = frame_from_str(frame)?;
        let r = orbit::piecewise_position(&self.inner, query.to_core(), self.scale, frame)
            .map_err(piecewise_err)?;
        Ok(r.to_vec())
    }

    /// Evaluate the piecewise model position and velocity at `query` in `frame`.
    /// Delegates to `sidereon_core::orbit::piecewise_position_velocity`.
    #[wasm_bindgen(js_name = positionVelocity)]
    pub fn position_velocity(
        &self,
        query: JsValue,
        frame: &str,
    ) -> Result<ReducedOrbitState, JsValue> {
        let query: CalendarEpochInput = serde_wasm_bindgen::from_value(query)
            .map_err(|e| type_error(&format!("invalid query epoch: {e}")))?;
        let frame = frame_from_str(frame)?;
        let (r, v) =
            orbit::piecewise_position_velocity(&self.inner, query.to_core(), self.scale, frame)
                .map_err(piecewise_err)?;
        Ok(ReducedOrbitState {
            position_m: r.to_vec(),
            velocity_m_s: v.to_vec(),
        })
    }

    /// Evaluate the piecewise model against `truth` ECEF samples, flagging the
    /// first epoch whose error exceeds `thresholdM`. Truth samples outside the
    /// model span are skipped. Delegates to
    /// `sidereon_core::orbit::piecewise_drift`.
    pub fn drift(&self, truth: JsValue, threshold_m: f64) -> Result<ReducedOrbitDrift, JsValue> {
        let truth = decode_samples(truth)?;
        let report = orbit::piecewise_drift(&self.inner, &truth, self.scale, threshold_m)
            .map_err(piecewise_err)?;
        Ok(drift_from_report(report, truth.len()))
    }

    /// Sample an SP3 product and evaluate this piecewise model against those
    /// positions. Delegates to
    /// `sidereon_core::orbit::drift_piecewise_reduced_orbit_source`.
    #[wasm_bindgen(js_name = driftSp3)]
    pub fn drift_sp3(
        &self,
        sp3: &Sp3,
        satellite: &str,
        options: JsValue,
    ) -> Result<ReducedOrbitDrift, JsValue> {
        let options = decode_source_drift_options(options)?;
        let source = ReducedOrbitSource::Sp3 {
            product: &sp3.inner,
            satellite: parse_satellite(satellite)?,
        };
        let result =
            orbit::drift_piecewise_reduced_orbit_source(&self.inner, source, options.to_core())
                .map_err(engine_error)?;
        Ok(drift_from_report(result.report, result.requested_samples))
    }

    /// Sample a TLE/SGP4 source in UTC and evaluate this piecewise model against
    /// those positions. Delegates to
    /// `sidereon_core::orbit::drift_piecewise_reduced_orbit_source`.
    #[wasm_bindgen(js_name = driftTle)]
    pub fn drift_tle(&self, tle: &Tle, options: JsValue) -> Result<ReducedOrbitDrift, JsValue> {
        let options = decode_source_drift_options(options)?;
        let source = ReducedOrbitSource::Sgp4 {
            satellite: tle.core_satellite(),
        };
        let result =
            orbit::drift_piecewise_reduced_orbit_source(&self.inner, source, options.to_core())
                .map_err(engine_error)?;
        Ok(drift_from_report(result.report, result.requested_samples))
    }
}

/// Fit a piecewise reduced-orbit model: tile `[t0, t1]` into segments of
/// `segmentSeconds` and fit each independently.
///
/// `samples` is an array of `{ epoch, xM, yM, zM }` (ECEF metres), `scale` the
/// time scale the epochs are expressed in, and `model` is `"circular_secular"`
/// or `"eccentric_secular"`. `t0` / `t1` are `{ year, month, day, hour, minute,
/// second }` calendar epochs in that scale. Delegates to
/// `sidereon_core::orbit::fit_piecewise`.
#[wasm_bindgen(js_name = fitPiecewiseReducedOrbit)]
pub fn fit_piecewise_reduced_orbit(
    samples: JsValue,
    scale: TimeScale,
    model: &str,
    t0: JsValue,
    t1: JsValue,
    segment_seconds: f64,
) -> Result<PiecewiseOrbit, JsValue> {
    // `segmentSeconds` is exposed as a JS number; the core takes an `i64`. Guard
    // integrality and range here, then cast, so the value crosses as a plain
    // number rather than a `BigInt` and a degenerate length surfaces as a
    // `RangeError` before the core call. The core still rejects a sub-one-second
    // rounded segment with its own `InvalidSegment`.
    if !segment_seconds.is_finite()
        || segment_seconds.fract() != 0.0
        || segment_seconds.abs() > i64::MAX as f64
    {
        return Err(crate::error::range_error(
            "segmentSeconds must be an integer number of seconds",
        ));
    }
    let segment_seconds = segment_seconds as i64;
    let samples = decode_samples(samples)?;
    let model = model_from_str(model)?;
    let scale: CoreTimeScale = scale.into();
    let t0: CalendarEpochInput = serde_wasm_bindgen::from_value(t0)
        .map_err(|e| type_error(&format!("invalid t0 epoch: {e}")))?;
    let t1: CalendarEpochInput = serde_wasm_bindgen::from_value(t1)
        .map_err(|e| type_error(&format!("invalid t1 epoch: {e}")))?;
    let inner = orbit::fit_piecewise(
        &samples,
        scale,
        model,
        t0.to_core(),
        t1.to_core(),
        segment_seconds,
    )
    .map_err(piecewise_err)?;
    Ok(PiecewiseOrbit { inner, scale })
}

/// Source-backed piecewise reduced-orbit fit result.
#[wasm_bindgen]
pub struct PiecewiseOrbitSourceFit {
    inner: CorePiecewiseOrbit,
    scale: CoreTimeScale,
    requested_samples: usize,
}

#[wasm_bindgen]
impl PiecewiseOrbitSourceFit {
    /// The fitted piecewise reduced-orbit model.
    #[wasm_bindgen(getter)]
    pub fn orbit(&self) -> PiecewiseOrbit {
        PiecewiseOrbit {
            inner: self.inner.clone(),
            scale: self.scale,
        }
    }

    /// Number of samples requested from the source.
    #[wasm_bindgen(getter, js_name = requestedSamples)]
    pub fn requested_samples(&self) -> usize {
        self.requested_samples
    }

    /// Number of source samples used by all fitted segments.
    #[wasm_bindgen(getter, js_name = usedSamples)]
    pub fn used_samples(&self) -> usize {
        self.inner
            .segments
            .iter()
            .map(|segment| segment.orbit.stats.n_samples)
            .sum()
    }
}

fn fit_piecewise_source(
    source: ReducedOrbitSource<'_>,
    scale: CoreTimeScale,
    options: PiecewiseSourceFitOptionsInput,
) -> Result<PiecewiseOrbitSourceFit, JsValue> {
    let model = model_from_str(&options.model)?;
    let result = orbit::fit_piecewise_reduced_orbit_source(
        source,
        PiecewiseOrbitSourceFitOptions {
            sampling: options.sampling(),
            model,
            segment_s: options.segment_seconds,
        },
    )
    .map_err(engine_error)?;
    Ok(PiecewiseOrbitSourceFit {
        inner: result.orbit,
        scale,
        requested_samples: result.requested_samples,
    })
}

/// Sample an SP3 product and fit a piecewise reduced orbit.
///
/// `options` is `{ t0, t1, cadenceS, model, segmentSeconds }`. Epochs are
/// interpreted in the SP3 product time scale. Delegates to
/// `sidereon_core::orbit::fit_piecewise_reduced_orbit_source`.
#[wasm_bindgen(js_name = fitPiecewiseReducedOrbitSp3)]
pub fn fit_piecewise_reduced_orbit_sp3(
    sp3: &Sp3,
    satellite: &str,
    options: JsValue,
) -> Result<PiecewiseOrbitSourceFit, JsValue> {
    let options = decode_piecewise_source_fit_options(options)?;
    let source = ReducedOrbitSource::Sp3 {
        product: &sp3.inner,
        satellite: parse_satellite(satellite)?,
    };
    fit_piecewise_source(source, sp3.inner.header.time_scale, options)
}

/// Sample a TLE/SGP4 source in UTC and fit a piecewise reduced orbit.
///
/// `options` is `{ t0, t1, cadenceS, model, segmentSeconds }`. Delegates to
/// `sidereon_core::orbit::fit_piecewise_reduced_orbit_source`.
#[wasm_bindgen(js_name = fitPiecewiseReducedOrbitTle)]
pub fn fit_piecewise_reduced_orbit_tle(
    tle: &Tle,
    options: JsValue,
) -> Result<PiecewiseOrbitSourceFit, JsValue> {
    let options = decode_piecewise_source_fit_options(options)?;
    let source = ReducedOrbitSource::Sgp4 {
        satellite: tle.core_satellite(),
    };
    fit_piecewise_source(source, CoreTimeScale::Utc, options)
}
