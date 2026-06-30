# sidereon

GNSS and astrodynamics for JavaScript and TypeScript: propagate satellites,
predict passes, solve precise positions (SPP / RTK / PPP), and convert between
coordinate frames and time — the real engine, compiled to WebAssembly, running
in the browser or Node — checked against the references the field trusts
(Vallado, Skyfield, IGS, IERS).

This isn't a reimplementation in JavaScript. It's the same Rust engine that
backs the Python, C, and Elixir interfaces, compiled to WebAssembly — so a
browser tab gets bit-for-bit the same numbers as the native builds, with no
server round-trip and no native add-on to install.

## Install

```sh
npm install @neilberkman/sidereon
```

It's a dual ESM/CJS package shipping prebuilt wasm — works in bundlers,
browsers, and Node. TypeScript declarations are bundled. There are two entry
points and they init differently:

- **Browser / bundler (ESM):** import the default `init`, `await` it once, then
  call the API. `init()` fetches the `.wasm` for you.
- **Node (CommonJS):** `require(...)` loads and initializes the wasm at require
  time — there is no init step.

## Quickstart: when does the ISS fly over you?

No data files, no setup — give it a two-line element set and a ground station,
and ask when the satellite is above the horizon.

```js
import init, { Tle, GroundStation } from "@neilberkman/sidereon";

await init(); // browser/bundler: fetches the wasm. (Node CJS: see below — no init.)

// Real orbital elements (grab fresh ones from CelesTrak any time).
const iss = new Tle(
  "1 25544U 98067A   26178.50947090  .00006280  00000+0  12016-3 0  9996",
  "2 25544  51.6322 248.9966 0004278 238.4942 121.5629 15.49454046573359",
);

// A ground station: latitude, longitude in degrees (altitude in metres, optional).
const berkeley = new GroundStation(37.87, -122.27);

// Epochs are UTC unix microseconds and cross as BigInt.
const us = (ms) => BigInt(ms) * 1000n;
const now = Date.now();

// Every pass above 10° over the next 24 hours.
const passes = iss.findPasses(berkeley, us(now), us(now + 24 * 3600 * 1000), 10.0);

const at = (uus) => new Date(Number(uus / 1000n)).toISOString().slice(11, 16);
for (const p of passes) {
  console.log(`${at(p.aosUnixUs)} UTC · ${(p.durationS / 60).toFixed(1)} min · peak ${p.maxElevationDeg.toFixed(0)}°`);
}
```

```
08:30 UTC · 6.8 min · peak 88°
10:09 UTC · 4.3 min · peak 16°
13:25 UTC · 3.2 min · peak 13°
15:01 UTC · 6.6 min · peak 56°
16:39 UTC · 3.5 min · peak 14°
```

`Tle` also gives you `propagate(epochs)` (TEME position/velocity over a
`BigInt64Array` of unix-microsecond epochs) and `lookAngles(station, epochs)`
(azimuth / elevation / range over a time grid).

### Node (CommonJS)

`require` resolves to the Node build, which loads the wasm at require time — no
init call, everything is ready synchronously:

```js
const { Tle, GroundStation } = require("@neilberkman/sidereon");

const iss = new Tle(line1, line2);
const passes = iss.findPasses(new GroundStation(37.87, -122.27), startUs, endUs, 10.0);
```

(Node ESM resolves the same default-`init` build as the browser; `await init()`
needs the wasm bytes there — `await init({ module_or_path: await readFile(new
URL("...sidereon_bg.wasm", import.meta.url)) })`. If you're not sure, `require`
is the simplest path in Node.)

## Precise positioning

The positioning engine is the other half of the library: load a precise
ephemeris, hand it pseudoranges, and get a least-squares fix back.

```js
import { loadSp3 } from "@neilberkman/sidereon"; // (after init)
import type { SppRequest } from "@neilberkman/sidereon/types";

const sp3 = loadSp3(new Uint8Array(sp3FileBytes));

const fix = sp3.solveSpp({
  observations: [
    { satelliteId: "G01", pseudorangeM: 21_000_123.4 },
    { satelliteId: "G08", pseudorangeM: 22_517_889.1 },
    // ...more satellites
  ],
  tRxJ2000S: 649_728_000,    // receive epoch, seconds since J2000
  tRxSecondOfDayS: 86_400,
  dayOfYear: 176,
  corrections: { ionosphere: true, troposphere: true },
  withGeodetic: true,
});

console.log(fix.positionM); // Float64Array [x, y, z] ECEF metres
console.log(fix.geodetic);  // Float64Array [latRad, lonRad, heightM]
console.log(fix.usedSats);  // satellites that contributed
```

`solveRtkFloat` / `solveRtkFixed` and `solvePppFloat` / `solvePppFixed` follow
the same shape — an options object in, a result object with `Float64Array`
positions and scalar attributes out.

## What's in the box

The wasm surface is broad — the same domains the native engine exposes:

- **Orbits** — SGP4/TLE and OMM, numerical propagation, passes, look angles, visibility
- **Frames & time** — TEME ↔ GCRS ↔ ITRS, geodetic ↔ ECEF, `Instant` with GMST/GAST and the resolved TT/UT1/TDB scales
- **Bodies** — Sun/Moon positions, eclipse / shadow geometry, plus JPL SPK (DAF/.bsp) kernels
- **Positioning** — SPP, RTK (float/fixed), PPP (float/fixed), DGNSS, DOP, velocity, RAIM/FDE
- **GNSS data** — SP3 (load/interpolate/merge), RINEX (obs/nav/clock), CRINEX (Hatanaka), ANTEX, IONEX slant delay, broadcast ephemeris
- **Space situational awareness** — conjunction/TCA screening, collision probability, CDM, covariance transforms
- **RF / signal** — link budget (FSPL, EIRP, C/N0, antenna gain), C/A code, acquisition/correlation

The binding adds no modeling of its own: every result is exactly what the engine
computes. Failures surface as the JS exception you'd expect — `Error` for engine
rejections (parse failures, non-converging solves, SGP4 error codes), `TypeError`
for malformed input, `RangeError` for out-of-domain numbers. Full signatures
live in the bundled TypeScript declarations (`sidereon.d.ts`), with the
plain-object request shapes in `@neilberkman/sidereon/types`.

A few conventions worth knowing: positions and state arrays cross as
`Float64Array` (multi-epoch arrays are flat row-major, `3 * epochCount`); SGP4
epoch grids are `BigInt64Array` of unix microseconds; SP3 query epochs are plain
numbers in seconds since J2000.

## Other languages

sidereon is one validated engine with first-class interfaces in **Rust**,
**Python**, **C**, **Elixir**, and **WebAssembly** — same numbers everywhere.
See the live demo and docs at [sidereon.dev](https://sidereon.dev).

## How it's validated

The SGP4 propagator is a Rust port of David Vallado's reference implementation,
bit-exact to it. Frames and time are checked against Skyfield and IERS; the
positioning stack is checked against IGS products. The published package ships
the prebuilt wasm, so there's nothing to compile to use it.

*Building from source (for contributors): `npm run build` (runs `wasm-pack
build` for the `web` and `nodejs` targets). Tests: `npm test`.*
