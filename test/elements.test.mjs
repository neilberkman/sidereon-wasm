// Classical element conversions (rv2coe / coe2rv) over
// sidereon_core::astro::elements. The check is a round trip: build a state from
// a known element set with coe2rv, recover the elements with rv2coe, and confirm
// the primary elements and the state come back. Angles are radians, lengths km.

import { test } from "node:test";
import assert from "node:assert/strict";

import { rv2coe, coe2rv } from "../pkg-node/sidereon.js";

const MU = 398600.4418; // km^3/s^2

const TOL = 1e-7;

test("coe2rv -> rv2coe recovers an elliptical inclined element set", () => {
  const a = 7000.0;
  const ecc = 0.01;
  const incl = 0.9;
  const raan = 0.5;
  const argp = 1.0;
  const nu = 2.0;
  const p = a * (1 - ecc * ecc);

  const state = coe2rv({ p, ecc, incl, raan, argp, nu }, MU);
  assert.equal(state.positionKm.length, 3);
  assert.equal(state.velocityKmS.length, 3);

  const coe = rv2coe(state.positionKm, state.velocityKmS, MU);
  assert.equal(coe.orbitType, "ellipticalInclined");
  assert.ok(Math.abs(coe.a - a) < 1e-6);
  assert.ok(Math.abs(coe.ecc - ecc) < TOL);
  assert.ok(Math.abs(coe.incl - incl) < TOL);
  assert.ok(Math.abs(coe.raan - raan) < TOL);
  assert.ok(Math.abs(coe.argp - argp) < TOL);
  assert.ok(Math.abs(coe.nu - nu) < TOL);
});

test("rv2coe -> coe2rv reproduces the original state vector", () => {
  // An arbitrary well-conditioned LEO state.
  const r = Float64Array.from([-6045.0, -3490.0, 2500.0]);
  const v = Float64Array.from([-3.457, 6.618, 2.533]);

  const coe = rv2coe(r, v, MU);
  const state = coe2rv(coe, MU);

  for (let i = 0; i < 3; i++) {
    assert.ok(Math.abs(state.positionKm[i] - r[i]) < 1e-6, `r[${i}]`);
    assert.ok(Math.abs(state.velocityKmS[i] - v[i]) < 1e-9, `v[${i}]`);
  }
});

test("rv2coe rejects a degenerate (zero-velocity) state via the engine", () => {
  assert.throws(() => rv2coe(Float64Array.from([7000, 0, 0]), Float64Array.from([0, 0, 0]), MU));
});

test("rv2coe rejects a wrong-length vector with a TypeError", () => {
  assert.throws(
    () => rv2coe(Float64Array.from([1, 2]), Float64Array.from([0, 1, 0]), MU),
    TypeError,
  );
});
