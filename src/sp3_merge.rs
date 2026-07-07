//! Multi-product SP3 merge. Marshals a JS array of parsed SP3 products plus an
//! options object into the core consensus merge (`sidereon_core::ephemeris::merge`)
//! and returns the merged product together with its audit report. The merge math
//! is the engine's, unchanged.

use std::collections::BTreeSet;

use serde::Deserialize;
use wasm_bindgen::prelude::*;

use sidereon_core::astro::time::{Instant, InstantRepr};
use sidereon_core::constants::{J2000_JD, SECONDS_PER_DAY};
use sidereon_core::ephemeris::{
    merge, AgreementMetric, MergeCombine, MergeFlag, MergeOptions, MergeReport, Sp3FrameLabelSet,
    Sp3FrameReconciliation,
};
use sidereon_core::GnssSystem;

use crate::error::{engine_error, range_error, type_error};
use crate::sp3::Sp3;

/// Merge controls. All fields optional; defaults match the core `MergeOptions`
/// (2-of-3 majority agreement, mean combine).
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct MergeOptionsInput {
    position_tolerance_m: Option<f64>,
    clock_tolerance_s: Option<f64>,
    min_agree: Option<usize>,
    clock_min_common: Option<usize>,
    combine: Option<String>,
    target_epoch_interval_s: Option<f64>,
    systems: Option<Vec<String>>,
    asserted_frame_label_sets: Option<Vec<Vec<String>>>,
    helmert: Option<bool>,
}

fn combine_kind(label: &str) -> Result<MergeCombine, JsValue> {
    match label {
        "mean" => Ok(MergeCombine::Mean),
        "median" => Ok(MergeCombine::Median),
        "precedence" => Ok(MergeCombine::Precedence),
        other => Err(type_error(&format!(
            "unknown SP3 merge combine {other:?}: expected \"mean\", \"median\", or \"precedence\""
        ))),
    }
}

fn parse_system(value: &str) -> Result<GnssSystem, JsValue> {
    match value.trim().to_ascii_uppercase().as_str() {
        "G" | "GPS" => Ok(GnssSystem::Gps),
        "R" | "GLO" | "GLONASS" => Ok(GnssSystem::Glonass),
        "E" | "GAL" | "GALILEO" => Ok(GnssSystem::Galileo),
        "C" | "BDS" | "BEIDOU" => Ok(GnssSystem::BeiDou),
        "J" | "QZSS" => Ok(GnssSystem::Qzss),
        "I" | "IRNSS" | "NAVIC" => Ok(GnssSystem::Navic),
        "S" | "SBAS" => Ok(GnssSystem::Sbas),
        other => Err(type_error(&format!(
            "unknown GNSS system {other:?}: expected one of G, R, E, C, J, I, S"
        ))),
    }
}

impl MergeOptionsInput {
    fn to_core(&self) -> Result<MergeOptions, JsValue> {
        let mut opts = MergeOptions::default();
        if let Some(value) = self.position_tolerance_m {
            if !(value.is_finite() && value > 0.0) {
                return Err(range_error(
                    "positionToleranceM must be positive and finite",
                ));
            }
            opts.position_tolerance_m = value;
        }
        if let Some(value) = self.clock_tolerance_s {
            if !(value.is_finite() && value > 0.0) {
                return Err(range_error("clockToleranceS must be positive and finite"));
            }
            opts.clock_tolerance_s = value;
        }
        if let Some(value) = self.min_agree {
            if value == 0 {
                return Err(range_error("minAgree must be at least 1"));
            }
            opts.min_agree = value;
        }
        if let Some(value) = self.clock_min_common {
            if value == 0 {
                return Err(range_error("clockMinCommon must be at least 1"));
            }
            opts.clock_min_common = value;
        }
        if let Some(label) = &self.combine {
            opts.combine = combine_kind(label)?;
        }
        if let Some(value) = self.target_epoch_interval_s {
            if !(value.is_finite() && value > 0.0) {
                return Err(range_error(
                    "targetEpochIntervalS must be positive and finite",
                ));
            }
            opts.target_epoch_interval_s = Some(value);
        }
        if let Some(labels) = &self.systems {
            if labels.is_empty() {
                return Err(type_error("systems must not be empty"));
            }
            let set = labels
                .iter()
                .map(|label| parse_system(label))
                .collect::<Result<BTreeSet<_>, JsValue>>()?;
            opts.systems = Some(set);
        }
        if let Some(label_sets) = &self.asserted_frame_label_sets {
            opts.frame_reconciliation.asserted_equivalent_label_sets =
                parse_asserted_frame_label_sets(label_sets)?;
        }
        opts.frame_reconciliation.helmert = self.helmert.unwrap_or(false);
        Ok(opts)
    }
}

