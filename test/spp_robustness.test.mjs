// The SPP robustness + integrity surface on `Sp3.solveSpp`: the opt-in
// Huber/IRLS reweighting (`robust`), the cold-start coarse search
// (`coarseSearchSeeds`), and the PDOP validation ceiling (`maxPdop`). Each knob
// delegates to a sidereon-core entry point: `robust` to `SolveInputs.robust`,
// `coarseSearchSeeds` / `maxPdop` to `SolvePolicy`. FDE (`Sp3.fde`) and
// `solveWithFallback` have their own suites (qc_fde, broadcast_fallback); this
// file proves the remaining levers the Elixir interface exposes.
//
// Observations are synthesized with the light-time iteration and Sagnac rotation
// the core solver itself models, so a clean set is consistent to ~mm and the
// robust path is byte-identical to the static path on it.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { loadSp3 } from "../pkg-node/sidereon.js";

const here = (rel) => fileURLToPath(new URL(rel, import.meta.url));
const C_M_S = 299792458.0;
const OMEGA_E = 7.2921151467e-5;
const norm3 = (a) => Math.hypot(a[0], a[1], a[2]);
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

async function loadFixtureSp3() {
  return loadSp3(await readFile(here("./fixtures/GRG0MGXFIN_20201760000_01D_15M_ORB.SP3")));
}

function synth(sp3, tRx, rx, rxClockS = 0) {
  const rxRadius = norm3(rx);
  const up = rx.map((c) => c / rxRadius);
  const out = [];
  for (const sat of sp3.satellites.filter((s) => s.startsWith("G"))) {
    let dtFlight = 0.075;
    let p;
    let dtSat;
    let range = 0;
    for (let it = 0; it < 4; it++) {
      const tTx = tRx - dtFlight;
      const interp = sp3.interpolate(sat, Float64Array.of(tTx));
      const raw = interp.positionM;
      dtSat = interp.clockS[0];
      if (!Number.isFinite(raw[0]) || !Number.isFinite(dtSat)) {
        p = null;
        break;
      }
      const theta = OMEGA_E * dtFlight;
      p = [
        raw[0] * Math.cos(theta) + raw[1] * Math.sin(theta),
        -raw[0] * Math.sin(theta) + raw[1] * Math.cos(theta),
        raw[2],
      ];
      range = norm3(sub3(p, rx));
      dtFlight = range / C_M_S;
    }
    if (!p) continue;
    const los = sub3(p, rx);
    const elDeg =
      (Math.asin((los[0] * up[0] + los[1] * up[1] + los[2] * up[2]) / range) * 180) / Math.PI;
    if (elDeg < 10) continue;
    out.push({ satelliteId: sat, pseudorangeM: range + C_M_S * (rxClockS - dtSat) });
  }
  return out;
}

function scenario(sp3) {
  const tRx = sp3.epochsJ2000Seconds()[48];
  const rx = geodeticToEcef(48.0, 11.0, 600.0);
  const observations = synth(sp3, tRx, rx, 0.0);
  const request = {
    observations,
    tRxJ2000S: tRx,
    tRxSecondOfDayS: 43200,
    dayOfYear: 176,
    initialGuess: [...rx, 0.0],
    corrections: { ionosphere: false, troposphere: false },
    withGeodetic: true,
  };
  return { rx, request };
}

test("robust on a clean set converges to the static reference fix", async () => {
  const sp3 = await loadFixtureSp3();
  const { request } = scenario(sp3);

  const staticSol = sp3.solveSpp(request);
  // On a clean, self-consistent set the IRLS outer loop reweights tiny residuals
  // and resolves, so it lands on the static elevation-weighted fix to within
  // sub-micrometre, not bit-for-bit (bit-equality holds only for the default
  // no-robust path). Both engine-default (robust: {}) and explicit tuning agree.
  for (const robust of [{}, { huberK: 1.5, scaleFloorM: 3.0, maxOuter: 8, outerTolM: 1e-5 }]) {
    const robustSol = sp3.solveSpp({ ...request, robust });
    const drift = norm3(sub3(Array.from(robustSol.positionM), Array.from(staticSol.positionM)));
    assert.ok(drift < 1e-6, `robust clean-set fix agrees within ${drift.toExponential(2)} m`);
    assert.equal(robustSol.usedSats.length, staticSol.usedSats.length, "same satellite count");
    assert.deepEqual(robustSol.usedSats, staticSol.usedSats, "usedSats equal");
  }
});

