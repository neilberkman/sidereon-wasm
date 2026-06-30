//! Static multi-epoch PPP: float and integer-fixed solves.
//!
//! Marshals the structured ionosphere-free epoch records, initial state, and
//! solve config from idiomatic JS objects into the `sidereon-core` PPP input
//! types, then calls `sidereon::solve_ppp_float` / `sidereon::solve_ppp_fixed`
//! against a parsed SP3 product. No modeling lives here.

use std::collections::BTreeMap;
use std::str::FromStr;

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use sidereon_core::atmosphere::troposphere::Met;
use sidereon_core::observables::ObservableEphemerisSource;
use sidereon_core::positioning::SurfaceMet;
use sidereon_core::ppp_corrections::CivilDateTime;
use sidereon_core::precise_positioning::{
    defaults::{
        AMBIGUITY_TOLERANCE_M, CLOCK_TOLERANCE_M, MAX_ITERATIONS, POSITION_TOLERANCE_M,
        RATIO_THRESHOLD, ZTD_TOLERANCE_M,
    },
    solve_ppp_auto_init_fixed, solve_ppp_auto_init_float, FixedAmbiguityOptions, FixedSolution,
    FixedSolveConfig, FloatEpoch, FloatObservation, FloatSolution, FloatSolveConfig,
    FloatSolveOptions, FloatState, IntegerStatus, MeasurementWeights, PppAutoInitOptions,
    PppInitialGuess, RangeCorrections, TropoMapping, TroposphereOptions, VmfSiteSample,
    VmfSiteSeries,
};
use sidereon_core::GnssSatelliteId;

use crate::error::{engine_error, type_error};
use crate::sp3::Sp3;

// --- input objects ----------------------------------------------------------

/// Civil epoch timestamp for a PPP epoch.
#[derive(Deserialize)]
struct CivilInput {
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: f64,
}

impl CivilInput {
    fn to_core(&self) -> CivilDateTime {
        CivilDateTime {
            year: self.year,
            month: self.month,
            day: self.day,
            hour: self.hour,
            minute: self.minute,
            second: self.second,
        }
    }
}

/// One ionosphere-free code/phase observation in a PPP epoch.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ObservationInput {
    satellite_id: String,
    ambiguity_id: String,
    code_m: f64,
    phase_m: f64,
    #[serde(default)]
    freq1_hz: f64,
    #[serde(default)]
    freq2_hz: f64,
}

impl ObservationInput {
    fn to_core(&self) -> Result<FloatObservation, JsValue> {
        let sat = GnssSatelliteId::from_str(&self.satellite_id)
            .map_err(|_| type_error(&format!("invalid satellite token: {}", self.satellite_id)))?;
        Ok(FloatObservation {
            sat,
            satellite_id: self.satellite_id.clone(),
            ambiguity_id: self.ambiguity_id.clone(),
            code_m: self.code_m,
            phase_m: self.phase_m,
            freq1_hz: self.freq1_hz,
            freq2_hz: self.freq2_hz,
        })
    }
}

/// One static PPP epoch.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EpochInput {
    civil: CivilInput,
    jd_whole: f64,
    jd_fraction: f64,
    t_rx_j2000_s: f64,
    observations: Vec<ObservationInput>,
}

impl EpochInput {
    fn to_core(&self) -> Result<FloatEpoch, JsValue> {
        let observations = self
            .observations
            .iter()
            .map(ObservationInput::to_core)
            .collect::<Result<Vec<_>, JsValue>>()?;
        Ok(FloatEpoch {
            epoch: self.civil.to_core(),
            jd_whole: self.jd_whole,
            jd_fraction: self.jd_fraction,
            t_rx_j2000_s: self.t_rx_j2000_s,
            observations,
        })
    }
}

fn epochs_to_core(epochs: &[EpochInput]) -> Result<Vec<FloatEpoch>, JsValue> {
    epochs.iter().map(EpochInput::to_core).collect()
}

/// Initial PPP state.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StateInput {
    position_m: [f64; 3],
    clocks_m: Vec<f64>,
    ambiguities_m: BTreeMap<String, f64>,
    #[serde(default)]
    ztd_m: f64,
}

impl StateInput {
    fn to_core(&self) -> FloatState {
        FloatState {
            position_m: self.position_m,
            clocks_m: self.clocks_m.clone(),
            ambiguities_m: self.ambiguities_m.clone(),
            ztd_m: self.ztd_m,
        }
    }
}

