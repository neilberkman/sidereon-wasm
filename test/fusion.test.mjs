// GNSS/INS fusion binding parity: stateful filter updates, time-sync replay,
// UKF selection, tight SP3 rows, and binary state codec round-trip.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  FusionRtsHistoryBuilder,
  GnssInsFilter,
  fusionStateBytesRoundTrip,
  loadSp3,
  smoothFusionRts,
  velocityMatchOutage,
} from "../pkg-node/sidereon.js";
import { fixture, f64Bits } from "./helpers.mjs";

const WGS84_A_M = 6378137.0;
const OMEGA_E = 7.2921151467e-5;

const eqBits = (value, hex) => assert.equal(f64Bits(value), BigInt(hex));

function baseConfig(filterKind = "ekf") {
  return {
    initialState: {
      tJ2000S: 0,
      positionEcefM: [WGS84_A_M, 0, 0],
      velocityEcefMps: [0, 0, 0],
      attitudeBodyToEcef: [1, 0, 0, 0, 1, 0, 0, 0, 1],
    },
    layout: "fifteen",
    covarianceDiagonal: Array(15).fill(1),
    imuSpec: {
      accelVrwMpsSqrtS: 0,
      gyroArwRadSqrtS: 0,
      accelBiasInstabMps2: 0,
      gyroBiasInstabRps: 0,
      accelBiasTauS: Infinity,
      gyroBiasTauS: Infinity,
    },
    filterKind,
    timeSync: { imuCapacity: 4, checkpointCapacity: 4 },
  };
}

function increment(tJ2000S, dtS) {
  return {
    kind: "increment",
    tJ2000S,
    deltaVelocityMps: [0.015625 * dtS, -0.0078125 * dtS, 0.00390625 * dtS],
    deltaThetaRad: [OMEGA_E * dtS, 0.0009765625 * dtS, -0.00048828125 * dtS],
    dtS,
  };
}

function looseFix(tJ2000S, positionEcefM) {
  return {
    tJ2000S,
    positionEcefM,
    covariance: [
      [4, 0, 0],
      [0, 4, 0],
      [0, 0, 4],
    ],
    satellitesUsed: 8,
  };
}

function loosePositionVelocityFix(tJ2000S = 0, fixStatus = "single") {
  return {
    tJ2000S,
    positionEcefM: [WGS84_A_M + 1, 2, -3],
    velocityEcefMps: [0.4, -0.2, 0.1],
    covariance: [
      [1, 0, 0, 0, 0, 0],
      [0, 1, 0, 0, 0, 0],
      [0, 0, 1, 0, 0, 0],
      [0, 0, 0, 1, 0, 0],
      [0, 0, 0, 0, 1, 0],
      [0, 0, 0, 0, 0, 1],
    ],
    satellitesUsed: 8,
    fixStatus,
  };
}

function zeroIncrement(tJ2000S, dtS) {
  return {
    kind: "increment",
    tJ2000S,
    deltaVelocityMps: [0, 0, 0],
    deltaThetaRad: [0, 0, 0],
    dtS,
  };
}

function diagonalBits(matrix, offset, n) {
  return Array.from({ length: n }, (_, i) => f64Bits(matrix[offset + i][offset + i]));
}

test("fusion time-sync replay and state bytes match reference bits", () => {
  const filter = new GnssInsFilter(baseConfig());
  filter.propagate(increment(1, 1));
  const update = filter.updateLooseTimeSync(looseFix(0.75, [WGS84_A_M + 0.125, -0.0625, 0.03125]));
  const state = filter.state();
  const encoded = filter.encodeState();
  const roundTrip = fusionStateBytesRoundTrip(encoded);
  const restored = GnssInsFilter.fromStateBytes(baseConfig(), encoded).state();

  assert.equal(update.lateMeasurement, true);
  assert.equal(update.replayedImuSegments, 2);
  assert.equal(update.restoredCheckpointEpochJ2000S, 0);
  assert.deepEqual(Buffer.from(encoded), Buffer.from(roundTrip));
  assert.ok(encoded.length > 0);

  eqBits(update.update.nis, "0x3FF76514A5737228");
  eqBits(state.positionEcefM[0], "0x415854A5451D0C2D");
  eqBits(state.positionEcefM[1], "0xBF98A921BD076F90");
  eqBits(state.covariance[0][0], "0x3FF9DCE1941815DA");
  eqBits(restored.positionEcefM[0], "0x415854A5451D0C2D");
});

