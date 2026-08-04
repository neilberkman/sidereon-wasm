//! Pure GNSS product identity and public-distributor derivation.
//!
//! Browser and Node callers own fetch, credentials, and cache policy. This
//! module only delegates exact catalog derivation to the network-free core.

use serde::Serialize;
use sidereon_core::data::{
    self as core_data, AnalysisCenter, DistributionSource, ProductDate, ProductIdentity,
    ProductType, Sp3ContentStartConvention as CoreSp3ContentStartConvention,
};
use sidereon_core::exact_cache::{build_commit_record, verify_commit_record};
use wasm_bindgen::prelude::*;

use crate::error::{engine_error, type_error};

/// Cataloged relationship between an SP3 filename epoch and its first content
/// epoch.
///
/// New values may be appended in later interface releases.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sp3ContentStartConvention {
    /// The first content epoch equals the filename epoch.
    FilenameEpoch = 0,
    /// The first content epoch is exactly 24 hours before the filename epoch.
    FilenameEpochMinusOneDay = 1,
}

fn content_start_from_core(
    value: CoreSp3ContentStartConvention,
) -> Result<Sp3ContentStartConvention, JsValue> {
    match value {
        CoreSp3ContentStartConvention::FilenameEpoch => {
            Ok(Sp3ContentStartConvention::FilenameEpoch)
        }
        CoreSp3ContentStartConvention::FilenameEpochMinusOneDay => {
            Ok(Sp3ContentStartConvention::FilenameEpochMinusOneDay)
        }
        // The core enum is non-exhaustive. A WASM release must add an explicit
        // public value before exposing a future convention.
        _ => Err(engine_error(
            "the core returned a content-start convention not exposed by this WASM interface",
        )),
    }
}

fn content_start_to_core(value: Sp3ContentStartConvention) -> CoreSp3ContentStartConvention {
    match value {
        Sp3ContentStartConvention::FilenameEpoch => CoreSp3ContentStartConvention::FilenameEpoch,
        Sp3ContentStartConvention::FilenameEpochMinusOneDay => {
            CoreSp3ContentStartConvention::FilenameEpochMinusOneDay
        }
    }
}

