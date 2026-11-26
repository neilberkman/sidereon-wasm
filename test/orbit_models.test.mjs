import { test } from "node:test";
import assert from "node:assert/strict";

import * as orbit from "../pkg-node/sidereon.js";
import { fixtureJson } from "./helpers.mjs";

const expected = fixtureJson("astrodynamics.json");
const vector = (...values) => Float64Array.of(...values);
const plain = (array) => Array.from(array);
const coordinates = (state) => [plain(state.positionKm), plain(state.velocityKmS)];
const typed = (name, detail) => (error) => {
  assert.equal(error.name, name);
  assert.equal(error.kind, detail.kind);
  assert.deepEqual(error.detail, detail);
  return true;
};

test("closed-form zonal propagation and Cartesian element conversions", () => {
  const golden = expected.analytic;
  using gravity = orbit.ZonalHarmonics.earth();
  using explicit = new orbit.ZonalHarmonics(...golden.gravity);
  using defaults = orbit.ZonalHarmonics.default();
  assert.deepEqual(
    [
      defaults.muKm3S2,
      defaults.reKm,
      defaults.c20,
      defaults.c30,
      defaults.c40,
      defaults.c50,
      defaults.c60,
    ],
    golden.gravity,
  );
  using zonal = new orbit.ZonalHarmonics(398600, 6375, -0.001, 0, 0, 0, 0);
  assert.ok(gravity.equals(explicit));
  assert.ok(!gravity.equals(zonal));
  assert.deepEqual(
    [
      gravity.muKm3S2,
      gravity.reKm,
      gravity.c20,
      gravity.c30,
      gravity.c40,
      gravity.c50,
      gravity.c60,
    ],
    golden.gravity,
  );
  using circular = new orbit.CircularElements(7000, 0.001, 0.002, 0.9, 0.2, 0.3);
  using circularCopy = new orbit.CircularElements(7000, 0.001, 0.002, 0.9, 0.2, 0.3);
  using circularOther = new orbit.CircularElements(7000, 0.001, 0.002, 0.9, 0.2, 0.4);
  assert.ok(circular.equals(circularCopy));
  assert.ok(!circular.equals(circularOther));
  assert.deepEqual(
    [
      circular.aKm,
      circular.ex,
      circular.ey,
      circular.iRad,
      circular.raanRad,
      circular.meanLatitudeArgumentRad,
    ],
    [7000, 0.001, 0.002, 0.9, 0.2, 0.3],
  );
  assert.equal(circular.eccentricity(), golden.eccentricity);
  using keplerian = new orbit.KeplerianElements(8000, 0.1, 0.9, 0.2, 0.3, 0.4);
  using keplerianCopy = new orbit.KeplerianElements(8000, 0.1, 0.9, 0.2, 0.3, 0.4);
  using keplerianOther = new orbit.KeplerianElements(8000, 0.1, 0.9, 0.2, 0.3, 0.5);
  assert.ok(keplerian.equals(keplerianCopy));
  assert.ok(!keplerian.equals(keplerianOther));
  assert.deepEqual(
    [
      keplerian.aKm,
      keplerian.e,
      keplerian.iRad,
      keplerian.argPerigeeRad,
      keplerian.raanRad,
      keplerian.meanAnomalyRad,
    ],
    [8000, 0.1, 0.9, 0.2, 0.3, 0.4],
  );
  using circularCartesian = circular.toCartesian(gravity.muKm3S2);
  using keplerianCartesian = keplerian.toCartesian(gravity.muKm3S2);
  assert.deepEqual(coordinates(circularCartesian), golden.circularCartesian);
  assert.deepEqual(coordinates(keplerianCartesian), golden.keplerianCartesian);
  using eh = orbit.EcksteinHechlerPropagator.fromMeanElements(0, circular, gravity);
  using bl = orbit.BrouwerLyddanePropagator.fromMeanElements(0, keplerian, gravity, 0);
  for (const [model, mean, expectedState] of [
    [eh, circular, golden.eh],
    [bl, keplerian, golden.bl],
  ]) {
    using state = model.propagate(60);
    const [sample] = model.ephemeris(vector(60));
    using sampled = sample;
    using meanState = model.meanElements();
    using modelGravity = model.gravity();
    assert.ok(modelGravity.equals(gravity));
    assert.ok(meanState.equals(mean));
    assert.equal(model.epochTdbSeconds(), 0);
    assert.equal(state.epochTdbSeconds, 60);
    assert.deepEqual(coordinates(state), expectedState);
    assert.deepEqual(coordinates(sampled), expectedState);
  }
  assert.equal(bl.m2RadS2(), 0);
  using ehDefaults = orbit.MeanConversionOptions.ECKSTEIN_HECHLER_DEFAULT();
  using blDefaults = orbit.MeanConversionOptions.BROUWER_LYDDANE_DEFAULT();
  using equivalent = new orbit.MeanConversionOptions(1e-11, 100);
  assert.deepEqual([ehDefaults.tolerance, ehDefaults.maxIterations], [1e-11, 100]);
  assert.deepEqual([blDefaults.tolerance, blDefaults.maxIterations], [1e-13, 200]);
  assert.ok(ehDefaults.equals(equivalent));
  assert.ok(!ehDefaults.equals(blDefaults));

  using ehOsc = eh.osculatingElements(0);
  using ehConverted = orbit.ecksteinHechlerOsculatingElements(circular, gravity);
  using blOsc = bl.osculatingElements(0);
  using blConverted = orbit.brouwerLyddaneOsculatingElements(keplerian, gravity);
  const circularFields = (elements) => [
    elements.aKm,
    elements.ex,
    elements.ey,
    elements.iRad,
    elements.raanRad,
    elements.meanLatitudeArgumentRad,
  ];
  const keplerianFields = (elements) => [
    elements.aKm,
    elements.e,
    elements.iRad,
    elements.argPerigeeRad,
    elements.raanRad,
    elements.meanAnomalyRad,
  ];
  assert.deepEqual(circularFields(ehOsc), golden.ehOsculating);
  assert.deepEqual(circularFields(ehConverted), golden.ehOsculating);
  assert.deepEqual(keplerianFields(blOsc), golden.blOsculating);
  assert.deepEqual(keplerianFields(blConverted), golden.blOsculating);

  using ehAt60 = new orbit.CartesianState(60, vector(...golden.eh[0]), vector(...golden.eh[1]));
  using blAt60 = new orbit.CartesianState(60, vector(...golden.bl[0]), vector(...golden.bl[1]));
  using ehMean = orbit.ecksteinHechlerMeanElements(ehAt60, gravity, ehDefaults);
  using blMean = orbit.brouwerLyddaneMeanElements(blAt60, gravity, blDefaults);
  using ehOther = orbit.ecksteinHechlerMeanElements(ehAt60, gravity, ehDefaults);
  using ehElements = ehMean.elements;
  using blElements = blMean.elements;
  assert.ok(ehMean.equals(ehOther));
  assert.ok(!ehMean.equals(blMean));
  assert.equal(ehMean.iterations, golden.ehMean.iterations);
  assert.equal(blMean.iterations, golden.blMean.iterations);
  assert.deepEqual(circularFields(ehElements), golden.ehMean.elements);
  assert.deepEqual(keplerianFields(blElements), golden.blMean.elements);
  using fromOsculatingEh = new orbit.EcksteinHechlerPropagator(ehAt60, gravity, ehDefaults);
  using fromOsculatingBl = new orbit.BrouwerLyddanePropagator(blAt60, gravity, 0, blDefaults);
  using ehInitial = fromOsculatingEh.meanElements();
  using blInitial = fromOsculatingBl.meanElements();
  assert.deepEqual(circularFields(ehInitial), golden.ehMean.elements);
  assert.deepEqual(keplerianFields(blInitial), golden.blMean.elements);

  using fromCartesianCircle = orbit.CircularElements.fromCartesian(
    vector(4, 0, 0),
    vector(0, 0, 1),
    4,
  );
  using fromCartesianKeplerian = orbit.KeplerianElements.fromCartesian(
    vector(4, 0, 0),
    vector(0, 0, 1),
    4,
  );
  assert.deepEqual(circularFields(fromCartesianCircle), [4, 0, 0, Math.PI / 2, 0, 0]);
  assert.deepEqual(keplerianFields(fromCartesianKeplerian), [4, 0, Math.PI / 2, 0, 0, 0]);
});

