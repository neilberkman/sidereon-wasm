//! Code-differential GNSS (DGPS): base-station pseudorange corrections, rover
//! application, and the corrected-observation SPP solve.
//!
//! All modeling is `sidereon_core::dgnss` (`pseudorange_corrections`,
//! `apply_corrections`, `solve_position`); this module only marshals the JS
//! request objects and packages the report. The corrections and solve paths need
//! the precise/broadcast source, so they hang off the [`Sp3`](crate::Sp3) handle;
//! [`dgnssApply`] is sourceless.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::dgnss::{
    apply_corrections, pseudorange_corrections, solve_position, CodeObservation, DgnssError,
};
use sidereon_core::ephemeris::Sp3 as CoreSp3;
use sidereon_core::positioning::{
    Corrections, KlobucharCoeffs, ReceiverSolution, SolveInputs, SurfaceMet,
};

use crate::error::{engine_error, range_error, type_error};

/// One code pseudorange observation: `{ satelliteId: "G21", pseudorangeM: 2.3e7 }`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeObsInput {
    satellite_id: String,
    pseudorange_m: f64,
}

impl From<&CodeObsInput> for CodeObservation {
    fn from(o: &CodeObsInput) -> Self {
        CodeObservation::new(o.satellite_id.clone(), o.pseudorange_m)
    }
}

/// One per-satellite pseudorange correction: `{ satelliteId, correctionM }`.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorrectionEntry {
    satellite_id: String,
    correction_m: f64,
}

/// Request for [`Sp3.dgnssCorrections`]: the surveyed base and its observations.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CorrectionsRequest {
    base_position_m: [f64; 3],
    base_observations: Vec<CodeObsInput>,
    t_rx_j2000_s: f64,
}

/// Request for [`Sp3.dgnssSolve`]: base + rover observations and the receive-time
/// scalars. The differential removes the common path delays, so the corrected
/// solve runs with ionosphere/troposphere off (handled by the core); the
/// Klobuchar/met fields exist only to fill the engine `SolveInputs`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SolveRequest {
    base_position_m: [f64; 3],
    base_observations: Vec<CodeObsInput>,
    rover_observations: Vec<CodeObsInput>,
    t_rx_j2000_s: f64,
    t_rx_second_of_day_s: f64,
    day_of_year: f64,
    #[serde(default)]
    initial_guess: [f64; 4],
    #[serde(default = "default_true")]
    with_geodetic: bool,
}

fn default_true() -> bool {
    true
}

fn map_err(err: DgnssError) -> JsValue {
    match err {
        DgnssError::InvalidInput { field, reason } => {
            range_error(&format!("invalid DGNSS input {field}: {reason}"))
        }
        DgnssError::Spp(e) => engine_error(e),
    }
}

fn obs_vec(input: &[CodeObsInput]) -> Vec<CodeObservation> {
    input.iter().map(CodeObservation::from).collect()
}

fn corrections_to_js(map: BTreeMap<String, f64>) -> Result<JsValue, JsValue> {
    let entries: Vec<CorrectionEntry> = map
        .into_iter()
        .map(|(satellite_id, correction_m)| CorrectionEntry {
            satellite_id,
            correction_m,
        })
        .collect();
    serde_wasm_bindgen::to_value(&entries).map_err(|e| engine_error(e.to_string()))
}

/// Compute per-satellite pseudorange corrections from a surveyed base station.
///
/// Returns a `{ satelliteId, correctionM }[]` array sorted by satellite token.
pub fn corrections(eph: &CoreSp3, request: JsValue) -> Result<JsValue, JsValue> {
    let req: CorrectionsRequest = serde_wasm_bindgen::from_value(request)
        .map_err(|e| type_error(&format!("invalid DGNSS corrections request: {e}")))?;
    let base = obs_vec(&req.base_observations);
    let map = pseudorange_corrections(eph, req.base_position_m, &base, req.t_rx_j2000_s)
        .map_err(map_err)?;
    corrections_to_js(map)
}

/// Result of applying base corrections to rover observations.
#[wasm_bindgen]
pub struct AppliedCorrections {
    corrected: Vec<CodeObservation>,
    dropped: Vec<String>,
}

#[wasm_bindgen]
impl AppliedCorrections {
    /// Corrected rover observations as `{ satelliteId, pseudorangeM }[]`, in
    /// rover-observation order.
    #[wasm_bindgen(getter)]
    pub fn corrected(&self) -> Result<JsValue, JsValue> {
        let out: Vec<CorrectedObs> = self
            .corrected
            .iter()
            .map(|o| CorrectedObs {
                satellite_id: o.satellite_id.clone(),
                pseudorange_m: o.pseudorange_m,
            })
            .collect();
        serde_wasm_bindgen::to_value(&out).map_err(|e| engine_error(e.to_string()))
    }

