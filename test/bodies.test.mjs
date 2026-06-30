// Analytic Sun/Moon ephemerides reproduce the engine fixture bits, against the
// bodies block of sp3_bodies.json (the same golden the Python binding asserts).

import { test } from "node:test";
import assert from "node:assert/strict";

import { sunMoonEci, sunMoonEcef } from "../pkg-node/sidereon.js";
import { fixtureJson, f64Bits, bigints } from "./helpers.mjs";

const FX = fixtureJson("sp3_bodies.json");
const eqBits = (value, hex) => assert.equal(f64Bits(value), BigInt(hex));
const epochs = () => bigints(FX.bodies.map((b) => b.unix_micros));

test("sun/moon ECI match reference bits", () => {
  const r = sunMoonEci(epochs());
  assert.equal(r.epochCount, FX.bodies.length);
  assert.equal(r.frame, "eci");
  assert.equal(r.sun.length, FX.bodies.length * 3);
  FX.bodies.forEach((b, i) => {
    for (let axis = 0; axis < 3; axis++) {
      eqBits(r.sun[i * 3 + axis], b.sun_eci_m_hex[axis]);
      eqBits(r.moon[i * 3 + axis], b.moon_eci_m_hex[axis]);
    }
  });
});

test("sun/moon ECEF match reference bits", () => {
  const r = sunMoonEcef(epochs());
  assert.equal(r.frame, "ecef");
  FX.bodies.forEach((b, i) => {
    for (let axis = 0; axis < 3; axis++) {
      eqBits(r.sun[i * 3 + axis], b.sun_ecef_m_hex[axis]);
      eqBits(r.moon[i * 3 + axis], b.moon_ecef_m_hex[axis]);
    }
  });
});

test("single-epoch sun/moon equals batch element", () => {
  const batch = sunMoonEci(epochs());
  FX.bodies.forEach((b, i) => {
    const one = sunMoonEci(bigints([b.unix_micros]));
    for (let axis = 0; axis < 3; axis++) {
      assert.equal(f64Bits(one.sun[axis]), f64Bits(batch.sun[i * 3 + axis]));
      assert.equal(f64Bits(one.moon[axis]), f64Bits(batch.moon[i * 3 + axis]));
    }
  });
});

test("empty epochs throw", () => {
  assert.throws(() => sunMoonEci(new BigInt64Array(0)));
});
