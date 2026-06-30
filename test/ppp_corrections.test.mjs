// Static PPP correction precompute through the WASM binding (`pppCorrections`):
// the solid-earth pole tide and ocean tide loading displacements, mirroring
// sidereon-python/tests/test_new_core_api.py against the same SP3 product and
// the same caller-supplied IERS polar motion / BLQ block. The correction algebra
// is `sidereon_core::ppp_corrections`; these assert the marshalling.

import { test } from "node:test";
import assert from "node:assert/strict";

import { loadSp3, pppCorrections } from "../pkg-node/sidereon.js";
import { fixture } from "./helpers.mjs";

const SP3_FILE = "GRG0MGXFIN_20201760000_01D_15M_ORB.SP3";
const SAT = "G21";
const T_RX_J2000_S = (2459025.0 - 2451545.0) * 86400.0;
const RECEIVER_M = Float64Array.from([3512900.0, 780500.0, 5248700.0]);
const F_L1_HZ = 1575.42e6;
const F_L2_HZ = 1227.6e6;

const loadProduct = () => loadSp3(fixture(`sp3/${SP3_FILE}`));

const epoch = () => ({
  year: 2020,
  month: 6,
  day: 24,
  hour: 12,
  minute: 0,
  second: 0.0,
  tRxJ2000S: T_RX_J2000_S,
  observations: [{ satelliteId: SAT, freq1Hz: F_L1_HZ, freq2Hz: F_L2_HZ }],
});

const finite3 = (v) => v.length === 3 && v.every(Number.isFinite);
const nonzero3 = (v) => v.some((x) => x !== 0.0);

test("pole tide produces a finite, non-zero displacement", () => {
  const sp3 = loadProduct();
  const corr = pppCorrections(sp3, [epoch()], RECEIVER_M, {
    poleTide: { xpArcsec: 0.2, ypArcsec: 0.35 },
  });

  assert.equal(corr.poleTide.length, 1);
  assert.equal(corr.poleTide[0].epochIndex, 0);
  assert.ok(finite3(corr.poleTide[0].vectorM));
  assert.ok(nonzero3(corr.poleTide[0].vectorM));
  // Off by default.
  assert.equal(corr.oceanLoading.length, 0);
  assert.equal(corr.tide.length, 0);
});

test("ocean loading produces a finite, non-zero displacement from a BLQ block", () => {
  const sp3 = loadProduct();
  const amplitudeM = [
    [0.003, 0.001, 0.0006, 0.0003, 0.002, 0.0012, 0.0006, 0.0002, 0.0001, 0.0001, 0.0001],
    [0.001, 0.0004, 0.0002, 0.0001, 0.0006, 0.0004, 0.0002, 0.0001, 0.0001, 0.0001, 0.0001],
    [0.0008, 0.0003, 0.0002, 0.0001, 0.0005, 0.0003, 0.0001, 0.0001, 0.0001, 0.0001, 0.0001],
  ];
  const phaseDeg = Array.from({ length: 3 }, () => Array(11).fill(0.0));

  const corr = pppCorrections(sp3, [epoch()], RECEIVER_M, {
    oceanLoading: { amplitudeM, phaseDeg },
  });

  assert.equal(corr.oceanLoading.length, 1);
  assert.equal(corr.oceanLoading[0].epochIndex, 0);
  assert.ok(finite3(corr.oceanLoading[0].vectorM));
  assert.ok(nonzero3(corr.oceanLoading[0].vectorM));
});

test("a BLQ block of the wrong shape throws a TypeError", () => {
  const sp3 = loadProduct();
  const phaseDeg = Array.from({ length: 3 }, () => Array(11).fill(0.0));
  // Two component rows instead of three.
  const tooFewRows = {
    amplitudeM: [Array(11).fill(0.0), Array(11).fill(0.0)],
    phaseDeg,
  };
  assert.throws(
    () => pppCorrections(sp3, [epoch()], RECEIVER_M, { oceanLoading: tooFewRows }),
    TypeError,
  );

  // A row with the wrong constituent count.
  const wrongConstituents = {
    amplitudeM: [Array(5).fill(0.0), Array(11).fill(0.0), Array(11).fill(0.0)],
    phaseDeg,
  };
  assert.throws(
    () => pppCorrections(sp3, [epoch()], RECEIVER_M, { oceanLoading: wrongConstituents }),
    TypeError,
  );
});

test("a non-finite BLQ value throws a RangeError", () => {
  const sp3 = loadProduct();
  const phaseDeg = Array.from({ length: 3 }, () => Array(11).fill(0.0));
  const amplitudeM = Array.from({ length: 3 }, () => Array(11).fill(0.0));
  amplitudeM[1][4] = Number.NaN;
  assert.throws(
    () => pppCorrections(sp3, [epoch()], RECEIVER_M, { oceanLoading: { amplitudeM, phaseDeg } }),
    RangeError,
  );
});

test("a non-finite pole-tide polar motion throws a RangeError", () => {
  const sp3 = loadProduct();
  assert.throws(
    () =>
      pppCorrections(sp3, [epoch()], RECEIVER_M, {
        poleTide: { xpArcsec: Infinity, ypArcsec: 0.1 },
      }),
    RangeError,
  );
});

test("no options yields all-empty correction tables", () => {
  const sp3 = loadProduct();
  const corr = pppCorrections(sp3, [epoch()], RECEIVER_M, {});
  assert.equal(corr.tide.length, 0);
  assert.equal(corr.poleTide.length, 0);
  assert.equal(corr.oceanLoading.length, 0);
  assert.equal(corr.windupM.length, 0);
  assert.equal(corr.satPcoEcef.length, 0);
  assert.equal(corr.satPcvM.length, 0);
});
