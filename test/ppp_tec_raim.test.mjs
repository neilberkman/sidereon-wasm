import test from "node:test";
import assert from "node:assert/strict";

import {
  PppRaimConfig,
  PppTecConfig,
  VMF_SITE_MAX_SAMPLES,
  VMF_SITE_SINGLE_SAMPLE_CLAMP_DAYS,
  pppEstimateTec,
  pppIonosphericPiercePoint,
  pppLevelSlantTecArc,
  pppRaimGlobalTest,
  pppSlantTecFromCodeGeometryFreeM,
  pppTecGroupDelayCoefficient,
  pppThinShellMappingFunction,
} from "../pkg-node/sidereon.js";

test("precise-positioning constants and config values are exact", () => {
  assert.equal(VMF_SITE_MAX_SAMPLES(), 8);
  assert.equal(VMF_SITE_SINGLE_SAMPLE_CLAMP_DAYS(), 0.25);
  assert.equal(pppTecGroupDelayCoefficient(), 403080000000000000);
  const tec = PppTecConfig.default();
  assert.equal(tec.shellHeightM, 350000);
  assert.equal(tec.earthRadiusM, 6371000);
  assert.ok(tec.equals(new PppTecConfig(350000, 6371000)));
  assert.ok(!tec.equals(new PppTecConfig(400000, 6371000)));
  const raim = PppRaimConfig.default();
  assert.equal(raim.missedDetectionProbability, 0.001);
  assert.equal(raim.measurementSigmaM, 1);
  assert.equal(raim.chiSquareThreshold, undefined);
  assert.ok(raim.equals(PppRaimConfig.default()));
});

test("TEC leveling, mapping, pierce point, and stream estimation preserve exact core values", () => {
  assert.equal(pppThinShellMappingFunction(Math.PI / 2), 1);
  assert.equal(pppSlantTecFromCodeGeometryFreeM(0, 1575.42e6, 1227.6e6), 0);
  const leveled = pppLevelSlantTecArc([
    { codeSlantTecTecu: 20, phaseSlantTecTecu: 25, elevationRad: Math.PI / 2 },
    { codeSlantTecTecu: 22, phaseSlantTecTecu: 27, elevationRad: Math.PI / 2 },
  ]).toJSON();
  assert.deepEqual(leveled, {
    phaseBiasTecu: 5,
    samples: [
      {
        codeSlantTecTecu: 20,
        phaseSlantTecTecu: 25,
        leveledSlantTecTecu: 20,
        mappingFunction: 1,
        verticalTecTecu: 20,
      },
      {
        codeSlantTecTecu: 22,
        phaseSlantTecTecu: 27,
        leveledSlantTecTecu: 22,
        mappingFunction: 1,
        verticalTecTecu: 22,
      },
    ],
  });
  const ipp = pppIonosphericPiercePoint(0, 0, Math.PI / 2, 0).toJSON();
  assert.deepEqual(ipp, {
    latitudeRad: -5.804363009498428e-17,
    longitudeRad: 0,
    latitudeDeg: -3.325655032061129e-15,
    longitudeDeg: 0,
    earthCentralAngleRad: -5.804363009498428e-17,
    shellHeightM: 350000,
  });

  const coefficient = pppTecGroupDelayCoefficient();
  const f1Hz = 1575.42e6;
  const f2Hz = 1227.6e6;
  const denominator = coefficient * (1 / f1Hz ** 2 - 1 / f2Hz ** 2);
  const observation = {
    satelliteId: "G01",
    ambiguityId: "arc",
    p1M: 0,
    p2M: -denominator * 20,
    phi1Cyc: (-denominator * 25) / (299792458 / f1Hz),
    phi2Cyc: 0,
    f1Hz,
    f2Hz,
  };
  const epochs = [0, 30].map((timeS) => ({
    timeS,
    receiverLatitudeRad: 0,
    receiverLongitudeRad: 0,
    observations: [{ observation, elevationRad: Math.PI / 2, azimuthRad: 0 }],
  }));
  const estimate = pppEstimateTec(epochs).toJSON();
  assert.equal(estimate.arcs.length, 1);
  assert.equal(estimate.arcs[0].satelliteId, "G01");
  assert.equal(estimate.arcs[0].ambiguityId, "arc");
  assert.equal(estimate.arcs[0].phaseBiasTecu, 4.9999999999999964);
  assert.equal(estimate.arcs[0].samples[0].verticalTecTecu, 20.000000000000004);
});

test("TEC refusals retain exact typed kinds", () => {
  assert.throws(
    () => pppLevelSlantTecArc([]),
    (error) => {
      assert.equal(error.details.family, "TecError");
      assert.equal(error.details.kind, "EMPTY_ARC");
      assert.equal(error.details.message, "TEC leveling arc must contain at least one sample");
      return true;
    },
  );
  assert.throws(
    () => new PppTecConfig(0, 6371000).validate(),
    (error) => {
      assert.equal(error.details.kind, "INVALID_SHELL_HEIGHT");
      return true;
    },
  );
});

test("PPP snapshot RAIM global test returns exact statistics and typed refusals", () => {
  const residuals = [
    {
      epochIndex: 0,
      satelliteId: "G01",
      ambiguityId: "G01",
      codeM: 3,
      phaseM: 4,
      codeWeight: 1,
      phaseWeight: 1,
    },
    {
      epochIndex: 0,
      satelliteId: "G02",
      ambiguityId: "G02",
      codeM: 0,
      phaseM: 0,
      codeWeight: 1,
      phaseWeight: 1,
    },
  ];
  const result = pppRaimGlobalTest(residuals, 3, new PppRaimConfig(0.001, 0.001, 1, 20));
  assert.equal(result.status, "faultDetected");
  assert.equal(result.detected, true);
  assert.equal(result.testStatistic, 25);
  assert.equal(result.threshold, 20);
  assert.equal(result.redundancy, 1);
  assert.ok(result.equals(pppRaimGlobalTest(residuals, 3, new PppRaimConfig(0.001, 0.001, 1, 20))));
  assert.throws(
    () => pppRaimGlobalTest(residuals, 3, new PppRaimConfig(0, 0.001, 1, 20)),
    (error) => {
      assert.deepEqual(error.details, {
        family: "RaimError",
        kind: "INVALID_CONFIG",
        field: "false_alarm_probability",
        reason: "must be inside (0, 1)",
        message: "invalid RAIM configuration false_alarm_probability: must be inside (0, 1)",
      });
      return true;
    },
  );
});
