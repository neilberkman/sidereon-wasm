//! Closed-form zonal orbit propagation and mean element conversions.

use serde_json::json;
use sidereon_core::astro::propagator::analytic::{self, AnalyticPropagationError as CoreError};
use wasm_bindgen::prelude::*;

use crate::error::typed_error;
use crate::marshal::vec3;
use crate::orbit_state::{CartesianState, CartesianVectors};

type JsResult<T> = Result<T, JsValue>;

/// Theory used for analytic orbit propagation and element conversion.
#[wasm_bindgen]
pub enum AnalyticTheory {
    /// Near-circular Eckstein-Hechler theory.
    EcksteinHechler,
    /// Elliptic-orbit Brouwer-Lyddane theory.
    BrouwerLyddane,
}

impl From<analytic::AnalyticTheory> for AnalyticTheory {
    fn from(value: analytic::AnalyticTheory) -> Self {
        match value {
            analytic::AnalyticTheory::EcksteinHechler => Self::EcksteinHechler,
            analytic::AnalyticTheory::BrouwerLyddane => Self::BrouwerLyddane,
        }
    }
}

/// The engine's display name for an analytic propagation theory.
#[wasm_bindgen(js_name = analyticTheoryToString)]
pub fn analytic_theory_to_string(
    #[wasm_bindgen(unchecked_param_type = "AnalyticTheory")] value: f64,
) -> JsResult<String> {
    let theory = match crate::error::index_arg(value, "theory")? {
        0 => analytic::AnalyticTheory::EcksteinHechler,
        1 => analytic::AnalyticTheory::BrouwerLyddane,
        _ => {
            return Err(crate::error::range_error(
                "theory must be an AnalyticTheory value",
            ))
        }
    };
    Ok(theory.to_string())
}

fn analytic_error(error: CoreError) -> JsValue {
    let detail = match &error {
        CoreError::NonFinite { field } => json!({"kind":"nonFinite", "field":field}),
        CoreError::InvalidParameter { field, reason } => {
            json!({"kind":"invalidParameter", "field":field, "reason":reason})
        }
        CoreError::NotElliptic { field } => json!({"kind":"notElliptic", "field":field}),
        CoreError::RetrogradeEquatorial { inclination_rad } => {
            json!({"kind":"retrogradeEquatorial", "inclinationRad":inclination_rad})
        }
        CoreError::InsideReferenceSphere {
            semi_major_axis_km,
            reference_radius_km,
        } => {
            json!({"kind":"insideReferenceSphere", "semiMajorAxisKm":semi_major_axis_km, "referenceRadiusKm":reference_radius_km})
        }
        CoreError::EccentricityTooLarge {
            eccentricity,
            limit,
        } => json!({"kind":"eccentricityTooLarge", "eccentricity":eccentricity, "limit":limit}),
        CoreError::NearlyEquatorial { inclination_rad } => {
            json!({"kind":"nearlyEquatorial", "inclinationRad":inclination_rad})
        }
        CoreError::NearCriticalInclination { inclination_rad } => {
            json!({"kind":"nearCriticalInclination", "inclinationRad":inclination_rad})
        }
        CoreError::MeanElementsNotConverged { theory, iterations } => {
            json!({"kind":"meanElementsNotConverged", "theory":AnalyticTheory::from(*theory) as u32, "iterations":iterations})
        }
        CoreError::KeplerNotConverged { field } => {
            json!({"kind":"keplerNotConverged", "field":field})
        }
        CoreError::NonFiniteResult { theory, field } => {
            json!({"kind":"nonFiniteResult", "theory":AnalyticTheory::from(*theory) as u32, "field":field})
        }
    };
    typed_error("AnalyticPropagationError", &error.to_string(), detail)
}
/// Circular mean or osculating orbital elements.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct CircularElements(analytic::CircularElements);

#[wasm_bindgen]
impl CircularElements {
    /// Compare the stored value.
    pub fn equals(&self, other: &CircularElements) -> bool {
        self.0 == other.0
    }

