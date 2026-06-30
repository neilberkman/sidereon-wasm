// Broadcast-ephemeris SPP and the precise-with-broadcast fallback through the
// WASM binding. The positioning is sidereon-core's `solve_broadcast` /
// `solve_with_fallback`; this checks the binding (a) solves broadcast-only,
// (b) prefers a precise product that covers the epoch and reports it as an exact
// precise source bit-for-bit, (c) drops to broadcast when no precise product is
// supplied or the nearest is beyond the cap, attaching the source provenance and
// the staleness/rejection reason, never substituting silently, and (d) makes the
// broadcast and precise fixes genuinely different solutions.
//
// GPS pseudoranges are synthesized from the committed DOY177 precise SP3 itself
// (geometric range to each satellite plus its SP3 clock term) for a receiver at
// the ESBC station, so the recovered position is checked against the synthesis
// truth rather than a fabricated number. The broadcast nav is the committed DOY177
// ESBC mixed navigation; the broadcast fix legitimately differs from the precise
// one by the broadcast signal-in-space error.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { loadSp3, loadRinexNav, solveWithFallback } from "../pkg-node/sidereon.js";

const here = (rel) => fileURLToPath(new URL(rel, import.meta.url));
const C_M_S = 299792458.0;
const norm3 = (a, i = 0) => Math.hypot(a[i], a[i + 1], a[i + 2]);
const sub3 = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];

function geodeticToEcef(latDeg, lonDeg, hM) {
  const a = 6378137.0;
  const f = 1 / 298.257223563;
  const e2 = f * (2 - f);
  const lat = (latDeg * Math.PI) / 180;
  const lon = (lonDeg * Math.PI) / 180;
  const N = a / Math.sqrt(1 - e2 * Math.sin(lat) ** 2);
  return [
    (N + hM) * Math.cos(lat) * Math.cos(lon),
    (N + hM) * Math.cos(lat) * Math.sin(lon),
    (N * (1 - e2) + hM) * Math.sin(lat),
  ];
}

