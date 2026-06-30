// TLE encode round-trip, element getters, and checksum-warning surface, against
// tle_roundtrip.json (the committed ISS lines + the engine's parsed elements).

import { test } from "node:test";
import assert from "node:assert/strict";

import { Tle } from "../pkg-node/sidereon.js";
import { fixtureJson } from "./helpers.mjs";

const FX = fixtureJson("tle_roundtrip.json");

test("toLines reproduces engine encoding character-exact", () => {
  const tle = new Tle(FX.tle.line1, FX.tle.line2, FX.opsmode);
  const [line1, line2] = tle.toLines();
  assert.equal(line1, FX.encoded.line1);
  assert.equal(line2, FX.encoded.line2);
  assert.equal(line1, FX.tle.line1);
  assert.equal(line2, FX.tle.line2);
});

test("element getters match reference", () => {
  const el = FX.elements;
  const tle = new Tle(FX.tle.line1, FX.tle.line2);
  assert.equal(tle.catalogNumber, el.catalog_number);
  assert.equal(tle.classification, el.classification);
  assert.equal(tle.internationalDesignator, el.international_designator);
  assert.equal(tle.epochYear, el.epoch_year);
  assert.equal(tle.epochDayOfYear, el.epoch_day_of_year);
  assert.equal(tle.inclinationDeg, el.inclination_deg);
  assert.equal(tle.raanDeg, el.raan_deg);
  assert.equal(tle.eccentricity, el.eccentricity);
  assert.equal(tle.argPerigeeDeg, el.arg_perigee_deg);
  assert.equal(tle.meanAnomalyDeg, el.mean_anomaly_deg);
  assert.equal(tle.meanMotionRevPerDay, el.mean_motion);
  assert.equal(tle.meanMotionDot, el.mean_motion_dot);
  assert.equal(tle.meanMotionDoubleDot, el.mean_motion_double_dot);
  assert.equal(tle.bstar, el.bstar);
  assert.equal(tle.revNumber, el.rev_number);
});

test("clean TLE has no checksum warnings", () => {
  const tle = new Tle(FX.tle.line1, FX.tle.line2);
  assert.equal(tle.checksumWarnings.length, 0);
});

test("checksum warnings match reference", () => {
  const c = FX.checksum_case;
  const tle = new Tle(c.line1, c.line2);
  const warnings = tle.checksumWarnings;
  assert.equal(warnings.length, c.warnings.length);
  warnings.forEach((got, i) => {
    assert.equal(got.lineLabel, c.warnings[i].line_label);
    assert.equal(got.expected, c.warnings[i].expected);
    assert.equal(got.computed, c.warnings[i].computed);
  });
});

test("bad TLE throws", () => {
  assert.throws(() => new Tle("not a tle", "also not a tle"));
});
