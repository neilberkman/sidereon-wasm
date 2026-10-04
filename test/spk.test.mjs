// JPL/NAIF SPK (.bsp) ephemeris reading through the WASM binding.
//
// The fixture is the same committed real Type-21 (Extended Modified Difference
// Array) kernel the core asserts on: a JPL Horizons ephemeris for asteroid 433
// Eros (NAIF target 20000433) relative to the Sun (NAIF center 10). The
// reference (et, [x, y, z, vx, vy, vz]) pairs below are CSPICE outputs lifted
// verbatim from the core test `real_type21_kernel_matches_cspice_reference`.
// Reproducing them through the JS API proves Type 21 works end-to-end across the
// wasm boundary, not just the plumbing.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import init, {
  Spk,
  SpkKernels,
  spkInertialFrameName,
  spkInertialFrameRotation,
} from "../pkg/sidereon.js";

const here = (rel) => fileURLToPath(new URL(rel, import.meta.url));
const wasmBytes = await readFile(here("../pkg/sidereon_bg.wasm"));
await init({ module_or_path: wasmBytes });

const EROS = 20000433;
const SUN = 10;
const SPK_TYPE_21 = 21;

// (et seconds past J2000 TDB, [x, y, z, vx, vy, vz]) km, km/s, CSPICE reference.
const REFERENCE = [
  [
    757339200.0,
    [
      198083634.33689928, 56306354.00566181, 67761020.0290685, -14.136880898003753,
      18.729945253375007, 8.080580941541488,
    ],
  ],
  [
    760501440.0,
    [
      140599517.39824444, 110142414.48840125, 87942357.2561364, -22.110500498220798,
      14.728648269072185, 4.367688325339683,
    ],
  ],
  [
    788961600.0,
    [
      -2423286.488811064, -220785626.12491044, -125794359.14041424, 20.360009383792537,
      -4.508637229520069, 1.1193915696949732,
    ],
  ],
];

async function loadKernel() {
  return new Spk(new Uint8Array(await readFile(here("./fixtures/spk/horizons_eros_type21.bsp"))));
}

test("the kernel parses and exposes its single Type-21 segment", async () => {
  const spk = await loadKernel();

  assert.equal(spk.segmentCount, 1);
  const segments = spk.segments;
  assert.equal(segments.length, 1);
  const [seg] = segments;
  assert.equal(seg.dataType, SPK_TYPE_21);
  assert.equal(seg.target, EROS);
  assert.equal(seg.center, SUN);
  assert.ok(seg.startEt < seg.stopEt, "coverage window is non-empty");
});

test("Type-21 states reproduce the CSPICE reference end-to-end", async () => {
  const spk = await loadKernel();

  let maxPositionError = 0;
  let maxVelocityError = 0;
  for (const [et, expected] of REFERENCE) {
    const state = spk.state(EROS, SUN, et);
    assert.equal(state.target, EROS);
    assert.equal(state.center, SUN);

    const pos = state.positionKm;
    const vel = state.velocityKmS;
    assert.ok(vel !== undefined, "Type-21 yields a velocity");

    for (let axis = 0; axis < 3; axis++) {
      maxPositionError = Math.max(maxPositionError, Math.abs(pos[axis] - expected[axis]));
      maxVelocityError = Math.max(maxVelocityError, Math.abs(vel[axis] - expected[axis + 3]));
    }
  }

  // Same parity gates the core asserts: ~1-ULP at these magnitudes
  // (|pos| ~2.2e8 km, |vel| ~20 km/s). Bit-exact agreement is sub-ULP.
  assert.ok(
    maxPositionError < 5e-8,
    `Type-21 position drift ${maxPositionError.toExponential()} km exceeds CSPICE parity gate`,
  );
  assert.ok(
    maxVelocityError < 1e-14,
    `Type-21 velocity drift ${maxVelocityError.toExponential()} km/s exceeds CSPICE parity gate`,
  );
});

test("a non-finite epoch is rejected with a RangeError", async () => {
  const spk = await loadKernel();
  assert.throws(() => spk.state(EROS, SUN, Number.NaN), RangeError);
});

test("an unknown body is rejected with an Error", async () => {
  const spk = await loadKernel();
  assert.throws(
    () => spk.state(99999999, SUN, REFERENCE[0][0]),
    (e) => e instanceof Error && /unknown SPK body/.test(e.message),
  );
});

test("an epoch outside coverage is rejected with an Error", async () => {
  const spk = await loadKernel();
  const seg = spk.segments[0];
  assert.throws(
    () => spk.state(EROS, SUN, seg.stopEt + 1e9),
    (e) => e instanceof Error,
  );
});

const J2000 = 1;
const ECLIPJ2000 = 17;

