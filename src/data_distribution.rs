//! Pure GNSS product identity and public-distributor derivation.
//!
//! Browser and Node callers own fetch, credentials, and cache policy. This
//! module only delegates exact catalog derivation to the network-free core.

use sidereon_core::data::{
    self as core_data, AnalysisCenter, DistributionSource, ProductDate, ProductIdentity,
    ProductType,
};
use sidereon_core::exact_cache::{build_commit_record, verify_commit_record};
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

    #[wasm_bindgen(getter, js_name = analysisCenter)]
    pub fn analysis_center(&self) -> String {
        self.inner.analysis_center.code().to_owned()
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

    #[wasm_bindgen(getter, js_name = formatVersion)]
    pub fn format_version(&self) -> Option<String> {
        self.inner.format_version.clone()
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

/// Build the shared exact-cache commit bytes for a complete immutable candidate.
///
/// Browser hosts should stage product, archive, and provenance under `entry_id`
/// and make the returned marker visible in the same IndexedDB transaction.
#[wasm_bindgen(js_name = buildExactCacheCommit)]
pub fn build_exact_cache_commit(
    identity: &GnssProductIdentity,
    source: &str,
    entry_id: &str,
    product: &[u8],
    archive: &[u8],
    provenance: &[u8],
) -> Result<Vec<u8>, JsValue> {
    build_commit_record(
        &identity.inner,
        distribution_source(source)?,
        entry_id,
        product,
        archive,
        provenance,
    )
    .map_err(engine_error)
}

/// Verify a shared commit marker against the requested full identity, source,
/// and all three immutable byte objects. Returns the committed entry id.
#[wasm_bindgen(js_name = verifyExactCacheCommit)]
pub fn verify_exact_cache_commit(
    identity: &GnssProductIdentity,
    source: &str,
    marker: &[u8],
    product: &[u8],
    archive: &[u8],
    provenance: &[u8],
) -> Result<String, JsValue> {
    verify_commit_record(
        &identity.inner,
        distribution_source(source)?,
        marker,
        product,
        archive,
        provenance,
    )
    .map(|verified| verified.entry_id)
    .map_err(engine_error)
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

/// Require available identities to be exactly the declared product set.
///
/// The expected set must be non-empty. Both lists reject duplicates; missing
/// and undeclared identities fail. Comparison includes every identity field,
/// not only the official filename. SP3 observed/predicted timing comes from
/// `Sp3.predictionSummary()`, not catalog fields or issue times.
#[wasm_bindgen]
pub struct GnssExactProductSet {
    expected: Vec<ProductIdentity>,
    available: Vec<ProductIdentity>,
}

#[wasm_bindgen]
impl GnssExactProductSet {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            expected: Vec::new(),
            available: Vec::new(),
        }
    }

    #[wasm_bindgen(js_name = addExpected)]
    pub fn add_expected(&mut self, identity: &GnssProductIdentity) {
        self.expected.push(identity.inner.clone());
    }

    #[wasm_bindgen(js_name = addAvailable)]
    pub fn add_available(&mut self, identity: &GnssProductIdentity) {
        self.available.push(identity.inner.clone());
    }

    #[wasm_bindgen(getter, js_name = expectedCount)]
    pub fn expected_count(&self) -> usize {
        self.expected.len()
    }

    #[wasm_bindgen(getter, js_name = availableCount)]
    pub fn available_count(&self) -> usize {
        self.available.len()
    }

    pub fn validate(&self) -> Result<(), JsValue> {
        core_data::validate_exact_product_set(&self.expected, &self.available).map_err(engine_error)
    }
}

impl Default for GnssExactProductSet {
    fn default() -> Self {
        Self::new()
    }
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
