//! RINEX lint, repair, and observation QC bindings.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon::rinex_qc::{
    Finding, FindingRef, LintReport, NavRepair as CoreNavRepair, ObsRepair as CoreObsRepair,
    RepairAction, RepairOptions, Severity,
};
use sidereon_core::observation_qc::{
    observation_qc_with_options, render_html as core_render_html, render_text as core_render_text,
    ClockJump, CycleSlipQc, IntervalSource, MpStats, MultipathReport, ObservationDataGap,
    ObservationQcNote, ObservationQcOptions, ObservationQcReport as CoreObservationQcReport,
    SatelliteMultipathQc, SatelliteObservationQc, SatelliteSignalQc, SnrStats, SsiHistogram,
    SystemCycleSlipQc, SystemMultipathQc, SystemSignalQc,
};
use sidereon_core::rinex::nav::encode_nav;
use sidereon_core::rinex::observations::{ObsEpochTime, PgmRunByDate};
use sidereon_core::GnssSystem;

use crate::error::{engine_error, type_error, utf8_text};
use crate::rinex_nav::{BroadcastRecordJs, IonoCorrectionsJs};
use crate::rinex_obs::RinexObs;

fn to_value<T: Serialize>(value: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(value).map_err(|e| type_error(&e.to_string()))
}

fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Fatal => "fatal",
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FindingRefJs {
    epoch_index: Option<usize>,
    satellite: Option<String>,
    field: Option<&'static str>,
}