    /// Construct from the supplied values.
    #[wasm_bindgen(constructor)]
    pub fn new(
        a_km: f64,
        ex: f64,
        ey: f64,
        i_rad: f64,
        raan_rad: f64,
        mean_latitude_argument_rad: f64,
    ) -> Self {
        Self(analytic::CircularElements {
            a_km,
            ex,
            ey,
            i_rad,
            raan_rad,
            mean_latitude_argument_rad,
        })
    }

    /// Semi-major axis in kilometres.
    #[wasm_bindgen(getter, js_name = aKm)]
    pub fn a_km(&self) -> f64 {
        self.0.a_km
    }

    /// Eccentricity times cosine of argument of perigee.
    #[wasm_bindgen(getter)]
    pub fn ex(&self) -> f64 {
        self.0.ex
    }

    /// Eccentricity times sine of argument of perigee.
    #[wasm_bindgen(getter)]
    pub fn ey(&self) -> f64 {
        self.0.ey
    }

    /// Inclination in radians.
    #[wasm_bindgen(getter, js_name = iRad)]
    pub fn i_rad(&self) -> f64 {
        self.0.i_rad
    }

    /// Right ascension of the ascending node in radians.
    #[wasm_bindgen(getter, js_name = raanRad)]
    pub fn raan_rad(&self) -> f64 {
        self.0.raan_rad
    }

    /// Mean argument of latitude in radians.
    #[wasm_bindgen(getter, js_name = meanLatitudeArgumentRad)]
    pub fn mean_latitude_argument_rad(&self) -> f64 {
        self.0.mean_latitude_argument_rad
    }

    /// Convert a Cartesian position and velocity to orbital elements.
    #[wasm_bindgen(js_name = fromCartesian)]
    pub fn from_cartesian(
        position_km: &[f64],
        velocity_km_s: &[f64],
        mu_km3_s2: f64,
    ) -> JsResult<Self> {
        analytic::CircularElements::from_cartesian(
            vec3("positionKm", position_km)?,
            vec3("velocityKmS", velocity_km_s)?,
            mu_km3_s2,
        )
        .map(Self)
        .map_err(analytic_error)
    }

    /// Convert the orbital elements to position in km and velocity in km/s.
    #[wasm_bindgen(js_name = toCartesian)]
    pub fn to_cartesian(&self, mu_km3_s2: f64) -> JsResult<CartesianVectors> {
        self.0
            .to_cartesian(mu_km3_s2)
            .map(|(position, velocity)| CartesianVectors { position, velocity })
            .map_err(analytic_error)
    }

    /// Magnitude of the eccentricity vector.
    pub fn eccentricity(&self) -> f64 {
        self.0.eccentricity()
    }
}

/// Keplerian elements with mean anomaly for analytic propagation.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct KeplerianElements(analytic::KeplerianElements);

#[wasm_bindgen]
impl KeplerianElements {
    /// Compare the stored value.
    pub fn equals(&self, other: &KeplerianElements) -> bool {
        self.0 == other.0
    }

    /// Construct from the supplied values.
    #[wasm_bindgen(constructor)]
    pub fn new(
        a_km: f64,
        e: f64,
        i_rad: f64,
        arg_perigee_rad: f64,
        raan_rad: f64,
        mean_anomaly_rad: f64,
    ) -> Self {
        Self(analytic::KeplerianElements {
            a_km,
            e,
            i_rad,
            arg_perigee_rad,
            raan_rad,
            mean_anomaly_rad,
        })
    }

    /// Semi-major axis in kilometres.
    #[wasm_bindgen(getter, js_name = aKm)]
    pub fn a_km(&self) -> f64 {
        self.0.a_km
    }

    /// Eccentricity.
    #[wasm_bindgen(getter)]
    pub fn e(&self) -> f64 {
        self.0.e
    }

    /// Inclination in radians.
    #[wasm_bindgen(getter, js_name = iRad)]
    pub fn i_rad(&self) -> f64 {
        self.0.i_rad
    }

    /// Argument of perigee in radians.
    #[wasm_bindgen(getter, js_name = argPerigeeRad)]
    pub fn arg_perigee_rad(&self) -> f64 {
        self.0.arg_perigee_rad
    }

