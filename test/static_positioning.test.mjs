import { test } from "node:test";
import assert from "node:assert/strict";

import { loadSp3, solveStatic } from "../pkg-node/sidereon.js";
import { fixture, f64Bits, geodeticToEcef, synthSp3Pseudoranges } from "./helpers.mjs";

function staticRequests(sp3) {
  const rx = geodeticToEcef(48.0, 11.0, 600.0);
  const epochs = sp3.epochsJ2000Seconds();
  return [40, 44, 48, 52].map((index) => ({
    observations: synthSp3Pseudoranges(sp3, epochs[index], rx, 0.0),
    tRxJ2000S: epochs[index],
    tRxSecondOfDayS: 43200,
    dayOfYear: 176,
    initialGuess: [...rx, 0.0],
    corrections: { ionosphere: false, troposphere: false },
    withGeodetic: true,
  }));
}

test("static positioning solve exposes core result surfaces", () => {
  const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const requests = staticRequests(sp3);
  const solution = sp3.solveStatic(requests, { withGeodetic: true });
  const free = solveStatic(sp3, requests, { withGeodetic: true });

  assert.deepEqual(Array.from(solution.positionM, f64Bits), [
    0x4150032cbb555f3bn,
    0x4128e6640c77806cn,
    0x4151fec28de3f2a6n,
  ]);
  assert.deepEqual(Array.from(free.positionM, f64Bits), Array.from(solution.positionM, f64Bits));
  assert.deepEqual(Array.from(solution.geodetic, f64Bits), [
    0x3feacee9f36ef549n,
    0x3fc893011f319040n,
    0x4082c0001cccbf34n,
  ]);
  assert.equal(f64Bits(solution.residualRmsM), 0x3efe05a7066b423en);
  assert.equal(solution.stateParameterCount, 7);
  assert.equal(solution.stateCovarianceM2.length, 49);
  assert.deepEqual(Array.from(solution.positionCovarianceEcefM2, f64Bits), [
    0x3ff55bd52bdd8efdn,
    0x3fc766b325a0b0b9n,
    0x3febbd2a000f1967n,
    0x3fc766b325a0b0b9n,
    0x3fd7b68301542806n,
    0x3fc72062e773124an,
    0x3febbd2a000f1967n,
    0x3fc72062e773124an,
    0x3ff56cf262816bafn,
  ]);
  assert.deepEqual(Array.from(solution.positionCovarianceEnuM2, f64Bits), [
    0x3fd593b616f2ccfan,
    0x3f90a8b726bc41fbn,
    0x3f5793a2295b7ac8n,
    0x3f90a8b726bc41fcn,
    0x3fde61340ba36127n,
    0xbfbb68b32a1d0fc6n,
    0x3f5793a2295b7c00n,
    0xbfbb68b32a1d0fb0n,
    0x4001dc96e3073c93n,
  ]);

  assert.deepEqual(
    solution.usedSats.map((epoch) => epoch.length),
    [8, 8, 8, 9],
  );
  assert.equal(solution.residuals.length, 33);
  assert.equal(solution.perEpochClocks.length, 4);
  assert.deepEqual(solution.metadata, {
    iterations: 2,
    converged: true,
    status: "StepTolerance",
    outerIterations: 0,
    finalRobustScaleM: null,
    usedMeasurements: 33,
    nParameters: 7,
    redundancy: 26,
  });
  assert.ok(solution.perEpochInfluence.length > 0);
  assert.ok(solution.perSatelliteInfluence.length > 0);
  assert.ok(solution.perSatelliteBatchInfluence.length > 0);
});
