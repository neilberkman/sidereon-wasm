//! Orbit and clock prediction scoring against precise ephemerides.

use std::sync::Arc;

use serde_json::json;
use sidereon_core::astro::time::model::Instant as CoreInstant;
use sidereon_core::ephemeris::{
    self, Sp3FrameLabelSet, Sp3FrameReconciliationOptions as CoreFrameOptions,
};
use sidereon_core::frame_catalog::FrameCatalogError;
use sidereon_core::orbit_scoring::{self as scoring, OrbitSource as CoreSource};
use wasm_bindgen::prelude::*;

use crate::error::{index_arg, range_error, type_error, typed_error};
use crate::frame_catalog::TerrestrialFrame;
use crate::frames::TimeScale;
use crate::gnss::GnssSystem;
use crate::marshal::vec3;
use crate::rinex_clock::ClockInstant;
use crate::rinex_nav::BroadcastEphemeris;
use crate::sp3::{parse_sat, Sp3};

type JsResult<T> = Result<T, JsValue>;

/// Reference velocity used to build the orbital frame.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TriadVelocity {
    /// Use inertial reference velocity.
    Inertial,
    /// Use Earth-fixed reference velocity.
    EarthFixed,
}

impl From<TriadVelocity> for scoring::TriadVelocity {
    fn from(value: TriadVelocity) -> Self {
        match value {
            TriadVelocity::Inertial => Self::Inertial,
            TriadVelocity::EarthFixed => Self::EarthFixed,
        }
    }
}

impl From<scoring::TriadVelocity> for TriadVelocity {
    fn from(value: scoring::TriadVelocity) -> Self {
        match value {
            scoring::TriadVelocity::Inertial => Self::Inertial,
            scoring::TriadVelocity::EarthFixed => Self::EarthFixed,
        }
    }
}

/// Core default.
#[wasm_bindgen(js_name = triadVelocityDefault)]
pub fn triad_velocity_default() -> TriadVelocity {
    scoring::TriadVelocity::default().into()
}

/// Clock comparison datum.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ClockDatum {
    /// Subtract the per-epoch, per-constellation median.
    EpochSystemMedian,
    /// Use the raw clock difference.
    Unadjusted,
}

impl From<ClockDatum> for scoring::ClockDatum {
    fn from(value: ClockDatum) -> Self {
        match value {
            ClockDatum::EpochSystemMedian => Self::EpochSystemMedian,
            ClockDatum::Unadjusted => Self::Unadjusted,
        }
    }
}

impl From<scoring::ClockDatum> for ClockDatum {
    fn from(value: scoring::ClockDatum) -> Self {
        match value {
            scoring::ClockDatum::EpochSystemMedian => Self::EpochSystemMedian,
            scoring::ClockDatum::Unadjusted => Self::Unadjusted,
        }
    }
}

/// Core default.
#[wasm_bindgen(js_name = clockDatumDefault)]
pub fn clock_datum_default() -> ClockDatum {
    scoring::ClockDatum::default().into()
}

/// Treatment of missing orbit samples.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GapPolicy {
    /// Count missing data in the coverage ledger.
    Count,
    /// Refuse the first missing record or prediction origin.
    Refuse,
}

impl From<GapPolicy> for scoring::GapPolicy {
    fn from(value: GapPolicy) -> Self {
        match value {
            GapPolicy::Count => Self::Count,
            GapPolicy::Refuse => Self::Refuse,
        }
    }
}

impl From<scoring::GapPolicy> for GapPolicy {
    fn from(value: scoring::GapPolicy) -> Self {
        match value {
            scoring::GapPolicy::Count => Self::Count,
            scoring::GapPolicy::Refuse => Self::Refuse,
        }
    }
}

/// Core default.
#[wasm_bindgen(js_name = gapPolicyDefault)]
pub fn gap_policy_default() -> GapPolicy {
    scoring::GapPolicy::default().into()
}

/// Satellite point represented by a position.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OrbitReferencePoint {
    /// The centre of mass, as in an SP3 product.
    CenterOfMass,
    /// The antenna phase centre, as in a broadcast ephemeris.
    AntennaPhaseCenter,
}

impl From<OrbitReferencePoint> for scoring::OrbitReferencePoint {
    fn from(value: OrbitReferencePoint) -> Self {
        match value {
            OrbitReferencePoint::CenterOfMass => Self::CenterOfMass,
            OrbitReferencePoint::AntennaPhaseCenter => Self::AntennaPhaseCenter,
        }
    }
}

impl From<scoring::OrbitReferencePoint> for OrbitReferencePoint {
    fn from(value: scoring::OrbitReferencePoint) -> Self {
        match value {
            scoring::OrbitReferencePoint::CenterOfMass => Self::CenterOfMass,
            scoring::OrbitReferencePoint::AntennaPhaseCenter => Self::AntennaPhaseCenter,
        }
    }
}

/// Navigation satellite orbit regime.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OrbitRegime {
    /// Medium Earth orbit.
    Meo,
    /// Inclined geosynchronous orbit.
    Igso,
    /// Geostationary orbit.
    Geo,
}

impl From<OrbitRegime> for scoring::OrbitRegime {
    fn from(value: OrbitRegime) -> Self {
        match value {
            OrbitRegime::Meo => Self::Meo,
            OrbitRegime::Igso => Self::Igso,
            OrbitRegime::Geo => Self::Geo,
        }
    }
}

impl From<scoring::OrbitRegime> for OrbitRegime {
    fn from(value: scoring::OrbitRegime) -> Self {
        match value {
            scoring::OrbitRegime::Meo => Self::Meo,
            scoring::OrbitRegime::Igso => Self::Igso,
            scoring::OrbitRegime::Geo => Self::Geo,
        }
    }
}

/// Core display name.
#[wasm_bindgen(js_name = orbitRegimeToString)]
pub fn orbit_regime_to_string(
    #[wasm_bindgen(unchecked_param_type = "OrbitRegime")] value: f64,
) -> JsResult<String> {
    Ok(scoring::OrbitRegime::from(orbit_regime_arg(value)?).to_string())
}

/// Classify an inertial state by navigation orbit regime.
#[wasm_bindgen(js_name = orbitRegimeFromInertialState)]
pub fn orbit_regime_from_inertial_state(
    position_m: &[f64],
    inertial_velocity_m_s: &[f64],
) -> JsResult<Option<OrbitRegime>> {
    Ok(scoring::OrbitRegime::from_inertial_state(
        vec3("positionM", position_m)?,
        vec3("inertialVelocityMS", inertial_velocity_m_s)?,
    )
    .map(Into::into))
}

/// Source or reference product in an orbit comparison.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ScoredProduct {
    /// The orbit source being scored.
    Source,
    /// The reference SP3 product.
    Reference,
}

