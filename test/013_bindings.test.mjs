// sidereon-core 0.13 capability binding parity.
//
// Provenance: observable-state batches use this repo's vendored SP3 fixture and
// compare the cached interpolant to the parsed SP3 source. Estimation and
// localization vectors mirror the public sidereon-core 0.13 tests.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  PreciseEphemerisInterpolant,
  alphaBetaFilterStep,
  alphaBetaSteadyStateGains,
  cfarCaFalseAlarmProbability,
  cfarCaMultiplierFromPfa,
  cfarCaThreshold,
  chanHoInitialGuess,
  ewmaUpdate,
  ewmaUpdatePowerOfTwo,
  kalmanCvSteadyStateGains,
  loadSp3,
  locateSource,
  madGaussianConsistency,
  madSpread,
  nis,
  nisExpectedValue,
  nisGate,
  nisGateThreshold,
  normalizedInnovation,
  observableStateMissingPositionEcefM,
  preciseEphemerisSamplesFromSamples,
  sourceCrlb,
  sourceDop,
  sourceSolveModeTdoa,
  sourceSolveModeToa,
  sp3PreciseEphemerisSamples,
} from "../pkg-node/sidereon.js";
import { f64Bits, fixture } from "./helpers.mjs";

const close = (actual, expected, tol, label) =>
  assert.ok(Math.abs(actual - expected) <= tol, `${label}: ${actual} vs ${expected}`);

const setupSp3 = () => loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));

function assertMaybeNumberEqual(actual, expected, label) {
  if (expected == null || actual == null) {
    assert.equal(actual, expected, label);
  } else if (Number.isNaN(expected) || Number.isNaN(actual)) {
    assert.equal(Number.isNaN(actual), Number.isNaN(expected), label);
  } else {
    assert.equal(f64Bits(actual), f64Bits(expected), label);
  }
}

function assertVectorBitsEqual(actual, expected, label) {
  assert.equal(actual.length, expected.length, `${label} length`);
  for (let i = 0; i < actual.length; i++) {
    assertMaybeNumberEqual(actual[i], expected[i], `${label}[${i}]`);
  }
}

function assertStateBatchEqual(actual, expected, label) {
  assert.equal(actual.count, expected.count, `${label} count`);
  assert.deepEqual(actual.statuses, expected.statuses, `${label} statuses`);
  assert.deepEqual(actual.elementResults, expected.elementResults, `${label} element results`);
  assert.equal(actual.positionsEcefM.length, expected.positionsEcefM.length, `${label} pos len`);
  for (let i = 0; i < actual.positionsEcefM.length; i++) {
    assertVectorBitsEqual(
      actual.positionsEcefM[i],
      expected.positionsEcefM[i],
      `${label} pos ${i}`,
    );
    assertMaybeNumberEqual(actual.clocksS[i], expected.clocksS[i], `${label} clock ${i}`);
  }
}

