// Covariance propagation and explicit STM transport through the WASM binding.
// The numbers below are core fixture/golden outputs for the same two-body arc.

import { test } from "node:test";
import assert from "node:assert/strict";

import { propagateCovariance, transportCovariance } from "../pkg-node/sidereon.js";

import { f64Bits, fixtureJson } from "./helpers.mjs";

const bits = (values) =>
  Array.from(values, (x) => `0x${f64Bits(x).toString(16).padStart(16, "0")}`);

const diagonal = (matrix) => [0, 1, 2, 3, 4, 5].map((i) => matrix[i * 6 + i]);

const covariance0 = Float64Array.from([
  1e-4, 0, 0, 0, 0, 0, 0, 2e-4, 0, 0, 0, 0, 0, 0, 3e-4, 0, 0, 0, 0, 0, 0, 1e-8, 0, 0, 0, 0, 0, 0,
  2e-8, 0, 0, 0, 0, 0, 0, 3e-8,
]);

const processNoise = {
  qRadialKm2S3: 1e-12,
  qTransverseKm2S3: 2e-12,
  qNormalKm2S3: 3e-12,
};

test("propagateCovariance matches the core two-body covariance golden", () => {
  const ephem = propagateCovariance({
    epochS: 0,
    positionKm: [7000, 0, 0],
    velocityKmS: [0, 7.546049108166282, 0],
    covariance: Array.from(covariance0),
    timesS: [0, 60, 120],
    covarianceFrame: "inertial",
    outputFrame: "inertial",
    processNoise,
    integrator: "rk4",
    initialStepS: 2,
    maxStepS: 2,
  });

  assert.equal(ephem.epochCount, 3);
  assert.equal(ephem.frame, "inertial");
  assert.deepEqual(Array.from(ephem.timesS), [0, 60, 120]);
  const expected = fixtureJson("numerical_models.json").covariance;
  assert.deepEqual(bits(ephem.positionKm.slice(6, 9)), expected.finalState.positionKm);
  assert.deepEqual(bits(ephem.velocityKmS.slice(6, 9)), expected.finalState.velocityKmS);
  assert.deepEqual(bits(diagonal(ephem.covariance.slice(72, 108))), expected.finalDiagonal);
  assert.deepEqual(bits(diagonal(ephem.covarianceAt(60))), expected.midDiagonal);
});

test("transportCovariance delegates STM transport and process noise to core", () => {
  const identity = Array.from({ length: 36 }, (_, i) => (i % 7 === 0 ? 1 : 0));
  const result = transportCovariance(
    covariance0,
    [
      {
        stateTransitionMatrix: identity,
        dtS: 60,
        qRotationEpochS: 0,
        qRotationPositionKm: [7000, 0, 0],
        qRotationVelocityKmS: [0, 7.546049108166282, 0],
      },
    ],
    { processNoise },
  );

  assert.equal(result.nodeCount, 2);
  assert.deepEqual(bits(diagonal(result.covariance.slice(36, 72))), [
    "0x3f1a3bb7de758e19",
    "0x3f2a3bb7de758e19",
    "0x3f33acc9e6d82a92",
    "0x3e459a8b2202c251",
    "0x3e559a8b2202c251",
    "0x3e6033e8598211bc",
  ]);
});
