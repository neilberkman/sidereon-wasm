// IONEX serializer reproduces the engine round-trip: parse a vertical-TEC
// product, re-encode it, and confirm re-parsing yields the same grid geometry
// and map axis, byte-stable on the second pass.

import { test } from "node:test";
import assert from "node:assert/strict";

import { loadIonex } from "../pkg-node/sidereon.js";
import { fixture } from "./helpers.mjs";

const INX = "synthetic_2map_7x7.20i";

test("toIonexString re-parses to the same product and is byte-stable", () => {
  const ionex = loadIonex(fixture(INX));
  const text = ionex.toIonexString();
  const reparsed = loadIonex(Buffer.from(text, "utf8"));

  assert.deepEqual(Array.from(reparsed.latNodesDeg), Array.from(ionex.latNodesDeg));
  assert.deepEqual(Array.from(reparsed.lonNodesDeg), Array.from(ionex.lonNodesDeg));
  assert.deepEqual(Array.from(reparsed.mapEpochsJ2000S), Array.from(ionex.mapEpochsJ2000S));
  assert.equal(reparsed.exponent, ionex.exponent);
  assert.equal(reparsed.shellHeightKm, ionex.shellHeightKm);

  // A slant delay query evaluates identically against the re-parsed product.
  const epoch = ionex.mapEpochsJ2000S[0];
  assert.equal(
    reparsed.slantDelay(12, 34, 45, 30, epoch, 1575.42e6),
    ionex.slantDelay(12, 34, 45, 30, epoch, 1575.42e6),
  );

  // Deterministic: re-encoding the re-parsed product is byte-identical.
  assert.equal(reparsed.toIonexString(), text);
});
