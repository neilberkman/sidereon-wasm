//! SGP4 / TLE propagation, topocentric look angles, and dense pass finding. The
//! kernels are `propagate_teme_arc` / `look_angle_arc` / `find_passes_for_satellite` over a
//! satellite (and its parsed elements) built once from the two TLE lines,
//! unchanged.

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use sidereon::passes::{
    find_passes_for_satellite, ground_track, look_angle_arc, look_angle_batch_serial,
    propagate_teme_arc, propagate_teme_batch_serial, visible_from_satellites,
    GroundStation as CoreGroundStation, PassFinderOptions, SatellitePass as CoreSatellitePass,
    UtcInstant, VisibleSatellite as CoreVisibleSatellite,
};
use sidereon::sgp4::{
    fit_tle as core_fit_tle, parse_tle_file_with_policy, DecayLatch as CoreDecayLatch, FitConfig,
    FitEpoch, FitSample, JulianDate as CoreJulianDate, Loss, OpsMode, RejectedTleRecord, Satellite,
    TleFit as CoreTleFit, TleMetadata, TleRecordIssue, XScale,
};
use sidereon::tle::{
    encode as encode_tle, parse_with_policy as parse_tle_with_policy,
    ChecksumWarning as CoreChecksumWarning, ChecksumWarningKind, TleElements, TlePolicy,
};
use sidereon_core::astro::passes::{
    find_passes_for_satellite_with_validity, ground_track_with_validity,
    look_angle_arc_with_validity, visible_from_satellites_with_validity, LookAngle,
};
use sidereon_core::frame::Wgs84Geodetic;
use sidereon_core::geometry::visible_at_elevation_mask;

use crate::error::{range_error, type_error, ut1_validity, validated_object};
use crate::marshal::{instants, vec3_finite};
use crate::ndm_error::omm_error;
use crate::omm::Omm;
use crate::sgp4_error::{
    decay_latched_error, fit_error, indexed_sgp4_error, look_angle_error, pass_error,
    record_issue_cause, sgp4_error, tle_error,
};

const UNIX_EPOCH_JD_WHOLE: i64 = 2_440_587;
const UNIX_EPOCH_JD_FRACTION: f64 = 0.5;
const MICROSECONDS_PER_DAY: i64 = 86_400_000_000;

fn unix_us_to_julian_date(epoch_unix_us: i64) -> CoreJulianDate {
    let days = epoch_unix_us.div_euclid(MICROSECONDS_PER_DAY);
    let rem = epoch_unix_us.rem_euclid(MICROSECONDS_PER_DAY);
    let mut whole = UNIX_EPOCH_JD_WHOLE + days;
    let mut fraction = UNIX_EPOCH_JD_FRACTION + rem as f64 / MICROSECONDS_PER_DAY as f64;
    if fraction >= 1.0 {
        whole += 1;
        fraction -= 1.0;
    }
    CoreJulianDate(whole as f64, fraction)
}

/// A geodetic ground station: WGS84 latitude/longitude in degrees, altitude in
/// metres. Pass to [`Tle.lookAngles`] / [`Tle.findPasses`].
#[wasm_bindgen]
pub struct GroundStation {
    inner: CoreGroundStation,
}

#[wasm_bindgen]
impl GroundStation {
    /// Create a ground station. `altitudeM` defaults to 0.
    #[wasm_bindgen(constructor)]
    pub fn new(latitude_deg: f64, longitude_deg: f64, altitude_m: Option<f64>) -> GroundStation {
        GroundStation {
            inner: CoreGroundStation {
                latitude_deg,
                longitude_deg,
                altitude_m: altitude_m.unwrap_or(0.0),
            },
        }
    }

    #[wasm_bindgen(getter, js_name = latitudeDeg)]
    pub fn latitude_deg(&self) -> f64 {
        self.inner.latitude_deg
    }

    #[wasm_bindgen(getter, js_name = longitudeDeg)]
    pub fn longitude_deg(&self) -> f64 {
        self.inner.longitude_deg
    }

    #[wasm_bindgen(getter, js_name = altitudeM)]
    pub fn altitude_m(&self) -> f64 {
        self.inner.altitude_m
    }
}

impl GroundStation {
    /// The wrapped core ground station, for sibling bindings (e.g. coverage)
    /// that hand a station straight to a core entry point.
    pub(crate) fn core(&self) -> CoreGroundStation {
        self.inner
    }
}

fn look_angles_js(looks: &[LookAngle]) -> LookAngles {
    LookAngles {
        azimuth_deg: looks.iter().map(|l| l.azimuth_deg).collect(),
        elevation_deg: looks.iter().map(|l| l.elevation_deg).collect(),
        range_km: looks.iter().map(|l| l.range_km).collect(),
    }
}

fn ground_track_js(points: &[Wgs84Geodetic]) -> GroundTrack {
    GroundTrack {
        latitude_deg: points.iter().map(|g| g.lat_rad.to_degrees()).collect(),
        longitude_deg: points.iter().map(|g| g.lon_rad.to_degrees()).collect(),
        altitude_km: points.iter().map(|g| g.height_m / 1000.0).collect(),
    }
}

fn js_array<T: Into<JsValue>>(values: impl IntoIterator<Item = T>) -> JsValue {
    let array = js_sys::Array::new();
    for value in values {
        array.push(&value.into());
    }
    array.into()
}

/// Map an `opsMode` string to the core enum. Defaults to `improved` (the engine
/// default, matching Python's `sgp4` package); `afspc` selects AFSPC parity.
fn ops_mode(label: Option<String>) -> Result<OpsMode, JsValue> {
    match label.as_deref() {
        None | Some("improved") => Ok(OpsMode::Improved),
        Some("afspc") => Ok(OpsMode::Afspc),
        Some(other) => Err(type_error(&format!(
            "invalid opsMode {other:?}: expected \"improved\" or \"afspc\""
        ))),
    }
}

/// Map a TLE checksum policy string to the core enum. `"strict"` (the
/// default) refuses a column-69 digit that disagrees with the checksum and a
/// column 69 that is not a digit; `"lenient"` reads both and reports each in
/// `checksumWarnings`, as Vallado's `twoline2rv` reads.
fn tle_policy(label: Option<String>) -> Result<TlePolicy, JsValue> {
    match label.as_deref() {
        None | Some("strict") => Ok(TlePolicy::Strict),
        Some("lenient") => Ok(TlePolicy::Lenient),
        Some(other) => Err(type_error(&format!(
            "invalid TLE policy {other:?}: expected \"strict\" or \"lenient\""
        ))),
    }
}

/// A TLE line whose column 69 did not confirm its checksum. A line that ends
/// before column 69 is read and reported under both policies; a mismatching
/// digit or a non-digit is reported only under the `"lenient"` policy, which
/// the `"strict"` policy refuses.
#[wasm_bindgen]
#[derive(Clone)]
pub struct ChecksumWarning {
    line_label: &'static str,
    kind: ChecksumWarningKind,
    computed: u8,
}

#[wasm_bindgen]
impl ChecksumWarning {
    /// Which line the discrepancy is on: `"line 1"` or `"line 2"`.
    #[wasm_bindgen(getter, js_name = lineLabel)]
    pub fn line_label(&self) -> String {
        self.line_label.to_string()
    }

    /// What column 69 held: `"mismatch"` (a digit that differs from the
    /// computed checksum), `"notDigit"` (a character other than a digit) or
    /// `"missing"` (the line ends before column 69).
    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> String {
        match self.kind {
            ChecksumWarningKind::Mismatch { .. } => "mismatch",
            ChecksumWarningKind::NotDigit { .. } => "notDigit",
            ChecksumWarningKind::Missing => "missing",
        }
        .to_string()
    }

    /// The checksum digit found in column 69 for a `"mismatch"`, else
    /// `undefined`.
    #[wasm_bindgen(getter)]
    pub fn expected(&self) -> Option<u8> {
        match self.kind {
            ChecksumWarningKind::Mismatch { expected } => Some(expected),
            _ => None,
        }
    }

    /// The character found in column 69 for a `"notDigit"`, else `undefined`.
    #[wasm_bindgen(getter)]
    pub fn found(&self) -> Option<String> {
        match self.kind {
            ChecksumWarningKind::NotDigit { found } => Some(found.to_string()),
            _ => None,
        }
    }

    /// The finding as the engine states it.
    #[wasm_bindgen(getter)]
    pub fn message(&self) -> String {
        CoreChecksumWarning {
            line_label: self.line_label,
            kind: self.kind,
            computed: self.computed,
        }
        .to_string()
    }

    /// The checksum digit recomputed from columns 1-68.
    #[wasm_bindgen(getter)]
    pub fn computed(&self) -> u8 {
        self.computed
    }
}

impl From<&CoreChecksumWarning> for ChecksumWarning {
    fn from(w: &CoreChecksumWarning) -> Self {
        Self {
            line_label: w.line_label,
            kind: w.kind,
            computed: w.computed,
        }
    }
}

/// A satellite pass over a ground station: acquisition of signal, loss of
/// signal, culmination time (all unix microseconds UTC), and the elevation at
/// culmination.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct SatellitePass {
    aos_unix_us: i64,
    los_unix_us: i64,
    culmination_unix_us: i64,
    max_elevation_deg: f64,
}

impl From<&CoreSatellitePass> for SatellitePass {
    fn from(pass: &CoreSatellitePass) -> Self {
        Self {
            aos_unix_us: pass.aos.unix_microseconds(),
            los_unix_us: pass.los.unix_microseconds(),
            culmination_unix_us: pass.culmination.unix_microseconds(),
            max_elevation_deg: pass.max_elevation_deg,
        }
    }
}

#[wasm_bindgen]
impl SatellitePass {
    /// Acquisition of signal (rise above the mask), unix microseconds UTC.
    #[wasm_bindgen(getter, js_name = aosUnixUs)]
    pub fn aos_unix_us(&self) -> i64 {
        self.aos_unix_us
    }

    /// Loss of signal (set below the mask), unix microseconds UTC.
    #[wasm_bindgen(getter, js_name = losUnixUs)]
    pub fn los_unix_us(&self) -> i64 {
        self.los_unix_us
    }

