//! IERS 2010 orientation and propagation context.

use std::sync::Arc;

use sidereon_core::astro::frames::{
    EarthOrientation as CoreEarthOrientation, EarthOrientationProvider,
    Iers2010EarthOrientationProvider as CoreProvider, Iers2010EopSample as CoreEopSample,
};
use sidereon_core::astro::propagator::PropagationContext as CoreContext;
use wasm_bindgen::prelude::*;

use crate::de::DeEphemeris;
use crate::domain_error::frame_error;
use crate::error::{range_error, type_error, ut1_validity};
use crate::marshal::mat3_flat;
use crate::spp::degrade_reason_label;

fn parse_mjd(mjd: f64) -> Result<i32, JsValue> {
    if mjd.fract() == 0.0 && (f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&mjd) {
        Ok(mjd as i32)
    } else {
        Err(range_error("mjdUtc must be a signed 32-bit integer"))
    }
}

/// Daily Earth orientation sample for IERS 2010.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct Iers2010EopSample(CoreEopSample);

#[wasm_bindgen]
impl Iers2010EopSample {
    /// Compare the stored value.
    pub fn equals(&self, other: &Iers2010EopSample) -> bool {
        self.0 == other.0
    }

    /// Specify daily Earth orientation parameters and optional pole rates.
    #[allow(clippy::too_many_arguments)]
    #[wasm_bindgen(constructor)]
    pub fn new(
        mjd_utc: f64,
        xp_arcsec: f64,
        yp_arcsec: f64,
        ut1_minus_utc_s: f64,
        lod_s: f64,
        dx_arcsec: f64,
        dy_arcsec: f64,
        pole_rates_rad_s: Option<Vec<f64>>,
    ) -> Result<Self, JsValue> {
        let pole_rates_rad_s = pole_rates_rad_s
            .map(|rates| {
                rates
                    .try_into()
                    .map_err(|_| type_error("poleRatesRadS must have length 2"))
            })
            .transpose()?;
        Ok(Self(CoreEopSample {
            mjd_utc: parse_mjd(mjd_utc)?,
            xp_arcsec,
            yp_arcsec,
            ut1_minus_utc_s,
            lod_s,
            dx_arcsec,
            dy_arcsec,
            pole_rates_rad_s,
        }))
    }

    /// UTC Modified Julian Date at midnight.
    #[wasm_bindgen(getter, js_name = mjdUtc)]
    pub fn mjd_utc(&self) -> i32 {
        self.0.mjd_utc
    }

    /// Polar motion X in arcseconds.
    #[wasm_bindgen(getter, js_name = xpArcsec)]
    pub fn xp_arcsec(&self) -> f64 {
        self.0.xp_arcsec
    }

    /// Polar motion Y in arcseconds.
    #[wasm_bindgen(getter, js_name = ypArcsec)]
    pub fn yp_arcsec(&self) -> f64 {
        self.0.yp_arcsec
    }

    /// UT1 minus UTC in seconds.
    #[wasm_bindgen(getter, js_name = ut1MinusUtcS)]
    pub fn ut1_minus_utc_s(&self) -> f64 {
        self.0.ut1_minus_utc_s
    }

    /// Excess length of day in seconds.
    #[wasm_bindgen(getter, js_name = lodS)]
    pub fn lod_s(&self) -> f64 {
        self.0.lod_s
    }

    /// Celestial intermediate pole X correction in arcseconds.
    #[wasm_bindgen(getter, js_name = dxArcsec)]
    pub fn dx_arcsec(&self) -> f64 {
        self.0.dx_arcsec
    }

    /// Celestial intermediate pole Y correction in arcseconds.
    #[wasm_bindgen(getter, js_name = dyArcsec)]
    pub fn dy_arcsec(&self) -> f64 {
        self.0.dy_arcsec
    }

    /// Optional x and y pole rates in radians per second.
    #[wasm_bindgen(getter, js_name = poleRatesRadS)]
    pub fn pole_rates_rad_s(&self) -> Option<Vec<f64>> {
        self.0.pole_rates_rad_s.map(|rates| rates.to_vec())
    }
}

/// Orientation and rotation rates between GCRF and ITRF.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct EarthOrientation(CoreEarthOrientation);

#[wasm_bindgen]
impl EarthOrientation {
    /// Compare the stored value.
    pub fn equals(&self, other: &EarthOrientation) -> bool {
        self.0 == other.0
    }

    /// IERS 2010 rotation quaternion, scalar component first.
    #[wasm_bindgen(js_name = iers2010Quaternion)]
    pub fn iers2010_quaternion(&self) -> Option<Vec<f64>> {
        self.0
            .iers2010_quaternion()
            .map(|quaternion| quaternion.to_vec())
    }

    /// Rate of Earth rotation in ITRF, radians per second.
    #[wasm_bindgen(js_name = earthRotationVectorItrfRadS)]
    pub fn earth_rotation_vector_itrf_rad_s(&self) -> Vec<f64> {
        self.0.earth_rotation_vector_itrf_rad_s().to_vec()
    }

    /// First UT1 table departure, if this orientation contains one.
    #[wasm_bindgen(js_name = ut1Degraded, unchecked_return_type = "Ut1DegradeReason | undefined")]
    pub fn ut1_degraded(&self) -> Option<String> {
        self.0
            .ut1_degraded()
            .map(|reason| degrade_reason_label(reason).to_owned())
    }

