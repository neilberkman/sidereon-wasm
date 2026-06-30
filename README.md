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
the same shape: an options object in, a result object with `Float64Array`
positions and scalar attributes out.

## Capabilities

The wasm surface mirrors the full breadth of the engine:

- **Orbit propagation:** SGP4 from TLE and OMM, numerical propagation, batch
  constellation propagation, pass prediction, look angles, coverage.
- **GNSS positioning:** SPP, RTK (float/fixed), PPP (float/fixed), DGNSS, RAIM/FDE,
  DOP, velocity.
- **Ephemeris and time:** broadcast ephemeris and SP3 (load/interpolate/merge),
  JPL SPK (DAF/.bsp) kernels, scale-aware time (`Instant` with GMST/GAST and
  resolved TT/UT1/TDB), Earth orientation parameters.
- **Geometry and events:** reference frames (TEME, GCRS, ITRS, geodetic, ECEF),
  look angles, eclipse and shadow geometry, conjunction screening with collision
  probability, initial orbit determination, orbital elements.
- **Atmosphere:** Klobuchar and NeQuick-G ionosphere, IONEX slant delay,
  troposphere models.
- **RF link budget:** free-space path loss, EIRP, C/N0, antenna gain.
- **Format parsing and serialization:** TLE/OMM, CCSDS, RINEX, CRINEX (Hatanaka),
  SP3, IONEX, ANTEX, RTCM.

The binding adds no modeling of its own: every result is exactly what the engine
computes. Failures surface as the JS exception you would expect (`Error` for
engine rejections such as parse failures, non-converging solves, and SGP4 error
codes; `TypeError` for malformed input; `RangeError` for out-of-domain numbers).
Full signatures live in the bundled TypeScript declarations (`sidereon.d.ts`),
with the plain-object request shapes in `@neilberkman/sidereon/types`.

A few conventions worth knowing: positions and state arrays cross as
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
