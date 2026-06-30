// Events (eclipse), body-angle, and DOP bindings reproduce the engine fixture
// bits, against events_bodies_dop.json plus the real SP3 product the DOP-series
// golden was sampled from.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  shadowFraction,
  eclipseStatus,
  sunAngle,
  moonAngle,
  sunElevation,
  phaseAngle,
  earthAngularRadius,
  gnssDop,
  gnssDopSeries,
  gnssVisibilitySeries,
  gnssPasses,
  Dop,
  Wgs84Geodetic,
  loadSp3,
} from "../pkg-node/sidereon.js";
import { fixture, fixtureJson, hexToF64, f64Bits } from "./helpers.mjs";

const FX = fixtureJson("events_bodies_dop.json");
const eqBits = (value, hex) => assert.equal(f64Bits(value), BigInt(hex));

// Flat (3n) Float64Array from a list of entries, reading a length-3 hex column.
const flat3 = (entries, key) => {
  const out = new Float64Array(entries.length * 3);
  entries.forEach((e, i) => {
    const row = e[key].map(hexToF64);
    out.set(row, i * 3);
  });
  return out;
};

test("shadow fraction + eclipse status match reference bits", () => {
  const sat = flat3(FX.eclipse, "satellite_position_km_hex");
  const sun = flat3(FX.eclipse, "sun_position_km_hex");
  const fractions = shadowFraction(sat, sun);
  assert.equal(fractions.length, FX.eclipse.length);
  FX.eclipse.forEach((c, i) => eqBits(fractions[i], c.shadow_fraction_hex));

  const statuses = eclipseStatus(sat, sun);
  FX.eclipse.forEach((c, i) => assert.equal(statuses[i], c.status.toLowerCase()));
});

test("angle helpers match reference bits", () => {
  const cases = FX.angles;
  const sat = flat3(cases, "satellite_position_km_hex");
  const sun = flat3(cases, "sun_position_km_hex");
  const moon = flat3(cases, "moon_position_km_hex");
  const obs = flat3(cases, "observer_position_km_hex");

  const checks = [
    [sunAngle(sat, sun), "sun_angle_deg_hex"],
    [moonAngle(sat, moon), "moon_angle_deg_hex"],
    [sunElevation(sat, sun), "sun_elevation_deg_hex"],
    [phaseAngle(sat, sun, obs), "phase_angle_deg_hex"],
    [earthAngularRadius(sat), "earth_angular_radius_deg_hex"],
  ];
  for (const [got, key] of checks) {
    assert.equal(got.length, cases.length);
    cases.forEach((c, i) => eqBits(got[i], c[key]));
  }
});

test("gnss DOP matches reference bits", () => {
  const c = FX.dop;
  const los = new Float64Array(c.line_of_sight_hex.length * 3);
  c.line_of_sight_hex.forEach((row, i) => los.set(row.map(hexToF64), i * 3));
  const weights = Float64Array.from(c.weights_hex.map(hexToF64));
  const receiver = new Wgs84Geodetic(
    hexToF64(c.receiver.lat_rad_hex),
    hexToF64(c.receiver.lon_rad_hex),
    hexToF64(c.receiver.height_m_hex),
  );

  const dop = gnssDop(los, weights, receiver);
  const constructed = Dop.fromLineOfSight(los, receiver, weights);
  for (const attr of ["gdop", "pdop", "hdop", "vdop", "tdop"]) {
    eqBits(dop[attr], c[`${attr}_hex`]);
    eqBits(constructed[attr], c[`${attr}_hex`]);
  }
});

test("DOP from az/el matches symmetric Rust geometry bits", () => {
  const receiver = new Wgs84Geodetic(0.0, 0.0);
  const az = Float64Array.from([45.0, 225.0, 135.0, 315.0]);
  const el = Float64Array.from([
    35.264389682754654, 35.264389682754654, -35.264389682754654, -35.264389682754654,
  ]);
  const dop = Dop.fromAzEl(az, el, receiver);
  const expected = {
    gdop: "0x3ff94c583ada5b53",
    pdop: "0x3ff8000000000000",
    hdop: "0x3ff3988e1409212e",
    vdop: "0x3febb67ae8584caa",
    tdop: "0x3fe0000000000000",
  };
  for (const [attr, bits] of Object.entries(expected)) eqBits(dop[attr], bits);
});