/// PPP measurement weights.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct WeightsInput {
    code: f64,
    phase: f64,
    elevation_weighting: bool,
}

impl Default for WeightsInput {
    fn default() -> Self {
        Self {
            code: 1.0,
            phase: 100.0,
            elevation_weighting: false,
        }
    }
}

impl WeightsInput {
    fn to_core(&self) -> MeasurementWeights {
        MeasurementWeights {
            code: self.code,
            phase: self.phase,
            elevation_weighting: self.elevation_weighting,
        }
    }
}

/// One VMF1 site-wise `a`-coefficient sample: `{ mjd, ah, aw }`.
///
/// The VMF data products tabulate the hydrostatic (`ah`) and wet (`aw`)
/// coefficients at the 00/06/12/18 UT nodes; supply the samples bracketing the
/// arc and the engine interpolates to each epoch.
#[derive(Deserialize)]
struct VmfSampleInput {
    mjd: f64,
    ah: f64,
    aw: f64,
}

impl VmfSampleInput {
    fn to_core(&self) -> VmfSiteSample {
        VmfSiteSample {
            mjd: self.mjd,
            ah: self.ah,
            aw: self.aw,
        }
    }
}

/// PPP troposphere controls.
///
/// When `vmf1` carries one or more samples the zenith delays are mapped with the
/// Vienna Mapping Function 1 (the site-wise `a`-coefficient series), otherwise
/// the climatological Niell (1996) mapping is used (no external data).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct TroposphereInput {
    enabled: bool,
    estimate_ztd: bool,
    pressure_hpa: f64,
    temperature_k: f64,
    relative_humidity: f64,
    vmf1: Vec<VmfSampleInput>,
}

impl Default for TroposphereInput {
    fn default() -> Self {
        let met = SurfaceMet::default();
        Self {
            enabled: false,
            estimate_ztd: false,
            pressure_hpa: met.pressure_hpa,
            temperature_k: met.temperature_k,
            relative_humidity: met.relative_humidity,
            vmf1: Vec::new(),
        }
    }
}

impl TroposphereInput {
    fn to_core(&self) -> Result<TroposphereOptions, JsValue> {
        if self.enabled {
            let met = Met::new(
                self.pressure_hpa,
                self.temperature_k,
                self.relative_humidity,
            )
            .map_err(engine_error)?;
            let mapping = if self.vmf1.is_empty() {
                TropoMapping::Niell
            } else {
                let samples: Vec<VmfSiteSample> =
                    self.vmf1.iter().map(VmfSampleInput::to_core).collect();
                TropoMapping::Vmf1(VmfSiteSeries::new(&samples).map_err(engine_error)?)
            };
            Ok(TroposphereOptions {
                enabled: true,
                estimate_ztd: self.estimate_ztd,
                met,
                mapping,
            })
        } else {
            Ok(TroposphereOptions::disabled())
        }
    }
}

/// Iteration and convergence controls for PPP.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct OptionsInput {
    max_iterations: usize,
    position_tolerance_m: f64,
    clock_tolerance_m: f64,
    ambiguity_tolerance_m: f64,
    ztd_tolerance_m: f64,
}

impl Default for OptionsInput {
    fn default() -> Self {
        Self {
            max_iterations: MAX_ITERATIONS,
            position_tolerance_m: POSITION_TOLERANCE_M,
            clock_tolerance_m: CLOCK_TOLERANCE_M,
            ambiguity_tolerance_m: AMBIGUITY_TOLERANCE_M,
            ztd_tolerance_m: ZTD_TOLERANCE_M,
        }
    }
}

impl OptionsInput {
    fn to_core(&self) -> FloatSolveOptions {
        FloatSolveOptions {
            max_iterations: self.max_iterations,
            position_tolerance_m: self.position_tolerance_m,
            clock_tolerance_m: self.clock_tolerance_m,
            ambiguity_tolerance_m: self.ambiguity_tolerance_m,
            ztd_tolerance_m: self.ztd_tolerance_m,
        }
    }
}

/// Complete typed configuration for a PPP float solve.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct FloatConfigInput {
    weights: WeightsInput,
    tropo: TroposphereInput,
    options: OptionsInput,
    residual_screen: bool,
}