fn parse_asserted_frame_label_sets(
    values: &[Vec<String>],
) -> Result<Vec<Sp3FrameLabelSet>, JsValue> {
    values
        .iter()
        .enumerate()
        .map(|(idx, labels)| {
            if labels.len() < 2 {
                return Err(type_error(&format!(
                    "assertedFrameLabelSets[{idx}] must contain at least two labels"
                )));
            }
            let trimmed = labels
                .iter()
                .map(|label| {
                    let trimmed = label.trim().to_string();
                    if trimmed.is_empty() {
                        Err(type_error(&format!(
                            "assertedFrameLabelSets[{idx}] contains an empty label"
                        )))
                    } else {
                        Ok(trimmed)
                    }
                })
                .collect::<Result<Vec<_>, JsValue>>()?;
            Ok(Sp3FrameLabelSet::new(trimmed))
        })
        .collect()
}

fn instant_to_j2000_seconds(epoch: &Instant) -> f64 {
    match epoch.repr {
        InstantRepr::JulianDate(jd) => ((jd.jd_whole - J2000_JD) + jd.fraction) * SECONDS_PER_DAY,
        InstantRepr::Nanos(_) => f64::NAN,
    }
}

/// Merge SP3 products with the core consensus merge path.
///
/// `sources` is a JS array of parsed [`Sp3`] products, ordered by source
/// precedence; the handles are consumed. `options` is an optional plain object
/// (see the `Sp3MergeOptions` TypeScript type). Returns an [`Sp3MergeResult`]
/// carrying the merged product and the audit report. Throws a `TypeError` for an
/// empty source list or bad options, and an `Error` for incompatible inputs
/// (mismatched time systems or coordinate frames).
#[wasm_bindgen(js_name = mergeSp3)]
pub fn merge_sp3(sources: Vec<Sp3>, options: JsValue) -> Result<Sp3MergeResult, JsValue> {
    if sources.is_empty() {
        return Err(type_error("mergeSp3 requires at least one SP3 product"));
    }

    let opts_input: MergeOptionsInput = if options.is_undefined() || options.is_null() {
        MergeOptionsInput::default()
    } else {
        serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid SP3 merge options: {e}")))?
    };
    let opts = opts_input.to_core()?;

    let core_sources: Vec<_> = sources.into_iter().map(|s| s.inner).collect();
    let (merged, report) = merge(&core_sources, &opts).map_err(engine_error)?;

    Ok(Sp3MergeResult {
        merged,
        report: report.into(),
    })
}

/// The result of an SP3 merge: the merged product plus the audit report.
#[wasm_bindgen]
pub struct Sp3MergeResult {
    merged: sidereon_core::ephemeris::Sp3,
    report: Sp3MergeReport,
}

#[wasm_bindgen]
impl Sp3MergeResult {
    /// The merged precise orbit and clock product.
    #[wasm_bindgen(getter)]
    pub fn sp3(&self) -> Sp3 {
        Sp3 {
            inner: self.merged.clone(),
        }
    }

    /// The merge audit report.
    #[wasm_bindgen(getter)]
    pub fn report(&self) -> Sp3MergeReport {
        self.report.clone()
    }
}

/// One SP3 merge audit flag for an epoch and satellite.
#[wasm_bindgen]
#[derive(Clone)]
pub struct Sp3MergeFlag {
    epoch_j2000_seconds: f64,
    satellite: String,
    sources: Vec<usize>,
}

#[wasm_bindgen]
impl Sp3MergeFlag {
    /// Flagged epoch as seconds since J2000 in the product time scale.
    #[wasm_bindgen(getter, js_name = epochJ2000Seconds)]
    pub fn epoch_j2000_seconds(&self) -> f64 {
        self.epoch_j2000_seconds
    }

    /// Satellite token, e.g. `"G01"`.
    #[wasm_bindgen(getter)]
    pub fn satellite(&self) -> String {
        self.satellite.clone()
    }

    /// Source indices (into the input array) this flag refers to.
    #[wasm_bindgen(getter)]
    pub fn sources(&self) -> Vec<usize> {
        self.sources.clone()
    }
}

impl From<MergeFlag> for Sp3MergeFlag {
    fn from(value: MergeFlag) -> Self {
        Self {
            epoch_j2000_seconds: instant_to_j2000_seconds(&value.epoch),
            satellite: value.satellite.to_string(),
            sources: value.sources,
        }
    }
}

