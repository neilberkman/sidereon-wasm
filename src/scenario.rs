//! Deterministic GNSS scenario simulator binding.
//!
//! The scenario schema is owned by `sidereon_core::scenario`; this module only
//! accepts JS values or JSON text, calls the core simulator, and returns arrays
//! plus the term ledger.

use serde::Serialize;
use wasm_bindgen::prelude::*;

use sidereon_core::scenario::{
    simulate_scenario as core_simulate_scenario, Scenario, SyntheticObservableArrays,
    SyntheticObservationSet, SyntheticReceiverTruth, SyntheticTermArrays, DEFAULT_SCENARIO_SEED,
    SCENARIO_ENGINE_VERSION, SCENARIO_SCHEMA_VERSION,
};

use crate::domain_error::scenario_error;
use crate::error::{engine_error, index_arg, type_error};
use crate::rinex_obs::{rinex_obs_write_error, RinexObs};

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| engine_error(format!("failed to serialize scenario result: {e}")))
}

fn parse_scenario_value(value: JsValue) -> Result<Scenario, JsValue> {
    if let Some(text) = value.as_string() {
        return parse_scenario_json_value(&text);
    }
    serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid scenario schema: {e}")))
}

fn parse_scenario_json_value(text: &str) -> Result<Scenario, JsValue> {
    serde_json::from_str(text).map_err(|e| type_error(&format!("invalid scenario JSON: {e}")))
}

fn simulate_value(value: JsValue) -> Result<ScenarioObservationSetJs, JsValue> {
    let scenario = parse_scenario_value(value)?;
    let set = core_simulate_scenario(&scenario).map_err(scenario_error)?;
    Ok(ScenarioObservationSetJs::from(set))
}

fn simulate_json_value(text: &str) -> Result<ScenarioObservationSetJs, JsValue> {
    let scenario = parse_scenario_json_value(text)?;
    let set = core_simulate_scenario(&scenario).map_err(scenario_error)?;
    Ok(ScenarioObservationSetJs::from(set))
}

fn deterministic_bytes(set: &ScenarioObservationSetJs) -> Result<Vec<u8>, JsValue> {
    serde_json::to_vec(set)
        .map_err(|e| engine_error(format!("failed to encode scenario bytes: {e}")))
}

fn hex_u64(value: u64) -> String {
    format!("0x{value:016x}")
}

/// Core scenario schema version accepted by the binding.
#[wasm_bindgen(js_name = scenarioSchemaVersion)]
pub fn scenario_schema_version() -> u32 {
    SCENARIO_SCHEMA_VERSION
}

/// Core scenario engine version string used in deterministic outputs.
#[wasm_bindgen(js_name = scenarioEngineVersion)]
pub fn scenario_engine_version() -> String {
    SCENARIO_ENGINE_VERSION.to_string()
}

/// Default scenario seed as a hexadecimal string.
#[wasm_bindgen(js_name = defaultScenarioSeedHex)]
pub fn default_scenario_seed_hex() -> String {
    hex_u64(DEFAULT_SCENARIO_SEED)
}

/// Simulate a scenario from a JS object or JSON string and return JS arrays.
#[wasm_bindgen(js_name = simulateScenario)]
pub fn simulate_scenario(value: JsValue) -> Result<JsValue, JsValue> {
    to_js(&simulate_value(value)?)
}

/// Simulate a scenario from JSON text and return JS arrays.
#[wasm_bindgen(js_name = simulateScenarioJson)]
pub fn simulate_scenario_json(text: &str) -> Result<JsValue, JsValue> {
    to_js(&simulate_json_value(text)?)
}

/// Simulate a scenario from a JS object or JSON string and return deterministic JSON bytes.
#[wasm_bindgen(js_name = simulateScenarioBytes)]
pub fn simulate_scenario_bytes(value: JsValue) -> Result<Vec<u8>, JsValue> {
    deterministic_bytes(&simulate_value(value)?)
}

/// A simulated scenario held on the engine side, for the exports that work on
/// the whole observation set: the RINEX observation product and text, and the
/// SPP observations of one epoch. Built by [`simulateScenarioSet`].
#[wasm_bindgen]
pub struct ScenarioSimulation {
    inner: SyntheticObservationSet,
}

#[wasm_bindgen]
impl ScenarioSimulation {
    /// The arrays and term ledger `simulateScenario` returns for the same
    /// scenario.
    #[wasm_bindgen(getter)]
    pub fn arrays(&self) -> Result<JsValue, JsValue> {
        to_js(&ScenarioObservationSetJs::from(self.inner.clone()))
    }

