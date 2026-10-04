//! One-epoch satellite/station coverage grid.
//!
//! Thin wrapper over `sidereon_core::astro::coverage`. The look-angle grid and
//! its visibility / access-count / max-elevation reductions all live in the
//! crate; this layer only collects the wrapped TLE satellites and ground
//! stations, calls the scalar kernel grid builder, and re-encodes the cells and
//! reductions. The grid is row-major `[satellite][station]`.

use wasm_bindgen::prelude::*;

use sidereon_core::astro::coverage::{
    access_counts, look_angles_batch_with_validity, max_elevation, visible_mask, LookAngleGrid,
};
use sidereon_core::astro::passes::UtcInstant;
use sidereon_core::astro::sgp4::Satellite;
use sidereon_core::astro::time::DegradeReason;

use crate::error::{type_error, ut1_validity};
use crate::sgp4::{GroundStation, Tle};

/// A computed look-angle grid for a set of satellites and ground stations at one
/// epoch. Build with [`coverageLookAngles`].
#[wasm_bindgen]
pub struct CoverageGrid {
    grid: LookAngleGrid,
    station_count: usize,
    ut1_degraded: Option<DegradeReason>,
}

#[wasm_bindgen]
impl CoverageGrid {
    /// Number of satellites (grid rows).
    #[wasm_bindgen(getter, js_name = satelliteCount)]
    pub fn satellite_count(&self) -> usize {
        self.grid.len()
    }

    /// Number of ground stations (grid columns).
    #[wasm_bindgen(getter, js_name = stationCount)]
    pub fn station_count(&self) -> usize {
        self.station_count
    }

    /// The UT1 departure the grid accepted under the `"permissive"` policy:
    /// `"beforeCoverage"` or `"afterCoverage"`, or `undefined` when the epoch
    /// lay inside the UT1 table.
    #[wasm_bindgen(getter, js_name = ut1Degraded, unchecked_return_type = "Ut1DegradeReason | undefined")]
    pub fn ut1_degraded(&self) -> Option<String> {
        self.ut1_degraded
            .map(|reason| crate::spp::degrade_reason_label(reason).to_owned())
    }

    /// The look angle for one satellite/station pair as `[azimuthDeg,
    /// elevationDeg, rangeKm]`, or `undefined` when that cell failed (the
    /// satellite was below the horizon geometry the kernel rejects) or the
    /// indices are out of range.
    #[wasm_bindgen(js_name = lookAngle)]
    pub fn look_angle(&self, satellite_index: usize, station_index: usize) -> Option<Vec<f64>> {
        match self.grid.get(satellite_index)?.get(station_index)? {
            Ok(look) => Some(vec![look.azimuth_deg, look.elevation_deg, look.range_km]),
            Err(_) => None,
        }
    }

    /// Complete typed error for one failed satellite/station cell, or
    /// `undefined` for a successful cell or an out-of-range index. The value
    /// uses the same `Error.detail` mapper as the scalar look-angle API.
    #[wasm_bindgen(js_name = cellError, unchecked_return_type = "(Error & { detail: LookAngleErrorDetail }) | undefined")]
    pub fn cell_error(&self, satellite_index: usize, station_index: usize) -> Option<JsValue> {
        let Some(Err(error)) = self
            .grid
            .get(satellite_index)
            .and_then(|row| row.get(station_index))
        else {
            return None;
        };
        Some(crate::sgp4_error::look_angle_error(error.clone()))
    }

    /// Row-major `[satellite][station]` visibility mask at `minElevationDeg`, as a
    /// flat `Uint8Array` of `1` (visible) / `0`. Error cells are not visible.
    /// Delegates to `sidereon_core::astro::coverage::visible_mask`.
    #[wasm_bindgen(js_name = visibleMask)]
    pub fn visible_mask(&self, min_elevation_deg: f64) -> Vec<u8> {
        visible_mask(&self.grid, min_elevation_deg)
            .into_iter()
            .flat_map(|row| row.into_iter().map(u8::from))
            .collect()
    }

    /// Number of visible satellites per station at `minElevationDeg`, as a flat
    /// array of length `stationCount`. Delegates to
    /// `sidereon_core::astro::coverage::access_counts`.
    #[wasm_bindgen(js_name = accessCounts)]
    pub fn access_counts(&self, min_elevation_deg: f64) -> Vec<usize> {
        access_counts(&self.grid, min_elevation_deg)
    }

    /// Maximum successful elevation per station, degrees, as a flat array of
    /// length `stationCount`. A station with no successful cell reports `NaN`.
    /// Delegates to `sidereon_core::astro::coverage::max_elevation`.
    #[wasm_bindgen(js_name = maxElevationDeg)]
    pub fn max_elevation_deg(&self) -> Vec<f64> {
        max_elevation(&self.grid)
            .into_iter()
            .map(|cell| cell.unwrap_or(f64::NAN))
            .collect()
    }
}

/// Build a look-angle coverage grid for `satellites` and `stations` at the
/// `epochUnixUs` (Unix microseconds) epoch. Each cell is the per-pair look angle
/// from `sidereon_core::astro::coverage::look_angles_batch_with_validity`.
/// `ut1Validity` is `"strict"` (the default), under which an epoch outside the
/// UT1 table fails every cell, or `"permissive"`, which computes the cells and
/// reports the departure in `ut1Degraded`.
#[wasm_bindgen(js_name = coverageLookAngles)]
pub fn coverage_look_angles(
    satellites: Vec<Tle>,
    stations: Vec<GroundStation>,
    epoch_unix_us: i64,
    ut1: Option<String>,
) -> Result<CoverageGrid, JsValue> {
    let mode = ut1_validity(ut1)?;
    if satellites.is_empty() {
        return Err(type_error("satellites must not be empty"));
    }
    if stations.is_empty() {
        return Err(type_error("stations must not be empty"));
    }
    let sats: Vec<Satellite> = satellites
        .iter()
        .map(|tle| tle.core_satellite().clone())
        .collect();
    let core_stations: Vec<_> = stations.iter().map(GroundStation::core).collect();
    let datetime = UtcInstant::from_unix_microseconds(epoch_unix_us);
    let validated = look_angles_batch_with_validity(&sats, &core_stations, datetime, mode);
    Ok(CoverageGrid {
        grid: validated.value,
        station_count: core_stations.len(),
        ut1_degraded: validated.degraded,
    })
}
