import { test } from "node:test";
import assert from "node:assert/strict";

import {
  ImuSimulator,
  ImuGrade,
  StrapdownMechanizer,
  attitudeYawPitchRollRad,
  correctImuSample,
  dcmToQuaternion,
  defaultImuSimSeed,
  gaussMarkovBiasDecay,
  gravityEcefMps2,
  imuCalibrationFromScalePpm,
  imuRateRandomWalk,
  imuSpecFromDatasheet,
  imuSpecPreset,
  mechanizeEcef,
  normalGravityMps2,
  navStateAttitude,
  normalizeAttitudeQuaternion,
  quaternionToDcm,
  randomWalkBiasTauS,
  reorthonormalizeDcm,
  rodriguesDeltaDcm,
  simulateImuSamples,
  simulateImuSamplesFromIncrements,
  trueImuIncrementBetween,
  validateImuSpec,
  validateImuBias,
  validateImuCalibration,
  validateImuRateRandomWalk,
  validateImuSimulationOptions,
} from "../pkg-node/sidereon.js";

const initialState = {
  tJ2000S: 0,
  positionEcefM: [6_378_137, 0, 0],
  velocityEcefMps: [0, 0, 0],
  attitudeBodyToEcef: [
    [1, 0, 0],
    [0, 1, 0],
    [0, 0, 1],
  ],
};

const imuSpec = {
  accelVrwMpsSqrtS: 0,
  gyroArwRadSqrtS: 0,
  accelBiasInstabMps2: 0,
  gyroBiasInstabRps: 0,
  accelBiasTauS: 1_000,
  gyroBiasTauS: 1_000,
};

test("strapdown mechanizer propagates a typed increment and returns full state", () => {
  const mechanizer = new StrapdownMechanizer(initialState);
  const state = mechanizer.propagate({
    kind: "increment",
    tJ2000S: 1,
    deltaVelocityMps: [0, 0, 0],
    deltaThetaRad: [0, 0, 0],
    dtS: 1,
  });
  assert.equal(state.tJ2000S, 1);
  assert.equal(state.positionEcefM.length, 3);
  assert.equal(state.velocityEcefMps.length, 3);
  assert.equal(state.attitudeBodyToEcef.length, 3);
  mechanizer.free();
});

test("inertial simulator preserves seeded deterministic increments", () => {
  const options = { output: "increment", seed: 19n };
  const first = new ImuSimulator(imuSpec, options);
  const second = new ImuSimulator(imuSpec, options);
  assert.deepEqual(first.bias(), { accelMps2: [0, 0, 0], gyroRps: [0, 0, 0] });
  assert.deepEqual(first.rateRandomWalk(), { accelMps2: [0, 0, 0], gyroRps: [0, 0, 0] });
  const increment = {
    tJ2000S: 1,
    deltaVelocityMps: [0.01, 0, 0],
    deltaThetaRad: [0, 0.001, 0],
    dtS: 1,
  };
  assert.deepEqual(first.sampleIncrement(increment), second.sampleIncrement(increment));
  first.free();
  second.free();
});

test("increment batch simulator and gravity helpers expose core calculations", () => {
  const batch = simulateImuSamplesFromIncrements(
    [{ tJ2000S: 1, deltaVelocityMps: [0, 0, 0], deltaThetaRad: [0, 0, 0], dtS: 1 }],
    imuSpec,
    { output: "increment", seed: 3n },
  );
  assert.equal(batch.samples.length, 1);
  assert.equal(batch.samples[0].kind, "increment");
  assert.equal(Object.getPrototypeOf(batch), Object.prototype);
  assert.equal(Object.getPrototypeOf(batch.samples[0]), Object.prototype);
  assert.equal(Object.getPrototypeOf(batch.biasHistory[0]), Object.prototype);
  assert.equal(Object.getPrototypeOf(batch.rateRandomWalkHistory[0]), Object.prototype);
  const laterState = { ...initialState, tJ2000S: 1 };
  const trueIncrement = trueImuIncrementBetween(initialState, laterState);
  assert.equal(Object.getPrototypeOf(trueIncrement), Object.prototype);
  assert.equal(trueIncrement.tJ2000S, 1);
  assert.equal(trueIncrement.dtS, 1);
  const trajectory = simulateImuSamples([initialState, laterState], imuSpec, { seed: 3n });
  assert.equal(Object.getPrototypeOf(trajectory), Object.prototype);
  assert.equal(trajectory.samples.length, 1);
  assert.ok(Number.isFinite(normalGravityMps2(0, 0)));
  assert.equal(gravityEcefMps2([6_378_137, 0, 0]).length, 3);
});