impl FloatConfigInput {
    fn to_core(&self) -> Result<FloatSolveConfig, JsValue> {
        Ok(FloatSolveConfig {
            weights: self.weights.to_core(),
            tropo: self.tropo.to_core()?,
            corrections: RangeCorrections::disabled(),
            opts: self.options.to_core(),
            residual_screen: self.residual_screen,
        })
    }
}

/// Integer ambiguity controls for PPP fixed solving.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixedAmbiguityInput {
    wavelengths_m: BTreeMap<String, f64>,
    offsets_m: BTreeMap<String, f64>,
    #[serde(default = "default_ratio_threshold")]
    ratio_threshold: f64,
}

fn default_ratio_threshold() -> f64 {
    RATIO_THRESHOLD
}

impl FixedAmbiguityInput {
    fn to_core(&self) -> FixedAmbiguityOptions {
        FixedAmbiguityOptions {
            wavelengths_m: self.wavelengths_m.clone(),
            offsets_m: self.offsets_m.clone(),
            ratio_threshold: self.ratio_threshold,
        }
    }
}

/// Complete typed configuration for a PPP fixed solve.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixedConfigInput {
    ambiguity: FixedAmbiguityInput,
    #[serde(default)]
    weights: WeightsInput,
    #[serde(default)]
    tropo: TroposphereInput,
    #[serde(default)]
    options: OptionsInput,
}

impl FixedConfigInput {
    fn to_core(&self) -> Result<FixedSolveConfig, JsValue> {
        Ok(FixedSolveConfig {
            weights: self.weights.to_core(),
            tropo: self.tropo.to_core()?,
            corrections: RangeCorrections::disabled(),
            opts: self.options.to_core(),
            ambiguity: self.ambiguity.to_core(),
        })
    }
}

// --- auto-init driver options -----------------------------------------------

/// Explicit static-position/clock seed that bypasses the SPP auto-init stages.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InitialGuessInput {
    position_m: [f64; 3],
    clock_m: f64,
}

impl InitialGuessInput {
    fn to_core(&self) -> PppInitialGuess {
        PppInitialGuess {
            position_m: self.position_m,
            clock_m: self.clock_m,
        }
    }
}

/// SPP surface meteorology for the auto-init seed troposphere.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SppMetInput {
    pressure_hpa: f64,
    temperature_k: f64,
    relative_humidity: f64,
}

impl Default for SppMetInput {
    fn default() -> Self {
        let met = SurfaceMet::default();
        Self {
            pressure_hpa: met.pressure_hpa,
            temperature_k: met.temperature_k,
            relative_humidity: met.relative_humidity,
        }
    }
}

impl SppMetInput {
    fn to_core(&self) -> SurfaceMet {
        SurfaceMet {
            pressure_hpa: self.pressure_hpa,
            temperature_k: self.temperature_k,
            relative_humidity: self.relative_humidity,
        }
    }
}

/// Auto-initialization policy for the raw-epochs PPP driver. Every field is
/// optional: omit `initialGuess` to run the per-epoch SPP seed, and the SPP
/// cold-start guess defaults to all-zero with the troposphere off.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct AutoInitOptionsInput {
    initial_guess: Option<InitialGuessInput>,
    spp_initial_guess: [f64; 4],
    spp_troposphere: bool,
    spp_met: SppMetInput,
}

impl AutoInitOptionsInput {
    fn to_core(&self) -> PppAutoInitOptions {
        PppAutoInitOptions {
            initial_guess: self.initial_guess.as_ref().map(InitialGuessInput::to_core),
            spp_initial_guess: self.spp_initial_guess,
            spp_troposphere: self.spp_troposphere,
            spp_met: self.spp_met.to_core(),
        }
    }
}

fn auto_init_options(options: JsValue) -> Result<PppAutoInitOptions, JsValue> {
    let input: AutoInitOptionsInput = if options.is_undefined() || options.is_null() {
        AutoInitOptionsInput::default()
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid PPP auto-init options: {e}")))?
    };
    Ok(input.to_core())
}

// --- solve entry points -----------------------------------------------------

