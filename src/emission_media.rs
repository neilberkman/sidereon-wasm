//! Emission-time satellite state and media-correction batch binding.
//!
//! The core evaluates satellite state, elevation cutoff, ionosphere delay, and
//! troposphere delay in one serial pass. This module only marshals the
//! index-aligned JS arrays and exposes the contiguous result arrays.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::ephemeris::Sp3 as CoreSp3;
use sidereon_core::observables::{
    emission_media_batch_at_j2000_s as core_emission_media_batch,
    EmissionMediaBatch as CoreEmissionMediaBatch,
    EmissionMediaBatchOptions as CoreEmissionMediaBatchOptions,
    EmissionMediaStatus as CoreEmissionMediaStatus, ObservableIonosphereCorrection,
    ObservableMediaOptions, ObservableTroposphereCorrection,
};
use sidereon_core::GnssSatelliteId;

use crate::error::{engine_error, range_error, type_error};
use crate::ionex::Ionex;
use crate::marshal::vec3_finite;

fn parse_sat(token: &str) -> Result<GnssSatelliteId, JsValue> {
    token
        .parse::<GnssSatelliteId>()
        .map_err(|e| type_error(&format!("invalid satellite token {token:?}: {e}")))
}

fn status_from_core(status: CoreEmissionMediaStatus) -> EmissionMediaStatus {
    match status {
        CoreEmissionMediaStatus::Valid => EmissionMediaStatus::Valid,
        CoreEmissionMediaStatus::Gap => EmissionMediaStatus::Gap,
        CoreEmissionMediaStatus::BelowElevationCutoff => EmissionMediaStatus::BelowElevationCutoff,
        CoreEmissionMediaStatus::Error => EmissionMediaStatus::Error,
    }
}

fn status_label(status: EmissionMediaStatus) -> &'static str {
    match status {
        EmissionMediaStatus::Valid => "valid",
        EmissionMediaStatus::Gap => "gap",
        EmissionMediaStatus::BelowElevationCutoff => "belowElevationCutoff",
        EmissionMediaStatus::Error => "error",
    }
}

fn option_to_nan(value: Option<f64>) -> f64 {
    value.unwrap_or(f64::NAN)
}

fn parse_satellites(tokens: Vec<String>) -> Result<Vec<GnssSatelliteId>, JsValue> {
    if tokens.is_empty() {
        return Err(type_error("satellites must not be empty"));
    }
    tokens
        .iter()
        .map(|sat| parse_sat(sat))
        .collect::<Result<Vec<_>, _>>()
}

fn check_lengths(satellites: &[GnssSatelliteId], epochs_j2000_s: &[f64]) -> Result<(), JsValue> {
    if satellites.len() != epochs_j2000_s.len() {
        return Err(type_error(&format!(
            "satellites ({}) and emissionEpochsJ2000S ({}) must have the same length",
            satellites.len(),
            epochs_j2000_s.len()
        )));
    }
    for (index, epoch) in epochs_j2000_s.iter().enumerate() {
        if !epoch.is_finite() {
            return Err(range_error(&format!(
                "emissionEpochsJ2000S[{index}] must be finite"
            )));
        }
    }
    Ok(())
}

fn decode_options(options: JsValue) -> Result<EmissionMediaOptionsInput, JsValue> {
    if options.is_undefined() || options.is_null() {
        Ok(EmissionMediaOptionsInput::default())
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid emission media options: {e}")))
    }
}

fn validate_options(input: &EmissionMediaOptionsInput) -> Result<(), JsValue> {
    if let Some(carrier_hz) = input.carrier_hz {
        if !(carrier_hz.is_finite() && carrier_hz > 0.0) {
            return Err(range_error("carrierHz must be finite and positive"));
        }
    }
    if let Some(min_elevation_rad) = input.min_elevation_rad {
        if !min_elevation_rad.is_finite() {
            return Err(range_error("minElevationRad must be finite"));
        }
    }
    Ok(())
}

