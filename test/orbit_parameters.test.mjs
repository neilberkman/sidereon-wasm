import { test } from "node:test";
import assert from "node:assert/strict";

import * as orbit from "../pkg-node/sidereon.js";
import { fixture, fixtureJson } from "./helpers.mjs";

const goldens = fixtureJson("astrodynamics.json");
const expected = goldens.parameterFit;
const seed = () =>
  new orbit.CartesianState(
    expected.seedEpoch,
    Float64Array.from(expected.seed[0]),
    Float64Array.from(expected.seed[1]),
  );
const samples = () =>
  expected.positionsM.map((position, i) => {
    using epoch = orbit.ClockInstant.fromNanos(orbit.TimeScale.Tt, BigInt(expected.nanos[i]));
    return new orbit.InertialPositionSample(epoch, Float64Array.from(position));
  });
const typed = (detail) => (error) => {
  assert.equal(error.name, "OrbitParameterFitError");
  assert.equal(error.kind, detail.kind);
  assert.deepEqual(error.detail, detail);
  if (detail.kind === "fit") assert.equal(error.toString(), detail.cause.message);
  return true;
};
const parameterName = "empirical.radial.constant";
const options = (estimatedParameters, additionalForces = []) =>
  new orbit.OrbitParameterFitOptions({
    forceModel: { kind: "composite", twoBody: false },
    estimatedParameters,
    additionalForces,
  });

test("configure drag in the fit options", () => {
  using settings = options([]);
  using weather = new orbit.SpaceWeather(120, 130, 5);
  using drag = orbit.DragForce.fromBcFactor(0.02, weather, 150);
  settings.setDrag(drag);
  using configured = settings.drag;
  using actual = configured.spaceWeather;
  assert.equal(configured.bcFactorM2Kg, 0.02);
  assert.equal(configured.cutoffAltitudeKm, 150);
  assert.deepEqual([actual.f107, actual.f107a, actual.ap], [120, 130, 5]);
});

