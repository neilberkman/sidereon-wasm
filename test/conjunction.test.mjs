// Conjunction + covariance binding reproduces the engine numbers bit-for-bit,
// against conjunction.json. Vectors are flat length-3 and matrices flat
// row-major (length 9 for 3x3, length 4 for 2x2).

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  ConjunctionState,
  encounterFrame,
  encounterPlaneCovariance,
  collisionProbability,
  rtnToEciCovariance,
  covarianceIsSymmetric,
  covarianceIsPositiveSemidefinite,
} from "../pkg-node/sidereon.js";
import { fixtureJson, hexToF64, f64Bits } from "./helpers.mjs";

const FX = fixtureJson("conjunction.json");
const eqBits = (value, hex) => assert.equal(f64Bits(value), BigInt(hex));
const vec = (hexList) => Float64Array.from(hexList.map(hexToF64));
const matFlat = (hexRows) => Float64Array.from(hexRows.flat().map(hexToF64));

const state = (entry) =>
  new ConjunctionState(
    vec(entry.position_km_hex),
    vec(entry.velocity_km_s_hex),
    matFlat(entry.covariance_km2_hex),
  );

const OBJ1 = state(FX.object1);
const OBJ2 = state(FX.object2);

const frame = () =>
  encounterFrame(OBJ1.positionKm, OBJ1.velocityKmS, OBJ2.positionKm, OBJ2.velocityKmS);

test("conjunction state exposes flat vectors / matrix", () => {
  assert.equal(OBJ1.positionKm.length, 3);
  assert.equal(OBJ1.velocityKmS.length, 3);
  assert.equal(OBJ1.covarianceKm2.length, 9);
});

test("encounter frame matches reference bits", () => {
  const f = frame();
  const ref = FX.frame;
  f.xHat.forEach((v, i) => eqBits(v, ref.x_hat_hex[i]));
  f.yHat.forEach((v, i) => eqBits(v, ref.y_hat_hex[i]));
  f.zHat.forEach((v, i) => eqBits(v, ref.z_hat_hex[i]));
  f.relativePositionKm.forEach((v, i) => eqBits(v, ref.relative_position_km_hex[i]));
  f.relativeVelocityKmS.forEach((v, i) => eqBits(v, ref.relative_velocity_km_s_hex[i]));
  eqBits(f.missKm, ref.miss_km_hex);
  eqBits(f.relativeSpeedKmS, ref.relative_speed_km_s_hex);
});

test("encounter-plane covariance matches reference bits", () => {
  const combined = matFlat(FX.combined_covariance_km2_hex);
  const projected = encounterPlaneCovariance(frame(), combined);
  assert.equal(projected.length, 4);
  const expected = FX.encounter_plane_covariance_hex.flat();
  projected.forEach((v, i) => eqBits(v, expected[i]));
});

test("collision probability methods match reference bits", () => {
  const hbr = hexToF64(FX.hard_body_radius_km_hex);
  for (const entry of FX.collision_probability) {
    const result = collisionProbability(OBJ1, OBJ2, hbr, entry.method.toLowerCase());
    eqBits(result.pc, entry.pc_hex);
    eqBits(result.missKm, entry.miss_km_hex);
    eqBits(result.relativeSpeedKmS, entry.relative_speed_km_s_hex);
    eqBits(result.sigmaXKm, entry.sigma_x_km_hex);
    eqBits(result.sigmaZKm, entry.sigma_z_km_hex);
  }
});

test("rtn -> eci covariance matches reference bits", () => {
  const ref = FX.rtn;
  const eci = rtnToEciCovariance(
    matFlat(ref.covariance_rtn_hex),
    vec(ref.position_km_hex),
    vec(ref.velocity_km_s_hex),
  );
  assert.equal(eci.length, 9);
  const expected = ref.covariance_eci_hex.flat();
  eci.forEach((v, i) => eqBits(v, expected[i]));
  assert.equal(covarianceIsSymmetric(matFlat(ref.covariance_rtn_hex)), ref.symmetric);
  assert.equal(
    covarianceIsPositiveSemidefinite(matFlat(ref.covariance_rtn_hex)),
    ref.positive_semidefinite,
  );
});

test("conjunction errors throw", () => {
  assert.throws(
    () => new ConjunctionState(new Float64Array(2), new Float64Array(3), new Float64Array(9)),
  );
  const eye = Float64Array.from([1, 0, 0, 0, 1, 0, 0, 0, 1]);
  assert.throws(() =>
    rtnToEciCovariance(eye, Float64Array.from([0, 0, 0]), Float64Array.from([1, 1, 1])),
  );
  assert.throws(() =>
    encounterFrame(
      Float64Array.from([0, 0, 0]),
      Float64Array.from([1, 1, 1]),
      Float64Array.from([1, 1, 1]),
      Float64Array.from([1, 1, 1]),
    ),
  );
  assert.throws(() => collisionProbability(OBJ1, OBJ2, -1.0));
  assert.throws(() => collisionProbability(OBJ1, OBJ2, 0.0));
});
