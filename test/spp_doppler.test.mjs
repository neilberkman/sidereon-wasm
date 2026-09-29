import { test } from "node:test";
import assert from "node:assert/strict";

import { loadSp3 } from "../pkg-node/sidereon.js";
import { coreGoldens, fixture, f64Bits, hexToF64, synthSp3Pseudoranges } from "./helpers.mjs";

// Inputs and expected values of the fused solve, reproduced natively by
// test/golden-gen: synthetic pseudoranges at the receiver and the velocity
// scenario's range rates converted to L1 Doppler.
const FUSED = coreGoldens().sppDoppler;

function request(observations) {
  const receiver = FUSED.receiverEcefM.map(hexToF64);
  return {
    observations,
    tRxJ2000S: hexToF64(FUSED.tRxJ2000S),
    tRxSecondOfDayS: 43200,
    dayOfYear: 176,
    initialGuess: [...receiver, 0.0],
    corrections: { ionosphere: false, troposphere: false },
    withGeodetic: true,
  };
}

const goldenObservations = () =>
  FUSED.observations.map((o) => ({
    satelliteId: o.satelliteId,
    pseudorangeM: hexToF64(o.pseudorangeM),
  }));

const dopplerRows = () =>
  FUSED.doppler.map((d) => ({
    satelliteId: d.satelliteId,
    dopplerHz: hexToF64(d.dopplerHz),
    carrierHz: hexToF64(d.carrierHz),
  }));

const bits = (hexes) => hexes.map((h) => BigInt(h));

test("solveSppWithDopplerVelocity populates receiver drift and covariance surfaces", () => {
  const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const fused = sp3.solveSppWithDopplerVelocity(request(goldenObservations()), dopplerRows());
  const receiver = fused.receiver;
  const velocity = fused.velocity;

  assert.equal(fused.velocityError, undefined);
  assert.equal(f64Bits(receiver.rxClockDriftSS), BigInt(FUSED.receiver.rxClockDriftSS));
  assert.deepEqual(Array.from(receiver.positionM, f64Bits), bits(FUSED.receiver.positionM));
  assert.deepEqual(
    Array.from(receiver.positionCovarianceEcefM2, f64Bits),
    bits(FUSED.receiver.positionCovarianceEcefM2),
  );
  assert.equal(receiver.positionCovarianceEnuM2.length, 9);

  assert.deepEqual(Array.from(velocity.velocityMS, f64Bits), bits(FUSED.velocity.velocityMS));
  assert.equal(f64Bits(velocity.speedMS), BigInt(FUSED.velocity.speedMS));
  assert.equal(f64Bits(velocity.clockDriftSS), BigInt(FUSED.velocity.clockDriftSS));
  assert.deepEqual(
    Array.from(velocity.stateCovariance, f64Bits),
    bits(FUSED.velocity.stateCovariance),
  );
  // The synthesized receiver is recovered.
  FUSED.receiverEcefM.map(hexToF64).forEach((c, i) => {
    assert.ok(Math.abs(receiver.positionM[i] - c) < 1e-2);
  });
});

test("pseudorange-only SPP exposes covariance and leaves clock drift absent", () => {
  const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const receiver = FUSED.receiverEcefM.map(hexToF64);
  const observations = synthSp3Pseudoranges(sp3, hexToF64(FUSED.tRxJ2000S), receiver, 0.0);
  const solution = sp3.solveSpp(request(observations));
  assert.equal(solution.rxClockDriftSS, undefined);
  assert.equal(solution.positionCovarianceEcefM2.length, 9);
  assert.equal(solution.positionCovarianceEnuM2.length, 9);
});
