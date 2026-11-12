import test from "node:test";
import assert from "node:assert/strict";

import {
  PppKinematicConfig,
  PppKinematicMotionModel,
  PppKinematicPositionProcessNoise,
  PppKinematicProcessNoise,
  PppKinematicState,
  predictKinematicState,
} from "../pkg-node/sidereon.js";

test("kinematic defaults expose exact core values and value equality", () => {
  const positionNoise = PppKinematicPositionProcessNoise.default();
  assert.equal(positionNoise.kind, "randomWalk");
  assert.equal(positionNoise.spectralDensityM2S, 1);
  assert.ok(positionNoise.equals(PppKinematicPositionProcessNoise.randomWalk(1)));
  const noise = PppKinematicProcessNoise.default();
  assert.equal(noise.clockWhiteM2S, 100);
  assert.equal(noise.ztdRandomWalkM2S, 1e-6);
  assert.equal(noise.ambiguityHoldM2S, 0);
  const config = PppKinematicConfig.default();
  assert.equal(config.motion.kind, "hold");
  assert.equal(config.newAmbiguityVarianceM2, 1e8);
  assert.ok(config.equals(PppKinematicConfig.default()));
});

test("kinematic prediction mutates core state and covariance exactly", () => {
  const state = PppKinematicState.default();
  const config = new PppKinematicConfig(
    undefined,
    PppKinematicMotionModel.constantVelocity(new Float64Array([1, -2, 0.5])),
    new PppKinematicProcessNoise(PppKinematicPositionProcessNoise.randomWalk(2), 3, 4, 5),
  );
  predictKinematicState(state, 2, ["G01"], config);
  assert.deepEqual(Array.from(state.positionM), [2, -4, 1]);
  assert.equal(state.dimension(), 6);
  assert.deepEqual(state.ambiguitiesM, { G01: 0 });
  assert.deepEqual(
    state.covarianceM2.map((row) => row.map((v) => v)),
    [
      [100000004, 0, 0, 0, 0, 0],
      [0, 100000004, 0, 0, 0, 0],
      [0, 0, 100000004, 0, 0, 0],
      [0, 0, 0, 100000006, 0, 0],
      [0, 0, 0, 0, 100000008, 0],
      [0, 0, 0, 0, 0, 100000010],
    ],
  );
});

test("kinematic prediction refusal preserves exact typed detail", () => {
  assert.throws(
    () => predictKinematicState(PppKinematicState.default(), -1, [], PppKinematicConfig.default()),
    (error) => {
      assert.deepEqual(error.details, {
        family: "KinematicSolveError",
        kind: "INVALID_INPUT",
        field: "kinematic PPP predict dt_s",
        reason: "must be non-negative",
        message: "invalid kinematic PPP input kinematic PPP predict dt_s: must be non-negative",
      });
      return true;
    },
  );
});