test("fit a radial acceleration parameter and Cartesian state to inertial positions", () => {
  using context = new orbit.PropagationContext();
  using model = orbit.EmpiricalRtnAcceleration.withZeroTerms([
    [orbit.RtnAxis.Radial, orbit.EmpiricalHarmonic.Constant],
  ]);
  using force = model.asForceModel();
  using optionsValue = options([parameterName], [force]);
  optionsValue.setPropagationContext(context);
  assert.deepEqual(optionsValue.estimatedParameters, [parameterName]);
  const [additional] = optionsValue.additionalForces;
  using configured = additional;
  using configuredContext = optionsValue.propagationContext;
  assert.equal(configuredContext.ut1Departure(), undefined);
  assert.equal(configured.parameterCount(), 1);
  assert.equal(optionsValue.observationSigmaM, 1);
  assert.equal(optionsValue.maxIterations, 25);
  assert.equal(optionsValue.convergenceThreshold, 1e-3);
  assert.equal(optionsValue.maxConditionNumber, 1e12);
  assert.equal(optionsValue.minLedgerSamples, 3);
  assert.equal(optionsValue.integrator, "dp54");
  assert.deepEqual(optionsValue.integratorOptions, {
    absTol: 1e-9,
    relTol: 1e-12,
    initialStepS: 60,
    minStepS: 1e-6,
    maxStepS: 3600,
    maxSteps: 1000000,
  });

  const observations = samples();
  const separatelyBuilt = samples();
  try {
    assert.ok(observations[0].equals(separatelyBuilt[0]));
    assert.ok(!observations[0].equals(observations[1]));
    using epoch = observations[0].epoch;
    assert.equal(epoch.nanos, 0n);
    assert.deepEqual(Array.from(observations[0].positionGcrfM), expected.positionsM[0]);
  } finally {
    for (const sample of separatelyBuilt) sample.free();
  }
  using fit = orbit.fitInertialOrbitParameters(observations, "G02", seed(), optionsValue);
  const validated = orbit.fitInertialOrbitParametersWithValidity(
    samples(),
    "G02",
    seed(),
    optionsValue,
    "strict",
  );
  using validatedFit = validated.value;
  assert.equal(validated.ut1Degraded, null);
  assert.ok(fit.equals(validatedFit));
  using anotherSatellite = orbit.fitInertialOrbitParameters(samples(), "G03", seed(), optionsValue);
  assert.equal(anotherSatellite.satellite, "G03");
  assert.ok(!fit.equals(anotherSatellite));
  assert.equal(fit.satellite, "G02");
  using initial = fit.initialState;
  assert.equal(initial.epochTdbSeconds, expected.stateEpoch);
  assert.deepEqual(
    [Array.from(initial.positionKm), Array.from(initial.velocityKmS)],
    expected.state,
  );
  const names = ["x_km", "y_km", "z_km", "vx_km_s", "vy_km_s", "vz_km_s"];
  assert.deepEqual(orbit.STATE_PARAMETER_NAMES(), names);
  assert.deepEqual(fit.parameterNames, [...names, parameterName]);
  assert.equal(fit.iterations, expected.iterations);
  assert.equal(fit.conditionNumber, expected.conditionNumber);
  assert.equal(fit.seedRms3dM, expected.seedRms);
  assert.equal(fit.residualStats.rms3dM, expected.rms);
  assert.equal(fit.sigma(parameterName), expected.sigma);
  assert.equal(fit.sigma("unknown"), undefined);
  using estimate = fit.forceParameter(parameterName);
  const [parameter] = fit.forceParameters;
  using listed = parameter;
  assert.ok(estimate.equals(listed));
  assert.equal(estimate.name, parameterName);
  assert.equal(estimate.initialValue, 0);
  assert.equal(estimate.value, expected.parameter);
  assert.equal(fit.forceParameter("unknown"), undefined);

  using covariance = fit.covariance;
  using unbounded = orbit.OrbitParameterCovariance.UNBOUNDED();
  assert.equal(unbounded.isEstimated, false);
  assert.equal(unbounded.matrix, undefined);
  assert.equal(unbounded.correlation, undefined);
  assert.equal(unbounded.varianceFactor, undefined);
  assert.equal(unbounded.degreesOfFreedom, undefined);
  using covarianceCopy = validatedFit.covariance;
  assert.ok(covariance.equals(covarianceCopy));
  assert.ok(!covariance.equals(unbounded));
  assert.equal(covariance.isEstimated, true);
  assert.deepEqual([covariance.varianceFactor, covariance.degreesOfFreedom], expected.variance);
  assert.equal(covariance.matrix.length, 49);
  assert.equal(covariance.correlation[48], 1);
  assert.equal(fit.correlation(parameterName, parameterName), 1);
  const residuals = fit.residuals;
  const validatedResiduals = validatedFit.residuals;
  try {
    assert.ok(residuals[0].equals(validatedResiduals[0]));
    assert.deepEqual(
      residuals.map((value) => [
        value.epochTdbJ2000S,
        value.elapsedTtS,
        value.radialM,
        value.alongM,
        value.crossM,
      ]),
      expected.residuals,
    );
    using epoch = residuals[1].variationalEpoch();
    assert.equal(epoch.tdbJ2000S, expected.residuals[1][0]);
    assert.equal(epoch.elapsedTtS, expected.residuals[1][1]);
    assert.ok(!residuals[0].equals(residuals[1]));
  } finally {
    for (const residual of [...residuals, ...validatedResiduals]) residual.free();
  }
});

test("parameter fitting refuses invalid observations or an unlisted parameter", () => {
  using requested = options([parameterName]);
  assert.throws(
    () => orbit.fitInertialOrbitParameters(samples(), "G02", seed(), requested),
    typed({
      kind: "invalidParameter",
      name: parameterName,
      reason: "is not a parameter of the force models",
    }),
  );
  assert.throws(
    () => orbit.fitInertialOrbitParameters([], "G02", undefined, requested),
    typed({
      kind: "fit",
      cause: {
        kind: "TOO_FEW_SAMPLES",
        satellite: "G02",
        got: 0,
        required: 2,
        message: "satellite G02 has 0 samples; need at least 2",
      },
    }),
  );
});

