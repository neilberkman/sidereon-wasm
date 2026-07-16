// IOD (Gibbs / Herrick-Gibbs / Gauss) and Lambert (Battin) bindings delegate to
// sidereon_core::astro::{iod,lambert}. The numeric checks use an ideal circular
// Keplerian arc in the Vallado reference gravity field (MU = 398600.4415), where
// the three-position velocity solvers recover the circular speed and a Lambert
// quarter-orbit transfer reproduces it.

import { test } from "node:test";
import assert from "node:assert/strict";

import { iodGibbs, iodHerrickGibbs, iodGaussAngles, lambertBattin } from "../pkg-node/sidereon.js";

const MU = 398600.4415; // km^3/s^2, Vallado reference suite value
const R = 7000.0; // km
const SPEED = Math.sqrt(MU / R); // circular speed, km/s

const pos = (thetaDeg) => {
  const t = (thetaDeg * Math.PI) / 180;
  return Float64Array.from([R * Math.cos(t), R * Math.sin(t), 0]);
};
const mag = (v) => Math.hypot(v[0], v[1], v[2]);

const float64Bits = (value) => {
  const values = new Float64Array([value]);
  return new BigUint64Array(values.buffer)[0];
};

const assertExactVector = (actual, expected, label) => {
  assert.equal(actual.length, expected.length, `${label} length`);
  for (let i = 0; i < expected.length; i += 1) {
    assert.equal(float64Bits(actual[i]), float64Bits(expected[i]), `${label}[${i}]`);
  }
};

const assertRelativeVector = (actual, expected, tolerance, label) => {
  assert.equal(actual.length, expected.length, `${label} length`);
  for (let i = 0; i < expected.length; i += 1) {
    const scale = Math.max(Math.abs(expected[i]), 1.0);
    assert.ok(
      Math.abs(actual[i] - expected[i]) <= tolerance * scale,
      `${label}[${i}]: got ${actual[i]}, expected ${expected[i]}`,
    );
  }
};

test("iodGibbs recovers the circular speed at the middle position", () => {
  const out = iodGibbs(pos(-10), pos(0), pos(10));
  assert.equal(out.velocityKmS.length, 3);
  assert.ok(Math.abs(mag(out.velocityKmS) - SPEED) < 1e-3);
  // At theta = 0 the velocity is tangential (+y for a counterclockwise orbit).
  assert.ok(Math.abs(out.velocityKmS[0]) < 1e-6);
  assert.ok(Math.abs(out.velocityKmS[2]) < 1e-9);
  assert.ok(out.velocityKmS[1] > 0);
  assert.ok(Number.isFinite(out.theta12Rad) && Number.isFinite(out.theta23Rad));
  assert.ok(Number.isFinite(out.coplanarityRad));
});

test("iodGibbs rejects a zero-magnitude position via the engine", () => {
  assert.throws(() => iodGibbs(Float64Array.from([0, 0, 0]), pos(0), pos(10)));
});

test("iodHerrickGibbs recovers the circular speed for closely-spaced samples", () => {
  const n = Math.sqrt(MU / R ** 3); // mean motion, rad/s
  const dtheta = 0.5; // degrees
  const dt = (dtheta * Math.PI) / 180 / n; // seconds between samples
  const jd2 = 2451545.0;
  const dJd = dt / 86400.0;
  const out = iodHerrickGibbs(pos(-dtheta), pos(0), pos(dtheta), jd2 - dJd, jd2, jd2 + dJd);
  assert.ok(Math.abs(mag(out.velocityKmS) - SPEED) < 1e-3);
  assert.ok(out.velocityKmS[1] > 0);
});

test("iodGibbs matches Vallado Example 7-3 velocity at zero ULP", () => {
  const out = iodGibbs(
    Float64Array.from([0.0, 0.0, 6378.1363]),
    Float64Array.from([0.0, -4464.696, -5102.509]),
    Float64Array.from([0.0, 5740.323, 3189.068]),
  );
  assertExactVector(
    out.velocityKmS,
    [0.0, 5.5311472050176125, -5.191806413494606],
    "Gibbs velocity",
  );
});

