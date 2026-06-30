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
import { fixture, fixtureJson } from "./helpers.mjs";

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
});

const mapWeights = (raw) => ({
  code: raw.code,
  phase: raw.phase,
  elevationWeighting: raw.elevation_weighting,
});

const mapTropo = (raw) => ({
  enabled: raw.enabled,
  estimateZtd: raw.estimate_ztd,
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
  residualScreen: fx.config.residual_screen,
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
});

test("PPP float position matches the engine reference", () => {
  const fx = fixtureJson("ppp_esbc.json");
  const sp3 = loadSp3Fixture(fx);
  const sol = solvePppFloat(sp3, mapEpochs(fx), mapState(fx), mapFloatConfig(fx));

  assertVecClose(sol.positionM, fx.expected.position_m, POS_TOL, "float position");
  assert.ok(sol.converged);
  assert.ok(sol.usedSats.length > 0);
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