    /// The determinism fingerprint of the observation set, as a hexadecimal
    /// string (`"0x..."`).
    #[wasm_bindgen(getter, js_name = determinismFingerprintHex)]
    pub fn determinism_fingerprint_hex(&self) -> String {
        hex_u64(self.inner.determinism_fingerprint())
    }

    /// The synthetic observations as a RINEX observation product.
    #[wasm_bindgen(js_name = toRinexObservationFile)]
    pub fn to_rinex_observation_file(&self) -> RinexObs {
        RinexObs {
            inner: self.inner.to_rinex_observation_file(),
        }
    }

    /// The synthetic observations as RINEX OBS text. Throws a
    /// `RinexObsWriteError` when the product would not read back as itself,
    /// for example a value its column cannot hold exactly.
    #[wasm_bindgen(js_name = toRinexString)]
    pub fn to_rinex_string(&self) -> Result<String, JsValue> {
        self.inner.to_rinex_string().map_err(rinex_obs_write_error)
    }

    /// The SPP observations of epoch `epochIndex`, each `{ satelliteId,
    /// pseudorangeM }`, in the order the set holds them. An index past the
    /// last epoch gives an empty array. Negative, fractional, non-finite, and
    /// out-of-range indices throw a `RangeError` rather than being wrapped.
    #[wasm_bindgen(js_name = sppObservationsForEpoch, unchecked_return_type = "Array<{ satelliteId: string; pseudorangeM: number }>")]
    pub fn spp_observations_for_epoch(&self, epoch_index: f64) -> Result<JsValue, JsValue> {
        let epoch_index = index_arg(epoch_index, "epochIndex")?;
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct SppObservationJs {
            satellite_id: String,
            pseudorange_m: f64,
        }
        let rows: Vec<SppObservationJs> = self
            .inner
            .spp_observations_for_epoch(epoch_index)
            .into_iter()
            .map(|obs| SppObservationJs {
                satellite_id: obs.satellite_id.to_string(),
                pseudorange_m: obs.pseudorange_m,
            })
            .collect();
        to_js(&rows)
    }
}

/// Simulate a scenario from a JS object or JSON string and keep the
/// observation set, for its RINEX and SPP exports.
#[wasm_bindgen(js_name = simulateScenarioSet)]
pub fn simulate_scenario_set(value: JsValue) -> Result<ScenarioSimulation, JsValue> {
    let scenario = parse_scenario_value(value)?;
    let inner = core_simulate_scenario(&scenario).map_err(scenario_error)?;
    Ok(ScenarioSimulation { inner })
}

