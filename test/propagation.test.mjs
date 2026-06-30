// Numerical orbit propagation + the analytic/TLE (SGP4) numerical path through
// the WASM binding, mirroring sidereon-python/tests/test_numerical.py and
// tests/test_propagation.py against the same committed goldens.
//
// The fixtures carry the engine's reference states as IEEE-754 hex bit patterns.
// Pure-arithmetic outputs (times, the initial state echoed back) are reproduced
// bit-exact. Integrated / SGP4 states are checked bit-exact where the wasm32
// libm agrees with the native golden, and otherwise to a tight physical
// tolerance: the residual is a cross-libm ULP difference in the transcendental
// kernels (sqrt/sin/cos/atan2), not a marshalling or units error in this layer.

import { test } from "node:test";
import assert from "node:assert/strict";

import { propagateState, Tle, GroundStation } from "../pkg-node/sidereon.js";
import { fixtureJson, hexToF64 } from "./helpers.mjs";

// Bit-exact if the wasm output equals the native golden; otherwise assert a
// tight absolute tolerance and report the residual so a real bug cannot hide.
function assertCloseBits(actual, expected, tol, label) {
  if (actual === expected) return;
  const diff = Math.abs(actual - expected);
  assert.ok(diff <= tol, `${label}: |${actual} - ${expected}| = ${diff} exceeds ${tol}`);
}

test("numerical propagation reproduces the engine (bit-exact / tight tolerance)", () => {
  const fx = fixtureJson("numerical_propagation.json");
  const opts = fx.options;

  const eph = propagateState({
    epochS: hexToF64(fx.epoch_s_hex),
    positionKm: fx.position_km_hex.map(hexToF64),
    velocityKmS: fx.velocity_km_s_hex.map(hexToF64),
    timesS: fx.samples.map((s) => hexToF64(s.time_s_hex)),
    forceModel: fx.force_model,
    integrator: fx.integrator,
    absTol: opts.abs_tol,
    relTol: opts.rel_tol,
    initialStepS: opts.initial_step_s,
    minStepS: opts.min_step_s,
    maxStepS: opts.max_step_s,
    maxSteps: opts.max_steps,
  });

  const n = fx.samples.length;
  assert.equal(eph.epochCount, n);
  const times = eph.timesS;
  const pos = eph.positionKm;
  const vel = eph.velocityKmS;
  const states = eph.states;
  assert.equal(times.length, n);
  assert.equal(pos.length, n * 3);
  assert.equal(vel.length, n * 3);
  assert.equal(states.length, n * 6);

  // 1e-6 km = 1 mm position, 1e-9 km/s = 1 um/s velocity: far below any real
  // dynamics signal, but above the cross-libm ULP floor of the integrator.
  const POS_TOL = 1e-6;
  const VEL_TOL = 1e-9;

  fx.samples.forEach((sample, i) => {
    // Output epochs are passed straight through: bit-exact, no tolerance.
    assert.equal(times[i], hexToF64(sample.time_s_hex));
    const expPos = sample.position_km_hex.map(hexToF64);
    const expVel = sample.velocity_km_s_hex.map(hexToF64);
    for (let axis = 0; axis < 3; axis++) {
      assertCloseBits(pos[i * 3 + axis], expPos[axis], POS_TOL, `pos[${i}][${axis}]`);
      assertCloseBits(vel[i * 3 + axis], expVel[axis], VEL_TOL, `vel[${i}][${axis}]`);
      // states layout is [x, y, z, vx, vy, vz].
      assertCloseBits(states[i * 6 + axis], expPos[axis], POS_TOL, `states pos[${i}][${axis}]`);
      assertCloseBits(states[i * 6 + axis + 3], expVel[axis], VEL_TOL, `states vel[${i}][${axis}]`);
    }
  });
});

test("first numerical sample echoes the initial state bit-exact", () => {
  const fx = fixtureJson("numerical_propagation.json");
  const opts = fx.options;
  const eph = propagateState({
    epochS: hexToF64(fx.epoch_s_hex),
    positionKm: fx.position_km_hex.map(hexToF64),
    velocityKmS: fx.velocity_km_s_hex.map(hexToF64),
    timesS: fx.samples.map((s) => hexToF64(s.time_s_hex)),
    forceModel: fx.force_model,
    integrator: fx.integrator,
    absTol: opts.abs_tol,
    relTol: opts.rel_tol,
    initialStepS: opts.initial_step_s,
    minStepS: opts.min_step_s,
    maxStepS: opts.max_step_s,
    maxSteps: opts.max_steps,
  });
  const expPos = fx.position_km_hex.map(hexToF64);
  const pos = eph.positionKm;
  // The sampler holds the initial state until the first requested epoch, so the
  // first sample is the input verbatim regardless of libm.
  for (let axis = 0; axis < 3; axis++) {
    assert.equal(pos[axis], expPos[axis]);
  }
});

