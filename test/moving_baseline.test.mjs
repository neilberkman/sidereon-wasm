// Moving-baseline RTK over sidereon_core::rtk_filter::moving_baseline. This
// mirrors the crate's own `recovers_baseline_per_epoch_as_base_moves` golden: a
// base receiver walks along a track while the rover holds a constant baseline,
// with perfect synthetic double-difference observations and known integer
// ambiguities. Each independent epoch must integer-fix and recover the true
// baseline, exercising the binding's per-epoch base + shared ambiguity-set
// marshalling and the warm-start sequence.

import { test } from "node:test";
import assert from "node:assert/strict";

import { solveMovingBaseline } from "../pkg-node/sidereon.js";

const C_M_S = 299792458.0;
const F_L1_HZ = 1575.42e6;
const LAMBDA = C_M_S / F_L1_HZ;

// Five well-spread satellites; G01 is the reference, G02..G05 carry the known
// integer ambiguities (cycles).
const SATS = [
  ["G01", [15000000.0, 7000000.0, 21000000.0], 0],
  ["G02", [-12000000.0, 18000000.0, 19000000.0], 4],
  ["G03", [20000000.0, -10000000.0, 17000000.0], -7],
  ["G04", [-19000000.0, -13000000.0, 20000000.0], 9],
  ["G05", [9000000.0, 22000000.0, 16000000.0], -3],
];

const range = (sat, recv) => Math.hypot(sat[0] - recv[0], sat[1] - recv[1], sat[2] - recv[2]);

// Build a perfect synthetic double-difference epoch for a base and a rover at
// base + baseline, with the fixed cycle biases.
function synthEpoch(base, baseline) {
  const rover = [base[0] + baseline[0], base[1] + baseline[1], base[2] + baseline[2]];
  const mk = ([id, pos, cycles]) => ({
    sat: id,
    sdAmbiguityId: id,
    baseCodeM: range(pos, base),
    basePhaseM: range(pos, base),
    roverCodeM: range(pos, rover),
    roverPhaseM: range(pos, rover) + cycles * LAMBDA,
    baseTxPos: pos,
    roverTxPos: pos,
    pos,
  });
  return {
    references: [mk(SATS[0])],
    nonref: SATS.slice(1).map(mk),
    dtS: 0.0,
  };
}

const AMBIGUITY_IDS = ["G02", "G03", "G04", "G05"];
const recordOf = (value) => Object.fromEntries(AMBIGUITY_IDS.map((id) => [id, value]));

const MODEL = {
  codeSigmaM: 0.3,
  phaseSigmaM: 0.003,
  sagnac: false,
  stochastic: "simple",
  elevationWeighting: false,
};

const BASES = [
  [4075580.0, 931854.0, 4801568.0],
  [4075585.0, 931860.0, 4801572.0],
  [4075590.0, 931867.0, 4801575.0],
];
const TRUTH = [1.2, -0.85, 0.91];
const TOL = 1e-6;

test("moving-baseline integer-fixes and recovers the true baseline as the base moves", () => {
  const solutions = solveMovingBaseline({
    epochs: BASES.map((base) => ({
      basePositionM: base,
      ...synthEpoch(base, TRUTH),
    })),
    ambiguityIds: AMBIGUITY_IDS,
    ambiguitySatellites: Object.fromEntries(AMBIGUITY_IDS.map((id) => [id, id])),
    wavelengthsM: recordOf(LAMBDA),
    offsetsM: recordOf(0.0),
    model: MODEL,
    floatOptions: { positionTolM: 1e-3, ambiguityTolM: 1e-6, maxIterations: 10 },
    fixedOptions: {
      positionTolM: 1e-3,
      ambiguityTolM: 1e-6,
      maxIterations: 10,
      ratioThreshold: 3.0,
    },
    initialBaselineM: [-30.0, 25.0, -10.0],
    warmStart: true,
  });

  assert.equal(solutions.length, BASES.length);
  const truthLen = Math.hypot(...TRUTH);
  for (let i = 0; i < solutions.length; i++) {
    const sol = solutions[i];
    assert.equal(sol.status, "Fixed");
    assert.deepEqual(sol.basePositionM, Float64Array.from(BASES[i]));
    for (let k = 0; k < 3; k++) {
      assert.ok(
        Math.abs(sol.baselineM[k] - TRUTH[k]) <= TOL,
        `epoch ${i} baseline[${k}]: ${sol.baselineM[k]} vs ${TRUTH[k]}`,
      );
    }
    assert.ok(Math.abs(sol.baselineLengthM - truthLen) <= TOL);
    assert.ok(sol.floatConverged);
  }
});

test("moving-baseline solve rejects malformed input with a TypeError", () => {
  assert.throws(() => solveMovingBaseline({ epochs: "nope" }), TypeError);
});