test("analytic propagation refusal details identify the state or theory domain", () => {
  using gravity = orbit.ZonalHarmonics.earth();
  assert.equal(
    orbit.analyticTheoryToString(orbit.AnalyticTheory.EcksteinHechler),
    "Eckstein-Hechler",
  );
  assert.equal(
    orbit.analyticTheoryToString(orbit.AnalyticTheory.BrouwerLyddane),
    "Brouwer-Lyddane",
  );
  const refuse = (parameters, detail) => {
    using mean = new orbit.CircularElements(...parameters);
    assert.throws(
      () => orbit.EcksteinHechlerPropagator.fromMeanElements(0, mean, gravity),
      (error) => {
        if (detail.kind === "nonFinite")
          assert.equal(error.toString(), "non-finite input mean elements");
        return typed("AnalyticPropagationError", detail)(error);
      },
    );
  };
  refuse([NaN, 0, 0, 0.9, 0, 0], { kind: "nonFinite", field: "mean elements" });
  refuse([6300, 0, 0, 0.9, 0, 0], {
    kind: "insideReferenceSphere",
    semiMajorAxisKm: 6300,
    referenceRadiusKm: expected.analytic.gravity[1],
  });
  refuse([7000, 0.2, 0, 0.9, 0, 0], {
    kind: "eccentricityTooLarge",
    eccentricity: 0.2,
    limit: 0.1,
  });
  refuse([7000, 0, 0, 0, 0, 0], { kind: "nearlyEquatorial", inclinationRad: 0 });
  const criticalInclination = Math.atan(2);
  refuse([7000, 0, 0, criticalInclination, 0, 0], {
    kind: "nearCriticalInclination",
    inclinationRad: criticalInclination,
  });
  using hyperbolic = new orbit.KeplerianElements(7000, 1, 0.9, 0, 0, 0);
  assert.throws(
    () => orbit.BrouwerLyddanePropagator.fromMeanElements(0, hyperbolic, gravity, 0),
    typed("AnalyticPropagationError", { kind: "notElliptic", field: "mean elements" }),
  );
  using retro = new orbit.CartesianState(0, vector(7000, 0, 0), vector(0, -7.546, 0));
  using options = orbit.MeanConversionOptions.BROUWER_LYDDANE_DEFAULT();
  assert.throws(
    () => orbit.brouwerLyddaneMeanElements(retro, gravity, options),
    typed("AnalyticPropagationError", { kind: "retrogradeEquatorial", inclinationRad: Math.PI }),
  );
  using osculating = new orbit.CartesianState(
    60,
    vector(...expected.analytic.eh[0]),
    vector(...expected.analytic.eh[1]),
  );
  using badTolerance = new orbit.MeanConversionOptions(0, 100);
  using oneIteration = new orbit.MeanConversionOptions(1e-20, 1);
  assert.throws(
    () => orbit.ecksteinHechlerMeanElements(osculating, gravity, badTolerance),
    typed("AnalyticPropagationError", {
      kind: "invalidParameter",
      field: "tolerance",
      reason: "must be positive",
    }),
  );
  assert.throws(
    () => orbit.ecksteinHechlerMeanElements(osculating, gravity, oneIteration),
    typed("AnalyticPropagationError", {
      kind: "meanElementsNotConverged",
      theory: orbit.AnalyticTheory.EcksteinHechler,
      iterations: 1,
    }),
  );
  using nearParabola = new orbit.KeplerianElements(8000, 0.99999, 0.5, 0, 0, 3 ** -9);
  assert.throws(
    () => nearParabola.toCartesian(gravity.muKm3S2),
    typed("AnalyticPropagationError", { kind: "keplerNotConverged", field: "eccentric anomaly" }),
  );
  using mean = new orbit.KeplerianElements(8000, 0.1, 0.9, 0.2, 0.3, 0.4);
  using model = orbit.BrouwerLyddanePropagator.fromMeanElements(0, mean, gravity, 1);
  assert.throws(
    () => model.propagate(Number.MAX_VALUE),
    typed("AnalyticPropagationError", {
      kind: "nonFiniteResult",
      theory: orbit.AnalyticTheory.BrouwerLyddane,
      field: "a_km",
    }),
  );
});