/// Exact public GNSS product identity, independent of distributor.
#[wasm_bindgen]
pub struct GnssProductIdentity {
    pub(crate) inner: ProductIdentity,
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PredictedLineCandidate {
    center: String,
    date: String,
    sample: String,
    issue: Option<String>,
    filename: String,
    url: String,
}

/// Ordered cross-line candidates for one predicted IONEX map date.
///
/// Both CODE predicted lines publish the same official filename for a map
/// date, but the two-day line is produced a day earlier, so `cod_prd2` is
/// routinely published while `cod_prd1` is still absent when CODE runs
/// behind. Candidates are ordered `cod_prd1` first, all cover the SAME map
/// date (never a neighboring day's map), and each keeps its own line
/// identity so resolved provenance names the line actually served. The walk
/// is opt-in; single-line requests keep their fail-closed behavior.
///
/// Returns an array of `{center, date, sample, issue, filename, url}`.
#[wasm_bindgen(js_name = predictedIonexLineCandidates)]
pub fn predicted_ionex_line_candidates(
    year: i32,
    month: u8,
    day: u8,
    sample: Option<String>,
) -> Result<JsValue, JsValue> {
    let date = ProductDate::new(year, month, day).map_err(engine_error)?;
    let candidates = core_data::predicted_ionex_line_candidates(date, sample.as_deref())
        .map_err(engine_error)?;
    let mut rows = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        rows.push(PredictedLineCandidate {
            center: candidate.center.code().to_owned(),
            date: format!(
                "{:04}-{:02}-{:02}",
                candidate.date.year, candidate.date.month, candidate.date.day
            ),
            sample: candidate.sample.clone(),
            issue: candidate.issue.clone(),
            filename: candidate.canonical_filename().map_err(engine_error)?,
            url: candidate.archive_url().map_err(engine_error)?,
        });
    }
    serde_wasm_bindgen::to_value(&rows).map_err(|error| engine_error(error.to_string()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PublishedObjectRow {
    path: String,
    observed_at: Option<String>,
}

/// Parse the object entries out of an archive listing body.
///
/// Dialect detection is closed: a body that fits none of the recognized
/// listing surfaces (Apache/XHTML autoindex, AIUB whole-tree CSV, FTP
/// `LIST`) throws instead of returning a best-effort empty result - a silent
/// empty parse would be indistinguishable from "nothing published".
/// `observedAt` is the archive-reported modification text, verbatim; archives
/// disagree on format and time zone, so it is never reinterpreted.
///
/// Returns an array of `{path, observedAt}`.
#[wasm_bindgen(js_name = parseArchiveListing)]
pub fn parse_archive_listing(body: &str) -> Result<JsValue, JsValue> {
    let objects = core_data::parse_archive_listing(body).map_err(engine_error)?;
    let rows: Vec<PublishedObjectRow> = objects
        .into_iter()
        .map(|object| PublishedObjectRow {
            path: object.path,
            observed_at: object.observed_at,
        })
        .collect();
    serde_wasm_bindgen::to_value(&rows).map_err(|error| engine_error(error.to_string()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NewestPublishedRow {
    date: String,
    issue: String,
    filename: String,
    observed_at: Option<String>,
}

/// Newest published issue for one center + product family within a listing
/// body, or `null` when the listing is readable but holds no object of the
/// line - deliberately distinct from an unreadable listing, which throws.
///
/// Returns `{date, issue, filename, observedAt}` or `null`.
#[wasm_bindgen(js_name = newestPublishedProduct)]
pub fn newest_published_product(
    center: &str,
    family: &str,
    listing_body: &str,
) -> Result<JsValue, JsValue> {
    let objects = core_data::parse_archive_listing(listing_body).map_err(engine_error)?;
    let newest = core_data::newest_published_product(
        analysis_center(center)?,
        product_type(family)?,
        &objects,
    )
    .map_err(engine_error)?;
    match newest {
        None => Ok(JsValue::NULL),
        Some(product) => {
            let row = NewestPublishedRow {
                date: format!(
                    "{:04}-{:02}-{:02}",
                    product.date.year, product.date.month, product.date.day
                ),
                issue: product.issue,
                filename: product.filename,
                observed_at: product.observed_at,
            };
            serde_wasm_bindgen::to_value(&row).map_err(|error| engine_error(error.to_string()))
        }
    }
}

/// Bounded archive listing URLs answering "newest published issue" for one
/// center + product family: at most two URLs, newest directory first (or one
/// whole-tree listing); never a polling loop. Browser and Node callers own
/// the fetch itself.
#[wasm_bindgen(js_name = publicationListingUrls)]
pub fn publication_listing_urls(
    center: &str,
    family: &str,
    year: i32,
    month: u8,
    day: u8,
) -> Result<Vec<String>, JsValue> {
    let date = ProductDate::new(year, month, day).map_err(engine_error)?;
    core_data::publication_listing_urls(analysis_center(center)?, product_type(family)?, date)
        .map_err(engine_error)
}

/// Whole minutes from a published issue's nominal epoch to a caller-supplied
/// UTC instant - the "N hours behind nominal" lag number. The verbatim
/// `observedAt` text carries the archive's own modification claim where one
/// exists.
#[wasm_bindgen(js_name = publishedIssueAgeMinutes)]
#[allow(clippy::too_many_arguments)]
pub fn published_issue_age_minutes(
    year: i32,
    month: u8,
    day: u8,
    issue: &str,
    filename: &str,
    now_year: i32,
    now_month: u8,
    now_day: u8,
    now_hour: u8,
    now_minute: u8,
    now_second: u8,
) -> Result<i64, JsValue> {
    let published = core_data::PublishedProduct {
        date: ProductDate::new(year, month, day).map_err(engine_error)?,
        issue: issue.to_owned(),
        filename: filename.to_owned(),
        observed_at: None,
    };
    let now = core_data::ProductDateTime::new(
        ProductDate::new(now_year, now_month, now_day).map_err(engine_error)?,
        now_hour,
        now_minute,
        now_second,
    )
    .map_err(engine_error)?;
    core_data::published_issue_age_minutes(&published, now).map_err(engine_error)
}

/// Index of the first cross-line predicted-IONEX candidate whose exact
/// archive object appears in a listing body, or `null` when neither line is
/// published. Candidates stay in preference order and keep their own
/// identities, so the resolved index preserves the line actually served in
/// provenance.
#[wasm_bindgen(js_name = resolveFirstPublishedPredictedIonex)]
pub fn resolve_first_published_predicted_ionex(
    year: i32,
    month: u8,
    day: u8,
    sample: Option<String>,
    listing_body: &str,
) -> Result<Option<usize>, JsValue> {
    let date = ProductDate::new(year, month, day).map_err(engine_error)?;
    let candidates = core_data::predicted_ionex_line_candidates(date, sample.as_deref())
        .map_err(engine_error)?;
    let objects = core_data::parse_archive_listing(listing_body).map_err(engine_error)?;
    core_data::resolve_first_published(&candidates, &objects).map_err(engine_error)
}

/// Return the catalog solution class for one center/product family.
///
/// This product-aware query distinguishes IGS combined final SP3 (`final`)
/// from IGS broadcast navigation (`broadcast`).
#[wasm_bindgen(js_name = productSolutionClass)]
pub fn product_solution_class(center: &str, family: &str) -> Result<String, JsValue> {
    core_data::product_solution_class(analysis_center(center)?, product_type(family)?)
        .map(|solution| solution.code().to_owned())
        .map_err(engine_error)
}

/// Return the officially cataloged default sampling token for a product date.
///
/// Unlike the legacy date-free default, this preserves historical cadence
/// changes such as the GFZ rapid and ultra-rapid transitions. On ESA
/// ultra-rapid's issue-level transition date this reports the `0000`/
/// start-of-day default; [`productIdentity`](product_identity) also considers
/// the requested issue.
#[wasm_bindgen(js_name = defaultSampleForDate)]
pub fn default_sample_for_date(
    center: &str,
    family: &str,
    year: i32,
    month: u8,
    day: u8,
) -> Result<String, JsValue> {
    let date = ProductDate::new(year, month, day).map_err(engine_error)?;
    core_data::default_sample_for_date(analysis_center(center)?, product_type(family)?, date)
        .map(str::to_owned)
        .map_err(engine_error)
}

/// Return every officially cataloged sampling token for a product date and
/// issue.
///
/// Syntax alone is not publication evidence: this is the complete catalog set
/// enforced by [`productIdentity`](product_identity). For issue-based product
/// lines, omitting `issue` selects `0000`, matching
/// [`defaultSampleForDate`](default_sample_for_date). Product construction
/// itself still requires an explicit issue.
#[wasm_bindgen(js_name = supportedSamples)]
pub fn supported_samples(
    center: &str,
    family: &str,
    year: i32,
    month: u8,
    day: u8,
    issue: Option<String>,
) -> Result<Vec<String>, JsValue> {
    let date = ProductDate::new(year, month, day).map_err(engine_error)?;
    core_data::supported_samples(
        analysis_center(center)?,
        product_type(family)?,
        date,
        issue.as_deref(),
    )
    .map(|samples| samples.iter().map(|sample| (*sample).to_owned()).collect())
    .map_err(engine_error)
}

/// Return the cataloged relationship between an SP3 filename epoch and its
/// first content epoch.
///
/// `issue` is required for ultra-rapid centers, must name a published issue,
/// and must be omitted for product lines without issue times. Exact requests
/// built with `ExactSp3Request.fromIdentity` apply this same catalog fact.
#[wasm_bindgen(js_name = sp3ContentStartConvention)]
pub fn sp3_content_start_convention(
    center: &str,
    year: i32,
    month: u8,
    day: u8,
    issue: Option<String>,
) -> Result<Sp3ContentStartConvention, JsValue> {
    let date = ProductDate::new(year, month, day).map_err(engine_error)?;
    core_data::sp3_content_start_convention(analysis_center(center)?, date, issue.as_deref())
        .map_err(engine_error)
        .and_then(content_start_from_core)
}

/// Return the signed whole seconds added to the filename epoch for a public
/// content-start convention.
///
/// JavaScript receives the `i64` result as a `bigint`, consistent with other
/// exact integral values in this interface.
#[wasm_bindgen(js_name = sp3ContentStartOffsetSeconds)]
pub fn sp3_content_start_offset_seconds(convention: Sp3ContentStartConvention) -> i64 {
    content_start_to_core(convention).content_start_offset_s()
}

/// Resolve an exact catalog product independently from distributor. When the
/// sample is omitted, ultra-rapid issue-level cadence transitions are applied.
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
