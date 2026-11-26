//! ICGEM gravity field data and spherical harmonic evaluation.

use std::sync::Arc;

use serde_json::json;
use sidereon_core::astro::forces::{self, TideSystem};
use wasm_bindgen::prelude::*;

use crate::domain_error::propagation_error;
use crate::error::{index_arg, range_error, type_error, typed_error};
use crate::marshal::vec3;

type JsResult<T> = Result<T, JsValue>;

fn degree_or_order(value: f64, field: &str) -> JsResult<u16> {
    u16::try_from(index_arg(value, field)?)
        .map_err(|_| range_error(&format!("{field} must be an integer from 0 through 65535")))
}

fn tide_system_label(value: TideSystem) -> &'static str {
    match value {
        TideSystem::TideFree => "tideFree",
        TideSystem::ZeroTide => "zeroTide",
        TideSystem::MeanTide => "meanTide",
    }
}

fn parse_tide_system(value: &str) -> JsResult<TideSystem> {
    match value {
        "tideFree" => Ok(TideSystem::TideFree),
        "zeroTide" => Ok(TideSystem::ZeroTide),
        "meanTide" => Ok(TideSystem::MeanTide),
        _ => Err(type_error(
            "tideSystem must be tideFree, zeroTide or meanTide",
        )),
    }
}

fn icgem_error(error: forces::IcgemError) -> JsValue {
    let detail = match &error {
        forces::IcgemError::MissingField {
            line,
            keyword,
            field,
        } => json!({"kind":"missingField", "line":line, "keyword":keyword, "field":field}),
        forces::IcgemError::InvalidField {
            line,
            keyword,
            field,
            value,
        } => {
            json!({"kind":"invalidField", "line":line, "keyword":keyword, "field":field, "value":value})
        }
        forces::IcgemError::MissingHeaderKeyword { keyword } => {
            json!({"kind":"missingHeaderKeyword", "keyword":keyword})
        }
        forces::IcgemError::RepeatedHeaderKeyword {
            line,
            first_line,
            keyword,
        } => {
            json!({"kind":"repeatedHeaderKeyword", "line":line, "firstLine":first_line, "keyword":keyword})
        }
        forces::IcgemError::MissingEndOfHead => json!({"kind":"missingEndOfHead"}),
        forces::IcgemError::KeywordNotInFormat { line, keyword } => {
            json!({"kind":"keywordNotInFormat", "line":line, "keyword":keyword})
        }
        forces::IcgemError::UnknownRecordKeyword { line, keyword } => {
            json!({"kind":"unknownRecordKeyword", "line":line, "keyword":keyword})
        }
        forces::IcgemError::MissingRecordKeyword { line } => {
            json!({"kind":"missingRecordKeyword", "line":line})
        }
        forces::IcgemError::DegreeAboveMaximum {
            line,
            degree,
            max_degree,
        } => {
            json!({"kind":"degreeAboveMaximum", "line":line, "degree":degree, "maxDegree":max_degree})
        }
        forces::IcgemError::OrderAboveDegree {
            line,
            degree,
            order,
        } => json!({"kind":"orderAboveDegree", "line":line, "degree":degree, "order":order}),
        forces::IcgemError::RepeatedCoefficient {
            line,
            first_line,
            degree,
            order,
        } => {
            json!({"kind":"repeatedCoefficient", "line":line, "firstLine":first_line, "degree":degree, "order":order})
        }
        forces::IcgemError::OverlappingTerm {
            line,
            first_line,
            keyword,
            degree,
            order,
        } => {
            json!({"kind":"overlappingTerm", "line":line, "firstLine":first_line, "keyword":keyword, "degree":degree, "order":order})
        }
        forces::IcgemError::UncoveredTerm {
            line,
            keyword,
            degree,
            order,
        } => {
            json!({"kind":"uncoveredTerm", "line":line, "keyword":keyword, "degree":degree, "order":order})
        }
        forces::IcgemError::MissingCoefficient { degree, order } => {
            json!({"kind":"missingCoefficient", "degree":degree, "order":order})
        }
        forces::IcgemError::TooFewRecords {
            max_degree,
            required,
            lines,
        } => {
            json!({"kind":"tooFewRecords", "maxDegree":max_degree, "required":required, "lines":lines})
        }
        forces::IcgemError::NormalizationOutOfRange {
            line,
            degree,
            order,
        } => json!({"kind":"normalizationOutOfRange", "line":line, "degree":degree, "order":order}),
        forces::IcgemError::TruncationOutOfRange {
            degree,
            order,
            max_degree,
        } => {
            json!({"kind":"truncationOutOfRange", "degree":degree, "order":order, "maxDegree":max_degree})
        }
        forces::IcgemError::UnknownTideSystem => json!({"kind":"unknownTideSystem"}),
        forces::IcgemError::TideSystemConflict { stated, assumed } => {
            json!({"kind":"tideSystemConflict", "stated":tide_system_label(*stated), "assumed":tide_system_label(*assumed)})
        }
        forces::IcgemError::EpochOutsideValidity {
            degree,
            order,
            epoch_tt_seconds,
        } => {
            json!({"kind":"epochOutsideValidity", "degree":degree, "order":order, "epochTtSeconds":epoch_tt_seconds})
        }
        forces::IcgemError::NonFiniteEpoch => json!({"kind":"nonFiniteEpoch"}),
        forces::IcgemError::NonFiniteCoefficient {
            degree,
            order,
            epoch_tt_seconds,
        } => {
            json!({"kind":"nonFiniteCoefficient", "degree":degree, "order":order, "epochTtSeconds":epoch_tt_seconds})
        }
        _ => json!({"kind":"unknown"}),
    };
    typed_error("IcgemError", &error.to_string(), detail)
}
/// Uncertainty columns declared by an ICGEM gravity field.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum IcgemErrors {
    /// `no`: no sigma columns.
    No,
    /// `calibrated`: two calibrated sigma columns.
    Calibrated,
    /// `formal`: two formal sigma columns.
    Formal,
    /// `calibrated_and_formal`: two calibrated then two formal columns.
    CalibratedAndFormal,
}

