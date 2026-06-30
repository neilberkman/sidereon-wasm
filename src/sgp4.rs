//! SGP4 / TLE propagation, topocentric look angles, and dense pass finding. The
//! kernels are `propagate_teme_arc` / `look_angle_arc` / `find_passes_for_satellite` over a
//! satellite (and its parsed elements) built once from the two TLE lines,
//! unchanged.

use wasm_bindgen::prelude::*;

use sidereon::passes::{
    find_passes_for_satellite, ground_track, look_angle_arc, look_angle_batch_serial,
    propagate_teme_arc, propagate_teme_batch_serial, visible_from_satellites,
    GroundStation as CoreGroundStation, PassFinderOptions, SatellitePass as CoreSatellitePass,
    UtcInstant, VisibleSatellite as CoreVisibleSatellite,
};
use sidereon::sgp4::{parse_tle_file_with_opsmode, OpsMode, Satellite};
use sidereon::tle::{
    encode as encode_tle, parse as parse_tle, ChecksumWarning as CoreChecksumWarning, TleElements,
};

use crate::error::{engine_error, range_error, type_error};
use crate::marshal::instants;

/// A geodetic ground station: WGS84 latitude/longitude in degrees, altitude in
/// metres. Pass to [`Tle.lookAngles`] / [`Tle.findPasses`].
#[wasm_bindgen]
pub struct GroundStation {
    inner: CoreGroundStation,
}

#[wasm_bindgen]
impl GroundStation {
    /// Create a ground station. `altitudeM` defaults to 0.
    #[wasm_bindgen(constructor)]
    pub fn new(latitude_deg: f64, longitude_deg: f64, altitude_m: Option<f64>) -> GroundStation {
        GroundStation {
            inner: CoreGroundStation {
                latitude_deg,
                longitude_deg,
                altitude_m: altitude_m.unwrap_or(0.0),
            },
        }
    }

    #[wasm_bindgen(getter, js_name = latitudeDeg)]
    pub fn latitude_deg(&self) -> f64 {
        self.inner.latitude_deg
    }

    #[wasm_bindgen(getter, js_name = longitudeDeg)]
    pub fn longitude_deg(&self) -> f64 {
        self.inner.longitude_deg
    }

    #[wasm_bindgen(getter, js_name = altitudeM)]
    pub fn altitude_m(&self) -> f64 {
        self.inner.altitude_m
    }
}

impl GroundStation {
    /// The wrapped core ground station, for sibling bindings (e.g. coverage)
    /// that hand a station straight to a core entry point.
    pub(crate) fn core(&self) -> CoreGroundStation {
        self.inner
    }
}

/// Map an `opsMode` string to the core enum. Defaults to `improved` (the engine
/// default, matching Python's `sgp4` package); `afspc` selects AFSPC parity.
fn ops_mode(label: Option<String>) -> Result<OpsMode, JsValue> {
    match label.as_deref() {
        None | Some("improved") => Ok(OpsMode::Improved),
        Some("afspc") => Ok(OpsMode::Afspc),
        Some(other) => Err(type_error(&format!(
            "invalid opsMode {other:?}: expected \"improved\" or \"afspc\""
        ))),
    }
}

/// An advisory TLE checksum discrepancy. The TLE grammar does not reject a line
/// on a bad modulo-10 checksum, so each mismatch is surfaced here rather than
/// thrown.
#[wasm_bindgen]
#[derive(Clone)]
pub struct ChecksumWarning {
    line_label: &'static str,
    expected: u8,
    computed: u8,
}

#[wasm_bindgen]
impl ChecksumWarning {
    /// Which line the discrepancy is on: `"line 1"` or `"line 2"`.
    #[wasm_bindgen(getter, js_name = lineLabel)]
    pub fn line_label(&self) -> String {
        self.line_label.to_string()
    }

    /// The checksum digit found in column 69 of the line.
    #[wasm_bindgen(getter)]
    pub fn expected(&self) -> u8 {
        self.expected
    }

    /// The checksum digit recomputed from columns 1-68.
    #[wasm_bindgen(getter)]
    pub fn computed(&self) -> u8 {
        self.computed
    }
}

impl From<&CoreChecksumWarning> for ChecksumWarning {
    fn from(w: &CoreChecksumWarning) -> Self {
        Self {
            line_label: w.line_label,
            expected: w.expected,
            computed: w.computed,
        }
    }
}

/// A satellite pass over a ground station: acquisition of signal, loss of
/// signal, culmination time (all unix microseconds UTC), and the elevation at
/// culmination.
#[wasm_bindgen]
#[derive(Clone, Copy)]
pub struct SatellitePass {
    aos_unix_us: i64,
    los_unix_us: i64,
    culmination_unix_us: i64,
    max_elevation_deg: f64,
}

impl From<&CoreSatellitePass> for SatellitePass {
    fn from(pass: &CoreSatellitePass) -> Self {
        Self {
            aos_unix_us: pass.aos.unix_microseconds(),
            los_unix_us: pass.los.unix_microseconds(),
            culmination_unix_us: pass.culmination.unix_microseconds(),
            max_elevation_deg: pass.max_elevation_deg,
        }
    }
}

#[wasm_bindgen]
impl SatellitePass {
    /// Acquisition of signal (rise above the mask), unix microseconds UTC.
    #[wasm_bindgen(getter, js_name = aosUnixUs)]
    pub fn aos_unix_us(&self) -> i64 {
        self.aos_unix_us
    }

    /// Loss of signal (set below the mask), unix microseconds UTC.
    #[wasm_bindgen(getter, js_name = losUnixUs)]
    pub fn los_unix_us(&self) -> i64 {
        self.los_unix_us
    }

    /// Culmination (maximum elevation) time, unix microseconds UTC.
    #[wasm_bindgen(getter, js_name = culminationUnixUs)]
    pub fn culmination_unix_us(&self) -> i64 {
        self.culmination_unix_us
    }

