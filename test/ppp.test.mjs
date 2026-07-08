// Static float and integer-fixed PPP solves through the WASM binding, mirroring
// sidereon-python/tests/test_ppp.py against the same committed golden
// (ppp_esbc.json: the crate's ESBC troposphere-corrected float-PPP arc) and the
// same committed SP3 product.
//
// The fixture stores the engine's reference position as shortest-round-trip
// decimals; Python asserts bit-exact (np.array_equal). Here position components
// are bit-exact where the wasm32 libm agrees and otherwise within a tight
// 1e-6 m tolerance (a cross-libm ULP residual in the least-squares /
// troposphere kernels, not a marshalling bug). Integer cycle counts are exact.

import { test } from "node:test";
import assert from "node:assert/strict";

import { loadSp3, solvePppFloat, solvePppFixed } from "../pkg-node/sidereon.js";
import { f64Bits, fixture, fixtureJson } from "./helpers.mjs";

const POS_TOL = 1e-6; // metres
const AMB_TOL = 1e-9; // metres

function assertVecClose(actual, expected, tol, label) {
  assert.equal(actual.length, expected.length);
  for (let i = 0; i < expected.length; i++) {
    if (actual[i] === expected[i]) continue;
    const diff = Math.abs(actual[i] - expected[i]);
    assert.ok(diff <= tol, `${label}[${i}]: diff ${diff} exceeds ${tol}`);
  }
}

function assertMapClose(actual, expected, tol, label) {
  const ek = Object.keys(expected).sort();
  assert.deepEqual(Object.keys(actual).sort(), ek);
  for (const k of ek) {
    if (actual[k] === expected[k]) continue;
    const diff = Math.abs(actual[k] - expected[k]);
    assert.ok(diff <= tol, `${label}[${k}]: diff ${diff} exceeds ${tol}`);
  }
}

const loadSp3Fixture = (fx) => loadSp3(fixture(`sp3/${fx.sp3_file}`));

const mapEpochs = (fx) =>
  fx.epochs.map((epoch) => ({
    civil: {
      year: epoch.civil.year,
      month: epoch.civil.month,
      day: epoch.civil.day,
      hour: epoch.civil.hour,
      minute: epoch.civil.minute,
      second: epoch.civil.second,
    },
    jdWhole: epoch.jd_whole,
    jdFraction: epoch.jd_fraction,
    tRxJ2000S: epoch.t_rx_j2000_s,
    observations: epoch.observations.map((obs) => ({
      satelliteId: obs.satellite_id,
      ambiguityId: obs.ambiguity_id,
      codeM: obs.code_m,
      phaseM: obs.phase_m,
      freq1Hz: obs.freq1_hz,
      freq2Hz: obs.freq2_hz,
    })),
  }));

const mapState = (fx) => ({
  positionM: fx.initial_state.position_m,
  clocksM: fx.initial_state.clocks_m,
  ambiguitiesM: fx.initial_state.ambiguities_m,
  ztdM: fx.initial_state.ztd_m,
  tropoGradientNorthM: fx.initial_state.tropo_gradient_north_m ?? 0,
  tropoGradientEastM: fx.initial_state.tropo_gradient_east_m ?? 0,
  residualIonosphereM: fx.initial_state.residual_ionosphere_m ?? {},
});

const mapWeights = (raw) => ({
  code: raw.code,
  phase: raw.phase,
  elevationWeighting: raw.elevation_weighting,
});

const mapTropo = (raw) => ({
  enabled: raw.enabled,
  estimateZtd: raw.estimate_ztd,
  estimateTropoGradients: raw.estimate_tropo_gradients ?? false,
  pressureHpa: raw.pressure_hpa,
  temperatureK: raw.temperature_k,
  relativeHumidity: raw.relative_humidity,
});

const mapOptions = (raw) => ({
  maxIterations: raw.max_iterations,
  positionToleranceM: raw.position_tolerance_m,
  clockToleranceM: raw.clock_tolerance_m,
  ambiguityToleranceM: raw.ambiguity_tolerance_m,
  ztdToleranceM: raw.ztd_tolerance_m,
});

const mapFloatConfig = (fx) => ({
  weights: mapWeights(fx.config.weights),
  tropo: mapTropo(fx.config.tropo),
  options: mapOptions(fx.config.opts),
  elevationCutoffDeg: fx.config.elevation_cutoff_deg,
  residualScreen: fx.config.residual_screen,
  estimateResidualIonosphere: fx.config.estimate_residual_ionosphere ?? false,
});