fn core_options<'a>(
    input: &EmissionMediaOptionsInput,
    ionex: Option<&'a sidereon_core::atmosphere::Ionex>,
) -> CoreEmissionMediaBatchOptions<'a> {
    let defaults = CoreEmissionMediaBatchOptions::default();
    let ionosphere_default = ionex.is_some();
    CoreEmissionMediaBatchOptions {
        carrier_hz: input.carrier_hz.unwrap_or(defaults.carrier_hz),
        media: ObservableMediaOptions {
            troposphere: input
                .troposphere
                .unwrap_or(false)
                .then_some(ObservableTroposphereCorrection::default()),
            ionosphere: if input.ionosphere.unwrap_or(ionosphere_default) {
                ionex.map(ObservableIonosphereCorrection::Ionex)
            } else {
                None
            },
        },
        min_elevation_rad: input.min_elevation_rad,
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct EmissionMediaOptionsInput {
    carrier_hz: Option<f64>,
    ionosphere: Option<bool>,
    troposphere: Option<bool>,
    min_elevation_rad: Option<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EmissionMediaElementResultJs {
    ok: bool,
    error: Option<String>,
}

/// Per-satellite status for an emission media batch row.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmissionMediaStatus {
    /// State, clock, and requested media corrections were produced.
    Valid,
    /// The ephemeris product has no usable state for this satellite and epoch.
    Gap,
    /// A state was available, but elevation was below the requested cutoff.
    BelowElevationCutoff,
    /// A non-gap scalar evaluation error occurred.
    Error,
}

/// Stable string label for an [`EmissionMediaStatus`] enum value.
#[wasm_bindgen(js_name = emissionMediaStatusLabel)]
pub fn emission_media_status_label(status: EmissionMediaStatus) -> String {
    status_label(status).to_string()
}

/// Contiguous state and media-correction arrays from one emission batch.
#[wasm_bindgen]
pub struct EmissionMediaBatch {
    inner: CoreEmissionMediaBatch,
}

impl EmissionMediaBatch {
    pub(crate) fn from_inner(inner: CoreEmissionMediaBatch) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl EmissionMediaBatch {
    /// Number of satellite rows in the batch.
    #[wasm_bindgen(getter)]
    pub fn count(&self) -> usize {
        self.inner.len()
    }

    /// Satellite ECEF positions as flat `[x0, y0, z0, ...]`, metres.
    ///
    /// Rows without a usable position are filled with `NaN`; check `statuses`
    /// before consuming the corresponding row.
    #[wasm_bindgen(getter, js_name = positionEcefM)]
    pub fn position_ecef_m(&self) -> Vec<f64> {
        let mut out = Vec::with_capacity(self.inner.positions_ecef_m.len() * 3);
        for position in &self.inner.positions_ecef_m {
            out.extend_from_slice(&position.unwrap_or([f64::NAN; 3]));
        }
        out
    }

    /// Satellite clock offsets in seconds, with `NaN` for unavailable rows.
    #[wasm_bindgen(getter, js_name = clockS)]
    pub fn clock_s(&self) -> Vec<f64> {
        self.inner
            .clocks_s
            .iter()
            .copied()
            .map(option_to_nan)
            .collect()
    }

    /// Ionospheric slant group delays in metres, with `NaN` for unavailable rows.
    #[wasm_bindgen(getter, js_name = ionosphereSlantDelayM)]
    pub fn ionosphere_slant_delay_m(&self) -> Vec<f64> {
        self.inner
            .ionosphere_slant_delays_m
            .iter()
            .copied()
            .map(option_to_nan)
            .collect()
    }

    /// Tropospheric slant delays in metres, with `NaN` for unavailable rows.
    #[wasm_bindgen(getter, js_name = troposphereDelayM)]
    pub fn troposphere_delay_m(&self) -> Vec<f64> {
        self.inner
            .troposphere_delays_m
            .iter()
            .copied()
            .map(option_to_nan)
            .collect()
    }

    /// Per-row typed status values, index-aligned with the numeric arrays.
    #[wasm_bindgen(getter)]
    pub fn statuses(&self) -> Vec<EmissionMediaStatus> {
        self.inner
            .statuses
            .iter()
            .copied()
            .map(status_from_core)
            .collect()
    }

    /// Per-row status labels, index-aligned with the numeric arrays.
    #[wasm_bindgen(getter, js_name = statusLabels)]
    pub fn status_labels(&self) -> Vec<String> {
        self.statuses()
            .iter()
            .map(|status| status_label(*status).to_string())
            .collect()
    }

    /// Per-row success and error messages as `{ ok, error }[]`.
    #[wasm_bindgen(getter, js_name = elementResults)]
    pub fn element_results(&self) -> Result<JsValue, JsValue> {
        let rows = self
            .inner
            .element_errors
            .iter()
            .map(|error| EmissionMediaElementResultJs {
                ok: error.is_none(),
                error: error.as_ref().map(ToString::to_string),
            })
            .collect::<Vec<_>>();
        serde_wasm_bindgen::to_value(&rows).map_err(|e| engine_error(e.to_string()))
    }

    /// Error message for row `index`, or `undefined` when the row has no error.
    pub fn error(&self, index: usize) -> Result<Option<String>, JsValue> {
        self.inner
            .element_errors
            .get(index)
            .map(|error| error.as_ref().map(ToString::to_string))
            .ok_or_else(|| range_error(&format!("row index {index} out of range")))
    }
}

pub(crate) fn emission_media_batch_sp3(
    sp3: &CoreSp3,
    satellites: Vec<String>,
    emission_epochs_j2000_s: &[f64],
    receiver_ecef_m: &[f64],
    options: JsValue,
) -> Result<EmissionMediaBatch, JsValue> {
    let satellites = parse_satellites(satellites)?;
    check_lengths(&satellites, emission_epochs_j2000_s)?;
    let receiver = vec3_finite("receiverEcefM", receiver_ecef_m)?;
    let options = decode_options(options)?;
    validate_options(&options)?;
    if options.ionosphere == Some(true) {
        return Err(type_error(
            "options.ionosphere requires an Ionex product; use emissionMediaBatchIonex",
        ));
    }
    let inner = core_emission_media_batch(
        sp3,
        &satellites,
        emission_epochs_j2000_s,
        receiver,
        core_options(&options, None),
    )
    .map_err(engine_error)?;
    Ok(EmissionMediaBatch::from_inner(inner))
}

pub(crate) fn emission_media_batch_sp3_ionex(
    sp3: &CoreSp3,
    ionex: &Ionex,
    satellites: Vec<String>,
    emission_epochs_j2000_s: &[f64],
    receiver_ecef_m: &[f64],
    options: JsValue,
) -> Result<EmissionMediaBatch, JsValue> {
    let satellites = parse_satellites(satellites)?;
    check_lengths(&satellites, emission_epochs_j2000_s)?;
    let receiver = vec3_finite("receiverEcefM", receiver_ecef_m)?;
    let options = decode_options(options)?;
    validate_options(&options)?;
    let inner = core_emission_media_batch(
        sp3,
        &satellites,
        emission_epochs_j2000_s,
        receiver,
        core_options(&options, Some(&ionex.inner)),
    )
    .map_err(engine_error)?;
    Ok(EmissionMediaBatch::from_inner(inner))
}