test("ECOM coefficient layouts, geometry and radiation forces", () => {
  const r = vector(26500, 100, 200);
  const v = vector(0, 3, 2);
  const sun = vector(1e8, 2e7, 1e7);
  const moon = vector(-380000, 0, 0);
  using coeff1 = orbit.Ecom1Coefficients.fromArray(new Float64Array(9).fill(1e-7));
  using coeff2 = orbit.Ecom2D2b1Coefficients.fromArray(new Float64Array(7).fill(1e-7));
  using coeff4 = orbit.Ecom2D4b1Coefficients.fromArray(vector(1, 2, 3, 4, 5, 6, 7, 8, 9));
  assert.deepEqual(plain(coeff1.toArray()), Array(9).fill(1e-7));
  assert.deepEqual(plain(coeff2.toArray()), Array(7).fill(1e-7));
  assert.deepEqual(plain(coeff4.toArray()), [1, 2, 3, 4, 5, 6, 7, 8, 9]);
  assert.equal(orbit.ECOM1_COEFFICIENT_COUNT(), 9);
  assert.equal(orbit.ECOM2_D2B1_COEFFICIENT_COUNT(), 7);
  assert.equal(orbit.ECOM2_D4B1_COEFFICIENT_COUNT(), 9);
  assert.deepEqual(orbit.Ecom1Coefficients.NAMES(), [
    "D0",
    "Dc",
    "Ds",
    "Y0",
    "Yc",
    "Ys",
    "B0",
    "Bc",
    "Bs",
  ]);
  assert.deepEqual(orbit.Ecom2D2b1Coefficients.NAMES(), [
    "B0",
    "B1c",
    "B1s",
    "D0",
    "D2c",
    "D2s",
    "Y0",
  ]);
  assert.deepEqual(orbit.Ecom2D4b1Coefficients.NAMES(), [
    "B0",
    "B1c",
    "B1s",
    "D0",
    "D2c",
    "D4c",
    "D2s",
    "D4s",
    "Y0",
  ]);
  assert.deepEqual(orbit.Ecom1Coefficients.PARAMETER_NAMES(), [
    "ecom1.d0",
    "ecom1.dc",
    "ecom1.ds",
    "ecom1.y0",
    "ecom1.yc",
    "ecom1.ys",
    "ecom1.b0",
    "ecom1.bc",
    "ecom1.bs",
  ]);
  assert.deepEqual(orbit.Ecom2D2b1Coefficients.PARAMETER_NAMES(), [
    "ecom2.b0",
    "ecom2.b1c",
    "ecom2.b1s",
    "ecom2.d0",
    "ecom2.d2c",
    "ecom2.d2s",
    "ecom2.y0",
  ]);
  assert.deepEqual(orbit.Ecom2D4b1Coefficients.PARAMETER_NAMES(), [
    "ecom2.b0",
    "ecom2.b1c",
    "ecom2.b1s",
    "ecom2.d0",
    "ecom2.d2c",
    "ecom2.d4c",
    "ecom2.d2s",
    "ecom2.d4s",
    "ecom2.y0",
  ]);
  assert.deepEqual(
    [
      coeff1.d0,
      coeff1.dc,
      coeff1.ds,
      coeff1.y0,
      coeff1.yc,
      coeff1.ys,
      coeff1.b0,
      coeff1.bc,
      coeff1.bs,
    ],
    Array(9).fill(1e-7),
  );
  assert.deepEqual(
    [
      coeff4.b0,
      coeff4.b1c,
      coeff4.b1s,
      coeff4.d0,
      coeff4.d2c,
      coeff4.d4c,
      coeff4.d2s,
      coeff4.d4s,
      coeff4.y0,
    ],
    [1, 2, 3, 4, 5, 6, 7, 8, 9],
  );

  using ecom1 = new orbit.Ecom1SolarRadiationPressure(coeff1);
  using ecom2 = orbit.Ecom2SolarRadiationPressure.d2b1(coeff2);
  using ecom4 = orbit.Ecom2SolarRadiationPressure.d4b1(coeff4);
  using configured = ecom1.coefficients();
  assert.ok(configured.equals(coeff1));
  assert.equal(ecom2.nD(), 1);
  assert.equal(ecom2.nB(), 1);
  assert.equal(ecom4.nD(), 2);
  assert.equal(ecom4.nB(), 1);
  assert.equal(orbit.Ecom2SolarRadiationPressure.coefficientCount(2, 1), 9);
  assert.deepEqual(orbit.Ecom2SolarRadiationPressure.parameterNames(1, 1), [
    "ecom2.b0",
    "ecom2.b1c",
    "ecom2.b1s",
    "ecom2.d0",
    "ecom2.d2c",
    "ecom2.d2s",
    "ecom2.y0",
  ]);
  for (const [model, series] of [
    [ecom1, "ecom1"],
    [ecom2, "ecom2"],
  ]) {
    using earthOnly = model.withMoonEclipse(false);
    using solarPartials = earthOnly.accelerationPartialsWithSun(r, v, sun);
    using lunarPartials = model.accelerationPartialsWithSunMoon(r, v, sun, moon);
    assert.deepEqual(
      plain(earthOnly.accelerationWithSun(r, v, sun)),
      expected.radiation[series + "Sun"],
    );
    assert.deepEqual(
      plain(model.accelerationWithSunMoon(r, v, sun, moon)),
      expected.radiation[series],
    );
    assert.deepEqual(
      [
        plain(solarPartials.wrtPosition),
        plain(solarPartials.wrtVelocity),
        plain(earthOnly.parameterPartialsWithSun(r, v, sun)),
      ],
      expected.radiation[series + "PartialsSun"],
    );
    assert.deepEqual(
      [
        plain(lunarPartials.wrtPosition),
        plain(lunarPartials.wrtVelocity),
        plain(model.parameterPartialsWithSunMoon(r, v, sun, moon)),
      ],
      expected.radiation[series + "PartialsSunMoon"],
    );
    for (const [method, required] of [
      ["accelerationWithSun", "acceleration_with_sun_moon"],
      ["accelerationPartialsWithSun", "acceleration_partials_with_sun_moon"],
      ["parameterPartialsWithSun", "parameter_partials_with_sun_moon"],
    ]) {
      assert.throws(
        () => model[method](r, v, sun),
        (error) => {
          assert.equal(error.name, "Error");
          assert.deepEqual(error.detail, {
            family: "propagation",
            cause: {
              kind: "invalidInput",
              message: `${series.toUpperCase()} lunar eclipse is enabled; call ${required}`,
            },
          });
          return true;
        },
      );
    }
  }
  using geometry = orbit.ecomGeometryWithShadow(
    r,
    v,
    sun,
    moon,
    orbit.EarthShadowModel.Wgs84Oblate,
  );
  using solarGeometry = orbit.ecomGeometry(r, v, sun);
  using sameGeometry = orbit.ecomGeometry(r, v, sun);
  using differentGeometry = orbit.ecomGeometry(vector(26500, 101, 200), v, sun);
  assert.ok(solarGeometry.equals(sameGeometry));
  assert.ok(!solarGeometry.equals(differentGeometry));
  for (const [sample, golden] of [
    [geometry, expected.radiation.geometry],
    [solarGeometry, expected.radiation.geometrySun],
  ]) {
    using axes = sample.dyb;
    assert.deepEqual([plain(axes.d), plain(axes.y), plain(axes.b)], golden.dyb);
    assert.equal(sample.argumentOfLatitudeRad, golden.u);
    assert.equal(sample.deltaURad, golden.delta);
    assert.equal(sample.lightingRatio, golden.light);
  }

  using context = new orbit.PropagationContext();
  using state = new orbit.CartesianState(1000, r, v);
  using radiation = new orbit.SolarRadiationPressure(1.2, 0.02);
  using customPressure = orbit.SolarRadiationPressure.withPressure(1.2, 0.02, 4e-6, 150e6);
  using earth = new orbit.EarthRadiationPressure(1.2, 0.02);
  assert.equal(orbit.SolarRadiationPressure.CR_PARAMETER(), "srp.cr");
  assert.equal(radiation.cr, 1.2);
  assert.equal(radiation.areaToMassM2Kg, 0.02);
  assert.equal(radiation.pressureNM2, 4.56e-6);
  assert.equal(radiation.auKm, 149597870.7);
  assert.equal(customPressure.pressureNM2, 4e-6);
  assert.equal(customPressure.auKm, 150e6);
  assert.equal(radiation.shadowFunction(), orbit.ShadowFunction.ApparentDisc);
  assert.equal(radiation.earthShadowModel(), orbit.EarthShadowModel.Wgs84Oblate);
  assert.equal(radiation.moonEclipseEnabled(), true);
  using conical = radiation.withShadowFunction(orbit.ShadowFunction.Conical);
  using spherical = conical.withEarthShadowModel(orbit.EarthShadowModel.Spherical);
  using earthOnly = spherical.withMoonEclipse(false);
  assert.equal(earthOnly.shadowFunction(), orbit.ShadowFunction.Conical);
  assert.equal(earthOnly.earthShadowModel(), orbit.EarthShadowModel.Spherical);
  assert.equal(earthOnly.moonEclipseEnabled(), false);
  assert.deepEqual(plain(radiation.acceleration(state, context)), expected.radiation.srp);
  assert.deepEqual(
    plain(earth.accelerationWithSunMoon(state, sun, moon)),
    expected.radiation.earth,
  );
});

