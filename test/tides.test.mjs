// Station tidal-displacement bindings delegate to sidereon_core::tides. Each
// returns a geocentric ITRF displacement (metres); the assertions check the
// physical scale (sub-metre), the zero-coefficient ocean-loading degenerate
// case, and the input-shape rejections.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  solidEarthTide,
  oceanTideLoading,
  solidEarthPoleTide,
} from "../pkg-node/sidereon.js";

// A mid-latitude ITRF station and coarse geocentric Sun/Moon directions (m).
const STATION = Float64Array.from([4517590.0, 837270.0, 4527420.0]);
const SUN = Float64Array.from([1.4e11, 0.4e11, 0.2e11]);
const MOON = Float64Array.from([3.0e8, 1.5e8, 1.0e8]);
const FHR = 12.0;

const mag = (v) => Math.hypot(v[0], v[1], v[2]);

test("solidEarthTide returns a finite sub-metre displacement", () => {
  const d = solidEarthTide(STATION, 2020, 6, 24, FHR, SUN, MOON);
  assert.equal(d.length, 3);
  assert.ok(d.every(Number.isFinite));
  assert.ok(mag(d) < 1.0);
});

test("oceanTideLoading with zero BLQ coefficients yields no displacement", () => {
  const zeros = new Float64Array(33);
  const d = oceanTideLoading(STATION, 2020, 6, 24, FHR, zeros, zeros);
  assert.equal(d.length, 3);
  assert.ok(mag(d) < 1e-12);
});

test("oceanTideLoading rejects a wrong-length BLQ grid", () => {
  const zeros = new Float64Array(33);
  assert.throws(() => oceanTideLoading(STATION, 2020, 6, 24, FHR, new Float64Array(10), zeros));
});

test("solidEarthPoleTide returns a finite sub-metre displacement", () => {
  const d = solidEarthPoleTide(STATION, 2020, 6, 24, FHR, 0.1, 0.3);
  assert.equal(d.length, 3);
  assert.ok(d.every(Number.isFinite));
  assert.ok(mag(d) < 1.0);
});
