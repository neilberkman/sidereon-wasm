//! Time-of-closest-approach (TCA) finding, screening, and collision probability.
//!
//! Thin wrappers over `sidereon_core::astro::tca`. The SGP4 propagation, the
//! event-finder bracketing/refinement, and the collision-probability evaluation
//! all live in the crate; this layer parses TLE line pairs, marshals the search
//! window and finder/Pc options, and re-encodes the candidate / conjunction
//! results as plain JS objects. Only the serial screening path is used, so no
//! thread pool is spawned under wasm32.
//!
//! Search-window times cross as unix-microsecond UTC stamps (`bigint`), the same
//! time convention as the rest of the binding, and are converted to the SGP4
//! UTC Julian date the finder consumes.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::astro::conjunction::PcMethod;
use sidereon_core::astro::sgp4::JulianDate;
use sidereon_core::astro::tca::{
    find_tca_candidates_from_tles, find_tca_conjunctions_from_tles,
    screen_tca_candidates_from_tle_catalog_serial, screen_tca_conjunctions_from_tle_catalog_serial,
    TcaCandidate, TcaConjunction, TcaFinderOptions, TcaPcOptions, TcaScreeningConjunctionHit,
    TcaScreeningHit, TcaTle, TcaWindow,
};

use crate::error::{engine_error, type_error};

/// Unix epoch as a Julian date (JD of 1970-01-01T00:00:00Z).
const UNIX_EPOCH_JD: f64 = 2_440_587.5;
const MICROSECONDS_PER_DAY: f64 = 86_400_000_000.0;

/// Convert a unix-microsecond UTC stamp to the SGP4 split UTC Julian date,
/// `whole = floor(jd)` and `fraction` in `[0, 1)` (the Skyfield convention the
/// SGP4 epoch uses), so the finder's tsince matches the TLE epoch arithmetic.
fn julian_date_from_unix_us(unix_us: i64) -> JulianDate {
    let jd_total = unix_us as f64 / MICROSECONDS_PER_DAY + UNIX_EPOCH_JD;
    let whole = jd_total.floor();
    JulianDate(whole, jd_total - whole)
}

// --- options ----------------------------------------------------------------

/// Finder sampling controls, mirroring `TcaFinderOptions` (defaults: 60 s coarse
/// step, 1e-3 s time tolerance).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct FinderOptionsInput {
    coarse_step_seconds: f64,
    time_tolerance_seconds: f64,
}

impl Default for FinderOptionsInput {
    fn default() -> Self {
        let d = TcaFinderOptions::default();
        Self {
            coarse_step_seconds: d.coarse_step_seconds,
            time_tolerance_seconds: d.time_tolerance_seconds,
        }
    }
}

impl FinderOptionsInput {
    fn to_core(&self) -> TcaFinderOptions {
        TcaFinderOptions {
            coarse_step_seconds: self.coarse_step_seconds,
            time_tolerance_seconds: self.time_tolerance_seconds,
        }
    }
}

fn parse_finder_options(value: JsValue) -> Result<TcaFinderOptions, JsValue> {
    if value.is_undefined() || value.is_null() {
        Ok(TcaFinderOptions::default())
    } else {
        let input: FinderOptionsInput = serde_wasm_bindgen::from_value(value)
            .map_err(|e| type_error(&format!("invalid TCA finder options: {e}")))?;
        Ok(input.to_core())
    }
}

fn parse_pc_method(label: Option<&str>) -> Result<PcMethod, JsValue> {
    match label {
        None | Some("foster_equal_area") => Ok(PcMethod::FosterEqualArea),
        Some("foster_numerical") => Ok(PcMethod::FosterNumerical),
        Some("alfano_2005") => Ok(PcMethod::Alfano2005),
        Some(other) => Err(type_error(&format!(
            "unknown Pc method {other:?}; expected \"foster_equal_area\", \"foster_numerical\", or \"alfano_2005\""
        ))),
    }
}

/// Collision-probability options, mirroring `TcaPcOptions`. Per-object position
/// covariances are optional flat row-major length-9 (3-by-3, km^2) arrays; when
/// omitted the core's fallback TCA position covariance is used.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct PcOptionsInput {
    hard_body_radius_km: f64,
    method: Option<String>,
    primary_covariance_km2: Option<Vec<f64>>,
    secondary_covariance_km2: Option<Vec<f64>>,
}

fn mat3_from_flat(name: &str, values: &[f64]) -> Result<[[f64; 3]; 3], JsValue> {
    if values.len() != 9 {
        return Err(type_error(&format!(
            "{name} must have length 9 (flat row-major 3-by-3), got {}",
            values.len()
        )));
    }
    Ok([
        [values[0], values[1], values[2]],
        [values[3], values[4], values[5]],
        [values[6], values[7], values[8]],
    ])
}