test("an IERS provider can supply orientation to the SP3 fit", () => {
  using product = orbit.loadSp3(fixture("sp3/g02_ecef_two_epoch.sp3"));
  const samples = [61159, 61160, 61161].map(
    (day) => new orbit.Iers2010EopSample(day, 0, 0, 0, 0, 0, 0),
  );
  using provider = new orbit.Iers2010EarthOrientationProvider(samples);
  using permissive = provider.withValidity("permissive");
  using fitOptions = options([]);
  assert.throws(
    () => orbit.fitSp3EcefOrbitParameters(product, "G02", permissive, undefined, fitOptions),
    typed({
      kind: "fit",
      cause: {
        kind: "UT1_VALIDITY_MISMATCH",
        fit: "strict",
        provider: "orientation",
        providerMode: "permissive",
        message: "orbit fit UT1 mode Strict differs from the Orientation provider's Permissive",
      },
    }),
  );
  const result = orbit.fitSp3EcefOrbitParametersWithValidity(
    product,
    "G02",
    provider,
    undefined,
    fitOptions,
    "strict",
  );
  using fit = result.value;
  using implicitValidity = orbit.fitSp3EcefOrbitParameters(
    product,
    "G02",
    provider,
    undefined,
    fitOptions,
  );
  for (const fitted of [fit, implicitValidity]) {
    using state = fitted.initialState;
    assert.equal(state.epochTdbSeconds, goldens.sp3ParameterFit.epoch);
    assert.deepEqual(
      [Array.from(state.positionKm), Array.from(state.velocityKmS)],
      goldens.sp3ParameterFit.state,
    );
  }
  assert.ok(fit.equals(implicitValidity));
  using covariance = fit.covariance;
  assert.equal(result.ut1Degraded, null);
  assert.equal(fit.satellite, "G02");
  assert.equal(covariance.isEstimated, false);
});

test("parameter fit diagnoses rank, convergence and numerical failures", () => {
  using model = orbit.EmpiricalRtnAcceleration.withZeroTerms([
    [orbit.RtnAxis.Radial, orbit.EmpiricalHarmonic.Constant],
  ]);
  using force = model.asForceModel();
  const configure = (fields) => {
    const settings = new orbit.OrbitParameterFitOptions({
      forceModel: { kind: "composite", twoBody: false },
      estimatedParameters: [parameterName],
      additionalForces: [force],
      ...fields,
    });
    return settings;
  };
  using normal = configure({});
  const observations = samples();
  observations.pop().free();
  observations.pop().free();
  assert.throws(
    () => orbit.fitInertialOrbitParameters(observations, "G02", seed(), normal),
    typed({
      kind: "singularNormalEquations",
      satellite: "G02",
      conditionNumber: { decimal: "inf", bitsHex: "7ff0000000000000" },
      weakest: [],
    }),
  );
  using illConditioned = configure({ maxConditionNumber: 2 });
  assert.throws(
    () => orbit.fitInertialOrbitParameters(samples(), "G02", seed(), illConditioned),
    typed({ kind: "illConditionedNormalEquations", satellite: "G02", ...expected.illConditioned }),
  );
  using nonconvergent = configure({ maxIterations: 1, convergenceThreshold: 1e-20 });
  assert.throws(
    () => orbit.fitInertialOrbitParameters(samples(), "G02", seed(), nonconvergent),
    typed({ kind: "didNotConverge", satellite: "G02", ...expected.nonconvergent }),
  );

  using tinyForce = new orbit.SolarRadiationPressure(1, Number.MIN_VALUE);
  using radiation = tinyForce.asForceModel();
  using srp = options(["srp.cr"], [radiation]);
  assert.throws(
    () => orbit.fitInertialOrbitParameters(samples(), "G02", seed(), srp),
    typed({ kind: "unobservableParameter", satellite: "G02", name: "srp.cr" }),
  );

  const huge = expected.nanos.map((value) => {
    using epoch = orbit.ClockInstant.fromNanos(orbit.TimeScale.Tt, BigInt(value));
    return new orbit.InertialPositionSample(epoch, Float64Array.of(1e308, 1e308, 1e308));
  });
  assert.throws(
    () => orbit.fitInertialOrbitParameters(huge, "G02", seed(), normal),
    typed({ kind: "numericalFailure", satellite: "G02", quantity: "sum of squared residuals" }),
  );
});