    /// Right ascension of the ascending node in radians.
    #[wasm_bindgen(getter, js_name = raanRad)]
    pub fn raan_rad(&self) -> f64 {
        self.0.raan_rad
    }

    /// Mean anomaly in radians.
    #[wasm_bindgen(getter, js_name = meanAnomalyRad)]
    pub fn mean_anomaly_rad(&self) -> f64 {
        self.0.mean_anomaly_rad
    }

    /// Convert a Cartesian position and velocity to orbital elements.
    #[wasm_bindgen(js_name = fromCartesian)]
    pub fn from_cartesian(
        position_km: &[f64],
        velocity_km_s: &[f64],
        mu_km3_s2: f64,
    ) -> JsResult<Self> {
        analytic::KeplerianElements::from_cartesian(
            vec3("positionKm", position_km)?,
            vec3("velocityKmS", velocity_km_s)?,
            mu_km3_s2,
        )
        .map(Self)
        .map_err(analytic_error)
    }

    /// Convert the orbital elements to position in km and velocity in km/s.
    #[wasm_bindgen(js_name = toCartesian)]
    pub fn to_cartesian(&self, mu_km3_s2: f64) -> JsResult<CartesianVectors> {
        self.0
            .to_cartesian(mu_km3_s2)
            .map(|(position, velocity)| CartesianVectors { position, velocity })
            .map_err(analytic_error)
    }
}

/// Unnormalized zonal coefficients and associated gravity constants.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct ZonalHarmonics(analytic::ZonalHarmonics);

#[wasm_bindgen]
impl ZonalHarmonics {
    /// Compare the stored value.
    pub fn equals(&self, other: &ZonalHarmonics) -> bool {
        self.0 == other.0
    }

    /// Construct from the supplied values.
    #[wasm_bindgen(constructor)]
    pub fn new(
        mu_km3_s2: f64,
        re_km: f64,
        c20: f64,
        c30: f64,
        c40: f64,
        c50: f64,
        c60: f64,
    ) -> Self {
        Self(analytic::ZonalHarmonics {
            mu_km3_s2,
            re_km,
            c20,
            c30,
            c40,
            c50,
            c60,
        })
    }

    /// Gravitational parameter in cubic kilometres per square second.
    #[wasm_bindgen(getter, js_name = muKm3S2)]
    pub fn mu_km3_s2(&self) -> f64 {
        self.0.mu_km3_s2
    }

    /// Reference radius in kilometres.
    #[wasm_bindgen(getter, js_name = reKm)]
    pub fn re_km(&self) -> f64 {
        self.0.re_km
    }

    /// Unnormalized C20 coefficient.
    #[wasm_bindgen(getter)]
    pub fn c20(&self) -> f64 {
        self.0.c20
    }

    /// Unnormalized C30 coefficient.
    #[wasm_bindgen(getter)]
    pub fn c30(&self) -> f64 {
        self.0.c30
    }

    /// Unnormalized C40 coefficient.
    #[wasm_bindgen(getter)]
    pub fn c40(&self) -> f64 {
        self.0.c40
    }

    /// Unnormalized C50 coefficient.
    #[wasm_bindgen(getter)]
    pub fn c50(&self) -> f64 {
        self.0.c50
    }

    /// Unnormalized C60 coefficient.
    #[wasm_bindgen(getter)]
    pub fn c60(&self) -> f64 {
        self.0.c60
    }

    /// Default zonal gravity constants.
    #[wasm_bindgen(js_name = default)]
    pub fn default_value() -> Self {
        Self(analytic::ZonalHarmonics::default())
    }

    /// Standard Earth zonal gravity constants.
    pub fn earth() -> Self {
        Self(analytic::ZonalHarmonics::earth())
    }
}

/// Convergence controls for conversion to mean orbital elements.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct MeanConversionOptions(analytic::MeanConversionOptions);

#[wasm_bindgen]
impl MeanConversionOptions {
    /// Compare the stored value.
    pub fn equals(&self, other: &MeanConversionOptions) -> bool {
        self.0 == other.0
    }

