//! Shared GNSS identifiers used by the RINEX and observable bindings.
//!
//! `GnssSystem` and `CarrierBand` mirror the `sidereon-core` enums one-to-one
//! and cross to JS as wasm-bindgen enums (their JS value is the variant index).
//! Their `letter` / `label` / `name` accessors are exposed as free functions
//! because wasm-bindgen does not support methods on exported enums.

use wasm_bindgen::prelude::*;

use sidereon_core::frequencies::CarrierBand as CoreCarrierBand;
use sidereon_core::GnssSystem as CoreGnssSystem;

/// A GNSS constellation. The JS value matches the variant order below.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GnssSystem {
    /// GPS, RINEX letter `G`.
    Gps,
    /// GLONASS, RINEX letter `R`.
    Glonass,
    /// Galileo, RINEX letter `E`.
    Galileo,
    /// BeiDou, RINEX letter `C`.
    BeiDou,
    /// QZSS, RINEX letter `J`.
    Qzss,
    /// NavIC, RINEX letter `I`.
    Navic,
    /// SBAS, RINEX letter `S`.
    Sbas,
}

impl From<CoreGnssSystem> for GnssSystem {
    fn from(system: CoreGnssSystem) -> Self {
        match system {
            CoreGnssSystem::Gps => Self::Gps,
            CoreGnssSystem::Glonass => Self::Glonass,
            CoreGnssSystem::Galileo => Self::Galileo,
            CoreGnssSystem::BeiDou => Self::BeiDou,
            CoreGnssSystem::Qzss => Self::Qzss,
            CoreGnssSystem::Navic => Self::Navic,
            CoreGnssSystem::Sbas => Self::Sbas,
        }
    }
}

impl From<GnssSystem> for CoreGnssSystem {
    fn from(system: GnssSystem) -> Self {
        match system {
            GnssSystem::Gps => CoreGnssSystem::Gps,
            GnssSystem::Glonass => CoreGnssSystem::Glonass,
            GnssSystem::Galileo => CoreGnssSystem::Galileo,
            GnssSystem::BeiDou => CoreGnssSystem::BeiDou,
            GnssSystem::Qzss => CoreGnssSystem::Qzss,
            GnssSystem::Navic => CoreGnssSystem::Navic,
            GnssSystem::Sbas => CoreGnssSystem::Sbas,
        }
    }
}

/// Canonical RINEX one-letter identifier for a constellation, e.g. `"G"`.
#[wasm_bindgen(js_name = gnssSystemLetter)]
pub fn gnss_system_letter(system: GnssSystem) -> String {
    CoreGnssSystem::from(system).letter().to_string()
}

/// Stable lower-case display label for a constellation, e.g. `"gps"`.
#[wasm_bindgen(js_name = gnssSystemLabel)]
pub fn gnss_system_label(system: GnssSystem) -> String {
    match system {
        GnssSystem::Gps => "gps",
        GnssSystem::Glonass => "glonass",
        GnssSystem::Galileo => "galileo",
        GnssSystem::BeiDou => "beidou",
        GnssSystem::Qzss => "qzss",
        GnssSystem::Navic => "navic",
        GnssSystem::Sbas => "sbas",
    }
    .to_string()
}

/// A canonical GNSS carrier band. The JS value matches the variant order below.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CarrierBand {
    /// GPS/QZSS L1.
    L1,
    /// GPS/QZSS L2.
    L2,
    /// GPS/QZSS L5.
    L5,
    /// Galileo E1.
    E1,
    /// Galileo E5a.
    E5a,
    /// Galileo E5b.
    E5b,
    /// Galileo E5 AltBOC.
    E5,
    /// Galileo E6.
    E6,
    /// BeiDou B1C.
    B1c,
    /// BeiDou B1I.
    B1i,
    /// BeiDou B2a.
    B2a,
    /// BeiDou B2b.
    B2b,
    /// BeiDou B2.
    B2,
    /// BeiDou B3I.
    B3i,
    /// GLONASS G1 FDMA.
    G1,
    /// GLONASS G2 FDMA.
    G2,
}

impl From<CarrierBand> for CoreCarrierBand {
    fn from(band: CarrierBand) -> Self {
        match band {
            CarrierBand::L1 => CoreCarrierBand::L1,
            CarrierBand::L2 => CoreCarrierBand::L2,
            CarrierBand::L5 => CoreCarrierBand::L5,
            CarrierBand::E1 => CoreCarrierBand::E1,
            CarrierBand::E5a => CoreCarrierBand::E5a,
            CarrierBand::E5b => CoreCarrierBand::E5b,
            CarrierBand::E5 => CoreCarrierBand::E5,
            CarrierBand::E6 => CoreCarrierBand::E6,
            CarrierBand::B1c => CoreCarrierBand::B1c,
            CarrierBand::B1i => CoreCarrierBand::B1i,
            CarrierBand::B2a => CoreCarrierBand::B2a,
            CarrierBand::B2b => CoreCarrierBand::B2b,
            CarrierBand::B2 => CoreCarrierBand::B2,
            CarrierBand::B3i => CoreCarrierBand::B3i,
            CarrierBand::G1 => CoreCarrierBand::G1,
            CarrierBand::G2 => CoreCarrierBand::G2,
        }
    }
}

impl From<CoreCarrierBand> for CarrierBand {
    fn from(band: CoreCarrierBand) -> Self {
        match band {
            CoreCarrierBand::L1 => CarrierBand::L1,
            CoreCarrierBand::L2 => CarrierBand::L2,
            CoreCarrierBand::L5 => CarrierBand::L5,
            CoreCarrierBand::E1 => CarrierBand::E1,
            CoreCarrierBand::E5a => CarrierBand::E5a,
            CoreCarrierBand::E5b => CarrierBand::E5b,
            CoreCarrierBand::E5 => CarrierBand::E5,
            CoreCarrierBand::E6 => CarrierBand::E6,
            CoreCarrierBand::B1c => CarrierBand::B1c,
            CoreCarrierBand::B1i => CarrierBand::B1i,
            CoreCarrierBand::B2a => CarrierBand::B2a,
            CoreCarrierBand::B2b => CarrierBand::B2b,
            CoreCarrierBand::B2 => CarrierBand::B2,
            CoreCarrierBand::B3i => CarrierBand::B3i,
            CoreCarrierBand::G1 => CarrierBand::G1,
            CoreCarrierBand::G2 => CarrierBand::G2,
        }
    }
}

/// Canonical lower-case carrier-band token, e.g. `"l1"`.
#[wasm_bindgen(js_name = carrierBandName)]
pub fn carrier_band_name(band: CarrierBand) -> String {
    CoreCarrierBand::from(band).name().to_string()
}