test("fusion UKF option applies the same loose measurement surface", () => {
  const filter = new GnssInsFilter(baseConfig("ukf"));
  const update = filter.updateLoose(looseFix(0, [WGS84_A_M + 0.5, -0.25, 0.125]));

  assert.equal(update.applied, true);
  assert.equal(update.rows, 3);
  eqBits(update.nis, "0x3FB0CCCCCCCCCCCC");
});

test("fusion field-mode omission keeps the loose update bit-identical", () => {
  const defaultFilter = new GnssInsFilter(baseConfig());
  const omittedConfig = baseConfig();
  omittedConfig.loose = {};
  const omittedFilter = new GnssInsFilter(omittedConfig);

  const defaultUpdate = defaultFilter.updateLoose(loosePositionVelocityFix());
  const omittedUpdate = omittedFilter.updateLoose(loosePositionVelocityFix());
  const defaultState = defaultFilter.state();
  const omittedState = omittedFilter.state();

  assert.deepEqual(omittedUpdate, defaultUpdate);
  assert.deepEqual(omittedState, defaultState);
  eqBits(defaultUpdate.nis, "0x401C6B851EB851E9");
  assert.deepEqual(defaultState.positionEcefM.map(f64Bits), [
    0x415854a660000000n,
    0x3fefffffffffffffn,
    0xbff7ffffffffffffn,
  ]);
  assert.deepEqual(defaultState.velocityEcefMps.map(f64Bits), [
    0x3fc9999999999999n,
    0xbfb9999999999999n,
    0x3fa9999999999999n,
  ]);
  assert.deepEqual(diagonalBits(defaultState.covariance, 0, 6), Array(6).fill(0x3fdfffffffffffffn));
});

test("fusion stationary update applies ZUPT and ZARU field-mode constraints", () => {
  const config = baseConfig();
  config.loose = {
    stationaryUpdates: {
      detector: {
        windowLen: 1,
        maxSpecificForceNormErrorMps2: 100,
        maxBodyRateWrtEcefNormRps: 1,
      },
      zeroVelocitySigmaMps: 0.5,
      zeroAngularRateSigmaRps: 0.05,
    },
  };

  const filter = new GnssInsFilter(config);
  filter.propagate(zeroIncrement(1, 1));
  const update = filter.updateStationary();
  const state = filter.state();

  assert.equal(update.applied, true);
  assert.deepEqual([update.rows, update.acceptedRows, update.rejectedRows], [6, 6, 0]);
  eqBits(update.nis, "0x404541AF8E65B9FC");
  assert.deepEqual(state.velocityEcefMps.map(f64Bits), [
    0xbff16320edfcd4c0n,
    0xbde64ef6efbb7204n,
    0x0000000000000000n,
  ]);
  assert.deepEqual(state.gyroBiasRps.map(f64Bits), [
    0x0000000000000000n,
    0x0000000000000000n,
    0xbf131173b6b2c903n,
  ]);
  assert.deepEqual(diagonalBits(state.covariance, 3, 6), [
    0x3fcc71c76e2f216en,
    0x3fcc71c6f3ff694dn,
    0x3fcc71c6f3b73afdn,
    0x3ff00a36e71a6702n,
    0x3ff00a36e71a6702n,
    0x3ff00a36e71a2cb0n,
  ]);
  assert.equal(new GnssInsFilter(baseConfig()).updateStationary(), null);
});

