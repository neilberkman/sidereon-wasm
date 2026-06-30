// EGM96 geoid accessors over sidereon_core::geoid. The embedded genuine EGM96
// 1-degree grid is the recommended metre-class default; the height converters
// are exact inverses through its undulation.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  egm96Undulation,
  egm96OrthometricHeightM,
  egm96EllipsoidalHeightM,
  geoidUndulation,
} from "../pkg-node/sidereon.js";

const DEG = Math.PI / 180;
const lat = 45 * DEG;
const lon = 10 * DEG;

test("egm96Undulation returns a finite metre value", () => {
  const n = egm96Undulation(lat, lon);
  assert.ok(Number.isFinite(n));
  // Global geoid undulation stays within roughly +/- 110 m.
  assert.ok(Math.abs(n) < 120);
});

test("egm96 orthometric/ellipsoidal conversions are exact inverses", () => {
  const h = 250.0;
  const H = egm96OrthometricHeightM(h, lat, lon);
  assert.equal(H, h - egm96Undulation(lat, lon));
  assert.equal(egm96EllipsoidalHeightM(H, lat, lon), h);
});

test("egm96 grid is the genuine model, distinct from the coarse built-in", () => {
  // The 1-degree EGM96 lookup differs from the coarse 30-degree fallback.
  assert.notEqual(egm96Undulation(lat, lon), geoidUndulation(lat, lon));
});