    /// Construct from the supplied values.
    #[wasm_bindgen(constructor)]
    pub fn new(tolerance: f64, max_iterations: f64) -> JsResult<Self> {
        Ok(Self(analytic::MeanConversionOptions::new(
            tolerance,
            crate::error::index_arg(max_iterations, "maxIterations")? as u32,
        )))
    }

    /// Relative convergence tolerance.
    #[wasm_bindgen(getter)]
    pub fn tolerance(&self) -> f64 {
        self.0.tolerance
    }

    /// Maximum iteration count.
    #[wasm_bindgen(getter, js_name = maxIterations)]
    pub fn max_iterations(&self) -> u32 {
        self.0.max_iterations
    }

    /// Default options for the eckstein-hechler theory.
    #[wasm_bindgen(js_name = ECKSTEIN_HECHLER_DEFAULT)]
    pub fn eckstein_hechler_default() -> Self {
        Self(analytic::MeanConversionOptions::ECKSTEIN_HECHLER_DEFAULT)
    }

    /// Default options for the brouwer-lyddane theory.
    #[wasm_bindgen(js_name = BROUWER_LYDDANE_DEFAULT)]
    pub fn brouwer_lyddane_default() -> Self {
        Self(analytic::MeanConversionOptions::BROUWER_LYDDANE_DEFAULT)
    }
}

/// Converged mean elements and the iteration count.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct MeanElements(MeanElementsValue);

#[wasm_bindgen]
impl MeanElements {
    /// Compare the stored value.
    pub fn equals(&self, other: &MeanElements) -> bool {
        self.0 == other.0
    }

    /// Mean orbital elements in the theory's parameterization.
    #[wasm_bindgen(getter, unchecked_return_type = "CircularElements | KeplerianElements")]
    pub fn elements(&self) -> JsValue {
        match &self.0 {
            MeanElementsValue::Circular(result) => CircularElements(result.elements).into(),
            MeanElementsValue::Keplerian(result) => KeplerianElements(result.elements).into(),
        }
    }

    /// Number of fixed-point iterations.
    #[wasm_bindgen(getter)]
    pub fn iterations(&self) -> u32 {
        match &self.0 {
            MeanElementsValue::Circular(result) => result.iterations,
            MeanElementsValue::Keplerian(result) => result.iterations,
        }
    }
}

#[derive(Clone, PartialEq)]
enum MeanElementsValue {
    Circular(analytic::MeanElements<analytic::CircularElements>),
    Keplerian(analytic::MeanElements<analytic::KeplerianElements>),
}

/// Eckstein-Hechler analytic orbit propagator.
#[wasm_bindgen]
pub struct EcksteinHechlerPropagator(pub(crate) analytic::EcksteinHechlerPropagator);

#[wasm_bindgen]
impl EcksteinHechlerPropagator {
    /// Build from an osculating Cartesian state and convert to mean elements.
    #[wasm_bindgen(constructor)]
    pub fn new(
        initial: &CartesianState,
        gravity: &ZonalHarmonics,
        options: &MeanConversionOptions,
    ) -> JsResult<Self> {
        analytic::EcksteinHechlerPropagator::new(&initial.0, &gravity.0, options.0)
            .map(Self)
            .map_err(analytic_error)
    }

    /// Build from mean orbital elements at a TDB epoch.
    #[wasm_bindgen(js_name = fromMeanElements)]
    pub fn from_mean_elements(
        epoch_tdb_seconds: f64,
        mean: &CircularElements,
        gravity: &ZonalHarmonics,
    ) -> JsResult<Self> {
        analytic::EcksteinHechlerPropagator::from_mean_elements(
            epoch_tdb_seconds,
            mean.0,
            &gravity.0,
        )
        .map(Self)
        .map_err(analytic_error)
    }

    /// Epoch of the mean orbital elements in TDB seconds from J2000.
    #[wasm_bindgen(js_name = epochTdbSeconds)]
    pub fn epoch_tdb_seconds(&self) -> f64 {
        self.0.epoch_tdb_seconds()
    }

