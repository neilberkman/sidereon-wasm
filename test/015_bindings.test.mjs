import { test } from "node:test";
import assert from "node:assert/strict";

import {
  PowerLawNoiseType,
  SbasErrorModel,
  SbasKMultipliers,
  allanDeviationPowerLawSlope,
  fitPowerLawNoise,
  fitPreciseEphemerisSampleOrbit,
  loadSp3,
  metricsFromEnuCovarianceM2,
  periodicityStrength,
  propagateState,
  repeatPeriod,
  reliabilityDesign,
  sbasProtectionLevels,
  siderealFilter,
  velocityMidas,
  GnssSystem,
  sp3PreciseEphemerisSamples,
  wtestNoncentrality,
} from "../pkg-node/sidereon.js";

import { f64Bits, fixture } from "./helpers.mjs";

function assertRel(actual, expected, rel, label) {
  const scale = Math.max(1, Math.abs(expected));
  assert.ok(Math.abs(actual - expected) <= rel * scale, `${label}: ${actual} vs ${expected}`);
}

function assertArrayBitsEqual(actual, expected, label) {
  assert.equal(actual.length, expected.length, `${label}: length`);
  for (let i = 0; i < actual.length; i++) {
    assert.equal(f64Bits(actual[i]), f64Bits(expected[i]), `${label}[${i}]`);
  }
}

function assertBitsEqual(actual, expected, label) {
  assert.equal(f64Bits(actual), f64Bits(expected), label);
}

function gps(prn) {
  return `G${String(prn).padStart(2, "0")}`;
}

function lineOfSightFromAzElDeg(azDeg, elDeg) {
  const az = (azDeg * Math.PI) / 180.0;
  const el = (elDeg * Math.PI) / 180.0;
  const cosEl = Math.cos(el);
  const east = cosEl * Math.sin(az);
  const north = cosEl * Math.cos(az);
  const up = Math.sin(el);
  return [up, east, north];
}

function protectionGeometryFromAzEl(azElDeg) {
  return {
    rows: azElDeg.map(([azDeg, elDeg], idx) => ({
      id: gps(idx + 1),
      lineOfSight: lineOfSightFromAzElDeg(azDeg, elDeg),
      system: "G",
      elevationRad: (elDeg * Math.PI) / 180.0,
    })),
    receiver: { latRad: 0.0, lonRad: 0.0, heightM: 0.0 },
    clockSystems: ["G"],
  };
}

test("015 error metrics: isotropic CEP pins 1.177410*sigma within 1e-6 rel and non-PSD is RangeError", () => {
  const sigma = 2.5;
  const metrics = metricsFromEnuCovarianceM2([
    [sigma * sigma, 0, 0],
    [0, sigma * sigma, 0],
    [0, 0, sigma * sigma],
  ]);

  assertRel(metrics.cepM.radiusM / sigma, 1.17741, 1e-6, "CEP/sigma");
  assert.throws(
    () =>
      metricsFromEnuCovarianceM2([
        [1, 0, 0],
        [0, -1, 0],
        [0, 0, 1],
      ]),
    RangeError,
  );
});

test("015 sidereal: under-covered flags pass through and repeat period is pinned", () => {
  assert.equal(repeatPeriod(GnssSystem.Gps), 86164.0905);

  const filtered = siderealFilter([10, 20, 30], 2, {
    sampleIntervalS: 1,
    priorPeriods: 1,
    minCoverage: 2,
  });
  assert.deepEqual(filtered.underCovered, [true, true]);
  assert.deepEqual(filtered.filtered, [10, 20, 30]);

  const strength = periodicityStrength([1, -1, 1, -1, 1, -1], [2], 1);
  assert.equal(strength.length, 1);
  assert.equal(strength[0].periodS, 2);
});

test("015 geodetic time series: MIDAS synthetic velocity matches Rust value to 1e-12", () => {
  const rate = [0.0125, -0.02, 0.004];
  const samples = [2020, 2021, 2022, 2023, 2024].map((epochYear) => {
    const dt = epochYear - 2020;
    return {
      epochYear,
      positionM: [rate[0] * dt, rate[1] * dt, rate[2] * dt],
    };
  });
  const velocity = velocityMidas(
    { frame: "enu", samples },
    { dominantPeriodYears: 1, periodToleranceYears: 0, minPairs: 3 },
  );
  for (let axis = 0; axis < 3; axis++) {
    assertRel(velocity.rateEnuMPerYr[axis], rate[axis], 1e-12, `rate axis ${axis}`);
  }
  assert.equal(velocity.sampleCount, 5);
});

