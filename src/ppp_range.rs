//! Owned precise-positioning range corrections and correction lookup builders.

use std::collections::BTreeMap;
use std::str::FromStr;

use js_sys::{Array, Object, Reflect};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::ppp_corrections::{
    EpochVectorCorrection, PppCorrections, SatScalarCorrection, SatVectorCorrection,
};
use sidereon_core::precise_positioning::{
    build_ppp_lookup as core_build_ppp_lookup,
    build_ppp_lookup_with_validity as core_build_ppp_lookup_with_validity, FloatEpoch, PcvSample,
    PppCorrectionLookup as CoreLookup, RangeCorrections, ReceiverAntennaFrequency,
    ReceiverAntennaOptions, SatelliteClockCorrections,
};
use sidereon_core::GnssSatelliteId;

use crate::error::{type_error, ut1_validity};
use crate::ppp::epochs_from_js;
use crate::ppp_corrections::{ppp_err, OptionsInput};
use crate::sp3::Sp3;

fn to_js<T: Serialize>(value: &T) -> JsValue {
    value
        .serialize(&serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true))
        .unwrap_or(JsValue::UNDEFINED)
}

/// One receiver antenna phase-centre variation sample.
#[wasm_bindgen]
#[derive(Clone)]
pub struct PppPcvSample {
    inner: PcvSample,
}

#[wasm_bindgen]
impl PppPcvSample {
    /// Construct a PCV sample. Omit `azimuthDeg` for a no-azimuth sample.
    #[wasm_bindgen(constructor)]
    pub fn new(zenith_deg: f64, value_m: f64, azimuth_deg: Option<f64>) -> Self {
        Self {
            inner: PcvSample {
                azimuth_deg,
                zenith_deg,
                value_m,
            },
        }
    }

    #[wasm_bindgen(getter, js_name = azimuthDeg)]
    pub fn azimuth_deg(&self) -> Option<f64> {
        self.inner.azimuth_deg
    }
    #[wasm_bindgen(getter, js_name = zenithDeg)]
    pub fn zenith_deg(&self) -> f64 {
        self.inner.zenith_deg
    }
    #[wasm_bindgen(getter, js_name = valueM)]
    pub fn value_m(&self) -> f64 {
        self.inner.value_m
    }
    pub fn equals(&self, other: &PppPcvSample) -> bool {
        self.inner == other.inner
    }
}

/// Receiver antenna calibration at one frequency.
#[wasm_bindgen]
#[derive(Clone)]
pub struct PppReceiverAntennaFrequency {
    inner: ReceiverAntennaFrequency,
}

#[wasm_bindgen]
impl PppReceiverAntennaFrequency {
    #[wasm_bindgen(constructor)]
    pub fn new(
        label: String,
        pco_m: &[f64],
        pcv_samples: Vec<PppPcvSample>,
    ) -> Result<Self, JsValue> {
        let pco_m: [f64; 3] = pco_m
            .try_into()
            .map_err(|_| type_error("pcoM must contain exactly 3 numbers"))?;
        Ok(Self {
            inner: ReceiverAntennaFrequency {
                label,
                pco_m,
                pcv_samples: pcv_samples.into_iter().map(|v| v.inner).collect(),
            },
        })
    }
    #[wasm_bindgen(getter)]
    pub fn label(&self) -> String {
        self.inner.label.clone()
    }
    #[wasm_bindgen(getter, js_name = pcoM)]
    pub fn pco_m(&self) -> Vec<f64> {
        self.inner.pco_m.to_vec()
    }
    #[wasm_bindgen(getter, js_name = pcvSamples)]
    pub fn pcv_samples(&self) -> Array {
        self.inner
            .pcv_samples
            .iter()
            .map(|inner| JsValue::from(PppPcvSample { inner: *inner }))
            .collect()
    }
    pub fn equals(&self, other: &PppReceiverAntennaFrequency) -> bool {
        self.inner == other.inner
    }
}

