// Exact-DOP bindings delegate to sidereon_core::geometry::{dop_at_epoch,
// dop_series}. They are cross-checked against the already-pinned gnssDopSeries
// (arbitrary-grid) path over the same SP3 product and epochs, so the direct
// single-epoch and uniform-window delegations must agree bit-for-bit with it.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  gnssDopSeries,
  gnssDopAtEpoch,
  gnssDopSeriesWindow,
  loadSp3,
} from "../pkg-node/sidereon.js";
import { fixture, fixtureJson, hexToF64, f64Bits } from "./helpers.mjs";

const STEP = 300.0;
const COUNT = 13;
const OPTIONS = { elevationMaskDeg: 5.0, systems: ["G"], weighting: "unit" };

const setup = () => {
  const sp3 = loadSp3(fixture("sp3/GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const trace = fixtureJson("spp_trace_L2_tropo.json");
  const truth = trace.fixture.final_solution.truth_x.slice(0, 3).map(hexToF64);
  const station = Float64Array.from(truth);
  const t0 = (2459024.5 - 2451545.0) * 86400.0 + 0.5 * 86400.0;
  return { sp3, station, t0 };
};

test("gnssDopAtEpoch matches the first gnssDopSeries sample", () => {
  const { sp3, station, t0 } = setup();
  const epochs = Float64Array.from({ length: COUNT }, (_, i) => t0 + i * STEP);
  const series = gnssDopSeries(sp3, station, epochs, OPTIONS);
  const at = gnssDopAtEpoch(sp3, station, t0, OPTIONS);

  for (const attr of ["gdop", "pdop", "hdop", "vdop", "tdop"]) {
    assert.equal(f64Bits(at[attr]), f64Bits(series[attr][0]));
  }
  assert.deepEqual(at.satellites, series.satellitesAt(0));
});

test("gnssDopSeriesWindow matches gnssDopSeries over the same uniform grid", () => {
  const { sp3, station, t0 } = setup();
  const epochs = Float64Array.from({ length: COUNT }, (_, i) => t0 + i * STEP);
  const series = gnssDopSeries(sp3, station, epochs, OPTIONS);
  const window = gnssDopSeriesWindow(sp3, station, t0, t0 + (COUNT - 1) * STEP, STEP, OPTIONS);

  assert.equal(window.length, series.epochCount);
  window.forEach((sample, i) => {
    assert.equal(sample.stepIndex, series.stepIndex[i]);
    for (const attr of ["gdop", "pdop", "hdop", "vdop", "tdop"]) {
      assert.equal(f64Bits(sample[attr]), f64Bits(series[attr][i]));
    }
    assert.deepEqual(sample.satellites, series.satellitesAt(i));
  });
});

test("gnssDopSeriesWindow rejects a non-integer step", () => {
  const { sp3, station, t0 } = setup();
  assert.throws(() => gnssDopSeriesWindow(sp3, station, t0, t0 + 600, 300.5, OPTIONS));
});
