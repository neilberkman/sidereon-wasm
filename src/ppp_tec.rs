//! Dual-frequency TEC estimation for precise positioning.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::precise_positioning::{
    code_geometry_free_m as core_code_geometry_free_m,
    estimate_code_slant_tec as core_estimate_code, estimate_phase_slant_tec as core_estimate_phase,
    estimate_tec as core_estimate_tec, ionospheric_pierce_point as core_pierce_point,
    level_slant_tec_arc as core_level_arc, phase_geometry_free_m as core_phase_geometry_free_m,
    slant_tec_from_code_geometry_free_m as core_slant_from_code,
    slant_tec_from_phase_geometry_free_m as core_slant_from_phase,
    thin_shell_mapping_function as core_mapping, vertical_tec_from_slant_tec as core_vertical,
    DualFrequencyObservation, IonosphericPiercePoint, TecConfig, TecEpoch, TecError, TecEstimate,
    TecLevelingResult, TecLevelingSample, TecObservation, ELECTRONS_PER_TECU_M2,
    TEC_GROUP_DELAY_COEFFICIENT,
};

use crate::error::type_error;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DualFrequencyObservationInput {
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

impl From<DualFrequencyObservationInput> for DualFrequencyObservation {
    fn from(value: DualFrequencyObservationInput) -> Self {
        Self {
            satellite_id: value.satellite_id,
            ambiguity_id: value.ambiguity_id,
            p1_m: value.p1_m,
            p2_m: value.p2_m,
            phi1_cyc: value.phi1_cyc,
            phi2_cyc: value.phi2_cyc,
            f1_hz: value.f1_hz,
            f2_hz: value.f2_hz,
            lli1: value.lli1,
            lli2: value.lli2,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TecObservationInput {
    observation: DualFrequencyObservationInput,
    elevation_rad: f64,
    azimuth_rad: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TecEpochInput {
    time_s: f64,
    receiver_latitude_rad: f64,
    receiver_longitude_rad: f64,
    observations: Vec<TecObservationInput>,
}

impl From<TecEpochInput> for TecEpoch {
    fn from(value: TecEpochInput) -> Self {
        Self {
            time_s: value.time_s,
            receiver_latitude_rad: value.receiver_latitude_rad,
            receiver_longitude_rad: value.receiver_longitude_rad,
            observations: value
                .observations
                .into_iter()
                .map(|row| TecObservation {
                    observation: row.observation.into(),
                    elevation_rad: row.elevation_rad,
                    azimuth_rad: row.azimuth_rad,
                })
                .collect(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LevelingSampleInput {
    code_slant_tec_tecu: f64,
    phase_slant_tec_tecu: f64,
    elevation_rad: f64,
}

impl From<LevelingSampleInput> for TecLevelingSample {
    fn from(value: LevelingSampleInput) -> Self {
        Self {
            code_slant_tec_tecu: value.code_slant_tec_tecu,
            phase_slant_tec_tecu: value.phase_slant_tec_tecu,
            elevation_rad: value.elevation_rad,
        }
    }
}

fn from_js<T: for<'de> Deserialize<'de>>(value: JsValue, name: &str) -> Result<T, JsValue> {
    serde_wasm_bindgen::from_value(value)
        .map_err(|error| type_error(&format!("invalid {name}: {error}")))
}

fn tec_error(error: TecError) -> JsValue {
    let kind = match error {
        TecError::NonFiniteObservation => "NON_FINITE_OBSERVATION",
        TecError::InvalidShellHeight => "INVALID_SHELL_HEIGHT",
        TecError::InvalidEarthRadius => "INVALID_EARTH_RADIUS",
        TecError::InvalidFrequency => "INVALID_FREQUENCY",
        TecError::EqualFrequencies => "EQUAL_FREQUENCIES",
        TecError::InvalidReceiverLatitude => "INVALID_RECEIVER_LATITUDE",
        TecError::InvalidReceiverLongitude => "INVALID_RECEIVER_LONGITUDE",
        TecError::InvalidElevation => "INVALID_ELEVATION",
        TecError::InvalidAzimuth => "INVALID_AZIMUTH",
        TecError::NonFiniteTec => "NON_FINITE_TEC",
        TecError::EmptyArc => "EMPTY_ARC",
        TecError::NoEpochs => "NO_EPOCHS",
        TecError::NoObservations => "NO_OBSERVATIONS",
        TecError::NonFiniteEpochTime => "NON_FINITE_EPOCH_TIME",
        TecError::EpochsNotOrdered => "EPOCHS_NOT_ORDERED",
        TecError::InsufficientArcSamples => "INSUFFICIENT_ARC_SAMPLES",
    };
    let exception = js_sys::RangeError::new(&error.to_string());
    let detail = serde_json::json!({
        "family": "TecError",
        "kind": kind,
        "message": error.to_string(),
    });
    let detail = detail
        .serialize(&serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true))
        .unwrap_or(JsValue::UNDEFINED);
    let _ = js_sys::Reflect::set(exception.as_ref(), &JsValue::from_str("details"), &detail);
    exception.into()
}

/// Thin-shell TEC configuration backed by the core value.
#[wasm_bindgen]
#[derive(Clone)]
pub struct PppTecConfig {
    inner: TecConfig,
}

#[wasm_bindgen]
impl PppTecConfig {
    #[wasm_bindgen(constructor)]
    pub fn new(shell_height_m: Option<f64>, earth_radius_m: Option<f64>) -> Self {
        let mut inner = TecConfig::default();
        if let Some(value) = shell_height_m {
            inner.shell_height_m = value;
        }
        if let Some(value) = earth_radius_m {
            inner.earth_radius_m = value;
        }
        Self { inner }
    }

    #[wasm_bindgen(js_name = default)]
    pub fn default_js() -> Self {
        Self {
            inner: TecConfig::default(),
        }
    }

    #[wasm_bindgen(getter, js_name = shellHeightM)]
    pub fn shell_height_m(&self) -> f64 {
        self.inner.shell_height_m
    }

    #[wasm_bindgen(getter, js_name = earthRadiusM)]
    pub fn earth_radius_m(&self) -> f64 {
        self.inner.earth_radius_m
    }

    pub fn validate(&self) -> Result<(), JsValue> {
        self.inner.validate().map_err(tec_error)
    }

    pub fn equals(&self, other: &PppTecConfig) -> bool {
        self.inner == other.inner
    }
}

impl PppTecConfig {
    fn core(config: Option<PppTecConfig>) -> TecConfig {
        config.map_or_else(TecConfig::default, |value| value.inner)
    }
}

macro_rules! value_class {
    ($name:ident, $core:ty, $value:ty, $output:literal) => {
        #[wasm_bindgen]
        #[derive(Clone)]
        pub struct $name {
            inner: $core,
        }

        #[wasm_bindgen]
        impl $name {
            pub fn equals(&self, other: &$name) -> bool {
                self.inner == other.inner
            }

            #[wasm_bindgen(js_name = toJSON, unchecked_return_type = $output)]
            pub fn to_json(&self) -> Result<JsValue, JsValue> {
                serde_wasm_bindgen::to_value(&<$value>::from_core(&self.inner))
                    .map_err(|error| type_error(&format!("could not marshal TEC result: {error}")))
            }
        }
    };
}

trait TecOutput<T> {
    fn from_core(value: &T) -> Self;
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EstimateOutput {
    arcs: Vec<ArcOutput>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ArcOutput {
    satellite_id: String,
    ambiguity_id: String,
    phase_bias_tecu: f64,
    samples: Vec<SampleOutput>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SampleOutput {
    time_s: f64,
    elevation_rad: f64,
    azimuth_rad: f64,
    code_geometry_free_m: f64,
    phase_geometry_free_m: f64,
    code_slant_tec_tecu: f64,
    phase_slant_tec_tecu: f64,
    leveled_slant_tec_tecu: f64,
    mapping_function: f64,
    vertical_tec_tecu: f64,
    pierce_point: PierceOutput,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PierceOutput {
    latitude_rad: f64,
    longitude_rad: f64,
    latitude_deg: f64,
    longitude_deg: f64,
    earth_central_angle_rad: f64,
    shell_height_m: f64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LevelOutput {
    phase_bias_tecu: f64,
    samples: Vec<LeveledOutput>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LeveledOutput {
    code_slant_tec_tecu: f64,
    phase_slant_tec_tecu: f64,
    leveled_slant_tec_tecu: f64,
    mapping_function: f64,
    vertical_tec_tecu: f64,
}

impl TecOutput<TecEstimate> for EstimateOutput {
    fn from_core(value: &TecEstimate) -> Self {
        Self {
            arcs: value
                .arcs
                .iter()
                .map(|arc| ArcOutput {
                    satellite_id: arc.satellite_id.clone(),
                    ambiguity_id: arc.ambiguity_id.clone(),
                    phase_bias_tecu: arc.phase_bias_tecu,
                    samples: arc
                        .samples
                        .iter()
                        .map(|s| SampleOutput {
                            time_s: s.time_s,
                            elevation_rad: s.elevation_rad,
                            azimuth_rad: s.azimuth_rad,
                            code_geometry_free_m: s.code_geometry_free_m,
                            phase_geometry_free_m: s.phase_geometry_free_m,
                            code_slant_tec_tecu: s.code_slant_tec_tecu,
                            phase_slant_tec_tecu: s.phase_slant_tec_tecu,
                            leveled_slant_tec_tecu: s.leveled_slant_tec_tecu,
                            mapping_function: s.mapping_function,
                            vertical_tec_tecu: s.vertical_tec_tecu,
                            pierce_point: PierceOutput::from_core(&s.pierce_point),
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}
impl TecOutput<IonosphericPiercePoint> for PierceOutput {
    fn from_core(v: &IonosphericPiercePoint) -> Self {
        Self {
            latitude_rad: v.latitude_rad,
            longitude_rad: v.longitude_rad,
            latitude_deg: v.latitude_deg,
            longitude_deg: v.longitude_deg,
            earth_central_angle_rad: v.earth_central_angle_rad,
            shell_height_m: v.shell_height_m,
        }
    }
}
impl TecOutput<TecLevelingResult> for LevelOutput {
    fn from_core(v: &TecLevelingResult) -> Self {
        Self {
            phase_bias_tecu: v.phase_bias_tecu,
            samples: v
                .samples
                .iter()
                .map(|s| LeveledOutput {
                    code_slant_tec_tecu: s.code_slant_tec_tecu,
                    phase_slant_tec_tecu: s.phase_slant_tec_tecu,
                    leveled_slant_tec_tecu: s.leveled_slant_tec_tecu,
                    mapping_function: s.mapping_function,
                    vertical_tec_tecu: s.vertical_tec_tecu,
                })
                .collect(),
        }
    }
}

value_class!(
    PppTecEstimate,
    TecEstimate,
    EstimateOutput,
    "PppTecEstimateValue"
);
value_class!(
    PppTecLevelingResult,
    TecLevelingResult,
    LevelOutput,
    "PppTecLevelingResultValue"
);
value_class!(
    PppIonosphericPiercePoint,
    IonosphericPiercePoint,
    PierceOutput,
    "PppIonosphericPiercePointValue"
);

/// Electrons per square metre represented by one TECU.
#[wasm_bindgen(js_name = pppElectronsPerTecuM2)]
pub fn electrons_per_tecu_m2() -> f64 {
    ELECTRONS_PER_TECU_M2
}

/// TECU group-delay coefficient used by the core.
#[wasm_bindgen(js_name = pppTecGroupDelayCoefficient)]
pub fn tec_group_delay_coefficient() -> f64 {
    TEC_GROUP_DELAY_COEFFICIENT
}

fn observation(value: JsValue) -> Result<DualFrequencyObservation, JsValue> {
    Ok(from_js::<DualFrequencyObservationInput>(value, "PPP TEC observation")?.into())
}

#[wasm_bindgen(js_name = pppCodeGeometryFreeM)]
pub fn code_geometry_free_m(value: JsValue) -> Result<f64, JsValue> {
    core_code_geometry_free_m(&observation(value)?).map_err(tec_error)
}
#[wasm_bindgen(js_name = pppPhaseGeometryFreeM)]
pub fn phase_geometry_free_m(value: JsValue) -> Result<f64, JsValue> {
    core_phase_geometry_free_m(&observation(value)?).map_err(tec_error)
}
#[wasm_bindgen(js_name = pppSlantTecFromCodeGeometryFreeM)]
pub fn slant_from_code(value: f64, f1_hz: f64, f2_hz: f64) -> Result<f64, JsValue> {
    core_slant_from_code(value, f1_hz, f2_hz).map_err(tec_error)
}
#[wasm_bindgen(js_name = pppSlantTecFromPhaseGeometryFreeM)]
pub fn slant_from_phase(value: f64, f1_hz: f64, f2_hz: f64) -> Result<f64, JsValue> {
    core_slant_from_phase(value, f1_hz, f2_hz).map_err(tec_error)
}

#[wasm_bindgen(js_name = pppEstimateCodeSlantTec)]
pub fn estimate_code(value: JsValue) -> Result<JsValue, JsValue> {
    let out = core_estimate_code(&observation(value)?).map_err(tec_error)?;
    serde_wasm_bindgen::to_value(&serde_json::json!({"satelliteId": out.satellite_id, "ambiguityId": out.ambiguity_id, "codeGeometryFreeM": out.code_geometry_free_m, "slantTecTecu": out.slant_tec_tecu})).map_err(|e| type_error(&e.to_string()))
}
#[wasm_bindgen(js_name = pppEstimatePhaseSlantTec)]
pub fn estimate_phase(value: JsValue) -> Result<JsValue, JsValue> {
    let out = core_estimate_phase(&observation(value)?).map_err(tec_error)?;
    serde_wasm_bindgen::to_value(&serde_json::json!({"satelliteId": out.satellite_id, "ambiguityId": out.ambiguity_id, "phaseGeometryFreeM": out.phase_geometry_free_m, "slantTecTecu": out.slant_tec_tecu})).map_err(|e| type_error(&e.to_string()))
}
#[wasm_bindgen(js_name = pppThinShellMappingFunction)]
pub fn mapping(elevation_rad: f64, config: Option<PppTecConfig>) -> Result<f64, JsValue> {
    core_mapping(elevation_rad, PppTecConfig::core(config)).map_err(tec_error)
}
#[wasm_bindgen(js_name = pppVerticalTecFromSlantTec)]
pub fn vertical(
    slant_tec_tecu: f64,
    elevation_rad: f64,
    config: Option<PppTecConfig>,
) -> Result<f64, JsValue> {
    core_vertical(slant_tec_tecu, elevation_rad, PppTecConfig::core(config)).map_err(tec_error)
}
#[wasm_bindgen(js_name = pppLevelSlantTecArc)]
pub fn level(
    samples: JsValue,
    config: Option<PppTecConfig>,
) -> Result<PppTecLevelingResult, JsValue> {
    let samples: Vec<TecLevelingSample> =
        from_js::<Vec<LevelingSampleInput>>(samples, "PPP TEC leveling samples")?
            .into_iter()
            .map(Into::into)
            .collect();
    core_level_arc(&samples, PppTecConfig::core(config))
        .map(|inner| PppTecLevelingResult { inner })
        .map_err(tec_error)
}
#[wasm_bindgen(js_name = pppEstimateTec)]
pub fn estimate(epochs: JsValue, config: Option<PppTecConfig>) -> Result<PppTecEstimate, JsValue> {
    let epochs: Vec<TecEpoch> = from_js::<Vec<TecEpochInput>>(epochs, "PPP TEC epochs")?
        .into_iter()
        .map(Into::into)
        .collect();
    core_estimate_tec(&epochs, PppTecConfig::core(config))
        .map(|inner| PppTecEstimate { inner })
        .map_err(tec_error)
}
#[wasm_bindgen(js_name = pppIonosphericPiercePoint)]
pub fn pierce(
    receiver_latitude_rad: f64,
    receiver_longitude_rad: f64,
    elevation_rad: f64,
    azimuth_rad: f64,
    config: Option<PppTecConfig>,
) -> Result<PppIonosphericPiercePoint, JsValue> {
    core_pierce_point(
        receiver_latitude_rad,
        receiver_longitude_rad,
        elevation_rad,
        azimuth_rad,
        PppTecConfig::core(config),
    )
    .map(|inner| PppIonosphericPiercePoint { inner })
    .map_err(tec_error)
}

#[wasm_bindgen(typescript_custom_section)]
const TYPES: &'static str = r#"
export interface PppDualFrequencyObservation { satelliteId: string; ambiguityId: string; p1M: number; p2M: number; phi1Cyc: number; phi2Cyc: number; f1Hz: number; f2Hz: number; lli1?: number; lli2?: number; }
export interface PppTecObservation { observation: PppDualFrequencyObservation; elevationRad: number; azimuthRad: number; }
export interface PppTecEpoch { timeS: number; receiverLatitudeRad: number; receiverLongitudeRad: number; observations: PppTecObservation[]; }
export interface PppTecLevelingSample { codeSlantTecTecu: number; phaseSlantTecTecu: number; elevationRad: number; }
export interface PppIonosphericPiercePointValue { latitudeRad: number; longitudeRad: number; latitudeDeg: number; longitudeDeg: number; earthCentralAngleRad: number; shellHeightM: number; }
export interface PppTecEstimateSample { timeS: number; elevationRad: number; azimuthRad: number; codeGeometryFreeM: number; phaseGeometryFreeM: number; codeSlantTecTecu: number; phaseSlantTecTecu: number; leveledSlantTecTecu: number; mappingFunction: number; verticalTecTecu: number; piercePoint: PppIonosphericPiercePointValue; }
export interface PppTecSatelliteArc { satelliteId: string; ambiguityId: string; phaseBiasTecu: number; samples: PppTecEstimateSample[]; }
export interface PppTecEstimateValue { arcs: PppTecSatelliteArc[]; }
export interface PppLeveledTecSample { codeSlantTecTecu: number; phaseSlantTecTecu: number; leveledSlantTecTecu: number; mappingFunction: number; verticalTecTecu: number; }
export interface PppTecLevelingResultValue { phaseBiasTecu: number; samples: PppLeveledTecSample[]; }
export interface PppCodeSlantTecEstimate { satelliteId: string; ambiguityId: string; codeGeometryFreeM: number; slantTecTecu: number; }
export interface PppPhaseSlantTecEstimate { satelliteId: string; ambiguityId: string; phaseGeometryFreeM: number; slantTecTecu: number; }
export type PppTecErrorKind = "NON_FINITE_OBSERVATION" | "INVALID_SHELL_HEIGHT" | "INVALID_EARTH_RADIUS" | "INVALID_FREQUENCY" | "EQUAL_FREQUENCIES" | "INVALID_RECEIVER_LATITUDE" | "INVALID_RECEIVER_LONGITUDE" | "INVALID_ELEVATION" | "INVALID_AZIMUTH" | "NON_FINITE_TEC" | "EMPTY_ARC" | "NO_EPOCHS" | "NO_OBSERVATIONS" | "NON_FINITE_EPOCH_TIME" | "EPOCHS_NOT_ORDERED" | "INSUFFICIENT_ARC_SAMPLES";
"#;