/// Per-(epoch, satellite) agreement statistics for one accepted merged cell:
/// how tightly the consensus sources clustered about the combined value.
#[wasm_bindgen]
#[derive(Clone)]
pub struct Sp3AgreementMetric {
    epoch_j2000_seconds: f64,
    satellite: String,
    position_members: usize,
    position_rms_m: f64,
    position_max_m: f64,
    clock_members: usize,
    clock_rms_s: Option<f64>,
    clock_max_s: Option<f64>,
}

#[wasm_bindgen]
impl Sp3AgreementMetric {
    /// Cell epoch as seconds since J2000 in the product time scale.
    #[wasm_bindgen(getter, js_name = epochJ2000Seconds)]
    pub fn epoch_j2000_seconds(&self) -> f64 {
        self.epoch_j2000_seconds
    }

    /// Satellite token, e.g. `"G01"`.
    #[wasm_bindgen(getter)]
    pub fn satellite(&self) -> String {
        self.satellite.clone()
    }

    /// Number of sources in the accepted position consensus (>= 1).
    #[wasm_bindgen(getter, js_name = positionMembers)]
    pub fn position_members(&self) -> usize {
        self.position_members
    }

    /// RMS of the consensus members' 3D distance from the combined position,
    /// metres (zero for a single-source cell).
    #[wasm_bindgen(getter, js_name = positionRmsM)]
    pub fn position_rms_m(&self) -> f64 {
        self.position_rms_m
    }

    /// Largest 3D distance of any consensus member from the combined position,
    /// metres.
    #[wasm_bindgen(getter, js_name = positionMaxM)]
    pub fn position_max_m(&self) -> f64 {
        self.position_max_m
    }

    /// Number of sources in the accepted clock consensus (0 when the cell
    /// carries no clock).
    #[wasm_bindgen(getter, js_name = clockMembers)]
    pub fn clock_members(&self) -> usize {
        self.clock_members
    }

    /// RMS of the consensus members' deviation from the combined clock, seconds;
    /// `undefined` when the cell carries no clock.
    #[wasm_bindgen(getter, js_name = clockRmsS)]
    pub fn clock_rms_s(&self) -> Option<f64> {
        self.clock_rms_s
    }

    /// Largest absolute clock deviation from the combined clock, seconds;
    /// `undefined` when the cell carries no clock.
    #[wasm_bindgen(getter, js_name = clockMaxS)]
    pub fn clock_max_s(&self) -> Option<f64> {
        self.clock_max_s
    }
}

impl From<AgreementMetric> for Sp3AgreementMetric {
    fn from(value: AgreementMetric) -> Self {
        Self {
            epoch_j2000_seconds: instant_to_j2000_seconds(&value.epoch),
            satellite: value.satellite.to_string(),
            position_members: value.position_members,
            position_rms_m: value.position_rms_m,
            position_max_m: value.position_max_m,
            clock_members: value.clock_members,
            clock_rms_s: value.clock_rms_s,
            clock_max_s: value.clock_max_s,
        }
    }
}

/// One coordinate-label reconciliation applied before SP3 merge consensus.
#[wasm_bindgen]
#[derive(Clone)]
pub struct Sp3FrameReconciliationReport {
    source_index: usize,
    source_label: String,
    target_label: String,
    method: String,
    asserted_label_set: Vec<String>,
    source_frame: Option<String>,
    target_frame: Option<String>,
    catalog_source_frame: Option<String>,
    catalog_target_frame: Option<String>,
    catalog_inverse: bool,
    reference_epoch_year: Option<f64>,
    translation_mm: Vec<f64>,
    scale_ppb: Option<f64>,
    rotation_mas: Vec<f64>,
    translation_mm_per_year: Vec<f64>,
    scale_ppb_per_year: Option<f64>,
    rotation_mas_per_year: Vec<f64>,
    provenance: Option<String>,
    epoch_year_span: Vec<f64>,
    records_affected: usize,
    identity: bool,
}

#[wasm_bindgen]
impl Sp3FrameReconciliationReport {
    /// Source index in the mergeSp3 input array.
    #[wasm_bindgen(getter, js_name = sourceIndex)]
    pub fn source_index(&self) -> usize {
        self.source_index
    }

    /// Original coordinate-system label on that source.
    #[wasm_bindgen(getter, js_name = sourceLabel)]
    pub fn source_label(&self) -> String {
        self.source_label.clone()
    }

    /// Target coordinate-system label, taken from source 0.
    #[wasm_bindgen(getter, js_name = targetLabel)]
    pub fn target_label(&self) -> String {
        self.target_label.clone()
    }

    /// Reconciliation mechanism: "asserted_equivalence" or "helmert".
    #[wasm_bindgen(getter)]
    pub fn method(&self) -> String {
        self.method.clone()
    }

