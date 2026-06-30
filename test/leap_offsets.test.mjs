// Leap-second accessors over sidereon_core::astro::time::scales. TAI-UTC and
// GPS-UTC on a UTC calendar date; the two differ by a constant 19 s.

import { test } from "node:test";
import assert from "node:assert/strict";

import { taiUtcOffsetS, gpsUtcOffsetS, leapSeconds } from "../pkg-node/sidereon.js";

test("TAI-UTC is 37 s and GPS-UTC is 18 s from 2017 onward", () => {
  assert.equal(taiUtcOffsetS(2020, 1, 1), 37.0);
  assert.equal(gpsUtcOffsetS(2020, 1, 1), 18.0);
});

test("taiUtcOffsetS agrees with the existing leapSeconds accessor", () => {
  assert.equal(taiUtcOffsetS(2020, 1, 1), leapSeconds(2020, 1, 1));
});

test("GPS-UTC equals TAI-UTC minus 19 s", () => {
  assert.equal(taiUtcOffsetS(2020, 1, 1) - gpsUtcOffsetS(2020, 1, 1), 19.0);
  // Holds across an earlier epoch too (2010: TAI-UTC = 34, GPS-UTC = 15).
  assert.equal(taiUtcOffsetS(2010, 6, 1) - gpsUtcOffsetS(2010, 6, 1), 19.0);
});
