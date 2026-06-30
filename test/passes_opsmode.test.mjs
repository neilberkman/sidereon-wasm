// Regression: Tle.findPasses (and the passes inside visibilitySeries) must honor
// the satellite's OpsMode, set at construction. Before the fix they routed
// through the ElementSet-based core finder, which hardcodes OpsMode::Afspc, so an
// "improved"-mode Tle silently got AFSPC passes — inconsistent with lookAngles,
// which uses the satellite's real mode. Both now route through
// find_passes_for_satellite(&self.satellite, ...), preserving opsmode.
//
// Note on the two TLEs used here:
//   * ISS is a near-Earth (non-deep-space) satellite. In this core opsmode only
//     affects the deep-space lyddane periodics, so improved == afspc bit-for-bit
//     for the ISS. It is used only to prove findPasses is self-consistent with
//     lookAngles (i.e. routes through the satellite), with real mid-lat passes.
//   * Catalog 23599 (incl 6.9 deg, ~322 min period) IS deep-space and low
//     inclination, so improved vs afspc genuinely diverge. It proves findPasses
//     output actually changes with opsmode and tracks the satellite's real mode.

import { test } from "node:test";
import assert from "node:assert/strict";

import { Tle, GroundStation } from "../pkg-node/sidereon.js";

const station = () => new GroundStation(40.0, -75.0, 0.0);
const MASK = 5.0;
const STEP = 30.0;
const day = (year, m, d) => BigInt(Date.UTC(year, m, d)) * 1000n;

// --- ISS: passes exist at mid-latitude; modes are identical for a LEO sat, so
//     this only checks findPasses ↔ lookAngles consistency per mode. -----------
const ISS_L1 = "1 25544U 98067A   18184.80969102  .00001614  00000-0  31745-4 0  9993";
const ISS_L2 = "2 25544  51.6414 295.8524 0003435 262.6267 204.2868 15.54005638121106";
const ISS_START = day(2018, 6, 3);
const ISS_END = ISS_START + 24n * 3600n * 1000000n;

for (const mode of ["improved", "afspc"]) {
  test(`ISS findPasses is consistent with lookAngles in ${mode} mode`, () => {
    const t = new Tle(ISS_L1, ISS_L2, mode);
    const s = station();
    const passes = t.findPasses(s, ISS_START, ISS_END, MASK, STEP);
    assert.ok(passes.length >= 2, "expected several ISS passes in the window");

    // lookAngles already uses the satellite's real opsmode. The finder's
    // max-elevation must reproduce lookAngles sampled at culmination — only true
    // if findPasses uses the same satellite (not the hardcoded-AFSPC element set).
    const culms = passes.map((p) => p.culminationUnixUs);
    const looks = t.lookAngles(s, BigInt64Array.from(culms));
    passes.forEach((p, i) => {
      assert.ok(
        Math.abs(looks.elevationDeg[i] - p.maxElevationDeg) < 1.0e-6,
        `culmination elevation mismatch at pass ${i}: ` +
          `finder=${p.maxElevationDeg} lookAngles=${looks.elevationDeg[i]}`,
      );
    });
  });
}

// --- Catalog 23599: deep-space, low inclination -> opsmode is observable. ------
const DS_L1 = "1 23599U 95029B   06171.76535463  .00085586  12891-6  12956-2 0  2905";
const DS_L2 = "2 23599   6.9327   0.2849 5782022 274.4436  25.2425  4.47796565123555";
const DS_START = day(2006, 5, 21);
const DS_END = DS_START + 24n * 3600n * 1000000n;

const dsPasses = (mode) =>
  new Tle(DS_L1, DS_L2, mode).findPasses(station(), DS_START, DS_END, MASK, STEP);

test("deep-space findPasses(improved) differs from findPasses(afspc) — opsmode is honored", () => {
  const improved = dsPasses("improved");
  const afspc = dsPasses("afspc");

  assert.ok(improved.length >= 1, "expected passes for 23599 in the window");
  assert.equal(improved.length, afspc.length, "same window/mask -> same pass count");

  // If findPasses ignored opsmode (the bug), these would be bit-identical.
  const culmsDiffer = improved.some((p, i) => p.culminationUnixUs !== afspc[i].culminationUnixUs);
  const elevsDiffer = improved.some((p, i) => p.maxElevationDeg !== afspc[i].maxElevationDeg);
  assert.ok(
    culmsDiffer && elevsDiffer,
    "improved vs afspc passes were identical — findPasses is ignoring opsmode",
  );
});

test("deep-space findPasses tracks the satellite's own mode, not hardcoded AFSPC", () => {
  const tImp = new Tle(DS_L1, DS_L2, "improved");
  const tAfspc = new Tle(DS_L1, DS_L2, "afspc");
  const s = station();

  const passesImp = tImp.findPasses(s, DS_START, DS_END, MASK, STEP);
  assert.ok(passesImp.length >= 1);

  const culms = BigInt64Array.from(passesImp.map((p) => p.culminationUnixUs));
  const lookImp = tImp.lookAngles(s, culms); // improved geometry
  const lookAfspc = tAfspc.lookAngles(s, culms); // afspc geometry (the bug's value)

  passesImp.forEach((p, i) => {
    // findPasses(improved) must match lookAngles(improved) ...
    assert.ok(
      Math.abs(p.maxElevationDeg - lookImp.elevationDeg[i]) < 1.0e-6,
      `improved finder vs improved lookAngles mismatch at pass ${i}`,
    );
    // ... and be DISTINGUISHABLE from the afspc geometry it used to wrongly emit.
    assert.notEqual(
      p.maxElevationDeg,
      lookAfspc.elevationDeg[i],
      `pass ${i} max elevation equals the AFSPC value — opsmode not honored`,
    );
  });
});
