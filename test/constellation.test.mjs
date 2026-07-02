// GNSS constellation identity catalog binding reproduces the core CelesTrak /
// NAVCEN build, merge, CSV export, and validation against the same fixtures the
// Rust core asserts on (gps_ops_sample.json, navcen_gps_sample.html).

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  fromCelestrakJson,
  parseNavcen,
  mergeNavcen,
  toCsv,
  validate,
  validateAgainstSp3Ids,
  isValid,
  diff,
  changed,
} from "../pkg-node/sidereon.js";
import { fixtureText } from "./helpers.mjs";

const GPS_OPS = fixtureText("constellation/gps_ops_sample.json");
const NAVCEN = fixtureText("constellation/navcen_gps_sample.html");

// The core uses None for an absent optional field, which serde surfaces as
// `undefined`; normalize to `null` so both spellings of "absent" compare equal.
const opt = (v) => v ?? null;

test("fromCelestrakJson derives PRN-sorted GPS records", () => {
  const records = fromCelestrakJson(GPS_OPS);
  assert.deepEqual(
    records.map((r) => r.prn),
    [3, 5, 13, 19],
  );

  const prn3 = records.find((r) => r.prn === 3);
  assert.equal(prn3.noradId, 40294);
  assert.equal(prn3.sp3Id, "G03");
  assert.equal(prn3.system, "GPS");
  assert.equal(prn3.source.celestrak.blockType, "IIF");
});

test("parseNavcen reads PRN/SVN rows and NANU usability", () => {
  const statuses = parseNavcen(NAVCEN);
  assert.deepEqual(
    statuses.map((s) => [s.prn, opt(s.svn)]),
    [
      [3, 69],
      [5, 50],
      [13, 43],
      [19, 59],
    ],
  );

  const prn19 = statuses.find((s) => s.prn === 19);
  assert.equal(prn19.usable, false);
  assert.equal(prn19.activeNanu, true);
  assert.equal(prn19.nanuType, "UNUSABLE");
});

test("mergeNavcen overlays SVN/usability onto compatible PRNs", () => {
  const records = fromCelestrakJson(GPS_OPS);
  const statuses = parseNavcen(NAVCEN);
  const merged = mergeNavcen(records, statuses);
  assert.deepEqual(
    merged.map((r) => [r.prn, opt(r.svn), r.usable]),
    [
      [3, 69, true],
      [5, 50, true],
      [13, null, true],
      [19, 59, false],
    ],
  );
});

test("toCsv exports the compact mapping CSV", () => {
  const merged = mergeNavcen(fromCelestrakJson(GPS_OPS), parseNavcen(NAVCEN));
  assert.equal(
    toCsv(merged, "lower"),
    "prn,norad_cat_id,active,sp3_id\n" +
      "3,40294,true,G03\n" +
      "5,35752,true,G05\n" +
      "13,68791,true,G13\n" +
      "19,28190,false,G19\n",
  );
  // "lower" is the default.
  assert.equal(toCsv(merged), toCsv(merged, "lower"));
});

test("validateAgainstSp3Ids reports inactive/unusable PRNs", () => {
  const merged = mergeNavcen(fromCelestrakJson(GPS_OPS), parseNavcen(NAVCEN));
  const report = validateAgainstSp3Ids(merged, ["G03", "G05", "G13"]);
  assert.deepEqual(report.inactiveUnusablePrns, [{ system: "GPS", prn: 19 }]);
  assert.deepEqual(report.missingSp3Ids, []);
  assert.deepEqual(report.extraSp3Ids, []);
});

test("validate / isValid round-trip the report", () => {
  const records = fromCelestrakJson(GPS_OPS);
  const report = validate(records);
  // A clean CelesTrak catalog has no duplicate or inactive findings.
  assert.deepEqual(report.duplicatePrns, []);
  assert.deepEqual(report.duplicateNoradIds, []);
  assert.deepEqual(report.inactiveUnusablePrns, []);
  assert.equal(isValid(report), true);
});

test("diff / changed detect snapshot changes", () => {
  const previous = fromCelestrakJson(GPS_OPS);
  const merged = mergeNavcen(previous, parseNavcen(NAVCEN));

  // Identical snapshots: no change.
  const none = diff(previous, previous);
  assert.equal(changed(none), false);

  // The NAVCEN merge flips PRN 19 unusable, a usability change on a held PRN.
  const report = diff(previous, merged);
  assert.equal(changed(report), true);
  assert.deepEqual(
    report.usabilityChanged.map((c) => [c.prn, c.from, c.to]),
    [[19, true, false]],
  );
});
