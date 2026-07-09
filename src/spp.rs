//! Single-point positioning. Marshals one idiomatic JS request object into the
//! core `SolveInputs` and returns the reference `ReceiverSolution`. The solve is
//! `sidereon::solve_spp` under the public default policy, unchanged.

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::ephemeris::Sp3 as CoreSp3;
use sidereon_core::positioning::{
    solve_spp_batch_serial, solve_spp_from_rinex_obs as core_solve_spp_from_rinex_obs,
    solve_with_doppler_velocity as core_solve_with_doppler_velocity,
    spp_inputs_from_rinex_obs as core_spp_inputs_from_rinex_obs, Corrections, DopplerObservation,
    EphemerisSource, KlobucharCoeffs, Observation, ReceiverSolution, RinexSppEpochSolution,
    RinexSppOptions as CoreRinexSppOptions, RobustConfig, SolveInputs, SolvePolicy,
    SppDopplerSolution as CoreSppDopplerSolution, SurfaceMet, DEFAULT_HUBER_K,
    DEFAULT_ROBUST_MAX_OUTER, DEFAULT_ROBUST_OUTER_TOL_M, DEFAULT_ROBUST_SCALE_FLOOR_M,
};
use sidereon_core::quality::SolutionValidationOptions;
use sidereon_core::rinex::observations::{
    ObsEpochTime as CoreObsEpochTime, SignalPolicy as CoreSignalPolicy,
};
use sidereon_core::{GnssSatelliteId, GnssSystem};

use crate::dop::Dop;
use crate::error::{engine_error, range_error, type_error};
use crate::geometry_quality::GeometryQuality;
use crate::marshal::mat3_flat;
use crate::observables::VelocitySolution;
use crate::rinex_nav::BroadcastEphemeris;
use crate::rinex_obs::RinexObs;

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| engine_error(format!("failed to serialize result: {e}")))
}

/// One constellation's time DOP: `{ system: "gps", tdop: 1.23 }`.
#[derive(Serialize)]
struct SystemTdopJs {
    system: String,
    tdop: f64,
}

fn system_label(system: GnssSystem) -> &'static str {
    system.as_str()
}

/// One pseudorange observation: `{ satelliteId: "G01", pseudorangeM: 2.3e7 }`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ObservationInput {
    satellite_id: String,
    pseudorange_m: f64,
}

/// One Doppler observation: `{ satelliteId, dopplerHz, carrierHz, satClockDriftSS? }`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DopplerObservationInput {
    satellite_id: String,
    doppler_hz: f64,
    carrier_hz: f64,
    #[serde(default)]
    sat_clock_drift_s_s: f64,
}

/// Boolean correction switches. Both default off.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct CorrectionsInput {
    ionosphere: bool,
    troposphere: bool,
}

/// GPS Klobuchar ionosphere coefficients.
#[derive(Deserialize, Default)]
struct KlobucharInput {
    #[serde(default)]
    alpha: [f64; 4],
    #[serde(default)]
    beta: [f64; 4],
}

/// Surface meteorology for the troposphere model.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SurfaceMetInput {
    pressure_hpa: f64,
    temperature_k: f64,
    relative_humidity: f64,
}

impl Default for SurfaceMetInput {
    fn default() -> Self {
        Self {
            pressure_hpa: 1013.25,
            temperature_k: 288.15,
            relative_humidity: 0.5,
        }
    }
}

/// Opt-in Huber/IRLS robust-reweighting tuning. The presence of the `robust` key
/// on the request enables the core outer reweighting loop; every field is
/// optional and falls back to the engine default ([`RobustConfig::default`]), so
/// `robust: {}` enables it at the engine-default tuning.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct RobustInput {
    huber_k: Option<f64>,
    scale_floor_m: Option<f64>,
    max_outer: Option<usize>,
    outer_tol_m: Option<f64>,
}

