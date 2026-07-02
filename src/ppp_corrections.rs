//! Static-arc PPP correction precompute binding.
//!
//! Thin marshaling over [`sidereon_core::ppp_corrections`]: for a precise-orbit
//! arc and a fixed receiver, precompute the per-epoch solid-earth tide, solid-earth
//! pole tide, and ocean tide loading displacements, the per-satellite carrier-phase
//! wind-up, and the satellite antenna PCO/PCV projection. No tide, wind-up, or
//! antenna algebra lives here; the numbers are exactly what `sidereon-core`
//! produces. The pole tide and ocean loading need caller-supplied data the engine
//! does not embed (IERS polar motion; the per-station Bos-Scherneck / HARDISP BLQ
//! block), supplied through the options object exactly like the Python/Elixir
//! interfaces.

use std::str::FromStr;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::bias::ClockReferenceObservables;
use sidereon_core::ppp_corrections::{
    build, CivilDateTime, CodeBiasOptions, OceanLoadingBlq, PoleTideOptions, PppCorrectionEpoch,
    PppCorrectionObservation, PppCorrectionsError, PppCorrectionsOptions, SatelliteAntenna,
    SatelliteAntennaFrequency, SatelliteAntennaOptions, NUM_OCEAN_CONSTITUENTS,
};
use sidereon_core::{GnssSatelliteId, GnssSystem};

use crate::bias::BiasSet;
use crate::error::{engine_error, range_error, type_error};
use crate::marshal::vec3_finite;
use crate::sp3::Sp3;

/// Map a core correction-build failure to the JS error kind the binding promises:
/// a caller-supplied non-finite/out-of-domain number is a `RangeError`; a genuine
/// coverage or tide-evaluation failure is an `Error`.
fn ppp_err(err: PppCorrectionsError) -> JsValue {
    match err {
        PppCorrectionsError::InvalidInput { .. }
        | PppCorrectionsError::WindupFrequency { .. }
        | PppCorrectionsError::SatelliteAntennaFrequency { .. }
        | PppCorrectionsError::CodeBiasObservable { .. } => range_error(&err.to_string()),
        PppCorrectionsError::Epoch { .. }
        | PppCorrectionsError::Tide { .. }
        | PppCorrectionsError::PoleTide { .. }
        | PppCorrectionsError::OceanLoading { .. }
        | PppCorrectionsError::Bias { .. } => engine_error(err),
    }
}

// --- input objects ----------------------------------------------------------

/// Civil UTC date/time `{ year, month, day, hour, minute, second }`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
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

/// One satellite observation row (carrier frequencies) for the precompute.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ObservationInput {
    satellite_id: String,
    freq1_hz: f64,
    freq2_hz: f64,
    #[serde(default)]
    glonass_channel: Option<i8>,
}

impl ObservationInput {
    fn to_core(&self) -> Result<PppCorrectionObservation, JsValue> {
        Ok(PppCorrectionObservation {
            sat: parse_sat(&self.satellite_id)?,
            freq1_hz: self.freq1_hz,
            freq2_hz: self.freq2_hz,
            glonass_channel: self.glonass_channel,
        })
    }
}

/// One receiver epoch: civil date/time fields, the receive time as continuous
/// seconds since J2000, and the visible-satellite rows.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EpochInput {
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: f64,
    t_rx_j2000_s: f64,
    observations: Vec<ObservationInput>,
}

impl EpochInput {
    fn to_core(&self) -> Result<PppCorrectionEpoch, JsValue> {
        Ok(PppCorrectionEpoch {
            epoch: CivilDateTime {
                year: self.year,
                month: self.month,
                day: self.day,
                hour: self.hour,
                minute: self.minute,
                second: self.second,
            },
            t_rx_j2000_s: self.t_rx_j2000_s,
            observations: self
                .observations
                .iter()
                .map(ObservationInput::to_core)
                .collect::<Result<_, _>>()?,
        })
    }
}

/// Frequency-dependent satellite antenna calibration.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SatelliteAntennaFrequencyInput {
    label: String,
    pco_m: [f64; 3],
    /// `[nadirDeg, pcvM]` pairs.
    noazi_pcv_m: Vec<(f64, f64)>,
}

impl From<&SatelliteAntennaFrequencyInput> for SatelliteAntennaFrequency {
    fn from(f: &SatelliteAntennaFrequencyInput) -> Self {
        SatelliteAntennaFrequency {
            label: f.label.clone(),
            pco_m: f.pco_m,
            noazi_pcv_m: f.noazi_pcv_m.clone(),
        }
    }
}

