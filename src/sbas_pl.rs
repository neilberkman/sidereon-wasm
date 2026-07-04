//! SBAS protection-level binding.
//!
//! The public entry point accepts the same protection geometry layout as ARAIM,
//! an SBAS range-error model, and fixed K multipliers. The gain projection and
//! horizontal/vertical protection-level computation are delegated to
//! `sidereon_core::sbas_pl`.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::sbas_pl::{
    sbas_protection_levels as core_sbas_protection_levels, AirborneModel as CoreAirborneModel,
    DegradationParams as CoreDegradationParams, SbasErrorModel as CoreSbasErrorModel,
    SbasKMultipliers as CoreSbasKMultipliers, SbasPlError as CoreSbasPlError,
    SbasProtection as CoreSbasProtection, SbasSisError as CoreSbasSisError,
};
use sidereon_core::GnssSatelliteId;

use crate::araim::parse_geometry;
use crate::error::{engine_error, range_error, type_error};
use crate::sbas::SbasCorrectionStore;

fn valid_positive_finite(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

fn valid_nonnegative_finite(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}

fn parse_sat(token: &str) -> Result<GnssSatelliteId, JsValue> {
    token
        .parse::<GnssSatelliteId>()
        .map_err(|e| type_error(&format!("invalid satellite token {token:?}: {e}")))
}

fn option_f64(value: Option<f64>) -> JsValue {
    value.map(JsValue::from_f64).unwrap_or(JsValue::NULL)
}

fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .map_err(|e| engine_error(format!("failed to serialize result: {e}")))
}

fn sbas_pl_error(error: CoreSbasPlError) -> JsValue {
    match error {
        CoreSbasPlError::InsufficientGeometry => type_error(&error.to_string()),
        CoreSbasPlError::InvalidErrorModel => range_error(&error.to_string()),
        CoreSbasPlError::NumericalFailure => engine_error(error),
    }
}

fn sbas_pl_error_name(error: SbasPlError) -> &'static str {
    match error {
        SbasPlError::InsufficientGeometry => "InsufficientGeometry",
        SbasPlError::NumericalFailure => "NumericalFailure",
        SbasPlError::InvalidErrorModel => "InvalidErrorModel",
    }
}

/// SBAS protection-level input or numerical failure.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SbasPlError {
    /// The geometry lacks enough independent rows for the active clocks.
    InsufficientGeometry,
    /// Matrix projection or covariance processing failed.
    NumericalFailure,
    /// The supplied error model or K multipliers are outside their domain.
    InvalidErrorModel,
}

impl From<CoreSbasPlError> for SbasPlError {
    fn from(value: CoreSbasPlError) -> Self {
        match value {
            CoreSbasPlError::InsufficientGeometry => Self::InsufficientGeometry,
            CoreSbasPlError::NumericalFailure => Self::NumericalFailure,
            CoreSbasPlError::InvalidErrorModel => Self::InvalidErrorModel,
        }
    }
}

/// Stable string label for an [`SbasPlError`] enum value.
#[wasm_bindgen(js_name = sbasPlErrorLabel)]
pub fn sbas_pl_error_label(error: SbasPlError) -> String {
    sbas_pl_error_name(error).to_string()
}

/// Fixed SBAS protection-level multipliers.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct SbasKMultipliers {
    inner: CoreSbasKMultipliers,
}

impl SbasKMultipliers {
    fn to_core(self) -> CoreSbasKMultipliers {
        self.inner
    }
}

#[wasm_bindgen]
impl SbasKMultipliers {
    /// Construct SBAS horizontal and vertical K multipliers.
    ///
    /// Both values must be positive finite numbers.
    #[wasm_bindgen(constructor)]
    pub fn new(k_h: f64, k_v: f64) -> Result<SbasKMultipliers, JsValue> {
        if !valid_positive_finite(k_h) || !valid_positive_finite(k_v) {
            return Err(range_error(
                "SBAS K multipliers must be positive finite numbers",
            ));
        }
        Ok(Self {
            inner: CoreSbasKMultipliers { k_h, k_v },
        })
    }

