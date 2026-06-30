//! Bodies binding: analytic Sun and Moon positions over an epoch grid.
//!
//! Each epoch's precise time scales come from the parity-critical
//! `UtcInstant::time_scales` path and the positions are exactly
//! `sun_moon_eci_at` / `sun_moon_ecef`, so the numbers are bit-identical to what
//! `sidereon-core` produces. The per-epoch loop runs inside Rust.

use wasm_bindgen::prelude::*;

use sidereon::passes::UtcInstant;
use sidereon_core::astro::bodies::{sun_moon_ecef, sun_moon_eci_at};

use crate::error::{engine_error, type_error};
use crate::marshal::flat3;

/// A batch of Sun and Moon positions, one per epoch: flat row-major `sun` and
/// `moon` `Float64Array`s of length `3 * epochCount`, in **metres**.
///
/// The frame (geocentric ECI of date vs Earth-fixed ITRS/ECEF) is fixed by which
/// function produced this object: [`sunMoonEci`] or [`sunMoonEcef`].
#[wasm_bindgen]
pub struct SunMoon {
    sun: Vec<f64>,
    moon: Vec<f64>,
    frame: &'static str,
}

#[wasm_bindgen]
impl SunMoon {
    /// Sun positions, metres, flat row-major `(n, 3)`.
    #[wasm_bindgen(getter)]
    pub fn sun(&self) -> Vec<f64> {
        self.sun.clone()
    }

    /// Moon positions, metres, flat row-major `(n, 3)`.
    #[wasm_bindgen(getter)]
    pub fn moon(&self) -> Vec<f64> {
        self.moon.clone()
    }

    /// The frame these positions are in: `"eci"` or `"ecef"`.
    #[wasm_bindgen(getter)]
    pub fn frame(&self) -> String {
        self.frame.to_string()
    }

    /// Number of epochs in the batch.
    #[wasm_bindgen(getter, js_name = epochCount)]
    pub fn epoch_count(&self) -> usize {
        self.sun.len() / 3
    }
}

fn collect<F>(epochs_unix_us: &[i64], frame: &'static str, f: F) -> Result<SunMoon, JsValue>
where
    F: Fn(
        &sidereon_core::astro::time::TimeScales,
    ) -> Result<
        sidereon_core::astro::bodies::SunMoon,
        sidereon_core::astro::bodies::SunMoonError,
    >,
{
    if epochs_unix_us.is_empty() {
        return Err(type_error("epochsUnixUs must not be empty"));
    }
    let mut sun = Vec::with_capacity(epochs_unix_us.len());
    let mut moon = Vec::with_capacity(epochs_unix_us.len());
    for &us in epochs_unix_us {
        let ts = UtcInstant::from_unix_microseconds(us).time_scales();
        let sm = f(&ts).map_err(engine_error)?;
        sun.push(sm.sun);
        moon.push(sm.moon);
    }
    Ok(SunMoon {
        sun: flat3(&sun),
        moon: flat3(&moon),
        frame,
    })
}

/// Analytic Sun and Moon positions in the geocentric ECI frame (mean equator and
/// equinox of date), metres, for a `BigInt64Array` of unix-microsecond epochs.
#[wasm_bindgen(js_name = sunMoonEci)]
pub fn sun_moon_eci(epochs_unix_us: &[i64]) -> Result<SunMoon, JsValue> {
    collect(epochs_unix_us, "eci", sun_moon_eci_at)
}

/// Analytic Sun and Moon geocentric positions in the Earth-fixed ITRS (ECEF)
/// frame, metres, for a `BigInt64Array` of unix-microsecond epochs.
#[wasm_bindgen(js_name = sunMoonEcef)]
pub fn sun_moon_ecef_batch(epochs_unix_us: &[i64]) -> Result<SunMoon, JsValue> {
    collect(epochs_unix_us, "ecef", sun_moon_ecef)
}