impl From<forces::IcgemErrors> for IcgemErrors {
    fn from(value: forces::IcgemErrors) -> Self {
        match value {
            forces::IcgemErrors::No => Self::No,
            forces::IcgemErrors::Calibrated => Self::Calibrated,
            forces::IcgemErrors::Formal => Self::Formal,
            forces::IcgemErrors::CalibratedAndFormal => Self::CalibratedAndFormal,
        }
    }
}

/// Coefficient normalization declared by an ICGEM gravity field.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum IcgemNormalization {
    /// `fully_normalized`, the default.
    FullyNormalized,
    /// `unnormalized`; converted to fully normalized on reading.
    Unnormalized,
}

impl From<forces::IcgemNormalization> for IcgemNormalization {
    fn from(value: forces::IcgemNormalization) -> Self {
        match value {
            forces::IcgemNormalization::FullyNormalized => Self::FullyNormalized,
            forces::IcgemNormalization::Unnormalized => Self::Unnormalized,
        }
    }
}

/// ICGEM file format version.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum IcgemFormat {
    /// ICGEM 1.0, with time-variable coefficients referenced to a single epoch.
    Icgem1_0,
    /// `icgem2.0`: time-variable records valid over `t0 <= t < t1`.
    Icgem2_0,
}

impl From<forces::IcgemFormat> for IcgemFormat {
    fn from(value: forces::IcgemFormat) -> Self {
        match value {
            forces::IcgemFormat::Icgem1_0 => Self::Icgem1_0,
            forces::IcgemFormat::Icgem2_0 => Self::Icgem2_0,
        }
    }
}