impl From<ScoredProduct> for scoring::ScoredProduct {
    fn from(value: ScoredProduct) -> Self {
        match value {
            ScoredProduct::Source => Self::Source,
            ScoredProduct::Reference => Self::Reference,
        }
    }
}

impl From<scoring::ScoredProduct> for ScoredProduct {
    fn from(value: scoring::ScoredProduct) -> Self {
        match value {
            scoring::ScoredProduct::Source => Self::Source,
            scoring::ScoredProduct::Reference => Self::Reference,
        }
    }
}

/// Core display name.
#[wasm_bindgen(js_name = scoredProductToString)]
pub fn scored_product_to_string(
    #[wasm_bindgen(unchecked_param_type = "ScoredProduct")] value: f64,
) -> JsResult<String> {
    Ok(scoring::ScoredProduct::from(scored_product_arg(value)?).to_string())
}

/// Position difference in the reference radial, transverse and normal frame.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct RtnComponents(pub(crate) scoring::RtnComponents);

#[wasm_bindgen]
impl RtnComponents {
    /// Compare the stored value.
    pub fn equals(&self, other: &RtnComponents) -> bool {
        self.0 == other.0
    }

    /// Build an orbit-frame position difference in metres.
    #[wasm_bindgen(constructor)]
    pub fn new(radial_m: f64, transverse_m: f64, normal_m: f64) -> Self {
        Self(scoring::RtnComponents {
            radial_m,
            transverse_m,
            normal_m,
        })
    }

    /// Radial difference in metres.
    #[wasm_bindgen(getter, js_name = radialM)]
    pub fn radial_m(&self) -> f64 {
        self.0.radial_m
    }

    /// Along-track difference in metres.
    #[wasm_bindgen(getter, js_name = transverseM)]
    pub fn transverse_m(&self) -> f64 {
        self.0.transverse_m
    }

    /// Cross-track difference in metres.
    #[wasm_bindgen(getter, js_name = normalM)]
    pub fn normal_m(&self) -> f64 {
        self.0.normal_m
    }
}

/// Earth-fixed orbit source state.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct OrbitSourceState(scoring::OrbitSourceState);

#[wasm_bindgen]
impl OrbitSourceState {
    /// Compare the stored value.
    pub fn equals(&self, other: &OrbitSourceState) -> bool {
        self.0 == other.0
    }

    /// Earth-fixed position in metres.
    #[wasm_bindgen(getter, js_name = positionEcefM)]
    pub fn position_ecef_m(&self) -> Vec<f64> {
        self.0.position_ecef_m.to_vec()
    }

    /// Clock offset in seconds, when present.
    #[wasm_bindgen(getter, js_name = clockS)]
    pub fn clock_s(&self) -> Option<f64> {
        self.0.clock_s
    }
}

/// Statistics of a signed error series.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct ErrorStats(scoring::ErrorStats);

#[wasm_bindgen]
impl ErrorStats {
    /// Compare the stored value.
    pub fn equals(&self, other: &ErrorStats) -> bool {
        self.0 == other.0
    }

    /// Root mean square error in metres.
    #[wasm_bindgen(getter, js_name = rmsM)]
    pub fn rms_m(&self) -> f64 {
        self.0.rms_m
    }

    /// Mean error in metres.
    #[wasm_bindgen(getter, js_name = meanM)]
    pub fn mean_m(&self) -> f64 {
        self.0.mean_m
    }

    /// Largest absolute error in metres.
    #[wasm_bindgen(getter, js_name = maxAbsM)]
    pub fn max_abs_m(&self) -> f64 {
        self.0.max_abs_m
    }
}

/// Orbit and clock error statistics in one prediction-horizon bin.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct HorizonBinStats(scoring::HorizonBinStats);

#[wasm_bindgen]
impl HorizonBinStats {
    /// Compare the stored value.
    pub fn equals(&self, other: &HorizonBinStats) -> bool {
        self.0 == other.0
    }

    /// Exclusive lower horizon in seconds.
    #[wasm_bindgen(getter, js_name = horizonStartS)]
    pub fn horizon_start_s(&self) -> f64 {
        self.0.horizon_start_s
    }

    /// Inclusive upper horizon in seconds.
    #[wasm_bindgen(getter, js_name = horizonEndS)]
    pub fn horizon_end_s(&self) -> f64 {
        self.0.horizon_end_s
    }

    /// Number of scored epochs.
    #[wasm_bindgen(getter)]
    pub fn count(&self) -> usize {
        self.0.count
    }

    /// Radial error.
    #[wasm_bindgen(getter)]
    pub fn radial(&self) -> Option<ErrorStats> {
        self.0.radial.map(ErrorStats)
    }

    /// Transverse error.
    #[wasm_bindgen(getter)]
    pub fn transverse(&self) -> Option<ErrorStats> {
        self.0.transverse.map(ErrorStats)
    }

    /// Normal error.
    #[wasm_bindgen(getter)]
    pub fn normal(&self) -> Option<ErrorStats> {
        self.0.normal.map(ErrorStats)
    }

    /// Three-dimensional orbit error.
    #[wasm_bindgen(getter, js_name = orbit3d)]
    pub fn orbit_3d(&self) -> Option<ErrorStats> {
        self.0.orbit_3d.map(ErrorStats)
    }

    /// Orbit-only SISRE.
    #[wasm_bindgen(getter, js_name = sisreOrbit)]
    pub fn sisre_orbit(&self) -> Option<ErrorStats> {
        self.0.sisre_orbit.map(ErrorStats)
    }

    /// Number of scored clock epochs.
    #[wasm_bindgen(getter, js_name = clockCount)]
    pub fn clock_count(&self) -> usize {
        self.0.clock_count
    }

    /// Clock error.
    #[wasm_bindgen(getter)]
    pub fn clock(&self) -> Option<ErrorStats> {
        self.0.clock.map(ErrorStats)
    }

    /// SISRE with clock.
    #[wasm_bindgen(getter)]
    pub fn sisre(&self) -> Option<ErrorStats> {
        self.0.sisre.map(ErrorStats)
    }
}

/// Counts of evaluated and missing satellite epochs.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct ScoreCoverage(scoring::ScoreCoverage);

#[wasm_bindgen]
impl ScoreCoverage {
    /// Compare the stored value.
    pub fn equals(&self, other: &ScoreCoverage) -> bool {
        self.0 == other.0
    }

