//! RINEX 2/3/4 observation parsing, writing and version 2 downgrade. Mirrors
//! the core `rinex::observations` surface: a parsed `RinexObs` with a typed
//! header and per-epoch records, the header in effect at each epoch, columnar
//! numeric series (pseudoranges, raw values, carrier phase) that cross to JS as
//! typed arrays row-aligned to satellite/code string arrays, and the strict
//! writer and downgrade with their typed refusals and changes.
//!
//! Every rule lives in `sidereon_core::rinex_obs`; this module only carries
//! what the core holds across the boundary. Absence is never turned into a
//! zero: a scalar getter that has no value returns `undefined`, a structured
//! object holds `null`, and a numeric typed array holds `NaN` beside a status
//! or validity array wherever `NaN` alone would not say why.

use serde::Serialize;
use wasm_bindgen::prelude::*;

use sidereon_core::rinex::observations::{
    carrier_phase_rows as core_carrier_phase_rows, observation_values as core_observation_values,
    pseudoranges as core_pseudoranges, CarrierPhaseRow as CoreCarrierPhaseRow,
    CorrectionUnavailable, ObsDowngradeChange as CoreObsDowngradeChange, ObsEpoch as CoreObsEpoch,
    ObsEpochTime as CoreObsEpochTime, ObsHeader as CoreObsHeader,
    ObsHeaderTimeline as CoreObsHeaderTimeline, ObsLeapSeconds as CoreObsLeapSeconds,
    ObsPhaseShift as CoreObsPhaseShift, ObsValue as CoreObsValue,
    ObservationFilter as CoreObservationFilter, ObservationKind as CoreObservationKind,
    ObservationValueRow as CoreObservationValueRow, RinexObs as CoreRinexObs,
    RinexObsWriteError as CoreRinexObsWriteError, SignalPolicy as CoreSignalPolicy,
    CYCLE_SLIP_FLAG,
};
use sidereon_core::{GnssSatelliteId, GnssSystem as CoreGnssSystem};

use crate::error::{
    engine_error, error_with_detail, index_arg, range_error, result_object, to_plain_js, utf8_text,
};
use crate::frames::TimeScale;
use crate::gnss::GnssSystem;

/// Observation kind inferred from a RINEX observation code.
#[wasm_bindgen]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ObservationKind {
    /// Code pseudorange, metres.
    Pseudorange,
    /// Carrier phase, cycles.
    CarrierPhase,
    /// Doppler, hertz.
    Doppler,
    /// Signal strength, dB-Hz.
    SignalStrength,
    /// Unknown leading RINEX code letter.
    Unknown,
}

impl From<CoreObservationKind> for ObservationKind {
    fn from(kind: CoreObservationKind) -> Self {
        match kind {
            CoreObservationKind::Pseudorange => Self::Pseudorange,
            CoreObservationKind::CarrierPhase => Self::CarrierPhase,
            CoreObservationKind::Doppler => Self::Doppler,
            CoreObservationKind::SignalStrength => Self::SignalStrength,
            CoreObservationKind::Unknown => Self::Unknown,
        }
    }
}

/// Stable lower-case label for an observation kind.
#[wasm_bindgen(js_name = observationKindLabel)]
pub fn observation_kind_label(kind: ObservationKind) -> String {
    match kind {
        ObservationKind::Pseudorange => "pseudorange",
        ObservationKind::CarrierPhase => "carrier_phase",
        ObservationKind::Doppler => "doppler",
        ObservationKind::SignalStrength => "signal_strength",
        ObservationKind::Unknown => "unknown",
    }
    .to_string()
}

/// The epoch flag whose records report cycle slips (`6`). A flag above 1 that
/// is not this one marks an event, whose records are `ObsEpoch.specialRecords`.
#[wasm_bindgen(js_name = rinexObsCycleSlipFlag)]
pub fn rinex_obs_cycle_slip_flag() -> u8 {
    CYCLE_SLIP_FLAG
}

fn nan_if_missing(value: Option<f64>) -> f64 {
    value.unwrap_or(f64::NAN)
}

fn u8_nan_if_missing(value: Option<u8>) -> f64 {
    value.map(f64::from).unwrap_or(f64::NAN)
}

/// A constellation in a structured object: its RINEX letter, e.g. `"G"`.
fn system_letter(system: CoreGnssSystem) -> String {
    system.letter().to_string()
}

// --- Plain-object shapes -----------------------------------------------------

/// One observation or cycle slip field: the value and its loss-of-lock and
/// signal-strength indicators, each `null` where the field is blank.
#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ObsValueJs {
    value: Option<f64>,
    lli: Option<u8>,
    ssi: Option<u8>,
}

impl From<&CoreObsValue> for ObsValueJs {
    fn from(value: &CoreObsValue) -> Self {
        Self {
            value: value.value,
            lli: value.lli,
            ssi: value.ssi,
        }
    }
}

/// One satellite's fields at an epoch, index-aligned to the header's
/// `obsCodes` for its constellation.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ObsSatelliteValuesJs {
    satellite: String,
    values: Vec<ObsValueJs>,
}

