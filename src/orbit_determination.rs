//! SP3-anchored numerical orbit fitting and residual ledgers.
//!
//! The functions in this module decode existing SP3 handles or precise-sample
//! arrays, call `sidereon_core::orbit_determination`, and serialize the report.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::astro::frames::orientation::TdbEarthOrientationProvider;
use sidereon_core::astro::math::least_squares::{SolveOptions, TrustRegionSolve};
use sidereon_core::astro::propagator::IntegratorOptions;
use sidereon_core::ephemeris::{
    fit_all_sp3_ecef_precise_orbits as core_fit_all_sp3_ecef_precise_orbits,
    fit_precise_ephemeris_sample_orbit as core_fit_precise_ephemeris_sample_orbit,
    fit_sp3_ecef_precise_orbit as core_fit_sp3_ecef_precise_orbit,
    fit_sp3_ecef_precise_orbits as core_fit_sp3_ecef_precise_orbits,
    fit_sp3_precise_orbit as core_fit_sp3_precise_orbit, OrbitArcSpan, OrbitFitCovariance,
    OrbitFitOptions, OrbitFitReport, OrbitFitSolution, OrbitResidualLedger, OrbitResidualStats,
};
use sidereon_core::geometry_quality::GeometryQualityThresholds;
use sidereon_core::{GnssSatelliteId, GnssSystem};

use crate::error::{engine_error, range_error, type_error};
use crate::force_model_input::{
    force_model_kind, integrator_kind, DragInput, ForceModelInput, IntegratorOptionsInput,
};
use crate::geometry_quality::GeometryQualityJs;
use crate::precise_samples::decode_core_samples;
use crate::sp3::Sp3;

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| engine_error(format!("failed to serialize result: {e}")))
}

fn parse_satellite(token: &str) -> Result<GnssSatelliteId, JsValue> {
    token
        .parse::<GnssSatelliteId>()
        .map_err(|e| type_error(&format!("invalid satellite token {token:?}: {e}")))
}