test("fusion fix-status weighting scales covariance in core order", () => {
  const config = baseConfig();
  config.loose = {
    fixStatusWeighting: {
      singleSigmaMultiplier: 3,
      floatSigmaMultiplier: 2,
      fixedSigmaMultiplier: 1,
    },
  };

  const byStatus = Object.fromEntries(
    ["single", "float", "fixed"].map((status) => {
      const filter = new GnssInsFilter(config);
      const update = filter.updateLoose(loosePositionVelocityFix(0, status));
      return [status, { update, state: filter.state() }];
    }),
  );

  eqBits(byStatus.single.update.nis, "0x3FF6BC6A7EF9DB22");
  eqBits(byStatus.float.update.nis, "0x4006BC6A7EF9DB22");
  eqBits(byStatus.fixed.update.nis, "0x401C6B851EB851E9");
  assert.ok(
    byStatus.fixed.state.covariance[0][0] < byStatus.float.state.covariance[0][0] &&
      byStatus.float.state.covariance[0][0] < byStatus.single.state.covariance[0][0],
  );
  assert.deepEqual(diagonalBits(byStatus.fixed.state.covariance, 0, 6), [
    0x3fdfffffffffffffn,
    0x3fdfffffffffffffn,
    0x3fdfffffffffffffn,
    0x3fdfffffffffffffn,
    0x3fdfffffffffffffn,
    0x3fdfffffffffffffn,
  ]);
  assert.deepEqual(diagonalBits(byStatus.float.state.covariance, 0, 6), [
    0x3fe999999999999an,
    0x3fe999999999999an,
    0x3fe999999999999an,
    0x3fe999999999999an,
    0x3fe999999999999an,
    0x3fe999999999999an,
  ]);
  assert.deepEqual(diagonalBits(byStatus.single.state.covariance, 0, 6), [
    0x3feccccccccccccdn,
    0x3feccccccccccccdn,
    0x3feccccccccccccdn,
    0x3feccccccccccccdn,
    0x3feccccccccccccdn,
    0x3feccccccccccccdn,
  ]);
});

test("fusion velocity matching and non-holonomic field-mode bindings match core", () => {
  const matched = velocityMatchOutage(
    [
      { tJ2000S: 0, positionEcefM: [0, 0, 0], velocityEcefMps: [1, 0, 0] },
      { tJ2000S: 1, positionEcefM: [1, 0, 0], velocityEcefMps: [1, 0, 0] },
      { tJ2000S: 2, positionEcefM: [2, 0, 0], velocityEcefMps: [1, 0, 0] },
    ],
    {
      tJ2000S: 2,
      positionEcefM: [4, 1, 0],
      velocityEcefMps: [2, 0, 0],
      covariance: [
        [1, 0, 0, 0, 0, 0],
        [0, 1, 0, 0, 0, 0],
        [0, 0, 1, 0, 0, 0],
        [0, 0, 0, 1, 0, 0],
        [0, 0, 0, 0, 1, 0],
        [0, 0, 0, 0, 0, 1],
      ],
      satellitesUsed: 8,
    },
    { maxOutageDurationS: 5 },
  );

  assert.deepEqual(matched.endpointPositionCorrectionEcefM.map(f64Bits), [
    0x4000000000000000n,
    0x3ff0000000000000n,
    0x0000000000000000n,
  ]);
  assert.deepEqual(matched.endpointVelocityCorrectionEcefMps.map(f64Bits), [
    0x3ff0000000000000n,
    0x0000000000000000n,
    0x0000000000000000n,
  ]);
  assert.deepEqual(matched.states[1].positionEcefM.map(f64Bits), [
    0x3ffc000000000000n,
    0x3fe0000000000000n,
    0x0000000000000000n,
  ]);
  assert.deepEqual(matched.states[1].velocityEcefMps.map(f64Bits), [
    0x4002000000000000n,
    0x3fe8000000000000n,
    0x0000000000000000n,
  ]);

  const config = baseConfig();
  config.imuToBodyDcm = [0, -1, 0, 1, 0, 0, 0, 0, 1];
  config.initialState.velocityEcefMps = [2, 0.4, -0.2];
  config.loose = {
    nonHolonomic: {
      lateralVelocitySigmaMps: 0.5,
      verticalVelocitySigmaMps: 0.5,
      minSpeedMps: 0.1,
      maxBodyRateWrtEcefNormRps: 1,
    },
  };
  const filter = new GnssInsFilter(config);
  const update = filter.updateNonHolonomic();
  const state = filter.state();

  assert.equal(update.applied, true);
  assert.deepEqual([update.rows, update.acceptedRows, update.rejectedRows], [2, 2, 0]);
  eqBits(update.nis, "0x3FA3813813813814");
  assert.deepEqual(state.velocityEcefMps.map(f64Bits), [
    0x4000000000000000n,
    0x3fd4b94b94b94b95n,
    0xbfc4b94b94b94b95n,
  ]);
});