/// Solve a static multi-epoch float PPP arc from raw epochs, auto-initializing
/// the float state from the SPP seed.
///
/// Unlike [`solvePppFloat`], the caller does not supply an initial state: the
/// driver seeds it (per-epoch SPP code solve, mean position, phase-minus-code
/// ambiguities) before the float solve. `epochs` and `config` are the same plain
/// objects [`solvePppFloat`] takes; `options` is an optional `PppAutoInitOptions`
/// object (pass `undefined` for the SPP-seeded defaults). Delegates to
/// `sidereon_core::precise_positioning::auto_init::solve_ppp_auto_init_float`.
/// Throws a `TypeError` for malformed input and an `Error` if the solve fails.
#[wasm_bindgen(js_name = solvePppAutoInitFloat)]
pub fn solve_ppp_auto_init_float_js(
    sp3: &Sp3,
    epochs: JsValue,
    options: JsValue,
    config: JsValue,
) -> Result<PppFloatSolution, JsValue> {
    let epochs: Vec<EpochInput> = serde_wasm_bindgen::from_value(epochs)
        .map_err(|e| type_error(&format!("invalid PPP epochs: {e}")))?;
    let cfg: FloatConfigInput = serde_wasm_bindgen::from_value(config)
        .map_err(|e| type_error(&format!("invalid PPP float config: {e}")))?;

    let epochs = epochs_to_core(&epochs)?;
    let inner = solve_ppp_auto_init_float(
        &sp3.inner,
        &epochs,
        auto_init_options(options)?,
        cfg.to_core()?,
    )
    .map_err(engine_error)?;

    Ok(PppFloatSolution { inner })
}

/// Solve a static integer-fixed PPP arc from raw epochs: SPP auto-init seed, the
/// float solve, then the LAMBDA integer fix and ambiguity-conditioned re-solve.
///
/// `epochs`, `floatConfig`, and `fixedConfig` are the same plain objects the
/// float and fixed solves take; `options` is an optional `PppAutoInitOptions`
/// object. Delegates to
/// `sidereon_core::precise_positioning::auto_init::solve_ppp_auto_init_fixed`.
/// Throws a `TypeError` for malformed input and an `Error` if the solve fails.
#[wasm_bindgen(js_name = solvePppAutoInitFixed)]
pub fn solve_ppp_auto_init_fixed_js(
    sp3: &Sp3,
    epochs: JsValue,
    options: JsValue,
    float_config: JsValue,
    fixed_config: JsValue,
) -> Result<PppFixedSolution, JsValue> {
    let epochs: Vec<EpochInput> = serde_wasm_bindgen::from_value(epochs)
        .map_err(|e| type_error(&format!("invalid PPP epochs: {e}")))?;
    let float_cfg: FloatConfigInput = serde_wasm_bindgen::from_value(float_config)
        .map_err(|e| type_error(&format!("invalid PPP float config: {e}")))?;
    let fixed_cfg: FixedConfigInput = serde_wasm_bindgen::from_value(fixed_config)
        .map_err(|e| type_error(&format!("invalid PPP fixed config: {e}")))?;

    let epochs = epochs_to_core(&epochs)?;
    let inner = solve_ppp_auto_init_fixed(
        &sp3.inner,
        &epochs,
        auto_init_options(options)?,
        float_cfg.to_core()?,
        fixed_cfg.to_core()?,
    )
    .map_err(engine_error)?;

    Ok(PppFixedSolution { inner })
}

/// Solve a static multi-epoch float PPP arc against an SP3 product.
///
/// `epochs`, `initialState`, and `config` are plain objects; see the
/// `PppEpoch`, `PppFloatState`, and `PppFloatConfig` TypeScript types. Throws a
/// `TypeError` for malformed input and an `Error` if the solve fails.
#[wasm_bindgen(js_name = solvePppFloat)]
pub fn solve_ppp_float(
    sp3: &Sp3,
    epochs: JsValue,
    initial_state: JsValue,
    config: JsValue,
) -> Result<PppFloatSolution, JsValue> {
    let epochs: Vec<EpochInput> = serde_wasm_bindgen::from_value(epochs)
        .map_err(|e| type_error(&format!("invalid PPP epochs: {e}")))?;
    let state: StateInput = serde_wasm_bindgen::from_value(initial_state)
        .map_err(|e| type_error(&format!("invalid PPP initial state: {e}")))?;
    let cfg: FloatConfigInput = serde_wasm_bindgen::from_value(config)
        .map_err(|e| type_error(&format!("invalid PPP float config: {e}")))?;

    let epochs = epochs_to_core(&epochs)?;
    let inner = sidereon::solve_ppp_float(
        &sp3.inner as &dyn ObservableEphemerisSource,
        &epochs,
        state.to_core(),
        cfg.to_core()?,
    )
    .map_err(engine_error)?;

    Ok(PppFloatSolution { inner })
}