/// Constant, trend, or periodic time-variable gravity term.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct IcgemTermKind(forces::IcgemTermKind);

#[wasm_bindgen]
impl IcgemTermKind {
    /// Compare the stored value.
    pub fn equals(&self, other: &IcgemTermKind) -> bool {
        self.0 == other.0
    }

    /// Constant part of a time-variable coefficient.
    #[wasm_bindgen(js_name = CONSTANT)]
    pub fn constant() -> Self {
        Self(forces::IcgemTermKind::Constant)
    }
    /// Linear trend per Julian year.
    #[wasm_bindgen(js_name = TREND)]
    pub fn trend() -> Self {
        Self(forces::IcgemTermKind::Trend)
    }
    /// Cosine amplitude at a period in Julian years.
    pub fn cosine(period_years: f64) -> Self {
        Self(forces::IcgemTermKind::Cosine { period_years })
    }

    /// Sine amplitude at a period in Julian years.
    pub fn sine(period_years: f64) -> Self {
        Self(forces::IcgemTermKind::Sine { period_years })
    }

    /// Period of a sinusoidal term in Julian years.
    #[wasm_bindgen(getter, js_name = periodYears)]
    pub fn period_years(&self) -> Option<f64> {
        match self.0 {
            forces::IcgemTermKind::Cosine { period_years }
            | forces::IcgemTermKind::Sine { period_years } => Some(period_years),
            _ => None,
        }
    }

    /// Stable name of the time-variable term.
    #[wasm_bindgen(getter)]
    pub fn label(&self) -> String {
        match self.0 {
            forces::IcgemTermKind::Constant => "constant",
            forces::IcgemTermKind::Trend => "trend",
            forces::IcgemTermKind::Cosine { .. } => "cosine",
            forces::IcgemTermKind::Sine { .. } => "sine",
        }
        .to_owned()
    }
}

/// Metadata from the gravity field header.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct IcgemHeader(forces::IcgemHeader);

#[wasm_bindgen]
impl IcgemHeader {
    /// Compare the stored value.
    pub fn equals(&self, other: &IcgemHeader) -> bool {
        self.0 == other.0
    }

    /// Model name.
    #[wasm_bindgen(getter, js_name = modelName)]
    pub fn model_name(&self) -> String {
        self.0.model_name.clone()
    }

    /// Gravitational parameter in cubic metres per square second.
    #[wasm_bindgen(getter, js_name = gravityConstantM3S2)]
    pub fn gravity_constant_m3_s2(&self) -> f64 {
        self.0.gravity_constant_m3_s2
    }

    /// Reference radius in metres.
    #[wasm_bindgen(getter, js_name = radiusM)]
    pub fn radius_m(&self) -> f64 {
        self.0.radius_m
    }

    /// Maximum degree stated in the file.
    #[wasm_bindgen(getter, js_name = maxDegree)]
    pub fn max_degree(&self) -> u16 {
        self.0.max_degree
    }

    /// Uncertainty column layout.
    #[wasm_bindgen(getter)]
    pub fn errors(&self) -> IcgemErrors {
        self.0.errors.into()
    }

    /// Normalization stated in the file.
    #[wasm_bindgen(getter)]
    pub fn normalization(&self) -> IcgemNormalization {
        self.0.normalization.into()
    }

    /// Tide system if stated.
    #[wasm_bindgen(getter, js_name = tideSystem, unchecked_return_type = "TideSystem | undefined")]
    pub fn tide_system(&self) -> Option<String> {
        self.0.tide_system.map(tide_system_label).map(str::to_owned)
    }

    /// ICGEM format version.
    #[wasm_bindgen(getter)]
    pub fn format(&self) -> IcgemFormat {
        self.0.format.into()
    }
}

/// Fully normalized coefficient uncertainties.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct IcgemSigmas(forces::IcgemSigmas);