impl From<&FindingRef> for FindingRefJs {
    fn from(value: &FindingRef) -> Self {
        Self {
            epoch_index: value.epoch_index,
            satellite: value.satellite.clone(),
            field: value.field,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FindingJs {
    code: &'static str,
    severity: &'static str,
    spec_ref: &'static str,
    repairable: bool,
    at: FindingRefJs,
    detail: String,
}

fn finding_js(finding: &Finding) -> FindingJs {
    FindingJs {
        code: finding.code(),
        severity: severity_label(finding.severity()),
        spec_ref: finding.spec_ref(),
        repairable: finding.is_repairable(),
        at: FindingRefJs::from(finding.at()),
        detail: format!("{finding:?}"),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SeverityCountsJs {
    fatal: usize,
    error: usize,
    warning: usize,
    info: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LintReportJs {
    clean: bool,
    decoded_from_crinex: bool,
    finding_count: usize,
    counts: SeverityCountsJs,
    findings: Vec<FindingJs>,
}

fn lint_report_js(report: &LintReport) -> LintReportJs {
    LintReportJs {
        clean: report.is_clean(),
        decoded_from_crinex: report.decoded_from_crinex,
        finding_count: report.findings.len(),
        counts: SeverityCountsJs {
            fatal: report.count(Severity::Fatal),
            error: report.count(Severity::Error),
            warning: report.count(Severity::Warning),
            info: report.count(Severity::Info),
        },
        findings: report.findings.iter().map(finding_js).collect(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RepairActionJs {
    id: &'static str,
    message: String,
}

fn action_js(action: &RepairAction) -> RepairActionJs {
    RepairActionJs {
        id: action.id,
        message: action.message.clone(),
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct FileStampInput {
    program: String,
    run_by: String,
    date: String,
}

impl FileStampInput {
    fn to_core(&self) -> PgmRunByDate {
        PgmRunByDate {
            program: self.program.clone(),
            run_by: self.run_by.clone(),
            date: self.date.clone(),
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct RepairOptionsInput {
    file_stamp: Option<FileStampInput>,
    set_interval: Option<bool>,
    set_time_of_last_obs: Option<bool>,
    set_obs_counts: Option<bool>,
    drop_empty_records: Option<bool>,
    sort_records: Option<bool>,
    drop_unsupported: Option<bool>,
}

fn repair_options(value: JsValue) -> Result<RepairOptions, JsValue> {
    if value.is_null() || value.is_undefined() {
        return Ok(RepairOptions::default());
    }
    let input: RepairOptionsInput = serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid RINEX repair options: {e}")))?;
    let defaults = RepairOptions::default();
    Ok(RepairOptions {
        file_stamp: input.file_stamp.map(|stamp| stamp.to_core()),
        set_interval: input.set_interval.unwrap_or(defaults.set_interval),
        set_time_of_last_obs: input
            .set_time_of_last_obs
            .unwrap_or(defaults.set_time_of_last_obs),
        set_obs_counts: input.set_obs_counts.unwrap_or(defaults.set_obs_counts),
        drop_empty_records: input
            .drop_empty_records
            .unwrap_or(defaults.drop_empty_records),
        sort_records: input.sort_records.unwrap_or(defaults.sort_records),
        drop_unsupported: input.drop_unsupported.unwrap_or(defaults.drop_unsupported),
    })
}

/// Lint RINEX observation text.
#[wasm_bindgen(js_name = lintRinexObs)]
pub fn lint_rinex_obs(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let text = utf8_text(bytes, "RINEX OBS source")?;
    to_value(&lint_report_js(&sidereon::lint_rinex_obs(&text)))
}

/// Lint RINEX navigation text.
#[wasm_bindgen(js_name = lintRinexNav)]
pub fn lint_rinex_nav(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let text = utf8_text(bytes, "RINEX NAV source")?;
    to_value(&lint_report_js(&sidereon::lint_rinex_nav(&text)))
}

/// Observation repair result.
#[wasm_bindgen]
pub struct RinexObsRepair {
    inner: CoreObsRepair,
    repaired_text: String,
}

#[wasm_bindgen]
impl RinexObsRepair {
    #[wasm_bindgen(getter)]
    pub fn repaired(&self) -> RinexObs {
        RinexObs::from_core(self.inner.repaired.clone())
    }

    #[wasm_bindgen(getter, js_name = repairedText)]
    pub fn repaired_text(&self) -> String {
        self.repaired_text.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn actions(&self) -> Result<JsValue, JsValue> {
        let actions: Vec<_> = self.inner.actions.iter().map(action_js).collect();
        to_value(&actions)
    }

    #[wasm_bindgen(getter)]
    pub fn remaining(&self) -> Result<JsValue, JsValue> {
        to_value(&lint_report_js(&self.inner.remaining))
    }

    #[wasm_bindgen(getter, js_name = decodedFromCrinex)]
    pub fn decoded_from_crinex(&self) -> bool {
        self.inner.decoded_from_crinex
    }

    #[wasm_bindgen(js_name = toCrinexString)]
    pub fn to_crinex_string(&self) -> Result<String, JsValue> {
        sidereon_core::rinex::qc::repair_obs_to_crinex_string(&self.inner).map_err(engine_error)
    }
}

/// Repair RINEX observation text.
#[wasm_bindgen(js_name = repairRinexObs)]
pub fn repair_rinex_obs(bytes: &[u8], options: JsValue) -> Result<RinexObsRepair, JsValue> {
    let text = utf8_text(bytes, "RINEX OBS source")?;
    let inner =
        sidereon::repair_rinex_obs(&text, &repair_options(options)?).map_err(engine_error)?;
    let repaired_text = inner.repaired.to_rinex_string();
    Ok(RinexObsRepair {
        inner,
        repaired_text,
    })
}

/// Navigation repair result.
#[wasm_bindgen]
pub struct RinexNavRepair {
    inner: CoreNavRepair,
    repaired_text: String,
}

#[wasm_bindgen]
impl RinexNavRepair {
    #[wasm_bindgen(getter)]
    pub fn records(&self) -> Vec<BroadcastRecordJs> {
        self.inner
            .records
            .iter()
            .cloned()
            .map(BroadcastRecordJs::from_core)
            .collect()
    }

    #[wasm_bindgen(getter, js_name = repairedText)]
    pub fn repaired_text(&self) -> String {
        self.repaired_text.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn actions(&self) -> Result<JsValue, JsValue> {
        let actions: Vec<_> = self.inner.actions.iter().map(action_js).collect();
        to_value(&actions)
    }

    #[wasm_bindgen(getter)]
    pub fn remaining(&self) -> Result<JsValue, JsValue> {
        to_value(&lint_report_js(&self.inner.remaining))
    }

    #[wasm_bindgen(getter)]
    pub fn iono(&self) -> Option<IonoCorrectionsJs> {
        self.inner.iono.map(IonoCorrectionsJs::from_core)
    }

    #[wasm_bindgen(getter, js_name = leapSeconds)]
    pub fn leap_seconds(&self) -> Option<f64> {
        self.inner.leap_seconds
    }
}

/// Repair RINEX navigation text.
#[wasm_bindgen(js_name = repairRinexNav)]
pub fn repair_rinex_nav(bytes: &[u8], options: JsValue) -> Result<RinexNavRepair, JsValue> {
    let text = utf8_text(bytes, "RINEX NAV source")?;
    let inner =
        sidereon::repair_rinex_nav(&text, &repair_options(options)?).map_err(engine_error)?;
    let repaired_text = encode_nav(&inner.records);
    Ok(RinexNavRepair {
        inner,
        repaired_text,
    })
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ObservationQcOptionsInput {
    interval_override_s: Option<f64>,
    gap_factor: Option<f64>,
    clock_jump_threshold_s: Option<f64>,
}

fn observation_qc_options(value: JsValue) -> Result<ObservationQcOptions, JsValue> {
    if value.is_null() || value.is_undefined() {
        return Ok(ObservationQcOptions::default());
    }
    let input: ObservationQcOptionsInput = serde_wasm_bindgen::from_value(value)
        .map_err(|e| type_error(&format!("invalid observation QC options: {e}")))?;
    let defaults = ObservationQcOptions::default();
    Ok(ObservationQcOptions {
        interval_override_s: input.interval_override_s,
        gap_factor: input.gap_factor.unwrap_or(defaults.gap_factor),
        clock_jump_threshold_s: input
            .clock_jump_threshold_s
            .unwrap_or(defaults.clock_jump_threshold_s),
    })
}

fn interval_source_label(source: IntervalSource) -> &'static str {
    match source {
        IntervalSource::Override => "override",
        IntervalSource::Header => "header",
        IntervalSource::Inferred => "inferred",
        IntervalSource::Unresolved => "unresolved",
    }
}

fn epoch_time_js(epoch: ObsEpochTime) -> ObsEpochTimeJs {
    ObsEpochTimeJs {
        year: epoch.year,
        month: epoch.month,
        day: epoch.day,
        hour: epoch.hour,
        minute: epoch.minute,
        second: epoch.second,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ObsEpochTimeJs {
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ObservationDataGapJs {
    start_epoch: ObsEpochTimeJs,
    end_epoch: ObsEpochTimeJs,
    nominal_interval_s: f64,
    observed_delta_s: f64,
    missing_epochs: usize,
}

fn data_gap_js(gap: &ObservationDataGap) -> ObservationDataGapJs {
    ObservationDataGapJs {
        start_epoch: epoch_time_js(gap.start_epoch),
        end_epoch: epoch_time_js(gap.end_epoch),
        nominal_interval_s: gap.nominal_interval_s,
        observed_delta_s: gap.observed_delta_s,
        missing_epochs: gap.missing_epochs,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClockJumpJs {
    epoch_index: usize,
    delta_s: f64,
}

fn clock_jump_js(jump: &ClockJump) -> ClockJumpJs {
    ClockJumpJs {
        epoch_index: jump.epoch_index,
        delta_s: jump.delta_s,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SatelliteObservationQcJs {
    satellite: String,
    epochs_with_observations: usize,
    value_observations: usize,
}

fn satellite_qc_js(value: &SatelliteObservationQc) -> SatelliteObservationQcJs {
    SatelliteObservationQcJs {
        satellite: value.satellite.to_string(),
        epochs_with_observations: value.epochs_with_observations,
        value_observations: value.value_observations,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SsiHistogramJs {
    counts: [u64; 10],
}

fn ssi_js(value: SsiHistogram) -> SsiHistogramJs {
    SsiHistogramJs {
        counts: value.counts,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SnrStatsJs {
    n: usize,
    mean: f64,
    min: f64,
    max: f64,
    std: Option<f64>,
}

fn snr_js(value: SnrStats) -> SnrStatsJs {
    SnrStatsJs {
        n: value.n,
        mean: value.mean,
        min: value.min,
        max: value.max,
        std: value.std,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SatelliteSignalQcJs {
    satellite: String,
    code: String,
    value_observations: usize,
    ssi: Option<SsiHistogramJs>,
    snr: Option<SnrStatsJs>,
}

fn satellite_signal_qc_js(value: &SatelliteSignalQc) -> SatelliteSignalQcJs {
    SatelliteSignalQcJs {
        satellite: value.satellite.to_string(),
        code: value.code.clone(),
        value_observations: value.value_observations,
        ssi: value.ssi.map(ssi_js),
        snr: value.snr.map(snr_js),
    }
}

fn system_label(system: GnssSystem) -> &'static str {
    match system {
        GnssSystem::Gps => "GPS",
        GnssSystem::Glonass => "GLONASS",
        GnssSystem::Galileo => "Galileo",
        GnssSystem::BeiDou => "BeiDou",
        GnssSystem::Qzss => "QZSS",
        GnssSystem::Sbas => "SBAS",
        GnssSystem::Navic => "NavIC",
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemSignalQcJs {
    system: &'static str,
    code: String,
    value_observations: usize,
    ssi: Option<SsiHistogramJs>,
    snr: Option<SnrStatsJs>,
}

fn system_signal_qc_js(value: &SystemSignalQc) -> SystemSignalQcJs {
    SystemSignalQcJs {
        system: system_label(value.system),
        code: value.code.clone(),
        value_observations: value.value_observations,
        ssi: value.ssi.map(ssi_js),
        snr: value.snr.map(snr_js),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemCycleSlipQcJs {
    system: &'static str,
    observations: usize,
    slips: usize,
    observations_per_slip: Option<f64>,
}

fn system_cycle_slip_qc_js(value: &SystemCycleSlipQc) -> SystemCycleSlipQcJs {
    SystemCycleSlipQcJs {
        system: system_label(value.system),
        observations: value.observations,
        slips: value.slips,
        observations_per_slip: value.observations_per_slip,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CycleSlipQcJs {
    observations: usize,
    total_slips: usize,
    observations_per_slip: Option<f64>,
    by_system: Vec<SystemCycleSlipQcJs>,
}

fn cycle_slip_qc_js(value: &CycleSlipQc) -> CycleSlipQcJs {
    CycleSlipQcJs {
        observations: value.observations,
        total_slips: value.total_slips,
        observations_per_slip: value.observations_per_slip,
        by_system: value.by_system.iter().map(system_cycle_slip_qc_js).collect(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MpStatsJs {
    n: usize,
    rms_m: f64,
}

fn mp_stats_js(value: MpStats) -> MpStatsJs {
    MpStatsJs {
        n: value.n,
        rms_m: value.rms_m,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SatelliteMultipathQcJs {
    satellite: String,
    mp1: Option<MpStatsJs>,
    mp2: Option<MpStatsJs>,
}

fn satellite_multipath_qc_js(value: &SatelliteMultipathQc) -> SatelliteMultipathQcJs {
    SatelliteMultipathQcJs {
        satellite: value.satellite.to_string(),
        mp1: value.mp1.map(mp_stats_js),
        mp2: value.mp2.map(mp_stats_js),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemMultipathQcJs {
    system: &'static str,
    mp1: Option<MpStatsJs>,
    mp2: Option<MpStatsJs>,
}

fn system_multipath_qc_js(value: &SystemMultipathQc) -> SystemMultipathQcJs {
    SystemMultipathQcJs {
        system: system_label(value.system),
        mp1: value.mp1.map(mp_stats_js),
        mp2: value.mp2.map(mp_stats_js),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MultipathReportJs {
    satellites: Vec<SatelliteMultipathQcJs>,
    systems: Vec<SystemMultipathQcJs>,
}

fn multipath_report_js(value: &MultipathReport) -> MultipathReportJs {
    MultipathReportJs {
        satellites: value
            .satellites
            .iter()
            .map(satellite_multipath_qc_js)
            .collect(),
        systems: value.systems.iter().map(system_multipath_qc_js).collect(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ObservationQcNoteJs {
    kind: &'static str,
    epoch_index: Option<usize>,
}

fn note_js(note: ObservationQcNote) -> ObservationQcNoteJs {
    match note {
        ObservationQcNote::NonMonotonicEpoch { epoch_index } => ObservationQcNoteJs {
            kind: "nonMonotonicEpoch",
            epoch_index: Some(epoch_index),
        },
        ObservationQcNote::IntervalUnresolved => ObservationQcNoteJs {
            kind: "intervalUnresolved",
            epoch_index: None,
        },
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ObservationQcReportJs {
    total_epoch_records: usize,
    observation_epochs: usize,
    event_records: usize,
    power_failure_epochs: usize,
    skipped_records: usize,
    interval_s: Option<f64>,
    interval_source: &'static str,
    missing_epochs: usize,
    data_gaps: Vec<ObservationDataGapJs>,
    clock_jumps: Vec<ClockJumpJs>,
    cycle_slips: CycleSlipQcJs,
    multipath: MultipathReportJs,
    satellites: Vec<SatelliteObservationQcJs>,
    satellite_signals: Vec<SatelliteSignalQcJs>,
    system_signals: Vec<SystemSignalQcJs>,
    notes: Vec<ObservationQcNoteJs>,
}

fn observation_qc_report_js(report: &CoreObservationQcReport) -> ObservationQcReportJs {
    ObservationQcReportJs {
        total_epoch_records: report.total_epoch_records,
        observation_epochs: report.observation_epochs,
        event_records: report.event_records,
        power_failure_epochs: report.power_failure_epochs,
        skipped_records: report.skipped_records,
        interval_s: report.interval_s,
        interval_source: interval_source_label(report.interval_source),
        missing_epochs: report.missing_epochs,
        data_gaps: report.data_gaps.iter().map(data_gap_js).collect(),
        clock_jumps: report.clock_jumps.iter().map(clock_jump_js).collect(),
        cycle_slips: cycle_slip_qc_js(&report.cycle_slips),
        multipath: multipath_report_js(&report.multipath),
        satellites: report.satellites.iter().map(satellite_qc_js).collect(),
        satellite_signals: report
            .satellite_signals
            .iter()
            .map(satellite_signal_qc_js)
            .collect(),
        system_signals: report
            .system_signals
            .iter()
            .map(system_signal_qc_js)
            .collect(),
        notes: report.notes.iter().copied().map(note_js).collect(),
    }
}

/// Aggregate observation QC report.
#[wasm_bindgen]
pub struct ObservationQcReport {
    inner: CoreObservationQcReport,
}

#[wasm_bindgen]
impl ObservationQcReport {
    #[wasm_bindgen(getter, js_name = totalEpochRecords)]
    pub fn total_epoch_records(&self) -> usize {
        self.inner.total_epoch_records
    }

    #[wasm_bindgen(getter, js_name = observationEpochs)]
    pub fn observation_epochs(&self) -> usize {
        self.inner.observation_epochs
    }

    #[wasm_bindgen(getter, js_name = eventRecords)]
    pub fn event_records(&self) -> usize {
        self.inner.event_records
    }

    #[wasm_bindgen(getter, js_name = powerFailureEpochs)]
    pub fn power_failure_epochs(&self) -> usize {
        self.inner.power_failure_epochs
    }

    #[wasm_bindgen(getter, js_name = skippedRecords)]
    pub fn skipped_records(&self) -> usize {
        self.inner.skipped_records
    }

    #[wasm_bindgen(getter, js_name = intervalS)]
    pub fn interval_s(&self) -> Option<f64> {
        self.inner.interval_s
    }

    #[wasm_bindgen(getter, js_name = intervalSource)]
    pub fn interval_source(&self) -> String {
        interval_source_label(self.inner.interval_source).to_string()
    }

    #[wasm_bindgen(getter, js_name = missingEpochs)]
    pub fn missing_epochs(&self) -> usize {
        self.inner.missing_epochs
    }

    #[wasm_bindgen(getter, js_name = dataGaps)]
    pub fn data_gaps(&self) -> Result<JsValue, JsValue> {
        let gaps: Vec<_> = self.inner.data_gaps.iter().map(data_gap_js).collect();
        to_value(&gaps)
    }

    #[wasm_bindgen(getter, js_name = clockJumps)]
    pub fn clock_jumps(&self) -> Result<JsValue, JsValue> {
        let jumps: Vec<_> = self.inner.clock_jumps.iter().map(clock_jump_js).collect();
        to_value(&jumps)
    }

    #[wasm_bindgen(getter, js_name = cycleSlips)]
    pub fn cycle_slips(&self) -> Result<JsValue, JsValue> {
        to_value(&cycle_slip_qc_js(&self.inner.cycle_slips))
    }

    #[wasm_bindgen(getter)]
    pub fn multipath(&self) -> Result<JsValue, JsValue> {
        to_value(&multipath_report_js(&self.inner.multipath))
    }

    #[wasm_bindgen(getter)]
    pub fn satellites(&self) -> Result<JsValue, JsValue> {
        let satellites: Vec<_> = self.inner.satellites.iter().map(satellite_qc_js).collect();
        to_value(&satellites)
    }

    #[wasm_bindgen(getter, js_name = satelliteSignals)]
    pub fn satellite_signals(&self) -> Result<JsValue, JsValue> {
        let signals: Vec<_> = self
            .inner
            .satellite_signals
            .iter()
            .map(satellite_signal_qc_js)
            .collect();
        to_value(&signals)
    }

    #[wasm_bindgen(getter, js_name = systemSignals)]
    pub fn system_signals(&self) -> Result<JsValue, JsValue> {
        let signals: Vec<_> = self
            .inner
            .system_signals
            .iter()
            .map(system_signal_qc_js)
            .collect();
        to_value(&signals)
    }

    #[wasm_bindgen(getter)]
    pub fn notes(&self) -> Result<JsValue, JsValue> {
        let notes: Vec<_> = self.inner.notes.iter().copied().map(note_js).collect();
        to_value(&notes)
    }

    #[wasm_bindgen(js_name = renderText)]
    pub fn render_text(&self) -> String {
        core_render_text(&self.inner)
    }

    #[wasm_bindgen(js_name = renderHtml)]
    pub fn render_html(&self) -> String {
        core_render_html(&self.inner)
    }

    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self) -> Result<String, JsValue> {
        serde_json::to_string(&observation_qc_report_js(&self.inner))
            .map_err(|e| type_error(&e.to_string()))
    }
}

/// Aggregate observation QC for a parsed RINEX OBS product.
#[wasm_bindgen(js_name = observationQc)]
pub fn observation_qc(obs: &RinexObs, options: JsValue) -> Result<ObservationQcReport, JsValue> {
    let inner = observation_qc_with_options(&obs.inner, observation_qc_options(options)?)
        .map_err(engine_error)?;
    Ok(ObservationQcReport { inner })
}