    /// Build empty coverage counts.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self(scoring::ScoreCoverage::default())
    }

    /// Requested satellite and epoch pairs.
    #[wasm_bindgen(getter)]
    pub fn epochs(&self) -> usize {
        self.0.epochs
    }

    /// Pairs scored.
    #[wasm_bindgen(getter)]
    pub fn scored(&self) -> usize {
        self.0.scored
    }

    /// Scored pairs with clock errors.
    #[wasm_bindgen(getter)]
    pub fn clocked(&self) -> usize {
        self.0.clocked
    }

    /// Pairs outside the horizon bins.
    #[wasm_bindgen(getter, js_name = outsideHorizonBins)]
    pub fn outside_horizon_bins(&self) -> usize {
        self.0.outside_horizon_bins
    }

    /// Pairs missing a prediction origin.
    #[wasm_bindgen(getter, js_name = noPredictionOrigin)]
    pub fn no_prediction_origin(&self) -> usize {
        self.0.no_prediction_origin
    }

    /// Pairs whose satellite is missing from the source.
    #[wasm_bindgen(getter, js_name = sourceSatelliteMissing)]
    pub fn source_satellite_missing(&self) -> usize {
        self.0.source_satellite_missing
    }

    /// Pairs whose epoch is missing from the source.
    #[wasm_bindgen(getter, js_name = sourceEpochMissing)]
    pub fn source_epoch_missing(&self) -> usize {
        self.0.source_epoch_missing
    }

    /// Pairs whose satellite is missing from the reference.
    #[wasm_bindgen(getter, js_name = referenceSatelliteMissing)]
    pub fn reference_satellite_missing(&self) -> usize {
        self.0.reference_satellite_missing
    }

    /// Pairs whose epoch is missing from the reference.
    #[wasm_bindgen(getter, js_name = referenceEpochMissing)]
    pub fn reference_epoch_missing(&self) -> usize {
        self.0.reference_epoch_missing
    }
}

/// Prediction-horizon scores for one satellite.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct SatelliteScore(scoring::SatelliteScore);

#[wasm_bindgen]
impl SatelliteScore {
    /// Compare the stored value.
    pub fn equals(&self, other: &SatelliteScore) -> bool {
        self.0 == other.0
    }

    /// Satellite identifier.
    #[wasm_bindgen(getter)]
    pub fn satellite(&self) -> String {
        self.0.satellite.to_string()
    }

    /// Prediction origin in seconds from J2000.
    #[wasm_bindgen(getter, js_name = predictionOriginJ2000S)]
    pub fn prediction_origin_j2000_s(&self) -> Option<f64> {
        self.0.prediction_origin_j2000_s
    }

    /// Reference orbit regime, if determined.
    #[wasm_bindgen(getter, js_name = orbitRegime)]
    pub fn orbit_regime(&self) -> Option<OrbitRegime> {
        self.0.orbit_regime.map(Into::into)
    }

    /// SISRE weights applied to the satellite.
    #[wasm_bindgen(getter, js_name = sisreWeights)]
    pub fn sisre_weights(&self) -> Option<SisreWeights> {
        self.0.sisre_weights.map(SisreWeights)
    }

    /// Statistics by horizon bin.
    #[wasm_bindgen(getter)]
    pub fn bins(&self) -> Vec<HorizonBinStats> {
        self.0.bins.iter().cloned().map(HorizonBinStats).collect()
    }

    /// Coverage of requested epochs.
    #[wasm_bindgen(getter)]
    pub fn coverage(&self) -> ScoreCoverage {
        ScoreCoverage(self.0.coverage)
    }
}

/// Prediction-horizon scores for one constellation.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct SystemScore(scoring::SystemScore);

#[wasm_bindgen]
impl SystemScore {
    /// Compare the stored value.
    pub fn equals(&self, other: &SystemScore) -> bool {
        self.0 == other.0
    }

    /// GNSS constellation.
    #[wasm_bindgen(getter)]
    pub fn system(&self) -> GnssSystem {
        self.0.system.into()
    }

    /// Statistics by horizon bin.
    #[wasm_bindgen(getter)]
    pub fn bins(&self) -> Vec<HorizonBinStats> {
        self.0.bins.iter().cloned().map(HorizonBinStats).collect()
    }

    /// Coverage across satellites.
    #[wasm_bindgen(getter)]
    pub fn coverage(&self) -> ScoreCoverage {
        ScoreCoverage(self.0.coverage)
    }
}

/// Orbit and clock prediction score report.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct OrbitScoreReport(scoring::OrbitScoreReport);

#[wasm_bindgen]
impl OrbitScoreReport {
    /// Compare the stored value.
    pub fn equals(&self, other: &OrbitScoreReport) -> bool {
        self.0 == other.0
    }

    /// Horizon bin edges in seconds.
    #[wasm_bindgen(getter, js_name = horizonBinEdgesS)]
    pub fn horizon_bin_edges_s(&self) -> Vec<f64> {
        self.0.horizon_bin_edges_s.clone()
    }

    /// Frame agreement or reconciliation.
    #[wasm_bindgen(getter)]
    pub fn frame(&self) -> FrameAgreement {
        FrameAgreement(self.0.frame.clone())
    }

    /// Point represented by source positions.
    #[wasm_bindgen(getter, js_name = sourceReferencePoint)]
    pub fn source_reference_point(&self) -> OrbitReferencePoint {
        self.0.source_reference_point.into()
    }

    /// Scores by satellite, in requested order.
    #[wasm_bindgen(getter, js_name = perSatellite)]
    pub fn per_satellite(&self) -> Vec<SatelliteScore> {
        self.0
            .per_satellite
            .iter()
            .cloned()
            .map(SatelliteScore)
            .collect()
    }

    /// Scores by constellation.
    #[wasm_bindgen(getter, js_name = perSystem)]
    pub fn per_system(&self) -> Vec<SystemScore> {
        self.0.per_system.iter().cloned().map(SystemScore).collect()
    }

    /// Coverage across all requested epochs and satellites.
    #[wasm_bindgen(getter)]
    pub fn coverage(&self) -> ScoreCoverage {
        ScoreCoverage(self.0.coverage)
    }
}

/// Earth-fixed satellite position and clock at one epoch.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct TrajectorySample(scoring::TrajectorySample);

#[wasm_bindgen]
impl TrajectorySample {
    /// Compare the stored value.
    pub fn equals(&self, other: &TrajectorySample) -> bool {
        self.0 == other.0
    }

    /// Build a trajectory sample with an optional clock value.
    #[wasm_bindgen(constructor)]
    pub fn new(
        satellite: &str,
        epoch: &ClockInstant,
        position_ecef_m: &[f64],
        clock_s: Option<f64>,
    ) -> JsResult<Self> {
        Ok(Self(scoring::TrajectorySample {
            satellite: parse_sat(satellite)?,
            epoch: epoch.0,
            position_ecef_m: vec3("positionEcefM", position_ecef_m)?,
            clock_s,
        }))
    }

    /// Satellite identifier.
    #[wasm_bindgen(getter)]
    pub fn satellite(&self) -> String {
        self.0.satellite.to_string()
    }

    /// Epoch in the trajectory time scale.
    #[wasm_bindgen(getter)]
    pub fn epoch(&self) -> ClockInstant {
        ClockInstant(self.0.epoch)
    }