/// Search integer ambiguities from a float PPP solution and re-solve fixed.
///
/// `epochs` and `config` are plain objects (`PppEpoch`, `PppFixedConfig`);
/// `floatSolution` is the result of [`solvePppFloat`] over the same arc. Throws
/// a `TypeError` for malformed input and an `Error` if the solve fails.
#[wasm_bindgen(js_name = solvePppFixed)]
pub fn solve_ppp_fixed(
    sp3: &Sp3,
    epochs: JsValue,
    float_solution: &PppFloatSolution,
    config: JsValue,
) -> Result<PppFixedSolution, JsValue> {
    let epochs: Vec<EpochInput> = serde_wasm_bindgen::from_value(epochs)
        .map_err(|e| type_error(&format!("invalid PPP epochs: {e}")))?;
    let cfg: FixedConfigInput = serde_wasm_bindgen::from_value(config)
        .map_err(|e| type_error(&format!("invalid PPP fixed config: {e}")))?;

    let epochs = epochs_to_core(&epochs)?;
    let inner = sidereon::solve_ppp_fixed(
        &sp3.inner as &dyn ObservableEphemerisSource,
        &epochs,
        float_solution.inner.clone(),
        cfg.to_core()?,
    )
    .map_err(engine_error)?;

    Ok(PppFixedSolution { inner })
}

fn integer_status_label(status: IntegerStatus) -> String {
    match status {
        IntegerStatus::Fixed => "Fixed".to_string(),
        IntegerStatus::NotFixed => "NotFixed".to_string(),
    }
}

/// Serialize an id-keyed map to a plain JS object (not a JS `Map`).
fn map_f64_object(map: &BTreeMap<String, f64>) -> JsValue {
    use serde::Serialize;
    let serializer = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
    map.serialize(&serializer).unwrap_or(JsValue::UNDEFINED)
}

fn map_i64_object(map: &BTreeMap<String, i64>) -> JsValue {
    use serde::Serialize;
    let serializer = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
    map.serialize(&serializer).unwrap_or(JsValue::UNDEFINED)
}

// --- result objects ---------------------------------------------------------

/// Static float PPP solution.
#[wasm_bindgen]
pub struct PppFloatSolution {
    inner: FloatSolution,
}

#[wasm_bindgen]
impl PppFloatSolution {
    /// ECEF position as a `Float64Array` `[x, y, z]`, metres.
    #[wasm_bindgen(getter, js_name = positionM)]
    pub fn position_m(&self) -> Vec<f64> {
        self.inner.position_m.to_vec()
    }

    /// Float ambiguities, metres, as an id-keyed object.
    #[wasm_bindgen(getter, js_name = ambiguitiesM)]
    pub fn ambiguities_m(&self) -> JsValue {
        map_f64_object(&self.inner.ambiguities_m)
    }

    /// Estimated zenith tropospheric delay residual, metres, or `undefined`.
    #[wasm_bindgen(getter, js_name = ztdResidualM)]
    pub fn ztd_residual_m(&self) -> Option<f64> {
        self.inner.ztd_residual_m
    }

    #[wasm_bindgen(getter, js_name = codeRmsM)]
    pub fn code_rms_m(&self) -> f64 {
        self.inner.code_rms_m
    }

    #[wasm_bindgen(getter, js_name = phaseRmsM)]
    pub fn phase_rms_m(&self) -> f64 {
        self.inner.phase_rms_m
    }

    #[wasm_bindgen(getter, js_name = weightedRmsM)]
    pub fn weighted_rms_m(&self) -> f64 {
        self.inner.weighted_rms_m
    }

    #[wasm_bindgen(getter)]
    pub fn converged(&self) -> bool {
        self.inner.converged
    }

    #[wasm_bindgen(getter)]
    pub fn iterations(&self) -> usize {
        self.inner.iterations
    }

    /// Satellite tokens used in the accepted solution.
    #[wasm_bindgen(getter, js_name = usedSats)]
    pub fn used_sats(&self) -> Vec<String> {
        self.inner.used_sats.clone()
    }
}

/// Static integer-fixed PPP solution.
#[wasm_bindgen]
pub struct PppFixedSolution {
    inner: FixedSolution,
}

