//! Snapshot RAIM and fault detection/exclusion for precise float PPP.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::geometry::{LineOfSight, Wgs84Geodetic};
use sidereon_core::precise_positioning::raim::{
    fault_free_accuracy_bounds as core_accuracy_bounds, fde_float_epoch as core_fde,
    global_test as core_global_test, global_test_with_geometry as core_global_with_geometry,
    per_satellite_statistics as core_statistics, protection_levels as core_protection_levels,
    protection_levels_with_weights as core_weighted_levels,
};
use sidereon_core::precise_positioning::{
    FloatResidual, ProtectionLevels, RaimConfig, RaimError, RaimFdeError, RaimFdeResult,
    RaimGeometryRow, RaimIdentification, RaimResult, RaimStatus, SatelliteTestStatistic,
};

use crate::error::{engine_error, type_error};
use crate::ppp::{EpochInput, FloatConfigInput, PppFloatSolution, StateInput};
use crate::sp3::Sp3;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResidualInput {
    epoch_index: usize,
    satellite_id: String,
    ambiguity_id: String,
    code_m: f64,
    phase_m: f64,
    code_weight: f64,
    phase_weight: f64,
}

impl From<ResidualInput> for FloatResidual {
    fn from(v: ResidualInput) -> Self {
        Self {
            epoch_index: v.epoch_index,
            satellite_id: v.satellite_id,
            ambiguity_id: v.ambiguity_id,
            code_m: v.code_m,
            phase_m: v.phase_m,
            code_weight: v.code_weight,
            phase_weight: v.phase_weight,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeometryInput {
    satellite_id: String,
    line_of_sight: [f64; 3],
}
impl From<GeometryInput> for RaimGeometryRow {
    fn from(v: GeometryInput) -> Self {
        Self {
            satellite_id: v.satellite_id,
            line_of_sight: LineOfSight::new(
                v.line_of_sight[0],
                v.line_of_sight[1],
                v.line_of_sight[2],
            ),
        }
    }
}

fn from_js<T: for<'de> Deserialize<'de>>(value: JsValue, name: &str) -> Result<T, JsValue> {
    serde_wasm_bindgen::from_value(value).map_err(|e| type_error(&format!("invalid {name}: {e}")))
}
fn residuals(value: JsValue) -> Result<Vec<FloatResidual>, JsValue> {
    Ok(from_js::<Vec<ResidualInput>>(value, "PPP RAIM residuals")?
        .into_iter()
        .map(Into::into)
        .collect())
}
fn geometry(value: JsValue) -> Result<Vec<RaimGeometryRow>, JsValue> {
    Ok(from_js::<Vec<GeometryInput>>(value, "PPP RAIM geometry")?
        .into_iter()
        .map(Into::into)
        .collect())
}

fn raim_error(error: RaimError) -> JsValue {
    let detail = match error {
        RaimError::InvalidConfig { field, reason } => {
            serde_json::json!({"family":"RaimError","kind":"INVALID_CONFIG","field":field,"reason":reason,"message":error.to_string()})
        }
        RaimError::InvalidResidual { field } => {
            serde_json::json!({"family":"RaimError","kind":"INVALID_RESIDUAL","field":field,"message":error.to_string()})
        }
        RaimError::InvalidGeometry { field, reason } => {
            serde_json::json!({"family":"RaimError","kind":"INVALID_GEOMETRY","field":field,"reason":reason,"message":error.to_string()})
        }
        RaimError::Dop(inner) => {
            serde_json::json!({"family":"RaimError","kind":"DOP","cause":format!("{inner:?}"),"message":error.to_string()})
        }
        RaimError::NumericalProtectionLevel => {
            serde_json::json!({"family":"RaimError","kind":"NUMERICAL_PROTECTION_LEVEL","message":error.to_string()})
        }
        RaimError::UnsupportedNoncentrality => {
            serde_json::json!({"family":"RaimError","kind":"UNSUPPORTED_NONCENTRALITY","message":error.to_string()})
        }
        RaimError::UnsupportedDof { dof, max } => {
            serde_json::json!({"family":"RaimError","kind":"UNSUPPORTED_DOF","dof":dof,"max":max,"message":error.to_string()})
        }
        RaimError::SingularGeometry => {
            serde_json::json!({"family":"RaimError","kind":"SINGULAR_GEOMETRY","message":error.to_string()})
        }
    };
    let exception: js_sys::Error = match error {
        RaimError::InvalidConfig { .. }
        | RaimError::InvalidResidual { .. }
        | RaimError::InvalidGeometry { .. } => js_sys::RangeError::new(&error.to_string()).into(),
        RaimError::Dop(_)
        | RaimError::SingularGeometry
        | RaimError::NumericalProtectionLevel
        | RaimError::UnsupportedNoncentrality
        | RaimError::UnsupportedDof { .. } => js_sys::Error::new(&error.to_string()),
    };
    let value = detail
        .serialize(&serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true))
        .unwrap_or(JsValue::UNDEFINED);
    let _ = js_sys::Reflect::set(exception.as_ref(), &JsValue::from_str("details"), &value);
    exception.into()
}

/// PPP snapshot RAIM configuration backed by the core value.
#[wasm_bindgen]
#[derive(Clone)]
pub struct PppRaimConfig {
    inner: RaimConfig,
}

#[wasm_bindgen]
impl PppRaimConfig {
    #[wasm_bindgen(constructor)]
    pub fn new(
        false_alarm_probability: Option<f64>,
        missed_detection_probability: Option<f64>,
        measurement_sigma_m: Option<f64>,
        chi_square_threshold: Option<f64>,
    ) -> Self {
        let mut inner = RaimConfig::default();
        if let Some(v) = false_alarm_probability {
            inner.false_alarm_probability = v;
        }
        if let Some(v) = missed_detection_probability {
            inner.missed_detection_probability = v;
        }
        if let Some(v) = measurement_sigma_m {
            inner.measurement_sigma_m = v;
        }
        inner.chi_square_threshold = chi_square_threshold;
        Self { inner }
    }
    #[wasm_bindgen(js_name = default)]
    pub fn default_js() -> Self {
        Self {
            inner: RaimConfig::default(),
        }
    }
    #[wasm_bindgen(getter, js_name = falseAlarmProbability)]
    pub fn false_alarm_probability(&self) -> f64 {
        self.inner.false_alarm_probability
    }
    #[wasm_bindgen(getter, js_name = missedDetectionProbability)]
    pub fn missed_detection_probability(&self) -> f64 {
        self.inner.missed_detection_probability
    }
    #[wasm_bindgen(getter, js_name = measurementSigmaM)]
    pub fn measurement_sigma_m(&self) -> f64 {
        self.inner.measurement_sigma_m
    }
    #[wasm_bindgen(getter, js_name = chiSquareThreshold)]
    pub fn chi_square_threshold(&self) -> Option<f64> {
        self.inner.chi_square_threshold
    }
    pub fn equals(&self, other: &PppRaimConfig) -> bool {
        self.inner == other.inner
    }
}
impl PppRaimConfig {
    fn core(value: Option<PppRaimConfig>) -> RaimConfig {
        value.map_or_else(RaimConfig::default, |v| v.inner)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatisticOutput {
    satellite_id: String,
    code: f64,
    phase: f64,
    statistic: f64,
    code_testable: bool,
    phase_testable: bool,
}
impl From<&SatelliteTestStatistic> for StatisticOutput {
    fn from(v: &SatelliteTestStatistic) -> Self {
        Self {
            satellite_id: v.satellite_id.clone(),
            code: v.code,
            phase: v.phase,
            statistic: v.statistic,
            code_testable: v.code_testable,
            phase_testable: v.phase_testable,
        }
    }
}

fn status(value: RaimStatus) -> &'static str {
    match value {
        RaimStatus::Passed => "passed",
        RaimStatus::FaultDetected => "faultDetected",
        RaimStatus::NotEnoughRedundancy => "notEnoughRedundancy",
    }
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct PppRaimResult {
    inner: RaimResult,
}
#[wasm_bindgen]
impl PppRaimResult {
    #[wasm_bindgen(getter)]
    pub fn status(&self) -> String {
        status(self.inner.status).into()
    }
    #[wasm_bindgen(getter)]
    pub fn detected(&self) -> bool {
        self.inner.detected
    }
    #[wasm_bindgen(getter, js_name = testStatistic)]
    pub fn test_statistic(&self) -> f64 {
        self.inner.test_statistic
    }
    #[wasm_bindgen(getter)]
    pub fn threshold(&self) -> Option<f64> {
        self.inner.threshold
    }
    #[wasm_bindgen(getter)]
    pub fn redundancy(&self) -> isize {
        self.inner.redundancy
    }
    #[wasm_bindgen(getter, js_name = satelliteStatistics, unchecked_return_type = "PppSatelliteTestStatistic[]")]
    pub fn satellite_statistics(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(
            &self
                .inner
                .satellite_statistics
                .iter()
                .map(StatisticOutput::from)
                .collect::<Vec<_>>(),
        )
        .map_err(|e| type_error(&e.to_string()))
    }
    #[wasm_bindgen(getter, js_name = mostLikelyFault)]
    pub fn most_likely_fault(&self) -> Option<String> {
        self.inner.most_likely_fault.clone()
    }
    #[wasm_bindgen(getter, js_name = hplM)]
    pub fn hpl_m(&self) -> Option<f64> {
        self.inner.hpl_m
    }
    #[wasm_bindgen(getter, js_name = vplM)]
    pub fn vpl_m(&self) -> Option<f64> {
        self.inner.vpl_m
    }
    pub fn equals(&self, other: &PppRaimResult) -> bool {
        self.inner == other.inner
    }
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct PppRaimIdentification {
    inner: RaimIdentification,
}
#[wasm_bindgen]
impl PppRaimIdentification {
    #[wasm_bindgen(getter, unchecked_return_type = "PppSatelliteTestStatistic[]")]
    pub fn statistics(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(
            &self
                .inner
                .statistics
                .iter()
                .map(StatisticOutput::from)
                .collect::<Vec<_>>(),
        )
        .map_err(|e| type_error(&e.to_string()))
    }
    #[wasm_bindgen(getter, js_name = mostLikelyFault)]
    pub fn most_likely_fault(&self) -> Option<String> {
        self.inner.most_likely_fault.clone()
    }
    pub fn equals(&self, other: &PppRaimIdentification) -> bool {
        self.inner == other.inner
    }
}

#[wasm_bindgen]
#[derive(Clone)]
pub struct PppProtectionLevels {
    inner: ProtectionLevels,
}
#[wasm_bindgen]
impl PppProtectionLevels {
    #[wasm_bindgen(getter, js_name = hplM)]
    pub fn hpl_m(&self) -> f64 {
        self.inner.hpl_m
    }
    #[wasm_bindgen(getter, js_name = vplM)]
    pub fn vpl_m(&self) -> f64 {
        self.inner.vpl_m
    }
    pub fn equals(&self, other: &PppProtectionLevels) -> bool {
        self.inner == other.inner
    }
}

#[wasm_bindgen]
pub struct PppRaimFdeResult {
    inner: RaimFdeResult,
}
#[wasm_bindgen]
impl PppRaimFdeResult {
    #[wasm_bindgen(getter)]
    pub fn solution(&self) -> PppFloatSolution {
        PppFloatSolution {
            inner: self.inner.solution.clone(),
        }
    }
    #[wasm_bindgen(getter)]
    pub fn raim(&self) -> PppRaimResult {
        PppRaimResult {
            inner: self.inner.raim.clone(),
        }
    }
    #[wasm_bindgen(getter, js_name = excludedSats)]
    pub fn excluded_sats(&self) -> Vec<String> {
        self.inner.excluded_sats.clone()
    }
    #[wasm_bindgen(getter, unchecked_return_type = "PppRaimFdeStatus")]
    pub fn status(&self) -> String {
        match self.inner.status {
            sidereon_core::precise_positioning::RaimFdeStatus::Untested => "untested",
            sidereon_core::precise_positioning::RaimFdeStatus::Clean => "clean",
            sidereon_core::precise_positioning::RaimFdeStatus::Restored => "restored",
            sidereon_core::precise_positioning::RaimFdeStatus::CannotExclude => "cannotExclude",
            sidereon_core::precise_positioning::RaimFdeStatus::IntegrityNotRestored => {
                "integrityNotRestored"
            }
        }
        .into()
    }
    pub fn equals(&self, other: &PppRaimFdeResult) -> bool {
        self.inner == other.inner
    }
}

#[wasm_bindgen(js_name = pppRaimGlobalTest)]
pub fn global_test(
    residual_values: JsValue,
    n_states: usize,
    config: Option<PppRaimConfig>,
) -> Result<PppRaimResult, JsValue> {
    core_global_test(
        &residuals(residual_values)?,
        n_states,
        PppRaimConfig::core(config),
    )
    .map(|inner| PppRaimResult { inner })
    .map_err(raim_error)
}
#[wasm_bindgen(js_name = pppRaimGlobalTestWithGeometry)]
pub fn global_test_with_geometry(
    residual_values: JsValue,
    geometry_values: JsValue,
    config: Option<PppRaimConfig>,
) -> Result<PppRaimResult, JsValue> {
    core_global_with_geometry(
        &residuals(residual_values)?,
        &geometry(geometry_values)?,
        PppRaimConfig::core(config),
    )
    .map(|inner| PppRaimResult { inner })
    .map_err(raim_error)
}
#[wasm_bindgen(js_name = pppRaimPerSatelliteStatistics)]
pub fn per_satellite_statistics(
    residual_values: JsValue,
    geometry_values: JsValue,
    config: Option<PppRaimConfig>,
) -> Result<PppRaimIdentification, JsValue> {
    core_statistics(
        &residuals(residual_values)?,
        &geometry(geometry_values)?,
        PppRaimConfig::core(config),
    )
    .map(|inner| PppRaimIdentification { inner })
    .map_err(raim_error)
}
/// Brown snapshot-RAIM fault protection levels; requires positive redundancy.
#[wasm_bindgen(js_name = pppRaimProtectionLevels)]
pub fn protection_levels(
    geometry_values: JsValue,
    receiver_latitude_rad: f64,
    receiver_longitude_rad: f64,
    receiver_height_m: f64,
    config: Option<PppRaimConfig>,
) -> Result<PppProtectionLevels, JsValue> {
    let receiver = Wgs84Geodetic {
        lat_rad: receiver_latitude_rad,
        lon_rad: receiver_longitude_rad,
        height_m: receiver_height_m,
    };
    core_protection_levels(
        &geometry(geometry_values)?,
        receiver,
        PppRaimConfig::core(config),
    )
    .map(|inner| PppProtectionLevels { inner })
    .map_err(raim_error)
}

/// Brown snapshot-RAIM fault protection levels with inverse-standard-deviation weights.
#[wasm_bindgen(js_name = pppRaimProtectionLevelsWithWeights)]
pub fn protection_levels_with_weights(
    geometry_values: JsValue,
    weights: Vec<f64>,
    latitude_rad: f64,
    longitude_rad: f64,
    height_m: f64,
    config: Option<PppRaimConfig>,
) -> Result<PppProtectionLevels, JsValue> {
    core_weighted_levels(
        &geometry(geometry_values)?,
        &weights,
        Wgs84Geodetic {
            lat_rad: latitude_rad,
            lon_rad: longitude_rad,
            height_m,
        },
        PppRaimConfig::core(config),
    )
    .map(|inner| PppProtectionLevels { inner })
    .map_err(raim_error)
}

/// Fault-free K × sigma × DOP accuracy bounds, with inverse-standard-deviation weights.
#[wasm_bindgen(js_name = pppRaimFaultFreeAccuracyBounds)]
pub fn fault_free_accuracy_bounds(
    geometry_values: JsValue,
    weights: Vec<f64>,
    latitude_rad: f64,
    longitude_rad: f64,
    height_m: f64,
    config: Option<PppRaimConfig>,
) -> Result<PppProtectionLevels, JsValue> {
    core_accuracy_bounds(
        &geometry(geometry_values)?,
        &weights,
        Wgs84Geodetic {
            lat_rad: latitude_rad,
            lon_rad: longitude_rad,
            height_m,
        },
        PppRaimConfig::core(config),
    )
    .map(|inner| PppProtectionLevels { inner })
    .map_err(raim_error)
}

fn fde(
    sp3: &Sp3,
    epoch: JsValue,
    initial_state: JsValue,
    solve_config: JsValue,
    raim_config: Option<PppRaimConfig>,
) -> Result<PppRaimFdeResult, JsValue> {
    let epoch: EpochInput = from_js(epoch, "PPP epoch")?;
    let state: StateInput = from_js(initial_state, "PPP initial state")?;
    let config: FloatConfigInput = from_js(solve_config, "PPP float config")?;
    core_fde(
        &sp3.inner,
        epoch.to_core()?,
        state.to_core(),
        config.to_core()?,
        PppRaimConfig::core(raim_config),
    )
    .map(|inner| PppRaimFdeResult { inner })
    .map_err(|error| match error {
        RaimFdeError::Raim(inner) => raim_error(inner),
        RaimFdeError::Solve(inner) => engine_error(inner),
    })
}
#[wasm_bindgen(js_name = pppFdeFloatEpoch)]
pub fn fde_float_epoch(
    sp3: &Sp3,
    epoch: JsValue,
    initial_state: JsValue,
    solve_config: JsValue,
    raim_config: Option<PppRaimConfig>,
) -> Result<PppRaimFdeResult, JsValue> {
    fde(sp3, epoch, initial_state, solve_config, raim_config)
}
#[wasm_bindgen(js_name = solvePppFloatEpochWithRaim)]
pub fn solve_float_epoch_with_raim(
    sp3: &Sp3,
    epoch: JsValue,
    initial_state: JsValue,
    solve_config: JsValue,
    raim_config: Option<PppRaimConfig>,
) -> Result<PppRaimFdeResult, JsValue> {
    fde(sp3, epoch, initial_state, solve_config, raim_config)
}

#[wasm_bindgen(typescript_custom_section)]
const TYPES: &'static str = r#"
export interface PppFloatResidualInput { epochIndex: number; satelliteId: string; ambiguityId: string; codeM: number; phaseM: number; codeWeight: number; phaseWeight: number; }
export interface PppRaimGeometryRow { satelliteId: string; lineOfSight: [number, number, number]; }
export type PppRaimStatus = "passed" | "faultDetected" | "notEnoughRedundancy";
export type PppRaimFdeStatus = "untested" | "clean" | "restored" | "cannotExclude" | "integrityNotRestored";
export interface PppSatelliteTestStatistic { satelliteId: string; code: number; phase: number; statistic: number; codeTestable: boolean; phaseTestable: boolean; }
export type PppRaimErrorDetail = { family: "RaimError"; kind: "INVALID_CONFIG"; field: string; reason: string; message: string } | { family: "RaimError"; kind: "INVALID_RESIDUAL"; field: string; message: string } | { family: "RaimError"; kind: "INVALID_GEOMETRY"; field: string; reason: string; message: string } | { family: "RaimError"; kind: "DOP"; cause: string; message: string } | { family: "RaimError"; kind: "SINGULAR_GEOMETRY" | "NUMERICAL_PROTECTION_LEVEL" | "UNSUPPORTED_NONCENTRALITY"; message: string } | { family: "RaimError"; kind: "UNSUPPORTED_DOF"; dof: number; max: number; message: string };
"#;