/// Simulate a scenario from JSON text and return deterministic JSON bytes.
#[wasm_bindgen(js_name = simulateScenarioJsonBytes)]
pub fn simulate_scenario_json_bytes(text: &str) -> Result<Vec<u8>, JsValue> {
    deterministic_bytes(&simulate_json_value(text)?)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScenarioObservationSetJs {
    schema_version: u32,
    engine_version: String,
    seed_hex: String,
    receiver_truth: Vec<SyntheticReceiverTruthJs>,
    observations: SyntheticObservableArraysJs,
    truth_terms: SyntheticTermArraysJs,
    observation_count: usize,
    determinism_fingerprint_hex: String,
}

impl From<SyntheticObservationSet> for ScenarioObservationSetJs {
    fn from(value: SyntheticObservationSet) -> Self {
        let observation_count = value.observation_count();
        let determinism_fingerprint_hex = hex_u64(value.determinism_fingerprint());
        Self {
            schema_version: value.schema_version,
            engine_version: value.engine_version,
            seed_hex: hex_u64(value.seed),
            receiver_truth: value
                .receiver_truth
                .into_iter()
                .map(SyntheticReceiverTruthJs::from)
                .collect(),
            observations: SyntheticObservableArraysJs::from(value.observations),
            truth_terms: SyntheticTermArraysJs::from(value.truth_terms),
            observation_count,
            determinism_fingerprint_hex,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SyntheticReceiverTruthJs {
    t_rx_j2000_s: f64,
    position_ecef_m: [f64; 3],
    velocity_ecef_m_s: [f64; 3],
    clock_m: f64,
    clock_rate_m_s: f64,
}

impl From<SyntheticReceiverTruth> for SyntheticReceiverTruthJs {
    fn from(value: SyntheticReceiverTruth) -> Self {
        Self {
            t_rx_j2000_s: value.t_rx_j2000_s,
            position_ecef_m: value.position_ecef_m,
            velocity_ecef_m_s: value.velocity_ecef_m_s,
            clock_m: value.clock_m,
            clock_rate_m_s: value.clock_rate_m_s,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SyntheticObservableArraysJs {
    epoch_offsets: Vec<usize>,
    epoch_index: Vec<usize>,
    satellite_id: Vec<String>,
    code_observable: Vec<String>,
    phase_observable: Vec<String>,
    doppler_observable: Vec<String>,
    carrier_hz: Vec<f64>,
    pseudorange_m: Vec<f64>,
    carrier_phase_cycles: Vec<f64>,
    doppler_hz: Vec<f64>,
}

impl From<SyntheticObservableArrays> for SyntheticObservableArraysJs {
    fn from(value: SyntheticObservableArrays) -> Self {
        Self {
            epoch_offsets: value.epoch_offsets,
            epoch_index: value.epoch_index,
            satellite_id: value
                .satellite_id
                .into_iter()
                .map(|sat| sat.to_string())
                .collect(),
            code_observable: value.code_observable,
            phase_observable: value.phase_observable,
            doppler_observable: value.doppler_observable,
            carrier_hz: value.carrier_hz,
            pseudorange_m: value.pseudorange_m,
            carrier_phase_cycles: value.carrier_phase_cycles,
            doppler_hz: value.doppler_hz,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SyntheticTermArraysJs {
    geometric_range_m: Vec<f64>,
    satellite_clock_m: Vec<f64>,
    receiver_clock_m: Vec<f64>,
    satellite_clock_error_m: Vec<f64>,
    ionosphere_m: Vec<f64>,
    troposphere_m: Vec<f64>,
    thermal_noise_m: Vec<f64>,
    multipath_m: Vec<f64>,
    quantization_m: Vec<f64>,
    carrier_phase_geometric_cycles: Vec<f64>,
    carrier_phase_receiver_clock_cycles: Vec<f64>,
    carrier_phase_satellite_clock_cycles: Vec<f64>,
    carrier_phase_satellite_clock_error_cycles: Vec<f64>,
    carrier_phase_ionosphere_cycles: Vec<f64>,
    carrier_phase_troposphere_cycles: Vec<f64>,
    carrier_phase_thermal_noise_cycles: Vec<f64>,
    carrier_phase_bias_cycles: Vec<f64>,
    carrier_phase_quantization_cycles: Vec<f64>,
    doppler_satellite_motion_hz: Vec<f64>,
    doppler_receiver_motion_hz: Vec<f64>,
    doppler_satellite_clock_hz: Vec<f64>,
    doppler_receiver_clock_hz: Vec<f64>,
    doppler_satellite_clock_error_hz: Vec<f64>,
    doppler_thermal_noise_hz: Vec<f64>,
    doppler_quantization_hz: Vec<f64>,
}

impl From<SyntheticTermArrays> for SyntheticTermArraysJs {
    fn from(value: SyntheticTermArrays) -> Self {
        Self {
            geometric_range_m: value.geometric_range_m,
            satellite_clock_m: value.satellite_clock_m,
            receiver_clock_m: value.receiver_clock_m,
            satellite_clock_error_m: value.satellite_clock_error_m,
            ionosphere_m: value.ionosphere_m,
            troposphere_m: value.troposphere_m,
            thermal_noise_m: value.thermal_noise_m,
            multipath_m: value.multipath_m,
            quantization_m: value.quantization_m,
            carrier_phase_geometric_cycles: value.carrier_phase_geometric_cycles,
            carrier_phase_receiver_clock_cycles: value.carrier_phase_receiver_clock_cycles,
            carrier_phase_satellite_clock_cycles: value.carrier_phase_satellite_clock_cycles,
            carrier_phase_satellite_clock_error_cycles: value
                .carrier_phase_satellite_clock_error_cycles,
            carrier_phase_ionosphere_cycles: value.carrier_phase_ionosphere_cycles,
            carrier_phase_troposphere_cycles: value.carrier_phase_troposphere_cycles,
            carrier_phase_thermal_noise_cycles: value.carrier_phase_thermal_noise_cycles,
            carrier_phase_bias_cycles: value.carrier_phase_bias_cycles,
            carrier_phase_quantization_cycles: value.carrier_phase_quantization_cycles,
            doppler_satellite_motion_hz: value.doppler_satellite_motion_hz,
            doppler_receiver_motion_hz: value.doppler_receiver_motion_hz,
            doppler_satellite_clock_hz: value.doppler_satellite_clock_hz,
            doppler_receiver_clock_hz: value.doppler_receiver_clock_hz,
            doppler_satellite_clock_error_hz: value.doppler_satellite_clock_error_hz,
            doppler_thermal_noise_hz: value.doppler_thermal_noise_hz,
            doppler_quantization_hz: value.doppler_quantization_hz,
        }
    }
}
