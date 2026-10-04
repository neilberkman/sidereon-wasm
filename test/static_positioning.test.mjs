import { test } from "node:test";
import assert from "node:assert/strict";

import { loadSp3, solveStatic } from "../pkg-node/sidereon.js";
import {
  coreGoldens,
  fixture,
  f64Bits,
  geodeticToEcef,
  hexToF64,
  synthSp3Pseudoranges,
} from "./helpers.mjs";

// Inputs and expected values of the four-epoch static solve, reproduced
// natively by test/golden-gen.
const STATIC = coreGoldens().static;
const bits = (hexes) => hexes.map((h) => BigInt(h));

function staticRequests() {
  const rx = STATIC.receiverEcefM.map(hexToF64);
  return STATIC.epochs.map((epoch) => ({
    observations: epoch.observations.map((o) => ({
      satelliteId: o.satelliteId,
      pseudorangeM: hexToF64(o.pseudorangeM),
    })),
    tRxJ2000S: hexToF64(epoch.tRxJ2000S),
    tRxSecondOfDayS: 43200,
    dayOfYear: 176,
    initialGuess: [...rx, 0.0],
    corrections: { ionosphere: false, troposphere: false },
    withGeodetic: true,
  }));
}

test("static positioning solve exposes core result surfaces", () => {
  const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const requests = staticRequests();
  const solution = sp3.solveStatic(requests, { withGeodetic: true });
  const free = solveStatic(sp3, requests, { withGeodetic: true });

  assert.deepEqual(Array.from(solution.positionM, f64Bits), bits(STATIC.positionM));
  assert.deepEqual(Array.from(free.positionM, f64Bits), Array.from(solution.positionM, f64Bits));
  assert.deepEqual(Array.from(solution.geodetic, f64Bits), bits(STATIC.geodetic));
  assert.equal(f64Bits(solution.residualRmsM), BigInt(STATIC.residualRmsM));
  assert.equal(solution.stateParameterCount, STATIC.stateParameterCount);
  assert.equal(solution.stateCovarianceM2.length, STATIC.stateParameterCount ** 2);
  assert.deepEqual(
    Array.from(solution.positionCovarianceEcefM2, f64Bits),
    bits(STATIC.positionCovarianceEcefM2),
  );
  assert.deepEqual(
    Array.from(solution.positionCovarianceEnuM2, f64Bits),
    bits(STATIC.positionCovarianceEnuM2),
  );

  assert.deepEqual(
    solution.usedSats.map((epoch) => epoch.length),
    STATIC.usedSatCounts,
  );
  assert.equal(solution.residuals.length, STATIC.residualCount);
  assert.equal(solution.perEpochClocks.length, STATIC.perEpochClockCount);
  assert.deepEqual(solution.metadata, STATIC.metadata);
  assert.ok(solution.perEpochInfluence.length > 0);
  assert.ok(solution.perSatelliteInfluence.length > 0);
  assert.ok(solution.perSatelliteBatchInfluence.length > 0);
  // The synthesized receiver is recovered.
  STATIC.receiverEcefM.map(hexToF64).forEach((c, i) => {
    assert.ok(Math.abs(solution.positionM[i] - c) < 1e-2);
  });
});

test("static solve options expose QZSS clock and troposphere selectors", () => {
  const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const solution = solveStatic(sp3, staticRequests(), {
    qzssClock: "separate",
    troposphereModel: "saastamoinenNiell",
  });
  assert.ok(solution.positionM.every(Number.isFinite));
  assert.throws(() => sp3.solveStatic(staticRequests(), { qzssClock: "own" }), TypeError);
  assert.throws(() => solveStatic(sp3, staticRequests(), { troposphereModel: "niell" }), TypeError);
});

test("static epochs leave out a GLONASS satellite with no carrier and keep the rest", () => {
  const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const rx = geodeticToEcef(48.0, 11.0, 600.0);
  const epochs = sp3.epochsJ2000Seconds();
  const indices = [40, 44, 48, 52];
  // GLONASS above 20 degrees only, clear of the 10-degree mask, so selection
  // at the truth seed reaches the carrier test for each of them.
  const glonassByEpoch = indices.map((index) =>
    synthSp3Pseudoranges(sp3, epochs[index], rx, 0.0, 20, ["R"]),
  );
  const requests = indices.map((index, i) => ({
    observations: [...synthSp3Pseudoranges(sp3, epochs[index], rx, 0.0), ...glonassByEpoch[i]],
    tRxJ2000S: epochs[index],
    tRxSecondOfDayS: 43200,
    dayOfYear: 176,
    initialGuess: [...rx, 0.0],
    corrections: { ionosphere: true, troposphere: false },
    withGeodetic: true,
  }));
  assert.ok(
    glonassByEpoch.every((rows) => rows.length > 0),
    "GLONASS in view at every epoch",
  );

  const solution = sp3.solveStatic(requests, { withGeodetic: true });
  assert.equal(solution.rejectedSats.length, indices.length);
  solution.rejectedSats.forEach((rows, i) => {
    assert.deepEqual(
      rows
        .filter((row) => row.reason === "ionosphereCarrierUnresolved")
        .map((row) => row.satelliteId),
      glonassByEpoch[i].map((obs) => obs.satelliteId),
      `epoch ${i}`,
    );
  });
  for (const epoch of solution.usedSats) {
    for (const sat of epoch) assert.ok(sat.startsWith("G"), `${sat} is a GPS satellite`);
  }
});
