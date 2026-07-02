// Wave 2 parity exports: Tle.groundTrack and visibleFromSatellites.
//
// Neither reinvents geometry: groundTrack is the engine's
// propagate -> TEME->GCRS -> GCRS->ITRS -> geodetic composition, and
// visibleFromSatellites is the same single-instant topocentric path lookAngles
// uses, honoring each Tle's opsmode. Both are checked here bit-for-bit against
// the binding's own existing primitives (no external goldens needed): the
// numbers MUST agree to the last IEEE-754 bit, not within a tolerance.
//
// The three TLEs share an epoch (~2026-06-17), so all are valid at a single
// instant: ISS (LEO), NAVSTAR 43 (GPS / MEO, deep-space), GALAXY 15 (GEO,
// deep-space). The deep-space objects are where AFSPC vs improved opsmode
// genuinely diverge.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  Tle,
  GroundStation,
  visibleFromSatellites,
  temeToGcrs,
  gcrsToItrs,
  ecefToGeodetic,
} from "../pkg-node/sidereon.js";
import { f64Bits, bigints } from "./helpers.mjs";

const eqBits = (a, b, msg) => assert.equal(f64Bits(a), f64Bits(b), msg);

const ISS = {
  id: "25544",
  l1: "1 25544U 98067A   26168.18949189  .00009113  00000+0  17172-3 0  9996",
  l2: "2 25544  51.6332 300.0813 0004737 195.1146 164.9702 15.49273435571752",
  inclDeg: 51.6332,
};
const NAVSTAR = {
  id: "24876",
  l1: "1 24876U 97035A   26167.20443871 -.00000012  00000+0  00000+0 0  9994",
  l2: "2 24876  55.9944  98.6138 0102442  56.9091 304.0464  2.00563771211931",
};
const GALAXY = {
  id: "28884",
  l1: "1 28884U 05041A   26167.71607684 -.00000267  00000+0  00000+0 0  9995",
  l2: "2 28884   3.5359  77.2731 0014354 137.8081 105.3728  0.98943614 75438",
};

const STATION = () => new GroundStation(51.5074, -0.1278, 11.0);
const EPOCH = BigInt(Date.UTC(2026, 5, 17, 12, 0, 0)) * 1000n;

const sats = (mode = "improved") => [ISS, NAVSTAR, GALAXY].map((s) => new Tle(s.l1, s.l2, mode));
const ids = () => [ISS, NAVSTAR, GALAXY].map((s) => s.id);
const linesById = { [ISS.id]: ISS, [NAVSTAR.id]: NAVSTAR, [GALAXY.id]: GALAXY };

// ── groundTrack ───────────────────────────────────────────────────────────────

const issEpochs = () => {
  const list = [];
  for (let i = 0; i < 8; i++) list.push(Date.UTC(2026, 5, 17, 4, i, 0) * 1000);
  return list;
};

test("groundTrack reproduces the propagate->frames->geodetic composition bit-for-bit", () => {
  const t = new Tle(ISS.l1, ISS.l2, "improved");
  const epochList = issEpochs();
  const epochs = bigints(epochList);

  const track = t.groundTrack(epochs);
  assert.equal(track.epochCount, epochList.length);
  assert.equal(track.latDeg.length, epochList.length);
  assert.equal(track.lonDeg.length, epochList.length);
  assert.equal(track.altKm.length, epochList.length);

  // Manual chain through the binding's own primitives. ground_track propagates
  // (TEME) then reduces with the non-Skyfield-compat frame path, so pass
  // skyfieldCompat=false to temeToGcrs / gcrsToItrs to match the engine.
  const prop = t.propagate(epochs);
  const gcrs = temeToGcrs(prop.positionKm, prop.velocityKmS, epochs, false);
  const ecef = gcrsToItrs(gcrs.positionKm, epochs, false);
  const geo = ecefToGeodetic(ecef); // flat (n,3): [latDeg, lonDeg, altKm]

  // core::ground_track returns Wgs84Geodetic, which stores lat/lon in radians
  // and height in metres; groundTrack converts back to deg/km for JS. So the
  // bit-exact reference is geo run through the same unit round-trips (Rust
  // f64::to_radians/to_degrees == multiply by these exact constants), not the
  // raw geo degrees.
  const RAD = Math.PI / 180.0;
  const DEG = 180.0 / Math.PI;
  for (let i = 0; i < epochList.length; i++) {
    eqBits(track.latDeg[i], geo[i * 3 + 0] * RAD * DEG, `lat bit mismatch at epoch ${i}`);
    eqBits(track.lonDeg[i], geo[i * 3 + 1] * RAD * DEG, `lon bit mismatch at epoch ${i}`);
    eqBits(track.altKm[i], (geo[i * 3 + 2] * 1000.0) / 1000.0, `alt bit mismatch at epoch ${i}`);
  }
});

