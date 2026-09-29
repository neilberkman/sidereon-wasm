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

test("a mismatched checksum is refused under the strict default", () => {
  const c = FX.checksum_case;
  assert.throws(
    () => new Tle(c.line1, c.line2),
    /checksum digit 0 does not match the computed checksum 3/,
  );
  assert.throws(() => new Tle(c.line1, c.line2, undefined, "strict"), Error);
});

test("checksum warnings match reference under the lenient policy", () => {
  const c = FX.checksum_case;
  const tle = new Tle(c.line1, c.line2, undefined, "lenient");
  const warnings = tle.checksumWarnings;
  assert.equal(warnings.length, c.warnings.length);
  warnings.forEach((got, i) => {
    assert.equal(got.lineLabel, c.warnings[i].line_label);
    assert.equal(got.kind, "mismatch");
    assert.equal(got.expected, c.warnings[i].expected);
    assert.equal(got.found, undefined);
    assert.equal(got.computed, c.warnings[i].computed);
    assert.match(got.message, /line 1 checksum digit 0 does not match the computed checksum 3/);
  });
});

test("a line that ends before column 69 is read and reported under both policies", () => {
  const line1 = FX.tle.line1.slice(0, 68);
  for (const policy of [undefined, "strict", "lenient"]) {
    const tle = new Tle(line1, FX.tle.line2, undefined, policy);
    const warnings = tle.checksumWarnings;
    assert.equal(warnings.length, 1);
    assert.equal(warnings[0].kind, "missing");
    assert.equal(warnings[0].expected, undefined);
  }
});

test("a non-digit column 69 is refused strict and reported lenient", () => {
  const line1 = `${FX.tle.line1.slice(0, 68)}X`;
  assert.throws(() => new Tle(line1, FX.tle.line2), Error);
  const tle = new Tle(line1, FX.tle.line2, undefined, "lenient");
  const [warning] = tle.checksumWarnings;
  assert.equal(warning.kind, "notDigit");
  assert.equal(warning.found, "X");
  assert.equal(warning.expected, undefined);
});

test("an unknown TLE policy is a TypeError", () => {
  assert.throws(() => new Tle(FX.tle.line1, FX.tle.line2, undefined, "loose"), TypeError);
});

test("blank element-set, revolution and ephemeris-type fields read as undefined", () => {
  const tle = new Tle(FX.tle.line1, FX.tle.line2);
  assert.equal(typeof tle.elementSetNumber, "number");
  assert.equal(typeof tle.ephemerisType, "number");
  // Columns 63 (ephemeris type) and 65-68 (element-set number) blanked, and
  // line 1 re-checksummed so the strict reader accepts it.
  const chars = FX.tle.line1.split("");
  chars[62] = " ";
  for (let i = 64; i < 68; i++) chars[i] = " ";
  const body = chars.slice(0, 68).join("");
  let sum = 0;
  for (const ch of body) {
    if (ch >= "0" && ch <= "9") sum += Number(ch);
    else if (ch === "-") sum += 1;
  }
  const blank = new Tle(`${body}${sum % 10}`, FX.tle.line2);
  assert.equal(blank.ephemerisType, undefined);
  assert.equal(blank.elementSetNumber, undefined);
  const [line1] = blank.toLines();
  assert.equal(line1.slice(62, 63), " ");
  assert.equal(line1.slice(64, 68), "    ");
});

test("bad TLE throws", () => {
  assert.throws(() => new Tle("not a tle", "also not a tle"));
});
