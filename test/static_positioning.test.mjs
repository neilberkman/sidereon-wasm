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
    0x3ff55bd517465463n,
    0x3fc766b35c17b0e5n,
    0x3febbd29eb8c8c0cn,
    0x3fc766b35c17b0e5n,
    0x3fd7b6831a525d22n,
    0x3fc72062921d7cdan,
    0x3febbd29eb8c8c0cn,
    0x3fc72062921d7cdan,
    0x3ff56cf25e0ef234n,
  ]);
  assert.deepEqual(Array.from(solution.positionCovarianceEnuM2, f64Bits), [
    0x3fd593b621d4d4a2n,
    0x3f90a8b39ecf521dn,
    0x3f5793a733f563d8n,
    0x3f90a8b39ecf5220n,
    0x3fde61340e1f516an,
    0xbfbb68b2b17f83fan,
    0x3f5793a733f56400n,
    0xbfbb68b2b17f8400n,
    0x4001dc96d7f66a2en,
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