    /// Earth-fixed position in metres.
    #[wasm_bindgen(getter, js_name = positionEcefM)]
    pub fn position_ecef_m(&self) -> Vec<f64> {
        self.0.position_ecef_m.to_vec()
    }

    /// Satellite clock offset in seconds, when present.
    #[wasm_bindgen(getter, js_name = clockS)]
    pub fn clock_s(&self) -> Option<f64> {
        self.0.clock_s
    }
}

/// How source coordinates were matched to the reference.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct FrameAgreement(scoring::FrameAgreement);

#[wasm_bindgen]
impl FrameAgreement {
    /// Compare the stored value.
    pub fn equals(&self, other: &FrameAgreement) -> bool {
        self.0 == other.0
    }

    /// The two labels match.
    #[wasm_bindgen(js_name = SAME_LABEL)]
    pub fn same_label() -> Self {
        Self(scoring::FrameAgreement::SameLabel)
    }
    /// The labels were asserted equivalent.
    #[wasm_bindgen(js_name = ASSERTED_EQUIVALENT)]
    pub fn asserted_equivalent() -> Self {
        Self(scoring::FrameAgreement::AssertedEquivalent)
    }
    /// The source uses the constellation broadcast datums.
    #[wasm_bindgen(js_name = BROADCAST_DATUMS)]
    pub fn broadcast_datums() -> Self {
        Self(scoring::FrameAgreement::BroadcastDatums)
    }
    /// Represent a Helmert transformation between two terrestrial realizations.
    pub fn helmert(
        #[wasm_bindgen(unchecked_param_type = "TerrestrialFrame")] from_frame: f64,
        #[wasm_bindgen(unchecked_param_type = "TerrestrialFrame")] to_frame: f64,
    ) -> JsResult<Self> {
        Ok(Self(scoring::FrameAgreement::Helmert {
            from: frame_arg(from_frame)?.into(),
            to: frame_arg(to_frame)?.into(),
        }))
    }

    /// Source realization in a Helmert transformation.
    #[allow(clippy::wrong_self_convention)]
    #[wasm_bindgen(js_name = fromFrame, getter)]
    pub fn from_frame(&self) -> Option<TerrestrialFrame> {
        match self.0 {
            scoring::FrameAgreement::Helmert { from, .. } => Some(from.into()),
            _ => None,
        }
    }

    /// Reference realization in a Helmert transformation.
    #[wasm_bindgen(getter, js_name = toFrame)]
    pub fn to_frame(&self) -> Option<TerrestrialFrame> {
        match self.0 {
            scoring::FrameAgreement::Helmert { to, .. } => Some(to.into()),
            _ => None,
        }
    }
}

/// Radial and transverse-normal weights for signal-in-space range error.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct SisreWeights(scoring::SisreWeights);

#[wasm_bindgen]
impl SisreWeights {
    /// Compare the stored value.
    pub fn equals(&self, other: &SisreWeights) -> bool {
        self.0 == other.0
    }

    /// Build SISRE weights from the radial weight and squared transverse-normal weight.
    #[wasm_bindgen(constructor)]
    pub fn new(radial: f64, transverse_normal_squared: f64) -> JsResult<Self> {
        scoring::SisreWeights::new(radial, transverse_normal_squared)
            .map(Self)
            .map_err(score_error)
    }

    /// Radial weight.
    pub fn radial(&self) -> f64 {
        self.0.radial()
    }

    /// Squared transverse-normal weight.
    #[wasm_bindgen(js_name = transverseNormalSquared)]
    pub fn transverse_normal_squared(&self) -> f64 {
        self.0.transverse_normal_squared()
    }

    /// Published SISRE weights for a constellation and regime, when available.
    #[wasm_bindgen(js_name = montenbruck2018)]
    pub fn montenbruck_2018(
        #[wasm_bindgen(unchecked_param_type = "GnssSystem")] system: f64,
        #[wasm_bindgen(unchecked_param_type = "OrbitRegime")] regime: f64,
    ) -> JsResult<Option<Self>> {
        Ok(scoring::SisreWeights::montenbruck_2018(
            crate::gnss_attitude::parse_system(system)?.into(),
            orbit_regime_arg(regime)?.into(),
        )
        .map(Self))
    }

    /// Compute the signal-in-space range error in metres.
    #[wasm_bindgen(js_name = orbitOnlyM)]
    pub fn orbit_only_m(&self, components: &RtnComponents) -> JsResult<f64> {
        self.0.orbit_only_m(components.0).map_err(score_error)
    }

    /// Compute the signal-in-space range error in metres.
    #[wasm_bindgen(js_name = withClockM)]
    pub fn with_clock_m(&self, components: &RtnComponents, clock_m: f64) -> JsResult<f64> {
        self.0
            .with_clock_m(components.0, clock_m)
            .map_err(score_error)
    }
}

/// Coordinate frame of an orbit source.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct OrbitFrame(scoring::OrbitFrame);

#[wasm_bindgen]
impl OrbitFrame {
    /// Compare the stored value.
    pub fn equals(&self, other: &OrbitFrame) -> bool {
        self.0 == other.0
    }

    /// Frame as reported by the broadcast ephemerides.
    #[wasm_bindgen(js_name = BROADCAST_DATUMS)]
    pub fn broadcast_datums() -> Self {
        Self(scoring::OrbitFrame::BroadcastDatums)
    }
    /// Build the selection from label.
    pub fn label(label: String) -> Self {
        Self(scoring::OrbitFrame::Label(label))
    }

    /// Configured label, if explicitly supplied.
    #[wasm_bindgen(getter, js_name = frameLabel)]
    pub fn frame_label(&self) -> Option<String> {
        match &self.0 {
            scoring::OrbitFrame::Label(value) => Some(value.clone()),
            _ => None,
        }
    }
}

/// Epochs to score against the reference product.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct ScoreEpochs(scoring::ScoreEpochs);

#[wasm_bindgen]
impl ScoreEpochs {
    /// Compare the stored value.
    pub fn equals(&self, other: &ScoreEpochs) -> bool {
        self.0 == other.0
    }

    /// Default selection.
    #[wasm_bindgen(js_name = REFERENCE_RECORDS)]
    pub fn reference_records() -> Self {
        Self(scoring::ScoreEpochs::default())
    }
    /// Build the core default selection.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self(scoring::ScoreEpochs::default())
    }

    /// Build the selection from instants. The instant handles are consumed.
    pub fn instants(instants: Vec<ClockInstant>) -> Self {
        Self(scoring::ScoreEpochs::Instants(
            instants.into_iter().map(|instant| instant.0).collect(),
        ))
    }

    /// Configured instants, if explicitly supplied.
    #[wasm_bindgen(getter, js_name = epochInstants, unchecked_return_type = "ClockInstant[] | undefined")]
    pub fn epoch_instants(&self) -> JsValue {
        match &self.0 {
            scoring::ScoreEpochs::Instants(instants) => instants
                .iter()
                .copied()
                .map(|epoch| JsValue::from(ClockInstant(epoch)))
                .collect::<js_sys::Array>()
                .into(),
            scoring::ScoreEpochs::ReferenceRecords => JsValue::UNDEFINED,
        }
    }
}