/// One satellite's antenna block, selected by PRN and an optional validity window.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SatelliteAntennaInput {
    sat: String,
    #[serde(default)]
    valid_from: Option<CivilInput>,
    #[serde(default)]
    valid_until: Option<CivilInput>,
    frequencies: Vec<SatelliteAntennaFrequencyInput>,
}

impl SatelliteAntennaInput {
    fn to_core(&self) -> Result<SatelliteAntenna, JsValue> {
        Ok(SatelliteAntenna {
            sat: parse_sat(&self.sat)?,
            valid_from: self.valid_from.as_ref().map(CivilInput::to_core),
            valid_until: self.valid_until.as_ref().map(CivilInput::to_core),
            frequencies: self.frequencies.iter().map(Into::into).collect(),
        })
    }
}

/// Satellite-antenna correction options.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SatelliteAntennaOptionsInput {
    freq1_label: String,
    freq1_hz: f64,
    freq2_label: String,
    freq2_hz: f64,
    antennas: Vec<SatelliteAntennaInput>,
}

impl SatelliteAntennaOptionsInput {
    fn to_core(&self) -> Result<SatelliteAntennaOptions, JsValue> {
        Ok(SatelliteAntennaOptions {
            freq1_label: self.freq1_label.clone(),
            freq1_hz: self.freq1_hz,
            freq2_label: self.freq2_label.clone(),
            freq2_hz: self.freq2_hz,
            antennas: self
                .antennas
                .iter()
                .map(SatelliteAntennaInput::to_core)
                .collect::<Result<_, _>>()?,
        })
    }
}

/// Solid-earth pole-tide options: the IERS polar motion of the date (arcsec).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PoleTideInput {
    xp_arcsec: f64,
    yp_arcsec: f64,
}

impl PoleTideInput {
    fn to_core(&self) -> Result<PoleTideOptions, JsValue> {
        if !self.xp_arcsec.is_finite() {
            return Err(range_error("poleTide.xpArcsec must be finite"));
        }
        if !self.yp_arcsec.is_finite() {
            return Err(range_error("poleTide.ypArcsec must be finite"));
        }
        Ok(PoleTideOptions {
            xp_arcsec: self.xp_arcsec,
            yp_arcsec: self.yp_arcsec,
        })
    }
}

/// Per-station ocean-loading BLQ coefficients. `amplitudeM` (metres) and
/// `phaseDeg` (degrees, positive lag) are each a `3 x 11` nested array indexed
/// `[component][constituent]`: component order radial/up (0), tangential EW/west
/// (1), tangential NS/south (2); constituent order M2 S2 N2 K2 K1 O1 P1 Q1 Mf Mm
/// Ssa.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OceanLoadingInput {
    amplitude_m: Vec<Vec<f64>>,
    phase_deg: Vec<Vec<f64>>,
}

fn blq_rows(name: &str, rows: &[Vec<f64>]) -> Result<[[f64; NUM_OCEAN_CONSTITUENTS]; 3], JsValue> {
    if rows.len() != 3 {
        return Err(type_error(&format!(
            "{name} must have 3 component rows (radial, EW, NS), got {}",
            rows.len()
        )));
    }
    let mut out = [[0.0; NUM_OCEAN_CONSTITUENTS]; 3];
    for (i, row) in rows.iter().enumerate() {
        if row.len() != NUM_OCEAN_CONSTITUENTS {
            return Err(type_error(&format!(
                "{name} row {i} must have {NUM_OCEAN_CONSTITUENTS} constituents, got {}",
                row.len()
            )));
        }
        for (j, &value) in row.iter().enumerate() {
            if !value.is_finite() {
                return Err(range_error(&format!("{name}[{i}][{j}] must be finite")));
            }
        }
        out[i].copy_from_slice(row);
    }
    Ok(out)
}

impl OceanLoadingInput {
    fn to_core(&self) -> Result<OceanLoadingBlq, JsValue> {
        Ok(OceanLoadingBlq {
            amplitude_m: blq_rows("oceanLoading.amplitudeM", &self.amplitude_m)?,
            phase_deg: blq_rows("oceanLoading.phaseDeg", &self.phase_deg)?,
        })
    }
}