impl Default for IcgemSigmas {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl IcgemSigmas {
    /// Compare the stored value.
    pub fn equals(&self, other: &IcgemSigmas) -> bool {
        self.0 == other.0
    }

    /// Build an uncertainty record with no sigmas.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self(forces::IcgemSigmas::default())
    }

    /// Calibrated cosine and sine uncertainties, if present.
    #[wasm_bindgen(getter)]
    pub fn calibrated(&self) -> Option<Vec<f64>> {
        self.0.calibrated.map(|sigmas| sigmas.to_vec())
    }

    /// Formal cosine and sine uncertainties, if present.
    #[wasm_bindgen(getter)]
    pub fn formal(&self) -> Option<Vec<f64>> {
        self.0.formal.map(|sigmas| sigmas.to_vec())
    }
}

/// A time-variable coefficient record.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct IcgemTimeVariableTerm(forces::IcgemTimeVariableTerm);

#[wasm_bindgen]
impl IcgemTimeVariableTerm {
    /// Compare the stored value.
    pub fn equals(&self, other: &IcgemTimeVariableTerm) -> bool {
        self.0 == other.0
    }

    /// Line number in the gravity field text.
    #[wasm_bindgen(getter)]
    pub fn line(&self) -> usize {
        self.0.line
    }

    /// Type and optional period of the record.
    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> IcgemTermKind {
        IcgemTermKind(self.0.kind)
    }

    /// Harmonic degree.
    #[wasm_bindgen(getter)]
    pub fn degree(&self) -> u16 {
        self.0.degree
    }

    /// Harmonic order.
    #[wasm_bindgen(getter)]
    pub fn order(&self) -> u16 {
        self.0.order
    }

    /// Fully normalized cosine value.
    #[wasm_bindgen(getter)]
    pub fn c(&self) -> f64 {
        self.0.c
    }

    /// Fully normalized sine value.
    #[wasm_bindgen(getter)]
    pub fn s(&self) -> f64 {
        self.0.s
    }

    /// Coefficient uncertainties.
    #[wasm_bindgen(getter)]
    pub fn sigmas(&self) -> IcgemSigmas {
        IcgemSigmas(self.0.sigmas)
    }

    /// Reference epoch in TT seconds from J2000.
    #[wasm_bindgen(getter, js_name = referenceTtSeconds)]
    pub fn reference_tt_seconds(&self) -> f64 {
        self.0.reference_tt_seconds
    }

    /// Inclusive validity start in TT seconds from J2000.
    #[wasm_bindgen(getter, js_name = validFromTtSeconds)]
    pub fn valid_from_tt_seconds(&self) -> f64 {
        self.0.valid_from_tt_seconds
    }

    /// Exclusive validity end in TT seconds from J2000.
    #[wasm_bindgen(getter, js_name = validUntilTtSeconds)]
    pub fn valid_until_tt_seconds(&self) -> f64 {
        self.0.valid_until_tt_seconds
    }
}

/// Fully normalized real spherical harmonic coefficient.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct SphericalHarmonicCoefficient(forces::SphericalHarmonicCoefficient);

#[wasm_bindgen]
impl SphericalHarmonicCoefficient {
    /// Compare the stored value.
    pub fn equals(&self, other: &SphericalHarmonicCoefficient) -> bool {
        self.0 == other.0
    }

    /// Harmonic degree.
    #[wasm_bindgen(getter)]
    pub fn degree(&self) -> u16 {
        self.0.degree
    }

    /// Harmonic order.
    #[wasm_bindgen(getter)]
    pub fn order(&self) -> u16 {
        self.0.order
    }

    /// Fully normalized cosine coefficient.
    #[wasm_bindgen(getter)]
    pub fn c(&self) -> f64 {
        self.0.c
    }

    /// Fully normalized sine coefficient.
    #[wasm_bindgen(getter)]
    pub fn s(&self) -> f64 {
        self.0.s
    }
}

