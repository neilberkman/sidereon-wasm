// Fleet batch propagation parity: propagateBatch is the batched form of
// Tle.propagate. It must agree with the per-satellite primitive to the last
// IEEE-754 bit, not within a tolerance, so the batch path is checked here
// against the binding's own Tle.propagate (no external goldens needed).
//
// The three TLEs share an epoch (~2026-06-17): ISS (LEO), NAVSTAR 43 (GPS /
// MEO, deep-space), GALAXY 15 (GEO, deep-space). The opsmode each Tle carries
// is honored per satellite, matching how Tle.propagate behaves.

import { test } from "node:test";
import assert from "node:assert/strict";

import { Tle, propagateBatch } from "../pkg-node/sidereon.js";
import { f64Bits, bigints } from "./helpers.mjs";

const eqBits = (a, b, msg) => assert.equal(f64Bits(a), f64Bits(b), msg);

const ISS = {
  l1: "1 25544U 98067A   26168.18949189  .00009113  00000+0  17172-3 0  9996",
  l2: "2 25544  51.6332 300.0813 0004737 195.1146 164.9702 15.49273435571752",
};
const NAVSTAR = {
  l1: "1 24876U 97035A   26167.20443871 -.00000012  00000+0  00000+0 0  9994",
  l2: "2 24876  55.9944  98.6138 0102442  56.9091 304.0464  2.00563771211931",
};
const GALAXY = {
  l1: "1 28884U 05041A   26167.71607684 -.00000267  00000+0  00000+0 0  9995",
  l2: "2 28884   3.5359  77.2731 0014354 137.8081 105.3728  0.98943614 75438",
};

const LINES = [ISS, NAVSTAR, GALAXY];
const newSats = (mode = "improved") => LINES.map((s) => new Tle(s.l1, s.l2, mode));

// A grid of unix-microsecond epochs around the shared TLE epoch.
const gridEpochs = (n) => {
  const list = [];
  for (let i = 0; i < n; i++) list.push(Date.UTC(2026, 5, 17, 12, i * 5, 0) * 1000);
  return list;
};

// Assert a FleetPropagation matches per-satellite Tle.propagate, element for
// element, over the given epoch list.
function assertFleetMatchesPerSat(mode, epochNums) {
  const epochs = bigints(epochNums);
  const fleet = propagateBatch(newSats(mode), epochs);

  assert.equal(fleet.satelliteCount, LINES.length, "satelliteCount");
  assert.equal(fleet.epochCount, epochNums.length, "epochCount");

  const pos = fleet.positionKm;
  const vel = fleet.velocityKmS;
  const stride = epochNums.length * 3;
  assert.equal(pos.length, LINES.length * stride, "positionKm length");
  assert.equal(vel.length, LINES.length * stride, "velocityKmS length");

  // Per-satellite reference: each Tle.propagate over the same grid must equal
  // the matching contiguous slice of the fleet arrays.
  const refSats = newSats(mode);
  for (let i = 0; i < refSats.length; i++) {
    const ref = refSats[i].propagate(epochs);
    const refPos = ref.positionKm;
    const refVel = ref.velocityKmS;
    assert.equal(ref.epochCount, epochNums.length, `sat ${i} ref epochCount`);
    for (let k = 0; k < stride; k++) {
      eqBits(pos[i * stride + k], refPos[k], `sat ${i} position elem ${k}`);
      eqBits(vel[i * stride + k], refVel[k], `sat ${i} velocity elem ${k}`);
    }
  }
}

test("propagateBatch matches per-satellite propagate at a single epoch", () => {
  assertFleetMatchesPerSat("improved", gridEpochs(1));
});

test("propagateBatch matches per-satellite propagate over N epochs", () => {
  assertFleetMatchesPerSat("improved", gridEpochs(6));
});

test("propagateBatch honors per-satellite opsmode (afspc)", () => {
  assertFleetMatchesPerSat("afspc", gridEpochs(4));
});

test("propagateBatch on an empty fleet yields empty arrays", () => {
  const fleet = propagateBatch([], bigints(gridEpochs(3)));
  assert.equal(fleet.satelliteCount, 0);
  assert.equal(fleet.epochCount, 3);
  assert.equal(fleet.positionKm.length, 0);
  assert.equal(fleet.velocityKmS.length, 0);
});
