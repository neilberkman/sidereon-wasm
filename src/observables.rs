//! Observation-domain math: carrier-frequency lookups, linear combinations,
//! cycle-slip detection, Hatch smoothing, measurement weighting, receiver
//! velocity, and GPS C/A signal generation. Every value delegates to
//! `sidereon-core`; this module only marshals JS input and output.

use std::str::FromStr;

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use sidereon_core::carrier_phase::{
    self, ArcEpoch, CycleSlipOptions as CoreCycleSlipOptions, IonoFreeSmoothResult as CoreIfSmooth,
    SlipReason as CoreSlipReason, SlipResult as CoreSlipResult, SmoothCodeResult as CoreSmoothCode,
    DEFAULT_HATCH_WINDOW_CAP,
};
use sidereon_core::combinations::{self, PseudorangeDropReason as CoreDropReason};
use sidereon_core::frequencies::{
    default_iono_free_pair, default_spp_frequency_hz, frequency_hz, glonass_g1_frequency_hz,
    rinex_band_frequency_hz, rinex_band_wavelength_m, wavelength_m, CarrierPair as CoreCarrierPair,
};
use sidereon_core::observables::{
    predict as core_predict, predict_batch as core_predict_batch, ObservableEphemerisSource,
    PredictOptions as CorePredictOptions, PredictRequest, PredictedObservables as CorePredicted,
};
use sidereon_core::quality::{
    self, PseudorangeVarianceModel as CoreVarModel, PseudorangeVarianceOptions as CoreVarOptions,
    RaimWeights as CoreRaimWeights, WeightEntry as CoreWeightEntry,
};
use sidereon_core::signal::{
    self, AcquisitionGrid as CoreAcqGrid, AcquisitionOptions as CoreAcqOptions,
    AcquisitionResult as CoreAcqResult, CorrelateOptions as CoreCorrelateOptions,
    CorrelationResult as CoreCorrelationResult, IqSample, ReplicaOptions as CoreReplicaOptions,
};
use sidereon_core::velocity::{
    self, VelocityObservable as CoreVelObservable, VelocityObservation as CoreVelObs,
    VelocitySolution as CoreVelSolution, VelocitySolveOptions as CoreVelOptions,
};
use sidereon_core::GnssSatelliteId;

use crate::error::{engine_error, range_error, type_error};
use crate::gnss::{CarrierBand, GnssSystem};
use crate::rinex_nav::BroadcastEphemeris;
use crate::sp3::Sp3;

// ---- error mapping -----------------------------------------------------------

fn domain_error<E: core::fmt::Display>(err: E) -> JsValue {
    range_error(&err.to_string())
}

fn parse_sat(token: &str) -> Result<GnssSatelliteId, JsValue> {
    GnssSatelliteId::from_str(token)
        .map_err(|e| type_error(&format!("invalid satellite token {token:?}: {e}")))
}

fn one_char(value: &str, message: &str) -> Result<char, JsValue> {
    let mut chars = value.chars();
    match (chars.next(), chars.next()) {
        (Some(ch), None) => Ok(ch),
        _ => Err(type_error(message)),
    }
}

// ---- enums -------------------------------------------------------------------

/// Reason a satellite was dropped from paired pseudorange combination.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PseudorangeDropReason {
    /// Present in band 2 only.
    MissingBand1,
    /// Present in band 1 only.
    MissingBand2,
    /// Repeated within at least one input band.
    DuplicateObservation,
    /// Unsupported constellation or override band.
    UnknownSystem,
}

impl From<CoreDropReason> for PseudorangeDropReason {
    fn from(reason: CoreDropReason) -> Self {
        match reason {
            CoreDropReason::MissingBand1 => Self::MissingBand1,
            CoreDropReason::MissingBand2 => Self::MissingBand2,
            CoreDropReason::DuplicateObservation => Self::DuplicateObservation,
            CoreDropReason::UnknownSystem => Self::UnknownSystem,
        }
    }
}

/// Stable lower-case label for a pseudorange drop reason.
#[wasm_bindgen(js_name = pseudorangeDropReasonLabel)]
pub fn pseudorange_drop_reason_label(reason: PseudorangeDropReason) -> String {
    match reason {
        PseudorangeDropReason::MissingBand1 => "missing_band1",
        PseudorangeDropReason::MissingBand2 => "missing_band2",
        PseudorangeDropReason::DuplicateObservation => "duplicate_observation",
        PseudorangeDropReason::UnknownSystem => "unknown_system",
    }
    .to_string()
}

/// Reason a carrier-phase arc split was flagged.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SlipReason {
    /// Loss-of-lock indicator bit 0 set on either band.
    Lli,
    /// Gap to the previous usable sample exceeded the threshold.
    DataGap,
    /// Geometry-free phase step exceeded the threshold.
    GeometryFree,
    /// Melbourne-Wubbena step exceeded the threshold.
    MelbourneWubbena,
}

impl From<CoreSlipReason> for SlipReason {
    fn from(reason: CoreSlipReason) -> Self {
        match reason {
            CoreSlipReason::Lli => Self::Lli,
            CoreSlipReason::DataGap => Self::DataGap,
            CoreSlipReason::GeometryFree => Self::GeometryFree,
            CoreSlipReason::MelbourneWubbena => Self::MelbourneWubbena,
        }
    }
}

/// Stable lower-case label for a slip reason.
#[wasm_bindgen(js_name = slipReasonLabel)]
pub fn slip_reason_label(reason: SlipReason) -> String {
    match reason {
        SlipReason::Lli => "lli",
        SlipReason::DataGap => "data_gap",
        SlipReason::GeometryFree => "geometry_free",
        SlipReason::MelbourneWubbena => "melbourne_wubbena",
    }
    .to_string()
}

// ---- carrier pair ------------------------------------------------------------

/// A standard two-carrier ionosphere-free pair.
#[wasm_bindgen]
pub struct CarrierPair {
    inner: CoreCarrierPair,
}

#[wasm_bindgen]
impl CarrierPair {
    /// Create a two-carrier pair.
    #[wasm_bindgen(constructor)]
    pub fn new(band1: CarrierBand, band2: CarrierBand) -> CarrierPair {
        CarrierPair {
            inner: CoreCarrierPair::new(band1.into(), band2.into()),
        }
    }

    /// First carrier band.
    #[wasm_bindgen(getter)]
    pub fn band1(&self) -> CarrierBand {
        self.inner.band1.into()
    }

    /// Second carrier band.
    #[wasm_bindgen(getter)]
    pub fn band2(&self) -> CarrierBand {
        self.inner.band2.into()
    }
}

// ---- frequency lookups -------------------------------------------------------

/// Carrier frequency in hertz for a constellation and canonical carrier band.
#[wasm_bindgen(js_name = carrierFrequencyHz)]
pub fn carrier_frequency_hz(system: GnssSystem, band: CarrierBand) -> Option<f64> {
    frequency_hz(system.into(), band.into())
}

/// Carrier wavelength in metres for a constellation and canonical carrier band.
#[wasm_bindgen(js_name = wavelengthM)]
pub fn wavelength_m_js(system: GnssSystem, band: CarrierBand) -> Option<f64> {
    wavelength_m(system.into(), band.into())
}

/// RINEX observation band frequency in hertz for a system and band digit.
#[wasm_bindgen(js_name = rinexBandFrequencyHz)]
pub fn rinex_band_frequency_hz_js(
    system: GnssSystem,
    band: &str,
    glonass_channel: Option<i8>,
) -> Result<Option<f64>, JsValue> {
    let band = one_char(
        band,
        "band must be a single RINEX observation band character",
    )?;
    Ok(rinex_band_frequency_hz(
        system.into(),
        band,
        glonass_channel,
    ))
}

