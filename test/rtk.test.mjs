// RTK float and validated-fixed solves through the WASM binding, mirroring
// sidereon-python/tests/test_rtk.py against the same committed golden
// (rtk_wtzr.json: the crate's validated WTZR/WTZZ static GPS RTK arc).
//
// The fixture stores the engine's reference baselines as shortest-round-trip
// decimals, so JSON.parse recovers the exact native f64. Python asserts
// bit-exact (np.array_equal); here the baseline components are bit-exact where
// the wasm32 libm agrees and otherwise within a tight 1e-6 m tolerance (a
// cross-libm ULP residual in the least-squares kernel, not a marshalling bug).

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  ObservabilityTier,
  observabilityTierLabel,
  solveRtkFloat,
  solveRtkFixed,
} from "../pkg-node/sidereon.js";
import { fixtureJson } from "./helpers.mjs";

const BASELINE_TOL = 1e-6; // metres

function assertBaseline(actual, expected, label) {
  assert.equal(actual.length, expected.length);
  for (let i = 0; i < expected.length; i++) {
    if (actual[i] === expected[i]) continue;
    const diff = Math.abs(actual[i] - expected[i]);
    assert.ok(
      diff <= BASELINE_TOL,
      `${label}[${i}]: |${actual[i]} - ${expected[i]}| = ${diff} exceeds ${BASELINE_TOL}`,
    );
  }
}

const mapSat = (row) => ({
  sat: row.sat,
  sdAmbiguityId: row.sd_ambiguity_id,
  baseCodeM: row.base_code_m,
  basePhaseM: row.base_phase_m,
  roverCodeM: row.rover_code_m,
  roverPhaseM: row.rover_phase_m,
  baseTxPos: row.base_tx_pos,
  roverTxPos: row.rover_tx_pos,
  pos: row.pos,
});

const mapEpochs = (fx) =>
  fx.epochs.map((epoch) => ({
    references: epoch.references.map(mapSat),
    nonref: epoch.nonref.map(mapSat),
    dtS: epoch.dt_s,
    velocityMps: epoch.velocity_mps ?? undefined,
  }));

const mapModel = (fx) => ({
  codeSigmaM: fx.model.code_sigma_m,
  phaseSigmaM: fx.model.phase_sigma_m,
  sagnac: fx.model.sagnac,
  stochastic: fx.model.stochastic.kind,
  elevationWeighting: fx.model.stochastic.elevation_weighting ?? false,
});

const mapFloatOpts = (fx) => ({
  positionTolM: fx.float_opts.position_tol_m,
  ambiguityTolM: fx.float_opts.ambiguity_tol_m,
  maxIterations: fx.float_opts.max_iterations,
});

const mapFixedOpts = (fx) => ({
  positionTolM: fx.fixed_opts.position_tol_m,
  ambiguityTolM: fx.fixed_opts.ambiguity_tol_m,
  maxIterations: fx.fixed_opts.max_iterations,
  ratioThreshold: fx.fixed_opts.ratio_threshold,
  partialAmbiguityResolution: fx.fixed_opts.partial_ambiguity_resolution,
  partialMinAmbiguities: fx.fixed_opts.partial_min_ambiguities,
});

const mapResidualOpts = (fx) => ({
  thresholdSigma: fx.residual_opts.threshold_sigma,
  maxExclusions: fx.residual_opts.max_exclusions,
});

