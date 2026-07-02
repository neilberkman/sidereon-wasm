//! Quality control: fault detection and exclusion (FDE) over the SPP solver.
//!
//! `sidereon_core::quality::fde_spp` is the single core driver for SPP with
//! RAIM-gated detection, exclusion, re-solve, and per-candidate solution
//! validation. This module is a thin wrapper: it marshals the JS request into
//! the driver's inputs/options, calls `fde_spp` against the SP3 ephemeris, and
//! packages the surviving solution plus the excluded satellites. No RAIM,
//! exclusion, validation, or solve loop lives here.

use std::collections::BTreeMap;
use std::str::FromStr;

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use sidereon_core::ephemeris::Sp3 as CoreSp3;
use sidereon_core::positioning::{
    Corrections, EphemerisSource, KlobucharCoeffs, Observation, ReceiverSolution, RobustConfig,
    SolveInputs, SurfaceMet, DEFAULT_HUBER_K, DEFAULT_ROBUST_MAX_OUTER, DEFAULT_ROBUST_OUTER_TOL_M,
    DEFAULT_ROBUST_SCALE_FLOOR_M,
};
use sidereon_core::quality::{
    self, FdeError, FdeOptions, FdeSppError, FdeSppOptions, RaimOptions, RaimWeights,
    SolutionValidationOptions,
};
use sidereon_core::GnssSatelliteId;

