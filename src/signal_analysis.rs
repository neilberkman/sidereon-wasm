//! GNSS signal-analysis binding.
//!
//! The exported modulation resource delegates spectrum, tracking, and
//! interference metrics to `sidereon_core::signal::analysis`.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::signal::analysis as core;

use crate::error::{engine_error, range_error, type_error};

fn signal_error<E: std::fmt::Display>(err: E) -> JsValue {
    range_error(&err.to_string())
}

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| engine_error(format!("failed to serialize signal-analysis result: {e}")))
}

fn dll_options(options: JsValue) -> Result<core::DllTrackingOptions, JsValue> {
    let input: DllTrackingOptionsInput = serde_wasm_bindgen::from_value(options)
        .map_err(|e| type_error(&format!("invalid DLL tracking options: {e}")))?;
    Ok(core::DllTrackingOptions {
        cn0_db_hz: input.cn0_db_hz,
        loop_bandwidth_hz: input.loop_bandwidth_hz,
        integration_time_s: input.integration_time_s,
        correlator_spacing_chips: input.correlator_spacing_chips,
        receiver_bandwidth_hz: input.receiver_bandwidth_hz,
    })
}

fn multipath_options(options: JsValue) -> Result<core::MultipathOptions, JsValue> {
    let input: MultipathOptionsInput = serde_wasm_bindgen::from_value(options)
        .map_err(|e| type_error(&format!("invalid multipath options: {e}")))?;
    Ok(core::MultipathOptions {
        multipath_to_direct_ratio: input.multipath_to_direct_ratio,
        correlator_spacing_chips: input.correlator_spacing_chips,
        receiver_bandwidth_hz: input.receiver_bandwidth_hz,
    })
}

/// Early-late DLL processing mode for thermal-noise jitter.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DllProcessing {
    /// Coherent early-minus-late processing.
    Coherent,
    /// Non-coherent early-minus-late power processing with squaring loss.
    NonCoherent,
}

impl From<DllProcessing> for core::DllProcessing {
    fn from(value: DllProcessing) -> Self {
        match value {
            DllProcessing::Coherent => Self::Coherent,
            DllProcessing::NonCoherent => Self::NonCoherent,
        }
    }
}

/// Stable GNSS signal modulation used by the analysis functions.
#[wasm_bindgen]
pub struct SignalAnalysisModulation {
    inner: core::SignalModulation,
}

#[wasm_bindgen]
impl SignalAnalysisModulation {
    /// Build a BPSK(n) modulation, where the code rate is `n * 1.023 MHz`.
    #[wasm_bindgen]
    pub fn bpsk(order: f64) -> Result<SignalAnalysisModulation, JsValue> {
        Ok(SignalAnalysisModulation {
            inner: core::SignalModulation::bpsk(order).map_err(signal_error)?,
        })
    }

    /// Build a sine-phased BOC(m,n) modulation.
    #[wasm_bindgen(js_name = sineBoc)]
    pub fn sine_boc(m: f64, n: f64) -> Result<SignalAnalysisModulation, JsValue> {
        Ok(SignalAnalysisModulation {
            inner: core::SignalModulation::boc_sine(m, n).map_err(signal_error)?,
        })
    }

    /// Build a cosine-phased BOC(m,n) modulation.
    #[wasm_bindgen(js_name = cosineBoc)]
    pub fn cosine_boc(m: f64, n: f64) -> Result<SignalAnalysisModulation, JsValue> {
        Ok(SignalAnalysisModulation {
            inner: core::SignalModulation::boc_cosine(m, n).map_err(signal_error)?,
        })
    }

    /// Build the normalized MBOC(6,1,1/11) spectrum.
    #[wasm_bindgen(js_name = mboc611Over11)]
    pub fn mboc_6_1_1_over_11() -> SignalAnalysisModulation {
        SignalAnalysisModulation {
            inner: core::SignalModulation::mboc_6_1_1_over_11(),
        }
    }

    /// Build the GPS L1C pilot TMBOC(6,1,4/33) spectrum.
    #[wasm_bindgen(js_name = tmboc614Over33)]
    pub fn tmboc_6_1_4_over_33() -> SignalAnalysisModulation {
        SignalAnalysisModulation {
            inner: core::SignalModulation::tmboc_6_1_4_over_33(),
        }
    }