fn parse_satellites(value: JsValue) -> Result<Vec<GnssSatelliteId>, JsValue> {
    let tokens: Vec<String> = serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid satellites: {e}")))?;
    tokens.iter().map(|token| parse_satellite(token)).collect()
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct OrbitFitOptionsInput {
    force_model: Option<ForceModelInput>,
    mu_km3_s2: Option<f64>,
    integrator: Option<String>,
    integrator_options: Option<IntegratorOptionsInput>,
    solver_options: Option<SolverOptionsInput>,
    linear_solve: Option<String>,
    min_ledger_samples: Option<usize>,
    drag: Option<DragInput>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct SolverOptionsInput {
    gtol: Option<f64>,
    ftol: Option<f64>,
    xtol: Option<f64>,
    max_nfev: Option<usize>,
}

fn solver_options(input: Option<SolverOptionsInput>) -> SolveOptions {
    let defaults = SolveOptions::default();
    let input = input.unwrap_or_default();
    SolveOptions {
        gtol: input.gtol.unwrap_or(defaults.gtol),
        ftol: input.ftol.unwrap_or(defaults.ftol),
        xtol: input.xtol.unwrap_or(defaults.xtol),
        max_nfev: input.max_nfev.unwrap_or(defaults.max_nfev),
    }
}

fn linear_solve(label: Option<String>) -> Result<TrustRegionSolve, JsValue> {
    match label.as_deref().unwrap_or("ownedGaussianFirstTie") {
        "nalgebraLu" | "nalgebra_lu" => Ok(TrustRegionSolve::NalgebraLu),
        "ownedGaussianFirstTie" | "owned_gaussian_first_tie" => {
            Ok(TrustRegionSolve::OwnedGaussianFirstTie)
        }
        other => Err(type_error(&format!(
            "invalid orbit linearSolve {other:?}: expected \"nalgebraLu\" or \"ownedGaussianFirstTie\""
        ))),
    }
}

fn orbit_options(input: JsValue) -> Result<OrbitFitOptions, JsValue> {
    let input: OrbitFitOptionsInput = if input.is_undefined() || input.is_null() {
        OrbitFitOptionsInput::default()
    } else {
        serde_wasm_bindgen::from_value(input)
            .map_err(|e| type_error(&format!("invalid orbit-fit options: {e}")))?
    };
    let defaults = OrbitFitOptions::default();
    let integrator_options: IntegratorOptions =
        input.integrator_options.unwrap_or_default().to_core();
    if integrator_options.initial_step <= 0.0 {
        return Err(range_error(
            "integratorOptions.initialStepS must be positive",
        ));
    }
    Ok(OrbitFitOptions {
        force_model: force_model_kind(input.force_model.as_ref(), input.mu_km3_s2)?,
        integrator: integrator_kind(input.integrator.as_deref())?,
        integrator_options,
        solver_options: solver_options(input.solver_options),
        linear_solve: linear_solve(input.linear_solve)?,
        geometry_thresholds: GeometryQualityThresholds::default(),
        min_ledger_samples: input
            .min_ledger_samples
            .unwrap_or(defaults.min_ledger_samples),
        drag: input.drag.as_ref().map(DragInput::to_core).transpose()?,
        space_weather: None,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OrbitFitCovarianceJs {
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    matrix: Option<[[f64; 6]; 6]>,
}

impl From<OrbitFitCovariance> for OrbitFitCovarianceJs {
    fn from(value: OrbitFitCovariance) -> Self {
        match value {
            OrbitFitCovariance::Estimated { matrix } => Self {
                kind: "estimated",
                matrix: Some(*matrix),
            },
            OrbitFitCovariance::Unbounded => Self {
                kind: "unbounded",
                matrix: None,
            },
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OrbitFitSolutionJs {
    satellite: String,
    initial_epoch_s: f64,
    initial_position_km: [f64; 3],
    initial_velocity_km_s: [f64; 3],
    covariance: OrbitFitCovarianceJs,
    geometry_quality: GeometryQualityJs,
    seed_rms_3d_m: f64,
    fit_rms_3d_m: f64,
    iterations: usize,
}

impl From<OrbitFitSolution> for OrbitFitSolutionJs {
    fn from(value: OrbitFitSolution) -> Self {
        Self {
            satellite: value.satellite.to_string(),
            initial_epoch_s: value.initial_state.epoch_tdb_seconds,
            initial_position_km: value.initial_state.position_array(),
            initial_velocity_km_s: value.initial_state.velocity_array(),
            covariance: value.covariance.into(),
            geometry_quality: value.geometry_quality.into(),
            seed_rms_3d_m: value.seed_rms_3d_m,
            fit_rms_3d_m: value.fit_rms_3d_m,
            iterations: value.iterations,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OrbitArcSpanJs {
    time_scale: String,
    start_j2000_s: f64,
    end_j2000_s: f64,
    duration_s: f64,
}

impl From<OrbitArcSpan> for OrbitArcSpanJs {
    fn from(value: OrbitArcSpan) -> Self {
        Self {
            time_scale: value.time_scale.abbrev().to_string(),
            start_j2000_s: value.start_j2000_s,
            end_j2000_s: value.end_j2000_s,
            duration_s: value.duration_s,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OrbitResidualStatsJs {
    radial_rms_m: f64,
    along_rms_m: f64,
    cross_rms_m: f64,
    rms_3d_m: f64,
    n: usize,
    low_sample_count: bool,
}

impl From<OrbitResidualStats> for OrbitResidualStatsJs {
    fn from(value: OrbitResidualStats) -> Self {
        Self {
            radial_rms_m: value.radial_rms_m,
            along_rms_m: value.along_rms_m,
            cross_rms_m: value.cross_rms_m,
            rms_3d_m: value.rms_3d_m,
            n: value.n,
            low_sample_count: value.low_sample_count,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SatelliteResidualStatsJs {
    satellite: String,
    stats: OrbitResidualStatsJs,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConstellationResidualStatsJs {
    system: String,
    stats: OrbitResidualStatsJs,
}

fn system_label(system: GnssSystem) -> &'static str {
    system.as_str()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OrbitResidualLedgerJs {
    per_satellite: Vec<SatelliteResidualStatsJs>,
    per_constellation: Vec<ConstellationResidualStatsJs>,
    arc_span: OrbitArcSpanJs,
}

impl From<OrbitResidualLedger> for OrbitResidualLedgerJs {
    fn from(value: OrbitResidualLedger) -> Self {
        Self {
            per_satellite: value
                .per_sat
                .into_iter()
                .map(|(satellite, stats)| SatelliteResidualStatsJs {
                    satellite: satellite.to_string(),
                    stats: stats.into(),
                })
                .collect(),
            per_constellation: value
                .per_constellation
                .into_iter()
                .map(|(system, stats)| ConstellationResidualStatsJs {
                    system: system_label(system).to_string(),
                    stats: stats.into(),
                })
                .collect(),
            arc_span: value.arc_span.into(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OrbitFitReportJs {
    fits: Vec<OrbitFitSolutionJs>,
    ledger: OrbitResidualLedgerJs,
}

impl From<OrbitFitReport> for OrbitFitReportJs {
    fn from(value: OrbitFitReport) -> Self {
        Self {
            fits: value
                .fits
                .into_values()
                .map(OrbitFitSolutionJs::from)
                .collect(),
            ledger: value.ledger.into(),
        }
    }
}

/// Fit one satellite orbit from a parsed SP3 precise product.
///
/// `satellite` is an IGS token such as `"G01"`. `options` may set
/// `forceModel`, `integrator`, `integratorOptions`, `solverOptions`,
/// `linearSolve`, `minLedgerSamples`, and `drag`.
#[wasm_bindgen(js_name = fitSp3PreciseOrbit)]
pub fn fit_sp3_precise_orbit(
    sp3: &Sp3,
    satellite: &str,
    options: JsValue,
) -> Result<JsValue, JsValue> {
    let sat = parse_satellite(satellite)?;
    let options = orbit_options(options)?;
    let report = core_fit_sp3_precise_orbit(&sp3.inner, sat, &options).map_err(engine_error)?;
    to_js(&OrbitFitReportJs::from(report))
}

/// Fit one satellite orbit from a parsed ECEF SP3 precise product.
///
/// `satellite` is an IGS token such as `"G01"`. SP3 position and optional
/// velocity records are transformed from Earth-fixed coordinates through the
/// core `TdbEarthOrientationProvider` before fitting.
#[wasm_bindgen(js_name = fitSp3EcefPreciseOrbit)]
pub fn fit_sp3_ecef_precise_orbit(
    sp3: &Sp3,
    satellite: &str,
    options: JsValue,
) -> Result<JsValue, JsValue> {
    let sat = parse_satellite(satellite)?;
    let options = orbit_options(options)?;
    let provider = TdbEarthOrientationProvider::new();
    let report = core_fit_sp3_ecef_precise_orbit(&sp3.inner, sat, &provider, &options)
        .map_err(engine_error)?;
    to_js(&OrbitFitReportJs::from(report))
}

/// Fit selected satellite orbits from a parsed ECEF SP3 precise product.
///
/// `satellites` is a string array of IGS tokens such as `["G01", "G02"]`.
/// The Earth-fixed to inertial conversion is handled by the core provider.
#[wasm_bindgen(js_name = fitSp3EcefPreciseOrbits)]
pub fn fit_sp3_ecef_precise_orbits(
    sp3: &Sp3,
    satellites: JsValue,
    options: JsValue,
) -> Result<JsValue, JsValue> {
    let sats = parse_satellites(satellites)?;
    let options = orbit_options(options)?;
    let provider = TdbEarthOrientationProvider::new();
    let report = core_fit_sp3_ecef_precise_orbits(&sp3.inner, &sats, &provider, &options)
        .map_err(engine_error)?;
    to_js(&OrbitFitReportJs::from(report))
}

/// Fit every satellite declared in a parsed ECEF SP3 precise product.
///
/// The residual ledger is computed against the original Earth-fixed SP3
/// observations by the core fit path.
#[wasm_bindgen(js_name = fitAllSp3EcefPreciseOrbits)]
pub fn fit_all_sp3_ecef_precise_orbits(sp3: &Sp3, options: JsValue) -> Result<JsValue, JsValue> {
    let options = orbit_options(options)?;
    let provider = TdbEarthOrientationProvider::new();
    let report = core_fit_all_sp3_ecef_precise_orbits(&sp3.inner, &provider, &options)
        .map_err(engine_error)?;
    to_js(&OrbitFitReportJs::from(report))
}

/// Fit one satellite orbit from caller-supplied precise ephemeris samples.
///
/// `samples` is the same array accepted by `preciseEphemerisSamplesFromSamples`:
/// `{ sat, epoch, positionEcefM, clockS?, clockEvent? }` with epochs in seconds
/// since J2000.
#[wasm_bindgen(js_name = fitPreciseEphemerisSampleOrbit)]
pub fn fit_precise_ephemeris_sample_orbit(
    samples: JsValue,
    satellite: &str,
    options: JsValue,
) -> Result<JsValue, JsValue> {
    let samples = decode_core_samples(samples)?;
    let sat = parse_satellite(satellite)?;
    let options = orbit_options(options)?;
    let report =
        core_fit_precise_ephemeris_sample_orbit(&samples, sat, &options).map_err(engine_error)?;
    to_js(&OrbitFitReportJs::from(report))
}
