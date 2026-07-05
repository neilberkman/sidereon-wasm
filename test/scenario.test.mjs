// Deterministic scenario simulator parity: the same schema and seed must
// reproduce identical bytes and core-pinned observable arrays.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";

import {
  simulateScenario,
  simulateScenarioJson,
  simulateScenarioJsonBytes,
} from "../pkg-node/sidereon.js";
import { f64Bits } from "./helpers.mjs";

const eqBits = (value, hex) => assert.equal(f64Bits(value), BigInt(hex));
const sha256 = (bytes) => createHash("sha256").update(Buffer.from(bytes)).digest("hex");

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

  assert.equal(fromJson.determinismFingerprintHex, "0x5680e30d41ce1db4");
  assert.equal(fromObject.determinismFingerprintHex, fromJson.determinismFingerprintHex);
  assert.equal(fromJson.observationCount, 10);
  assert.deepEqual(fromJson.observations.epochOffsets, [0, 5, 10]);
  assert.equal(fromJson.observations.satelliteId[0], "G01");

  eqBits(fromJson.observations.pseudorangeM[0], "0x41733F38567B8EB6");
  eqBits(fromJson.observations.carrierPhaseCycles[0], "0x4199492DFEABD3F0");
  eqBits(fromJson.observations.dopplerHz[0], "0xBFC00D7EDDBD533C");
  eqBits(fromJson.truthTerms.geometricRangeM[0], "0x41733F367001A84B");
  eqBits(fromJson.truthTerms.thermalNoiseM[0], "0x3FD7711C52A2AF5B");
  eqBits(fromJson.receiverTruth[1].positionEcefM[0], "0x415854A640000000");
});

test("scenario simulator is byte-deterministic for the same schema and seed", () => {
  const text = JSON.stringify(SCENARIO);
  const first = simulateScenarioJsonBytes(text);
  const second = simulateScenarioJsonBytes(text);

  assert.deepEqual(Buffer.from(first), Buffer.from(second));
  assert.equal(first.length, 4991);
  assert.equal(sha256(first), "0ff02a6d91290d07e7a2d9a2fb629ce1378e91c3f95cf9183f26726994066d9a");
});