    /// Caller assertion set, empty unless assertion reconciliation was used.
    #[wasm_bindgen(getter, js_name = assertedLabelSet)]
    pub fn asserted_label_set(&self) -> Vec<String> {
        self.asserted_label_set.clone()
    }

    /// Resolved source terrestrial frame for Helmert reconciliation.
    #[wasm_bindgen(getter, js_name = sourceFrame)]
    pub fn source_frame(&self) -> Option<String> {
        self.source_frame.clone()
    }

    /// Resolved target terrestrial frame for Helmert reconciliation.
    #[wasm_bindgen(getter, js_name = targetFrame)]
    pub fn target_frame(&self) -> Option<String> {
        self.target_frame.clone()
    }

    /// Source frame of the published catalog row used for Helmert reconciliation.
    #[wasm_bindgen(getter, js_name = catalogSourceFrame)]
    pub fn catalog_source_frame(&self) -> Option<String> {
        self.catalog_source_frame.clone()
    }

    /// Target frame of the published catalog row used for Helmert reconciliation.
    #[wasm_bindgen(getter, js_name = catalogTargetFrame)]
    pub fn catalog_target_frame(&self) -> Option<String> {
        self.catalog_target_frame.clone()
    }

    /// True when the published catalog row was applied in reverse.
    #[wasm_bindgen(getter, js_name = catalogInverse)]
    pub fn catalog_inverse(&self) -> bool {
        self.catalog_inverse
    }

    /// Published transform reference epoch, if a catalog entry was used.
    #[wasm_bindgen(getter, js_name = referenceEpochYear)]
    pub fn reference_epoch_year(&self) -> Option<f64> {
        self.reference_epoch_year
    }

    /// Translation parameters in millimetres, empty for non-catalog identity.
    #[wasm_bindgen(getter, js_name = translationMm)]
    pub fn translation_mm(&self) -> Vec<f64> {
        self.translation_mm.clone()
    }

    /// Scale parameter in parts per billion, undefined for non-catalog identity.
    #[wasm_bindgen(getter, js_name = scalePpb)]
    pub fn scale_ppb(&self) -> Option<f64> {
        self.scale_ppb
    }

    /// Rotation parameters in milliarcseconds, empty for non-catalog identity.
    #[wasm_bindgen(getter, js_name = rotationMas)]
    pub fn rotation_mas(&self) -> Vec<f64> {
        self.rotation_mas.clone()
    }

    /// Translation rates in millimetres per year.
    #[wasm_bindgen(getter, js_name = translationMmPerYear)]
    pub fn translation_mm_per_year(&self) -> Vec<f64> {
        self.translation_mm_per_year.clone()
    }

    /// Scale rate in parts per billion per year.
    #[wasm_bindgen(getter, js_name = scalePpbPerYear)]
    pub fn scale_ppb_per_year(&self) -> Option<f64> {
        self.scale_ppb_per_year
    }

    /// Rotation rates in milliarcseconds per year.
    #[wasm_bindgen(getter, js_name = rotationMasPerYear)]
    pub fn rotation_mas_per_year(&self) -> Vec<f64> {
        self.rotation_mas_per_year.clone()
    }

    /// Published-table provenance for the catalog entry.
    #[wasm_bindgen(getter)]
    pub fn provenance(&self) -> Option<String> {
        self.provenance.clone()
    }

    /// Inclusive decimal-year span of affected records.
    #[wasm_bindgen(getter, js_name = epochYearSpan)]
    pub fn epoch_year_span(&self) -> Vec<f64> {
        self.epoch_year_span.clone()
    }

    /// Number of satellite position records covered by the reconciliation.
    #[wasm_bindgen(getter, js_name = recordsAffected)]
    pub fn records_affected(&self) -> usize {
        self.records_affected
    }

    /// True when both labels resolved to the same terrestrial realization.
    #[wasm_bindgen(getter)]
    pub fn identity(&self) -> bool {
        self.identity
    }
}