    /// Elevation at culmination, degrees.
    #[wasm_bindgen(getter, js_name = maxElevationDeg)]
    pub fn max_elevation_deg(&self) -> f64 {
        self.max_elevation_deg
    }

    /// Pass duration (LOS minus AOS), seconds.
    #[wasm_bindgen(getter, js_name = durationS)]
    pub fn duration_s(&self) -> f64 {
        (self.los_unix_us - self.aos_unix_us) as f64 / 1.0e6
    }
}

/// A parsed two-line element set with an initialized SGP4 satellite.
#[wasm_bindgen]
#[derive(Clone)]
pub struct Tle {
    elements: TleElements,
    satellite: Satellite,
    checksum_warnings: Vec<CoreChecksumWarning>,
}

impl Tle {
    /// Wrap an already-initialized core `Satellite`, recovering the parsed
    /// elements and checksum advisories from its own (validated) TLE lines. Used
    /// by [`parse_tle_file`] so each record's SGP4 record is reused rather than
    /// re-initialized.
    fn from_core_satellite(satellite: Satellite) -> Result<Tle, JsValue> {
        let parsed = parse_tle(satellite.line1(), satellite.line2()).map_err(engine_error)?;
        Ok(Tle {
            elements: parsed.elements,
            satellite,
            checksum_warnings: parsed.checksum_warnings,
        })
    }

    /// The wrapped core SGP4 satellite, for sibling bindings (e.g. coverage)
    /// that hand a satellite straight to a core entry point.
    pub(crate) fn core_satellite(&self) -> &Satellite {
        &self.satellite
    }
}

#[wasm_bindgen]
impl Tle {
    /// Parse two TLE lines and initialize SGP4. `opsMode` is `"improved"`
    /// (default) or `"afspc"`. Throws an `Error` if the lines fail to parse or
    /// SGP4 fails to initialize.
    #[wasm_bindgen(constructor)]
    pub fn new(line1: &str, line2: &str, ops_mode_label: Option<String>) -> Result<Tle, JsValue> {
        let mode = ops_mode(ops_mode_label)?;
        let parsed = parse_tle(line1, line2).map_err(engine_error)?;
        let satellite =
            Satellite::from_tle_with_opsmode(line1, line2, mode).map_err(engine_error)?;
        Ok(Tle {
            elements: parsed.elements,
            satellite,
            checksum_warnings: parsed.checksum_warnings,
        })
    }

    /// Re-encode the parsed elements as the two 69-character TLE lines (with
    /// checksums), as a `[line1, line2]` string array. For a well-formed input
    /// the round-trip is character-exact.
    #[wasm_bindgen(js_name = toLines)]
    pub fn to_lines(&self) -> Vec<String> {
        let (line1, line2) = encode_tle(&self.elements);
        vec![line1, line2]
    }

    /// Advisory checksum discrepancies found while parsing. Empty when both
    /// lines' checksums are valid.
    #[wasm_bindgen(getter, js_name = checksumWarnings)]
    pub fn checksum_warnings(&self) -> Vec<ChecksumWarning> {
        self.checksum_warnings
            .iter()
            .map(ChecksumWarning::from)
            .collect()
    }

    /// Propagate over a `BigInt64Array` of unix-microsecond epochs. Returns TEME
    /// position (km) and velocity (km/s). Throws an `Error` on SGP4 failure.
    #[wasm_bindgen]
    pub fn propagate(&self, epochs_unix_us: &[i64]) -> Result<TlePropagation, JsValue> {
        let predictions =
            propagate_teme_arc(&self.satellite, &instants(epochs_unix_us)).map_err(engine_error)?;
        let mut positions = Vec::with_capacity(predictions.len() * 3);
        let mut velocities = Vec::with_capacity(predictions.len() * 3);
        for p in &predictions {
            positions.extend_from_slice(&p.position);
            velocities.extend_from_slice(&p.velocity);
        }
        Ok(TlePropagation {
            positions,
            velocities,
        })
    }

    /// Topocentric azimuth/elevation/range from `station` over a
    /// `BigInt64Array` of unix-microsecond epochs. Throws an `Error` on failure.
    #[wasm_bindgen(js_name = lookAngles)]
    pub fn look_angles(
        &self,
        station: &GroundStation,
        epochs_unix_us: &[i64],
    ) -> Result<LookAngles, JsValue> {
        let looks = look_angle_arc(&self.satellite, station.inner, &instants(epochs_unix_us))
            .map_err(engine_error)?;
        Ok(LookAngles {
            azimuth_deg: looks.iter().map(|l| l.azimuth_deg).collect(),
            elevation_deg: looks.iter().map(|l| l.elevation_deg).collect(),
            range_km: looks.iter().map(|l| l.range_km).collect(),
        })
    }

    /// Find passes over `station` within `[startUnixUs, endUnixUs)` by dense
    /// elevation sampling. `elevationMaskDeg` defaults to 0, `stepSeconds` to 30,
    /// `timeToleranceS` to 1e-3. Throws a `RangeError` on a non-positive step or
    /// an end at or before the start.
    #[wasm_bindgen(js_name = findPasses)]
    pub fn find_passes(
        &self,
        station: &GroundStation,
        start_unix_us: i64,
        end_unix_us: i64,
        elevation_mask_deg: Option<f64>,
        step_seconds: Option<f64>,
        time_tolerance_s: Option<f64>,
    ) -> Result<Vec<SatellitePass>, JsValue> {
        let options = pass_options(elevation_mask_deg, step_seconds, time_tolerance_s)?;
        if end_unix_us <= start_unix_us {
            return Err(range_error("endUnixUs must be after startUnixUs"));
        }
        let passes = find_passes_for_satellite(
            &self.satellite,
            station.inner,
            UtcInstant::from_unix_microseconds(start_unix_us),
            UtcInstant::from_unix_microseconds(end_unix_us),
            options,
        )
        .map_err(engine_error)?;
        Ok(passes.iter().map(SatellitePass::from).collect())
    }

