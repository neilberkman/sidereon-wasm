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

test("iodGaussAngles surfaces a degenerate time geometry as an engine error", () => {
  // Equal observation times collapse the polynomial divisors; the core returns
  // InvalidTimeGeometry, which the binding throws.
  const zeros = Float64Array.from([0, 0, 0]);
  const jd = Float64Array.from([2451545.0, 2451545.0, 2451545.0]);
  const rseci = Float64Array.from([6378, 0, 0, 6378, 0, 0, 6378, 0, 0]);
  assert.throws(() => iodGaussAngles(zeros, zeros, jd, zeros, rseci));
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

test("lambertBattin rejects an unknown direction selector", () => {
  const v1Hint = Float64Array.from([0, SPEED, 0]);
  assert.throws(() => lambertBattin(pos(0), pos(90), v1Hint, 1000, "sideways", "low", 0));
  assert.throws(() => lambertBattin(pos(0), pos(90), v1Hint, 1000, "short", "medium", 0));
});