fn parse_pc_options(value: JsValue) -> Result<TcaPcOptions, JsValue> {
    let input: PcOptionsInput = serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid TCA Pc options: {e}")))?;
    let method = parse_pc_method(input.method.as_deref())?;
    match (input.primary_covariance_km2, input.secondary_covariance_km2) {
        (None, None) => Ok(TcaPcOptions::with_default_covariance(
            input.hard_body_radius_km,
            method,
        )),
        (primary, secondary) => {
            let default = TcaPcOptions::with_default_covariance(input.hard_body_radius_km, method);
            let primary = match primary {
                Some(values) => mat3_from_flat("primaryCovarianceKm2", &values)?,
                None => default.covariances.primary_covariance_km2,
            };
            let secondary = match secondary {
                Some(values) => mat3_from_flat("secondaryCovarianceKm2", &values)?,
                None => default.covariances.secondary_covariance_km2,
            };
            Ok(TcaPcOptions::with_covariances(
                input.hard_body_radius_km,
                method,
                primary,
                secondary,
            ))
        }
    }
}

/// A borrowed TLE line pair.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TleInput {
    line1: String,
    line2: String,
}

fn parse_tle_catalog(value: JsValue) -> Result<Vec<TleInput>, JsValue> {
    serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid TLE catalog: {e}")))
}

// --- result objects ---------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CandidateObject {
    tca_jd_whole: f64,
    tca_jd_fraction: f64,
    tca_jd: f64,
    tca_seconds_since_window_start: f64,
    miss_distance_km: f64,
    relative_position_km: Vec<f64>,
    relative_velocity_km_s: Vec<f64>,
}

