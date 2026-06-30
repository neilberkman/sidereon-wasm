//! RINEX 3/4 observation parsing. Mirrors the core `rinex::observations`
//! surface: a parsed `RinexObs` with a typed header and per-epoch records, and
//! columnar numeric series (pseudoranges, raw values, carrier phase) that cross
//! to JS as `Float64Array`s row-aligned to satellite/code string arrays.

use wasm_bindgen::prelude::*;

use sidereon_core::rinex::observations::{
    carrier_phase_rows as core_carrier_phase_rows, observation_values as core_observation_values,
    pseudoranges as core_pseudoranges, CarrierPhaseRow as CoreCarrierPhaseRow,
    ObsEpoch as CoreObsEpoch, ObsEpochTime as CoreObsEpochTime, ObsHeader as CoreObsHeader,
    ObsPhaseShift as CoreObsPhaseShift, ObservationFilter as CoreObservationFilter,
    ObservationKind as CoreObservationKind, ObservationValueRow as CoreObservationValueRow,
    RinexObs as CoreRinexObs, SignalPolicy as CoreSignalPolicy,
};
use sidereon_core::GnssSatelliteId;

use crate::error::{engine_error, range_error, utf8_text};
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

fn nan_if_missing(value: Option<f64>) -> f64 {
    value.unwrap_or(f64::NAN)
}

fn u8_nan_if_missing(value: Option<u8>) -> f64 {
    value.map(f64::from).unwrap_or(f64::NAN)
}

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

/// One `SYS / PHASE SHIFT` record from a RINEX OBS header.
#[wasm_bindgen]
pub struct ObsPhaseShift {
    inner: CoreObsPhaseShift,
}

#[wasm_bindgen]
impl ObsPhaseShift {
    /// Constellation this correction applies to.
    #[wasm_bindgen(getter)]
    pub fn system(&self) -> GnssSystem {
        self.inner.system.into()
    }

    /// RINEX carrier observation code, such as `L1C`.
    #[wasm_bindgen(getter)]
    pub fn code(&self) -> String {
        self.inner.code.clone()
    }

    /// Phase correction in carrier cycles.
    #[wasm_bindgen(getter, js_name = correctionCycles)]
    pub fn correction_cycles(&self) -> f64 {
        self.inner.correction_cycles
    }

    /// Satellite tokens this correction is restricted to. Empty means all.
    #[wasm_bindgen(getter)]
    pub fn satellites(&self) -> Vec<String> {
        self.inner
            .satellites
            .iter()
            .map(|s| s.to_string())
            .collect()
    }
}

/// Parsed RINEX OBS header.
#[wasm_bindgen]
pub struct ObsHeader {
    inner: CoreObsHeader,
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

    /// Nominal epoch interval, seconds, or `undefined`.
    #[wasm_bindgen(getter, js_name = intervalS)]
    pub fn interval_s(&self) -> Option<f64> {
        self.inner.interval_s
    }

    /// Marker or station name, or `undefined`.
    #[wasm_bindgen(getter, js_name = markerName)]
    pub fn marker_name(&self) -> Option<String> {
        self.inner.marker_name.clone()
    }