    /// Mean orbital elements at the initial epoch.
    #[wasm_bindgen(js_name = meanElements)]
    pub fn mean_elements(&self) -> CircularElements {
        CircularElements(self.0.mean_elements())
    }

    /// Zonal gravity constants.
    pub fn gravity(&self) -> ZonalHarmonics {
        ZonalHarmonics(*self.0.gravity())
    }

    /// Evaluate osculating orbital elements at the given TDB epoch.
    #[wasm_bindgen(js_name = osculatingElements)]
    pub fn osculating_elements(&self, epoch_tdb_seconds: f64) -> JsResult<CircularElements> {
        self.0
            .osculating_elements(epoch_tdb_seconds)
            .map(CircularElements)
            .map_err(analytic_error)
    }

    /// Evaluate the Cartesian state at the given TDB epoch.
    pub fn propagate(&self, epoch_tdb_seconds: f64) -> JsResult<CartesianState> {
        self.0
            .propagate(epoch_tdb_seconds)
            .map(CartesianState)
            .map_err(analytic_error)
    }

    /// Evaluate Cartesian states at the given TDB epochs in input order.
    pub fn ephemeris(&self, epochs_tdb_seconds: &[f64]) -> JsResult<Vec<CartesianState>> {
        self.0
            .ephemeris(epochs_tdb_seconds)
            .map(|states| states.into_iter().map(CartesianState).collect())
            .map_err(analytic_error)
    }
}

/// Convert osculating Cartesian state to Eckstein-Hechler mean elements.
#[wasm_bindgen(js_name = ecksteinHechlerMeanElements)]
pub fn eckstein_hechler_mean_elements(
    osculating: &CartesianState,
    gravity: &ZonalHarmonics,
    options: &MeanConversionOptions,
) -> JsResult<MeanElements> {
    analytic::eckstein_hechler_mean_elements(&osculating.0, &gravity.0, options.0)
        .map(|result| MeanElements(MeanElementsValue::Circular(result)))
        .map_err(analytic_error)
}

/// Convert Eckstein-Hechler mean elements to osculating elements.
#[wasm_bindgen(js_name = ecksteinHechlerOsculatingElements)]
pub fn eckstein_hechler_osculating_elements(
    mean: &CircularElements,
    gravity: &ZonalHarmonics,
) -> JsResult<CircularElements> {
    analytic::eckstein_hechler_osculating_elements(&mean.0, &gravity.0)
        .map(CircularElements)
        .map_err(analytic_error)
}

/// Brouwer-Lyddane analytic orbit propagator.
#[wasm_bindgen]
pub struct BrouwerLyddanePropagator(pub(crate) analytic::BrouwerLyddanePropagator);

#[wasm_bindgen]
impl BrouwerLyddanePropagator {
    /// Build from an osculating Cartesian state and convert to mean elements.
    #[wasm_bindgen(constructor)]
    pub fn new(
        initial: &CartesianState,
        gravity: &ZonalHarmonics,
        m2_rad_s2: f64,
        options: &MeanConversionOptions,
    ) -> JsResult<Self> {
        analytic::BrouwerLyddanePropagator::new(&initial.0, &gravity.0, m2_rad_s2, options.0)
            .map(Self)
            .map_err(analytic_error)
    }

    /// Build from mean orbital elements at a TDB epoch.
    #[wasm_bindgen(js_name = fromMeanElements)]
    pub fn from_mean_elements(
        epoch_tdb_seconds: f64,
        mean: &KeplerianElements,
        gravity: &ZonalHarmonics,
        m2_rad_s2: f64,
    ) -> JsResult<Self> {
        analytic::BrouwerLyddanePropagator::from_mean_elements(
            epoch_tdb_seconds,
            mean.0,
            &gravity.0,
            m2_rad_s2,
        )
        .map(Self)
        .map_err(analytic_error)
    }

    /// Epoch of the mean orbital elements in TDB seconds from J2000.
    #[wasm_bindgen(js_name = epochTdbSeconds)]
    pub fn epoch_tdb_seconds(&self) -> f64 {
        self.0.epoch_tdb_seconds()
    }

