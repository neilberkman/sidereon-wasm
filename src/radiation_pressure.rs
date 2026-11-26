//! Empirical solar radiation pressure and Earth radiation pressure.

use std::sync::Arc;

use sidereon_core::astro::forces;
use wasm_bindgen::prelude::*;

use crate::domain_error::propagation_error;
use crate::error::{index_arg, type_error};
use crate::events::{
    earth_shadow_model, shadow_function, shadow_function_label, EarthShadowModel, ShadowFunction,
};
use crate::force_models::{
    flatten_parameter_partials, force_methods, AccelerationPartials, ForceModel,
};
use crate::gnss_attitude::DybAxes;
use crate::marshal::vec3;
use crate::orbit_state::CartesianState;

type JsResult<T> = Result<T, JsValue>;

/// Acceleration coefficients for Ecom1 in metres per square second.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct Ecom1Coefficients(forces::Ecom1Coefficients);

#[wasm_bindgen]
impl Ecom1Coefficients {
    /// Compare the stored value.
    pub fn equals(&self, other: &Ecom1Coefficients) -> bool {
        self.0 == other.0
    }

    /// Construct coefficients, using zero for each omitted value.
    #[wasm_bindgen(constructor)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        d0: Option<f64>,
        dc: Option<f64>,
        ds: Option<f64>,
        y0: Option<f64>,
        yc: Option<f64>,
        ys: Option<f64>,
        b0: Option<f64>,
        bc: Option<f64>,
        bs: Option<f64>,
    ) -> Self {
        Self(forces::Ecom1Coefficients {
            d0: d0.unwrap_or_default(),
            dc: dc.unwrap_or_default(),
            ds: ds.unwrap_or_default(),
            y0: y0.unwrap_or_default(),
            yc: yc.unwrap_or_default(),
            ys: ys.unwrap_or_default(),
            b0: b0.unwrap_or_default(),
            bc: bc.unwrap_or_default(),
            bs: bs.unwrap_or_default(),
        })
    }

    /// Coefficient d0 in metres per square second.
    #[wasm_bindgen(getter)]
    pub fn d0(&self) -> f64 {
        self.0.d0
    }

    /// Coefficient dc in metres per square second.
    #[wasm_bindgen(getter)]
    pub fn dc(&self) -> f64 {
        self.0.dc
    }

    /// Coefficient ds in metres per square second.
    #[wasm_bindgen(getter)]
    pub fn ds(&self) -> f64 {
        self.0.ds
    }

    /// Coefficient y0 in metres per square second.
    #[wasm_bindgen(getter)]
    pub fn y0(&self) -> f64 {
        self.0.y0
    }

    /// Coefficient yc in metres per square second.
    #[wasm_bindgen(getter)]
    pub fn yc(&self) -> f64 {
        self.0.yc
    }

    /// Coefficient ys in metres per square second.
    #[wasm_bindgen(getter)]
    pub fn ys(&self) -> f64 {
        self.0.ys
    }

    /// Coefficient b0 in metres per square second.
    #[wasm_bindgen(getter)]
    pub fn b0(&self) -> f64 {
        self.0.b0
    }

    /// Coefficient bc in metres per square second.
    #[wasm_bindgen(getter)]
    pub fn bc(&self) -> f64 {
        self.0.bc
    }

    /// Coefficient bs in metres per square second.
    #[wasm_bindgen(getter)]
    pub fn bs(&self) -> f64 {
        self.0.bs
    }

    /// Read a coefficient sequence in model order.
    #[wasm_bindgen(js_name = fromArray)]
    pub fn from_array(values: &[f64]) -> JsResult<Self> {
        let values: [f64; forces::ECOM1_COEFFICIENT_COUNT] = values
            .try_into()
            .map_err(|_| type_error("values must match the Ecom1Coefficients coefficient count"))?;
        Ok(Self(forces::Ecom1Coefficients::from_array(values)))
    }

    /// Return the coefficients in model order.
    #[wasm_bindgen(js_name = toArray)]
    pub fn to_array(&self) -> Vec<f64> {
        self.0.to_array().to_vec()
    }

    /// Coefficient names in model order.
    #[wasm_bindgen(js_name = NAMES)]
    pub fn names() -> Vec<String> {
        forces::Ecom1Coefficients::NAMES.map(str::to_owned).to_vec()
    }
    /// Estimable parameter names in model order.
    #[wasm_bindgen(js_name = PARAMETER_NAMES)]
    pub fn parameter_names() -> Vec<String> {
        forces::Ecom1Coefficients::PARAMETER_NAMES
            .map(str::to_owned)
            .to_vec()
    }
}

/// Acceleration coefficients for Ecom2D2b1 in metres per square second.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct Ecom2D2b1Coefficients(forces::Ecom2D2b1Coefficients);

#[wasm_bindgen]
impl Ecom2D2b1Coefficients {
    /// Compare the stored value.
    pub fn equals(&self, other: &Ecom2D2b1Coefficients) -> bool {
        self.0 == other.0
    }