test("groundTrack sub-points are physically plausible and longitude is monotonic", () => {
  const t = new Tle(ISS.l1, ISS.l2, "improved");
  const track = t.groundTrack(bigints(issEpochs()));

  for (let i = 0; i < track.epochCount; i++) {
    assert.ok(
      Math.abs(track.latDeg[i]) <= ISS.inclDeg + 1.0e-3,
      `lat ${track.latDeg[i]} exceeds inclination`,
    );
    assert.ok(
      track.altKm[i] > 300.0 && track.altKm[i] < 600.0,
      `alt ${track.altKm[i]} km implausible`,
    );
  }

  const deltas = [];
  for (let i = 1; i < track.epochCount; i++) {
    let d = track.lonDeg[i] - track.lonDeg[i - 1];
    if (d > 180.0) d -= 360.0;
    else if (d < -180.0) d += 360.0;
    deltas.push(d);
  }
  assert.ok(
    deltas.every((d) => d > 0.0) || deltas.every((d) => d < 0.0),
    `longitude not monotonic: ${deltas}`,
  );
});

// ── visibleFromSatellites ─────────────────────────────────────────────────────

test("visibleFromSatellites matches per-satellite lookAngles bit-for-bit", () => {
  // mask -90 so nothing is filtered: every satellite comes back.
  const visible = visibleFromSatellites(sats(), ids(), STATION(), EPOCH, -90.0);
  assert.equal(visible.length, 3);

  // Cross-check each result against lookAngles for a freshly built Tle of the
  // same lines + opsmode, at the same single epoch.
  for (const v of visible) {
    const s = linesById[v.catalogNumber];
    const look = new Tle(s.l1, s.l2, "improved").lookAngles(STATION(), BigInt64Array.from([EPOCH]));
    eqBits(v.azimuthDeg, look.azimuthDeg[0], `az mismatch for ${v.catalogNumber}`);
    eqBits(v.elevationDeg, look.elevationDeg[0], `el mismatch for ${v.catalogNumber}`);
    eqBits(v.rangeKm, look.rangeKm[0], `range mismatch for ${v.catalogNumber}`);
    assert.equal(v.positionKm.length, 3);
    assert.ok(v.positionKm.every(Number.isFinite));
  }
});

test("visibleFromSatellites sorts by elevation descending", () => {
  const visible = visibleFromSatellites(sats(), ids(), STATION(), EPOCH, -90.0);
  assert.equal(visible.length, 3);
  for (let i = 1; i < visible.length; i++) {
    assert.ok(visible[i - 1].elevationDeg >= visible[i].elevationDeg, "not sorted by elevation");
  }
  // The three objects are at distinct elevations (LEO/MEO/GEO geometry differs).
  const els = visible.map((v) => v.elevationDeg);
  assert.ok(new Set(els).size === 3, `expected distinct elevations, got ${els}`);
});

test("visibleFromSatellites filters by the elevation mask", () => {
  const all = visibleFromSatellites(sats(), ids(), STATION(), EPOCH, -90.0);
  const maxEl = Math.max(...all.map((v) => v.elevationDeg));

  // A mask above every satellite's elevation drops them all.
  assert.equal(visibleFromSatellites(sats(), ids(), STATION(), EPOCH, maxEl + 1.0).length, 0);

  // Every returned satellite is at or above the mask.
  const some = visibleFromSatellites(sats(), ids(), STATION(), EPOCH, maxEl);
  assert.ok(some.length >= 1);
  for (const v of some) assert.ok(v.elevationDeg >= maxEl);
});

test("visibleFromSatellites honors each Tle's opsmode (deep-space afspc != improved)", () => {
  // Catalog 23599: deep-space, low inclination (incl 6.9 deg, ~322 min period).
  // This is the regime where the lyddane periodics make AFSPC vs improved genuinely
  // diverge (same object the existing passes_opsmode regression uses). The
  // divergence grows with time since epoch, so evaluate ~120 days out, where the
  // two modes are bit-distinct at a single instant.
  const DS_L1 = "1 23599U 95029B   06171.76535463  .00085586  12891-6  12956-2 0  2905";
  const DS_L2 = "2 23599   6.9327   0.2849 5782022 274.4436  25.2425  4.47796565123555";
  const dsEpoch = BigInt(Date.UTC(2006, 5, 20, 18, 21, 0) + 120 * 86400000) * 1000n;

  const improved = visibleFromSatellites(
    [new Tle(DS_L1, DS_L2, "improved")],
    ["23599"],
    STATION(),
    dsEpoch,
    -90.0,
  );
  const afspc = visibleFromSatellites(
    [new Tle(DS_L1, DS_L2, "afspc")],
    ["23599"],
    STATION(),
    dsEpoch,
    -90.0,
  );
  assert.equal(improved.length, 1);
  assert.equal(afspc.length, 1);
  // If opsmode were ignored these would be bit-identical.
  assert.ok(
    f64Bits(improved[0].elevationDeg) !== f64Bits(afspc[0].elevationDeg) ||
      f64Bits(improved[0].azimuthDeg) !== f64Bits(afspc[0].azimuthDeg),
    "deep-space improved vs afspc identical: opsmode not honored",
  );
});

test("visibleFromSatellites maps ids to catalogNumber and rejects a length mismatch", () => {
  const labels = ["alpha", "bravo", "charlie"];
  const visible = visibleFromSatellites(sats(), labels, STATION(), EPOCH, -90.0);
  assert.deepEqual([...visible.map((v) => v.catalogNumber)].sort(), [...labels].sort());

  assert.throws(() => visibleFromSatellites(sats(), ["only-one"], STATION(), EPOCH, -90.0));
});
