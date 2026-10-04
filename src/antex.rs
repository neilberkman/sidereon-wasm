//! ANTEX antenna-calibration binding: parse an ANTEX 1.4 product, read every
//! record it retains, look up receiver / satellite phase-center offsets (PCO)
//! and variations (PCV), and write the product back.
//!
//! Parsing, lookup and writing are entirely `sidereon_core::antex`; this module
//! carries what the core retains across the boundary. PCO/PCV are metres. A
//! record the source does not carry stays absent (`undefined` from a getter,
//! `null` in a structured object), never a zero. Validity seconds cross as the
//! exact decimal the file states.

use serde::Serialize;
use wasm_bindgen::prelude::*;

use sidereon_core::antex::{
    Antenna as CoreAntenna, AntennaKind, Antex as CoreAntex, AntexDateTime as CoreAntexDateTime,
    AntexError, Frequency as CoreFrequency, PcvGrid, PcvSample as CorePcvSample, PcvType,
    SecondFraction,
};

use crate::error::{error_with_detail, range_error, to_plain_js, type_error, utf8_text};

fn kind_label(kind: AntennaKind) -> &'static str {
    match kind {
        AntennaKind::Receiver => "receiver",
        AntennaKind::Satellite => "satellite",
    }
}

/// Significant digits a `SecondFraction` holds: its digits are a `u64`.
const MAX_FRACTION_SIGNIFICANT_DIGITS: usize = 19;

/// Read the digits after a seconds field's decimal point as an exact
/// fraction: `"9999999"` is 0.9999999 s, `"0000000012345678"` is
/// 1.2345678e-9 s. Leading zeros set the scale and are kept; trailing zeros
/// do not change the value.
fn parse_fraction_digits(text: &str) -> Result<SecondFraction, JsValue> {
    if !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(type_error(
            "fractionDigits must hold only the decimal digits after the point",
        ));
    }
    let trimmed = text.trim_end_matches('0');
    let significant = trimmed.trim_start_matches('0');
    if significant.is_empty() {
        return Ok(SecondFraction::ZERO);
    }
    if significant.len() > MAX_FRACTION_SIGNIFICANT_DIGITS {
        return Err(range_error(&format!(
            "fractionDigits has {} significant digits; at most {MAX_FRACTION_SIGNIFICANT_DIGITS} are held exactly",
            significant.len()
        )));
    }
    let digits: u64 = significant.parse().map_err(|_| {
        range_error(&format!(
            "fractionDigits {text:?} has more significant digits than are held exactly"
        ))
    })?;
    SecondFraction::new(digits, trimmed.len() as u64)
        .ok_or_else(|| range_error("fractionDigits does not state a fraction below one second"))
}

/// The digits after the decimal point of an exact fraction, leading zeros
/// kept and no trailing zero; empty for zero.
fn fraction_digits_text(fraction: SecondFraction) -> String {
    if fraction.digits() == 0 {
        return String::new();
    }
    format!(
        "{:0>width$}",
        fraction.digits(),
        width = fraction.scale() as usize
    )
}

/// A GPS-time ANTEX validity instant, `VALID FROM` / `VALID UNTIL`.
///
/// GPS time has no leap-second label, so the second is `0..=59`. The fraction
/// of the second is exact: `fractionDigits` is the decimal the file states,
/// with every digit kept.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct AntexDateTime {
    inner: CoreAntexDateTime,
}

#[wasm_bindgen]
impl AntexDateTime {
    /// Create an ANTEX validity instant. `hour` / `minute` / `second` default
    /// to 0. `fractionDigits` is the exact fraction of the second as the
    /// decimal digits after the point, e.g. `"9999999"` for `59.9999999`
    /// with `second` 59; omitted is zero. Throws a `RangeError` on a date or
    /// time outside the GPS calendar (a second of 60 included) and a
    /// `TypeError` on non-digit `fractionDigits`.
    #[wasm_bindgen(constructor)]
    pub fn new(
        year: i32,
        month: u8,
        day: u8,
        hour: Option<u8>,
        minute: Option<u8>,
        second: Option<u8>,
        fraction_digits: Option<String>,
    ) -> Result<AntexDateTime, JsValue> {
        let fraction = match fraction_digits.as_deref() {
            Some(text) => parse_fraction_digits(text)?,
            None => SecondFraction::ZERO,
        };
        CoreAntexDateTime::new_with_fraction(
            year,
            month,
            day,
            hour.unwrap_or(0),
            minute.unwrap_or(0),
            second.unwrap_or(0),
            fraction,
        )
        .map(|inner| AntexDateTime { inner })
        .map_err(|_| range_error("invalid ANTEX datetime"))
    }

