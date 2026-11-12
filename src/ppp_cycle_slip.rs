//! Precise-positioning cycle-slip detection and dual-frequency preparation.

use serde::{Deserialize, Serialize};
use wasm_bindgen::{prelude::*, JsCast};

use sidereon_core::carrier_phase::{CarrierPhaseError, CycleSlipOptions, SlipReason};
use sidereon_core::combinations::IonosphereFreeError;
use sidereon_core::precise_positioning::cycle_slip::{
    detect_cycle_slips as core_detect, geometry_free_m as core_geometry_free,
    melbourne_wubbena_cycles as core_mw, update_geometry_free as core_update_gf,
    update_melbourne_wubbena as core_update_mw, CycleSlipConfig, CycleSlipConfigError,
    CycleSlipError, CycleSlipFlagEpoch, GeometryFreeUpdate, MelbourneWubbenaUpdate,
    RunningMeanVariance, SatelliteCycleSlipState, DEFAULT_MINIMUM_ARC_LENGTH,
    DEFAULT_RUNNING_STATISTIC_K_FACTOR,
};
use sidereon_core::precise_positioning::{
    prepare_widelane_fixed_epochs as core_prepare, split_float_cycle_slip_epochs as core_split,
    CycleSlipPolicy, DualFrequencyEpoch, DualFrequencyObservation, FloatCycleSlipEpoch,
    FloatCycleSlipObservation, FloatCycleSlipTaggedEpoch, WideLanePrepError, WideLanePrepOptions,
    WideLanePrepResult,
};

use crate::error::type_error;
use crate::frames::ExactEpochValue;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ObservationInput {
    satellite_id: String,
    ambiguity_id: String,
    p1_m: f64,
    p2_m: f64,
    phi1_cyc: f64,
    phi2_cyc: f64,
    f1_hz: f64,
    f2_hz: f64,
    lli1: Option<i64>,
    lli2: Option<i64>,
}
impl From<ObservationInput> for DualFrequencyObservation {
    fn from(v: ObservationInput) -> Self {
        Self {
            satellite_id: v.satellite_id,
            ambiguity_id: v.ambiguity_id,
            p1_m: v.p1_m,
            p2_m: v.p2_m,
            phi1_cyc: v.phi1_cyc,
            phi2_cyc: v.phi2_cyc,
            f1_hz: v.f1_hz,
            f2_hz: v.f2_hz,
            lli1: v.lli1,
            lli2: v.lli2,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EpochInput {
    gap_time_s: Option<f64>,
    observations: Vec<ObservationInput>,
}

fn from_js<T: for<'de> Deserialize<'de>>(value: JsValue, name: &str) -> Result<T, JsValue> {
    serde_wasm_bindgen::from_value(value).map_err(|e| type_error(&format!("invalid {name}: {e}")))
}
fn observation(value: JsValue) -> Result<DualFrequencyObservation, JsValue> {
    Ok(from_js::<ObservationInput>(value, "PPP dual-frequency observation")?.into())
}

fn epochs(value: JsValue) -> Result<Vec<DualFrequencyEpoch>, JsValue> {
    let array = value
        .dyn_into::<js_sys::Array>()
        .map_err(|_| type_error("PPP dual-frequency epochs must be an array"))?;
    array
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let input: EpochInput =
                from_js(value.clone(), &format!("PPP dual-frequency epoch {index}"))?;
            let exact = js_sys::Reflect::get(&value, &JsValue::from_str("gapEpoch"))?;
            Ok(DualFrequencyEpoch {
                gap_time_s: input.gap_time_s,
                gap_epoch: ExactEpochValue::from_js(&exact, "gapEpoch")?,
                observations: input.observations.into_iter().map(Into::into).collect(),
            })
        })
        .collect()
}

fn config_kind(error: CycleSlipConfigError) -> &'static str {
    match error {
        CycleSlipConfigError::InvalidMelbourneWubbenaThreshold => {
            "INVALID_MELBOURNE_WUBBENA_THRESHOLD"
        }
        CycleSlipConfigError::InvalidGeometryFreeThreshold => "INVALID_GEOMETRY_FREE_THRESHOLD",
        CycleSlipConfigError::InvalidMinimumArcLength => "INVALID_MINIMUM_ARC_LENGTH",
        CycleSlipConfigError::InvalidRunningStatisticKFactor => {
            "INVALID_RUNNING_STATISTIC_K_FACTOR"
        }
        CycleSlipConfigError::InvalidMaximumGap => "INVALID_MAXIMUM_GAP",
    }
}
fn cycle_error(error: CycleSlipError) -> JsValue {
    let detail = match error {
        CycleSlipError::InvalidConfig(inner) => {
            serde_json::json!({"family":"CycleSlipError","kind":"INVALID_CONFIG","configKind":config_kind(inner),"reason":inner.to_string(),"message":error.to_string()})
        }
        CycleSlipError::NonFiniteObservation => {
            serde_json::json!({"family":"CycleSlipError","kind":"NON_FINITE_OBSERVATION","message":error.to_string()})
        }
        CycleSlipError::NonFiniteEpochTime => {
            serde_json::json!({"family":"CycleSlipError","kind":"NON_FINITE_EPOCH_TIME","message":error.to_string()})
        }
        CycleSlipError::EpochsNotOrdered => {
            serde_json::json!({"family":"CycleSlipError","kind":"EPOCHS_NOT_ORDERED","message":error.to_string()})
        }
        CycleSlipError::InvalidFrequency => {
            serde_json::json!({"family":"CycleSlipError","kind":"INVALID_FREQUENCY","message":error.to_string()})
        }
        CycleSlipError::EqualFrequencies => {
            serde_json::json!({"family":"CycleSlipError","kind":"EQUAL_FREQUENCIES","message":error.to_string()})
        }
    };
    structured_range_error(&error.to_string(), detail)
}
fn structured_range_error(message: &str, detail: serde_json::Value) -> JsValue {
    let error = js_sys::RangeError::new(message);
    let detail = detail
        .serialize(&serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true))
        .unwrap_or(JsValue::UNDEFINED);
    let _ = js_sys::Reflect::set(error.as_ref(), &JsValue::from_str("details"), &detail);
    error.into()
}

