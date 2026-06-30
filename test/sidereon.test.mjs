// Real Node test against the wasm-pack `--target web` build: ESM import, async
// init, then exercise SP3 query, the reference SPP solve, IONEX slant delay, and
// SGP4 propagation / look angles against committed fixtures.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import init, { loadSp3, loadIonex, Tle, GroundStation } from "../pkg/sidereon.js";

const here = (rel) => fileURLToPath(new URL(rel, import.meta.url));

// wasm-bindgen web init: hand it the wasm bytes so no fetch/URL is needed.
const wasmBytes = await readFile(here("../pkg/sidereon_bg.wasm"));
await init({ module_or_path: wasmBytes });

// Decode a big-endian IEEE-754 hex bit pattern ("0x417b...") to a JS number, so
// the SPP fixture's exact float64 inputs cross the boundary without rounding.
function hexToF64(s) {
  const bits = BigInt(s);
  const view = new DataView(new ArrayBuffer(8));
  view.setBigUint64(0, bits, false);
  return view.getFloat64(0, false);
}

const norm3 = (a, i = 0) => Math.hypot(a[i], a[i + 1], a[i + 2]);

test("loadSp3 parses and queries a real precise-ephemeris product", async () => {
  const sp3 = loadSp3(await readFile(here("./fixtures/GRG0MGXFIN_20201760000_01D_15M_ORB.SP3")));

  assert.equal(sp3.epochCount, 96);
  assert.ok(sp3.satellites.includes("G01"), "G01 present");

  const epochs = sp3.epochsJ2000Seconds();
  assert.ok(epochs instanceof Float64Array && epochs.length === 96);

  // Interpolate G01 at its first node; a GPS satellite sits ~26,560 km from the
  // Earth centre, so the ECEF radius must land in the MEO shell.
  const interp = sp3.interpolate("G01", epochs.slice(0, 1));
  assert.equal(interp.epochCount, 1);
  const pos = interp.positionM;
  assert.ok(pos instanceof Float64Array && pos.length === 3);
  const radiusKm = norm3(pos) / 1000;
  assert.ok(radiusKm > 25000 && radiusKm < 28000, `GPS radius ${radiusKm} km in MEO shell`);

  // Exact parsed record agrees with the interpolated node to sub-metre.
  const state = sp3.state("G01", 0);
  assert.ok(Math.abs(norm3(state.positionM) - norm3(pos)) < 1.0);

  // A bad token is a TypeError, a coverage gap is an Error.
  assert.throws(() => sp3.interpolate("ZZ9", epochs.slice(0, 1)), TypeError);
});

test("solveSpp reproduces the engine reference solution", async () => {
  const fx = JSON.parse(
    await readFile(here("./fixtures/spp_trace_L0_minimal.json"), "utf8"),
  ).fixture;
  const inp = fx.inputs;

  const sp3 = loadSp3(await readFile(here(`./fixtures/${inp.sp3_file}`)));
  assert.equal(sp3.epochCount, 96);

  const request = {
    observations: inp.observations.map((o) => ({
      satelliteId: o.sat_id,
      pseudorangeM: hexToF64(o.p_meas_m),
    })),
    tRxJ2000S: hexToF64(inp.t_rx_j2000_s),
    tRxSecondOfDayS: hexToF64(inp.t_rx_sod_s),
    dayOfYear: hexToF64(inp.doy),
    initialGuess: fx.frozen.initial_guess_x0.map(hexToF64),
    // L0_minimal: geometry + clock + Sagnac only, no iono, no tropo.
    corrections: { ionosphere: false, troposphere: false },
    klobuchar: {
      alpha: inp.klobuchar_alpha.map(hexToF64),
      beta: inp.klobuchar_beta.map(hexToF64),
    },
    met: {
      pressureHpa: hexToF64(inp.met.pressure_hpa),
      temperatureK: hexToF64(inp.met.temperature_k),
      relativeHumidity: hexToF64(inp.met.relative_humidity),
    },
    withGeodetic: true,
  };

  const sol = sp3.solveSpp(request);

  const expected = fx.final_solution.x.map(hexToF64);
  const got = sol.positionM;
  assert.ok(got instanceof Float64Array && got.length === 3);

  const dx = got[0] - expected[0];
  const dy = got[1] - expected[1];
  const dz = got[2] - expected[2];
  const err = Math.hypot(dx, dy, dz);
  // The fixture documents AGREEMENT_BOUND_M = 1e-6 across an independent solve.
  assert.ok(err < 1.0e-6, `position agrees within ${err} m`);

  const expectedClock = hexToF64(fx.final_solution.rx_clock_s);
  assert.ok(Math.abs(sol.rxClockS - expectedClock) < 1.0e-9, "rx clock agrees");

  assert.ok(sol.geodetic instanceof Float64Array && sol.geodetic.length === 3);
  assert.equal(sol.usedSats.length, sol.residualsM.length);

  // Camel-case scalar accessors mirror the array.
  assert.equal(sol.xM, got[0]);
  assert.equal(sol.zM, got[2]);

  // An empty observation list is rejected as a TypeError, never a trap.
  assert.throws(() => sp3.solveSpp({ ...request, observations: [] }), TypeError);
});

