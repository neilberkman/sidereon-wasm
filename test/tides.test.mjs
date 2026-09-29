// Station tidal-displacement bindings delegate to sidereon_core::tides. Each
// returns a geocentric ITRF displacement (metres); the assertions check the
// physical scale (sub-metre), the zero-coefficient ocean-loading degenerate
// case, and the input-shape rejections.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  solidEarthTide,
  oceanTideLoading,
  solidEarthPoleTide,
  stationTideDisplacement,
  stationTideDisplacementBatch,
  StationTideConstants,
  writeOceanLoadingBlqBlock,
} from "../pkg-node/sidereon.js";

// A mid-latitude ITRF station and coarse geocentric Sun/Moon directions (m).
const STATION = Float64Array.from([4517590.0, 837270.0, 4527420.0]);
const SUN = Float64Array.from([1.4e11, 0.4e11, 0.2e11]);
const MOON = Float64Array.from([3.0e8, 1.5e8, 1.0e8]);
const FHR = 12.0;

const mag = (v) => Math.hypot(v[0], v[1], v[2]);

test("solidEarthTide returns a finite sub-metre displacement", () => {
  const d = solidEarthTide(STATION, 2020, 6, 24, FHR, SUN, MOON);
  assert.equal(d.length, 3);
  assert.ok(d.every(Number.isFinite));
  assert.ok(mag(d) < 1.0);
});

test("oceanTideLoading with zero BLQ coefficients yields no displacement", () => {
  const zeros = new Float64Array(33);
  const d = oceanTideLoading(STATION, 2020, 6, 24, FHR, zeros, zeros);
  assert.equal(d.length, 3);
  assert.ok(mag(d) < 1e-12);
});

test("oceanTideLoading rejects a wrong-length BLQ grid", () => {
  const zeros = new Float64Array(33);
  assert.throws(() => oceanTideLoading(STATION, 2020, 6, 24, FHR, new Float64Array(10), zeros));
});

test("solidEarthPoleTide returns a finite sub-metre displacement", () => {
  const d = solidEarthPoleTide(STATION, 2020, 6, 24, FHR, 0.1, 0.3);
  assert.equal(d.length, 3);
  assert.ok(d.every(Number.isFinite));
  assert.ok(mag(d) < 1.0);
});

test("station displacement exposes components and selected tide constants", () => {
  const request = {
    stationEcefM: Array.from(STATION),
    year: 2020,
    month: 6,
    day: 24,
    hour: 12,
    minute: 0,
    second: 0,
    solidEarthTide: true,
    constants: StationTideConstants.Conventions,
    validity: "strict",
  };
  const result = stationTideDisplacement(request);
  assert.equal(result.solidEarthTideEcefM.length, 3);
  assert.equal(result.poleTideEcefM, null);
  assert.equal(result.oceanLoadingEcefM, null);
  assert.equal(result.degraded, null);
});

test("station displacement reports permissive UT1 coverage degradation", () => {
  const request = {
    stationEcefM: Array.from(STATION),
    year: 1800,
    month: 1,
    day: 1,
    hour: 0,
    minute: 0,
    second: 0,
    solidEarthTide: true,
    validity: "permissive",
  };
  const result = stationTideDisplacement(request);
  assert.equal(result.degraded, "beforeCoverage");
  assert.equal(result.solidEarthTideEcefM.length, 3);
});

test("station displacement accepts geodetic station coordinates", () => {
  const result = stationTideDisplacement({
    stationGeodetic: { latitudeRad: 0.5, longitudeRad: 0.2, heightM: 30 },
    year: 2020,
    month: 6,
    day: 24,
    hour: 12,
    minute: 0,
    second: 0,
    solidEarthTide: false,
  });
  assert.deepEqual(result.ecefM, [0, 0, 0]);
  assert.equal(result.solidEarthTideEcefM, null);
});

test("station displacement batch keeps core row results and typed UT1 refusal", () => {
  const rows = stationTideDisplacementBatch({
    stationEcefM: Array.from(STATION),
    epochs: [
      { year: 2020, month: 6, day: 24, hour: 12, minute: 0, second: 0 },
      { year: 1800, month: 1, day: 1, hour: 0, minute: 0, second: 0 },
    ],
    solidEarthTide: true,
    validity: "strict",
  });
  assert.equal(rows.length, 2);
  assert.equal(rows[0].index, 0);
  assert.equal(rows[0].value.degraded, null);
  assert.equal(rows[0].error, null);
  assert.equal(rows[1].value, null);
  // Strict validity refuses UT1 outside the table by name.
  assert.equal(rows[1].error.kind, "FRAME_TRANSFORM");
  assert.deepEqual(rows[1].error.source, {
    kind: "UT1_OUTSIDE_COVERAGE",
    reason: "BEFORE_COVERAGE",
  });
  const permissive = stationTideDisplacementBatch({
    stationEcefM: Array.from(STATION),
    epochs: [{ year: 1800, month: 1, day: 1, hour: 0, minute: 0, second: 0 }],
    solidEarthTide: true,
    validity: "permissive",
  });
  assert.equal(permissive[0].value.degraded, "beforeCoverage");
  assert.equal(permissive[0].error, null);
});

test("low-level station tide failures retain typed core validation fields", () => {
  assert.throws(
    () => solidEarthTide(STATION, 2020, 6, 24, 25, SUN, MOON),
    (error) => {
      assert.equal(error.name, "TideError");
      assert.equal(error.detail.kind, "INVALID_INPUT");
      assert.equal(typeof error.detail.field, "string");
      assert.equal(error.detail.reason, "OUT_OF_RANGE");
      return true;
    },
  );
});

test("station-tide requests reject unknown top-level and nested keys", () => {
  const request = {
    stationEcefM: Array.from(STATION),
    year: 2020,
    month: 6,
    day: 24,
    hour: 12,
    minute: 0,
    second: 0,
    solidEarthTide: false,
  };
  assert.throws(
    () => stationTideDisplacement({ ...request, solidEarthTidee: false }),
    (error) => error.name === "TypeError" && error.message.includes("solidEarthTidee"),
  );
  assert.throws(
    () =>
      stationTideDisplacement({
        ...request,
        stationEcefM: undefined,
        stationGeodetic: { latitudeRad: 0.5, longitudeRad: 0.2, heightM: 30, heigthM: 30 },
      }),
    (error) => error.name === "TypeError" && error.message.includes("heigthM"),
  );
  assert.throws(
    () =>
      stationTideDisplacementBatch({
        stationEcefM: Array.from(STATION),
        epochs: [{ year: 2020, month: 6, day: 24, hour: 12, minute: 0, second: 0, mintue: 0 }],
        solidEarthTide: false,
      }),
    (error) => error.name === "TypeError" && error.message.includes("mintue"),
  );
  const coefficients = Array.from({ length: 3 }, () => Array(11).fill(0));
  assert.throws(
    () =>
      writeOceanLoadingBlqBlock({
        station: "TEST",
        amplitudeM: coefficients,
        phaseDeg: coefficients,
        comments: [{ placement: "beforeStation", line: "$$ retained", palcement: "afterRows" }],
      }),
    (error) => error.name === "TypeError" && error.message.includes("palcement"),
  );
});
