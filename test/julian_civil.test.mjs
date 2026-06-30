// JulianDate.fromUtcCivil over sidereon_core::astro::time::model::Instant::
// from_utc_civil: the no-leap civil UTC two-part Julian date. The split follows
// the engine's split_julian_date convention (jd_whole is the *.5 civil-midnight
// boundary, fraction the within-day part).

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  civilToJ2000Seconds,
  j2000SecondsToCivil,
  JulianDate,
  splitJdToJ2000Seconds,
} from "../pkg-node/sidereon.js";

test("fromUtcCivil at midnight has the *.5 boundary and zero fraction", () => {
  // 2000-01-01T00:00:00Z is JD 2451544.5; civil-midnight boundary is 2451544.5,
  // with zero residual fraction.
  const jd = JulianDate.fromUtcCivil(2000, 1, 1);
  assert.equal(jd.whole, 2451544.5);
  assert.equal(jd.fraction, 0);
  assert.equal(jd.jd, 2451544.5);
});

test("fromUtcCivil carries the within-day clock as the fraction", () => {
  const jd = JulianDate.fromUtcCivil(2000, 1, 1, 12, 0, 0);
  assert.equal(jd.whole, 2451544.5);
  assert.equal(jd.fraction, 0.5);
  assert.equal(jd.jd, 2451545.0); // J2000 noon
});

test("civil J2000 conversion wrappers round trip whole seconds", () => {
  const seconds = civilToJ2000Seconds(2000, 1, 1, 12, 0, 0);
  assert.equal(seconds, 0);
  assert.equal(splitJdToJ2000Seconds(2451544.5, 0.5), 0);

  const civil = j2000SecondsToCivil(0n);
  assert.equal(civil.year, 2000n);
  assert.equal(civil.month, 1n);
  assert.equal(civil.day, 1n);
  assert.equal(civil.hour, 12n);
  assert.equal(civil.minute, 0n);
  assert.equal(civil.second, 0n);
});

test("fromUtcCivil accepts a fractional second", () => {
  const jd = JulianDate.fromUtcCivil(2000, 1, 1, 6, 0, 0);
  assert.ok(Math.abs(jd.fraction - 0.25) < 1e-12);
});

test("fromUtcCivil rejects an out-of-range clock field with a RangeError", () => {
  assert.throws(() => JulianDate.fromUtcCivil(2020, 6, 25, 25, 0, 0), RangeError);
});
