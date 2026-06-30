// Broadcast-vs-precise ephemeris accuracy (SISRE orbit/clock) through the WASM
// binding. The differencing, RAC decomposition, finite-difference velocity, and
// the RMS/median/datum statistics are sidereon-core's `broadcast_comparison`;
// this checks the binding marshals a real comparison of the committed day-177
// MGEX broadcast nav against the GBM precise SP3.
//
// These mirror the Elixir binding's physical-truth gates and structural
// invariants (there is no frozen golden; the comparison output is a
// physical-accuracy band): GPS broadcast orbits sit ~1-2 m RMS (3D) against the
// precise product, the RAC components combine in quadrature to the 3D RMS, and
// the datum-removed clock error is smaller than the raw clock difference.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { loadSp3, loadRinexNav } from "../pkg-node/sidereon.js";

const here = (rel) => fileURLToPath(new URL(rel, import.meta.url));

async function load() {
  const sp3 = loadSp3(
    await readFile(here("./fixtures/sp3/GBM0MGXRAP_20201770000_01D_05M_ORB_120epoch.sp3")),
  );
  const nav = loadRinexNav(
    await readFile(here("./fixtures/nav/ESBC00DNK_R_20201770000_01D_MN.rnx")),
  );
  return { sp3, nav };
}

// A multi-epoch GPS window well inside the precise product span (the velocity
// neighbours at +/- step/2 must also stay inside, so leave a node of margin).
function window(sp3) {
  const epochs = sp3.epochsJ2000Seconds();
  const step = epochs[1] - epochs[0];
  const sats = sp3.satellites.filter((s) => s.startsWith("G"));
  return { from: epochs[2], to: epochs[epochs.length - 3], step, sats };
}

test("GPS broadcast orbits sit in the expected accuracy band vs precise", async () => {
  const { sp3, nav } = await load();
  const { from, to, step, sats } = window(sp3);
  assert.ok(sats.length >= 20, "GPS satellites present in the precise product");

  const report = nav.compareToSp3(sp3, sats, from, to, step);
  const o = report.overall;

  assert.ok(o.count > 100, `compared many epochs (got ${o.count})`);
  assert.ok(o.orbit3dRmsM > 0.1 && o.orbit3dRmsM < 6.0, `3D orbit RMS in band: ${o.orbit3dRmsM}`);
  assert.ok(o.orbit3dMaxM < 15.0, `3D orbit max bounded: ${o.orbit3dMaxM}`);
});

test("RAC components combine in quadrature to the 3D RMS", async () => {
  const { sp3, nav } = await load();
  const { from, to, step, sats } = window(sp3);
  const o = nav.compareToSp3(sp3, sats, from, to, step).overall;

  assert.ok(o.radialRmsM > 0 && o.alongRmsM > 0 && o.crossRmsM > 0);
  const quad = Math.hypot(o.radialRmsM, o.alongRmsM, o.crossRmsM);
  assert.ok(Math.abs(o.orbit3dRmsM - quad) < 1e-6, `${o.orbit3dRmsM} vs ${quad}`);
});

test("datum-removed clock error is smaller than the raw clock difference", async () => {
  const { sp3, nav } = await load();
  const { from, to, step, sats } = window(sp3);
  const o = nav.compareToSp3(sp3, sats, from, to, step).overall;

  assert.equal(typeof o.clockRmsM, "number");
  assert.ok(o.clockRmsM > 0);
  assert.ok(o.clockDatumRemovedRmsM > 0);
  assert.ok(o.clockDatumRemovedRmsM < o.clockRmsM);
});

test("per-satellite stats are present and the report is deterministic", async () => {
  const { sp3, nav } = await load();
  const { from, to, step, sats } = window(sp3);

  const a = nav.compareToSp3(sp3, sats, from, to, step);
  const b = nav.compareToSp3(sp3, sats, from, to, step);
  assert.deepEqual(a, b);

  assert.ok(a.perSatellite.length > 0);
  const compared = a.perSatellite.filter((s) => s.stats.count > 0);
  assert.ok(compared.length > 10, "many GPS satellites compared");
  for (const s of compared) {
    assert.ok(s.stats.orbit3dRmsM > 0);
  }
});

test("an empty window throws", async () => {
  const { sp3, nav } = await load();
  const { from, sats } = window(sp3);
  assert.throws(() => nav.compareToSp3(sp3, sats, from, from - 100, 300));
});

test("a window/step that would overflow the epoch cap is rejected, not truncated", async () => {
  const { sp3, nav } = await load();
  const { from, sats } = window(sp3);
  // ~3e10 epochs (a year of one-millisecond steps) exceeds the cap; the binding
  // must reject it rather than silently comparing only the first slice.
  assert.throws(() => nav.compareToSp3(sp3, sats, from, from + 31_536_000, 0.001), RangeError);
});

// The window-form driver `compareWindowToSp3` delegates the grid sampling, the
// final snap to the window end, and the lockstep precise-date advance to
// sidereon-core's `compare_window`, rather than building the per-epoch keys in
// the binding. It lands in the same physical-accuracy band as `compareToSp3`.

test("compareWindowToSp3 lands in the broadcast accuracy band and is deterministic", async () => {
  const { sp3, nav } = await load();
  const { from, to, step, sats } = window(sp3);

  const report = nav.compareWindowToSp3(sp3, sats, from, to, step);
  const o = report.overall;

  assert.ok(o.count > 100, `compared many epochs (got ${o.count})`);
  assert.ok(o.orbit3dRmsM > 0.1 && o.orbit3dRmsM < 6.0, `3D orbit RMS in band: ${o.orbit3dRmsM}`);
  assert.ok(o.orbit3dMaxM < 15.0, `3D orbit max bounded: ${o.orbit3dMaxM}`);

  // RAC components combine in quadrature to the 3D RMS, and the call is pure.
  const quad = Math.hypot(o.radialRmsM, o.alongRmsM, o.crossRmsM);
  assert.ok(Math.abs(o.orbit3dRmsM - quad) < 1e-6, `${o.orbit3dRmsM} vs ${quad}`);
  assert.deepEqual(report, nav.compareWindowToSp3(sp3, sats, from, to, step));
});

test("compareWindowToSp3 honours an explicit velocityHalfS", async () => {
  const { sp3, nav } = await load();
  const { from, to, step, sats } = window(sp3);

  // The default half step is round(step / 2); passing it explicitly reproduces
  // the default-argument result exactly.
  const auto = nav.compareWindowToSp3(sp3, sats, from, to, step);
  const explicit = nav.compareWindowToSp3(sp3, sats, from, to, step, Math.round(step / 2));
  assert.deepEqual(auto, explicit);
});

test("compareWindowToSp3 rejects a non-positive step and half step", async () => {
  const { sp3, nav } = await load();
  const { from, to, sats } = window(sp3);
  assert.throws(() => nav.compareWindowToSp3(sp3, sats, from, to, 0), RangeError);
  assert.throws(() => nav.compareWindowToSp3(sp3, sats, from, to, 300, 0), RangeError);
});
