// CelesTrak CSSI space-weather table parsing, table queries, and decay source use.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  DragForce,
  SpaceWeather,
  civilToJ2000Seconds,
  estimateDecayWithSpaceWeather,
  parseSpaceWeather,
  parseSpaceWeatherTxt,
} from "../pkg-node/sidereon.js";

import { f64Bits, fixture, fixtureText } from "./helpers.mjs";

const bits = (values) =>
  Array.from(values, (x) => `0x${f64Bits(x).toString(16).padStart(16, "0")}`);

test("parseSpaceWeather exposes daily, monthly, coverage, and query rows", () => {
  const table = parseSpaceWeather(fixture("space_weather/SW-All-20260702-trim.csv"));

  assert.equal(table.dayCount, 14);
  assert.equal(table.monthlyCount, 2);
  assert.deepEqual(table.diagnostics, { skipCount: 0, warningCount: 0, skips: [], warnings: [] });
  assert.deepEqual(table.coverage, {
    firstJ2000S: 120484800,
    lastObservedJ2000S: 836136000,
    lastDailyPredictedJ2000S: 836395200,
    endJ2000S: 846763200,
  });

  const observed = table.sampleAt(civilToJ2000Seconds(2026, 7, 1, 12, 0, 0));
  assert.deepEqual(observed, {
    f107: 202.6,
    f107a: 145.9,
    ap: 12,
    class: "observed",
    apDefaulted: false,
  });
  assert.equal(table.day(2026, 7, 3).class, "dailyPredicted");
  assert.equal(table.day(2026, 7, 3).apAvg, 25);
  assert.deepEqual(table.sampleAt(civilToJ2000Seconds(2026, 9, 15, 12, 0, 0)), {
    f107: 118.9,
    f107a: 128.7,
    ap: 4,
    class: "monthlyPredicted",
    apDefaulted: true,
  });
  assert.deepEqual(
    Array.from(table.apArrayAt(civilToJ2000Seconds(2003, 10, 31, 13, 0, 0))),
    [116, 154, 111, 154, 179, 183.125, 236.5],
  );

  const txt = parseSpaceWeatherTxt(fixtureText("space_weather/SW-All-20260702-trim.txt"));
  assert.equal(txt.dayCount, table.dayCount);
  assert.equal(txt.monthlyCount, table.monthlyCount);
  assert.deepEqual(txt.sampleAt(civilToJ2000Seconds(2026, 7, 1, 12, 0, 0)), observed);
});

test("estimateDecayWithSpaceWeather uses the parsed table source", () => {
  const table = parseSpaceWeather(fixture("space_weather/SW-All-20260702-trim.csv"));
  const drag = DragForce.fromBcFactor(0.02, new SpaceWeather(150, 150, 4), 100);
  const decay = estimateDecayWithSpaceWeather(drag, table, {
    epochS: civilToJ2000Seconds(2003, 10, 30, 0, 0, 0),
    positionKm: [6478.3, 0, 0],
    velocityKmS: [0, 7.85, 0],
    scanStepS: 30,
    maxDurationS: 7200,
    maxScanSamples: 300,
    reentryAltitudeKm: 100,
    crossingToleranceS: 1,
  });

  assert.equal(f64Bits(decay.timeToDecayS), 0x4062390000000000n);
  assert.equal(f64Bits(decay.reentryEpochS), 0x419cc9a347200000n);
  assert.deepEqual(bits(decay.reentryPositionKm), [
    "0x40b8e9cbef7c2275",
    "0x4091bf17fdabc600",
    "0x0000000000000000",
  ]);
  assert.deepEqual(bits(decay.reentryVelocityKmS), [
    "0xbff5fc98a5d29a88",
    "0x401ec41273d4f53e",
    "0x0000000000000000",
  ]);
  assert.equal(f64Bits(decay.reentryAltitudeKm), 0x4059000f3e7287f2n);
});
