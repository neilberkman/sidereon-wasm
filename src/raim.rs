//! Direct post-solve RAIM and standalone range RAIM/FDE bindings.
//!
//! Direct `raim` takes post-fit satellite residual lists and optional residual
//! weights. `raimFdeDesign` covers the separate design-row FDE case. Both call
//! `sidereon_core::quality` for the integrity math.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::quality::{
    self, raim as core_raim, raim_fde_design, raim_for_solution as core_raim_for_solution,
    PseudorangeVarianceModel, PseudorangeVarianceOptions, QualityError, RaimInput as CoreRaimInput,
    RaimOptions as CoreRaimOptions, RaimResult as CoreRaimResult, RaimWeights as CoreRaimWeights,
    RangeChiSquareTest, RangeFdeOptions, RangeFdeResult, RangeFdeRow, RangeMeasurementDiagnostic,
    WeightEntry as CoreWeightEntry,
};

use crate::error::{engine_error, type_error};
use crate::spp::SppSolution;

fn serializer() -> serde_wasm_bindgen::Serializer {
    serde_wasm_bindgen::Serializer::new()
        .serialize_maps_as_objects(true)
        .serialize_missing_as_null(true)
}

pub(crate) fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serializer())
        .map_err(|e| engine_error(format!("failed to serialize result: {e}")))
}