    /// Calendar year.
    #[wasm_bindgen(getter)]
    pub fn year(&self) -> i32 {
        self.inner.year
    }

    /// Calendar month, 1..=12.
    #[wasm_bindgen(getter)]
    pub fn month(&self) -> u8 {
        self.inner.month
    }

    /// Calendar day of month.
    #[wasm_bindgen(getter)]
    pub fn day(&self) -> u8 {
        self.inner.day
    }

    /// Hour of day.
    #[wasm_bindgen(getter)]
    pub fn hour(&self) -> u8 {
        self.inner.hour
    }

    /// Minute of hour.
    #[wasm_bindgen(getter)]
    pub fn minute(&self) -> u8 {
        self.inner.minute
    }

    /// Whole second of minute, `0..=59`.
    #[wasm_bindgen(getter)]
    pub fn second(&self) -> u8 {
        self.inner.second
    }

    /// Exact fraction of the second as the decimal digits after the point,
    /// leading zeros kept and no trailing zero: `"9999999"` for
    /// `59.9999999`, `""` for a whole second.
    #[wasm_bindgen(getter, js_name = fractionDigits)]
    pub fn fraction_digits(&self) -> String {
        fraction_digits_text(self.inner.fraction)
    }

    /// The fraction of the second in whole nanoseconds, or `undefined` when
    /// it is not a whole number of them (a stated `1.2345678E-9`, say).
    #[wasm_bindgen(getter)]
    pub fn nanosecond(&self) -> Option<u32> {
        self.inner.fraction.nanoseconds()
    }
}

