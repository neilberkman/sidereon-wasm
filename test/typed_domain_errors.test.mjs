import { test } from "node:test";
import assert from "node:assert/strict";

import {
  ExactCacheSingleFlightWait,
  Instant,
  coe2eq,
  coe2rv,
  meanToTrue,
  propagateState,
  rv2coe,
  simulateScenario,
  simulateScenarioSet,
  solveKepler,
  trueToEccentric,
} from "../pkg-node/sidereon.js";

const MU = 398600.4418;

function captureError(call) {
  let captured;
  assert.throws(call, (error) => {
    captured = error;
    return error instanceof Error;
  });
  return captured;
}

function assertDetail(call, family, kind) {
  const error = captureError(call);
  assert.ok(error instanceof Error);
  assert.equal(error.name, "Error");
  assert.equal(error.detail.family, family);
  assert.equal(error.detail.cause.kind, kind);
  return error.detail;
}

const scenario = (elevationMaskDeg = 0) => ({
  schema_version: 1,
  seed: 7,
  epochs: { start_j2000_s: 820497600, count: 1, cadence_s: 30 },
  receiver: {
    kind: "static_geodetic",
    position: { lat_rad: 0, lon_rad: 0, height_m: 0 },
  },
  constellation: {
    kind: "synthetic_keplerian",
    satellites: [
      {
        satellite_id: { system: "Gps", prn: 1 },
        semi_major_axis_m: 26560000,
        eccentricity: 0,
        inclination_rad: 0,
        raan_rad: 0,
        arg_perigee_rad: 0,
        mean_anomaly_rad: 0,
        epoch_j2000_s: 820497600,
        clock_bias_s: 0,
        clock_drift_s_s: 0,
      },
    ],
  },
  signals: [
    {
      system: "Gps",
      code_observable: "C1C",
      phase_observable: "L1C",
      doppler_observable: "D1C",
      carrier_hz: 1575420000,
      carrier_phase_bias_cycles: 0,
    },
  ],
  error_budget: {
    receiver_clock: {
      enabled: false,
      bias_s: 0,
      drift_s_s: 0,
      power_law_coefficients: [0, 0, 0, 0, 0],
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
      enabled: false,
      pseudorange_sigma_m: 0,
      carrier_phase_sigma_m: 0,
      doppler_sigma_hz: 0,
    },
    multipath: {
      enabled: false,
      amplitude_m: 0,
      reflector_height_m: 0,
      phase_rad: 0,
    },
    elevation_mask_deg: elevationMaskDeg,
  },
});

test("orbital domain errors retain variant data, exact floats, and old Error behavior", () => {
  const anomaly = assertDetail(() => solveKepler(0.25, -0.1), "anomaly", "negativeEccentricity");
  assert.equal(anomaly.cause.kind, "negativeEccentricity");
  assert.equal(
    "eccentricity must be non-negative",
    captureError(() => solveKepler(0.25, -0.1)).message,
  );

  const nonFinite = assertDetail(() => solveKepler(Number.NaN, 0.1), "anomaly", "nonFinite");
  assert.equal(nonFinite.cause.field, "mean_anom");
  assert.equal(nonFinite.cause.kind, "nonFinite");

  const asymptote = assertDetail(() => trueToEccentric(3, 1.5), "anomaly", "beyondAsymptote");
  assert.equal(asymptote.cause.nu.decimal, "3");
  assert.equal(asymptote.cause.nu.bitsHex, "4008000000000000");

  const elements = assertDetail(() => rv2coe([0, 0, 0], [0, 0, 0], MU), "elements", "zeroPosition");
  assert.equal(elements.cause.kind, "zeroPosition");
  assert.equal(
    coe2rv({ p: 7000, ecc: 0.01, incl: 0.2, raan: 0, argp: 0, nu: 0 }, MU).positionKm.length,
    3,
  );

  const equinoctial = assertDetail(
    () => coe2eq({ p: 7000, ecc: 1, incl: 0.2, raan: 0, argp: 0, nu: 0 }),
    "equinoctial",
    "parabolicEquinoctial",
  );
  assert.equal(equinoctial.cause.kind, "parabolicEquinoctial");

  const nestedAnomaly = assertDetail(
    () => coe2eq({ p: 7000, a: 7000, ecc: -0.1, incl: 0.2, raan: 0, argp: 0, nu: 0 }),
    "equinoctial",
    "anomaly",
  );
  assert.equal(nestedAnomaly.cause.cause.kind, "negativeEccentricity");

  const nestedElements = assertDetail(
    () => coe2eq({ p: -1, a: 7000, ecc: 0.1, incl: 0.2, raan: 0, argp: 0, nu: 0 }),
    "equinoctial",
    "elements",
  );
  assert.equal(nestedElements.cause.cause.kind, "nonPositiveSemiLatus");

  const retained = anomaly;
  meanToTrue(0.5, 0.1);
  assert.equal(retained.cause.kind, "negativeEccentricity");
});