impl From<Sp3FrameReconciliation> for Sp3FrameReconciliationReport {
    fn from(value: Sp3FrameReconciliation) -> Self {
        Self {
            source_index: value.source_index,
            source_label: value.source_label,
            target_label: value.target_label,
            method: match value.method {
                sidereon_core::ephemeris::Sp3FrameReconciliationMethod::AssertedEquivalence => {
                    "asserted_equivalence".to_string()
                }
                sidereon_core::ephemeris::Sp3FrameReconciliationMethod::Helmert => {
                    "helmert".to_string()
                }
            },
            asserted_label_set: value.asserted_label_set.unwrap_or_default(),
            source_frame: value.source_frame.map(|frame| frame.to_string()),
            target_frame: value.target_frame.map(|frame| frame.to_string()),
            catalog_source_frame: value.catalog_source_frame.map(|frame| frame.to_string()),
            catalog_target_frame: value.catalog_target_frame.map(|frame| frame.to_string()),
            catalog_inverse: value.catalog_inverse,
            reference_epoch_year: value.reference_epoch_year,
            translation_mm: value
                .parameters
                .map(|parameters| parameters.translation_mm.to_vec())
                .unwrap_or_default(),
            scale_ppb: value.parameters.map(|parameters| parameters.scale_ppb),
            rotation_mas: value
                .parameters
                .map(|parameters| parameters.rotation_mas.to_vec())
                .unwrap_or_default(),
            translation_mm_per_year: value
                .rates
                .map(|rates| rates.translation_mm_per_year.to_vec())
                .unwrap_or_default(),
            scale_ppb_per_year: value.rates.map(|rates| rates.scale_ppb_per_year),
            rotation_mas_per_year: value
                .rates
                .map(|rates| rates.rotation_mas_per_year.to_vec())
                .unwrap_or_default(),
            provenance: value.provenance,
            epoch_year_span: value
                .epoch_year_span
                .map(|span| span.to_vec())
                .unwrap_or_default(),
            records_affected: value.records_affected,
            identity: value.identity,
        }
    }
}

/// Audit report returned with a merged SP3 product.
#[wasm_bindgen]
#[derive(Clone)]
pub struct Sp3MergeReport {
    frame_reconciliations: Vec<Sp3FrameReconciliationReport>,
    quarantined: Vec<Sp3MergeFlag>,
    single_source: Vec<Sp3MergeFlag>,
    position_outliers: Vec<Sp3MergeFlag>,
    agreement: Vec<Sp3AgreementMetric>,
}

#[wasm_bindgen]
impl Sp3MergeReport {
    /// Coordinate-label reconciliations applied before consensus.
    #[wasm_bindgen(getter, js_name = frameReconciliations)]
    pub fn frame_reconciliations(&self) -> Vec<Sp3FrameReconciliationReport> {
        self.frame_reconciliations.clone()
    }

    #[wasm_bindgen(getter, js_name = frameReconciliationCount)]
    pub fn frame_reconciliation_count(&self) -> usize {
        self.frame_reconciliations.len()
    }

    /// Cells omitted because sources disagreed beyond tolerance.
    #[wasm_bindgen(getter)]
    pub fn quarantined(&self) -> Vec<Sp3MergeFlag> {
        self.quarantined.clone()
    }

    /// Cells carried from one source because no cross-check was possible.
    #[wasm_bindgen(getter, js_name = singleSource)]
    pub fn single_source(&self) -> Vec<Sp3MergeFlag> {
        self.single_source.clone()
    }

    /// Cells where an accepted consensus rejected source outliers.
    #[wasm_bindgen(getter, js_name = positionOutliers)]
    pub fn position_outliers(&self) -> Vec<Sp3MergeFlag> {
        self.position_outliers.clone()
    }

    #[wasm_bindgen(getter, js_name = quarantinedCount)]
    pub fn quarantined_count(&self) -> usize {
        self.quarantined.len()
    }

    #[wasm_bindgen(getter, js_name = singleSourceCount)]
    pub fn single_source_count(&self) -> usize {
        self.single_source.len()
    }

    #[wasm_bindgen(getter, js_name = positionOutlierCount)]
    pub fn position_outlier_count(&self) -> usize {
        self.position_outliers.len()
    }

    /// Per-(epoch, satellite) agreement statistics for every accepted cell, in
    /// output (epoch, then satellite) order, one entry per cell written to the
    /// merged product.
    #[wasm_bindgen(getter)]
    pub fn agreement(&self) -> Vec<Sp3AgreementMetric> {
        self.agreement.clone()
    }

    #[wasm_bindgen(getter, js_name = agreementCount)]
    pub fn agreement_count(&self) -> usize {
        self.agreement.len()
    }
}

impl From<MergeReport> for Sp3MergeReport {
    fn from(value: MergeReport) -> Self {
        Self {
            frame_reconciliations: value
                .frame_reconciliations
                .into_iter()
                .map(Into::into)
                .collect(),
            quarantined: value.quarantined.into_iter().map(Into::into).collect(),
            single_source: value.single_source.into_iter().map(Into::into).collect(),
            position_outliers: value
                .position_outliers
                .into_iter()
                .map(Into::into)
                .collect(),
            agreement: value.agreement.into_iter().map(Into::into).collect(),
        }
    }
}