    /// Construct coefficients, using zero for each omitted value.
    #[wasm_bindgen(constructor)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        b0: Option<f64>,
        b1c: Option<f64>,
        b1s: Option<f64>,
        d0: Option<f64>,
        d2c: Option<f64>,
        d2s: Option<f64>,
        y0: Option<f64>,
    ) -> Self {
        Self(forces::Ecom2D2b1Coefficients {
            b0: b0.unwrap_or_default(),
            b1c: b1c.unwrap_or_default(),
            b1s: b1s.unwrap_or_default(),
            d0: d0.unwrap_or_default(),
            d2c: d2c.unwrap_or_default(),
            d2s: d2s.unwrap_or_default(),
            y0: y0.unwrap_or_default(),
        })
    }

    /// Coefficient b0 in metres per square second.
    #[wasm_bindgen(getter)]
    pub fn b0(&self) -> f64 {
        self.0.b0
    }

    /// Coefficient b1c in metres per square second.
    #[wasm_bindgen(getter)]
    pub fn b1c(&self) -> f64 {
        self.0.b1c
    }

    /// Coefficient b1s in metres per square second.
    #[wasm_bindgen(getter)]
    pub fn b1s(&self) -> f64 {
        self.0.b1s
    }

    /// Coefficient d0 in metres per square second.
    #[wasm_bindgen(getter)]
    pub fn d0(&self) -> f64 {
        self.0.d0
    }

    /// Coefficient d2c in metres per square second.
    #[wasm_bindgen(getter)]
    pub fn d2c(&self) -> f64 {
        self.0.d2c
    }

    /// Coefficient d2s in metres per square second.
    #[wasm_bindgen(getter)]
    pub fn d2s(&self) -> f64 {
        self.0.d2s
    }

    /// Coefficient y0 in metres per square second.
    #[wasm_bindgen(getter)]
    pub fn y0(&self) -> f64 {
        self.0.y0
    }

    /// Read a coefficient sequence in model order.
    #[wasm_bindgen(js_name = fromArray)]
    pub fn from_array(values: &[f64]) -> JsResult<Self> {
        let values: [f64; forces::ECOM2_D2B1_COEFFICIENT_COUNT] =
            values.try_into().map_err(|_| {
                type_error("values must match the Ecom2D2b1Coefficients coefficient count")
            })?;
        Ok(Self(forces::Ecom2D2b1Coefficients::from_array(values)))
    }

    /// Return the coefficients in model order.
    #[wasm_bindgen(js_name = toArray)]
    pub fn to_array(&self) -> Vec<f64> {
        self.0.to_array().to_vec()
    }

    /// Coefficient names in model order.
    #[wasm_bindgen(js_name = NAMES)]
    pub fn names() -> Vec<String> {
        forces::Ecom2D2b1Coefficients::NAMES
            .map(str::to_owned)
            .to_vec()
    }
    /// Estimable parameter names in model order.
    #[wasm_bindgen(js_name = PARAMETER_NAMES)]
    pub fn parameter_names() -> Vec<String> {
        forces::Ecom2D2b1Coefficients::PARAMETER_NAMES
            .map(str::to_owned)
            .to_vec()
    }
}

/// Acceleration coefficients for Ecom2D4b1 in metres per square second.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct Ecom2D4b1Coefficients(forces::Ecom2D4b1Coefficients);

#[wasm_bindgen]
impl Ecom2D4b1Coefficients {
    /// Compare the stored value.
    pub fn equals(&self, other: &Ecom2D4b1Coefficients) -> bool {
        self.0 == other.0
    }

