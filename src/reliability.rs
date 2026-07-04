//! Classical range-observation reliability diagnostics.
//!
//! This module marshals pre-data reliability-design rows and ARAIM geometry into
//! `sidereon_core::quality`. It does not read residuals or measured ranges; all
//! internal and external reliability math lives in the core crate.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::quality::{
    reliability_araim as core_reliability_araim, reliability_design as core_reliability_design,
    wtest_noncentrality_components as core_wtest_noncentrality_components,
    ObservationReliability as CoreObservationReliability, QualityError,
    RangeReliabilityRow as CoreRangeReliabilityRow, ReliabilityOptions as CoreReliabilityOptions,
    ReliabilityReport as CoreReliabilityReport, ReliabilitySummary as CoreReliabilitySummary,
};

use crate::araim::{parse_geometry as parse_araim_geometry, parse_ism as parse_araim_ism};
use crate::error::{engine_error, range_error, type_error};

fn serializer() -> serde_wasm_bindgen::Serializer {
    serde_wasm_bindgen::Serializer::new()
        .serialize_maps_as_objects(true)
        .serialize_missing_as_null(true)
}

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serializer())
        .map_err(|e| engine_error(format!("failed to serialize result: {e}")))
}

fn quality_error(error: QualityError) -> JsValue {
    match error {
        QualityError::InvalidProbability
        | QualityError::InvalidReliabilityParameter
        | QualityError::InvalidWeight => range_error(&error.to_string()),
        QualityError::InvalidDesign => type_error(&error.to_string()),
        QualityError::SingularGeometry => engine_error(error),
        _ => engine_error(error),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RangeReliabilityRowInput {
    id: String,
    design_row: Vec<f64>,
    sigma_m: f64,
}

impl From<RangeReliabilityRowInput> for CoreRangeReliabilityRow {
    fn from(value: RangeReliabilityRowInput) -> Self {
        Self {
            id: value.id,
            design_row: value.design_row,
            sigma_m: value.sigma_m,
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ReliabilityOptionsInput {
    alpha: Option<f64>,
    power: Option<f64>,
    beta: Option<f64>,
    lambda0: Option<f64>,
    lambda0_override: Option<f64>,
    min_redundancy: Option<f64>,
}

impl ReliabilityOptionsInput {
    fn to_core(&self) -> Result<CoreReliabilityOptions, JsValue> {
        let defaults = CoreReliabilityOptions::default();
        let beta = match (self.beta, self.power) {
            (Some(_), Some(_)) => return Err(type_error("set either beta or power, not both")),
            (Some(beta), None) => beta,
            (None, Some(power)) => 1.0 - power,
            (None, None) => defaults.beta,
        };
        let lambda0_override = match (self.lambda0_override, self.lambda0) {
            (Some(_), Some(_)) => {
                return Err(type_error(
                    "set either lambda0Override or lambda0, not both",
                ));
            }
            (Some(lambda0), None) | (None, Some(lambda0)) => Some(lambda0),
            (None, None) => None,
        };
        Ok(CoreReliabilityOptions {
            alpha: self.alpha.unwrap_or(defaults.alpha),
            beta,
            lambda0_override,
            min_redundancy: self.min_redundancy.unwrap_or(defaults.min_redundancy),
        })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WtestNoncentralityJs {
    alpha: f64,
    power: f64,
    beta: f64,
    delta0: f64,
    lambda0: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ObservationReliabilityJs {
    id: String,
    redundancy: f64,
    mdb_m: Option<f64>,
    external_enu_m: Option<[f64; 3]>,
    bias_to_noise: Option<f64>,
    uncheckable: bool,
}

impl From<CoreObservationReliability> for ObservationReliabilityJs {
    fn from(value: CoreObservationReliability) -> Self {
        Self {
            id: value.id,
            redundancy: value.redundancy,
            mdb_m: value.mdb_m,
            external_enu_m: value.external_enu_m,
            bias_to_noise: value.bias_to_noise,
            uncheckable: value.uncheckable,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReliabilityMaxMdbJs {
    id: String,
    mdb_m: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReliabilityMinRedundancyJs {
    id: String,
    redundancy: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReliabilitySummaryJs {
    n_obs: usize,
    n_params: usize,
    dof: usize,
    sum_redundancy: f64,
    lambda0: f64,
    max_mdb_m: Option<ReliabilityMaxMdbJs>,
    min_redundancy: ReliabilityMinRedundancyJs,
    n_uncheckable: usize,
}

impl From<CoreReliabilitySummary> for ReliabilitySummaryJs {
    fn from(value: CoreReliabilitySummary) -> Self {
        Self {
            n_obs: value.n_obs,
            n_params: value.n_params,
            dof: value.dof,
            sum_redundancy: value.sum_redundancy,
            lambda0: value.lambda0,
            max_mdb_m: value
                .max_mdb_m
                .map(|(id, mdb_m)| ReliabilityMaxMdbJs { id, mdb_m }),
            min_redundancy: ReliabilityMinRedundancyJs {
                id: value.min_redundancy.0,
                redundancy: value.min_redundancy.1,
            },
            n_uncheckable: value.n_uncheckable,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReliabilityReportJs {
    per_observation: Vec<ObservationReliabilityJs>,
    summary: ReliabilitySummaryJs,
}

impl From<CoreReliabilityReport> for ReliabilityReportJs {
    fn from(value: CoreReliabilityReport) -> Self {
        Self {
            per_observation: value
                .per_observation
                .into_iter()
                .map(ObservationReliabilityJs::from)
                .collect(),
            summary: ReliabilitySummaryJs::from(value.summary),
        }
    }
}

fn parse_options(value: JsValue) -> Result<CoreReliabilityOptions, JsValue> {
    if value.is_undefined() || value.is_null() {
        return ReliabilityOptionsInput::default().to_core();
    }
    let input: ReliabilityOptionsInput = serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid reliability options: {e}")))?;
    input.to_core()
}

/// Compute Baarda's w-test noncentrality from false-alarm probability and power.
///
/// `alpha` is the two-sided false-alarm probability. `power` is detection power,
/// so the core missed-detection probability is `1 - power`. The returned object
/// contains the core-provided `delta0` and `lambda0` values.
#[wasm_bindgen(js_name = wtestNoncentrality)]
pub fn wtest_noncentrality(alpha: f64, power: f64) -> Result<JsValue, JsValue> {
    let beta = 1.0 - power;
    let components = core_wtest_noncentrality_components(alpha, beta).map_err(quality_error)?;
    to_js(&WtestNoncentralityJs {
        alpha,
        power,
        beta,
        delta0: components.delta0,
        lambda0: components.lambda0,
    })
}

/// Compute pre-data reliability from supplied range design rows.
///
/// `rows` is an array of `{ id, designRow, sigmaM }` objects. `options` may set
/// `{ alpha?, power?, beta?, lambda0Override?, lambda0?, minRedundancy? }`.
/// `power` is converted to the core missed-detection probability. Rows below
/// `minRedundancy` return `null` for `mdbM`, `externalEnuM`, and `biasToNoise`.
#[wasm_bindgen(js_name = reliabilityDesign)]
pub fn reliability_design(rows: JsValue, options: JsValue) -> Result<JsValue, JsValue> {
    let rows: Vec<RangeReliabilityRowInput> = serde_wasm_bindgen::from_value(rows)
        .map_err(|e| type_error(&format!("invalid reliability rows: {e}")))?;
    let rows = rows
        .into_iter()
        .map(CoreRangeReliabilityRow::from)
        .collect::<Vec<_>>();
    let options = parse_options(options)?;
    let report = core_reliability_design(&rows, &options).map_err(quality_error)?;
    to_js(&ReliabilityReportJs::from(report))
}

/// Compute pre-data ARAIM reliability using an ARAIM ISM range-error model.
///
/// `geometry` and `ism` use the same plain objects accepted by `araim`.
/// `options` follows `reliabilityDesign`. External effects are returned in local
/// east, north, up metres. Uncheckable rows return `null` for optional fields.
#[wasm_bindgen(js_name = reliabilityAraim)]
pub fn reliability_araim(
    geometry: JsValue,
    ism: JsValue,
    options: JsValue,
) -> Result<JsValue, JsValue> {
    let geometry = parse_araim_geometry(geometry)?;
    let ism = parse_araim_ism(ism)?;
    let options = parse_options(options)?;
    let report = core_reliability_araim(&geometry, &ism, &options).map_err(engine_error)?;
    to_js(&ReliabilityReportJs::from(report))
}