test("0.13 observable-state batches and precise interpolant match SP3 source", () => {
  const sp3 = setupSp3();
  const handle = PreciseEphemerisInterpolant.fromSp3(sp3);
  const epochs = sp3.epochsJ2000Seconds();
  const sats = sp3.satellites.filter((sat) => sat.startsWith("G")).slice(0, 2);
  const missing = "S20";
  const sharedEpoch = 0.5 * (epochs[1] + epochs[2]);

  assert.deepEqual([...handle.satellites].sort(), [...sp3.satellites].sort());
  assert.ok(handle.timeScale.length > 0);
  assert.deepEqual(observableStateMissingPositionEcefM().map(Number.isNaN), [true, true, true]);

  const sharedSats = [...sats, missing];
  const directShared = sp3.observableStatesAtSharedJ2000S(sharedSats, sharedEpoch);
  const handleShared = handle.observableStatesAtSharedJ2000S(sharedSats, sharedEpoch);
  assertStateBatchEqual(handleShared, directShared, "shared");
  assert.equal(directShared.statuses[2], "gap");

  const perSats = [sats[0], sats[1], missing, sats[0]];
  const perEpochs = [epochs[0], 0.25 * epochs[1] + 0.75 * epochs[2], epochs[1], epochs[0] - 86_400];
  const directPerEpoch = sp3.observableStatesAtJ2000S(perSats, perEpochs);
  const handlePerEpoch = handle.observableStatesAtJ2000S(perSats, perEpochs);
  assertStateBatchEqual(handlePerEpoch, directPerEpoch, "per-epoch");
  assert.equal(directPerEpoch.statuses[2], "gap");
  assert.equal(directPerEpoch.statuses[3], "gap");

  const samples = sp3PreciseEphemerisSamples(sp3);
  const sampleSource = preciseEphemerisSamplesFromSamples(samples);
  const fromSampleSource = PreciseEphemerisInterpolant.fromPreciseEphemerisSamples(sampleSource);
  const fromRawSamples = PreciseEphemerisInterpolant.fromSamples(samples);
  const sampleBatch = sampleSource.observableStatesAtJ2000S(perSats, perEpochs);
  assertStateBatchEqual(
    fromSampleSource.observableStatesAtJ2000S(perSats, perEpochs),
    sampleBatch,
    "sample-source handle",
  );
  assertStateBatchEqual(
    fromRawSamples.observableStatesAtJ2000S(perSats, perEpochs),
    sampleBatch,
    "raw-sample handle",
  );
});

test("0.13 estimation and detection primitives match core vectors", () => {
  const gains = alphaBetaSteadyStateGains(4.0);
  close(gains.alpha, 0.864_145_399_682_717_8, 1e-12, "alpha");
  close(gains.beta, 0.737_169_180_900_238_8, 1e-12, "beta");

  const kalman = kalmanCvSteadyStateGains(4.0, 1.0, 1.0);
  close(kalman.positionGain, gains.alpha, 1e-12, "kalman position gain");
  close(kalman.rateGain, gains.beta, 1e-12, "kalman rate gain");

  const step = alphaBetaFilterStep({ level: 5.0, rate: 2.0 }, 8.0, 2.0, {
    alpha: 0.6,
    beta: 0.8,
  });
  assert.deepEqual(step.predicted, { level: 9.0, rate: 2.0 });
  assert.equal(step.innovation, -1.0);
  assert.deepEqual(step.updated, { level: 8.4, rate: 1.6 });

  assert.equal(normalizedInnovation(2.0, 4.0), 1.0);
  assert.equal(nis(1.0, 1.0), 1.0);
  assert.equal(nisExpectedValue(3), 3.0);
  close(nisGateThreshold(1, 0.95), 3.841_458_820_694_124, 1e-12, "NIS threshold");
  const gate = nisGate(1.0, 1.0, 1, 0.95);
  assert.equal(gate.inGate, true);
  assert.equal(gate.dof, 1);

  const q75 = 0.674_489_750_196_081_7;
  close(madSpread([-2.0 * q75, -q75, 0.0, q75, 2.0 * q75], 1e-12), 1.0, 1e-12, "MAD");
  close(madGaussianConsistency(), 1.482_602_218_505_602, 0.0, "MAD consistency");
  assert.equal(ewmaUpdate(16.0, 2.0, 1.0 / 16.0), 15.125);
  assert.equal(ewmaUpdatePowerOfTwo(16.0, 2.0, 4), 15.125);

  const multiplier = cfarCaMultiplierFromPfa(4, 1e-3);
  close(multiplier, 18.493_653_007_613_965, 1e-12, "CFAR multiplier");
  const threshold = cfarCaThreshold(4, 1e-3, 5.0);
  assert.equal(threshold, 5.0 * multiplier);
  close(cfarCaFalseAlarmProbability(4, threshold, 5.0), 1e-3, 1e-12, "CFAR PFA");

  assert.throws(() => nis(1.0, 0.0), RangeError);
});