    /// Construct coefficients, using zero for each omitted value.
    #[wasm_bindgen(constructor)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        b0: Option<f64>,
        b1c: Option<f64>,
        b1s: Option<f64>,
        d0: Option<f64>,
        d2c: Option<f64>,
        d4c: Option<f64>,
        d2s: Option<f64>,
        d4s: Option<f64>,
        y0: Option<f64>,
    ) -> Self {
        Self(forces::Ecom2D4b1Coefficients {
            b0: b0.unwrap_or_default(),
            b1c: b1c.unwrap_or_default(),
            b1s: b1s.unwrap_or_default(),
            d0: d0.unwrap_or_default(),
            d2c: d2c.unwrap_or_default(),
            d4c: d4c.unwrap_or_default(),
            d2s: d2s.unwrap_or_default(),
            d4s: d4s.unwrap_or_default(),
            y0: y0.unwrap_or_default(),
        })
    }

    /// Coefficient b0 in metres per square second.
    #[wasm_bindgen(getter)]
    pub fn b0(&self) -> f64 {
        self.0.b0
    }

    /// Coefficient b1c in metres per square second.
    #[wasm_bindgen(getter)]
    pub fn b1c(&self) -> f64 {
        self.0.b1c
    }

    /// Coefficient b1s in metres per square second.
    #[wasm_bindgen(getter)]
    pub fn b1s(&self) -> f64 {
        self.0.b1s
    }

    /// Coefficient d0 in metres per square second.
    #[wasm_bindgen(getter)]
    pub fn d0(&self) -> f64 {
        self.0.d0
    }

    /// Coefficient d2c in metres per square second.
    #[wasm_bindgen(getter)]
    pub fn d2c(&self) -> f64 {
        self.0.d2c
    }

    /// Coefficient d4c in metres per square second.
    #[wasm_bindgen(getter)]
    pub fn d4c(&self) -> f64 {
        self.0.d4c
    }

    /// Coefficient d2s in metres per square second.
    #[wasm_bindgen(getter)]
    pub fn d2s(&self) -> f64 {
        self.0.d2s
    }

    /// Coefficient d4s in metres per square second.
    #[wasm_bindgen(getter)]
    pub fn d4s(&self) -> f64 {
        self.0.d4s
    }

    /// Coefficient y0 in metres per square second.
    #[wasm_bindgen(getter)]
    pub fn y0(&self) -> f64 {
        self.0.y0
    }

    /// Read a coefficient sequence in model order.
    #[wasm_bindgen(js_name = fromArray)]
    pub fn from_array(values: &[f64]) -> JsResult<Self> {
        let values: [f64; forces::ECOM2_D4B1_COEFFICIENT_COUNT] =
            values.try_into().map_err(|_| {
                type_error("values must match the Ecom2D4b1Coefficients coefficient count")
            })?;
        Ok(Self(forces::Ecom2D4b1Coefficients::from_array(values)))
    }

    /// Return the coefficients in model order.
    #[wasm_bindgen(js_name = toArray)]
    pub fn to_array(&self) -> Vec<f64> {
        self.0.to_array().to_vec()
    }

    /// Coefficient names in model order.
    #[wasm_bindgen(js_name = NAMES)]
    pub fn names() -> Vec<String> {
        forces::Ecom2D4b1Coefficients::NAMES
            .map(str::to_owned)
            .to_vec()
    }
    /// Estimable parameter names in model order.
    #[wasm_bindgen(js_name = PARAMETER_NAMES)]
    pub fn parameter_names() -> Vec<String> {
        forces::Ecom2D4b1Coefficients::PARAMETER_NAMES
            .map(str::to_owned)
            .to_vec()
    }
}

/// Sun-fixed axes, angular arguments, and lighting for ECOM.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct EcomGeometry(forces::EcomGeometry);

#[wasm_bindgen]
impl EcomGeometry {
    /// Compare the stored value.
    pub fn equals(&self, other: &EcomGeometry) -> bool {
        self.0 == other.0
    }

    /// Sun-fixed D, Y and B axes.
    #[wasm_bindgen(getter)]
    pub fn dyb(&self) -> DybAxes {
        DybAxes(self.0.dyb)
    }

    /// Argument of latitude in radians, when defined.
    #[wasm_bindgen(getter, js_name = argumentOfLatitudeRad)]
    pub fn argument_of_latitude_rad(&self) -> Option<f64> {
        self.0.argument_of_latitude_rad
    }

    /// Sun-relative argument of latitude in radians, when defined.
    #[wasm_bindgen(getter, js_name = deltaURad)]
    pub fn delta_u_rad(&self) -> Option<f64> {
        self.0.delta_u_rad
    }

    /// Fraction of incident sunlight.
    #[wasm_bindgen(getter, js_name = lightingRatio)]
    pub fn lighting_ratio(&self) -> f64 {
        self.0.lighting_ratio
    }
}

/// Evaluate ECOM geometry from inertial satellite state and Sun position.
#[wasm_bindgen(js_name = ecomGeometry)]
pub fn ecom_geometry(
    position_km: &[f64],
    velocity_km_s: &[f64],
    sun_position_km: &[f64],
) -> JsResult<EcomGeometry> {
    forces::ecom_geometry(
        vec3("positionKm", position_km)?.into(),
        vec3("velocityKmS", velocity_km_s)?.into(),
        vec3("sunPositionKm", sun_position_km)?.into(),
    )
    .map(EcomGeometry)
    .map_err(propagation_error)
}

/// Evaluate ECOM geometry with an explicit Earth figure and optional Moon position.
#[wasm_bindgen(js_name = ecomGeometryWithShadow)]
pub fn ecom_geometry_with_shadow(
    position_km: &[f64],
    velocity_km_s: &[f64],
    sun_position_km: &[f64],
    moon_position_km: Option<Vec<f64>>,
    #[wasm_bindgen(unchecked_param_type = "EarthShadowModel")] earth_shadow_model_value: f64,
) -> JsResult<EcomGeometry> {
    forces::ecom_geometry_with_shadow(
        vec3("positionKm", position_km)?.into(),
        vec3("velocityKmS", velocity_km_s)?.into(),
        vec3("sunPositionKm", sun_position_km)?.into(),
        moon_position_km
            .as_deref()
            .map(|value| vec3("moonPositionKm", value).map(Into::into))
            .transpose()?,
        earth_shadow_model(earth_shadow_model_value)?,
    )
    .map(EcomGeometry)
    .map_err(propagation_error)
}

/// ECOM1 empirical solar radiation pressure.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct Ecom1SolarRadiationPressure(pub(crate) forces::Ecom1SolarRadiationPressure);

#[wasm_bindgen]
impl Ecom1SolarRadiationPressure {
    /// Compare the stored value.
    pub fn equals(&self, other: &Ecom1SolarRadiationPressure) -> bool {
        self.0 == other.0
    }

