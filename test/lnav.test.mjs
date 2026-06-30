// GPS LNAV codec binding delegates to sidereon_core::navigation::lnav. The core
// assertion is the encode -> decode round trip: integer fields recover exactly,
// and scaled fields chosen on the LSB grid (or zero) recover exactly. The HOW
// helpers (tow / subframeId) and the parity primitives are checked against the
// freshly-encoded subframes.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  lnavEncode,
  lnavDecode,
  lnavTow,
  lnavSubframeId,
  lnavParity,
  lnavParityValid,
} from "../pkg-node/sidereon.js";

// Integer fields plus scaled fields snapped to (or left at) the LSB grid so the
// round trip is exact: toc/toe are exact multiples of the 16 s LSB.
const PARAMS = {
  weekNumber: 150,
  l2Code: 1,
  l2PDataFlag: 0,
  uraIndex: 0,
  svHealth: 0,
  iodc: 100,
  tgd: 0.0,
  toc: 7200.0,
  af0: 0.0,
  af1: 0.0,
  af2: 0.0,
  iode: 100,
  crs: 0.0,
  deltaN: 0.0,
  m0: 0.0,
  cuc: 0.0,
  eccentricity: 0.0,
  cus: 0.0,
  sqrtA: 0.0,
  toe: 7200.0,
  fitIntervalFlag: 0,
  aodo: 0,
  cic: 0.0,
  omega0: 0.0,
  cis: 0.0,
  i0: 0.0,
  crc: 0.0,
  omega: 0.0,
  omegaDot: 0.0,
  idot: 0.0,
};
const OPTIONS = { tow: 100, alert: 0, antiSpoof: 0, integrity: 0, tlmMessage: 0 };

test("lnavEncode produces three 300-bit subframes", () => {
  const sf = lnavEncode(PARAMS, OPTIONS);
  for (const s of [sf.subframe1, sf.subframe2, sf.subframe3]) {
    assert.equal(s.length, 300);
    assert.ok(Array.from(s).every((b) => b === 0 || b === 1));
  }
});

test("lnavDecode round-trips the encoded parameters", () => {
  const sf = lnavEncode(PARAMS, OPTIONS);
  const d = lnavDecode(sf.subframe1, sf.subframe2, sf.subframe3);

  assert.equal(d.weekNumber, 150);
  assert.equal(d.l2Code, 1);
  assert.equal(d.uraIndex, 0);
  assert.equal(d.svHealth, 0);
  assert.equal(d.iodc, 100);
  assert.equal(d.iode, 100);
  assert.equal(d.toc, 7200);
  assert.equal(d.toe, 7200);
  assert.equal(d.fitIntervalFlag, 0);
  assert.equal(d.aodo, 0);
  for (const attr of [
    "tgd",
    "af0",
    "af1",
    "af2",
    "crs",
    "deltaN",
    "m0",
    "cuc",
    "eccentricity",
    "cus",
    "sqrtA",
    "cic",
    "omega0",
    "cis",
    "i0",
    "crc",
    "omega",
    "omegaDot",
    "idot",
  ]) {
    assert.equal(d[attr], 0.0, `${attr} round-trips to zero`);
  }
});

test("lnavTow and lnavSubframeId read the HOW of each subframe", () => {
  const sf = lnavEncode(PARAMS, OPTIONS);
  assert.equal(lnavTow(sf.subframe1), 100n);
  assert.equal(lnavSubframeId(sf.subframe1), 1n);
  assert.equal(lnavSubframeId(sf.subframe2), 2n);
  assert.equal(lnavSubframeId(sf.subframe3), 3n);
  // A buffer of an unsupported length yields undefined.
  assert.equal(lnavTow(Uint8Array.from([0, 1, 0])), undefined);
});

test("the encoded HOW word passes the parity check", () => {
  const sf = lnavEncode(PARAMS, OPTIONS);
  const word1 = sf.subframe1.slice(0, 30);
  const word2 = sf.subframe1.slice(30, 60);
  const d29Prev = word1[28];
  const d30Prev = word1[29];
  assert.equal(lnavParityValid(word2, d29Prev, d30Prev), true);
});

test("lnavParity of all-zero data is all zero", () => {
  const parity = lnavParity(new Uint8Array(24), 0, 0);
  assert.equal(parity.length, 6);
  assert.deepEqual(Array.from(parity), [0, 0, 0, 0, 0, 0]);
});

test("lnavParity rejects a wrong-length data word", () => {
  assert.throws(() => lnavParity(new Uint8Array(23), 0, 0));
});

test("lnavEncode rejects an out-of-range field", () => {
  assert.throws(() => lnavEncode({ ...PARAMS, weekNumber: 2000 }, OPTIONS));
});
