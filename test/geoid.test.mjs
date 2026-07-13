// Geoid undulation over sidereon_core::geoid. The free functions resolve against
// the built-in coarse grid; GeoidGrid wraps a caller-supplied grid with bilinear
// interpolation. A tiny 2x2 grid gives exact corner and midpoint values.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  geoidUndulation,
  geoidUndulationsDeg,
  geoidUndulationsRad,
  orthometricHeightM,
  ellipsoidalHeightM,
  GeoidGrid,
  ProjVgridshiftArithmetic,
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

test("geoid batch lookups and grid height conversions match core output", () => {
  const grid = new GeoidGrid(-10, 20, 10, 10, 2, 2, Float64Array.from([1, 2, 3, 4]));

  assert.deepEqual(
    Array.from(grid.undulationsDeg(Float64Array.from([-10, 20, -5, 25, 0, 30]))),
    [1, 2.5, 4],
  );
  assert.deepEqual(
    Array.from(grid.undulationsRad(Float64Array.from([-10 * DEG, 20 * DEG, -5 * DEG, 25 * DEG]))),
    [1, 2.5],
  );
  assert.equal(grid.orthometricHeightDeg(100, -5, 25), 97.5);
  assert.equal(grid.ellipsoidalHeightDeg(97.5, -5, 25), 100);

  assert.deepEqual(Array.from(geoidUndulationsDeg(Float64Array.from([0, 0]))), [
    geoidUndulation(0, 0),
  ]);
  assert.deepEqual(Array.from(geoidUndulationsRad(Float64Array.from([0, 0]))), [
    geoidUndulation(0, 0),
  ]);
});

test("GeoidGrid.fromText parses the documented format identically", () => {
  const text = "# header then samples\n-10 20 10 10 2 2\n1 2\n3 4\n";
  const grid = GeoidGrid.fromText(text);
  assert.equal(grid.undulationDeg(-5, 25), 2.5);
});

test("GeoidGrid.new rejects a sample count that does not match the dimensions", () => {
  assert.throws(() => new GeoidGrid(-10, 20, 10, 10, 2, 2, Float64Array.from([1, 2, 3])));
});

function projEgm96GtxFixture() {
  const rows = 721;
  const columns = 1440;
  const headerBytes = 40;
  const bytes = new Uint8Array(headerBytes + rows * columns * 4);
  const view = new DataView(bytes.buffer);
  view.setFloat64(0, -90, false);
  view.setFloat64(8, -180, false);
  view.setFloat64(16, 0.25, false);
  view.setFloat64(24, 0.25, false);
  view.setInt32(32, rows, false);
  view.setInt32(36, columns, false);
  view.setFloat32(headerBytes, 1, false);
  view.setFloat32(headerBytes + 4, 2, false);
  view.setFloat32(headerBytes + columns * 4, 3, false);
  view.setFloat32(headerBytes + (columns + 1) * 4, 4, false);
  return bytes;
}

test("PROJ EGM96 GTX loader requires explicit fused or separate arithmetic", () => {
  const bytes = projEgm96GtxFixture();
  assert.throws(() => GeoidGrid.fromProjEgm96Gtx(bytes.subarray(0, -1)), /must be .* bytes/);

  const grid = GeoidGrid.fromProjEgm96Gtx(bytes);
  const latitude = -89.875 * DEG;
  const longitude = -179.875 * DEG;
  const separate = grid.undulationProjRad(
    latitude,
    longitude,
    ProjVgridshiftArithmetic.SeparateMultiplyAdd,
  );
  const fused = grid.undulationProjRad(
    latitude,
    longitude,
    ProjVgridshiftArithmetic.FusedMultiplyAdd,
  );
  assert.ok(Math.abs(separate - 2.5) < 1e-12);
  assert.ok(Math.abs(fused - 2.5) < 1e-12);
});

test("PROJ vertical-grid coordinate failures are typed RangeErrors", () => {
  const grid = GeoidGrid.fromProjEgm96Gtx(projEgm96GtxFixture());

  assert.throws(
    () => grid.undulationProjRad(Number.NaN, 0, ProjVgridshiftArithmetic.SeparateMultiplyAdd),
    (error) => {
      assert.ok(error instanceof RangeError);
      assert.equal(error.name, "NonFiniteCoordinate");
      assert.equal(error.kind, "NonFiniteCoordinate");
      assert.equal(error.coordinate, "latitude");
      assert.deepEqual(error.detail, {
        name: "NonFiniteCoordinate",
        message: "PROJ vertical-grid latitude coordinate is not finite",
        coordinate: "latitude",
      });
      return true;
    },
  );

  assert.throws(
    () => grid.undulationProjRad(2, 0, ProjVgridshiftArithmetic.FusedMultiplyAdd),
    (error) => {
      assert.ok(error instanceof RangeError);
      assert.equal(error.name, "CoordinateOutsideGrid");
      assert.equal(error.kind, "CoordinateOutsideGrid");
      assert.equal(error.coordinate, "latitude");
      return true;
    },
  );

  assert.throws(
    () =>
      grid.undulationProjRad(
        0,
        Number.POSITIVE_INFINITY,
        ProjVgridshiftArithmetic.SeparateMultiplyAdd,
      ),
    (error) => {
      assert.ok(error instanceof RangeError);
      assert.equal(error.kind, "NonFiniteCoordinate");
      assert.equal(error.coordinate, "longitude");
      return true;
    },
  );

  const regional = new GeoidGrid(-10, 20, 10, 10, 2, 2, Float64Array.from([1, 2, 3, 4]));
  assert.throws(
    () => regional.undulationProjRad(-5 * DEG, 0, ProjVgridshiftArithmetic.SeparateMultiplyAdd),
    (error) => {
      assert.ok(error instanceof RangeError);
      assert.equal(error.kind, "CoordinateOutsideGrid");
      assert.equal(error.coordinate, "longitude");
      return true;
    },
  );
});