/// PPP correction precompute switches.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct OptionsInput {
    solid_earth_tide: bool,
    phase_windup: bool,
    satellite_antenna: Option<SatelliteAntennaOptionsInput>,
    pole_tide: Option<PoleTideInput>,
    ocean_loading: Option<OceanLoadingInput>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SatObservablePairInput {
    sat: String,
    obs1: String,
    obs2: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SystemObservablePairInput {
    system: String,
    obs1: String,
    obs2: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct CodeBiasInput {
    used_observables_per_sat: Vec<SatObservablePairInput>,
    used_observables_default: Vec<SystemObservablePairInput>,
    clock_reference: Vec<SystemObservablePairInput>,
}

fn parse_system(value: &str) -> Result<GnssSystem, JsValue> {
    match value {
        "gps" => Ok(GnssSystem::Gps),
        "glonass" => Ok(GnssSystem::Glonass),
        "galileo" => Ok(GnssSystem::Galileo),
        "beidou" => Ok(GnssSystem::BeiDou),
        "qzss" => Ok(GnssSystem::Qzss),
        "navic" => Ok(GnssSystem::Navic),
        "sbas" => Ok(GnssSystem::Sbas),
        other => Err(type_error(&format!("invalid GNSS system label {other:?}"))),
    }
}

impl CodeBiasInput {
    fn to_core(&self, bias_set: &BiasSet) -> Result<CodeBiasOptions, JsValue> {
        Ok(CodeBiasOptions {
            bias_set: bias_set.core(),
            used_observables_per_sat: self
                .used_observables_per_sat
                .iter()
                .map(|entry| {
                    Ok((
                        parse_sat(&entry.sat)?,
                        (entry.obs1.clone(), entry.obs2.clone()),
                    ))
                })
                .collect::<Result<_, JsValue>>()?,
            used_observables_default: self
                .used_observables_default
                .iter()
                .map(|entry| {
                    Ok((
                        parse_system(&entry.system)?,
                        (entry.obs1.clone(), entry.obs2.clone()),
                    ))
                })
                .collect::<Result<_, JsValue>>()?,
            clock_reference: Some(ClockReferenceObservables {
                per_system: self
                    .clock_reference
                    .iter()
                    .map(|entry| {
                        Ok((
                            parse_system(&entry.system)?,
                            (entry.obs1.clone(), entry.obs2.clone()),
                        ))
                    })
                    .collect::<Result<_, JsValue>>()?,
            }),
        })
    }
}

impl OptionsInput {
    fn to_core(&self) -> Result<PppCorrectionsOptions, JsValue> {
        Ok(PppCorrectionsOptions {
            solid_earth_tide: self.solid_earth_tide,
            pole_tide: self
                .pole_tide
                .as_ref()
                .map(PoleTideInput::to_core)
                .transpose()?,
            ocean_loading: self
                .ocean_loading
                .as_ref()
                .map(OceanLoadingInput::to_core)
                .transpose()?,
            phase_windup: self.phase_windup,
            satellite_antenna: self
                .satellite_antenna
                .as_ref()
                .map(SatelliteAntennaOptionsInput::to_core)
                .transpose()?,
            code_bias: None,
        })
    }
}

fn parse_sat(token: &str) -> Result<GnssSatelliteId, JsValue> {
    GnssSatelliteId::from_str(token)
        .map_err(|_| type_error(&format!("invalid satellite token: {token}")))
}

// --- result objects ---------------------------------------------------------

/// `{ epochIndex, vectorM: [dx, dy, dz] }` (metres).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EpochVectorJs {
    epoch_index: usize,
    vector_m: [f64; 3],
}

/// `{ sat, epochIndex, valueM }` (metres).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SatScalarJs {
    sat: String,
    epoch_index: usize,
    value_m: f64,
}

/// `{ sat, epochIndex, vectorM: [dx, dy, dz] }` (metres).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SatVectorJs {
    sat: String,
    epoch_index: usize,
    vector_m: [f64; 3],
}

/// The precomputed correction tables.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PppCorrectionsJs {
    tide: Vec<EpochVectorJs>,
    pole_tide: Vec<EpochVectorJs>,
    ocean_loading: Vec<EpochVectorJs>,
    windup_m: Vec<SatScalarJs>,
    sat_pco_ecef: Vec<SatVectorJs>,
    sat_pcv_m: Vec<SatScalarJs>,
    code_bias_m: Vec<SatScalarJs>,
}

// --- entry point ------------------------------------------------------------

