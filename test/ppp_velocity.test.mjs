import { test } from "node:test";
import assert from "node:assert/strict";

import {
  predictRangeRateMS,
  solveVelocityLeastSquares,
  stationaryReceiverVelocityState,
} from "../pkg-node/sidereon.js";

const row = (satelliteId, satellitePositionM, measuredRangeRateMS) => ({
  satelliteId,
  satellitePositionM,
  satelliteVelocityMS: [0, 0, 0],
  measuredRangeRateMS,
  sigmaMS: 0.1,
  satelliteClockDriftMS: 0,
});

test("range-rate prediction and stationary state expose exact core values", () => {
  assert.deepEqual(stationaryReceiverVelocityState([1, 2, 3]), {
    positionM: [1, 2, 3],
    velocityMS: [0, 0, 0],
    clockDriftMS: 0,
  });
  assert.deepEqual(
    predictRangeRateMS(
      {
        satelliteId: "G01",
        satellitePositionM: [20_200_000, 0, 0],
        satelliteVelocityMS: [2, 3, 4],
        measuredRangeRateMS: 0,
        sigmaMS: 0.1,
        satelliteClockDriftMS: 0.2,
      },
      { positionM: [0, 0, 0], velocityMS: [0, 0, 0], clockDriftMS: 0.5 },
    ),
    { losUnit: [1, 0, 0], rangeRateMS: 2.3 },
  );
  assert.equal(
    predictRangeRateMS(
      {
        satelliteId: "G01",
        satellitePositionM: [0, 0, 0],
        satelliteVelocityMS: [0, 0, 0],
        measuredRangeRateMS: 0,
        sigmaMS: 1,
        satelliteClockDriftMS: 0,
      },
      stationaryReceiverVelocityState([0, 0, 0]),
    ),
    undefined,
  );
});

test("velocity least squares recovers the hand-checked state", () => {
  const solution = solveVelocityLeastSquares(
    [
      row("G01", [20_000_000, 0, 0], -2.75),
      row("G02", [0, 20_000_000, 0], 2.25),
      row("G03", [0, 0, 20_000_000], -0.25),
      row("G04", [-20_000_000, -20_000_000, -20_000_000], 1.1160254037844386),
    ],
    [0, 0, 0],
    undefined,
  );
  assert.ok(Math.abs(solution.velocityMS[0] - 3) < 1e-12);
  assert.ok(Math.abs(solution.velocityMS[1] + 2) < 1e-12);
  assert.ok(Math.abs(solution.velocityMS[2] - 0.5) < 1e-12);
  assert.ok(Math.abs(solution.clockDriftMS - 0.25) < 1e-12);
  assert.ok(solution.residualRmsMS < 1e-12);
  assert.deepEqual(solution.usedSats, ["G01", "G02", "G03", "G04"]);
  assert.deepEqual(solution.downweightedSats, []);
  assert.equal(solution.robustIterations, 0);
  assert.equal(solution.robustScaleMS, null);
});

test("velocity input and solver refusals keep exact details", () => {
  assert.throws(
    () => predictRangeRateMS({ satelliteId: "not-a-sat" }, {}),
    (error) => error instanceof TypeError && /velocity observation/.test(error.message),
  );
  assert.throws(
    () => solveVelocityLeastSquares([], [0, 0, 0], undefined),
    (error) => {
      assert.deepEqual(error.details, {
        family: "VelocitySolveError",
        kind: "TOO_FEW_OBSERVATIONS",
        required: 4,
        actual: 0,
        message: "too few velocity observations: required 4, got 0",
      });
      return /required 4, got 0/.test(error.message);
    },
  );
});