/// Choice of published or fixed SISRE weights.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct SisreWeightSelection(scoring::SisreWeightSelection);

#[wasm_bindgen]
impl SisreWeightSelection {
    /// Compare the stored value.
    pub fn equals(&self, other: &SisreWeightSelection) -> bool {
        self.0 == other.0
    }

    /// Default selection.
    #[wasm_bindgen(js_name = MONTENBRUCK_2018)]
    pub fn montenbruck_2018() -> Self {
        Self(scoring::SisreWeightSelection::default())
    }
    /// Build the core default selection.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self(scoring::SisreWeightSelection::default())
    }

    /// Build the selection from weights.
    pub fn fixed(weights: &SisreWeights) -> Self {
        Self(scoring::SisreWeightSelection::Fixed(weights.0))
    }

    /// Configured weights, if explicitly supplied.
    #[wasm_bindgen(getter, js_name = fixedWeights)]
    pub fn fixed_weights(&self) -> Option<SisreWeights> {
        match self.0 {
            scoring::SisreWeightSelection::Fixed(value) => Some(SisreWeights(value)),
            _ => None,
        }
    }
}

/// Frame assertions and catalog Helmert policy for comparing orbit products.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct Sp3FrameReconciliationOptions(CoreFrameOptions);

#[wasm_bindgen]
impl Sp3FrameReconciliationOptions {
    /// Compare the stored value.
    pub fn equals(&self, other: &Sp3FrameReconciliationOptions) -> bool {
        self.0 == other.0
    }

    /// Build frame reconciliation controls.
    #[wasm_bindgen(constructor)]
    pub fn new(
        #[wasm_bindgen(unchecked_optional_param_type = "string[][] | null")]
        asserted_equivalent_label_sets: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "boolean | null")] helmert: Option<bool>,
    ) -> JsResult<Self> {
        let asserted_equivalent_label_sets: Option<Vec<Vec<String>>> =
            serde_wasm_bindgen::from_value(asserted_equivalent_label_sets)
                .map_err(|error| type_error(&format!("invalid frame label sets: {error}")))?;
        let mut options = CoreFrameOptions::disabled();
        options.asserted_equivalent_label_sets = asserted_equivalent_label_sets
            .unwrap_or_default()
            .into_iter()
            .map(Sp3FrameLabelSet::new)
            .collect();
        options.helmert = helmert.unwrap_or_default();
        Ok(Self(options))
    }

    /// Sets of labels asserted to represent one terrestrial frame.
    #[wasm_bindgen(getter, js_name = assertedEquivalentLabelSets, unchecked_return_type = "string[][]")]
    pub fn asserted_equivalent_label_sets(&self) -> Result<JsValue, JsValue> {
        let sets: Vec<_> = self
            .0
            .asserted_equivalent_label_sets
            .iter()
            .map(|set| set.labels.iter().cloned().collect::<Vec<_>>())
            .collect();
        crate::error::to_plain_js(&sets, "frame label sets")
    }

    /// Whether catalog Helmert transformations are allowed.
    #[wasm_bindgen(getter)]
    pub fn helmert(&self) -> bool {
        self.0.helmert
    }
}

/// Controls for orbit prediction scoring.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct OrbitScoreOptions(scoring::OrbitScoreOptions);

#[wasm_bindgen]
impl OrbitScoreOptions {
    /// Compare the stored value.
    pub fn equals(&self, other: &OrbitScoreOptions) -> bool {
        self.0 == other.0
    }

    /// Configure horizon bins, epoch selection, reference triad, and comparison policies.
    #[wasm_bindgen(constructor)]
    pub fn new(horizon_bin_edges_s: &[f64]) -> Self {
        Self(scoring::OrbitScoreOptions::new(
            horizon_bin_edges_s.to_vec(),
        ))
    }

    /// Set the requested epochs.
    #[wasm_bindgen(js_name = setEpochs)]
    pub fn set_epochs(&mut self, epochs: &ScoreEpochs) {
        self.0.epochs = epochs.0.clone();
    }

    /// Set the reference velocity used to build the orbit frame.
    #[wasm_bindgen(js_name = setTriad)]
    pub fn set_triad(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "TriadVelocity")] value: f64,
    ) -> JsResult<()> {
        self.0.triad = triad_velocity_arg(value)?.into();
        Ok(())
    }

    /// Set the velocity half step in seconds.
    #[wasm_bindgen(js_name = setVelocityHalfStepS)]
    pub fn set_velocity_half_step_s(&mut self, value: f64) {
        self.0.velocity_half_step_s = value;
    }

    /// Set the SISRE weights.
    #[wasm_bindgen(js_name = setSisreWeights)]
    pub fn set_sisre_weights(&mut self, weights: &SisreWeightSelection) {
        self.0.sisre_weights = weights.0;
    }

    /// Set the clock comparison datum.
    #[wasm_bindgen(js_name = setClockDatum)]
    pub fn set_clock_datum(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "ClockDatum")] value: f64,
    ) -> JsResult<()> {
        self.0.clock_datum = clock_datum_arg(value)?.into();
        Ok(())
    }

    /// Set the policy for missing source and reference records.
    #[wasm_bindgen(js_name = setGaps)]
    pub fn set_gaps(
        &mut self,
        #[wasm_bindgen(unchecked_param_type = "GapPolicy")] value: f64,
    ) -> JsResult<()> {
        self.0.gaps = gap_policy_arg(value)?.into();
        Ok(())
    }

    /// Set the allowed terrestrial frame reconciliation.
    #[wasm_bindgen(js_name = setFrameReconciliation)]
    pub fn set_frame_reconciliation(&mut self, frames: &Sp3FrameReconciliationOptions) {
        self.0.frame_reconciliation = frames.0.clone();
    }

    /// Epoch selection.
    #[wasm_bindgen(getter)]
    pub fn epochs(&self) -> ScoreEpochs {
        ScoreEpochs(self.0.epochs.clone())
    }

    /// Prediction horizon bin edges in seconds.
    #[wasm_bindgen(getter, js_name = horizonBinEdgesS)]
    pub fn horizon_bin_edges_s(&self) -> Vec<f64> {
        self.0.horizon_bin_edges_s.clone()
    }

    /// Frame velocity selection.
    #[wasm_bindgen(getter)]
    pub fn triad(&self) -> TriadVelocity {
        self.0.triad.into()
    }

    /// Reference velocity half step in seconds.
    #[wasm_bindgen(getter, js_name = velocityHalfStepS)]
    pub fn velocity_half_step_s(&self) -> f64 {
        self.0.velocity_half_step_s
    }

    /// SISRE weight selection.
    #[wasm_bindgen(getter, js_name = sisreWeights)]
    pub fn sisre_weights(&self) -> SisreWeightSelection {
        SisreWeightSelection(self.0.sisre_weights)
    }

    /// Clock comparison datum.
    #[wasm_bindgen(getter, js_name = clockDatum)]
    pub fn clock_datum(&self) -> ClockDatum {
        self.0.clock_datum.into()
    }

    /// Policy for missing satellite or epoch data.
    #[wasm_bindgen(getter)]
    pub fn gaps(&self) -> GapPolicy {
        self.0.gaps.into()
    }

    /// Frame reconciliation policy.
    #[wasm_bindgen(getter, js_name = frameReconciliation)]
    pub fn frame_reconciliation(&self) -> Sp3FrameReconciliationOptions {
        Sp3FrameReconciliationOptions(self.0.frame_reconciliation.clone())
    }
}

