// Multi-system constellation surface through the WASM binding (core A2): the
// per-system `fromCelestrakJson(json, system)` dispatch, GLONASS FDMA channel
// identity on records, the standalone `glonassFdmaChannel` / `gnssSp3Id`
// helpers, and the system-keyed Validation tuple shape. Uses the same GLONASS
// OMM sample the Rust core asserts on.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  fromCelestrakJson,
  glonassFdmaChannel,
  gnssSp3Id,
  validate,
} from "../pkg-node/sidereon.js";
import { fixtureText } from "./helpers.mjs";

const GLO_OPS = fixtureText("constellation/glonass_ops_sample.json");

test("fromCelestrakJson dispatches per system and resolves GLONASS slots + FDMA channels", () => {
  const records = fromCelestrakJson(GLO_OPS, "glonass");
  assert.ok(records.length > 0);
  // Every record is a GLONASS R-token with a resolved FDMA channel in [-7, 6].
  for (const r of records) {
    assert.equal(r.system, "glonass");
    assert.ok(r.sp3Id.startsWith("R"));
    assert.equal(typeof r.fdmaChannel, "number");
    assert.ok(r.fdmaChannel >= -7 && r.fdmaChannel <= 6);
  }
  // The record's FDMA channel agrees with the standalone slot->channel helper.
  for (const r of records) {
    assert.equal(r.fdmaChannel, glonassFdmaChannel(r.prn));
  }
});

test("gnssSp3Id renders the canonical token per system", () => {
  assert.equal(gnssSp3Id("gps", 7), "G07");
  assert.equal(gnssSp3Id("glonass", 13), "R13");
  assert.equal(gnssSp3Id("galileo", 1), "E01");
  assert.throws(() => gnssSp3Id("nope", 1), TypeError);
});

test("gnssSp3Id rejects invalid PRNs instead of coercing them", () => {
  // A u16 parameter would let wasm-bindgen silently coerce these before the
  // guard runs (-1 -> 65535, 1.5 -> 1); they must throw instead.
  assert.throws(() => gnssSp3Id("gps", -1), TypeError);
  assert.throws(() => gnssSp3Id("gps", 1.5), TypeError);
  assert.throws(() => gnssSp3Id("gps", 0), TypeError);
  assert.throws(() => gnssSp3Id("gps", 70000), TypeError);
  assert.throws(() => gnssSp3Id("gps", Number.NaN), TypeError);
});

test("glonassFdmaChannel returns undefined outside the tabulated slot range", () => {
  assert.equal(glonassFdmaChannel(1), 1);
  assert.equal(glonassFdmaChannel(0), undefined);
  assert.equal(glonassFdmaChannel(25), undefined);
});

test("validate reports duplicate / inactive findings as system-keyed pairs", () => {
  const records = fromCelestrakJson(GLO_OPS, "glonass");
  const report = validate(records);
  // A clean GLONASS sample has no duplicate or inactive findings; both fields
  // are now arrays of { system, prn } pairs rather than bare PRNs.
  assert.deepEqual(report.duplicatePrns, []);
  assert.deepEqual(report.inactiveUnusablePrns, []);

  // Inject a duplicate slot and confirm the pair shape carries the system.
  const dup = [records[0], { ...records[0] }];
  const dupReport = validate(dup);
  assert.deepEqual(dupReport.duplicatePrns, [{ system: "glonass", prn: records[0].prn }]);
});