    /// Mean orbital elements at the initial epoch.
    #[wasm_bindgen(js_name = meanElements)]
    pub fn mean_elements(&self) -> KeplerianElements {
        KeplerianElements(self.0.mean_elements())
    }

    /// Zonal gravity constants.
    pub fn gravity(&self) -> ZonalHarmonics {
        ZonalHarmonics(*self.0.gravity())
    }

    /// Evaluate osculating orbital elements at the given TDB epoch.
    #[wasm_bindgen(js_name = osculatingElements)]
    pub fn osculating_elements(&self, epoch_tdb_seconds: f64) -> JsResult<KeplerianElements> {
        self.0
            .osculating_elements(epoch_tdb_seconds)
            .map(KeplerianElements)
            .map_err(analytic_error)
    }

    /// Evaluate the Cartesian state at the given TDB epoch.
    pub fn propagate(&self, epoch_tdb_seconds: f64) -> JsResult<CartesianState> {
        self.0
            .propagate(epoch_tdb_seconds)
            .map(CartesianState)
            .map_err(analytic_error)
    }

    /// Evaluate Cartesian states at the given TDB epochs in input order.
    pub fn ephemeris(&self, epochs_tdb_seconds: &[f64]) -> JsResult<Vec<CartesianState>> {
        self.0
            .ephemeris(epochs_tdb_seconds)
            .map(|states| states.into_iter().map(CartesianState).collect())
            .map_err(analytic_error)
    }

    /// Secular along-track coefficient in radians per square second.
    #[wasm_bindgen(js_name = m2RadS2)]
    pub fn m2_rad_s2(&self) -> f64 {
        self.0.m2_rad_s2()
    }
}

/// Convert osculating Cartesian state to Brouwer-Lyddane mean elements.
#[wasm_bindgen(js_name = brouwerLyddaneMeanElements)]
pub fn brouwer_lyddane_mean_elements(
    osculating: &CartesianState,
    gravity: &ZonalHarmonics,
    options: &MeanConversionOptions,
) -> JsResult<MeanElements> {
    analytic::brouwer_lyddane_mean_elements(&osculating.0, &gravity.0, options.0)
        .map(|result| MeanElements(MeanElementsValue::Keplerian(result)))
        .map_err(analytic_error)
}

/// Convert Brouwer-Lyddane mean elements to osculating elements.
#[wasm_bindgen(js_name = brouwerLyddaneOsculatingElements)]
pub fn brouwer_lyddane_osculating_elements(
    mean: &KeplerianElements,
    gravity: &ZonalHarmonics,
) -> JsResult<KeplerianElements> {
    analytic::brouwer_lyddane_osculating_elements(&mean.0, &gravity.0)
        .map(KeplerianElements)
        .map_err(analytic_error)
}

#[wasm_bindgen(typescript_custom_section)]
const ANALYTIC_TYPES: &str = r#"
/** Details returned by AnalyticPropagationError. */
export type AnalyticPropagationErrorDetail =
    | { kind: "nonFinite"; field: string }
    | { kind: "invalidParameter"; field: string; reason: string }
    | { kind: "notElliptic"; field: string }
    | { kind: "retrogradeEquatorial"; inclinationRad: number }
    | { kind: "insideReferenceSphere"; semiMajorAxisKm: number; referenceRadiusKm: number }
    | { kind: "eccentricityTooLarge"; eccentricity: number; limit: number }
    | { kind: "nearlyEquatorial"; inclinationRad: number }
    | { kind: "nearCriticalInclination"; inclinationRad: number }
    | { kind: "meanElementsNotConverged"; theory: AnalyticTheory; iterations: number }
    | { kind: "keplerNotConverged"; field: string }
    | { kind: "nonFiniteResult"; theory: AnalyticTheory; field: string };

/** AnalyticPropagationError with the core diagnostic message and structured detail. */
export interface AnalyticPropagationError extends Error {
    readonly name: "AnalyticPropagationError";
    readonly kind: AnalyticPropagationErrorDetail["kind"];
    readonly detail: AnalyticPropagationErrorDetail;
}
"#;