test("RTK float baseline matches the engine reference", () => {
  const fx = fixtureJson("rtk_wtzr.json");
  const sol = solveRtkFloat({
    epochs: mapEpochs(fx),
    base: fx.base_arp_m,
    ambiguityIds: fx.ambiguity_ids,
    model: mapModel(fx),
    initialBaselineM: fx.initial_baseline_m,
    options: mapFloatOpts(fx),
  });

  assertBaseline(sol.baselineM, fx.expected.float_baseline_m, "float baseline");
  assert.ok(sol.converged);
  assert.equal(sol.geometryQuality.tier, ObservabilityTier.Nominal);
  assert.equal(observabilityTierLabel(sol.geometryQuality.tier), "Nominal");
  assert.equal(sol.geometryQuality.covarianceValidated, true);
  assert.equal(sol.redundancy, sol.geometryQuality.redundancy);
  assert.equal(sol.raimCheckable, sol.geometryQuality.raimCheckable);
  // Ambiguities cross as an id-keyed object; one per ambiguity id.
  assert.equal(Object.keys(sol.ambiguitiesM).length, fx.ambiguity_ids.length);
});

test("RTK fixed baseline matches the engine reference", () => {
  const fx = fixtureJson("rtk_wtzr.json");
  const sol = solveRtkFixed({
    epochs: mapEpochs(fx),
    base: fx.base_arp_m,
    ambiguityIds: fx.ambiguity_ids,
    ambiguitySatellites: fx.ambiguity_satellites,
    wavelengthsM: fx.wavelengths_m,
    offsetsM: fx.offsets_m,
    model: mapModel(fx),
    floatOptions: mapFloatOpts(fx),
    fixedOptions: mapFixedOpts(fx),
    residualOptions: mapResidualOpts(fx),
    floatOnlySystems: fx.float_only_systems,
    initialBaselineM: fx.initial_baseline_m,
  });

  assertBaseline(sol.fixedBaselineM, fx.expected.fixed_baseline_m, "fixed baseline");
  assertBaseline(
    sol.floatBaselineM,
    fx.expected.validated_float_baseline_m,
    "validated float baseline",
  );
  assert.equal(sol.integerStatus, fx.expected.fixed_integer_status);
  assert.equal(sol.geometryQuality.tier, ObservabilityTier.Nominal);
  assert.equal(sol.geometryQuality.covarianceValidated, true);
  assert.equal(sol.redundancy, sol.geometryQuality.redundancy);
  assert.equal(sol.raimCheckable, sol.geometryQuality.raimCheckable);
});

test("RTK rejects an unknown stochastic model", () => {
  const fx = fixtureJson("rtk_wtzr.json");
  assert.throws(
    () =>
      solveRtkFloat({
        epochs: mapEpochs(fx),
        base: fx.base_arp_m,
        ambiguityIds: fx.ambiguity_ids,
        model: { ...mapModel(fx), stochastic: "bogus" },
        initialBaselineM: fx.initial_baseline_m,
        options: mapFloatOpts(fx),
      }),
    TypeError,
  );
});

test("RTK rank-deficient float geometry throws a singular geometry error", () => {
  const base = [4_075_580.0, 931_854.0, 4_801_568.0];
  const refPos = [15_000_000.0, 7_000_000.0, 21_000_000.0];
  const repeated = [-12_000_000.0, 18_000_000.0, 19_000_000.0];
  const row = (sat, pos) => ({
    sat,
    sdAmbiguityId: sat,
    baseCodeM: 20_000_000.0,
    basePhaseM: 20_000_000.0,
    roverCodeM: 20_000_001.0,
    roverPhaseM: 20_000_001.0,
    baseTxPos: pos,
    roverTxPos: pos,
    pos,
  });

  assert.throws(
    () =>
      solveRtkFloat({
        epochs: [
          {
            references: [row("G01", refPos)],
            nonref: [row("G02", repeated), row("G03", repeated), row("G04", repeated)],
            dtS: 0.0,
          },
        ],
        base,
        ambiguityIds: ["G02", "G03", "G04"],
        model: { codeSigmaM: 0.3, phaseSigmaM: 0.003 },
        initialBaselineM: [1.2, -0.85, 0.91],
      }),
    (err) => {
      assert.ok(err instanceof Error);
      assert.match(err.message, /RTK float geometry is singular/i);
      return true;
    },
  );
});
