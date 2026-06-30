// Astro Doppler binding delegates to sidereon_core::astro::doppler. The probe
// state and the frozen result are the core frozen_bits_regression goldens in
// astro/doppler.rs (UTC 2018-07-04 00:00:00, 437 MHz carrier); wasm libm may
// differ from the native libm the golden was emitted with by a few ULP on these
// frame-transform outputs, so the absolute values are matched to a tight
// relative tolerance and the internal dopplerHz = ratio * frequency identity is
// matched bit-for-bit.

import { test } from "node:test";
import assert from "node:assert/strict";

import { dopplerRangeRate, dopplerShift } from "../pkg-node/sidereon.js";
import { f64Bits } from "./helpers.mjs";

const POSITION_KM = Float64Array.from([
  3700.21121120399539, 2015.91221812060553, 5309.513078070447591,
]);
const VELOCITY_KM_S = Float64Array.from([
  -3.398428894395407, 6.869656830559572, -0.239850181126689,
]);
const LAT = 40.0;
const LON = -74.0;
const ALT = 0.0;
const FREQ = 437.0e6;

// Frozen bits from astro/doppler.rs::frozen_bits_regression (second = 0).
const EXPECTED_RANGE_RATE = 2.11937962917790934e-1;
const EXPECTED_RATIO = -7.06948948388124429e-7;

test("dopplerRangeRate matches the frozen range rate and ratio", () => {
  const [rangeRate, ratio] = dopplerRangeRate(
    POSITION_KM,
    VELOCITY_KM_S,
    LAT,
    LON,
    ALT,
    2018,
    7,
    4,
    0,
    0,
    0.0,
  );
  assert.ok(Math.abs(rangeRate - EXPECTED_RANGE_RATE) < 1e-9);
  assert.ok(Math.abs(ratio - EXPECTED_RATIO) < 1e-15);
});

test("dopplerShift returns a consistent shift, ratio, and range rate", () => {
  const shift = dopplerShift(
    POSITION_KM,
    VELOCITY_KM_S,
    LAT,
    LON,
    ALT,
    2018,
    7,
    4,
    0,
    0,
    0.0,
    FREQ,
  );
  assert.ok(Math.abs(shift.rangeRateKmS - EXPECTED_RANGE_RATE) < 1e-9);
  assert.ok(Math.abs(shift.dopplerRatio - EXPECTED_RATIO) < 1e-15);
  // The carrier shift is exactly ratio * frequency, bit-for-bit.
  assert.equal(f64Bits(shift.dopplerHz), f64Bits(shift.dopplerRatio * FREQ));
});

test("dopplerRangeRate rejects a wrong-length state vector", () => {
  assert.throws(() =>
    dopplerRangeRate(
      Float64Array.from([1, 2]),
      VELOCITY_KM_S,
      LAT,
      LON,
      ALT,
      2018,
      7,
      4,
      0,
      0,
      0.0,
    ),
  );
});