    /// Configured ECOM1 acceleration coefficients.
    pub fn coefficients(&self) -> Ecom1Coefficients {
        Ecom1Coefficients(self.0.coefficients())
    }

    /// Build ECOM1 from its acceleration coefficients.
    #[wasm_bindgen(constructor)]
    pub fn new(coefficients: &Ecom1Coefficients) -> JsResult<Self> {
        forces::Ecom1SolarRadiationPressure::new(coefficients.0)
            .map(Self)
            .map_err(propagation_error)
    }

    /// Select the eclipse lighting function.
    #[wasm_bindgen(js_name = withShadowFunction)]
    pub fn with_shadow_function(
        &self,
        #[wasm_bindgen(unchecked_param_type = "ShadowFunction")] function: f64,
    ) -> JsResult<Self> {
        Ok(Self(
            self.0.with_shadow_function(shadow_function(function)?),
        ))
    }

    /// Select the Earth figure for occultation.
    #[wasm_bindgen(js_name = withEarthShadowModel)]
    pub fn with_earth_shadow_model(
        &self,
        #[wasm_bindgen(unchecked_param_type = "EarthShadowModel")] model: f64,
    ) -> JsResult<Self> {
        Ok(Self(
            self.0.with_earth_shadow_model(earth_shadow_model(model)?),
        ))
    }

    /// Enable or disable lunar occultation.
    #[wasm_bindgen(js_name = withMoonEclipse)]
    pub fn with_moon_eclipse(&self, enabled: bool) -> Self {
        Self(self.0.with_moon_eclipse(enabled))
    }

    /// Configured eclipse lighting function.
    #[wasm_bindgen(js_name = shadowFunction)]
    pub fn shadow_function(&self) -> JsResult<ShadowFunction> {
        shadow_function_label(self.0.shadow_function())
    }

    /// Configured Earth figure for occultation.
    #[wasm_bindgen(js_name = earthShadowModel)]
    pub fn earth_shadow_model(&self) -> EarthShadowModel {
        self.0.earth_shadow_model().into()
    }

    /// Whether the model includes lunar occultation.
    #[wasm_bindgen(js_name = moonEclipseEnabled)]
    pub fn moon_eclipse_enabled(&self) -> bool {
        self.0.moon_eclipse_enabled()
    }

    /// Evaluate acceleration with explicit Sun position.
    #[wasm_bindgen(js_name = accelerationWithSun)]
    pub fn acceleration_with_sun(
        &self,
        position_km: &[f64],
        velocity_km_s: &[f64],
        sun_position_km: &[f64],
    ) -> JsResult<Vec<f64>> {
        self.0
            .acceleration_with_sun(
                vec3("positionKm", position_km)?.into(),
                vec3("velocityKmS", velocity_km_s)?.into(),
                vec3("sunPositionKm", sun_position_km)?.into(),
            )
            .map(|value| value.as_slice().to_vec())
            .map_err(propagation_error)
    }

    /// Evaluate acceleration with explicit Sun and Moon positions.
    #[wasm_bindgen(js_name = accelerationWithSunMoon)]
    pub fn acceleration_with_sun_moon(
        &self,
        position_km: &[f64],
        velocity_km_s: &[f64],
        sun_position_km: &[f64],
        moon_position_km: &[f64],
    ) -> JsResult<Vec<f64>> {
        self.0
            .acceleration_with_sun_moon(
                vec3("positionKm", position_km)?.into(),
                vec3("velocityKmS", velocity_km_s)?.into(),
                vec3("sunPositionKm", sun_position_km)?.into(),
                vec3("moonPositionKm", moon_position_km)?.into(),
            )
            .map(|value| value.as_slice().to_vec())
            .map_err(propagation_error)
    }

    /// Evaluate acceleration partials with explicit Sun position.
    #[wasm_bindgen(js_name = accelerationPartialsWithSun)]
    pub fn acceleration_partials_with_sun(
        &self,
        position_km: &[f64],
        velocity_km_s: &[f64],
        sun_position_km: &[f64],
    ) -> JsResult<AccelerationPartials> {
        self.0
            .acceleration_partials_with_sun(
                vec3("positionKm", position_km)?.into(),
                vec3("velocityKmS", velocity_km_s)?.into(),
                vec3("sunPositionKm", sun_position_km)?.into(),
            )
            .map(AccelerationPartials)
            .map_err(propagation_error)
    }

    /// Evaluate acceleration partials with explicit Sun and Moon positions.
    #[wasm_bindgen(js_name = accelerationPartialsWithSunMoon)]
    pub fn acceleration_partials_with_sun_moon(
        &self,
        position_km: &[f64],
        velocity_km_s: &[f64],
        sun_position_km: &[f64],
        moon_position_km: &[f64],
    ) -> JsResult<AccelerationPartials> {
        self.0
            .acceleration_partials_with_sun_moon(
                vec3("positionKm", position_km)?.into(),
                vec3("velocityKmS", velocity_km_s)?.into(),
                vec3("sunPositionKm", sun_position_km)?.into(),
                vec3("moonPositionKm", moon_position_km)?.into(),
            )
            .map(AccelerationPartials)
            .map_err(propagation_error)
    }