/// Core cycle-slip detector configuration.
#[wasm_bindgen]
#[derive(Clone)]
pub struct PppCycleSlipConfig {
    inner: CycleSlipConfig,
}
#[wasm_bindgen]
impl PppCycleSlipConfig {
    #[wasm_bindgen(constructor)]
    pub fn new(
        melbourne_wubbena_threshold_cycles: Option<f64>,
        geometry_free_threshold_m: Option<f64>,
        minimum_arc_length: Option<usize>,
        running_statistic_k_factor: Option<f64>,
        maximum_gap_s: Option<f64>,
    ) -> Self {
        let mut inner = CycleSlipConfig::default();
        if let Some(v) = melbourne_wubbena_threshold_cycles {
            inner.melbourne_wubbena_threshold_cycles = v;
        }
        if let Some(v) = geometry_free_threshold_m {
            inner.geometry_free_threshold_m = v;
        }
        if let Some(v) = minimum_arc_length {
            inner.minimum_arc_length = v;
        }
        if let Some(v) = running_statistic_k_factor {
            inner.running_statistic_k_factor = v;
        }
        if let Some(v) = maximum_gap_s {
            inner.maximum_gap_s = v;
        }
        Self { inner }
    }
    #[wasm_bindgen(js_name = default)]
    pub fn default_js() -> Self {
        Self {
            inner: CycleSlipConfig::default(),
        }
    }
    #[wasm_bindgen(getter, js_name = melbourneWubbenaThresholdCycles)]
    pub fn mw_threshold(&self) -> f64 {
        self.inner.melbourne_wubbena_threshold_cycles
    }
    #[wasm_bindgen(getter, js_name = geometryFreeThresholdM)]
    pub fn gf_threshold(&self) -> f64 {
        self.inner.geometry_free_threshold_m
    }
    #[wasm_bindgen(getter, js_name = minimumArcLength)]
    pub fn minimum_arc_length(&self) -> usize {
        self.inner.minimum_arc_length
    }
    #[wasm_bindgen(getter, js_name = runningStatisticKFactor)]
    pub fn k_factor(&self) -> f64 {
        self.inner.running_statistic_k_factor
    }
    #[wasm_bindgen(getter, js_name = maximumGapS)]
    pub fn maximum_gap_s(&self) -> f64 {
        self.inner.maximum_gap_s
    }
    pub fn validate(&self) -> Result<(), JsValue> {
        self.inner.validate().map_err(|e| cycle_error(e.into()))
    }
    pub fn equals(&self, other: &PppCycleSlipConfig) -> bool {
        self.inner == other.inner
    }
}
impl PppCycleSlipConfig {
    fn core(value: Option<PppCycleSlipConfig>) -> CycleSlipConfig {
        value.map_or_else(CycleSlipConfig::default, |v| v.inner)
    }
}