impl Default for ScoreCoverage {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for ScoreEpochs {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for SisreWeightSelection {
    fn default() -> Self {
        Self::new()
    }
}

fn triad_velocity_arg(value: f64) -> JsResult<TriadVelocity> {
    const VALUES: [TriadVelocity; 2] = [TriadVelocity::Inertial, TriadVelocity::EarthFixed];
    VALUES
        .get(index_arg(value, "triadVelocity")?)
        .copied()
        .ok_or_else(|| range_error("triadVelocity must be a TriadVelocity value"))
}

fn clock_datum_arg(value: f64) -> JsResult<ClockDatum> {
    const VALUES: [ClockDatum; 2] = [ClockDatum::EpochSystemMedian, ClockDatum::Unadjusted];
    VALUES
        .get(index_arg(value, "clockDatum")?)
        .copied()
        .ok_or_else(|| range_error("clockDatum must be a ClockDatum value"))
}

fn gap_policy_arg(value: f64) -> JsResult<GapPolicy> {
    const VALUES: [GapPolicy; 2] = [GapPolicy::Count, GapPolicy::Refuse];
    VALUES
        .get(index_arg(value, "gapPolicy")?)
        .copied()
        .ok_or_else(|| range_error("gapPolicy must be a GapPolicy value"))
}

fn orbit_regime_arg(value: f64) -> JsResult<OrbitRegime> {
    const VALUES: [OrbitRegime; 3] = [OrbitRegime::Meo, OrbitRegime::Igso, OrbitRegime::Geo];
    VALUES
        .get(index_arg(value, "orbitRegime")?)
        .copied()
        .ok_or_else(|| range_error("orbitRegime must be a OrbitRegime value"))
}

fn scored_product_arg(value: f64) -> JsResult<ScoredProduct> {
    const VALUES: [ScoredProduct; 2] = [ScoredProduct::Source, ScoredProduct::Reference];
    VALUES
        .get(index_arg(value, "scoredProduct")?)
        .copied()
        .ok_or_else(|| range_error("scoredProduct must be a ScoredProduct value"))
}

fn frame_arg(value: f64) -> JsResult<TerrestrialFrame> {
    const VALUES: [TerrestrialFrame; 4] = [
        TerrestrialFrame::Itrf2020,
        TerrestrialFrame::Itrf2014,
        TerrestrialFrame::Itrf2008,
        TerrestrialFrame::Etrf2020,
    ];
    VALUES
        .get(index_arg(value, "frame")?)
        .copied()
        .ok_or_else(|| range_error("frame must be a TerrestrialFrame value"))
}

pub(crate) fn score_error(error: scoring::OrbitScoreError) -> JsValue {
    use scoring::OrbitScoreError as E;
    let detail = match &error {
        E::InvalidInput { field, reason } => {
            json!({"kind":"invalidInput","field":field,"reason":reason})
        }
        E::TimeSystemMismatch {
            orbit_source,
            reference,
        } => {
            json!({"kind":"timeSystemMismatch","orbitSource":TimeScale::from(*orbit_source) as u32,"reference":TimeScale::from(*reference) as u32})
        }
        E::TimeSystemLabelMismatch {
            orbit_source,
            reference,
        } => {
            json!({"kind":"timeSystemLabelMismatch","orbitSource":orbit_source.label(),"reference":reference.label()})
        }
        E::InstantTimeSystemMismatch {
            field,
            index,
            found,
            expected,
        } => {
            json!({"kind":"instantTimeSystemMismatch","field":field,"index":index,"found":TimeScale::from(*found) as u32,"expected":TimeScale::from(*expected) as u32})
        }
        E::FrameMismatch {
            orbit_source,
            reference,
        } => json!({"kind":"frameMismatch","orbitSource":orbit_source,"reference":reference}),
        E::FrameTransform(source) => {
            let cause = match source {
                FrameCatalogError::InvalidInput { field, reason } => {
                    json!({"kind":"invalidInput","field":field,"reason":reason})
                }
                FrameCatalogError::NoCatalogPath { from, to } => {
                    json!({"kind":"noCatalogPath","from":TerrestrialFrame::from(*from) as u32,"to":TerrestrialFrame::from(*to) as u32})
                }
                FrameCatalogError::SingularTransform {
                    from,
                    to,
                    epoch_year,
                } => {
                    json!({"kind":"singularTransform","from":TerrestrialFrame::from(*from) as u32,"to":TerrestrialFrame::from(*to) as u32,"epochYear":epoch_year})
                }
            };
            json!({"kind":"frameTransform","error":cause})
        }
        E::MissingSatellite { product, satellite } => {
            json!({"kind":"missingSatellite","product":ScoredProduct::from(*product) as u32,"satellite":satellite.to_string()})
        }
        E::MissingEpoch {
            product,
            satellite,
            epoch_index,
            error,
        } => {
            json!({"kind":"missingEpoch","product":ScoredProduct::from(*product) as u32,"satellite":satellite.to_string(),"epochIndex":epoch_index,"error":crate::core_error::CoreErrorDetail::from(error)})
        }
        E::NoPredictionOrigin { satellite } => {
            json!({"kind":"noPredictionOrigin","satellite":satellite.to_string()})
        }
        E::NoSisreWeights { satellite, regime } => {
            json!({"kind":"noSisreWeights","satellite":satellite.to_string(),"regime":OrbitRegime::from(*regime) as u32})
        }
        E::UndeterminedOrbitRegime { satellite } => {
            json!({"kind":"undeterminedOrbitRegime","satellite":satellite.to_string()})
        }
        E::NonFiniteSourceState {
            satellite,
            epoch_index,
        } => {
            json!({"kind":"nonFiniteSourceState","satellite":satellite.to_string(),"epochIndex":epoch_index})
        }
        E::DegenerateReferenceState {
            satellite,
            epoch_index,
        } => {
            json!({"kind":"degenerateReferenceState","satellite":satellite.to_string(),"epochIndex":epoch_index})
        }
        E::NonFiniteResult { satellite } => {
            json!({"kind":"nonFiniteResult","satellite":satellite.to_string()})
        }
        _ => json!({"kind":"unknown"}),
    };
    typed_error("OrbitScoreError", &error.to_string(), detail)
}

/// Default half step for deriving velocity from position samples, in seconds.
#[wasm_bindgen(js_name = DEFAULT_VELOCITY_HALF_STEP_S)]
pub fn default_velocity_half_step_s() -> f64 {
    scoring::DEFAULT_VELOCITY_HALF_STEP_S
}

/// Minimum permitted velocity half step, in seconds.
#[wasm_bindgen(js_name = MIN_VELOCITY_HALF_STEP_S)]
pub fn min_velocity_half_step_s() -> f64 {
    scoring::MIN_VELOCITY_HALF_STEP_S
}

/// Resolve a position difference along the reference radial, transverse and normal axes.
#[wasm_bindgen(js_name = rtnComponents)]
pub fn rtn_components(
    difference_m: &[f64],
    position_m: &[f64],
    velocity_m_s: &[f64],
) -> JsResult<RtnComponents> {
    scoring::rtn_components(
        vec3("differenceM", difference_m)?,
        vec3("positionM", position_m)?,
        vec3("velocityMS", velocity_m_s)?,
    )
    .map(RtnComponents)
    .map_err(score_error)
}

/// Express inertial velocity in Earth-fixed axes, in metres per second.
#[wasm_bindgen(js_name = inertialVelocityFromEarthFixed)]
pub fn inertial_velocity_from_earth_fixed(
    position_ecef_m: &[f64],
    velocity_ecef_m_s: &[f64],
) -> JsResult<Vec<f64>> {
    scoring::inertial_velocity_from_earth_fixed(
        vec3("positionEcefM", position_ecef_m)?,
        vec3("velocityEcefMS", velocity_ecef_m_s)?,
    )
    .map(|value| value.to_vec())
    .map_err(score_error)
}

type SourceResult<T> = Result<T, scoring::OrbitScoreError>;

#[derive(Clone)]
enum Source {
    Sp3 {
        product: Arc<ephemeris::Sp3>,
        cutoff: Option<CoreInstant>,
    },
    Broadcast {
        ephemeris: Arc<ephemeris::BroadcastEphemeris>,
        origin: CoreInstant,
    },
    Trajectory(Arc<scoring::OrbitTrajectory>),
}

impl Source {
    fn with_source<T>(
        &self,
        evaluate: impl FnOnce(&dyn CoreSource) -> SourceResult<T>,
    ) -> SourceResult<T> {
        match self {
            Self::Sp3 { product, cutoff } => {
                let source = match cutoff {
                    Some(cutoff) => scoring::PredictedSp3::with_cutoff(product, *cutoff)?,
                    None => scoring::PredictedSp3::from_prediction_flags(product)?,
                };
                evaluate(&source)
            }
            Self::Broadcast { ephemeris, origin } => {
                evaluate(&scoring::BroadcastOrbit::new(ephemeris, *origin)?)
            }
            Self::Trajectory(trajectory) => evaluate(trajectory.as_ref()),
        }
    }
}

/// Source handle accepted by orbit scoring.
#[wasm_bindgen]
#[derive(Clone)]
pub struct OrbitSource(Source);

impl OrbitSource {
    pub(crate) fn with_source<T>(
        &self,
        evaluate: impl FnOnce(&dyn CoreSource) -> SourceResult<T>,
    ) -> SourceResult<T> {
        self.0.with_source(evaluate)
    }
}

/// SP3 orbit with a prediction origin from record flags or a specified cutoff.
#[wasm_bindgen]
pub struct PredictedSp3(Source);

#[wasm_bindgen]
impl PredictedSp3 {
    /// Read each satellite's prediction origin from the product's flags.
    #[wasm_bindgen(js_name = fromPredictionFlags)]
    pub fn from_prediction_flags(product: &Sp3) -> JsResult<Self> {
        let source = Source::Sp3 {
            product: Arc::new(product.inner.clone()),
            cutoff: None,
        };
        source.with_source(|_| Ok(())).map_err(score_error)?;
        Ok(Self(source))
    }

