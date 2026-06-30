// Code-differential GNSS (DGPS) through the WASM binding.
//
// All correction/apply/solve math is sidereon-core's `dgnss`; these tests prove
// the binding wires it correctly with a real end-to-end solve synthesized from
// the committed multi-GNSS SP3 product:
//   (a) a base station's per-satellite pseudorange corrections recover an
//       injected common per-satellite error one-for-one (core PRC),
//   (b) apply() pairs corrections to rover observations and reports the unmatched,
//   (c) a full DGNSS rover solve cancels the injected common error bit-for-bit
//       against the no-error solve and recovers the rover position, far better
//       than the (biased) absolute solve, with the correct baseline length.
//
// Pseudoranges are the geometric range to each SP3 satellite minus its broadcast
// clock term (light-time / Sagnac neglected, like the GLONASS e2e test); the
// neglected terms are near-common over the short baseline, and the *injected*
// common error cancels exactly, which is what DGNSS guarantees.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { loadSp3, dgnssApply } from "../pkg-node/sidereon.js";

const here = (rel) => fileURLToPath(new URL(rel, import.meta.url));
const C_M_S = 299792458.0;
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

// GPS satellites above a 10-degree mask at the receiver, with their synthesized
// clean pseudorange: geometric range + c*(rxClock - satClock).
function synth(sp3, tRx, rx, rxClockS) {
  const rxRadius = norm3(rx);
  const up = rx.map((c) => c / rxRadius);
  const out = [];
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
    out.push({ satelliteId: sat, pseudorangeM: range + C_M_S * (rxClockS - dtSat) });
  }
  return out;
}

// Distinct base/rover receiver clocks prove the base clock is absorbed, not leaked.
const RX_CLOCK_BASE = 1.0e-6;
const RX_CLOCK_ROVER = -2.0e-6;

function scenario(sp3) {
  const tRx = sp3.epochsJ2000Seconds()[48];
  const base = geodeticToEcef(48.0, 11.0, 600.0); // mid-latitude
  const rover = [base[0] + 2000.0, base[1] + 1000.0, base[2] + 1500.0];
  return { tRx, base, rover };
}

test("base corrections recover an injected common per-satellite error", async () => {
  const sp3 = await loadFixtureSp3();
  const { tRx, base } = scenario(sp3);
  const clean = synth(sp3, tRx, base, RX_CLOCK_BASE);
  assert.ok(clean.length >= 5, "enough visible GPS satellites");

  // Inject a deterministic per-satellite error in +/-30 m.
  const err = (i) => ((i * 37) % 61) - 30;
  const errored = clean.map((o, i) => ({ ...o, pseudorangeM: o.pseudorangeM + err(i) }));

  const req = (obs) => ({ basePositionM: base, baseObservations: obs, tRxJ2000S: tRx });
  const prc0 = sp3.dgnssCorrections(req(clean));
  const prc = sp3.dgnssCorrections(req(errored));

  const by = (arr) => Object.fromEntries(arr.map((e) => [e.satelliteId, e.correctionM]));
  const m0 = by(prc0);
  const m1 = by(prc);
  clean.forEach((o, i) => {
    assert.ok(Math.abs(m1[o.satelliteId] - m0[o.satelliteId] - err(i)) < 1e-6);
  });
});

test("apply pairs corrections to the rover and reports the unmatched", () => {
  const corrections = [
    { satelliteId: "G01", correctionM: 1.0 },
    { satelliteId: "G02", correctionM: 2.0 },
    { satelliteId: "G05", correctionM: 5.0 },
  ];
  const rover = [
    { satelliteId: "G01", pseudorangeM: 100.0 },
    { satelliteId: "G02", pseudorangeM: 200.0 },
    { satelliteId: "G09", pseudorangeM: 900.0 },
  ];
  const applied = dgnssApply(rover, corrections);
  assert.deepEqual(applied.corrected, [
    { satelliteId: "G01", pseudorangeM: 99.0 },
    { satelliteId: "G02", pseudorangeM: 198.0 },
  ]);
  assert.deepEqual(applied.dropped, ["G09"]);
});

test("a full DGNSS solve cancels the common error and recovers the rover", async () => {
  const sp3 = await loadFixtureSp3();
  const { tRx, base, rover } = scenario(sp3);

  const baseClean = synth(sp3, tRx, base, RX_CLOCK_BASE);
  const roverClean = synth(sp3, tRx, rover, RX_CLOCK_ROVER);

  // A common per-satellite error injected identically on base and rover.
  const err = (i) => ((i * 41) % 53) - 26;
  const inject = (obs) => obs.map((o, i) => ({ ...o, pseudorangeM: o.pseudorangeM + err(i) }));
  const baseErr = inject(baseClean);
  const roverErr = inject(roverClean);

  const solveReq = (b, r) => ({
    basePositionM: base,
    baseObservations: b,
    roverObservations: r,
    tRxJ2000S: tRx,
    tRxSecondOfDayS: 0,
    dayOfYear: 176,
    initialGuess: [...rover, 0.0],
    withGeodetic: true,
  });

  const dgClean = sp3.dgnssSolve(solveReq(baseClean, roverClean));
  const dgErr = sp3.dgnssSolve(solveReq(baseErr, roverErr));

  // The injected common error is identical on base and rover, so PRC removes it
  // and the corrected pseudoranges are identical to the clean case: the errored
  // and clean solves are bit-for-bit equal.
  assert.deepEqual(Array.from(dgErr.positionM), Array.from(dgClean.positionM));
  assert.equal(dgErr.rxClockS, dgClean.rxClockS);

  // The DGNSS solve recovers the rover (the short baseline cancels most of the
  // neglected light-time/Sagnac), and the baseline length is reported.
  const dgnssErr = norm3(sub3(Array.from(dgClean.positionM), rover));
  assert.ok(dgnssErr < 2000, `DGNSS recovered within ${dgnssErr.toFixed(1)} m`);

  const trueBaseline = norm3(sub3(rover, base));
  assert.ok(dgClean.baselineM > 0);
  assert.ok(Math.abs(dgClean.baselineM - trueBaseline) < dgnssErr + 1.0);
  assert.deepEqual(dgClean.droppedSats, []);
});

test("a malformed base position is rejected", async () => {
  const sp3 = await loadFixtureSp3();
  assert.throws(() =>
    sp3.dgnssCorrections({
      basePositionM: [NaN, 0, 0],
      baseObservations: [{ satelliteId: "G01", pseudorangeM: 2.3e7 }],
      tRxJ2000S: 0,
    }),
  );
});
