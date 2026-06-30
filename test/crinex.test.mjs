// CRINEX (Hatanaka) decoding through the WASM binding, mirroring
// sidereon-python/tests/test_crinex.py against the same committed fixtures.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  decodeCrinex,
  decodeCrinexLines,
  encodeCrinex,
  loadCrinex,
  parseRinexObs,
} from "../pkg-node/sidereon.js";

import { fixture, fixtureText, splitlines } from "./helpers.mjs";

const ESBC_CRX = "obs/ESBC00DNK_R_20201770000_01D_30S_MO_trim.crx";
const ESBC_RNX = "obs/ESBC00DNK_R_20201770000_01D_30S_MO_trim.rnx";
const ALGO_CRX = "obs/algo0010_2015001_v1_trim.crx";
const ALGO_RNX = "obs/algo0010_2015001_v1_trim.rnx";

test("decode crinex matches the plain rinex reference and parses epochs", () => {
  const decoded = decodeCrinex(fixture(ESBC_CRX));
  const reference = fixtureText(ESBC_RNX);

  assert.deepEqual(splitlines(decoded), splitlines(reference));

  const decodedObs = parseRinexObs(Buffer.from(decoded, "utf8"));
  const referenceObs = parseRinexObs(Buffer.from(reference, "utf8"));
  assert.equal(decodedObs.epochCount, 2);
  assert.equal(referenceObs.epochCount, 2);
  assert.deepEqual(
    decodedObs.epochs.map((e) => e.satellites),
    referenceObs.epochs.map((e) => e.satellites),
  );
  assert.equal(decodedObs.epochs[0].epoch.second, referenceObs.epochs[0].epoch.second);
  assert.equal(decodedObs.epochs[1].epoch.second, referenceObs.epochs[1].epoch.second);
});

test("decode crinex lines matches the rinex v1 reference", () => {
  const lines = decodeCrinexLines(fixture(ALGO_CRX));
  assert.deepEqual(lines, splitlines(fixtureText(ALGO_RNX)));
});

test("encode crinex round-trips through decode for v3 and v1 references", () => {
  for (const rnx of [ESBC_RNX, ALGO_RNX]) {
    const reference = fixtureText(rnx);
    const encoded = encodeCrinex(reference);
    // The CRINEX header marks the compact form (it is not the plain RINEX text).
    assert.match(encoded, /CRINEX VERS {3}\/ TYPE/);
    assert.notEqual(encoded, reference);
    // decode(encode(rinex)) == rinex for any RINEX OBS text that round-trips.
    assert.deepEqual(
      splitlines(decodeCrinex(Buffer.from(encoded, "utf8"))),
      splitlines(reference),
    );
  }
});

test("encode crinex rejects malformed rinex", () => {
  assert.throws(() => encodeCrinex("not a rinex obs file\n"), Error);
});

test("load accepts bytes and errors are typed", () => {
  const expectedLines = splitlines(fixtureText(ESBC_RNX));
  assert.deepEqual(splitlines(loadCrinex(fixture(ESBC_CRX))), expectedLines);
  assert.deepEqual(
    splitlines(loadCrinex(Buffer.from(fixtureText(ESBC_CRX), "utf8"))),
    expectedLines,
  );

  assert.throws(() => decodeCrinex(Buffer.from("not a crinex file\n", "utf8")), Error);
  // A non-UTF-8 buffer is rejected as a TypeError before decoding.
  assert.throws(() => loadCrinex(Uint8Array.from([0xff])), TypeError);
});