test("robust down-weights an outlier and beats the static solve", async () => {
  const sp3 = await loadFixtureSp3();
  const { rx, request } = scenario(sp3);

  // A moderate 80 m bias on one satellite: large enough to drag the static fix,
  // the regime where Huber reweighting (keep-all, down-weight) improves the fix.
  const faultIdx = Math.floor(request.observations.length / 3);
  const faulted = request.observations.map((o, i) =>
    i === faultIdx ? { ...o, pseudorangeM: o.pseudorangeM + 80.0 } : o,
  );

  const staticSol = sp3.solveSpp({ ...request, observations: faulted });
  const robustSol = sp3.solveSpp({ ...request, observations: faulted, robust: {} });

  const staticErr = norm3(sub3(Array.from(staticSol.positionM), rx));
  const robustErr = norm3(sub3(Array.from(robustSol.positionM), rx));

  // Robust keeps every satellite (it reweights, it does not exclude).
  assert.equal(robustSol.usedSats.length, faulted.length, "robust keeps all satellites");
  assert.ok(staticErr > 5.0, `static solve is dragged by the outlier (${staticErr.toFixed(1)} m)`);
  assert.ok(
    robustErr < staticErr,
    `robust (${robustErr.toFixed(2)} m) beats static (${staticErr.toFixed(2)} m)`,
  );
});

test("a malformed robust tuning value is rejected as a RangeError", async () => {
  const sp3 = await loadFixtureSp3();
  const { request } = scenario(sp3);
  assert.throws(() => sp3.solveSpp({ ...request, robust: { huberK: -1 } }), RangeError);
  assert.throws(() => sp3.solveSpp({ ...request, robust: { scaleFloorM: 0 } }), RangeError);
  assert.throws(() => sp3.solveSpp({ ...request, robust: { maxOuter: 0 } }), RangeError);
  assert.throws(() => sp3.solveSpp({ ...request, robust: { outerTolM: Infinity } }), RangeError);
});

test("coarseSearchSeeds rescues a starved antipodal cold start", async () => {
  const sp3 = await loadFixtureSp3();
  const { rx, request } = scenario(sp3);

  // An antipodal seed freezes the elevation mask where every satellite is below
  // the horizon, so the single static solve starves (no usable satellites).
  const cold = { ...request, initialGuess: [-rx[0], -rx[1], -rx[2], 0] };
  assert.throws(() => sp3.solveSpp(cold), "antipodal cold start without coarse search starves");

  // The golden-spiral seed lattice lands a seed in the convergence basin and
  // selects the best redundant converged fix, recovering the truth.
  const warm = sp3.solveSpp({ ...cold, coarseSearchSeeds: 24 });
  const warmErr = norm3(sub3(Array.from(warm.positionM), rx));
  assert.ok(warmErr < 1.0, `coarse-search fix recovered within ${warmErr.toFixed(4)} m`);
});

test("coarseSearchSeeds below one is rejected as a RangeError", async () => {
  const sp3 = await loadFixtureSp3();
  const { request } = scenario(sp3);
  assert.throws(() => sp3.solveSpp({ ...request, coarseSearchSeeds: 0 }), RangeError);
});

test("maxPdop refuses a fix whose geometry exceeds the ceiling", async () => {
  const sp3 = await loadFixtureSp3();
  const { request } = scenario(sp3);

  // A generous ceiling admits the well-conditioned full set; a ceiling below the
  // achieved PDOP refuses the fix through the core SolvePolicy validation gate.
  const ok = sp3.solveSpp({ ...request, maxPdop: 100.0 });
  const pdop = ok.dop.pdop;
  assert.ok(Number.isFinite(pdop) && pdop > 0, `solved geometry has PDOP ${pdop}`);

  assert.throws(
    () => sp3.solveSpp({ ...request, maxPdop: pdop / 2 }),
    `a ceiling below PDOP ${pdop} is refused`,
  );

  // A non-positive ceiling is a boundary RangeError, not a solver failure.
  assert.throws(() => sp3.solveSpp({ ...request, maxPdop: 0 }), RangeError);
  assert.throws(() => sp3.solveSpp({ ...request, maxPdop: -1 }), RangeError);
});
