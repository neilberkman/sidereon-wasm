//! ARAIM multi-hypothesis snapshot integrity binding.
//!
//! The JS API passes line-of-sight geometry, an integrity support message, and
//! an allocation object into `sidereon_core::araim`. The result is returned as a
//! plain object with protection levels and per-mode monitor data.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::araim::{
    araim as core_araim, enumerate_fault_modes as core_enumerate_fault_modes,
    AraimGeometry as CoreAraimGeometry, AraimRow as CoreAraimRow,
    ConstellationIsm as CoreConstellationIsm, IntegrityAllocation as CoreIntegrityAllocation,
    Ism as CoreIsm, SatelliteIsm as CoreSatelliteIsm, SatelliteIsmModel as CoreSatelliteIsmModel,
};
use sidereon_core::frame::Wgs84Geodetic;
use sidereon_core::geometry::LineOfSight;
use sidereon_core::{GnssSatelliteId, GnssSystem};

use crate::error::{engine_error, range_error, type_error};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AraimGeometryInput {
    rows: Vec<AraimRowInput>,
    receiver: AraimReceiverInput,
    clock_systems: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AraimRowInput {
    id: String,
    line_of_sight: [f64; 3],
    #[serde(default)]
    system: Option<String>,
    elevation_rad: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AraimReceiverInput {
    lat_rad: f64,
    lon_rad: f64,
    height_m: f64,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
struct SatelliteIsmModelJs {
    sigma_ura_m: f64,
    sigma_ure_m: f64,
    #[serde(default)]
    effective_sigma_int_m: Option<f64>,
    #[serde(default)]
    effective_sigma_acc_m: Option<f64>,
    b_nom_m: f64,
    p_sat: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConstellationIsmInput {
    system: String,
    p_const: f64,
    default_sat: SatelliteIsmModelJs,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SatelliteIsmInput {
    id: String,
    sigma_ura_m: f64,
    sigma_ure_m: f64,
    #[serde(default)]
    effective_sigma_int_m: Option<f64>,
    #[serde(default)]
    effective_sigma_acc_m: Option<f64>,
    b_nom_m: f64,
    p_sat: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IsmInput {
    constellations: Vec<ConstellationIsmInput>,
    #[serde(default)]
    satellites: Vec<SatelliteIsmInput>,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
struct IntegrityAllocationJs {
    phmi_total: f64,
    phmi_vert: f64,
    phmi_hor: f64,
    pfa_vert: f64,
    pfa_hor: f64,
    p_threshold_unmonitored: f64,
    #[serde(default = "default_p_emt")]
    p_emt: f64,
    max_fault_order: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FaultHypothesisJs {
    excluded: Vec<String>,
    excluded_constellation: Option<String>,
    prior: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FaultModeJs {
    excluded: Vec<String>,
    excluded_constellation: Option<String>,
    prior: f64,
    sigma_int_enu_m: [f64; 3],
    bias_enu_m: [f64; 3],
    threshold_enu_m: [f64; 3],
    monitorable: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AraimResultJs {
    available: bool,
    hpl_m: f64,
    vpl_m: f64,
    sigma_acc_h_m: f64,
    sigma_acc_v_m: f64,
    emt_m: f64,
    fault_modes: Vec<FaultModeJs>,
    p_unmonitored: f64,
    /// Alias for `available`, kept for compatibility.
    availability: bool,
}

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| engine_error(format!("failed to serialize result: {e}")))
}

fn parse_sat(token: &str) -> Result<GnssSatelliteId, JsValue> {
    token
        .parse::<GnssSatelliteId>()
        .map_err(|e| type_error(&format!("invalid satellite token {token:?}: {e}")))
}

fn parse_system(label: &str) -> Result<GnssSystem, JsValue> {
    match label {
        "G" | "gps" | "GPS" | "Gps" => Ok(GnssSystem::Gps),
        "R" | "glonass" | "GLONASS" | "Glonass" => Ok(GnssSystem::Glonass),
        "E" | "galileo" | "Galileo" => Ok(GnssSystem::Galileo),
        "C" | "beidou" | "BeiDou" => Ok(GnssSystem::BeiDou),
        "J" | "qzss" | "QZSS" | "Qzss" => Ok(GnssSystem::Qzss),
        "I" | "navic" | "NavIC" | "Navic" => Ok(GnssSystem::Navic),
        "S" | "sbas" | "SBAS" | "Sbas" => Ok(GnssSystem::Sbas),
        other => Err(type_error(&format!("invalid GNSS system {other:?}"))),
    }
}

fn default_p_emt() -> f64 {
    1.0e-5
}

fn system_label(system: GnssSystem) -> String {
    system.as_str().to_string()
}

fn satellite_tokens(sats: Vec<GnssSatelliteId>) -> Vec<String> {
    sats.into_iter().map(|sat| sat.to_string()).collect()
}

fn receiver(input: AraimReceiverInput) -> Result<Wgs84Geodetic, JsValue> {
    Wgs84Geodetic::new(input.lat_rad, input.lon_rad, input.height_m)
        .map_err(|e| range_error(&e.to_string()))
}

fn geometry(input: AraimGeometryInput) -> Result<CoreAraimGeometry, JsValue> {
    let receiver = receiver(input.receiver)?;
    let clock_systems = input
        .clock_systems
        .iter()
        .map(|system| parse_system(system))
        .collect::<Result<Vec<_>, _>>()?;
    let rows = input
        .rows
        .into_iter()
        .map(|row| {
            let id = parse_sat(&row.id)?;
            let system = match row.system {
                Some(system) => parse_system(&system)?,
                None => id.system,
            };
            Ok(CoreAraimRow {
                id,
                line_of_sight: LineOfSight::new(
                    row.line_of_sight[0],
                    row.line_of_sight[1],
                    row.line_of_sight[2],
                ),
                system,
                elevation_rad: row.elevation_rad,
            })
        })
        .collect::<Result<Vec<_>, JsValue>>()?;
    Ok(CoreAraimGeometry {
        rows,
        receiver,
        clock_systems,
    })
}

fn satellite_model(input: SatelliteIsmModelJs) -> Result<CoreSatelliteIsmModel, JsValue> {
    match (input.effective_sigma_int_m, input.effective_sigma_acc_m) {
        (Some(effective_sigma_int_m), Some(effective_sigma_acc_m)) => {
            Ok(CoreSatelliteIsmModel::new_with_effective_sigmas(
                input.sigma_ura_m,
                input.sigma_ure_m,
                input.b_nom_m,
                input.p_sat,
                effective_sigma_int_m,
                effective_sigma_acc_m,
            ))
        }
        (None, None) => Ok(CoreSatelliteIsmModel::new(
            input.sigma_ura_m,
            input.sigma_ure_m,
            input.b_nom_m,
            input.p_sat,
        )),
        _ => Err(type_error(
            "effectiveSigmaIntM and effectiveSigmaAccM must be set together",
        )),
    }
}

fn ism(input: IsmInput) -> Result<CoreIsm, JsValue> {
    let constellations = input
        .constellations
        .into_iter()
        .map(|constellation| {
            Ok(CoreConstellationIsm::new(
                parse_system(&constellation.system)?,
                constellation.p_const,
                satellite_model(constellation.default_sat)?,
            ))
        })
        .collect::<Result<Vec<_>, JsValue>>()?;
    let satellites = input
        .satellites
        .into_iter()
        .map(|satellite| {
            match (
                satellite.effective_sigma_int_m,
                satellite.effective_sigma_acc_m,
            ) {
                (Some(effective_sigma_int_m), Some(effective_sigma_acc_m)) => {
                    Ok(CoreSatelliteIsm::new_with_effective_sigmas(
                        parse_sat(&satellite.id)?,
                        satellite.sigma_ura_m,
                        satellite.sigma_ure_m,
                        satellite.b_nom_m,
                        satellite.p_sat,
                        effective_sigma_int_m,
                        effective_sigma_acc_m,
                    ))
                }
                (None, None) => Ok(CoreSatelliteIsm::new(
                    parse_sat(&satellite.id)?,
                    satellite.sigma_ura_m,
                    satellite.sigma_ure_m,
                    satellite.b_nom_m,
                    satellite.p_sat,
                )),
                _ => Err(type_error(
                    "effectiveSigmaIntM and effectiveSigmaAccM must be set together",
                )),
            }
        })
        .collect::<Result<Vec<_>, JsValue>>()?;
    Ok(CoreIsm::new(constellations, satellites))
}

fn allocation(input: IntegrityAllocationJs) -> CoreIntegrityAllocation {
    CoreIntegrityAllocation {
        phmi_total: input.phmi_total,
        phmi_vert: input.phmi_vert,
        phmi_hor: input.phmi_hor,
        pfa_vert: input.pfa_vert,
        pfa_hor: input.pfa_hor,
        p_threshold_unmonitored: input.p_threshold_unmonitored,
        p_emt: input.p_emt,
        max_fault_order: input.max_fault_order,
    }
}

fn allocation_from_core(input: CoreIntegrityAllocation) -> IntegrityAllocationJs {
    IntegrityAllocationJs {
        phmi_total: input.phmi_total,
        phmi_vert: input.phmi_vert,
        phmi_hor: input.phmi_hor,
        pfa_vert: input.pfa_vert,
        pfa_hor: input.pfa_hor,
        p_threshold_unmonitored: input.p_threshold_unmonitored,
        p_emt: input.p_emt,
        max_fault_order: input.max_fault_order,
    }
}

pub(crate) fn parse_geometry(value: JsValue) -> Result<CoreAraimGeometry, JsValue> {
    let input: AraimGeometryInput = serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid ARAIM geometry: {e}")))?;
    geometry(input)
}

pub(crate) fn parse_ism(value: JsValue) -> Result<CoreIsm, JsValue> {
    let input: IsmInput = serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid ARAIM ISM: {e}")))?;
    ism(input)
}

fn parse_allocation(value: JsValue) -> Result<CoreIntegrityAllocation, JsValue> {
    if value.is_undefined() || value.is_null() {
        return Ok(CoreIntegrityAllocation::lpv_200());
    }
    let input: IntegrityAllocationJs = serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid ARAIM allocation: {e}")))?;
    Ok(allocation(input))
}

fn fault_hypothesis_from_core(value: sidereon_core::araim::FaultHypothesis) -> FaultHypothesisJs {
    FaultHypothesisJs {
        excluded: satellite_tokens(value.excluded),
        excluded_constellation: value.excluded_constellation.map(system_label),
        prior: value.prior,
    }
}

fn fault_mode_from_core(value: sidereon_core::araim::FaultMode) -> FaultModeJs {
    FaultModeJs {
        excluded: satellite_tokens(value.excluded),
        excluded_constellation: value.excluded_constellation.map(system_label),
        prior: value.prior,
        sigma_int_enu_m: value.sigma_int_enu_m,
        bias_enu_m: value.bias_enu_m,
        threshold_enu_m: value.threshold_enu_m,
        monitorable: value.monitorable,
    }
}

fn result_from_core(value: sidereon_core::araim::AraimResult) -> AraimResultJs {
    AraimResultJs {
        available: value.available,
        hpl_m: value.hpl_m,
        vpl_m: value.vpl_m,
        sigma_acc_h_m: value.sigma_acc_h_m,
        sigma_acc_v_m: value.sigma_acc_v_m,
        emt_m: value.emt_m,
        fault_modes: value
            .fault_modes
            .into_iter()
            .map(fault_mode_from_core)
            .collect(),
        p_unmonitored: value.p_unmonitored,
        availability: value.available,
    }
}

/// LPV-200 ARAIM integrity and continuity allocation.
///
/// The returned object can be passed to `araim`. Probability fields are
/// dimensionless, `pEmt` defaults to `1e-5`, and `maxFaultOrder` is an integer
/// fault order.
#[wasm_bindgen(js_name = araimLpv200Allocation)]
pub fn araim_lpv_200_allocation() -> Result<JsValue, JsValue> {
    to_js(&allocation_from_core(CoreIntegrityAllocation::lpv_200()))
}

/// Enumerate ARAIM fault hypotheses for the given geometry, ISM, and allocation.
///
/// Inputs mirror `araim`. The returned priors are probabilities. Excluded
/// satellites are string tokens such as `"G01"`, and excluded constellations are
/// labels such as `"GPS"`.
#[wasm_bindgen(js_name = araimFaultModes)]
pub fn araim_fault_modes(
    geometry: JsValue,
    ism: JsValue,
    allocation: JsValue,
) -> Result<JsValue, JsValue> {
    let geometry = parse_geometry(geometry)?;
    let ism = parse_ism(ism)?;
    let allocation = parse_allocation(allocation)?;
    let modes: Vec<FaultHypothesisJs> = core_enumerate_fault_modes(&geometry, &ism, &allocation)
        .map_err(engine_error)?
        .into_iter()
        .map(fault_hypothesis_from_core)
        .collect();
    to_js(&modes)
}

/// Run ARAIM MHSS protection-level computation.
///
/// `geometry.rows` contains satellite IDs, ECEF line-of-sight unit vectors,
/// optional constellation labels, and elevations in radians. `receiver` is WGS84
/// geodetic radians plus ellipsoidal height meters. `ism` contains
/// constellation defaults and optional satellite overrides. Satellite models may
/// provide paired `effectiveSigmaIntM` and `effectiveSigmaAccM` fields; omit both
/// to let the core derive them from elevation. `allocation` may be omitted to use
/// `araimLpv200Allocation()`. Returned `hplM`, `vplM`, `sigmaAccHM`,
/// `sigmaAccVM`, `emtM`, and per-mode ENU arrays are meters.
#[wasm_bindgen(js_name = araim)]
pub fn araim(geometry: JsValue, ism: JsValue, allocation: JsValue) -> Result<JsValue, JsValue> {
    let geometry = parse_geometry(geometry)?;
    let ism = parse_ism(ism)?;
    let allocation = parse_allocation(allocation)?;
    let result = core_araim(&geometry, &ism, &allocation).map_err(engine_error)?;
    to_js(&result_from_core(result))
}
