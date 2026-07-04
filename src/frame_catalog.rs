//! Terrestrial frame catalog binding.
//!
//! The built-in 14-parameter Helmert catalog and all epoch-aware transforms
//! live in `sidereon_core::frame_catalog`. This module exposes the catalog as
//! WASM handles and marshals Cartesian metre arrays across the JS boundary.

use serde::Serialize;
use wasm_bindgen::prelude::*;

use sidereon_core::frame_catalog::{
    catalog as core_catalog, catalog_entry as core_catalog_entry,
    propagate_position as core_propagate_position, transform as core_transform,
    transform_from_epoch as core_transform_from_epoch, FrameCatalogError as CoreFrameCatalogError,
    HelmertParameters as CoreHelmertParameters, HelmertRates as CoreHelmertRates,
    HelmertTransform as CoreHelmertTransform, TerrestrialFrame as CoreTerrestrialFrame,
    TerrestrialPositionM, TerrestrialVelocityMPerYear,
};

use crate::error::{engine_error, range_error, type_error};
use crate::marshal::vec3_finite;

/// A supported terrestrial reference-frame realization.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TerrestrialFrame {
    /// International Terrestrial Reference Frame 2020.
    Itrf2020,
    /// International Terrestrial Reference Frame 2014.
    Itrf2014,
    /// International Terrestrial Reference Frame 2008.
    Itrf2008,
    /// European Terrestrial Reference Frame 2020.
    Etrf2020,
}

impl From<TerrestrialFrame> for CoreTerrestrialFrame {
    fn from(frame: TerrestrialFrame) -> Self {
        match frame {
            TerrestrialFrame::Itrf2020 => Self::Itrf2020,
            TerrestrialFrame::Itrf2014 => Self::Itrf2014,
            TerrestrialFrame::Itrf2008 => Self::Itrf2008,
            TerrestrialFrame::Etrf2020 => Self::Etrf2020,
        }
    }
}

impl From<CoreTerrestrialFrame> for TerrestrialFrame {
    fn from(frame: CoreTerrestrialFrame) -> Self {
        match frame {
            CoreTerrestrialFrame::Itrf2020 => Self::Itrf2020,
            CoreTerrestrialFrame::Itrf2014 => Self::Itrf2014,
            CoreTerrestrialFrame::Itrf2008 => Self::Itrf2008,
            CoreTerrestrialFrame::Etrf2020 => Self::Etrf2020,
        }
    }
}

/// Stable string label for a [`TerrestrialFrame`] enum value.
#[wasm_bindgen(js_name = terrestrialFrameLabel)]
pub fn terrestrial_frame_label(frame: TerrestrialFrame) -> String {
    CoreTerrestrialFrame::from(frame).to_string()
}

fn frame_catalog_error(error: CoreFrameCatalogError) -> JsValue {
    match error {
        CoreFrameCatalogError::InvalidInput { .. } => range_error(&error.to_string()),
        CoreFrameCatalogError::NoCatalogPath { .. } => type_error(&error.to_string()),
        CoreFrameCatalogError::SingularTransform { .. } => engine_error(error),
    }
}

fn position(values: &[f64], field: &str) -> Result<TerrestrialPositionM, JsValue> {
    TerrestrialPositionM::from_array(vec3_finite(field, values)?).map_err(frame_catalog_error)
}

fn velocity(values: &[f64], field: &str) -> Result<TerrestrialVelocityMPerYear, JsValue> {
    TerrestrialVelocityMPerYear::from_array(vec3_finite(field, values)?)
        .map_err(frame_catalog_error)
}

fn optional_velocity(
    values: Option<Vec<f64>>,
    field: &str,
) -> Result<Option<TerrestrialVelocityMPerYear>, JsValue> {
    values
        .as_deref()
        .map(|values| velocity(values, field))
        .transpose()
}

fn finite(value: f64, field: &str) -> Result<f64, JsValue> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(range_error(&format!("{field} must be a finite number")))
    }
}

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| engine_error(format!("failed to serialize result: {e}")))
}

/// Helmert parameters in the units used by the published catalog tables.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct HelmertParameters {
    inner: CoreHelmertParameters,
}