fn satellite_values_js<'a>(
    records: impl Iterator<Item = (&'a GnssSatelliteId, &'a Vec<CoreObsValue>)>,
) -> Vec<ObsSatelliteValuesJs> {
    records
        .map(|(sat, values)| ObsSatelliteValuesJs {
            satellite: sat.to_string(),
            values: values.iter().map(ObsValueJs::from).collect(),
        })
        .collect()
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ObsProgramRunByDateJs {
    program: String,
    run_by: String,
    date: String,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ObsReceiverJs {
    number: String,
    receiver_type: String,
    version: String,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ObsAntennaJs {
    number: String,
    antenna_type: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ObsScaleFactorJs {
    system: String,
    factor: f64,
    codes: Vec<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ObsPrnObsCountJs {
    satellite: String,
    counts: Vec<Option<usize>>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ObsGlonassBiasJs {
    code: String,
    bias_m: Option<f64>,
}

/// The `GLONASS COD/PHS/BIS` bias the header gives one signal.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "status")]
enum GlonassCodePhaseBiasJs {
    /// The header gives the signal this bias, metres.
    #[serde(rename = "available", rename_all = "camelCase")]
    Available { bias_m: f64 },
    /// The header gives the signal no bias: no record, no entry for the code,
    /// or a RINEX 4 file, which ignores the record.
    #[serde(rename = "none")]
    NotGiven,
    /// The header declares the bias unknown: a blank record or a blank bias.
    #[serde(rename = "unknown")]
    Unknown,
    /// One header block gives the signal different biases, in record order,
    /// `null` for a blank one.
    #[serde(rename = "ambiguous", rename_all = "camelCase")]
    Ambiguous { biases_m: Vec<Option<f64>> },
}

/// The `SYS / PHASE SHIFT` correction the header in effect gives one carrier
/// phase row.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "status")]
enum CarrierPhaseShiftJs {
    /// The header gives this correction, cycles. No record, or a blank
    /// correction, is 0, and so is every row of a RINEX 4 file, which ignores
    /// the record.
    #[serde(rename = "available", rename_all = "camelCase")]
    Available { cycles: f64 },
    /// The only record covering the row names just its constellation, which
    /// declares the alignment unknown.
    #[serde(rename = "unknown")]
    Unknown,
    /// Records in one header block give the row different corrections, in
    /// record order, `null` for a blank one.
    #[serde(rename = "ambiguous", rename_all = "camelCase")]
    Ambiguous { corrections: Vec<Option<f64>> },
}

impl CarrierPhaseShiftJs {
    fn from_core(shift: &Result<f64, CorrectionUnavailable>) -> Self {
        match shift {
            Ok(cycles) => Self::Available { cycles: *cycles },
            Err(CorrectionUnavailable::Unknown) => Self::Unknown,
            Err(CorrectionUnavailable::Ambiguous { corrections }) => Self::Ambiguous {
                corrections: corrections.clone(),
            },
        }
    }

    fn status(&self) -> &'static str {
        match self {
            Self::Available { .. } => "available",
            Self::Unknown => "unknown",
            Self::Ambiguous { .. } => "ambiguous",
        }
    }
}

// --- Civil epoch ---------------------------------------------------------------

/// Civil epoch from a RINEX observation file, in the file time scale.
#[wasm_bindgen]
pub struct ObsEpochTime {
    inner: CoreObsEpochTime,
}

#[wasm_bindgen]
impl ObsEpochTime {
    /// Calendar year.
    #[wasm_bindgen(getter)]
    pub fn year(&self) -> i32 {
        self.inner.year
    }

    /// Calendar month, 1..12.
    #[wasm_bindgen(getter)]
    pub fn month(&self) -> u8 {
        self.inner.month
    }

    /// Calendar day of month, 1..31.
    #[wasm_bindgen(getter)]
    pub fn day(&self) -> u8 {
        self.inner.day
    }

    /// Hour of day, 0..23.
    #[wasm_bindgen(getter)]
    pub fn hour(&self) -> u8 {
        self.inner.hour
    }

    /// Minute of hour, 0..59.
    #[wasm_bindgen(getter)]
    pub fn minute(&self) -> u8 {
        self.inner.minute
    }

    /// Fractional seconds of minute.
    #[wasm_bindgen(getter)]
    pub fn second(&self) -> f64 {
        self.inner.second
    }
}

// --- Header records --------------------------------------------------------------

/// One `SYS / PHASE SHIFT` record from a RINEX OBS header.
#[wasm_bindgen]
pub struct ObsPhaseShift {
    inner: CoreObsPhaseShift,
}

#[wasm_bindgen]
impl ObsPhaseShift {
    /// Constellation this record applies to.
    #[wasm_bindgen(getter)]
    pub fn system(&self) -> GnssSystem {
        self.inner.system.into()
    }

    /// RINEX carrier observation code, such as `L1C`, or `undefined` for a
    /// record naming only its constellation, which declares the phase
    /// alignment unknown (RINEX 3.05 section 5.2.12).
    #[wasm_bindgen(getter)]
    pub fn code(&self) -> Option<String> {
        self.inner.code.clone()
    }

    /// Phase correction in carrier cycles, or `undefined` where the record
    /// leaves the field blank ("Correction applied (cycles) or blank if none").
    #[wasm_bindgen(getter, js_name = correctionCycles)]
    pub fn correction_cycles(&self) -> Option<f64> {
        self.inner.correction_cycles
    }

    /// Satellite tokens this record is restricted to. With
    /// `unrepresentableSatellites` also empty, the record applies to every
    /// satellite of its constellation and code.
    #[wasm_bindgen(getter)]
    pub fn satellites(&self) -> Vec<String> {
        self.inner
            .satellites
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    /// Satellites the record names by a well-formed RINEX designator no
    /// satellite id holds, such as `R28`, as written. The correction applies to
    /// none of them; they are kept so the record is written back whole.
    #[wasm_bindgen(getter, js_name = unrepresentableSatellites)]
    pub fn unrepresentable_satellites(&self) -> Vec<String> {
        self.inner.unrepresentable_satellites.clone()
    }

    /// Whether the record applies to every satellite of its constellation and
    /// code: it names no satellite, representable or not.
    #[wasm_bindgen(getter, js_name = coversEverySatellite)]
    pub fn covers_every_satellite(&self) -> bool {
        self.inner.covers_every_satellite()
    }

    /// How many satellites the record names, representable or not.
    #[wasm_bindgen(getter, js_name = satelliteCount)]
    pub fn satellite_count(&self) -> usize {
        self.inner.satellite_count()
    }
}

/// The `LEAP SECONDS` header record. Its integer fields cross as `bigint`, so
/// every value the engine holds arrives exactly.
#[wasm_bindgen]
pub struct ObsLeapSeconds {
    inner: CoreObsLeapSeconds,
}

#[wasm_bindgen]
impl ObsLeapSeconds {
    /// Current leap-second count.
    #[wasm_bindgen(getter)]
    pub fn current(&self) -> i64 {
        self.inner.current
    }

    /// Future or past leap-second count, or `undefined` where the field is
    /// blank.
    #[wasm_bindgen(getter, js_name = deltaFuture)]
    pub fn delta_future(&self) -> Option<i64> {
        self.inner.delta_future
    }

    /// Week number of the future or past count, or `undefined` where blank.
    #[wasm_bindgen(getter)]
    pub fn week(&self) -> Option<i64> {
        self.inner.week
    }

    /// Day number of the future or past count, or `undefined` where blank.
    #[wasm_bindgen(getter)]
    pub fn day(&self) -> Option<i64> {
        self.inner.day
    }

    /// The time system identifier the record states (`GPS`, `BDS` or `BDT`),
    /// exactly as written, or `undefined` where the field is blank. A blank
    /// field and an explicit `GPS` stay distinct.
    #[wasm_bindgen(getter, js_name = timeSystem)]
    pub fn time_system(&self) -> Option<String> {
        self.inner.time_system.clone()
    }
}

/// Parsed RINEX OBS header: the file header, or the header in effect at one
/// epoch from `RinexObs.headerAt` or an `ObsHeaderTimeline`.
#[wasm_bindgen]
pub struct ObsHeader {
    inner: CoreObsHeader,
}

impl ObsHeader {
    fn from_core(inner: CoreObsHeader) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl ObsHeader {
    /// RINEX version, for example `3.05`.
    #[wasm_bindgen(getter)]
    pub fn version(&self) -> f64 {
        self.inner.version
    }

    /// Surveyed a-priori ECEF position `[x, y, z]`, metres, or `undefined`.
    #[wasm_bindgen(getter, js_name = approxPositionM)]
    pub fn approx_position_m(&self) -> Option<Vec<f64>> {
        self.inner.approx_position_m.map(|p| p.to_vec())
    }

    /// Antenna H/E/N offset `[h, e, n]`, metres, or `undefined`.
    #[wasm_bindgen(getter, js_name = antennaDeltaHenM)]
    pub fn antenna_delta_hen_m(&self) -> Option<Vec<f64>> {
        self.inner.antenna_delta_hen_m.map(|d| d.to_vec())
    }

    /// Nominal epoch interval, seconds, or `undefined`. RINEX permits zero for
    /// an unknown interval, so a non-positive value is not a cadence.
    #[wasm_bindgen(getter, js_name = intervalS)]
    pub fn interval_s(&self) -> Option<f64> {
        self.inner.interval_s
    }

    /// Marker or station name, or `undefined`.
    #[wasm_bindgen(getter, js_name = markerName)]
    pub fn marker_name(&self) -> Option<String> {
        self.inner.marker_name.clone()
    }

    /// Marker number, or `undefined`.
    #[wasm_bindgen(getter, js_name = markerNumber)]
    pub fn marker_number(&self) -> Option<String> {
        self.inner.marker_number.clone()
    }

    /// Marker type, or `undefined`.
    #[wasm_bindgen(getter, js_name = markerType)]
    pub fn marker_type(&self) -> Option<String> {
        self.inner.marker_type.clone()
    }

    /// Observer name, or `undefined`.
    #[wasm_bindgen(getter)]
    pub fn observer(&self) -> Option<String> {
        self.inner.observer.clone()
    }

    /// Observer agency, or `undefined`.
    #[wasm_bindgen(getter)]
    pub fn agency(&self) -> Option<String> {
        self.inner.agency.clone()
    }

    /// Signal-strength unit, e.g. `DBHZ`, or `undefined`.
    #[wasm_bindgen(getter, js_name = signalStrengthUnit)]
    pub fn signal_strength_unit(&self) -> Option<String> {
        self.inner.signal_strength_unit.clone()
    }

    /// Declared distinct-satellite count (`# OF SATELLITES`), or `undefined`.
    #[wasm_bindgen(getter, js_name = declaredSatelliteCount)]
    pub fn declared_satellite_count(&self) -> Option<usize> {
        self.inner.n_satellites
    }

    /// `PGM / RUN BY / DATE` as `{ program, runBy, date }`, or `null`.
    #[wasm_bindgen(getter, js_name = programRunByDate, unchecked_return_type = "ObsProgramRunByDate | null")]
    pub fn program_run_by_date(&self) -> Result<JsValue, JsValue> {
        let record = self
            .inner
            .program_run_by_date
            .as_ref()
            .map(|record| ObsProgramRunByDateJs {
                program: record.program.clone(),
                run_by: record.run_by.clone(),
                date: record.date.clone(),
            });
        to_plain_js(&record, "RINEX OBS PGM / RUN BY / DATE")
    }

    /// Header comments in file order.
    #[wasm_bindgen(getter)]
    pub fn comments(&self) -> Vec<String> {
        self.inner.comments.clone()
    }

    /// `REC # / TYPE / VERS` as `{ number, receiverType, version }`, or `null`.
    #[wasm_bindgen(getter, unchecked_return_type = "ObsReceiver | null")]
    pub fn receiver(&self) -> Result<JsValue, JsValue> {
        let record = self.inner.receiver.as_ref().map(|record| ObsReceiverJs {
            number: record.number.clone(),
            receiver_type: record.receiver_type.clone(),
            version: record.version.clone(),
        });
        to_plain_js(&record, "RINEX OBS REC # / TYPE / VERS")
    }

    /// `ANT # / TYPE` as `{ number, antennaType }`, or `null`.
    #[wasm_bindgen(getter, unchecked_return_type = "ObsAntenna | null")]
    pub fn antenna(&self) -> Result<JsValue, JsValue> {
        let record = self.inner.antenna.as_ref().map(|record| ObsAntennaJs {
            number: record.number.clone(),
            antenna_type: record.antenna_type.clone(),
        });
        to_plain_js(&record, "RINEX OBS ANT # / TYPE")
    }

    /// Constellations with observation-code lists.
    #[wasm_bindgen(getter)]
    pub fn systems(&self) -> Vec<GnssSystem> {
        self.inner
            .obs_codes
            .keys()
            .copied()
            .map(Into::into)
            .collect()
    }

    /// The version 2 `# / TYPES OF OBSERV` names as read, in order; empty at
    /// version 3. In a header from `RinexObs.headerAt`, the names in effect at
    /// that epoch.
    #[wasm_bindgen(getter, js_name = rinex2Types)]
    pub fn rinex2_types(&self) -> Vec<String> {
        self.inner.rinex2_types.clone()
    }

    /// The constellation a version 2 file's version record names, or
    /// `undefined` for a mixed file and at version 3.
    #[wasm_bindgen(getter, js_name = rinex2System)]
    pub fn rinex2_system(&self) -> Option<GnssSystem> {
        self.inner.rinex2_system.map(Into::into)
    }

    /// Carrier phase-shift records, in header order.
    #[wasm_bindgen(getter, js_name = phaseShifts)]
    pub fn phase_shifts(&self) -> Vec<ObsPhaseShift> {
        self.inner
            .phase_shifts
            .iter()
            .cloned()
            .map(|inner| ObsPhaseShift { inner })
            .collect()
    }

    /// `SYS / SCALE FACTOR` records in header order, each
    /// `{ system, factor, codes }`; an empty `codes` covers every code of the
    /// constellation. Stored values are already divided by their factor.
    #[wasm_bindgen(getter, js_name = scaleFactors, unchecked_return_type = "ObsScaleFactor[]")]
    pub fn scale_factors(&self) -> Result<JsValue, JsValue> {
        let records: Vec<ObsScaleFactorJs> = self
            .inner
            .scale_factors
            .iter()
            .map(|record| ObsScaleFactorJs {
                system: system_letter(record.system),
                factor: record.factor,
                codes: record.codes.clone(),
            })
            .collect();
        to_plain_js(&records, "RINEX OBS SYS / SCALE FACTOR records")
    }

    /// GLONASS slot/frequency-channel pairs, flat `[slot0, chan0, slot1, ...]`.
    #[wasm_bindgen(getter, js_name = glonassSlots)]
    pub fn glonass_slots(&self) -> Vec<i32> {
        self.inner
            .glonass_slots
            .iter()
            .flat_map(|(&slot, &channel)| [i32::from(slot), i32::from(channel)])
            .collect()
    }

    /// The `GLONASS COD/PHS/BIS` entries as written, each `{ code, biasM }`
    /// with `biasM` `null` for a blank bias; `[]` for a blank record, which
    /// declares the biases unknown; `null` where the header carries no record.
    #[wasm_bindgen(getter, js_name = glonassCodPhsBis, unchecked_return_type = "ObsGlonassBias[] | null")]
    pub fn glonass_cod_phs_bis(&self) -> Result<JsValue, JsValue> {
        let entries = self.inner.glonass_cod_phs_bis.as_ref().map(|entries| {
            entries
                .iter()
                .map(|(code, bias)| ObsGlonassBiasJs {
                    code: code.clone(),
                    bias_m: *bias,
                })
                .collect::<Vec<_>>()
        });
        to_plain_js(&entries, "RINEX OBS GLONASS COD/PHS/BIS record")
    }

    /// The code-phase bias the header gives a GLONASS signal, as a
    /// `GlonassCodePhaseBias` whose `status` is `available` (with `biasM`),
    /// `none`, `unknown` (a blank record or bias) or `ambiguous` (with every
    /// bias one block gives, `null` for a blank one). A RINEX 4 header gives
    /// none, as RINEX 4.00 Table A2 tells decoders to ignore the record.
    #[wasm_bindgen(js_name = glonassCodePhaseBias, unchecked_return_type = "GlonassCodePhaseBias")]
    pub fn glonass_code_phase_bias(&self, code: &str) -> Result<JsValue, JsValue> {
        let bias = match self.inner.glonass_code_phase_bias(code) {
            Ok(Some(bias_m)) => GlonassCodePhaseBiasJs::Available { bias_m },
            Ok(None) => GlonassCodePhaseBiasJs::NotGiven,
            Err(CorrectionUnavailable::Unknown) => GlonassCodePhaseBiasJs::Unknown,
            Err(CorrectionUnavailable::Ambiguous { corrections }) => {
                GlonassCodePhaseBiasJs::Ambiguous {
                    biases_m: corrections,
                }
            }
        };
        to_plain_js(&bias, "RINEX OBS GLONASS code-phase bias")
    }

    /// The `LEAP SECONDS` record, or `undefined`.
    #[wasm_bindgen(getter, js_name = leapSeconds)]
    pub fn leap_seconds(&self) -> Option<ObsLeapSeconds> {
        self.inner
            .leap_seconds
            .clone()
            .map(|inner| ObsLeapSeconds { inner })
    }

    /// First observation epoch, or `undefined`.
    #[wasm_bindgen(getter, js_name = timeOfFirstObsEpoch)]
    pub fn time_of_first_obs_epoch(&self) -> Option<ObsEpochTime> {
        self.inner
            .time_of_first_obs
            .map(|(epoch, _)| ObsEpochTime { inner: epoch })
    }

    /// Time scale of the first observation epoch, or `undefined`.
    #[wasm_bindgen(getter, js_name = timeOfFirstObsScale)]
    pub fn time_of_first_obs_scale(&self) -> Option<TimeScale> {
        self.inner.time_of_first_obs.map(|(_, scale)| scale.into())
    }

    /// Last observation epoch, or `undefined`.
    #[wasm_bindgen(getter, js_name = timeOfLastObsEpoch)]
    pub fn time_of_last_obs_epoch(&self) -> Option<ObsEpochTime> {
        self.inner
            .time_of_last_obs
            .map(|(epoch, _)| ObsEpochTime { inner: epoch })
    }

    /// Time scale of the last observation epoch, or `undefined`.
    #[wasm_bindgen(getter, js_name = timeOfLastObsScale)]
    pub fn time_of_last_obs_scale(&self) -> Option<TimeScale> {
        self.inner.time_of_last_obs.map(|(_, scale)| scale.into())
    }

    /// `PRN / # OF OBS` counts, each `{ satellite, counts }` with `counts`
    /// index-aligned to the constellation's `obsCodes` and `null` for a blank
    /// count.
    #[wasm_bindgen(getter, js_name = prnObsCounts, unchecked_return_type = "ObsPrnObsCount[]")]
    pub fn prn_obs_counts(&self) -> Result<JsValue, JsValue> {
        let counts: Vec<ObsPrnObsCountJs> = self
            .inner
            .prn_obs_counts
            .iter()
            .map(|(sat, counts)| ObsPrnObsCountJs {
                satellite: sat.to_string(),
                counts: counts.clone(),
            })
            .collect();
        to_plain_js(&counts, "RINEX OBS PRN / # OF OBS counts")
    }

    /// Header labels read and not retained, which a rewrite does not carry.
    #[wasm_bindgen(getter, js_name = unretainedHeaderLabels)]
    pub fn unretained_header_labels(&self) -> Vec<String> {
        self.inner.unretained_header_labels.clone()
    }

    /// Observation codes for a constellation: the union of every list the file
    /// declares for it, the file header's codes first. Every epoch's values
    /// and cycle slips are index-aligned to this list. Empty where the header
    /// holds no list for the constellation.
    #[wasm_bindgen(js_name = obsCodes)]
    pub fn obs_codes(&self, system: GnssSystem) -> Vec<String> {
        self.inner
            .obs_codes
            .get(&system.into())
            .cloned()
            .unwrap_or_default()
    }

    /// The code list this header itself declares for a constellation, in
    /// declared order: the file header's list, or in a header from
    /// `RinexObs.headerAt` the list in effect at that epoch. `undefined` where
    /// it declares none.
    #[wasm_bindgen(js_name = declaredObsCodes)]
    pub fn declared_obs_codes(&self, system: GnssSystem) -> Option<Vec<String>> {
        self.inner
            .declared_obs_codes
            .get(&CoreGnssSystem::from(system))
            .cloned()
    }
}

// --- Epochs ------------------------------------------------------------------------

/// One RINEX OBS epoch record. Labelled observation rows are read through
/// `RinexObs` methods; the raw fields are here.
#[wasm_bindgen]
pub struct ObsEpoch {
    inner: CoreObsEpoch,
}

#[wasm_bindgen]
impl ObsEpoch {
    /// Civil epoch in the file time scale, or `undefined` for an event whose
    /// epoch fields are blank. RINEX lets an event without a significant epoch
    /// leave them blank; an observation or cycle slip epoch always has one.
    #[wasm_bindgen(getter)]
    pub fn epoch(&self) -> Option<ObsEpochTime> {
        self.inner.epoch.map(|inner| ObsEpochTime { inner })
    }

    /// RINEX epoch flag: `0` an observation epoch, `1` a power failure, `6`
    /// cycle slip records (`rinexObsCycleSlipFlag()`), and any other flag
    /// above 1 an event.
    #[wasm_bindgen(getter)]
    pub fn flag(&self) -> u8 {
        self.inner.flag
    }

    /// Receiver clock offset from the epoch line, seconds, or `undefined`.
    #[wasm_bindgen(getter, js_name = rcvClockOffsetS)]
    pub fn rcv_clock_offset_s(&self) -> Option<f64> {
        self.inner.rcv_clock_offset_s
    }

    /// RINEX 4.02 epoch picoseconds, or `undefined`.
    #[wasm_bindgen(getter, js_name = epochPicoseconds)]
    pub fn epoch_picoseconds(&self) -> Option<u32> {
        self.inner.epoch_picoseconds
    }

    /// The satellite or special-record count the epoch line declared.
    #[wasm_bindgen(getter, js_name = declaredRecordCount)]
    pub fn declared_record_count(&self) -> usize {
        self.inner.declared_record_count
    }

    /// The records an event epoch carried, verbatim and in order; empty for an
    /// observation or cycle slip epoch. Header records among them take effect
    /// for the epochs after the event (see `RinexObs.headerAt`).
    #[wasm_bindgen(getter, js_name = specialRecords)]
    pub fn special_records(&self) -> Vec<String> {
        self.inner.special_records.clone()
    }

    /// Satellite tokens with observations at this epoch, ascending. Empty for
    /// an event or cycle slip epoch.
    #[wasm_bindgen(getter)]
    pub fn satellites(&self) -> Vec<String> {
        self.inner.sats.keys().map(|s| s.to_string()).collect()
    }

    /// Number of satellites with observations at this epoch.
    #[wasm_bindgen(getter, js_name = satelliteCount)]
    pub fn satellite_count(&self) -> usize {
        self.inner.sats.len()
    }

    /// Every observation field at this epoch, per satellite, index-aligned to
    /// the header's `obsCodes` for its constellation and `null` where blank.
    #[wasm_bindgen(getter, unchecked_return_type = "ObsSatelliteValues[]")]
    pub fn observations(&self) -> Result<JsValue, JsValue> {
        to_plain_js(
            &satellite_values_js(self.inner.sats.iter()),
            "RINEX OBS observations",
        )
    }

    /// Satellite tokens a flag 6 epoch reports cycle slips for, ascending.
    #[wasm_bindgen(getter, js_name = cycleSlipSatellites)]
    pub fn cycle_slip_satellites(&self) -> Vec<String> {
        self.inner
            .cycle_slips
            .keys()
            .map(|s| s.to_string())
            .collect()
    }

    /// The cycle slips a flag 6 epoch reports, per satellite, index-aligned to
    /// the header's `obsCodes` as `observations` is, `null` where blank. Slips
    /// are held apart from observations and feed no measurement consumer.
    /// Empty for every other flag.
    #[wasm_bindgen(getter, js_name = cycleSlips, unchecked_return_type = "ObsSatelliteValues[]")]
    pub fn cycle_slips(&self) -> Result<JsValue, JsValue> {
        to_plain_js(
            &satellite_values_js(self.inner.cycle_slips.iter()),
            "RINEX OBS cycle slips",
        )
    }
}

/// The header in effect at every epoch of a product, built once by
/// `RinexObs.headerTimeline()` so a loop over the epochs looks each one up.
#[wasm_bindgen]
pub struct ObsHeaderTimeline {
    inner: CoreObsHeaderTimeline,
}

#[wasm_bindgen]
impl ObsHeaderTimeline {
    /// The header in effect at an epoch index: the file header with every
    /// event at or before it laid over it. Past the last epoch, the header
    /// after every event; `RinexObs.headerAt` refuses such an index instead.
    pub fn at(&self, epoch_index: f64) -> Result<ObsHeader, JsValue> {
        let index = index_arg(epoch_index, "epochIndex")?;
        Ok(ObsHeader::from_core(self.inner.at(index).clone()))
    }

    /// The position, in `segments` order, of the header in effect at an epoch
    /// index.
    #[wasm_bindgen(js_name = segmentIndex)]
    pub fn segment_index(&self, epoch_index: f64) -> Result<usize, JsValue> {
        let index = index_arg(epoch_index, "epochIndex")?;
        Ok(self.inner.segment_index(index))
    }

    /// Each header with the index of the first epoch it is in effect at, in
    /// file order, beginning with the file header at index 0. An event at
    /// epoch 0 gives a second segment at index 0; segments are neither merged
    /// nor reordered.
    #[wasm_bindgen(getter)]
    pub fn segments(&self) -> Vec<ObsHeaderSegment> {
        self.inner
            .segments()
            .map(|(first_epoch_index, header)| ObsHeaderSegment {
                first_epoch_index,
                header: header.clone(),
            })
            .collect()
    }

    /// Number of segments, the file header included.
    #[wasm_bindgen(getter, js_name = segmentCount)]
    pub fn segment_count(&self) -> usize {
        self.inner.segments().count()
    }
}

/// One header of an `ObsHeaderTimeline` and the first epoch it is in effect at.
#[wasm_bindgen]
pub struct ObsHeaderSegment {
    first_epoch_index: usize,
    header: CoreObsHeader,
}

#[wasm_bindgen]
impl ObsHeaderSegment {
    /// Zero-based index of the first epoch this header is in effect at.
    #[wasm_bindgen(getter, js_name = firstEpochIndex)]
    pub fn first_epoch_index(&self) -> usize {
        self.first_epoch_index
    }

    /// The header in effect from `firstEpochIndex`.
    #[wasm_bindgen(getter)]
    pub fn header(&self) -> ObsHeader {
        ObsHeader::from_core(self.header.clone())
    }
}

// --- Filters and policies ------------------------------------------------------------

/// Optional observation-code allow-list for raw and carrier-phase rows. Build
/// with `new ObservationFilter()` then chain `.withSystem(system, codes)`.
#[wasm_bindgen]
#[derive(Clone)]
pub struct ObservationFilter {
    inner: CoreObservationFilter,
}

#[wasm_bindgen]
impl ObservationFilter {
    /// An empty filter that keeps every parsed observation.
    #[wasm_bindgen(constructor)]
    pub fn new() -> ObservationFilter {
        ObservationFilter {
            inner: CoreObservationFilter::all(),
        }
    }

    /// Return a copy with one constellation's code allow-list set.
    #[wasm_bindgen(js_name = withSystem)]
    pub fn with_system(&self, system: GnssSystem, codes: Vec<String>) -> ObservationFilter {
        let mut map = self.inner.codes.clone();
        map.insert(system.into(), codes);
        ObservationFilter {
            inner: CoreObservationFilter::from_entries(map),
        }
    }
}

impl Default for ObservationFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-constellation single-frequency pseudorange code-selection policy. Build
/// with `new SignalPolicy()` then chain `.withSystem(system, codes)`, or use
/// the static `defaultFor(version)`.
#[wasm_bindgen]
#[derive(Clone)]
pub struct SignalPolicy {
    inner: CoreSignalPolicy,
}

#[wasm_bindgen]
impl SignalPolicy {
    /// An empty policy with no constellation preferences.
    #[wasm_bindgen(constructor)]
    pub fn new() -> SignalPolicy {
        SignalPolicy {
            inner: CoreSignalPolicy {
                codes: Default::default(),
            },
        }
    }

    /// The core default pseudorange policy for a RINEX version.
    #[wasm_bindgen(js_name = defaultFor)]
    pub fn default_for(version: f64) -> Result<SignalPolicy, JsValue> {
        Ok(SignalPolicy {
            inner: CoreSignalPolicy::default_for(version).map_err(engine_error)?,
        })
    }

    /// Return a copy with one constellation's preference list set.
    #[wasm_bindgen(js_name = withSystem)]
    pub fn with_system(&self, system: GnssSystem, codes: Vec<String>) -> SignalPolicy {
        SignalPolicy {
            inner: self.inner.clone().with_override(system.into(), codes),
        }
    }
}

impl Default for SignalPolicy {
    fn default() -> Self {
        Self::new()
    }
}

// --- Series --------------------------------------------------------------------------

/// Flattened pseudorange rows from one RINEX OBS epoch. `satellites` is
/// row-aligned with `rangesM`; ranges are metres.
#[wasm_bindgen]
pub struct PseudorangeSeries {
    satellites: Vec<String>,
    ranges_m: Vec<f64>,
}

#[wasm_bindgen]
impl PseudorangeSeries {
    /// Satellite tokens, row-aligned with `rangesM`.
    #[wasm_bindgen(getter)]
    pub fn satellites(&self) -> Vec<String> {
        self.satellites.clone()
    }

    /// Pseudorange values, metres, as a `Float64Array`.
    #[wasm_bindgen(getter, js_name = rangesM)]
    pub fn ranges_m(&self) -> Vec<f64> {
        self.ranges_m.clone()
    }

    /// Number of rows.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> usize {
        self.ranges_m.len()
    }
}

/// Flattened raw observation rows from one RINEX OBS epoch. Numeric arrays are
/// row-aligned with `satellites`, `codes`, and `kinds`; blank RINEX values, LLI,
/// and SSI are `NaN`. The reader refuses a non-finite value, so `NaN` here
/// always means a blank field.
#[wasm_bindgen]
pub struct ObservationValueSeries {
    satellites: Vec<String>,
    codes: Vec<String>,
    kinds: Vec<ObservationKind>,
    values: Vec<f64>,
    lli: Vec<f64>,
    ssi: Vec<f64>,
}

#[wasm_bindgen]
impl ObservationValueSeries {
    /// Satellite tokens, row-aligned with all arrays.
    #[wasm_bindgen(getter)]
    pub fn satellites(&self) -> Vec<String> {
        self.satellites.clone()
    }

    /// RINEX observation codes, row-aligned with all arrays.
    #[wasm_bindgen(getter)]
    pub fn codes(&self) -> Vec<String> {
        self.codes.clone()
    }

    /// Observation kinds, row-aligned with all arrays.
    #[wasm_bindgen(getter)]
    pub fn kinds(&self) -> Vec<ObservationKind> {
        self.kinds.clone()
    }

    /// Parsed observation values, as a `Float64Array`.
    #[wasm_bindgen(getter)]
    pub fn values(&self) -> Vec<f64> {
        self.values.clone()
    }

    /// RINEX LLI values, `NaN` for blanks.
    #[wasm_bindgen(getter)]
    pub fn lli(&self) -> Vec<f64> {
        self.lli.clone()
    }

    /// RINEX SSI values, `NaN` for blanks.
    #[wasm_bindgen(getter)]
    pub fn ssi(&self) -> Vec<f64> {
        self.ssi.clone()
    }

    /// Number of flattened rows.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> usize {
        self.values.len()
    }
}

/// Flattened carrier-phase rows from one RINEX OBS epoch, read with the header
/// in effect at that epoch. Numeric arrays are row-aligned with `satellites`
/// and `codes`; a missing value and unknown carrier metadata are `NaN`.
///
/// Every row is kept whatever its phase-shift correction. `phaseShiftCycles`
/// holds the correction where the header gives one and `NaN` where it does
/// not; `phaseShiftAvailable` is 1 or 0 beside it, `phaseShiftStatus` says
/// `available`, `unknown` or `ambiguous`, and `phaseShiftCorrections` holds
/// each row's full status, the conflicting values of an ambiguous one
/// included. An unavailable correction is never reported as 0.
#[wasm_bindgen]
pub struct CarrierPhaseSeries {
    satellites: Vec<String>,
    codes: Vec<String>,
    value_cycles: Vec<f64>,
    frequency_hz: Vec<f64>,
    wavelength_m: Vec<f64>,
    value_m: Vec<f64>,
    phase_shift_cycles: Vec<f64>,
    phase_shift_available: Vec<u8>,
    phase_shift_status: Vec<String>,
    phase_shift_corrections: Vec<CarrierPhaseShiftJs>,
    lli: Vec<f64>,
    ssi: Vec<f64>,
}

#[wasm_bindgen]
impl CarrierPhaseSeries {
    /// Satellite tokens, row-aligned with all arrays.
    #[wasm_bindgen(getter)]
    pub fn satellites(&self) -> Vec<String> {
        self.satellites.clone()
    }

    /// RINEX carrier observation codes, row-aligned with all arrays.
    #[wasm_bindgen(getter)]
    pub fn codes(&self) -> Vec<String> {
        self.codes.clone()
    }

    /// Carrier phase, cycles.
    #[wasm_bindgen(getter, js_name = valueCycles)]
    pub fn value_cycles(&self) -> Vec<f64> {
        self.value_cycles.clone()
    }

    /// Carrier frequency, hertz.
    #[wasm_bindgen(getter, js_name = frequencyHz)]
    pub fn frequency_hz(&self) -> Vec<f64> {
        self.frequency_hz.clone()
    }

    /// Carrier wavelength, metres.
    #[wasm_bindgen(getter, js_name = wavelengthM)]
    pub fn wavelength_m(&self) -> Vec<f64> {
        self.wavelength_m.clone()
    }

    /// Carrier phase, metres.
    #[wasm_bindgen(getter, js_name = valueM)]
    pub fn value_m(&self) -> Vec<f64> {
        self.value_m.clone()
    }

    /// Reported `SYS / PHASE SHIFT` correction, cycles, where the header in
    /// effect gives one, and `NaN` where it does not (see `phaseShiftStatus`).
    /// RINEX 3 phases are already aligned, so it is metadata and is not
    /// re-applied to `valueCycles`.
    #[wasm_bindgen(getter, js_name = phaseShiftCycles)]
    pub fn phase_shift_cycles(&self) -> Vec<f64> {
        self.phase_shift_cycles.clone()
    }

    /// 1 where `phaseShiftCycles` holds a correction, 0 where it is `NaN`.
    #[wasm_bindgen(getter, js_name = phaseShiftAvailable)]
    pub fn phase_shift_available(&self) -> Vec<u8> {
        self.phase_shift_available.clone()
    }

    /// Each row's correction status: `available`, `unknown` (the only record
    /// covering it names just its constellation) or `ambiguous` (one block
    /// gives it different corrections).
    #[wasm_bindgen(getter, js_name = phaseShiftStatus, unchecked_return_type = "CorrectionStatus[]")]
    pub fn phase_shift_status(&self) -> Vec<String> {
        self.phase_shift_status.clone()
    }

    /// Each row's correction as a `CarrierPhaseShift`: the cycles where
    /// available, and every conflicting value, `null` for a blank one, where
    /// ambiguous.
    #[wasm_bindgen(getter, js_name = phaseShiftCorrections, unchecked_return_type = "CarrierPhaseShift[]")]
    pub fn phase_shift_corrections(&self) -> Result<JsValue, JsValue> {
        to_plain_js(
            &self.phase_shift_corrections,
            "RINEX OBS carrier phase-shift corrections",
        )
    }

    /// RINEX LLI values, `NaN` for blanks.
    #[wasm_bindgen(getter)]
    pub fn lli(&self) -> Vec<f64> {
        self.lli.clone()
    }

    /// RINEX SSI values, `NaN` for blanks.
    #[wasm_bindgen(getter)]
    pub fn ssi(&self) -> Vec<f64> {
        self.ssi.clone()
    }

    /// Number of flattened rows.
    #[wasm_bindgen(getter)]
    pub fn length(&self) -> usize {
        self.value_cycles.len()
    }
}

// --- Writer refusals and downgrade changes -------------------------------------------

/// Why a product could not be written or downgraded, as the `detail` of a
/// thrown `RinexObsWriteError`: a discriminated union on `kind`, each variant
/// carrying the engine's payload and message. A constellation is its RINEX
/// letter and a satellite its token.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind")]
enum RinexObsWriteErrorDetailJs {
    #[serde(rename = "CODE_LISTS_NOT_VERSION_TWO", rename_all = "camelCase")]
    CodeListsNotVersionTwo {
        system: String,
        position: usize,
        code: Option<String>,
        message: String,
    },
    #[serde(rename = "NOT_VERSION_TWO", rename_all = "camelCase")]
    NotVersionTwo { version: f64, message: String },
    #[serde(rename = "SCALE_FACTORS_IN_VERSION_TWO", rename_all = "camelCase")]
    ScaleFactorsInVersionTwo { count: usize, message: String },
    #[serde(rename = "VALUES_WITHOUT_CODES", rename_all = "camelCase")]
    ValuesWithoutCodes {
        epoch_index: usize,
        satellite: String,
        codes: usize,
        values: usize,
        message: String,
    },
    #[serde(rename = "COUNTS_WITHOUT_CODES", rename_all = "camelCase")]
    CountsWithoutCodes {
        satellite: String,
        codes: usize,
        counts: usize,
        message: String,
    },
    #[serde(rename = "CODE_LIST_NOT_STATED", rename_all = "camelCase")]
    CodeListNotStated { system: String, message: String },
    #[serde(rename = "EPOCH_FLAG_TOO_WIDE", rename_all = "camelCase")]
    EpochFlagTooWide {
        epoch_index: usize,
        flag: u8,
        message: String,
    },
    #[serde(rename = "EPOCH_TIME_MISSING", rename_all = "camelCase")]
    EpochTimeMissing {
        epoch_index: usize,
        flag: u8,
        message: String,
    },
    #[serde(rename = "EPOCH_PICOSECONDS_NOT_IN_VERSION", rename_all = "camelCase")]
    EpochPicosecondsNotInVersion {
        epoch_index: usize,
        version: f64,
        message: String,
    },
    #[serde(rename = "TOO_MANY_OBSERVATION_TYPES", rename_all = "camelCase")]
    TooManyObservationTypes { count: usize, message: String },
    #[serde(rename = "CODE_LISTS_NOT_UNION", rename_all = "camelCase")]
    CodeListsNotUnion { system: String, message: String },
    #[serde(rename = "VALUE_OUTSIDE_DECLARED_LIST", rename_all = "camelCase")]
    ValueOutsideDeclaredList {
        epoch_index: usize,
        satellite: String,
        code: Option<String>,
        message: String,
    },
    #[serde(rename = "DECLARED_LIST_NOT_STATED", rename_all = "camelCase")]
    DeclaredListNotStated { system: String, message: String },
    /// The engine's field is `message`; it crosses as `readerError` so it does
    /// not collide with the error's own `message`.
    #[serde(rename = "EVENT_RECORDS_UNREADABLE", rename_all = "camelCase")]
    EventRecordsUnreadable {
        reader_error: String,
        message: String,
    },
    #[serde(rename = "OBSERVABLE_NOT_REPRESENTABLE", rename_all = "camelCase")]
    ObservableNotRepresentable {
        system: String,
        code: String,
        version: f64,
        message: String,
    },
    #[serde(
        rename = "LEAP_SECONDS_TIME_SYSTEM_NOT_IN_VERSION",
        rename_all = "camelCase"
    )]
    LeapSecondsTimeSystemNotInVersion {
        time_system: String,
        version: f64,
        message: String,
    },
    #[serde(rename = "INVALID_LEAP_SECONDS_TIME_SYSTEM", rename_all = "camelCase")]
    InvalidLeapSecondsTimeSystem {
        time_system: String,
        message: String,
    },
    #[serde(rename = "READ_BACK_MISMATCH", rename_all = "camelCase")]
    ReadBackMismatch { what: String, message: String },
}

impl RinexObsWriteErrorDetailJs {
    fn from_core(err: CoreRinexObsWriteError) -> Self {
        let message = err.to_string();
        match err {
            CoreRinexObsWriteError::CodeListsNotVersionTwo {
                system,
                position,
                code,
            } => Self::CodeListsNotVersionTwo {
                system: system_letter(system),
                position,
                code,
                message,
            },
            CoreRinexObsWriteError::NotVersionTwo { version } => {
                Self::NotVersionTwo { version, message }
            }
            CoreRinexObsWriteError::ScaleFactorsInVersionTwo { count } => {
                Self::ScaleFactorsInVersionTwo { count, message }
            }
            CoreRinexObsWriteError::ValuesWithoutCodes {
                epoch_index,
                satellite,
                codes,
                values,
            } => Self::ValuesWithoutCodes {
                epoch_index,
                satellite: satellite.to_string(),
                codes,
                values,
                message,
            },
            CoreRinexObsWriteError::CountsWithoutCodes {
                satellite,
                codes,
                counts,
            } => Self::CountsWithoutCodes {
                satellite: satellite.to_string(),
                codes,
                counts,
                message,
            },
            CoreRinexObsWriteError::CodeListNotStated { system } => Self::CodeListNotStated {
                system: system_letter(system),
                message,
            },
            CoreRinexObsWriteError::EpochFlagTooWide { epoch_index, flag } => {
                Self::EpochFlagTooWide {
                    epoch_index,
                    flag,
                    message,
                }
            }
            CoreRinexObsWriteError::EpochTimeMissing { epoch_index, flag } => {
                Self::EpochTimeMissing {
                    epoch_index,
                    flag,
                    message,
                }
            }
            CoreRinexObsWriteError::EpochPicosecondsNotInVersion {
                epoch_index,
                version,
            } => Self::EpochPicosecondsNotInVersion {
                epoch_index,
                version,
                message,
            },
            CoreRinexObsWriteError::TooManyObservationTypes { count } => {
                Self::TooManyObservationTypes { count, message }
            }
            CoreRinexObsWriteError::CodeListsNotUnion { system } => Self::CodeListsNotUnion {
                system: system_letter(system),
                message,
            },
            CoreRinexObsWriteError::ValueOutsideDeclaredList {
                epoch_index,
                satellite,
                code,
            } => Self::ValueOutsideDeclaredList {
                epoch_index,
                satellite: satellite.to_string(),
                code,
                message,
            },
            CoreRinexObsWriteError::DeclaredListNotStated { system } => {
                Self::DeclaredListNotStated {
                    system: system_letter(system),
                    message,
                }
            }
            CoreRinexObsWriteError::EventRecordsUnreadable {
                message: reader_error,
            } => Self::EventRecordsUnreadable {
                reader_error,
                message,
            },
            CoreRinexObsWriteError::ObservableNotRepresentable {
                system,
                code,
                version,
            } => Self::ObservableNotRepresentable {
                system: system_letter(system),
                code,
                version,
                message,
            },
            CoreRinexObsWriteError::LeapSecondsTimeSystemNotInVersion {
                time_system,
                version,
            } => Self::LeapSecondsTimeSystemNotInVersion {
                time_system,
                version,
                message,
            },
            CoreRinexObsWriteError::InvalidLeapSecondsTimeSystem { time_system } => {
                Self::InvalidLeapSecondsTimeSystem {
                    time_system,
                    message,
                }
            }
            CoreRinexObsWriteError::ReadBackMismatch { what } => {
                Self::ReadBackMismatch { what, message }
            }
        }
    }
}

/// A thrown `RinexObsWriteError` carrying the typed `detail`. Shared by every
/// entry point that writes or downgrades an observation product, the repair
/// result's text and CRINEX writers included.
pub(crate) fn rinex_obs_write_error(err: CoreRinexObsWriteError) -> JsValue {
    let message = err.to_string();
    error_with_detail(
        "RinexObsWriteError",
        &message,
        &RinexObsWriteErrorDetailJs::from_core(err),
    )
}

/// One change `downgradeToRinex2` made, as a discriminated union on `kind`.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind")]
enum ObsDowngradeChangeJs {
    #[serde(rename = "CODE_RENAMED", rename_all = "camelCase")]
    CodeRenamed {
        system: String,
        from: String,
        to: String,
    },
    #[serde(rename = "CODE_MOVED", rename_all = "camelCase")]
    CodeMoved {
        system: String,
        code: String,
        from: usize,
        to: usize,
    },
    #[serde(rename = "CODE_ADDED", rename_all = "camelCase")]
    CodeAdded { system: String, code: String },
    #[serde(rename = "CODE_LIST_REMOVED", rename_all = "camelCase")]
    CodeListRemoved { system: String, codes: Vec<String> },
    #[serde(rename = "VALUE_ROUNDED", rename_all = "camelCase")]
    ValueRounded {
        epoch_index: usize,
        satellite: String,
        code: String,
        from: f64,
        to: f64,
    },
    #[serde(rename = "CYCLE_SLIP_ROUNDED", rename_all = "camelCase")]
    CycleSlipRounded {
        epoch_index: usize,
        satellite: String,
        code: String,
        from: f64,
        to: f64,
    },
    #[serde(rename = "SCALE_FACTORS_REMOVED", rename_all = "camelCase")]
    ScaleFactorsRemoved { count: usize },
    #[serde(rename = "EPOCH_PICOSECONDS_REMOVED", rename_all = "camelCase")]
    EpochPicosecondsRemoved {
        epoch_index: usize,
        picoseconds: u32,
    },
    #[serde(rename = "CLOCK_OFFSET_ROUNDED", rename_all = "camelCase")]
    ClockOffsetRounded {
        epoch_index: usize,
        from: f64,
        to: f64,
    },
    #[serde(rename = "IN_EVENT_LISTS", rename_all = "camelCase")]
    InEventLists {
        epoch_index: usize,
        change: Box<ObsDowngradeChangeJs>,
    },
    #[serde(rename = "DEPRECATED_RECORDS_REMOVED", rename_all = "camelCase")]
    DeprecatedRecordsRemoved {
        label: String,
        epoch_index: Option<usize>,
        records: Vec<String>,
    },
    #[serde(rename = "EVENT_RECORDS_REWRITTEN", rename_all = "camelCase")]
    EventRecordsRewritten {
        epoch_index: usize,
        from: Vec<String>,
        to: Vec<String>,
    },
}

impl ObsDowngradeChangeJs {
    fn from_core(change: CoreObsDowngradeChange) -> Self {
        match change {
            CoreObsDowngradeChange::CodeRenamed { system, from, to } => Self::CodeRenamed {
                system: system_letter(system),
                from,
                to,
            },
            CoreObsDowngradeChange::CodeMoved {
                system,
                code,
                from,
                to,
            } => Self::CodeMoved {
                system: system_letter(system),
                code,
                from,
                to,
            },
            CoreObsDowngradeChange::CodeAdded { system, code } => Self::CodeAdded {
                system: system_letter(system),
                code,
            },
            CoreObsDowngradeChange::CodeListRemoved { system, codes } => Self::CodeListRemoved {
                system: system_letter(system),
                codes,
            },
            CoreObsDowngradeChange::ValueRounded {
                epoch_index,
                satellite,
                code,
                from,
                to,
            } => Self::ValueRounded {
                epoch_index,
                satellite: satellite.to_string(),
                code,
                from,
                to,
            },
            CoreObsDowngradeChange::CycleSlipRounded {
                epoch_index,
                satellite,
                code,
                from,
                to,
            } => Self::CycleSlipRounded {
                epoch_index,
                satellite: satellite.to_string(),
                code,
                from,
                to,
            },
            CoreObsDowngradeChange::ScaleFactorsRemoved { count } => {
                Self::ScaleFactorsRemoved { count }
            }
            CoreObsDowngradeChange::EpochPicosecondsRemoved {
                epoch_index,
                picoseconds,
            } => Self::EpochPicosecondsRemoved {
                epoch_index,
                picoseconds,
            },
            CoreObsDowngradeChange::ClockOffsetRounded {
                epoch_index,
                from,
                to,
            } => Self::ClockOffsetRounded {
                epoch_index,
                from,
                to,
            },
            CoreObsDowngradeChange::InEventLists {
                epoch_index,
                change,
            } => Self::InEventLists {
                epoch_index,
                change: Box::new(Self::from_core(*change)),
            },
            CoreObsDowngradeChange::DeprecatedRecordsRemoved {
                label,
                epoch_index,
                records,
            } => Self::DeprecatedRecordsRemoved {
                label,
                epoch_index,
                records,
            },
            CoreObsDowngradeChange::EventRecordsRewritten {
                epoch_index,
                from,
                to,
            } => Self::EventRecordsRewritten {
                epoch_index,
                from,
                to,
            },
        }
    }
}

// --- Product -------------------------------------------------------------------------

/// A parsed RINEX 2, 3 or 4 observation file.
#[wasm_bindgen]
pub struct RinexObs {
    pub(crate) inner: CoreRinexObs,
}

impl RinexObs {
    pub(crate) fn from_core(inner: CoreRinexObs) -> Self {
        Self { inner }
    }
}

#[wasm_bindgen]
impl RinexObs {
    /// The file header. Its `obsCodes` is the union of every list the file
    /// declares; `headerAt` gives the header in effect at one epoch.
    #[wasm_bindgen(getter)]
    pub fn header(&self) -> ObsHeader {
        ObsHeader::from_core(self.inner.header().clone())
    }

    /// Epoch records in file order, events and cycle slip records included, so
    /// indices stay stable.
    #[wasm_bindgen(getter)]
    pub fn epochs(&self) -> Vec<ObsEpoch> {
        self.inner
            .epochs()
            .iter()
            .cloned()
            .map(|inner| ObsEpoch { inner })
            .collect()
    }

    /// Number of parsed epoch records.
    #[wasm_bindgen(getter, js_name = epochCount)]
    pub fn epoch_count(&self) -> usize {
        self.inner.epochs().len()
    }

    /// Records the reader skipped because a satellite token named no
    /// satellite id the engine holds (an extended GLONASS slot such as `R28`),
    /// and contradictory phase-shift or GLONASS bias records it kept and
    /// counted.
    #[wasm_bindgen(getter, js_name = skippedRecords)]
    pub fn skipped_records(&self) -> usize {
        self.inner.skipped_records
    }

    /// One epoch by zero-based index. An index that is negative, fractional or
    /// past the last epoch is a `RangeError`.
    pub fn epoch(&self, epoch_index: f64) -> Result<ObsEpoch, JsValue> {
        self.check_epoch(epoch_index).map(|(_, epoch)| ObsEpoch {
            inner: epoch.clone(),
        })
    }

    /// The header in effect at one epoch: the file header with the header
    /// records of every event at or before it laid over it. Its `obsCodes` is
    /// the product's union and its `declaredObsCodes` the lists in effect at
    /// the epoch. An index past the last epoch is a `RangeError`; a loop over
    /// many epochs uses `headerTimeline()`.
    #[wasm_bindgen(js_name = headerAt)]
    pub fn header_at(&self, epoch_index: f64) -> Result<ObsHeader, JsValue> {
        let (index, _) = self.check_epoch(epoch_index)?;
        self.inner
            .header_at(index)
            .map(ObsHeader::from_core)
            .map_err(engine_error)
    }

    /// The header in effect at every epoch, built once.
    #[wasm_bindgen(js_name = headerTimeline)]
    pub fn header_timeline(&self) -> Result<ObsHeaderTimeline, JsValue> {
        self.inner
            .header_timeline()
            .map(|inner| ObsHeaderTimeline { inner })
            .map_err(engine_error)
    }

    /// Observation codes for a constellation: the union of every list the file
    /// declares, in the order first declared.
    #[wasm_bindgen(js_name = obsCodes)]
    pub fn obs_codes(&self, system: GnssSystem) -> Vec<String> {
        self.inner
            .obs_codes(system.into())
            .map(|c| c.to_vec())
            .unwrap_or_default()
    }

    /// Flatten raw observation values for one epoch.
    #[wasm_bindgen(js_name = observationValues)]
    pub fn observation_values(
        &self,
        epoch_index: f64,
        filter: Option<ObservationFilter>,
    ) -> Result<ObservationValueSeries, JsValue> {
        let (_, epoch) = self.check_epoch(epoch_index)?;
        let filter = filter.unwrap_or_default().inner;
        let rows = core_observation_values(&self.inner, epoch, &filter).map_err(engine_error)?;
        Ok(observation_value_series(rows))
    }

    /// Flatten carrier-phase values for one epoch, read with the header in
    /// effect at that epoch, so a phase shift or GLONASS channel an earlier
    /// event declared applies. Every row is kept with its phase-shift status.
    #[wasm_bindgen(js_name = carrierPhaseRows)]
    pub fn carrier_phase_rows(
        &self,
        epoch_index: f64,
        filter: Option<ObservationFilter>,
    ) -> Result<CarrierPhaseSeries, JsValue> {
        let (index, epoch) = self.check_epoch(epoch_index)?;
        let filter = filter.unwrap_or_default().inner;
        let header = self.inner.header_at(index).map_err(engine_error)?;
        let rows = core_carrier_phase_rows(&header, epoch, &filter).map_err(engine_error)?;
        Ok(carrier_phase_series(rows))
    }

    /// Extract single-frequency pseudoranges for one epoch.
    pub fn pseudoranges(
        &self,
        epoch_index: f64,
        policy: Option<SignalPolicy>,
    ) -> Result<PseudorangeSeries, JsValue> {
        let (_, epoch) = self.check_epoch(epoch_index)?;
        let policy = match policy {
            Some(policy) => policy.inner,
            None => {
                CoreSignalPolicy::default_for(self.inner.header().version).map_err(engine_error)?
            }
        };
        let rows = core_pseudoranges(&self.inner, epoch, &policy).map_err(engine_error)?;
        let mut satellites = Vec::with_capacity(rows.len());
        let mut ranges_m = Vec::with_capacity(rows.len());
        for (sat, range_m) in rows {
            satellites.push(sat.to_string());
            ranges_m.push(range_m);
        }
        Ok(PseudorangeSeries {
            satellites,
            ranges_m,
        })
    }

    /// Serialize to RINEX observation text: version 2 records below 3.0,
    /// version 3 records otherwise. Deterministic.
    ///
    /// The text is returned only when reading it back gives this product.
    /// Otherwise this throws a `RinexObsWriteError` whose `detail` is a
    /// `RinexObsWriteErrorDetail` naming what the text could not carry; nothing
    /// is dropped, rounded, wrapped or truncated to make the product fit. A
    /// product that has to lose something to become a version 2 file goes
    /// through `downgradeToRinex2`, which reports every change.
    #[wasm_bindgen(js_name = toRinexString)]
    pub fn to_rinex_string(&self) -> Result<String, JsValue> {
        self.inner.to_rinex_string().map_err(rinex_obs_write_error)
    }

    /// This product as one a version 2 file can state exactly, returned as
    /// `{ obs, value, changes }`: the new product (`value` is the same
    /// instance) and every change the downgrade made, in order. This product
    /// is not modified.
    ///
    /// `version` must be a version 2 (from 2.0 up to, not including, 3.0).
    /// What version 2 cannot state, such as a code on a carrier version 2 has
    /// no name for or a `LEAP SECONDS` time system it does not support, is
    /// refused with a `RinexObsWriteError` rather than changed.
    #[wasm_bindgen(js_name = downgradeToRinex2, unchecked_return_type = "RinexObsDowngrade")]
    pub fn downgrade_to_rinex2(&self, version: f64) -> Result<JsValue, JsValue> {
        let (product, changes) = self
            .inner
            .downgrade_to_rinex2(version)
            .map_err(rinex_obs_write_error)?;
        let obs: JsValue = RinexObs::from_core(product).into();
        let changes: Vec<ObsDowngradeChangeJs> = changes
            .into_iter()
            .map(ObsDowngradeChangeJs::from_core)
            .collect();
        let changes = to_plain_js(&changes, "RINEX OBS downgrade changes")?;
        result_object(
            &[("obs", &obs), ("value", &obs), ("changes", &changes)],
            "RINEX OBS downgrade result",
        )
    }
}

impl RinexObs {
    /// A caller's epoch index, read exactly and checked against the product.
    fn check_epoch(&self, epoch_index: f64) -> Result<(usize, &CoreObsEpoch), JsValue> {
        let index = index_arg(epoch_index, "epochIndex")?;
        let epochs = self.inner.epochs();
        epochs
            .get(index)
            .map(|epoch| (index, epoch))
            .ok_or_else(|| {
                range_error(&format!(
                    "epoch index {index} out of range for {} epochs",
                    epochs.len()
                ))
            })
    }
}

fn observation_value_series(
    rows: Vec<(GnssSatelliteId, Vec<CoreObservationValueRow>)>,
) -> ObservationValueSeries {
    let count = rows.iter().map(|(_, r)| r.len()).sum();
    let mut out = ObservationValueSeries {
        satellites: Vec::with_capacity(count),
        codes: Vec::with_capacity(count),
        kinds: Vec::with_capacity(count),
        values: Vec::with_capacity(count),
        lli: Vec::with_capacity(count),
        ssi: Vec::with_capacity(count),
    };
    for (sat, sat_rows) in rows {
        let token = sat.to_string();
        for row in sat_rows {
            out.satellites.push(token.clone());
            out.codes.push(row.code);
            out.kinds.push(row.kind.into());
            out.values.push(nan_if_missing(row.value));
            out.lli.push(u8_nan_if_missing(row.lli));
            out.ssi.push(u8_nan_if_missing(row.ssi));
        }
    }
    out
}

fn carrier_phase_series(
    rows: Vec<(GnssSatelliteId, Vec<CoreCarrierPhaseRow>)>,
) -> CarrierPhaseSeries {
    let count = rows.iter().map(|(_, r)| r.len()).sum();
    let mut out = CarrierPhaseSeries {
        satellites: Vec::with_capacity(count),
        codes: Vec::with_capacity(count),
        value_cycles: Vec::with_capacity(count),
        frequency_hz: Vec::with_capacity(count),
        wavelength_m: Vec::with_capacity(count),
        value_m: Vec::with_capacity(count),
        phase_shift_cycles: Vec::with_capacity(count),
        phase_shift_available: Vec::with_capacity(count),
        phase_shift_status: Vec::with_capacity(count),
        phase_shift_corrections: Vec::with_capacity(count),
        lli: Vec::with_capacity(count),
        ssi: Vec::with_capacity(count),
    };
    for (sat, sat_rows) in rows {
        let token = sat.to_string();
        for row in sat_rows {
            let shift = CarrierPhaseShiftJs::from_core(&row.phase_shift_cycles);
            out.satellites.push(token.clone());
            out.codes.push(row.code);
            out.value_cycles.push(nan_if_missing(row.value_cycles));
            out.frequency_hz.push(nan_if_missing(row.frequency_hz));
            out.wavelength_m.push(nan_if_missing(row.wavelength_m));
            out.value_m.push(nan_if_missing(row.value_m));
            out.phase_shift_cycles.push(nan_if_missing(
                row.phase_shift_cycles.as_ref().ok().copied(),
            ));
            out.phase_shift_available
                .push(u8::from(row.phase_shift_cycles.is_ok()));
            out.phase_shift_status.push(shift.status().to_string());
            out.phase_shift_corrections.push(shift);
            out.lli.push(u8_nan_if_missing(row.lli));
            out.ssi.push(u8_nan_if_missing(row.ssi));
        }
    }
    out
}

/// Parse a RINEX OBS byte buffer (UTF-8 text) into an observation product.
/// Throws a `TypeError` on non-UTF-8 input and an `Error` on a parse failure.
#[wasm_bindgen(js_name = parseRinexObs)]
pub fn parse_rinex_obs(bytes: &[u8]) -> Result<RinexObs, JsValue> {
    let text = utf8_text(bytes, "RINEX OBS source")?;
    Ok(RinexObs {
        inner: CoreRinexObs::parse(&text)
            .map_err(|error| crate::positioning_error::core_source_error(&error))?,
    })
}

/// Alias of [`parseRinexObs`] for callers that read a file as bytes.
#[wasm_bindgen(js_name = loadRinexObs)]
pub fn load_rinex_obs(bytes: &[u8]) -> Result<RinexObs, JsValue> {
    parse_rinex_obs(bytes)
}

// --- TypeScript declarations -----------------------------------------------------------

// The plain-object shapes the getters and methods above return, and the names
// their `unchecked_return_type` attributes resolve against. `wasm-pack` writes
// them into both `sidereon.d.ts` targets; `types/sidereon-extra.d.ts`
// re-exports them.
#[wasm_bindgen(typescript_custom_section)]
const TS_RINEX_OBS_DEFINITIONS: &str = r#"
/** One observation or cycle slip field; `null` where the field is blank. */
export interface ObsValue {
  value: number | null;
  lli: number | null;
  ssi: number | null;
}

/** One satellite's fields, index-aligned to the header's `obsCodes`. */
export interface ObsSatelliteValues {
  satellite: string;
  values: ObsValue[];
}

export interface ObsProgramRunByDate {
  program: string;
  runBy: string;
  date: string;
}

export interface ObsReceiver {
  number: string;
  receiverType: string;
  version: string;
}

export interface ObsAntenna {
  number: string;
  antennaType: string;
}

/** A `SYS / SCALE FACTOR` record; `system` is the RINEX letter. */
export interface ObsScaleFactor {
  system: string;
  factor: number;
  codes: string[];
}

/** `PRN / # OF OBS` counts, `null` for a blank count. */
export interface ObsPrnObsCount {
  satellite: string;
  counts: (number | null)[];
}

/** One `GLONASS COD/PHS/BIS` entry as written; `biasM` is `null` when blank. */
export interface ObsGlonassBias {
  code: string;
  biasM: number | null;
}

export type GlonassCodePhaseBias =
  | { status: "available"; biasM: number }
  | { status: "none" }
  | { status: "unknown" }
  | { status: "ambiguous"; biasesM: (number | null)[] };

export type CorrectionStatus = "available" | "unknown" | "ambiguous";

export type CarrierPhaseShift =
  | { status: "available"; cycles: number }
  | { status: "unknown" }
  | { status: "ambiguous"; corrections: (number | null)[] };

export type RinexObsWriteErrorDetail =
  | {
      kind: "CODE_LISTS_NOT_VERSION_TWO";
      system: string;
      position: number;
      code: string | null;
      message: string;
    }
  | { kind: "NOT_VERSION_TWO"; version: number; message: string }
  | { kind: "SCALE_FACTORS_IN_VERSION_TWO"; count: number; message: string }
  | {
      kind: "VALUES_WITHOUT_CODES";
      epochIndex: number;
      satellite: string;
      codes: number;
      values: number;
      message: string;
    }
  | {
      kind: "COUNTS_WITHOUT_CODES";
      satellite: string;
      codes: number;
      counts: number;
      message: string;
    }
  | { kind: "CODE_LIST_NOT_STATED"; system: string; message: string }
  | { kind: "EPOCH_FLAG_TOO_WIDE"; epochIndex: number; flag: number; message: string }
  | { kind: "EPOCH_TIME_MISSING"; epochIndex: number; flag: number; message: string }
  | {
      kind: "EPOCH_PICOSECONDS_NOT_IN_VERSION";
      epochIndex: number;
      version: number;
      message: string;
    }
  | { kind: "TOO_MANY_OBSERVATION_TYPES"; count: number; message: string }
  | { kind: "CODE_LISTS_NOT_UNION"; system: string; message: string }
  | {
      kind: "VALUE_OUTSIDE_DECLARED_LIST";
      epochIndex: number;
      satellite: string;
      code: string | null;
      message: string;
    }
  | { kind: "DECLARED_LIST_NOT_STATED"; system: string; message: string }
  | { kind: "EVENT_RECORDS_UNREADABLE"; readerError: string; message: string }
  | {
      kind: "OBSERVABLE_NOT_REPRESENTABLE";
      system: string;
      code: string;
      version: number;
      message: string;
    }
  | {
      kind: "LEAP_SECONDS_TIME_SYSTEM_NOT_IN_VERSION";
      timeSystem: string;
      version: number;
      message: string;
    }
  | { kind: "INVALID_LEAP_SECONDS_TIME_SYSTEM"; timeSystem: string; message: string }
  | { kind: "READ_BACK_MISMATCH"; what: string; message: string };

export type ObsDowngradeChange =
  | { kind: "CODE_RENAMED"; system: string; from: string; to: string }
  | { kind: "CODE_MOVED"; system: string; code: string; from: number; to: number }
  | { kind: "CODE_ADDED"; system: string; code: string }
  | { kind: "CODE_LIST_REMOVED"; system: string; codes: string[] }
  | {
      kind: "VALUE_ROUNDED";
      epochIndex: number;
      satellite: string;
      code: string;
      from: number;
      to: number;
    }
  | {
      kind: "CYCLE_SLIP_ROUNDED";
      epochIndex: number;
      satellite: string;
      code: string;
      from: number;
      to: number;
    }
  | { kind: "SCALE_FACTORS_REMOVED"; count: number }
  | { kind: "EPOCH_PICOSECONDS_REMOVED"; epochIndex: number; picoseconds: number }
  | { kind: "CLOCK_OFFSET_ROUNDED"; epochIndex: number; from: number; to: number }
  | { kind: "IN_EVENT_LISTS"; epochIndex: number; change: ObsDowngradeChange }
  | {
      kind: "DEPRECATED_RECORDS_REMOVED";
      label: string;
      epochIndex: number | null;
      records: string[];
    }
  | { kind: "EVENT_RECORDS_REWRITTEN"; epochIndex: number; from: string[]; to: string[] };

export interface RinexObsDowngrade {
  obs: RinexObs;
  value: RinexObs;
  changes: ObsDowngradeChange[];
}
"#;

#[cfg(test)]
mod writer_contract_tests {
    use super::*;
    use std::collections::BTreeSet;

    fn satellite(system: CoreGnssSystem, prn: u8) -> GnssSatelliteId {
        GnssSatelliteId { system, prn }
    }

    #[test]
    fn every_writer_error_variant_maps_to_literal_detail_and_display() {
        let cases: Vec<(CoreRinexObsWriteError, serde_json::Value, &str)> = vec![
            (
                CoreRinexObsWriteError::CodeListsNotVersionTwo {
                    system: CoreGnssSystem::Gps,
                    position: 2,
                    code: Some("C1C".into()),
                },
                serde_json::json!({"kind":"CODE_LISTS_NOT_VERSION_TWO","system":"G","position":2,"code":"C1C","message":"RINEX OBS version 2 has no observation type that every constellation reads back as its own code at position 2, where GPS holds \"C1C\""}),
                "RINEX OBS version 2 has no observation type that every constellation reads back as its own code at position 2, where GPS holds \"C1C\"",
            ),
            (
                CoreRinexObsWriteError::CodeListsNotVersionTwo {
                    system: CoreGnssSystem::Galileo,
                    position: 1,
                    code: None,
                },
                serde_json::json!({"kind":"CODE_LISTS_NOT_VERSION_TWO","system":"E","position":1,"code":null,"message":"RINEX OBS version 2 names one list of codes for every constellation, and Galileo holds 1 codes where another constellation holds a different number"}),
                "RINEX OBS version 2 names one list of codes for every constellation, and Galileo holds 1 codes where another constellation holds a different number",
            ),
            (
                CoreRinexObsWriteError::NotVersionTwo { version: 3.5 },
                serde_json::json!({"kind":"NOT_VERSION_TWO","version":3.5,"message":"RINEX OBS version 3.5 is not a version 2"}),
                "RINEX OBS version 3.5 is not a version 2",
            ),
            (
                CoreRinexObsWriteError::ScaleFactorsInVersionTwo { count: 2 },
                serde_json::json!({"kind":"SCALE_FACTORS_IN_VERSION_TWO","count":2,"message":"RINEX OBS version 2 would carry 2 SYS / SCALE FACTOR records, which version 2 readers that do not apply them read as physical values; downgrade_to_rinex2 removes them"}),
                "RINEX OBS version 2 would carry 2 SYS / SCALE FACTOR records, which version 2 readers that do not apply them read as physical values; downgrade_to_rinex2 removes them",
            ),
            (
                CoreRinexObsWriteError::ValuesWithoutCodes {
                    epoch_index: 3,
                    satellite: satellite(CoreGnssSystem::Galileo, 7),
                    codes: 2,
                    values: 4,
                },
                serde_json::json!({"kind":"VALUES_WITHOUT_CODES","epochIndex":3,"satellite":"E07","codes":2,"values":4,"message":"RINEX OBS epoch 3 satellite E07 holds 4 values for 2 observation codes"}),
                "RINEX OBS epoch 3 satellite E07 holds 4 values for 2 observation codes",
            ),
            (
                CoreRinexObsWriteError::CountsWithoutCodes {
                    satellite: satellite(CoreGnssSystem::Gps, 12),
                    codes: 2,
                    counts: 3,
                },
                serde_json::json!({"kind":"COUNTS_WITHOUT_CODES","satellite":"G12","codes":2,"counts":3,"message":"RINEX OBS PRN / # OF OBS for G12 holds 3 counts for 2 observation codes"}),
                "RINEX OBS PRN / # OF OBS for G12 holds 3 counts for 2 observation codes",
            ),
            (
                CoreRinexObsWriteError::CodeListNotStated { system: CoreGnssSystem::BeiDou },
                serde_json::json!({"kind":"CODE_LIST_NOT_STATED","system":"C","message":"RINEX OBS version 2 would not state BeiDou's code list: no observation or PRN / # OF OBS count names BeiDou, so a reader builds no list for it, and the type names do not read as it; downgrade_to_rinex2 removes the list"}),
                "RINEX OBS version 2 would not state BeiDou's code list: no observation or PRN / # OF OBS count names BeiDou, so a reader builds no list for it, and the type names do not read as it; downgrade_to_rinex2 removes the list",
            ),
            (
                CoreRinexObsWriteError::EpochFlagTooWide { epoch_index: 4, flag: 12 },
                serde_json::json!({"kind":"EPOCH_FLAG_TOO_WIDE","epochIndex":4,"flag":12,"message":"RINEX OBS epoch 4 flag 12 does not fit the one-digit flag field"}),
                "RINEX OBS epoch 4 flag 12 does not fit the one-digit flag field",
            ),
            (
                CoreRinexObsWriteError::EpochTimeMissing { epoch_index: 5, flag: 0 },
                serde_json::json!({"kind":"EPOCH_TIME_MISSING","epochIndex":5,"flag":0,"message":"RINEX OBS epoch 5 with flag 0 has no epoch time, which only an event may leave blank"}),
                "RINEX OBS epoch 5 with flag 0 has no epoch time, which only an event may leave blank",
            ),
            (
                CoreRinexObsWriteError::EpochPicosecondsNotInVersion { epoch_index: 6, version: 4.01 },
                serde_json::json!({"kind":"EPOCH_PICOSECONDS_NOT_IN_VERSION","epochIndex":6,"version":4.01,"message":"RINEX OBS epoch 6 carries picoseconds, which a version 4.01 epoch record has no field for"}),
                "RINEX OBS epoch 6 carries picoseconds, which a version 4.01 epoch record has no field for",
            ),
            (
                CoreRinexObsWriteError::TooManyObservationTypes { count: 1000 },
                serde_json::json!({"kind":"TOO_MANY_OBSERVATION_TYPES","count":1000,"message":"RINEX OBS version 2 would need at least 1000 observation types, more than the 999 its count field declares"}),
                "RINEX OBS version 2 would need at least 1000 observation types, more than the 999 its count field declares",
            ),
            (
                CoreRinexObsWriteError::CodeListsNotUnion { system: CoreGnssSystem::Sbas },
                serde_json::json!({"kind":"CODE_LISTS_NOT_UNION","system":"S","message":"RINEX OBS SBAS code list is not the union of the lists the header and its events declare"}),
                "RINEX OBS SBAS code list is not the union of the lists the header and its events declare",
            ),
            (
                CoreRinexObsWriteError::ValueOutsideDeclaredList {
                    epoch_index: 7,
                    satellite: satellite(CoreGnssSystem::Gps, 4),
                    code: Some("L1C".into()),
                },
                serde_json::json!({"kind":"VALUE_OUTSIDE_DECLARED_LIST","epochIndex":7,"satellite":"G04","code":"L1C","message":"RINEX OBS epoch 7 satellite G04 holds a value under \"L1C\", which the list in effect at that epoch does not declare"}),
                "RINEX OBS epoch 7 satellite G04 holds a value under \"L1C\", which the list in effect at that epoch does not declare",
            ),
            (
                CoreRinexObsWriteError::ValueOutsideDeclaredList {
                    epoch_index: 8,
                    satellite: satellite(CoreGnssSystem::Glonass, 2),
                    code: None,
                },
                serde_json::json!({"kind":"VALUE_OUTSIDE_DECLARED_LIST","epochIndex":8,"satellite":"R02","code":null,"message":"RINEX OBS epoch 8 satellite R02 is of a constellation with no code list in effect at that epoch"}),
                "RINEX OBS epoch 8 satellite R02 is of a constellation with no code list in effect at that epoch",
            ),
            (
                CoreRinexObsWriteError::DeclaredListNotStated { system: CoreGnssSystem::Qzss },
                serde_json::json!({"kind":"DECLARED_LIST_NOT_STATED","system":"J","message":"RINEX OBS QZSS declared code list is not what the version 2 type names state for it"}),
                "RINEX OBS QZSS declared code list is not what the version 2 type names state for it",
            ),
            (
                CoreRinexObsWriteError::EventRecordsUnreadable { message: "bad event record".into() },
                serde_json::json!({"kind":"EVENT_RECORDS_UNREADABLE","readerError":"bad event record","message":"RINEX OBS event header records do not read: bad event record"}),
                "RINEX OBS event header records do not read: bad event record",
            ),
            (
                CoreRinexObsWriteError::ObservableNotRepresentable {
                    system: CoreGnssSystem::Gps,
                    code: "L1C".into(),
                    version: 2.11,
                },
                serde_json::json!({"kind":"OBSERVABLE_NOT_REPRESENTABLE","system":"G","code":"L1C","version":2.11,"message":"RINEX OBS GPS code L1C is on a carrier that version 2.11 cannot represent"}),
                "RINEX OBS GPS code L1C is on a carrier that version 2.11 cannot represent",
            ),
            (
                CoreRinexObsWriteError::LeapSecondsTimeSystemNotInVersion {
                    time_system: "BDT".into(),
                    version: 2.11,
                },
                serde_json::json!({"kind":"LEAP_SECONDS_TIME_SYSTEM_NOT_IN_VERSION","timeSystem":"BDT","version":2.11,"message":"RINEX OBS LEAP SECONDS time system BDT is not supported in version 2.11"}),
                "RINEX OBS LEAP SECONDS time system BDT is not supported in version 2.11",
            ),
            (
                CoreRinexObsWriteError::InvalidLeapSecondsTimeSystem { time_system: "X?".into() },
                serde_json::json!({"kind":"INVALID_LEAP_SECONDS_TIME_SYSTEM","timeSystem":"X?","message":"RINEX OBS LEAP SECONDS invalid time system identifier: \"X?\""}),
                "RINEX OBS LEAP SECONDS invalid time system identifier: \"X?\"",
            ),
            (
                CoreRinexObsWriteError::ReadBackMismatch { what: "header.version".into() },
                serde_json::json!({"kind":"READ_BACK_MISMATCH","what":"header.version","message":"RINEX OBS text would not read back as the product: header.version"}),
                "RINEX OBS text would not read back as the product: header.version",
            ),
        ];
        assert_eq!(
            cases.len(),
            20,
            "18 variants plus both optional-value cases"
        );
        let kinds: BTreeSet<_> = cases
            .iter()
            .map(|(_, expected, _)| expected["kind"].as_str().unwrap())
            .collect();
        assert_eq!(kinds.len(), 18, "every error variant is covered");

        for (source, expected, display) in cases {
            assert_eq!(source.to_string(), display, "literal core Display message");
            let mapped = RinexObsWriteErrorDetailJs::from_core(source.clone());
            assert_eq!(serde_json::to_value(&mapped).unwrap(), expected);
            assert_eq!(mapped, mapped.clone());
            assert_eq!(mapped, RinexObsWriteErrorDetailJs::from_core(source));
        }
    }

    #[test]
    fn every_downgrade_change_variant_maps_every_field_and_nested_value() {
        let cases: Vec<(CoreObsDowngradeChange, serde_json::Value)> = vec![
            (
                CoreObsDowngradeChange::CodeRenamed {
                    system: CoreGnssSystem::Gps,
                    from: "C1X".into(),
                    to: "C1C".into(),
                },
                serde_json::json!({"kind":"CODE_RENAMED","system":"G","from":"C1X","to":"C1C"}),
            ),
            (
                CoreObsDowngradeChange::CodeMoved {
                    system: CoreGnssSystem::Galileo,
                    code: "L1C".into(),
                    from: 3,
                    to: 1,
                },
                serde_json::json!({"kind":"CODE_MOVED","system":"E","code":"L1C","from":3,"to":1}),
            ),
            (
                CoreObsDowngradeChange::CodeAdded {
                    system: CoreGnssSystem::BeiDou,
                    code: "C2I".into(),
                },
                serde_json::json!({"kind":"CODE_ADDED","system":"C","code":"C2I"}),
            ),
            (
                CoreObsDowngradeChange::CodeListRemoved {
                    system: CoreGnssSystem::Sbas,
                    codes: vec!["C1C".into(), "L1C".into()],
                },
                serde_json::json!({"kind":"CODE_LIST_REMOVED","system":"S","codes":["C1C","L1C"]}),
            ),
            (
                CoreObsDowngradeChange::ValueRounded {
                    epoch_index: 2,
                    satellite: satellite(CoreGnssSystem::Gps, 8),
                    code: "C1C".into(),
                    from: 1.2345,
                    to: 1.235,
                },
                serde_json::json!({"kind":"VALUE_ROUNDED","epochIndex":2,"satellite":"G08","code":"C1C","from":1.2345,"to":1.235}),
            ),
            (
                CoreObsDowngradeChange::CycleSlipRounded {
                    epoch_index: 3,
                    satellite: satellite(CoreGnssSystem::Glonass, 9),
                    code: "L2C".into(),
                    from: -2.3456,
                    to: -2.346,
                },
                serde_json::json!({"kind":"CYCLE_SLIP_ROUNDED","epochIndex":3,"satellite":"R09","code":"L2C","from":-2.3456,"to":-2.346}),
            ),
            (
                CoreObsDowngradeChange::ScaleFactorsRemoved { count: 4 },
                serde_json::json!({"kind":"SCALE_FACTORS_REMOVED","count":4}),
            ),
            (
                CoreObsDowngradeChange::EpochPicosecondsRemoved {
                    epoch_index: 5,
                    picoseconds: 98765,
                },
                serde_json::json!({"kind":"EPOCH_PICOSECONDS_REMOVED","epochIndex":5,"picoseconds":98765}),
            ),
            (
                CoreObsDowngradeChange::ClockOffsetRounded {
                    epoch_index: 6,
                    from: -0.1234567896,
                    to: -0.12345679,
                },
                serde_json::json!({"kind":"CLOCK_OFFSET_ROUNDED","epochIndex":6,"from":-0.1234567896,"to":-0.12345679}),
            ),
            (
                CoreObsDowngradeChange::InEventLists {
                    epoch_index: 7,
                    change: Box::new(CoreObsDowngradeChange::CodeMoved {
                        system: CoreGnssSystem::BeiDou,
                        code: "C1I".into(),
                        from: 3,
                        to: 1,
                    }),
                },
                serde_json::json!({"kind":"IN_EVENT_LISTS","epochIndex":7,"change":{"kind":"CODE_MOVED","system":"C","code":"C1I","from":3,"to":1}}),
            ),
            (
                CoreObsDowngradeChange::DeprecatedRecordsRemoved {
                    label: "GLONASS COD/PHS/BIS".into(),
                    epoch_index: None,
                    records: vec!["C1C -10.000".into()],
                },
                serde_json::json!({"kind":"DEPRECATED_RECORDS_REMOVED","label":"GLONASS COD/PHS/BIS","epochIndex":null,"records":["C1C -10.000"]}),
            ),
            (
                CoreObsDowngradeChange::EventRecordsRewritten {
                    epoch_index: 9,
                    from: vec!["G    1 C1C".into()],
                    to: vec!["     1    C1".into()],
                },
                serde_json::json!({"kind":"EVENT_RECORDS_REWRITTEN","epochIndex":9,"from":["G    1 C1C"],"to":["     1    C1"]}),
            ),
            (
                CoreObsDowngradeChange::DeprecatedRecordsRemoved {
                    label: "SYS / PHASE SHIFT".into(),
                    epoch_index: Some(4),
                    records: vec!["G L1C 0.250".into(), "G L2C -0.125".into()],
                },
                serde_json::json!({"kind":"DEPRECATED_RECORDS_REMOVED","label":"SYS / PHASE SHIFT","epochIndex":4,"records":["G L1C 0.250","G L2C -0.125"]}),
            ),
        ];
        assert_eq!(
            cases.len(),
            13,
            "12 variants plus both optional epoch-index values"
        );
        let kinds: BTreeSet<_> = cases
            .iter()
            .map(|(_, expected)| expected["kind"].as_str().unwrap())
            .collect();
        assert_eq!(kinds.len(), 12, "every downgrade-change variant is covered");

        for (source, expected) in cases {
            let mapped = ObsDowngradeChangeJs::from_core(source.clone());
            assert_eq!(serde_json::to_value(&mapped).unwrap(), expected);
            assert_eq!(mapped, mapped.clone());
            assert_eq!(mapped, ObsDowngradeChangeJs::from_core(source));
        }
    }
}