test("empirical RTN parameters, composite forces and variational propagation", () => {
  using empirical = orbit.EmpiricalRtnAcceleration.constantTerms().withTerm(
    orbit.RtnAxis.Radial,
    orbit.EmpiricalHarmonic.Constant,
    1e-7,
  );
  using zero = orbit.EmpiricalRtnAcceleration.constantTerms();
  using all = orbit.EmpiricalRtnAcceleration.allTerms();
  using empty = new orbit.EmpiricalRtnAcceleration();
  using selected = orbit.EmpiricalRtnAcceleration.withZeroTerms([
    [orbit.RtnAxis.Radial, orbit.EmpiricalHarmonic.Constant],
  ]);
  assert.equal(empty.parameterCount(), 0);
  assert.equal(all.parameterCount(), 9);
  assert.equal(selected.parameterCount(), 1);
  assert.equal(empirical.parameterCount(), 3);
  assert.equal(orbit.EmpiricalRtnAcceleration.parameterName(0, 0), "empirical.radial.constant");
  using copied = orbit.EmpiricalRtnAcceleration.constantTerms().withTerm(
    orbit.RtnAxis.Radial,
    orbit.EmpiricalHarmonic.Constant,
    1e-7,
  );
  assert.ok(empirical.equals(copied));
  assert.equal(
    empirical.coefficientKmS2(orbit.RtnAxis.Radial, orbit.EmpiricalHarmonic.Constant),
    1e-7,
  );
  assert.deepEqual(empirical.terms(), [
    [orbit.RtnAxis.Radial, orbit.EmpiricalHarmonic.Constant, 1e-7],
    [orbit.RtnAxis.AlongTrack, orbit.EmpiricalHarmonic.Constant, 0],
    [orbit.RtnAxis.CrossTrack, orbit.EmpiricalHarmonic.Constant, 0],
  ]);
  assert.ok(!empirical.equals(zero));
  assert.deepEqual(orbit.rtnAxisAll(), [0, 1, 2]);
  assert.equal(orbit.rtnAxisCompare(orbit.RtnAxis.Radial, orbit.RtnAxis.AlongTrack), -1);
  assert.equal(
    orbit.empiricalHarmonicCompare(orbit.EmpiricalHarmonic.Sine, orbit.EmpiricalHarmonic.Cosine),
    1,
  );
  assert.deepEqual(orbit.empiricalHarmonicAll(), [0, 1, 2]);

  using initial = new orbit.CartesianState(0, vector(7000, 0, 0), vector(0, 7.5, 0));
  using same = new orbit.CartesianState(0, vector(7000, 0, 0), vector(0, 7.5, 0));
  using different = new orbit.CartesianState(1, vector(7000, 0, 0), vector(0, 7.5, 0));
  assert.ok(initial.equals(same));
  assert.ok(!initial.equals(different));
  using context = new orbit.PropagationContext();
  using force = empirical.asForceModel();
  using combined = new orbit.CompositeForceModel([force.clone()]);
  using independent = new orbit.ForceParameter("empirical.radial.constant", 1e-7);
  const parameters = force.parameters();
  try {
    assert.equal(parameters[0].name, orbit.EmpiricalRtnAcceleration.parameterName(0, 0));
    assert.equal(parameters[0].value, 1e-7);
    assert.ok(parameters[0].equals(independent));
    assert.ok(!parameters[0].equals(parameters[1]));
  } finally {
    for (const parameter of parameters) parameter.free();
  }
  assert.deepEqual(plain(combined.acceleration(initial, context)), [1e-7, 0, 0]);
  assert.deepEqual(
    plain(force.accelerationWithParameters(vector(2e-7, 0, 0), initial, context)),
    [2e-7, 0, 0],
  );
  assert.deepEqual(
    plain(combined.parameterPartials(vector(1e-7, 0, 0), initial, context)),
    [1, 0, 0, 0, 1, 0, 0, 0, 1],
  );
  using partials = force.accelerationPartials(vector(0, 0, 0), initial, context);
  using zeroPartials = orbit.AccelerationPartials.zeros();
  using otherPartials = orbit.AccelerationPartials.positionOnly(vector(1, 0, 0, 0, 0, 0, 0, 0, 0));
  assert.deepEqual(plain(partials.wrtPosition), Array(9).fill(0));
  assert.deepEqual(plain(partials.wrtVelocity), Array(9).fill(0));
  assert.ok(partials.equals(zeroPartials));
  assert.ok(!partials.equals(otherPartials));
  assert.deepEqual(plain(otherPartials.wrtPosition), [1, 0, 0, 0, 0, 0, 0, 0, 0]);
  assert.deepEqual(plain(otherPartials.wrtVelocity), Array(9).fill(0));

  const epoch = orbit.VariationalEpoch.fromTdbSeconds(0, 60);
  using equivalent = new orbit.VariationalEpoch(60, expected.variational.elapsedTt);
  using differentEpoch = new orbit.VariationalEpoch(60, 60);
  assert.equal(epoch.tdbJ2000S, 60);
  assert.equal(epoch.elapsedTtS, expected.variational.elapsedTt);
  assert.ok(epoch.equals(equivalent));
  assert.ok(!epoch.equals(differentEpoch));
  using equations = new orbit.VariationalEquations(force, vector(1e-7, 0, 0), vector(0));
  assert.deepEqual(plain(equations.parameters), [1e-7, 0, 0]);
  assert.deepEqual(plain(equations.sensitivityParameters), [0]);
  using forceFromEquations = equations.force;
  assert.equal(forceFromEquations.parameterCount(), 3);
  const [sample] = orbit.propagateVariational(
    initial,
    [epoch],
    equations,
    "dp54",
    undefined,
    context,
  );
  using result = sample;
  const [atStart, atEnd] = orbit.propagateVariational(
    initial,
    [orbit.VariationalEpoch.fromTdbSeconds(0, 0), orbit.VariationalEpoch.fromTdbSeconds(0, 60)],
    equations,
    "dp54",
    undefined,
    context,
  );
  using start = atStart;
  using end = atEnd;
  assert.ok(result.equals(end));
  assert.ok(!result.equals(start));
  using state = result.state;
  assert.deepEqual(coordinates(state), [
    expected.variational.position,
    expected.variational.velocity,
  ]);
  assert.deepEqual(plain(result.stateTransition), expected.variational.stm);
  assert.deepEqual(plain(result.sensitivity), expected.variational.sensitivity);
  using ephemeris = orbit.propagateState({
    epochS: 0,
    positionKm: [7000, 0, 0],
    velocityKmS: [0, 7.5, 0],
    timesS: [60],
    forceModel: { kind: "composite", twoBody: false },
    additionalForces: [force],
    context,
  });
  assert.deepEqual(plain(ephemeris.states), expected.additionalForceEphemeris.flat());
});