impl From<CoreHelmertParameters> for HelmertParameters {
    fn from(inner: CoreHelmertParameters) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl HelmertParameters {
    /// Construct Helmert parameters from translation millimetres, scale parts
    /// per billion, and rotation milliarcseconds.
    #[wasm_bindgen(constructor)]
    pub fn new(
        translation_mm: &[f64],
        scale_ppb: f64,
        rotation_mas: &[f64],
    ) -> Result<HelmertParameters, JsValue> {
        Ok(Self {
            inner: CoreHelmertParameters {
                translation_mm: vec3_finite("translationMm", translation_mm)?,
                scale_ppb: finite(scale_ppb, "scalePpb")?,
                rotation_mas: vec3_finite("rotationMas", rotation_mas)?,
            },
        })
    }

    /// Translation components `[Tx, Ty, Tz]`, in millimetres.
    #[wasm_bindgen(getter, js_name = translationMm)]
    pub fn translation_mm(&self) -> Vec<f64> {
        self.inner.translation_mm.to_vec()
    }

    /// Scale difference, in parts per billion.
    #[wasm_bindgen(getter, js_name = scalePpb)]
    pub fn scale_ppb(&self) -> f64 {
        self.inner.scale_ppb
    }

    /// Rotation components `[Rx, Ry, Rz]`, in milliarcseconds.
    #[wasm_bindgen(getter, js_name = rotationMas)]
    pub fn rotation_mas(&self) -> Vec<f64> {
        self.inner.rotation_mas.to_vec()
    }
}

/// Helmert parameter rates in the units used by the published catalog tables.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct HelmertRates {
    inner: CoreHelmertRates,
}

impl From<CoreHelmertRates> for HelmertRates {
    fn from(inner: CoreHelmertRates) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl HelmertRates {
    /// Construct Helmert parameter rates from translation millimetres per year,
    /// scale parts per billion per year, and rotation milliarcseconds per year.
    #[wasm_bindgen(constructor)]
    pub fn new(
        translation_mm_per_year: &[f64],
        scale_ppb_per_year: f64,
        rotation_mas_per_year: &[f64],
    ) -> Result<HelmertRates, JsValue> {
        Ok(Self {
            inner: CoreHelmertRates {
                translation_mm_per_year: vec3_finite(
                    "translationMmPerYear",
                    translation_mm_per_year,
                )?,
                scale_ppb_per_year: finite(scale_ppb_per_year, "scalePpbPerYear")?,
                rotation_mas_per_year: vec3_finite("rotationMasPerYear", rotation_mas_per_year)?,
            },
        })
    }

    /// Translation rates `[Tx, Ty, Tz]`, in millimetres per year.
    #[wasm_bindgen(getter, js_name = translationMmPerYear)]
    pub fn translation_mm_per_year(&self) -> Vec<f64> {
        self.inner.translation_mm_per_year.to_vec()
    }

    /// Scale rate, in parts per billion per year.
    #[wasm_bindgen(getter, js_name = scalePpbPerYear)]
    pub fn scale_ppb_per_year(&self) -> f64 {
        self.inner.scale_ppb_per_year
    }

    /// Rotation rates `[Rx, Ry, Rz]`, in milliarcseconds per year.
    #[wasm_bindgen(getter, js_name = rotationMasPerYear)]
    pub fn rotation_mas_per_year(&self) -> Vec<f64> {
        self.inner.rotation_mas_per_year.to_vec()
    }
}

/// One published 14-parameter Helmert catalog entry.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct HelmertTransform {
    inner: CoreHelmertTransform,
}

