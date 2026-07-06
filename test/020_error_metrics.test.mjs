import { test } from "node:test";
import assert from "node:assert/strict";

import {
  errorEllipseFromEnuM2,
  horizontalRadiusAt,
  metricsFromEcefCovarianceM2,
  metricsFromEnuCovarianceM2,
  metricsFromKinematicSolution,
  metricsFromPositionCovariance,
  sphericalRadiusAt,
  verticalRadiusAt,
} from "../pkg-node/sidereon.js";

function assertRel(actual, expected, rel, label) {
  const scale = Math.max(1, Math.abs(expected));
  assert.ok(Math.abs(actual - expected) <= rel * scale, `${label}: ${actual} vs ${expected}`);
}

function assertRadius(actual, expected, label) {
  assertRel(actual.probability, expected.probability, 0, `${label} probability`);
  assertRel(actual.radiusM, expected.radiusM, 1e-12, `${label} radius`);
  assertRel(actual.approxM, expected.approxM, 1e-12, `${label} approx`);
  assert.equal(actual.approxValid, expected.approxValid, `${label} approxValid`);
}

function assertMetricsClose(actual, expected, label) {
  assertRel(actual.sigmaEM, expected.sigmaEM, 1e-12, `${label} sigmaE`);
  assertRel(actual.sigmaNM, expected.sigmaNM, 1e-12, `${label} sigmaN`);
  assertRel(actual.sigmaUM, expected.sigmaUM, 1e-12, `${label} sigmaU`);
  assertRel(actual.drmsM, expected.drmsM, 1e-12, `${label} drms`);
  assertRel(actual.twoDrmsM, expected.twoDrmsM, 1e-12, `${label} twoDrms`);
  assertRel(actual.vepM, expected.vepM, 1e-12, `${label} vep`);
  assertRel(actual.mrseM, expected.mrseM, 1e-12, `${label} mrse`);
  assertRel(actual.ellipse.semiMajorM, expected.ellipse.semiMajorM, 1e-12, `${label} major`);
  assertRel(actual.ellipse.semiMinorM, expected.ellipse.semiMinorM, 1e-12, `${label} minor`);
  assertRel(actual.ellipse.orientationRad, expected.ellipse.orientationRad, 1e-12, `${label} az`);
  assertRadius(actual.cepM, expected.cepM, `${label} cep`);
  assertRadius(actual.r95M, expected.r95M, `${label} r95`);
  assertRadius(actual.r99M, expected.r99M, `${label} r99`);
  assertRadius(actual.sepM, expected.sepM, `${label} sep`);
}

function transpose3(m) {
  return [
    [m[0][0], m[1][0], m[2][0]],
    [m[0][1], m[1][1], m[2][1]],
    [m[0][2], m[1][2], m[2][2]],
  ];
}

function matMul3(a, b) {
  return a.map((row) =>
    [0, 1, 2].map((j) => row[0] * b[0][j] + row[1] * b[1][j] + row[2] * b[2][j]),
  );
}

function ecefCovarianceFromEnu(covarianceEnuM2, receiver) {
  const sinLat = Math.sin(receiver.latRad);
  const cosLat = Math.cos(receiver.latRad);
  const sinLon = Math.sin(receiver.lonRad);
  const cosLon = Math.cos(receiver.lonRad);
  const rotation = [
    [-sinLon, cosLon, 0],
    [-sinLat * cosLon, -sinLat * sinLon, cosLat],
    [cosLat * cosLon, cosLat * sinLon, sinLat],
  ];
  return matMul3(transpose3(rotation), matMul3(covarianceEnuM2, rotation));
}