    /// Topocentric visibility arrays and a dense pass list over an epoch grid.
    ///
    /// `epochsUnixUs` must be a strictly increasing `BigInt64Array` with at least
    /// two samples. The az/el geometry is the same path as [`lookAngles`]; the
    /// pass list is the dense finder over `[first, last]`.
    #[wasm_bindgen(js_name = visibilitySeries)]
    pub fn visibility_series(
        &self,
        station: &GroundStation,
        epochs_unix_us: &[i64],
        elevation_mask_deg: Option<f64>,
        step_seconds: Option<f64>,
        time_tolerance_s: Option<f64>,
    ) -> Result<VisibilitySeries, JsValue> {
        let mask = elevation_mask_deg.unwrap_or(PassFinderOptions::default().elevation_mask_deg);
        let options = pass_options(elevation_mask_deg, step_seconds, time_tolerance_s)?;
        let inst = instants(epochs_unix_us);
        if inst.len() < 2 {
            return Err(type_error("epochsUnixUs must contain at least two samples"));
        }
        if epochs_unix_us.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(range_error("epochsUnixUs must be strictly increasing"));
        }
        let looks = look_angle_arc(&self.satellite, station.inner, &inst).map_err(engine_error)?;
        let passes = find_passes_for_satellite(
            &self.satellite,
            station.inner,
            inst[0],
            *inst.last().expect("non-empty instants checked"),
            options,
        )
        .map_err(engine_error)?;
        Ok(VisibilitySeries {
            epochs_unix_us: epochs_unix_us.to_vec(),
            azimuth_deg: looks.iter().map(|l| l.azimuth_deg).collect(),
            elevation_deg: looks.iter().map(|l| l.elevation_deg).collect(),
            range_km: looks.iter().map(|l| l.range_km).collect(),
            visible: looks
                .iter()
                .map(|l| u8::from(l.elevation_deg >= mask))
                .collect(),
            passes: passes.iter().map(SatellitePass::from).collect(),
        })
    }

    /// Sub-satellite (ground-track) WGS84 geodetic points over a `BigInt64Array`
    /// of unix-microsecond epochs. For each epoch the satellite is propagated to
    /// TEME and reduced TEME->GCRS->ECEF->geodetic by the engine's own transforms,
    /// honoring this `Tle`'s opsmode. Throws an `Error` on propagation or frame
    /// failure.
    #[wasm_bindgen(js_name = groundTrack)]
    pub fn ground_track(&self, epochs_unix_us: &[i64]) -> Result<GroundTrack, JsValue> {
        let points =
            ground_track(&self.satellite, &instants(epochs_unix_us)).map_err(engine_error)?;
        Ok(GroundTrack {
            latitude_deg: points.iter().map(|g| g.lat_rad.to_degrees()).collect(),
            longitude_deg: points.iter().map(|g| g.lon_rad.to_degrees()).collect(),
            altitude_km: points.iter().map(|g| g.height_m / 1000.0).collect(),
        })
    }

    /// NORAD catalog number (as recorded in the TLE).
    #[wasm_bindgen(getter, js_name = catalogNumber)]
    pub fn catalog_number(&self) -> String {
        self.elements.catalog_number.clone()
    }

    /// Classification character (`U`/`C`/`S`).
    #[wasm_bindgen(getter)]
    pub fn classification(&self) -> String {
        self.elements.classification.clone()
    }

    /// International designator (COSPAR ID).
    #[wasm_bindgen(getter, js_name = internationalDesignator)]
    pub fn international_designator(&self) -> String {
        self.elements.international_designator.clone()
    }

    /// Four-digit epoch year.
    #[wasm_bindgen(getter, js_name = epochYear)]
    pub fn epoch_year(&self) -> i32 {
        self.elements.epoch_year
    }

    /// Fractional day-of-year of the epoch.
    #[wasm_bindgen(getter, js_name = epochDayOfYear)]
    pub fn epoch_day_of_year(&self) -> f64 {
        self.elements.epoch_day_of_year
    }

    /// Inclination, degrees.
    #[wasm_bindgen(getter, js_name = inclinationDeg)]
    pub fn inclination_deg(&self) -> f64 {
        self.elements.inclination_deg
    }

    /// Right ascension of the ascending node, degrees.
    #[wasm_bindgen(getter, js_name = raanDeg)]
    pub fn raan_deg(&self) -> f64 {
        self.elements.raan_deg
    }

    /// Orbital eccentricity (dimensionless).
    #[wasm_bindgen(getter)]
    pub fn eccentricity(&self) -> f64 {
        self.elements.eccentricity
    }

    /// Argument of perigee, degrees.
    #[wasm_bindgen(getter, js_name = argPerigeeDeg)]
    pub fn arg_perigee_deg(&self) -> f64 {
        self.elements.arg_perigee_deg
    }

    /// Mean anomaly at epoch, degrees.
    #[wasm_bindgen(getter, js_name = meanAnomalyDeg)]
    pub fn mean_anomaly_deg(&self) -> f64 {
        self.elements.mean_anomaly_deg
    }

    /// Mean motion, revolutions per day.
    #[wasm_bindgen(getter, js_name = meanMotionRevPerDay)]
    pub fn mean_motion_rev_per_day(&self) -> f64 {
        self.elements.mean_motion
    }

    /// First derivative of mean motion (rev/day^2).
    #[wasm_bindgen(getter, js_name = meanMotionDot)]
    pub fn mean_motion_dot(&self) -> f64 {
        self.elements.mean_motion_dot
    }

    /// Second derivative of mean motion (rev/day^3).
    #[wasm_bindgen(getter, js_name = meanMotionDoubleDot)]
    pub fn mean_motion_double_dot(&self) -> f64 {
        self.elements.mean_motion_double_dot
    }

    /// B* drag term (TLE dimensionless convention).
    #[wasm_bindgen(getter)]
    pub fn bstar(&self) -> f64 {
        self.elements.bstar
    }

    /// Revolution number at epoch.
    #[wasm_bindgen(getter, js_name = revNumber)]
    pub fn rev_number(&self) -> i32 {
        self.elements.rev_number
    }
}

