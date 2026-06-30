// Geoid undulation over sidereon_core::geoid. The free functions resolve against
// the built-in coarse grid; GeoidGrid wraps a caller-supplied grid with bilinear
// interpolation. A tiny 2x2 grid gives exact corner and midpoint values.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  geoidUndulation,
  orthometricHeightM,
  ellipsoidalHeightM,
  GeoidGrid,
} from "../pkg-node/sidereon.js";

const DEG = Math.PI / 180;

test("geoidUndulation returns a finite metre value from the built-in grid", () => {
  assert.ok(Number.isFinite(geoidUndulation(45 * DEG, 10 * DEG)));
});

test("orthometric and ellipsoidal height conversions are exact inverses", () => {
  const lat = 45 * DEG;
  const lon = 10 * DEG;
  const h = 100.0;
  const H = orthometricHeightM(h, lat, lon);
  // N cancels exactly, so the round trip is bit-exact.
  assert.equal(ellipsoidalHeightM(H, lat, lon), h);
  assert.equal(H, h - geoidUndulation(lat, lon));
});

test("GeoidGrid interpolates corners exactly and the midpoint as the mean", () => {
  // 2x2 grid: origin (-10, 20), 10-degree spacing, row-major samples (lat outer,
  // lon inner): [(-10,20)=1, (-10,30)=2, (0,20)=3, (0,30)=4].
  const grid = new GeoidGrid(-10, 20, 10, 10, 2, 2, Float64Array.from([1, 2, 3, 4]));
  assert.equal(grid.undulationDeg(-10, 20), 1);
  assert.equal(grid.undulationDeg(0, 30), 4);
  assert.equal(grid.undulationDeg(-5, 25), 2.5);
  // undulationRad agrees with undulationDeg at the same position.
  assert.equal(grid.undulationRad(-5 * DEG, 25 * DEG), grid.undulationDeg(-5, 25));
});

test("GeoidGrid.fromText parses the documented format identically", () => {
  const text = "# header then samples\n-10 20 10 10 2 2\n1 2\n3 4\n";
  const grid = GeoidGrid.fromText(text);
  assert.equal(grid.undulationDeg(-5, 25), 2.5);
});

test("GeoidGrid.new rejects a sample count that does not match the dimensions", () => {
  assert.throws(() => new GeoidGrid(-10, 20, 10, 10, 2, 2, Float64Array.from([1, 2, 3])));
});