test("attitude, stand-alone mechanization, and configuration routes delegate to core", () => {
  const quaternion = dcmToQuaternion(initialState.attitudeBodyToEcef);
  assert.deepEqual(quaternion, { w: 1, x: 0, y: 0, z: 0 });
  assert.deepEqual(quaternionToDcm(quaternion), initialState.attitudeBodyToEcef);
  assert.deepEqual(normalizeAttitudeQuaternion({ w: 2, x: 0, y: 0, z: 0 }), quaternion);
  // Core forms pitch as asin(-C[2][0]); for the identity that is asin(-0) = -0,
  // which the binding carries through unchanged.
  assert.deepEqual(attitudeYawPitchRollRad(initialState.attitudeBodyToEcef), [0, -0, 0]);
  assert.deepEqual(
    reorthonormalizeDcm(initialState.attitudeBodyToEcef),
    initialState.attitudeBodyToEcef,
  );
  assert.deepEqual(rodriguesDeltaDcm([0, 0, 0]), initialState.attitudeBodyToEcef);
  const increment = { tJ2000S: 1, deltaVelocityMps: [0, 0, 0], deltaThetaRad: [0, 0, 0], dtS: 1 };
  assert.equal(mechanizeEcef({ state: initialState, increment }).tJ2000S, 1);
  const mechanizer = new StrapdownMechanizer(initialState);
  mechanizer.setConfig({ coningCorrection: "off" });
  mechanizer.free();
  assert.equal(defaultImuSimSeed(), 0x4d59_5df4_d0f3_3173n);
  assert.equal(gaussMarkovBiasDecay(0, randomWalkBiasTauS()), 1);
  assert.equal(validateImuSpec(imuSpecPreset(ImuGrade.Mems)), undefined);
  assert.deepEqual(imuSpecFromDatasheet(imuSpec), imuSpec);
  assert.deepEqual(navStateAttitude(initialState).yawPitchRollRad, [0, -0, 0]);
  const calibration = imuCalibrationFromScalePpm([1, 2, 3], [4, 5, 6]);
  assert.equal(validateImuCalibration(calibration), undefined);
  assert.equal(validateImuBias({ accelMps2: [0, 0, 0], gyroRps: [0, 0, 0] }), undefined);
  const rateWalk = imuRateRandomWalk(0.01, 0.001);
  assert.deepEqual(rateWalk, { accelMps2SqrtS: 0.01, gyroRpsSqrtS: 0.001 });
  assert.equal(validateImuRateRandomWalk(rateWalk), undefined);
  assert.equal(validateImuSimulationOptions({ seed: 7n }), undefined);
  assert.equal(imuSpecPreset(ImuGrade.Tactical).accelVrwMpsSqrtS, 5e-3);
  assert.throws(
    () => validateImuSpec({ ...imuSpec, accelVrwMpsSqrtS: -1 }),
    (error) => {
      assert.equal(error.name, "InertialError");
      assert.deepEqual(error.detail, {
        kind: "INVALID_INPUT",
        field: "accel_vrw_mps_sqrt_s",
        reason: "must be non-negative",
      });
      return true;
    },
  );
});

test("fixed three-vector and 3-by-3 matrix inputs reject trailing components", () => {
  assert.throws(
    () =>
      dcmToQuaternion([
        [1, 0, 0, 9],
        [0, 1, 0],
        [0, 0, 1],
      ]),
    TypeError,
  );
  assert.throws(
    () =>
      dcmToQuaternion([
        [1, 0, 0],
        [0, 1, 0],
        [0, 0, 1],
        [9, 9, 9],
      ]),
    TypeError,
  );
  assert.throws(
    () =>
      validateImuCalibration({
        accelScaleMisalignment: [
          [1, 0, 0],
          [0, 1, 0],
          [0, 0, 1],
          [9, 9, 9],
        ],
        gyroScaleMisalignment: [
          [1, 0, 0],
          [0, 1, 0],
          [0, 0, 1],
        ],
      }),
    TypeError,
  );
  assert.throws(
    () => new StrapdownMechanizer({ ...initialState, positionEcefM: [6_378_137, 0, 0, 9] }),
    TypeError,
  );
});