    /// Precision-approach SBAS K multipliers.
    #[wasm_bindgen(js_name = precisionApproach)]
    pub fn precision_approach() -> SbasKMultipliers {
        Self {
            inner: CoreSbasKMultipliers::PRECISION_APPROACH,
        }
    }

    /// En-route through non-precision-approach SBAS K multipliers.
    #[wasm_bindgen(js_name = enRouteNpa)]
    pub fn en_route_npa() -> SbasKMultipliers {
        Self {
            inner: CoreSbasKMultipliers::EN_ROUTE_NPA,
        }
    }

    /// Horizontal K multiplier.
    #[wasm_bindgen(getter, js_name = kH)]
    pub fn k_h(&self) -> f64 {
        self.inner.k_h
    }

    /// Vertical K multiplier.
    #[wasm_bindgen(getter, js_name = kV)]
    pub fn k_v(&self) -> f64 {
        self.inner.k_v
    }
}

/// SBAS protection-level output for one geometry snapshot.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct SbasProtection {
    inner: CoreSbasProtection,
}

impl From<CoreSbasProtection> for SbasProtection {
    fn from(inner: CoreSbasProtection) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl SbasProtection {
    /// Horizontal protection level, metres.
    #[wasm_bindgen(getter, js_name = hplM)]
    pub fn hpl_m(&self) -> f64 {
        self.inner.hpl_m
    }

    /// Vertical protection level, metres.
    #[wasm_bindgen(getter, js_name = vplM)]
    pub fn vpl_m(&self) -> f64 {
        self.inner.vpl_m
    }

    /// Horizontal one-sigma semi-major axis, metres.
    #[wasm_bindgen(getter, js_name = dMajorM)]
    pub fn d_major_m(&self) -> f64 {
        self.inner.d_major_m
    }

    /// Vertical one-sigma standard deviation, metres.
    #[wasm_bindgen(getter, js_name = sigmaUM)]
    pub fn sigma_u_m(&self) -> f64 {
        self.inner.sigma_u_m
    }

    /// East one-sigma standard deviation, metres.
    #[wasm_bindgen(getter, js_name = dEastM)]
    pub fn d_east_m(&self) -> f64 {
        self.inner.d_east_m
    }

    /// North one-sigma standard deviation, metres.
    #[wasm_bindgen(getter, js_name = dNorthM)]
    pub fn d_north_m(&self) -> f64 {
        self.inner.d_north_m
    }

    /// East-north covariance term, square metres.
    #[wasm_bindgen(getter, js_name = dEnM2)]
    pub fn d_en_m2(&self) -> f64 {
        self.inner.d_en_m2
    }
}

/// One satellite's SBAS one-sigma range-error budget.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct SbasSisError {
    inner: CoreSbasSisError,
}