/// A named entry from a parsed TLE file: the satellite's name line (empty for a
/// bare 2-line set) paired with its initialized [`Tle`].
#[wasm_bindgen]
#[derive(Clone)]
pub struct NamedTle {
    name: String,
    tle: Tle,
}

#[wasm_bindgen]
impl NamedTle {
    /// The satellite name from the preceding name line, with any CelesTrak `0 `
    /// marker stripped. Empty string for a bare 2-line element set.
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// The initialized two-line element set. Call `.propagate()` /
    /// `.lookAngles()` / `.findPasses()` on it directly.
    #[wasm_bindgen(getter)]
    pub fn tle(&self) -> Tle {
        self.tle.clone()
    }
}

/// The result of [`parseTleFile`]: the satellites that parsed, plus a count of
/// complete records that were skipped because SGP4 initialization failed.
#[wasm_bindgen]
pub struct ParsedTleFile {
    satellites: Vec<NamedTle>,
    skipped: usize,
}

#[wasm_bindgen]
impl ParsedTleFile {
    /// The successfully parsed satellites, in file order, each as a `{ name, tle }`.
    #[wasm_bindgen(getter)]
    pub fn satellites(&self) -> Vec<NamedTle> {
        self.satellites.clone()
    }

    /// How many complete `(line 1, line 2)` records were found but skipped
    /// because their element set failed SGP4 initialization.
    #[wasm_bindgen(getter)]
    pub fn skipped(&self) -> usize {
        self.skipped
    }

    /// Number of satellites that parsed (length of `satellites`).
    #[wasm_bindgen(getter)]
    pub fn count(&self) -> usize {
        self.satellites.len()
    }
}

/// Parse a multi-record TLE file (CelesTrak / Space-Track style) into named,
/// initialized [`Tle`] instances. Handles bare 2-line sets, 3-line name+line1+line2
/// sets, and CelesTrak `0 NAME` markers; CRLF endings, blank lines, and
/// surrounding whitespace are tolerated. A record whose element set fails SGP4
/// initialization is skipped and counted in `skipped` rather than aborting the
/// whole file. `opsMode` is `"improved"` (default) or `"afspc"`.
#[wasm_bindgen(js_name = parseTleFile)]
pub fn parse_tle_file(
    text: &str,
    ops_mode_label: Option<String>,
) -> Result<ParsedTleFile, JsValue> {
    let mode = ops_mode(ops_mode_label)?;
    let parsed = parse_tle_file_with_opsmode(text, mode);
    let mut satellites = Vec::with_capacity(parsed.satellites.len());
    for named in parsed.satellites {
        satellites.push(NamedTle {
            name: named.name,
            tle: Tle::from_core_satellite(named.satellite)?,
        });
    }
    Ok(ParsedTleFile {
        satellites,
        skipped: parsed.skipped,
    })
}

fn pass_options(
    elevation_mask_deg: Option<f64>,
    step_seconds: Option<f64>,
    time_tolerance_s: Option<f64>,
) -> Result<PassFinderOptions, JsValue> {
    let core_default = PassFinderOptions::default();
    let elevation_mask_deg = elevation_mask_deg.unwrap_or(core_default.elevation_mask_deg);
    let step_seconds = step_seconds.unwrap_or(core_default.coarse_step_seconds);
    let time_tolerance_seconds = time_tolerance_s.unwrap_or(core_default.time_tolerance_seconds);
    if !elevation_mask_deg.is_finite() {
        return Err(range_error("elevationMaskDeg must be finite"));
    }
    if !step_seconds.is_finite() || step_seconds <= 0.0 {
        return Err(range_error("stepSeconds must be positive"));
    }
    if !time_tolerance_seconds.is_finite() || time_tolerance_seconds <= 0.0 {
        return Err(range_error("timeToleranceS must be positive"));
    }
    Ok(PassFinderOptions {
        elevation_mask_deg,
        coarse_step_seconds: step_seconds,
        time_tolerance_seconds,
    })
}

/// TEME states from a batched SGP4 propagation. Each array is flat row-major,
/// length `3 * epochCount`.
#[wasm_bindgen]
pub struct TlePropagation {
    positions: Vec<f64>,
    velocities: Vec<f64>,
}

#[wasm_bindgen]
impl TlePropagation {
    /// TEME positions, km, flat `[x0, y0, z0, x1, ...]`.
    #[wasm_bindgen(getter, js_name = positionKm)]
    pub fn position_km(&self) -> Vec<f64> {
        self.positions.clone()
    }

    /// TEME velocities, km/s, flat `[vx0, vy0, vz0, ...]`.
    #[wasm_bindgen(getter, js_name = velocityKmS)]
    pub fn velocity_km_s(&self) -> Vec<f64> {
        self.velocities.clone()
    }

    /// Number of epochs propagated.
    #[wasm_bindgen(getter, js_name = epochCount)]
    pub fn epoch_count(&self) -> usize {
        self.positions.len() / 3
    }
}

/// Topocentric look angles from a batched arc, each a `Float64Array` of length
/// `epochCount`.
#[wasm_bindgen]
pub struct LookAngles {
    azimuth_deg: Vec<f64>,
    elevation_deg: Vec<f64>,
    range_km: Vec<f64>,
}