test("IMU correction returns core values and typed refusals", () => {
  const model = {
    bias: { accelMps2: [0, 0, 0], gyroRps: [0, 0, 0] },
    calibration: {
      accelScaleMisalignment: [
        [0, 0, 0],
        [0, 0, 0],
        [0, 0, 0],
      ],
      gyroScaleMisalignment: [
        [0, 0, 0],
        [0, 0, 0],
        [0, 0, 0],
      ],
    },
  };
  const corrected = correctImuSample(
    {
      kind: "rate",
      tJ2000S: 1,
      specificForceMps2: [0, 0, 0],
      angularRateRps: [0, 0, 0],
    },
    0,
    model,
  );
  assert.deepEqual(corrected.deltaVelocityMps, [0, 0, 0]);
  assert.throws(
    () =>
      correctImuSample(
        {
          kind: "increment",
          tJ2000S: 0,
          deltaVelocityMps: [0, 0, 0],
          deltaThetaRad: [0, 0, 0],
          dtS: 1,
        },
        0,
        model,
      ),
    (error) => {
      assert.equal(error.name, "InertialError");
      assert.deepEqual(error.detail, { kind: "NON_MONOTONIC_SAMPLE" });
      return true;
    },
  );
  const singularModel = {
    ...model,
    calibration: {
      accelScaleMisalignment: [
        [-1, 0, 0],
        [0, 0, 0],
        [0, 0, 0],
      ],
      gyroScaleMisalignment: [
        [0, 0, 0],
        [0, 0, 0],
        [0, 0, 0],
      ],
    },
  };
  assert.throws(
    () =>
      correctImuSample(
        {
          kind: "rate",
          tJ2000S: 1,
          specificForceMps2: [0, 0, 0],
          angularRateRps: [0, 0, 0],
        },
        0,
        singularModel,
      ),
    (error) => {
      assert.equal(error.name, "InertialError");
      assert.deepEqual(error.detail, { kind: "SINGULAR_CALIBRATION" });
      return true;
    },
  );
});

test("inertial input objects reject unknown keys at every nested entry point", () => {
  const assertUnknownKey = (action, key) => {
    assert.throws(action, (error) => {
      assert.equal(error.name, "TypeError");
      assert.match(error.message, new RegExp(key));
      return true;
    });
  };
  assertUnknownKey(
    () => new StrapdownMechanizer({ ...initialState, positonEcefM: [0, 0, 0] }),
    "positonEcefM",
  );
  const mechanizer = new StrapdownMechanizer(initialState);
  assertUnknownKey(
    () => mechanizer.setConfig({ coningCorrection: "off", coningCorrecton: "off" }),
    "coningCorrecton",
  );
  assertUnknownKey(
    () =>
      mechanizer.setImuErrorModel({
        bias: { accelMps2: [0, 0, 0], gyroRps: [0, 0, 0], gyroRpp: [0, 0, 0] },
        calibration: {
          accelScaleMisalignment: [
            [0, 0, 0],
            [0, 0, 0],
            [0, 0, 0],
          ],
          gyroScaleMisalignment: [
            [0, 0, 0],
            [0, 0, 0],
            [0, 0, 0],
          ],
        },
      }),
    "gyroRpp",
  );
  mechanizer.free();
  assertUnknownKey(
    () =>
      validateImuSimulationOptions({
        seed: 3n,
        calibration: {
          accelScaleMisalignment: [
            [0, 0, 0],
            [0, 0, 0],
            [0, 0, 0],
          ],
          gyroScaleMisalignment: [
            [0, 0, 0],
            [0, 0, 0],
            [0, 0, 0],
          ],
          gyroScaleMisalignmnt: [
            [0, 0, 0],
            [0, 0, 0],
            [0, 0, 0],
          ],
        },
      }),
    "gyroScaleMisalignmnt",
  );
  assertUnknownKey(
    () =>
      simulateImuSamples(
        [
          { ...initialState, gyroBiasRpsTypo: [0, 0, 0] },
          { ...initialState, tJ2000S: 1 },
        ],
        imuSpec,
        { seed: 5n },
      ),
    "gyroBiasRpsTypo",
  );
  assertUnknownKey(
    () =>
      simulateImuSamplesFromIncrements(
        [{ tJ2000S: 1, deltaVelocityMps: [0, 0, 0], deltaThetaRad: [0, 0, 0], dtS: 1, dtTypo: 1 }],
        imuSpec,
        { seed: 7n },
      ),
    "dtTypo",
  );
  assert.equal(validateImuSimulationOptions({ seed: 0xffff_ffff_ffff_ffffn }), undefined);
});