    /// Evaluate parameter partials with explicit Sun position.
    #[wasm_bindgen(js_name = parameterPartialsWithSun)]
    pub fn parameter_partials_with_sun(
        &self,
        position_km: &[f64],
        velocity_km_s: &[f64],
        sun_position_km: &[f64],
    ) -> JsResult<Vec<f64>> {
        self.0
            .parameter_partials_with_sun(
                vec3("positionKm", position_km)?.into(),
                vec3("velocityKmS", velocity_km_s)?.into(),
                vec3("sunPositionKm", sun_position_km)?.into(),
            )
            .map(flatten_parameter_partials)
            .map_err(propagation_error)
    }

    /// Evaluate parameter partials with explicit Sun and Moon positions.
    #[wasm_bindgen(js_name = parameterPartialsWithSunMoon)]
    pub fn parameter_partials_with_sun_moon(
        &self,
        position_km: &[f64],
        velocity_km_s: &[f64],
        sun_position_km: &[f64],
        moon_position_km: &[f64],
    ) -> JsResult<Vec<f64>> {
        self.0
            .parameter_partials_with_sun_moon(
                vec3("positionKm", position_km)?.into(),
                vec3("velocityKmS", velocity_km_s)?.into(),
                vec3("sunPositionKm", sun_position_km)?.into(),
                vec3("moonPositionKm", moon_position_km)?.into(),
            )
            .map(flatten_parameter_partials)
            .map_err(propagation_error)
    }
}

/// ECOM2 empirical solar radiation pressure.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct Ecom2SolarRadiationPressure(pub(crate) forces::Ecom2SolarRadiationPressure);

#[wasm_bindgen]
impl Ecom2SolarRadiationPressure {
    /// Compare the stored value.
    pub fn equals(&self, other: &Ecom2SolarRadiationPressure) -> bool {
        self.0 == other.0
    }

    /// Coefficient layout for the selected D and B harmonic orders.
    #[wasm_bindgen(js_name = coefficientCount)]
    pub fn coefficient_count(n_d: f64, n_b: f64) -> JsResult<Option<usize>> {
        Ok(forces::Ecom2SolarRadiationPressure::coefficient_count(
            index_arg(n_d, "nD")?,
            index_arg(n_b, "nB")?,
        ))
    }

    /// Coefficient layout for the selected D and B harmonic orders.
    #[wasm_bindgen(js_name = parameterNames)]
    pub fn parameter_names(n_d: f64, n_b: f64) -> JsResult<Vec<String>> {
        forces::Ecom2SolarRadiationPressure::parameter_names(
            index_arg(n_d, "nD")?,
            index_arg(n_b, "nB")?,
        )
        .map_err(propagation_error)
    }

    /// Build the ECOM2 D2B1 coefficient layout.
    pub fn d2b1(coefficients: &Ecom2D2b1Coefficients) -> JsResult<Self> {
        forces::Ecom2SolarRadiationPressure::d2b1(coefficients.0)
            .map(Self)
            .map_err(propagation_error)
    }

    /// Build the ECOM2 D4B1 coefficient layout.
    pub fn d4b1(coefficients: &Ecom2D4b1Coefficients) -> JsResult<Self> {
        forces::Ecom2SolarRadiationPressure::d4b1(coefficients.0)
            .map(Self)
            .map_err(propagation_error)
    }

    /// The configured D harmonic order.
    #[wasm_bindgen(js_name = nD)]
    pub fn n_d(&self) -> usize {
        self.0.n_d()
    }

    /// The configured B harmonic order.
    #[wasm_bindgen(js_name = nB)]
    pub fn n_b(&self) -> usize {
        self.0.n_b()
    }

    /// Configured ECOM2 acceleration coefficients in model order.
    pub fn coefficients(&self) -> Vec<f64> {
        self.0.coefficients().to_vec()
    }

    /// Build ECOM2 from its acceleration coefficients.
    #[wasm_bindgen(constructor)]
    pub fn new(n_d: f64, n_b: f64, coefficients: Vec<f64>) -> JsResult<Self> {
        forces::Ecom2SolarRadiationPressure::new(
            index_arg(n_d, "nD")?,
            index_arg(n_b, "nB")?,
            coefficients,
        )
        .map(Self)
        .map_err(propagation_error)
    }

    /// Select the eclipse lighting function.
    #[wasm_bindgen(js_name = withShadowFunction)]
    pub fn with_shadow_function(
        &self,
        #[wasm_bindgen(unchecked_param_type = "ShadowFunction")] function: f64,
    ) -> JsResult<Self> {
        Ok(Self(
            self.0
                .clone()
                .with_shadow_function(shadow_function(function)?),
        ))
    }

    /// Select the Earth figure for occultation.
    #[wasm_bindgen(js_name = withEarthShadowModel)]
    pub fn with_earth_shadow_model(
        &self,
        #[wasm_bindgen(unchecked_param_type = "EarthShadowModel")] model: f64,
    ) -> JsResult<Self> {
        Ok(Self(
            self.0
                .clone()
                .with_earth_shadow_model(earth_shadow_model(model)?),
        ))
    }

