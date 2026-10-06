// Deterministic scenario simulator parity: the same schema and seed must
// reproduce identical bytes and core-pinned observable arrays.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  parseRinexObs,
  simulateScenario,
  simulateScenarioSet,
  simulateScenarioJson,
  simulateScenarioJsonBytes,
} from "../pkg-node/sidereon.js";
import { coreGoldens, f64Bits } from "./helpers.mjs";

const eqBits = (value, hex) => assert.equal(f64Bits(value), BigInt(hex));

const START = 820497600;
const SATELLITES = [
  [1, 0, 0, 0],
  [2, 0, 0, Math.PI / 3],
  [3, 0, 0, -Math.PI / 3],
  [4, 0, Math.PI / 2, Math.PI / 3],
  [5, 0, Math.PI / 2, -Math.PI / 3],
].map(([prn, raanRad, inclinationRad, meanAnomalyRad]) => ({
  satellite_id: { system: "Gps", prn },
  semi_major_axis_m: 26560000,
  eccentricity: 0,
  inclination_rad: inclinationRad,
  raan_rad: raanRad,
  arg_perigee_rad: 0,
  mean_anomaly_rad: meanAnomalyRad,
  epoch_j2000_s: START,
  clock_bias_s: 0,
  clock_drift_s_s: 0,
}));

const SCENARIO = {
  schema_version: 1,
  seed: 123456789,
  epochs: { start_j2000_s: START, count: 2, cadence_s: 30 },
  receiver: { kind: "static_geodetic", position: { lat_rad: 0, lon_rad: 0, height_m: 0 } },
  constellation: { kind: "synthetic_keplerian", satellites: SATELLITES },
  signals: [
    {
      system: "Gps",
      code_observable: "C1C",
      phase_observable: "L1C",
      doppler_observable: "D1C",
      carrier_hz: 1575420000,
      carrier_phase_bias_cycles: 12.25,
    },
  ],
  error_budget: {
    receiver_clock: {
      enabled: true,
      bias_s: 1e-7,
      drift_s_s: 1e-10,
      power_law_coefficients: [1e-24, 1e-26, 1e-22, 1e-26, 1e-28],
    },
    satellite_clock: {
      enabled: false,
      bias_s: 0,
      drift_s_s: 0,
      power_law_coefficients: [0, 0, 0, 0, 0],
    },
    ionosphere: { kind: "off" },
    troposphere: { kind: "off" },
    thermal_noise: {
      enabled: true,
      pseudorange_sigma_m: 0.25,
      carrier_phase_sigma_m: 0.002,
      doppler_sigma_hz: 0.02,
    },
    multipath: { enabled: true, amplitude_m: 0.15, reflector_height_m: 1.25, phase_rad: 0.3 },
    elevation_mask_deg: -90,
  },
};

test("scenario simulator returns pinned arrays", () => {
  const text = JSON.stringify(SCENARIO);
  const fromJson = simulateScenarioJson(text);
  const fromObject = simulateScenario(SCENARIO);

  // The fingerprint stamps the engine version by design, so it changes each
  // release; the invariant is cross-entry-point agreement, and the value pins
  // below anchor the actual numbers.
  assert.equal(fromObject.determinismFingerprintHex, fromJson.determinismFingerprintHex);
  // The same scenario simulated natively by test/golden-gen.
  const ref = coreGoldens().scenario;
  assert.equal(fromJson.observationCount, ref.observationCount);
  assert.deepEqual(fromJson.observations.epochOffsets, ref.epochOffsets);
  assert.equal(fromJson.observations.satelliteId[0], ref.firstSatellite);

  eqBits(fromJson.observations.pseudorangeM[0], ref.pseudorangeM0);
  eqBits(fromJson.observations.carrierPhaseCycles[0], ref.carrierPhaseCycles0);
  eqBits(fromJson.observations.dopplerHz[0], ref.dopplerHz0);
  eqBits(fromJson.truthTerms.geometricRangeM[0], ref.geometricRangeM0);
  eqBits(fromJson.truthTerms.thermalNoiseM[0], ref.thermalNoiseM0);
  eqBits(fromJson.receiverTruth[1].positionEcefM[0], ref.receiverTruth1PositionEcefM0);
});

test("scenario simulator is byte-deterministic for the same schema and seed", () => {
  const text = JSON.stringify(SCENARIO);
  const first = simulateScenarioJsonBytes(text);
  const second = simulateScenarioJsonBytes(text);

  assert.deepEqual(Buffer.from(first), Buffer.from(second));
  const payload = JSON.parse(Buffer.from(first).toString("utf8"));
  assert.equal(payload.schemaVersion, 1);
  assert.match(payload.engineVersion, /^\d+\.\d+\.\d+:scenario-observables-v1$/);
});

test("the simulation set writes the engine's RINEX text and SPP observations", () => {
  const ref = coreGoldens().scenario;
  const sim = simulateScenarioSet(SCENARIO);
  // The fingerprint includes the engine version. This exact 3.0.2 value was
  // produced by the candidate core pinned in Cargo.lock.
  assert.equal(sim.determinismFingerprintHex, ref.determinismFingerprintHex);
  assert.equal(sim.arrays.observationCount, ref.observationCount);

  // The text is the engine's own for the same scenario, reproduced natively
  // by test/golden-gen, and the product reads back to it.
  const text = sim.toRinexString();
  assert.equal(text, ref.rinexText);
  assert.equal(sim.toRinexObservationFile().toRinexString(), text);
  assert.equal(parseRinexObs(new TextEncoder().encode(text)).toRinexString(), text);

  assert.deepEqual(
    sim.sppObservationsForEpoch(0).map((obs) => [obs.satelliteId, f64Bits(obs.pseudorangeM)]),
    ref.sppObservationsEpoch0.map((obs) => [obs.satelliteId, BigInt(obs.pseudorangeM)]),
  );
  assert.deepEqual(sim.sppObservationsForEpoch(1_000_000), []);
});