/// Receiver antenna correction options.
#[wasm_bindgen]
#[derive(Clone)]
pub struct PppReceiverAntennaOptions {
    inner: ReceiverAntennaOptions,
}

#[wasm_bindgen]
impl PppReceiverAntennaOptions {
    #[wasm_bindgen(constructor)]
    pub fn new(
        freq1_label: String,
        freq1_hz: f64,
        freq2_label: String,
        freq2_hz: f64,
        frequencies: Vec<PppReceiverAntennaFrequency>,
    ) -> Self {
        Self {
            inner: ReceiverAntennaOptions::new(
                freq1_label,
                freq1_hz,
                freq2_label,
                freq2_hz,
                frequencies.into_iter().map(|v| v.inner).collect(),
            ),
        }
    }
    #[wasm_bindgen(getter, js_name = freq1Label)]
    pub fn freq1_label(&self) -> String {
        self.inner.freq1_label.clone()
    }
    #[wasm_bindgen(getter, js_name = freq1Hz)]
    pub fn freq1_hz(&self) -> f64 {
        self.inner.freq1_hz
    }
    #[wasm_bindgen(getter, js_name = freq2Label)]
    pub fn freq2_label(&self) -> String {
        self.inner.freq2_label.clone()
    }
    #[wasm_bindgen(getter, js_name = freq2Hz)]
    pub fn freq2_hz(&self) -> f64 {
        self.inner.freq2_hz
    }
    #[wasm_bindgen(getter)]
    pub fn frequencies(&self) -> Array {
        self.inner
            .frequencies
            .iter()
            .cloned()
            .map(|inner| JsValue::from(PppReceiverAntennaFrequency { inner }))
            .collect()
    }
    pub fn equals(&self, other: &PppReceiverAntennaOptions) -> bool {
        self.inner == other.inner
    }
}

/// Fine satellite clock series keyed by satellite token.
#[wasm_bindgen]
#[derive(Clone)]
pub struct PppSatelliteClockCorrections {
    inner: SatelliteClockCorrections,
}

