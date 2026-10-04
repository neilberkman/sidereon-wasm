use std::io::Read;
use std::str::FromStr;

use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use sidereon_core::astro::time::model::{Instant, JulianDateSplit, TimeScale};
use sidereon_core::bias::{
    write_bias_sinex, write_bias_sinex_bytes, write_code_dcb, write_code_dcb_bytes,
    BiasDeparture as CoreBiasDeparture, BiasError as CoreBiasError, BiasLookup, BiasMode,
    BiasNotice as CoreBiasNotice, BiasObservableFamily, BiasReadPolicy, BiasRecord,
    BiasSet as CoreBiasSet, BiasUnit, CodeDcbOptions,
};
use sidereon_core::constants::{J2000_JD, SECONDS_PER_DAY};
use sidereon_core::{GnssSatelliteId, GnssSystem};

use crate::error::{engine_error, error_with_detail, to_plain_js, type_error};
use crate::label::{lower_camel_variant, Label};

/// Read a Bias-SINEX / CODE DCB read policy: `"strict"` (the default) refuses
/// a departure from Bias-SINEX 1.00 by name; `"lenient"` reads it and reports
/// each departure in `notices`.
fn read_policy(label: Option<String>) -> Result<BiasReadPolicy, JsValue> {
    match label.as_deref() {
        None | Some("strict") => Ok(BiasReadPolicy::Strict),
        Some("lenient") => Ok(BiasReadPolicy::Lenient),
        Some(other) => Err(type_error(&format!(
            "invalid bias read policy {other:?}: expected \"strict\" or \"lenient\""
        ))),
    }
}

pub(crate) fn time_scale_label(scale: TimeScale) -> &'static str {
    match scale {
        TimeScale::Utc => "utc",
        TimeScale::Tai => "tai",
        TimeScale::Tt => "tt",
        TimeScale::Tdb => "tdb",
        TimeScale::Gpst => "gpst",
        TimeScale::Gst => "gst",
        TimeScale::Bdt => "bdt",
        TimeScale::Glonasst => "glonasst",
        TimeScale::Qzsst => "qzsst",
        TimeScale::Tcg => "tcg",
        TimeScale::Tcb => "tcb",
    }
}

/// The outcome of a bias lookup. `value` is set only for `"available"`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BiasLookupJs {
    /// `"available"`, `"absent"`, `"unsupportedScale"`, `"ambiguous"`,
    /// `"carrierFrequencyRequired"`, `"invalidCarrierFrequency"`,
    /// `"carrierFrequencyUnknown"`, `"undefinedSlopeReference"`,
    /// `"invalidEpoch"`, or, for a status this binding does not name yet, the
    /// engine variant's name in the same case.
    status: Label,
    value: Option<f64>,
    /// Indices into `records` the value comes from, or the conflicting
    /// records of an `"ambiguous"` lookup, or the one record a
    /// `"carrierFrequencyRequired"` or `"undefinedSlopeReference"` names.
    records: Vec<usize>,
    /// Records that also cover the epoch but that a later start overrides.
    overridden: Vec<usize>,
    /// For `"unsupportedScale"`, the product's time scale (`null` when it has
    /// none) and the query's.
    product_scale: Option<&'static str>,
    query_scale: Option<&'static str>,
    /// For `"carrierFrequencyUnknown"`, the observable code.
    observable: Option<String>,
}