    /// Constellations with declared observation-code lists.
    #[wasm_bindgen(getter)]
    pub fn systems(&self) -> Vec<GnssSystem> {
        self.inner
            .obs_codes
            .keys()
            .copied()
            .map(Into::into)
            .collect()
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

    /// GLONASS slot/frequency-channel pairs, flat `[slot0, chan0, slot1, ...]`.
    #[wasm_bindgen(getter, js_name = glonassSlots)]
    pub fn glonass_slots(&self) -> Vec<i32> {
        self.inner
            .glonass_slots
            .iter()
            .flat_map(|(&slot, &channel)| [i32::from(slot), i32::from(channel)])
            .collect()
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

    /// Observation codes for a constellation, in RINEX header order.
    #[wasm_bindgen(js_name = obsCodes)]
    pub fn obs_codes(&self, system: GnssSystem) -> Vec<String> {
        self.inner
            .obs_codes
            .get(&system.into())
            .cloned()
            .unwrap_or_default()
    }
}

/// One RINEX OBS epoch. Observation values are read through `RinexObs` methods.
#[wasm_bindgen]
pub struct ObsEpoch {
    inner: CoreObsEpoch,
}

#[wasm_bindgen]
impl ObsEpoch {
    /// Civil epoch in the file time scale.
    #[wasm_bindgen(getter)]
    pub fn epoch(&self) -> ObsEpochTime {
        ObsEpochTime {
            inner: self.inner.epoch,
        }
    }

    /// RINEX epoch flag. `0` is a normal observation epoch.
    #[wasm_bindgen(getter)]
    pub fn flag(&self) -> u8 {
        self.inner.flag
    }

    /// Satellite tokens present at this epoch.
    #[wasm_bindgen(getter)]
    pub fn satellites(&self) -> Vec<String> {
        self.inner.sats.keys().map(|s| s.to_string()).collect()
    }

    /// Number of satellites present at this epoch.
    #[wasm_bindgen(getter, js_name = satelliteCount)]
    pub fn satellite_count(&self) -> usize {
        self.inner.sats.len()
    }
}

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
/// and SSI are `NaN`.
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

/// Flattened carrier-phase rows from one RINEX OBS epoch. Numeric arrays are
/// row-aligned with `satellites` and `codes`; missing values and unknown
/// carrier metadata are `NaN`.
#[wasm_bindgen]
pub struct CarrierPhaseSeries {
    satellites: Vec<String>,
    codes: Vec<String>,
    value_cycles: Vec<f64>,
    frequency_hz: Vec<f64>,
    wavelength_m: Vec<f64>,
    value_m: Vec<f64>,
    phase_shift_cycles: Vec<f64>,
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

    /// Applied phase shift, cycles.
    #[wasm_bindgen(getter, js_name = phaseShiftCycles)]
    pub fn phase_shift_cycles(&self) -> Vec<f64> {
        self.phase_shift_cycles.clone()
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

/// A parsed RINEX 3/4 observation file.
#[wasm_bindgen]
pub struct RinexObs {
    inner: CoreRinexObs,
}

#[wasm_bindgen]
impl RinexObs {
    /// Parsed RINEX OBS header.
    #[wasm_bindgen(getter)]
    pub fn header(&self) -> ObsHeader {
        ObsHeader {
            inner: self.inner.header().clone(),
        }
    }

    /// Epoch records in file order.
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

    /// One epoch by zero-based index. Throws a `RangeError` if out of range.
    pub fn epoch(&self, epoch_index: usize) -> Result<ObsEpoch, JsValue> {
        self.check_epoch(epoch_index).map(|epoch| ObsEpoch {
            inner: epoch.clone(),
        })
    }

    /// Observation codes for a constellation, in header order.
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
        epoch_index: usize,
        filter: Option<ObservationFilter>,
    ) -> Result<ObservationValueSeries, JsValue> {
        let epoch = self.check_epoch(epoch_index)?;
        let filter = filter.unwrap_or_default().inner;
        let rows = core_observation_values(&self.inner, epoch, &filter).map_err(engine_error)?;
        Ok(observation_value_series(rows))
    }

    /// Flatten carrier-phase values for one epoch.
    #[wasm_bindgen(js_name = carrierPhaseRows)]
    pub fn carrier_phase_rows(
        &self,
        epoch_index: usize,
        filter: Option<ObservationFilter>,
    ) -> Result<CarrierPhaseSeries, JsValue> {
        let epoch = self.check_epoch(epoch_index)?;
        let filter = filter.unwrap_or_default().inner;
        let rows = core_carrier_phase_rows(&self.inner, epoch, &filter).map_err(engine_error)?;
        Ok(carrier_phase_series(rows))
    }

    /// Extract single-frequency pseudoranges for one epoch.
    pub fn pseudoranges(
        &self,
        epoch_index: usize,
        policy: Option<SignalPolicy>,
    ) -> Result<PseudorangeSeries, JsValue> {
        let epoch = self.check_epoch(epoch_index)?;
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

    /// Serialize to standard RINEX 3 observation text. Deterministic: the same
    /// product always produces byte-identical text, and re-parsing the output
    /// reproduces the same header and epochs.
    #[wasm_bindgen(js_name = toRinexString)]
    pub fn to_rinex_string(&self) -> String {
        self.inner.to_rinex_string()
    }
}

impl RinexObs {
    fn check_epoch(&self, epoch_index: usize) -> Result<&CoreObsEpoch, JsValue> {
        self.inner.epochs().get(epoch_index).ok_or_else(|| {
            range_error(&format!(
                "epoch index {epoch_index} out of range for {} epochs",
                self.inner.epochs().len()
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
        lli: Vec::with_capacity(count),
        ssi: Vec::with_capacity(count),
    };
    for (sat, sat_rows) in rows {
        let token = sat.to_string();
        for row in sat_rows {
            out.satellites.push(token.clone());
            out.codes.push(row.code);
            out.value_cycles.push(nan_if_missing(row.value_cycles));
            out.frequency_hz.push(nan_if_missing(row.frequency_hz));
            out.wavelength_m.push(nan_if_missing(row.wavelength_m));
            out.value_m.push(nan_if_missing(row.value_m));
            out.phase_shift_cycles.push(row.phase_shift_cycles);
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
        inner: CoreRinexObs::parse(&text).map_err(engine_error)?,
    })
}

/// Alias of [`parseRinexObs`] for callers that read a file as bytes.
#[wasm_bindgen(js_name = loadRinexObs)]
pub fn load_rinex_obs(bytes: &[u8]) -> Result<RinexObs, JsValue> {
    parse_rinex_obs(bytes)
}