test("020 error metrics: circular covariance oracles and helpers", () => {
  const sigmaM = 3.25;
  const covariance = [
    [sigmaM * sigmaM, 0, 0],
    [0, sigmaM * sigmaM, 0],
    [0, 0, sigmaM * sigmaM],
  ];
  const metrics = metricsFromEnuCovarianceM2(covariance);

  const expectedCep50 = Math.sqrt(2 * Math.log(2)) * sigmaM;
  const expectedR95 = Math.sqrt(-2 * Math.log(1 - 0.95)) * sigmaM;
  const expectedDrms = Math.SQRT2 * sigmaM;
  assertRel(metrics.cepM.radiusM, expectedCep50, 1e-12, "cep50");
  assertRel(metrics.r95M.radiusM, expectedR95, 1e-12, "r95");
  assertRel(metrics.drmsM, expectedDrms, 1e-12, "drms");
  assertRel(metrics.twoDrmsM, 2 * expectedDrms, 1e-12, "twoDrms");

  const ellipse = errorEllipseFromEnuM2(covariance);
  assertRel(ellipse.semiMajorM, sigmaM, 1e-12, "ellipse major");
  assertRel(ellipse.semiMinorM, sigmaM, 1e-12, "ellipse minor");
  assertRel(ellipse.orientationRad, 0, 1e-12, "ellipse orientation");

  assertRadius(horizontalRadiusAt(covariance, 0.95), metrics.r95M, "horizontal");
  assertRadius(sphericalRadiusAt(covariance, 0.5), metrics.sepM, "spherical");
  assertRel(verticalRadiusAt(sigmaM * sigmaM, 0.5), 0.6744897501960817 * sigmaM, 1e-12, "vertical");
  assertRel(metrics.vepM, 0.67449 * sigmaM, 1e-12, "metric vep");

  const fromPositionCovariance = metricsFromPositionCovariance({
    ecefM2: covariance.map((row) => row.map((value) => value * 2)),
    enuM2: covariance,
  });
  assertMetricsClose(fromPositionCovariance, metrics, "positionCovariance");
});

test("020 error metrics: elongated covariance ellipse oracle", () => {
  const covariance = [
    [9, 2, 0],
    [2, 4, 0],
    [0, 0, 1.44],
  ];
  const ellipse = errorEllipseFromEnuM2(covariance);

  const trace = covariance[0][0] + covariance[1][1];
  const delta = Math.sqrt((covariance[0][0] - covariance[1][1]) ** 2 + 4 * covariance[0][1] ** 2);
  const majorLambda = 0.5 * (trace + delta);
  const minorLambda = 0.5 * (trace - delta);
  assertRel(ellipse.semiMajorM, Math.sqrt(majorLambda), 1e-12, "major");
  assertRel(ellipse.semiMinorM, Math.sqrt(minorLambda), 1e-12, "minor");
  assertRel(ellipse.orientationRad, 0.5 * Math.atan2(4, 5), 1e-12, "orientation");

  const metrics = metricsFromEnuCovarianceM2(covariance);
  assert.equal(metrics.cepM.approxValid, true);
  const expectedCepApprox = 0.6152 * Math.sqrt(majorLambda) + 0.562 * Math.sqrt(minorLambda);
  assertRel(metrics.cepM.approxM, expectedCepApprox, 1e-12, "cep approx");
  assert.ok(Math.abs(metrics.cepM.radiusM - metrics.cepM.approxM) / metrics.cepM.radiusM < 0.03);
});

test("020 error metrics: ECEF and kinematic paths agree with rotated ENU", () => {
  const receiver = { latRad: 0, lonRad: 0, heightM: 0 };
  const covarianceEnu = [
    [5, 0.25, 0.1],
    [0.25, 2, -0.2],
    [0.1, -0.2, 1.25],
  ];
  const covarianceEcef = ecefCovarianceFromEnu(covarianceEnu, receiver);

  const fromEnu = metricsFromEnuCovarianceM2(covarianceEnu);
  const fromEcef = metricsFromEcefCovarianceM2(covarianceEcef, receiver);
  assertMetricsClose(fromEcef, fromEnu, "ecef");

  const fromKinematic = metricsFromKinematicSolution({
    positionM: [6378137, 0, 0],
    positionCovarianceM2: covarianceEcef,
  });
  assertMetricsClose(fromKinematic, fromEnu, "kinematic");
});