/// RINEX observation band wavelength in metres for a system and band digit.
#[wasm_bindgen(js_name = rinexBandWavelengthM)]
pub fn rinex_band_wavelength_m_js(
    system: GnssSystem,
    band: &str,
    glonass_channel: Option<i8>,
) -> Result<Option<f64>, JsValue> {
    let band = one_char(
        band,
        "band must be a single RINEX observation band character",
    )?;
    Ok(rinex_band_wavelength_m(
        system.into(),
        band,
        glonass_channel,
    ))
}

/// GLONASS G1 FDMA carrier frequency in hertz for channel `k`.
#[wasm_bindgen(js_name = glonassG1FrequencyHz)]
pub fn glonass_g1_frequency_hz_js(channel: i8) -> f64 {
    glonass_g1_frequency_hz(channel)
}

/// Single-frequency carrier used by the SPP ionosphere-scaling policy.
#[wasm_bindgen(js_name = defaultSppFrequencyHz)]
pub fn default_spp_frequency_hz_js(system: GnssSystem) -> Option<f64> {
    default_spp_frequency_hz(system.into())
}

/// Standard dual-frequency ionosphere-free carrier pair for a constellation.
#[wasm_bindgen(js_name = defaultPair)]
pub fn default_pair(system: GnssSystem) -> Option<CarrierPair> {
    default_iono_free_pair(system.into()).map(|inner| CarrierPair { inner })
}

// ---- linear combinations -----------------------------------------------------

/// Ionosphere-free coefficient `gamma = f1^2 / (f1^2 - f2^2)`.
#[wasm_bindgen]
pub fn gamma(f1_hz: f64, f2_hz: f64) -> Result<f64, JsValue> {
    combinations::gamma(f1_hz, f2_hz).map_err(domain_error)
}

/// Equal-variance noise amplification of the ionosphere-free combination.
#[wasm_bindgen(js_name = noiseAmplification)]
pub fn noise_amplification(f1_hz: f64, f2_hz: f64) -> Result<f64, JsValue> {
    combinations::noise_amplification(f1_hz, f2_hz).map_err(domain_error)
}

/// Ionosphere-free code or meter-valued phase combination, metres.
#[wasm_bindgen(js_name = ionosphereFree)]
pub fn ionosphere_free(obs1_m: f64, obs2_m: f64, f1_hz: f64, f2_hz: f64) -> Result<f64, JsValue> {
    combinations::ionosphere_free(obs1_m, obs2_m, f1_hz, f2_hz).map_err(domain_error)
}

/// Ionosphere-free carrier-phase combination from meter-valued phase inputs.
#[wasm_bindgen(js_name = ionosphereFreePhaseM)]
pub fn ionosphere_free_phase_m(
    phase1_m: f64,
    phase2_m: f64,
    f1_hz: f64,
    f2_hz: f64,
) -> Result<f64, JsValue> {
    combinations::ionosphere_free_phase_m(phase1_m, phase2_m, f1_hz, f2_hz).map_err(domain_error)
}

/// Ionosphere-free carrier-phase combination from cycle-valued phase inputs.
#[wasm_bindgen(js_name = ionosphereFreePhaseCycles)]
pub fn ionosphere_free_phase_cycles(
    phi1_cycles: f64,
    phi2_cycles: f64,
    f1_hz: f64,
    f2_hz: f64,
) -> Result<f64, JsValue> {
    combinations::ionosphere_free_phase_cycles(phi1_cycles, phi2_cycles, f1_hz, f2_hz)
        .map_err(domain_error)
}

/// Carrier phase converted to metres, `L = c / f * phi`.
#[wasm_bindgen(js_name = phaseMeters)]
pub fn phase_meters(phi_cycles: f64, f_hz: f64) -> Result<f64, JsValue> {
    carrier_phase::phase_meters(phi_cycles, f_hz).map_err(domain_error)
}

/// Geometry-free phase combination `L_GF = L1 - L2`, metres.
#[wasm_bindgen(js_name = geometryFree)]
pub fn geometry_free(l1_m: f64, l2_m: f64) -> Result<f64, JsValue> {
    carrier_phase::geometry_free(l1_m, l2_m).map_err(domain_error)
}

/// Wide-lane wavelength `c / (f1 - f2)`, metres.
#[wasm_bindgen(js_name = wideLaneWavelength)]
pub fn wide_lane_wavelength(f1_hz: f64, f2_hz: f64) -> Result<f64, JsValue> {
    carrier_phase::wide_lane_wavelength(f1_hz, f2_hz).map_err(domain_error)
}

/// Narrow-lane code combination, metres.
#[wasm_bindgen(js_name = narrowLaneCode)]
pub fn narrow_lane_code(p1_m: f64, p2_m: f64, f1_hz: f64, f2_hz: f64) -> Result<f64, JsValue> {
    carrier_phase::narrow_lane_code(p1_m, p2_m, f1_hz, f2_hz).map_err(domain_error)
}

/// Melbourne-Wubbena combination, metres.
#[wasm_bindgen(js_name = melbourneWubbena)]
pub fn melbourne_wubbena(
    phi1_cycles: f64,
    phi2_cycles: f64,
    p1_m: f64,
    p2_m: f64,
    f1_hz: f64,
    f2_hz: f64,
) -> Result<f64, JsValue> {
    carrier_phase::melbourne_wubbena(phi1_cycles, phi2_cycles, p1_m, p2_m, f1_hz, f2_hz)
        .map_err(domain_error)
}

/// Melbourne-Wubbena wide-lane ambiguity estimate, wide-lane cycles.
#[wasm_bindgen(js_name = wideLaneCycles)]
pub fn wide_lane_cycles(
    phi1_cycles: f64,
    phi2_cycles: f64,
    p1_m: f64,
    p2_m: f64,
    f1_hz: f64,
    f2_hz: f64,
) -> Result<f64, JsValue> {
    carrier_phase::wide_lane_cycles(phi1_cycles, phi2_cycles, p1_m, p2_m, f1_hz, f2_hz)
        .map_err(domain_error)
}

/// Result of combining two pseudorange bands into ionosphere-free ranges.
#[wasm_bindgen]
pub struct IonoFreePseudorangeResult {
    combined_sats: Vec<String>,
    combined_m: Vec<f64>,
    dropped_sats: Vec<String>,
    dropped_reasons: Vec<PseudorangeDropReason>,
}

#[wasm_bindgen]
impl IonoFreePseudorangeResult {
    /// Satellite tokens with a combined ionosphere-free range, sorted.
    #[wasm_bindgen(getter, js_name = combinedSats)]
    pub fn combined_sats(&self) -> Vec<String> {
        self.combined_sats.clone()
    }

    /// Combined ionosphere-free ranges, metres, row-aligned with `combinedSats`.
    #[wasm_bindgen(getter, js_name = combinedM)]
    pub fn combined_m(&self) -> Vec<f64> {
        self.combined_m.clone()
    }

    /// Satellite tokens dropped from the combination, sorted.
    #[wasm_bindgen(getter, js_name = droppedSats)]
    pub fn dropped_sats(&self) -> Vec<String> {
        self.dropped_sats.clone()
    }

