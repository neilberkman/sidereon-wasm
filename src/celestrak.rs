//! Network-free CelesTrak and NAVCEN catalog and response interfaces.
use crate::error::{error_with_detail, to_plain_js, type_error};
use crate::gnss::GnssSystem;
use sidereon_core::data::{
    self, CelestrakGpFormat as F, CelestrakGpQuery as Q, CelestrakGpSpecial as S,
};
use wasm_bindgen::prelude::*;

/// GP response encoding.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub enum CelestrakGpFormat {
    /// Named three-line TLE records.
    Tle,
    /// Three-line element records with a prefixed name.
    ThreeLine,
    /// Two-line element records.
    TwoLine,
    /// CCSDS XML.
    Xml,
    /// CCSDS keyword-value notation.
    Kvn,
    /// Compact GP JSON.
    Json,
    /// Formatted GP JSON.
    JsonPretty,
    /// GP CSV.
    Csv,
}
impl From<CelestrakGpFormat> for F {
    fn from(v: CelestrakGpFormat) -> Self {
        match v {
            CelestrakGpFormat::Tle => F::Tle,
            CelestrakGpFormat::ThreeLine => F::ThreeLine,
            CelestrakGpFormat::TwoLine => F::TwoLine,
            CelestrakGpFormat::Xml => F::Xml,
            CelestrakGpFormat::Kvn => F::Kvn,
            CelestrakGpFormat::Json => F::Json,
            CelestrakGpFormat::JsonPretty => F::JsonPretty,
            CelestrakGpFormat::Csv => F::Csv,
        }
    }
}
/// GP special selector.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub enum CelestrakGpSpecial {
    /// GEO protected zone.
    Gpz,
    /// Extended GEO protected zone.
    GpzPlus,
    /// Potentially decaying objects.
    Decaying,
}
impl From<CelestrakGpSpecial> for S {
    fn from(v: CelestrakGpSpecial) -> Self {
        match v {
            CelestrakGpSpecial::Gpz => S::Gpz,
            CelestrakGpSpecial::GpzPlus => S::GpzPlus,
            CelestrakGpSpecial::Decaying => S::Decaying,
        }
    }
}
/// Wire spelling of a GP format.
#[wasm_bindgen(js_name = celestrakGpFormatString)]
pub fn format_string(format: CelestrakGpFormat) -> String {
    F::from(format).as_str().into()
}
/// Wire spelling of a special selector.
#[wasm_bindgen(js_name = celestrakGpSpecialString)]
pub fn special_string(special: CelestrakGpSpecial) -> String {
    S::from(special).as_str().into()
}
fn catalog_error(e: data::DataCatalogError) -> JsValue {
    error_with_detail(
        "DataCatalogError",
        &e.to_string(),
        &crate::domain_error::catalog_error(&e),
    )
}
/// CelesTrak GP endpoint metadata.
#[wasm_bindgen(js_name = celestrakGpSourceEntry, unchecked_return_type = "CelestrakGpSourceEntry")]
pub fn gp_source() -> Result<JsValue, JsValue> {
    let e = data::celestrak_gp_source_entry();
    to_plain_js(
        &serde_json::json!({"protocol":e.protocol.as_str(),"host":e.host,"rootUrl":e.root_url}),
        "source",
    )
}
/// NAVCEN endpoint metadata.
#[wasm_bindgen(js_name = navcenSourceEntry, unchecked_return_type = "NavcenSourceEntry")]
pub fn navcen_source() -> Result<JsValue, JsValue> {
    let e = data::navcen_source_entry();
    to_plain_js(
        &serde_json::json!({"protocol":e.protocol.as_str(),"host":e.host,"rootUrl":e.root_url}),
        "source",
    )
}
/// NAVCEN constellation page.
#[wasm_bindgen(js_name = navcenConstellationUrl)]
pub fn navcen_url() -> String {
    data::navcen_constellation_url().into()
}
/// Catalog host allow-list.
#[wasm_bindgen(js_name = allowedHosts)]
pub fn allowed_hosts() -> Vec<String> {
    data::allowed_hosts().iter().map(|s| (*s).into()).collect()
}
/// CelesTrak group for a GNSS constellation.
#[wasm_bindgen(js_name = celestrakGroup)]
pub fn group(system: GnssSystem) -> String {
    sidereon_core::constellation::celestrak_group(system.into()).into()
}
/// CelesTrak GP URL for a GNSS constellation.
#[wasm_bindgen(js_name = celestrakConstellationUrl)]
pub fn constellation_url(system: GnssSystem, format: CelestrakGpFormat) -> Result<String, JsValue> {
    data::celestrak_constellation_url(system.into(), format.into()).map_err(catalog_error)
}
/// Build a GP URL from a selector object with `kind` and `value`.
#[wasm_bindgen(js_name = celestrakGpUrl)]
pub fn gp_url(
    #[wasm_bindgen(unchecked_param_type = "CelestrakGpQuery")] query: JsValue,
    format: CelestrakGpFormat,
) -> Result<String, JsValue> {
    let kind = js_sys::Reflect::get(&query, &JsValue::from_str("kind"))?
        .as_string()
        .ok_or_else(|| type_error("query.kind must be a string"))?;
    let value = js_sys::Reflect::get(&query, &JsValue::from_str("value"))?;
    let text = value.as_string();
    let text = || {
        text.as_deref()
            .ok_or_else(|| type_error("query.value must be a string"))
    };
    let query = match kind.as_str() {
        "group" => Q::Group(text()?),
        "internationalDesignator" => Q::InternationalDesignator(text()?),
        "name" => Q::Name(text()?),
        "special" => {
            let number = value
                .as_f64()
                .ok_or_else(|| type_error("special selector must be a number"))?;
            Q::Special(match number {
                n if n == CelestrakGpSpecial::Gpz as u32 as f64 => S::from(CelestrakGpSpecial::Gpz),
                n if n == CelestrakGpSpecial::GpzPlus as u32 as f64 => {
                    S::from(CelestrakGpSpecial::GpzPlus)
                }
                n if n == CelestrakGpSpecial::Decaying as u32 as f64 => {
                    S::from(CelestrakGpSpecial::Decaying)
                }
                _ => {
                    let url = data::celestrak_gp_url(Q::Special(S::Gpz), F::Tle)
                        .map_err(catalog_error)?;
                    let selector = url
                        .split_once('?')
                        .and_then(|(_, query)| query.split_once('='))
                        .map(|(key, _)| key)
                        .ok_or_else(|| type_error("invalid core GP URL"))?;
                    let value: String = js_sys::Number::from(number)
                        .to_string_with_radix(10)?
                        .into();
                    let boundary = data::celestrak_gp_url(Q::CatalogNumber(0), F::Tle)
                        .expect_err("zero catalog number is invalid");
                    let data::DataCatalogError::InvalidCelestrakGpSelector {
                        selector: core_selector,
                        ..
                    } = boundary
                    else {
                        return Err(catalog_error(boundary));
                    };
                    let message = data::DataCatalogError::InvalidCelestrakGpSelector {
                        selector: core_selector,
                        value: value.clone(),
                    }
                    .to_string()
                    .replacen(core_selector, selector, 1);
                    return Err(error_with_detail(
                        "DataCatalogError",
                        &message,
                        &serde_json::json!({"kind":"invalidCelestrakGpSelector", "selector":selector, "value":value}),
                    ));
                }
            })
        }
        "catalogNumber" => {
            let number = value
                .as_f64()
                .ok_or_else(|| type_error("catalog number must be a number"))?;
            if !number.is_finite()
                || number.fract() != 0.0
                || !(0.0..=u32::MAX as f64).contains(&number)
            {
                let error = data::celestrak_gp_url(Q::CatalogNumber(0), F::Tle)
                    .expect_err("zero catalog number is invalid");
                let data::DataCatalogError::InvalidCelestrakGpSelector { selector, .. } = error
                else {
                    return Err(catalog_error(error));
                };
                return Err(catalog_error(
                    data::DataCatalogError::InvalidCelestrakGpSelector {
                        selector,
                        value: js_sys::Number::from(number)
                            .to_string_with_radix(10)?
                            .into(),
                    },
                ));
            }
            Q::CatalogNumber(number as u32)
        }
        _ => return Err(type_error("invalid GP selector kind")),
    };
    data::celestrak_gp_url(query, format.into()).map_err(catalog_error)
}