fn lookup_to_js(lookup: BiasLookup) -> Result<JsValue, JsValue> {
    let mut out = BiasLookupJs {
        status: lower_camel_variant(&lookup),
        value: None,
        records: Vec::new(),
        overridden: Vec::new(),
        product_scale: None,
        query_scale: None,
        observable: None,
    };
    match lookup {
        BiasLookup::Available {
            value,
            records,
            overridden,
        } => {
            out.status = Label::Borrowed("available");
            out.value = Some(value);
            out.records = records;
            out.overridden = overridden;
        }
        BiasLookup::Absent => out.status = Label::Borrowed("absent"),
        BiasLookup::UnsupportedScale { product, query } => {
            out.status = Label::Borrowed("unsupportedScale");
            out.product_scale = product.map(time_scale_label);
            out.query_scale = Some(time_scale_label(query));
        }
        BiasLookup::Ambiguous { records } => {
            out.status = Label::Borrowed("ambiguous");
            out.records = records;
        }
        BiasLookup::CarrierFrequencyRequired { record } => {
            out.status = Label::Borrowed("carrierFrequencyRequired");
            out.records = vec![record];
        }
        BiasLookup::InvalidCarrierFrequency => {
            out.status = Label::Borrowed("invalidCarrierFrequency")
        }
        BiasLookup::CarrierFrequencyUnknown { observable } => {
            out.status = Label::Borrowed("carrierFrequencyUnknown");
            out.observable = Some(observable);
        }
        BiasLookup::UndefinedSlopeReference { record } => {
            out.status = Label::Borrowed("undefinedSlopeReference");
            out.records = vec![record];
        }
        BiasLookup::InvalidEpoch => out.status = Label::Borrowed("invalidEpoch"),
        _ => {}
    }
    to_plain_js(&out, "bias lookup")
}

fn maybe_gzip(bytes: &[u8]) -> Result<Vec<u8>, JsValue> {
    if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut decoder = GzDecoder::new(bytes);
        let mut out = Vec::new();
        decoder
            .read_to_end(&mut out)
            .map_err(|e| engine_error(format!("gzip decode failed: {e}")))?;
        Ok(out)
    } else {
        Ok(bytes.to_vec())
    }
}

fn parse_sat(token: &str) -> Result<GnssSatelliteId, JsValue> {
    GnssSatelliteId::from_str(token)
        .map_err(|e| type_error(&format!("invalid satellite token {token:?}: {e}")))
}

fn parse_system(value: &str) -> Result<GnssSystem, JsValue> {
    match value {
        "gps" => Ok(GnssSystem::Gps),
        "glonass" => Ok(GnssSystem::Glonass),
        "galileo" => Ok(GnssSystem::Galileo),
        "beidou" => Ok(GnssSystem::BeiDou),
        "qzss" => Ok(GnssSystem::Qzss),
        "navic" => Ok(GnssSystem::Navic),
        "sbas" => Ok(GnssSystem::Sbas),
        other => Err(type_error(&format!("invalid GNSS system label {other:?}"))),
    }
}

fn parse_time_scale(value: Option<&str>) -> Result<TimeScale, JsValue> {
    match value.unwrap_or("gpst") {
        "utc" => Ok(TimeScale::Utc),
        "tai" => Ok(TimeScale::Tai),
        "tt" => Ok(TimeScale::Tt),
        "tdb" => Ok(TimeScale::Tdb),
        "gpst" => Ok(TimeScale::Gpst),
        "gst" => Ok(TimeScale::Gst),
        "bdt" => Ok(TimeScale::Bdt),
        "glonasst" => Ok(TimeScale::Glonasst),
        "qzsst" => Ok(TimeScale::Qzsst),
        "tcg" => Ok(TimeScale::Tcg),
        "tcb" => Ok(TimeScale::Tcb),
        other => Err(type_error(&format!("invalid time scale {other:?}"))),
    }
}

