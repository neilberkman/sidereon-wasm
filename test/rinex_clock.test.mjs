// RINEX clock parsing through the WASM binding, mirroring
// sidereon-python/tests/test_rinex_clock.py against the same committed fixture.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  parseRinexClock,
  loadRinexClock,
  parseRinexClockLossy,
  loadRinexClockLossy,
  ClockEpoch,
} from "../pkg-node/sidereon.js";

import { fixture, f64Bits } from "./helpers.mjs";

const CLK = "clk/synthetic_rinex_clock.clk";

test("rinex clock series and interpolation parse bit-exact", () => {
  const clock = parseRinexClock(fixture(CLK));

  assert.deepEqual(clock.satellites, ["G05", "G24"]);
  assert.equal(clock.satelliteCount, 2);
  assert.equal(clock.sampleCount, 5);

  const bySat = new Map(clock.series.map((s) => [s.satellite, s]));
  const g05 = bySat.get("G05");
  const g24 = clock.seriesFor("G24");
  assert.notEqual(g24, undefined);

  assert.ok(g05.gpsSeconds instanceof Float64Array);
  assert.equal(g05.gpsSeconds.length, 3);
  assert.equal(g05.biasS.length, 3);
  assert.equal(g05.length, 3);
  assert.equal(g24.length, 2);
  for (let i = 1; i < g05.gpsSeconds.length; i++) {
    assert.equal(g05.gpsSeconds[i] - g05.gpsSeconds[i - 1], 30.0);
  }
  assert.equal(f64Bits(g05.biasS[1]), 0xbf2a36e36f0d4275n);

  const epoch = new ClockEpoch(2026, 5, 13, 0, 0, 30.0);
  assert.equal(epoch.gpsSeconds, g05.gpsSeconds[1]);
  assert.equal(f64Bits(clock.clockS("G05", epoch)), 0xbf2a36e36f0d4275n);

  const g24Exact = new ClockEpoch(2026, 5, 13, 0, 0, 0.0);
  const g24Mid = new ClockEpoch(2026, 5, 13, 0, 0, 15.0);
  assert.equal(f64Bits(clock.clockS("G24", g24Exact)), 0x3f0a36e2eb1c432dn);
  assert.equal(f64Bits(clock.clockSAtGpsSeconds("G24", g24Mid.gpsSeconds)), 0x3f0a36e4a2ea40can);
  assert.equal(clock.clockS("G99", epoch), undefined);
  assert.equal(clock.clockS("G05", new ClockEpoch(2026, 5, 13, 1, 0, 0.0)), undefined);

  assert.throws(() => clock.clockSAtGpsSeconds("G05", NaN), RangeError);
});

test("load accepts bytes", () => {
  assert.equal(loadRinexClock(fixture(CLK)).sampleCount, 5);
  assert.deepEqual(loadRinexClock(fixture(CLK)).satellites, ["G05", "G24"]);
});

test("strict parse errors and lossy variant skips bad rows", () => {
  const shortAs = "AS G05  2026 05 13 00 00  0.000000  1\n";
  assert.throws(() => parseRinexClock(Buffer.from(shortAs, "utf8")), Error);

  const text =
    "AS G05  2026 05 13 00 00  0.000000  1   1.0e-04\n" +
    "AS G06  2026 05 13 00 00  bad-second  1   2.0e-04\n";
  assert.throws(() => parseRinexClock(Buffer.from(text, "utf8")), Error);

  const lossy = parseRinexClockLossy(Buffer.from(text, "utf8"));
  assert.deepEqual(lossy.satellites, ["G05"]);
  assert.equal(
    f64Bits(lossy.clockS("G05", new ClockEpoch(2026, 5, 13, 0, 0, 0.0))),
    f64Bits(1.0e-4),
  );
  assert.equal(loadRinexClockLossy(Buffer.from(shortAs, "utf8")).sampleCount, 0);
});

test("toRinexString re-parses to the same series and is byte-stable", () => {
  const clock = parseRinexClock(fixture(CLK));
  const text = clock.toRinexString();
  const reparsed = parseRinexClock(Buffer.from(text, "utf8"));
  assert.equal(reparsed.satelliteCount, clock.satelliteCount);
  assert.equal(reparsed.sampleCount, clock.sampleCount);
  assert.equal(reparsed.toRinexString(), text);
});
