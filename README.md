# @neilberkman/sidereon

GNSS and astrodynamics in the browser and in Node: propagate satellites, predict
passes, solve precise positions (SPP / RTK / PPP), and convert between coordinate
frames and time scales. This is the JavaScript and TypeScript interface to the
sidereon engine.

The engine is written in Rust and compiled to WebAssembly, so a browser tab or a
Node process gets the same numbers the native builds produce, with no server
round-trip and no native add-on to install. Results are reference-validated: the
SGP4 propagator is a bit-exact port of Vallado's reference implementation, frames
and time are checked against Skyfield and IERS, and the positioning stack is
checked against IGS products.

## Install

```sh
npm install @neilberkman/sidereon
```

The package is dual ESM/CJS and ships prebuilt wasm with bundled TypeScript
declarations. The two entry points initialize differently:

- **Browser / bundler (ESM):** import the default `init`, `await` it once, then
  call the API. `init()` fetches the `.wasm` for you.
- **Node (CommonJS):** `require(...)` loads and initializes the wasm at require
  time, so there is no init step and everything is ready synchronously.

## Example: propagate a TLE

Parse a two-line element set, run SGP4, and take look angles (azimuth, elevation,
range) from a ground station. No data files, no setup.

```js
import init, { Tle, GroundStation } from "@neilberkman/sidereon";

await init();

const tle = new Tle(
  "1 25544U 98067A   24001.50000000  .00016717  00000-0  10270-3 0  9009",
  "2 25544  51.6400 208.8657 0002644 250.3037 109.7782 15.49560812999990",
);
const station = new GroundStation(51.5, -0.1, 10.0);

// Epochs are unix microseconds as BigInt64Array.
const t = BigInt(Date.UTC(2024, 0, 1, 12)) * 1000n;
const look = tle.lookAngles(station, new BigInt64Array([t]));
console.log(look.azimuthDeg[0], look.elevationDeg[0], look.rangeKm[0]);
```

`Tle` also gives you `propagate(epochs)` (TEME position/velocity over a
`BigInt64Array` of unix-microsecond epochs) and `findPasses(station, start, end,
minElevationDeg)` for visibility windows.

### Node (CommonJS)

`require` resolves to the Node build, which initializes the wasm at require time:

```js
const { Tle, GroundStation } = require("@neilberkman/sidereon");

const tle = new Tle(line1, line2);
const look = tle.lookAngles(new GroundStation(51.5, -0.1, 10.0), epochs);
```

## Example: precise positioning

Load a precise SP3 ephemeris, hand it pseudoranges, and get a least-squares fix
back.

```js
import init, { loadSp3 } from "@neilberkman/sidereon";

await init();

// SP3-c precise orbits, as bytes (read from a file, fetch, or string).
const sp3 = loadSp3(new TextEncoder().encode(sp3Text));

// GPS L1 pseudoranges (m) for the satellites in view at the epoch.
const solution = sp3.solveSpp({
  observations: [
    { satelliteId: "G08", pseudorangeM: 23825519.8 },
    { satelliteId: "G10", pseudorangeM: 22717690.1 },
    { satelliteId: "G16", pseudorangeM: 20478653.4 },
    { satelliteId: "G18", pseudorangeM: 21768335.2 },
    { satelliteId: "G20", pseudorangeM: 21248327.7 },
    { satelliteId: "G21", pseudorangeM: 20808709.8 },
  ],
  tRxJ2000S: 646272000.0,
  tRxSecondOfDayS: 43200.0,
  dayOfYear: 176.5,
  initialGuess: [4.5e6, 0.5e6, 4.5e6, 0],
  corrections: { ionosphere: false, troposphere: false },
  withGeodetic: true,
});

console.log(solution.positionM); // ECEF m ~ [4484128, 550582, 4487561]
console.log(solution.rxClockS);  // receiver clock bias, seconds
```

`solveRtkFloat` / `solveRtkFixed` and `solvePppFloat` / `solvePppFixed` follow
the same pattern: an options object in, a result object with `Float64Array`
positions and scalar attributes out.

## Example: post-solve integrity

Use `raim` on per-satellite post-fit residuals after a solve. The direct result
has `faultDetected`, `testStatistic`, `threshold`, `worstSat`,
`reducedChiSquare`, `normalizedResiduals`, `rmsM`, and `dof`. RAIM residual
tests must use per-satellite residual variances; unit weights on metre-scale
residuals make `faultDetected` saturate near 100%. The JS API takes
inverse-variance weights, so compute them from your variance model.

