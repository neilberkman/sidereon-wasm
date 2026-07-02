use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::astro::relative::{
    cw_propagate as core_cw_propagate, cw_stm as core_cw_stm,
    lvlh_to_inertial_rotation as core_lvlh_to_inertial_rotation,
    mean_motion_circular as core_mean_motion_circular,
    mean_motion_from_state as core_mean_motion_from_state, relative_state as core_relative_state,
    ric_to_inertial_rotation as core_ric_to_inertial_rotation,
    rsw_to_inertial_rotation as core_rsw_to_inertial_rotation,
    rtn_to_inertial_rotation as core_rtn_to_inertial_rotation,
};
use sidereon_core::astro::state::CartesianState;

use crate::error::{engine_error, type_error};
use crate::marshal::{mat3_flat, vec3_finite};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StateInput {
    epoch_s: f64,
    position_km: Vec<f64>,
    velocity_km_s: Vec<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StateOutput {
    epoch_s: f64,
    position_km: [f64; 3],
    velocity_km_s: [f64; 3],
}

fn state_from_js(value: JsValue, name: &str) -> Result<CartesianState, JsValue> {
    let input: StateInput = serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid {name}: {e}")))?;
    let position = vec3_finite(&format!("{name}.positionKm"), &input.position_km)?;
    let velocity = vec3_finite(&format!("{name}.velocityKmS"), &input.velocity_km_s)?;
    Ok(CartesianState::new(input.epoch_s, position, velocity))
}

fn rtn_error<E: core::fmt::Debug>(err: E) -> JsValue {
    engine_error(format!("{err:?}"))
}

fn state_to_js(state: CartesianState) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(&StateOutput {
        epoch_s: state.epoch_tdb_seconds,
        position_km: state.position_array(),
        velocity_km_s: state.velocity_array(),
    })
    .map_err(|e| type_error(&e.to_string()))
}

fn chief_state(position_km: &[f64], velocity_km_s: &[f64]) -> Result<CartesianState, JsValue> {
    Ok(CartesianState::new(
        0.0,
        vec3_finite("positionKm", position_km)?,
        vec3_finite("velocityKmS", velocity_km_s)?,
    ))
}

#[wasm_bindgen(js_name = rswRotation)]
pub fn rsw_rotation(position_km: &[f64], velocity_km_s: &[f64]) -> Result<Vec<f64>, JsValue> {
    let state = chief_state(position_km, velocity_km_s)?;
    Ok(mat3_flat(
        &core_rsw_to_inertial_rotation(&state).map_err(rtn_error)?,
    ))
}

#[wasm_bindgen(js_name = rtnRotation)]
pub fn rtn_rotation(position_km: &[f64], velocity_km_s: &[f64]) -> Result<Vec<f64>, JsValue> {
    let state = chief_state(position_km, velocity_km_s)?;
    Ok(mat3_flat(
        &core_rtn_to_inertial_rotation(&state).map_err(rtn_error)?,
    ))
}

#[wasm_bindgen(js_name = ricRotation)]
pub fn ric_rotation(position_km: &[f64], velocity_km_s: &[f64]) -> Result<Vec<f64>, JsValue> {
    let state = chief_state(position_km, velocity_km_s)?;
    Ok(mat3_flat(
        &core_ric_to_inertial_rotation(&state).map_err(rtn_error)?,
    ))
}

#[wasm_bindgen(js_name = lvlhRotation)]
pub fn lvlh_rotation(position_km: &[f64], velocity_km_s: &[f64]) -> Result<Vec<f64>, JsValue> {
    let state = chief_state(position_km, velocity_km_s)?;
    Ok(mat3_flat(
        &core_lvlh_to_inertial_rotation(&state).map_err(rtn_error)?,
    ))
}

#[wasm_bindgen(js_name = relativeState)]
pub fn relative_state(chief: JsValue, deputy: JsValue) -> Result<JsValue, JsValue> {
    let chief = state_from_js(chief, "chief")?;
    let deputy = state_from_js(deputy, "deputy")?;
    state_to_js(core_relative_state(&chief, &deputy).map_err(rtn_error)?)
}

#[wasm_bindgen(js_name = cwStm)]
pub fn cw_stm(n_rad_s: f64, dt_s: f64) -> Result<Vec<f64>, JsValue> {
    let matrix = core_cw_stm(n_rad_s, dt_s).map_err(rtn_error)?;
    let mut out = Vec::with_capacity(36);
    for row in &matrix {
        out.extend_from_slice(row);
    }
    Ok(out)
}

#[wasm_bindgen(js_name = cwPropagate)]
pub fn cw_propagate(rel: JsValue, n_rad_s: f64, dt_s: f64) -> Result<JsValue, JsValue> {
    let rel = state_from_js(rel, "relativeState")?;
    state_to_js(core_cw_propagate(&rel, n_rad_s, dt_s).map_err(rtn_error)?)
}

#[wasm_bindgen(js_name = meanMotionCircular)]
pub fn mean_motion_circular(radius_km: f64) -> Result<f64, JsValue> {
    core_mean_motion_circular(radius_km).map_err(rtn_error)
}

#[wasm_bindgen(js_name = meanMotionFromState)]
pub fn mean_motion_from_state(position_km: &[f64], velocity_km_s: &[f64]) -> Result<f64, JsValue> {
    let state = chief_state(position_km, velocity_km_s)?;
    core_mean_motion_from_state(&state).map_err(rtn_error)
}