#[wasm_bindgen]
impl LookAngles {
    /// Azimuth, degrees clockwise from north.
    #[wasm_bindgen(getter, js_name = azimuthDeg)]
    pub fn azimuth_deg(&self) -> Vec<f64> {
        self.azimuth_deg.clone()
    }

    /// Elevation, degrees above the horizon.
    #[wasm_bindgen(getter, js_name = elevationDeg)]
    pub fn elevation_deg(&self) -> Vec<f64> {
        self.elevation_deg.clone()
    }

    /// Slant range, kilometres.
    #[wasm_bindgen(getter, js_name = rangeKm)]
    pub fn range_km(&self) -> Vec<f64> {
        self.range_km.clone()
    }

    /// Number of epochs evaluated.
    #[wasm_bindgen(getter, js_name = epochCount)]
    pub fn epoch_count(&self) -> usize {
        self.azimuth_deg.len()
    }
}

/// Per-epoch topocentric visibility plus the dense pass list over the grid
/// window.
#[wasm_bindgen]
pub struct VisibilitySeries {
    epochs_unix_us: Vec<i64>,
    azimuth_deg: Vec<f64>,
    elevation_deg: Vec<f64>,
    range_km: Vec<f64>,
    visible: Vec<u8>,
    passes: Vec<SatellitePass>,
}

#[wasm_bindgen]
impl VisibilitySeries {
    /// Epoch grid, UTC unix microseconds, as a `BigInt64Array`.
    #[wasm_bindgen(getter, js_name = epochUnixUs)]
    pub fn epoch_unix_us(&self) -> Vec<i64> {
        self.epochs_unix_us.clone()
    }

    /// Azimuth, degrees clockwise from north.
    #[wasm_bindgen(getter, js_name = azimuthDeg)]
    pub fn azimuth_deg(&self) -> Vec<f64> {
        self.azimuth_deg.clone()
    }

    /// Elevation, degrees above the horizon.
    #[wasm_bindgen(getter, js_name = elevationDeg)]
    pub fn elevation_deg(&self) -> Vec<f64> {
        self.elevation_deg.clone()
    }

    /// Slant range, kilometres.
    #[wasm_bindgen(getter, js_name = rangeKm)]
    pub fn range_km(&self) -> Vec<f64> {
        self.range_km.clone()
    }

    /// Visibility mask as a `Uint8Array` (1 where `elevationDeg >=
    /// elevationMaskDeg`, else 0).
    #[wasm_bindgen(getter)]
    pub fn visible(&self) -> Vec<u8> {
        self.visible.clone()
    }

    /// Dense pass-finder results over the epoch-grid window.
    #[wasm_bindgen(getter)]
    pub fn passes(&self) -> Vec<SatellitePass> {
        self.passes.clone()
    }

    /// Number of epochs evaluated.
    #[wasm_bindgen(getter, js_name = epochCount)]
    pub fn epoch_count(&self) -> usize {
        self.epochs_unix_us.len()
    }

    /// Number of passes found over the epoch-grid window.
    #[wasm_bindgen(getter, js_name = passCount)]
    pub fn pass_count(&self) -> usize {
        self.passes.len()
    }
}

/// Sub-satellite ground-track points from a batched [`Tle.groundTrack`] call.
/// Each array is a `Float64Array` of length `epochCount`, aligned to the input
/// epoch grid. WGS84 geodetic: latitude/longitude in degrees, ellipsoidal height
/// in kilometres.
#[wasm_bindgen]
pub struct GroundTrack {
    latitude_deg: Vec<f64>,
    longitude_deg: Vec<f64>,
    altitude_km: Vec<f64>,
}

#[wasm_bindgen]
impl GroundTrack {
    /// Geodetic latitude of the sub-satellite point, degrees north.
    #[wasm_bindgen(getter, js_name = latDeg)]
    pub fn lat_deg(&self) -> Vec<f64> {
        self.latitude_deg.clone()
    }

    /// Geodetic longitude of the sub-satellite point, degrees east in `[-180, 180]`.
    #[wasm_bindgen(getter, js_name = lonDeg)]
    pub fn lon_deg(&self) -> Vec<f64> {
        self.longitude_deg.clone()
    }

    /// Ellipsoidal height above the WGS84 ellipsoid, kilometres.
    #[wasm_bindgen(getter, js_name = altKm)]
    pub fn alt_km(&self) -> Vec<f64> {
        self.altitude_km.clone()
    }

    /// Number of epochs evaluated.
    #[wasm_bindgen(getter, js_name = epochCount)]
    pub fn epoch_count(&self) -> usize {
        self.latitude_deg.len()
    }
}

/// One satellite visible above the elevation mask at a single instant, from
/// [`visibleFromSatellites`].
#[wasm_bindgen]
#[derive(Clone)]
pub struct VisibleSatellite {
    catalog_number: String,
    azimuth_deg: f64,
    elevation_deg: f64,
    range_km: f64,
    position_km: Vec<f64>,
}

impl From<&CoreVisibleSatellite> for VisibleSatellite {
    fn from(v: &CoreVisibleSatellite) -> Self {
        Self {
            catalog_number: v.catalog_number.clone(),
            azimuth_deg: v.azimuth_deg,
            elevation_deg: v.elevation_deg,
            range_km: v.range_km,
            position_km: v.position_km.to_vec(),
        }
    }
}

#[wasm_bindgen]
impl VisibleSatellite {
    /// The caller-supplied identity (the `ids[i]` paired with this satellite):
    /// a NORAD catalog number, a name, or whatever the caller chose.
    #[wasm_bindgen(getter, js_name = catalogNumber)]
    pub fn catalog_number(&self) -> String {
        self.catalog_number.clone()
    }

    /// Topocentric azimuth, degrees clockwise from north.
    #[wasm_bindgen(getter, js_name = azimuthDeg)]
    pub fn azimuth_deg(&self) -> f64 {
        self.azimuth_deg
    }