/// A receiver or satellite ANTEX antenna calibration block.
#[wasm_bindgen]
pub struct Antenna {
    inner: CoreAntenna,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CalibrationJs {
    method: String,
    agency: String,
    antennas_calibrated: Option<u32>,
    date: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PcvSampleJs {
    /// `"noAzimuth"` for a `NOAZI` row, `"azimuth"` for a numeric-azimuth row.
    grid: &'static str,
    azimuth_deg: Option<f64>,
    zenith_deg: f64,
    value_m: f64,
}

impl From<&CorePcvSample> for PcvSampleJs {
    fn from(sample: &CorePcvSample) -> Self {
        Self {
            grid: match sample.grid {
                PcvGrid::NoAzimuth => "noAzimuth",
                PcvGrid::Azimuth => "azimuth",
            },
            azimuth_deg: sample.azimuth_deg,
            zenith_deg: sample.zenith_deg,
            value_m: sample.value_m,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FrequencyRmsJs {
    pco_m: Option<[f64; 3]>,
    pcv_samples: Vec<PcvSampleJs>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FrequencyJs {
    frequency: String,
    pco_m: [f64; 3],
    pcv_samples: Vec<PcvSampleJs>,
    rms: Option<FrequencyRmsJs>,
}

impl From<&CoreFrequency> for FrequencyJs {
    fn from(frequency: &CoreFrequency) -> Self {
        Self {
            frequency: frequency.frequency.clone(),
            pco_m: frequency.pco_m,
            pcv_samples: frequency
                .pcv_samples
                .iter()
                .map(PcvSampleJs::from)
                .collect(),
            rms: frequency.rms.as_ref().map(|rms| FrequencyRmsJs {
                pco_m: rms.pco_m,
                pcv_samples: rms.pcv_samples.iter().map(PcvSampleJs::from).collect(),
            }),
        }
    }
}

#[wasm_bindgen]
impl Antenna {
    /// ANTEX `TYPE / SERIAL` id.
    #[wasm_bindgen(getter)]
    pub fn id(&self) -> String {
        self.inner.id.clone()
    }

    /// Block role: `"receiver"` or `"satellite"`.
    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> String {
        kind_label(self.inner.kind).to_string()
    }

    /// ANTEX antenna type field.
    #[wasm_bindgen(getter, js_name = antennaType)]
    pub fn antenna_type(&self) -> String {
        self.inner.antenna_type.clone()
    }

    /// ANTEX serial, PRN, or radome field.
    #[wasm_bindgen(getter)]
    pub fn serial(&self) -> String {
        self.inner.serial.clone()
    }

    /// `COMMENT` records between `START OF ANTENNA` and `TYPE / SERIAL NO`, in
    /// file order.
    #[wasm_bindgen(getter, js_name = leadingComments)]
    pub fn leading_comments(&self) -> Vec<String> {
        self.inner.leading_comments.clone()
    }

    /// Every `METH / BY / # / DATE` record of the block, in file order, each
    /// `{ method, agency, antennasCalibrated, date }` with
    /// `antennasCalibrated` `null` where the field is blank.
    #[wasm_bindgen(getter, unchecked_return_type = "AntexCalibration[]")]
    pub fn calibrations(&self) -> Result<JsValue, JsValue> {
        let rows: Vec<CalibrationJs> = self
            .inner
            .calibrations
            .iter()
            .map(|calibration| CalibrationJs {
                method: calibration.method.clone(),
                agency: calibration.agency.clone(),
                antennas_calibrated: calibration.antennas_calibrated,
                date: calibration.date.clone(),
            })
            .collect();
        to_plain_js(&rows, "ANTEX calibration records")
    }

    /// `DAZI` azimuth grid spacing, degrees, or `undefined` when the block has
    /// no such record.
    #[wasm_bindgen(getter, js_name = daziDeg)]
    pub fn dazi_deg(&self) -> Option<f64> {
        self.inner.dazi_deg
    }

    /// `ZEN1`, degrees, or `undefined` when the block has no
    /// `ZEN1 / ZEN2 / DZEN` record.
    #[wasm_bindgen(getter, js_name = zenithStartDeg)]
    pub fn zenith_start_deg(&self) -> Option<f64> {
        self.inner.zenith_grid.map(|grid| grid.start_deg)
    }

    /// `ZEN2`, degrees, or `undefined` when the block has no
    /// `ZEN1 / ZEN2 / DZEN` record.
    #[wasm_bindgen(getter, js_name = zenithEndDeg)]
    pub fn zenith_end_deg(&self) -> Option<f64> {
        self.inner.zenith_grid.map(|grid| grid.end_deg)
    }

    /// `DZEN`, degrees, or `undefined` when the block has no
    /// `ZEN1 / ZEN2 / DZEN` record.
    #[wasm_bindgen(getter, js_name = zenithStepDeg)]
    pub fn zenith_step_deg(&self) -> Option<f64> {
        self.inner.zenith_grid.map(|grid| grid.step_deg)
    }

    /// Whether the block carries a `# OF FREQUENCIES` record. The writer
    /// states the number of frequency sections in it.
    #[wasm_bindgen(getter, js_name = hasFrequencyCount)]
    pub fn has_frequency_count(&self) -> bool {
        self.inner.has_frequency_count
    }

    /// Optional SINEX calibration code.
    #[wasm_bindgen(getter, js_name = sinexCode)]
    pub fn sinex_code(&self) -> Option<String> {
        self.inner.sinex_code.clone()
    }

    /// First valid instant, or `undefined` for a block open on that side.
    #[wasm_bindgen(getter, js_name = validFrom)]
    pub fn valid_from(&self) -> Option<AntexDateTime> {
        self.inner.valid_from.map(|inner| AntexDateTime { inner })
    }

    /// Last valid instant, or `undefined` for a block open on that side.
    #[wasm_bindgen(getter, js_name = validUntil)]
    pub fn valid_until(&self) -> Option<AntexDateTime> {
        self.inner.valid_until.map(|inner| AntexDateTime { inner })
    }

    /// `COMMENT` records inside the block after `TYPE / SERIAL NO`, in file
    /// order.
    #[wasm_bindgen(getter)]
    pub fn comments(&self) -> Vec<String> {
        self.inner.comments.clone()
    }

    /// Frequency labels, e.g. `"G01"`, in file order. A label the file gives
    /// more than one section appears once per section.
    #[wasm_bindgen(getter)]
    pub fn frequencies(&self) -> Vec<String> {
        self.inner
            .frequencies
            .iter()
            .map(|frequency| frequency.frequency.clone())
            .collect()
    }

    /// Every frequency section in file order, each
    /// `{ frequency, pcoM, pcvSamples, rms }`: the `NORTH / EAST / UP` offset
    /// in metres, the PCV grid values in row order, and the
    /// `START OF FREQ RMS` section or `null`.
    #[wasm_bindgen(js_name = frequencySections, unchecked_return_type = "AntexFrequency[]")]
    pub fn frequency_sections(&self) -> Result<JsValue, JsValue> {
        let sections: Vec<FrequencyJs> = self
            .inner
            .frequencies
            .iter()
            .map(FrequencyJs::from)
            .collect();
        to_plain_js(&sections, "ANTEX frequency sections")
    }

    /// The frequency section with the trimmed label. Several sections with the
    /// label must be identical; differing ones are refused with an
    /// `AntexLookupError` whose `detail.kind` is `"AMBIGUOUS_FREQUENCY"`, and
    /// an absent label with `"UNKNOWN_FREQUENCY"`.
    #[wasm_bindgen(unchecked_return_type = "AntexFrequency")]
    pub fn frequency(&self, frequency: &str) -> Result<JsValue, JsValue> {
        let section = self
            .inner
            .frequency(frequency)
            .map_err(antex_lookup_error)?;
        to_plain_js(&FrequencyJs::from(section), "ANTEX frequency section")
    }

    /// Whether this antenna block is valid at `epoch`.
    #[wasm_bindgen(js_name = validAt)]
    pub fn valid_at(&self, epoch: &AntexDateTime) -> bool {
        self.inner.valid_at(epoch.inner)
    }

    /// Frequency-dependent phase-center offset, north/east/up metres, as a
    /// length-3 `Float64Array`. An unknown or ambiguous label throws an
    /// `AntexLookupError`.
    pub fn pco(&self, frequency: &str) -> Result<Vec<f64>, JsValue> {
        self.inner
            .pco(frequency)
            .map(|p| p.to_vec())
            .map_err(antex_lookup_error)
    }

    /// Frequency-dependent phase-center variation, metres. `azimuthDeg` is
    /// optional; without it the no-azimuth interpolation is used. A non-finite
    /// angle is a `RangeError`; an unknown or ambiguous label, a zenith outside
    /// the block's grid or an empty grid throws an `AntexLookupError`.
    pub fn pcv(
        &self,
        frequency: &str,
        zenith_deg: f64,
        azimuth_deg: Option<f64>,
    ) -> Result<f64, JsValue> {
        if !zenith_deg.is_finite() {
            return Err(range_error("zenith_deg must be finite"));
        }
        if let Some(az) = azimuth_deg {
            if !az.is_finite() {
                return Err(range_error("azimuth_deg must be finite"));
            }
        }
        self.inner
            .pcv(frequency, zenith_deg, azimuth_deg)
            .map_err(antex_lookup_error)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AntexVersionJs {
    version: f64,
    system: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PcvTypeRecordJs {
    /// `"absolute"` or `"relative"`.
    pcv_type: &'static str,
    reference_antenna_type: String,
    reference_antenna_serial: String,
    /// The antenna relative values refer to: the stated type, or `AOAD/M_T`
    /// when a relative file leaves it blank; `null` for absolute values.
    reference_antenna: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AntexHeaderJs {
    version: Option<AntexVersionJs>,
    pcv_type: Option<PcvTypeRecordJs>,
    comments: Vec<String>,
    end_of_header: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OuterCommentJs {
    blocks_before: usize,
    text: String,
}

/// A parsed ANTEX receiver and satellite antenna calibration product.
#[wasm_bindgen]
pub struct Antex {
    inner: CoreAntex,
}

impl Antex {
    pub(crate) fn core(&self) -> &CoreAntex {
        &self.inner
    }
}

#[wasm_bindgen]
impl Antex {
    /// Number of distinct `TYPE / SERIAL` ids. Each id's latest block is the
    /// one [`Antex.antenna`] returns; every block is in
    /// [`Antex.antennaBlocks`].
    #[wasm_bindgen(getter, js_name = antennaCount)]
    pub fn antenna_count(&self) -> usize {
        self.inner.antennas.len()
    }

    /// Number of antenna blocks in the file, validity intervals of one id
    /// counted separately.
    #[wasm_bindgen(getter, js_name = blockCount)]
    pub fn block_count(&self) -> usize {
        self.inner.antenna_blocks().count()
    }

    /// ANTEX `TYPE / SERIAL` ids in deterministic order.
    #[wasm_bindgen(getter, js_name = antennaIds)]
    pub fn antenna_ids(&self) -> Vec<String> {
        self.inner.antennas.keys().cloned().collect()
    }

    /// The header records: `{ version, pcvType, comments, endOfHeader }`.
    /// `version` is `ANTEX VERSION / SYST` as `{ version, system }` and
    /// `pcvType` is `PCV TYPE / REFANT` as
    /// `{ pcvType, referenceAntennaType, referenceAntennaSerial,
    /// referenceAntenna }`; each is `null` when the file has no such record,
    /// and `system` is `null` when its column is blank. `pcvType` tells
    /// relative values from absolute ones.
    #[wasm_bindgen(getter, unchecked_return_type = "AntexHeader")]
    pub fn header(&self) -> Result<JsValue, JsValue> {
        let header = &self.inner.header;
        let value = AntexHeaderJs {
            version: header.version.map(|version| AntexVersionJs {
                version: version.version,
                system: version.system.map(String::from),
            }),
            pcv_type: header.pcv_type.as_ref().map(|record| PcvTypeRecordJs {
                pcv_type: match record.pcv_type {
                    PcvType::Absolute => "absolute",
                    PcvType::Relative => "relative",
                },
                reference_antenna_type: record.reference_antenna_type.clone(),
                reference_antenna_serial: record.reference_antenna_serial.clone(),
                reference_antenna: record.reference_antenna().map(String::from),
            }),
            comments: header.comments.clone(),
            end_of_header: header.end_of_header,
        };
        to_plain_js(&value, "ANTEX header")
    }

    /// `COMMENT` records after `END OF HEADER` outside every antenna block,
    /// in file order, each `{ blocksBefore, text }` with `blocksBefore` the
    /// number of antenna blocks before it.
    #[wasm_bindgen(getter, js_name = outerComments, unchecked_return_type = "AntexOuterComment[]")]
    pub fn outer_comments(&self) -> Result<JsValue, JsValue> {
        let rows: Vec<OuterCommentJs> = self
            .inner
            .outer_comments
            .iter()
            .map(|comment| OuterCommentJs {
                blocks_before: comment.blocks_before,
                text: comment.text.clone(),
            })
            .collect();
        to_plain_js(&rows, "ANTEX outer comments")
    }

    /// Number of records the parser skipped or found inconsistent: a corrupt
    /// PCV value, a line outside any record, a `# OF FREQUENCIES` count that
    /// disagrees with the sections read, a header record after
    /// `END OF HEADER`, a block or section its own end record does not close.
    /// A clean file reads with 0.
    #[wasm_bindgen(getter, js_name = skippedRecords)]
    pub fn skipped_records(&self) -> usize {
        self.inner.skipped_records()
    }

    /// Return an antenna by exact `TYPE / SERIAL` id (its latest block), or
    /// `undefined`.
    pub fn antenna(&self, id: &str) -> Option<Antenna> {
        self.inner
            .antenna(id)
            .cloned()
            .map(|inner| Antenna { inner })
    }

    /// Every antenna block in file order.
    #[wasm_bindgen(js_name = antennaBlocks)]
    pub fn antenna_blocks(&self) -> Vec<Antenna> {
        self.inner
            .antenna_blocks()
            .cloned()
            .map(|inner| Antenna { inner })
            .collect()
    }

    /// Every validity block of a `TYPE / SERIAL` id, in file order.
    #[wasm_bindgen(js_name = antennaIntervals)]
    pub fn antenna_intervals(&self, id: &str) -> Vec<Antenna> {
        self.inner
            .antenna_intervals(id)
            .cloned()
            .map(|inner| Antenna { inner })
            .collect()
    }

    /// The block of a `TYPE / SERIAL` id valid at `epoch`, or `undefined`.
    #[wasm_bindgen(js_name = antennaAt)]
    pub fn antenna_at(&self, id: &str, epoch: &AntexDateTime) -> Option<Antenna> {
        self.inner
            .antenna_at(id, epoch.inner)
            .cloned()
            .map(|inner| Antenna { inner })
    }

    /// Return the satellite antenna for `prn` valid at `epoch`, or `undefined`.
    #[wasm_bindgen(js_name = satelliteAntenna)]
    pub fn satellite_antenna(&self, prn: &str, epoch: &AntexDateTime) -> Option<Antenna> {
        self.inner
            .satellite_antenna(prn, epoch.inner)
            .cloned()
            .map(|inner| Antenna { inner })
    }

    /// Serialize to ANTEX 1.4 text. Every record is written from a retained
    /// value, in the order the format lays records out, and no record the
    /// source did not carry is written apart from the start and end records
    /// of blocks and sections. Deterministic: the same product always produces
    /// byte-identical text, and for a product read with no skipped records,
    /// re-parsing the output yields an equal product.
    ///
    /// Throws an `AntexWriteError` whose `detail` is
    /// `{ kind: "UNWRITABLE", field, reason, message }` where a value cannot be
    /// stated in its fixed column without overflow or precision loss (validity
    /// seconds included), a frequency label is not a system flag and a
    /// two-column number, or the product's antennas disagree with the validity
    /// intervals it retains. The writer refuses rather than rounding or
    /// dropping the value.
    #[wasm_bindgen(js_name = toAntexString)]
    pub fn to_antex_string(&self) -> Result<String, JsValue> {
        self.inner.encode().map_err(antex_write_error)
    }
}

/// Why an ANTEX product could not be written, as the `detail` of the thrown
/// `AntexWriteError`.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind")]
enum AntexWriteErrorDetailJs {
    /// A field, record or context the writer cannot state faithfully.
    #[serde(rename = "UNWRITABLE", rename_all = "camelCase")]
    Unwritable {
        field: String,
        reason: String,
        message: String,
    },
    /// Any other ANTEX error the writer returned, with the engine's message in
    /// full. The writer raises only `Unwritable` today.
    #[serde(rename = "OTHER", rename_all = "camelCase")]
    Other { message: String },
}

fn antex_write_error(err: AntexError) -> JsValue {
    let message = err.to_string();
    let detail = match err {
        AntexError::Unwritable { field, reason } => AntexWriteErrorDetailJs::Unwritable {
            field: field.to_string(),
            reason,
            message: message.clone(),
        },
        _ => AntexWriteErrorDetailJs::Other {
            message: message.clone(),
        },
    };
    error_with_detail("AntexWriteError", &message, &detail)
}

/// Why an ANTEX product did not read or a lookup was refused, as the `detail`
/// of the thrown `AntexParseError` or `AntexLookupError`.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind")]
enum AntexErrorDetailJs {
    #[serde(rename = "INVALID_DATE_TIME", rename_all = "camelCase")]
    InvalidDateTime { message: String },
    /// `antennaId` is `null` for a header record.
    #[serde(rename = "INVALID_FIELD", rename_all = "camelCase")]
    InvalidField {
        antenna_id: Option<String>,
        record: String,
        field: String,
        value: String,
        message: String,
    },
    #[serde(rename = "REPEATED_RECORD", rename_all = "camelCase")]
    RepeatedRecord {
        antenna_id: Option<String>,
        record: String,
        message: String,
    },
    #[serde(rename = "DEGENERATE_GRID", rename_all = "camelCase")]
    DegenerateGrid {
        antenna_id: String,
        frequency: String,
        reason: String,
        message: String,
    },
    #[serde(rename = "INVALID_INPUT", rename_all = "camelCase")]
    InvalidInput {
        field: String,
        reason: String,
        message: String,
    },
    #[serde(rename = "UNKNOWN_FREQUENCY", rename_all = "camelCase")]
    UnknownFrequency {
        antenna_id: String,
        frequency: String,
        message: String,
    },
    #[serde(rename = "AMBIGUOUS_FREQUENCY", rename_all = "camelCase")]
    AmbiguousFrequency {
        antenna_id: String,
        frequency: String,
        sections: usize,
        message: String,
    },
    #[serde(rename = "MISSING_PCO", rename_all = "camelCase")]
    MissingPco {
        antenna_id: String,
        frequency: String,
        message: String,
    },
    #[serde(rename = "EMPTY_PCV_GRID", rename_all = "camelCase")]
    EmptyPcvGrid {
        antenna_id: String,
        frequency: String,
        message: String,
    },
    #[serde(rename = "UNWRITABLE", rename_all = "camelCase")]
    Unwritable {
        field: String,
        reason: String,
        message: String,
    },
}

impl AntexErrorDetailJs {
    fn from_core(err: AntexError) -> Self {
        let message = err.to_string();
        match err {
            AntexError::InvalidDateTime => Self::InvalidDateTime { message },
            AntexError::InvalidField {
                antenna_id,
                record,
                field,
                value,
            } => Self::InvalidField {
                antenna_id,
                record: record.to_string(),
                field: field.to_string(),
                value,
                message,
            },
            AntexError::RepeatedRecord { antenna_id, record } => Self::RepeatedRecord {
                antenna_id,
                record: record.to_string(),
                message,
            },
            AntexError::DegenerateGrid {
                antenna_id,
                frequency,
                reason,
            } => Self::DegenerateGrid {
                antenna_id,
                frequency,
                reason,
                message,
            },
            AntexError::InvalidInput { field, reason } => Self::InvalidInput {
                field: field.to_string(),
                reason: reason.to_string(),
                message,
            },
            AntexError::UnknownFrequency {
                antenna_id,
                frequency,
            } => Self::UnknownFrequency {
                antenna_id,
                frequency,
                message,
            },
            AntexError::AmbiguousFrequency {
                antenna_id,
                frequency,
                sections,
            } => Self::AmbiguousFrequency {
                antenna_id,
                frequency,
                sections,
                message,
            },
            AntexError::MissingPco {
                antenna_id,
                frequency,
            } => Self::MissingPco {
                antenna_id,
                frequency,
                message,
            },
            AntexError::EmptyPcvGrid {
                antenna_id,
                frequency,
            } => Self::EmptyPcvGrid {
                antenna_id,
                frequency,
                message,
            },
            AntexError::Unwritable { field, reason } => Self::Unwritable {
                field: field.to_string(),
                reason,
                message,
            },
        }
    }
}

fn antex_error(name: &str, err: AntexError) -> JsValue {
    let message = err.to_string();
    let detail = AntexErrorDetailJs::from_core(err);
    error_with_detail(name, &message, &detail)
}

fn antex_lookup_error(err: AntexError) -> JsValue {
    antex_error("AntexLookupError", err)
}

/// Parse an ANTEX 1.4 antenna product from in-memory bytes (a `Uint8Array`).
///
/// Throws a `TypeError` on non-UTF-8 input and an `AntexParseError` whose
/// `detail` is an `AntexErrorDetail` on a record that does not read: a field
/// that is blank or not a number (`INVALID_FIELD`, naming the record, field
/// and antenna), a once-only record repeated with different content
/// (`REPEATED_RECORD`), a PCV row that would put two values on one grid
/// position (`DEGENERATE_GRID`), a frequency section without a PCO
/// (`MISSING_PCO`). A malformed record the reader can step over is counted
/// in `skippedRecords` instead.
#[wasm_bindgen(js_name = loadAntex)]
pub fn load_antex(bytes: &[u8]) -> Result<Antex, JsValue> {
    let text = utf8_text(bytes, "ANTEX")?;
    let inner = CoreAntex::parse(&text).map_err(|err| antex_error("AntexParseError", err))?;
    Ok(Antex { inner })
}

// The ANTEX record, lookup and refusal shapes. `wasm-pack` writes these into
// both `sidereon.d.ts` targets; `types/sidereon-extra.d.ts` re-exports them.
#[wasm_bindgen(typescript_custom_section)]
const TS_ANTEX_DEFINITIONS: &str = r#"
export type AntexWriteErrorDetail =
  | { kind: "UNWRITABLE"; field: string; reason: string; message: string }
  | { kind: "OTHER"; message: string };

/** `ANTEX VERSION / SYST`; `system` is null when its column is blank. */
export interface AntexVersion {
  version: number;
  system: string | null;
}

/**
 * `PCV TYPE / REFANT`. `referenceAntenna` is the antenna relative values
 * refer to (the stated type, or `AOAD/M_T` when a relative file leaves it
 * blank), and null for absolute values.
 */
export interface AntexPcvType {
  pcvType: "absolute" | "relative";
  referenceAntennaType: string;
  referenceAntennaSerial: string;
  referenceAntenna: string | null;
}

/** The header records; a record the file does not carry is null. */
export interface AntexHeader {
  version: AntexVersion | null;
  pcvType: AntexPcvType | null;
  comments: string[];
  endOfHeader: boolean;
}

/** A comment outside every antenna block, after `blocksBefore` blocks. */
export interface AntexOuterComment {
  blocksBefore: number;
  text: string;
}

/** `METH / BY / # / DATE`; `antennasCalibrated` is null when blank. */
export interface AntexCalibration {
  method: string;
  agency: string;
  antennasCalibrated: number | null;
  date: string;
}

/** One PCV grid value in metres; `azimuthDeg` is null on a `NOAZI` row. */
export interface AntexPcvSample {
  grid: "noAzimuth" | "azimuth";
  azimuthDeg: number | null;
  zenithDeg: number;
  valueM: number;
}

/** A `START OF FREQ RMS` section; `pcoM` is null when it has no offset. */
export interface AntexFrequencyRms {
  pcoM: [number, number, number] | null;
  pcvSamples: AntexPcvSample[];
}

/** One frequency section: north/east/up offset and grid values, metres. */
export interface AntexFrequency {
  frequency: string;
  pcoM: [number, number, number];
  pcvSamples: AntexPcvSample[];
  rms: AntexFrequencyRms | null;
}

/**
 * The `detail` of a thrown `AntexParseError` or `AntexLookupError`.
 * `antennaId` is null for a header record.
 */
export type AntexErrorDetail =
  | { kind: "INVALID_DATE_TIME"; message: string }
  | {
      kind: "INVALID_FIELD";
      antennaId: string | null;
      record: string;
      field: string;
      value: string;
      message: string;
    }
  | { kind: "REPEATED_RECORD"; antennaId: string | null; record: string; message: string }
  | {
      kind: "DEGENERATE_GRID";
      antennaId: string;
      frequency: string;
      reason: string;
      message: string;
    }
  | { kind: "INVALID_INPUT"; field: string; reason: string; message: string }
  | { kind: "UNKNOWN_FREQUENCY"; antennaId: string; frequency: string; message: string }
  | {
      kind: "AMBIGUOUS_FREQUENCY";
      antennaId: string;
      frequency: string;
      sections: number;
      message: string;
    }
  | { kind: "MISSING_PCO"; antennaId: string; frequency: string; message: string }
  | { kind: "EMPTY_PCV_GRID"; antennaId: string; frequency: string; message: string }
  | { kind: "UNWRITABLE"; field: string; reason: string; message: string };
"#;