fn epoch_from_j2000(epoch_j2000_s: f64, scale: TimeScale) -> Result<Instant, JsValue> {
    let days = epoch_j2000_s / SECONDS_PER_DAY;
    let whole = J2000_JD + days.floor();
    let fraction = days - days.floor();
    let split = JulianDateSplit::new(whole, fraction).map_err(|error| {
        let message = error.to_string();
        crate::tropo::time_model_error_with(error, "Error", &message)
    })?;
    Ok(Instant::from_julian_date(scale, split))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeDcbOptionsInput {
    obs1: String,
    obs2: String,
    year: i32,
    month: u8,
    #[serde(default)]
    time_scale: Option<String>,
    #[serde(default)]
    receiver_system: Option<String>,
}

impl CodeDcbOptionsInput {
    fn to_core(&self) -> Result<CodeDcbOptions, JsValue> {
        let mut options = CodeDcbOptions::new(
            (self.obs1.clone(), self.obs2.clone()),
            self.year,
            self.month,
            parse_time_scale(self.time_scale.as_deref())?,
        );
        options.receiver_system = self
            .receiver_system
            .as_deref()
            .map(parse_system)
            .transpose()?;
        Ok(options)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BiasRecordJs {
    kind: String,
    target: String,
    svn: Option<String>,
    obs1: String,
    obs2: Option<String>,
    /// Start and end tokens as the row states them.
    valid_from: String,
    valid_until: String,
    value: f64,
    sigma: Option<f64>,
    slope: Option<f64>,
    slope_sigma: Option<f64>,
    /// `"code"`, `"phase"` or `"mixed"`, from the observable codes.
    family: &'static str,
    /// `"ns"` (value in seconds) or `"cyc"` (value in cycles), as the row
    /// states it.
    unit: &'static str,
    is_phase: bool,
    line: Option<usize>,
}

fn record_to_js(record: &BiasRecord) -> BiasRecordJs {
    BiasRecordJs {
        kind: format!("{:?}", record.kind).to_ascii_lowercase(),
        target: format!("{:?}", record.target),
        svn: record.svn.clone(),
        obs1: record.obs1.clone(),
        obs2: record.obs2.clone(),
        valid_from: record.raw_epochs.0.clone(),
        valid_until: record.raw_epochs.1.clone(),
        value: record.value,
        sigma: record.sigma,
        slope: record.slope,
        slope_sigma: record.slope_sigma,
        family: match record.family {
            BiasObservableFamily::Code => "code",
            BiasObservableFamily::Phase => "phase",
            BiasObservableFamily::Mixed => "mixed",
        },
        unit: match record.unit {
            BiasUnit::Nanoseconds => "ns",
            BiasUnit::Cycles => "cyc",
        },
        is_phase: record.is_phase(),
        line: record.line,
    }
}

fn bias_departure_payload(departure: &CoreBiasDeparture) -> serde_json::Value {
    use CoreBiasDeparture as Departure;
    match departure {
        Departure::HeaderLayout { reason } => {
            serde_json::json!({"kind":"headerLayout","reason":reason})
        }
        Departure::OtherVersion { version } => {
            serde_json::json!({"kind":"otherVersion","version":version})
        }
        Departure::MissingFooter => serde_json::json!({"kind":"missingFooter"}),
        Departure::ContentAfterFooter { line } => {
            serde_json::json!({"kind":"contentAfterFooter","line":line})
        }
        Departure::UnexpectedControlLine { line } => {
            serde_json::json!({"kind":"unexpectedControlLine","line":line})
        }
        Departure::UnclosedBlock { name, line } => {
            serde_json::json!({"kind":"unclosedBlock","name":name,"line":line})
        }
        Departure::UnopenedBlockEnd { name, line } => {
            serde_json::json!({"kind":"unopenedBlockEnd","name":name,"line":line})
        }
        Departure::MismatchedBlockEnd { open, close, line } => {
            serde_json::json!({"kind":"mismatchedBlockEnd","open":open,"close":close,"line":line})
        }
        Departure::NestedBlock { open, inner, line } => {
            serde_json::json!({"kind":"nestedBlock","open":open,"inner":inner,"line":line})
        }
        Departure::MissingBlock { name } => serde_json::json!({"kind":"missingBlock","name":name}),
        Departure::UnknownBlock { name, line } => {
            serde_json::json!({"kind":"unknownBlock","name":name,"line":line})
        }
        Departure::BlockStartSuffix { line } => {
            serde_json::json!({"kind":"blockStartSuffix","line":line})
        }
        Departure::DataOutsideBlock { line } => {
            serde_json::json!({"kind":"dataOutsideBlock","line":line})
        }
        Departure::MissingDeclaration { keyword } => {
            serde_json::json!({"kind":"missingDeclaration","keyword":keyword})
        }
        Departure::UnsupportedBiasMode { line, label } => {
            serde_json::json!({"kind":"unsupportedBiasMode","line":line,"label":label})
        }
        Departure::NonStandardTimeSystem { line, label } => {
            serde_json::json!({"kind":"nonStandardTimeSystem","line":line,"label":label})
        }
        Departure::HeaderModeMismatch {
            header,
            description,
        } => {
            serde_json::json!({"kind":"headerModeMismatch","header":header,"description":bias_mode_label(*description)})
        }
        Departure::UnknownDcbTimeSystem { line, label } => {
            serde_json::json!({"kind":"unknownDcbTimeSystem","line":line,"label":label})
        }
        Departure::EstimateCountMismatch {
            declared,
            solution_rows,
        } => {
            serde_json::json!({"kind":"estimateCountMismatch","declared":declared,"solutionRows":solution_rows})
        }
        _ => serde_json::json!({"kind":"other","message":format!("{departure:?}")}),
    }
}

fn bias_mode_label(mode: BiasMode) -> &'static str {
    match mode {
        BiasMode::Absolute => "absolute",
        BiasMode::Relative => "relative",
        BiasMode::Unspecified => "unspecified",
    }
}

fn bias_notice_payload(notice: &CoreBiasNotice) -> serde_json::Value {
    use CoreBiasNotice as Notice;
    match notice {
        Notice::Departure(departure) => {
            serde_json::json!({"kind":"departure","departure":bias_departure_payload(departure)})
        }
        Notice::InvalidUtf8 { line } => serde_json::json!({"kind":"invalidUtf8","line":line}),
        Notice::RepeatedDeclaration { line, keyword } => {
            serde_json::json!({"kind":"repeatedDeclaration","line":line,"keyword":keyword})
        }
        Notice::ConflictingDeclaration { line, keyword } => {
            serde_json::json!({"kind":"conflictingDeclaration","line":line,"keyword":keyword})
        }
        Notice::Overlap { first, second } => {
            serde_json::json!({"kind":"overlap","first":first,"second":second})
        }
        Notice::DcbTimeSystemAssumed => serde_json::json!({"kind":"dcbTimeSystemAssumed"}),
        Notice::DcbTimeSystemAlias { line, label } => {
            serde_json::json!({"kind":"dcbTimeSystemAlias","line":line,"label":label})
        }
        _ => serde_json::json!({"kind":"unknown","message":format!("{notice:?}")}),
    }
}

pub(crate) fn bias_error_payload(error: &CoreBiasError) -> serde_json::Value {
    use CoreBiasError as BiasError;
    match error {
        BiasError::InvalidInput { field, reason } => {
            serde_json::json!({"kind":"invalidInput","field":field,"reason":reason})
        }
        BiasError::InvalidEpoch => serde_json::json!({"kind":"invalidEpoch"}),
        BiasError::UnknownObservable { code } => {
            serde_json::json!({"kind":"unknownObservable","code":code})
        }
        BiasError::UnsupportedVersion { version } => {
            serde_json::json!({"kind":"unsupportedVersion","version":version})
        }
        BiasError::MissingDcbMetadata => serde_json::json!({"kind":"missingDcbMetadata"}),
        BiasError::MissingClockReference => serde_json::json!({"kind":"missingClockReference"}),
        BiasError::MissingWriterMetadata { field } => {
            serde_json::json!({"kind":"missingWriterMetadata","field":field})
        }
        BiasError::Utf8 => serde_json::json!({"kind":"utf8"}),
        BiasError::Departure { departure } => {
            serde_json::json!({"kind":"departure","departure":bias_departure_payload(departure)})
        }
        BiasError::InvalidUtf8Line { line } => {
            serde_json::json!({"kind":"invalidUtf8Line","line":line})
        }
        BiasError::UnsupportedTimeSystem { scale } => {
            serde_json::json!({"kind":"unsupportedTimeSystem","scale":scale.map(time_scale_label)})
        }
        BiasError::DcbRecordMismatch { record, field } => {
            serde_json::json!({"kind":"dcbRecordMismatch","record":record,"field":field})
        }
    }
}

fn bias_error(error: CoreBiasError) -> JsValue {
    let detail = bias_error_payload(&error);
    error_with_detail("BiasError", &error.to_string(), &detail)
}

#[wasm_bindgen]
pub struct BiasSet {
    inner: CoreBiasSet,
}

impl BiasSet {
    pub(crate) fn core(&self) -> CoreBiasSet {
        self.inner.clone()
    }
}

#[wasm_bindgen]
impl BiasSet {
    #[wasm_bindgen(getter, js_name = recordCount)]
    pub fn record_count(&self) -> usize {
        self.inner.records().len()
    }

    #[wasm_bindgen(getter, js_name = skippedRecords)]
    pub fn skipped_records(&self) -> usize {
        self.inner.skipped_records()
    }

    #[wasm_bindgen(getter)]
    pub fn records(&self) -> Result<JsValue, JsValue> {
        let records: Vec<BiasRecordJs> = self.inner.records().iter().map(record_to_js).collect();
        serde_wasm_bindgen::to_value(&records).map_err(|e| type_error(&e.to_string()))
    }

    /// `BIAS_MODE`: `"absolute"`, `"relative"`, or `"unspecified"` when the
    /// product declares no usable mode.
    #[wasm_bindgen(getter)]
    pub fn mode(&self) -> String {
        match self.inner.mode() {
            BiasMode::Absolute => "absolute",
            BiasMode::Relative => "relative",
            BiasMode::Unspecified => "unspecified",
        }
        .to_string()
    }

    /// The product's time scale (`"gpst"`, `"utc"`, ...), or `undefined` when
    /// it declares none a lookup can use.
    #[wasm_bindgen(getter, js_name = timeScale)]
    pub fn time_scale(&self) -> Option<String> {
        self.inner
            .time_scale()
            .map(|scale| time_scale_label(scale).to_string())
    }

    /// The time-system label as the product writes it, or `undefined`.
    #[wasm_bindgen(getter, js_name = timeSystemLabel)]
    pub fn time_system_label(&self) -> Option<String> {
        self.inner.time_system_label().map(str::to_owned)
    }

    /// The non-fatal findings of the read, each as the engine states it:
    /// departures read through under the lenient policy, overlaps, skipped
    /// rows and time-system notes.
    #[wasm_bindgen(getter)]
    pub fn notices(&self) -> Vec<String> {
        self.inner
            .notices()
            .iter()
            .map(|notice| format!("{notice:?}"))
            .collect()
    }

    #[wasm_bindgen(
        getter,
        js_name = noticeDetails,
        unchecked_return_type = "BiasNoticeDetail[]"
    )]
    pub fn notice_details(&self) -> Result<JsValue, JsValue> {
        let details: Vec<serde_json::Value> = self
            .inner
            .notices()
            .iter()
            .map(bias_notice_payload)
            .collect();
        to_plain_js(&details, "bias notice details")
    }

    /// Count of every physical input line by what the reader made of it.
    #[wasm_bindgen(getter, js_name = lineCounts, unchecked_return_type = "BiasLineCounts")]
    pub fn line_counts(&self) -> Result<JsValue, JsValue> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct LineCountsJs {
            lines: usize,
            header_footer: usize,
            comments: usize,
            blank: usize,
            block_delimiters: usize,
            info_rows: usize,
            records: usize,
            skipped: usize,
            block_body: usize,
            other: usize,
        }
        let c = self.inner.line_counts();
        to_plain_js(
            &LineCountsJs {
                lines: c.lines,
                header_footer: c.header_footer,
                comments: c.comments,
                blank: c.blank,
                block_delimiters: c.block_delimiters,
                info_rows: c.info_rows,
                records: c.records,
                skipped: c.skipped,
                block_body: c.block_body,
                other: c.other,
            },
            "bias line counts",
        )
    }

    /// Write the product as Bias-SINEX bytes. A product read from Bias-SINEX
    /// and not edited is restated byte for byte. Throws an `Error` for a value
    /// its field cannot hold.
    #[wasm_bindgen(js_name = toBiasSinex)]
    pub fn to_bias_sinex(&self) -> Result<Vec<u8>, JsValue> {
        write_bias_sinex_bytes(&self.inner).map_err(bias_error)
    }

    #[wasm_bindgen(js_name = toBiasSinexBytes)]
    pub fn to_bias_sinex_bytes(&self) -> Result<Vec<u8>, JsValue> {
        write_bias_sinex_bytes(&self.inner).map_err(bias_error)
    }

    #[wasm_bindgen(js_name = toBiasSinexText)]
    pub fn to_bias_sinex_text(&self) -> Result<String, JsValue> {
        write_bias_sinex(&self.inner).map_err(bias_error)
    }

    /// Write the product as CODE DCB bytes. A set read from CODE DCB is
    /// restated as its source lines. Throws an `Error` for a record the DCB
    /// metadata does not describe or a value its field cannot hold.
    #[wasm_bindgen(js_name = toCodeDcb)]
    pub fn to_code_dcb(&self) -> Result<Vec<u8>, JsValue> {
        write_code_dcb_bytes(&self.inner).map_err(bias_error)
    }

    #[wasm_bindgen(js_name = toCodeDcbBytes)]
    pub fn to_code_dcb_bytes(&self) -> Result<Vec<u8>, JsValue> {
        write_code_dcb_bytes(&self.inner).map_err(bias_error)
    }

    #[wasm_bindgen(js_name = toCodeDcbText)]
    pub fn to_code_dcb_text(&self) -> Result<String, JsValue> {
        write_code_dcb(&self.inner).map_err(bias_error)
    }

    /// Code observable-specific bias, seconds, as a `BiasLookup`.
    #[wasm_bindgen(js_name = codeOsbSeconds, unchecked_return_type = "BiasLookup")]
    pub fn code_osb_seconds(
        &self,
        sat: &str,
        obs: &str,
        epoch_j2000_s: f64,
        time_scale: Option<String>,
    ) -> Result<JsValue, JsValue> {
        let sat = parse_sat(sat)?;
        let epoch = epoch_from_j2000(epoch_j2000_s, parse_time_scale(time_scale.as_deref())?)?;
        lookup_to_js(self.inner.code_osb_seconds(sat, obs, epoch))
    }

    /// Phase observable-specific bias, cycles, as a `BiasLookup`. `carrierHz`
    /// converts a phase bias stated in nanoseconds; without it such a lookup
    /// is `"carrierFrequencyRequired"`, since a GLONASS FDMA carrier depends on
    /// the satellite's channel.
    #[wasm_bindgen(js_name = phaseOsbCycles, unchecked_return_type = "BiasLookup")]
    pub fn phase_osb_cycles(
        &self,
        sat: &str,
        obs: &str,
        epoch_j2000_s: f64,
        time_scale: Option<String>,
        carrier_hz: Option<f64>,
    ) -> Result<JsValue, JsValue> {
        let sat = parse_sat(sat)?;
        let epoch = epoch_from_j2000(epoch_j2000_s, parse_time_scale(time_scale.as_deref())?)?;
        lookup_to_js(self.inner.phase_osb_cycles(sat, obs, epoch, carrier_hz))
    }

    /// Code differential bias `obs1 - obs2`, seconds, as a `BiasLookup`.
    #[wasm_bindgen(js_name = codeDsbSeconds, unchecked_return_type = "BiasLookup")]
    pub fn code_dsb_seconds(
        &self,
        sat: &str,
        obs1: &str,
        obs2: &str,
        epoch_j2000_s: f64,
        time_scale: Option<String>,
    ) -> Result<JsValue, JsValue> {
        let sat = parse_sat(sat)?;
        let epoch = epoch_from_j2000(epoch_j2000_s, parse_time_scale(time_scale.as_deref())?)?;
        lookup_to_js(self.inner.code_dsb_seconds(sat, obs1, obs2, epoch))
    }

    /// Ionosphere-free code bias model, metres, as a `BiasLookup`.
    #[wasm_bindgen(js_name = codeBiasModelM, unchecked_return_type = "BiasLookup")]
    #[allow(clippy::too_many_arguments)]
    pub fn code_bias_model_m(
        &self,
        sat: &str,
        used_obs1: &str,
        used_obs2: &str,
        freq1_hz: f64,
        freq2_hz: f64,
        glonass_channel: Option<i8>,
        clock_ref_obs1: &str,
        clock_ref_obs2: &str,
        epoch_j2000_s: f64,
        time_scale: Option<String>,
    ) -> Result<JsValue, JsValue> {
        let sat = parse_sat(sat)?;
        let epoch = epoch_from_j2000(epoch_j2000_s, parse_time_scale(time_scale.as_deref())?)?;
        lookup_to_js(self.inner.code_bias_model_m(
            sat,
            (used_obs1, used_obs2),
            (freq1_hz, freq2_hz),
            glonass_channel,
            (clock_ref_obs1, clock_ref_obs2),
            epoch,
        ))
    }
}

