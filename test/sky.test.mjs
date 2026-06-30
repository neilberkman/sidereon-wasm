// Ground-observer Sun and Moon geometry over sidereon_core::astro::bodies. The
// station is geodetic (degrees, kilometres) and each instant is a
// unix-microsecond epoch (a JS BigInt). Structural checks: angle ranges, range
// magnitudes, illuminated fraction, and the rise/set + transit finders.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  sunAzEl,
  moonAzEl,
  moonIllumination,
  moonElevationDeg,
  findMoonElevationCrossings,
  findMoonTransits,
} from "../pkg-node/sidereon.js";

// Royal Observatory, Greenwich.
const LAT = 51.4769;
const LON = 0.0;
const ALT_KM = 0.046;
// 2024-01-01T00:00:00Z and 2024-01-02T00:00:00Z in unix microseconds.
const T0 = 1_704_067_200_000_000n;
const T1 = 1_704_153_600_000_000n;

test("sunAzEl returns angles in range and an au-scale slant range", () => {
  const s = sunAzEl(LAT, LON, ALT_KM, T0);
  assert.ok(s.azimuthDeg >= 0 && s.azimuthDeg < 360);
  assert.ok(s.elevationDeg >= -90 && s.elevationDeg <= 90);
  assert.ok(s.rangeKm > 1.3e8 && s.rangeKm < 1.6e8);
});

test("moonAzEl returns angles in range and a lunar-distance slant range", () => {
  const m = moonAzEl(LAT, LON, ALT_KM, T0);
  assert.ok(m.azimuthDeg >= 0 && m.azimuthDeg < 360);
  assert.ok(m.elevationDeg >= -90 && m.elevationDeg <= 90);
  assert.ok(m.rangeKm > 3.4e5 && m.rangeKm < 4.1e5);
});

test("moonElevationDeg agrees with moonAzEl elevation bit-for-bit", () => {
  assert.equal(moonElevationDeg(LAT, LON, ALT_KM, T0), moonAzEl(LAT, LON, ALT_KM, T0).elevationDeg);
});

test("moonElevationDeg throws (not crashes) for an out-of-range latitude", () => {
  // Latitude 120 is finite, so station() validation passes, but the core
  // geometry rejects it. Going through moon_az_el surfaces that as a thrown JS
  // error rather than panicking the wasm module.
  assert.throws(() => moonElevationDeg(120, LON, ALT_KM, T0), Error);
});

test("moonIllumination returns a fraction in [0,1] and a phase angle in [0,180]", () => {
  const il = moonIllumination(LAT, LON, ALT_KM, T0);
  assert.ok(il.illuminatedFraction >= 0 && il.illuminatedFraction <= 1);
  assert.ok(il.phaseAngleDeg >= 0 && il.phaseAngleDeg <= 180);
});

test("findMoonElevationCrossings yields rise/set events over a day", () => {
  const crossings = findMoonElevationCrossings(LAT, LON, ALT_KM, T0, T1, undefined);
  assert.ok(Array.isArray(crossings));
  for (const c of crossings) {
    assert.ok(c.timeUnixUs >= T0 && c.timeUnixUs <= T1);
    assert.ok(c.kind === "rising" || c.kind === "setting");
    assert.ok(Number.isFinite(c.elevationDeg));
  }
});

test("findMoonTransits yields upper/lower culminations over a day", () => {
  const transits = findMoonTransits(LAT, LON, ALT_KM, T0, T1, 300.0, 1.0);
  assert.ok(Array.isArray(transits));
  // The Moon culminates about once a day; expect at least one transit.
  assert.ok(transits.length >= 1);
  for (const t of transits) {
    assert.ok(t.timeUnixUs >= T0 && t.timeUnixUs <= T1);
    assert.ok(t.kind === "upper" || t.kind === "lower");
    assert.ok(Number.isFinite(t.elevationDeg));
  }
});

test("a non-finite station coordinate throws RangeError", () => {
  assert.throws(() => sunAzEl(NaN, LON, ALT_KM, T0), RangeError);
});
