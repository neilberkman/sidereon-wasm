//! Neutral-atmosphere density (NRLMSISE-00).
//!
//! Thin wrapper over `sidereon_core::astro::atmosphere`. The model evaluation,
//! local-solar-time helper, and default variation switches all live in the
//! crate; this layer only assembles the input struct and unpacks the result.
//! `nrlmsise00` runs the model under the core default flags (metric output), so
//! no switch or flag literals live here. Mirrors the Elixir
//! `Sidereon.Atmosphere` surface.

use wasm_bindgen::prelude::*;

use sidereon_core::astro::atmosphere::{
    nrlmsise00_with_lst, NrlmsiseInput, DEFAULT_AP, DEFAULT_F107, DEFAULT_F107A,
};

use crate::error::engine_error;

/// Canonical quiet-Sun space-weather indices for NRLMSISE-00, mirrored from the
/// core so a JS caller can pass moderate-activity defaults to [`atmosphereDensity`]
/// without re-deriving the magic numbers.
#[wasm_bindgen]
pub struct SpaceWeatherDefaults;

#[wasm_bindgen]
impl SpaceWeatherDefaults {
    /// Daily F10.7 solar radio flux.
    #[wasm_bindgen(getter)]
    pub fn f107(&self) -> f64 {
        DEFAULT_F107
    }

    /// 81-day centred F10.7 average.
    #[wasm_bindgen(getter)]
    pub fn f107a(&self) -> f64 {
        DEFAULT_F107A
    }

    /// Daily magnetic Ap index.
    #[wasm_bindgen(getter)]
    pub fn ap(&self) -> f64 {
        DEFAULT_AP
    }
}

/// The canonical quiet-Sun space-weather defaults for NRLMSISE-00. Delegates to
/// `sidereon_core::astro::atmosphere::{DEFAULT_F107, DEFAULT_F107A, DEFAULT_AP}`.
#[wasm_bindgen(js_name = atmosphereSpaceWeatherDefaults)]
pub fn atmosphere_space_weather_defaults() -> SpaceWeatherDefaults {
    SpaceWeatherDefaults
}

/// Total mass density and temperature at altitude from NRLMSISE-00.
#[wasm_bindgen]
pub struct AtmosphereDensity {
    density_kg_m3: f64,
    temperature_k: f64,
}

#[wasm_bindgen]
impl AtmosphereDensity {
    /// Total mass density, kilograms per cubic metre.
    #[wasm_bindgen(getter, js_name = densityKgM3)]
    pub fn density_kg_m3(&self) -> f64 {
        self.density_kg_m3
    }

    /// Temperature at the requested altitude, kelvin.
    #[wasm_bindgen(getter, js_name = temperatureK)]
    pub fn temperature_k(&self) -> f64 {
        self.temperature_k
    }
}

/// Evaluate NRLMSISE-00 neutral-atmosphere density and temperature.
///
/// `latDeg` / `lonDeg` are geodetic degrees, `altKm` geodetic altitude in
/// kilometres, `year` / `doy` the UT year and day-of-year (1-366), `sec` the
/// seconds-in-day, and `f107` / `f107a` / `ap` the space-weather indices (daily
/// F10.7, 81-day centred F10.7 average, and daily magnetic Ap). The local solar
/// time is derived in the core (`sec/3600 + lonDeg/15`). Delegates to
/// `sidereon_core::astro::atmosphere::nrlmsise00_with_lst` with `lst = None`, so
/// the core derives the solar time under its default flags.
#[wasm_bindgen(js_name = atmosphereDensity)]
#[allow(clippy::too_many_arguments)]
pub fn atmosphere_density(
    lat_deg: f64,
    lon_deg: f64,
    alt_km: f64,
    year: i32,
    doy: i32,
    sec: f64,
    f107: f64,
    f107a: f64,
    ap: f64,
) -> Result<AtmosphereDensity, JsValue> {
    let input = NrlmsiseInput {
        year,
        doy,
        sec,
        alt: alt_km,
        g_lat: lat_deg,
        g_long: lon_deg,
        // Derived in the core: passing `None` below makes `nrlmsise00_with_lst`
        // fill `lst` from `local_solar_time(sec, g_long)`, so this seed is ignored.
        lst: 0.0,
        f107a,
        f107,
        ap,
        ap_array: None,
    };
    let output = nrlmsise00_with_lst(&input, None).map_err(engine_error)?;
    Ok(AtmosphereDensity {
        density_kg_m3: output.density(),
        temperature_k: output.temperature_alt(),
    })
}

#[cfg(test)]
mod drift_tests {
    //! The exposed quiet-Sun space-weather defaults track the core
    //! `DEFAULT_F107` / `DEFAULT_F107A` / `DEFAULT_AP` constants, pinned to their
    //! documented moderate-activity values.
    use super::*;

    #[test]
    fn space_weather_defaults_track_core() {
        let d = SpaceWeatherDefaults;
        assert_eq!(d.f107(), DEFAULT_F107);
        assert_eq!(d.f107a(), DEFAULT_F107A);
        assert_eq!(d.ap(), DEFAULT_AP);
    }

    #[test]
    fn core_space_weather_constants_pinned() {
        assert_eq!(DEFAULT_F107, 150.0);
        assert_eq!(DEFAULT_F107A, 150.0);
        assert_eq!(DEFAULT_AP, 4.0);
    }
}