    /// Build Galileo E1 CBOC(6,1,1/11) with the plus chip-pulse convention.
    #[wasm_bindgen(js_name = cboc611Over11Plus)]
    pub fn cboc_6_1_1_over_11_plus() -> SignalAnalysisModulation {
        SignalAnalysisModulation {
            inner: core::SignalModulation::cboc_6_1_1_over_11(core::CbocSign::Plus),
        }
    }

    /// Build Galileo E1 CBOC(6,1,1/11) with the minus chip-pulse convention.
    #[wasm_bindgen(js_name = cboc611Over11Minus)]
    pub fn cboc_6_1_1_over_11_minus() -> SignalAnalysisModulation {
        SignalAnalysisModulation {
            inner: core::SignalModulation::cboc_6_1_1_over_11(core::CbocSign::Minus),
        }
    }

    /// Stable core label for this modulation.
    #[wasm_bindgen(getter)]
    pub fn label(&self) -> String {
        self.inner.label().to_string()
    }

    /// Code rate in hertz when the modulation has one unambiguous rate.
    #[wasm_bindgen(getter, js_name = codeRateHz)]
    pub fn code_rate_hz(&self) -> Result<f64, JsValue> {
        self.inner.code_rate_hz().map_err(signal_error)
    }

    /// Normalized power spectral density at an offset frequency, in `1/Hz`.
    #[wasm_bindgen(js_name = psdHz)]
    pub fn psd_hz(&self, offset_hz: f64) -> Result<f64, JsValue> {
        self.inner.psd_hz(offset_hz).map_err(signal_error)
    }

    /// Signal power inside a two-sided receiver bandwidth.
    #[wasm_bindgen(js_name = powerInBand)]
    pub fn power_in_band(&self, receiver_bandwidth_hz: f64) -> Result<f64, JsValue> {
        core::power_in_band(&self.inner, receiver_bandwidth_hz).map_err(signal_error)
    }

    /// Fraction of total signal power inside a two-sided receiver bandwidth.
    #[wasm_bindgen(js_name = fractionPowerInBand)]
    pub fn fraction_power_in_band(&self, receiver_bandwidth_hz: f64) -> Result<f64, JsValue> {
        core::fraction_power_in_band(&self.inner, receiver_bandwidth_hz).map_err(signal_error)
    }

    /// RMS, or Gabor, bandwidth over a two-sided receiver bandwidth.
    #[wasm_bindgen(js_name = rmsBandwidthHz)]
    pub fn rms_bandwidth_hz(&self, receiver_bandwidth_hz: f64) -> Result<f64, JsValue> {
        core::rms_bandwidth_hz(&self.inner, receiver_bandwidth_hz).map_err(signal_error)
    }

    /// Normalized in-band autocorrelation at a delay.
    #[wasm_bindgen]
    pub fn autocorrelation(
        &self,
        delay_s: f64,
        receiver_bandwidth_hz: f64,
    ) -> Result<f64, JsValue> {
        core::autocorrelation(&self.inner, delay_s, receiver_bandwidth_hz).map_err(signal_error)
    }

    /// SSC against white interference normalized over the receiver bandwidth.
    #[wasm_bindgen(js_name = whiteNoiseSpectralSeparationHz)]
    pub fn white_noise_spectral_separation_hz(
        &self,
        receiver_bandwidth_hz: f64,
    ) -> Result<f64, JsValue> {
        core::white_noise_spectral_separation_hz(&self.inner, receiver_bandwidth_hz)
            .map_err(signal_error)
    }

    /// Early-late DLL thermal-noise jitter for this modulation.
    #[wasm_bindgen(js_name = dllThermalNoiseJitter)]
    pub fn dll_thermal_noise_jitter(
        &self,
        options: JsValue,
        processing: DllProcessing,
    ) -> Result<JsValue, JsValue> {
        let out =
            core::dll_thermal_noise_jitter(&self.inner, dll_options(options)?, processing.into())
                .map_err(signal_error)?;
        to_js(&DllJitterJs::from(out))
    }

    /// Published lower bound for DLL code-delay jitter.
    #[wasm_bindgen(js_name = dllLowerBound)]
    pub fn dll_lower_bound(&self, options: JsValue) -> Result<JsValue, JsValue> {
        let out =
            core::dll_lower_bound(&self.inner, dll_options(options)?).map_err(signal_error)?;
        to_js(&DllJitterJs::from(out))
    }

