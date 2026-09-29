// Code-differential GNSS (DGPS) through the WASM binding.
//
// All correction/apply/solve math is sidereon-core's `dgnss`; these tests prove
// the binding wires it correctly with a real end-to-end solve synthesized from
// the committed multi-GNSS SP3 product:
//   (a) a base station's per-satellite pseudorange corrections recover an
//       injected common per-satellite error one-for-one, to within the change
//       the error makes to the modeled range (core PRC),
//   (b) apply() pairs corrections to rover observations and reports the unmatched,
//   (c) a full DGNSS rover solve cancels the injected common error, to within
//       a bound propagated through the solve geometry, against the no-error
//       solve and recovers the rover position, far better
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

/** G = (H^T W H)^-1 H^T W for an n-by-4 H and weights w. */
function weightedPseudoInverse(H, w) {
  const n = H.length;
  const N = [0, 1, 2, 3].map((r) =>
    [0, 1, 2, 3].map((c) => H.reduce((acc, row, i) => acc + row[r] * w[i] * row[c], 0)),
  );
  // Gauss-Jordan inverse of the 4x4 normal matrix.
  const A = N.map((row, r) => [...row, ...[0, 1, 2, 3].map((c) => (c === r ? 1 : 0))]);
  for (let col = 0; col < 4; col++) {
    let pivot = col;
    for (let r = col + 1; r < 4; r++) if (Math.abs(A[r][col]) > Math.abs(A[pivot][col])) pivot = r;
    [A[col], A[pivot]] = [A[pivot], A[col]];
    const p = A[col][col];
    for (let c = 0; c < 8; c++) A[col][c] /= p;
    for (let r = 0; r < 4; r++) {
      if (r === col) continue;
      const f = A[r][col];
      for (let c = 0; c < 8; c++) A[r][c] -= f * A[col][c];
    }
  }
  const Ninv = A.map((row) => row.slice(4));
  return [0, 1, 2, 3].map((r) =>
    Array.from({ length: n }, (_, i) =>
      [0, 1, 2, 3].reduce((acc, c) => acc + Ninv[r][c] * H[i][c] * w[i], 0),
    ),
  );
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

// Bounds on how far an injected pseudorange error e moves the engine's model.
//
// Each satellite is placed at the transmission epoch its pseudorange states
// (RTKLIB satposs), t_tx = t_rx - P / c - dts, so e moves that epoch by e / c.
// The engine holds t_tx in J2000 seconds, about 6.5e8 here, where one unit in
// the last place is 1.19e-7 s, more than e / c for |e| below 35 m; each
// evaluation of t_tx is within one unit of the exact value, so the two
// placements differ by at most |e| / c + 2 ulp(t_tx). The modeled range moves
// by the rate of the geometric range plus Sagnac term times that, and the
// satellite clock term by c * (clock rate) times it. A satellite seen from a
// static receiver closes or recedes at under 950 m/s (the +/-5 kHz L1 Doppler
// of a static receiver); RANGE_RATE_MAX_M_S states 1000 m/s. A GPS clock
// drifts under 1e-10 s/s; CLOCK_RATE_MAX states 1e-9. Each modeled range is
// also rounded, a few units in the last place of the pseudorange:
// ROUNDING_ULPS of them.
const RANGE_RATE_MAX_M_S = 1000.0;
const CLOCK_RATE_MAX = 1.0e-9;
const ROUNDING_ULPS = 4;
const ulp = (x) => 2 ** (Math.floor(Math.log2(Math.abs(x))) - 52);
/** The largest change `e` makes to the modeled range of a pseudorange `p` received at `tRx`. */
const modelShiftBoundM = (e, p, tRx) =>
  (RANGE_RATE_MAX_M_S + C_M_S * CLOCK_RATE_MAX) * (Math.abs(e) / C_M_S + 2 * ulp(tRx)) +
  ROUNDING_ULPS * ulp(p);

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
  // The correction carries e less the change e makes to the modeled range.
  clean.forEach((o, i) => {
    const bound = modelShiftBoundM(err(i), o.pseudorangeM, tRx);
    const got = Math.abs(m1[o.satelliteId] - m0[o.satelliteId] - err(i));
    assert.ok(got <= bound, `${o.satelliteId}: ${got} exceeds ${bound}`);
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

  // The injected common error is identical on base and rover, so the
  // correction removes it. What remains is the change e makes to the modeled
  // range of satellite i at the base (through the correction) and at the rover:
  // at most modelShiftBoundM at each, their sum d_i on the corrected residual.
  //
  // The rover solve is a least-squares fit with the weights w_i = sin^2(el_i)
  // at the initial guess (spp: SIGMA0_M = 1). To first order a residual change
  // delta moves the state [x, y, z, c dtr] by G delta, G = (H^T W H)^-1 H^T W,
  // H_i = [-u_i, 1] with u_i the unit line of sight. So
  //   |dx_k| <= sum_i |G_ki| d_i.
  // H is formed here from the solved position and the SP3 positions at the
  // reception epoch, which differ from the solver's transmit-epoch geometry by
  // under 1e-4 in relative terms; GEOMETRY_MARGIN allows 1e-3. Each solve stops
  // at the relative step tolerance 1e-14 (spp: SPP_SOLVER_XTOL), so each state
  // component also carries up to 1e-14 |x| per solve.
  const GEOMETRY_MARGIN = 1.0e-3;
  const XTOL = 1.0e-14;
  const used = dgClean.usedSats;
  const errIndex = Object.fromEntries(roverClean.map((o, i) => [o.satelliteId, i]));
  const basePr = Object.fromEntries(baseClean.map((o) => [o.satelliteId, o.pseudorangeM]));
  const roverPr = Object.fromEntries(roverClean.map((o) => [o.satelliteId, o.pseudorangeM]));
  const x = Array.from(dgClean.positionM);
  const radius = norm3(rover);
  const up = rover.map((c) => c / radius);
  const H = [];
  const w = [];
  const d = [];
  for (const sat of used) {
    const p = sp3.interpolate(sat, Float64Array.of(tRx)).positionM;
    const los = sub3(Array.from(p), x);
    const range = norm3(los);
    const u = los.map((c) => c / range);
    H.push([-u[0], -u[1], -u[2], 1]);
    const guessLos = sub3(Array.from(p), rover);
    const sinEl =
      (guessLos[0] * up[0] + guessLos[1] * up[1] + guessLos[2] * up[2]) / norm3(guessLos);
    w.push(sinEl * sinEl);
    const e = err(errIndex[sat]);
    d.push(modelShiftBoundM(e, basePr[sat], tRx) + modelShiftBoundM(e, roverPr[sat], tRx));
  }
  const G = weightedPseudoInverse(H, w);
  const bound = G.map((row) => row.reduce((acc, g, i) => acc + Math.abs(g) * d[i], 0));
  const clockM = dgClean.rxClockS * C_M_S;
  const stateNorm = Math.hypot(...x, clockM);
  const allowance = (k) => (1 + GEOMETRY_MARGIN) * bound[k] + 2 * XTOL * stateNorm;
  Array.from(dgErr.positionM).forEach((v, k) => {
    const got = Math.abs(v - dgClean.positionM[k]);
    assert.ok(got <= allowance(k), `position[${k}]: ${got} exceeds ${allowance(k)}`);
  });
  const clockGot = Math.abs(dgErr.rxClockS - dgClean.rxClockS) * C_M_S;
  assert.ok(clockGot <= allowance(3), `clock: ${clockGot} m exceeds ${allowance(3)} m`);

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

test("a public DGNSS solve retains the nested SPP refusal", async () => {
  const sp3 = await loadFixtureSp3();
  const { tRx, base } = scenario(sp3);
  const baseObservations = synth(sp3, tRx, base, RX_CLOCK_BASE);
  const roverObservations = baseObservations.slice(0, 1);

  assert.throws(
    () =>
      sp3.dgnssSolve({
        basePositionM: base,
        baseObservations,
        roverObservations,
        tRxJ2000S: tRx,
        tRxSecondOfDayS: 3600.0,
        dayOfYear: 176.0,
        initialGuess: [...base, 0.0],
      }),
    (error) => {
      assert.equal(error.name, "PositioningError");
      assert.equal(
        error.message,
        "only 1 usable satellites; need at least 4 (3 position + 1 clock per GNSS)",
      );
      assert.deepEqual(error.detail, {
        kind: "TOO_FEW_SATELLITES",
        message: error.message,
        used: 1,
        required: 4,
      });
      return true;
    },
  );
});