    /// Culmination (maximum elevation) time, unix microseconds UTC.
    #[wasm_bindgen(getter, js_name = culminationUnixUs)]
    pub fn culmination_unix_us(&self) -> i64 {
        self.culmination_unix_us
    }

    /// Elevation at culmination, degrees.
    #[wasm_bindgen(getter, js_name = maxElevationDeg)]
    pub fn max_elevation_deg(&self) -> f64 {
        self.max_elevation_deg
    }

    /// Pass duration (LOS minus AOS), seconds.
    #[wasm_bindgen(getter, js_name = durationS)]
    pub fn duration_s(&self) -> f64 {
        (self.los_unix_us - self.aos_unix_us) as f64 / 1.0e6
    }
}

/// A parsed two-line element set with an initialized SGP4 satellite.
#[wasm_bindgen]
#[derive(Clone)]
pub struct Tle {
    elements: TleElements,
    satellite: Satellite,
    checksum_warnings: Vec<CoreChecksumWarning>,
}

/// A reusable SGP4 satellite initialized from an OMM.
#[wasm_bindgen]
#[derive(Clone)]
pub struct Sgp4Satellite {
    satellite: Satellite,
}

#[wasm_bindgen]
impl Sgp4Satellite {
    /// Initialize SGP4 from an OMM through the engine's canonical OMM bridge.
    /// Throws an `OmmError` with structured detail for incompatible metadata,
    /// a missing `MEAN_MOTION` or `BSTAR`, or another invalid OMM field.
    #[wasm_bindgen(js_name = fromOmm)]
    pub fn from_omm(omm: &Omm) -> Result<Sgp4Satellite, JsValue> {
        // Validate through Omm::to_element_set first so its lossless OmmError
        // reaches JavaScript. Satellite::from_omm maps the same bridge errors
        // to SGP4 input errors, then initializes the reusable core satellite.
        omm.core().to_element_set().map_err(omm_error)?;
        let satellite = Satellite::from_omm(omm.core()).map_err(sgp4_error)?;
        Ok(Sgp4Satellite { satellite })
    }

    /// Propagate over a `BigInt64Array` of unix-microsecond epochs. Returns TEME
    /// position (km) and velocity (km/s). Throws an `Error` on SGP4 failure.
    pub fn propagate(&self, epochs_unix_us: &[i64]) -> Result<TlePropagation, JsValue> {
        propagate_satellite(&self.satellite, epochs_unix_us)
    }
}

fn propagate_satellite(
    satellite: &Satellite,
    epochs_unix_us: &[i64],
) -> Result<TlePropagation, JsValue> {
    let predictions =
        propagate_teme_arc(satellite, &instants(epochs_unix_us)).map_err(sgp4_error)?;
    let mut positions = Vec::with_capacity(predictions.len() * 3);
    let mut velocities = Vec::with_capacity(predictions.len() * 3);
    for prediction in &predictions {
        positions.extend_from_slice(&prediction.position);
        velocities.extend_from_slice(&prediction.velocity);
    }
    Ok(TlePropagation {
        positions,
        velocities,
    })
}

/// Stateful opt-in latch for SGP4 decay-like failures.
///
/// Pass one latch per satellite to [`Tle.propagateWithDecayLatch`]. The first
/// decay-like SGP4 failure records the requested epoch in minutes since the TLE
/// epoch; later requests at the same or a later epoch throw immediately through
/// the core latch instead of returning a raw post-decay state.
#[wasm_bindgen]
pub struct DecayLatch {
    inner: CoreDecayLatch,
}

impl Default for DecayLatch {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl DecayLatch {
    /// Construct an empty decay latch.
    #[wasm_bindgen(constructor)]
    pub fn new() -> DecayLatch {
        DecayLatch {
            inner: CoreDecayLatch::new(),
        }
    }

    /// First observed decay-like epoch, in minutes since the TLE epoch.
    #[wasm_bindgen(getter, js_name = firstFailingEpochMinutes)]
    pub fn first_failing_epoch_minutes(&self) -> Option<f64> {
        self.inner.first_failing_epoch().map(|epoch| epoch.0)
    }

    /// Clear the recorded decay state.
    pub fn clear(&mut self) {
        self.inner.clear();
    }
}

impl Tle {
    /// Wrap an already-initialized core `Satellite` with the checksum findings
    /// its reader accepted, recovering the parsed elements from its own TLE
    /// lines under the policy it was read with. Used by [`parse_tle_file`] so
    /// each record's SGP4 record is reused rather than re-initialized.
    fn from_core_satellite(
        satellite: Satellite,
        policy: TlePolicy,
        checksum_warnings: Vec<CoreChecksumWarning>,
    ) -> Result<Tle, JsValue> {
        let parsed = parse_tle_with_policy(satellite.line1(), satellite.line2(), policy)
            .map_err(tle_error)?;
        Ok(Tle {
            elements: parsed.elements,
            satellite,
            checksum_warnings,
        })
    }

