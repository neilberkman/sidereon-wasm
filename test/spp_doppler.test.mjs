import { test } from "node:test";
import assert from "node:assert/strict";

import { CarrierBand, GnssSystem, carrierFrequencyHz, loadSp3 } from "../pkg-node/sidereon.js";
import {
  C_M_S,
  VELOCITY_OBS_BITS,
  fixture,
  f64Bits,
  hexToF64,
  synthSp3Pseudoranges,
} from "./helpers.mjs";

function request(sp3) {
  const receiver = [4_500_000.0, 500_000.0, 4_500_000.0];
  const tRx = 646_272_000.0;
  return {
    observations: synthSp3Pseudoranges(sp3, tRx, receiver, 0.0),
    tRxJ2000S: tRx,
    tRxSecondOfDayS: 43200,
    dayOfYear: 176,
    initialGuess: [...receiver, 0.0],
    corrections: { ionosphere: false, troposphere: false },
    withGeodetic: true,
  };
}

function dopplerRows() {
  const fL1 = carrierFrequencyHz(GnssSystem.Gps, CarrierBand.L1);
  return VELOCITY_OBS_BITS.map(([sat, bits]) => ({
    satelliteId: sat,
    dopplerHz: -(hexToF64(bits) * fL1) / C_M_S,
    carrierHz: fL1,
  }));
}

test("solveSppWithDopplerVelocity populates receiver drift and covariance surfaces", () => {
  const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const fused = sp3.solveSppWithDopplerVelocity(request(sp3), dopplerRows());
  const receiver = fused.receiver;
  const velocity = fused.velocity;

  assert.equal(fused.velocityError, undefined);
  assert.equal(f64Bits(receiver.rxClockDriftSS), 0x3e112e0be8901777n);
  assert.deepEqual(Array.from(receiver.positionM, f64Bits), [
    0x41512a8800015ed1n,
    0x411e84800001d354n,
    0x41512a87ffffdccbn,
  ]);
  assert.deepEqual(Array.from(receiver.positionCovarianceEcefM2, f64Bits), [
    0x40193a9077c650e9n,
    0x3fe305bde5375d98n,
    0x400fa76c67ca7ddbn,
    0x3fe305bde5375d98n,
    0x3ff769a2448963b7n,
    0x3ff092afb8c6bab6n,
    0x400fa76c67ca7ddbn,
    0x3ff092afb8c6bab6n,
    0x40162857c17a1776n,
  ]);
  assert.equal(receiver.positionCovarianceEnuM2.length, 9);

  assert.deepEqual(Array.from(velocity.velocityMS, f64Bits), [
    0x4027ffffffe610ecn,
    0xc01bffffff6377b4n,
    0x4008000000805100n,
  ]);
  assert.equal(f64Bits(velocity.speedMS), 0x402c6ce322627e07n);
  assert.equal(f64Bits(velocity.clockDriftSS), 0x3e112e0be8901777n);
  assert.deepEqual(Array.from(velocity.stateCovariance, f64Bits), [
    0x3ff0906b12ad87ben,
    0xbfd3507feaeae73fn,
    0x3fe4b8aaad3889c6n,
    0x3e2653d233439f01n,
    0xbfd3507feaeae740n,
    0x3fe06337a5bec445n,
    0x3f9ceec75f860debn,
    0xbdfba852d025dfd4n,
    0x3fe4b8aaad3889c5n,
    0x3f9ceec75f860e2an,
    0x3ffc72af9d7671d6n,
    0x3e30eae3e3ae76f1n,
    0x3e2653d233439f01n,
    0xbdfba852d025dfc9n,
    0x3e30eae3e3ae76f3n,
    0x3c6ae29fdfe73eafn,
  ]);
});

test("pseudorange-only SPP exposes covariance and leaves clock drift absent", () => {
  const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const solution = sp3.solveSpp(request(sp3));
  assert.equal(solution.rxClockDriftSS, undefined);
  assert.equal(solution.positionCovarianceEcefM2.length, 9);
  assert.equal(solution.positionCovarianceEnuM2.length, 9);
});