test("radiation model options and ECOM coefficient defaults preserve their values", () => {
  using first = new orbit.Ecom1Coefficients();
  using low = new orbit.Ecom2D2b1Coefficients();
  using high = new orbit.Ecom2D4b1Coefficients();
  for (const [coefficients, same, different, values] of [
    [
      first,
      orbit.Ecom1Coefficients.fromArray(vector(0, 0, 0, 0, 0, 0, 0, 0, 0)),
      new orbit.Ecom1Coefficients(1),
      Array(9).fill(0),
    ],
    [
      low,
      orbit.Ecom2D2b1Coefficients.fromArray(vector(0, 0, 0, 0, 0, 0, 0)),
      new orbit.Ecom2D2b1Coefficients(1),
      Array(7).fill(0),
    ],
    [
      high,
      orbit.Ecom2D4b1Coefficients.fromArray(vector(0, 0, 0, 0, 0, 0, 0, 0, 0)),
      new orbit.Ecom2D4b1Coefficients(1),
      Array(9).fill(0),
    ],
  ]) {
    using matching = same;
    using changed = different;
    assert.deepEqual(plain(coefficients.toArray()), values);
    assert.ok(coefficients.equals(matching));
    assert.ok(!coefficients.equals(changed));
  }
  using coefficients = new orbit.Ecom2D2b1Coefficients(1, 2, 3, 4, 5, 6, 7);
  assert.deepEqual(
    [
      coefficients.b0,
      coefficients.b1c,
      coefficients.b1s,
      coefficients.d0,
      coefficients.d2c,
      coefficients.d2s,
      coefficients.y0,
    ],
    [1, 2, 3, 4, 5, 6, 7],
  );
  using ecom1 = new orbit.Ecom1SolarRadiationPressure(first);
  using sameEcom1 = new orbit.Ecom1SolarRadiationPressure(first);
  using ecom2 = new orbit.Ecom2SolarRadiationPressure(1, 1, [1, 2, 3, 4, 5, 6, 7]);
  using sameEcom2 = orbit.Ecom2SolarRadiationPressure.d2b1(coefficients);
  assert.deepEqual(plain(ecom2.coefficients()), [1, 2, 3, 4, 5, 6, 7]);
  assert.ok(ecom1.equals(sameEcom1));
  assert.ok(ecom2.equals(sameEcom2));
  for (const model of [ecom1, ecom2]) {
    assert.equal(model.earthShadowModel(), orbit.EarthShadowModel.Wgs84Oblate);
    assert.equal(model.shadowFunction(), orbit.ShadowFunction.ApparentDisc);
    assert.equal(model.moonEclipseEnabled(), true);
    using conical = model.withShadowFunction(orbit.ShadowFunction.Conical);
    using spherical = conical.withEarthShadowModel(orbit.EarthShadowModel.Spherical);
    using earthOnly = spherical.withMoonEclipse(false);
    assert.equal(earthOnly.earthShadowModel(), orbit.EarthShadowModel.Spherical);
    assert.equal(earthOnly.shadowFunction(), orbit.ShadowFunction.Conical);
    assert.equal(earthOnly.moonEclipseEnabled(), false);
    assert.ok(!earthOnly.equals(model));
  }

  using earth = new orbit.EarthRadiationPressure(1.2, 0.02);
  using withMoon = earth.withMoonEclipse(true);
  assert.equal(earth.moonEclipseEnabled(), false);
  assert.equal(withMoon.moonEclipseEnabled(), true);
  const request = {
    epochS: 0,
    positionKm: [7000, 0, 0],
    velocityKmS: [0, 7.5, 0],
    timesS: [600],
    forceModel: {
      twoBody: false,
      earthRadiationPressure: { cr: 1.2, areaToMassM2Kg: 0.02, moonEclipse: false },
    },
  };
  using ephemeris = orbit.propagateState(request);
  assert.deepEqual(plain(ephemeris.states), expected.earthRadiationEphemeris.flat());
});

