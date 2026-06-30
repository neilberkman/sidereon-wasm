// Standalone force-model accelerations reproduce the core force-model bits.
//
// The two-body and J2 golden bit patterns are exactly the
// `acceleration_matches_orbis_force_wrapper_bits` goldens in
// sidereon-core/src/astro/forces/{two_body,j2}.rs, so the WASM binding is held
// to the same 0-ULP bar the core asserts (and the same bits the Elixir binding
// pins in propagator_test.exs).

import { test } from "node:test";
import assert from "node:assert/strict";

import { forceTwoBodyAcceleration, forceJ2Acceleration } from "../pkg-node/sidereon.js";
import { f64Bits } from "./helpers.mjs";

// The shared probe state, kilometres and km/s, from the core force goldens.
const POSITION_KM = [7000.0, -1210.0, 1300.0];
const VELOCITY_KM_S = [0.0, 0.0, 0.0];

test("two-body acceleration matches the core force-model bits", () => {
  const a = forceTwoBodyAcceleration(POSITION_KM, VELOCITY_KM_S);
  assert.equal(a.length, 3);
  assert.equal(f64Bits(a[0]), 13798562943973640097n);
  assert.equal(f64Bits(a[1]), 4563548234789153053n);
  assert.equal(f64Bits(a[2]), 13787359517156423902n);
});

test("J2 acceleration matches the core force-model bits", () => {
  const a = forceJ2Acceleration(POSITION_KM, VELOCITY_KM_S);
  assert.equal(a.length, 3);
  assert.equal(f64Bits(a[0]), 13754131348549160135n);
  assert.equal(f64Bits(a[1]), 4519025615523880849n);
  assert.equal(f64Bits(a[2]), 13750824904549515386n);
});

test("a zero-magnitude position is rejected by the engine", () => {
  assert.throws(() => forceTwoBodyAcceleration([0, 0, 0], VELOCITY_KM_S));
  assert.throws(() => forceJ2Acceleration([0, 0, 0], VELOCITY_KM_S));
});

test("a non-finite input throws a RangeError", () => {
  assert.throws(() => forceTwoBodyAcceleration([NaN, 0, 0], VELOCITY_KM_S), RangeError);
  assert.throws(() => forceJ2Acceleration(POSITION_KM, [Infinity, 0, 0]), RangeError);
});

test("a wrong-length vector throws a TypeError", () => {
  assert.throws(() => forceTwoBodyAcceleration([1, 2], VELOCITY_KM_S), TypeError);
});