    /// Enable or disable lunar occultation.
    #[wasm_bindgen(js_name = withMoonEclipse)]
    pub fn with_moon_eclipse(&self, enabled: bool) -> Self {
        Self(self.0.clone().with_moon_eclipse(enabled))
    }

    /// Configured eclipse lighting function.
    #[wasm_bindgen(js_name = shadowFunction)]
    pub fn shadow_function(&self) -> JsResult<ShadowFunction> {
        shadow_function_label(self.0.shadow_function())
    }

    /// Configured Earth figure for occultation.
    #[wasm_bindgen(js_name = earthShadowModel)]
    pub fn earth_shadow_model(&self) -> EarthShadowModel {
        self.0.earth_shadow_model().into()
    }

    /// Whether the model includes lunar occultation.
    #[wasm_bindgen(js_name = moonEclipseEnabled)]
    pub fn moon_eclipse_enabled(&self) -> bool {
        self.0.moon_eclipse_enabled()
    }

    /// Evaluate acceleration with explicit Sun position.
    #[wasm_bindgen(js_name = accelerationWithSun)]
    pub fn acceleration_with_sun(
        &self,
        position_km: &[f64],
        velocity_km_s: &[f64],
        sun_position_km: &[f64],
    ) -> JsResult<Vec<f64>> {
        self.0
            .acceleration_with_sun(
                vec3("positionKm", position_km)?.into(),
                vec3("velocityKmS", velocity_km_s)?.into(),
                vec3("sunPositionKm", sun_position_km)?.into(),
            )
            .map(|value| value.as_slice().to_vec())
            .map_err(propagation_error)
    }

    /// Evaluate acceleration with explicit Sun and Moon positions.
    #[wasm_bindgen(js_name = accelerationWithSunMoon)]
    pub fn acceleration_with_sun_moon(
        &self,
        position_km: &[f64],
        velocity_km_s: &[f64],
        sun_position_km: &[f64],
        moon_position_km: &[f64],
    ) -> JsResult<Vec<f64>> {
        self.0
            .acceleration_with_sun_moon(
                vec3("positionKm", position_km)?.into(),
                vec3("velocityKmS", velocity_km_s)?.into(),
                vec3("sunPositionKm", sun_position_km)?.into(),
                vec3("moonPositionKm", moon_position_km)?.into(),
            )
            .map(|value| value.as_slice().to_vec())
            .map_err(propagation_error)
    }

    /// Evaluate acceleration partials with explicit Sun position.
    #[wasm_bindgen(js_name = accelerationPartialsWithSun)]
    pub fn acceleration_partials_with_sun(
        &self,
        position_km: &[f64],
        velocity_km_s: &[f64],
        sun_position_km: &[f64],
    ) -> JsResult<AccelerationPartials> {
        self.0
            .acceleration_partials_with_sun(
                vec3("positionKm", position_km)?.into(),
                vec3("velocityKmS", velocity_km_s)?.into(),
                vec3("sunPositionKm", sun_position_km)?.into(),
            )
            .map(AccelerationPartials)
            .map_err(propagation_error)
    }

    /// Evaluate acceleration partials with explicit Sun and Moon positions.
    #[wasm_bindgen(js_name = accelerationPartialsWithSunMoon)]
    pub fn acceleration_partials_with_sun_moon(
        &self,
        position_km: &[f64],
        velocity_km_s: &[f64],
        sun_position_km: &[f64],
        moon_position_km: &[f64],
    ) -> JsResult<AccelerationPartials> {
        self.0
            .acceleration_partials_with_sun_moon(
                vec3("positionKm", position_km)?.into(),
                vec3("velocityKmS", velocity_km_s)?.into(),
                vec3("sunPositionKm", sun_position_km)?.into(),
                vec3("moonPositionKm", moon_position_km)?.into(),
            )
            .map(AccelerationPartials)
            .map_err(propagation_error)
    }

    /// Evaluate parameter partials with explicit Sun position.
    #[wasm_bindgen(js_name = parameterPartialsWithSun)]
    pub fn parameter_partials_with_sun(
        &self,
        position_km: &[f64],
        velocity_km_s: &[f64],
        sun_position_km: &[f64],
    ) -> JsResult<Vec<f64>> {
        self.0
            .parameter_partials_with_sun(
                vec3("positionKm", position_km)?.into(),
                vec3("velocityKmS", velocity_km_s)?.into(),
                vec3("sunPositionKm", sun_position_km)?.into(),
            )
            .map(flatten_parameter_partials)
            .map_err(propagation_error)
    }

    /// Evaluate parameter partials with explicit Sun and Moon positions.
    #[wasm_bindgen(js_name = parameterPartialsWithSunMoon)]
    pub fn parameter_partials_with_sun_moon(
        &self,
        position_km: &[f64],
        velocity_km_s: &[f64],
        sun_position_km: &[f64],
        moon_position_km: &[f64],
    ) -> JsResult<Vec<f64>> {
        self.0
            .parameter_partials_with_sun_moon(
                vec3("positionKm", position_km)?.into(),
                vec3("velocityKmS", velocity_km_s)?.into(),
                vec3("sunPositionKm", sun_position_km)?.into(),
                vec3("moonPositionKm", moon_position_km)?.into(),
            )
            .map(flatten_parameter_partials)
            .map_err(propagation_error)
    }
}

