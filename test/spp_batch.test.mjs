// Batch SPP through the WASM binding. `Sp3.solveSppBatch(epochs, options)`
// delegates to the serial reference batch kernel
// `sidereon_core::spp::solve_spp_batch_serial`, whose element `i` is
// byte-for-byte identical to the single `solve_spp` on `epochs[i]` under the same
// shared `withGeodetic` flag and `SolvePolicy`. The binding never spawns the
// rayon pool (wasm is single-threaded), matching the serial-only rule the
// SGP4/look-angle batches already follow. Observations are synthesized with the
// light-time iteration and Sagnac rotation the core solver itself models, so the
// per-epoch fixes converge cleanly.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { loadSp3 } from "../pkg-node/sidereon.js";
import { f64Bits } from "./helpers.mjs";

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

async function loadFixtureSp3() {
  return loadSp3(await readFile(here("./fixtures/GRG0MGXFIN_20201760000_01D_15M_ORB.SP3")));
}

const rx = geodeticToEcef(48.0, 11.0, 600.0);

function requests(sp3, epochIndices) {
  const ts = sp3.epochsJ2000Seconds();
  return epochIndices.map((idx) => ({
    observations: synth(sp3, ts[idx], rx, 0.0),
    tRxJ2000S: ts[idx],
    tRxSecondOfDayS: 43200,
    dayOfYear: 176,
    initialGuess: [...rx, 0.0],
    corrections: { ionosphere: false, troposphere: false },
    withGeodetic: true,
  }));
}

test("solveSppBatch is bit-identical to per-epoch solveSpp", async () => {
  const sp3 = await loadFixtureSp3();
  const reqs = requests(sp3, [40, 44, 48, 52]);

  const batch = sp3.solveSppBatch(reqs, { withGeodetic: true });
  assert.equal(batch.count, reqs.length);

  for (let i = 0; i < reqs.length; i++) {
    assert.equal(batch.isOk(i), true, `epoch ${i} converged`);
    assert.equal(batch.error(i), undefined, `epoch ${i} has no error`);

    const single = sp3.solveSpp(reqs[i]);
    const fromBatch = batch.solution(i);

    Array.from(fromBatch.positionM).forEach((v, k) => {
      assert.equal(f64Bits(v), f64Bits(single.positionM[k]), `epoch ${i} position[${k}] bits`);
    });
    assert.equal(f64Bits(fromBatch.rxClockS), f64Bits(single.rxClockS), `epoch ${i} clock bits`);
    assert.deepEqual(fromBatch.usedSats, single.usedSats, `epoch ${i} usedSats`);
  }
});

test("solveSppBatch with no options defaults withGeodetic to true", async () => {
  const sp3 = await loadFixtureSp3();
  const reqs = requests(sp3, [48]);
  const batch = sp3.solveSppBatch(reqs);
  assert.equal(batch.count, 1);
  assert.equal(batch.isOk(0), true);
  assert.ok(batch.solution(0).geodetic, "geodetic populated by default");
});

test("a shared maxPdop ceiling surfaces a per-epoch error, not a throw", async () => {
  const sp3 = await loadFixtureSp3();
  const reqs = requests(sp3, [40, 48]);

  // A PDOP ceiling below 1 is unreachable, so every epoch fails its policy
  // validation; the batch records the failure per epoch rather than aborting.
  const batch = sp3.solveSppBatch(reqs, { maxPdop: 1e-6 });
  assert.equal(batch.count, reqs.length);
  for (let i = 0; i < reqs.length; i++) {
    assert.equal(batch.isOk(i), false, `epoch ${i} rejected`);
    assert.equal(typeof batch.error(i), "string", `epoch ${i} carries a message`);
    assert.throws(() => batch.solution(i), Error, `epoch ${i} solution() throws`);
  }
});

test("an out-of-range epoch index throws a RangeError", async () => {
  const sp3 = await loadFixtureSp3();
  const batch = sp3.solveSppBatch(requests(sp3, [48]));
  assert.throws(() => batch.isOk(5), RangeError);
  assert.throws(() => batch.error(5), RangeError);
  assert.throws(() => batch.solution(5), RangeError);
});

test("an empty batch yields a zero-count result", async () => {
  const sp3 = await loadFixtureSp3();
  const batch = sp3.solveSppBatch([], {});
  assert.equal(batch.count, 0);
  assert.throws(() => batch.solution(0), RangeError);
});
