//! Station tidal displacement models (IERS): solid-earth, ocean loading, and
//! solid-earth pole tide.
//!
//! Thin wrapper over `sidereon_core::tides`. The Love/Shida expansions, the BLQ
//! constituent sum, and the pole-tide geometry all live in the crate; this layer
//! only reshapes the ECEF vectors and BLQ coefficient grids and re-encodes the
//! displacement. Every result is a geocentric ITRF displacement in metres.

use wasm_bindgen::prelude::*;

use sidereon_core::tides::{
    ocean_tide_loading, solid_earth_pole_tide, solid_earth_tide, OceanLoadingBlq,
    NUM_OCEAN_CONSTITUENTS,
};

use crate::error::{engine_error, type_error};
use crate::marshal::vec3_finite;

/// Number of BLQ components (radial, EW, NS) times the constituent count: the
/// length of each flat row-major ocean-loading grid this binding accepts.
const OCEAN_GRID_LEN: usize = 3 * NUM_OCEAN_CONSTITUENTS;

/// Reshape a flat row-major `(3, NUM_OCEAN_CONSTITUENTS)` buffer into the BLQ
/// component-by-constituent grid, rejecting a wrong length (`TypeError`).
fn ocean_grid(name: &str, values: &[f64]) -> Result<[[f64; NUM_OCEAN_CONSTITUENTS]; 3], JsValue> {
    if values.len() != OCEAN_GRID_LEN {
        return Err(type_error(&format!(
            "{name} must have length {OCEAN_GRID_LEN} (flat row-major 3-by-{NUM_OCEAN_CONSTITUENTS}), got {}",
            values.len()
        )));
    }
    let mut grid = [[0.0_f64; NUM_OCEAN_CONSTITUENTS]; 3];
    for (component, row) in grid.iter_mut().enumerate() {
        for (constituent, cell) in row.iter_mut().enumerate() {
            *cell = values[component * NUM_OCEAN_CONSTITUENTS + constituent];
        }
    }
    Ok(grid)
}

/// Solid-earth tide displacement of an ITRF station, metres (ECEF).
///
/// `stationEcefM`, `sunEcefM`, `moonEcefM` are length-3 geocentric ECEF metre
/// vectors. The epoch is the UTC `year`/`month`/`day` plus `fractionalHour`
/// (`hour + min/60 + sec/3600`, in `[0, 24)`). Returns the displacement
/// `[dx, dy, dz]`. Delegates to `sidereon_core::tides::solid_earth_tide`.
#[wasm_bindgen(js_name = solidEarthTide)]
pub fn solid_earth_tide_js(
    station_ecef_m: &[f64],
    year: i32,
    month: i32,
    day: i32,
    fractional_hour: f64,
    sun_ecef_m: &[f64],
    moon_ecef_m: &[f64],
) -> Result<Vec<f64>, JsValue> {
    let xsta = vec3_finite("stationEcefM", station_ecef_m)?;
    let xsun = vec3_finite("sunEcefM", sun_ecef_m)?;
    let xmon = vec3_finite("moonEcefM", moon_ecef_m)?;
    let d = solid_earth_tide(&xsta, year, month, day, fractional_hour, &xsun, &xmon)
        .map_err(engine_error)?;
    Ok(d.to_vec())
}

/// Ocean tide loading displacement of an ITRF station, metres (ECEF).
///
/// `stationEcefM` is a length-3 geocentric ECEF metre vector and the epoch is
/// the UTC `year`/`month`/`day` plus `fractionalHour`. `amplitudeM` and
/// `phaseDeg` are the station's BLQ coefficients as flat row-major
/// `(3, 11)` `Float64Array`s: component order radial / EW-west / NS-south, and
/// constituent order M2 S2 N2 K2 K1 O1 P1 Q1 Mf Mm Ssa. Returns the displacement
/// `[dx, dy, dz]`. Delegates to `sidereon_core::tides::ocean_tide_loading`.
#[wasm_bindgen(js_name = oceanTideLoading)]
pub fn ocean_tide_loading_js(
    station_ecef_m: &[f64],
    year: i32,
    month: i32,
    day: i32,
    fractional_hour: f64,
    amplitude_m: &[f64],
    phase_deg: &[f64],
) -> Result<Vec<f64>, JsValue> {
    let xsta = vec3_finite("stationEcefM", station_ecef_m)?;
    let blq = OceanLoadingBlq {
        amplitude_m: ocean_grid("amplitudeM", amplitude_m)?,
        phase_deg: ocean_grid("phaseDeg", phase_deg)?,
    };
    let d =
        ocean_tide_loading(&xsta, year, month, day, fractional_hour, &blq).map_err(engine_error)?;
    Ok(d.to_vec())
}

/// Solid-earth pole tide displacement of an ITRF station, metres (ECEF).
///
/// `stationEcefM` is a length-3 geocentric ECEF metre vector and the epoch is
/// the UTC `year`/`month`/`day` plus `fractionalHour`. `xpArcsec` / `ypArcsec`
/// are the polar-motion coordinates in arcseconds. Returns the displacement
/// `[dx, dy, dz]`. Delegates to `sidereon_core::tides::solid_earth_pole_tide`.
#[wasm_bindgen(js_name = solidEarthPoleTide)]
pub fn solid_earth_pole_tide_js(
    station_ecef_m: &[f64],
    year: i32,
    month: i32,
    day: i32,
    fractional_hour: f64,
    xp_arcsec: f64,
    yp_arcsec: f64,
) -> Result<Vec<f64>, JsValue> {
    let xsta = vec3_finite("stationEcefM", station_ecef_m)?;
    let d = solid_earth_pole_tide(
        &xsta,
        year,
        month,
        day,
        fractional_hour,
        xp_arcsec,
        yp_arcsec,
    )
    .map_err(engine_error)?;
    Ok(d.to_vec())
}