/// Static and time-variable coefficients from an ICGEM gravity field.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct IcgemGravityField(forces::IcgemGravityField);

#[wasm_bindgen]
impl IcgemGravityField {
    /// Compare the stored value.
    pub fn equals(&self, other: &IcgemGravityField) -> bool {
        self.0 == other.0
    }

    /// Parse an ICGEM gravity field.
    pub fn parse(text: &str) -> JsResult<Self> {
        forces::IcgemGravityField::parse(text)
            .map(Self)
            .map_err(icgem_error)
    }

    /// Parse an ICGEM gravity field from UTF-8 bytes.
    #[wasm_bindgen(js_name = fromBytes)]
    pub fn from_bytes(bytes: &[u8]) -> JsResult<Self> {
        let text = crate::error::utf8_text(bytes, "ICGEM gravity field")?;
        Self::parse(&text)
    }

    /// Parse an ICGEM gravity field with a storage truncation.
    #[wasm_bindgen(js_name = parseTruncated)]
    pub fn parse_truncated(text: &str, max_degree: f64, max_order: f64) -> JsResult<Self> {
        forces::IcgemGravityField::parse_truncated(
            text,
            degree_or_order(max_degree, "maxDegree")?,
            degree_or_order(max_order, "maxOrder")?,
        )
        .map(Self)
        .map_err(icgem_error)
    }

    /// Gravity field metadata.
    pub fn header(&self) -> IcgemHeader {
        IcgemHeader(self.0.header().clone())
    }

    /// Highest stored degree.
    #[wasm_bindgen(js_name = storedMaxDegree)]
    pub fn stored_max_degree(&self) -> u16 {
        self.0.stored_max_degree()
    }

    /// Highest stored order.
    #[wasm_bindgen(js_name = storedMaxOrder)]
    pub fn stored_max_order(&self) -> u16 {
        self.0.stored_max_order()
    }

    /// Whether any stored coefficient varies with time.
    #[wasm_bindgen(js_name = isTimeVariable)]
    pub fn is_time_variable(&self) -> bool {
        self.0.is_time_variable()
    }

    /// Fully normalized static coefficient, if stored.
    #[wasm_bindgen(js_name = staticCoefficient)]
    pub fn static_coefficient(
        &self,
        degree: f64,
        order: f64,
    ) -> JsResult<Option<SphericalHarmonicCoefficient>> {
        Ok(self
            .0
            .static_coefficient(
                degree_or_order(degree, "degree")?,
                degree_or_order(order, "order")?,
            )
            .map(SphericalHarmonicCoefficient))
    }

    /// Uncertainties of a static coefficient, if stored.
    #[wasm_bindgen(js_name = staticSigmas)]
    pub fn static_sigmas(&self, degree: f64, order: f64) -> JsResult<Option<IcgemSigmas>> {
        Ok(self
            .0
            .static_sigmas(
                degree_or_order(degree, "degree")?,
                degree_or_order(order, "order")?,
            )
            .map(IcgemSigmas))
    }

    /// Time-variable records stored by the field.
    #[wasm_bindgen(js_name = timeVariableTerms)]
    pub fn time_variable_terms(&self) -> Vec<IcgemTimeVariableTerm> {
        self.0
            .time_variable_terms()
            .iter()
            .copied()
            .map(IcgemTimeVariableTerm)
            .collect()
    }

    /// Fully normalized coefficient at a TT epoch, if stored.
    #[wasm_bindgen(js_name = coefficientAt)]
    pub fn coefficient_at(
        &self,
        degree: f64,
        order: f64,
        epoch_tt_seconds: f64,
    ) -> JsResult<Option<SphericalHarmonicCoefficient>> {
        self.0
            .coefficient_at(
                degree_or_order(degree, "degree")?,
                degree_or_order(order, "order")?,
                epoch_tt_seconds,
            )
            .map(|value| value.map(SphericalHarmonicCoefficient))
            .map_err(icgem_error)
    }

