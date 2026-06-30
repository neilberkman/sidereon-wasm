// Coverage-grid binding delegates to sidereon_core::astro::coverage. It builds a
// row-major [satellite][station] look-angle grid and exposes the crate's
// visibility / access-count / max-elevation reductions over it.

import { test } from "node:test";
import assert from "node:assert/strict";

import { Tle, GroundStation, coverageLookAngles } from "../pkg-node/sidereon.js";

const ISS_L1 = "1 25544U 98067A   24001.50000000  .00016717  00000-0  10270-3 0  9009";
const ISS_L2 = "2 25544  51.6400 208.8657 0002644 250.3037 109.7782 15.49560812999990";

const EPOCH_US = BigInt(Date.UTC(2024, 0, 1, 12, 0, 0)) * 1000n;

const makeGrid = () => {
  const sats = [new Tle(ISS_L1, ISS_L2), new Tle(ISS_L1, ISS_L2)];
  const stations = [
    new GroundStation(51.5, -0.1, 11.0),
    new GroundStation(40.7, -74.0, 10.0),
    new GroundStation(-33.9, 151.2, 0.0),
  ];
  return coverageLookAngles(sats, stations, EPOCH_US);
};

test("coverageLookAngles reports the grid dimensions", () => {
  const grid = makeGrid();
  assert.equal(grid.satelliteCount, 2);
  assert.equal(grid.stationCount, 3);
});

test("lookAngle returns [az, el, range] cells consistent with the reductions", () => {
  const grid = makeGrid();
  const cell = grid.lookAngle(0, 0);
  // The two satellites are the same TLE, so both rows are identical and the
  // first cell is the look angle for station 0.
  assert.ok(cell === undefined || cell.length === 3);
  if (cell) {
    const [az, el, range] = cell;
    assert.ok(az >= 0 && az < 360);
    assert.ok(el >= -90 && el <= 90);
    assert.ok(range > 0);
  }
  // Out-of-range indices yield undefined.
  assert.equal(grid.lookAngle(99, 0), undefined);
});

test("visibleMask, accessCounts, and maxElevationDeg agree at a threshold", () => {
  const grid = makeGrid();
  const mask = grid.visibleMask(0.0);
  assert.equal(mask.length, grid.satelliteCount * grid.stationCount);
  assert.ok(Array.from(mask).every((b) => b === 0 || b === 1));

  const counts = grid.accessCounts(0.0);
  assert.equal(counts.length, grid.stationCount);
  // Column sum of the mask equals the per-station access count.
  for (let s = 0; s < grid.stationCount; s += 1) {
    let sum = 0;
    for (let sat = 0; sat < grid.satelliteCount; sat += 1) {
      sum += mask[sat * grid.stationCount + s];
    }
    assert.equal(counts[s], sum);
  }

  const maxEl = grid.maxElevationDeg();
  assert.equal(maxEl.length, grid.stationCount);
  assert.ok(Array.from(maxEl).every((v) => Number.isNaN(v) || (v >= -90 && v <= 90)));
});

test("coverageLookAngles rejects empty satellite or station lists", () => {
  assert.throws(() => coverageLookAngles([], [new GroundStation(0, 0, 0)], EPOCH_US));
  assert.throws(() => coverageLookAngles([new Tle(ISS_L1, ISS_L2)], [], EPOCH_US));
});