test("015 clock stability: WhiteFM ADEV slope is exact and under-sampled fit is flagged", () => {
  assert.equal(allanDeviationPowerLawSlope(PowerLawNoiseType.WhiteFM), -0.5);

  const adev = {
    tauS: [1, 2],
    deviation: [1, Math.SQRT1_2],
    n: [10, 9],
  };
  const fit = fitPowerLawNoise(adev, adev, {
    basicTauS: 1,
    minPointsPerOctave: 3,
  });
  assert.ok(fit.dominantPerOctave.length > 0);
  assert.equal(fit.dominantPerOctave[0].dominance.kind, "flagged");
  assert.equal(fit.dominantPerOctave[0].dominance.flag, "underSampled");
});

test("015 orbit determination: two-epoch sample fit reports unbounded covariance and low-sample ledger", () => {
  const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const samples = sp3PreciseEphemerisSamples(sp3)
    .filter((sample) => sample.sat === "G01")
    .slice(0, 2);
  const report = fitPreciseEphemerisSampleOrbit(samples, "G01", {
    forceModel: "two_body",
    integrator: "rk4",
    integratorOptions: { initialStepS: 60, maxStepS: 60 },
    solverOptions: { maxNfev: 100 },
    minLedgerSamples: 3,
  });

  assert.equal(report.fits.length, 1);
  assert.equal(report.fits[0].covariance.kind, "unbounded");
  assert.equal(report.ledger.perSatellite[0].stats.lowSampleCount, true);
  assert.equal(report.ledger.perSatellite[0].stats.n, 2);
});

test("015 propagation: composite two-body/J2 with extras disabled matches legacy bits", () => {
  const request = {
    epochS: 0,
    positionKm: [7000, -1210, 1300],
    velocityKmS: [1, 7.2, 0.5],
    timesS: [0, 60, 120],
    integrator: "rk4",
    initialStepS: 10,
  };
  const legacy = propagateState({ ...request, forceModel: "two_body_j2" });
  const composite = propagateState({
    ...request,
    forceModel: {
      kind: "composite",
      twoBody: true,
      zonal: { maxDegree: 2 },
      thirdBody: false,
      solarRadiationPressure: false,
      relativity: false,
    },
  });

  assertArrayBitsEqual(composite.positionKm, legacy.positionKm, "positionKm");
  assertArrayBitsEqual(composite.velocityKmS, legacy.velocityKmS, "velocityKmS");
});

test("015 reliability: Baarda constants are pinned through the WASM API", () => {
  const result = wtestNoncentrality(0.001, 0.8);

  assertBitsEqual(result.delta0, 4.132147965064809, "delta0");
  assertBitsEqual(result.lambda0, 17.074646805189243, "lambda0");
});

test("015 reliability: design redundancy sums and zero-redundancy rows stay null", () => {
  const report = reliabilityDesign(
    [
      { id: "xOnly", designRow: [1, 0], sigmaM: 1 },
      { id: "yA", designRow: [0, 1], sigmaM: 1 },
      { id: "yB", designRow: [0, 1], sigmaM: 1 },
    ],
    undefined,
  );
  const sum = report.perObservation.reduce((acc, obs) => acc + obs.redundancy, 0);

  assertRel(sum, report.summary.dof, 1e-14, "sum per-observation redundancy");
  assertRel(report.summary.sumRedundancy, report.summary.dof, 1e-14, "summary redundancy");

  const xOnly = report.perObservation.find((obs) => obs.id === "xOnly");
  assert.equal(xOnly.uncheckable, true);
  assert.equal(xOnly.mdbM, null);
  assert.equal(xOnly.externalEnuM, null);
  assert.equal(xOnly.biasToNoise, null);
});

test("015 SBAS PL: K constants and fixed weighted geometry match Rust reference", () => {
  const pa = SbasKMultipliers.precisionApproach();
  const enRoute = SbasKMultipliers.enRouteNpa();

  assertBitsEqual(pa.kH, 6.0, "precision kH");
  assertBitsEqual(pa.kV, 5.33, "precision kV");
  assertBitsEqual(enRoute.kH, 6.18, "en-route kH");
  assertBitsEqual(enRoute.kV, 5.33, "en-route kV");

  const geometry = protectionGeometryFromAzEl([
    [15.0, 15.0],
    [80.0, 70.0],
    [155.0, 25.0],
    [230.0, 55.0],
    [310.0, 35.0],
  ]);
  const model = new SbasErrorModel(
    [2.0, 1.0, 1.5, 1.2, 1.8].map((sigmaM, idx) => ({
      id: gps(idx + 1),
      sigmaM,
    })),
  );
  const protection = sbasProtectionLevels(geometry, model, pa);

  assertRel(protection.hplM, 9.064491010405014, 1e-12, "HPL");
  assertRel(protection.vplM, 13.664070819648263, 1e-12, "VPL");
});