test("fusion robust loose recorded RTS smoothing matches reference bits", () => {
  const config = baseConfig();
  config.imuSpec = "mems";
  config.loose = {
    updateOptions: { innovationGate: { thresholdSigma: 4, minRows: 2 } },
    measurementReweighting: {},
    predictionAdaptation: {},
  };
  const filter = new GnssInsFilter(config);
  const history = FusionRtsHistoryBuilder.fromFilter(filter);
  filter.propagateRecorded(
    {
      kind: "rate",
      tJ2000S: 1,
      specificForceMps2: [0, 0, 0],
      angularRateRps: [0, 0, 0],
    },
    history,
  );
  const update = filter.updateLooseRecorded(
    {
      tJ2000S: 1,
      positionEcefM: [WGS84_A_M + 0.35, 0.2, -0.1],
      covariance: [
        [0.5, 0, 0],
        [0, 0.5, 0],
        [0, 0, 0.5],
      ],
      satellitesUsed: 7,
    },
    history,
  );
  const recorded = history.finish();
  const smoothed = smoothFusionRts(recorded);
  const recordedEpochs = recorded.epochs;
  const smoothedEpochs = smoothed.epochs;
  const state = filter.state();

  assert.equal(update.applied, true);
  assert.deepEqual([update.rows, update.acceptedRows, update.rejectedRows], [3, 3, 0]);
  assert.equal(update.ekf.innovationGate.maxRejectedAbsNormalizedInnovation, null);
  assert.equal(recorded.epochCount, 2);
  assert.equal(smoothed.epochCount, 2);
  assert.equal(recordedEpochs[0].transitionFromPrevious, null);
  assert.equal(recordedEpochs[1].transitionFromPrevious.length, 15);
  assert.equal(recordedEpochs[1].transitionFromPrevious[0].length, 15);
  assert.equal(smoothedEpochs[0].rtsGainToNext.length, 17);
  assert.equal(smoothedEpochs[0].rtsGainToNext[0].length, 17);
  assert.equal(smoothedEpochs[1].rtsGainToNext, null);

  eqBits(update.nis, "0x400A42AD3B07976F");
  eqBits(update.ekf.innovationGate.maxAbsNormalizedInnovation, "0x3FFCF4BA7AE7BCC0");
  eqBits(state.positionEcefM[0], "0x415854A602757FB6");
  eqBits(state.positionEcefM[1], "0x3FC7B6B11D7FA0D8");
  eqBits(state.positionEcefM[2], "0xBFB7B6B11D5C2B22");
  eqBits(smoothedEpochs[0].snapshot.state.positionEcefM[0], "0x415854A6AFB47DAB");
  eqBits(smoothedEpochs[0].snapshot.state.positionEcefM[1], "0x3FB5122C16E56642");
  eqBits(smoothedEpochs[0].snapshot.state.positionEcefM[2], "0xBFA5122C1780E0A5");
  eqBits(smoothedEpochs[0].errorStateCorrection[0], "0xBFFBED1F6AC3E068");
  eqBits(smoothedEpochs[0].errorStateCorrection[1], "0xBFB5122C16E56642");
  eqBits(smoothedEpochs[0].errorStateCorrection[2], "0x3FA5122C1780E0A5");
  eqBits(recordedEpochs[1].transitionFromPrevious[0][0], "0x3FF000019D17A15A");
  eqBits(recordedEpochs[1].transitionFromPrevious[1][1], "0x3FEFFFFE650C7E2C");
  eqBits(recordedEpochs[1].transitionFromPrevious[2][2], "0x3FEFFFFE639F13D3");
});

test("fusion tight SP3 observation update matches reference bits", () => {
  const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const filter = new GnssInsFilter({
    initialState: {
      tJ2000S: 646272000,
      positionEcefM: [4484128, 550582, 4487561],
      velocityEcefMps: [0, 0, 0],
      attitudeBodyToEcef: [1, 0, 0, 0, 1, 0, 0, 0, 1],
    },
    layout: "fifteen",
    covarianceDiagonal: Array(15).fill(10),
    imuSpec: "mems",
    filterKind: "ekf",
    tight: {
      lightTime: true,
      sagnac: true,
      initialClockBiasVarianceM2: 1e8,
      initialClockDriftVarianceM2S2: 1e4,
    },
  });

  const update = filter.updateTightSp3(sp3, {
    tJ2000S: 646272000,
    observations: [{ satelliteId: "G08", pseudorangeM: 23825519.8, pseudorangeSigmaM: 3.0 }],
  });
  const clock = filter.tightClockState();

  assert.equal(update.applied, true);
  assert.equal(update.rows, 1);
  eqBits(update.nis, "0x4021FFF5A609DA6D");
  eqBits(clock.biasM, "0x40DD4BF764C1C30C");
  eqBits(clock.covariance[0], "0x4032FFFFC36F2BC8");
});