/// Cannonball radiation pressure due to Earth albedo and thermal emission.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct EarthRadiationPressure(pub(crate) forces::EarthRadiationPressure);

#[wasm_bindgen]
impl EarthRadiationPressure {
    /// Compare the stored value.
    pub fn equals(&self, other: &EarthRadiationPressure) -> bool {
        self.0 == other.0
    }

    /// Build Earth radiation pressure from reflectivity and area-to-mass ratio.
    #[wasm_bindgen(constructor)]
    pub fn new(cr: f64, area_to_mass_m2_kg: f64) -> JsResult<Self> {
        forces::EarthRadiationPressure::new(cr, area_to_mass_m2_kg)
            .map(Self)
            .map_err(propagation_error)
    }

    /// Reflectivity coefficient.
    #[wasm_bindgen(getter)]
    pub fn cr(&self) -> f64 {
        self.0.cr()
    }

    /// Area-to-mass ratio in square metres per kilogram.
    #[wasm_bindgen(getter, js_name = areaToMassM2Kg)]
    pub fn area_to_mass_m2_kg(&self) -> f64 {
        self.0.area_to_mass_m2_kg()
    }

    /// Solar radiation pressure at one astronomical unit, in newtons per square metre.
    #[wasm_bindgen(getter, js_name = pressureNM2)]
    pub fn pressure_n_m2(&self) -> f64 {
        self.0.pressure_n_m2()
    }

    /// Earth radius in kilometres.
    #[wasm_bindgen(getter, js_name = earthRadiusKm)]
    pub fn earth_radius_km(&self) -> f64 {
        self.0.earth_radius_km()
    }

    /// Astronomical unit in kilometres.
    #[wasm_bindgen(getter, js_name = auKm)]
    pub fn au_km(&self) -> f64 {
        self.0.au_km()
    }

    /// Enable or disable lunar occultation.
    #[wasm_bindgen(js_name = withMoonEclipse)]
    pub fn with_moon_eclipse(&self, enabled: bool) -> Self {
        Self(self.0.with_moon_eclipse(enabled))
    }

    /// Whether the model includes lunar occultation.
    #[wasm_bindgen(js_name = moonEclipseEnabled)]
    pub fn moon_eclipse_enabled(&self) -> bool {
        self.0.moon_eclipse_enabled()
    }

    /// Evaluate Earth radiation pressure with explicit Sun and Moon positions.
    #[wasm_bindgen(js_name = accelerationWithSunMoon)]
    pub fn acceleration_with_sun_moon(
        &self,
        state: &CartesianState,
        sun_pos_km: &[f64],
        moon_pos_km: &[f64],
    ) -> JsResult<Vec<f64>> {
        self.0
            .acceleration_with_sun_moon(
                &state.0,
                vec3("sunPositionKm", sun_pos_km)?.into(),
                vec3("moonPositionKm", moon_pos_km)?.into(),
            )
            .map(|value| value.as_slice().to_vec())
            .map_err(propagation_error)
    }
}

force_methods!(Ecom1SolarRadiationPressure, 0, acceleration, acceleration);

#[wasm_bindgen]
impl Ecom1SolarRadiationPressure {
    /// Return a shared handle usable for propagation and estimation.
    #[wasm_bindgen(js_name = asForceModel)]
    pub fn as_force_model(&self) -> ForceModel {
        ForceModel(Arc::new(self.0))
    }
}

force_methods!(Ecom2SolarRadiationPressure, 0, acceleration, acceleration);

#[wasm_bindgen]
impl Ecom2SolarRadiationPressure {
    /// Return a shared handle usable for propagation and estimation.
    #[wasm_bindgen(js_name = asForceModel)]
    pub fn as_force_model(&self) -> ForceModel {
        ForceModel(Arc::new(self.0.clone()))
    }
}

force_methods!(EarthRadiationPressure, 0, acceleration, acceleration);

#[wasm_bindgen]
impl EarthRadiationPressure {
    /// Return a shared handle usable for propagation and estimation.
    #[wasm_bindgen(js_name = asForceModel)]
    pub fn as_force_model(&self) -> ForceModel {
        ForceModel(Arc::new(self.0))
    }
}

/// Number of coefficients in the ECOM1 layout.
#[wasm_bindgen(js_name = ECOM1_COEFFICIENT_COUNT)]
pub fn ecom1_coefficient_count() -> usize {
    forces::ECOM1_COEFFICIENT_COUNT
}

/// Number of coefficients in the ECOM2 D2B1 layout.
#[wasm_bindgen(js_name = ECOM2_D2B1_COEFFICIENT_COUNT)]
pub fn ecom2_d2b1_coefficient_count() -> usize {
    forces::ECOM2_D2B1_COEFFICIENT_COUNT
}

/// Number of coefficients in the ECOM2 D4B1 layout.
#[wasm_bindgen(js_name = ECOM2_D4B1_COEFFICIENT_COUNT)]
pub fn ecom2_d4b1_coefficient_count() -> usize {
    forces::ECOM2_D4B1_COEFFICIENT_COUNT
}