/// Parsed CelesTrak response with accepted and rejected records.
#[wasm_bindgen]
pub struct CelestrakGpResponse {
    inner: data::CelestrakGpResponse,
}
#[wasm_bindgen]
impl CelestrakGpResponse {
    /// Accepted records in source order. Each access clones all records into a fresh array; retain the array when iterating.
    #[wasm_bindgen(getter)]
    pub fn records(&self) -> Vec<CelestrakGpRecord> {
        self.inner
            .records
            .iter()
            .cloned()
            .map(|inner| CelestrakGpRecord { inner })
            .collect()
    }
    /// Rejected records in source order. Each access clones all rejections into a fresh array; retain the array when iterating.
    #[wasm_bindgen(getter)]
    pub fn rejected(&self) -> Vec<CelestrakGpRejectedRecord> {
        self.inner
            .rejected
            .iter()
            .cloned()
            .map(|inner| CelestrakGpRejectedRecord { inner })
            .collect()
    }
    /// Compare response values.
    pub fn equals(&self, other: &CelestrakGpResponse) -> bool {
        self.inner == other.inner
    }
}
/// Parse a complete GP response from its exact bytes.
#[wasm_bindgen(js_name = parseCelestrakGpResponse)]
pub fn parse_response(
    bytes: &[u8],
    format: CelestrakGpFormat,
) -> Result<CelestrakGpResponse, JsValue> {
    data::parse_celestrak_gp_response(bytes, format.into())
        .map(|inner| CelestrakGpResponse { inner })
        .map_err(catalog_error)
}
/// Accepted GP record, retaining native source data.
#[wasm_bindgen]
pub struct CelestrakGpRecord {
    inner: data::CelestrakGpRecord,
}
#[wasm_bindgen]
impl CelestrakGpRecord {
    /// One-based record index.
    #[wasm_bindgen(getter, js_name = recordIndex)]
    pub fn record_index(&self) -> usize {
        self.inner.record_index
    }
    /// One-based source line, when known.
    #[wasm_bindgen(getter, js_name = lineNumber)]
    pub fn line_number(&self) -> Option<usize> {
        self.inner.line_number
    }
    /// Exact source bytes assigned to the record.
    #[wasm_bindgen(getter)]
    pub fn raw(&self) -> Vec<u8> {
        self.inner.raw.as_bytes().to_vec()
    }
    /// Object name, when provided.
    #[wasm_bindgen(getter, js_name = objectName)]
    pub fn object_name(&self) -> Option<String> {
        self.inner.object_name.clone()
    }
    /// Native TLE or OMM payload.
    #[wasm_bindgen(getter, unchecked_return_type = "CelestrakTleRecord | Omm")]
    pub fn payload(&self) -> JsValue {
        match &self.inner.payload {
            data::CelestrakGpPayload::Tle(record) => CelestrakTleRecord {
                inner: (**record).clone(),
            }
            .into(),
            data::CelestrakGpPayload::Omm(record) => {
                crate::omm::Omm::from_core((**record).clone()).into()
            }
        }
    }
    /// Convert to a canonical OMM.
    #[wasm_bindgen(js_name = toOmm)]
    pub fn to_omm(&self) -> Result<crate::omm::Omm, JsValue> {
        self.inner
            .to_omm()
            .map(crate::omm::Omm::from_core)
            .map_err(record_error)
    }
    /// Convert to the core SGP4 element set.
    #[wasm_bindgen(js_name = toElementSet, unchecked_return_type = "ElementSet")]
    pub fn to_element_set(&self) -> Result<JsValue, JsValue> {
        let value = self.inner.to_element_set().map_err(record_error)?;
        to_plain_js(
            &serde_json::json!({
                "epoch": [value.epoch.0, value.epoch.1], "bstar": value.bstar,
                "meanMotionDot": value.mean_motion_dot, "meanMotionDoubleDot": value.mean_motion_double_dot,
                "eccentricity": value.eccentricity, "argumentOfPerigeeDeg": value.argument_of_perigee_deg,
                "inclinationDeg": value.inclination_deg, "meanAnomalyDeg": value.mean_anomaly_deg,
                "meanMotionRevPerDay": value.mean_motion_rev_per_day, "rightAscensionDeg": value.right_ascension_deg,
                "catalogNumber": value.catalog_number, "ommEpochDays": value.omm_epoch_days
            }),
            "element set",
        )
    }
    /// Compare record values.
    pub fn equals(&self, other: &CelestrakGpRecord) -> bool {
        self.inner == other.inner
    }
}
fn record_error(e: data::CelestrakGpRecordError) -> JsValue {
    match e {
        data::CelestrakGpRecordError::Tle(e) => crate::sgp4_error::tle_error(e),
        data::CelestrakGpRecordError::Omm(e) => crate::ndm_error::omm_error(e),
    }
}
/// TLE payload, with fixed-column values and source lines.
#[wasm_bindgen]
pub struct CelestrakTleRecord {
    inner: data::CelestrakTleRecord,
}
#[wasm_bindgen]
impl CelestrakTleRecord {
    /// Original first line.
    #[wasm_bindgen(getter)]
    pub fn line1(&self) -> String {
        self.inner.line1.clone()
    }
    /// Original second line.
    #[wasm_bindgen(getter)]
    pub fn line2(&self) -> String {
        self.inner.line2.clone()
    }
    /// Checksum findings accepted by the reader.
    #[wasm_bindgen(getter, js_name = checksumWarnings)]
    pub fn checksum_warnings(&self) -> Vec<crate::sgp4::ChecksumWarning> {
        self.inner
            .checksum_warnings
            .iter()
            .map(Into::into)
            .collect()
    }
    /// Parsed fixed-column fields.
    #[wasm_bindgen(getter, unchecked_return_type = "TleElements")]
    pub fn elements(&self) -> Result<JsValue, JsValue> {
        tle_elements_value(&self.inner.elements)
    }
    /// Compare TLE record values.
    pub fn equals(&self, other: &CelestrakTleRecord) -> bool {
        self.inner == other.inner
    }
}
pub(crate) fn tle_elements_value(
    e: &sidereon_core::astro::tle::TleElements,
) -> Result<JsValue, JsValue> {
    to_plain_js(
        &serde_json::json!({"catalogNumber":e.catalog_number,"classification":e.classification,"internationalDesignator":e.international_designator,"epochYear":e.epoch_year,"epochDayOfYear":e.epoch_day_of_year,"meanMotionDot":e.mean_motion_dot,"meanMotionDoubleDot":e.mean_motion_double_dot,"meanMotionDoubleDotText":e.mean_motion_double_dot_text,"bstar":e.bstar,"bstarText":e.bstar_text,"ephemerisType":e.ephemeris_type,"elsetNumber":e.elset_number,"inclinationDeg":e.inclination_deg,"raanDeg":e.raan_deg,"eccentricity":e.eccentricity,"argPerigeeDeg":e.arg_perigee_deg,"meanAnomalyDeg":e.mean_anomaly_deg,"meanMotion":e.mean_motion,"revNumber":e.rev_number}),
        "TLE elements",
    )
}
/// Rejected source record and its typed reason.
#[wasm_bindgen]
pub struct CelestrakGpRejectedRecord {
    inner: data::CelestrakGpRejectedRecord,
}
#[wasm_bindgen]
impl CelestrakGpRejectedRecord {
    /// One-based record index.
    #[wasm_bindgen(getter, js_name = recordIndex)]
    pub fn record_index(&self) -> usize {
        self.inner.record_index
    }
    /// One-based source line, when known.
    #[wasm_bindgen(getter, js_name = lineNumber)]
    pub fn line_number(&self) -> Option<usize> {
        self.inner.line_number
    }
    /// Exact source bytes assigned to this rejection.
    #[wasm_bindgen(getter)]
    pub fn raw(&self) -> Vec<u8> {
        self.inner.raw.as_bytes().to_vec()
    }
    /// Typed rejection and its details.
    #[wasm_bindgen(getter)]
    pub fn reason(&self) -> CelestrakGpRejectionReason {
        CelestrakGpRejectionReason {
            inner: self.inner.reason.clone(),
        }
    }
    /// Compare rejection values.
    pub fn equals(&self, other: &CelestrakGpRejectedRecord) -> bool {
        self.inner == other.inner
    }
}

