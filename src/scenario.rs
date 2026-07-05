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

use crate::error::{engine_error, type_error};

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
    let set = core_simulate_scenario(&scenario).map_err(engine_error)?;
    Ok(ScenarioObservationSetJs::from(set))
}

fn simulate_json_value(text: &str) -> Result<ScenarioObservationSetJs, JsValue> {
    let scenario = parse_scenario_json_value(text)?;
    let set = core_simulate_scenario(&scenario).map_err(engine_error)?;
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