fn parse_sinex(bytes: &[u8], policy: Option<String>) -> Result<BiasSet, JsValue> {
    let policy = read_policy(policy)?;
    let bytes = maybe_gzip(bytes)?;
    let parsed = CoreBiasSet::parse_bias_sinex_with_policy(&bytes, policy).map_err(bias_error)?;
    Ok(BiasSet {
        inner: parsed.value,
    })
}

/// Read a Bias-SINEX product (gzip or plain). `policy` is `"strict"` (the
/// default), which refuses a file that departs from Bias-SINEX 1.00 and a
/// version other than `1.00`, or `"lenient"`, which reads such a file and
/// reports each departure in `notices`.
#[wasm_bindgen(js_name = loadBiasSinex)]
pub fn load_bias_sinex(bytes: &[u8], policy: Option<String>) -> Result<BiasSet, JsValue> {
    parse_sinex(bytes, policy)
}

/// Same as `loadBiasSinex`; the non-fatal findings are on `notices`.
#[wasm_bindgen(js_name = loadBiasSinexLossy)]
pub fn load_bias_sinex_lossy(bytes: &[u8], policy: Option<String>) -> Result<BiasSet, JsValue> {
    parse_sinex(bytes, policy)
}

fn parse_dcb(bytes: &[u8], options: JsValue, policy: Option<String>) -> Result<BiasSet, JsValue> {
    let policy = read_policy(policy)?;
    let bytes = maybe_gzip(bytes)?;
    let options = if options.is_undefined() || options.is_null() {
        None
    } else {
        let input: CodeDcbOptionsInput = serde_wasm_bindgen::from_value(options)
            .map_err(|e| type_error(&format!("invalid CODE DCB options: {e}")))?;
        Some(input.to_core()?)
    };
    let parsed =
        CoreBiasSet::parse_code_dcb_with_policy(&bytes, options, policy).map_err(bias_error)?;
    Ok(BiasSet {
        inner: parsed.value,
    })
}

/// Read a CODE DCB product (gzip or plain). `policy` is `"strict"` (the
/// default), which refuses a generated title whose time-system label is
/// unknown when no options are given, or `"lenient"`, which reads the rows,
/// leaves the product without a time scale or DCB metadata and reports it.
#[wasm_bindgen(js_name = loadCodeDcb)]
pub fn load_code_dcb(
    bytes: &[u8],
    options: JsValue,
    policy: Option<String>,
) -> Result<BiasSet, JsValue> {
    parse_dcb(bytes, options, policy)
}

/// Same as `loadCodeDcb`; the non-fatal findings are on `notices`.
#[wasm_bindgen(js_name = loadCodeDcbLossy)]
pub fn load_code_dcb_lossy(
    bytes: &[u8],
    options: JsValue,
    policy: Option<String>,
) -> Result<BiasSet, JsValue> {
    parse_dcb(bytes, options, policy)
}