/// A typed GP rejection family and its nested cause.
#[wasm_bindgen]
pub struct CelestrakGpRejectionReason {
    inner: data::CelestrakGpRejectionReason,
}
#[wasm_bindgen]
impl CelestrakGpRejectionReason {
    /// Native rejection family.
    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> String {
        match &self.inner {
            data::CelestrakGpRejectionReason::Tle(_) => "tle",
            data::CelestrakGpRejectionReason::Omm(_) => "omm",
            data::CelestrakGpRejectionReason::Structural(_) => "structural",
        }
        .into()
    }
    /// Structured error detail, including nested causes.
    #[wasm_bindgen(getter, unchecked_return_type = "CelestrakGpRejectionDetail")]
    pub fn detail(&self) -> Result<JsValue, JsValue> {
        use data::{CelestrakGpRejectionReason as R, CelestrakGpStructuralError as E};
        let value = match &self.inner {
            R::Tle(e) => {
                serde_json::json!({"message":e.to_string(),"cause":crate::sgp4_error::tle_cause(e)})
            }
            R::Omm(e) => {
                serde_json::json!({"message":e.to_string(),"cause":crate::ndm_error::omm_detail(e)})
            }
            R::Structural(e) => match e {
                E::UnexpectedContent => serde_json::json!({"kind":"unexpectedContent"}),
                E::IncompleteRecord => serde_json::json!({"kind":"incompleteRecord"}),
                E::InvalidTleStructure => serde_json::json!({"kind":"invalidTleStructure"}),
                E::InvalidElementSet(e) => {
                    serde_json::json!({"kind":"invalidElementSet","message":e.to_string(),"cause":crate::sgp4_error::sgp4_cause(e)})
                }
            },
        };
        to_plain_js(&value, "rejection detail")
    }
    /// Compare rejection reasons by value.
    pub fn equals(&self, other: &CelestrakGpRejectionReason) -> bool {
        self.inner == other.inner
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TleElementsInput {
    catalog_number: String,
    classification: String,
    international_designator: String,
    epoch_year: f64,
    epoch_day_of_year: f64,
    mean_motion_dot: f64,
    mean_motion_double_dot: f64,
    mean_motion_double_dot_text: Option<String>,
    bstar: f64,
    bstar_text: Option<String>,
    ephemeris_type: Option<i32>,
    elset_number: Option<i32>,
    inclination_deg: f64,
    raan_deg: f64,
    eccentricity: f64,
    arg_perigee_deg: f64,
    mean_anomaly_deg: f64,
    mean_motion: f64,
    rev_number: Option<i32>,
}

pub(crate) fn tle_elements(
    value: JsValue,
) -> Result<sidereon_core::astro::tle::TleElements, JsValue> {
    let e: TleElementsInput = serde_wasm_bindgen::from_value(value)
        .map_err(|error| type_error(&format!("invalid TLE elements: {error}")))?;
    if !e.epoch_year.is_finite()
        || e.epoch_year.fract() != 0.0
        || e.epoch_year < i32::MIN as f64
        || e.epoch_year > i32::MAX as f64
    {
        return Err(crate::sgp4_error::tle_error(
            sidereon_core::astro::tle::TleError::InvalidField {
                field: "epoch_year",
                reason: "expected an integer in i32 range",
            },
        ));
    }
    Ok(sidereon_core::astro::tle::TleElements {
        catalog_number: e.catalog_number,
        classification: e.classification,
        international_designator: e.international_designator,
        epoch_year: e.epoch_year as i32,
        epoch_day_of_year: e.epoch_day_of_year,
        mean_motion_dot: e.mean_motion_dot,
        mean_motion_double_dot: e.mean_motion_double_dot,
        mean_motion_double_dot_text: e.mean_motion_double_dot_text,
        bstar: e.bstar,
        bstar_text: e.bstar_text,
        ephemeris_type: e.ephemeris_type,
        elset_number: e.elset_number,
        inclination_deg: e.inclination_deg,
        raan_deg: e.raan_deg,
        eccentricity: e.eccentricity,
        arg_perigee_deg: e.arg_perigee_deg,
        mean_anomaly_deg: e.mean_anomaly_deg,
        mean_motion: e.mean_motion,
        rev_number: e.rev_number,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omm_payload_conversion_retains_the_core_error() {
        let mut omm =
            sidereon_core::astro::omm::parse_json(include_str!("../test/fixtures/omm/25544.json"))
                .unwrap();
        omm.mean_motion = None;
        let record = data::CelestrakGpRecord {
            record_index: 1,
            line_number: Some(1),
            raw: String::new(),
            object_name: Some("ISS (ZARYA)".to_owned()),
            payload: data::CelestrakGpPayload::Omm(Box::new(omm)),
        };
        let error = record.to_element_set().unwrap_err();
        let data::CelestrakGpRecordError::Omm(error) = error else {
            panic!("expected OMM error");
        };
        assert_eq!(
            serde_json::to_value(crate::ndm_error::omm_detail(&error)).unwrap(),
            serde_json::json!({"kind":"MISSING_FIELD","format":null,"byteOffset":null,
                "fields":null,"message":"OMM missing required field MEAN_MOTION",
                "field":"MEAN_MOTION","value":null,"issue":null,"line":null,
                "unit":null,"expectedUnit":null,"first":null,"second":null,
                "count":null,"expectedCount":null,"index":null,"source":null})
        );
    }
}
