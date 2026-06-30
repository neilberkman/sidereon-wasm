// Direct broadcast ionosphere bindings delegate to
// sidereon_core::atmosphere::ionosphere: klobucharDelay -> klobuchar_native and
// galileoNequickDelay -> ionosphere_delay with the Galileo NeQuick-G model. Both
// return a positive dispersive group delay in metres.

import { test } from "node:test";
import assert from "node:assert/strict";

import { klobucharDelay, galileoNequickDelay } from "../pkg-node/sidereon.js";

const L1_HZ = 1575.42e6;
// Representative broadcast Klobuchar coefficient set.
const ALPHA = Float64Array.from([1.0e-8, 0.0, -5.96e-8, 5.96e-8]);
const BETA = Float64Array.from([9.0e4, 0.0, -1.97e5, 0.0]);

test("klobucharDelay returns a positive finite L1 delay", () => {
  const delay = klobucharDelay(ALPHA, BETA, 40.0, -100.0, 0.0, 30.0, 50400.0, L1_HZ);
  assert.ok(Number.isFinite(delay));
  assert.ok(delay > 0.0);
});

test("klobucharDelay scales dispersively below L1", () => {
  // The same geometry on a lower carrier sees a larger group delay (~1/f^2).
  const l1 = klobucharDelay(ALPHA, BETA, 40.0, -100.0, 0.0, 30.0, 50400.0, L1_HZ);
  const l5 = klobucharDelay(ALPHA, BETA, 40.0, -100.0, 0.0, 30.0, 50400.0, 1176.45e6);
  assert.ok(l5 > l1);
});

test("klobucharDelay rejects a wrong-length coefficient row", () => {
  assert.throws(() =>
    klobucharDelay(Float64Array.from([1, 2, 3]), BETA, 40, -100, 0, 30, 50400, L1_HZ),
  );
});

test("galileoNequickDelay returns a positive finite delay (default coefficients)", () => {
  // A zero broadcast set selects the Galileo-recommended default ionisation.
  const delay = galileoNequickDelay(0, 0, 0, 40.0, -100.0, 0.0, 30.0, 2020, 6, 24, 12, 0, 0.0, L1_HZ);
  assert.ok(Number.isFinite(delay));
  assert.ok(delay > 0.0);
});

test("galileoNequickDelay responds to the broadcast coefficients", () => {
  const base = galileoNequickDelay(0, 0, 0, 40.0, -100.0, 0.0, 30.0, 2020, 6, 24, 12, 0, 0.0, L1_HZ);
  const hot = galileoNequickDelay(200, 0, 0, 40.0, -100.0, 0.0, 30.0, 2020, 6, 24, 12, 0, 0.0, L1_HZ);
  assert.ok(hot > base);
});