function arrivals(sensors, source, origin, speed) {
  return sensors.map((sensor) => {
    const s = sensor.propagationSpeedMS ?? speed;
    const d = Math.hypot(...source.map((x, i) => x - sensor.positionM[i]));
    return origin + d / s;
  });
}

function assertPositionClose(actual, expected, tol, label) {
  assert.equal(actual.length, expected.length, `${label} length`);
  for (let i = 0; i < actual.length; i++) close(actual[i], expected[i], tol, `${label}[${i}]`);
}

test("0.13 source-localization primitives recover reference vectors", () => {
  const sensors3d = [
    { positionM: [0.0, 0.0, 0.0] },
    { positionM: [1200.0, 0.0, 0.0] },
    { positionM: [0.0, 900.0, 0.0] },
    { positionM: [0.0, 0.0, 700.0] },
    { positionM: [1100.0, 800.0, 600.0] },
  ];
  const source3d = [320.0, 260.0, 180.0];
  const origin = 12.5;
  const speed = 343.0;
  const times3d = arrivals(sensors3d, source3d, origin, speed);

  const seed = chanHoInitialGuess(sensors3d, times3d, speed, sourceSolveModeToa());
  assertPositionClose(seed.positionM, source3d, 1e-8, "seed position");
  close(seed.originTimeS, origin, 1e-10, "seed origin");
  assert.ok(seed.residualRmsS < 1e-11);

  const solution = locateSource(sensors3d, times3d, speed, { timingSigmaS: 0.001 });
  assertPositionClose(solution.positionM, source3d, 1e-7, "solution position");
  close(solution.originTimeS, origin, 1e-10, "solution origin");
  assert.ok(["Weak", "Nominal"].includes(solution.geometryQuality.tier));
  assert.equal(solution.geometryQuality.rank, 4);
  assert.equal(solution.geometryQuality.redundancy, 1);
  assert.equal(solution.geometryQuality.raimCheckable, true);
  assert.equal(solution.geometryQuality.covarianceValidated, true);
  assert.ok(solution.covariance);
  assert.equal(solution.residuals.length, sensors3d.length);
  assert.ok(solution.residuals.every((row) => Math.abs(row.residualS) < 1e-10));

  const sensors2d = [
    { positionM: [0.0, 0.0] },
    { positionM: [1000.0, 0.0] },
    { positionM: [0.0, 800.0] },
    { positionM: [900.0, 900.0] },
  ];
  const source2d = [300.0, 260.0];
  const times2d = arrivals(sensors2d, source2d, 4.0, 340.0);
  const tdoa = locateSource(sensors2d, times2d, 340.0, {
    ...sourceSolveModeTdoa(0),
    timingSigmaS: 0.001,
  });
  assertPositionClose(tdoa.positionM, source2d, 1e-7, "tdoa position");
  close(tdoa.originTimeS, 4.0, 1e-9, "tdoa origin");
  assert.equal(tdoa.residuals.length, sensors2d.length - 1);

  const square = [
    { positionM: [100.0, 0.0] },
    { positionM: [-100.0, 0.0] },
    { positionM: [0.0, 100.0] },
    { positionM: [0.0, -100.0] },
  ];
  const dop = sourceDop(square, [0.0, 0.0], 10.0);
  close(dop.pdop, 10.0, 1e-12, "source PDOP");
  close(dop.hdop, 10.0, 1e-12, "source HDOP");
  assert.equal(dop.vdop, 0.0);
  close(dop.tdop, 0.5, 1e-12, "source TDOP");
  close(dop.gdop, Math.sqrt(100.25), 1e-12, "source GDOP");

  const crlb = sourceCrlb(square, [0.0, 0.0], 10.0, 0.01);
  close(crlb.dop.pdop, 10.0, 1e-12, "CRLB PDOP");
  close(crlb.covariance.positionM2[0][0], 0.005, 1e-15, "CRLB xx");
  close(crlb.covariance.positionM2[1][1], 0.005, 1e-15, "CRLB yy");
  close(crlb.covariance.originTimeS2, 0.000025, 1e-18, "CRLB origin time");
});
