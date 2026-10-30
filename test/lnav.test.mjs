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

// Independent raw broadcast integers retained from the unit-contract review.
// Constructing the transmitted words here avoids using lnavEncode as the
// decoder's oracle and protects the public transmitted-integer-times-LSB scale.
const PARITY_TAPS = [
  [1, 2, 3, 5, 6, 10, 11, 12, 13, 14, 17, 18, 20, 23],
  [2, 3, 4, 6, 7, 11, 12, 13, 14, 15, 18, 19, 21, 24],
  [1, 3, 4, 5, 7, 8, 12, 13, 14, 15, 16, 19, 20, 22],
  [2, 4, 5, 6, 8, 9, 13, 14, 15, 16, 17, 20, 21, 23],
  [1, 3, 5, 6, 7, 9, 10, 14, 15, 16, 17, 18, 21, 22, 24],
  [3, 5, 6, 8, 9, 10, 11, 13, 15, 19, 22, 23, 24],
];
const PARITY_MASKS = PARITY_TAPS.map((positions) =>
  positions.reduce((mask, position) => mask | (1 << (24 - position)), 0),
);

function parity(value) {
  let result = 0;
  while (value !== 0) {
    result ^= value & 1;
    value >>>= 1;
  }
  return result;
}

function transmittedSubframe(words) {
  let previousD29 = 0;
  let previousD30 = 0;
  const bits = [];
  for (const sourceWord of words) {
    let parityBits = 0;
    for (let index = 0; index < PARITY_MASKS.length; index += 1) {
      const previous = [0, 2, 5].includes(index) ? previousD29 : previousD30;
      parityBits = parityBits * 2 + (parity(sourceWord & PARITY_MASKS[index]) ^ previous);
    }
    const transmittedData = sourceWord ^ (previousD30 === 1 ? 0xffffff : 0);
    const transmittedWord = transmittedData * 64 + parityBits;
    for (let index = 29; index >= 0; index -= 1) {
      bits.push(Math.floor(transmittedWord / 2 ** index) & 1);
    }
    previousD29 = (parityBits >> 1) & 1;
    previousD30 = parityBits & 1;
  }
  return Uint8Array.from(bits);
}

function signedBits(value, width) {
  return Number(BigInt.asUintN(width, BigInt(value)));
}

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

test("lnavDecode preserves raw LNAV angular fields in semicircles", () => {
  const raw = {
    deltaN: 23_456,
    m0: -123_456_789,
    omega0: -345_678_901,
    i0: 567_890_123,
    omega: -678_901_234,
    omegaDot: 3_456_789,
    idot: -4_321,
  };
  const m0 = signedBits(raw.m0, 32);
  const omega0 = signedBits(raw.omega0, 32);
  const i0 = signedBits(raw.i0, 32);
  const omega = signedBits(raw.omega, 32);
  const subframe1Words = Array.from(
    { length: 10 },
    (_, index) => (0x731fa9 + 0x13579b * index) & 0xffffff,
  );
  const subframe2Words = Array.from(
    { length: 10 },
    (_, index) => (0x2468ad + 0x13579b * index) & 0xffffff,
  );
  const subframe3Words = Array.from(
    { length: 10 },
    (_, index) => (0x1953bf + 0x13579b * index) & 0xffffff,
  );
  subframe2Words[3] = signedBits(raw.deltaN, 16) * 256 + Math.floor(m0 / 2 ** 24);
  subframe2Words[4] = m0 & 0xffffff;
  subframe3Words[2] = Math.floor(omega0 / 2 ** 24);
  subframe3Words[3] = omega0 & 0xffffff;
  subframe3Words[4] = Math.floor(i0 / 2 ** 24);
  subframe3Words[5] = i0 & 0xffffff;
  subframe3Words[6] = Math.floor(omega / 2 ** 24);
  subframe3Words[7] = omega & 0xffffff;
  subframe3Words[8] = signedBits(raw.omegaDot, 24);
  subframe3Words[9] = signedBits(raw.idot, 14) * 4;

  const decoded = lnavDecode(
    transmittedSubframe(subframe1Words),
    transmittedSubframe(subframe2Words),
    transmittedSubframe(subframe3Words),
  );
  const expected = {
    deltaN: raw.deltaN / 2 ** 43,
    m0: raw.m0 / 2 ** 31,
    omega0: raw.omega0 / 2 ** 31,
    i0: raw.i0 / 2 ** 31,
    omega: raw.omega / 2 ** 31,
    omegaDot: raw.omegaDot / 2 ** 43,
    idot: raw.idot / 2 ** 43,
  };
  for (const [field, value] of Object.entries(expected)) {
    assert.equal(decoded[field], value, `${field} is the raw integer times its LNAV LSB`);
    assert.notEqual(decoded[field], value * Math.PI, `${field} is not converted to radians`);
  }
});

test("lnavTow and lnavSubframeId read the HOW of each subframe", () => {
  const sf = lnavEncode(PARAMS, OPTIONS);
  // `options.tow` is the exact HOW count encoded into each independently
  // returned subframe; callers advance it when constructing a time sequence.
  assert.equal(lnavTow(sf.subframe1), 100n);
  assert.equal(lnavTow(sf.subframe2), 100n);
  assert.equal(lnavTow(sf.subframe3), 100n);
  assert.equal(lnavSubframeId(sf.subframe1), 1n);
  assert.equal(lnavSubframeId(sf.subframe2), 2n);
  assert.equal(lnavSubframeId(sf.subframe3), 3n);
  // A buffer of an unsupported length yields undefined.
  assert.equal(lnavTow(Uint8Array.from([0, 1, 0])), undefined);
});

test("full-subframe HOW helpers remove the preceding TLM parity complement", () => {
  const tow = 54_321;
  const sf = lnavEncode(PARAMS, { ...OPTIONS, tow, integrity: 1 });
  const transmittedHow = sf.subframe1.slice(30, 60);

  assert.equal(sf.subframe1[29], 1);
  assert.equal(lnavTow(sf.subframe1), BigInt(tow));
  assert.equal(lnavSubframeId(sf.subframe1), 1n);
  assert.equal(lnavTow(transmittedHow), BigInt(0x1ffff ^ tow));
  assert.equal(lnavSubframeId(transmittedHow), BigInt(0b111 ^ 1));
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