use crate::error::{engine_error, range_error, type_error};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ObservationInput {
    satellite_id: String,
    pseudorange_m: f64,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct CorrectionsInput {
    ionosphere: bool,
    troposphere: bool,
}

#[derive(Deserialize, Default)]
struct KlobucharInput {
    #[serde(default)]
    alpha: [f64; 4],
    #[serde(default)]
    beta: [f64; 4],
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SurfaceMetInput {
    pressure_hpa: f64,
    temperature_k: f64,
    relative_humidity: f64,
}

impl Default for SurfaceMetInput {
    fn default() -> Self {
        let met = SurfaceMet::default();
        Self {
            pressure_hpa: met.pressure_hpa,
            temperature_k: met.temperature_k,
            relative_humidity: met.relative_humidity,
        }
    }
}

/// One RAIM residual weight: `{ satelliteId, weight }`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WeightInput {
    satellite_id: String,
    weight: f64,
}

/// The FDE request: the SPP solve inputs plus the RAIM/exclusion options.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FdeRequest {
    observations: Vec<ObservationInput>,
    t_rx_j2000_s: f64,
    t_rx_second_of_day_s: f64,
    day_of_year: f64,
    #[serde(default)]
    initial_guess: [f64; 4],
    #[serde(default)]
    corrections: CorrectionsInput,
    #[serde(default)]
    klobuchar: KlobucharInput,
    #[serde(default)]
    met: SurfaceMetInput,
    #[serde(default)]
    glonass_channels: Vec<(u8, i8)>,
    #[serde(default = "default_true")]
    with_geodetic: bool,
    /// RAIM false-alarm probability; defaults to the core RAIM default.
    #[serde(default)]
    p_fa: Option<f64>,
    /// Per-satellite RAIM weights; absent or empty means unit weights.
    #[serde(default)]
    weights: Vec<WeightInput>,
    /// Override for the number of distinct GNSS clock systems.
    #[serde(default)]
    n_systems: Option<i64>,
    /// Maximum exclusions; defaults to `max(observationCount - 4, 0)`.
    #[serde(default)]
    max_iterations: Option<usize>,
    /// Optional PDOP ceiling applied to each candidate solution.
    #[serde(default)]
    max_pdop: Option<f64>,
    #[serde(default)]
    robust: Option<RobustInput>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct RobustInput {
    huber_k: Option<f64>,
    scale_floor_m: Option<f64>,
    max_outer: Option<usize>,
    outer_tol_m: Option<f64>,
}

impl RobustInput {
    fn to_config(&self) -> Result<RobustConfig, JsValue> {
        let huber_k = self.huber_k.unwrap_or(DEFAULT_HUBER_K);
        let scale_floor_m = self.scale_floor_m.unwrap_or(DEFAULT_ROBUST_SCALE_FLOOR_M);
        let outer_tol_m = self.outer_tol_m.unwrap_or(DEFAULT_ROBUST_OUTER_TOL_M);
        let max_outer = self.max_outer.unwrap_or(DEFAULT_ROBUST_MAX_OUTER);
        if !(huber_k.is_finite() && huber_k > 0.0) {
            return Err(range_error("robust.huberK must be finite and positive"));
        }
        if !(scale_floor_m.is_finite() && scale_floor_m > 0.0) {
            return Err(range_error(
                "robust.scaleFloorM must be finite and positive",
            ));
        }
        if !(outer_tol_m.is_finite() && outer_tol_m >= 0.0) {
            return Err(range_error(
                "robust.outerTolM must be finite and non-negative",
            ));
        }
        if max_outer < 1 {
            return Err(range_error("robust.maxOuter must be at least 1"));
        }
        Ok(RobustConfig {
            huber_k,
            scale_floor_m,
            max_outer,
            outer_tol_m,
        })
    }
}

fn default_true() -> bool {
    true
}

/// A fault-detection-and-exclusion result: the surviving solution, the excluded
/// satellites in exclusion order, and the number of exclusions performed.
#[wasm_bindgen]
pub struct FdeSolution {
    solution: ReceiverSolution,
    excluded: Vec<String>,
    iterations: usize,
}

#[wasm_bindgen]
impl FdeSolution {
    /// Surviving-solution ECEF position `[x, y, z]`, metres.
    #[wasm_bindgen(getter, js_name = positionM)]
    pub fn position_m(&self) -> Vec<f64> {
        vec![
            self.solution.position.x_m,
            self.solution.position.y_m,
            self.solution.position.z_m,
        ]
    }

    /// Receiver clock bias, seconds.
    #[wasm_bindgen(getter, js_name = rxClockS)]
    pub fn rx_clock_s(&self) -> f64 {
        self.solution.rx_clock_s
    }

    /// `[latRad, lonRad, heightM]` when geodetic output was requested.
    #[wasm_bindgen(getter)]
    pub fn geodetic(&self) -> Option<Vec<f64>> {
        self.solution
            .geodetic
            .map(|g| vec![g.lat_rad, g.lon_rad, g.height_m])
    }

    /// Satellite tokens used in the surviving solution, ascending.
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

    /// Excluded satellite tokens, in the order RAIM removed them.
    #[wasm_bindgen(getter)]
    pub fn excluded(&self) -> Vec<String> {
        self.excluded.clone()
    }

    /// Number of exclusions performed before the set passed RAIM.
    #[wasm_bindgen(getter)]
    pub fn iterations(&self) -> usize {
        self.iterations
    }
}

fn fde_error_to_js(err: FdeError<FdeSppError>) -> JsValue {
    match err {
        FdeError::FaultUnresolved(stat) => engine_error(format!(
            "RAIM fault unresolved after the exclusion budget, test statistic {stat}"
        )),
        FdeError::Solve(FdeSppError::Spp(e)) => engine_error(e),
        FdeError::Solve(FdeSppError::Validation(e)) => {
            engine_error(format!("solution validation rejected a candidate: {e:?}"))
        }
        FdeError::Raim(e) => engine_error(format!("RAIM configuration rejected: {e:?}")),
    }
}

fn raim_options(req: &FdeRequest) -> Result<RaimOptions, JsValue> {
    let defaults = RaimOptions::default();
    // The RAIM false-alarm probability is a caller-supplied out-of-domain
    // candidate, so reject a non-finite or out-of-(0,1) value at the boundary
    // with a RangeError (the JS class for a bad numeric range) rather than
    // letting it surface from the core as a generic Error.
    if let Some(p_fa) = req.p_fa {
        if !(p_fa.is_finite() && p_fa > 0.0 && p_fa < 1.0) {
            return Err(range_error(
                "pFa must be a finite number in the open interval (0, 1)",
            ));
        }
    }
    let weights = if req.weights.is_empty() {
        RaimWeights::Unit
    } else {
        RaimWeights::BySatellite(
            req.weights
                .iter()
                .map(|w| (w.satellite_id.clone(), w.weight))
                .collect::<BTreeMap<_, _>>(),
        )
    };
    Ok(RaimOptions {
        p_fa: req.p_fa.unwrap_or(defaults.p_fa),
        weights,
        n_systems: req.n_systems.map(|n| n as isize),
    })
}

/// Run FDE against the given ephemeris under the core RAIM-gated exclusion loop.
pub fn fde(eph: &CoreSp3, request: JsValue) -> Result<FdeSolution, JsValue> {
    let req: FdeRequest = serde_wasm_bindgen::from_value(request)
        .map_err(|e| type_error(&format!("invalid FDE request: {e}")))?;

    if req.observations.is_empty() {
        return Err(type_error("observations must contain at least one entry"));
    }

    let observations = req
        .observations
        .iter()
        .map(|obs| {
            let satellite_id = GnssSatelliteId::from_str(&obs.satellite_id).map_err(|_| {
                type_error(&format!("invalid satellite token: {}", obs.satellite_id))
            })?;
            Ok(Observation {
                satellite_id,
                pseudorange_m: obs.pseudorange_m,
            })
        })
        .collect::<Result<Vec<_>, JsValue>>()?;

    let inputs = SolveInputs {
        observations: observations.clone(),
        t_rx_j2000_s: req.t_rx_j2000_s,
        t_rx_second_of_day_s: req.t_rx_second_of_day_s,
        day_of_year: req.day_of_year,
        initial_guess: req.initial_guess,
        corrections: Corrections {
            ionosphere: req.corrections.ionosphere,
            troposphere: req.corrections.troposphere,
        },
        klobuchar: KlobucharCoeffs {
            alpha: req.klobuchar.alpha,
            beta: req.klobuchar.beta,
        },
        beidou_klobuchar: None,
        galileo_nequick: None,
        sbas_iono: None,
        glonass_channels: req.glonass_channels.iter().copied().collect(),
        met: SurfaceMet {
            pressure_hpa: req.met.pressure_hpa,
            temperature_k: req.met.temperature_k,
            relative_humidity: req.met.relative_humidity,
        },
        robust: None,
    };

    let options = FdeSppOptions {
        fde: FdeOptions {
            raim: raim_options(&req)?,
            max_iterations: req
                .max_iterations
                .unwrap_or_else(|| observations.len().saturating_sub(4)),
        },
        validation: SolutionValidationOptions {
            max_pdop: req.max_pdop,
            ..Default::default()
        },
    };

    let result = if let Some(robust) = &req.robust {
        quality::spp_robust_fde_driver(
            eph as &dyn EphemerisSource,
            &inputs,
            req.with_geodetic,
            robust.to_config()?,
            &options,
        )
    } else {
        quality::fde_spp(
            eph as &dyn EphemerisSource,
            &inputs,
            req.with_geodetic,
            &options,
        )
    }
    .map_err(fde_error_to_js)?;

    Ok(FdeSolution {
        solution: result.solution,
        excluded: result.excluded,
        iterations: result.iterations,
    })
}