/// Mutable core running mean/variance statistic.
#[wasm_bindgen]
#[derive(Clone)]
pub struct PppRunningMeanVariance {
    inner: RunningMeanVariance,
}
impl Default for PppRunningMeanVariance {
    fn default() -> Self {
        Self::new()
    }
}
#[wasm_bindgen]
impl PppRunningMeanVariance {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: RunningMeanVariance::new(),
        }
    }
    #[wasm_bindgen(js_name = default)]
    pub fn default_js() -> Self {
        Self::new()
    }
    #[wasm_bindgen(getter, js_name = sampleCount)]
    pub fn sample_count(&self) -> usize {
        self.inner.sample_count
    }
    #[wasm_bindgen(getter)]
    pub fn mean(&self) -> f64 {
        self.inner.mean
    }
    #[wasm_bindgen(getter)]
    pub fn variance(&self) -> f64 {
        self.inner.variance
    }
    pub fn push(&mut self, value: f64) {
        self.inner.push(value)
    }
    #[wasm_bindgen(js_name = standardDeviation)]
    pub fn standard_deviation(&self) -> Option<f64> {
        self.inner.standard_deviation()
    }
    pub fn equals(&self, other: &PppRunningMeanVariance) -> bool {
        self.inner == other.inner
    }
}

/// Mutable detector state for one satellite ambiguity arc.
#[wasm_bindgen]
#[derive(Clone)]
pub struct PppSatelliteCycleSlipState {
    inner: SatelliteCycleSlipState,
}
impl Default for PppSatelliteCycleSlipState {
    fn default() -> Self {
        Self::new()
    }
}
#[wasm_bindgen]
impl PppSatelliteCycleSlipState {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: SatelliteCycleSlipState::new(),
        }
    }
    #[wasm_bindgen(js_name = default)]
    pub fn default_js() -> Self {
        Self::new()
    }
    #[wasm_bindgen(getter, js_name = previousEpochTimeS)]
    pub fn previous_epoch_time_s(&self) -> Option<f64> {
        self.inner.previous_epoch_time_s
    }
    #[wasm_bindgen(getter, js_name = previousEpoch)]
    pub fn previous_epoch(&self) -> Option<ExactEpochValue> {
        self.inner.previous_epoch.map(ExactEpochValue::from_core)
    }
    #[wasm_bindgen(getter, js_name = previousMelbourneWubbenaCycles)]
    pub fn previous_mw(&self) -> Option<f64> {
        self.inner.previous_melbourne_wubbena_cycles
    }
    #[wasm_bindgen(getter, js_name = melbourneWubbena)]
    pub fn mw(&self) -> PppRunningMeanVariance {
        PppRunningMeanVariance {
            inner: self.inner.melbourne_wubbena,
        }
    }
    #[wasm_bindgen(getter, js_name = previousGeometryFreeM)]
    pub fn previous_gf(&self) -> Option<f64> {
        self.inner.previous_geometry_free_m
    }
    #[wasm_bindgen(js_name = resetArc)]
    pub fn reset_arc(&mut self) {
        self.inner.reset_arc()
    }
    pub fn equals(&self, other: &PppSatelliteCycleSlipState) -> bool {
        self.inner == other.inner
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MwOutput {
    melbourne_wubbena_cycles: f64,
    slip: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GfOutput {
    geometry_free_m: f64,
    slip: bool,
    reset: bool,
}
fn mw_output(v: MelbourneWubbenaUpdate) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(&MwOutput {
        melbourne_wubbena_cycles: v.melbourne_wubbena_cycles,
        slip: v.slip,
    })
    .map_err(|e| type_error(&e.to_string()))
}
fn gf_output(v: GeometryFreeUpdate) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(&GfOutput {
        geometry_free_m: v.geometry_free_m,
        slip: v.slip,
        reset: v.reset,
    })
    .map_err(|e| type_error(&e.to_string()))
}

