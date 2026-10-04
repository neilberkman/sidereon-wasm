// Quality control: fault detection and exclusion (FDE) through the WASM binding.
//
// FDE math is sidereon-core's `quality::fde` (RAIM-gated exclusion loop around
// the SPP solver); these tests prove the binding wires it correctly with a real
// end-to-end solve synthesized from the committed SP3 product:
//   (a) a clean, self-consistent observation set passes RAIM with no exclusions
//       and recovers the receiver position,
//   (b) a single large pseudorange fault is detected, excluded by name, and the
//       surviving solution recovers the truth far better than the faulted solve,
//   (c) an out-of-range RAIM probability is rejected.
//
// Pseudoranges are synthesized with the light-time iteration and Sagnac rotation
// the core solver itself models, so the clean set is consistent to ~mm and RAIM
// has no false alarm to chew on.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { loadSp3 } from "../pkg-node/sidereon.js";
import { synthSp3Pseudoranges } from "./helpers.mjs";

const here = (rel) => fileURLToPath(new URL(rel, import.meta.url));
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

function scenario(sp3) {
  const tRx = sp3.epochsJ2000Seconds()[48];
  const rx = geodeticToEcef(48.0, 11.0, 600.0);
  // Self-consistent GPS pseudoranges (light time, Earth rotation and the
  // precise-clock relativistic term), so the SPP residuals are ~mm and RAIM
  // sees no fault.
  const observations = synthSp3Pseudoranges(sp3, tRx, rx, 0.0);
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

test("a clean set passes RAIM with no exclusions and recovers the position", async () => {
  const sp3 = await loadFixtureSp3();
  const { rx, request } = scenario(sp3);
  assert.ok(request.observations.length >= 6, "enough visible GPS satellites");

  const fde = sp3.fde(request);
  assert.deepEqual(fde.excluded, []);
  assert.equal(fde.iterations, 0);
  assert.equal(fde.raim.testable, true);
  assert.equal(fde.raim.faultDetected, false);
  assert.equal(Object.keys(fde.raim.normalizedResiduals).length, fde.usedSats.length);
  assert.deepEqual(fde.solution.usedSats, fde.usedSats);
  assert.deepEqual(Array.from(fde.solution.residualsM), Array.from(fde.residualsM));
  assert.equal(fde.solution.pseudorangeVariancesM2.length, fde.usedSats.length);
  assert.equal(fde.solution.weights.length, fde.usedSats.length);
  assert.equal(fde.solution.metadata.usedCount, fde.usedSats.length);

  const err = norm3(sub3(Array.from(fde.positionM), rx));
  assert.ok(err < 1.0, `clean FDE recovered within ${err.toFixed(4)} m`);
});

test("a single large fault is detected and excluded by name", async () => {
  const sp3 = await loadFixtureSp3();
  const { rx, request } = scenario(sp3);

  // Bias one mid-list satellite by 300 m; it becomes the RAIM-localizable fault.
  const faultIdx = Math.floor(request.observations.length / 2);
  const faulted = request.observations.map((o, i) =>
    i === faultIdx ? { ...o, pseudorangeM: o.pseudorangeM + 300.0 } : o,
  );
  const faultSat = request.observations[faultIdx].satelliteId;

  // The faulted absolute solve (no exclusion) is biased.
  const biased = sp3.solveSpp({ ...request, observations: faulted });
  const biasedErr = norm3(sub3(Array.from(biased.positionM), rx));
  assert.ok(biasedErr > 5.0, `faulted solve is biased (${biasedErr.toFixed(1)} m)`);

  // FDE excludes the faulted satellite and recovers the position.
  const fde = sp3.fde({ ...request, observations: faulted });
  assert.ok(fde.excluded.includes(faultSat), `excluded ${faultSat}`);
  assert.ok(!fde.usedSats.includes(faultSat), "faulted satellite is not in the solution");
  assert.deepEqual(fde.solution.usedSats, fde.usedSats);
  assert.equal(fde.raim.faultDetected, false);
  assert.equal(fde.raim.testable, true);

  const unitWeighted = sp3.fde({
    ...request,
    observations: faulted,
    weightsMode: "unit",
  });
  assert.ok(unitWeighted.excluded.includes(faultSat));

  const fdeErr = norm3(sub3(Array.from(fde.positionM), rx));
  assert.ok(fdeErr < biasedErr / 5.0, `FDE (${fdeErr.toFixed(2)} m) beats biased`);
});

test("the robust FDE driver accepts robust tuning and excludes the fault", async () => {
  const sp3 = await loadFixtureSp3();
  const { request } = scenario(sp3);
  const faultIdx = Math.floor(request.observations.length / 2);
  const faulted = request.observations.map((o, i) =>
    i === faultIdx ? { ...o, pseudorangeM: o.pseudorangeM + 300.0 } : o,
  );
  const faultSat = request.observations[faultIdx].satelliteId;

  const fde = sp3.sppRobustFdeDriver({
    ...request,
    observations: faulted,
    robust: { huberK: 1.5, scaleFloorM: 3.0, maxOuter: 8, outerTolM: 1e-5 },
  });
  assert.ok(fde.excluded.includes(faultSat), `excluded ${faultSat}`);
  assert.ok(!fde.usedSats.includes(faultSat));
  assert.ok(fde.positionM.every(Number.isFinite));
  const solution = fde.solution;
  assert.equal(solution.weights.length, solution.usedSats.length);
  assert.equal(solution.pseudorangeVariancesM2.length, solution.usedSats.length);
});

test("an out-of-range RAIM probability is rejected", async () => {
  const sp3 = await loadFixtureSp3();
  const { request } = scenario(sp3);
  for (const pFa of [1.5, 0, Infinity]) {
    assert.throws(
      () => sp3.fde({ ...request, pFa }),
      (error) => error instanceof RangeError && error.detail.kind === "INVALID_PROBABILITY",
    );
  }
  for (const nSystems of [4294967297, -4294967295]) {
    assert.throws(
      () => sp3.fde({ ...request, nSystems }),
      (error) => error instanceof RangeError && error.detail.kind === "INVALID_SYSTEM_COUNT",
    );
  }
});

test("FDE accepts the legacy empty weight list as explicit unit weights", async () => {
  const sp3 = await loadFixtureSp3();
  const { request } = scenario(sp3);
  const fde = sp3.fde({ ...request, weights: [] });
  assert.equal(fde.raim.faultDetected, false);
});

test("FDE accepts an empty explicit BySatellite map", async () => {
  const sp3 = await loadFixtureSp3();
  const { request } = scenario(sp3);
  const fde = sp3.fde({ ...request, weightsMode: "bySatellite", weights: [] });
  assert.equal(fde.raim.faultDetected, false);
});
