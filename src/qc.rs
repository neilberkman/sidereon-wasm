//! Quality control: fault detection and exclusion (FDE) over the SPP solver.
//!
//! `sidereon_core::quality::fde_spp` is the single core driver for SPP with
//! RAIM-gated detection, exclusion, re-solve, and per-candidate solution
//! validation. This module is a thin wrapper: it marshals the JS request into
//! the driver's inputs/options, calls `fde_spp` against the SP3 ephemeris, and
//! packages the surviving solution plus the excluded satellites. No RAIM,
//! exclusion, validation, or solve loop lives here.

use std::str::FromStr;

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use sidereon_core::positioning::{
    Corrections, EphemerisSource, KlobucharCoeffs, Observation, ReceiverSolution, RobustConfig,
    SolveInputs, SurfaceMet, DEFAULT_HUBER_K, DEFAULT_ROBUST_MAX_OUTER, DEFAULT_ROBUST_OUTER_TOL_M,
    DEFAULT_ROBUST_SCALE_FLOOR_M,
};
use sidereon_core::quality::{
    self, FdeError, FdeOptions, FdeSppError, FdeSppOptions, SolutionValidationOptions,
};
use sidereon_core::GnssSatelliteId;

use crate::error::{range_error, type_error};

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
    /// Single-frequency (default) or ionosphere-free code, as on `solveSpp`.
    #[serde(default)]
    pseudorange_code: crate::spp::PseudorangeCodeInput,
    #[serde(default)]
    qzss_clock: crate::spp::QzssClockInput,
    #[serde(default)]
    troposphere_model: crate::spp::TroposphereModelInput,
    /// Maximum exclusions; defaults to the core's single-exclusion policy.
    #[serde(default)]
    max_exclusions: Option<usize>,
    /// Largest residual RMS (metres) an exclusion may leave; defaults to 100 m.
    #[serde(default)]
    max_exclusion_rms_m: Option<f64>,
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
        let mut cfg = RobustConfig::default();
        cfg.huber_k = huber_k;
        cfg.scale_floor_m = scale_floor_m;
        cfg.max_outer = max_outer;
        cfg.outer_tol_m = outer_tol_m;
        Ok(cfg)
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
    raim: crate::raim::RaimResultObject,
}

#[wasm_bindgen]
impl FdeSolution {
    /// The complete accepted receiver solution, including its covariance,
    /// variances, effective weights, status, clocks and diagnostics.
    #[wasm_bindgen(getter)]
    pub fn solution(&self) -> crate::spp::SppSolution {
        crate::spp::SppSolution::from_inner(self.solution.clone())
    }

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

    /// The accepted solution's detection test, including weighted residuals.
    #[wasm_bindgen(getter, unchecked_return_type = "FdeRaimResult")]
    pub fn raim(&self) -> Result<JsValue, JsValue> {
        crate::raim::to_js(&self.raim)
    }
}

fn fde_error_to_js(err: FdeError<ReceiverSolution, FdeSppError>) -> JsValue {
    crate::positioning_error::positioning_error(&crate::positioning_error::fde_detail(&err))
}

/// Run FDE against the given ephemeris under the core RAIM-gated exclusion loop.
/// Omitted `weightsMode` and weights use the solution's pseudorange variances;
/// accepted modes are `solution`, `unit`, and `bySatellite`. An explicitly
/// empty legacy `weights` array keeps its unit-weight meaning. The default
/// exclusion budget is one and the default candidate residual RMS cap is 100 m.
pub fn fde(eph: &dyn EphemerisSource, request: JsValue) -> Result<FdeSolution, JsValue> {
    if js_sys::Reflect::has(&request, &JsValue::from_str("maxIterations")).unwrap_or(false) {
        return Err(type_error(
            "maxIterations is unsupported; use maxExclusions",
        ));
    }
    let req: FdeRequest = serde_wasm_bindgen::from_value(request.clone())
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
        pseudorange_code: req.pseudorange_code.into(),
        qzss_clock: req.qzss_clock.into(),
        troposphere_model: req.troposphere_model.into(),
    };

    let defaults = FdeOptions::default();
    let mut fde = FdeOptions::new(
        crate::raim::options_from_js(&request, true)?,
        req.max_exclusions.unwrap_or(defaults.max_exclusions),
    );
    fde.max_exclusion_rms_m = req
        .max_exclusion_rms_m
        .unwrap_or(defaults.max_exclusion_rms_m);
    let mut validation = SolutionValidationOptions::default();
    validation.max_pdop = req.max_pdop;
    let options = FdeSppOptions::new(fde, validation);

    let result = if let Some(robust) = &req.robust {
        quality::spp_robust_fde_driver(
            eph,
            &inputs,
            req.with_geodetic,
            robust.to_config()?,
            &options,
        )
    } else {
        quality::fde_spp(eph, &inputs, req.with_geodetic, &options)
    }
    .map_err(fde_error_to_js)?;

    let raim_input = sidereon_core::quality::RaimInput {
        used_sats: result
            .solution
            .used_sats
            .iter()
            .map(ToString::to_string)
            .collect(),
        residuals_m: result.solution.residuals_m.clone(),
        variances_m2: Some(result.solution.pseudorange_variances_m2.clone()),
    };
    let raim = crate::raim::result_from_core(result.raim, &raim_input);
    Ok(FdeSolution {
        solution: result.solution,
        excluded: result.excluded,
        iterations: result.iterations,
        raim,
    })
}