#[wasm_bindgen(js_name=pppDefaultMinimumArcLength)]
pub fn default_minimum_arc_length() -> usize {
    DEFAULT_MINIMUM_ARC_LENGTH
}
#[wasm_bindgen(js_name=pppDefaultRunningStatisticKFactor)]
pub fn default_running_statistic_k_factor() -> f64 {
    DEFAULT_RUNNING_STATISTIC_K_FACTOR
}
#[wasm_bindgen(js_name=pppMelbourneWubbenaCycles)]
pub fn melbourne_wubbena(
    #[wasm_bindgen(unchecked_param_type = "PppDualFrequencyObservation")] value: JsValue,
) -> Result<f64, JsValue> {
    core_mw(&observation(value)?).map_err(cycle_error)
}
#[wasm_bindgen(js_name=pppGeometryFreeM)]
pub fn geometry_free(
    #[wasm_bindgen(unchecked_param_type = "PppDualFrequencyObservation")] value: JsValue,
) -> Result<f64, JsValue> {
    core_geometry_free(&observation(value)?).map_err(cycle_error)
}
#[wasm_bindgen(js_name=pppUpdateMelbourneWubbena, unchecked_return_type="PppMelbourneWubbenaUpdate")]
pub fn update_mw(
    state: &mut PppSatelliteCycleSlipState,
    #[wasm_bindgen(unchecked_param_type = "PppDualFrequencyObservation")] value: JsValue,
    config: Option<PppCycleSlipConfig>,
) -> Result<JsValue, JsValue> {
    mw_output(
        core_update_mw(
            &mut state.inner,
            &observation(value)?,
            PppCycleSlipConfig::core(config),
        )
        .map_err(cycle_error)?,
    )
}
#[wasm_bindgen(js_name=pppUpdateGeometryFree, unchecked_return_type="PppGeometryFreeUpdate")]
pub fn update_gf(
    state: &mut PppSatelliteCycleSlipState,
    #[wasm_bindgen(unchecked_param_type = "PppDualFrequencyObservation")] value: JsValue,
    epoch_time_s: Option<f64>,
    epoch: Option<ExactEpochValue>,
    config: Option<PppCycleSlipConfig>,
) -> Result<JsValue, JsValue> {
    gf_output(
        core_update_gf(
            &mut state.inner,
            &observation(value)?,
            epoch_time_s,
            epoch.map(|v| v.core()),
            PppCycleSlipConfig::core(config),
        )
        .map_err(cycle_error)?,
    )
}

fn reason(v: SlipReason) -> &'static str {
    match v {
        SlipReason::Lli => "lli",
        SlipReason::DataGap => "dataGap",
        SlipReason::GeometryFree => "geometryFree",
        SlipReason::MelbourneWubbena => "melbourneWubbena",
    }
}

