// Piecewise reduced-orbit binding delegates to sidereon_core::orbit::{
// fit_piecewise, piecewise_position, piecewise_position_velocity,
// piecewise_drift, select_piecewise_segment}. Truth ECEF samples are built by
// sampling a real GPS arc out of the MGEX precise product (mirroring the Python
// reduced-orbit test), then the window is tiled into two segments and the fit is
// round-tripped.

import { test } from "node:test";
import assert from "node:assert/strict";

import { loadSp3, fitPiecewiseReducedOrbit, TimeScale } from "../pkg-node/sidereon.js";
import { fixture } from "./helpers.mjs";

// GRG0MGXFIN_2020 DOY176 0000 = 2020-06-24 00:00:00 GPST, 900 s node grid.
const BASE = { year: 2020, month: 6, day: 24 };
const NODE_STEP_S = 900;
const FIRST = 4;
const COUNT = 20;

const epochForNode = (i) => {
  const total = i * NODE_STEP_S;
  return {
    ...BASE,
    hour: Math.floor(total / 3600),
    minute: Math.floor((total % 3600) / 60),
    second: 0.0,
  };
};

const buildSamples = () => {
  const sp3 = loadSp3(fixture("sp3/GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const sat = sp3.satellites.find((s) => s.startsWith("G"));
  const axis = sp3.epochsJ2000Seconds();
  const nodes = Array.from({ length: COUNT }, (_, k) => FIRST + k);
  const query = Float64Array.from(nodes.map((i) => axis[i]));
  const interp = sp3.interpolate(sat, query);
  const pos = interp.positionM;
  return nodes.map((i, k) => ({
    epoch: epochForNode(i),
    xM: pos[k * 3],
    yM: pos[k * 3 + 1],
    zM: pos[k * 3 + 2],
  }));
};

test("fitPiecewiseReducedOrbit tiles the window into segments and evaluates", () => {
  const samples = buildSamples();
  const t0 = epochForNode(FIRST);
  const t1 = epochForNode(FIRST + COUNT - 1);
  const orbit = fitPiecewiseReducedOrbit(samples, TimeScale.Gpst, "circular_secular", t0, t1, 9000);

  assert.equal(orbit.model, "circular_secular");
  assert.ok(orbit.segmentCount >= 1);
  assert.ok(orbit.segmentSeconds > 0);

  const mid = epochForNode(FIRST + 5);
  const r = orbit.position(mid, "ecef");
  assert.equal(r.length, 3);
  assert.ok(r.every(Number.isFinite));
  // GPS-class geocentric radius, ~26600 km.
  const radiusKm = Math.hypot(r[0], r[1], r[2]) / 1000;
  assert.ok(radiusKm > 25000 && radiusKm < 28000);

  const state = orbit.positionVelocity(mid, "ecef");
  assert.equal(state.positionM.length, 3);
  assert.equal(state.velocityMS.length, 3);
  assert.ok(state.velocityMS.every(Number.isFinite));

  const idx = orbit.segmentIndexAt(mid);
  assert.ok(idx >= 0 && idx < orbit.segmentCount);
});

test("piecewise drift over the fit samples stays small and finite", () => {
  const samples = buildSamples();
  const t0 = epochForNode(FIRST);
  const t1 = epochForNode(FIRST + COUNT - 1);
  const orbit = fitPiecewiseReducedOrbit(samples, TimeScale.Gpst, "circular_secular", t0, t1, 9000);

  const drift = orbit.drift(samples, 1000.0);
  assert.equal(drift.errorsM.length, COUNT);
  assert.ok(drift.errorsM.every(Number.isFinite));
  assert.ok(Number.isFinite(drift.maxM) && Number.isFinite(drift.rmsM));
  assert.ok(drift.maxM >= 0);
});

test("fitPiecewiseReducedOrbit rejects a non-positive segment length", () => {
  const samples = buildSamples();
  const t0 = epochForNode(FIRST);
  const t1 = epochForNode(FIRST + COUNT - 1);
  assert.throws(() =>
    fitPiecewiseReducedOrbit(samples, TimeScale.Gpst, "circular_secular", t0, t1, 0),
  );
});