    /// Topocentric elevation, degrees above the horizon.
    #[wasm_bindgen(getter, js_name = elevationDeg)]
    pub fn elevation_deg(&self) -> f64 {
        self.elevation_deg
    }

    /// Slant range from the ground station, kilometres.
    #[wasm_bindgen(getter, js_name = rangeKm)]
    pub fn range_km(&self) -> f64 {
        self.range_km
    }

    /// TEME position of the satellite at the instant, km, as a length-3
    /// `Float64Array` `[x, y, z]`.
    #[wasm_bindgen(getter, js_name = positionKm)]
    pub fn position_km(&self) -> Vec<f64> {
        self.position_km.clone()
    }
}

/// Satellites visible above `minElevationDeg` from `station` at a single instant,
/// from already-initialized [`Tle`]s: the opsmode-preserving constellation
/// snapshot.
///
/// Each `Tle` in `satellites` carries the opsmode it was constructed with, so a
/// deep-space / opsmode-sensitive object is evaluated in its own mode (unlike the
/// element-based core path, which hardcodes AFSPC). `ids` supplies the identity
/// out-of-band: `ids[i]` (a catalog number, name, or anything) becomes the
/// `catalogNumber` of `satellites[i]`, so the two arrays must be the same length.
/// The `Tle` instances are consumed by this call.
///
/// `epochUnixUs` is a unix-microsecond UTC `bigint`. Per-satellite propagation or
/// frame failures are skipped; the result is filtered by `minElevationDeg` and
/// sorted by elevation descending. Throws an `Error` on an invalid station,
/// elevation threshold, or `ids`/`satellites` length mismatch.
#[wasm_bindgen(js_name = visibleFromSatellites)]
pub fn visible_from_satellites_js(
    satellites: Vec<Tle>,
    ids: Vec<String>,
    station: &GroundStation,
    epoch_unix_us: i64,
    min_elevation_deg: f64,
) -> Result<Vec<VisibleSatellite>, JsValue> {
    let sats: Vec<Satellite> = satellites.into_iter().map(|t| t.satellite).collect();
    let visible = visible_from_satellites(
        &sats,
        &ids,
        station.inner,
        UtcInstant::from_unix_microseconds(epoch_unix_us),
        min_elevation_deg,
    )
    .map_err(engine_error)?;
    Ok(visible.iter().map(VisibleSatellite::from).collect())
}

/// Propagate a fleet of already-initialized [`Tle`]s over a shared epoch grid in
/// a single call: the batched form of [`Tle.propagate`].
///
/// Element `(i, j)` of the result is `satellites[i]` propagated to
/// `epochsUnixUs[j]`, bit-for-bit identical to `satellites[i].propagate([
/// epochsUnixUs[j] ])` on its own; this is a thin wrapper over the engine's
/// serial batch kernel (the binding never spawns the rayon thread pool, since
/// wasm is single-threaded). Each `Tle` carries the opsmode it was constructed
/// with, so a deep-space / opsmode-sensitive object is propagated in its own
/// mode. The `Tle` instances are consumed by this call.
///
/// `epochsUnixUs` is a `BigInt64Array` of unix-microsecond UTC epochs shared by
/// every satellite. The hot case for a constellation animation is a single epoch
/// (`epochCount == 1`), giving one TEME state per satellite, but any epoch count
/// is supported. An empty fleet or empty epoch grid yields empty arrays. Throws
/// an `Error` (naming the satellite index) if a satellite fails to propagate.
#[wasm_bindgen(js_name = propagateBatch)]
pub fn propagate_batch(
    satellites: Vec<Tle>,
    epochs_unix_us: &[i64],
) -> Result<FleetPropagation, JsValue> {
    let sats: Vec<Satellite> = satellites.into_iter().map(|t| t.satellite).collect();
    let satellite_count = sats.len();
    let datetimes = instants(epochs_unix_us);
    let epoch_count = datetimes.len();

    let results = propagate_teme_batch_serial(&sats, &datetimes);

    let mut positions = Vec::with_capacity(satellite_count * epoch_count * 3);
    let mut velocities = Vec::with_capacity(satellite_count * epoch_count * 3);
    for (idx, arc) in results.into_iter().enumerate() {
        let predictions = arc.map_err(|e| engine_error(format!("satellite {idx}: {e}")))?;
        for p in &predictions {
            positions.extend_from_slice(&p.position);
            velocities.extend_from_slice(&p.velocity);
        }
    }

    Ok(FleetPropagation {
        satellite_count,
        epoch_count,
        positions,
        velocities,
    })
}

/// TEME states from a batched fleet SGP4 propagation. Each array is flat
/// row-major with shape `(satelliteCount, epochCount, 3)`: satellite `i`'s arc
/// occupies the contiguous slice `[i * epochCount * 3 .. (i + 1) * epochCount *
/// 3]`, and within it epoch `j` is `[.. j * 3 + 3]`. Satellite `i`'s arc equals
/// the [`TlePropagation`] from `satellites[i].propagate(epochsUnixUs)`.
#[wasm_bindgen]
pub struct FleetPropagation {
    satellite_count: usize,
    epoch_count: usize,
    positions: Vec<f64>,
    velocities: Vec<f64>,
}

#[wasm_bindgen]
impl FleetPropagation {
    /// TEME positions, km, flat row-major `(satelliteCount, epochCount, 3)`,
    /// length `3 * satelliteCount * epochCount`.
    #[wasm_bindgen(getter, js_name = positionKm)]
    pub fn position_km(&self) -> Vec<f64> {
        self.positions.clone()
    }

    /// TEME velocities, km/s, flat row-major `(satelliteCount, epochCount, 3)`,
    /// length `3 * satelliteCount * epochCount`.
    #[wasm_bindgen(getter, js_name = velocityKmS)]
    pub fn velocity_km_s(&self) -> Vec<f64> {
        self.velocities.clone()
    }

