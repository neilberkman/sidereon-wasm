import { test } from "node:test";
import assert from "node:assert/strict";

import {
  buildPppLookup,
  buildPppLookupWithValidity,
  loadSp3,
  PppCorrectionLookup,
  PppRangeCorrections,
  PppReceiverAntennaFrequency,
  PppSatelliteClockCorrections,
  pppCorrections,
  solvePppFloat,
  solvePppFloatWithCorrections,
} from "../pkg-node/sidereon.js";
import { fixture, fixtureJson } from "./helpers.mjs";

const fx = fixtureJson("ppp_esbc.json");
const sp3 = loadSp3(fixture(`sp3/${fx.sp3_file}`));
const epochs = fx.epochs.map((epoch) => ({
  civil: epoch.civil,
  jdWhole: epoch.jd_whole,
  jdFraction: epoch.jd_fraction,
  tRxJ2000S: epoch.t_rx_j2000_s,
  observations: epoch.observations.map((row) => ({
    satelliteId: row.satellite_id,
    ambiguityId: row.ambiguity_id,
    codeM: row.code_m,
    phaseM: row.phase_m,
    freq1Hz: row.freq1_hz,
    freq2Hz: row.freq2_hz,
  })),
}));
const state = {
  positionM: fx.initial_state.position_m,
  clocksM: fx.initial_state.clocks_m,
  ambiguitiesM: fx.initial_state.ambiguities_m,
  ztdM: fx.initial_state.ztd_m,
};
const config = {
  weights: {
    code: fx.config.weights.code,
    phase: fx.config.weights.phase,
    elevationWeighting: fx.config.weights.elevation_weighting,
  },
  tropo: {
    enabled: fx.config.tropo.enabled,
    estimateZtd: fx.config.tropo.estimate_ztd,
    pressureHpa: fx.config.tropo.pressure_hpa,
    temperatureK: fx.config.tropo.temperature_k,
    relativeHumidity: fx.config.tropo.relative_humidity,
  },
  options: {
    maxIterations: fx.config.opts.max_iterations,
    positionToleranceM: fx.config.opts.position_tolerance_m,
    clockToleranceM: fx.config.opts.clock_tolerance_m,
    ambiguityToleranceM: fx.config.opts.ambiguity_tolerance_m,
    ztdToleranceM: fx.config.opts.ztd_tolerance_m,
  },
  residualScreen: fx.config.residual_screen,
};

test("PPP lookup owns exact core tables and compares by value", () => {
  const empty = new PppCorrectionLookup();
  const builtEmpty = buildPppLookup(sp3, epochs, state.positionM, {});
  assert.ok(empty.equals(builtEmpty));
  assert.ok(
    builtEmpty.equals(buildPppLookupWithValidity(sp3, epochs, state.positionM, {}, "strict")),
  );

  const tide = buildPppLookup(sp3, epochs, state.positionM, { solidEarthTide: true });
  assert.equal(tide.equals(empty), false);
  assert.equal(tide.tideEnabled, true);
  assert.equal(tide.tide.length, 120);
  assert.deepEqual(tide.tide[0], {
    epochIndex: 0,
    vectorM: [-0.06488784864296782, -0.0015824961967540643, -0.12331009164895183],
  });
  const correctionEpochs = epochs.map((epoch) => ({
    ...epoch.civil,
    tRxJ2000S: epoch.tRxJ2000S,
    observations: epoch.observations.map((row) => ({
      satelliteId: row.satelliteId,
      freq1Hz: row.freq1Hz,
      freq2Hz: row.freq2Hz,
    })),
  }));
  const value = pppCorrections(sp3, correctionEpochs, state.positionM, {
    solidEarthTide: true,
  });
  assert.ok(PppCorrectionLookup.fromCorrections(value, { solidEarthTide: true }).equals(tide));
});

test("range corrections change the solve by the exact core result", () => {
  const disabled = solvePppFloat(sp3, epochs, state, config);
  const corrections = new PppRangeCorrections(
    undefined,
    true,
    undefined,
    new PppCorrectionLookup(),
  );
  const corrected = solvePppFloatWithCorrections(sp3, epochs, state, config, corrections);
  assert.deepEqual(
    Array.from(disabled.positionM),
    [3582104.3891192144, 532587.3737544214, 5232753.363293018],
  );
  assert.deepEqual(
    Array.from(corrected.positionM),
    [3582105.241679125, 532590.2474113037, 5232755.541550067],
  );
  assert.equal(corrected.equals(disabled), false);
  assert.ok(corrections.lookup().equals(new PppCorrectionLookup()));
});

test("correction constructors and builders refuse malformed foreign values", () => {
  assert.throws(
    () => new PppReceiverAntennaFrequency("L1", [0, 0], []),
    /pcoM must contain exactly 3 numbers/,
  );
  assert.throws(
    () => new PppSatelliteClockCorrections({ bad: [[0, 0]] }),
    /invalid satellite clock token: bad/,
  );
  assert.throws(
    () => buildPppLookup(sp3, epochs, [0, 0], {}),
    /receiverEcefM must contain exactly 3 numbers/,
  );
});
