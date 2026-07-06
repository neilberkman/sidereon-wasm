import { test } from "node:test";
import assert from "node:assert/strict";

import {
  TrackFilter,
  TrackFilterConfig,
  TrackRtsHistoryBuilder,
  SmoothedTrack,
  propagateState,
  smoothTrackRts,
} from "../pkg-node/sidereon.js";

const matrixEntryCount = (matrix) => matrix.reduce((sum, row) => sum + row.length, 0);
const finiteRows = (rows) => rows.flat().every(Number.isFinite);

test("019 track filter gated update suppresses a position spike", () => {
  const config = new TrackFilterConfig({
    frame: "callerDefinedCartesian",
    initialTS: 0,
    initialPositionM: [0],
    initialVelocityMS: [1],
    initialCovariance: [
      [1, 0],
      [0, 1],
    ],
    accelerationVarianceSpectralDensityM2S3: 0.1,
  });
  const filter = new TrackFilter(config);
  const history = TrackRtsHistoryBuilder.fromFilter(filter);

  const prediction = filter.predictRecorded(1, history);
  const predictedPosition = prediction.predicted.positionM.slice();
  const predictedCovariance = prediction.predicted.covariance.map((row) => row.slice());

  const spike = { positionM: [100], covarianceM2: [[0.01]] };
  const innovation = filter.positionInnovation(spike);
  assert.ok(innovation.nis > 0);

  const gated = filter.updatePositionGatedRecorded({ ...spike, confidence: 0.95 }, history);
  assert.equal(gated.gate.inGate, false);
  assert.equal(gated.update, null);
  assert.deepEqual(gated.state.positionM, predictedPosition);
  assert.deepEqual(gated.state.covariance, predictedCovariance);
  assert.deepEqual(filter.state.positionM, predictedPosition);

  const recorded = history.finish();
  assert.equal(recorded.epochCount, recorded.epochs.length);
  assert.deepEqual(recorded.epochs.at(-1).predicted.positionM, predictedPosition);
  assert.deepEqual(recorded.epochs.at(-1).updated.positionM, predictedPosition);

  const smoothed = smoothTrackRts(recorded);
  assert.ok(smoothed instanceof SmoothedTrack);
  assert.equal(smoothed.epochCount, recorded.epochCount);
  assert.deepEqual(smoothed.epochs.at(-1).state.positionM, predictedPosition);
});

test("019 track filter records a fix plus covariance and smooths the arc", () => {
  const filter = TrackFilter.fromPosition({
    frame: "ecef",
    initialTS: 0,
    initialPositionM: [0, 0, 0],
    positionCovarianceM2: [
      [1, 0, 0],
      [0, 1, 0],
      [0, 0, 1],
    ],
    initialVelocityVarianceM2S2: 25,
    accelerationVarianceSpectralDensityM2S3: 0.05,
  });
  const history = TrackRtsHistoryBuilder.fromFilter(filter);

  filter.predictRecorded(1, history);
  const update = filter.updatePositionRecorded(
    {
      positionM: [1, 0, 0],
      covarianceM2: [
        [0.25, 0, 0],
        [0, 0.25, 0],
        [0, 0, 0.25],
      ],
    },
    history,
  );

  assert.equal(update.updated.frame, "ecef");
  assert.ok(update.innovation.nis >= 0);
  assert.equal(
    matrixEntryCount(update.kalmanGain),
    update.updated.stateDimension * update.innovation.innovation.length,
  );
  assert.ok(update.updated.positionM[0] > update.predicted.positionM[0]);

  const recorded = history.finish();
  const smoothed = smoothTrackRts(recorded);
  assert.equal(smoothed.epochCount, recorded.epochCount);
  assert.equal(
    matrixEntryCount(smoothed.epochs[0].rtsGainToNext),
    update.updated.stateDimension ** 2,
  );
  assert.equal(smoothed.epochs.at(-1).rtsGainToNext, null);
  assert.ok(finiteRows(smoothed.epochs[0].state.covariance));
});

test("019 force model accepts solid Earth tide components", () => {
  const ephemeris = propagateState({
    epochS: 0,
    positionKm: [7078, -30, 820],
    velocityKmS: [0.2, 7.35, 1.05],
    timesS: [0, 60],
    integrator: "rk4",
    initialStepS: 30,
    maxStepS: 30,
    forceModel: {
      kind: "composite",
      twoBody: true,
      solidEarthTide: true,
      solidEarthPoleTide: true,
      thirdBody: false,
      solarRadiationPressure: false,
      relativity: false,
    },
  });

  assert.equal(ephemeris.timesS.length, ephemeris.epochCount);
  assert.equal(ephemeris.positionKm.length, ephemeris.velocityKmS.length);
  assert.equal(ephemeris.states.length, ephemeris.positionKm.length + ephemeris.velocityKmS.length);
  assert.ok(Array.from(ephemeris.states).every(Number.isFinite));
});