/// Cannonball solar radiation pressure.
#[wasm_bindgen]
pub struct SolarRadiationPressure(forces::SolarRadiationPressure);

#[wasm_bindgen]
impl SolarRadiationPressure {
    /// Name of the estimable reflectivity coefficient.
    #[wasm_bindgen(js_name = CR_PARAMETER)]
    pub fn cr_parameter() -> String {
        forces::SolarRadiationPressure::CR_PARAMETER.to_owned()
    }

    /// Construct with apparent-disc occultation, WGS84 Earth and lunar occultation enabled.
    #[wasm_bindgen(constructor)]
    pub fn new(cr: f64, area_to_mass_m2_kg: f64) -> JsResult<Self> {
        forces::SolarRadiationPressure::new(cr, area_to_mass_m2_kg)
            .map(Self)
            .map_err(propagation_error)
    }

    /// Construct with a reference pressure and distance.
    #[wasm_bindgen(js_name = withPressure)]
    pub fn with_pressure(
        cr: f64,
        area_to_mass_m2_kg: f64,
        pressure_n_m2: f64,
        au_km: f64,
    ) -> JsResult<Self> {
        forces::SolarRadiationPressure::with_pressure(cr, area_to_mass_m2_kg, pressure_n_m2, au_km)
            .map(Self)
            .map_err(propagation_error)
    }

    /// Reflectivity coefficient.
    #[wasm_bindgen(getter)]
    pub fn cr(&self) -> f64 {
        self.0.cr()
    }

    /// Area-to-mass ratio in square metres per kilogram.
    #[wasm_bindgen(getter, js_name = areaToMassM2Kg)]
    pub fn area_to_mass_m2_kg(&self) -> f64 {
        self.0.area_to_mass_m2_kg()
    }

    /// Reference solar pressure in newtons per square metre.
    #[wasm_bindgen(getter, js_name = pressureNM2)]
    pub fn pressure_n_m2(&self) -> f64 {
        self.0.pressure_n_m2()
    }

    /// Reference distance in kilometres.
    #[wasm_bindgen(getter, js_name = auKm)]
    pub fn au_km(&self) -> f64 {
        self.0.au_km()
    }

    /// Return a model with the selected eclipse lighting function.
    #[wasm_bindgen(js_name = withShadowFunction)]
    pub fn with_shadow_function(
        &self,
        #[wasm_bindgen(unchecked_param_type = "ShadowFunction")] function: f64,
    ) -> JsResult<Self> {
        Ok(Self(
            self.0.with_shadow_function(shadow_function(function)?),
        ))
    }

    /// Return a model with the selected Earth figure for occultation.
    #[wasm_bindgen(js_name = withEarthShadowModel)]
    pub fn with_earth_shadow_model(
        &self,
        #[wasm_bindgen(unchecked_param_type = "EarthShadowModel")] model: f64,
    ) -> JsResult<Self> {
        Ok(Self(
            self.0.with_earth_shadow_model(earth_shadow_model(model)?),
        ))
    }

    /// Return a model with lunar occultation enabled or disabled.
    #[wasm_bindgen(js_name = withMoonEclipse)]
    pub fn with_moon_eclipse(&self, enabled: bool) -> Self {
        Self(self.0.with_moon_eclipse(enabled))
    }

    /// Configured eclipse lighting function.
    #[wasm_bindgen(js_name = shadowFunction)]
    pub fn shadow_function(&self) -> JsResult<ShadowFunction> {
        shadow_function_label(self.0.shadow_function())
    }

    /// Configured Earth figure for occultation.
    #[wasm_bindgen(js_name = earthShadowModel)]
    pub fn earth_shadow_model(&self) -> EarthShadowModel {
        self.0.earth_shadow_model().into()
    }

    /// Whether the configured lighting function includes lunar occultation.
    #[wasm_bindgen(js_name = moonEclipseEnabled)]
    pub fn moon_eclipse_enabled(&self) -> bool {
        self.0.moon_eclipse_enabled()
    }

    /// Evaluate acceleration using inertial satellite, Sun and Moon positions in kilometres.
    #[wasm_bindgen(js_name = accelerationWithSunMoon)]
    pub fn acceleration_with_sun_moon(
        &self,
        position_km: &[f64],
        sun_position_km: &[f64],
        moon_position_km: &[f64],
    ) -> JsResult<Vec<f64>> {
        self.0
            .acceleration_with_sun_moon(
                vec3("positionKm", position_km)?.into(),
                vec3("sunPositionKm", sun_position_km)?.into(),
                vec3("moonPositionKm", moon_position_km)?.into(),
            )
            .map(|value| value.as_slice().to_vec())
            .map_err(propagation_error)
    }

    /// Compare the full force configuration by value.
    pub fn equals(&self, other: &SolarRadiationPressure) -> bool {
        self.0 == other.0
    }

    /// Return a shared handle usable for propagation and estimation.
    #[wasm_bindgen(js_name = asForceModel)]
    pub fn as_force_model(&self) -> ForceModel {
        ForceModel(Arc::new(self.0))
    }
}

force_methods!(SolarRadiationPressure, 0, acceleration, acceleration);