test("two-body circular orbit returns to its start after one period", () => {
  const mu = 398600.4418;
  const r = 7000.0;
  const v = Math.sqrt(mu / r);
  const period = 2.0 * Math.PI * Math.sqrt((r * r * r) / mu);
  const eph = propagateState({
    epochS: 0.0,
    positionKm: [r, 0.0, 0.0],
    velocityKmS: [0.0, v, 0.0],
    timesS: [0.0, period],
    forceModel: "two_body",
    integrator: "dp54",
    absTol: 1e-12,
    relTol: 1e-12,
  });
  const pos = eph.positionKm;
  assert.ok(Math.abs(pos[3] - r) < 1e-6);
  assert.ok(Math.abs(pos[4]) < 1e-6);
});

test("empty times returns an empty ephemeris", () => {
  const eph = propagateState({
    epochS: 0.0,
    positionKm: [7000.0, 0.0, 0.0],
    velocityKmS: [0.0, 7.5, 0.0],
    timesS: [],
  });
  assert.equal(eph.epochCount, 0);
  assert.equal(eph.timesS.length, 0);
  assert.equal(eph.positionKm.length, 0);
  assert.equal(eph.velocityKmS.length, 0);
  assert.equal(eph.states.length, 0);
});

test("bad position shape throws TypeError", () => {
  assert.throws(
    () =>
      propagateState({
        epochS: 0.0,
        positionKm: [7000.0, 0.0], // length 2, not 3
        velocityKmS: [0.0, 7.5, 0.0],
        timesS: [0.0, 60.0],
      }),
    TypeError,
  );
});

test("unknown force model throws TypeError", () => {
  assert.throws(
    () =>
      propagateState({
        epochS: 0.0,
        positionKm: [7000.0, 0.0, 0.0],
        velocityKmS: [0.0, 7.5, 0.0],
        timesS: [0.0, 60.0],
        forceModel: "newtonian_soup",
      }),
    TypeError,
  );
});

// --- analytic / TLE (SGP4) numerical path -----------------------------------

const epochsBigInt = (fx) => BigInt64Array.from(fx.epochs.map((e) => BigInt(e.unix_microseconds)));

test("SGP4 TLE propagation reproduces the engine (bit-exact / tight tolerance)", () => {
  const fx = fixtureJson("sgp4_topocentric.json");
  const tle = new Tle(fx.tle.line1, fx.tle.line2, fx.opsmode);
  const prop = tle.propagate(epochsBigInt(fx));

  const n = fx.epochs.length;
  assert.equal(prop.epochCount, n);
  const pos = prop.positionKm;
  const vel = prop.velocityKmS;
  assert.equal(pos.length, n * 3);
  assert.equal(vel.length, n * 3);

  // 1e-6 km / 1e-9 km/s: well below real orbit signal, above the SGP4 cross-libm
  // ULP floor.
  fx.epochs.forEach((epoch, i) => {
    const expPos = epoch.position_km_hex.map(hexToF64);
    const expVel = epoch.velocity_km_s_hex.map(hexToF64);
    for (let axis = 0; axis < 3; axis++) {
      assertCloseBits(pos[i * 3 + axis], expPos[axis], 1e-6, `tle pos[${i}][${axis}]`);
      assertCloseBits(vel[i * 3 + axis], expVel[axis], 1e-9, `tle vel[${i}][${axis}]`);
    }
  });
});

test("SGP4 look angles reproduce the engine (bit-exact / tight tolerance)", () => {
  const fx = fixtureJson("sgp4_topocentric.json");
  const tle = new Tle(fx.tle.line1, fx.tle.line2, fx.opsmode);
  const station = new GroundStation(
    fx.station.latitude_deg,
    fx.station.longitude_deg,
    fx.station.altitude_m,
  );
  const look = tle.lookAngles(station, epochsBigInt(fx));

  const az = look.azimuthDeg;
  const el = look.elevationDeg;
  const rng = look.rangeKm;
  fx.epochs.forEach((epoch, i) => {
    // 1e-9 deg ~ 1e-11 rad of pointing, 1e-6 km = 1 mm range.
    assertCloseBits(az[i], hexToF64(epoch.azimuth_deg_hex), 1e-9, `az[${i}]`);
    assertCloseBits(el[i], hexToF64(epoch.elevation_deg_hex), 1e-9, `el[${i}]`);
    assertCloseBits(rng[i], hexToF64(epoch.range_km_hex), 1e-6, `range[${i}]`);
  });
});
