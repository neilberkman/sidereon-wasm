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

import init, { Spk } from "../pkg/sidereon.js";

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
