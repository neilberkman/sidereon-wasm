import { test } from "node:test";
import assert from "node:assert/strict";

import {
  Tle,
  fitPiecewiseReducedOrbitSp3,
  fitPiecewiseReducedOrbitTle,
  fitReducedOrbitSp3,
  fitReducedOrbitTle,
  loadSp3,
} from "../pkg-node/sidereon.js";
import { fixture } from "./helpers.mjs";

const BASE = { year: 2020, month: 6, day: 24 };
const NODE_STEP_S = 900;
const FIRST = 4;
const COUNT = 12;

const epochForNode = (i) => {
  const total = i * NODE_STEP_S;
  return {
    ...BASE,
    hour: Math.floor(total / 3600),
    minute: Math.floor((total % 3600) / 60),
    second: 0.0,
  };
};

const sp3Source = () => {
  const sp3 = loadSp3(fixture("sp3/GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  return { sp3, satellite: sp3.satellites.find((s) => s.startsWith("G")) };
};

const sp3Options = () => ({
  t0: epochForNode(FIRST),
  t1: epochForNode(FIRST + COUNT - 1),
  cadenceS: NODE_STEP_S,
  model: "circular_secular",
});

const sp3DriftOptions = () => ({
  t0: epochForNode(FIRST),
  t1: epochForNode(FIRST + COUNT - 1),
  cadenceS: NODE_STEP_S,
  thresholdM: 1.0e8,
});

const TLE_L1 = "1 25544U 98067A   18184.80969102  .00001614  00000-0  31745-4 0  9993";
const TLE_L2 = "2 25544  51.6414 295.8524 0003435 262.6267 204.2868 15.54005638121106";

const tleWindow = {
  t0: { year: 2018, month: 7, day: 3, hour: 20, minute: 0, second: 0.0 },
  t1: { year: 2018, month: 7, day: 3, hour: 21, minute: 0, second: 0.0 },
  cadenceS: 600,
};

const finiteDrift = (drift, requested) => {
  assert.equal(drift.requestedSamples, requested);
  assert.equal(drift.usedSamples, requested);
  assert.equal(drift.errorsM.length, requested);
  assert.ok(drift.errorsM.every(Number.isFinite));
  assert.ok(Number.isFinite(drift.maxM));
  assert.ok(Number.isFinite(drift.rmsM));
};

test("source-backed reduced-orbit fit and drift sample SP3 and TLE sources", () => {
  const { sp3, satellite } = sp3Source();
  const fit = fitReducedOrbitSp3(sp3, satellite, sp3Options());
  assert.equal(fit.requestedSamples, COUNT);
  assert.equal(fit.usedSamples, COUNT);
  assert.equal(fit.orbit.model, "circular_secular");
  finiteDrift(fit.orbit.driftSp3(sp3, satellite, sp3DriftOptions()), COUNT);

  const piecewiseFit = fitPiecewiseReducedOrbitSp3(sp3, satellite, {
    ...sp3Options(),
    segmentSeconds: 5400,
  });
  assert.equal(piecewiseFit.requestedSamples, COUNT);
  assert.equal(piecewiseFit.orbit.model, "circular_secular");
  assert.ok(piecewiseFit.orbit.segmentCount >= 1);
  finiteDrift(piecewiseFit.orbit.driftSp3(sp3, satellite, sp3DriftOptions()), COUNT);

  const tle = new Tle(TLE_L1, TLE_L2, "afspc");
  const tleFit = fitReducedOrbitTle(tle, { ...tleWindow, model: "circular_secular" });
  assert.equal(tleFit.requestedSamples, 7);
  assert.ok(tleFit.usedSamples >= 4);
  finiteDrift(tleFit.orbit.driftTle(tle, { ...tleWindow, thresholdM: 1.0e9 }), 7);

  const tlePiecewiseFit = fitPiecewiseReducedOrbitTle(tle, {
    ...tleWindow,
    model: "circular_secular",
    segmentSeconds: 1800,
  });
  assert.equal(tlePiecewiseFit.requestedSamples, 7);
  assert.ok(tlePiecewiseFit.orbit.segmentCount >= 1);
  finiteDrift(tlePiecewiseFit.orbit.driftTle(tle, { ...tleWindow, thresholdM: 1.0e9 }), 7);
});