    /// One-path early-late multipath error envelope on a delay grid.
    #[wasm_bindgen(js_name = multipathErrorEnvelope)]
    pub fn multipath_error_envelope(
        &self,
        options: JsValue,
        delay_chips: &[f64],
    ) -> Result<JsValue, JsValue> {
        let out =
            core::multipath_error_envelope(&self.inner, multipath_options(options)?, delay_chips)
                .map_err(signal_error)?;
        let out: Vec<MultipathEnvelopePointJs> = out
            .into_iter()
            .map(MultipathEnvelopePointJs::from)
            .collect();
        to_js(&out)
    }
}

/// Spectral separation coefficient between two modulations, in hertz.
#[wasm_bindgen(js_name = spectralSeparationCoefficientHz)]
pub fn spectral_separation_coefficient_hz(
    desired: &SignalAnalysisModulation,
    interference: &SignalAnalysisModulation,
    receiver_bandwidth_hz: f64,
) -> Result<f64, JsValue> {
    core::spectral_separation_coefficient_hz(
        &desired.inner,
        &interference.inner,
        receiver_bandwidth_hz,
    )
    .map_err(signal_error)
}

/// Spectral separation coefficient between two modulations, in dB-Hz.
#[wasm_bindgen(js_name = spectralSeparationCoefficientDbHz)]
pub fn spectral_separation_coefficient_db_hz(
    desired: &SignalAnalysisModulation,
    interference: &SignalAnalysisModulation,
    receiver_bandwidth_hz: f64,
) -> Result<f64, JsValue> {
    core::spectral_separation_coefficient_db_hz(
        &desired.inner,
        &interference.inner,
        receiver_bandwidth_hz,
    )
    .map_err(signal_error)
}

/// Effective C/N0 degradation for one finite-band interference term.
#[wasm_bindgen(js_name = effectiveCn0Degradation)]
pub fn effective_cn0_degradation(
    desired: &SignalAnalysisModulation,
    interference: &SignalAnalysisModulation,
    cn0_db_hz: f64,
    receiver_bandwidth_hz: f64,
    power_ratio_to_carrier: f64,
) -> Result<JsValue, JsValue> {
    let term = core::InterferenceTerm::new(interference.inner.clone(), power_ratio_to_carrier);
    let out =
        core::effective_cn0_degradation(&desired.inner, cn0_db_hz, receiver_bandwidth_hz, &[term])
            .map_err(signal_error)?;
    to_js(&Cn0DegradationJs::from(out))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DllTrackingOptionsInput {
    cn0_db_hz: f64,
    loop_bandwidth_hz: f64,
    integration_time_s: f64,
    correlator_spacing_chips: f64,
    receiver_bandwidth_hz: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MultipathOptionsInput {
    multipath_to_direct_ratio: f64,
    correlator_spacing_chips: f64,
    receiver_bandwidth_hz: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Cn0DegradationJs {
    effective_cn0_hz: f64,
    effective_cn0_db_hz: f64,
    degradation_db: f64,
}

impl From<core::Cn0Degradation> for Cn0DegradationJs {
    fn from(value: core::Cn0Degradation) -> Self {
        Self {
            effective_cn0_hz: value.effective_cn0_hz,
            effective_cn0_db_hz: value.effective_cn0_db_hz,
            degradation_db: value.degradation_db,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DllJitterJs {
    seconds: f64,
    chips: f64,
    meters: f64,
    squaring_loss: f64,
}

impl From<core::DllJitter> for DllJitterJs {
    fn from(value: core::DllJitter) -> Self {
        Self {
            seconds: value.seconds,
            chips: value.chips,
            meters: value.meters,
            squaring_loss: value.squaring_loss,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MultipathEnvelopePointJs {
    delay_chips: f64,
    delay_s: f64,
    in_phase_chips: f64,
    in_phase_s: f64,
    in_phase_m: f64,
    anti_phase_chips: f64,
    anti_phase_s: f64,
    anti_phase_m: f64,
    running_average_chips: f64,
    running_average_s: f64,
    running_average_m: f64,
}

impl From<core::MultipathEnvelopePoint> for MultipathEnvelopePointJs {
    fn from(value: core::MultipathEnvelopePoint) -> Self {
        Self {
            delay_chips: value.delay_chips,
            delay_s: value.delay_s,
            in_phase_chips: value.in_phase_chips,
            in_phase_s: value.in_phase_s,
            in_phase_m: value.in_phase_m,
            anti_phase_chips: value.anti_phase_chips,
            anti_phase_s: value.anti_phase_s,
            anti_phase_m: value.anti_phase_m,
            running_average_chips: value.running_average_chips,
            running_average_s: value.running_average_s,
            running_average_m: value.running_average_m,
        }
    }
}
