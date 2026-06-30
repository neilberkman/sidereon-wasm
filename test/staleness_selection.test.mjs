// Product-staleness selection through the WASM binding: IONEX diurnal shift and
// SP3 nearest-prior degradation, plus the typed selection errors. The selection
// logic is sidereon-core's `staleness` module; this checks the binding marshals
// the product set in, attaches the staleness metadata to every result, evaluates
// the selected product bit-for-bit against the caller's product on the exact
// path, and throws a discriminable error (name + structured detail) when no
// product fits the request and cap.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import {
  loadIonex,
  loadSp3,
  selectIonex,
  selectSp3,
  selectSp3OverRange,
} from "../pkg-node/sidereon.js";
import { f64Bits } from "./helpers.mjs";

const here = (rel) => fileURLToPath(new URL(rel, import.meta.url));
const DAY_S = 86_400;

const ionexBytes = await readFile(here("./fixtures/synthetic_2map_7x7.20i"));
const ionex = () => loadIonex(ionexBytes);

const sp3Day177Bytes = await readFile(
  here("./fixtures/sp3/GBM0MGXRAP_20201770000_01D_05M_ORB_120epoch.sp3"),
);
const sp3Day177 = () => loadSp3(sp3Day177Bytes);

const sp3Day176Bytes = await readFile(here("./fixtures/GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
const sp3Day176 = () => loadSp3(sp3Day176Bytes);

// --- IONEX selection --------------------------------------------------------

test("selectIonex returns the present product on the exact path, with zero-staleness metadata", () => {
  const epochs = ionex().mapEpochsJ2000S;
  const epoch = epochs[0];

  const direct = ionex().slantDelay(12, 34, 45, 30, epoch, 1575.42e6);
  const selection = selectIonex([ionex()], epoch, undefined);

  assert.equal(selection.metadata.kind, "exact");
  assert.equal(selection.metadata.stalenessS, 0);
  assert.equal(selection.metadata.stalenessDays, 0);
  assert.equal(selection.metadata.requestedEpochJ2000S, epoch);
  assert.equal(selection.metadata.sourceEpochJ2000S, epoch);

  // The exact selection evaluates bit-for-bit identically to the caller's product.
  const viaSelection = selection.slantDelay(12, 34, 45, 30, epoch, 1575.42e6);
  assert.equal(f64Bits(viaSelection), f64Bits(direct));
  // ...whether queried on the selection or on its exposed Ionex handle.
  const viaHandle = selection.ionex.slantDelay(12, 34, 45, 30, epoch, 1575.42e6);
  assert.equal(f64Bits(viaHandle), f64Bits(direct));
});

test("selectIonex degrades by a whole-day diurnal shift onto a later epoch", () => {
  const epochs = ionex().mapEpochsJ2000S;
  const last = epochs[epochs.length - 1];
  const requested = last + DAY_S; // one day past the freshest map

  const selection = selectIonex([ionex()], requested, { maxStalenessDays: 3 });

  assert.equal(selection.metadata.kind, "diurnalShift");
  assert.equal(selection.metadata.stalenessS, DAY_S);
  assert.equal(selection.metadata.stalenessDays, 1);
  assert.equal(selection.metadata.requestedEpochJ2000S, requested);
  assert.equal(selection.metadata.sourceEpochJ2000S, last);

  // The shifted grid covers the requested epoch, so a slant delay is produced.
  const delay = selection.slantDelay(12, 34, 45, 30, requested, 1575.42e6);
  assert.ok(Number.isFinite(delay) && delay > 0, `diurnal-shifted delay positive (${delay})`);
});

// --- SP3 selection ----------------------------------------------------------

test("selectSp3 returns the covering product on the exact path, querying it bit-for-bit", () => {
  const product = sp3Day177();
  const epochs = product.epochsJ2000Seconds();
  const epoch = epochs[12];
  const sat = product.satellites.find((s) => s.startsWith("G"));

  const direct = product.interpolate(sat, Float64Array.of(epoch));
  const selection = selectSp3([sp3Day177()], epoch, undefined);

  assert.equal(selection.metadata.kind, "exact");
  assert.equal(selection.metadata.stalenessS, 0);

  const state = selection.positionAtJ2000Seconds(sat, epoch);
  assert.equal(f64Bits(state.positionM[0]), f64Bits(direct.positionM[0]));
  assert.equal(f64Bits(state.positionM[1]), f64Bits(direct.positionM[1]));
  assert.equal(f64Bits(state.positionM[2]), f64Bits(direct.positionM[2]));
  assert.equal(f64Bits(state.clockS), f64Bits(direct.clockS[0]));
});

test("selectSp3 degrades to the most-recent prior product, measuring staleness to it", () => {
  const requested = sp3Day177().epochsJ2000Seconds()[12]; // one hour into DOY177
  const priorLast = sp3Day176().epochsJ2000Seconds().at(-1);

  const selection = selectSp3([sp3Day176()], requested, { maxStalenessDays: 3 });

  assert.equal(selection.metadata.kind, "nearestPrior");
  assert.equal(selection.metadata.requestedEpochJ2000S, requested);
  assert.equal(selection.metadata.sourceEpochJ2000S, priorLast);
  assert.equal(selection.metadata.stalenessS, requested - priorLast);
  assert.ok(selection.metadata.stalenessS > 0, "prior product is genuinely stale");
  assert.ok(selection.metadata.stalenessS < 3 * DAY_S, "within the cap");
});

test("selectSp3OverRange returns the covering product unchanged for a fully-covered range", () => {
  const epochs = sp3Day177().epochsJ2000Seconds();
  const selection = selectSp3OverRange([sp3Day177()], epochs[10], epochs[20], undefined);
  assert.equal(selection.metadata.kind, "exact");
  assert.equal(selection.metadata.stalenessS, 0);
  // The range request reports the most-stale (end) epoch as the requested one.
  assert.equal(selection.metadata.requestedEpochJ2000S, epochs[20]);
});

// --- Typed selection errors -------------------------------------------------

test("an empty product set throws a typed EmptyProductSet error", () => {
  assert.throws(
    () => selectSp3([], 0, undefined),
    (e) => e instanceof Error && e.name === "EmptyProductSet",
  );
});

test("a request beyond the staleness cap throws BeyondStalenessCap carrying the numbers", () => {
  const requested = sp3Day177().epochsJ2000Seconds()[12];
  const priorLast = sp3Day176().epochsJ2000Seconds().at(-1);

  assert.throws(
    () => selectSp3([sp3Day176()], requested, { maxStalenessS: 60 }),
    (e) => {
      assert.ok(e instanceof Error);
      assert.equal(e.name, "BeyondStalenessCap");
      assert.equal(e.detail.name, "BeyondStalenessCap");
      assert.equal(e.detail.maxStalenessS, 60);
      assert.equal(e.detail.stalenessS, requested - priorLast);
      assert.equal(e.detail.sourceEpochJ2000S, priorLast);
      assert.equal(e.detail.requestedEpochJ2000S, requested);
      return true;
    },
  );
});

test("a request with only later products throws NoPriorProduct", () => {
  const epochs = sp3Day177().epochsJ2000Seconds();
  const before = epochs[0] - 2 * DAY_S; // precedes every available product
  assert.throws(
    () => selectSp3([sp3Day177()], before, undefined),
    (e) => {
      assert.ok(e instanceof Error);
      assert.equal(e.name, "NoPriorProduct");
      assert.equal(e.detail.requestedEpochJ2000S, before);
      return true;
    },
  );
});

test("a non-finite / negative policy cap throws InvalidPolicy, not silent admission", () => {
  const epoch = sp3Day177().epochsJ2000Seconds()[0];
  assert.throws(
    () => selectSp3([sp3Day177()], epoch, { maxStalenessS: -1 }),
    (e) => e instanceof Error && e.name === "InvalidPolicy" && e.detail.maxStalenessS === -1,
  );
});

test("setting both policy fields is rejected as a malformed request", () => {
  const epoch = sp3Day177().epochsJ2000Seconds()[0];
  assert.throws(
    () => selectSp3([sp3Day177()], epoch, { maxStalenessS: 100, maxStalenessDays: 1 }),
    TypeError,
  );
});
