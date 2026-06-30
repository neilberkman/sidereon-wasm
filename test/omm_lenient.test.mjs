// Lenient constellation catalog build through the WASM binding
// (`fromCelestrakJsonLenient`): a raw combined feed yields the resolvable
// records plus a skip list carrying each unresolved entry's identity. Strict
// (`fromCelestrakJson`) and lenient agree on the records when nothing is skipped.

import { test } from "node:test";
import assert from "node:assert/strict";

import { fromCelestrakJson, fromCelestrakJsonLenient } from "../pkg-node/sidereon.js";
import { fixtureText } from "./helpers.mjs";

const GPS_OPS = fixtureText("constellation/gps_ops_sample.json");

test("lenient build matches strict when every entry resolves", () => {
  const strict = fromCelestrakJson(GPS_OPS, "gps");
  const lenient = fromCelestrakJsonLenient(GPS_OPS, "gps");

  assert.equal(lenient.skipped.length, 0);
  assert.deepEqual(lenient.records, strict);
  assert.ok(lenient.records.length > 0);
  assert.ok(lenient.records.every((r) => r.system === "gps"));
});

test("lenient build skips entries of another system, keeping their identity", () => {
  // A pure GPS feed read as GLONASS: no GPS OBJECT_NAME resolves to a GLONASS
  // slot, so every entry is skipped (not thrown) with its identity preserved.
  const catalog = fromCelestrakJsonLenient(GPS_OPS, "glonass");

  assert.equal(catalog.records.length, 0);
  assert.ok(catalog.skipped.length > 0);
  for (const s of catalog.skipped) {
    assert.equal(typeof s.objectName, "string");
    assert.equal(typeof s.noradId, "number");
  }
  // Strict would throw on the first unresolvable name.
  assert.throws(() => fromCelestrakJson(GPS_OPS, "glonass"));
});

test("lenient build defaults the system to gps", () => {
  const withDefault = fromCelestrakJsonLenient(GPS_OPS);
  const explicit = fromCelestrakJsonLenient(GPS_OPS, "gps");
  assert.deepEqual(withDefault, explicit);
});
