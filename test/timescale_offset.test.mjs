// Inter-system time-scale offsets through the WASM binding (core A1 surface):
// the fixed-offset query for the atomic scales, the leap-aware query for the
// UTC-based scales, and the new GLONASST / QZSST scales. The numbers are the
// engine's; this only checks the marshalling and the throw-vs-value contract.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  timescaleOffsetS,
  timescaleOffsetAtS,
  timeScaleAbbrev,
  TimeScale,
} from "../pkg-node/sidereon.js";

test("fixed atomic offsets resolve without an epoch", () => {
  // GPST - BDT = +14 s (BDT epoch is 14 leap seconds behind GPST), so the
  // offset added to a GPST reading to get the BDT reading is -14 s.
  assert.equal(timescaleOffsetS(TimeScale.Gpst, TimeScale.Bdt), -14);
  assert.equal(timescaleOffsetS(TimeScale.Bdt, TimeScale.Gpst), 14);
  // TT = TAI + 32.184 s.
  assert.ok(Math.abs(timescaleOffsetS(TimeScale.Tai, TimeScale.Tt) - 32.184) < 1e-9);
});

test("QZSST and GST are nominally synchronous with GPST", () => {
  assert.equal(timescaleOffsetS(TimeScale.Gpst, TimeScale.Qzsst), 0);
  assert.equal(timescaleOffsetS(TimeScale.Gpst, TimeScale.Gst), 0);
  // The new scales carry abbreviations through the same path.
  assert.equal(timeScaleAbbrev(TimeScale.Qzsst), "QZSST");
  assert.equal(timeScaleAbbrev(TimeScale.Glonasst), "GLONASST");
});

test("UTC-based scales reject the no-epoch query as a RangeError", () => {
  assert.throws(() => timescaleOffsetS(TimeScale.Utc, TimeScale.Gpst), RangeError);
  assert.throws(() => timescaleOffsetS(TimeScale.Gpst, TimeScale.Glonasst), RangeError);
  // TDB has no fixed/constant offset and is rejected too.
  assert.throws(() => timescaleOffsetS(TimeScale.Tdb, TimeScale.Tt), RangeError);
});

test("leap-aware query resolves the UTC-based scales at an epoch", () => {
  const jd = 2459025.5; // 2020-07-01, 18 leap seconds.
  assert.equal(timescaleOffsetAtS(TimeScale.Utc, TimeScale.Gpst, jd), 18);
  assert.equal(timescaleOffsetAtS(TimeScale.Gpst, TimeScale.Utc, jd), -18);
  // GLONASST = UTC + 3 h, so GPST -> GLONASST = 10800 - 18 = 10782 s.
  assert.equal(timescaleOffsetAtS(TimeScale.Gpst, TimeScale.Glonasst, jd), 10782);
  // Atomic pairs ignore the epoch; a non-finite jd is fine when no leap is needed.
  assert.equal(timescaleOffsetAtS(TimeScale.Gpst, TimeScale.Bdt, Number.NaN), -14);
});

test("a non-finite epoch is rejected when a leap count is needed", () => {
  assert.throws(() => timescaleOffsetAtS(TimeScale.Utc, TimeScale.Gpst, Number.NaN), RangeError);
});