    /// Drop reasons row-aligned with `droppedSats`.
    #[wasm_bindgen(getter, js_name = droppedReasons)]
    pub fn dropped_reasons(&self) -> Vec<PseudorangeDropReason> {
        self.dropped_reasons.clone()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PseudorangeObsInput {
    satellite_id: String,
    value_m: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PseudorangeOverrideInput {
    system: String,
    band1: String,
    band2: String,
}

/// Combine two satellite-keyed pseudorange bands into ionosphere-free ranges.
/// `band1` and `band2` are arrays of `{ satelliteId, valueM }`. `overrides` is
/// an optional array of `{ system, band1, band2 }` RINEX band selections.
#[wasm_bindgen(js_name = ionosphereFreePseudoranges)]
pub fn ionosphere_free_pseudoranges(
    band1: JsValue,
    band2: JsValue,
    overrides: JsValue,
) -> Result<IonoFreePseudorangeResult, JsValue> {
    let band1: Vec<PseudorangeObsInput> = serde_wasm_bindgen::from_value(band1)
        .map_err(|e| type_error(&format!("invalid band1: {e}")))?;
    let band2: Vec<PseudorangeObsInput> = serde_wasm_bindgen::from_value(band2)
        .map_err(|e| type_error(&format!("invalid band2: {e}")))?;
    let overrides: Vec<PseudorangeOverrideInput> =
        if overrides.is_undefined() || overrides.is_null() {
            Vec::new()
        } else {
            serde_wasm_bindgen::from_value(overrides)
                .map_err(|e| type_error(&format!("invalid overrides: {e}")))?
        };

    let band1: Vec<(String, f64)> = band1
        .into_iter()
        .map(|o| (o.satellite_id, o.value_m))
        .collect();
    let band2: Vec<(String, f64)> = band2
        .into_iter()
        .map(|o| (o.satellite_id, o.value_m))
        .collect();
    let overrides: Vec<(char, String, String)> = overrides
        .into_iter()
        .map(|o| {
            let system = one_char(
                &o.system,
                "override system must be a single RINEX system character",
            )?;
            Ok((system, o.band1, o.band2))
        })
        .collect::<Result<Vec<_>, JsValue>>()?;

    let (combined, dropped) =
        combinations::ionosphere_free_pseudoranges(&band1, &band2, &overrides)
            .map_err(domain_error)?;

    let mut combined_sats = Vec::with_capacity(combined.len());
    let mut combined_m = Vec::with_capacity(combined.len());
    for (sat, value) in combined {
        combined_sats.push(sat);
        combined_m.push(value);
    }
    let mut dropped_sats = Vec::with_capacity(dropped.len());
    let mut dropped_reasons = Vec::with_capacity(dropped.len());
    for (sat, reason) in dropped {
        dropped_sats.push(sat);
        dropped_reasons.push(reason.into());
    }

    Ok(IonoFreePseudorangeResult {
        combined_sats,
        combined_m,
        dropped_sats,
        dropped_reasons,
    })
}

// ---- quality / weighting -----------------------------------------------------

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct PseudorangeVarianceOptionsInput {
    a_m: Option<f64>,
    b_m: Option<f64>,
    model: Option<String>,
    cn0_dbhz: Option<f64>,
    cn0_scale_m2: Option<f64>,
}

impl PseudorangeVarianceOptionsInput {
    fn to_core(&self) -> Result<CoreVarOptions, JsValue> {
        let model = match self.model.as_deref() {
            None | Some("elevation") => CoreVarModel::Elevation,
            Some("elevation_cn0") => CoreVarModel::ElevationCn0,
            Some(other) => {
                return Err(type_error(&format!(
                    "invalid variance model {other:?}: expected \"elevation\" or \"elevation_cn0\""
                )))
            }
        };
        let core_default = CoreVarOptions::default();
        Ok(CoreVarOptions {
            a_m: self.a_m.unwrap_or(core_default.a_m),
            b_m: self.b_m.unwrap_or(core_default.b_m),
            model,
            cn0_dbhz: self.cn0_dbhz,
            cn0_scale_m2: self.cn0_scale_m2.unwrap_or(core_default.cn0_scale_m2),
        })
    }
}

fn var_options(options: JsValue) -> Result<CoreVarOptions, JsValue> {
    let input: PseudorangeVarianceOptionsInput = if options.is_undefined() || options.is_null() {
        PseudorangeVarianceOptionsInput::default()
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid variance options: {e}")))?
    };
    input.to_core()
}

/// Pseudorange measurement variance, square metres.
#[wasm_bindgen(js_name = pseudorangeVariance)]
pub fn pseudorange_variance(elevation_deg: f64, options: JsValue) -> Result<f64, JsValue> {
    let options = var_options(options)?;
    quality::pseudorange_variance(elevation_deg, options).map_err(domain_error)
}

/// Satellite-keyed measurement sigmas (metres) or inverse-variance weights.
#[wasm_bindgen]
pub struct SatelliteVector {
    satellite_ids: Vec<String>,
    values: Vec<f64>,
}

#[wasm_bindgen]
impl SatelliteVector {
    /// Satellite tokens, sorted, row-aligned with `values`.
    #[wasm_bindgen(getter, js_name = satelliteIds)]
    pub fn satellite_ids(&self) -> Vec<String> {
        self.satellite_ids.clone()
    }

    /// Per-satellite values as a `Float64Array`.
    #[wasm_bindgen(getter)]
    pub fn values(&self) -> Vec<f64> {
        self.values.clone()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WeightEntryInput {
    satellite_id: String,
    elevation_deg: f64,
    #[serde(default)]
    cn0_dbhz: Option<f64>,
}

fn weight_entries(entries: JsValue) -> Result<Vec<CoreWeightEntry>, JsValue> {
    let entries: Vec<WeightEntryInput> = serde_wasm_bindgen::from_value(entries)
        .map_err(|e| type_error(&format!("invalid weight entries: {e}")))?;
    Ok(entries
        .into_iter()
        .map(|e| CoreWeightEntry {
            satellite_id: e.satellite_id,
            elevation_deg: e.elevation_deg,
            cn0_dbhz: e.cn0_dbhz,
        })
        .collect())
}

fn map_to_vector(map: std::collections::BTreeMap<String, f64>) -> SatelliteVector {
    let (satellite_ids, values) = map.into_iter().unzip();
    SatelliteVector {
        satellite_ids,
        values,
    }
}

/// Build satellite-keyed pseudorange sigmas in metres from `{ satelliteId,
/// elevationDeg, cn0Dbhz? }` entries. Invalid entries are dropped by the core.
#[wasm_bindgen]
pub fn sigmas(entries: JsValue, options: JsValue) -> Result<SatelliteVector, JsValue> {
    let entries = weight_entries(entries)?;
    let options = var_options(options)?;
    Ok(map_to_vector(quality::sigmas(&entries, options)))
}

/// Build satellite-keyed inverse-variance pseudorange weights.
#[wasm_bindgen(js_name = weightVector)]
pub fn weight_vector(entries: JsValue, options: JsValue) -> Result<SatelliteVector, JsValue> {
    let entries = weight_entries(entries)?;
    let options = var_options(options)?;
    Ok(map_to_vector(quality::weight_vector(&entries, options)))
}

/// RAIM weighting mode: either unit weights or per-satellite inverse-variance
/// weights (missing satellites default to unit weight).
#[wasm_bindgen]
pub struct RaimWeights {
    inner: CoreRaimWeights,
}

#[wasm_bindgen]
impl RaimWeights {
    /// Unit weights, equivalent to sigma = 1 m for every satellite.
    #[wasm_bindgen(js_name = unit)]
    pub fn unit() -> RaimWeights {
        RaimWeights {
            inner: CoreRaimWeights::Unit,
        }
    }

    /// Per-satellite inverse-variance weights. `weights` must be positive and
    /// finite and the same length as `satelliteIds`. Throws a `TypeError` on a
    /// length mismatch and a `RangeError` on a non-positive weight.
    #[wasm_bindgen(js_name = bySatellite)]
    pub fn by_satellite(
        satellite_ids: Vec<String>,
        weights: &[f64],
    ) -> Result<RaimWeights, JsValue> {
        if satellite_ids.len() != weights.len() {
            return Err(type_error(
                "satelliteIds and weights must have the same length",
            ));
        }
        let mut map = std::collections::BTreeMap::new();
        for (id, &weight) in satellite_ids.into_iter().zip(weights.iter()) {
            if !weight.is_finite() || weight <= 0.0 {
                return Err(range_error("RAIM weights must be positive finite values"));
            }
            map.insert(id, weight);
        }
        Ok(RaimWeights {
            inner: CoreRaimWeights::BySatellite(map),
        })
    }

    /// True when all satellites use unit weight.
    #[wasm_bindgen(getter, js_name = isUnit)]
    pub fn is_unit(&self) -> bool {
        matches!(self.inner, CoreRaimWeights::Unit)
    }

    /// Satellite tokens for per-satellite weights, sorted by token.
    #[wasm_bindgen(getter, js_name = satelliteIds)]
    pub fn satellite_ids(&self) -> Vec<String> {
        match &self.inner {
            CoreRaimWeights::Unit => Vec::new(),
            CoreRaimWeights::BySatellite(weights) => weights.keys().cloned().collect(),
        }
    }

    /// Inverse-variance weights as a `Float64Array`, sorted by satellite token.
    #[wasm_bindgen(getter)]
    pub fn weights(&self) -> Vec<f64> {
        match &self.inner {
            CoreRaimWeights::Unit => Vec::new(),
            CoreRaimWeights::BySatellite(weights) => weights.values().copied().collect(),
        }
    }
}

// ---- carrier-phase arc processing -------------------------------------------

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ArcEpochInput {
    phi1_cycles: Option<f64>,
    phi2_cycles: Option<f64>,
    p1_m: Option<f64>,
    p2_m: Option<f64>,
    lli1: Option<i64>,
    lli2: Option<i64>,
    f1_hz: Option<f64>,
    f2_hz: Option<f64>,
    gap_time_s: Option<f64>,
}

impl ArcEpochInput {
    fn into_core(self) -> ArcEpoch {
        ArcEpoch {
            phi1_cycles: self.phi1_cycles,
            phi2_cycles: self.phi2_cycles,
            p1_m: self.p1_m,
            p2_m: self.p2_m,
            lli1: self.lli1,
            lli2: self.lli2,
            f1_hz: self.f1_hz,
            f2_hz: self.f2_hz,
            gap_time_s: self.gap_time_s,
        }
    }
}

fn arc_from_js(arc: JsValue) -> Result<Vec<ArcEpoch>, JsValue> {
    let arc: Vec<ArcEpochInput> = serde_wasm_bindgen::from_value(arc)
        .map_err(|e| type_error(&format!("invalid arc: {e}")))?;
    Ok(arc.into_iter().map(ArcEpochInput::into_core).collect())
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct CycleSlipOptionsInput {
    gf_threshold_m: Option<f64>,
    mw_threshold_cycles: Option<f64>,
    min_arc_gap_s: Option<f64>,
}

fn cycle_slip_options(options: JsValue) -> Result<CoreCycleSlipOptions, JsValue> {
    let input: CycleSlipOptionsInput = if options.is_undefined() || options.is_null() {
        CycleSlipOptionsInput::default()
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid cycle-slip options: {e}")))?
    };
    let core_default = CoreCycleSlipOptions::default();
    Ok(CoreCycleSlipOptions {
        gf_threshold_m: input.gf_threshold_m.unwrap_or(core_default.gf_threshold_m),
        mw_threshold_cycles: input
            .mw_threshold_cycles
            .unwrap_or(core_default.mw_threshold_cycles),
        min_arc_gap_s: input.min_arc_gap_s.unwrap_or(core_default.min_arc_gap_s),
    })
}

/// Cycle-slip classification for one input epoch.
#[wasm_bindgen]
pub struct SlipResult {
    inner: CoreSlipResult,
}

#[wasm_bindgen]
impl SlipResult {
    /// Whether any slip reason was flagged.
    #[wasm_bindgen(getter)]
    pub fn slip(&self) -> bool {
        self.inner.slip
    }

    /// Slip reasons in deterministic API order.
    #[wasm_bindgen(getter)]
    pub fn reasons(&self) -> Vec<SlipReason> {
        self.inner.reasons.iter().copied().map(Into::into).collect()
    }

    /// Current geometry-free phase, metres, or `undefined`.
    #[wasm_bindgen(getter, js_name = gfM)]
    pub fn gf_m(&self) -> Option<f64> {
        self.inner.gf_m
    }

    /// Current Melbourne-Wubbena combination, metres, or `undefined`.
    #[wasm_bindgen(getter, js_name = mwM)]
    pub fn mw_m(&self) -> Option<f64> {
        self.inner.mw_m
    }

    /// Whether the epoch was skipped because a frequency was unavailable.
    #[wasm_bindgen(getter)]
    pub fn skipped(&self) -> bool {
        self.inner.skipped
    }
}

/// Hatch-smoothed single-frequency code output for one epoch.
#[wasm_bindgen]
pub struct SmoothCodeResult {
    inner: CoreSmoothCode,
}

#[wasm_bindgen]
impl SmoothCodeResult {
    /// Smoothed code pseudorange, metres, or `undefined`.
    #[wasm_bindgen(getter, js_name = pSmoothM)]
    pub fn p_smooth_m(&self) -> Option<f64> {
        self.inner.p_smooth_m
    }

    /// Hatch window length used at this epoch.
    #[wasm_bindgen(getter)]
    pub fn window(&self) -> usize {
        self.inner.window
    }

    /// True when a prior running window was reset by a slip.
    #[wasm_bindgen(getter)]
    pub fn reset(&self) -> bool {
        self.inner.reset
    }
}

/// Hatch-smoothed ionosphere-free code output for one epoch.
#[wasm_bindgen]
pub struct IonoFreeSmoothResult {
    inner: CoreIfSmooth,
}

#[wasm_bindgen]
impl IonoFreeSmoothResult {
    /// Smoothed ionosphere-free code pseudorange, metres, or `undefined`.
    #[wasm_bindgen(getter, js_name = pSmoothM)]
    pub fn p_smooth_m(&self) -> Option<f64> {
        self.inner.p_smooth_m
    }

    /// Instantaneous ionosphere-free code, metres, or `undefined`.
    #[wasm_bindgen(getter, js_name = pIfM)]
    pub fn p_if_m(&self) -> Option<f64> {
        self.inner.p_if_m
    }

    /// Instantaneous ionosphere-free carrier phase, metres, or `undefined`.
    #[wasm_bindgen(getter, js_name = lIfM)]
    pub fn l_if_m(&self) -> Option<f64> {
        self.inner.l_if_m
    }

    /// Hatch window length used at this epoch.
    #[wasm_bindgen(getter)]
    pub fn window(&self) -> usize {
        self.inner.window
    }

    /// True when a prior running window was reset by a slip.
    #[wasm_bindgen(getter)]
    pub fn reset(&self) -> bool {
        self.inner.reset
    }
}

/// Detect cycle slips on a time-ordered single-satellite carrier-phase arc.
/// `arc` is an array of `{ phi1Cycles?, phi2Cycles?, p1M?, p2M?, lli1?, lli2?,
/// f1Hz?, f2Hz?, gapTimeS? }`.
#[wasm_bindgen(js_name = detectCycleSlips)]
pub fn detect_cycle_slips(arc: JsValue, options: JsValue) -> Result<Vec<SlipResult>, JsValue> {
    let arc = arc_from_js(arc)?;
    let options = cycle_slip_options(options)?;
    Ok(carrier_phase::detect_cycle_slips(&arc, options)
        .map_err(domain_error)?
        .into_iter()
        .map(|inner| SlipResult { inner })
        .collect())
}

/// Single-frequency Hatch carrier-smoothed code on band 1.
#[wasm_bindgen(js_name = smoothCode)]
pub fn smooth_code(
    arc: JsValue,
    options: JsValue,
    hatch_window_cap: Option<usize>,
) -> Result<Vec<SmoothCodeResult>, JsValue> {
    let arc = arc_from_js(arc)?;
    let options = cycle_slip_options(options)?;
    Ok(carrier_phase::smooth_code(
        &arc,
        options,
        hatch_window_cap.unwrap_or(DEFAULT_HATCH_WINDOW_CAP),
    )
    .map_err(domain_error)?
    .into_iter()
    .map(|inner| SmoothCodeResult { inner })
    .collect())
}

/// Dual-frequency ionosphere-free Hatch carrier-smoothed code.
#[wasm_bindgen(js_name = smoothIonoFreeCode)]
pub fn smooth_iono_free_code(
    arc: JsValue,
    options: JsValue,
    hatch_window_cap: Option<usize>,
) -> Result<Vec<IonoFreeSmoothResult>, JsValue> {
    let arc = arc_from_js(arc)?;
    let options = cycle_slip_options(options)?;
    Ok(carrier_phase::smooth_iono_free_code(
        &arc,
        options,
        hatch_window_cap.unwrap_or(DEFAULT_HATCH_WINDOW_CAP),
    )
    .map_err(domain_error)?
    .into_iter()
    .map(|inner| IonoFreeSmoothResult { inner })
    .collect())
}

// ---- velocity ----------------------------------------------------------------

/// Convert a Doppler shift in hertz to pseudorange rate in metres per second.
#[wasm_bindgen(js_name = dopplerToRangeRate)]
pub fn doppler_to_range_rate(doppler_hz: f64, carrier_hz: f64) -> Result<f64, JsValue> {
    velocity::doppler_to_range_rate(doppler_hz, carrier_hz).map_err(domain_error)
}

/// Convert a pseudorange rate in metres per second to Doppler shift in hertz.
#[wasm_bindgen(js_name = rangeRateToDoppler)]
pub fn range_rate_to_doppler(range_rate_m_s: f64, carrier_hz: f64) -> Result<f64, JsValue> {
    velocity::range_rate_to_doppler(range_rate_m_s, carrier_hz).map_err(domain_error)
}

/// Receiver velocity solve result.
#[wasm_bindgen]
pub struct VelocitySolution {
    inner: CoreVelSolution,
}

#[wasm_bindgen]
impl VelocitySolution {
    /// Receiver ECEF velocity `[vx, vy, vz]`, metres per second.
    #[wasm_bindgen(getter, js_name = velocityMS)]
    pub fn velocity_m_s(&self) -> Vec<f64> {
        self.inner.velocity_m_s.to_vec()
    }

    /// Receiver speed, metres per second.
    #[wasm_bindgen(getter, js_name = speedMS)]
    pub fn speed_m_s(&self) -> f64 {
        self.inner.speed_m_s
    }

    /// Receiver clock drift, seconds per second.
    #[wasm_bindgen(getter, js_name = clockDriftSS)]
    pub fn clock_drift_s_s(&self) -> f64 {
        self.inner.clock_drift_s_s
    }

    /// Satellite tokens contributing rows, in residual order.
    #[wasm_bindgen(getter, js_name = usedSats)]
    pub fn used_sats(&self) -> Vec<String> {
        self.inner
            .used_sats
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    /// Post-fit range-rate residuals, metres per second.
    #[wasm_bindgen(getter, js_name = residualsMS)]
    pub fn residuals_m_s(&self) -> Vec<f64> {
        self.inner.residuals_m_s.iter().map(|(_, r)| *r).collect()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VelocityObsInput {
    satellite_id: String,
    value: f64,
    carrier_hz: f64,
    #[serde(default)]
    sat_clock_drift_s_s: f64,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct VelocitySolveOptionsInput {
    observable: Option<String>,
    light_time: Option<bool>,
    sagnac: Option<bool>,
}

fn velocity_options(options: JsValue) -> Result<CoreVelOptions, JsValue> {
    let input: VelocitySolveOptionsInput = if options.is_undefined() || options.is_null() {
        VelocitySolveOptionsInput::default()
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid velocity options: {e}")))?
    };
    let core_default = CoreVelOptions::default();
    let observable = match input.observable.as_deref() {
        None => core_default.observable,
        Some("range_rate") => CoreVelObservable::RangeRate,
        Some("doppler") => CoreVelObservable::Doppler,
        Some(other) => {
            return Err(type_error(&format!(
                "invalid observable {other:?}: expected \"range_rate\" or \"doppler\""
            )))
        }
    };
    Ok(CoreVelOptions {
        observable,
        light_time: input.light_time.unwrap_or(core_default.light_time),
        sagnac: input.sagnac.unwrap_or(core_default.sagnac),
    })
}

/// Solve receiver ECEF velocity and clock drift from one epoch of observations.
/// `observations` is an array of `{ satelliteId, value, carrierHz,
/// satClockDriftSS? }`; `receiverEcefM` is a length-3 `Float64Array`.
fn decode_velocity_observations(observations: JsValue) -> Result<Vec<CoreVelObs>, JsValue> {
    let observations: Vec<VelocityObsInput> = serde_wasm_bindgen::from_value(observations)
        .map_err(|e| type_error(&format!("invalid observations: {e}")))?;
    observations
        .into_iter()
        .map(|o| {
            Ok(CoreVelObs {
                satellite_id: parse_sat(&o.satellite_id)?,
                value: o.value,
                carrier_hz: o.carrier_hz,
                sat_clock_drift_s_s: o.sat_clock_drift_s_s,
            })
        })
        .collect::<Result<Vec<_>, JsValue>>()
}

fn receiver_ecef(receiver_ecef_m: &[f64]) -> Result<[f64; 3], JsValue> {
    if receiver_ecef_m.len() != 3 {
        return Err(type_error("receiverEcefM must have length 3"));
    }
    Ok([receiver_ecef_m[0], receiver_ecef_m[1], receiver_ecef_m[2]])
}

/// Solve receiver velocity over an SP3 precise product. See
/// [`solveVelocityBroadcast`] for the broadcast-ephemeris counterpart.
#[wasm_bindgen(js_name = solveVelocity)]
pub fn solve_velocity(
    sp3: &Sp3,
    observations: JsValue,
    receiver_ecef_m: &[f64],
    t_rx_j2000_s: f64,
    options: JsValue,
) -> Result<VelocitySolution, JsValue> {
    let observations = decode_velocity_observations(observations)?;
    let receiver = receiver_ecef(receiver_ecef_m)?;
    let options = velocity_options(options)?;
    let inner = velocity::solve(&sp3.inner, &observations, receiver, t_rx_j2000_s, options)
        .map_err(engine_error)?;
    Ok(VelocitySolution { inner })
}

/// Solve receiver ECEF velocity and clock drift from one epoch of observations
/// over a broadcast ephemeris store. Identical to [`solveVelocity`] except the
/// satellite states come from broadcast records. Delegates to
/// `sidereon_core::velocity::solve`.
#[wasm_bindgen(js_name = solveVelocityBroadcast)]
pub fn solve_velocity_broadcast(
    broadcast: &BroadcastEphemeris,
    observations: JsValue,
    receiver_ecef_m: &[f64],
    t_rx_j2000_s: f64,
    options: JsValue,
) -> Result<VelocitySolution, JsValue> {
    let observations = decode_velocity_observations(observations)?;
    let receiver = receiver_ecef(receiver_ecef_m)?;
    let options = velocity_options(options)?;
    let inner = velocity::solve(
        &broadcast.inner,
        &observations,
        receiver,
        t_rx_j2000_s,
        options,
    )
    .map_err(engine_error)?;
    Ok(VelocitySolution { inner })
}

// ---- observable prediction ---------------------------------------------------

/// Predicted GNSS observables for one satellite at one receive epoch. Every
/// field is computed by `sidereon_core::observables::predict`.
#[wasm_bindgen]
pub struct PredictedObservables {
    inner: CorePredicted,
}

#[wasm_bindgen]
impl PredictedObservables {
    /// Geometric range after optional Sagnac rotation, metres.
    #[wasm_bindgen(getter, js_name = geometricRangeM)]
    pub fn geometric_range_m(&self) -> f64 {
        self.inner.geometric_range_m
    }

    /// Range-rate line-of-sight projection, metres per second.
    #[wasm_bindgen(getter, js_name = rangeRateMS)]
    pub fn range_rate_m_s(&self) -> f64 {
        self.inner.range_rate_m_s
    }

    /// Doppler shift at the option carrier frequency, hertz.
    #[wasm_bindgen(getter, js_name = dopplerHz)]
    pub fn doppler_hz(&self) -> f64 {
        self.inner.doppler_hz
    }

    /// Satellite clock offset at transmit time, seconds, or `undefined`.
    #[wasm_bindgen(getter, js_name = satClockS)]
    pub fn sat_clock_s(&self) -> Option<f64> {
        self.inner.sat_clock_s
    }

    /// Topocentric elevation, degrees.
    #[wasm_bindgen(getter, js_name = elevationDeg)]
    pub fn elevation_deg(&self) -> f64 {
        self.inner.elevation_deg
    }

    /// Topocentric azimuth in `[0, 360)`, degrees.
    #[wasm_bindgen(getter, js_name = azimuthDeg)]
    pub fn azimuth_deg(&self) -> f64 {
        self.inner.azimuth_deg
    }

    /// Transmit-time offset from receive time, rounded to microseconds.
    #[wasm_bindgen(getter, js_name = transmitOffsetUs)]
    pub fn transmit_offset_us(&self) -> i64 {
        self.inner.transmit_offset_us
    }

    /// Transmit time as seconds since J2000.
    #[wasm_bindgen(getter, js_name = transmitTimeJ2000S)]
    pub fn transmit_time_j2000_s(&self) -> f64 {
        self.inner.transmit_time_j2000_s
    }

    /// Receiver-to-satellite line-of-sight unit vector in ECEF, `[x, y, z]`.
    #[wasm_bindgen(getter, js_name = losUnit)]
    pub fn los_unit(&self) -> Vec<f64> {
        self.inner.los_unit.to_vec()
    }

    /// Sagnac-rotated satellite ECEF position, metres, `[x, y, z]`.
    #[wasm_bindgen(getter, js_name = satPosEcefM)]
    pub fn sat_pos_ecef_m(&self) -> Vec<f64> {
        self.inner.sat_pos_ecef_m.to_vec()
    }

    /// Sagnac-rotated satellite ECEF velocity, metres per second, `[vx, vy, vz]`.
    #[wasm_bindgen(getter, js_name = satVelocityMS)]
    pub fn sat_velocity_m_s(&self) -> Vec<f64> {
        self.inner.sat_velocity_m_s.to_vec()
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct PredictOptionsInput {
    carrier_hz: Option<f64>,
    light_time: Option<bool>,
    sagnac: Option<bool>,
}

pub(crate) fn predict_options(options: JsValue) -> Result<CorePredictOptions, JsValue> {
    let input: PredictOptionsInput = if options.is_undefined() || options.is_null() {
        PredictOptionsInput::default()
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid predict options: {e}")))?
    };
    let core_default = CorePredictOptions::default();
    Ok(CorePredictOptions {
        carrier_hz: input.carrier_hz.unwrap_or(core_default.carrier_hz),
        light_time: input.light_time.unwrap_or(core_default.light_time),
        sagnac: input.sagnac.unwrap_or(core_default.sagnac),
    })
}

fn predict_from_source(
    source: &dyn ObservableEphemerisSource,
    satellite: &str,
    receiver_ecef_m: &[f64],
    t_rx_j2000_s: f64,
    options: JsValue,
) -> Result<PredictedObservables, JsValue> {
    let sat = parse_sat(satellite)?;
    let receiver = receiver_ecef(receiver_ecef_m)?;
    let options = predict_options(options)?;
    let inner = core_predict(source, sat, receiver, t_rx_j2000_s, options).map_err(engine_error)?;
    Ok(PredictedObservables { inner })
}

/// Predict observables for one satellite from an SP3 precise product. See
/// [`observablesBroadcast`] for the broadcast-ephemeris counterpart. Delegates
/// to `sidereon_core::observables::predict`.
#[wasm_bindgen(js_name = observablesSp3)]
pub fn observables_sp3(
    sp3: &Sp3,
    satellite: &str,
    receiver_ecef_m: &[f64],
    t_rx_j2000_s: f64,
    options: JsValue,
) -> Result<PredictedObservables, JsValue> {
    predict_from_source(
        &sp3.inner,
        satellite,
        receiver_ecef_m,
        t_rx_j2000_s,
        options,
    )
}

/// Predict observables for one satellite from a broadcast ephemeris store.
/// Delegates to `sidereon_core::observables::predict`.
#[wasm_bindgen(js_name = observablesBroadcast)]
pub fn observables_broadcast(
    broadcast: &BroadcastEphemeris,
    satellite: &str,
    receiver_ecef_m: &[f64],
    t_rx_j2000_s: f64,
    options: JsValue,
) -> Result<PredictedObservables, JsValue> {
    predict_from_source(
        &broadcast.inner,
        satellite,
        receiver_ecef_m,
        t_rx_j2000_s,
        options,
    )
}

/// The per-request results of a batch observable prediction, index-aligned to
/// the input requests. Each request independently either produced observables or
/// failed; query a request with [`PredictBatch.isOk`] /
/// [`PredictBatch.observables`] / [`PredictBatch.error`].
#[wasm_bindgen]
pub struct PredictBatch {
    results: Vec<Result<CorePredicted, String>>,
}

#[wasm_bindgen]
impl PredictBatch {
    /// Number of requests in the batch.
    #[wasm_bindgen(getter)]
    pub fn count(&self) -> usize {
        self.results.len()
    }

    /// Whether request `index` produced observables. Throws a `RangeError` for
    /// an out-of-range index.
    #[wasm_bindgen(js_name = isOk)]
    pub fn is_ok(&self, index: usize) -> Result<bool, JsValue> {
        self.results
            .get(index)
            .map(Result::is_ok)
            .ok_or_else(|| range_error(&format!("request index {index} out of range")))
    }

    /// The observables for request `index`. Throws a `RangeError` for an
    /// out-of-range index and an `Error` carrying that request's failure message
    /// when the prediction failed (check [`PredictBatch.isOk`] first).
    pub fn observables(&self, index: usize) -> Result<PredictedObservables, JsValue> {
        match self
            .results
            .get(index)
            .ok_or_else(|| range_error(&format!("request index {index} out of range")))?
        {
            Ok(inner) => Ok(PredictedObservables { inner: *inner }),
            Err(message) => Err(engine_error(message.clone())),
        }
    }

    /// The failure message for request `index`, or `undefined` when it
    /// succeeded. Throws a `RangeError` for an out-of-range index.
    pub fn error(&self, index: usize) -> Result<Option<String>, JsValue> {
        match self
            .results
            .get(index)
            .ok_or_else(|| range_error(&format!("request index {index} out of range")))?
        {
            Ok(_) => Ok(None),
            Err(message) => Ok(Some(message.clone())),
        }
    }
}

/// Build the `(satellite, receiver, epoch)` request tuples shared by both batch
/// entry points, validating the three index-aligned input arrays.
fn build_predict_requests(
    satellites: Vec<String>,
    receivers_ecef_m: &[f64],
    epochs_j2000_s: &[f64],
) -> Result<Vec<PredictRequest>, JsValue> {
    if satellites.is_empty() {
        return Err(type_error("satellites must not be empty"));
    }
    if !receivers_ecef_m.len().is_multiple_of(3) {
        return Err(type_error(&format!(
            "receiversEcefM length ({}) must be a multiple of 3 (flat row-major n-by-3)",
            receivers_ecef_m.len()
        )));
    }
    let n = satellites.len();
    if receivers_ecef_m.len() / 3 != n {
        return Err(type_error(&format!(
            "receiversEcefM ({} rows) and satellites ({n}) must have the same length",
            receivers_ecef_m.len() / 3
        )));
    }
    if epochs_j2000_s.len() != n {
        return Err(type_error(&format!(
            "epochsJ2000S ({}) and satellites ({n}) must have the same length",
            epochs_j2000_s.len()
        )));
    }
    let mut requests = Vec::with_capacity(n);
    for (i, token) in satellites.iter().enumerate() {
        let sat = parse_sat(token)?;
        let receiver = receiver_ecef(&receivers_ecef_m[i * 3..i * 3 + 3])?;
        let epoch = epochs_j2000_s[i];
        if !epoch.is_finite() {
            return Err(range_error(&format!("epochsJ2000S[{i}] must be finite")));
        }
        requests.push((sat, receiver, epoch));
    }
    Ok(requests)
}

fn predict_batch_from_source(
    source: &dyn ObservableEphemerisSource,
    satellites: Vec<String>,
    receivers_ecef_m: &[f64],
    epochs_j2000_s: &[f64],
    options: JsValue,
) -> Result<PredictBatch, JsValue> {
    let requests = build_predict_requests(satellites, receivers_ecef_m, epochs_j2000_s)?;
    let options = predict_options(options)?;
    let results = core_predict_batch(source, &requests, options)
        .into_iter()
        .map(|result| result.map_err(|e| e.to_string()))
        .collect();
    Ok(PredictBatch { results })
}

/// Predict observables for many `(satellite, receiver, epoch)` requests from an
/// SP3 precise product, serially.
///
/// `satellites` is an array of satellite tokens, `receiversEcefM` a flat
/// row-major `(n, 3)` `Float64Array` of receiver ECEF positions (metres), and
/// `epochsJ2000S` a `Float64Array` of receive epochs (seconds since J2000); all
/// three are index-aligned and length `n`. Element `i` of the result corresponds
/// to request `i`. Delegates to the serial reference kernel
/// `sidereon_core::observables::predict_batch`; the binding never spawns the
/// rayon thread pool the parallel variant uses.
#[wasm_bindgen(js_name = predictBatchSp3)]
pub fn predict_batch_sp3(
    sp3: &Sp3,
    satellites: Vec<String>,
    receivers_ecef_m: &[f64],
    epochs_j2000_s: &[f64],
    options: JsValue,
) -> Result<PredictBatch, JsValue> {
    predict_batch_from_source(
        &sp3.inner,
        satellites,
        receivers_ecef_m,
        epochs_j2000_s,
        options,
    )
}

/// Predict observables for many `(satellite, receiver, epoch)` requests from a
/// broadcast ephemeris store, serially. See [`predictBatchSp3`] for the argument
/// shapes. Delegates to the serial `sidereon_core::observables::predict_batch`.
#[wasm_bindgen(js_name = predictBatchBroadcast)]
pub fn predict_batch_broadcast(
    broadcast: &BroadcastEphemeris,
    satellites: Vec<String>,
    receivers_ecef_m: &[f64],
    epochs_j2000_s: &[f64],
    options: JsValue,
) -> Result<PredictBatch, JsValue> {
    predict_batch_from_source(
        &broadcast.inner,
        satellites,
        receivers_ecef_m,
        epochs_j2000_s,
        options,
    )
}

// ---- signal ------------------------------------------------------------------

/// GPS C/A code chips for a PRN, an `Int8Array` of length 1023, chips `+1`/`-1`.
#[wasm_bindgen(js_name = caCode)]
pub fn ca_code(prn: i64) -> Result<Vec<i8>, JsValue> {
    signal::ca_code(prn).map_err(domain_error)
}

/// One wrapping GPS C/A chip at a zero-based index.
#[wasm_bindgen(js_name = caChip)]
pub fn ca_chip(prn: i64, index: i64) -> Result<i8, JsValue> {
    signal::ca_chip(prn, index).map_err(domain_error)
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ReplicaOptionsInput {
    sample_rate_hz: Option<f64>,
    num_samples: Option<usize>,
    code_phase_chips: Option<f64>,
    code_doppler_hz: Option<f64>,
}

fn replica_options(options: JsValue) -> Result<CoreReplicaOptions, JsValue> {
    if options.is_undefined() || options.is_null() {
        return Ok(CoreReplicaOptions::one_code_period());
    }
    let input: ReplicaOptionsInput = serde_wasm_bindgen::from_value(options)
        .map_err(|e| type_error(&format!("invalid replica options: {e}")))?;
    let core_default = CoreReplicaOptions::one_code_period();
    Ok(CoreReplicaOptions {
        sample_rate_hz: input.sample_rate_hz.unwrap_or(core_default.sample_rate_hz),
        num_samples: input.num_samples.unwrap_or(core_default.num_samples),
        code_phase_chips: input
            .code_phase_chips
            .unwrap_or(core_default.code_phase_chips),
        code_doppler_hz: input
            .code_doppler_hz
            .unwrap_or(core_default.code_doppler_hz),
    })
}

/// Build a sampled GPS C/A code replica, an `Int8Array`.
#[wasm_bindgen]
pub fn replica(prn: i64, options: JsValue) -> Result<Vec<i8>, JsValue> {
    let options = replica_options(options)?;
    signal::replica(prn, options).map_err(domain_error)
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct CorrelateOptionsInput {
    sample_rate_hz: Option<f64>,
    doppler_hz: Option<f64>,
    code_phase_chips: Option<f64>,
    code_doppler_hz: Option<f64>,
}

fn correlate_options(options: JsValue) -> Result<CoreCorrelateOptions, JsValue> {
    if options.is_undefined() || options.is_null() {
        return Ok(CoreCorrelateOptions::default());
    }
    let input: CorrelateOptionsInput = serde_wasm_bindgen::from_value(options)
        .map_err(|e| type_error(&format!("invalid correlate options: {e}")))?;
    let core_default = CoreCorrelateOptions::default();
    Ok(CoreCorrelateOptions {
        sample_rate_hz: input.sample_rate_hz.unwrap_or(core_default.sample_rate_hz),
        doppler_hz: input.doppler_hz.unwrap_or(core_default.doppler_hz),
        code_phase_chips: input
            .code_phase_chips
            .unwrap_or(core_default.code_phase_chips),
        code_doppler_hz: input
            .code_doppler_hz
            .unwrap_or(core_default.code_doppler_hz),
    })
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct AcquisitionOptionsInput {
    sample_rate_hz: Option<f64>,
    doppler_min_hz: Option<f64>,
    doppler_max_hz: Option<f64>,
    doppler_step_hz: Option<f64>,
}

fn acquisition_options(options: JsValue) -> Result<CoreAcqOptions, JsValue> {
    if options.is_undefined() || options.is_null() {
        return Ok(CoreAcqOptions::default());
    }
    let input: AcquisitionOptionsInput = serde_wasm_bindgen::from_value(options)
        .map_err(|e| type_error(&format!("invalid acquisition options: {e}")))?;
    let core_default = CoreAcqOptions::default();
    Ok(CoreAcqOptions {
        sample_rate_hz: input.sample_rate_hz.unwrap_or(core_default.sample_rate_hz),
        doppler_min_hz: input.doppler_min_hz.unwrap_or(core_default.doppler_min_hz),
        doppler_max_hz: input.doppler_max_hz.unwrap_or(core_default.doppler_max_hz),
        doppler_step_hz: input
            .doppler_step_hz
            .unwrap_or(core_default.doppler_step_hz),
    })
}

/// Reshape an interleaved `[i0, q0, i1, q1, ...]` `Float64Array` into IQ samples.
fn iq_samples(name: &str, iq: &[f64]) -> Result<Vec<IqSample>, JsValue> {
    if !iq.len().is_multiple_of(2) {
        return Err(type_error(&format!(
            "{name} must be an interleaved [i, q] array of even length"
        )));
    }
    Ok(iq
        .chunks_exact(2)
        .map(|c| IqSample::new(c[0], c[1]))
        .collect())
}

/// Coherent GPS C/A correlation result.
#[wasm_bindgen]
pub struct CorrelationResult {
    inner: CoreCorrelationResult,
}

#[wasm_bindgen]
impl CorrelationResult {
    /// In-phase coherent sum.
    #[wasm_bindgen(getter)]
    pub fn i(&self) -> f64 {
        self.inner.i
    }

    /// Quadrature coherent sum.
    #[wasm_bindgen(getter)]
    pub fn q(&self) -> f64 {
        self.inner.q
    }

    /// Squared magnitude, `i*i + q*q`.
    #[wasm_bindgen(getter)]
    pub fn power(&self) -> f64 {
        self.inner.power
    }
}

/// Acquisition search-grid metadata.
#[wasm_bindgen]
pub struct AcquisitionGrid {
    inner: CoreAcqGrid,
}

#[wasm_bindgen]
impl AcquisitionGrid {
    /// Doppler bins searched, hertz, as a `Float64Array`.
    #[wasm_bindgen(getter, js_name = dopplerHz)]
    pub fn doppler_hz(&self) -> Vec<f64> {
        self.inner.doppler_hz.clone()
    }

    /// Number of code-phase bins searched.
    #[wasm_bindgen(getter, js_name = codePhaseBins)]
    pub fn code_phase_bins(&self) -> usize {
        self.inner.code_phase_bins
    }

    /// Doppler step, hertz.
    #[wasm_bindgen(getter, js_name = dopplerStepHz)]
    pub fn doppler_step_hz(&self) -> f64 {
        self.inner.doppler_step_hz
    }

    /// Samples per C/A chip.
    #[wasm_bindgen(getter, js_name = samplesPerChip)]
    pub fn samples_per_chip(&self) -> f64 {
        self.inner.samples_per_chip
    }
}

/// Result of a 2D C/A code-phase and Doppler acquisition search.
#[wasm_bindgen]
pub struct AcquisitionResult {
    inner: CoreAcqResult,
}

#[wasm_bindgen]
impl AcquisitionResult {
    /// Recovered C/A code phase, chips.
    #[wasm_bindgen(getter, js_name = codePhaseChips)]
    pub fn code_phase_chips(&self) -> f64 {
        self.inner.code_phase_chips
    }

    /// Recovered Doppler bin, hertz.
    #[wasm_bindgen(getter, js_name = dopplerHz)]
    pub fn doppler_hz(&self) -> f64 {
        self.inner.doppler_hz
    }

    /// Peak-to-mean-off-peak acquisition metric.
    #[wasm_bindgen(getter, js_name = peakMetric)]
    pub fn peak_metric(&self) -> f64 {
        self.inner.peak_metric
    }

    /// Alias of `peakMetric`.
    #[wasm_bindgen(getter)]
    pub fn metric(&self) -> f64 {
        self.inner.metric
    }

    /// Peak correlator power.
    #[wasm_bindgen(getter, js_name = peakPower)]
    pub fn peak_power(&self) -> f64 {
        self.inner.peak_power
    }

    /// Search-grid metadata.
    #[wasm_bindgen(getter)]
    pub fn grid(&self) -> AcquisitionGrid {
        AcquisitionGrid {
            inner: self.inner.grid.clone(),
        }
    }
}

/// Coherently correlate interleaved IQ samples against a GPS C/A PRN replica.
/// `iq` is `[i0, q0, i1, q1, ...]`.
#[wasm_bindgen]
pub fn correlate(iq: &[f64], prn: i64, options: JsValue) -> Result<CorrelationResult, JsValue> {
    let iq = iq_samples("iq", iq)?;
    let options = correlate_options(options)?;
    signal::correlate(&iq, prn, options)
        .map(|inner| CorrelationResult { inner })
        .map_err(domain_error)
}

/// Acquire a GPS C/A PRN by direct code-phase and Doppler search. `samples` is
/// interleaved `[i0, q0, i1, q1, ...]`.
#[wasm_bindgen]
pub fn acquire(samples: &[f64], prn: i64, options: JsValue) -> Result<AcquisitionResult, JsValue> {
    let samples = iq_samples("samples", samples)?;
    let options = acquisition_options(options)?;
    signal::acquire(&samples, prn, options)
        .map(|inner| AcquisitionResult { inner })
        .map_err(domain_error)
}

/// Coherent integration loss from residual frequency error.
#[wasm_bindgen(js_name = coherentLoss)]
pub fn coherent_loss(freq_error_hz: f64, integration_time_s: f64) -> Result<f64, JsValue> {
    signal::coherent_loss(freq_error_hz, integration_time_s).map_err(domain_error)
}

/// Coherent integration loss in decibels.
#[wasm_bindgen(js_name = coherentLossDb)]
pub fn coherent_loss_db(freq_error_hz: f64, integration_time_s: f64) -> Result<f64, JsValue> {
    signal::coherent_loss_db(freq_error_hz, integration_time_s).map_err(domain_error)
}

/// Post-correlation predetection SNR in decibels.
#[wasm_bindgen(js_name = snrPostDb)]
pub fn snr_post_db(cn0_dbhz: f64, integration_time_s: f64) -> Result<f64, JsValue> {
    signal::snr_post_db(cn0_dbhz, integration_time_s).map_err(domain_error)
}

#[cfg(test)]
mod drift_tests {
    //! The pseudorange-variance defaults track the core
    //! `PseudorangeVarianceOptions::default()` rather than literals in this
    //! binding. The cycle-slip and signal option builders likewise resolve every
    //! absent field from a core `Default`/constructor, so their defaults are the
    //! core values by construction.
    use super::*;

    #[test]
    fn variance_defaults_track_core() {
        let got = PseudorangeVarianceOptionsInput::default()
            .to_core()
            .expect("default variance options are valid");
        let core = CoreVarOptions::default();
        assert_eq!(got.a_m, core.a_m);
        assert_eq!(got.b_m, core.b_m);
        assert_eq!(got.cn0_scale_m2, core.cn0_scale_m2);
    }

    #[test]
    fn velocity_defaults_track_core() {
        // The velocity option builder fills every omitted field from
        // `VelocitySolveOptions::default()`, so the empty-input result is the
        // core default by construction.
        let core = CoreVelOptions::default();
        assert!(core.light_time);
        assert!(core.sagnac);
    }

    #[test]
    fn hatch_window_cap_default_pinned() {
        // The Hatch smoothing entry points fall back to this core constant when
        // the caller omits `hatchWindowCap`.
        assert_eq!(DEFAULT_HATCH_WINDOW_CAP, 100);
    }
}