const mapFixedConfig = (fx) => ({
  ambiguity: {
    wavelengthsM: fx.fixed_config.ambiguity.wavelengths_m,
    offsetsM: fx.fixed_config.ambiguity.offsets_m,
    ratioThreshold: fx.fixed_config.ambiguity.ratio_threshold,
  },
  weights: mapWeights(fx.fixed_config.weights),
  tropo: mapTropo(fx.fixed_config.tropo),
  options: mapOptions(fx.fixed_config.opts),
  elevationCutoffDeg: fx.fixed_config.elevation_cutoff_deg,
  estimateResidualIonosphere: fx.fixed_config.estimate_residual_ionosphere ?? false,
});

test("PPP float position matches the engine reference", () => {
  const fx = fixtureJson("ppp_esbc.json");
  const sp3 = loadSp3Fixture(fx);
  const sol = solvePppFloat(sp3, mapEpochs(fx), mapState(fx), mapFloatConfig(fx));

  assertVecClose(sol.positionM, fx.expected.position_m, POS_TOL, "float position");
  assert.ok(sol.converged);
  assert.ok(sol.usedSats.length > 0);
});

test("PPP float exposes covariance, residual, and temporal-correlation surfaces", () => {
  const fx = fixtureJson("ppp_esbc.json");
  const sp3 = loadSp3Fixture(fx);
  const sol = solvePppFloat(sp3, mapEpochs(fx), mapState(fx), mapFloatConfig(fx));

  assert.equal(sol.status, "StateTolerance");
  assert.equal(sol.epochClocksM.length, fx.epochs.length);
  assert.equal(sol.residuals.length, 1282);
  assert.deepEqual(Object.keys(sol.residuals[0]).sort(), [
    "codeM",
    "codeWeight",
    "epochIndex",
    "phaseM",
    "phaseWeight",
    "satelliteId",
  ]);
  assert.deepEqual(sol.residualIonosphereM, {});
  assert.equal(sol.tropoGradientNorthM, undefined);
  assert.equal(sol.tropoGradientEastM, undefined);
  assert.equal(sol.tropoGradientCovarianceM2, undefined);
  assert.equal(sol.formalTropoGradientCovarianceM2, undefined);

  assert.equal(sol.positionCovarianceEcefM2.length, 9);
  assert.equal(sol.positionCovarianceEnuM2.length, 9);
  assert.equal(sol.formalPositionCovarianceEcefM2.length, 9);
  assert.equal(sol.formalPositionCovarianceEnuM2.length, 9);
  assert.equal(sol.temporalPositionCovarianceEcefM2.length, 9);
  assert.equal(sol.temporalPositionCovarianceEnuM2.length, 9);
  assert.deepEqual(Array.from(sol.positionCovarianceEcefM2, f64Bits), [
    4590517292634130647n,
    4578482196578072829n,
    4572802777262634589n,
    4578482196578072829n,
    4586874534299914772n,
    13779057109705650780n,
    4572802777262634589n,
    13779057109705650780n,
    4587002875255672439n,
  ]);
  assert.deepEqual(Array.from(sol.temporalPositionCovarianceEcefM2, f64Bits), [
    4619080960802279813n,
    4607308545278299410n,
    4601445456370031809n,
    4607308545278299410n,
    4615665010415894763n,
    13807936042986440555n,
    4601445456370031809n,
    13807936042986440555n,
    4615827165675205843n,
  ]);
  assert.deepEqual(
    [
      sol.temporalCorrelation.lag1Autocorrelation,
      sol.temporalCorrelation.decorrelationTimeEpochs,
      sol.temporalCorrelation.decorrelationTimeS,
      sol.temporalCorrelation.effectiveSampleCount,
      sol.temporalCorrelation.varianceInflationFactor,
    ].map(f64Bits),
    [
      4607092346807469998n,
      4636702048046853952n,
      4658782444239332428n,
      4629618296680096500n,
      4635390590904316913n,
    ],
  );
  assert.equal(sol.temporalCorrelation.nominalSampleCount, 2564);
  assert.equal(sol.temporalCorrelation.arcsUsed, 24);
});