    /// The wrapped core SGP4 satellite, for sibling bindings (e.g. coverage)
    /// that hand a satellite straight to a core entry point.
    pub(crate) fn core_satellite(&self) -> &Satellite {
        &self.satellite
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FitSampleInput {
    epoch: Vec<f64>,
    position_teme_km: Vec<f64>,
    #[serde(default)]
    velocity_teme_km_s: Option<Vec<f64>>,
}

fn split_jd(values: &[f64], name: &str) -> Result<CoreJulianDate, JsValue> {
    if values.len() != 2 {
        return Err(type_error(&format!(
            "{name} must have length 2 as [jdWhole, jdFraction], got {}",
            values.len()
        )));
    }
    if !values[0].is_finite() || !values[1].is_finite() {
        return Err(range_error(&format!("{name} values must be finite")));
    }
    Ok(CoreJulianDate(values[0], values[1]))
}

impl FitSampleInput {
    fn to_core(&self, index: usize) -> Result<FitSample, JsValue> {
        Ok(FitSample {
            epoch: split_jd(&self.epoch, &format!("samples[{index}].epoch"))?,
            position_teme_km: vec3_finite(
                &format!("samples[{index}].positionTemeKm"),
                &self.position_teme_km,
            )?,
            velocity_teme_km_s: self
                .velocity_teme_km_s
                .as_ref()
                .map(|velocity| vec3_finite(&format!("samples[{index}].velocityTemeKmS"), velocity))
                .transpose()?,
        })
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct FitMetadataInput {
    catalog_number: Option<u32>,
    classification: Option<String>,
    international_designator: Option<String>,
    element_set_number: Option<i32>,
    rev_at_epoch: Option<i64>,
    object_name: Option<String>,
}

impl FitMetadataInput {
    fn to_core(&self) -> TleMetadata {
        let defaults = TleMetadata::default();
        TleMetadata {
            catalog_number: self.catalog_number.unwrap_or(defaults.catalog_number),
            classification: self
                .classification
                .clone()
                .unwrap_or(defaults.classification),
            international_designator: self
                .international_designator
                .clone()
                .unwrap_or(defaults.international_designator),
            element_set_number: self
                .element_set_number
                .unwrap_or(defaults.element_set_number),
            rev_at_epoch: self.rev_at_epoch.unwrap_or(defaults.rev_at_epoch),
            object_name: self.object_name.clone().unwrap_or(defaults.object_name),
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum XScaleInput {
    Label(String),
    Values(Vec<f64>),
}

fn x_scale(input: Option<XScaleInput>) -> Result<Option<XScale>, JsValue> {
    match input {
        None => Ok(None),
        Some(XScaleInput::Label(label)) => match label.as_str() {
            "unit" => Ok(Some(XScale::Unit)),
            "jac" => Ok(Some(XScale::Jac)),
            other => Err(type_error(&format!(
                "invalid xScale {other:?}: expected \"unit\", \"jac\", or a numeric array"
            ))),
        },
        Some(XScaleInput::Values(values)) => Ok(Some(XScale::Values(values))),
    }
}

fn loss(label: Option<&str>) -> Result<Loss, JsValue> {
    match label.unwrap_or("linear") {
        "linear" => Ok(Loss::Linear),
        "softL1" | "soft_l1" => Ok(Loss::SoftL1),
        "huber" => Ok(Loss::Huber),
        "cauchy" => Ok(Loss::Cauchy),
        "arctan" => Ok(Loss::Arctan),
        other => Err(type_error(&format!(
            "invalid loss {other:?}: expected \"linear\", \"softL1\", \"huber\", \"cauchy\", or \"arctan\""
        ))),
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct FitConfigInput {
    epoch: Option<String>,
    epoch_sample_index: Option<usize>,
    epoch_jd: Option<Vec<f64>>,
    fit_bstar: Option<bool>,
    bstar_seed: Option<f64>,
    use_velocity: Option<bool>,
    velocity_weight_s: Option<f64>,
    weights: Option<Vec<f64>>,
    ops_mode: Option<String>,
    ftol: Option<f64>,
    xtol: Option<f64>,
    gtol: Option<f64>,
    max_nfev: Option<usize>,
    x_scale: Option<XScaleInput>,
    loss: Option<String>,
    f_scale: Option<f64>,
    metadata: Option<FitMetadataInput>,
}

fn fit_epoch(input: &FitConfigInput) -> Result<FitEpoch, JsValue> {
    if let Some(jd) = &input.epoch_jd {
        return Ok(FitEpoch::Jd(split_jd(jd, "epochJd")?));
    }
    if let Some(index) = input.epoch_sample_index {
        return Ok(FitEpoch::Sample(index));
    }
    match input.epoch.as_deref().unwrap_or("midpoint") {
        "midpoint" => Ok(FitEpoch::Midpoint),
        "first" => Ok(FitEpoch::First),
        "last" => Ok(FitEpoch::Last),
        "sample" => Err(type_error(
            "epoch \"sample\" requires epochSampleIndex to be set",
        )),
        other => Err(type_error(&format!(
            "invalid fit epoch {other:?}: expected \"midpoint\", \"first\", or \"last\""
        ))),
    }
}

fn fit_config(value: JsValue) -> Result<FitConfig, JsValue> {
    let input: FitConfigInput = if value.is_null() || value.is_undefined() {
        FitConfigInput::default()
    } else {
        serde_wasm_bindgen::from_value(value)
            .map_err(|e| type_error(&format!("invalid TLE fit config: {e}")))?
    };
    let defaults = FitConfig::default();
    let mut options = FitConfig::default();
    options.epoch = fit_epoch(&input)?;
    options.fit_bstar = input.fit_bstar.unwrap_or(defaults.fit_bstar);
    options.bstar_seed = input.bstar_seed.unwrap_or(defaults.bstar_seed);
    options.use_velocity = input.use_velocity.unwrap_or(defaults.use_velocity);
    options.velocity_weight_s = input.velocity_weight_s;
    options.weights = input.weights;
    options.opsmode = ops_mode(input.ops_mode)?;
    options.ftol = input.ftol;
    options.xtol = input.xtol;
    options.gtol = input.gtol;
    options.max_nfev = input.max_nfev;
    options.x_scale = x_scale(input.x_scale)?;
    options.loss = loss(input.loss.as_deref())?;
    options.f_scale = input.f_scale.unwrap_or(defaults.f_scale);
    options.metadata = input
        .metadata
        .map(|metadata| metadata.to_core())
        .unwrap_or(defaults.metadata);
    Ok(options)
}

/// Result of fitting a TLE to TEME samples.
#[wasm_bindgen]
pub struct TleFit {
    inner: CoreTleFit,
}

#[wasm_bindgen]
impl TleFit {
    #[wasm_bindgen(getter)]
    pub fn elements(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.inner.elements).map_err(|e| type_error(&e.to_string()))
    }

    #[wasm_bindgen(getter)]
    pub fn line1(&self) -> String {
        self.inner.line1.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn line2(&self) -> String {
        self.inner.line2.clone()
    }

    #[wasm_bindgen(js_name = toLines)]
    pub fn to_lines(&self) -> Vec<String> {
        vec![self.inner.line1.clone(), self.inner.line2.clone()]
    }

    #[wasm_bindgen(getter)]
    pub fn omm(&self) -> Omm {
        Omm::from_core(self.inner.omm.clone())
    }

    #[wasm_bindgen(getter)]
    pub fn stats(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.inner.stats).map_err(|e| type_error(&e.to_string()))
    }
}

/// Fit SGP4 mean elements and optional B* to TEME state samples.
#[wasm_bindgen(js_name = fitTle)]
pub fn fit_tle(samples: JsValue, config: JsValue) -> Result<TleFit, JsValue> {
    let sample_inputs: Vec<FitSampleInput> = serde_wasm_bindgen::from_value(samples)
        .map_err(|e| type_error(&format!("invalid TLE fit samples: {e}")))?;
    let core_samples: Vec<_> = sample_inputs
        .iter()
        .enumerate()
        .map(|(index, sample)| sample.to_core(index))
        .collect::<Result<_, _>>()?;
    let inner = core_fit_tle(&core_samples, &fit_config(config)?).map_err(fit_error)?;
    Ok(TleFit { inner })
}

#[wasm_bindgen]
impl Tle {
    /// Parse two TLE lines and initialize SGP4. `opsMode` is `"improved"`
    /// (default) or `"afspc"`. `policy` is `"strict"` (default), which refuses
    /// a column-69 checksum digit that disagrees and a column 69 that is not a
    /// digit, or `"lenient"`, which reads both and reports each in
    /// `checksumWarnings`. Throws an `Error` if the lines fail to parse or SGP4
    /// fails to initialize.
    #[wasm_bindgen(constructor)]
    pub fn new(
        line1: &str,
        line2: &str,
        ops_mode_label: Option<String>,
        policy: Option<String>,
    ) -> Result<Tle, JsValue> {
        let mode = ops_mode(ops_mode_label)?;
        let policy = tle_policy(policy)?;
        let parsed = parse_tle_with_policy(line1, line2, policy).map_err(tle_error)?;
        let (satellite, _) =
            Satellite::from_tle_with_policy(line1, line2, mode, policy).map_err(sgp4_error)?;
        Ok(Tle {
            elements: parsed.elements,
            satellite,
            checksum_warnings: parsed.checksum_warnings,
        })
    }

    /// Re-encode the parsed elements as the two 69-character TLE lines (with
    /// checksums), as a `[line1, line2]` string array. For a well-formed input
    /// the round-trip is character-exact.
    #[wasm_bindgen(js_name = toLines)]
    pub fn to_lines(&self) -> Result<Vec<String>, JsValue> {
        let (line1, line2) = encode_tle(&self.elements).map_err(tle_error)?;
        Ok(vec![line1, line2])
    }

    /// Checksum findings the policy accepted while parsing: a line with no
    /// column 69 under either policy, and under `"lenient"` also a mismatching
    /// or non-digit column 69. Empty when both lines' checksums are valid.
    #[wasm_bindgen(getter, js_name = checksumWarnings)]
    pub fn checksum_warnings(&self) -> Vec<ChecksumWarning> {
        self.checksum_warnings
            .iter()
            .map(ChecksumWarning::from)
            .collect()
    }

    /// Propagate over a `BigInt64Array` of unix-microsecond epochs. Returns TEME
    /// position (km) and velocity (km/s). Throws an `Error` on SGP4 failure.
    #[wasm_bindgen]
    pub fn propagate(&self, epochs_unix_us: &[i64]) -> Result<TlePropagation, JsValue> {
        propagate_satellite(&self.satellite, epochs_unix_us)
    }

    /// Propagate over unix-microsecond epochs with an opt-in decay latch.
    ///
    /// The returned TEME arrays match [`propagate`] until the first decay-like
    /// SGP4 failure. At that point the core latch records the failing epoch and
    /// this method throws. Later calls using the same latch at that epoch or a
    /// later one also throw, while raw [`propagate`] remains stateless.
    #[wasm_bindgen(js_name = propagateWithDecayLatch)]
    pub fn propagate_with_decay_latch(
        &self,
        epochs_unix_us: &[i64],
        latch: &mut DecayLatch,
    ) -> Result<TlePropagation, JsValue> {
        let mut positions = Vec::with_capacity(epochs_unix_us.len() * 3);
        let mut velocities = Vec::with_capacity(epochs_unix_us.len() * 3);
        for &epoch_unix_us in epochs_unix_us {
            let jd = unix_us_to_julian_date(epoch_unix_us);
            let prediction = self
                .satellite
                .propagate_jd_with_decay_latch(jd, &mut latch.inner)
                .map_err(decay_latched_error)?;
            positions.extend_from_slice(&prediction.position);
            velocities.extend_from_slice(&prediction.velocity);
        }
        Ok(TlePropagation {
            positions,
            velocities,
        })
    }

    /// Topocentric azimuth/elevation/range from `station` over a
    /// `BigInt64Array` of unix-microsecond epochs. Throws an `Error` on failure.
    #[wasm_bindgen(js_name = lookAngles)]
    pub fn look_angles(
        &self,
        station: &GroundStation,
        epochs_unix_us: &[i64],
    ) -> Result<LookAngles, JsValue> {
        let looks = look_angle_arc(&self.satellite, station.inner, &instants(epochs_unix_us))
            .map_err(look_angle_error)?;
        Ok(look_angles_js(&looks))
    }

    /// [`lookAngles`] under a UT1 validity policy: `"strict"` (the default)
    /// refuses an epoch outside the UT1 table, `"permissive"` accepts it.
    /// Returns `{ value, ut1Degraded }` with `value` the `LookAngles`.
    #[wasm_bindgen(js_name = lookAnglesWithValidity, unchecked_return_type = "Ut1Validated<LookAngles>")]
    pub fn look_angles_with_validity(
        &self,
        station: &GroundStation,
        epochs_unix_us: &[i64],
        ut1: Option<String>,
    ) -> Result<JsValue, JsValue> {
        let validated = look_angle_arc_with_validity(
            &self.satellite,
            station.inner,
            &instants(epochs_unix_us),
            ut1_validity(ut1)?,
        )
        .map_err(look_angle_error)?;
        validated_object(
            &JsValue::from(look_angles_js(&validated.value)),
            validated.degraded,
        )
    }

    /// Find passes over `station` within `[startUnixUs, endUnixUs)` by dense
    /// elevation sampling. `elevationMaskDeg` defaults to 0, `stepSeconds` to 30,
    /// `timeToleranceS` to 1e-3. Throws a `RangeError` on a non-positive step or
    /// an end at or before the start.
    #[wasm_bindgen(js_name = findPasses)]
    pub fn find_passes(
        &self,
        station: &GroundStation,
        start_unix_us: i64,
        end_unix_us: i64,
        elevation_mask_deg: Option<f64>,
        step_seconds: Option<f64>,
        time_tolerance_s: Option<f64>,
    ) -> Result<Vec<SatellitePass>, JsValue> {
        let options = pass_options(elevation_mask_deg, step_seconds, time_tolerance_s)?;
        if end_unix_us <= start_unix_us {
            return Err(range_error("endUnixUs must be after startUnixUs"));
        }
        let passes = find_passes_for_satellite(
            &self.satellite,
            station.inner,
            UtcInstant::from_unix_microseconds(start_unix_us),
            UtcInstant::from_unix_microseconds(end_unix_us),
            options,
        )
        .map_err(pass_error)?;
        Ok(passes.iter().map(SatellitePass::from).collect())
    }

    /// [`findPasses`] under a UT1 validity policy. The search checks every
    /// instant it evaluates, so under `"strict"` (the default) a window
    /// reaching past the UT1 table is refused rather than cut short. Returns
    /// `{ value, ut1Degraded }` with `value` the `SatellitePass[]`.
    #[wasm_bindgen(js_name = findPassesWithValidity, unchecked_return_type = "Ut1Validated<SatellitePass[]>")]
    #[allow(clippy::too_many_arguments)]
    pub fn find_passes_with_validity(
        &self,
        station: &GroundStation,
        start_unix_us: i64,
        end_unix_us: i64,
        elevation_mask_deg: Option<f64>,
        step_seconds: Option<f64>,
        time_tolerance_s: Option<f64>,
        ut1: Option<String>,
    ) -> Result<JsValue, JsValue> {
        let options = pass_options(elevation_mask_deg, step_seconds, time_tolerance_s)?;
        if end_unix_us <= start_unix_us {
            return Err(range_error("endUnixUs must be after startUnixUs"));
        }
        let validated = find_passes_for_satellite_with_validity(
            &self.satellite,
            station.inner,
            UtcInstant::from_unix_microseconds(start_unix_us),
            UtcInstant::from_unix_microseconds(end_unix_us),
            options,
            ut1_validity(ut1)?,
        )
        .map_err(pass_error)?;
        validated_object(
            &js_array(validated.value.iter().map(SatellitePass::from)),
            validated.degraded,
        )
    }

    /// Topocentric visibility arrays and a dense pass list over an epoch grid.
    ///
    /// `epochsUnixUs` must be a strictly increasing `BigInt64Array` with at least
    /// two samples. The az/el geometry is the same path as [`lookAngles`]; the
    /// pass list is the dense finder over `[first, last]`.
    #[wasm_bindgen(js_name = visibilitySeries)]
    pub fn visibility_series(
        &self,
        station: &GroundStation,
        epochs_unix_us: &[i64],
        elevation_mask_deg: Option<f64>,
        step_seconds: Option<f64>,
        time_tolerance_s: Option<f64>,
    ) -> Result<VisibilitySeries, JsValue> {
        let mask = elevation_mask_deg.unwrap_or(PassFinderOptions::default().elevation_mask_deg);
        let options = pass_options(elevation_mask_deg, step_seconds, time_tolerance_s)?;
        let inst = instants(epochs_unix_us);
        if inst.len() < 2 {
            return Err(type_error("epochsUnixUs must contain at least two samples"));
        }
        if epochs_unix_us.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(range_error("epochsUnixUs must be strictly increasing"));
        }
        let looks =
            look_angle_arc(&self.satellite, station.inner, &inst).map_err(look_angle_error)?;
        let passes = find_passes_for_satellite(
            &self.satellite,
            station.inner,
            inst[0],
            *inst.last().expect("non-empty instants checked"),
            options,
        )
        .map_err(pass_error)?;
        Ok(VisibilitySeries {
            epochs_unix_us: epochs_unix_us.to_vec(),
            azimuth_deg: looks.iter().map(|l| l.azimuth_deg).collect(),
            elevation_deg: looks.iter().map(|l| l.elevation_deg).collect(),
            range_km: looks.iter().map(|l| l.range_km).collect(),
            visible: looks
                .iter()
                .map(|l| u8::from(visible_at_elevation_mask(l.elevation_deg, mask)))
                .collect(),
            passes: passes.iter().map(SatellitePass::from).collect(),
        })
    }

    /// Sub-satellite (ground-track) WGS84 geodetic points over a `BigInt64Array`
    /// of unix-microsecond epochs. For each epoch the satellite is propagated to
    /// TEME and reduced TEME->GCRS->ECEF->geodetic by the engine's own transforms,
    /// honoring this `Tle`'s opsmode. Throws an `Error` on propagation or frame
    /// failure.
    #[wasm_bindgen(js_name = groundTrack)]
    pub fn ground_track(&self, epochs_unix_us: &[i64]) -> Result<GroundTrack, JsValue> {
        let points =
            ground_track(&self.satellite, &instants(epochs_unix_us)).map_err(look_angle_error)?;
        Ok(ground_track_js(&points))
    }

    /// [`groundTrack`] under a UT1 validity policy, as
    /// [`lookAnglesWithValidity`]. Returns `{ value, ut1Degraded }` with
    /// `value` the `GroundTrack`.
    #[wasm_bindgen(js_name = groundTrackWithValidity, unchecked_return_type = "Ut1Validated<GroundTrack>")]
    pub fn ground_track_with_validity(
        &self,
        epochs_unix_us: &[i64],
        ut1: Option<String>,
    ) -> Result<JsValue, JsValue> {
        let validated = ground_track_with_validity(
            &self.satellite,
            &instants(epochs_unix_us),
            ut1_validity(ut1)?,
        )
        .map_err(look_angle_error)?;
        validated_object(
            &JsValue::from(ground_track_js(&validated.value)),
            validated.degraded,
        )
    }

    /// NORAD catalog number (as recorded in the TLE).
    #[wasm_bindgen(getter, js_name = catalogNumber)]
    pub fn catalog_number(&self) -> String {
        self.elements.catalog_number.clone()
    }

    /// Classification character (`U`/`C`/`S`).
    #[wasm_bindgen(getter)]
    pub fn classification(&self) -> String {
        self.elements.classification.clone()
    }

    /// International designator (COSPAR ID).
    #[wasm_bindgen(getter, js_name = internationalDesignator)]
    pub fn international_designator(&self) -> String {
        self.elements.international_designator.clone()
    }

    /// Four-digit epoch year.
    #[wasm_bindgen(getter, js_name = epochYear)]
    pub fn epoch_year(&self) -> i32 {
        self.elements.epoch_year
    }

    /// Fractional day-of-year of the epoch.
    #[wasm_bindgen(getter, js_name = epochDayOfYear)]
    pub fn epoch_day_of_year(&self) -> f64 {
        self.elements.epoch_day_of_year
    }

    /// Inclination, degrees.
    #[wasm_bindgen(getter, js_name = inclinationDeg)]
    pub fn inclination_deg(&self) -> f64 {
        self.elements.inclination_deg
    }

    /// Right ascension of the ascending node, degrees.
    #[wasm_bindgen(getter, js_name = raanDeg)]
    pub fn raan_deg(&self) -> f64 {
        self.elements.raan_deg
    }

    /// Orbital eccentricity (dimensionless).
    #[wasm_bindgen(getter)]
    pub fn eccentricity(&self) -> f64 {
        self.elements.eccentricity
    }

    /// Argument of perigee, degrees.
    #[wasm_bindgen(getter, js_name = argPerigeeDeg)]
    pub fn arg_perigee_deg(&self) -> f64 {
        self.elements.arg_perigee_deg
    }

    /// Mean anomaly at epoch, degrees.
    #[wasm_bindgen(getter, js_name = meanAnomalyDeg)]
    pub fn mean_anomaly_deg(&self) -> f64 {
        self.elements.mean_anomaly_deg
    }

    /// Mean motion, revolutions per day.
    #[wasm_bindgen(getter, js_name = meanMotionRevPerDay)]
    pub fn mean_motion_rev_per_day(&self) -> f64 {
        self.elements.mean_motion
    }

    /// First derivative of mean motion (rev/day^2).
    #[wasm_bindgen(getter, js_name = meanMotionDot)]
    pub fn mean_motion_dot(&self) -> f64 {
        self.elements.mean_motion_dot
    }

    /// Second derivative of mean motion (rev/day^3).
    #[wasm_bindgen(getter, js_name = meanMotionDoubleDot)]
    pub fn mean_motion_double_dot(&self) -> f64 {
        self.elements.mean_motion_double_dot
    }

    /// B* drag term (TLE dimensionless convention).
    #[wasm_bindgen(getter)]
    pub fn bstar(&self) -> f64 {
        self.elements.bstar
    }

    /// Revolution number at epoch, or `undefined` when the field is blank.
    #[wasm_bindgen(getter, js_name = revNumber)]
    pub fn rev_number(&self) -> Option<i32> {
        self.elements.rev_number
    }

    /// Element-set number (line 1 columns 65-68), or `undefined` when the
    /// field is blank.
    #[wasm_bindgen(getter, js_name = elementSetNumber)]
    pub fn element_set_number(&self) -> Option<i32> {
        self.elements.elset_number
    }

    /// Ephemeris type (line 1 column 63), or `undefined` when the field is
    /// blank. SGP4 reads a blank type as 0.
    #[wasm_bindgen(getter, js_name = ephemerisType)]
    pub fn ephemeris_type(&self) -> Option<i32> {
        self.elements.ephemeris_type
    }
}

/// A named entry from a parsed TLE file: the satellite's name line (empty for a
/// bare 2-line set) paired with its initialized [`Tle`].
#[wasm_bindgen]
#[derive(Clone)]
pub struct NamedTle {
    name: String,
    tle: Tle,
    line_number: usize,
}

#[wasm_bindgen]
impl NamedTle {
    /// The satellite name from the preceding name line, with any CelesTrak `0 `
    /// marker stripped. Empty string for a bare 2-line element set.
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// The initialized two-line element set. Call `.propagate()` /
    /// `.lookAngles()` / `.findPasses()` on it directly.
    #[wasm_bindgen(getter)]
    pub fn tle(&self) -> Tle {
        self.tle.clone()
    }

    /// One-based line number of the record's line 1 in the file text.
    #[wasm_bindgen(getter, js_name = lineNumber)]
    pub fn line_number(&self) -> usize {
        self.line_number
    }

    /// Checksum findings the policy accepted for this record (the same list
    /// as `tle.checksumWarnings`).
    #[wasm_bindgen(getter, js_name = checksumWarnings)]
    pub fn checksum_warnings(&self) -> Vec<ChecksumWarning> {
        self.tle.checksum_warnings()
    }
}

/// A stretch of a TLE file the reader did not turn into a satellite.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RejectedTleRecordJs {
    line_number: usize,
    name: String,
    issue: &'static str,
    message: String,
    detail: serde_json::Value,
}

impl From<&RejectedTleRecord> for RejectedTleRecordJs {
    fn from(record: &RejectedTleRecord) -> Self {
        Self {
            line_number: record.line_number,
            name: record.name.clone(),
            issue: match record.issue {
                TleRecordIssue::Invalid(_) => "invalid",
                TleRecordIssue::MissingLine2 => "missingLine2",
                TleRecordIssue::OrphanLine2 => "orphanLine2",
                TleRecordIssue::OrphanName => "orphanName",
            },
            message: record.issue.to_string(),
            detail: record_issue_cause(&record.issue),
        }
    }
}

/// The result of [`parseTleFile`]: the satellites that parsed, and every
/// other non-blank stretch of the file with the reason it did not.
#[wasm_bindgen]
pub struct ParsedTleFile {
    satellites: Vec<NamedTle>,
    rejected: Vec<RejectedTleRecord>,
}

#[wasm_bindgen]
impl ParsedTleFile {
    /// The successfully parsed satellites, in file order, each as a `{ name, tle }`.
    #[wasm_bindgen(getter)]
    pub fn satellites(&self) -> Vec<NamedTle> {
        self.satellites.clone()
    }

    /// Number of entries in `rejected`.
    #[wasm_bindgen(getter)]
    pub fn skipped(&self) -> usize {
        self.rejected.len()
    }

    /// Every rejected record, stray line and orphan name line, in file order,
    /// as `{ lineNumber, name, issue, message, detail }`. `lineNumber` is one-based
    /// and names the name line when the record had one; `issue` is
    /// `"invalid"` (refused by the TLE grammar, the checksum policy or SGP4
    /// initialization), `"missingLine2"`, `"orphanLine2"` or `"orphanName"`.
    /// `detail` preserves the exact issue variant and, for invalid records,
    /// the complete SGP4 error payload.
    #[wasm_bindgen(getter, unchecked_return_type = "RejectedTleRecord[]")]
    pub fn rejected(&self) -> Result<JsValue, JsValue> {
        crate::error::to_plain_js(
            &self
                .rejected
                .iter()
                .map(RejectedTleRecordJs::from)
                .collect::<Vec<_>>(),
            "rejected TLE records",
        )
    }

    /// Number of satellites that parsed (length of `satellites`).
    #[wasm_bindgen(getter)]
    pub fn count(&self) -> usize {
        self.satellites.len()
    }
}

/// Parse a multi-record TLE file (CelesTrak / Space-Track style) into named,
/// initialized [`Tle`] instances. Handles bare 2-line sets, 3-line name+line1+line2
/// sets, and CelesTrak `0 NAME` markers; CRLF endings, blank lines, and
/// surrounding whitespace are tolerated. A record that fails is kept out of
/// `satellites` and listed in `rejected` with its line number and reason; the
/// rest of the file is still read. `opsMode` is `"improved"` (default) or
/// `"afspc"`; `policy` is `"strict"` (default) or `"lenient"`, as for `new Tle`.
#[wasm_bindgen(js_name = parseTleFile)]
pub fn parse_tle_file(
    text: &str,
    ops_mode_label: Option<String>,
    policy: Option<String>,
) -> Result<ParsedTleFile, JsValue> {
    let mode = ops_mode(ops_mode_label)?;
    let policy = tle_policy(policy)?;
    let parsed = parse_tle_file_with_policy(text, mode, policy);
    let mut satellites = Vec::with_capacity(parsed.satellites.len());
    for named in parsed.satellites {
        satellites.push(NamedTle {
            name: named.name,
            line_number: named.line_number,
            tle: Tle::from_core_satellite(named.satellite, policy, named.checksum_warnings)?,
        });
    }
    Ok(ParsedTleFile {
        satellites,
        rejected: parsed.rejected,
    })
}

fn pass_options(
    elevation_mask_deg: Option<f64>,
    step_seconds: Option<f64>,
    time_tolerance_s: Option<f64>,
) -> Result<PassFinderOptions, JsValue> {
    let core_default = PassFinderOptions::default();
    let elevation_mask_deg = elevation_mask_deg.unwrap_or(core_default.elevation_mask_deg);
    let step_seconds = step_seconds.unwrap_or(core_default.coarse_step_seconds);
    let time_tolerance_seconds = time_tolerance_s.unwrap_or(core_default.time_tolerance_seconds);
    if !elevation_mask_deg.is_finite() {
        return Err(range_error("elevationMaskDeg must be finite"));
    }
    if !step_seconds.is_finite() || step_seconds <= 0.0 {
        return Err(range_error("stepSeconds must be positive"));
    }
    if !time_tolerance_seconds.is_finite() || time_tolerance_seconds <= 0.0 {
        return Err(range_error("timeToleranceS must be positive"));
    }
    let mut options = PassFinderOptions::default();
    options.elevation_mask_deg = elevation_mask_deg;
    options.coarse_step_seconds = step_seconds;
    options.time_tolerance_seconds = time_tolerance_seconds;
    Ok(options)
}

/// TEME states from a batched SGP4 propagation. Each array is flat row-major,
/// length `3 * epochCount`.
#[wasm_bindgen]
pub struct TlePropagation {
    positions: Vec<f64>,
    velocities: Vec<f64>,
}

#[wasm_bindgen]
impl TlePropagation {
    /// TEME positions, km, flat `[x0, y0, z0, x1, ...]`.
    #[wasm_bindgen(getter, js_name = positionKm)]
    pub fn position_km(&self) -> Vec<f64> {
        self.positions.clone()
    }

    /// TEME velocities, km/s, flat `[vx0, vy0, vz0, ...]`.
    #[wasm_bindgen(getter, js_name = velocityKmS)]
    pub fn velocity_km_s(&self) -> Vec<f64> {
        self.velocities.clone()
    }

    /// Number of epochs propagated.
    #[wasm_bindgen(getter, js_name = epochCount)]
    pub fn epoch_count(&self) -> usize {
        self.positions.len() / 3
    }
}

/// Topocentric look angles from a batched arc, each a `Float64Array` of length
/// `epochCount`.
#[wasm_bindgen]
pub struct LookAngles {
    azimuth_deg: Vec<f64>,
    elevation_deg: Vec<f64>,
    range_km: Vec<f64>,
}

#[wasm_bindgen]
impl LookAngles {
    /// Azimuth, degrees clockwise from north.
    #[wasm_bindgen(getter, js_name = azimuthDeg)]
    pub fn azimuth_deg(&self) -> Vec<f64> {
        self.azimuth_deg.clone()
    }

    /// Elevation, degrees above the horizon.
    #[wasm_bindgen(getter, js_name = elevationDeg)]
    pub fn elevation_deg(&self) -> Vec<f64> {
        self.elevation_deg.clone()
    }

    /// Slant range, kilometres.
    #[wasm_bindgen(getter, js_name = rangeKm)]
    pub fn range_km(&self) -> Vec<f64> {
        self.range_km.clone()
    }

    /// Number of epochs evaluated.
    #[wasm_bindgen(getter, js_name = epochCount)]
    pub fn epoch_count(&self) -> usize {
        self.azimuth_deg.len()
    }
}

/// Per-epoch topocentric visibility plus the dense pass list over the grid
/// window.
#[wasm_bindgen]
pub struct VisibilitySeries {
    epochs_unix_us: Vec<i64>,
    azimuth_deg: Vec<f64>,
    elevation_deg: Vec<f64>,
    range_km: Vec<f64>,
    visible: Vec<u8>,
    passes: Vec<SatellitePass>,
}

#[wasm_bindgen]
impl VisibilitySeries {
    /// Epoch grid, UTC unix microseconds, as a `BigInt64Array`.
    #[wasm_bindgen(getter, js_name = epochUnixUs)]
    pub fn epoch_unix_us(&self) -> Vec<i64> {
        self.epochs_unix_us.clone()
    }

    /// Azimuth, degrees clockwise from north.
    #[wasm_bindgen(getter, js_name = azimuthDeg)]
    pub fn azimuth_deg(&self) -> Vec<f64> {
        self.azimuth_deg.clone()
    }

    /// Elevation, degrees above the horizon.
    #[wasm_bindgen(getter, js_name = elevationDeg)]
    pub fn elevation_deg(&self) -> Vec<f64> {
        self.elevation_deg.clone()
    }

    /// Slant range, kilometres.
    #[wasm_bindgen(getter, js_name = rangeKm)]
    pub fn range_km(&self) -> Vec<f64> {
        self.range_km.clone()
    }

    /// Visibility mask as a `Uint8Array` (1 where `elevationDeg >=
    /// elevationMaskDeg`, else 0).
    #[wasm_bindgen(getter)]
    pub fn visible(&self) -> Vec<u8> {
        self.visible.clone()
    }

    /// Dense pass-finder results over the epoch-grid window.
    #[wasm_bindgen(getter)]
    pub fn passes(&self) -> Vec<SatellitePass> {
        self.passes.clone()
    }

    /// Number of epochs evaluated.
    #[wasm_bindgen(getter, js_name = epochCount)]
    pub fn epoch_count(&self) -> usize {
        self.epochs_unix_us.len()
    }

    /// Number of passes found over the epoch-grid window.
    #[wasm_bindgen(getter, js_name = passCount)]
    pub fn pass_count(&self) -> usize {
        self.passes.len()
    }
}

/// Sub-satellite ground-track points from a batched [`Tle.groundTrack`] call.
/// Each array is a `Float64Array` of length `epochCount`, aligned to the input
/// epoch grid. WGS84 geodetic: latitude/longitude in degrees, ellipsoidal height
/// in kilometres.
#[wasm_bindgen]
pub struct GroundTrack {
    latitude_deg: Vec<f64>,
    longitude_deg: Vec<f64>,
    altitude_km: Vec<f64>,
}

#[wasm_bindgen]
impl GroundTrack {
    /// Geodetic latitude of the sub-satellite point, degrees north.
    #[wasm_bindgen(getter, js_name = latDeg)]
    pub fn lat_deg(&self) -> Vec<f64> {
        self.latitude_deg.clone()
    }

    /// Geodetic longitude of the sub-satellite point, degrees east in `[-180, 180]`.
    #[wasm_bindgen(getter, js_name = lonDeg)]
    pub fn lon_deg(&self) -> Vec<f64> {
        self.longitude_deg.clone()
    }

    /// Ellipsoidal height above the WGS84 ellipsoid, kilometres.
    #[wasm_bindgen(getter, js_name = altKm)]
    pub fn alt_km(&self) -> Vec<f64> {
        self.altitude_km.clone()
    }

    /// Number of epochs evaluated.
    #[wasm_bindgen(getter, js_name = epochCount)]
    pub fn epoch_count(&self) -> usize {
        self.latitude_deg.len()
    }
}

/// One satellite visible above the elevation mask at a single instant, from
/// [`visibleFromSatellites`].
#[wasm_bindgen]
#[derive(Clone)]
pub struct VisibleSatellite {
    catalog_number: String,
    azimuth_deg: f64,
    elevation_deg: f64,
    range_km: f64,
    position_km: Vec<f64>,
}

impl From<&CoreVisibleSatellite> for VisibleSatellite {
    fn from(v: &CoreVisibleSatellite) -> Self {
        Self {
            catalog_number: v.catalog_number.clone(),
            azimuth_deg: v.azimuth_deg,
            elevation_deg: v.elevation_deg,
            range_km: v.range_km,
            position_km: v.position_km.to_vec(),
        }
    }
}

#[wasm_bindgen]
impl VisibleSatellite {
    /// The caller-supplied identity (the `ids[i]` paired with this satellite):
    /// a NORAD catalog number, a name, or whatever the caller chose.
    #[wasm_bindgen(getter, js_name = catalogNumber)]
    pub fn catalog_number(&self) -> String {
        self.catalog_number.clone()
    }

    /// Topocentric azimuth, degrees clockwise from north.
    #[wasm_bindgen(getter, js_name = azimuthDeg)]
    pub fn azimuth_deg(&self) -> f64 {
        self.azimuth_deg
    }

    /// Topocentric elevation, degrees above the horizon.
    #[wasm_bindgen(getter, js_name = elevationDeg)]
    pub fn elevation_deg(&self) -> f64 {
        self.elevation_deg
    }

    /// Slant range from the ground station, kilometres.
    #[wasm_bindgen(getter, js_name = rangeKm)]
    pub fn range_km(&self) -> f64 {
        self.range_km
    }

    /// TEME position of the satellite at the instant, km, as a length-3
    /// `Float64Array` `[x, y, z]`.
    #[wasm_bindgen(getter, js_name = positionKm)]
    pub fn position_km(&self) -> Vec<f64> {
        self.position_km.clone()
    }
}

/// Satellites visible above `minElevationDeg` from `station` at a single instant,
/// from already-initialized [`Tle`]s: the opsmode-preserving constellation
/// snapshot.
///
/// Each `Tle` in `satellites` carries the opsmode it was constructed with, so a
/// deep-space / opsmode-sensitive object is evaluated in its own mode (unlike the
/// element-based core path, which hardcodes AFSPC). `ids` supplies the identity
/// out-of-band: `ids[i]` (a catalog number, name, or anything) becomes the
/// `catalogNumber` of `satellites[i]`, so the two arrays must be the same length.
/// The `Tle` instances are consumed by this call.
///
/// `epochUnixUs` is a unix-microsecond UTC `bigint`. Per-satellite propagation or
/// frame failures are skipped; the result is filtered by `minElevationDeg` and
/// sorted by elevation descending. Throws an `Error` on an invalid station,
/// elevation threshold, or `ids`/`satellites` length mismatch.
#[wasm_bindgen(js_name = visibleFromSatellites)]
pub fn visible_from_satellites_js(
    satellites: Vec<Tle>,
    ids: Vec<String>,
    station: &GroundStation,
    epoch_unix_us: i64,
    min_elevation_deg: f64,
) -> Result<Vec<VisibleSatellite>, JsValue> {
    let sats: Vec<Satellite> = satellites.into_iter().map(|t| t.satellite).collect();
    let visible = visible_from_satellites(
        &sats,
        &ids,
        station.inner,
        UtcInstant::from_unix_microseconds(epoch_unix_us),
        min_elevation_deg,
    )
    .map_err(pass_error)?;
    Ok(visible.iter().map(VisibleSatellite::from).collect())
}

/// [`visibleFromSatellites`] under a UT1 validity policy: `"strict"` (the
/// default) refuses an epoch outside the UT1 table, `"permissive"` accepts it.
/// Returns `{ value, ut1Degraded }` with `value` the `VisibleSatellite[]`.
#[wasm_bindgen(
    js_name = visibleFromSatellitesWithValidity,
    unchecked_return_type = "Ut1Validated<VisibleSatellite[]>"
)]
pub fn visible_from_satellites_with_validity_js(
    satellites: Vec<Tle>,
    ids: Vec<String>,
    station: &GroundStation,
    epoch_unix_us: i64,
    min_elevation_deg: f64,
    ut1: Option<String>,
) -> Result<JsValue, JsValue> {
    let sats: Vec<Satellite> = satellites.into_iter().map(|t| t.satellite).collect();
    let validated = visible_from_satellites_with_validity(
        &sats,
        &ids,
        station.inner,
        UtcInstant::from_unix_microseconds(epoch_unix_us),
        min_elevation_deg,
        ut1_validity(ut1)?,
    )
    .map_err(pass_error)?;
    validated_object(
        &js_array(validated.value.iter().map(VisibleSatellite::from)),
        validated.degraded,
    )
}

/// Propagate a fleet of already-initialized [`Tle`]s over a shared epoch grid in
/// a single call: the batched form of [`Tle.propagate`].
///
/// Element `(i, j)` of the result is `satellites[i]` propagated to
/// `epochsUnixUs[j]`, bit-for-bit identical to `satellites[i].propagate([
/// epochsUnixUs[j] ])` on its own; this is a thin wrapper over the engine's
/// serial batch kernel (the binding never spawns the rayon thread pool, since
/// wasm is single-threaded). Each `Tle` carries the opsmode it was constructed
/// with, so a deep-space / opsmode-sensitive object is propagated in its own
/// mode. The `Tle` instances are consumed by this call.
///
/// `epochsUnixUs` is a `BigInt64Array` of unix-microsecond UTC epochs shared by
/// every satellite. The hot case for a constellation animation is a single epoch
/// (`epochCount == 1`), giving one TEME state per satellite, but any epoch count
/// is supported. An empty fleet or empty epoch grid yields empty arrays. Throws
/// an `Error` (naming the satellite index) if a satellite fails to propagate.
#[wasm_bindgen(js_name = propagateBatch)]
pub fn propagate_batch(
    satellites: Vec<Tle>,
    epochs_unix_us: &[i64],
) -> Result<FleetPropagation, JsValue> {
    let sats: Vec<Satellite> = satellites.into_iter().map(|t| t.satellite).collect();
    let satellite_count = sats.len();
    let datetimes = instants(epochs_unix_us);
    let epoch_count = datetimes.len();

    let results = propagate_teme_batch_serial(&sats, &datetimes);

    let mut positions = Vec::with_capacity(satellite_count * epoch_count * 3);
    let mut velocities = Vec::with_capacity(satellite_count * epoch_count * 3);
    for (idx, arc) in results.into_iter().enumerate() {
        let predictions = arc.map_err(|e| indexed_sgp4_error(idx, e))?;
        for p in &predictions {
            positions.extend_from_slice(&p.position);
            velocities.extend_from_slice(&p.velocity);
        }
    }

    Ok(FleetPropagation {
        satellite_count,
        epoch_count,
        positions,
        velocities,
    })
}

/// TEME states from a batched fleet SGP4 propagation. Each array is flat
/// row-major with shape `(satelliteCount, epochCount, 3)`: satellite `i`'s arc
/// occupies the contiguous slice `[i * epochCount * 3 .. (i + 1) * epochCount *
/// 3]`, and within it epoch `j` is `[.. j * 3 + 3]`. Satellite `i`'s arc equals
/// the [`TlePropagation`] from `satellites[i].propagate(epochsUnixUs)`.
#[wasm_bindgen]
pub struct FleetPropagation {
    satellite_count: usize,
    epoch_count: usize,
    positions: Vec<f64>,
    velocities: Vec<f64>,
}

#[wasm_bindgen]
impl FleetPropagation {
    /// TEME positions, km, flat row-major `(satelliteCount, epochCount, 3)`,
    /// length `3 * satelliteCount * epochCount`.
    #[wasm_bindgen(getter, js_name = positionKm)]
    pub fn position_km(&self) -> Vec<f64> {
        self.positions.clone()
    }

    /// TEME velocities, km/s, flat row-major `(satelliteCount, epochCount, 3)`,
    /// length `3 * satelliteCount * epochCount`.
    #[wasm_bindgen(getter, js_name = velocityKmS)]
    pub fn velocity_km_s(&self) -> Vec<f64> {
        self.velocities.clone()
    }

    /// Number of satellites in the fleet (the leading axis).
    #[wasm_bindgen(getter, js_name = satelliteCount)]
    pub fn satellite_count(&self) -> usize {
        self.satellite_count
    }

    /// Number of epochs each satellite was propagated to (the second axis).
    #[wasm_bindgen(getter, js_name = epochCount)]
    pub fn epoch_count(&self) -> usize {
        self.epoch_count
    }
}

/// A built-once constellation of already-initialized SGP4 satellites for repeated
/// batch operations.
///
/// Build it once from parsed [`Tle`]s, then call [`Constellation.propagate`] (and
/// `visible` / `lookAngleArcs` / `groundTracks` / `passes`) as often as you like:
/// it OWNS its satellites and BORROWS them on each call, so unlike the free
/// [`propagateBatch`] (which consumes the `Tle` handles it is given) the same
/// `Constellation` drives a live scene across frames with no re-parse and no
/// per-frame handle churn. This is the JS form of Elixir's `Sidereon.Constellation`.
///
/// It does no parsing or I/O: TLE text becomes satellites at the interface
/// boundary ([`Tle`] / [`parseTleFile`]); the constellation only batches the core
/// geometry over the satellites it was handed.
#[wasm_bindgen]
pub struct Constellation {
    satellites: Vec<Satellite>,
    ids: Vec<String>,
}

#[wasm_bindgen]
impl Constellation {
    /// Build a constellation from already-parsed [`Tle`]s, taking ownership of
    /// them. Each `Tle` keeps the opsmode it was constructed with, and its NORAD
    /// catalog number becomes the satellite's id in `visible`. The input order is
    /// the fleet order (the leading axis of every batch result and the
    /// `satelliteIndex` of every pass). The `Tle` handles are consumed; clone first
    /// (`tle.clone()`) to keep a per-satellite handle.
    #[wasm_bindgen(constructor)]
    pub fn new(satellites: Vec<Tle>) -> Constellation {
        let ids = satellites
            .iter()
            .map(|t| t.elements.catalog_number.clone())
            .collect();
        Constellation {
            satellites: satellites.into_iter().map(|t| t.satellite).collect(),
            ids,
        }
    }

    /// Number of satellites in the constellation (the leading axis of every batch
    /// result).
    #[wasm_bindgen(getter, js_name = satelliteCount)]
    pub fn satellite_count(&self) -> usize {
        self.satellites.len()
    }

    /// The satellites' NORAD catalog numbers, in fleet order.
    #[wasm_bindgen(getter, js_name = catalogNumbers)]
    pub fn catalog_numbers(&self) -> Vec<String> {
        self.ids.clone()
    }

    /// Propagate the whole constellation over a shared epoch grid in one call,
    /// borrowing it (NOT consumed, so the same `Constellation` drives every frame).
    ///
    /// `epochsUnixUs` is a `BigInt64Array` of unix-microsecond UTC epochs shared
    /// by every satellite. Element `(i, j)` of the result is satellite `i`
    /// propagated to epoch `j`, bit-for-bit identical to the per-satellite
    /// [`Tle.propagate`] path. A satellite that fails to propagate yields `NaN`
    /// for all of its epochs, keeping the result index-aligned (mirroring Elixir's
    /// `propagate_all`, which surfaces per-satellite outcomes rather than failing
    /// the whole batch). An empty constellation or empty epoch grid yields empty
    /// arrays.
    #[wasm_bindgen]
    pub fn propagate(&self, epochs_unix_us: &[i64]) -> FleetPropagation {
        let datetimes = instants(epochs_unix_us);
        let epoch_count = datetimes.len();
        let satellite_count = self.satellites.len();

        let results = propagate_teme_batch_serial(&self.satellites, &datetimes);

        let mut positions = Vec::with_capacity(satellite_count * epoch_count * 3);
        let mut velocities = Vec::with_capacity(satellite_count * epoch_count * 3);
        for arc in results {
            match arc {
                Ok(predictions) => {
                    for p in &predictions {
                        positions.extend_from_slice(&p.position);
                        velocities.extend_from_slice(&p.velocity);
                    }
                }
                Err(_) => {
                    // Index-aligned NaN fill: a failed satellite never drops the
                    // fleet out of alignment or freezes a live frame.
                    for _ in 0..epoch_count * 3 {
                        positions.push(f64::NAN);
                        velocities.push(f64::NAN);
                    }
                }
            }
        }

        FleetPropagation {
            satellite_count,
            epoch_count,
            positions,
            velocities,
        }
    }

    /// Satellites above `minElevationDeg` from `station` at a single epoch, each
    /// with its catalog number and topocentric az/el/range, sorted by elevation
    /// (highest first). The constellation form of the core `visibleFromSatellites`
    /// (Elixir `Constellation.visible_from`). Throws on an invalid station or
    /// elevation threshold.
    #[wasm_bindgen]
    pub fn visible(
        &self,
        station: &GroundStation,
        epoch_unix_us: i64,
        min_elevation_deg: f64,
    ) -> Result<Vec<VisibleSatellite>, JsValue> {
        let visible = visible_from_satellites(
            &self.satellites,
            &self.ids,
            station.inner,
            UtcInstant::from_unix_microseconds(epoch_unix_us),
            min_elevation_deg,
        )
        .map_err(pass_error)?;
        Ok(visible.iter().map(VisibleSatellite::from).collect())
    }

    /// Topocentric az/el/range arcs from `station` for every satellite over a
    /// shared epoch grid, in fleet order (element `i` is satellite `i`'s arc). A
    /// satellite that fails to propagate yields an empty arc, so the result stays
    /// index-aligned with the constellation. The batched form of [`Tle.lookAngles`].
    #[wasm_bindgen(js_name = lookAngleArcs)]
    pub fn look_angle_arcs(
        &self,
        station: &GroundStation,
        epochs_unix_us: &[i64],
    ) -> Vec<LookAngles> {
        let datetimes = instants(epochs_unix_us);
        let results = look_angle_batch_serial(&self.satellites, station.inner, &datetimes);
        results
            .into_iter()
            .map(|arc| match arc {
                Ok(looks) => LookAngles {
                    azimuth_deg: looks.iter().map(|l| l.azimuth_deg).collect(),
                    elevation_deg: looks.iter().map(|l| l.elevation_deg).collect(),
                    range_km: looks.iter().map(|l| l.range_km).collect(),
                },
                Err(_) => LookAngles {
                    azimuth_deg: Vec::new(),
                    elevation_deg: Vec::new(),
                    range_km: Vec::new(),
                },
            })
            .collect()
    }

    /// Per-satellite detailed outcomes for `lookAngleArcs`. Each result has
    /// `satelliteIndex`, `value` (the angle arrays or `null`), and `error` (the
    /// complete typed core error or `null`). The legacy method keeps its
    /// index-aligned empty-arc placeholders.
    #[wasm_bindgen(js_name = lookAngleArcOutcomes, unchecked_return_type = "FleetLookAngleOutcome[]")]
    pub fn look_angle_arc_outcomes(
        &self,
        station: &GroundStation,
        epochs_unix_us: &[i64],
    ) -> Result<js_sys::Array, JsValue> {
        let datetimes = instants(epochs_unix_us);
        let outcomes = js_sys::Array::new();
        for (satellite_index, result) in
            look_angle_batch_serial(&self.satellites, station.inner, &datetimes)
                .into_iter()
                .enumerate()
        {
            let outcome = js_sys::Object::new();
            js_sys::Reflect::set(
                &outcome,
                &JsValue::from_str("satelliteIndex"),
                &JsValue::from_f64(satellite_index as f64),
            )?;
            match result {
                Ok(looks) => {
                    let value = js_sys::Object::new();
                    let azimuth = js_sys::Float64Array::from(
                        looks
                            .iter()
                            .map(|l| l.azimuth_deg)
                            .collect::<Vec<_>>()
                            .as_slice(),
                    );
                    let elevation = js_sys::Float64Array::from(
                        looks
                            .iter()
                            .map(|l| l.elevation_deg)
                            .collect::<Vec<_>>()
                            .as_slice(),
                    );
                    let range = js_sys::Float64Array::from(
                        looks
                            .iter()
                            .map(|l| l.range_km)
                            .collect::<Vec<_>>()
                            .as_slice(),
                    );
                    js_sys::Reflect::set(
                        &value,
                        &JsValue::from_str("azimuthDeg"),
                        &azimuth.into(),
                    )?;
                    js_sys::Reflect::set(
                        &value,
                        &JsValue::from_str("elevationDeg"),
                        &elevation.into(),
                    )?;
                    js_sys::Reflect::set(&value, &JsValue::from_str("rangeKm"), &range.into())?;
                    js_sys::Reflect::set(&outcome, &JsValue::from_str("value"), &value)?;
                    js_sys::Reflect::set(&outcome, &JsValue::from_str("error"), &JsValue::NULL)?;
                }
                Err(error) => {
                    js_sys::Reflect::set(&outcome, &JsValue::from_str("value"), &JsValue::NULL)?;
                    js_sys::Reflect::set(
                        &outcome,
                        &JsValue::from_str("error"),
                        &look_angle_error(error),
                    )?;
                }
            }
            outcomes.push(&outcome);
        }
        Ok(outcomes)
    }

    /// Sub-satellite WGS84 ground tracks for every satellite over a shared epoch
    /// grid, in fleet order (element `i` is satellite `i`'s track), each reduced
    /// TEME->GCRS->ITRS->geodetic by the engine's validated transforms. A satellite
    /// that fails yields an empty track, keeping the result index-aligned. The
    /// batched form of [`Tle.groundTrack`].
    #[wasm_bindgen(js_name = groundTracks)]
    pub fn ground_tracks(&self, epochs_unix_us: &[i64]) -> Vec<GroundTrack> {
        let datetimes = instants(epochs_unix_us);
        self.satellites
            .iter()
            .map(|satellite| match ground_track(satellite, &datetimes) {
                Ok(points) => GroundTrack {
                    latitude_deg: points.iter().map(|g| g.lat_rad.to_degrees()).collect(),
                    longitude_deg: points.iter().map(|g| g.lon_rad.to_degrees()).collect(),
                    altitude_km: points.iter().map(|g| g.height_m / 1000.0).collect(),
                },
                Err(_) => GroundTrack {
                    latitude_deg: Vec::new(),
                    longitude_deg: Vec::new(),
                    altitude_km: Vec::new(),
                },
            })
            .collect()
    }

    /// Per-satellite detailed outcomes for `groundTracks`, retaining every
    /// indexed look-angle/frame/SGP4 failure while preserving the legacy empty
    /// track rows.
    #[wasm_bindgen(js_name = groundTrackOutcomes, unchecked_return_type = "FleetGroundTrackOutcome[]")]
    pub fn ground_track_outcomes(&self, epochs_unix_us: &[i64]) -> Result<js_sys::Array, JsValue> {
        let datetimes = instants(epochs_unix_us);
        let outcomes = js_sys::Array::new();
        for (satellite_index, satellite) in self.satellites.iter().enumerate() {
            let outcome = js_sys::Object::new();
            js_sys::Reflect::set(
                &outcome,
                &JsValue::from_str("satelliteIndex"),
                &JsValue::from_f64(satellite_index as f64),
            )?;
            match ground_track(satellite, &datetimes) {
                Ok(points) => {
                    let value = js_sys::Object::new();
                    let latitude = js_sys::Float64Array::from(
                        points
                            .iter()
                            .map(|g| g.lat_rad.to_degrees())
                            .collect::<Vec<_>>()
                            .as_slice(),
                    );
                    let longitude = js_sys::Float64Array::from(
                        points
                            .iter()
                            .map(|g| g.lon_rad.to_degrees())
                            .collect::<Vec<_>>()
                            .as_slice(),
                    );
                    let altitude = js_sys::Float64Array::from(
                        points
                            .iter()
                            .map(|g| g.height_m / 1000.0)
                            .collect::<Vec<_>>()
                            .as_slice(),
                    );
                    js_sys::Reflect::set(
                        &value,
                        &JsValue::from_str("latitudeDeg"),
                        &latitude.into(),
                    )?;
                    js_sys::Reflect::set(
                        &value,
                        &JsValue::from_str("longitudeDeg"),
                        &longitude.into(),
                    )?;
                    js_sys::Reflect::set(
                        &value,
                        &JsValue::from_str("altitudeKm"),
                        &altitude.into(),
                    )?;
                    js_sys::Reflect::set(&outcome, &JsValue::from_str("value"), &value)?;
                    js_sys::Reflect::set(&outcome, &JsValue::from_str("error"), &JsValue::NULL)?;
                }
                Err(error) => {
                    js_sys::Reflect::set(&outcome, &JsValue::from_str("value"), &JsValue::NULL)?;
                    js_sys::Reflect::set(
                        &outcome,
                        &JsValue::from_str("error"),
                        &look_angle_error(error),
                    )?;
                }
            }
            outcomes.push(&outcome);
        }
        Ok(outcomes)
    }

    /// Passes over `station` within `[startUnixUs, endUnixUs)` for every satellite,
    /// flattened across the constellation: each [`FleetPass`] carries the
    /// `satelliteIndex` (fleet-order) it belongs to. `elevationMaskDeg` defaults to
    /// 0, `stepSeconds` to 30, `timeToleranceS` to 1e-3. A satellite that fails to
    /// scan contributes no passes. Throws a `RangeError` on a non-positive step or
    /// an end at or before the start.
    #[wasm_bindgen]
    pub fn passes(
        &self,
        station: &GroundStation,
        start_unix_us: i64,
        end_unix_us: i64,
        elevation_mask_deg: Option<f64>,
        step_seconds: Option<f64>,
        time_tolerance_s: Option<f64>,
    ) -> Result<Vec<FleetPass>, JsValue> {
        let options = pass_options(elevation_mask_deg, step_seconds, time_tolerance_s)?;
        if end_unix_us <= start_unix_us {
            return Err(range_error("endUnixUs must be after startUnixUs"));
        }
        let start = UtcInstant::from_unix_microseconds(start_unix_us);
        let end = UtcInstant::from_unix_microseconds(end_unix_us);

        let mut out = Vec::new();
        for (index, satellite) in self.satellites.iter().enumerate() {
            let passes =
                match find_passes_for_satellite(satellite, station.inner, start, end, options) {
                    Ok(passes) => passes,
                    Err(_) => continue,
                };
            for pass in &passes {
                out.push(FleetPass {
                    satellite_index: index as u32,
                    pass: SatellitePass::from(pass),
                });
            }
        }
        Ok(out)
    }

    /// Per-satellite detailed outcomes for `passes`. Each row keeps its fleet
    /// index and either the satellite's pass list (possibly empty) or the full
    /// typed `PassError`; the flattened legacy pass list is unchanged.
    #[wasm_bindgen(js_name = passOutcomes, unchecked_return_type = "FleetPassOutcome[]")]
    pub fn pass_outcomes(
        &self,
        station: &GroundStation,
        start_unix_us: i64,
        end_unix_us: i64,
        elevation_mask_deg: Option<f64>,
        step_seconds: Option<f64>,
        time_tolerance_s: Option<f64>,
    ) -> Result<js_sys::Array, JsValue> {
        let options = pass_options(elevation_mask_deg, step_seconds, time_tolerance_s)?;
        if end_unix_us <= start_unix_us {
            return Err(range_error("endUnixUs must be after startUnixUs"));
        }
        let start = UtcInstant::from_unix_microseconds(start_unix_us);
        let end = UtcInstant::from_unix_microseconds(end_unix_us);
        let outcomes = js_sys::Array::new();
        for (satellite_index, satellite) in self.satellites.iter().enumerate() {
            let outcome = js_sys::Object::new();
            js_sys::Reflect::set(
                &outcome,
                &JsValue::from_str("satelliteIndex"),
                &JsValue::from_f64(satellite_index as f64),
            )?;
            match find_passes_for_satellite(satellite, station.inner, start, end, options) {
                Ok(passes) => {
                    let rows = js_sys::Array::new();
                    for pass in &passes {
                        let row = js_sys::Object::new();
                        js_sys::Reflect::set(
                            &row,
                            &JsValue::from_str("aosUnixUs"),
                            &js_sys::BigInt::from(pass.aos.unix_microseconds()).into(),
                        )?;
                        js_sys::Reflect::set(
                            &row,
                            &JsValue::from_str("losUnixUs"),
                            &js_sys::BigInt::from(pass.los.unix_microseconds()).into(),
                        )?;
                        js_sys::Reflect::set(
                            &row,
                            &JsValue::from_str("maxElevationDeg"),
                            &JsValue::from_f64(pass.max_elevation_deg),
                        )?;
                        js_sys::Reflect::set(
                            &row,
                            &JsValue::from_str("culminationUnixUs"),
                            &js_sys::BigInt::from(pass.culmination.unix_microseconds()).into(),
                        )?;
                        rows.push(&row);
                    }
                    js_sys::Reflect::set(&outcome, &JsValue::from_str("value"), &rows)?;
                    js_sys::Reflect::set(&outcome, &JsValue::from_str("error"), &JsValue::NULL)?;
                }
                Err(error) => {
                    js_sys::Reflect::set(&outcome, &JsValue::from_str("value"), &JsValue::NULL)?;
                    js_sys::Reflect::set(
                        &outcome,
                        &JsValue::from_str("error"),
                        &pass_error(error),
                    )?;
                }
            }
            outcomes.push(&outcome);
        }
        Ok(outcomes)
    }
}

/// One pass in a [`Constellation.passes`] result: the pass geometry plus the
/// fleet-order `satelliteIndex` of the satellite it belongs to (map that index to
/// your own per-satellite metadata).
#[wasm_bindgen]
pub struct FleetPass {
    satellite_index: u32,
    pass: SatellitePass,
}

#[wasm_bindgen]
impl FleetPass {
    /// Fleet-order index of the satellite this pass belongs to.
    #[wasm_bindgen(getter, js_name = satelliteIndex)]
    pub fn satellite_index(&self) -> u32 {
        self.satellite_index
    }

    /// AOS (acquisition of signal), unix microseconds.
    #[wasm_bindgen(getter, js_name = aosUnixUs)]
    pub fn aos_unix_us(&self) -> i64 {
        self.pass.aos_unix_us()
    }

    /// LOS (loss of signal), unix microseconds.
    #[wasm_bindgen(getter, js_name = losUnixUs)]
    pub fn los_unix_us(&self) -> i64 {
        self.pass.los_unix_us()
    }

    /// Culmination (peak elevation) time, unix microseconds.
    #[wasm_bindgen(getter, js_name = culminationUnixUs)]
    pub fn culmination_unix_us(&self) -> i64 {
        self.pass.culmination_unix_us()
    }

    /// Peak elevation during the pass, degrees.
    #[wasm_bindgen(getter, js_name = maxElevationDeg)]
    pub fn max_elevation_deg(&self) -> f64 {
        self.pass.max_elevation_deg()
    }
}

#[cfg(test)]
mod drift_tests {
    //! The pass-finder defaults track the core `PassFinderOptions::default()`
    //! rather than literals duplicated in this binding.
    use super::*;

    #[test]
    fn pass_options_defaults_track_core() {
        let got = pass_options(None, None, None).expect("default pass options are valid");
        let core = PassFinderOptions::default();
        assert_eq!(got.elevation_mask_deg, core.elevation_mask_deg);
        assert_eq!(got.coarse_step_seconds, core.coarse_step_seconds);
        assert_eq!(got.time_tolerance_seconds, core.time_tolerance_seconds);
    }
}

#[wasm_bindgen(typescript_custom_section)]
const FLEET_OUTCOME_TYPES: &'static str = r#"
export type FleetSgp4ErrorCause =
    | { kind: "invalidInput"; field: string; inputKind: string; reason: string }
    | { kind: "nonFiniteOutput"; field: string }
    | { kind: "invalidTle"; message: string }
    | { kind: "sgp4"; code: number }
    | { kind: "resonanceStepBudget"; budget: string };
export type FleetLookAngleErrorCause =
    | { kind: "invalidInput"; field: string; reason: string }
    | { kind: "init" | "propagate"; message: string; cause: FleetSgp4ErrorCause }
    | {
          kind: "frameTransform";
          message: string;
          cause:
              | { kind: "invalidInput"; field: string; reason: string }
              | { kind: "ut1OutsideCoverage"; reason: "beforeCoverage" | "afterCoverage" };
      };
export type FleetLookAngleError = Error & {
    detail: { family: "lookAngle"; cause: FleetLookAngleErrorCause };
};
export type FleetPassErrorCause =
    | { kind: "invalidInput"; field: string; reason: string }
    | { kind: "ut1OutsideCoverage"; reason: "beforeCoverage" | "afterCoverage" };
export type FleetPassError = Error & {
    detail: { family: "pass"; cause: FleetPassErrorCause };
};
export type FleetLookAngleArcValue = {
    azimuthDeg: Float64Array;
    elevationDeg: Float64Array;
    rangeKm: Float64Array;
};
export type FleetGroundTrackValue = {
    latitudeDeg: Float64Array;
    longitudeDeg: Float64Array;
    altitudeKm: Float64Array;
};
export type FleetPassOutcomeValue = {
    aosUnixUs: bigint;
    losUnixUs: bigint;
    maxElevationDeg: number;
    culminationUnixUs: bigint;
};
export type FleetLookAngleOutcome = {
    satelliteIndex: number;
    value: FleetLookAngleArcValue | null;
    error: FleetLookAngleError | null;
};
export type FleetGroundTrackOutcome = {
    satelliteIndex: number;
    value: FleetGroundTrackValue | null;
    error: FleetLookAngleError | null;
};
export type FleetPassOutcome = {
    satelliteIndex: number;
    value: FleetPassOutcomeValue[] | null;
    error: FleetPassError | null;
};
"#;