test("loadIonex parses a TEC grid and returns a positive slant delay", async () => {
  const ionex = loadIonex(await readFile(here("./fixtures/synthetic_2map_7x7.20i")));

  assert.deepEqual(Array.from(ionex.latNodesDeg), [60, 40, 20, 0, -20, -40, -60]);
  assert.equal(ionex.lonNodesDeg.length, 7);
  assert.equal(ionex.exponent, -1);
  assert.equal(ionex.shellHeightKm, 450);

  const epochs = ionex.mapEpochsJ2000S;
  assert.equal(epochs.length, 2);

  // L1 (1575.42 MHz), straight up at the grid centre, on the first map epoch.
  const delay = ionex.slantDelay(0, 0, 0, 90, epochs[0], 1575.42e6);
  assert.ok(Number.isFinite(delay) && delay > 0, `slant delay ${delay} m is positive`);

  // Non-finite input is a RangeError.
  assert.throws(() => ionex.slantDelay(NaN, 0, 0, 90, epochs[0], 1575.42e6), RangeError);
});

test("Tle propagates SGP4 and reports look angles", async () => {
  const line1 = "1 25544U 98067A   18184.80969102  .00001614  00000-0  31745-4 0  9993";
  const line2 = "2 25544  51.6414 295.8524 0003435 262.6267 204.2868 15.54005638121106";
  const tle = new Tle(line1, line2);

  // Near the TLE epoch (2018-07-04). unix micros as BigInt64Array.
  const t0 = BigInt(Date.UTC(2018, 6, 4, 0, 0, 0)) * 1000n;
  const epochs = new BigInt64Array([t0, t0 + 600n * 1000000n]);

  const prop = tle.propagate(epochs);
  assert.equal(prop.epochCount, 2);
  const p = prop.positionKm;
  assert.ok(p instanceof Float64Array && p.length === 6);
  // ISS orbital radius ~6780 km (LEO).
  const r0 = norm3(p, 0);
  assert.ok(r0 > 6500 && r0 < 7000, `ISS radius ${r0} km in LEO shell`);
  // Orbital speed ~7.66 km/s.
  const v0 = norm3(prop.velocityKmS, 0);
  assert.ok(v0 > 7 && v0 < 8, `ISS speed ${v0} km/s`);

  const station = new GroundStation(37.7749, -122.4194); // San Francisco
  const looks = tle.lookAngles(station, epochs);
  assert.equal(looks.epochCount, 2);
  for (const el of looks.elevationDeg) {
    assert.ok(el >= -90 && el <= 90, `elevation ${el} in range`);
  }
  for (const az of looks.azimuthDeg) {
    assert.ok(az >= 0 && az <= 360, `azimuth ${az} in range`);
  }
  for (const rng of looks.rangeKm) {
    assert.ok(rng > 0 && rng < 45000, `range ${rng} km plausible`);
  }

  // Malformed TLE is an Error, never a trap.
  assert.throws(() => new Tle("garbage", "garbage"), Error);
});