    /// Rover satellite tokens with no matching base correction, in rover order.
    #[wasm_bindgen(getter)]
    pub fn dropped(&self) -> Vec<String> {
        self.dropped.clone()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CorrectedObs {
    satellite_id: String,
    pseudorange_m: f64,
}

/// Apply base pseudorange corrections to rover observations by satellite token.
///
/// `roverObservations` is `{ satelliteId, pseudorangeM }[]`; `corrections` is the
/// `{ satelliteId, correctionM }[]` array from [`Sp3.dgnssCorrections`]. Output
/// order follows the rover order; rover satellites with no correction are
/// reported in `dropped`.
#[wasm_bindgen(js_name = dgnssApply)]
pub fn dgnss_apply(
    rover_observations: JsValue,
    corrections: JsValue,
) -> Result<AppliedCorrections, JsValue> {
    let rover: Vec<CodeObsInput> = serde_wasm_bindgen::from_value(rover_observations)
        .map_err(|e| type_error(&format!("invalid rover observations: {e}")))?;
    let entries: Vec<CorrectionEntry> = serde_wasm_bindgen::from_value(corrections)
        .map_err(|e| type_error(&format!("invalid corrections: {e}")))?;
    let map: BTreeMap<String, f64> = entries
        .into_iter()
        .map(|e| (e.satellite_id, e.correction_m))
        .collect();
    let applied = apply_corrections(&obs_vec(&rover), &map).map_err(map_err)?;
    Ok(AppliedCorrections {
        corrected: applied.corrected,
        dropped: applied.dropped,
    })
}

/// A DGNSS rover solve: the corrected SPP solution plus the base-relative
/// baseline.
#[wasm_bindgen]
pub struct DgnssSolution {
    solution: ReceiverSolution,
    baseline_vector_m: [f64; 3],
    baseline_m: f64,
    dropped_sats: Vec<String>,
}

#[wasm_bindgen]
impl DgnssSolution {
    /// Corrected rover ECEF position `[x, y, z]`, metres.
    #[wasm_bindgen(getter, js_name = positionM)]
    pub fn position_m(&self) -> Vec<f64> {
        vec![
            self.solution.position.x_m,
            self.solution.position.y_m,
            self.solution.position.z_m,
        ]
    }

    /// Receiver clock bias, seconds (absorbs the base/rover clock difference).
    #[wasm_bindgen(getter, js_name = rxClockS)]
    pub fn rx_clock_s(&self) -> f64 {
        self.solution.rx_clock_s
    }

    /// `[latRad, lonRad, heightM]` when the solve was asked for geodetic output.
    #[wasm_bindgen(getter)]
    pub fn geodetic(&self) -> Option<Vec<f64>> {
        self.solution
            .geodetic
            .map(|g| vec![g.lat_rad, g.lon_rad, g.height_m])
    }

    /// Satellite tokens used in the accepted solution, ascending.
    #[wasm_bindgen(getter, js_name = usedSats)]
    pub fn used_sats(&self) -> Vec<String> {
        self.solution
            .used_sats
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    /// Post-fit residuals, metres, index-aligned to `usedSats`.
    #[wasm_bindgen(getter, js_name = residualsM)]
    pub fn residuals_m(&self) -> Vec<f64> {
        self.solution.residuals_m.clone()
    }

    /// Rover-minus-base ECEF vector `[dx, dy, dz]`, metres.
    #[wasm_bindgen(getter, js_name = baselineVectorM)]
    pub fn baseline_vector_m(&self) -> Vec<f64> {
        self.baseline_vector_m.to_vec()
    }

    /// Baseline length, metres.
    #[wasm_bindgen(getter, js_name = baselineM)]
    pub fn baseline_m(&self) -> f64 {
        self.baseline_m
    }

    /// Rover satellite tokens without a matching base correction.
    #[wasm_bindgen(getter, js_name = droppedSats)]
    pub fn dropped_sats(&self) -> Vec<String> {
        self.dropped_sats.clone()
    }
}

/// Compute corrections, apply them to the rover, and solve corrected SPP.
pub fn solve(eph: &CoreSp3, request: JsValue) -> Result<DgnssSolution, JsValue> {
    let req: SolveRequest = serde_wasm_bindgen::from_value(request)
        .map_err(|e| type_error(&format!("invalid DGNSS solve request: {e}")))?;

    // The base/rover tokens are validated inside the core solve; the SolveInputs
    // observations field is replaced by the corrected rover set, so it starts
    // empty. The core disables ionosphere/troposphere for the differential solve.
    let inputs = SolveInputs {
        observations: Vec::new(),
        t_rx_j2000_s: req.t_rx_j2000_s,
        t_rx_second_of_day_s: req.t_rx_second_of_day_s,
        day_of_year: req.day_of_year,
        initial_guess: req.initial_guess,
        corrections: Corrections::NONE,
        klobuchar: KlobucharCoeffs {
            alpha: [0.0; 4],
            beta: [0.0; 4],
        },
        beidou_klobuchar: None,
        galileo_nequick: None,
        glonass_channels: BTreeMap::new(),
        met: SurfaceMet {
            pressure_hpa: 1013.25,
            temperature_k: 288.15,
            relative_humidity: 0.5,
        },
        robust: None,
    };

    let base = obs_vec(&req.base_observations);
    let rover = obs_vec(&req.rover_observations);
    let result = solve_position(
        eph,
        req.base_position_m,
        &base,
        &rover,
        inputs,
        req.with_geodetic,
    )
    .map_err(map_err)?;

    Ok(DgnssSolution {
        solution: result.solution,
        baseline_vector_m: result.baseline_vector_m,
        baseline_m: result.baseline_m,
        dropped_sats: result.dropped_sats,
    })
}
