// Real Node test against the wasm-pack `--target web` build: ESM import, async
// init, then exercise SP3 query, the reference SPP solve, IONEX slant delay, and
// SGP4 propagation / look angles against committed fixtures.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import init, {
  loadSp3,
  loadIonex,
  Tle,
  GroundStation,
  sp3PreciseEphemerisSamples,
  sp3PreciseEphemerisAccuracySamples,
  preciseEphemerisSamplesFromSamplesWithAccuracy,
  preciseEphemerisSamplesFromSamples,
  PreciseEphemerisInterpolant,
  ExactEpochQuery,
} from "../pkg/sidereon.js";
import { coreGoldens, f64Bits } from "./helpers.mjs";

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

test("SP3 accuracy sidecars survive sample and cached-interpolant construction", async () => {
  const sp3 = loadSp3(await readFile(here("./fixtures/GRG0MGXFIN_20201760000_01D_15M_ORB.SP3")));
  const samples = sp3PreciseEphemerisSamples(sp3);
  const accuracy = sp3PreciseEphemerisAccuracySamples(sp3);
  const source = preciseEphemerisSamplesFromSamplesWithAccuracy(samples, accuracy);
  const cached = PreciseEphemerisInterpolant.fromSamplesWithAccuracy(samples, accuracy);
  // The product lists its satellites in header order; a sample source holds
  // them keyed by satellite, so compare membership.
  assert.deepEqual([...source.satellites].sort(), [...sp3.satellites].sort());
  assert.equal(accuracy.length, samples.length);
  assert.equal(samples[0].instant.representation.kind, "julianDate");
  assert.equal(accuracy[0].instant.representation.kind, "julianDate");
  assert.deepEqual(
    accuracy.map(({ sat, epoch, instant }) => [sat, epoch, instant]),
    samples.map(({ sat, epoch, instant }) => [sat, epoch, instant]),
  );
  const first = samples[0];
  const query = ExactEpochQuery.fromBinaryJ2000Seconds(first.epoch);
  const parsedState = sp3.stateAtExactQuery(first.sat, query);
  const cachedState = cached.evaluateExact(first.sat, query);
  assert.deepEqual(cachedState.positionM, parsedState.positionM);
  const selected = source.selectedPositionClockAtExactQueries(first.sat, query, query);
  assert.deepEqual(selected.value.positionEcefM, Array.from(parsedState.positionM));
  const transmitClock = source.transmitEpochClockAtExactQueries(first.sat, query, query);
  assert.ok(transmitClock === null || typeof transmitClock === "object");
  assert.equal(typeof sp3.ephemerisVarianceAtExactQuery(first.sat, query, query), "number");
  assert.ok(
    ["term", "unavailable", "notApplicable"].includes(
      sp3.clockRelativityAtExactQuery(first.sat, query, parsedState.positionM).kind,
    ),
  );
});

test("sample instant DTO preserves distinct exact epochs sharing one rounded second", () => {
  const instant = (nanos) => ({
    scale: "GPST",
    representation: { kind: "nanos", nanos },
  });
  const samples = [
    {
      sat: "G01",
      epoch: 1_000_000_000,
      instant: instant("1000000000000000000"),
      positionEcefM: [20_000_000, 0, 0],
      clockS: 0,
      clockEvent: false,
    },
    {
      sat: "G01",
      epoch: 1_000_000_000,
      instant: instant("1000000002000000000"),
      positionEcefM: [20_000_001, 0, 0],
      clockS: 0,
      clockEvent: false,
    },
    {
      sat: "G02",
      epoch: 1_000_000_000,
      instant: instant("1000000000000000001"),
      positionEcefM: [20_000_000, 1, 0],
      clockS: 0,
      clockEvent: false,
    },
    {
      sat: "G02",
      epoch: 1_000_000_000,
      instant: instant("1000000002000000001"),
      positionEcefM: [20_000_001, 1, 0],
      clockS: 0,
      clockEvent: false,
    },
  ];
  assert.equal(samples[0].epoch, samples[2].epoch);
  assert.notEqual(samples[0].instant.representation.nanos, samples[2].instant.representation.nanos);
  assert.equal(
    Number(BigInt(samples[0].instant.representation.nanos)) / 1e9,
    Number(BigInt(samples[2].instant.representation.nanos)) / 1e9,
  );
  assert.doesNotThrow(() => preciseEphemerisSamplesFromSamples(samples));
  const unknown = { kind: "unknown" };
  const accuracy = samples.map((sample) => ({
    sat: sample.sat,
    epoch: sample.epoch,
    instant: sample.instant,
    positionVarianceM2: [unknown, unknown, unknown],
    clockVarianceM2: unknown,
  }));
  assert.doesNotThrow(() => preciseEphemerisSamplesFromSamplesWithAccuracy(samples, accuracy));
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
  const requestWithOmittedCorrections = { ...request };
  delete requestWithOmittedCorrections.corrections;
  const omittedCorrections = sp3.solveSpp(requestWithOmittedCorrections);
  assert.deepEqual(omittedCorrections.positionM, sol.positionM);
  assert.equal(omittedCorrections.clockBiasM, sol.clockBiasM);

  // The trace's independent solve leaves out the precise-clock relativistic
  // term -2 r.v / c^2 that positioning applies (RTKLIB peph2pos), so its
  // final solution is no longer the engine's. The engine's own solve of these
  // inputs, reproduced natively by test/golden-gen, is the reference, compared
  // to the bit.
  const ref = coreGoldens().sppTrace;
  const got = sol.positionM;
  assert.ok(got instanceof Float64Array && got.length === 3);
  assert.deepEqual(
    Array.from(got, f64Bits),
    ref.positionM.map((h) => BigInt(h)),
  );
  assert.equal(f64Bits(sol.rxClockS), BigInt(ref.rxClockS));
  // How the solve ended, as the engine reports it for the same inputs.
  assert.deepEqual(sol.metadata, ref.metadata);
  assert.equal(sol.metadata.status, "SelectionSettled");
  assert.equal(sol.metadata.converged, true);

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

test("sample stateAtExactQuery preserves the same typed missing-satellite cause as evaluateExact", async () => {
  const sp3 = loadSp3(await readFile(here("./fixtures/GRG0MGXFIN_20201760000_01D_15M_ORB.SP3")));
  const samples = sp3PreciseEphemerisSamples(sp3);
  const source = preciseEphemerisSamplesFromSamples(samples);
  const query = ExactEpochQuery.fromBinaryJ2000Seconds(samples[0].epoch);
  const capture = (call) => {
    try {
      call();
    } catch (error) {
      return error;
    }
    assert.fail("expected missing-satellite lookup to throw");
  };
  const direct = capture(() => source.stateAtExactQuery("G99", query));
  const cached = capture(() =>
    PreciseEphemerisInterpolant.fromPreciseEphemerisSamples(source).evaluateExact("G99", query),
  );
  assert.equal(direct.name, "Error");
  assert.equal(direct.message, "unknown satellite: G99");
  assert.deepEqual(direct.detail, { kind: "UNKNOWN_SATELLITE", satelliteId: "G99" });
  assert.deepEqual(direct.cause, direct.detail);
  assert.deepEqual(cached.detail, direct.detail);
  assert.equal(cached.message, direct.message);
});