#[wasm_bindgen]
impl PppSatelliteClockCorrections {
    /// Construct from `{ G01: [[gpsSeconds, clockBiasSeconds], ...] }` or omit for the core default.
    #[wasm_bindgen(constructor)]
    pub fn new(series: Option<JsValue>) -> Result<Self, JsValue> {
        let Some(series) = series else {
            return Ok(Self {
                inner: SatelliteClockCorrections::default(),
            });
        };
        let object = Object::from(series);
        let mut out = BTreeMap::new();
        for key in Object::keys(&object).iter() {
            let token = key
                .as_string()
                .ok_or_else(|| type_error("satellite clock keys must be strings"))?;
            let sat = GnssSatelliteId::from_str(&token)
                .map_err(|_| type_error(&format!("invalid satellite clock token: {token}")))?;
            let value = Reflect::get(&object, &key)?;
            let rows: Vec<(f64, f64)> = serde_wasm_bindgen::from_value(value).map_err(|e| {
                type_error(&format!("invalid satellite clock series for {token}: {e}"))
            })?;
            out.insert(sat, rows);
        }
        Ok(Self {
            inner: SatelliteClockCorrections { series: out },
        })
    }
    #[wasm_bindgen(getter)]
    pub fn series(&self) -> JsValue {
        let rows: BTreeMap<String, &Vec<(f64, f64)>> = self
            .inner
            .series
            .iter()
            .map(|(sat, values)| (sat.to_string(), values))
            .collect();
        to_js(&rows)
    }
    pub fn equals(&self, other: &PppSatelliteClockCorrections) -> bool {
        self.inner == other.inner
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct EpochVector {
    epoch_index: usize,
    vector_m: [f64; 3],
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SatScalar {
    satellite_id: String,
    epoch_index: usize,
    value_m: f64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SatVector {
    satellite_id: String,
    epoch_index: usize,
    vector_m: [f64; 3],
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CorrectionsValue {
    tide: Vec<EpochVector>,
    pole_tide: Vec<EpochVector>,
    ocean_loading: Vec<EpochVector>,
    windup_m: Vec<CorrectionsSatScalar>,
    sat_pco_ecef: Vec<CorrectionsSatVector>,
    sat_pcv_m: Vec<CorrectionsSatScalar>,
    code_bias_m: Vec<CorrectionsSatScalar>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CorrectionsSatScalar {
    sat: String,
    epoch_index: usize,
    value_m: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CorrectionsSatVector {
    sat: String,
    epoch_index: usize,
    vector_m: [f64; 3],
}

fn correction_sat(token: String) -> Result<GnssSatelliteId, JsValue> {
    GnssSatelliteId::from_str(&token)
        .map_err(|_| type_error(&format!("invalid PPP corrections satellite token: {token}")))
}

/// Owned indexed PPP correction lookup.
#[wasm_bindgen]
#[derive(Clone)]
pub struct PppCorrectionLookup {
    pub(crate) inner: CoreLookup,
    degraded: Option<&'static str>,
}

#[wasm_bindgen]
impl PppCorrectionLookup {
    /// Construct the core default empty, disabled lookup.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: CoreLookup::default(),
            degraded: None,
        }
    }
    pub fn equals(&self, other: &PppCorrectionLookup) -> bool {
        self.inner == other.inner
    }
    /// Convert the plain value returned by `pppCorrections` into an owned lookup.
    #[wasm_bindgen(js_name = fromCorrections)]
    pub fn from_corrections(
        value: JsValue,
        options: JsValue,
    ) -> Result<PppCorrectionLookup, JsValue> {
        let value: CorrectionsValue = serde_wasm_bindgen::from_value(value)
            .map_err(|error| type_error(&format!("invalid PPP corrections value: {error}")))?;
        let options: OptionsInput = if options.is_null() || options.is_undefined() {
            OptionsInput::default()
        } else {
            serde_wasm_bindgen::from_value(options)
                .map_err(|error| type_error(&format!("invalid PPP correction options: {error}")))?
        };
        let corrections = PppCorrections {
            tide: value
                .tide
                .into_iter()
                .map(|row| EpochVectorCorrection {
                    epoch_index: row.epoch_index,
                    vector_m: row.vector_m,
                })
                .collect(),
            pole_tide: value
                .pole_tide
                .into_iter()
                .map(|row| EpochVectorCorrection {
                    epoch_index: row.epoch_index,
                    vector_m: row.vector_m,
                })
                .collect(),
            ocean_loading: value
                .ocean_loading
                .into_iter()
                .map(|row| EpochVectorCorrection {
                    epoch_index: row.epoch_index,
                    vector_m: row.vector_m,
                })
                .collect(),
            windup_m: value
                .windup_m
                .into_iter()
                .map(|row| {
                    Ok(SatScalarCorrection {
                        sat: correction_sat(row.sat)?,
                        epoch_index: row.epoch_index,
                        value_m: row.value_m,
                    })
                })
                .collect::<Result<_, JsValue>>()?,
            sat_pco_ecef: value
                .sat_pco_ecef
                .into_iter()
                .map(|row| {
                    Ok(SatVectorCorrection {
                        sat: correction_sat(row.sat)?,
                        epoch_index: row.epoch_index,
                        vector_m: row.vector_m,
                    })
                })
                .collect::<Result<_, JsValue>>()?,
            sat_pcv_m: value
                .sat_pcv_m
                .into_iter()
                .map(|row| {
                    Ok(SatScalarCorrection {
                        sat: correction_sat(row.sat)?,
                        epoch_index: row.epoch_index,
                        value_m: row.value_m,
                    })
                })
                .collect::<Result<_, JsValue>>()?,
            code_bias_m: value
                .code_bias_m
                .into_iter()
                .map(|row| {
                    Ok(SatScalarCorrection {
                        sat: correction_sat(row.sat)?,
                        epoch_index: row.epoch_index,
                        value_m: row.value_m,
                    })
                })
                .collect::<Result<_, JsValue>>()?,
            diagnostics: Default::default(),
        };
        Ok(PppCorrectionLookup {
            inner: CoreLookup::from_options(corrections, &options.to_core()?),
            degraded: None,
        })
    }
    #[wasm_bindgen(getter)]
    pub fn tide(&self) -> JsValue {
        to_js(
            &self
                .inner
                .tide
                .iter()
                .map(|(&epoch_index, &vector_m)| EpochVector {
                    epoch_index,
                    vector_m,
                })
                .collect::<Vec<_>>(),
        )
    }
    #[wasm_bindgen(getter, js_name = poleTide)]
    pub fn pole_tide(&self) -> JsValue {
        to_js(
            &self
                .inner
                .pole_tide
                .iter()
                .map(|(&epoch_index, &vector_m)| EpochVector {
                    epoch_index,
                    vector_m,
                })
                .collect::<Vec<_>>(),
        )
    }
    #[wasm_bindgen(getter, js_name = oceanLoading)]
    pub fn ocean_loading(&self) -> JsValue {
        to_js(
            &self
                .inner
                .ocean_loading
                .iter()
                .map(|(&epoch_index, &vector_m)| EpochVector {
                    epoch_index,
                    vector_m,
                })
                .collect::<Vec<_>>(),
        )
    }
    #[wasm_bindgen(getter, js_name = windupM)]
    pub fn windup_m(&self) -> JsValue {
        to_js(
            &self
                .inner
                .windup_m
                .iter()
                .map(|((sat, epoch_index), &value_m)| SatScalar {
                    satellite_id: sat.to_string(),
                    epoch_index: *epoch_index,
                    value_m,
                })
                .collect::<Vec<_>>(),
        )
    }
    #[wasm_bindgen(getter, js_name = satPcoEcef)]
    pub fn sat_pco_ecef(&self) -> JsValue {
        to_js(
            &self
                .inner
                .sat_pco_ecef
                .iter()
                .map(|((sat, epoch_index), &vector_m)| SatVector {
                    satellite_id: sat.to_string(),
                    epoch_index: *epoch_index,
                    vector_m,
                })
                .collect::<Vec<_>>(),
        )
    }
    #[wasm_bindgen(getter, js_name = satPcvM)]
    pub fn sat_pcv_m(&self) -> JsValue {
        to_js(
            &self
                .inner
                .sat_pcv_m
                .iter()
                .map(|((sat, epoch_index), &value_m)| SatScalar {
                    satellite_id: sat.to_string(),
                    epoch_index: *epoch_index,
                    value_m,
                })
                .collect::<Vec<_>>(),
        )
    }
    #[wasm_bindgen(getter, js_name = codeBiasM)]
    pub fn code_bias_m(&self) -> JsValue {
        to_js(
            &self
                .inner
                .code_bias_m
                .iter()
                .map(|((sat, epoch_index), &value_m)| SatScalar {
                    satellite_id: sat.to_string(),
                    epoch_index: *epoch_index,
                    value_m,
                })
                .collect::<Vec<_>>(),
        )
    }
    #[wasm_bindgen(getter, js_name = tideEnabled)]
    pub fn tide_enabled(&self) -> bool {
        self.inner.tide_enabled
    }
    #[wasm_bindgen(getter, js_name = poleTideEnabled)]
    pub fn pole_tide_enabled(&self) -> bool {
        self.inner.pole_tide_enabled
    }
    #[wasm_bindgen(getter, js_name = oceanLoadingEnabled)]
    pub fn ocean_loading_enabled(&self) -> bool {
        self.inner.ocean_loading_enabled
    }
    #[wasm_bindgen(getter, js_name = windupEnabled)]
    pub fn windup_enabled(&self) -> bool {
        self.inner.windup_enabled
    }
    #[wasm_bindgen(getter, js_name = satelliteAntennaEnabled)]
    pub fn satellite_antenna_enabled(&self) -> bool {
        self.inner.satellite_antenna_enabled
    }
    #[wasm_bindgen(getter, js_name = codeBiasEnabled)]
    pub fn code_bias_enabled(&self) -> bool {
        self.inner.code_bias_enabled
    }
    /// First permissive UT1 coverage departure, or `undefined`.
    #[wasm_bindgen(getter, js_name = ut1Degraded)]
    pub fn ut1_degraded(&self) -> Option<String> {
        self.degraded.map(str::to_owned)
    }
}

impl Default for PppCorrectionLookup {
    fn default() -> Self {
        Self::new()
    }
}

/// Owned complete range-correction selection for a PPP solve.
#[wasm_bindgen]
#[derive(Clone)]
pub struct PppRangeCorrections {
    pub(crate) inner: RangeCorrections,
}

#[wasm_bindgen]
impl PppRangeCorrections {
    #[wasm_bindgen(constructor)]
    pub fn new(
        receiver_antenna: Option<PppReceiverAntennaOptions>,
        sat_clock_relativity: bool,
        satellite_clock: Option<PppSatelliteClockCorrections>,
        ppp: Option<PppCorrectionLookup>,
    ) -> Self {
        Self {
            inner: RangeCorrections {
                receiver_antenna: receiver_antenna.map(|v| v.inner),
                sat_clock_relativity,
                satellite_clock: satellite_clock.map(|v| v.inner),
                ppp: ppp.map(|v| v.inner).unwrap_or_default(),
            },
        }
    }
    pub fn equals(&self, other: &PppRangeCorrections) -> bool {
        self.inner == other.inner
    }
    pub fn lookup(&self) -> PppCorrectionLookup {
        PppCorrectionLookup {
            inner: self.inner.ppp.clone(),
            degraded: None,
        }
    }
    #[wasm_bindgen(getter, js_name = satClockRelativity)]
    pub fn sat_clock_relativity(&self) -> bool {
        self.inner.sat_clock_relativity
    }
}

fn inputs(
    epochs: JsValue,
    options: JsValue,
) -> Result<
    (
        Vec<FloatEpoch>,
        sidereon_core::ppp_corrections::PppCorrectionsOptions,
    ),
    JsValue,
> {
    let epochs = epochs_from_js(epochs)?;
    let options: OptionsInput = if options.is_null() || options.is_undefined() {
        OptionsInput::default()
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid PPP correction options: {e}")))?
    };
    Ok((epochs, options.to_core()?))
}

/// Build an indexed lookup at the static receiver position.
#[wasm_bindgen(js_name = buildPppLookup)]
pub fn build_ppp_lookup(
    sp3: &Sp3,
    epochs: JsValue,
    receiver_ecef_m: &[f64],
    options: JsValue,
) -> Result<PppCorrectionLookup, JsValue> {
    let receiver: [f64; 3] = receiver_ecef_m
        .try_into()
        .map_err(|_| type_error("receiverEcefM must contain exactly 3 numbers"))?;
    let (epochs, options) = inputs(epochs, options)?;
    let inner = core_build_ppp_lookup(&sp3.inner, &epochs, receiver, &options).map_err(ppp_err)?;
    Ok(PppCorrectionLookup {
        inner,
        degraded: None,
    })
}

/// Build an indexed lookup with explicit UT1 validity policy.
#[wasm_bindgen(js_name = buildPppLookupWithValidity)]
pub fn build_ppp_lookup_with_validity(
    sp3: &Sp3,
    epochs: JsValue,
    receiver_ecef_m: &[f64],
    options: JsValue,
    validity: Option<String>,
) -> Result<PppCorrectionLookup, JsValue> {
    let receiver: [f64; 3] = receiver_ecef_m
        .try_into()
        .map_err(|_| type_error("receiverEcefM must contain exactly 3 numbers"))?;
    let (epochs, options) = inputs(epochs, options)?;
    let validated = core_build_ppp_lookup_with_validity(
        &sp3.inner,
        &epochs,
        receiver,
        &options,
        ut1_validity(validity)?,
    )
    .map_err(ppp_err)?;
    Ok(PppCorrectionLookup {
        inner: validated.value,
        degraded: validated.degraded.map(crate::spp::degrade_reason_label),
    })
}
