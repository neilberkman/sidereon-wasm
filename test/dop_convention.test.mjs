// DOP with an explicit ENU convention over sidereon_core::geometry::
// dop_with_convention. The geodetic-normal path must match the default gnssDop
// bit-for-bit; the geocentric-radial path leaves GDOP/PDOP/TDOP identical and
// only shifts HDOP/VDOP.

import { test } from "node:test";
import assert from "node:assert/strict";

import { gnssDop, dopWithConvention, Wgs84Geodetic } from "../pkg-node/sidereon.js";

const DEG = Math.PI / 180;
const s = Math.SQRT1_2; // sqrt(1/2)

// A deliberately anisotropic 6-satellite geometry (biased toward the positive
// octant), so the position cofactor is not isotropic and rotating it into a
// different ENU actually moves HDOP/VDOP.
const LOS = Float64Array.from([1, 0, 0, 0, 1, 0, 0, 0, 1, s, s, 0, s, 0, s, 0, s, s]);
const WEIGHTS = Float64Array.from([1, 1, 1, 1, 1, 1]);
// Mid-latitude receiver, where the geodetic normal and geocentric radial differ.
const receiver = () => new Wgs84Geodetic(45 * DEG, 10 * DEG, 0.0);

test("geodeticNormal matches the default gnssDop bit-for-bit", () => {
  const base = gnssDop(LOS, WEIGHTS, receiver());
  const conv = dopWithConvention(LOS, WEIGHTS, receiver(), "geodeticNormal");
  for (const attr of ["gdop", "pdop", "hdop", "vdop", "tdop"]) {
    assert.equal(conv[attr], base[attr]);
  }
});

test("geocentricRadial keeps GDOP/PDOP/TDOP and shifts HDOP/VDOP", () => {
  const geodetic = dopWithConvention(LOS, WEIGHTS, receiver(), "geodeticNormal");
  const radial = dopWithConvention(LOS, WEIGHTS, receiver(), "geocentricRadial");
  // GDOP and TDOP read the unrotated cofactor, so they stay bit-identical.
  for (const attr of ["gdop", "tdop"]) {
    assert.equal(radial[attr], geodetic[attr]);
  }
  // PDOP is the trace of the rotated position block: convention-invariant in
  // exact arithmetic, so identical to within rounding.
  assert.ok(Math.abs(radial.pdop - geodetic.pdop) / geodetic.pdop < 1e-12);
  // Horizontal/vertical change, by a small relative amount.
  assert.notEqual(radial.hdop, geodetic.hdop);
  assert.ok(Math.abs(radial.hdop - geodetic.hdop) / geodetic.hdop < 1e-2);
  assert.ok(Math.abs(radial.vdop - geodetic.vdop) / geodetic.vdop < 1e-2);
});

test("an unknown convention throws TypeError", () => {
  assert.throws(() => dopWithConvention(LOS, WEIGHTS, receiver(), "bogus"), TypeError);
});