/// One core cycle-slip detection epoch.
#[wasm_bindgen]
#[derive(Clone)]
pub struct PppCycleSlipFlagEpoch {
    inner: CycleSlipFlagEpoch,
}
#[wasm_bindgen]
impl PppCycleSlipFlagEpoch {
    #[wasm_bindgen(getter,js_name=gapTimeS)]
    pub fn gap_time_s(&self) -> Option<f64> {
        self.inner.gap_time_s
    }
    #[wasm_bindgen(getter,js_name=gapEpoch)]
    pub fn gap_epoch(&self) -> Option<ExactEpochValue> {
        self.inner.gap_epoch.map(ExactEpochValue::from_core)
    }
    #[wasm_bindgen(getter, unchecked_return_type = "PppCycleSlipFlagObservation[]")]
    pub fn observations(&self) -> Result<JsValue, JsValue> {
        let rows=self.inner.observations.iter().map(|v|serde_json::json!({"satelliteId":v.satellite_id,"slip":v.slip,"reasons":v.reasons.iter().copied().map(reason).collect::<Vec<_>>() })).collect::<Vec<_>>();
        rows.serialize(&serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true))
            .map_err(|e| type_error(&e.to_string()))
    }
    pub fn equals(&self, other: &PppCycleSlipFlagEpoch) -> bool {
        self.inner == other.inner
    }
}
#[wasm_bindgen(js_name=pppDetectCycleSlips, unchecked_return_type="PppCycleSlipFlagEpoch[]")]
pub fn detect(
    #[wasm_bindgen(unchecked_param_type = "PppDualFrequencyEpoch[]")] value: JsValue,
    config: Option<PppCycleSlipConfig>,
) -> Result<js_sys::Array, JsValue> {
    Ok(
        core_detect(&epochs(value)?, PppCycleSlipConfig::core(config))
            .map_err(cycle_error)?
            .into_iter()
            .map(|inner| JsValue::from(PppCycleSlipFlagEpoch { inner }))
            .collect(),
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CycleOptionsInput {
    gf_threshold_m: Option<f64>,
    mw_threshold_cycles: Option<f64>,
    min_arc_gap_s: Option<f64>,
}
fn cycle_options(value: JsValue) -> Result<CycleSlipOptions, JsValue> {
    if value.is_null() || value.is_undefined() {
        return Ok(CycleSlipOptions::default());
    }
    let v: CycleOptionsInput = from_js(value, "PPP cycle-slip options")?;
    let mut out = CycleSlipOptions::default();
    if let Some(x) = v.gf_threshold_m {
        out.gf_threshold_m = x
    }
    if let Some(x) = v.mw_threshold_cycles {
        out.mw_threshold_cycles = x
    }
    if let Some(x) = v.min_arc_gap_s {
        out.min_arc_gap_s = x
    }
    Ok(out)
}
fn policy(value: &str) -> Result<CycleSlipPolicy, JsValue> {
    match value {
        "error" => Ok(CycleSlipPolicy::Error),
        "dropSatellite" => Ok(CycleSlipPolicy::DropSatellite),
        "splitArc" => Ok(CycleSlipPolicy::SplitArc),
        _ => Err(type_error(
            "cycleSlipPolicy must be error, dropSatellite, or splitArc",
        )),
    }
}

fn prep_error(error: WideLanePrepError) -> JsValue {
    // WideLanePrepError does not implement Display yet; retain its Debug text
    // only as the human-readable message, never as a discriminator or field.
    let detail = match &error {
        WideLanePrepError::CycleSlipDetected {
            satellite_id,
            epoch_index,
            reasons,
        } => {
            serde_json::json!({"family":"WideLanePrepError","kind":"CYCLE_SLIP_DETECTED","satelliteId":satellite_id,"epochIndex":epoch_index,"reasons":reasons.iter().copied().map(reason).collect::<Vec<_>>(),"message":format!("{error:?}")})
        }
        WideLanePrepError::WideLaneFailed {
            ambiguity_id,
            reason,
        } => {
            serde_json::json!({"family":"WideLanePrepError","kind":"WIDE_LANE_FAILED","ambiguityId":ambiguity_id,"cause":carrier_phase_error_detail(*reason),"message":format!("{error:?}")})
        }
        WideLanePrepError::TooFewWideLaneEpochs {
            ambiguity_id,
            count,
            minimum,
        } => {
            serde_json::json!({"family":"WideLanePrepError","kind":"TOO_FEW_WIDE_LANE_EPOCHS","ambiguityId":ambiguity_id,"count":count,"minimum":minimum,"message":format!("{error:?}")})
        }
        WideLanePrepError::WideLaneNotInteger {
            ambiguity_id,
            mean_cycles,
            fixed_cycles,
        } => {
            serde_json::json!({"family":"WideLanePrepError","kind":"WIDE_LANE_NOT_INTEGER","ambiguityId":ambiguity_id,"meanCycles":mean_cycles,"fixedCycles":fixed_cycles,"message":format!("{error:?}")})
        }
        WideLanePrepError::MissingWideLaneAmbiguity(id) => {
            serde_json::json!({"family":"WideLanePrepError","kind":"MISSING_WIDE_LANE_AMBIGUITY","ambiguityId":id,"message":format!("{error:?}")})
        }
        WideLanePrepError::InconsistentFrequencies(id) => {
            serde_json::json!({"family":"WideLanePrepError","kind":"INCONSISTENT_FREQUENCIES","ambiguityId":id,"message":format!("{error:?}")})
        }
        WideLanePrepError::IonosphereFreeFailed {
            satellite_id,
            reason,
        } => {
            serde_json::json!({"family":"WideLanePrepError","kind":"IONOSPHERE_FREE_FAILED","satelliteId":satellite_id,"cause":ionosphere_free_error_detail(reason),"message":format!("{error:?}")})
        }
    };
    structured_range_error("PPP wide-lane preparation failed", detail)
}

fn carrier_phase_error_detail(error: CarrierPhaseError) -> serde_json::Value {
    let kind = match error {
        CarrierPhaseError::EqualFrequencies => "EQUAL_FREQUENCIES",
        CarrierPhaseError::InvalidFrequency => "INVALID_FREQUENCY",
        CarrierPhaseError::InvalidObservation => "INVALID_OBSERVATION",
        CarrierPhaseError::InvalidThreshold => "INVALID_THRESHOLD",
    };
    serde_json::json!({"family":"CarrierPhaseError","kind":kind,"message":error.to_string()})
}

fn ionosphere_free_error_detail(error: &IonosphereFreeError) -> serde_json::Value {
    match error {
        IonosphereFreeError::UnknownSystem(system) => {
            serde_json::json!({"family":"IonosphereFreeError","kind":"UNKNOWN_SYSTEM","system":system.to_string(),"message":error.to_string()})
        }
        IonosphereFreeError::UnknownBand { system, band } => {
            serde_json::json!({"family":"IonosphereFreeError","kind":"UNKNOWN_BAND","system":system.to_string(),"band":band,"message":error.to_string()})
        }
        IonosphereFreeError::EqualFrequencies => {
            serde_json::json!({"family":"IonosphereFreeError","kind":"EQUAL_FREQUENCIES","message":error.to_string()})
        }
        IonosphereFreeError::InvalidFrequency => {
            serde_json::json!({"family":"IonosphereFreeError","kind":"INVALID_FREQUENCY","message":error.to_string()})
        }
        IonosphereFreeError::InvalidObservation => {
            serde_json::json!({"family":"IonosphereFreeError","kind":"INVALID_OBSERVATION","message":error.to_string()})
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PrepOutput {
    epochs: Vec<PreparedEpochOutput>,
    wavelengths_m: std::collections::BTreeMap<String, f64>,
    offsets_m: std::collections::BTreeMap<String, f64>,
    wide_lane_cycles: std::collections::BTreeMap<String, i64>,
    dropped_sats: Vec<String>,
    split_arcs: Vec<SplitOutput>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreparedEpochOutput {
    epoch_index: usize,
    observations: Vec<PreparedObservationOutput>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreparedObservationOutput {
    satellite_id: String,
    ambiguity_id: String,
    code_m: f64,
    phase_m: f64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SplitOutput {
    satellite_id: String,
    ambiguity_id: String,
    start_epoch_index: usize,
    end_epoch_index: usize,
    n_epochs: usize,
}
impl From<&WideLanePrepResult> for PrepOutput {
    fn from(v: &WideLanePrepResult) -> Self {
        Self {
            epochs: v
                .epochs
                .iter()
                .map(|e| PreparedEpochOutput {
                    epoch_index: e.epoch_index,
                    observations: e
                        .observations
                        .iter()
                        .map(|o| PreparedObservationOutput {
                            satellite_id: o.satellite_id.clone(),
                            ambiguity_id: o.ambiguity_id.clone(),
                            code_m: o.code_m,
                            phase_m: o.phase_m,
                        })
                        .collect(),
                })
                .collect(),
            wavelengths_m: v.wavelengths_m.clone(),
            offsets_m: v.offsets_m.clone(),
            wide_lane_cycles: v.wide_lane_cycles.clone(),
            dropped_sats: v.dropped_sats.clone(),
            split_arcs: v
                .split_arcs
                .iter()
                .map(|a| SplitOutput {
                    satellite_id: a.satellite_id.clone(),
                    ambiguity_id: a.ambiguity_id.clone(),
                    start_epoch_index: a.start_epoch_index,
                    end_epoch_index: a.end_epoch_index,
                    n_epochs: a.n_epochs,
                })
                .collect(),
        }
    }
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct PppWideLanePrepResult {
    inner: WideLanePrepResult,
}
#[wasm_bindgen]
impl PppWideLanePrepResult {
    pub fn equals(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
    #[wasm_bindgen(js_name=toJSON,unchecked_return_type="PppWideLanePrepResultValue")]
    pub fn to_json(&self) -> Result<JsValue, JsValue> {
        PrepOutput::from(&self.inner)
            .serialize(
                &serde_wasm_bindgen::Serializer::new()
                    .serialize_maps_as_objects(true)
                    .serialize_large_number_types_as_bigints(true),
            )
            .map_err(|e| type_error(&e.to_string()))
    }
}

#[wasm_bindgen(js_name=preparePppWidelaneFixedEpochs)]
pub fn prepare(
    #[wasm_bindgen(unchecked_param_type = "PppDualFrequencyEpoch[]")] value: JsValue,
    min_epochs: usize,
    tolerance_cycles: f64,
    cycle_slip_policy: String,
    #[wasm_bindgen(unchecked_param_type = "PppCycleSlipOptions | null | undefined")]
    options: JsValue,
) -> Result<PppWideLanePrepResult, JsValue> {
    core_prepare(
        &epochs(value)?,
        WideLanePrepOptions::new(min_epochs, tolerance_cycles),
        policy(&cycle_slip_policy)?,
        cycle_options(options)?,
    )
    .map(|inner| PppWideLanePrepResult { inner })
    .map_err(prep_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FloatObsInput {
    satellite_id: String,
    ambiguity_id: String,
    raw: Option<ObservationInput>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FloatEpochInput {
    gap_time_s: Option<f64>,
    observations: Vec<FloatObsInput>,
}
fn float_epochs(value: JsValue) -> Result<Vec<FloatCycleSlipEpoch>, JsValue> {
    let array = value
        .dyn_into::<js_sys::Array>()
        .map_err(|_| type_error("PPP float cycle-slip epochs must be an array"))?;
    array
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let input: FloatEpochInput =
                from_js(v.clone(), &format!("PPP float cycle-slip epoch {i}"))?;
            let exact = js_sys::Reflect::get(&v, &JsValue::from_str("gapEpoch"))?;
            Ok(FloatCycleSlipEpoch {
                gap_time_s: input.gap_time_s,
                gap_epoch: ExactEpochValue::from_js(&exact, "gapEpoch")?,
                observations: input
                    .observations
                    .into_iter()
                    .map(|o| FloatCycleSlipObservation {
                        satellite_id: o.satellite_id,
                        ambiguity_id: o.ambiguity_id,
                        raw: o.raw.map(Into::into),
                    })
                    .collect(),
            })
        })
        .collect()
}
fn tagged_output(value: &[FloatCycleSlipTaggedEpoch]) -> Vec<serde_json::Value> {
    value.iter().map(|e|serde_json::json!({"observations":e.observations.iter().map(|o|serde_json::json!({"satelliteId":o.satellite_id,"ambiguityId":o.ambiguity_id})).collect::<Vec<_>>() })).collect()
}
#[wasm_bindgen(js_name=splitPppFloatCycleSlipEpochs,unchecked_return_type="PppFloatCycleSlipTaggedEpoch[]")]
pub fn split(
    #[wasm_bindgen(unchecked_param_type = "PppFloatCycleSlipEpoch[]")] value: JsValue,
    #[wasm_bindgen(unchecked_param_type = "PppCycleSlipOptions | null | undefined")]
    options: JsValue,
) -> Result<JsValue, JsValue> {
    tagged_output(&core_split(&float_epochs(value)?, cycle_options(options)?))
        .serialize(&serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true))
        .map_err(|e| type_error(&e.to_string()))
}

#[wasm_bindgen(typescript_custom_section)]
const TYPES: &'static str = r#"
export interface PppDualFrequencyEpoch { gapTimeS?: number; gapEpoch?: ExactEpoch | null; observations: PppDualFrequencyObservation[]; }
export interface PppMelbourneWubbenaUpdate { melbourneWubbenaCycles: number; slip: boolean; }
export interface PppGeometryFreeUpdate { geometryFreeM: number; slip: boolean; reset: boolean; }
export type PppCycleSlipReason = "lli" | "dataGap" | "geometryFree" | "melbourneWubbena";
export interface PppCycleSlipFlagObservation { satelliteId: string; slip: boolean; reasons: PppCycleSlipReason[]; }
export interface PppCycleSlipOptions { gfThresholdM?: number; mwThresholdCycles?: number; minArcGapS?: number; }
export type PppCycleSlipPolicy = "error" | "dropSatellite" | "splitArc";
export interface PppPreparedFloatObservation { satelliteId: string; ambiguityId: string; codeM: number; phaseM: number; }
export interface PppPreparedFloatEpoch { epochIndex: bigint; observations: PppPreparedFloatObservation[]; }
export interface PppSplitArc { satelliteId: string; ambiguityId: string; startEpochIndex: bigint; endEpochIndex: bigint; nEpochs: bigint; }
export interface PppWideLanePrepResultValue { epochs: PppPreparedFloatEpoch[]; wavelengthsM: Record<string, number>; offsetsM: Record<string, number>; wideLaneCycles: Record<string, bigint>; droppedSats: string[]; splitArcs: PppSplitArc[]; }
export interface PppFloatCycleSlipObservation { satelliteId: string; ambiguityId: string; raw?: PppDualFrequencyObservation | null; }
export interface PppFloatCycleSlipEpoch { gapTimeS?: number; gapEpoch?: ExactEpoch | null; observations: PppFloatCycleSlipObservation[]; }
export interface PppFloatCycleSlipTaggedObservation { satelliteId: string; ambiguityId: string; }
export interface PppFloatCycleSlipTaggedEpoch { observations: PppFloatCycleSlipTaggedObservation[]; }
"#;
