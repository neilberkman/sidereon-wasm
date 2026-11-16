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
import { coreGoldens, fixture, f64Bits } from "./helpers.mjs";

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

test("fusion UKF options report their invalid field before filter construction", () => {
  for (const [field, value, coreField] of [
    ["alpha", Number.NaN, "ukf_alpha"],
    ["beta", Number.POSITIVE_INFINITY, "ukf_beta"],
    ["kappa", Number.POSITIVE_INFINITY, "ukf_kappa"],
  ]) {
    const config = baseConfig("ukf");
    config.initialState.positionEcefM[0] = Number.NaN;
    config.ukf = { transform: { [field]: value } };
    assert.throws(
      () => new GnssInsFilter(config),
      (error) =>
        error instanceof RangeError &&
        error.message === `invalid fusion input ${coreField}: must be finite`,
    );
  }
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

test("fusion robust loose recorded RTS smoothing retains transition pins", () => {
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

  // The same filter and update run natively by test/golden-gen.
  const ref = coreGoldens().fusionTight;
  assert.equal(update.applied, ref.applied);
  assert.equal(update.rows, ref.rows);
  eqBits(update.nis, ref.nis);
  eqBits(clock.biasM, ref.clockBiasM);
  eqBits(clock.covariance[0], ref.clockCovariance0);
});

test("joint innovation gate reports its chi-square threshold", () => {
  const config = baseConfig();
  config.loose = { updateOptions: { jointInnovationGateProbability: 0.05 } };
  const filter = new GnssInsFilter(config);
  const update = filter.updateLoose({
    tJ2000S: 0,
    positionEcefM: [WGS84_A_M, 0, 0],
    covariance: [
      [0.5, 0, 0],
      [0, 0.5, 0],
      [0, 0, 0.5],
    ],
    satellitesUsed: 7,
  });
  assert.equal(update.ekf.jointInnovationGate.degreesOfFreedom, 3);
  assert.equal(update.ekf.jointInnovationGate.falseAlarmProbability, 0.05);
  assert.ok(Math.abs(update.ekf.jointInnovationGate.threshold - 7.814727903251179) < 1e-12);
  assert.equal(update.ekf.jointInnovationGate.accepted, true);
});

test("UKF joint innovation probability gates the complete measurement", () => {
  const config = baseConfig("ukf");
  config.ukf = { jointInnovationGateProbability: 0.05 };
  const filter = new GnssInsFilter(config);
  const update = filter.updateLoose(looseFix(0, [WGS84_A_M + 100, 0, 0]));
  assert.equal(update.applied, false);
  assert.equal(update.ekf.jointInnovationGate.degreesOfFreedom, 3);
  assert.equal(update.ekf.jointInnovationGate.falseAlarmProbability, 0.05);
  assert.ok(Math.abs(update.ekf.jointInnovationGate.threshold - 7.814727903251179) < 1e-12);
  assert.equal(update.ekf.jointInnovationGate.accepted, false);
});

// Literal position-only observation: H = -I, P = diag(4,9,16), R =
// diag(1,4,9), v = (1,-2,3). Robust thresholds are deliberately above
// every standardized residual, so neither taper nor adaptation alters S.
test("robust fusion uses the independently calculated raw innovation covariance", () => {
  const config = baseConfig();
  config.covarianceDiagonal = [4, 9, 16, ...Array(12).fill(1)];
  config.loose = {
    updateOptions: { jointInnovationGateProbability: 0.05 },
    measurementReweighting: { k0Sigma: 1.5, k1Sigma: 3 },
    predictionAdaptation: { threshold: 100 },
  };
  const h = [
    [-1, 0, 0],
    [0, -1, 0],
    [0, 0, -1],
  ];
  const p = [
    [4, 0, 0],
    [0, 9, 0],
    [0, 0, 16],
  ];
  const r = [
    [1, 0, 0],
    [0, 4, 0],
    [0, 0, 9],
  ];
  const innovation = [1, -2, 3];
  const s = h.map((row, i) =>
    h.map(
      (other, j) =>
        r[i][j] +
        row.reduce(
          (sum, value, k) => sum + value * p[k].reduce((sub, pk, l) => sub + pk * other[l], 0),
          0,
        ),
    ),
  );
  const expectedNis = innovation.reduce((sum, value, i) => sum + (value * value) / s[i][i], 0);
  const update = new GnssInsFilter(config).updateLoose({
    tJ2000S: 0,
    positionEcefM: [WGS84_A_M + 1, -2, 3],
    covariance: [
      [1, 0, 0],
      [0, 4, 0],
      [0, 0, 9],
    ],
    satellitesUsed: 7,
  });
  assert.deepEqual(update.ekf.innovationCovariance, s);
  assert.ok(Math.abs(update.nis - expectedNis) < 1e-14);
  assert.ok(Math.abs(update.ekf.jointInnovationGate.statistic - expectedNis) < 1e-14);
  assert.equal(update.ekf.jointInnovationGate.accepted, true);
});

test("joint robust gate uses raw rather than tapered measurement covariance", () => {
  const config = baseConfig();
  config.loose = {
    updateOptions: { jointInnovationGateProbability: 0.05 },
    measurementReweighting: { k0Sigma: 1.5, k1Sigma: 3 },
    predictionAdaptation: {},
  };
  // H=-I, P=I, R=I and innovation (4,0,0) give raw S=2I and NIS=8.
  // IGG inflates the first measurement variance, so gating on that inflated
  // covariance would pass the 7.8147 threshold instead of rejecting.
  const update = new GnssInsFilter(config).updateLoose({
    tJ2000S: 0,
    positionEcefM: [WGS84_A_M + 4, 0, 0],
    covariance: [
      [1, 0, 0],
      [0, 1, 0],
      [0, 0, 1],
    ],
    satellitesUsed: 7,
  });
  assert.ok(Math.abs(update.ekf.jointInnovationGate.statistic - 8) < 1e-14);
  assert.equal(update.ekf.jointInnovationGate.accepted, false);
  assert.equal(update.applied, false);
  assert.ok(update.ekf.innovationCovariance[0][0] > 2);
});

test("tight joint gates apply to EKF and UKF observations", () => {
  const product = fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3")
    .toString()
    .replace(/^PG08.*$/gm, "PG08  -7385.701131  20983.278657  14326.711553    -38.591721");
  const sp3 = loadSp3(Buffer.from(product));
  const receiver = [4484128, 550582, 4487561];
  const satellite = [-7385701.131, 20983278.657, 14326711.553];
  const range = Math.hypot(...satellite.map((x, i) => x - receiver[i]));
  const prediction = range - 299792458 * -38.591721e-6;
  // H = [1], P = [2], R = [1], so S = 3 and NIS = innovation² / 3.
  for (const kind of ["ekf", "ukf"]) {
    for (const innovation of [1, 8]) {
      const config = baseConfig(kind);
      config.initialState.tJ2000S = 646228800;
      config.initialState.positionEcefM = receiver;
      config.covarianceDiagonal = Array(15).fill(0);
      config.tight = { lightTime: false, sagnac: false, initialClockBiasVarianceM2: 2 };
      if (kind === "ekf") config.tight.updateOptions = { jointInnovationGateProbability: 0.01 };
      else config.ukf = { jointInnovationGateProbability: 0.01 };
      const filter = new GnssInsFilter(config);
      const before = filter.state();
      const clock = filter.tightClockState();
      const report = filter.updateTightSp3(sp3, {
        tJ2000S: 646228800,
        observations: [
          { satelliteId: "G08", pseudorangeM: prediction + innovation, pseudorangeSigmaM: 1 },
        ],
      });
      const gate = report.ekf.jointInnovationGate;
      assert.equal(gate.degreesOfFreedom, 1);
      assert.equal(gate.falseAlarmProbability, 0.01);
      assert.ok(Math.abs(gate.threshold - 6.6348966010212145) < 1e-12);
      assert.ok(
        Math.abs(gate.statistic - innovation ** 2 / 3) < 1e-5,
        `${kind} ${innovation} ${gate.statistic}`,
      );
      assert.ok(Math.abs(report.ekf.innovationCovariance[0][0] - 3) < 1e-6);
      assert.equal(gate.accepted, innovation === 1);
      assert.equal(report.applied, innovation === 1);
      if (innovation === 8) {
        assert.deepEqual(filter.state(), before);
        assert.deepEqual(filter.tightClockState(), clock);
      }
    }
  }
});

test("filter kind and innovation gate options are validated", () => {
  const gate = { jointInnovationGateProbability: 0.05 };
  for (const [kind, section, field] of [
    ["ekf", "ukf", "ukf_update_options"],
    ["ukf", "loose", "loose.update_options"],
    ["ukf", "tight", "tight.update_options"],
  ]) {
    const config = baseConfig(kind);
    config[section] = section === "ukf" ? gate : { updateOptions: gate };
    assert.throws(
      () => new GnssInsFilter(config),
      (error) => error instanceof RangeError && error.message.includes(field),
    );
  }
  for (const [kind, section, field] of [
    ["ekf", "ukf", "ukf_update_options"],
    ["ukf", "loose", "loose.update_options"],
    ["ukf", "tight", "tight.update_options"],
  ]) {
    const config = baseConfig(kind);
    const rowGate = { innovationGate: { thresholdSigma: 3, minRows: 1 } };
    config[section] = section === "ukf" ? rowGate : { updateOptions: rowGate };
    assert.throws(
      () => new GnssInsFilter(config),
      (error) => error instanceof RangeError && error.message.includes(field),
    );
  }
  const wrongTransform = baseConfig("ekf");
  wrongTransform.ukf = { transform: { alpha: 0.25 } };
  assert.throws(() => new GnssInsFilter(wrongTransform), /ukf_update_options/);
  for (const [kind, section] of [
    ["ekf", "loose"],
    ["ekf", "tight"],
    ["ukf", "ukf"],
  ]) {
    for (const probability of [0, 1, -0.1, NaN, Infinity]) {
      const config = baseConfig(kind);
      config[section] =
        section === "ukf"
          ? { jointInnovationGateProbability: probability }
          : { updateOptions: { jointInnovationGateProbability: probability } };
      assert.throws(() => new GnssInsFilter(config), RangeError);
    }
    const config = baseConfig(kind);
    const both = { ...gate, innovationGate: { thresholdSigma: 3, minRows: 1 } };
    config[section] = section === "ukf" ? both : { updateOptions: both };
    assert.throws(() => new GnssInsFilter(config), /mutually exclusive/);
  }
});

test("IGG removes all loose rows and coasts with either gate", () => {
  for (const kind of ["ekf", "ukf"]) {
    for (const gate of [
      { jointInnovationGateProbability: 0.01 },
      { innovationGate: { thresholdSigma: 3, minRows: 1 } },
    ]) {
      const config = baseConfig(kind);
      config.loose = { measurementReweighting: {} };
      if (kind === "ekf") config.loose.updateOptions = gate;
      else config.ukf = gate;
      const filter = new GnssInsFilter(config);
      const before = filter.state();
      const fix = loosePositionVelocityFix();
      fix.positionEcefM = [WGS84_A_M + 50, 50, 50];
      fix.velocityEcefMps = [50, 50, 50];
      const update = filter.updateLoose(fix);
      assert.equal(update.applied, false);
      assert.equal(update.ekf.acceptedRows, 0);
      assert.equal(update.ekf.rejectedRows, 6);
      assert.equal(update.ekf.jointInnovationGate, null);
      if (gate.innovationGate) {
        assert.equal(update.ekf.innovationGate.inputRows, 6);
        assert.equal(update.ekf.innovationGate.rejectedRows, 6);
        assert.equal(update.ekf.innovationGate.coasted, true);
      }
      assert.deepEqual(filter.state(), before);
    }
  }
});

test("rejected loose update preserves tight cross covariance", () => {
  const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const config = baseConfig();
  config.initialState.tJ2000S = 646272000;
  config.initialState.positionEcefM = [4484128, 550582, 4487561];
  config.loose = { updateOptions: { jointInnovationGateProbability: 0.05 } };
  const a = new GnssInsFilter(config);
  const b = new GnssInsFilter(config);
  const epoch = {
    tJ2000S: 646272000,
    observations: [{ satelliteId: "G08", pseudorangeM: 23825519.8, pseudorangeSigmaM: 3 }],
  };
  a.updateTightSp3(sp3, epoch);
  b.updateTightSp3(sp3, epoch);
  a.propagate(zeroIncrement(646272001, 1));
  b.propagate(zeroIncrement(646272001, 1));
  assert.equal(a.updateLoose(looseFix(646272001, [4485128, 550582, 4487561])).applied, false);
  a.propagate(zeroIncrement(646272002, 1));
  b.propagate(zeroIncrement(646272002, 1));
  epoch.tJ2000S = 646272002;
  assert.deepEqual(a.updateTightSp3(sp3, epoch), b.updateTightSp3(sp3, epoch));
  assert.deepEqual(a.state(), b.state());
  assert.deepEqual(a.tightClockState(), b.tightClockState());
});