impl From<&TcaCandidate> for CandidateObject {
    fn from(c: &TcaCandidate) -> Self {
        Self {
            tca_jd_whole: c.tca_time.0,
            tca_jd_fraction: c.tca_time.1,
            tca_jd: c.tca_time.0 + c.tca_time.1,
            tca_seconds_since_window_start: c.tca_seconds_since_window_start,
            miss_distance_km: c.miss_distance_km,
            relative_position_km: c.relative_position_km.to_vec(),
            relative_velocity_km_s: c.relative_velocity_km_s.to_vec(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConjunctionObject {
    candidate: CandidateObject,
    pc: f64,
    miss_km: f64,
    relative_speed_km_s: f64,
    sigma_x_km: f64,
    sigma_z_km: f64,
}

impl From<&TcaConjunction> for ConjunctionObject {
    fn from(c: &TcaConjunction) -> Self {
        Self {
            candidate: CandidateObject::from(&c.candidate),
            pc: c.collision_probability.pc,
            miss_km: c.collision_probability.miss_km,
            relative_speed_km_s: c.collision_probability.relative_speed_km_s,
            sigma_x_km: c.collision_probability.sigma_x_km,
            sigma_z_km: c.collision_probability.sigma_z_km,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScreeningHitObject {
    secondary_index: usize,
    candidate: CandidateObject,
}

impl From<&TcaScreeningHit> for ScreeningHitObject {
    fn from(h: &TcaScreeningHit) -> Self {
        Self {
            secondary_index: h.secondary_index,
            candidate: CandidateObject::from(&h.candidate),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScreeningConjunctionObject {
    secondary_index: usize,
    conjunction: ConjunctionObject,
}

impl From<&TcaScreeningConjunctionHit> for ScreeningConjunctionObject {
    fn from(h: &TcaScreeningConjunctionHit) -> Self {
        Self {
            secondary_index: h.secondary_index,
            conjunction: ConjunctionObject::from(&h.conjunction),
        }
    }
}

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(value).map_err(|e| type_error(&e.to_string()))
}

// --- entry points -----------------------------------------------------------

/// Find local TCA candidates between two satellites over a UTC window.
///
/// `primaryLine1` / `primaryLine2` and `secondaryLine1` / `secondaryLine2` are
/// the TLE line pairs; `windowStartUnixMicros` / `windowEndUnixMicros` are the
/// window bounds as unix-microsecond UTC stamps (`bigint`); `options` is an
/// optional `TcaFinderOptions` object. Returns an array of `TcaCandidate`
/// objects. Delegates to
/// `sidereon_core::astro::tca::find_tca_candidates_from_tles`.
#[wasm_bindgen(js_name = findTcaCandidates)]
#[allow(clippy::too_many_arguments)]
pub fn find_tca_candidates(
    primary_line1: &str,
    primary_line2: &str,
    secondary_line1: &str,
    secondary_line2: &str,
    window_start_unix_micros: i64,
    window_end_unix_micros: i64,
    options: JsValue,
) -> Result<JsValue, JsValue> {
    let opts = parse_finder_options(options)?;
    let candidates = find_tca_candidates_from_tles(
        primary_line1,
        primary_line2,
        secondary_line1,
        secondary_line2,
        julian_date_from_unix_us(window_start_unix_micros),
        julian_date_from_unix_us(window_end_unix_micros),
        opts,
    )
    .map_err(engine_error)?;
    let objects: Vec<CandidateObject> = candidates.iter().map(CandidateObject::from).collect();
    to_js(&objects)
}

/// Find local TCA candidates between two satellites and evaluate collision
/// probability at each.
///
/// Like `findTcaCandidates`, plus `pcOptions` (a `TcaPcOptions` object with the
/// hard-body radius, Pc method, and optional per-object covariances). Returns an
/// array of `TcaConjunction` objects. Delegates to
/// `sidereon_core::astro::tca::find_tca_conjunctions_from_tles`.
#[wasm_bindgen(js_name = findTcaConjunctions)]
#[allow(clippy::too_many_arguments)]
pub fn find_tca_conjunctions(
    primary_line1: &str,
    primary_line2: &str,
    secondary_line1: &str,
    secondary_line2: &str,
    window_start_unix_micros: i64,
    window_end_unix_micros: i64,
    pc_options: JsValue,
    options: JsValue,
) -> Result<JsValue, JsValue> {
    let tca_options = parse_finder_options(options)?;
    let pc_options = parse_pc_options(pc_options)?;
    let conjunctions = find_tca_conjunctions_from_tles(
        TcaTle::new(primary_line1, primary_line2),
        TcaTle::new(secondary_line1, secondary_line2),
        julian_date_from_unix_us(window_start_unix_micros),
        julian_date_from_unix_us(window_end_unix_micros),
        tca_options,
        pc_options,
    )
    .map_err(engine_error)?;
    let objects: Vec<ConjunctionObject> =
        conjunctions.iter().map(ConjunctionObject::from).collect();
    to_js(&objects)
}

/// Screen a primary satellite against a secondary TLE catalog for threshold TCAs.
///
/// `primaryLine1` / `primaryLine2` is the primary TLE; `secondaries` is an array
/// of `{ line1, line2 }`; `missDistanceThresholdKm` is the miss-distance cutoff.
/// Returns one `TcaScreeningHit` (`{ secondaryIndex, candidate }`) per local TCA
/// at or below the threshold, in catalog then time order. Delegates to
/// `sidereon_core::astro::tca::screen_tca_candidates_from_tle_catalog_serial`.
#[wasm_bindgen(js_name = screenTcaCandidates)]
pub fn screen_tca_candidates(
    primary_line1: &str,
    primary_line2: &str,
    secondaries: JsValue,
    window_start_unix_micros: i64,
    window_end_unix_micros: i64,
    miss_distance_threshold_km: f64,
    options: JsValue,
) -> Result<JsValue, JsValue> {
    let opts = parse_finder_options(options)?;
    let catalog = parse_tle_catalog(secondaries)?;
    let secondary_tles: Vec<TcaTle<'_>> = catalog
        .iter()
        .map(|t| TcaTle::new(&t.line1, &t.line2))
        .collect();
    let window = TcaWindow::new(
        julian_date_from_unix_us(window_start_unix_micros),
        julian_date_from_unix_us(window_end_unix_micros),
    );
    let hits = screen_tca_candidates_from_tle_catalog_serial(
        TcaTle::new(primary_line1, primary_line2),
        &secondary_tles,
        window,
        miss_distance_threshold_km,
        opts,
    )
    .map_err(engine_error)?;
    let objects: Vec<ScreeningHitObject> = hits.iter().map(ScreeningHitObject::from).collect();
    to_js(&objects)
}

/// Screen a primary against a secondary TLE catalog and evaluate collision
/// probability at each threshold TCA.
///
/// Like `screenTcaCandidates`, plus `pcOptions`. Returns an array of
/// `TcaScreeningConjunctionHit` (`{ secondaryIndex, conjunction }`). Delegates to
/// `sidereon_core::astro::tca::screen_tca_conjunctions_from_tle_catalog_serial`.
#[wasm_bindgen(js_name = screenTcaConjunctions)]
#[allow(clippy::too_many_arguments)]
pub fn screen_tca_conjunctions(
    primary_line1: &str,
    primary_line2: &str,
    secondaries: JsValue,
    window_start_unix_micros: i64,
    window_end_unix_micros: i64,
    miss_distance_threshold_km: f64,
    pc_options: JsValue,
    options: JsValue,
) -> Result<JsValue, JsValue> {
    let tca_options = parse_finder_options(options)?;
    let pc_options = parse_pc_options(pc_options)?;
    let catalog = parse_tle_catalog(secondaries)?;
    let secondary_tles: Vec<TcaTle<'_>> = catalog
        .iter()
        .map(|t| TcaTle::new(&t.line1, &t.line2))
        .collect();
    let window = TcaWindow::new(
        julian_date_from_unix_us(window_start_unix_micros),
        julian_date_from_unix_us(window_end_unix_micros),
    );
    let hits = screen_tca_conjunctions_from_tle_catalog_serial(
        TcaTle::new(primary_line1, primary_line2),
        &secondary_tles,
        window,
        miss_distance_threshold_km,
        tca_options,
        pc_options,
    )
    .map_err(engine_error)?;
    let objects: Vec<ScreeningConjunctionObject> =
        hits.iter().map(ScreeningConjunctionObject::from).collect();
    to_js(&objects)
}