    /// Number of satellites in the fleet (the leading axis).
    #[wasm_bindgen(getter, js_name = satelliteCount)]
    pub fn satellite_count(&self) -> usize {
        self.satellite_count
    }

    /// Number of epochs each satellite was propagated to (the second axis).
    #[wasm_bindgen(getter, js_name = epochCount)]
    pub fn epoch_count(&self) -> usize {
        self.epoch_count
    }
}

/// A built-once constellation of already-initialized SGP4 satellites for repeated
/// batch operations.
///
/// Build it once from parsed [`Tle`]s, then call [`Constellation.propagate`] (and
/// `visible` / `lookAngleArcs` / `groundTracks` / `passes`) as often as you like:
/// it OWNS its satellites and BORROWS them on each call, so unlike the free
/// [`propagateBatch`] (which consumes the `Tle` handles it is given) the same
/// `Constellation` drives a live scene across frames with no re-parse and no
/// per-frame handle churn. This is the JS form of Elixir's `Sidereon.Constellation`.
///
/// It does no parsing or I/O: TLE text becomes satellites at the interface
/// boundary ([`Tle`] / [`parseTleFile`]); the constellation only batches the core
/// geometry over the satellites it was handed.
#[wasm_bindgen]
pub struct Constellation {
    satellites: Vec<Satellite>,
    ids: Vec<String>,
}

#[wasm_bindgen]
impl Constellation {
    /// Build a constellation from already-parsed [`Tle`]s, taking ownership of
    /// them. Each `Tle` keeps the opsmode it was constructed with, and its NORAD
    /// catalog number becomes the satellite's id in `visible`. The input order is
    /// the fleet order (the leading axis of every batch result and the
    /// `satelliteIndex` of every pass). The `Tle` handles are consumed; clone first
    /// (`tle.clone()`) to keep a per-satellite handle.
    #[wasm_bindgen(constructor)]
    pub fn new(satellites: Vec<Tle>) -> Constellation {
        let ids = satellites
            .iter()
            .map(|t| t.elements.catalog_number.clone())
            .collect();
        Constellation {
            satellites: satellites.into_iter().map(|t| t.satellite).collect(),
            ids,
        }
    }

    /// Number of satellites in the constellation (the leading axis of every batch
    /// result).
    #[wasm_bindgen(getter, js_name = satelliteCount)]
    pub fn satellite_count(&self) -> usize {
        self.satellites.len()
    }

    /// The satellites' NORAD catalog numbers, in fleet order.
    #[wasm_bindgen(getter, js_name = catalogNumbers)]
    pub fn catalog_numbers(&self) -> Vec<String> {
        self.ids.clone()
    }

    /// Propagate the whole constellation over a shared epoch grid in one call,
    /// borrowing it (NOT consumed, so the same `Constellation` drives every frame).
    ///
    /// `epochsUnixUs` is a `BigInt64Array` of unix-microsecond UTC epochs shared
    /// by every satellite. Element `(i, j)` of the result is satellite `i`
    /// propagated to epoch `j`, bit-for-bit identical to the per-satellite
    /// [`Tle.propagate`] path. A satellite that fails to propagate yields `NaN`
    /// for all of its epochs, keeping the result index-aligned (mirroring Elixir's
    /// `propagate_all`, which surfaces per-satellite outcomes rather than failing
    /// the whole batch). An empty constellation or empty epoch grid yields empty
    /// arrays.
    #[wasm_bindgen]
    pub fn propagate(&self, epochs_unix_us: &[i64]) -> FleetPropagation {
        let datetimes = instants(epochs_unix_us);
        let epoch_count = datetimes.len();
        let satellite_count = self.satellites.len();

        let results = propagate_teme_batch_serial(&self.satellites, &datetimes);

        let mut positions = Vec::with_capacity(satellite_count * epoch_count * 3);
        let mut velocities = Vec::with_capacity(satellite_count * epoch_count * 3);
        for arc in results {
            match arc {
                Ok(predictions) => {
                    for p in &predictions {
                        positions.extend_from_slice(&p.position);
                        velocities.extend_from_slice(&p.velocity);
                    }
                }
                Err(_) => {
                    // Index-aligned NaN fill: a failed satellite never drops the
                    // fleet out of alignment or freezes a live frame.
                    for _ in 0..epoch_count * 3 {
                        positions.push(f64::NAN);
                        velocities.push(f64::NAN);
                    }
                }
            }
        }

        FleetPropagation {
            satellite_count,
            epoch_count,
            positions,
            velocities,
        }
    }

    /// Satellites above `minElevationDeg` from `station` at a single epoch, each
    /// with its catalog number and topocentric az/el/range, sorted by elevation
    /// (highest first). The constellation form of the core `visibleFromSatellites`
    /// (Elixir `Constellation.visible_from`). Throws on an invalid station or
    /// elevation threshold.
    #[wasm_bindgen]
    pub fn visible(
        &self,
        station: &GroundStation,
        epoch_unix_us: i64,
        min_elevation_deg: f64,
    ) -> Result<Vec<VisibleSatellite>, JsValue> {
        let visible = visible_from_satellites(
            &self.satellites,
            &self.ids,
            station.inner,
            UtcInstant::from_unix_microseconds(epoch_unix_us),
            min_elevation_deg,
        )
        .map_err(engine_error)?;
        Ok(visible.iter().map(VisibleSatellite::from).collect())
    }