    /// Use the specified prediction cutoff in the product's time scale.
    #[wasm_bindgen(js_name = withCutoff)]
    pub fn with_cutoff(product: &Sp3, cutoff: &ClockInstant) -> JsResult<Self> {
        let source = Source::Sp3 {
            product: Arc::new(product.inner.clone()),
            cutoff: Some(cutoff.0),
        };
        source.with_source(|_| Ok(())).map_err(score_error)?;
        Ok(Self(source))
    }

    /// Copy the source's parsed SP3 product.
    pub fn product(&self) -> Sp3 {
        match &self.0 {
            Source::Sp3 { product, .. } => Sp3 {
                inner: product.as_ref().clone(),
            },
            _ => unreachable!("SP3 source"),
        }
    }
}

/// Broadcast ephemeris with a caller-supplied prediction origin.
#[wasm_bindgen]
pub struct BroadcastOrbit(Source);

#[wasm_bindgen]
impl BroadcastOrbit {
    /// Construct from a broadcast ephemeris and a GPS-time prediction origin.
    #[wasm_bindgen(constructor)]
    pub fn new(ephemeris: &BroadcastEphemeris, prediction_origin: &ClockInstant) -> JsResult<Self> {
        let source = Source::Broadcast {
            ephemeris: ephemeris.inner.clone(),
            origin: prediction_origin.0,
        };
        source.with_source(|_| Ok(())).map_err(score_error)?;
        Ok(Self(source))
    }
}

/// Caller-supplied Earth-fixed orbit positions and optional clocks.
#[wasm_bindgen]
pub struct OrbitTrajectory(Arc<scoring::OrbitTrajectory>);

#[wasm_bindgen]
impl OrbitTrajectory {
    /// Build from a frame label, a prediction origin, and samples. Sample handles are consumed.
    #[wasm_bindgen(constructor)]
    pub fn new(
        frame_label: String,
        prediction_origin: &ClockInstant,
        samples: Vec<TrajectorySample>,
    ) -> JsResult<Self> {
        scoring::OrbitTrajectory::new(
            frame_label,
            prediction_origin.0,
            samples.into_iter().map(|sample| sample.0),
        )
        .map(Arc::new)
        .map(Self)
        .map_err(score_error)
    }

