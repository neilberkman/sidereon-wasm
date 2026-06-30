// Full three-dimensional NeQuick-G slant model bindings delegate to
// sidereon_core::atmosphere::ionosphere::{nequick_g_stec_tecu, nequick_g_delay_m}.
// These integrate the NeQuick 2 electron-density profiler along the ray (distinct
// from the compact broadcast-driven galileoNequickDelay), returning slant TEC in
// TECU and the dispersive group delay in positive metres.

import { test } from "node:test";
import assert from "node:assert/strict";

import { nequickGStecTecu, nequickGDelayM } from "../pkg-node/sidereon.js";

const L1_HZ = 1575.42e6;

// A mid-latitude receiver looking at a GNSS-altitude satellite, June noon UTC.
const RAY = {
  ai0: 0,
  ai1: 0,
  ai2: 0,
  month: 6,
  utcHours: 12,
  stationLonDeg: -100.0,
  stationLatDeg: 40.0,
  stationHeightM: 0.0,
  satelliteLonDeg: -100.0,
  satelliteLatDeg: 50.0,
  satelliteHeightM: 20_200_000.0,
};

test("nequickGStecTecu returns a positive finite slant TEC", () => {
  const stec = nequickGStecTecu(RAY);
  assert.ok(Number.isFinite(stec));
  assert.ok(stec > 0.0);
});

test("nequickGDelayM returns a positive dispersive delay consistent with the STEC", () => {
  const stec = nequickGStecTecu(RAY);
  const delay = nequickGDelayM(RAY, L1_HZ);
  assert.ok(Number.isFinite(delay));
  assert.ok(delay > 0.0);
  // The delay is the slant TEC mapped by the dispersive 40.3e16 / f^2 relation.
  const expected = stec * (40.3e16 / (L1_HZ * L1_HZ));
  assert.ok(Math.abs(delay - expected) <= 1e-9, `${delay} vs ${expected}`);
});

test("nequickGDelayM scales dispersively below L1", () => {
  const l1 = nequickGDelayM(RAY, L1_HZ);
  const l5 = nequickGDelayM(RAY, 1176.45e6);
  assert.ok(l5 > l1);
});

test("nequickGStecTecu responds to the broadcast ionisation coefficients", () => {
  const base = nequickGStecTecu(RAY);
  const hot = nequickGStecTecu({ ...RAY, ai0: 200 });
  assert.ok(hot > base);
});

test("nequickGStecTecu rejects an out-of-range month", () => {
  assert.throws(() => nequickGStecTecu({ ...RAY, month: 13 }));
});

test("nequickGDelayM rejects a non-positive frequency", () => {
  assert.throws(() => nequickGDelayM(RAY, 0));
});