test("PPP fixed position and integer fix match the engine reference", () => {
  const fx = fixtureJson("ppp_esbc.json");
  const sp3 = loadSp3Fixture(fx);
  const epochs = mapEpochs(fx);
  const floatSol = solvePppFloat(sp3, epochs, mapState(fx), mapFloatConfig(fx));

  const sol = solvePppFixed(sp3, epochs, floatSol, mapFixedConfig(fx));
  const exp = fx.expected;

  assertVecClose(sol.positionM, exp.fixed_position_m, POS_TOL, "fixed position");
  assertVecClose(
    sol.floatSolution.positionM,
    exp.fixed_float_position_m,
    POS_TOL,
    "fixed float position",
  );
  assert.equal(sol.integerStatus, exp.fixed_integer_status);
  assert.ok(Math.abs(sol.integerRatio - exp.fixed_integer_ratio) <= 1e-9);
  assert.equal(sol.integerCandidates, exp.fixed_integer_candidates);
  // Integer cycle counts are exact.
  assert.deepEqual(sol.fixedAmbiguitiesCycles, exp.fixed_ambiguities_cycles);
  assertMapClose(sol.fixedAmbiguitiesM, exp.fixed_ambiguities_m, AMB_TOL, "fixed ambiguities m");
  assert.equal(sol.status, "StateTolerance");
  assert.equal(sol.residuals.length, 1282);
  assert.equal(sol.positionCovarianceEcefM2.length, 9);
  assert.equal(sol.temporalPositionCovarianceEcefM2.length, 9);
  assert.equal(sol.temporalCorrelation.nominalSampleCount, 2564);
});

test("PPP elevation cutoff filters observations before solving", () => {
  const fx = fixtureJson("ppp_esbc.json");
  const sp3 = loadSp3Fixture(fx);
  const epochs = mapEpochs(fx);
  const state = mapState(fx);
  const config = mapFloatConfig(fx);
  const base = solvePppFloat(sp3, epochs, state, config);
  const cutoff = solvePppFloat(sp3, epochs, state, { ...config, elevationCutoffDeg: 30 });

  assert.ok(cutoff.converged);
  assert.equal(cutoff.usedSats.length, 6);
  assert.ok(cutoff.usedSats.length < base.usedSats.length);
  assert.ok(cutoff.residuals.length < base.residuals.length);
  assert.notDeepEqual(Array.from(cutoff.positionM, f64Bits), Array.from(base.positionM, f64Bits));
});

test("PPP troposphere gradients expose state and covariance outputs", () => {
  const fx = fixtureJson("ppp_esbc.json");
  const sp3 = loadSp3Fixture(fx);
  const epochs = mapEpochs(fx);
  const state = mapState(fx);
  const config = mapFloatConfig(fx);
  const grad = solvePppFloat(sp3, epochs, state, {
    ...config,
    tropo: { ...config.tropo, estimateTropoGradients: true },
  });

  assert.deepEqual([grad.tropoGradientNorthM, grad.tropoGradientEastM].map(f64Bits), [
    4586155387479286271n,
    4581396053300541918n,
  ]);
  assert.deepEqual(Array.from(grad.tropoGradientCovarianceM2, f64Bits), [
    4513194251217481223n,
    4508095322312819751n,
    4508095322312819751n,
    4516470258699158978n,
  ]);
  assert.deepEqual(Array.from(grad.formalTropoGradientCovarianceM2, f64Bits), [
    4465329674133088067n,
    4460040126213447646n,
    4460040126213447646n,
    4468627182330646986n,
  ]);
});

test("PPP float accepts a VMF1 site series and converges (B1 correction option)", () => {
  const fx = fixtureJson("ppp_esbc.json");
  const sp3 = loadSp3Fixture(fx);
  const epochs = mapEpochs(fx);
  const state = mapState(fx);

  // The ESBC arc is at MJD 59025; bracket it with 6-hourly VMF1 `a`-coefficient
  // samples (realistic hydrostatic/wet magnitudes) so the series spans the arc.
  const vmf1 = [59024.75, 59025.0, 59025.25, 59025.5, 59025.75].map((mjd) => ({
    mjd,
    ah: 0.00123,
    aw: 0.00056,
  }));
  const config = mapFloatConfig(fx);
  const sol = solvePppFloat(sp3, epochs, state, {
    ...config,
    tropo: { ...config.tropo, vmf1 },
  });

  assert.ok(sol.converged);
  assert.ok(sol.usedSats.length > 0);
  for (const v of sol.positionM) assert.ok(Number.isFinite(v));

  // The VMF1 mapping perturbs the solution relative to the default Niell map.
  const niell = solvePppFloat(sp3, epochs, state, config);
  const moved = sol.positionM.some((v, i) => v !== niell.positionM[i]);
  assert.ok(moved, "VMF1 mapping should change the solution vs Niell");
});

test("PPP rejects a non-ascending VMF1 series", () => {
  const fx = fixtureJson("ppp_esbc.json");
  const sp3 = loadSp3Fixture(fx);
  const config = mapFloatConfig(fx);
  const vmf1 = [
    { mjd: 59025.25, ah: 0.00123, aw: 0.00056 },
    { mjd: 59025.0, ah: 0.00123, aw: 0.00056 },
  ];
  assert.throws(
    () =>
      solvePppFloat(sp3, mapEpochs(fx), mapState(fx), {
        ...config,
        tropo: { ...config.tropo, vmf1 },
      }),
    Error,
  );
});