test("gnss DOP series matches real SP3 fixture", () => {
  const sp3 = loadSp3(fixture("sp3/GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const trace = fixtureJson("spp_trace_L2_tropo.json");
  const truth = trace.fixture.final_solution.truth_x.slice(0, 3).map(hexToF64);
  const station = Float64Array.from(truth);
  const t0 = (2459024.5 - 2451545.0) * 86400.0 + 0.5 * 86400.0;
  const epochs = Float64Array.from({ length: 13 }, (_, i) => t0 + i * 300.0);

  const series = gnssDopSeries(sp3, station, epochs, {
    elevationMaskDeg: 5.0,
    systems: ["G"],
    weighting: "unit",
  });

  assert.equal(series.epochCount, 13);
  assert.deepEqual(Array.from(series.stepIndex), [...Array(13).keys()]);
  assert.deepEqual(Array.from(series.j2000Seconds), Array.from(epochs));
  assert.deepEqual(
    Array.from(series.satelliteCount),
    [9, 9, 9, 9, 10, 11, 11, 11, 11, 11, 11, 11, 11],
  );
  assert.deepEqual(series.satellitesAt(0), [
    "G21",
    "G16",
    "G26",
    "G20",
    "G27",
    "G18",
    "G10",
    "G08",
    "G07",
  ]);

  // wasm32 libm vs the native libm the golden was emitted with can differ by a
  // few ULP on these transcendental-heavy (sqrt of LOS-matrix inverse trace)
  // outputs, so the series golden is matched within 1 ULP exactly as the Python
  // binding asserts it (test_events_bodies_dop.py::_assert_bits_within_one_ulp).
  const expectedFirst = {
    gdop: "0x4000c042642e3cbc",
    pdop: "0x3ffd34cde2c7e400",
    hdop: "0x3ff257e7df379517",
    vdop: "0x3ff6ba2ad4e284af",
    tdop: "0x3ff069acbf06750f",
  };
  for (const [attr, bits] of Object.entries(expectedFirst)) {
    const diff = f64Bits(series[attr][0]) - BigInt(bits);
    assert.ok(diff <= 1n && diff >= -1n, `${attr} within 1 ULP (diff ${diff})`);
  }
});

// `stepSeconds` is a plain JS number, not a BigInt: passing an ordinary numeric
// literal must reach the binding and produce samples rather than throwing in the
// wasm integer ABI.
test("gnss visibility series and passes accept a numeric stepSeconds", () => {
  const sp3 = loadSp3(fixture("sp3/GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const trace = fixtureJson("spp_trace_L2_tropo.json");
  const truth = trace.fixture.final_solution.truth_x.slice(0, 3).map(hexToF64);
  const station = Float64Array.from(truth);
  const t0 = (2459024.5 - 2451545.0) * 86400.0 + 0.5 * 86400.0;
  const t1 = t0 + 3600.0;

  const series = gnssVisibilitySeries(sp3, station, t0, t1, 300, {
    elevationMaskDeg: 5.0,
    systems: ["G"],
  });
  assert.ok(series.length > 0);
  assert.equal(series[0].stepIndex, 0);
  assert.ok(series[0].nVisible >= 0);

  const passes = gnssPasses(sp3, station, t0, t1, 300, {
    elevationMaskDeg: 5.0,
    systems: ["G"],
  });
  assert.ok(Array.isArray(passes));
  for (const pass of passes) {
    assert.equal(typeof pass.satellite, "string");
    assert.ok(pass.setStepIndex >= pass.riseStepIndex);
  }
});

test("gnss visibility series rejects a non-integer stepSeconds", () => {
  const sp3 = loadSp3(fixture("sp3/GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const station = Float64Array.from([1.1e6, -4.5e6, 4.3e6]);
  const t0 = (2459024.5 - 2451545.0) * 86400.0 + 0.5 * 86400.0;
  assert.throws(() => gnssVisibilitySeries(sp3, station, t0, t0 + 3600.0, 0, {}));
  assert.throws(() => gnssVisibilitySeries(sp3, station, t0, t0 + 3600.0, 12.5, {}));
  assert.throws(() => gnssPasses(sp3, station, t0, t0 + 3600.0, -5, {}));
});

test("events bad inputs throw", () => {
  assert.throws(() => shadowFraction(new Float64Array(0), new Float64Array(0)));
  assert.throws(() => sunAngle(new Float64Array(2), new Float64Array(3)));
  assert.throws(() => phaseAngle(new Float64Array(3), new Float64Array(6), new Float64Array(3)));
  assert.throws(() => new Wgs84Geodetic(Math.PI, 0.0));
  assert.throws(() =>
    gnssDop(new Float64Array(9), Float64Array.from([1, 1, 1]), new Wgs84Geodetic(0, 0)),
  );
});