impl From<CoreSbasSisError> for SbasSisError {
    fn from(inner: CoreSbasSisError) -> Self {
        Self { inner }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SbasSisErrorJs {
    id: String,
    sigma_flt_m: f64,
    sigma_uire_m: f64,
    sigma_air_m: f64,
    sigma_tropo_m: f64,
    sigma_m: Option<f64>,
    variance_m2: Option<f64>,
}

impl From<CoreSbasSisError> for SbasSisErrorJs {
    fn from(value: CoreSbasSisError) -> Self {
        Self {
            id: value.id.to_string(),
            sigma_flt_m: value.sigma_flt_m,
            sigma_uire_m: value.sigma_uire_m,
            sigma_air_m: value.sigma_air_m,
            sigma_tropo_m: value.sigma_tropo_m,
            sigma_m: value.sigma_m(),
            variance_m2: value.variance_m2(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SbasSisErrorInput {
    id: String,
    sigma_m: Option<f64>,
    sigma_flt_m: Option<f64>,
    sigma_uire_m: Option<f64>,
    sigma_air_m: Option<f64>,
    sigma_tropo_m: Option<f64>,
}

impl SbasSisErrorInput {
    fn to_core(&self) -> Result<CoreSbasSisError, JsValue> {
        let id = parse_sat(&self.id)?;
        let any_component = self.sigma_flt_m.is_some()
            || self.sigma_uire_m.is_some()
            || self.sigma_air_m.is_some()
            || self.sigma_tropo_m.is_some();
        if let (Some(sigma_m), false) = (self.sigma_m, any_component) {
            return Ok(CoreSbasSisError {
                id,
                sigma_flt_m: sigma_m,
                sigma_uire_m: 0.0,
                sigma_air_m: 0.0,
                sigma_tropo_m: 0.0,
            });
        }
        if self.sigma_m.is_some() && any_component {
            return Err(type_error(
                "set either sigmaM or component sigmas, not both",
            ));
        }
        Ok(CoreSbasSisError {
            id,
            sigma_flt_m: self
                .sigma_flt_m
                .ok_or_else(|| type_error("sigmaFltM is required"))?,
            sigma_uire_m: self.sigma_uire_m.unwrap_or(0.0),
            sigma_air_m: self.sigma_air_m.unwrap_or(0.0),
            sigma_tropo_m: self.sigma_tropo_m.unwrap_or(0.0),
        })
    }
}

#[wasm_bindgen]
impl SbasSisError {
    /// Construct one SBAS range-error row.
    ///
    /// Component sigmas are metres. They are combined by root-sum-square when
    /// the model is evaluated.
    #[wasm_bindgen(constructor)]
    pub fn new(
        id: &str,
        sigma_flt_m: f64,
        sigma_uire_m: f64,
        sigma_air_m: f64,
        sigma_tropo_m: f64,
    ) -> Result<SbasSisError, JsValue> {
        Ok(Self {
            inner: CoreSbasSisError {
                id: parse_sat(id)?,
                sigma_flt_m,
                sigma_uire_m,
                sigma_air_m,
                sigma_tropo_m,
            },
        })
    }

    /// Satellite token for this range-error row.
    #[wasm_bindgen(getter)]
    pub fn id(&self) -> String {
        self.inner.id.to_string()
    }

    /// Fast and long-term correction residual sigma, metres.
    #[wasm_bindgen(getter, js_name = sigmaFltM)]
    pub fn sigma_flt_m(&self) -> f64 {
        self.inner.sigma_flt_m
    }

    /// User ionospheric range-error sigma, metres.
    #[wasm_bindgen(getter, js_name = sigmaUireM)]
    pub fn sigma_uire_m(&self) -> f64 {
        self.inner.sigma_uire_m
    }

    /// Airborne receiver noise, divergence, and multipath sigma, metres.
    #[wasm_bindgen(getter, js_name = sigmaAirM)]
    pub fn sigma_air_m(&self) -> f64 {
        self.inner.sigma_air_m
    }

    /// Tropospheric residual sigma, metres.
    #[wasm_bindgen(getter, js_name = sigmaTropoM)]
    pub fn sigma_tropo_m(&self) -> f64 {
        self.inner.sigma_tropo_m
    }

    /// Sum-of-squares range variance, square metres, or `null` if invalid.
    #[wasm_bindgen(js_name = varianceM2)]
    pub fn variance_m2(&self) -> JsValue {
        option_f64(self.inner.variance_m2())
    }

    /// Total one-sigma range error, metres, or `null` if invalid.
    #[wasm_bindgen(js_name = sigmaM)]
    pub fn sigma_m(&self) -> JsValue {
        option_f64(self.inner.sigma_m())
    }
}

/// Index-aligned SBAS error model for protection-level geometry rows.
#[wasm_bindgen]
#[derive(Clone)]
pub struct SbasErrorModel {
    pub(crate) inner: CoreSbasErrorModel,
}

#[wasm_bindgen]
impl SbasErrorModel {
    /// Construct an SBAS error model from rows.
    ///
    /// `rows` is an array of `{ id, sigmaFltM, sigmaUireM?, sigmaAirM?,
    /// sigmaTropoM? }` objects. As a shorthand, a row may provide only
    /// `{ id, sigmaM }`, which is stored as the total one-sigma range term.
    #[wasm_bindgen(constructor)]
    pub fn new(rows: JsValue) -> Result<SbasErrorModel, JsValue> {
        let rows: Vec<SbasSisErrorInput> = serde_wasm_bindgen::from_value(rows)
            .map_err(|e| type_error(&format!("invalid SBAS error rows: {e}")))?;
        let rows = rows
            .iter()
            .map(SbasSisErrorInput::to_core)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            inner: CoreSbasErrorModel::new(rows),
        })
    }

    /// Build an SBAS error model from a decoded correction store.
    ///
    /// `geo` is an SBAS satellite token, `geometry` is protection geometry, and
    /// `epochJ2000S` is the receive epoch used for freshness checks. Omit
    /// `airborne` or `degradation` to use their defaults.
    #[wasm_bindgen(js_name = fromStore)]
    pub fn from_store(
        store: &SbasCorrectionStore,
        geo: &str,
        geometry: JsValue,
        airborne: Option<AirborneModel>,
        epoch_j2000_s: f64,
        degradation: Option<DegradationParams>,
    ) -> Result<SbasErrorModel, JsValue> {
        let geo = parse_sat(geo)?;
        let geometry = parse_geometry(geometry)?;
        let airborne = airborne.map(|value| value.inner).unwrap_or_default();
        let degradation = degradation.map(|value| value.inner).unwrap_or_default();
        let inner = CoreSbasErrorModel::from_store(
            &store.inner,
            geo,
            &geometry,
            &airborne,
            epoch_j2000_s,
            &degradation,
        )
        .map_err(sbas_pl_error)?;
        Ok(Self { inner })
    }

    /// Number of range-error rows in the model.
    #[wasm_bindgen(getter, js_name = rowCount)]
    pub fn row_count(&self) -> usize {
        self.inner.rows.len()
    }

    /// Return the row for a satellite token, or `undefined` if it is absent.
    #[wasm_bindgen(js_name = rowFor)]
    pub fn row_for(&self, id: &str) -> Result<Option<SbasSisError>, JsValue> {
        let id = parse_sat(id)?;
        Ok(self.inner.row_for(id).copied().map(SbasSisError::from))
    }

    /// Error-model rows as plain objects.
    #[wasm_bindgen(getter)]
    pub fn rows(&self) -> Result<JsValue, JsValue> {
        let rows = self
            .inner
            .rows
            .iter()
            .copied()
            .map(SbasSisErrorJs::from)
            .collect::<Vec<_>>();
        to_js(&rows)
    }
}

/// Airborne receiver and multipath contribution model.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct AirborneModel {
    inner: CoreAirborneModel,
}

#[wasm_bindgen]
impl AirborneModel {
    /// Construct an airborne model from a receiver noise term in metres.
    #[wasm_bindgen(constructor)]
    pub fn new(sigma_noise_divergence_m: f64) -> Result<AirborneModel, JsValue> {
        if !valid_nonnegative_finite(sigma_noise_divergence_m) {
            return Err(range_error(
                "sigmaNoiseDivergenceM must be a non-negative finite number",
            ));
        }
        Ok(Self {
            inner: CoreAirborneModel::new(sigma_noise_divergence_m),
        })
    }

    /// Default airborne model.
    #[wasm_bindgen(js_name = defaultModel)]
    pub fn default_model() -> AirborneModel {
        Self {
            inner: CoreAirborneModel::default(),
        }
    }

    /// AAD-A airborne model.
    #[wasm_bindgen(js_name = aadA)]
    pub fn aad_a() -> AirborneModel {
        Self {
            inner: CoreAirborneModel::aad_a(),
        }
    }

    /// Receiver noise and code-carrier divergence sigma, metres.
    #[wasm_bindgen(getter, js_name = sigmaNoiseDivergenceM)]
    pub fn sigma_noise_divergence_m(&self) -> f64 {
        self.inner.sigma_noise_divergence_m
    }

    /// Airborne receiver, divergence, and multipath sigma, metres, or `null`.
    #[wasm_bindgen(js_name = sigmaAirM)]
    pub fn sigma_air_m(&self, elevation_rad: f64) -> JsValue {
        option_f64(self.inner.sigma_air_m(elevation_rad))
    }
}

/// SBAS degradation terms used when deriving an error model from a store.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct DegradationParams {
    inner: CoreDegradationParams,
}

#[wasm_bindgen]
impl DegradationParams {
    /// Construct SBAS degradation parameters.
    ///
    /// Omitted numeric fields use the no-degradation defaults. Invalid values
    /// throw a `RangeError`.
    #[wasm_bindgen(constructor)]
    pub fn new(
        delta_udre: Option<f64>,
        eps_fc_m: Option<f64>,
        eps_rrc_m: Option<f64>,
        eps_ltc_m: Option<f64>,
        eps_er_m: Option<f64>,
        eps_iono_m: Option<f64>,
        rss_udre: Option<bool>,
    ) -> Result<DegradationParams, JsValue> {
        let defaults = CoreDegradationParams::default();
        let inner = CoreDegradationParams {
            delta_udre: delta_udre.unwrap_or(defaults.delta_udre),
            eps_fc_m: eps_fc_m.unwrap_or(defaults.eps_fc_m),
            eps_rrc_m: eps_rrc_m.unwrap_or(defaults.eps_rrc_m),
            eps_ltc_m: eps_ltc_m.unwrap_or(defaults.eps_ltc_m),
            eps_er_m: eps_er_m.unwrap_or(defaults.eps_er_m),
            eps_iono_m: eps_iono_m.unwrap_or(defaults.eps_iono_m),
            rss_udre: rss_udre.unwrap_or(defaults.rss_udre),
        };
        if !inner.is_valid() {
            return Err(range_error("invalid SBAS degradation parameters"));
        }
        Ok(Self { inner })
    }

    /// No extra degradation and no UDRE inflation.
    #[wasm_bindgen(js_name = none)]
    pub fn none() -> DegradationParams {
        Self {
            inner: CoreDegradationParams::none(),
        }
    }

    /// Default degradation parameters.
    #[wasm_bindgen(js_name = defaultParams)]
    pub fn default_params() -> DegradationParams {
        Self {
            inner: CoreDegradationParams::default(),
        }
    }

    /// Variance multiplier applied to the UDRE variance table.
    #[wasm_bindgen(getter, js_name = deltaUdre)]
    pub fn delta_udre(&self) -> f64 {
        self.inner.delta_udre
    }

    /// Fast-correction degradation term, metres.
    #[wasm_bindgen(getter, js_name = epsFcM)]
    pub fn eps_fc_m(&self) -> f64 {
        self.inner.eps_fc_m
    }

    /// Range-rate-correction degradation term, metres.
    #[wasm_bindgen(getter, js_name = epsRrcM)]
    pub fn eps_rrc_m(&self) -> f64 {
        self.inner.eps_rrc_m
    }

    /// Long-term-correction degradation term, metres.
    #[wasm_bindgen(getter, js_name = epsLtcM)]
    pub fn eps_ltc_m(&self) -> f64 {
        self.inner.eps_ltc_m
    }

    /// En-route degradation term, metres.
    #[wasm_bindgen(getter, js_name = epsErM)]
    pub fn eps_er_m(&self) -> f64 {
        self.inner.eps_er_m
    }

    /// Ionospheric degradation term added to UIRE, metres.
    #[wasm_bindgen(getter, js_name = epsIonoM)]
    pub fn eps_iono_m(&self) -> f64 {
        self.inner.eps_iono_m
    }

    /// Whether UDRE degradation terms are combined by root-sum-square.
    #[wasm_bindgen(getter, js_name = rssUdre)]
    pub fn rss_udre(&self) -> bool {
        self.inner.rss_udre
    }

    /// True when all degradation parameters are inside the valid domain.
    #[wasm_bindgen(js_name = isValid)]
    pub fn is_valid(&self) -> bool {
        self.inner.is_valid()
    }
}

/// Compute SBAS horizontal and vertical protection levels.
///
/// `geometry` is the ARAIM protection geometry object with rows, receiver, and
/// clock systems. `model` supplies one range-error row per satellite. `k`
/// supplies the fixed SBAS multipliers.
#[wasm_bindgen(js_name = sbasProtectionLevels)]
pub fn sbas_protection_levels(
    geometry: JsValue,
    model: &SbasErrorModel,
    k: &SbasKMultipliers,
) -> Result<SbasProtection, JsValue> {
    let geometry = parse_geometry(geometry)?;
    let protection =
        core_sbas_protection_levels(&geometry, &model.inner, k.to_core()).map_err(sbas_pl_error)?;
    Ok(SbasProtection::from(protection))
}