fn quality_error(error: QualityError) -> JsValue {
    crate::positioning_error::quality_error(error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RaimInput {
    used_sats: Vec<String>,
    residuals_m: Vec<f64>,
    #[serde(default)]
    variances_m2: Option<Vec<f64>>,
}

impl RaimInput {
    fn to_core(&self) -> CoreRaimInput {
        CoreRaimInput {
            used_sats: self.used_sats.clone(),
            residuals_m: self.residuals_m.clone(),
            variances_m2: self.variances_m2.clone(),
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct RaimOptionsInput {
    p_fa: Option<f64>,
    weights_mode: Option<String>,
    weight_entries: Option<Vec<WeightEntryInput>>,
    variance_options: Option<PseudorangeVarianceOptionsInput>,
    n_systems: Option<i64>,
}

fn raim_weight_entries(input: &RaimOptionsInput) -> Result<Option<CoreRaimWeights>, JsValue> {
    let Some(entries) = &input.weight_entries else {
        return Ok(None);
    };
    let entries: Vec<CoreWeightEntry> = entries.iter().map(WeightEntryInput::to_core).collect();
    let variance_options = input
        .variance_options
        .as_ref()
        .map(PseudorangeVarianceOptionsInput::to_core)
        .transpose()?
        .unwrap_or_default();
    Ok(Some(CoreRaimWeights::BySatellite(quality::weight_vector(
        &entries,
        variance_options,
    ))))
}

fn resolve_raim_weights(
    mode: Option<&str>,
    explicit: Option<CoreRaimWeights>,
    entries: Option<CoreRaimWeights>,
    empty_weights_as_unit: bool,
    empty_array: bool,
) -> Result<CoreRaimWeights, JsValue> {
    if explicit.is_some() && entries.is_some() {
        return Err(type_error("set either weights or weightEntries, not both"));
    }
    let supplied = explicit.or(entries);
    match mode {
        None => Ok(if empty_weights_as_unit && empty_array {
            CoreRaimWeights::Unit
        } else {
            supplied.unwrap_or_else(|| CoreRaimOptions::default().weights)
        }),
        Some("solution") => match supplied {
            None | Some(CoreRaimWeights::Solution) => Ok(CoreRaimWeights::Solution),
            Some(_) => Err(type_error(
                "weightsMode solution conflicts with supplied weights",
            )),
        },
        Some("unit") => match supplied {
            None | Some(CoreRaimWeights::Unit) => Ok(CoreRaimWeights::Unit),
            Some(_) => Err(type_error(
                "weightsMode unit conflicts with supplied weights",
            )),
        },
        Some("bySatellite") => match supplied {
            None => Ok(CoreRaimWeights::BySatellite(BTreeMap::new())),
            Some(weights @ CoreRaimWeights::BySatellite(_)) => Ok(weights),
            Some(_) => Err(type_error(
                "weightsMode bySatellite requires satellite weights",
            )),
        },
        Some(other) => Err(type_error(&format!(
            "invalid weightsMode {other:?}: expected solution, unit, or bySatellite"
        ))),
    }
}

/// Parse common raw-RAIM options for both `raim` and `raimForSolution`.
/// An explicitly empty legacy array can retain the FDE API's Unit convention.
pub(crate) fn options_from_js(
    options: &JsValue,
    empty_weights_as_unit: bool,
) -> Result<CoreRaimOptions, JsValue> {
    let input: RaimOptionsInput = if options.is_undefined() || options.is_null() {
        RaimOptionsInput::default()
    } else {
        serde_wasm_bindgen::from_value(options.clone())
            .map_err(|e| type_error(&format!("invalid RAIM options: {e}")))?
    };
    let explicit = if options.is_undefined() || options.is_null() {
        None
    } else {
        parse_weights_option(options)?
    };
    let entries = raim_weight_entries(&input)?;
    let empty_array = !options.is_undefined()
        && !options.is_null()
        && optional_property(options, "weights")?.is_some_and(|weights| {
            js_sys::Array::is_array(&weights) && js_sys::Array::from(&weights).length() == 0
        });
    let weights = resolve_raim_weights(
        input.weights_mode.as_deref(),
        explicit,
        entries,
        empty_weights_as_unit,
        empty_array,
    )?;

    let defaults = CoreRaimOptions::default();
    let p_fa = input.p_fa.unwrap_or(defaults.p_fa);
    if !p_fa.is_finite() || !(0.0 < p_fa && p_fa < 1.0) {
        return Err(crate::positioning_error::quality_error(
            QualityError::InvalidProbability,
        ));
    }
    let n_systems = input
        .n_systems
        .map(isize::try_from)
        .transpose()
        .map_err(|_| crate::positioning_error::quality_error(QualityError::InvalidSystemCount))?;
    let mut core = CoreRaimOptions::default();
    core.p_fa = p_fa;
    core.weights = weights;
    core.n_systems = n_systems;
    Ok(core)
}

fn property(value: &JsValue, name: &str) -> Result<JsValue, JsValue> {
    js_sys::Reflect::get(value, &JsValue::from_str(name))
        .map_err(|_| type_error(&format!("failed to read {name}")))
}

fn optional_property(value: &JsValue, name: &str) -> Result<Option<JsValue>, JsValue> {
    let property = property(value, name)?;
    if property.is_undefined() || property.is_null() {
        Ok(None)
    } else {
        Ok(Some(property))
    }
}

fn parse_weights_option(options: &JsValue) -> Result<Option<CoreRaimWeights>, JsValue> {
    if options.is_undefined() || options.is_null() {
        return Ok(None);
    }
    optional_property(options, "weights")?
        .map(parse_weights_value)
        .transpose()
}

fn parse_weights_value(value: JsValue) -> Result<CoreRaimWeights, JsValue> {
    if let Some(mode) = value.as_string() {
        return match mode.as_str() {
            "solution" => Ok(CoreRaimWeights::Solution),
            "unit" => Ok(CoreRaimWeights::Unit),
            "bySatellite" => Ok(CoreRaimWeights::BySatellite(BTreeMap::new())),
            other => Err(type_error(&format!(
                "invalid RAIM weights mode {other:?}: expected solution, unit, or bySatellite"
            ))),
        };
    }
    if let Some(mode) = optional_property(&value, "mode")? {
        let mode = mode
            .as_string()
            .ok_or_else(|| type_error("weights.mode must be a string"))?;
        return match mode.as_str() {
            "solution" => Ok(CoreRaimWeights::Solution),
            "unit" => Ok(CoreRaimWeights::Unit),
            "bySatellite" => {
                if let Some(satellite_ids) = optional_property(&value, "satelliteIds")? {
                    let satellite_ids: Vec<String> = serde_wasm_bindgen::from_value(satellite_ids)
                        .map_err(|e| {
                            type_error(&format!("invalid RAIM weight satelliteIds: {e}"))
                        })?;
                    let weights = optional_property(&value, "weights")?
                        .or(optional_property(&value, "values")?);
                    let weights: Vec<f64> = match weights {
                        Some(values) => serde_wasm_bindgen::from_value(values)
                            .map_err(|e| type_error(&format!("invalid RAIM weight values: {e}")))?,
                        None if satellite_ids.is_empty() => Vec::new(),
                        None => return Err(type_error("weights must include weights values")),
                    };
                    weights_from_vectors(&satellite_ids, &weights)
                } else {
                    Err(type_error(
                        "bySatellite weights require satelliteIds and values",
                    ))
                }
            }
            other => Err(type_error(&format!(
                "invalid weights.mode {other:?}: expected solution, unit, or bySatellite"
            ))),
        };
    }
    if property(&value, "isUnit")?.as_bool().unwrap_or(false) {
        return Ok(CoreRaimWeights::Unit);
    }
    if property(&value, "isSolution")?.as_bool().unwrap_or(false) {
        return Ok(CoreRaimWeights::Solution);
    }
    if let Some(satellite_ids) = optional_property(&value, "satelliteIds")? {
        let satellite_ids: Vec<String> = serde_wasm_bindgen::from_value(satellite_ids)
            .map_err(|e| type_error(&format!("invalid RAIM weight satelliteIds: {e}")))?;
        let weights = optional_property(&value, "weights")?
            .or(optional_property(&value, "values")?)
            .ok_or_else(|| type_error("weights must include weights values"))?;
        let weights: Vec<f64> = serde_wasm_bindgen::from_value(weights)
            .map_err(|e| type_error(&format!("invalid RAIM weight values: {e}")))?;
        return weights_from_vectors(&satellite_ids, &weights);
    }
    let input: RaimWeightsInput = serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid RAIM weights: {e}")))?;
    input.to_core()
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RaimWeightsInput {
    Vector {
        #[serde(rename = "satelliteIds")]
        satellite_ids: Vec<String>,
        #[serde(default)]
        weights: Option<Vec<f64>>,
        #[serde(default)]
        values: Option<Vec<f64>>,
    },
    Entries(Vec<WeightPairInput>),
    Map(BTreeMap<String, f64>),
}

impl RaimWeightsInput {
    fn to_core(&self) -> Result<CoreRaimWeights, JsValue> {
        match self {
            Self::Vector {
                satellite_ids,
                weights,
                values,
            } => {
                let weights = match (weights, values) {
                    (Some(_), Some(_)) => {
                        return Err(type_error("set either weights or values, not both"))
                    }
                    (Some(weights), None) | (None, Some(weights)) => weights,
                    (None, None) => return Err(type_error("weights must include weights values")),
                };
                weights_from_vectors(satellite_ids, weights)
            }
            Self::Entries(entries) => weights_from_pairs(
                entries
                    .iter()
                    .map(|entry| (entry.satellite_id.clone(), entry.weight)),
            ),
            Self::Map(map) => {
                weights_from_pairs(map.iter().map(|(id, weight)| (id.clone(), *weight)))
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WeightPairInput {
    satellite_id: String,
    weight: f64,
}

fn weights_from_pairs<I>(pairs: I) -> Result<CoreRaimWeights, JsValue>
where
    I: IntoIterator<Item = (String, f64)>,
{
    let mut map = BTreeMap::new();
    for (satellite_id, weight) in pairs {
        if !weight.is_finite() || weight <= 0.0 {
            return Err(crate::positioning_error::quality_error(
                QualityError::InvalidWeight,
            ));
        }
        map.insert(satellite_id, weight);
    }
    Ok(CoreRaimWeights::BySatellite(map))
}

fn weights_from_vectors(
    satellite_ids: &[String],
    weights: &[f64],
) -> Result<CoreRaimWeights, JsValue> {
    if satellite_ids.len() != weights.len() {
        return Err(type_error(
            "satelliteIds and weights must have the same length",
        ));
    }
    weights_from_pairs(satellite_ids.iter().cloned().zip(weights.iter().copied()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WeightEntryInput {
    satellite_id: String,
    elevation_deg: f64,
    #[serde(default)]
    cn0_dbhz: Option<f64>,
}

impl WeightEntryInput {
    fn to_core(&self) -> CoreWeightEntry {
        CoreWeightEntry {
            satellite_id: self.satellite_id.clone(),
            elevation_deg: self.elevation_deg,
            cn0_dbhz: self.cn0_dbhz,
        }
    }
}

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
    fn to_core(&self) -> Result<PseudorangeVarianceOptions, JsValue> {
        let model = match self.model.as_deref() {
            None | Some("elevation") => PseudorangeVarianceModel::Elevation,
            Some("elevation_cn0") => PseudorangeVarianceModel::ElevationCn0,
            Some(other) => {
                return Err(type_error(&format!(
                    "invalid variance model {other:?}: expected \"elevation\" or \"elevation_cn0\""
                )))
            }
        };
        let defaults = PseudorangeVarianceOptions::default();
        let mut options = PseudorangeVarianceOptions::default();
        options.a_m = self.a_m.unwrap_or(defaults.a_m);
        options.b_m = self.b_m.unwrap_or(defaults.b_m);
        options.model = model;
        options.cn0_dbhz = self.cn0_dbhz;
        options.cn0_scale_m2 = self.cn0_scale_m2.unwrap_or(defaults.cn0_scale_m2);
        Ok(options)
    }
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RaimResultObject {
    fault_detected: bool,
    test_statistic: f64,
    threshold: Option<f64>,
    testable: bool,
    worst_sat: Option<String>,
    reduced_chi_square: Option<f64>,
    normalized_residuals: BTreeMap<String, f64>,
    rms_m: f64,
    dof: isize,
}

pub(crate) fn result_from_core(result: CoreRaimResult, input: &CoreRaimInput) -> RaimResultObject {
    let rms_m = if input.residuals_m.is_empty() {
        0.0
    } else {
        (input
            .residuals_m
            .iter()
            .map(|residual| residual * residual)
            .sum::<f64>()
            / input.residuals_m.len() as f64)
            .sqrt()
    };
    let reduced_chi_square = if result.dof > 0 {
        Some(result.test_statistic / result.dof as f64)
    } else {
        None
    };
    RaimResultObject {
        fault_detected: result.fault_detected,
        test_statistic: result.test_statistic,
        threshold: result.threshold,
        testable: result.testable,
        worst_sat: result.worst_sat,
        reduced_chi_square,
        normalized_residuals: result.normalized_residuals,
        rms_m,
        dof: result.dof,
    }
}

/// Run direct post-solve residual RAIM.
///
/// `input` is `{ usedSats, residualsM, variancesM2? }`. Under the default
/// solution-weight mode, `variancesM2` is required and supplies the variances
/// used by the solve; alternatively set `weights: { isUnit: true }` or provide
/// explicit per-satellite weights. Use `options` fields `pFa`, `nSystems`,
/// and either `weights` or `weightEntries` with `varianceOptions`.
/// The result has the fault flag, chi-square statistic, threshold, largest
/// normalized residual satellite, reduced chi-square, normalized residual map,
/// RMS residual, and degrees of freedom.
#[wasm_bindgen(js_name = raim)]
pub fn raim(input: JsValue, options: JsValue) -> Result<JsValue, JsValue> {
    let input: RaimInput = serde_wasm_bindgen::from_value(input)
        .map_err(|e| type_error(&format!("invalid RAIM input: {e}")))?;
    let core_input = input.to_core();
    let core_options = options_from_js(&options, false)?;
    let result = core_raim(&core_input, &core_options).map_err(quality_error)?;
    to_js(&result_from_core(result, &core_input))
}

/// Run residual chi-square RAIM directly over an existing SPP solution.
///
/// `solution` is a `SppSolution` from `solveSpp`, `solveBroadcast`, fallback,
/// or a RINEX OBS convenience result. `options` is the same `RaimOptions` object
/// accepted by `raim`; omit it for the core defaults.
#[wasm_bindgen(js_name = raimForSolution)]
pub fn raim_for_solution(solution: &SppSolution, options: JsValue) -> Result<JsValue, JsValue> {
    let options = options_from_js(&options, false)?;
    let result = core_raim_for_solution(&solution.inner, &options).map_err(quality_error)?;
    let input = CoreRaimInput {
        used_sats: solution
            .inner
            .used_sats
            .iter()
            .map(ToString::to_string)
            .collect(),
        residuals_m: solution.inner.residuals_m.clone(),
        variances_m2: Some(solution.inner.pseudorange_variances_m2.clone()),
    };
    to_js(&result_from_core(result, &input))
}

/// One linearized range measurement: `{ id, residualM, designRow, weight }`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RangeFdeRowInput {
    id: String,
    residual_m: f64,
    design_row: Vec<f64>,
    weight: f64,
}

impl RangeFdeRowInput {
    fn to_core(&self) -> RangeFdeRow {
        RangeFdeRow {
            id: self.id.clone(),
            residual_m: self.residual_m,
            design_row: self.design_row.clone(),
            weight: self.weight,
        }
    }
}

/// RAIM/FDE options. Every field is optional and falls back to the core default
/// (`pFa` the demo5 `1.0e-3`, one exclusion, `minRedundancy` 1, and the
/// RTKLIB demo5 100 m residual RMS cap).
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct RangeFdeOptionsInput {
    p_fa: Option<f64>,
    max_exclusions: Option<usize>,
    min_redundancy: Option<usize>,
    max_exclusion_rms_m: Option<f64>,
}

impl RangeFdeOptionsInput {
    fn to_core(&self) -> RangeFdeOptions {
        let defaults = RangeFdeOptions::default();
        let mut options = RangeFdeOptions::default();
        options.p_fa = self.p_fa.unwrap_or(defaults.p_fa);
        options.max_exclusions = self.max_exclusions.unwrap_or(defaults.max_exclusions);
        options.min_redundancy = self.min_redundancy.unwrap_or(defaults.min_redundancy);
        options.max_exclusion_rms_m = self
            .max_exclusion_rms_m
            .unwrap_or(defaults.max_exclusion_rms_m);
        options
    }
}

// --- result mirror objects --------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChiSquareTestObject {
    weighted_sum_squares: f64,
    dof: isize,
    threshold: Option<f64>,
    testable: bool,
    fault_detected: bool,
}

impl From<&RangeChiSquareTest> for ChiSquareTestObject {
    fn from(t: &RangeChiSquareTest) -> Self {
        Self {
            weighted_sum_squares: t.weighted_sum_squares,
            dof: t.dof,
            threshold: t.threshold,
            testable: t.testable,
            fault_detected: t.fault_detected,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticObject {
    id: String,
    excluded: bool,
    post_fit_residual_m: f64,
    normalized_residual: f64,
}

impl From<&RangeMeasurementDiagnostic> for DiagnosticObject {
    fn from(d: &RangeMeasurementDiagnostic) -> Self {
        Self {
            id: d.id.clone(),
            excluded: d.excluded,
            post_fit_residual_m: d.post_fit_residual_m,
            normalized_residual: d.normalized_residual,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RangeFdeResultObject {
    state_correction: Vec<f64>,
    state_covariance: Vec<Vec<f64>>,
    global_test: ChiSquareTestObject,
    excluded: Vec<String>,
    diagnostics: Vec<DiagnosticObject>,
    iterations: usize,
}

impl From<&RangeFdeResult> for RangeFdeResultObject {
    fn from(r: &RangeFdeResult) -> Self {
        Self {
            state_correction: r.state_correction.clone(),
            state_covariance: r.state_covariance.clone(),
            global_test: ChiSquareTestObject::from(&r.global_test),
            excluded: r.excluded.clone(),
            diagnostics: r.diagnostics.iter().map(DiagnosticObject::from).collect(),
            iterations: r.iterations,
        }
    }
}

/// Run standalone range RAIM/FDE over a linearized measurement set.
///
/// `rows` is an array of `{ id, residualM, designRow, weight }` objects (each
/// `designRow` the measurement's design-matrix row, of length equal to the
/// estimated state dimension). `options` is an optional `{ pFa?, maxExclusions?,
/// minRedundancy?, maxExclusionRmsM? }` object (pass `undefined` for the core
/// defaults). Returns the protected state correction, covariance, global
/// chi-square test, the excluded ids, the per-measurement diagnostics, and the exclusion count. FDE
/// computation runs in `sidereon_core::quality::raim_fde_design`.
/// Malformed input throws a `TypeError`; a rank-deficient or rejected set throws
/// an `Error`.
#[wasm_bindgen(js_name = raimFdeDesign)]
pub fn raim_fde_design_js(rows: JsValue, options: JsValue) -> Result<JsValue, JsValue> {
    let rows: Vec<RangeFdeRowInput> = serde_wasm_bindgen::from_value(rows)
        .map_err(|e| type_error(&format!("invalid RAIM/FDE rows: {e}")))?;
    let options: RangeFdeOptionsInput = if options.is_undefined() || options.is_null() {
        RangeFdeOptionsInput::default()
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid RAIM/FDE options: {e}")))?
    };

    let core_rows: Vec<RangeFdeRow> = rows.iter().map(RangeFdeRowInput::to_core).collect();
    let result = raim_fde_design(&core_rows, &options.to_core())
        .map_err(crate::positioning_error::quality_error)?;

    to_js(&RangeFdeResultObject::from(&result))
}