impl From<CoreHelmertTransform> for HelmertTransform {
    fn from(inner: CoreHelmertTransform) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl HelmertTransform {
    /// Published source frame for the forward transform.
    #[wasm_bindgen(getter)]
    pub fn from(&self) -> TerrestrialFrame {
        self.inner.from.into()
    }

    /// Published target frame for the forward transform.
    #[wasm_bindgen(getter)]
    pub fn to(&self) -> TerrestrialFrame {
        self.inner.to.into()
    }

    /// Parameter reference epoch, expressed as a decimal year.
    #[wasm_bindgen(getter, js_name = referenceEpochYear)]
    pub fn reference_epoch_year(&self) -> f64 {
        self.inner.reference_epoch_year
    }

    /// Parameters at `referenceEpochYear`.
    #[wasm_bindgen(getter)]
    pub fn parameters(&self) -> HelmertParameters {
        self.inner.parameters.into()
    }

    /// Linear rates of the seven Helmert parameters.
    #[wasm_bindgen(getter)]
    pub fn rates(&self) -> HelmertRates {
        self.inner.rates.into()
    }

    /// Published table or memo that supplied this entry.
    #[wasm_bindgen(getter)]
    pub fn provenance(&self) -> String {
        self.inner.provenance.to_string()
    }

    /// Evaluate the seven Helmert parameters at a decimal year.
    #[wasm_bindgen(js_name = parametersAt)]
    pub fn parameters_at(&self, epoch_year: f64) -> Result<HelmertParameters, JsValue> {
        self.inner
            .parameters_at(epoch_year)
            .map(HelmertParameters::from)
            .map_err(frame_catalog_error)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TerrestrialStateJs {
    position_m: [f64; 3],
    velocity_m_per_year: Option<[f64; 3]>,
}

/// Return the built-in terrestrial frame catalog entries.
#[wasm_bindgen(js_name = frameCatalog)]
pub fn frame_catalog() -> Vec<HelmertTransform> {
    core_catalog()
        .iter()
        .copied()
        .map(|inner| HelmertTransform { inner })
        .collect()
}

/// Return the published catalog entry for the requested forward direction.
#[wasm_bindgen(js_name = frameCatalogEntry)]
pub fn frame_catalog_entry(
    from: TerrestrialFrame,
    to: TerrestrialFrame,
) -> Option<HelmertTransform> {
    core_catalog_entry(from.into(), to.into())
        .copied()
        .map(|inner| HelmertTransform { inner })
}

/// Propagate a station position from one decimal year to another.
///
/// `positionM` and `velocityMPerYear` are length-3 arrays in metres and metres
/// per year. Returns a length-3 array in metres. Delegates to
/// `sidereon_core::frame_catalog::propagate_position`.
#[wasm_bindgen(js_name = frameCatalogPropagatePosition)]
pub fn frame_catalog_propagate_position(
    position_m: &[f64],
    velocity_m_per_year: &[f64],
    from_epoch_year: f64,
    to_epoch_year: f64,
) -> Result<Vec<f64>, JsValue> {
    let position = position(position_m, "positionM")?;
    let velocity = velocity(velocity_m_per_year, "velocityMPerYear")?;
    let out = core_propagate_position(position, velocity, from_epoch_year, to_epoch_year)
        .map_err(frame_catalog_error)?;
    Ok(out.as_array().to_vec())
}

/// Transform a Cartesian station position and optional velocity between frames.
///
/// `positionM` is a length-3 metre array. `velocityMPerYear` may be `undefined`
/// or a length-3 metres-per-year array. Returns a plain object with
/// `positionM` and `velocityMPerYear` (`null` when no velocity was supplied).
#[wasm_bindgen(js_name = frameCatalogTransform)]
pub fn frame_catalog_transform(
    position_m: &[f64],
    velocity_m_per_year: Option<Vec<f64>>,
    from: TerrestrialFrame,
    to: TerrestrialFrame,
    epoch_year: f64,
) -> Result<JsValue, JsValue> {
    let state = core_transform(
        position(position_m, "positionM")?,
        optional_velocity(velocity_m_per_year, "velocityMPerYear")?,
        from.into(),
        to.into(),
        epoch_year,
    )
    .map_err(frame_catalog_error)?;
    to_js(&TerrestrialStateJs {
        position_m: state.position.as_array(),
        velocity_m_per_year: state.velocity.map(TerrestrialVelocityMPerYear::as_array),
    })
}

/// Propagate a station to a transform epoch, then transform it between frames.
///
/// `positionEpochYear` is the epoch of the input coordinates. The transform is
/// evaluated at `transformEpochYear`. Returns a plain object with `positionM`
/// and `velocityMPerYear`.
#[wasm_bindgen(js_name = frameCatalogTransformFromEpoch)]
pub fn frame_catalog_transform_from_epoch(
    position_m: &[f64],
    velocity_m_per_year: &[f64],
    position_epoch_year: f64,
    from: TerrestrialFrame,
    to: TerrestrialFrame,
    transform_epoch_year: f64,
) -> Result<JsValue, JsValue> {
    let state = core_transform_from_epoch(
        position(position_m, "positionM")?,
        velocity(velocity_m_per_year, "velocityMPerYear")?,
        position_epoch_year,
        from.into(),
        to.into(),
        transform_epoch_year,
    )
    .map_err(frame_catalog_error)?;
    to_js(&TerrestrialStateJs {
        position_m: state.position.as_array(),
        velocity_m_per_year: state.velocity.map(TerrestrialVelocityMPerYear::as_array),
    })
}
