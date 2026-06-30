//! RF link-budget binding. Every function delegates to
//! `sidereon_core::astro::rf` after finite/positive validation; no formula lives
//! here.

use wasm_bindgen::prelude::*;

use sidereon_core::astro::rf as core_rf;

use crate::error::{engine_error, range_error};

fn ensure_finite(name: &str, value: f64) -> Result<f64, JsValue> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(range_error(&format!("{name} must be finite")))
    }
}

fn ensure_positive(name: &str, value: f64) -> Result<f64, JsValue> {
    let v = ensure_finite(name, value)?;
    if v > 0.0 {
        Ok(v)
    } else {
        Err(range_error(&format!("{name} must be positive")))
    }
}

/// Link-budget inputs for [`LinkBudget.margin`].
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct LinkBudget {
    eirp_dbw: f64,
    fspl_db: f64,
    receiver_gt_dbk: f64,
    other_losses_db: f64,
    required_cn0_dbhz: f64,
}

#[wasm_bindgen]
impl LinkBudget {
    /// Build a link-budget input object. `otherLossesDb` defaults to 0. Throws a
    /// `RangeError` on a non-finite field.
    #[wasm_bindgen(constructor)]
    pub fn new(
        eirp_dbw: f64,
        fspl_db: f64,
        receiver_gt_dbk: f64,
        required_cn0_dbhz: f64,
        other_losses_db: Option<f64>,
    ) -> Result<LinkBudget, JsValue> {
        Ok(LinkBudget {
            eirp_dbw: ensure_finite("eirpDbw", eirp_dbw)?,
            fspl_db: ensure_finite("fsplDb", fspl_db)?,
            receiver_gt_dbk: ensure_finite("receiverGtDbk", receiver_gt_dbk)?,
            required_cn0_dbhz: ensure_finite("requiredCn0Dbhz", required_cn0_dbhz)?,
            other_losses_db: ensure_finite("otherLossesDb", other_losses_db.unwrap_or(0.0))?,
        })
    }

    /// Transmitter EIRP, dBW.
    #[wasm_bindgen(getter, js_name = eirpDbw)]
    pub fn eirp_dbw(&self) -> f64 {
        self.eirp_dbw
    }

    /// Free-space path loss, dB.
    #[wasm_bindgen(getter, js_name = fsplDb)]
    pub fn fspl_db(&self) -> f64 {
        self.fspl_db
    }

    /// Receiver figure of merit G/T, dB/K.
    #[wasm_bindgen(getter, js_name = receiverGtDbk)]
    pub fn receiver_gt_dbk(&self) -> f64 {
        self.receiver_gt_dbk
    }

    /// Sum of miscellaneous losses, dB.
    #[wasm_bindgen(getter, js_name = otherLossesDb)]
    pub fn other_losses_db(&self) -> f64 {
        self.other_losses_db
    }

    /// Required C/N0 threshold, dB-Hz.
    #[wasm_bindgen(getter, js_name = requiredCn0Dbhz)]
    pub fn required_cn0_dbhz(&self) -> f64 {
        self.required_cn0_dbhz
    }

    /// Link margin, dB, for this budget.
    #[wasm_bindgen(getter)]
    pub fn margin(&self) -> Result<f64, JsValue> {
        core_rf::link_margin(&core_rf::LinkBudget {
            eirp_dbw: self.eirp_dbw,
            fspl_db: self.fspl_db,
            receiver_gt_dbk: self.receiver_gt_dbk,
            other_losses_db: self.other_losses_db,
            required_cn0_dbhz: self.required_cn0_dbhz,
        })
        .map_err(engine_error)
    }
}

/// Free-space path loss, dB, for range in km and frequency in MHz.
#[wasm_bindgen]
pub fn fspl(distance_km: f64, frequency_mhz: f64) -> Result<f64, JsValue> {
    core_rf::fspl(
        ensure_positive("distanceKm", distance_km)?,
        ensure_positive("frequencyMhz", frequency_mhz)?,
    )
    .map_err(engine_error)
}

/// Effective isotropic radiated power, dBW.
#[wasm_bindgen]
pub fn eirp(tx_power_dbm: f64, tx_antenna_gain_dbi: f64) -> Result<f64, JsValue> {
    core_rf::eirp(
        ensure_finite("txPowerDbm", tx_power_dbm)?,
        ensure_finite("txAntennaGainDbi", tx_antenna_gain_dbi)?,
    )
    .map_err(engine_error)
}

/// Carrier-to-noise-density ratio, dB-Hz. `otherLossesDb` defaults to 0.
#[wasm_bindgen]
pub fn cn0(
    eirp_dbw: f64,
    fspl_db: f64,
    receiver_gt_dbk: f64,
    other_losses_db: Option<f64>,
) -> Result<f64, JsValue> {
    core_rf::cn0(
        ensure_finite("eirpDbw", eirp_dbw)?,
        ensure_finite("fsplDb", fspl_db)?,
        ensure_finite("receiverGtDbk", receiver_gt_dbk)?,
        ensure_finite("otherLossesDb", other_losses_db.unwrap_or(0.0))?,
    )
    .map_err(engine_error)
}

/// Wavelength, metres, for frequency in Hz.
#[wasm_bindgen]
pub fn wavelength(frequency_hz: f64) -> Result<f64, JsValue> {
    core_rf::wavelength(ensure_positive("frequencyHz", frequency_hz)?).map_err(engine_error)
}

/// Parabolic-dish antenna gain, dBi.
#[wasm_bindgen(js_name = dishGain)]
pub fn dish_gain(diameter_m: f64, frequency_hz: f64, efficiency: f64) -> Result<f64, JsValue> {
    core_rf::dish_gain(
        ensure_positive("diameterM", diameter_m)?,
        ensure_positive("frequencyHz", frequency_hz)?,
        ensure_positive("efficiency", efficiency)?,
    )
    .map_err(engine_error)
}