test("propagation refusal and success keep their typed family", () => {
  const error = captureError(() =>
    propagateState({
      epochS: 0,
      positionKm: [7000, 0, 0],
      velocityKmS: [0, 7.5, 0],
      timesS: [0, 1e8],
      forceModel: "two_body",
      integrator: "dp54",
      initialStepS: 1,
      maxSteps: 1,
    }),
  );
  assert.equal(error.detail.family, "propagation");
  assert.equal(error.detail.cause.kind, "maxStepsExceeded");
  assert.throws(
    () =>
      propagateState({
        epochS: 0,
        positionKm: [7000, 0, 0],
        velocityKmS: [0, 7.5, 0],
        timesS: [0],
        forceModel: "two_body",
        integrator: "dp54",
        initialStepS: 0,
      }),
    RangeError,
  );
  const successful = propagateState({
    epochS: 0,
    positionKm: [7000, 0, 0],
    velocityKmS: [0, 7.5, 0],
    timesS: [0],
    forceModel: "two_body",
    integrator: "dp54",
  });
  assert.equal(successful.epochCount, 1);
  successful.free();
});

test("frame and scenario errors retain structured causes while success remains unchanged", () => {
  const instant = Instant.fromUtc(1800, 1, 1);
  const error = captureError(() => instant.gmstRadiansWithValidity());
  assert.equal(error.detail.family, "frameTransform");
  assert.equal(error.detail.cause.kind, "ut1OutsideCoverage");
  const permissive = instant.gmstRadiansWithValidity("permissive");
  assert.equal(permissive.ut1Degraded, "beforeCoverage");
  assert.ok(Number.isFinite(permissive.value));
  instant.free();

  const scenarioError = captureError(() => simulateScenario(scenario(91)));
  assert.equal(scenarioError.detail.family, "scenario");
  assert.equal(scenarioError.detail.cause.kind, "invalidInput");
  assert.equal(scenarioError.detail.cause.field, "error_budget.elevation_mask_deg");
  assert.equal(simulateScenario(scenario()).schemaVersion, 1);

  const external = scenario();
  external.constellation = {
    kind: "external_products",
    source: { kind: "sp3", product_id: "fixture", content_digest: "sha256:test" },
    satellites: [{ system: "Gps", prn: 1 }],
  };
  const externalError = captureError(() => simulateScenario(external));
  assert.equal(externalError.detail.cause.kind, "externalSourceRequired");
});

test("exact-cache option errors expose the core variant and valid waits still work", () => {
  const error = captureError(() => new ExactCacheSingleFlightWait(0, 0, 0, 0, 0));
  assert.equal(error.detail.family, "exactCache");
  assert.equal(error.detail.cause.kind, "invalidSingleFlightOptions");

  const wait = new ExactCacheSingleFlightWait(0, 1, 2, 10, 100);
  assert.equal(wait.observe(0, new Uint8Array()).action, "wait");
  wait.free();
});

test("scenario epoch index rejects wrapped numeric inputs and keeps past-end behavior", () => {
  const simulation = simulateScenarioSet(scenario());
  assert.throws(() => simulation.sppObservationsForEpoch(-1), RangeError);
  assert.throws(() => simulation.sppObservationsForEpoch(0.5), RangeError);
  assert.throws(() => simulation.sppObservationsForEpoch(Number.NaN), RangeError);
  assert.throws(() => simulation.sppObservationsForEpoch(Number.POSITIVE_INFINITY), RangeError);
  assert.throws(() => simulation.sppObservationsForEpoch(2 ** 32), RangeError);
  assert.ok(Array.isArray(simulation.sppObservationsForEpoch(0)));
  assert.deepEqual(simulation.sppObservationsForEpoch(100), []);
  simulation.free();
});
