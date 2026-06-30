// Observational-astronomy geometry over sidereon_core::astro::observation. Each
// case uses a hand-checkable geometry so the binding's marshalling is verified
// against a closed-form value.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  subSolarPoint,
  terminatorLatitudeDeg,
  parallacticAngleDeg,
  satelliteVisualMagnitude,
  subObserverPoint,
} from "../pkg-node/sidereon.js";

test("subSolarPoint of an equatorial prime-meridian Sun is (0, 0)", () => {
  const p = subSolarPoint(Float64Array.from([1, 0, 0]));
  assert.ok(Math.abs(p.latitudeDeg) < 1e-12);
  assert.ok(Math.abs(p.longitudeDeg) < 1e-12);
});

test("subSolarPoint of a +z Sun is the north pole", () => {
  const p = subSolarPoint(Float64Array.from([0, 0, 5]));
  assert.ok(Math.abs(p.latitudeDeg - 90) < 1e-9);
});

test("terminatorLatitudeDeg crosses the equator a quarter turn from the Sun", () => {
  // Equinox sub-solar point (declination 0) at longitude 0: the terminator
  // crosses the equator at the quadrature meridians (+/-90 deg).
  assert.ok(Math.abs(terminatorLatitudeDeg(0, 0, 90)) < 1e-9);
  // Away from quadrature the equinox terminator saturates to the pole.
  assert.ok(Math.abs(Math.abs(terminatorLatitudeDeg(0, 0, 0)) - 90) < 1e-9);
});

test("parallacticAngleDeg is zero on the meridian", () => {
  assert.ok(Math.abs(parallacticAngleDeg(45, 0, 20)) < 1e-12);
});

test("satelliteVisualMagnitude at the reference range and zero phase is the standard magnitude", () => {
  assert.ok(Math.abs(satelliteVisualMagnitude(1000, 0, 5.0, 1000) - 5.0) < 1e-12);
});

test("satelliteVisualMagnitude dims with range (5 log10 distance term)", () => {
  const near = satelliteVisualMagnitude(1000, 0, 5.0, 1000);
  const far = satelliteVisualMagnitude(10000, 0, 5.0, 1000);
  assert.ok(Math.abs(far - near - 5.0) < 1e-9); // 10x range -> +5 mag
});

test("subObserverPoint maps a +x observer over a +z-pole body to lon -90", () => {
  const p = subObserverPoint(Float64Array.from([1, 0, 0]), 0, 90, 0);
  assert.ok(Math.abs(p.latitudeDeg) < 1e-9);
  assert.ok(Math.abs(p.longitudeDeg + 90) < 1e-9);
});

test("subSolarPoint rejects a zero vector via the engine", () => {
  assert.throws(() => subSolarPoint(Float64Array.from([0, 0, 0])));
});