impl RobustInput {
    /// Resolve to a core [`RobustConfig`], filling each absent field from the
    /// engine default and rejecting an out-of-domain tuning value at the boundary
    /// with a `RangeError` (the JS class for a bad numeric range), so a malformed
    /// knob never reaches the solver.
    fn to_config(&self) -> Result<RobustConfig, JsValue> {
        let huber_k = self.huber_k.unwrap_or(DEFAULT_HUBER_K);
        let scale_floor_m = self.scale_floor_m.unwrap_or(DEFAULT_ROBUST_SCALE_FLOOR_M);
        let outer_tol_m = self.outer_tol_m.unwrap_or(DEFAULT_ROBUST_OUTER_TOL_M);
        let max_outer = self.max_outer.unwrap_or(DEFAULT_ROBUST_MAX_OUTER);
        if !(huber_k.is_finite() && huber_k > 0.0) {
            return Err(range_error(
                "robust.huberK must be a finite positive number",
            ));
        }
        if !(scale_floor_m.is_finite() && scale_floor_m > 0.0) {
            return Err(range_error(
                "robust.scaleFloorM must be a finite positive number",
            ));
        }
        if !(outer_tol_m.is_finite() && outer_tol_m >= 0.0) {
            return Err(range_error(
                "robust.outerTolM must be a finite non-negative number",
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

/// The full SPP request. `observations`, `tRxJ2000S`, `tRxSecondOfDayS`, and
/// `dayOfYear` are required; the rest carry engine-standard defaults.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SppRequest {
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
    /// GLONASS FDMA channel numbers as `[slot, channel]` pairs, e.g.
    /// `[[1, 1], [2, -4]]`: each `slot` is the GLONASS satellite slot/PRN and
    /// each `channel` the FDMA frequency channel `k` (valid `[-7, +6]`). Absent
    /// or empty is correct for any solve with no GLONASS observation. A GLONASS
    /// observation solved with the ionosphere correction on but no channel here
    /// (or a channel outside the valid range) is rejected by the engine.
    #[serde(default)]
    glonass_channels: Vec<(u8, i8)>,
    #[serde(default = "default_true")]
    with_geodetic: bool,
    /// Opt-in Huber/IRLS robust reweighting. Absent runs the static
    /// elevation-weighted reference solve byte-identically; present routes through
    /// the core `SolveInputs.robust` outer reweighting loop. Shared by the SP3,
    /// broadcast-only, and fallback paths, since it is a property of the solve
    /// inputs.
    #[serde(default)]
    robust: Option<RobustInput>,
    /// Cold-start convergence-basin widening: the number of near-surface
    /// golden-spiral seeds the core `SolvePolicy` tries (plus the caller's
    /// `initialGuess`), selecting the best redundant converged fix. Absent is the
    /// single exact solve from `initialGuess`. Honored only on the SP3 `solveSpp`
    /// path (the policy-bearing core entry), not the broadcast/fallback paths.
    #[serde(default)]
    coarse_search_seeds: Option<usize>,
    /// Optional positive PDOP ceiling applied by the core `SolvePolicy` solution
    /// validation: a fix whose geometry is rank-deficient or exceeds this ceiling
    /// is refused with an `Error`. Honored only on the SP3 `solveSpp` path.
    #[serde(default)]
    max_pdop: Option<f64>,
}

fn default_true() -> bool {
    true
}

/// Build the core [`SolveInputs`] and the caller's `withGeodetic` flag from an
/// already-parsed request. The opt-in `robust` Huber/IRLS config is resolved
/// here because it is a property of the solve inputs, shared by every path.
fn build_solve_inputs(req: &SppRequest) -> Result<(SolveInputs, bool), JsValue> {
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

    let robust = match &req.robust {
        Some(r) => Some(r.to_config()?),
        None => None,
    };

    let inputs = SolveInputs {
        observations,
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
        robust,
    };

    Ok((inputs, req.with_geodetic))
}

/// Build the SP3-path [`SolvePolicy`] from a parsed request: the optional PDOP
/// validation ceiling and the optional coarse-search seed count. Out-of-domain
/// values are rejected at the boundary with a `RangeError`.
fn build_policy(req: &SppRequest) -> Result<SolvePolicy, JsValue> {
    make_policy(req.max_pdop, req.coarse_search_seeds)
}

/// Validate the optional PDOP ceiling and coarse-search seed count at the
/// boundary (a `RangeError` for an out-of-domain value) and assemble the core
/// [`SolvePolicy`]. Shared by the single-solve and batch paths.
fn make_policy(
    max_pdop: Option<f64>,
    coarse_search_seeds: Option<usize>,
) -> Result<SolvePolicy, JsValue> {
    if let Some(max_pdop) = max_pdop {
        if !(max_pdop.is_finite() && max_pdop > 0.0) {
            return Err(range_error("maxPdop must be a finite positive number"));
        }
    }
    if let Some(seeds) = coarse_search_seeds {
        if seeds < 1 {
            return Err(range_error("coarseSearchSeeds must be at least 1"));
        }
    }
    Ok(SolvePolicy {
        validation: SolutionValidationOptions {
            max_pdop,
            ..Default::default()
        },
        coarse_search_seeds,
    })
}

/// Marshal one SPP request JS object into the core [`SolveInputs`] and the
/// caller's `withGeodetic` flag. Shared by the broadcast-only path and the
/// precise-with-broadcast fallback (which take no [`SolvePolicy`]), so the
/// `coarseSearchSeeds` / `maxPdop` request fields are ignored on those paths;
/// the `robust` field is honored everywhere because it lives on the inputs.
pub(crate) fn build_inputs(request: JsValue) -> Result<(SolveInputs, bool), JsValue> {
    let req: SppRequest = serde_wasm_bindgen::from_value(request)
        .map_err(|e| type_error(&format!("invalid SPP request: {e}")))?;
    build_solve_inputs(&req)
}

/// Marshal one SPP request and solve against `eph` under the request's
/// [`SolvePolicy`] (coarse-search seeds, PDOP ceiling) and inputs (Huber/IRLS).
pub fn solve(eph: &CoreSp3, request: JsValue) -> Result<SppSolution, JsValue> {
    let req: SppRequest = serde_wasm_bindgen::from_value(request)
        .map_err(|e| type_error(&format!("invalid SPP request: {e}")))?;
    let (inputs, with_geodetic) = build_solve_inputs(&req)?;
    let policy = build_policy(&req)?;

    // Serial reference path: solve_spp, never the rayon batch variant.
    let solution = sidereon::solve_spp(eph as &dyn EphemerisSource, &inputs, with_geodetic, policy)
        .map_err(engine_error)?;

    Ok(SppSolution { inner: solution })
}

fn build_doppler_observations(value: JsValue) -> Result<Vec<DopplerObservation>, JsValue> {
    let rows: Vec<DopplerObservationInput> = serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid Doppler observations: {e}")))?;
    rows.into_iter()
        .map(|row| {
            Ok(DopplerObservation {
                satellite_id: GnssSatelliteId::from_str(&row.satellite_id).map_err(|_| {
                    type_error(&format!("invalid satellite token: {}", row.satellite_id))
                })?,
                doppler_hz: row.doppler_hz,
                carrier_hz: row.carrier_hz,
                sat_clock_drift_s_s: row.sat_clock_drift_s_s,
            })
        })
        .collect::<Result<Vec<_>, JsValue>>()
}

/// Marshal one SPP request and a Doppler row array into the core fused
/// position/velocity entry point.
pub fn solve_with_doppler_velocity(
    eph: &CoreSp3,
    request: JsValue,
    doppler_observations: JsValue,
) -> Result<SppDopplerSolution, JsValue> {
    let req: SppRequest = serde_wasm_bindgen::from_value(request)
        .map_err(|e| type_error(&format!("invalid SPP request: {e}")))?;
    let (inputs, with_geodetic) = build_solve_inputs(&req)?;
    let doppler_observations = build_doppler_observations(doppler_observations)?;
    let inner =
        core_solve_with_doppler_velocity(eph, &inputs, &doppler_observations, with_geodetic)
            .map_err(engine_error)?;
    Ok(SppDopplerSolution { inner })
}

/// Shared batch options applied to every epoch of a batch SPP solve. These are
/// the `solve_spp_batch_serial` parameters that core shares across the batch (the
/// `withGeodetic` flag and the `SolvePolicy`); the per-epoch entries carry only
/// the [`SolveInputs`]. Every field is optional: `withGeodetic` defaults to
/// `true`, and an absent `maxPdop` / `coarseSearchSeeds` is the engine default.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct SppBatchOptions {
    with_geodetic: Option<bool>,
    coarse_search_seeds: Option<usize>,
    max_pdop: Option<f64>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct RinexSppOptionsInput {
    signal_policy: Option<BTreeMap<String, Vec<String>>>,
    corrections: CorrectionsInput,
    initial_guess: Option<[f64; 4]>,
    satellites: Option<Vec<String>>,
    met: SurfaceMetInput,
    robust: Option<RobustInput>,
}

fn parse_gnss_system(label: &str) -> Result<GnssSystem, JsValue> {
    match label {
        "G" | "GPS" | "gps" | "Gps" => Ok(GnssSystem::Gps),
        "R" | "GLO" | "GLONASS" | "glonass" | "Glonass" => Ok(GnssSystem::Glonass),
        "E" | "GAL" | "GALILEO" | "galileo" | "Galileo" => Ok(GnssSystem::Galileo),
        "C" | "BDS" | "BEIDOU" | "beidou" | "BeiDou" => Ok(GnssSystem::BeiDou),
        "J" | "QZSS" | "qzss" | "Qzss" => Ok(GnssSystem::Qzss),
        "I" | "IRNSS" | "NAVIC" | "navic" | "NavIC" | "Navic" => Ok(GnssSystem::Navic),
        "S" | "SBAS" | "sbas" | "Sbas" => Ok(GnssSystem::Sbas),
        other => Err(type_error(&format!(
            "invalid GNSS system {other:?}: expected a RINEX system letter or label"
        ))),
    }
}

impl RinexSppOptionsInput {
    fn signal_policy(&self, obs: &RinexObs) -> Result<CoreSignalPolicy, JsValue> {
        if let Some(policy) = &self.signal_policy {
            let mut codes = BTreeMap::new();
            for (system, preferred_codes) in policy {
                codes.insert(parse_gnss_system(system)?, preferred_codes.clone());
            }
            Ok(CoreSignalPolicy { codes })
        } else {
            CoreSignalPolicy::default_for(obs.inner.header().version).map_err(engine_error)
        }
    }

    fn to_core(&self, obs: &RinexObs) -> Result<CoreRinexSppOptions, JsValue> {
        let mut options =
            CoreRinexSppOptions::new(self.signal_policy(obs)?).with_corrections(Corrections {
                ionosphere: self.corrections.ionosphere,
                troposphere: self.corrections.troposphere,
            });
        if let Some(initial_guess) = self.initial_guess {
            options = options.with_initial_guess(initial_guess);
        }
        if let Some(satellites) = &self.satellites {
            let satellites = satellites
                .iter()
                .map(|satellite| {
                    GnssSatelliteId::from_str(satellite)
                        .map_err(|_| type_error(&format!("invalid satellite token: {satellite}")))
                })
                .collect::<Result<BTreeSet<_>, _>>()?;
            options = options.with_satellites(satellites);
        }
        options = options.with_surface_met(SurfaceMet {
            pressure_hpa: self.met.pressure_hpa,
            temperature_k: self.met.temperature_k,
            relative_humidity: self.met.relative_humidity,
        });
        if let Some(robust) = &self.robust {
            options = options.with_robust(Some(robust.to_config()?));
        }
        Ok(options)
    }
}

fn rinex_spp_options(obs: &RinexObs, value: JsValue) -> Result<CoreRinexSppOptions, JsValue> {
    let input: RinexSppOptionsInput = if value.is_undefined() || value.is_null() {
        RinexSppOptionsInput::default()
    } else {
        serde_wasm_bindgen::from_value(value)
            .map_err(|e| type_error(&format!("invalid RINEX SPP options: {e}")))?
    };
    input.to_core(obs)
}

fn rinex_solve_policy(value: JsValue) -> Result<(bool, SolvePolicy), JsValue> {
    let options: SppBatchOptions = if value.is_undefined() || value.is_null() {
        SppBatchOptions::default()
    } else {
        serde_wasm_bindgen::from_value(value)
            .map_err(|e| type_error(&format!("invalid RINEX SPP solve options: {e}")))?
    };
    Ok((
        options.with_geodetic.unwrap_or(true),
        make_policy(options.max_pdop, options.coarse_search_seeds)?,
    ))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RinexSppEpochTimeObject {
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: f64,
}

impl From<CoreObsEpochTime> for RinexSppEpochTimeObject {
    fn from(epoch: CoreObsEpochTime) -> Self {
        Self {
            year: epoch.year,
            month: epoch.month,
            day: epoch.day,
            hour: epoch.hour,
            minute: epoch.minute,
            second: epoch.second,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RinexSppObservationObject {
    satellite_id: String,
    pseudorange_m: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RinexSppCorrectionsObject {
    ionosphere: bool,
    troposphere: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RinexSppEpochInputsObject {
    epoch_index: usize,
    epoch: RinexSppEpochTimeObject,
    observations: Vec<RinexSppObservationObject>,
    t_rx_j2000_s: f64,
    t_rx_second_of_day_s: f64,
    day_of_year: f64,
    initial_guess: [f64; 4],
    corrections: RinexSppCorrectionsObject,
    glonass_channels: Vec<[i32; 2]>,
}

impl From<&sidereon_core::positioning::RinexSppEpochInputs> for RinexSppEpochInputsObject {
    fn from(epoch: &sidereon_core::positioning::RinexSppEpochInputs) -> Self {
        Self {
            epoch_index: epoch.epoch_index,
            epoch: epoch.epoch.into(),
            observations: epoch
                .inputs
                .observations
                .iter()
                .map(|observation| RinexSppObservationObject {
                    satellite_id: observation.satellite_id.to_string(),
                    pseudorange_m: observation.pseudorange_m,
                })
                .collect(),
            t_rx_j2000_s: epoch.inputs.t_rx_j2000_s,
            t_rx_second_of_day_s: epoch.inputs.t_rx_second_of_day_s,
            day_of_year: epoch.inputs.day_of_year,
            initial_guess: epoch.inputs.initial_guess,
            corrections: RinexSppCorrectionsObject {
                ionosphere: epoch.inputs.corrections.ionosphere,
                troposphere: epoch.inputs.corrections.troposphere,
            },
            glonass_channels: epoch
                .inputs
                .glonass_channels
                .iter()
                .map(|(slot, channel)| [i32::from(*slot), i32::from(*channel)])
                .collect(),
        }
    }
}

/// Assemble parsed RINEX OBS epochs into broadcast-backed SPP solve inputs.
///
/// `source` is a parsed RINEX NAV broadcast store. `options` accepts
/// `signalPolicy`, `corrections`, `initialGuess`, `satellites`, `met`, and
/// `robust`; omit it for the core default policy for the observation file.
#[wasm_bindgen(js_name = sppInputsFromRinexObs)]
pub fn spp_inputs_from_rinex_obs_js(
    source: &BroadcastEphemeris,
    obs: &RinexObs,
    options: JsValue,
) -> Result<JsValue, JsValue> {
    let options = rinex_spp_options(obs, options)?;
    let epochs = core_spp_inputs_from_rinex_obs(&obs.inner, &source.inner, &options)
        .map_err(engine_error)?;
    let out: Vec<RinexSppEpochInputsObject> =
        epochs.iter().map(RinexSppEpochInputsObject::from).collect();
    to_js(&out)
}

/// Solve parsed RINEX OBS epochs serially against a broadcast ephemeris store.
///
/// `rinexOptions` controls observation assembly; `solveOptions` accepts
/// `withGeodetic`, `maxPdop`, and `coarseSearchSeeds`. The returned batch keeps
/// per-epoch solve failures, so use `isOk(index)` before `solution(index)`.
#[wasm_bindgen(js_name = solveSppFromRinexObs)]
pub fn solve_spp_from_rinex_obs_js(
    source: &BroadcastEphemeris,
    obs: &RinexObs,
    rinex_options: JsValue,
    solve_options: JsValue,
) -> Result<RinexSppSolutionBatch, JsValue> {
    let rinex_options = rinex_spp_options(obs, rinex_options)?;
    let (with_geodetic, policy) = rinex_solve_policy(solve_options)?;
    let epochs = core_solve_spp_from_rinex_obs(
        &source.inner,
        &obs.inner,
        &rinex_options,
        with_geodetic,
        policy,
    )
    .map_err(engine_error)?;
    Ok(RinexSppSolutionBatch { epochs })
}

/// Per-epoch results returned by `solveSppFromRinexObs`.
#[wasm_bindgen]
pub struct RinexSppSolutionBatch {
    epochs: Vec<RinexSppEpochSolution>,
}

#[wasm_bindgen]
impl RinexSppSolutionBatch {
    /// Number of assembled RINEX epochs in the batch.
    #[wasm_bindgen(getter)]
    pub fn count(&self) -> usize {
        self.epochs.len()
    }

    /// Original RINEX OBS epoch index for assembled batch item `index`.
    #[wasm_bindgen(js_name = epochIndex)]
    pub fn epoch_index(&self, index: usize) -> Result<usize, JsValue> {
        self.epochs
            .get(index)
            .map(|epoch| epoch.epoch_index)
            .ok_or_else(|| range_error(&format!("epoch index {index} out of range")))
    }

    /// Whether assembled batch item `index` converged to a solution.
    #[wasm_bindgen(js_name = isOk)]
    pub fn is_ok(&self, index: usize) -> Result<bool, JsValue> {
        self.epochs
            .get(index)
            .map(|epoch| epoch.solution.is_ok())
            .ok_or_else(|| range_error(&format!("epoch index {index} out of range")))
    }

    /// The SPP solution for assembled batch item `index`.
    pub fn solution(&self, index: usize) -> Result<SppSolution, JsValue> {
        match self
            .epochs
            .get(index)
            .ok_or_else(|| range_error(&format!("epoch index {index} out of range")))?
            .solution
            .as_ref()
        {
            Ok(solution) => Ok(SppSolution::from_inner(solution.clone())),
            Err(error) => Err(engine_error(error.to_string())),
        }
    }

    /// The solve error for assembled batch item `index`, or `undefined`.
    pub fn error(&self, index: usize) -> Result<Option<String>, JsValue> {
        match self
            .epochs
            .get(index)
            .ok_or_else(|| range_error(&format!("epoch index {index} out of range")))?
            .solution
            .as_ref()
        {
            Ok(_) => Ok(None),
            Err(error) => Ok(Some(error.to_string())),
        }
    }
}

/// Solve a batch of independent SPP epochs against this shared ephemeris.
///
/// `epochs` is an array of SPP request objects (the same `SppRequest` shape as
/// `solveSpp`); only the [`SolveInputs`] portion of each entry is used. The
/// `withGeodetic` flag and the `SolvePolicy` (PDOP ceiling, coarse-search seeds)
/// are shared across the whole batch and taken from `options`, so any
/// `withGeodetic` / `maxPdop` / `coarseSearchSeeds` set on an individual entry is
/// ignored. Element `i` of the result corresponds to `epochs[i]` and is either a
/// solution or that epoch's solve error. Delegates to the serial reference batch
/// kernel `sidereon_core::spp::solve_spp_batch_serial`; the binding never spawns
/// the rayon thread pool the parallel variant uses.
pub fn solve_batch(
    eph: &CoreSp3,
    epochs: JsValue,
    options: JsValue,
) -> Result<SppBatchSolution, JsValue> {
    let reqs: Vec<SppRequest> = serde_wasm_bindgen::from_value(epochs)
        .map_err(|e| type_error(&format!("invalid SPP batch epochs: {e}")))?;
    let opts: SppBatchOptions = if options.is_undefined() || options.is_null() {
        SppBatchOptions::default()
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid SPP batch options: {e}")))?
    };

    let with_geodetic = opts.with_geodetic.unwrap_or(true);
    let policy = make_policy(opts.max_pdop, opts.coarse_search_seeds)?;

    let mut inputs = Vec::with_capacity(reqs.len());
    for req in &reqs {
        // Reuse the single-solve marshalling; the per-request `withGeodetic` it
        // also returns is discarded because the batch shares one flag.
        let (input, _per_epoch_geodetic) = build_solve_inputs(req)?;
        inputs.push(input);
    }

    let results =
        solve_spp_batch_serial(eph as &dyn EphemerisSource, &inputs, with_geodetic, policy);

    let epochs: Vec<Result<ReceiverSolution, String>> = results
        .into_iter()
        .map(|result| result.map_err(|e| e.to_string()))
        .collect();

    Ok(SppBatchSolution { epochs })
}

/// The per-epoch results of a batch SPP solve, index-aligned to the input epochs.
/// Each epoch independently either converged to a solution or failed; query an
/// epoch with [`SppBatchSolution.isOk`] / [`SppBatchSolution.solution`] /
/// [`SppBatchSolution.error`].
#[wasm_bindgen]
pub struct SppBatchSolution {
    epochs: Vec<Result<ReceiverSolution, String>>,
}

#[wasm_bindgen]
impl SppBatchSolution {
    /// Number of epochs in the batch (equals the input epoch count).
    #[wasm_bindgen(getter)]
    pub fn count(&self) -> usize {
        self.epochs.len()
    }

    /// Whether epoch `index` converged to a solution. Throws a `RangeError` for
    /// an out-of-range index.
    #[wasm_bindgen(js_name = isOk)]
    pub fn is_ok(&self, index: usize) -> Result<bool, JsValue> {
        self.epochs
            .get(index)
            .map(Result::is_ok)
            .ok_or_else(|| range_error(&format!("epoch index {index} out of range")))
    }

    /// The solution for epoch `index`. Throws a `RangeError` for an out-of-range
    /// index and an `Error` carrying that epoch's solve-failure message when the
    /// epoch did not converge (check [`SppBatchSolution.isOk`] first).
    pub fn solution(&self, index: usize) -> Result<SppSolution, JsValue> {
        match self
            .epochs
            .get(index)
            .ok_or_else(|| range_error(&format!("epoch index {index} out of range")))?
        {
            Ok(solution) => Ok(SppSolution {
                inner: solution.clone(),
            }),
            Err(message) => Err(engine_error(message.clone())),
        }
    }

    /// The solve-failure message for epoch `index`, or `undefined` when the epoch
    /// converged. Throws a `RangeError` for an out-of-range index.
    pub fn error(&self, index: usize) -> Result<Option<String>, JsValue> {
        match self
            .epochs
            .get(index)
            .ok_or_else(|| range_error(&format!("epoch index {index} out of range")))?
        {
            Ok(_) => Ok(None),
            Err(message) => Ok(Some(message.clone())),
        }
    }
}

/// Position solution with an optional Doppler velocity solve.
#[wasm_bindgen]
pub struct SppDopplerSolution {
    inner: CoreSppDopplerSolution,
}

#[wasm_bindgen]
impl SppDopplerSolution {
    /// Receiver position, clock, and covariance solution.
    #[wasm_bindgen(getter)]
    pub fn receiver(&self) -> SppSolution {
        SppSolution {
            inner: self.inner.receiver.clone(),
        }
    }

    /// Doppler-derived receiver velocity and clock drift, if the velocity rows solved.
    #[wasm_bindgen(getter)]
    pub fn velocity(&self) -> Option<VelocitySolution> {
        self.inner
            .velocity
            .clone()
            .map(VelocitySolution::from_inner)
    }

    /// Velocity-solve failure text when Doppler rows were present but unusable.
    #[wasm_bindgen(getter, js_name = velocityError)]
    pub fn velocity_error(&self) -> Option<String> {
        self.inner.velocity_error.map(|error| error.to_string())
    }
}

/// The result of an SPP solve.
#[wasm_bindgen]
pub struct SppSolution {
    pub(crate) inner: ReceiverSolution,
}

impl SppSolution {
    /// Wrap a core [`ReceiverSolution`] (used by the broadcast and fallback
    /// paths, which solve through their own core entry points).
    pub(crate) fn from_inner(inner: ReceiverSolution) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl SppSolution {
    /// ECEF position as a `Float64Array` `[x, y, z]`, metres.
    #[wasm_bindgen(getter, js_name = positionM)]
    pub fn position_m(&self) -> Vec<f64> {
        vec![
            self.inner.position.x_m,
            self.inner.position.y_m,
            self.inner.position.z_m,
        ]
    }

    /// ECEF X, metres.
    #[wasm_bindgen(getter, js_name = xM)]
    pub fn x_m(&self) -> f64 {
        self.inner.position.x_m
    }

    /// ECEF Y, metres.
    #[wasm_bindgen(getter, js_name = yM)]
    pub fn y_m(&self) -> f64 {
        self.inner.position.y_m
    }

    /// ECEF Z, metres.
    #[wasm_bindgen(getter, js_name = zM)]
    pub fn z_m(&self) -> f64 {
        self.inner.position.z_m
    }

    /// Receiver clock bias, seconds.
    #[wasm_bindgen(getter, js_name = rxClockS)]
    pub fn rx_clock_s(&self) -> f64 {
        self.inner.rx_clock_s
    }

    /// Receiver clock drift in seconds per second when a Doppler solve was fused.
    #[wasm_bindgen(getter, js_name = rxClockDriftSS)]
    pub fn rx_clock_drift_s_s(&self) -> Option<f64> {
        self.inner.rx_clock_drift_s_s
    }

    /// ECEF position covariance, flat row-major 3-by-3 in square metres.
    #[wasm_bindgen(getter, js_name = positionCovarianceEcefM2)]
    pub fn position_covariance_ecef_m2(&self) -> Vec<f64> {
        mat3_flat(&self.inner.position_covariance.ecef_m2)
    }

    /// ENU position covariance, flat row-major 3-by-3 in square metres.
    #[wasm_bindgen(getter, js_name = positionCovarianceEnuM2)]
    pub fn position_covariance_enu_m2(&self) -> Vec<f64> {
        mat3_flat(&self.inner.position_covariance.enu_m2)
    }

    /// `[latRad, lonRad, heightM]` as a `Float64Array` if the solve was asked
    /// for geodetic output, otherwise `undefined`.
    #[wasm_bindgen(getter)]
    pub fn geodetic(&self) -> Option<Vec<f64>> {
        self.inner
            .geodetic
            .map(|g| vec![g.lat_rad, g.lon_rad, g.height_m])
    }

    /// Satellite tokens used in the accepted solution, ascending.
    #[wasm_bindgen(getter, js_name = usedSats)]
    pub fn used_sats(&self) -> Vec<String> {
        self.inner
            .used_sats
            .iter()
            .map(|sat| sat.to_string())
            .collect()
    }

    /// Post-fit residuals, metres, index-aligned to `usedSats`.
    #[wasm_bindgen(getter, js_name = residualsM)]
    pub fn residuals_m(&self) -> Vec<f64> {
        self.inner.residuals_m.clone()
    }

    /// Geometry observability and covariance-validation diagnostics for this
    /// solved design. `ZeroRedundancy` marks unvalidated snapshot covariance
    /// bounds, `Weak` leaves large bounds unclamped, and rank-deficient designs
    /// are returned as a singular-geometry `Error` rather than a solution.
    #[wasm_bindgen(getter, js_name = geometryQuality)]
    pub fn geometry_quality(&self) -> GeometryQuality {
        self.inner.geometry_quality.into()
    }

    /// Degrees of freedom in the accepted solve: `usedCount - (3 + clocks)`.
    #[wasm_bindgen(getter)]
    pub fn redundancy(&self) -> i32 {
        self.inner.metadata.redundancy as i32
    }

    /// Whether residual-based RAIM can test the accepted solve.
    #[wasm_bindgen(getter, js_name = raimCheckable)]
    pub fn raim_checkable(&self) -> bool {
        self.inner.metadata.raim_checkable
    }

    /// Dilution-of-precision scalars (GDOP/PDOP/HDOP/VDOP/TDOP) from the
    /// converged geometry, or `undefined` when the converged geometry is
    /// rank-deficient. The same `Dop` produced by [`gnssDop`](crate::gnss_dop).
    #[wasm_bindgen(getter)]
    pub fn dop(&self) -> Option<Dop> {
        self.inner.dop.clone().map(Dop::from)
    }

    /// Per-constellation time (clock) DOP as a `{ system, tdop }[]`, one entry
    /// per GNSS in the solve in ascending system order (the same order as the
    /// per-system clocks). The first entry's `tdop` equals the reference clock's
    /// `dop.tdop`. Empty when the converged geometry is rank-deficient.
    #[wasm_bindgen(getter, js_name = systemTdops)]
    pub fn system_tdops(&self) -> Result<JsValue, JsValue> {
        let out: Vec<SystemTdopJs> = self
            .inner
            .system_tdops
            .iter()
            .map(|(system, tdop)| SystemTdopJs {
                system: system_label(*system).to_string(),
                tdop: *tdop,
            })
            .collect();
        serde_wasm_bindgen::to_value(&out).map_err(|e| engine_error(e.to_string()))
    }
}

#[cfg(test)]
mod drift_tests {
    //! The binding holds no robust-reweighting default of its own: every absent
    //! field resolves from the canonical `sidereon_core::positioning` constant.
    use super::*;

    #[test]
    fn robust_defaults_track_core() {
        let config = RobustInput::default()
            .to_config()
            .expect("engine-default robust tuning is valid");
        assert_eq!(config.huber_k, DEFAULT_HUBER_K);
        assert_eq!(config.scale_floor_m, DEFAULT_ROBUST_SCALE_FLOOR_M);
        assert_eq!(config.max_outer, DEFAULT_ROBUST_MAX_OUTER);
        assert_eq!(config.outer_tol_m, DEFAULT_ROBUST_OUTER_TOL_M);
    }
}