```js
import { RaimWeights, raim } from "@neilberkman/sidereon";

const usedSats = ["G01", "G02", "G03", "G04", "G05", "G06"];
const residualsM = [0.2, -0.1, 0.3, 0.2, 9.0, -0.2];
const elevationDeg = [72, 42, 35, 64, 50, 28];
const sigma0M = 0.8;
const weights = Float64Array.from(
  elevationDeg.map((el) => {
    const sinEl = Math.max(Math.sin((el * Math.PI) / 180), 0.2);
    const varianceM2 = (sigma0M / sinEl) ** 2;
    return 1 / varianceM2;
  }),
);
const integrity = raim(
  { usedSats, residualsM },
  { pFa: 1e-3, weights: RaimWeights.bySatellite(usedSats, weights) },
);

console.log(integrity.faultDetected, integrity.testStatistic, integrity.worstSat);
```

Use `araim(geometry, ism, allocation)` for protection levels from line-of-sight
geometry and an integrity support message. `araimLpv200Allocation()` provides the
default LPV-200 budget. The direct result has `hplM`, `vplM`, `sigmaAccHM`, and
`sigmaAccVM`, plus the detailed monitor fields.

## Example: PROJ EGM96 vertical-grid interpolation

`GeoidGrid.fromProjEgm96Gtx(bytes)` loads the public OSGeo
`egm96_15.gtx` grid. Lookup requires an explicit arithmetic recipe because
valid PROJ builds can differ by one ULP:

```js
import { GeoidGrid, ProjVgridshiftArithmetic } from "@neilberkman/sidereon";

const grid = GeoidGrid.fromProjEgm96Gtx(gtxBytes);
const undulationM = grid.undulationProjRad(
  latitudeRad,
  longitudeRad,
  ProjVgridshiftArithmetic.FusedMultiplyAdd,
);
```

Use `SeparateMultiplyAdd` for a PROJ build without floating-point contraction.
Invalid coordinates throw a `RangeError` whose `kind` is
`"NonFiniteCoordinate"` or `"CoordinateOutsideGrid"`; its `coordinate` field
is `"latitude"` or `"longitude"`, and `detail` carries the complete typed
record.

## Capabilities

The wasm surface mirrors the full breadth of the engine:

- **Orbit propagation:** SGP4 from TLE and OMM, numerical propagation with a
  composable force model (spherical-harmonic geopotential to selectable degree
  and order, Sun/Moon third-body, solar radiation pressure, relativistic
  correction, space-weather-driven atmospheric drag) and orbital decay
  estimation with a post-decay validity latch, Kepler two-body propagation,
  batch constellation propagation, pass prediction, look angles, coverage, and
  batch least-squares orbit fitting against precise ephemerides (including
  terrestrial-frame SP3 through the Earth-orientation chain) with a
  per-satellite residual ledger.
- **GNSS positioning:** SPP, public `solveStatic` multi-epoch static
  positioning with covariance, leave-one-out redundancy diagnostics, and
  robust weighting, RINEX observation to SPP helpers
  (`sppInputsFromRinexObs` and `solveSppFromRinexObs`), RTK (float/fixed,
  sequential/static arcs, wide-lane fixed), PPP (float/fixed, including
  SPP-seeded auto-init), static PPP temporal-correlation covariance with
  calibrated day-length bounds, optional elevation cutoff, optional
  tropospheric-gradient estimation, DGNSS, moving-baseline RTK, DOP, velocity,
  RAIM over existing SPP solutions, broadcast-ephemeris FDE, and a
  Huber-reweighted SPP driver that runs fault detection and exclusion
  (RAIM/FDE) with iterative reweighting.
- **Integrity and error bounds:** direct post-solve RAIM fault detection,
  multi-constellation ARAIM protection levels,
  SBAS protection levels (DO-229), per-observation reliability (minimal
  detectable bias, internal/external), observability classification of every
  solution (rank, redundancy, conditioning), and covariance-derived error
  metrics (CEP, R95, SEP, error ellipse) that report wide or flagged bounds
  for weak geometry rather than fabricated confidence.
- **GNSS corrections and biases:** SBAS message decoding with SBAS-corrected
  solves, RTCM SSR orbit and clock correction streams, RTCM 3 broadcast
  ephemeris decode for GPS (1019), GLONASS (1020), Galileo (1045/1046),
  BeiDou (1042), and QZSS (1044), each real-data validated, Bias-SINEX code
  and phase biases (DCB/OSB).