test("parameter fits report invalid input, frame, propagation and residual geometry details", () => {
  const position = Float64Array.of(7000, 0, 0);
  const zero = Float64Array.of(0, 0, 0);
  const sample = (epoch, r) => {
    using instant = orbit.ClockInstant.fromNanos(orbit.TimeScale.Tdb, BigInt(epoch) * 1000000000n);
    return new orbit.InertialPositionSample(instant, r);
  };
  const invalid = (kind, message, fields) =>
    typed({ kind: "fit", cause: { kind, message, ...fields } });
  using settings = new orbit.OrbitParameterFitOptions();
  const configuredForces = fixtureJson("astrodynamics.json").fitForceConfiguration;
  using fitForce = settings.forceModel;
  using forceState = new orbit.CartesianState(0, position, Float64Array.of(0, 7.5, 0));
  using forceContext = new orbit.PropagationContext();
  assert.deepEqual(
    Array.from(fitForce.acceleration(forceState, forceContext)),
    configuredForces.default,
  );
  using j2Settings = new orbit.OrbitParameterFitOptions({ forceModel: "two_body_j2" });
  using j2Force = j2Settings.forceModel;
  assert.deepEqual(
    Array.from(j2Force.acceleration(forceState, forceContext)),
    configuredForces.twoBodyJ2,
  );
  assert.equal(settings.spaceWeather, undefined);
  using table = orbit.parseSpaceWeather(fixture("space_weather/SW-All-20260702-trim.csv"));
  settings.setSpaceWeather(table);
  using configured = settings.spaceWeather;
  assert.equal(configured.day(2003, 10, 27).apAvg, 11);

  const satellite = "G02";
  const invalidObservation = "position components must be finite";
  assert.throws(
    () =>
      orbit.fitInertialOrbitParameters(
        [sample(0, Float64Array.of(NaN, 0, 0))],
        satellite,
        undefined,
        settings,
      ),
    invalid(
      "INVALID_OBSERVATION",
      `satellite G02 has an invalid observation: ${invalidObservation}`,
      { satellite, reason: invalidObservation },
    ),
  );
  const observations = () =>
    [0, 120, 240].map((epoch) => sample(epoch, Float64Array.of(7e6, 0, 0)));
  assert.throws(
    () => orbit.fitInertialOrbitParameters(observations(), satellite, undefined, settings),
    invalid(
      "PROPAGATION",
      "satellite G02 propagation failed: Invalid input: space weather source without drag",
      {
        satellite,
        source: { kind: "invalidInput", message: "space weather source without drag" },
      },
    ),
  );
  using defaults = new orbit.OrbitParameterFitOptions();
  const badEpoch = new orbit.CartesianState(1, position, zero);
  using overflow = new orbit.OrbitParameterFitOptions({ forceModel: { twoBody: false } });
  assert.throws(
    () =>
      orbit.fitInertialOrbitParameters(
        observations(),
        satellite,
        new orbit.CartesianState(2 ** -36, position, Float64Array.of(Number.MAX_VALUE, 0, 0)),
        overflow,
      ),
    invalid(
      "PROPAGATION",
      "satellite G02 propagation failed: Numerical failure: state not finite within a step",
      {
        satellite,
        source: { kind: "numericalFailure", message: "state not finite within a step" },
      },
    ),
  );
  using weather = new orbit.SpaceWeather(120, 130, 5);
  using drag = orbit.DragForce.fromBcFactor(0.02, weather, 90);
  settings.setDrag(drag);
  const initial = () => new orbit.CartesianState(2 ** -36, position, zero);
  const sourceFailure = "space weather: space-weather lookup before coverage";
  assert.throws(
    () => orbit.fitInertialOrbitParameters(observations(), satellite, initial(), settings),
    invalid(
      "PROPAGATION",
      `satellite G02 propagation failed: Force model failure: ${sourceFailure}`,
      {
        satellite,
        source: { kind: "forceModelFailure", message: sourceFailure },
      },
    ),
  );
  const invalidEpoch = "initial-state seed epoch must match the first sample";
  assert.throws(
    () => orbit.fitInertialOrbitParameters(observations(), satellite, badEpoch, defaults),
    invalid("INVALID_EPOCH", `satellite G02 has an invalid epoch: ${invalidEpoch}`, {
      satellite,
      reason: invalidEpoch,
    }),
  );
  assert.throws(
    () =>
      orbit.fitInertialOrbitParameters(
        [sample(0, position), sample(0, position)],
        satellite,
        undefined,
        defaults,
      ),
    invalid("NON_MONOTONIC_EPOCHS", "satellite G02 sample epochs are not strictly increasing", {
      satellite,
    }),
  );
  using invalidOption = new orbit.OrbitParameterFitOptions({ minLedgerSamples: 0 });
  assert.throws(
    () => orbit.fitInertialOrbitParameters(observations(), satellite, undefined, invalidOption),
    invalid("INVALID_OPTION", "invalid orbit-fit min_ledger_samples: not positive", {
      field: "min_ledger_samples",
      reason: "not positive",
    }),
  );
  using short = new orbit.OrbitParameterFitOptions({
    integratorOptions: { maxSteps: 1, initialStepS: 1 },
  });
  assert.throws(
    () => orbit.fitInertialOrbitParameters(samples(), satellite, seed(), short),
    invalid("PROPAGATION", "satellite G02 propagation failed: Maximum number of steps exceeded", {
      satellite,
      source: { kind: "maxStepsExceeded" },
    }),
  );

  for (const [r, mu, variant, kind] of [
    [7000, Number.MIN_VALUE, "ParallelPositionVelocity", "parallelPositionVelocity"],
    [2 ** -110, 0, "ZeroPosition", "zeroPosition"],
  ]) {
    using noGravity = new orbit.OrbitParameterFitOptions({
      forceModel: { kind: "two_body", muKm3S2: mu },
    });
    const initial = new orbit.CartesianState(2 ** -36, Float64Array.of(r, 0, 0), zero);
    const nodes = [0, 120, 240].map((epoch) => sample(epoch, Float64Array.of(r * 1000, 0, 0)));
    assert.throws(
      () => orbit.fitInertialOrbitParameters(nodes, satellite, initial, noGravity),
      invalid("RTN_FRAME", `satellite G02 RTN frame failed: ${variant}`, {
        satellite,
        source: { kind },
      }),
    );
  }
  using product = orbit.loadSp3(fixture("sp3/g02_ecef_two_epoch.sp3"));
  using future = new orbit.Iers2010EarthOrientationProvider(
    [61161, 61162].map((day) => new orbit.Iers2010EopSample(day, 0, 0, 0, 0, 0, 0)),
  );
  using context = new orbit.PropagationContext();
  using strict = context.withBodyFixedFrameProvider(future);
  settings.setPropagationContext(strict);
  assert.throws(
    () => orbit.fitInertialOrbitParameters(observations(), satellite, initial(), settings),
    invalid(
      "PROPAGATION",
      "satellite G02 propagation failed: Force model reads UT1 outside the table: instant precedes the UT1 table coverage",
      {
        satellite,
        source: { kind: "ut1OutsideCoverage", reason: "beforeCoverage" },
      },
    ),
  );
  assert.throws(
    () => orbit.fitSp3EcefOrbitParameters(product, satellite, future, undefined, defaults),
    invalid(
      "FRAME",
      "satellite G02 frame transform failed: frame transform reads UT1, but the instant precedes the UT1 table coverage",
      {
        satellite,
        source: { kind: "ut1OutsideCoverage", reason: "beforeCoverage" },
      },
    ),
  );
});