test("force evaluation refuses non-finite inputs it reads and recovers finite J2 acceleration", () => {
  using context = new orbit.PropagationContext();
  using j2 = orbit.ForceModel.j2();
  using unavailableEpoch = new orbit.CartesianState(
    NaN,
    Float64Array.of(7000, 100, 200),
    Float64Array.of(NaN, Infinity, -Infinity),
  );
  assert.deepEqual(
    plain(j2.acceleration(unavailableEpoch, context)),
    expected.gravityAccelerations.j2UnusedInputs,
  );
  using distant = new orbit.CartesianState(
    0,
    Float64Array.of(1e60, 2e60, 3e60),
    Float64Array.of(0, 0, 0),
  );
  assert.deepEqual(
    plain(j2.acceleration(distant, context)),
    expected.gravityAccelerations.j2Distant,
  );
  using invalid = new orbit.CartesianState(
    0,
    Float64Array.of(NaN, 100, 200),
    Float64Array.of(0, 0, 0),
  );
  assert.throws(
    () => j2.acceleration(invalid, context),
    (error) => {
      assert.equal(error.message, "Invalid input: position must be finite");
      assert.deepEqual(error.detail, {
        family: "propagation",
        cause: { kind: "invalidInput", message: "position must be finite" },
      });
      return true;
    },
  );
  using srp = new orbit.SolarRadiationPressure(1.2, 0.02);
  assert.throws(
    () => srp.acceleration(unavailableEpoch, context),
    (error) => {
      assert.equal(error.message, "Invalid input: epoch must be finite");
      assert.deepEqual(error.detail, {
        family: "propagation",
        cause: { kind: "invalidInput", message: "epoch must be finite" },
      });
      return true;
    },
  );
});