- **Timing, estimation, and geodesy:** Allan-family clock stability with
  power-law noise identification (IEEE 1139), scalar Kalman and alpha-beta
  trackers, CFAR detection thresholds, source localization (ToA/TDOA),
  station velocity (MIDAS) with trajectory fitting and step detection,
  repeating-geometry (sidereal) filtering, geodesic direct and inverse
  problems (Karney), an epoch-aware terrestrial frame catalog (ITRF/ETRF
  Helmert sets), and EGM2008 geoid grids alongside EGM96.
- **Ephemeris and time:** broadcast ephemeris and SP3 (load/interpolate/merge),
  source-agnostic precise ephemeris sampling (one sampling interface over SP3,
  broadcast, or caller-supplied samples), JPL SPK (DAF/.bsp) kernels,
  scale-aware time (`Instant` with GMST/GAST and resolved TT/UT1/TDB), Earth
  orientation parameters.
- **Geometry and events:** reference frames (TEME, GCRS, ITRS, geodetic, ECEF),
  relative motion in RIC/RTN/LVLH frames with Clohessy-Wiltshire propagation,
  look angles, eclipse and shadow geometry, angular separation, position angle,
  phase angle, beta angle, conjunction screening with collision probability,
  initial orbit determination, Lambert transfer solutions, orbital elements
  with anomaly conversions and equinoctial / modified equinoctial forms.
- **Observational astronomy:** apparent places (astrometric and apparent
  RA/Dec plus topocentric azimuth/elevation with optional refraction) for the
  Sun, Moon, and any SPK body; Moon rise/set and meridian-transit finding;
  sub-solar and sub-observer points, day-night terminator, parallactic angle,
  satellite visual magnitude.
- **Almanac:** seasons, moon phases, lunar and solar eclipses, planetary
  conjunctions and oppositions, Sun/Moon/planet meridian transits.
- **Atmosphere and Earth models:** Klobuchar and NeQuick-G ionosphere, IONEX
  slant delay, troposphere models, geoid undulation (EGM96), PROJ EGM96 GTX
  interpolation with explicit fused/separate arithmetic and typed coordinate
  errors, solid Earth and pole tides, ocean tide loading, DTED terrain
  elevation lookup with batch queries, and memory-mappable terrain stores.
- **RF link budget:** free-space path loss, EIRP, C/N0, antenna gain, Doppler
  shift and range rate.
- **GNSS/INS fusion:** strapdown mechanization with an error-state EKF (UKF
  option), loose and tight coupling, IGG-III loose updates, an RTS smoother,
  a serializable filter state, and field mode (zero-velocity and
  zero-angular-rate updates, non-holonomic constraints, per-fix-status
  weighting, IMU-to-body mounting matrix), all off by default.
- **Reference-station static solve:** rover and reference observations in, one
  station coordinate with covariance and typed per-mode errors out.
- **Scenario simulation:** deterministic synthetic observables plus a
  ground-truth error ledger from a versioned scenario; identical bytes for the
  same scenario and seed.
- **Signal analysis:** closed-form BPSK/BOC spectra, spectral separation
  coefficients, DLL jitter, and multipath error envelopes against published
  constants, plus GPS C/A correlation helpers.
- **Format parsing and serialization:** TLE/OMM, CCSDS (OEM/OPM/CDM/TDM),
  RINEX observation/navigation/clock, CRINEX (Hatanaka), SP3, IONEX, ANTEX,
  Bias-SINEX, RTCM.

The binding adds no modeling of its own: every result is exactly what the engine
computes. Failures surface as the JS exception you would expect (`Error` for
engine rejections such as parse failures, non-converging solves, and SGP4 error
codes; `TypeError` for malformed input; `RangeError` for out-of-domain numbers).
Full signatures live in the bundled TypeScript declarations (`sidereon.d.ts`),
with the plain-object request types in `@neilberkman/sidereon/types`, including
typed ARAIM, RTK/PPP, fusion, signal-analysis, and terrain protocols.

A few conventions to know: positions and state arrays cross as
`Float64Array` (multi-epoch arrays are flat row-major, `3 * epochCount`); SGP4
epoch grids are `BigInt64Array` of unix microseconds; SP3 query epochs are plain
numbers in seconds since J2000.

## Live demo

The interactive demo at [sidereon.dev](https://sidereon.dev) runs on this exact
package: every computation happens client-side in your browser via this wasm
build.

## Links

- **Engine and core repo:** https://github.com/neilberkman/sidereon
- **Live demo:** https://sidereon.dev
- **Sibling interfaces:** sidereon-python (PyPI), sidereon-c, sidereon-ex (Hex).
  One validated engine, the same numbers in every language.

## License

MIT
