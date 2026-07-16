//! Pure GNSS product identity and public-distributor derivation.
//!
//! Browser and Node callers own fetch, credentials, and cache policy. This
//! module only delegates exact catalog derivation to the network-free core.

use sidereon_core::data::{
    self as core_data, AnalysisCenter, DistributionSource, ProductDate, ProductIdentity,
    ProductType,
};
use wasm_bindgen::prelude::*;

use crate::error::{engine_error, type_error};

/// Exact public GNSS product identity, independent of distributor.
#[wasm_bindgen]
pub struct GnssProductIdentity {
    inner: ProductIdentity,
}

#[wasm_bindgen]
impl GnssProductIdentity {
    #[wasm_bindgen(getter)]
    pub fn family(&self) -> String {
        self.inner.family.code().to_owned()
    }

    #[wasm_bindgen(getter, js_name = publisher)]
    pub fn publisher(&self) -> String {
        self.inner.publisher.code().to_owned()
    }

    #[wasm_bindgen(getter, js_name = solutionClass)]
    pub fn solution_class(&self) -> String {
        self.inner.solution.code().to_owned()
    }

    #[wasm_bindgen(getter)]
    pub fn campaign(&self) -> String {
        self.inner.campaign.code().to_owned()
    }

    #[wasm_bindgen(getter, js_name = filenameVersion)]
    pub fn filename_version(&self) -> u8 {
        self.inner.version
    }

    #[wasm_bindgen(getter)]
    pub fn year(&self) -> i32 {
        self.inner.date.year
    }

    #[wasm_bindgen(getter)]
    pub fn month(&self) -> u8 {
        self.inner.date.month
    }

    #[wasm_bindgen(getter)]
    pub fn day(&self) -> u8 {
        self.inner.date.day
    }

    #[wasm_bindgen(getter)]
    pub fn issue(&self) -> Option<String> {
        self.inner.issue.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn span(&self) -> String {
        self.inner.span.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn sample(&self) -> String {
        self.inner.sample.clone()
    }

    #[wasm_bindgen(getter, js_name = officialFilename)]
    pub fn official_filename(&self) -> String {
        self.inner.official_filename.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn format(&self) -> String {
        self.inner.format.code().to_owned()
    }

    #[wasm_bindgen(getter, js_name = predictionHorizonDays)]
    pub fn prediction_horizon_days(&self) -> Option<u8> {
        self.inner.prediction_horizon_days
    }

    /// Stable validated identity key suitable for caller-managed cache paths.
    #[wasm_bindgen(getter, js_name = cacheKey)]
    pub fn cache_key(&self) -> Result<String, JsValue> {
        self.inner.key().map_err(engine_error)
    }
}

/// Public location and transport metadata for one exact product identity.
#[wasm_bindgen]
pub struct GnssDistributionLocation {
    source: DistributionSource,
    original_url: Option<String>,
    archive_filename: String,
    compression: core_data::ArchiveCompression,
}

#[wasm_bindgen]
impl GnssDistributionLocation {
    #[wasm_bindgen(getter)]
    pub fn source(&self) -> String {
        self.source.code().to_owned()
    }

    #[wasm_bindgen(getter, js_name = originalUrl)]
    pub fn original_url(&self) -> Option<String> {
        self.original_url.clone()
    }

    #[wasm_bindgen(getter, js_name = archiveFilename)]
    pub fn archive_filename(&self) -> String {
        self.archive_filename.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn compression(&self) -> String {
        self.compression.as_str().to_owned()
    }
}

fn product_type(value: &str) -> Result<ProductType, JsValue> {
    ProductType::from_code(value)
        .ok_or_else(|| type_error("family must be sp3, ionex, clk, or nav"))
}

fn analysis_center(value: &str) -> Result<AnalysisCenter, JsValue> {
    AnalysisCenter::from_code(value).ok_or_else(|| type_error("unknown analysis center"))
}

fn distribution_source(value: &str) -> Result<DistributionSource, JsValue> {
    match value {
        "direct" => Ok(DistributionSource::Direct),
        "nasa_cddis" => Ok(DistributionSource::NasaCddis),
        "local_file" => Ok(DistributionSource::LocalFile),
        "in_memory" => Ok(DistributionSource::InMemory),
        _ => Err(type_error(
            "source must be direct, nasa_cddis, local_file, or in_memory",
        )),
    }
}

fn product_spec(
    center: &str,
    family: &str,
    year: i32,
    month: u8,
    day: u8,
    sample: Option<String>,
    issue: Option<String>,
) -> Result<core_data::ProductSpec, JsValue> {
    let center = analysis_center(center)?;
    let family = product_type(family)?;
    let date = ProductDate::new(year, month, day).map_err(engine_error)?;
    core_data::product(center, family, date, sample.as_deref(), issue.as_deref())
        .map_err(engine_error)
}

/// Resolve an exact catalog product independently from distributor.
#[wasm_bindgen(js_name = productIdentity)]
pub fn product_identity(
    center: &str,
    family: &str,
    year: i32,
    month: u8,
    day: u8,
    sample: Option<String>,
    issue: Option<String>,
) -> Result<GnssProductIdentity, JsValue> {
    let inner = product_spec(center, family, year, month, day, sample, issue)?
        .identity()
        .map_err(engine_error)?;
    Ok(GnssProductIdentity { inner })
}

/// Resolve one explicit public distributor without performing network IO.
#[wasm_bindgen(js_name = distributionLocation)]
#[allow(clippy::too_many_arguments)]
pub fn distribution_location(
    center: &str,
    family: &str,
    year: i32,
    month: u8,
    day: u8,
    sample: Option<String>,
    issue: Option<String>,
    source: &str,
) -> Result<GnssDistributionLocation, JsValue> {
    let source = distribution_source(source)?;
    let location = product_spec(center, family, year, month, day, sample, issue)?
        .distribution_location(source)
        .map_err(engine_error)?;
    Ok(GnssDistributionLocation {
        source: location.source,
        original_url: location.original_url,
        archive_filename: location.archive_filename,
        compression: location.compression,
    })
}
