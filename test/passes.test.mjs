// Dense-sample pass finding + visibility series reproduce the engine reference
// arc (pass_finder.json). The finder honors the satellite's opsmode, and the
// golden was generated in AFSPC mode, so the Tle here is constructed with
// opsMode "afspc" to match the golden culmination elevations.

import { test } from "node:test";
import assert from "node:assert/strict";

import { Tle, GroundStation } from "../pkg-node/sidereon.js";
import { fixtureJson, hexToF64, bigints } from "./helpers.mjs";

const FX = fixtureJson("pass_finder.json");
const MAX_ELEVATION_TOL_DEG = 1.0e-9;

const station = () =>
  new GroundStation(FX.station.latitude_deg, FX.station.longitude_deg, FX.station.altitude_m);
const tle = () => new Tle(FX.tle.line1, FX.tle.line2, "afspc");
const opts = FX.options;

const find = () =>
  tle().findPasses(
    station(),
    BigInt(FX.window.start_unix_us),
    BigInt(FX.window.end_unix_us),
    opts.elevation_mask_deg,
    opts.coarse_step_seconds,
    opts.time_tolerance_seconds,
  );

test("passes match reference", () => {
  const passes = find();
  assert.equal(passes.length, FX.passes.length);
  assert.ok(passes.length >= 2);
  passes.forEach((got, i) => {
    const want = FX.passes[i];
    assert.equal(got.aosUnixUs, BigInt(want.aos_unix_us));
    assert.equal(got.losUnixUs, BigInt(want.los_unix_us));
    assert.equal(got.culminationUnixUs, BigInt(want.culmination_unix_us));
    assert.ok(
      Math.abs(got.maxElevationDeg - hexToF64(want.max_elevation_deg_hex)) < MAX_ELEVATION_TOL_DEG,
    );
    assert.ok(got.aosUnixUs <= got.culminationUnixUs && got.culminationUnixUs <= got.losUnixUs);
    assert.ok(got.durationS > 0.0);
  });
});

test("visibility series matches reference passes and culminations", () => {
  const t = tle();
  const s = station();
  const epochList = [
    FX.window.start_unix_us,
    ...FX.passes.map((p) => p.culmination_unix_us),
    FX.window.end_unix_us,
  ];
  const epochs = bigints(epochList);

  const series = t.visibilitySeries(
    s,
    epochs,
    opts.elevation_mask_deg,
    opts.coarse_step_seconds,
    opts.time_tolerance_seconds,
  );
  const look = t.lookAngles(s, epochs);

  assert.deepEqual(Array.from(series.epochUnixUs), Array.from(epochs));
  assert.equal(series.elevationDeg.length, epochList.length);
  assert.equal(series.epochCount, epochList.length);
  assert.deepEqual(Array.from(series.azimuthDeg), Array.from(look.azimuthDeg));
  assert.deepEqual(Array.from(series.elevationDeg), Array.from(look.elevationDeg));
  assert.deepEqual(Array.from(series.rangeKm), Array.from(look.rangeKm));

  assert.equal(series.passCount, FX.passes.length);
  series.passes.forEach((got, i) => {
    const want = FX.passes[i];
    assert.equal(got.aosUnixUs, BigInt(want.aos_unix_us));
    assert.equal(got.culminationUnixUs, BigInt(want.culmination_unix_us));
    assert.ok(
      Math.abs(got.maxElevationDeg - hexToF64(want.max_elevation_deg_hex)) < MAX_ELEVATION_TOL_DEG,
    );
  });

  // Each culmination epoch (indices 1..passes) is visible above the mask.
  FX.passes.forEach((want, idx0) => {
    const idx = idx0 + 1;
    assert.equal(series.visible[idx], 1);
    assert.ok(
      Math.abs(series.elevationDeg[idx] - hexToF64(want.max_elevation_deg_hex)) <
        MAX_ELEVATION_TOL_DEG,
    );
  });
});

test("higher mask keeps fewer passes", () => {
  const t = tle();
  const s = station();
  const start = BigInt(FX.window.start_unix_us);
  const end = BigInt(FX.window.end_unix_us);
  const low = t.findPasses(s, start, end, 0.0, 10.0);
  const high = t.findPasses(s, start, end, 40.0, 10.0);
  assert.ok(high.length <= low.length);
  for (const p of high) assert.ok(p.maxElevationDeg >= 40.0);
});

test("bad window throws", () => {
  const t = tle();
  const s = new GroundStation(51.5, -0.1);
  assert.throws(() => t.findPasses(s, 1000n, 1000n));
});

test("non-positive step throws", () => {
  const t = tle();
  const s = new GroundStation(51.5, -0.1);
  assert.throws(() => t.findPasses(s, 0n, 1000000n, 0.0, 0.0));
});

test("visibility series rejects non-increasing grid", () => {
  const t = tle();
  const s = station();
  assert.throws(() =>
    t.visibilitySeries(s, bigints([FX.window.start_unix_us, FX.window.start_unix_us])),
  );
});