test("iodHerrickGibbs matches Vallado Example 7-4 velocity at zero ULP", () => {
  const out = iodHerrickGibbs(
    Float64Array.from([3419.85564, 6019.82602, 2784.60022]),
    Float64Array.from([2935.91195, 6326.18324, 2660.59584]),
    Float64Array.from([2434.95202, 6597.38674, 2521.52311]),
    0.0,
    (60.0 + 16.48) / 86400.0,
    (120.0 + 33.04) / 86400.0,
  );
  assertExactVector(
    out.velocityKmS,
    [-6.441557227511062, 3.777559606719521, -1.7205675602414345],
    "Herrick-Gibbs velocity",
  );
});

test("iodGaussAngles surfaces a degenerate time geometry as an engine error", () => {
  // Equal observation times collapse the polynomial divisors; the core returns
  // InvalidTimeGeometry, which the binding throws.
  const zeros = Float64Array.from([0, 0, 0]);
  const jd = Float64Array.from([2451545.0, 2451545.0, 2451545.0]);
  const rseci = Float64Array.from([6378, 0, 0, 6378, 0, 0, 6378, 0, 0]);
  assert.throws(() => iodGaussAngles(zeros, zeros, jd, zeros, rseci));
});

test("iodGaussAngles matches Vallado Example 7-2", () => {
  const radians = (degrees) => (degrees * Math.PI) / 180.0;
  const out = iodGaussAngles(
    Float64Array.from([18.667717, 35.664741, 36.996583], radians),
    Float64Array.from([0.939913, 45.025748, 67.886655], radians),
    Float64Array.from([2456159.5, 2456159.5, 2456159.5]),
    Float64Array.from([0.4864351851851852, 0.49199074074074073, 0.4947685185185185]),
    Float64Array.from([
      4054.881, 2748.195, 4074.237, 3956.224, 2888.232, 4074.364, 3905.073, 2956.935, 4074.43,
    ]),
  );
  assertRelativeVector(
    out.positionKm,
    [6313.378130210396, 5247.50563344895, 6467.707164431651],
    1.0e-12,
    "Gauss position",
  );
  assertRelativeVector(
    out.velocityKmS,
    [-4.185488280436629, 4.7884929168898145, 1.721714659663034],
    1.0e-12,
    "Gauss velocity",
  );
});

test("lambertBattin reproduces the circular speed for a quarter-orbit transfer", () => {
  const period = 2 * Math.PI * Math.sqrt(R ** 3 / MU);
  const v1Hint = Float64Array.from([0, SPEED, 0]);
  const transfer = lambertBattin(pos(0), pos(90), v1Hint, period / 4, "short", "low", 0);
  assert.equal(transfer.departureVelocityKmS.length, 3);
  assert.equal(transfer.arrivalVelocityKmS.length, 3);
  assert.ok(Math.abs(mag(transfer.departureVelocityKmS) - SPEED) < 0.05);
  assert.ok(Math.abs(mag(transfer.arrivalVelocityKmS) - SPEED) < 0.05);
});

test("lambertBattin matches the Vallado short-way high-energy reference", () => {
  const earthRadiusKm = 6378.1363;
  const out = lambertBattin(
    Float64Array.from([2.5 * earthRadiusKm, 0.0, 0.0]),
    Float64Array.from([1.9151111 * earthRadiusKm, 1.606969 * earthRadiusKm, 0.0]),
    Float64Array.from([0.0, 4.999792554221911, 0.0]),
    92854.234,
    "short",
    "high",
    1,
  );
  assertRelativeVector(
    out.departureVelocityKmS,
    [-0.8696153795282852, 6.3351545812502374, 0.0],
    1.0e-12,
    "Lambert departure velocity",
  );
  assertRelativeVector(
    out.arrivalVelocityKmS,
    [-3.405994961791248, 5.41198791828363, 0.0],
    1.0e-12,
    "Lambert arrival velocity",
  );
});

test("lambertBattin rejects an unknown direction selector", () => {
  const v1Hint = Float64Array.from([0, SPEED, 0]);
  assert.throws(() => lambertBattin(pos(0), pos(90), v1Hint, 1000, "sideways", "low", 0));
  assert.throws(() => lambertBattin(pos(0), pos(90), v1Hint, 1000, "short", "medium", 0));
});