/// Precompute static PPP correction tables for a precise-orbit arc.
///
/// `epochs` is an array of `PppCorrectionEpoch` objects, `receiverEcefM` is the
/// fixed receiver position (`Float64Array` of length 3, metres), and `options`
/// selects which corrections to compute: `solidEarthTide` / `phaseWindup`
/// booleans, plus optional `satelliteAntenna` (a `PppSatelliteAntennaOptions`),
/// `poleTide` (a `PoleTideOptions`, the IERS polar motion of the date), and
/// `oceanLoading` (an `OceanLoadingBlq`, the station's BLQ block). Returns a
/// `PppCorrections` whose fields are each keyed by the input epoch index. Throws a
/// `TypeError` on a malformed shape or an invalid satellite token, and an `Error`
/// on an invalid epoch or a tide/coverage failure.
#[wasm_bindgen(js_name = pppCorrections)]
pub fn ppp_corrections(
    sp3: &Sp3,
    epochs: JsValue,
    receiver_ecef_m: &[f64],
    options: JsValue,
) -> Result<JsValue, JsValue> {
    let epochs: Vec<EpochInput> = serde_wasm_bindgen::from_value(epochs)
        .map_err(|e| type_error(&format!("invalid PPP correction epochs: {e}")))?;
    let opts: OptionsInput = if options.is_undefined() || options.is_null() {
        OptionsInput::default()
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid PPP correction options: {e}")))?
    };

    let receiver = vec3_finite("receiverEcefM", receiver_ecef_m)?;
    let core_epochs: Vec<PppCorrectionEpoch> = epochs
        .iter()
        .map(EpochInput::to_core)
        .collect::<Result<_, _>>()?;
    let core_options = opts.to_core()?;

    build_to_js(sp3, &core_epochs, receiver, core_options)
}

#[wasm_bindgen(js_name = pppCorrectionsWithCodeBias)]
pub fn ppp_corrections_with_code_bias(
    sp3: &Sp3,
    epochs: JsValue,
    receiver_ecef_m: &[f64],
    options: JsValue,
    bias_set: &BiasSet,
    code_bias: JsValue,
) -> Result<JsValue, JsValue> {
    let epochs: Vec<EpochInput> = serde_wasm_bindgen::from_value(epochs)
        .map_err(|e| type_error(&format!("invalid PPP correction epochs: {e}")))?;
    let opts: OptionsInput = if options.is_undefined() || options.is_null() {
        OptionsInput::default()
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid PPP correction options: {e}")))?
    };
    let code_bias: CodeBiasInput = serde_wasm_bindgen::from_value(code_bias)
        .map_err(|e| type_error(&format!("invalid code-bias options: {e}")))?;
    let receiver = vec3_finite("receiverEcefM", receiver_ecef_m)?;
    let core_epochs: Vec<PppCorrectionEpoch> = epochs
        .iter()
        .map(EpochInput::to_core)
        .collect::<Result<_, _>>()?;
    let mut core_options = opts.to_core()?;
    core_options.code_bias = Some(code_bias.to_core(bias_set)?);
    build_to_js(sp3, &core_epochs, receiver, core_options)
}

fn build_to_js(
    sp3: &Sp3,
    core_epochs: &[PppCorrectionEpoch],
    receiver: [f64; 3],
    core_options: PppCorrectionsOptions,
) -> Result<JsValue, JsValue> {
    let corrections = build(&sp3.inner, core_epochs, receiver, &core_options).map_err(ppp_err)?;

    let out = PppCorrectionsJs {
        tide: corrections
            .tide
            .iter()
            .map(|c| EpochVectorJs {
                epoch_index: c.epoch_index,
                vector_m: c.vector_m,
            })
            .collect(),
        pole_tide: corrections
            .pole_tide
            .iter()
            .map(|c| EpochVectorJs {
                epoch_index: c.epoch_index,
                vector_m: c.vector_m,
            })
            .collect(),
        ocean_loading: corrections
            .ocean_loading
            .iter()
            .map(|c| EpochVectorJs {
                epoch_index: c.epoch_index,
                vector_m: c.vector_m,
            })
            .collect(),
        windup_m: corrections
            .windup_m
            .iter()
            .map(|c| SatScalarJs {
                sat: c.sat.to_string(),
                epoch_index: c.epoch_index,
                value_m: c.value_m,
            })
            .collect(),
        sat_pco_ecef: corrections
            .sat_pco_ecef
            .iter()
            .map(|c| SatVectorJs {
                sat: c.sat.to_string(),
                epoch_index: c.epoch_index,
                vector_m: c.vector_m,
            })
            .collect(),
        sat_pcv_m: corrections
            .sat_pcv_m
            .iter()
            .map(|c| SatScalarJs {
                sat: c.sat.to_string(),
                epoch_index: c.epoch_index,
                value_m: c.value_m,
            })
            .collect(),
        code_bias_m: corrections
            .code_bias_m
            .iter()
            .map(|c| SatScalarJs {
                sat: c.sat.to_string(),
                epoch_index: c.epoch_index,
                value_m: c.value_m,
            })
            .collect(),
    };

    serde_wasm_bindgen::to_value(&out).map_err(|e| engine_error(e.to_string()))
}
