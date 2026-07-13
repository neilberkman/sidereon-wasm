//! SP3 precise-ephemeris product: parse, query satellite states by epoch, and
//! feed the SPP solver. Positions cross to JS as `Float64Array`, epochs as plain
//! numbers (seconds since J2000 in the product's own time scale).

use serde::Serialize;
use wasm_bindgen::prelude::*;

use sidereon_core::astro::time::{Instant, InstantRepr};
use sidereon_core::constants::{J2000_JD, SECONDS_PER_DAY};
use sidereon_core::ephemeris::{
    align_clock_reference as core_align_clock_reference,
    clock_reference_offset as core_clock_reference_offset,
    precise_interpolant_store_checksum64 as core_precise_interpolant_store_checksum64,
    ClockReferenceOffset as CoreClockReferenceOffset,
    MmapPreciseEphemerisInterpolant as CorePreciseInterpolantArtifact,
    PreciseInterpolantStoreError as CorePreciseInterpolantStoreError, Sp3 as CoreSp3,
};
use sidereon_core::Error as CoreError;
use sidereon_core::GnssSatelliteId;

use crate::error::{engine_error, range_error, type_error};
use crate::spp::{self, SppSolution};

/// Parse a satellite token (e.g. `"G01"`) into a typed id, or a `TypeError`.
fn parse_sat(token: &str) -> Result<GnssSatelliteId, JsValue> {
    token
        .parse::<GnssSatelliteId>()
        .map_err(|e| type_error(&format!("invalid satellite token {token:?}: {e}")))
}

fn instant_to_j2000_seconds(epoch: &Instant) -> f64 {
    match epoch.repr {
        InstantRepr::JulianDate(jd) => ((jd.jd_whole - J2000_JD) + jd.fraction) * SECONDS_PER_DAY,
        InstantRepr::Nanos(_) => f64::NAN,
    }
}

fn attach_detail<T: Serialize>(value: &JsValue, detail: &T) {
    let detail_value =
        serde_wasm_bindgen::to_value(detail).expect("serialize precise artifact error detail");
    js_sys::Reflect::set(value, &JsValue::from_str("detail"), &detail_value)
        .expect("attach precise artifact error detail");
}