    /// Apply a tide system when the field lacks one.
    #[wasm_bindgen(js_name = withAssumedTideSystem)]
    pub fn with_assumed_tide_system(
        &self,
        #[wasm_bindgen(unchecked_param_type = "TideSystem")] tide_system: &str,
    ) -> JsResult<Self> {
        self.0
            .clone()
            .with_assumed_tide_system(parse_tide_system(tide_system)?)
            .map(Self)
            .map_err(icgem_error)
    }

    /// Build a spherical-harmonic force model.
    pub fn gravity(&self, max_degree: f64, max_order: f64) -> JsResult<SphericalHarmonicGravity> {
        self.0
            .gravity(
                degree_or_order(max_degree, "maxDegree")?,
                degree_or_order(max_order, "maxOrder")?,
            )
            .map(Arc::new)
            .map(SphericalHarmonicGravity)
            .map_err(icgem_error)
    }

    /// Build a spherical-harmonic force model converted to the requested tide system.
    #[wasm_bindgen(js_name = gravityInTideSystem)]
    pub fn gravity_in_tide_system(
        &self,
        max_degree: f64,
        max_order: f64,
        #[wasm_bindgen(unchecked_param_type = "TideSystem")] tide_system: &str,
    ) -> JsResult<SphericalHarmonicGravity> {
        self.0
            .gravity_in_tide_system(
                degree_or_order(max_degree, "maxDegree")?,
                degree_or_order(max_order, "maxOrder")?,
                parse_tide_system(tide_system)?,
            )
            .map(Arc::new)
            .map(SphericalHarmonicGravity)
            .map_err(icgem_error)
    }
}

/// Fully normalized gravity force in a body-fixed frame.
#[wasm_bindgen]
#[derive(Clone, PartialEq)]
pub struct SphericalHarmonicGravity(pub(crate) Arc<forces::SphericalHarmonicGravity>);

#[wasm_bindgen]
impl SphericalHarmonicGravity {
    /// Compare the stored value.
    pub fn equals(&self, other: &SphericalHarmonicGravity) -> bool {
        self.0 == other.0
    }

    /// Whether the gravity field has time-variable coefficients.
    #[wasm_bindgen(js_name = isTimeVariable)]
    pub fn is_time_variable(&self) -> bool {
        self.0.is_time_variable()
    }

    /// Evaluate perturbing acceleration in kilometres per square second.
    #[wasm_bindgen(js_name = bodyFixedAccelerationKmS2)]
    pub fn body_fixed_acceleration_km_s2(
        &self,
        position_body_fixed_km: &[f64],
    ) -> JsResult<Vec<f64>> {
        self.0
            .body_fixed_acceleration_km_s2(vec3("positionBodyFixedKm", position_body_fixed_km)?)
            .map(|acceleration| acceleration.to_vec())
            .map_err(propagation_error)
    }

    /// Evaluate perturbing acceleration in kilometres per square second at the given TT epoch.
    #[wasm_bindgen(js_name = bodyFixedAccelerationAtKmS2)]
    pub fn body_fixed_acceleration_at_km_s2(
        &self,
        position_body_fixed_km: &[f64],
        epoch_tt_seconds: f64,
    ) -> JsResult<Vec<f64>> {
        self.0
            .body_fixed_acceleration_at_km_s2(
                vec3("positionBodyFixedKm", position_body_fixed_km)?,
                epoch_tt_seconds,
            )
            .map(|acceleration| acceleration.to_vec())
            .map_err(propagation_error)
    }

    /// Evaluate perturbing potential in square kilometres per square second.
    #[wasm_bindgen(js_name = bodyFixedPotentialKm2S2)]
    pub fn body_fixed_potential_km2_s2(&self, position_body_fixed_km: &[f64]) -> JsResult<f64> {
        self.0
            .body_fixed_potential_km2_s2(vec3("positionBodyFixedKm", position_body_fixed_km)?)
            .map_err(propagation_error)
    }