    /// Rotation from GCRF into ITRF.
    #[wasm_bindgen(js_name = gcrfToItrfMatrix)]
    pub fn gcrf_to_itrf_matrix(&self) -> Vec<f64> {
        mat3_flat(&self.0.gcrf_to_itrf_matrix())
    }

    /// Rotation from ITRF into GCRF.
    #[wasm_bindgen(js_name = itrfToGcrfMatrix)]
    pub fn itrf_to_gcrf_matrix(&self) -> Vec<f64> {
        mat3_flat(&self.0.itrf_to_gcrf_matrix())
    }

    /// Time derivative of the GCRF to ITRF rotation, per second.
    #[wasm_bindgen(js_name = gcrfToItrfRotationRateMatrix)]
    pub fn gcrf_to_itrf_rotation_rate_matrix(&self) -> Vec<f64> {
        mat3_flat(&self.0.gcrf_to_itrf_rotation_rate_matrix())
    }

    /// Time derivative of the ITRF to GCRF rotation, per second.
    #[wasm_bindgen(js_name = itrfToGcrfRotationRateMatrix)]
    pub fn itrf_to_gcrf_rotation_rate_matrix(&self) -> Vec<f64> {
        mat3_flat(&self.0.itrf_to_gcrf_rotation_rate_matrix())
    }
}

/// CIO-based IERS 2010 provider from daily Earth orientation samples.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct Iers2010EarthOrientationProvider(pub(crate) Arc<CoreProvider>);

#[wasm_bindgen]
impl Iers2010EarthOrientationProvider {
    #[wasm_bindgen(js_name = __cloneHandle, skip_typescript)]
    pub fn clone_handle(&self) -> Self {
        self.clone()
    }

    /// Compare the stored value.
    pub fn equals(&self, other: &Iers2010EarthOrientationProvider) -> bool {
        self.0 == other.0
    }

    /// Build a strict provider from samples in increasing UTC MJD order.
    /// The sample handles are consumed.
    #[wasm_bindgen(constructor)]
    pub fn new(samples: Vec<Iers2010EopSample>) -> Result<Self, JsValue> {
        CoreProvider::new(samples.into_iter().map(|sample| sample.0).collect())
            .map(Arc::new)
            .map(Self)
            .map_err(frame_error)
    }

    /// Select the policy for epochs outside available samples.
    #[wasm_bindgen(js_name = withValidity)]
    pub fn with_validity(
        &self,
        #[wasm_bindgen(unchecked_param_type = "Ut1Validity")] validity: String,
    ) -> Result<Self, JsValue> {
        Ok(Self(Arc::new(
            self.0
                .as_ref()
                .clone()
                .with_validity(ut1_validity(Some(validity))?),
        )))
    }

    /// Evaluate at UTC MJD and seconds within that UTC day.
    #[wasm_bindgen(js_name = orientationAtUtcMjd)]
    pub fn orientation_at_utc_mjd(
        &self,
        mjd_utc: f64,
        seconds_in_day: f64,
    ) -> Result<EarthOrientation, JsValue> {
        self.0
            .orientation_at_utc_mjd(parse_mjd(mjd_utc)?, seconds_in_day)
            .map(EarthOrientation)
            .map_err(frame_error)
    }

    /// Evaluate at TDB seconds from J2000.
    #[wasm_bindgen(js_name = orientationAtTdbSeconds)]
    pub fn orientation_at_tdb_seconds(
        &self,
        epoch_tdb_seconds: f64,
    ) -> Result<EarthOrientation, JsValue> {
        self.0
            .orientation_at_tdb_seconds(epoch_tdb_seconds)
            .map(EarthOrientation)
            .map_err(frame_error)
    }
}

/// Optional external ephemeris and terrestrial orientation for force evaluation.
#[wasm_bindgen]
#[derive(Clone, Default)]
pub struct PropagationContext {
    pub(crate) inner: CoreContext,
    de: Option<Arc<sidereon_core::astro::de::DeEphemeris>>,
}

#[wasm_bindgen]
impl PropagationContext {
    #[wasm_bindgen(js_name = __cloneHandle, skip_typescript)]
    pub fn clone_handle(&self) -> Self {
        self.clone()
    }

    /// Build an empty context using the core defaults.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Use a DE ephemeris for Sun and Moon positions during propagation.
    #[wasm_bindgen(js_name = withDeEphemeris)]
    pub fn with_de_ephemeris(&self, ephemeris: &DeEphemeris) -> Self {
        Self {
            inner: self.inner.clone().with_de_ephemeris(ephemeris.0.clone()),
            de: Some(ephemeris.0.clone()),
        }
    }

    /// Return the attached ephemeris if supplied.
    #[wasm_bindgen(js_name = deEphemeris)]
    pub fn de_ephemeris(&self) -> Option<DeEphemeris> {
        self.de.clone().map(DeEphemeris)
    }

    /// Use the IERS 2010 provider for the terrestrial frame during propagation.
    #[wasm_bindgen(js_name = withBodyFixedFrameProvider)]
    pub fn with_body_fixed_frame_provider(
        &self,
        provider: &Iers2010EarthOrientationProvider,
    ) -> Self {
        Self {
            inner: self
                .inner
                .clone()
                .with_body_fixed_frame_provider(provider.0.clone()),
            de: self.de.clone(),
        }
    }

    /// First recorded departure from UT1 coverage, if any.
    #[wasm_bindgen(js_name = ut1Departure, unchecked_return_type = "Ut1DegradeReason | undefined")]
    pub fn ut1_departure(&self) -> Option<String> {
        self.inner
            .ut1_departure()
            .map(|reason| degrade_reason_label(reason).to_owned())
    }
}