    /// Specify whether the input clocks include the periodic relativistic correction.
    #[wasm_bindgen(js_name = withClockRelativityIncluded)]
    pub fn with_clock_relativity_included(&self, included: bool) -> Self {
        Self(Arc::new(
            self.0
                .as_ref()
                .clone()
                .with_clock_relativity_included(included),
        ))
    }

    /// Distinct sample epochs in ascending order.
    pub fn epochs(&self) -> Vec<ClockInstant> {
        self.0.epochs().into_iter().map(ClockInstant).collect()
    }

    /// Satellite identifiers in ascending order.
    pub fn satellites(&self) -> Vec<String> {
        self.0
            .satellites()
            .map(|satellite| satellite.to_string())
            .collect()
    }

    /// Compare all trajectory data by value.
    pub fn equals(&self, other: &OrbitTrajectory) -> bool {
        self.0 == other.0
    }
}

macro_rules! source_methods {
    ($type:ident, $source:expr) => {
        #[wasm_bindgen]
        impl $type {
            /// Return a shared handle for orbit scoring.
            #[wasm_bindgen(js_name = asOrbitSource)]
            pub fn as_orbit_source(&self) -> OrbitSource {
                OrbitSource(($source)(self))
            }

            /// Time scale of the source epoch axis.
            #[wasm_bindgen(js_name = timeScale)]
            pub fn time_scale(&self) -> JsResult<TimeScale> {
                ($source)(self)
                    .with_source(|source| Ok(source.time_scale().into()))
                    .map_err(score_error)
            }

            /// Explicit SP3 time system, if available.
            #[wasm_bindgen(js_name = sp3TimeSystem)]
            pub fn sp3_time_system(&self) -> JsResult<Option<String>> {
                ($source)(self)
                    .with_source(|source| {
                        Ok(source
                            .sp3_time_system()
                            .map(|system| system.label().to_owned()))
                    })
                    .map_err(score_error)
            }

            /// Frame of the source positions.
            pub fn frame(&self) -> JsResult<OrbitFrame> {
                ($source)(self)
                    .with_source(|source| Ok(OrbitFrame(source.frame())))
                    .map_err(score_error)
            }

            /// Satellite point represented by the source positions.
            #[wasm_bindgen(js_name = referencePoint)]
            pub fn reference_point(&self) -> JsResult<OrbitReferencePoint> {
                ($source)(self)
                    .with_source(|source| Ok(source.reference_point().into()))
                    .map_err(score_error)
            }

            /// Whether the source clocks include the periodic relativistic correction.
            #[wasm_bindgen(js_name = clockIncludesRelativity)]
            pub fn clock_includes_relativity(&self) -> JsResult<bool> {
                ($source)(self)
                    .with_source(|source| Ok(source.clock_includes_relativity()))
                    .map_err(score_error)
            }

            /// State at seconds from J2000 in the source time scale.
            #[wasm_bindgen(js_name = stateAtJ2000S)]
            pub fn state_at_j2000_s(
                &self,
                satellite: &str,
                t_j2000_s: f64,
            ) -> JsResult<OrbitSourceState> {
                let satellite = parse_sat(satellite)?;
                ($source)(self)
                    .with_source(|source| Ok(source.state_at_j2000_s(satellite, t_j2000_s)))
                    .map_err(score_error)?
                    .map(OrbitSourceState)
                    .map_err(|error| crate::core_error::core_error_js(&error))
            }

            /// Prediction origin in seconds from J2000 in the source time scale.
            #[wasm_bindgen(js_name = predictionOriginJ2000S)]
            pub fn prediction_origin_j2000_s(&self, satellite: &str) -> JsResult<Option<f64>> {
                let satellite = parse_sat(satellite)?;
                ($source)(self)
                    .with_source(|source| Ok(source.prediction_origin_j2000_s(satellite)))
                    .map_err(score_error)
            }
        }
    };
}

source_methods!(PredictedSp3, |source: &PredictedSp3| source.0.clone());
source_methods!(BroadcastOrbit, |source: &BroadcastOrbit| source.0.clone());
source_methods!(OrbitTrajectory, |source: &OrbitTrajectory| {
    Source::Trajectory(source.0.clone())
});

/// Score orbit and clock predictions against a precise reference SP3 product.
#[wasm_bindgen(js_name = scoreOrbit)]
pub fn score_orbit(
    source: &OrbitSource,
    reference: &Sp3,
    satellites: Vec<String>,
    options: &OrbitScoreOptions,
) -> JsResult<OrbitScoreReport> {
    let satellites = satellites
        .iter()
        .map(|satellite| parse_sat(satellite))
        .collect::<Result<Vec<_>, _>>()?;
    source
        .0
        .with_source(|source| {
            scoring::score_orbit(source, &reference.inner, &satellites, &options.0)
        })
        .map(OrbitScoreReport)
        .map_err(score_error)
}

#[wasm_bindgen(typescript_custom_section)]
const SCORE_ERROR_TYPES: &str = r#"
declare const orbitSourceHandle: unique symbol;

/** Opaque handle for an Earth-fixed scoring source. */
export interface OrbitSource {
    readonly [orbitSourceHandle]: never;
}

/** Detail of a terrestrial frame catalog error during scoring. */
export type OrbitScoreFrameError =
  | { kind: "invalidInput"; field: string; reason: string }
  | { kind: "noCatalogPath"; from: TerrestrialFrame; to: TerrestrialFrame }
  | { kind: "singularTransform"; from: TerrestrialFrame; to: TerrestrialFrame; epochYear: number };

/** Orbit scoring input, coverage or evaluation failure. */
export type OrbitScoreErrorDetail =
  | { kind: "invalidInput"; field: string; reason: string }
  | { kind: "timeSystemMismatch"; orbitSource: TimeScale; reference: TimeScale }
  | { kind: "timeSystemLabelMismatch" | "frameMismatch"; orbitSource: string; reference: string }
  | { kind: "instantTimeSystemMismatch"; field: string; index: number; found: TimeScale; expected: TimeScale }
  | { kind: "frameTransform"; error: OrbitScoreFrameError }
  | { kind: "missingSatellite"; product: ScoredProduct; satellite: string }
  | { kind: "missingEpoch"; product: ScoredProduct; satellite: string; epochIndex: number; error: CoreErrorDetail }
  | { kind: "noPredictionOrigin" | "undeterminedOrbitRegime" | "nonFiniteResult"; satellite: string }
  | { kind: "noSisreWeights"; satellite: string; regime: OrbitRegime }
  | { kind: "nonFiniteSourceState" | "degenerateReferenceState"; satellite: string; epochIndex: number }
  | { kind: "unknown" };

/** Error thrown by scoring and orbit source constructors. */
export interface OrbitScoreError extends Error {
  readonly name: "OrbitScoreError";
  readonly kind: OrbitScoreErrorDetail["kind"];
  readonly detail: OrbitScoreErrorDetail;
}
"#;