    /// Evaluate perturbing potential in square kilometres per square second at the given TT epoch.
    #[wasm_bindgen(js_name = bodyFixedPotentialAtKm2S2)]
    pub fn body_fixed_potential_at_km2_s2(
        &self,
        position_body_fixed_km: &[f64],
        epoch_tt_seconds: f64,
    ) -> JsResult<f64> {
        self.0
            .body_fixed_potential_at_km2_s2(
                vec3("positionBodyFixedKm", position_body_fixed_km)?,
                epoch_tt_seconds,
            )
            .map_err(propagation_error)
    }

    /// Fully normalized coefficient at a TT epoch, if in the truncation.
    #[wasm_bindgen(js_name = coefficientAt)]
    pub fn coefficient_at(
        &self,
        degree: f64,
        order: f64,
        epoch_tt_seconds: f64,
    ) -> JsResult<Option<SphericalHarmonicCoefficient>> {
        self.0
            .coefficient_at(
                degree_or_order(degree, "degree")?,
                degree_or_order(order, "order")?,
                epoch_tt_seconds,
            )
            .map(|value| value.map(SphericalHarmonicCoefficient))
            .map_err(propagation_error)
    }
}

#[wasm_bindgen(typescript_custom_section)]
const ICGEM_TYPES: &str = r#"
/** Details returned by IcgemError. */
export type IcgemErrorDetail =
    | { kind: "missingField"; line: number; keyword: string; field: string }
    | { kind: "invalidField"; line: number; keyword: string; field: string; value: string }
    | { kind: "missingHeaderKeyword"; keyword: string }
    | { kind: "repeatedHeaderKeyword"; line: number; firstLine: number; keyword: string }
    | { kind: "missingEndOfHead" }
    | { kind: "keywordNotInFormat"; line: number; keyword: string }
    | { kind: "unknownRecordKeyword"; line: number; keyword: string }
    | { kind: "missingRecordKeyword"; line: number }
    | { kind: "degreeAboveMaximum"; line: number; degree: number; maxDegree: number }
    | { kind: "orderAboveDegree"; line: number; degree: number; order: number }
    | { kind: "repeatedCoefficient"; line: number; firstLine: number; degree: number; order: number }
    | { kind: "overlappingTerm"; line: number; firstLine: number; keyword: string; degree: number; order: number }
    | { kind: "uncoveredTerm"; line: number; keyword: string; degree: number; order: number }
    | { kind: "missingCoefficient"; degree: number; order: number }
    | { kind: "tooFewRecords"; maxDegree: number; required: number; lines: number }
    | { kind: "normalizationOutOfRange"; line: number; degree: number; order: number }
    | { kind: "truncationOutOfRange"; degree: number; order: number; maxDegree: number }
    | { kind: "unknownTideSystem" }
    | { kind: "tideSystemConflict"; stated: TideSystem; assumed: TideSystem }
    | { kind: "epochOutsideValidity"; degree: number; order: number; epochTtSeconds: number }
    | { kind: "nonFiniteEpoch" }
    | { kind: "nonFiniteCoefficient"; degree: number; order: number; epochTtSeconds: number }
    | { kind: "unknown" };

/** IcgemError with the core diagnostic message and structured detail. */
export interface IcgemError extends Error {
    readonly name: "IcgemError";
    readonly kind: IcgemErrorDetail["kind"];
    readonly detail: IcgemErrorDetail;
}
"#;

crate::force_models::force_methods!(SphericalHarmonicGravity, 0, acceleration, acceleration);

#[wasm_bindgen]
impl SphericalHarmonicGravity {
    /// Return a shared handle usable for propagation and estimation.
    #[wasm_bindgen(js_name = asForceModel)]
    pub fn as_force_model(&self) -> crate::force_models::ForceModel {
        crate::force_models::ForceModel(self.0.clone())
    }
}