const sp3Day177Bytes = await readFile(
  here("./fixtures/sp3/GBM0MGXRAP_20201770000_01D_05M_ORB_120epoch.sp3"),
);
const sp3Day176Bytes = await readFile(here("./fixtures/GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
const navBytes = await readFile(here("./fixtures/nav/ESBC00DNK_R_20201770000_01D_MN.rnx"));

const sp3Day177 = () => loadSp3(sp3Day177Bytes);
const sp3Day176 = () => loadSp3(sp3Day176Bytes);
const nav = loadRinexNav(navBytes);

// Synthesize a self-consistent GPS observation set at one DOY177 SP3 epoch for a
// receiver at the ESBC station (Denmark), keeping satellites above a 10-degree
// elevation mask.
function scenario() {
  const sp3 = sp3Day177();
  const epochs = sp3.epochsJ2000Seconds();
  const tRx = epochs[12]; // ~1 h into DOY177, well inside the broadcast fit window
  const rx = geodeticToEcef(55.69, 12.43, 50.0);
  const rxRadius = norm3(rx);
  const up = rx.map((c) => c / rxRadius);

  const observations = [];
  for (const sat of sp3.satellites.filter((s) => s.startsWith("G"))) {
    const interp = sp3.interpolate(sat, Float64Array.of(tRx));
    const p = interp.positionM;
    const dtSat = interp.clockS[0];
    if (!Number.isFinite(p[0]) || !Number.isFinite(dtSat)) continue;
    const los = sub3(p, rx);
    const range = norm3(los);
    const elDeg =
      (Math.asin((los[0] * up[0] + los[1] * up[1] + los[2] * up[2]) / range) * 180) / Math.PI;
    if (elDeg < 10) continue;
    observations.push({ satelliteId: sat, pseudorangeM: range - C_M_S * dtSat });
  }

  return {
    rx,
    request: {
      observations,
      tRxJ2000S: tRx,
      tRxSecondOfDayS: 0,
      dayOfYear: 177,
      initialGuess: [rx[0], rx[1], rx[2], 0],
      corrections: { ionosphere: false, troposphere: false },
      withGeodetic: true,
    },
  };
}

// Full bit-for-bit equality of a fallback solution against a direct solve.
function assertSolutionBitsEq(a, b) {
  for (let i = 0; i < 3; i++) {
    assert.equal(a.positionM[i], b.positionM[i], `positionM[${i}] bit-equal`);
  }
  assert.equal(a.rxClockS, b.rxClockS, "rxClockS bit-equal");
  assert.deepEqual(Array.from(a.residualsM), Array.from(b.residualsM), "residuals bit-equal");
  assert.deepEqual(a.usedSats, b.usedSats, "usedSats equal");
}

test("solveBroadcast solves a position from broadcast ephemeris alone", () => {
  const { rx, request } = scenario();
  assert.ok(request.observations.length >= 5, "a redundant GPS set was synthesized");

  const sol = nav.solveBroadcast(request);
  assert.ok(sol.usedSats.length >= 4, "solved with at least four satellites");
  for (const s of sol.usedSats) assert.ok(s.startsWith("G"), `${s} is GPS`);
  // The synthesized pseudoranges neglect light-time / Sagnac, so the recovered
  // position sits a few tens of metres from the geometric synthesis point (the
  // same simplification the GLONASS SPP test documents); a loose bound confirms
  // the broadcast-only solve converges to a sane Earth-surface fix.
  const err = norm3(sub3(Array.from(sol.positionM), rx));
  assert.ok(
    Number.isFinite(err) && err < 2000,
    `broadcast fix within ${err.toFixed(1)} m of truth`,
  );
});

test("fallback uses a precise product that covers the epoch, reporting an exact precise source", () => {
  const { request } = scenario();

  const precise = sp3Day177().solveSpp(request);
  const sourced = solveWithFallback([sp3Day177()], nav, request, { maxStalenessDays: 3 });

  const source = sourced.source;
  assert.equal(source.kind, "precise");
  assert.equal(source.isPrecise, true);
  assert.equal(source.isPreciseExact, true);
  assert.equal(source.isBroadcast, false);
  assert.equal(source.broadcastReason, null);
  assert.equal(source.staleness.kind, "exact");
  assert.equal(source.staleness.stalenessS, 0);

  // The precise-present path changes no output bit versus solving the SP3 directly.
  assertSolutionBitsEq(sourced.solution, precise);
});

test("fallback drops to broadcast when no precise product is supplied, naming the reason", () => {
  const { request } = scenario();

  const broadcast = nav.solveBroadcast(request);
  const sourced = solveWithFallback([], nav, request, { maxStalenessDays: 3 });

  const source = sourced.source;
  assert.equal(source.kind, "broadcast");
  assert.equal(source.isBroadcast, true);
  assert.equal(source.isPrecise, false);
  assert.equal(source.staleness, null);
  assert.equal(source.broadcastReason.kind, "preciseUnavailable");
  assert.equal(source.broadcastReason.selectionError.name, "EmptyProductSet");

  // The broadcast fix is bit-for-bit the broadcast-only solve.
  assertSolutionBitsEq(sourced.solution, broadcast);
});

test("fallback drops to broadcast when the nearest precise product is beyond the cap", () => {
  const { request } = scenario();

  const broadcast = nav.solveBroadcast(request);
  // The only precise product is the prior day (DOY176); a 60 s cap puts it beyond
  // reach, so the staleness layer declines and broadcast produces the fix.
  const sourced = solveWithFallback([sp3Day176()], nav, request, { maxStalenessS: 60 });

  const source = sourced.source;
  assert.equal(source.kind, "broadcast");
  assert.equal(source.broadcastReason.kind, "preciseUnavailable");
  assert.equal(source.broadcastReason.selectionError.name, "BeyondStalenessCap");
  assert.equal(source.broadcastReason.selectionError.maxStalenessS, 60);
  assert.ok(
    source.broadcastReason.selectionError.stalenessS > 60,
    "the rejection carries the staleness that exceeded the cap",
  );

  assertSolutionBitsEq(sourced.solution, broadcast);
});

test("fallback drops to broadcast when a within-cap precise product cannot reach the epoch, carrying the degraded reason", () => {
  const { request } = scenario();
  const priorLast = sp3Day176().epochsJ2000Seconds().at(-1);

  const broadcast = nav.solveBroadcast(request);
  // DOY176 is the only precise product and a 3-day cap admits it as the
  // most-recent prior, but its coverage ends ~a day before the DOY177 request
  // epoch, so the selected precise product is unusable for this epoch and the
  // fallback degrades to broadcast WITHOUT discarding the provenance.
  const sourced = solveWithFallback([sp3Day176()], nav, request, { maxStalenessDays: 3 });

  const source = sourced.source;
  assert.equal(source.kind, "broadcast");
  assert.equal(source.staleness, null);
  assert.equal(source.broadcastReason.kind, "preciseDegradedUnusable");
  // The staleness of the precise product that was TRIED is surfaced, not dropped.
  assert.equal(source.broadcastReason.selectionError, null);
  assert.equal(source.broadcastReason.attemptedStaleness.kind, "nearestPrior");
  assert.equal(source.broadcastReason.attemptedStaleness.sourceEpochJ2000S, priorLast);
  assert.equal(source.broadcastReason.attemptedStaleness.stalenessS, request.tRxJ2000S - priorLast);
  assert.ok(
    typeof source.broadcastReason.preciseError === "string" &&
      source.broadcastReason.preciseError.length > 0,
    "the precise solve error that triggered the fallback is reported",
  );

  assertSolutionBitsEq(sourced.solution, broadcast);
});

test("the broadcast and precise fixes are genuinely different solutions", () => {
  const { request } = scenario();

  const precise = solveWithFallback([sp3Day177()], nav, request, { maxStalenessDays: 3 });
  const broadcast = solveWithFallback([], nav, request, { maxStalenessDays: 3 });

  assert.equal(precise.source.kind, "precise");
  assert.equal(broadcast.source.kind, "broadcast");

  const delta = norm3(
    sub3(Array.from(precise.solution.positionM), Array.from(broadcast.solution.positionM)),
  );
  // A degenerate/zeroed broadcast source would collapse the two fixes onto each
  // other; a real broadcast-vs-precise pair differs by the signal-in-space error.
  assert.ok(delta > 0.01, `broadcast and precise differ (${delta.toFixed(3)} m)`);
  assert.ok(delta < 50, `the difference is the labeled SIS-level delta (${delta.toFixed(3)} m)`);
});