test("NAIF inertial frames are named and rotated as IRFNAM and IRFROT give them", () => {
  assert.equal(spkInertialFrameName(J2000), "J2000");
  assert.equal(spkInertialFrameName(ECLIPJ2000), "ECLIPJ2000");
  assert.equal(spkInertialFrameName(0), undefined);
  assert.equal(spkInertialFrameName(22), undefined);

  // IRFROT forms the identity as a product of rotations, which can leave a
  // signed zero; `+ 0` compares -0 and 0 as equal.
  assert.deepEqual(
    Array.from(spkInertialFrameRotation(J2000, J2000), (v) => v + 0),
    [1, 0, 0, 0, 1, 0, 0, 0, 1],
  );
  const toEcliptic = spkInertialFrameRotation(J2000, ECLIPJ2000);
  assert.equal(toEcliptic.length, 9);
  // J2000 to ECLIPJ2000 is a rotation about x by the J2000 obliquity.
  assert.equal(toEcliptic[0], 1);
  assert.ok(Math.abs(toEcliptic[4] - Math.cos((84381.448 / 3600) * (Math.PI / 180))) < 1e-15);
  assert.throws(() => spkInertialFrameRotation(J2000, 22), Error);
});

test("stateInFrame rotates the composed state into the requested inertial frame", async () => {
  const spk = await loadKernel();
  const [et] = REFERENCE[0];
  const native = spk.state(EROS, SUN, et);
  const same = spk.stateInFrame(EROS, SUN, et, native.frame);
  assert.deepEqual(Array.from(same.positionKm), Array.from(native.positionKm));
  assert.deepEqual(Array.from(same.velocityKmS), Array.from(native.velocityKmS));
  assert.equal(same.frame, native.frame);

  const other = native.frame === ECLIPJ2000 ? J2000 : ECLIPJ2000;
  const rotated = spk.stateInFrame(EROS, SUN, et, other);
  assert.equal(rotated.frame, other);
  const m = spkInertialFrameRotation(native.frame, other);
  // Each rotated component is a three-term dot product m_r . v. Evaluated in
  // floating point in any order, it differs from the exact product by at most
  // gamma_3 * sum_j |m_rj v_j|, gamma_3 = 3u / (1 - 3u), u = 2^-53 (Higham,
  // Accuracy and Stability of Numerical Algorithms, 3.1). The engine's value
  // and the one formed here therefore differ by at most twice that.
  const u = 2 ** -53;
  const gamma3 = (3 * u) / (1 - 3 * u);
  const check = (got, v, label) => {
    for (let r = 0; r < 3; r++) {
      const row = [m[3 * r], m[3 * r + 1], m[3 * r + 2]];
      const want = row[0] * v[0] + row[1] * v[1] + row[2] * v[2];
      const bound = 2 * gamma3 * row.reduce((acc, mj, j) => acc + Math.abs(mj * v[j]), 0);
      const diff = Math.abs(got[r] - want);
      assert.ok(diff <= bound, `${label}[${r}]: ${diff} exceeds ${bound}`);
    }
  };
  check(rotated.positionKm, native.positionKm, "position");
  check(rotated.velocityKmS, native.velocityKmS, "velocity");
  assert.throws(() => spk.stateInFrame(EROS, SUN, et, 22), Error);
});

test("SpkKernels resolves states across its kernels as the single kernel does", async () => {
  const spk = await loadKernel();
  const kernels = new SpkKernels();
  assert.equal(kernels.length, 0);
  assert.throws(() => kernels.state(EROS, SUN, REFERENCE[0][0]), Error);

  kernels.push(spk);
  kernels.pushBytes(
    new Uint8Array(await readFile(here("./fixtures/spk/horizons_eros_type21.bsp"))),
  );
  assert.equal(kernels.length, 2);
  // A buffer that is not a kernel is refused and leaves the set unchanged.
  assert.throws(() => kernels.pushBytes(new Uint8Array(64)), Error);
  assert.equal(kernels.length, 2);

  for (const [et] of REFERENCE) {
    const single = spk.state(EROS, SUN, et);
    const set = kernels.state(EROS, SUN, et);
    assert.deepEqual(Array.from(set.positionKm), Array.from(single.positionKm));
    assert.deepEqual(Array.from(set.velocityKmS), Array.from(single.velocityKmS));
    const inFrame = kernels.stateInFrame(EROS, SUN, et, ECLIPJ2000);
    const direct = spk.stateInFrame(EROS, SUN, et, ECLIPJ2000);
    assert.deepEqual(Array.from(inFrame.positionKm), Array.from(direct.positionKm));
  }
  assert.throws(() => kernels.state(EROS, SUN, Number.NaN), RangeError);
});