#[wasm_bindgen]
impl PppFixedSolution {
    /// ECEF position as a `Float64Array` `[x, y, z]`, metres.
    #[wasm_bindgen(getter, js_name = positionM)]
    pub fn position_m(&self) -> Vec<f64> {
        self.inner.position_m.to_vec()
    }

    /// Fixed ambiguities, integer cycles, as an id-keyed object.
    #[wasm_bindgen(getter, js_name = fixedAmbiguitiesCycles)]
    pub fn fixed_ambiguities_cycles(&self) -> JsValue {
        map_i64_object(&self.inner.fixed_ambiguities_cycles)
    }

    /// Fixed ambiguities, metres, as an id-keyed object.
    #[wasm_bindgen(getter, js_name = fixedAmbiguitiesM)]
    pub fn fixed_ambiguities_m(&self) -> JsValue {
        map_f64_object(&self.inner.fixed_ambiguities_m)
    }

    /// Estimated zenith tropospheric delay residual, metres, or `undefined`.
    #[wasm_bindgen(getter, js_name = ztdResidualM)]
    pub fn ztd_residual_m(&self) -> Option<f64> {
        self.inner.ztd_residual_m
    }

    /// The float solution that seeded the integer search.
    #[wasm_bindgen(getter, js_name = floatSolution)]
    pub fn float_solution(&self) -> PppFloatSolution {
        PppFloatSolution {
            inner: self.inner.float_solution.clone(),
        }
    }

    /// Integer ambiguity-fix status: `"Fixed"` or `"NotFixed"`.
    #[wasm_bindgen(getter, js_name = integerStatus)]
    pub fn integer_status(&self) -> String {
        integer_status_label(self.inner.integer.integer_status)
    }

    #[wasm_bindgen(getter, js_name = integerRatio)]
    pub fn integer_ratio(&self) -> f64 {
        self.inner.integer.integer_ratio
    }

    #[wasm_bindgen(getter, js_name = integerCandidates)]
    pub fn integer_candidates(&self) -> usize {
        self.inner.integer.integer_candidates
    }

    #[wasm_bindgen(getter, js_name = codeRmsM)]
    pub fn code_rms_m(&self) -> f64 {
        self.inner.code_rms_m
    }

    #[wasm_bindgen(getter, js_name = phaseRmsM)]
    pub fn phase_rms_m(&self) -> f64 {
        self.inner.phase_rms_m
    }

    #[wasm_bindgen(getter, js_name = weightedRmsM)]
    pub fn weighted_rms_m(&self) -> f64 {
        self.inner.weighted_rms_m
    }

    #[wasm_bindgen(getter)]
    pub fn converged(&self) -> bool {
        self.inner.converged
    }

    #[wasm_bindgen(getter)]
    pub fn iterations(&self) -> usize {
        self.inner.iterations
    }

    /// Satellite tokens used in the accepted solution.
    #[wasm_bindgen(getter, js_name = usedSats)]
    pub fn used_sats(&self) -> Vec<String> {
        self.inner.used_sats.clone()
    }
}

#[cfg(test)]
mod drift_tests {
    //! The PPP iteration, convergence-tolerance, and LAMBDA ratio defaults track
    //! the canonical core constants rather than literals duplicated here.
    use super::*;

    #[test]
    fn float_options_defaults_track_core() {
        let d = OptionsInput::default();
        assert_eq!(d.max_iterations, MAX_ITERATIONS);
        assert_eq!(d.position_tolerance_m, POSITION_TOLERANCE_M);
        assert_eq!(d.clock_tolerance_m, CLOCK_TOLERANCE_M);
        assert_eq!(d.ambiguity_tolerance_m, AMBIGUITY_TOLERANCE_M);
        assert_eq!(d.ztd_tolerance_m, ZTD_TOLERANCE_M);
    }

    #[test]
    fn fixed_ratio_threshold_tracks_core() {
        assert_eq!(default_ratio_threshold(), RATIO_THRESHOLD);
    }

    #[test]
    fn core_ppp_constants_pinned() {
        assert_eq!(MAX_ITERATIONS, 8);
        assert_eq!(POSITION_TOLERANCE_M, 1.0e-4);
        assert_eq!(CLOCK_TOLERANCE_M, 1.0e-4);
        assert_eq!(AMBIGUITY_TOLERANCE_M, 1.0e-4);
        assert_eq!(ZTD_TOLERANCE_M, 1.0e-4);
        assert_eq!(RATIO_THRESHOLD, 3.0);
    }
}
