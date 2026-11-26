// Standalone force-model accelerations reproduce the core force-model bits.

import { test } from "node:test";
import assert from "node:assert/strict";

import { forceTwoBodyAcceleration, forceJ2Acceleration } from "../pkg-node/sidereon.js";
import { f64Bits, fixtureJson } from "./helpers.mjs";

const expected = fixtureJson("astrodynamics.json").gravityAccelerations;

// The shared probe state, kilometres and km/s, from the core force goldens.
const POSITION_KM = [7000.0, -1210.0, 1300.0];
const VELOCITY_KM_S = [0.0, 0.0, 0.0];

test("two-body acceleration matches the core force-model bits", () => {
  const a = forceTwoBodyAcceleration(POSITION_KM, VELOCITY_KM_S);
  assert.deepEqual(Array.from(a, f64Bits), expected.twoBody.map(f64Bits));
});

test("J2 acceleration matches the core force-model bits", () => {
  const a = forceJ2Acceleration(POSITION_KM, VELOCITY_KM_S);
  assert.deepEqual(Array.from(a, f64Bits), expected.j2.map(f64Bits));
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