    /// Topocentric az/el/range arcs from `station` for every satellite over a
    /// shared epoch grid, in fleet order (element `i` is satellite `i`'s arc). A
    /// satellite that fails to propagate yields an empty arc, so the result stays
    /// index-aligned with the constellation. The batched form of [`Tle.lookAngles`].
    #[wasm_bindgen(js_name = lookAngleArcs)]
    pub fn look_angle_arcs(
        &self,
        station: &GroundStation,
        epochs_unix_us: &[i64],
    ) -> Vec<LookAngles> {
        let datetimes = instants(epochs_unix_us);
        let results = look_angle_batch_serial(&self.satellites, station.inner, &datetimes);
        results
            .into_iter()
            .map(|arc| match arc {
                Ok(looks) => LookAngles {
                    azimuth_deg: looks.iter().map(|l| l.azimuth_deg).collect(),
                    elevation_deg: looks.iter().map(|l| l.elevation_deg).collect(),
                    range_km: looks.iter().map(|l| l.range_km).collect(),
                },
                Err(_) => LookAngles {
                    azimuth_deg: Vec::new(),
                    elevation_deg: Vec::new(),
                    range_km: Vec::new(),
                },
            })
            .collect()
    }

    /// Sub-satellite WGS84 ground tracks for every satellite over a shared epoch
    /// grid, in fleet order (element `i` is satellite `i`'s track), each reduced
    /// TEME->GCRS->ITRS->geodetic by the engine's validated transforms. A satellite
    /// that fails yields an empty track, keeping the result index-aligned. The
    /// batched form of [`Tle.groundTrack`].
    #[wasm_bindgen(js_name = groundTracks)]
    pub fn ground_tracks(&self, epochs_unix_us: &[i64]) -> Vec<GroundTrack> {
        let datetimes = instants(epochs_unix_us);
        self.satellites
            .iter()
            .map(|satellite| match ground_track(satellite, &datetimes) {
                Ok(points) => GroundTrack {
                    latitude_deg: points.iter().map(|g| g.lat_rad.to_degrees()).collect(),
                    longitude_deg: points.iter().map(|g| g.lon_rad.to_degrees()).collect(),
                    altitude_km: points.iter().map(|g| g.height_m / 1000.0).collect(),
                },
                Err(_) => GroundTrack {
                    latitude_deg: Vec::new(),
                    longitude_deg: Vec::new(),
                    altitude_km: Vec::new(),
                },
            })
            .collect()
    }

    /// Passes over `station` within `[startUnixUs, endUnixUs)` for every satellite,
    /// flattened across the constellation: each [`FleetPass`] carries the
    /// `satelliteIndex` (fleet-order) it belongs to. `elevationMaskDeg` defaults to
    /// 0, `stepSeconds` to 30, `timeToleranceS` to 1e-3. A satellite that fails to
    /// scan contributes no passes. Throws a `RangeError` on a non-positive step or
    /// an end at or before the start.
    #[wasm_bindgen]
    pub fn passes(
        &self,
        station: &GroundStation,
        start_unix_us: i64,
        end_unix_us: i64,
        elevation_mask_deg: Option<f64>,
        step_seconds: Option<f64>,
        time_tolerance_s: Option<f64>,
    ) -> Result<Vec<FleetPass>, JsValue> {
        let options = pass_options(elevation_mask_deg, step_seconds, time_tolerance_s)?;
        if end_unix_us <= start_unix_us {
            return Err(range_error("endUnixUs must be after startUnixUs"));
        }
        let start = UtcInstant::from_unix_microseconds(start_unix_us);
        let end = UtcInstant::from_unix_microseconds(end_unix_us);

        let mut out = Vec::new();
        for (index, satellite) in self.satellites.iter().enumerate() {
            let passes =
                match find_passes_for_satellite(satellite, station.inner, start, end, options) {
                    Ok(passes) => passes,
                    Err(_) => continue,
                };
            for pass in &passes {
                out.push(FleetPass {
                    satellite_index: index as u32,
                    pass: SatellitePass::from(pass),
                });
            }
        }
        Ok(out)
    }
}

/// One pass in a [`Constellation.passes`] result: the pass geometry plus the
/// fleet-order `satelliteIndex` of the satellite it belongs to (map that index to
/// your own per-satellite metadata).
#[wasm_bindgen]
pub struct FleetPass {
    satellite_index: u32,
    pass: SatellitePass,
}

#[wasm_bindgen]
impl FleetPass {
    /// Fleet-order index of the satellite this pass belongs to.
    #[wasm_bindgen(getter, js_name = satelliteIndex)]
    pub fn satellite_index(&self) -> u32 {
        self.satellite_index
    }

    /// AOS (acquisition of signal), unix microseconds.
    #[wasm_bindgen(getter, js_name = aosUnixUs)]
    pub fn aos_unix_us(&self) -> i64 {
        self.pass.aos_unix_us()
    }

    /// LOS (loss of signal), unix microseconds.
    #[wasm_bindgen(getter, js_name = losUnixUs)]
    pub fn los_unix_us(&self) -> i64 {
        self.pass.los_unix_us()
    }

    /// Culmination (peak elevation) time, unix microseconds.
    #[wasm_bindgen(getter, js_name = culminationUnixUs)]
    pub fn culmination_unix_us(&self) -> i64 {
        self.pass.culmination_unix_us()
    }

    /// Peak elevation during the pass, degrees.
    #[wasm_bindgen(getter, js_name = maxElevationDeg)]
    pub fn max_elevation_deg(&self) -> f64 {
        self.pass.max_elevation_deg()
    }
}

#[cfg(test)]
mod drift_tests {
    //! The pass-finder defaults track the core `PassFinderOptions::default()`
    //! rather than literals duplicated in this binding.
    use super::*;

    #[test]
    fn pass_options_defaults_track_core() {
        let got = pass_options(None, None, None).expect("default pass options are valid");
        let core = PassFinderOptions::default();
        assert_eq!(got.elevation_mask_deg, core.elevation_mask_deg);
        assert_eq!(got.coarse_step_seconds, core.coarse_step_seconds);
        assert_eq!(got.time_tolerance_seconds, core.time_tolerance_seconds);
    }
}
