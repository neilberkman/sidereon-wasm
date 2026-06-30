// SPP-seeded PPP auto-init drivers delegate to
// sidereon_core::precise_positioning::auto_init::{solve_ppp_auto_init_float,
// solve_ppp_auto_init_fixed}. Unlike solvePppFloat / solvePppFixed, no initial
// state is supplied: the driver seeds it from the per-epoch SPP solve. Run
// against the same committed ESBC arc and SP3 product as ppp.test.mjs; the
// converged solution matches the explicitly-seeded float/fixed solves.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  loadSp3,
  solvePppFloat,
  solvePppAutoInitFloat,
  solvePppAutoInitFixed,
} from "../pkg-node/sidereon.js";
import { fixture, fixtureJson } from "./helpers.mjs";

const POS_TOL = 1e-6; // metres

const fx = fixtureJson("ppp_esbc.json");
const sp3 = loadSp3(fixture(`sp3/${fx.sp3_file}`));

const epochs = fx.epochs.map((epoch) => ({
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

const floatConfig = {
  weights: mapWeights(fx.config.weights),
  tropo: mapTropo(fx.config.tropo),
  options: mapOptions(fx.config.opts),
  residualScreen: fx.config.residual_screen,
};
const fixedConfig = {
  ambiguity: {
    wavelengthsM: fx.fixed_config.ambiguity.wavelengths_m,
    offsetsM: fx.fixed_config.ambiguity.offsets_m,
    ratioThreshold: fx.fixed_config.ambiguity.ratio_threshold,
  },
  weights: mapWeights(fx.fixed_config.weights),
  tropo: mapTropo(fx.fixed_config.tropo),
  options: mapOptions(fx.fixed_config.opts),
};

function assertVecClose(actual, expected, tol, label) {
  assert.equal(actual.length, expected.length);
  for (let i = 0; i < expected.length; i++) {
    if (actual[i] === expected[i]) continue;
    assert.ok(Math.abs(actual[i] - expected[i]) <= tol, `${label}[${i}]`);
  }
}

test("solvePppAutoInitFloat (SPP seed) matches the engine float reference", () => {
  const sol = solvePppAutoInitFloat(sp3, epochs, undefined, floatConfig);
  assert.ok(sol.converged);
  assert.ok(sol.usedSats.length > 0);
  // The SPP-seeded arc converges to the same optimum the explicitly-seeded
  // float solve reaches.
  assertVecClose(sol.positionM, fx.expected.position_m, POS_TOL, "auto float position");

  const seeded = solvePppFloat(
    sp3,
    epochs,
    {
      positionM: fx.initial_state.position_m,
      clocksM: fx.initial_state.clocks_m,
      ambiguitiesM: fx.initial_state.ambiguities_m,
      ztdM: fx.initial_state.ztd_m,
    },
    floatConfig,
  );
  assertVecClose(sol.positionM, seeded.positionM, POS_TOL, "auto vs seeded float");
});

test("solvePppAutoInitFloat honours an explicit initial guess", () => {
  const guess = {
    positionM: fx.initial_state.position_m,
    clockM: fx.initial_state.clocks_m[0],
  };
  const sol = solvePppAutoInitFloat(sp3, epochs, { initialGuess: guess }, floatConfig);
  assert.ok(sol.converged);
  assertVecClose(sol.positionM, fx.expected.position_m, POS_TOL, "guess float position");
});

test("solvePppAutoInitFixed (SPP seed) matches the engine fixed reference", () => {
  const sol = solvePppAutoInitFixed(sp3, epochs, undefined, floatConfig, fixedConfig);
  assertVecClose(sol.positionM, fx.expected.fixed_position_m, POS_TOL, "auto fixed position");
  assert.equal(sol.integerStatus, fx.expected.fixed_integer_status);
  assert.ok(sol.usedSats.length > 0);
});