fn typed_artifact_error<T: Serialize>(name: &'static str, message: String, detail: &T) -> JsValue {
    let js_error = js_sys::Error::new(&message);
    js_error.set_name(name);
    let value: JsValue = js_error.into();
    js_sys::Reflect::set(&value, &JsValue::from_str("kind"), &JsValue::from_str(name))
        .expect("attach precise artifact error kind");
    attach_detail(&value, detail);
    value
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Sp3EpochPredictionJs {
    epoch_j2000_seconds: f64,
    observed: bool,
    orbit_predicted_satellites: Vec<String>,
    clock_predicted_satellites: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Sp3PredictionSummaryJs {
    epochs: Vec<Sp3EpochPredictionJs>,
    observed_through_j2000_seconds: Option<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreciseInterpolantArtifactErrorDetail {
    name: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tag: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    satellite_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    found: Option<String>,
}

impl PreciseInterpolantArtifactErrorDetail {
    fn new(name: &'static str, message: String) -> Self {
        Self {
            name,
            message,
            path: None,
            reason: None,
            version: None,
            tag: None,
            satellite_id: None,
            expected: None,
            found: None,
        }
    }
}

fn hex_u64(value: u64) -> String {
    format!("{value:#x}")
}

fn precise_artifact_error_name(error: PreciseInterpolantArtifactError) -> &'static str {
    match error {
        PreciseInterpolantArtifactError::Io => "Io",
        PreciseInterpolantArtifactError::Parse => "Parse",
        PreciseInterpolantArtifactError::UnsupportedVersion => "UnsupportedVersion",
        PreciseInterpolantArtifactError::UnsupportedTimeScale => "UnsupportedTimeScale",
        PreciseInterpolantArtifactError::UnsupportedSatelliteSystem => "UnsupportedSatelliteSystem",
        PreciseInterpolantArtifactError::DuplicateSatellite => "DuplicateSatellite",
        PreciseInterpolantArtifactError::Checksum => "Checksum",
        PreciseInterpolantArtifactError::SatelliteChecksum => "SatelliteChecksum",
    }
}

fn precise_artifact_error(error: CorePreciseInterpolantStoreError) -> JsValue {
    let kind = PreciseInterpolantArtifactError::from(&error);
    let name = precise_artifact_error_name(kind);
    let mut detail = PreciseInterpolantArtifactErrorDetail::new(name, error.to_string());
    match &error {
        CorePreciseInterpolantStoreError::Io { path, .. } => {
            detail.path = Some(path.display().to_string());
        }
        CorePreciseInterpolantStoreError::Parse { reason } => {
            detail.reason = Some(reason.clone());
        }
        CorePreciseInterpolantStoreError::UnsupportedVersion { version } => {
            detail.version = Some(*version);
        }
        CorePreciseInterpolantStoreError::UnsupportedTimeScale { tag }
        | CorePreciseInterpolantStoreError::UnsupportedSatelliteSystem { tag } => {
            detail.tag = Some(*tag);
        }
        CorePreciseInterpolantStoreError::DuplicateSatellite { sat } => {
            detail.satellite_id = Some(sat.to_string());
        }
        CorePreciseInterpolantStoreError::Checksum { expected, found } => {
            detail.expected = Some(hex_u64(*expected));
            detail.found = Some(hex_u64(*found));
        }
        CorePreciseInterpolantStoreError::SatelliteChecksum {
            sat,
            expected,
            found,
        } => {
            detail.satellite_id = Some(sat.to_string());
            detail.expected = Some(hex_u64(*expected));
            detail.found = Some(hex_u64(*found));
        }
    }
    typed_artifact_error(name, detail.message.clone(), &detail)
}

/// Error category for precise-interpolant artifact open or serialization.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreciseInterpolantArtifactError {
    /// File I/O failed in the core artifact API.
    Io,
    /// Artifact bytes could not be parsed.
    Parse,
    /// The artifact version tag is unsupported.
    UnsupportedVersion,
    /// The artifact time-scale tag is unsupported.
    UnsupportedTimeScale,
    /// A satellite-system tag is unsupported.
    UnsupportedSatelliteSystem,
    /// A satellite appears more than once in the artifact index.
    DuplicateSatellite,
    /// The artifact file-level checksum did not match.
    Checksum,
    /// A satellite payload checksum did not match its index record.
    SatelliteChecksum,
}

impl From<&CorePreciseInterpolantStoreError> for PreciseInterpolantArtifactError {
    fn from(value: &CorePreciseInterpolantStoreError) -> Self {
        match value {
            CorePreciseInterpolantStoreError::Io { .. } => Self::Io,
            CorePreciseInterpolantStoreError::Parse { .. } => Self::Parse,
            CorePreciseInterpolantStoreError::UnsupportedVersion { .. } => Self::UnsupportedVersion,
            CorePreciseInterpolantStoreError::UnsupportedTimeScale { .. } => {
                Self::UnsupportedTimeScale
            }
            CorePreciseInterpolantStoreError::UnsupportedSatelliteSystem { .. } => {
                Self::UnsupportedSatelliteSystem
            }
            CorePreciseInterpolantStoreError::DuplicateSatellite { .. } => Self::DuplicateSatellite,
            CorePreciseInterpolantStoreError::Checksum { .. } => Self::Checksum,
            CorePreciseInterpolantStoreError::SatelliteChecksum { .. } => Self::SatelliteChecksum,
        }
    }
}

/// Stable string label for a [`PreciseInterpolantArtifactError`] enum value.
#[wasm_bindgen(js_name = preciseInterpolantArtifactErrorLabel)]
pub fn precise_interpolant_artifact_error_label(error: PreciseInterpolantArtifactError) -> String {
    precise_artifact_error_name(error).to_string()
}

/// A parsed SP3-c or SP3-d precise-ephemeris product.
///
/// Create with [`load_sp3`]. Query interpolated states with
/// [`Sp3.interpolate`], the exact parsed records with [`Sp3.state`], the node
/// epoch axis with [`Sp3.epochsJ2000Seconds`], run positioning with
/// [`Sp3.solveSpp`], and serialize back with [`Sp3.toSp3String`].
#[wasm_bindgen]
pub struct Sp3 {
    pub(crate) inner: CoreSp3,
}

#[wasm_bindgen]
impl Sp3 {
    /// Number of epochs in the product.
    #[wasm_bindgen(getter, js_name = epochCount)]
    pub fn epoch_count(&self) -> usize {
        self.inner.epoch_count()
    }

    /// Per-epoch observed/predicted flags and the contiguous observed-through
    /// boundary, derived from the parsed SP3 record flags.
    #[wasm_bindgen(js_name = predictionSummary)]
    pub fn prediction_summary(&self) -> Result<JsValue, JsValue> {
        let summary = self.inner.prediction_summary();
        let value = Sp3PredictionSummaryJs {
            epochs: summary
                .epochs
                .into_iter()
                .map(|epoch| Sp3EpochPredictionJs {
                    epoch_j2000_seconds: instant_to_j2000_seconds(&epoch.epoch),
                    observed: epoch.is_observed(),
                    orbit_predicted_satellites: epoch
                        .orbit_predicted_satellites
                        .into_iter()
                        .map(|satellite| satellite.to_string())
                        .collect(),
                    clock_predicted_satellites: epoch
                        .clock_predicted_satellites
                        .into_iter()
                        .map(|satellite| satellite.to_string())
                        .collect(),
                })
                .collect(),
            observed_through_j2000_seconds: summary
                .observed_through
                .as_ref()
                .map(instant_to_j2000_seconds),
        };
        serde_wasm_bindgen::to_value(&value).map_err(|error| engine_error(error.to_string()))
    }

    /// Satellite tokens present in the product (e.g. `"G01"`), ascending.
    #[wasm_bindgen(getter)]
    pub fn satellites(&self) -> Vec<String> {
        self.inner
            .satellites()
            .iter()
            .map(|sat| sat.to_string())
            .collect()
    }

    /// The product's parsed epochs as seconds since J2000 (the product's own
    /// time scale), ascending. This is the exact axis [`Sp3.interpolate`]
    /// consumes.
    #[wasm_bindgen(js_name = epochsJ2000Seconds)]
    pub fn epochs_j2000_seconds(&self) -> Vec<f64> {
        self.inner.epochs_j2000_seconds()
    }

    /// Interpolate `satellite`'s position and clock at each query epoch.
    ///
    /// `j2000Seconds` is a `Float64Array` of query times in seconds since J2000,
    /// in the product's own time scale. Throws a `TypeError` if the satellite is
    /// absent or the query array is empty, and an `Error` if a query lies in a
    /// coverage gap (the engine refuses to extrapolate).
    #[wasm_bindgen]
    pub fn interpolate(
        &self,
        satellite: &str,
        j2000_seconds: &[f64],
    ) -> Result<Sp3Interpolation, JsValue> {
        let sat = parse_sat(satellite)?;
        if j2000_seconds.is_empty() {
            return Err(type_error("j2000Seconds array is empty"));
        }

        let mut positions = Vec::with_capacity(j2000_seconds.len() * 3);
        let mut clocks = Vec::with_capacity(j2000_seconds.len());
        for &q in j2000_seconds {
            let state = self
                .inner
                .position_at_j2000_seconds(sat, q)
                .map_err(|e| match e {
                    CoreError::UnknownSatellite(id) => {
                        type_error(&format!("satellite {id} is not in the product"))
                    }
                    other => engine_error(format!("interpolation at j2000 second {q}: {other}")),
                })?;
            let p = state.position.as_array();
            positions.extend_from_slice(&p);
            clocks.push(state.clock_s.unwrap_or(f64::NAN));
        }
        Ok(Sp3Interpolation { positions, clocks })
    }

    /// The exact parsed state of `satellite` at record `epochIndex` (no
    /// interpolation). Throws a `RangeError` past the last epoch and a
    /// `TypeError` if the satellite has no record there.
    #[wasm_bindgen]
    pub fn state(&self, satellite: &str, epoch_index: usize) -> Result<Sp3State, JsValue> {
        let sat = parse_sat(satellite)?;
        let state = self.inner.state(sat, epoch_index).map_err(|e| match e {
            CoreError::EpochOutOfRange => {
                crate::error::range_error(&format!("epoch index {epoch_index} out of range"))
            }
            CoreError::UnknownSatellite(id) => type_error(&format!(
                "satellite {id} has no record at epoch {epoch_index}"
            )),
            other => engine_error(other),
        })?;
        Ok(Sp3State {
            position: state.position.as_array().to_vec(),
            clock_s: state.clock_s,
            velocity: state.velocity.map(|v| v.as_array().to_vec()),
            clock_event: state.flags.clock_event,
            clock_predicted: state.flags.clock_predicted,
            maneuver: state.flags.maneuver,
            orbit_predicted: state.flags.orbit_predicted,
        })
    }

    /// Run single-point positioning against this ephemeris.
    ///
    /// `request` is a plain object; see the `SppRequest` TypeScript type. Throws
    /// a `TypeError` for malformed input and an `Error` if the solve fails.
    #[wasm_bindgen(js_name = solveSpp)]
    pub fn solve_spp(&self, request: JsValue) -> Result<SppSolution, JsValue> {
        spp::solve(&self.inner, request)
    }

    /// Run SPP and attach a Doppler velocity/clock-drift solve when Doppler rows solve.
    ///
    /// `request` is the normal SPP request object. `dopplerObservations` is an
    /// array of `{ satelliteId, dopplerHz, carrierHz, satClockDriftSS? }`. The
    /// returned receiver solution carries `rxClockDriftSS` when velocity solved.
    #[wasm_bindgen(js_name = solveSppWithDopplerVelocity)]
    pub fn solve_spp_with_doppler_velocity(
        &self,
        request: JsValue,
        doppler_observations: JsValue,
    ) -> Result<crate::spp::SppDopplerSolution, JsValue> {
        spp::solve_with_doppler_velocity(&self.inner, request, doppler_observations)
    }

    /// Solve a batch of independent SPP epochs against this ephemeris in one call.
    ///
    /// `epochs` is an array of SPP request objects (the `SppRequest` shape) and
    /// `options` the shared `{ withGeodetic?, maxPdop?, coarseSearchSeeds? }`
    /// applied to every epoch. Returns an `SppBatchSolution` whose per-epoch
    /// results are index-aligned to `epochs`; each epoch independently converged
    /// or failed. Delegates to the serial reference batch kernel.
    #[wasm_bindgen(js_name = solveSppBatch)]
    pub fn solve_spp_batch(
        &self,
        epochs: JsValue,
        options: JsValue,
    ) -> Result<crate::spp::SppBatchSolution, JsValue> {
        spp::solve_batch(&self.inner, epochs, options)
    }

    /// Solve one static receiver position from multiple SPP-shaped epochs.
    ///
    /// `epochs` is an array of SPP request objects. `options` accepts
    /// `{ initialPositionM?, withGeodetic?, robust? }` and returns shared
    /// position, per-epoch clocks, covariance, residual, and influence surfaces.
    #[wasm_bindgen(js_name = solveStatic)]
    pub fn solve_static(
        &self,
        epochs: JsValue,
        options: JsValue,
    ) -> Result<crate::static_positioning::StaticSolution, JsValue> {
        crate::static_positioning::solve_static_sp3(&self.inner, epochs, options)
    }

    /// Compute DGNSS pseudorange corrections from a surveyed base station.
    ///
    /// `request` is `{ basePositionM, baseObservations, tRxJ2000S }`; returns a
    /// `{ satelliteId, correctionM }[]` array sorted by satellite token.
    #[wasm_bindgen(js_name = dgnssCorrections)]
    pub fn dgnss_corrections(&self, request: JsValue) -> Result<JsValue, JsValue> {
        crate::dgnss::corrections(&self.inner, request)
    }

    /// Solve a DGNSS rover position: compute base corrections, apply them to the
    /// rover, and run the corrected SPP. `request` carries the base + rover
    /// observations and the receive-time scalars; see the `DgnssSolveRequest`
    /// TypeScript type. Returns the corrected solution and the base baseline.
    #[wasm_bindgen(js_name = dgnssSolve)]
    pub fn dgnss_solve(&self, request: JsValue) -> Result<crate::dgnss::DgnssSolution, JsValue> {
        crate::dgnss::solve(&self.inner, request)
    }

    /// Run fault detection and exclusion (FDE) against this ephemeris.
    ///
    /// `request` is the SPP solve request plus the RAIM/exclusion options (see
    /// the `FdeRequest` TypeScript type). The core loop solves, runs RAIM, and
    /// excludes the worst satellite until the set passes or the exclusion budget
    /// is exhausted. Returns the surviving solution and the excluded satellites;
    /// throws an `Error` if the fault is unresolved.
    #[wasm_bindgen(js_name = fde)]
    pub fn fde(&self, request: JsValue) -> Result<crate::qc::FdeSolution, JsValue> {
        crate::qc::fde(&self.inner, request)
    }

    /// Run the core robust-reweighted SPP driver under the RAIM/FDE exclusion loop.
    ///
    /// `request` is the FDE request with a `robust` object. The implementation
    /// delegates to `sidereon_core::quality::spp_robust_fde_driver`.
    #[wasm_bindgen(js_name = sppRobustFdeDriver)]
    pub fn spp_robust_fde_driver(
        &self,
        request: JsValue,
    ) -> Result<crate::qc::FdeSolution, JsValue> {
        crate::qc::fde(&self.inner, request)
    }

    /// Estimate `other`'s per-epoch common clock offset relative to this product.
    ///
    /// The result is one row per matched epoch with enough common clocked
    /// satellites. Subtract `offsetS` from `other` clocks to put them on this
    /// product's clock datum. Delegates to
    /// `sidereon_core::ephemeris::clock_reference_offset`.
    #[wasm_bindgen(js_name = clockReferenceOffset)]
    pub fn clock_reference_offset(
        &self,
        other: &Sp3,
        min_common: Option<usize>,
    ) -> Result<Vec<Sp3ClockReferenceOffset>, JsValue> {
        let min_common = min_common.unwrap_or(5);
        if min_common == 0 {
            return Err(range_error("minCommon must be at least 1"));
        }
        Ok(
            core_clock_reference_offset(&self.inner, &other.inner, min_common)
                .into_iter()
                .map(Into::into)
                .collect(),
        )
    }

    /// Return a copy of `other` with its clocks shifted onto this product's
    /// clock datum. Epochs without an offset estimate are left unchanged.
    /// Delegates to `sidereon_core::ephemeris::align_clock_reference`.
    #[wasm_bindgen(js_name = alignClockReference)]
    pub fn align_clock_reference(
        &self,
        other: &Sp3,
        min_common: Option<usize>,
    ) -> Result<Sp3, JsValue> {
        let min_common = min_common.unwrap_or(5);
        if min_common == 0 {
            return Err(range_error("minCommon must be at least 1"));
        }
        Ok(Sp3 {
            inner: core_align_clock_reference(&self.inner, &other.inner, min_common),
        })
    }

    /// Serialize to standard SP3 text (the version named by the header, `c` or
    /// `d`). Deterministic: the same product always produces byte-identical text.
    #[wasm_bindgen(js_name = toSp3String)]
    pub fn to_sp3_string(&self) -> String {
        self.inner.to_sp3_string()
    }

    /// Build deterministic precise-interpolant artifact bytes from this SP3 product.
    #[wasm_bindgen(js_name = preciseInterpolantArtifactBytes)]
    pub fn precise_interpolant_artifact_bytes(&self) -> Result<Vec<u8>, JsValue> {
        self.inner
            .precise_interpolant_store_bytes()
            .map_err(precise_artifact_error)
    }

    /// Predict geometric ranges for many `(satellite, receiver, epoch)` requests
    /// against this ephemeris in one call. `requests` is an array of
    /// `{ sat, receiverEcefM, tRxJ2000S }`; returns an array of
    /// `{ geometricRangeM, satClockS, transmitTimeJ2000S, satPosEcefM }`
    /// index-aligned to `requests`. The same call shape works on a
    /// `PreciseEphemerisSampleSource`. Delegates to the serial reference kernel
    /// `sidereon_core::observables::predict_ranges`.
    #[wasm_bindgen(js_name = predictRanges)]
    pub fn predict_ranges(&self, requests: JsValue, options: JsValue) -> Result<JsValue, JsValue> {
        crate::precise_samples::predict_ranges_over(&self.inner, requests, options)
    }

    /// Evaluate emission-time state and media corrections for index-aligned satellites.
    ///
    /// `satellites` and `emissionEpochsJ2000S` share a row count. `receiverEcefM`
    /// is `[x, y, z]` metres. Without an IONEX product this can still request
    /// troposphere corrections by passing `{ troposphere: true }`.
    #[wasm_bindgen(js_name = emissionMediaBatch)]
    pub fn emission_media_batch(
        &self,
        satellites: Vec<String>,
        emission_epochs_j2000_s: &[f64],
        receiver_ecef_m: &[f64],
        options: JsValue,
    ) -> Result<crate::emission_media::EmissionMediaBatch, JsValue> {
        crate::emission_media::emission_media_batch_sp3(
            &self.inner,
            satellites,
            emission_epochs_j2000_s,
            receiver_ecef_m,
            options,
        )
    }

    /// Evaluate emission-time state plus IONEX/troposphere media corrections.
    ///
    /// `options.ionosphere` defaults to `true` on this IONEX-bearing path.
    /// `options.troposphere` defaults to `false`.
    #[wasm_bindgen(js_name = emissionMediaBatchIonex)]
    pub fn emission_media_batch_ionex(
        &self,
        ionex: &crate::ionex::Ionex,
        satellites: Vec<String>,
        emission_epochs_j2000_s: &[f64],
        receiver_ecef_m: &[f64],
        options: JsValue,
    ) -> Result<crate::emission_media::EmissionMediaBatch, JsValue> {
        crate::emission_media::emission_media_batch_sp3_ionex(
            &self.inner,
            ionex,
            satellites,
            emission_epochs_j2000_s,
            receiver_ecef_m,
            options,
        )
    }
}

/// Parse an SP3-c or SP3-d byte buffer (the full, already-decompressed file)
/// into a precise-ephemeris product. Throws an `Error` on malformed input.
#[wasm_bindgen(js_name = loadSp3)]
pub fn load_sp3(bytes: &[u8]) -> Result<Sp3, JsValue> {
    let inner = sidereon::load_sp3(bytes).map_err(engine_error)?;
    Ok(Sp3 { inner })
}

/// Compute the precise-interpolant artifact file-level checksum for byte content.
#[wasm_bindgen(js_name = preciseInterpolantArtifactChecksum64)]
pub fn precise_interpolant_artifact_checksum64(bytes: &[u8]) -> u64 {
    core_precise_interpolant_store_checksum64(bytes)
}

/// Open precise-interpolant artifact bytes as an evaluable in-memory product.
///
/// The returned handle owns its byte buffer because JS byte slices cannot be
/// borrowed across calls by this class boundary.
#[wasm_bindgen(js_name = openPreciseInterpolantArtifact)]
pub fn open_precise_interpolant_artifact(
    bytes: &[u8],
) -> Result<PreciseInterpolantArtifact, JsValue> {
    let inner =
        CorePreciseInterpolantArtifact::from_vec(bytes.to_vec()).map_err(precise_artifact_error)?;
    Ok(PreciseInterpolantArtifact { inner })
}

/// Evaluable precise-interpolant artifact opened from canonical store bytes.
#[wasm_bindgen]
pub struct PreciseInterpolantArtifact {
    inner: CorePreciseInterpolantArtifact<'static>,
}

#[wasm_bindgen]
impl PreciseInterpolantArtifact {
    /// Number of bytes retained by this artifact handle.
    #[wasm_bindgen(getter, js_name = byteLength)]
    pub fn byte_length(&self) -> usize {
        self.inner.as_bytes().len()
    }

    /// File-level artifact checksum.
    #[wasm_bindgen(getter)]
    pub fn checksum64(&self) -> u64 {
        self.inner.checksum64()
    }

    /// Artifact time scale label from the stored epoch axis.
    #[wasm_bindgen(getter, js_name = timeScale)]
    pub fn time_scale(&self) -> String {
        format!("{:?}", self.inner.time_scale())
    }

    /// Satellite tokens present in the artifact, ascending.
    #[wasm_bindgen(getter)]
    pub fn satellites(&self) -> Vec<String> {
        self.inner
            .satellites()
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    /// Evaluate one satellite state at a J2000-second epoch.
    pub fn evaluate(&self, satellite: &str, j2000_seconds: f64) -> Result<Sp3State, JsValue> {
        let sat = parse_sat(satellite)?;
        let state = self
            .inner
            .position_at_j2000_seconds(sat, j2000_seconds)
            .map_err(engine_error)?;
        Ok(Sp3State {
            position: state.position.as_array().to_vec(),
            clock_s: state.clock_s,
            velocity: state.velocity.map(|v| v.as_array().to_vec()),
            clock_event: state.flags.clock_event,
            clock_predicted: state.flags.clock_predicted,
            maneuver: state.flags.maneuver,
            orbit_predicted: state.flags.orbit_predicted,
        })
    }
}

/// One epoch's common clock offset between two SP3 products.
#[wasm_bindgen]
#[derive(Clone)]
pub struct Sp3ClockReferenceOffset {
    epoch_j2000_seconds: f64,
    offset_s: f64,
    satellites: usize,
}

#[wasm_bindgen]
impl Sp3ClockReferenceOffset {
    /// Matched epoch as seconds since J2000 in the product time scale.
    #[wasm_bindgen(getter, js_name = epochJ2000Seconds)]
    pub fn epoch_j2000_seconds(&self) -> f64 {
        self.epoch_j2000_seconds
    }

    /// `other - reference` clock datum, seconds.
    #[wasm_bindgen(getter, js_name = offsetS)]
    pub fn offset_s(&self) -> f64 {
        self.offset_s
    }

    /// Number of satellites used in the median offset estimate.
    #[wasm_bindgen(getter)]
    pub fn satellites(&self) -> usize {
        self.satellites
    }
}

impl From<CoreClockReferenceOffset> for Sp3ClockReferenceOffset {
    fn from(value: CoreClockReferenceOffset) -> Self {
        Self {
            epoch_j2000_seconds: instant_to_j2000_seconds(&value.epoch),
            offset_s: value.offset_s,
            satellites: value.satellites,
        }
    }
}

/// A batch of interpolated SP3 states.
#[wasm_bindgen]
pub struct Sp3Interpolation {
    positions: Vec<f64>,
    clocks: Vec<f64>,
}

#[wasm_bindgen]
impl Sp3Interpolation {
    /// Interpolated ECEF positions, metres, as a flat row-major `Float64Array`
    /// of length `3 * epochCount` (`[x0, y0, z0, x1, y1, z1, ...]`).
    #[wasm_bindgen(getter, js_name = positionM)]
    pub fn position_m(&self) -> Vec<f64> {
        self.positions.clone()
    }

    /// Interpolated clock offsets, seconds, as a `Float64Array` (NaN where the
    /// satellite has no clock estimate at that epoch).
    #[wasm_bindgen(getter, js_name = clockS)]
    pub fn clock_s(&self) -> Vec<f64> {
        self.clocks.clone()
    }

    /// Number of query epochs in the batch.
    #[wasm_bindgen(getter, js_name = epochCount)]
    pub fn epoch_count(&self) -> usize {
        self.clocks.len()
    }
}

/// The exact parsed state of one satellite at one SP3 epoch.
#[wasm_bindgen]
pub struct Sp3State {
    position: Vec<f64>,
    clock_s: Option<f64>,
    velocity: Option<Vec<f64>>,
    clock_event: bool,
    clock_predicted: bool,
    maneuver: bool,
    orbit_predicted: bool,
}

#[wasm_bindgen]
impl Sp3State {
    /// ECEF position as a `Float64Array` `[x, y, z]`, metres.
    #[wasm_bindgen(getter, js_name = positionM)]
    pub fn position_m(&self) -> Vec<f64> {
        self.position.clone()
    }

    /// Clock offset in seconds, or `undefined` for the bad-clock sentinel.
    #[wasm_bindgen(getter, js_name = clockS)]
    pub fn clock_s(&self) -> Option<f64> {
        self.clock_s
    }

    /// ECEF velocity as a `Float64Array` `[vx, vy, vz]`, metres per second, or
    /// `undefined` for a position-only product.
    #[wasm_bindgen(getter, js_name = velocityMS)]
    pub fn velocity_m_s(&self) -> Option<Vec<f64>> {
        self.velocity.clone()
    }

    /// Clock discontinuity (`E`) flagged at this epoch.
    #[wasm_bindgen(getter, js_name = clockEvent)]
    pub fn clock_event(&self) -> bool {
        self.clock_event
    }

    /// The clock is predicted, not fitted.
    #[wasm_bindgen(getter, js_name = clockPredicted)]
    pub fn clock_predicted(&self) -> bool {
        self.clock_predicted
    }

    /// The satellite was being maneuvered at this epoch.
    #[wasm_bindgen(getter)]
    pub fn maneuver(&self) -> bool {
        self.maneuver
    }

    /// The orbit is predicted, not fitted.
    #[wasm_bindgen(getter, js_name = orbitPredicted)]
    pub fn orbit_predicted(&self) -> bool {
        self.orbit_predicted
    }
}
