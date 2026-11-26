import { test } from "node:test";
import assert from "node:assert/strict";

import * as orbit from "../pkg-node/sidereon.js";
import { f64Bits, fixture, fixtureJson } from "./helpers.mjs";

const golden = fixtureJson("astrodynamics.json");
const vector = (...values) => Float64Array.of(...values);
const values = (array) => Array.from(array);
const typed = (name, detail) => (error) => {
  assert.equal(error.name, name);
  assert.equal(error.kind, detail.kind);
  assert.equal(error.toString(), error.message);
  assert.deepEqual(error.detail, detail);
  return true;
};

test("GCRS solar and lunar positions, frame rotations and TT–TDB conversions", () => {
  using instant = orbit.Instant.fromUnixMicros(1593043200000000n);
  using century = orbit.sunMoonGcrs(0.25);
  using seconds = orbit.sunMoonGcrsAtTdbSeconds(1000);
  using at = orbit.sunMoonGcrsAt(instant);
  for (const [result, expected] of [
    [century, golden.solar.byCentury],
    [seconds, golden.solar.bySeconds],
    [at, golden.solar.at],
  ]) {
    assert.equal(result.frame, "gcrs");
    assert.equal(result.epochCount, 1);
    assert.deepEqual([values(result.sun), values(result.moon)], expected);
  }
  assert.deepEqual(values(orbit.gcrsToMeanOfDateMatrix(instant)), golden.solar.toMeanOfDate);
  assert.deepEqual(values(orbit.meanOfDateToGcrsMatrix(instant)), golden.solar.toGcrs);
  assert.equal(orbit.tdbFromTtJ2000Seconds(1000), golden.solar.tdb);
  assert.equal(orbit.ttFromTdbJ2000Seconds(1000), golden.solar.tt);
  assert.equal(orbit.tdbMinusTtSeconds(1000), golden.solar.tdbMinusTt);
});

test("DE440 inputs, constants, geocentric states and typed errors", () => {
  const bytes = fixture("bodies/de440.440");
  using de = orbit.DeEphemeris.fromBytes(bytes);
  const expected = golden.de;
  const [epoch] = expected.coverage;
  assert.equal(de.version(), 440);
  assert.equal(de.recordSize(), bytes.length / 3);
  assert.equal(de.constants().size, expected.constantCount);
  assert.equal(de.constants().get("AU"), 149597870.7);
  assert.equal(de.astronomicalUnitM(), expected.au);
  assert.equal(de.earthMoonMassRatio(), expected.earthMoonMassRatio);
  assert.equal(de.gmM3S2(orbit.DeBody.Sun), expected.sunGm);
  assert.deepEqual(values(de.coverageTdbSeconds()), expected.coverage);
  using result = de.sunMoonAtTdbSeconds(epoch);
  using another = de.sunMoonAtTdbSeconds(epoch);
  using later = de.sunMoonAtTdbSeconds(epoch + 1);
  using sun = result.sun;
  using moon = result.moon;
  using otherSun = another.sun;
  assert.deepEqual([values(sun.positionM), values(sun.velocityMS)], expected.sun);
  assert.deepEqual([values(moon.positionM), values(moon.velocityMS)], expected.moon);
  assert.ok(result.equals(another));
  assert.ok(!result.equals(later));
  assert.ok(sun.equals(otherSun));
  assert.ok(!sun.equals(moon));
  using positions = de.sunMoonPositionsAtTdbSeconds(epoch);
  assert.deepEqual(
    [values(positions.sun), values(positions.moon)],
    [expected.sun[0], expected.moon[0]],
  );

  using empty = new orbit.PropagationContext();
  assert.equal(empty.deEphemeris(), undefined);
  assert.equal(empty.ut1Departure(), undefined);
  using configured = empty.withDeEphemeris(de);
  using stored = configured.deEphemeris();
  assert.equal(stored.version(), 440);

  assert.throws(
    () => orbit.DeEphemeris.fromBytes(new Uint8Array()),
    (error) => {
      assert.equal(error.toString(), "Invalid DE ephemeris: incomplete first header record");
      return typed("DeError", { kind: "invalidFile", reason: "incomplete first header record" })(
        error,
      );
    },
  );
  assert.throws(() => de.sunMoonAtTdbSeconds(NaN), typed("DeError", { kind: "nonFiniteEpoch" }));
  assert.throws(
    () => de.sunMoonPositionsAtTdbSeconds(epoch - 10),
    typed("DeError", { kind: "outsideCoverage", epochTdbSeconds: epoch - 10 }),
  );
  assert.throws(() => de.gmM3S2(99), {
    name: "RangeError",
    message: "body must be a DeBody value",
  });

  const damaged = Uint8Array.from(bytes);
  const dataview = new DataView(damaged.buffer);
  const sunOffset = dataview.getUint32(2696 + 12 * 10, true) - 1;
  dataview.setFloat64(2 * de.recordSize() + 8 * sunOffset, Number.MAX_VALUE, true);
  using overflow = orbit.DeEphemeris.fromBytes(damaged);
  assert.throws(
    () => overflow.sunMoonAtTdbSeconds(epoch),
    typed("DeError", { kind: "nonFiniteState" }),
  );
});

test("daily Earth orientation samples and IERS rotation with rates", () => {
  const sample = (mjd) =>
    new orbit.Iers2010EopSample(
      mjd,
      0.1,
      0.2,
      -0.25,
      0.001,
      0.0001,
      -0.0002,
      vector(1e-15, -2e-15),
    );
  using first = sample(59025);
  using same = sample(59025);
  using other = sample(59026);
  assert.deepEqual(
    [
      first.mjdUtc,
      first.xpArcsec,
      first.ypArcsec,
      first.ut1MinusUtcS,
      first.lodS,
      first.dxArcsec,
      first.dyArcsec,
    ],
    [59025, 0.1, 0.2, -0.25, 0.001, 0.0001, -0.0002],
  );
  assert.deepEqual(values(first.poleRatesRadS), [1e-15, -2e-15]);
  assert.ok(first.equals(same));
  assert.ok(!first.equals(other));

  const samples = () => [59025, 59026, 59027].map(sample);
  using provider = new orbit.Iers2010EarthOrientationProvider(samples());
  using duplicate = new orbit.Iers2010EarthOrientationProvider(samples());
  using permissive = provider.withValidity("permissive");
  assert.ok(provider.equals(duplicate));
  assert.ok(!provider.equals(permissive));
  using result = provider.orientationAtUtcMjd(59026, 0);
  using again = provider.orientationAtUtcMjd(59026, 0);
  using nearby = provider.orientationAtUtcMjd(59026, 1);
  assert.ok(result.equals(again));
  assert.ok(!result.equals(nearby));
  assert.equal(result.ut1Degraded(), undefined);
  assert.deepEqual(values(result.iers2010Quaternion()), golden.eop.quaternion);
  assert.deepEqual(values(result.earthRotationVectorItrfRadS()), golden.eop.rotation);
  assert.deepEqual(values(result.gcrfToItrfMatrix()), golden.eop.gcrfToItrf);
  assert.deepEqual(values(result.itrfToGcrfMatrix()), golden.eop.itrfToGcrf);
  assert.deepEqual(values(result.gcrfToItrfRotationRateMatrix()), golden.eop.gcrfToItrfRate);
  assert.deepEqual(values(result.itrfToGcrfRotationRateMatrix()), golden.eop.itrfToGcrfRate);
  using tdb = provider.orientationAtTdbSeconds(golden.eop.epochTdb);
  assert.deepEqual(values(tdb.gcrfToItrfMatrix()), golden.eop.gcrfToItrfAtTdb);
  using context = new orbit.PropagationContext();
  using supplied = context.withBodyFixedFrameProvider(provider);
  assert.equal(supplied.ut1Departure(), undefined);
  const frameFailure = (cause) => ({
    name: "Error",
    detail: { family: "frameTransform", cause },
  });
  assert.throws(
    () => provider.orientationAtUtcMjd(59025, NaN),
    frameFailure({ kind: "invalidInput", field: "seconds_in_day", reason: "outside the UTC day" }),
  );
  assert.throws(
    () => provider.orientationAtTdbSeconds(0),
    frameFailure({ kind: "ut1OutsideCoverage", reason: "beforeCoverage" }),
  );
});

test("nominal GNSS yaw, solar axes and block-specific yaw laws", () => {
  const pos = vector(26500000, 0, 0);
  const vel = vector(0, 3000, 2000);
  const sun = vector(1e11, 2e10, 1e10);
  using sample = new orbit.AttitudeSample(pos, vel, sun);
  using duplicate = new orbit.AttitudeSample(pos, vel, sun);
  using another = new orbit.AttitudeSample(pos, vel, vector(1e11, 2e10, 2e10));
  assert.ok(sample.equals(duplicate));
  assert.ok(!sample.equals(another));
  for (const [actual, expected] of [
    [sample.positionM, pos],
    [sample.velocityMS, vel],
    [sample.sunPositionM, sun],
  ]) {
    assert.deepEqual(actual, expected);
  }
  using yaw = orbit.nominalYaw(pos, vel, sun);
  using sameYaw = orbit.nominalYaw(pos, vel, sun);
  using otherYaw = orbit.nominalYaw(pos, vel, vector(1e11, 2e10, 2e10));
  assert.equal(yaw.yawRad, golden.attitude.yaw);
  assert.equal(yaw.betaRad, golden.attitude.beta);
  assert.deepEqual([values(yaw.bodyX), values(yaw.bodyY), values(yaw.bodyZ)], golden.attitude.body);
  assert.ok(yaw.equals(sameYaw));
  assert.ok(!yaw.equals(otherYaw));
  using axes = orbit.dybAxes(pos, sun);
  using sameAxes = orbit.dybAxes(pos, sun);
  using otherAxes = orbit.dybAxes(pos, vector(1e11, 2e10, 2e10));
  assert.deepEqual([values(axes.d), values(axes.y), values(axes.b)], golden.attitude.dyb);
  assert.ok(axes.equals(sameAxes));
  assert.ok(!axes.equals(otherAxes));

  const radians = (degrees) => (degrees * Math.PI) / 180;
  const constants = {
    GPS_IIA_DEFAULT_YAW_BIAS_RAD: 0.5,
    GPS_IIR_DEFAULT_YAW_RATE_RAD_S: 0.2,
    GPS_IIF_DEFAULT_YAW_RATE_RAD_S: 0.11,
    GPS_IIF_DEFAULT_YAW_BIAS_RAD: -0.7,
    GPS_SHADOW_LIMIT_RAD: 166.75,
    GLONASS_M_DEFAULT_YAW_RATE_RAD_S: 0.25,
    GLONASS_M_SHADOW_LIMIT_RAD: 165.8,
    GALILEO_IOV_BETA_X_RAD: 15,
    GALILEO_IOV_BETA_Y_RAD: 2,
    GPS_III_BETA_X_RAD: 15,
    GPS_III_BETA_Y_RAD: 5.8,
    BEIDOU3_SECM_BETA0_RAD: 3,
    BEIDOU2_YS_TO_ON_BETA_RAD: 4.1,
    BEIDOU2_ON_TO_YS_BETA_RAD: 3.9,
  };
  for (const [name, degrees] of Object.entries(constants)) {
    assert.equal(orbit[name](), radians(degrees), name);
  }
  for (const [constant, beta, inclusive, angle, threshold, period] of [
    [orbit.GALILEO_FOC_TURN, 4.1, false, orbit.CosineTurnStartAngle.Colinearity, 10, 5656],
    [orbit.BEIDOU3_CAST_MEO_TURN, 3, true, orbit.CosineTurnStartAngle.OrbitAngle, 6, 3090],
    [orbit.BEIDOU3_CAST_IGSO_TURN, 3, true, orbit.CosineTurnStartAngle.OrbitAngle, 6, 5740],
  ]) {
    using turn = constant();
    using same = new orbit.CosineTurnParameters(
      radians(beta),
      inclusive,
      angle,
      radians(threshold),
      period,
    );
    using other = new orbit.CosineTurnParameters(
      radians(beta),
      inclusive,
      angle,
      radians(threshold),
      period + 1,
    );
    assert.deepEqual(
      [
        turn.betaLimitRad,
        turn.betaLimitInclusive,
        turn.startAngle,
        turn.startAngleRad,
        turn.periodS,
      ],
      [radians(beta), inclusive, angle, radians(threshold), period],
    );
    assert.ok(turn.equals(same));
    assert.ok(!turn.equals(other));
  }

  using iia = orbit.GnssBlock.GPS_IIA();
  using iir = orbit.GnssBlock.GPS_IIR();
  using galileo = orbit.GnssBlock.fromIgsName("GALILEO-2");
  using foc = orbit.GnssBlock.GALILEO_FOC();
  using byConstellation = orbit.GnssBlock.fromConstellationBlockType(orbit.GnssSystem.Gps, "IIA");
  assert.equal(iia.toString(), "BLOCK IIA");
  assert.equal(iia.igsName(), "BLOCK IIA");
  assert.equal(iia.system(), orbit.GnssSystem.Gps);
  assert.ok(iia.equals(byConstellation));
  assert.ok(!iia.equals(iir));
  assert.equal(iia.compare(iir), -1);
  assert.ok(galileo.equals(foc));

  using law = orbit.GnssAttitudeLaw.forBlock(iir);
  using expectedLaw = orbit.GnssAttitudeLaw.gpsIir(radians(0.2));
  using otherLaw = orbit.GnssAttitudeLaw.gpsIir(radians(0.1));
  assert.ok(law.equals(expectedLaw));
  assert.ok(!law.equals(otherLaw));
  assert.equal(orbit.gpsIiaDefaultYawRateRadS(0), undefined);
  assert.equal(orbit.gpsIiaDefaultYawRateRadS(1), radians(0.1211));

  using focTurn = orbit.GALILEO_FOC_TURN();
  using meoTurn = orbit.BEIDOU3_CAST_MEO_TURN();
  using igsoTurn = orbit.BEIDOU3_CAST_IGSO_TURN();
  for (const [makeBlock, name, makeLaw, prn] of [
    [
      orbit.GnssBlock.GPS_IIA,
      "BLOCK IIA",
      () => orbit.GnssAttitudeLaw.gpsIia(radians(0.1211), radians(0.5), radians(166.75)),
      1,
    ],
    [orbit.GnssBlock.GPS_IIRM, "BLOCK IIR-M", () => orbit.GnssAttitudeLaw.gpsIir(radians(0.2))],
    [
      orbit.GnssBlock.GPS_IIF,
      "BLOCK IIF",
      () => orbit.GnssAttitudeLaw.gpsIif(radians(0.11), radians(-0.7), radians(166.75)),
    ],
    [
      orbit.GnssBlock.GPS_III,
      "BLOCK IIIA",
      () => orbit.GnssAttitudeLaw.galileoIov(radians(15), radians(5.8)),
    ],
    [
      orbit.GnssBlock.GLONASS_M,
      "GLONASS-M",
      () => orbit.GnssAttitudeLaw.glonassM(radians(0.25), radians(165.8)),
    ],
    [
      orbit.GnssBlock.GALILEO_IOV,
      "GALILEO-1",
      () => orbit.GnssAttitudeLaw.galileoIov(radians(15), radians(2)),
    ],
    [orbit.GnssBlock.GALILEO_FOC, "GALILEO-2", () => orbit.GnssAttitudeLaw.cosineTurn(focTurn)],
    [orbit.GnssBlock.BEIDOU2_GEO, "BEIDOU-2G", orbit.GnssAttitudeLaw.orbitNormal],
    [
      orbit.GnssBlock.BEIDOU2_IGSO,
      "BEIDOU-2I",
      () => orbit.GnssAttitudeLaw.beidouOrbitNormalSwitch(radians(4.1), radians(3.9)),
    ],
    [
      orbit.GnssBlock.BEIDOU2_MEO,
      "BEIDOU-2M",
      () => orbit.GnssAttitudeLaw.beidouOrbitNormalSwitch(radians(4.1), radians(3.9)),
    ],
    [orbit.GnssBlock.BEIDOU3_GEO, "BEIDOU-3G-CAST", orbit.GnssAttitudeLaw.orbitNormal],
    [orbit.GnssBlock.BEIDOU3_IGSO, "BEIDOU-3I", () => orbit.GnssAttitudeLaw.cosineTurn(igsoTurn)],
    [
      () => orbit.GnssBlock.beidou3Meo(orbit.BeidouManufacturer.Cast),
      "BEIDOU-3M-CAST",
      () => orbit.GnssAttitudeLaw.cosineTurn(meoTurn),
    ],
    [
      () => orbit.GnssBlock.beidou3Meo(orbit.BeidouManufacturer.Secm),
      "BEIDOU-3M-SECM",
      () => orbit.GnssAttitudeLaw.fixedBetaYawSteering(radians(3)),
    ],
    [
      () => orbit.GnssBlock.beidou3Meo(),
      "BEIDOU-3M",
      () => orbit.GnssAttitudeLaw.beidou3MeoUnknownManufacturer(meoTurn, radians(3)),
    ],
  ]) {
    using block = makeBlock();
    using lawForBlock = orbit.GnssAttitudeLaw.forBlock(block, prn);
    using specified = makeLaw();
    assert.equal(block.igsName(), name);
    assert.ok(lawForBlock.equals(specified), name);
  }

  assert.throws(
    () => orbit.nominalYaw(vector(NaN, 0, 0), vel, sun),
    typed("GnssAttitudeError", { kind: "nonFinite", field: "position_m" }),
  );
  assert.throws(
    () => orbit.dybAxes(vector(0, 0, 0), sun),
    typed("GnssAttitudeError", { kind: "zeroLength", field: "position_m" }),
  );
  assert.throws(
    () => orbit.GnssAttitudeLaw.forBlock(iia),
    typed("GnssAttitudeError", { kind: "noDefaultYawRate", prn: null }),
  );
  assert.throws(
    () => orbit.GnssBlock.fromIgsName("UNKNOWN"),
    (error) => {
      assert.equal(error.toString(), 'unknown GNSS block name "UNKNOWN"');
      return typed("GnssBlockError", { kind: "unknown", name: "UNKNOWN" })(error);
    },
  );
  assert.throws(
    () => orbit.GnssBlock.fromIgsName("BLOCK I"),
    typed("GnssBlockError", { kind: "unsupported", name: "BLOCK I" }),
  );
  assert.throws(
    () => orbit.GnssBlock.fromConstellationBlockType(orbit.GnssSystem.BeiDou, "BDS-2"),
    typed("GnssBlockError", { kind: "beidouOrbitClassRequired", blockType: "BDS-2" }),
  );
  assert.throws(
    () => orbit.dybAxes(pos, vector(1e11, 0, 0)),
    typed("GnssAttitudeError", { kind: "degenerateGeometry", tS: null }),
  );
  assert.equal(
    orbit.beidouManufacturerCompare(orbit.BeidouManufacturer.Cast, orbit.BeidouManufacturer.Secm),
    -1,
  );
  assert.equal(
    orbit.beidouOrbitClassCompare(orbit.BeidouOrbitClass.Meo, orbit.BeidouOrbitClass.Geo),
    1,
  );
});

test("Earth and Moon eclipses, gradients and gradient refusals", () => {
  const r = vector(7000, 0, 0);
  const s = vector(150000000, 0, 0);
  const moon = vector(300000, 0, 0);
  const model = orbit.EarthShadowModel.Wgs84Oblate;
  assert.equal(orbit.IAU_2015_SOLAR_RADIUS_KM(), 695700);
  assert.equal(orbit.moonShadowFraction(r, s, moon), 1);
  assert.equal(orbit.shadowFractionWithMoon(r, s, moon), 1);
  assert.equal(orbit.shadowFractionWithModelAndMoon(r, s, moon, model), 1);
  assert.equal(orbit.statusWithMoon(r, s, moon), "umbra");
  assert.equal(orbit.statusWithModelAndMoon(r, s, moon, model), "umbra");
  const clearMoon = vector(-384000, 0, 0);
  using g1 = orbit.shadowFractionWithGradient(r, s);
  using g2 = orbit.shadowFractionWithModelAndGradient(r, s, model);
  using g3 = orbit.shadowFractionWithModelMoonAndGradient(r, s, clearMoon, model);
  for (const gradient of [g1, g2, g3]) {
    assert.equal(gradient.fraction, 0);
    assert.deepEqual(values(gradient.gradientPerKm).map(Math.abs), [0, 0, 0]);
  }

  assert.throws(
    () => orbit.shadowFractionWithGradient(r, vector(0, 0, 0)),
    typed("ShadowGradientError", {
      kind: "eclipse",
      cause: { kind: "invalidInput", field: "sun_pos", reason: "zero vector" },
    }),
  );
  assert.throws(
    () => orbit.shadowFractionWithGradient(vector(-2000000, 0, 0), s),
    (error) => {
      assert.equal(error.toString(), "shadow fraction gradient is not finite");
      return typed("ShadowGradientError", { kind: "numericalFailure" })(error);
    },
  );
  assert.throws(
    () => orbit.moonShadowFraction(r, s, vector(NaN, 0, 0)),
    typed("EclipseError", { kind: "invalidInput", field: "moon_pos", reason: "not finite" }),
  );
});

function propagationContext() {
  using orientation = new orbit.Iers2010EarthOrientationProvider(
    [52941, 52942, 52943].map(
      (day) => new orbit.Iers2010EopSample(day, 0.1, 0.2, -0.2, 0.001, 0, 0),
    ),
  );
  using context = new orbit.PropagationContext();
  return context.withBodyFixedFrameProvider(orientation);
}

const bits = (array) =>
  Array.from(array, (value) => `0x${f64Bits(value).toString(16).padStart(16, "0")}`);
const stateBits = (state) => ({
  positionKm: bits(state.positionKm),
  velocityKmS: bits(state.velocityKmS),
});
const numerical = fixtureJson("numerical_models.json");

test("explicit orientation governs nonzero state, transition and covariance propagation", () => {
  const expected = numerical.contextPropagation;
  const times = vector(expected.epoch, expected.epoch + 60, expected.epoch + 120);
  const request = {
    epochS: expected.epoch,
    positionKm: [6770, 170, 200],
    velocityKmS: [-0.2, 7.55, 0.8],
    timesS: times,
    forceModel: "earth_phase_b",
    integrator: "rk4",
    initialStepS: 2,
    maxStepS: 2,
  };
  using context = propagationContext();
  using initial = new orbit.CartesianState(expected.epoch, request.positionKm, request.velocityKmS);
  using model = new orbit.StatePropagator(initial, {
    forceModel: request.forceModel,
    integrator: "rk4",
    integratorOptions: { initialStepS: 2, maxStepS: 2 },
  });
  const checkEphemeris = (ephemeris, reference) => {
    assert.deepEqual(values(ephemeris.timesS), values(times));
    assert.deepEqual(
      bits(ephemeris.positionKm),
      reference.flatMap((entry) => entry.positionKm),
    );
    assert.deepEqual(
      bits(ephemeris.velocityKmS),
      reference.flatMap((entry) => entry.velocityKmS),
    );
  };
  using baseline = orbit.propagateState(request);
  checkEphemeris(baseline, expected.baseline);
  using configured = orbit.propagateState({ ...request, context });
  checkEphemeris(configured, expected.states);
  assert.notDeepEqual(expected.states.at(-1), expected.baseline.at(-1));
  const states = model.ephemerisWithContext(times, context);
  try {
    assert.deepEqual(
      states.map((state) => state.epochTdbSeconds),
      values(times),
    );
    assert.deepEqual(states.map(stateBits), expected.states);
  } finally {
    for (const state of states) state.free();
  }
  assert.notEqual(expected.stm[3], expected.stm[18]);
  assert.deepEqual(bits(model.stateTransitionMatrixToWithContext(times[2], context)), expected.stm);
  const diagonal = [1e-4, 2e-4, 3e-4, 1e-8, 2e-8, 3e-8];
  const covariance = Float64Array.from({ length: 36 }, (_, index) =>
    index % 7 === 0 ? diagonal[index / 7] : 0,
  );
  using propagated = model.propagateCovarianceWithContext(covariance, times, context, {
    covarianceFrame: "inertial",
    outputFrame: "inertial",
    processNoise: { qRadialKm2S3: 1e-12, qTransverseKm2S3: 2e-12, qNormalKm2S3: 3e-12 },
  });
  assert.deepEqual(bits(propagated.covariance), expected.covariance);
  const pair = model.propagateStateWithCovarianceWithContext(covariance, 120, context);
  using pairState = pair.state;
  assert.equal(pairState.epochTdbSeconds, times[2]);
  assert.deepEqual(stateBits(pairState), expected.pairState);
  assert.deepEqual(bits(pair.covariance), expected.pairCovariance);
});

test("propagation composes additional forces and preserves fixed-drag extra keys", () => {
  const expected = numerical.contextPropagation;
  const times = vector(expected.epoch, expected.epoch + 60, expected.epoch + 120);
  using context = propagationContext();
  using radial = orbit.EmpiricalRtnAcceleration.constantTerms();
  using radialTerm = radial.withTerm(orbit.RtnAxis.Radial, orbit.EmpiricalHarmonic.Constant, 1e-7);
  using radialForce = radialTerm.asForceModel();
  using normalTerm = radial.withTerm(
    orbit.RtnAxis.CrossTrack,
    orbit.EmpiricalHarmonic.Constant,
    3e-7,
  );
  using normalForce = normalTerm.asForceModel();
  const additionalForces = [radialForce, normalForce];
  const drag = {
    bcFactorM2Kg: 0.02,
    cutoffAltitudeKm: 100,
    spaceWeather: { f107: 150, f107a: 150, ap: 12 },
  };
  const request = {
    epochS: expected.epoch,
    positionKm: [6770, 170, 200],
    velocityKmS: [-0.2, 7.55, 0.8],
    timesS: times,
    forceModel: "earth_phase_b",
    integrator: "rk4",
    initialStepS: 2,
    maxStepS: 2,
    additionalForces,
  };
  using table = orbit.parseSpaceWeather(fixture("space_weather/SW-All-20260702-trim.csv"));
  using fixed = orbit.propagateState({
    ...request,
    context,
    denseOutput: true,
    maxsteps: 1,
    drag: { ...drag, cutoffAltitudekm: 1000, spaceWeather: { ...drag.spaceWeather, Ap: 99 } },
  });
  using sourced = orbit.propagateStateWithSpaceWeather({ ...request, drag }, table, context);
  for (const [result, kind] of [
    [fixed, "fixed"],
    [sourced, "table"],
  ]) {
    assert.deepEqual(
      bits(result.positionKm),
      expected[kind].flatMap((entry) => entry.positionKm),
    );
    assert.deepEqual(
      bits(result.velocityKmS),
      expected[kind].flatMap((entry) => entry.velocityKmS),
    );
  }
  using initial = new orbit.CartesianState(expected.epoch, request.positionKm, request.velocityKmS);
  const modelOptions = {
    forceModel: request.forceModel,
    integrator: "rk4",
    integratorOptions: { initialStepS: 2, maxStepS: 2 },
  };
  using model = new orbit.StatePropagator(initial, { ...modelOptions, drag });
  model.setSpaceWeather(table);
  const states = model.ephemerisWithContext(times, context, additionalForces);
  try {
    assert.deepEqual(states.map(stateBits), expected.table);
  } finally {
    for (const state of states) state.free();
  }
  using propagated = model.propagateToWithContext(times[2], context, { additionalForces });
  using finalState = propagated.finalState;
  assert.equal(finalState.epochTdbSeconds, times[2]);
  assert.deepEqual(stateBits(finalState), expected.finalState);
  assert.equal(radialForce.parameterCount(), 3);
  assert.equal(normalForce.parameterCount(), 3);

  const refusal = (error) => {
    assert.equal(error.name, "Error");
    assert.deepEqual(error.detail, {
      family: "propagation",
      cause: { kind: "invalidInput", message: "space weather source without drag" },
    });
    return true;
  };
  using withoutDrag = new orbit.StatePropagator(initial, modelOptions);
  assert.throws(() => withoutDrag.setSpaceWeather(table), refusal);
  assert.throws(() => orbit.propagateStateWithSpaceWeather(request, table, context), refusal);
  const unchanged = withoutDrag.ephemerisWithContext(times, context);
  try {
    assert.deepEqual(unchanged.map(stateBits), expected.states);
  } finally {
    for (const state of unchanged) state.free();
  }
});

test("table-driven propagation rejects unknown request, drag and weather keys", () => {
  const request = {
    epochS: 0,
    positionKm: [7000, 0, 0],
    velocityKmS: [0, 7.5, 0],
    timesS: [30],
  };
  const keys =
    "epochS, positionKm, velocityKmS, timesS, forceModel, integrator, absTol, relTol, initialStepS, minStepS, maxStepS, maxSteps, muKm3S2, drag, additionalForces";
  using table = orbit.parseSpaceWeather(fixture("space_weather/SW-All-20260702-trim.csv"));
  using context = new orbit.PropagationContext();
  const propagate = (extra) =>
    orbit.propagateStateWithSpaceWeather({ ...request, ...extra }, table, context);
  for (const [field, value] of [
    ["denseOutput", true],
    ["maxsteps", 10],
    ["context", context],
  ]) {
    assert.throws(() => propagate({ [field]: value }), {
      name: "TypeError",
      message: `unknown propagation request property '${field}'; expected one of ${keys}`,
    });
  }
  assert.throws(() => propagate({ drag: { bcFactorM2Kg: 0.02, cutoffAltitudekm: 100 } }), {
    name: "TypeError",
    message:
      "unknown drag property 'cutoffAltitudekm'; expected one of bcFactorM2Kg, ballisticCoefficientKgM2, cd, areaM2, massKg, cutoffAltitudeKm, spaceWeather",
  });
  assert.throws(() => propagate({ drag: { bcFactorM2Kg: 0.02, spaceWeather: { Ap: 12 } } }), {
    name: "TypeError",
    message: "unknown space weather property 'Ap'; expected one of f107, f107a, ap",
  });

  for (const [additionalForces, message] of [
    [{}, "additionalForces must be an array of ForceModel handles"],
    [[{}], "additionalForces must contain live ForceModel handles"],
  ]) {
    assert.throws(() => orbit.propagateState({ ...request, additionalForces }), {
      name: "TypeError",
      message,
    });
  }
});

test("state propagator and single-epoch options reject unknown keys", () => {
  using initial = new orbit.CartesianState(0, vector(7000, 0, 0), vector(0, 7.5, 0));
  using force = orbit.ForceModel.twoBody();
  using context = new orbit.PropagationContext();
  for (const [key, value] of [
    ["additionalForces", [force]],
    ["context", context],
    ["denseOutput", true],
  ]) {
    const options = Object.defineProperty({ forceModel: "earth_phase_b" }, key, { value });
    assert.throws(() => new orbit.StatePropagator(initial, options), {
      name: "TypeError",
      message: `unknown state propagator options property '${key}'; expected one of forceModel, integrator, integratorOptions, drag`,
    });
  }
  assert.throws(
    () =>
      new orbit.StatePropagator(initial, {
        forceModel: { kind: "composite", earthRadiationPressure: { moonEclips: false } },
      }),
    {
      name: "TypeError",
      message:
        "unknown Earth radiation pressure property 'moonEclips'; expected one of cr, areaToMassM2Kg, moonEclipse",
    },
  );
  using model = new orbit.StatePropagator(initial);
  const refusal = {
    name: "TypeError",
    message:
      "unknown state propagation options property 'maxSteps'; expected one of denseOutput, additionalForces",
  };
  assert.throws(() => model.propagateTo(0, { maxSteps: 1 }), refusal);
  assert.throws(() => model.propagateToWithContext(0, context, { maxSteps: 1 }), refusal);
});

test("explicit-context covariance options reject unknown keys at each level", () => {
  using initial = new orbit.CartesianState(0, vector(7000, 0, 0), vector(0, 7.5, 0));
  using model = new orbit.StatePropagator(initial);
  using context = new orbit.PropagationContext();
  const propagate = (options) =>
    model.propagateCovarianceWithContext(new Float64Array(36), vector(0), context, options);
  assert.throws(() => propagate({ outputframe: "rtn" }), {
    name: "TypeError",
    message:
      "unknown covariance options property 'outputframe'; expected one of covarianceFrame, outputFrame, processNoise",
  });
  assert.throws(() => propagate({ processNoise: { qRadialKm2S: 1e-12 } }), {
    name: "TypeError",
    message:
      "unknown process noise property 'qRadialKm2S'; expected one of qRadialKm2S3, qTransverseKm2S3, qNormalKm2S3",
  });
});

test("orbit parameter fit options reject unknown keys at each level", () => {
  assert.throws(() => new orbit.OrbitParameterFitOptions({ estimatedParameter: ["Cr"] }), {
    name: "TypeError",
    message:
      "unknown orbit parameter fit options property 'estimatedParameter'; expected one of forceModel, additionalForces, drag, estimatedParameters, integrator, integratorOptions, observationSigmaM, maxIterations, convergenceThreshold, maxConditionNumber, minLedgerSamples",
  });
  assert.throws(() => new orbit.OrbitParameterFitOptions({ drag: { bcFactor: 0.02 } }), {
    name: "TypeError",
    message:
      "unknown drag property 'bcFactor'; expected one of bcFactorM2Kg, ballisticCoefficientKgM2, cd, areaM2, massKg, cutoffAltitudeKm, spaceWeather",
  });
});

test("integration options reject unknown keys in new propagation and fitting APIs", () => {
  using initial = new orbit.CartesianState(0, vector(7000, 0, 0), vector(0, 7.5, 0));
  using context = new orbit.PropagationContext();
  using force = orbit.ForceModel.twoBody();
  using equations = new orbit.VariationalEquations(force, vector(), vector());
  for (const [key, value] of [
    ["denseOutput", true],
    ["maxsteps", 1],
  ]) {
    const integratorOptions = { [key]: value };
    const refusal = {
      name: "TypeError",
      message: `unknown integrator options property '${key}'; expected one of absTol, relTol, initialStepS, minStepS, maxStepS, maxSteps`,
    };
    assert.throws(
      () => orbit.propagateVariational(initial, [], equations, "dp54", integratorOptions, context),
      refusal,
    );
    assert.throws(() => new orbit.OrbitParameterFitOptions({ integratorOptions }), refusal);
    assert.throws(() => new orbit.StatePropagator(initial, { integratorOptions }), refusal);
  }
});

test("numeric astrodynamics enums preserve their declared values", () => {
  assert.deepEqual(orbit.AttitudeMode, {
    Nominal: 0,
    NoonTurn: 1,
    MidnightTurn: 2,
    Shadow: 3,
    OrbitNormal: 4,
    ModifiedYawSteering: 5,
    0: "Nominal",
    1: "NoonTurn",
    2: "MidnightTurn",
    3: "Shadow",
    4: "OrbitNormal",
    5: "ModifiedYawSteering",
  });
  assert.deepEqual(
    [orbit.CosineTurnStartAngle.Colinearity, orbit.CosineTurnStartAngle.OrbitAngle],
    [0, 1],
  );
  assert.deepEqual([orbit.BeidouManufacturer.Cast, orbit.BeidouManufacturer.Secm], [0, 1]);
  assert.deepEqual(
    [orbit.BeidouOrbitClass.Geo, orbit.BeidouOrbitClass.Igso, orbit.BeidouOrbitClass.Meo],
    [0, 1, 2],
  );
  assert.deepEqual(
    [
      orbit.DeBody.Sun,
      orbit.DeBody.Mercury,
      orbit.DeBody.Venus,
      orbit.DeBody.EarthMoon,
      orbit.DeBody.Earth,
      orbit.DeBody.Moon,
      orbit.DeBody.Mars,
      orbit.DeBody.Jupiter,
      orbit.DeBody.Saturn,
      orbit.DeBody.Uranus,
      orbit.DeBody.Neptune,
      orbit.DeBody.Pluto,
      orbit.DeBody.SolarSystem,
    ],
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
  );
  assert.deepEqual([orbit.ShadowFunction.ApparentDisc, orbit.ShadowFunction.Conical], [0, 1]);
  assert.deepEqual(
    [
      orbit.EmpiricalHarmonic.Constant,
      orbit.EmpiricalHarmonic.Cosine,
      orbit.EmpiricalHarmonic.Sine,
    ],
    [0, 1, 2],
  );
  assert.deepEqual(
    [orbit.RtnAxis.Radial, orbit.RtnAxis.AlongTrack, orbit.RtnAxis.CrossTrack],
    [0, 1, 2],
  );
  assert.deepEqual(
    [orbit.AnalyticTheory.EcksteinHechler, orbit.AnalyticTheory.BrouwerLyddane],
    [0, 1],
  );
});

test("decay uses explicit IERS orientation and preserves released request keys", () => {
  using weather = new orbit.SpaceWeather(150, 150, 12);
  using drag = orbit.DragForce.fromBcFactor(0.02, weather, 100);
  using table = orbit.parseSpaceWeather(fixture("space_weather/SW-All-20260702-trim.csv"));
  using context = propagationContext();
  const request = {
    epochS: numerical.contextPropagation.epoch,
    positionKm: [6478.3, 0, 0.13],
    velocityKmS: [0, 7.85, 0.01],
    scanStepS: 30,
    maxDurationS: 7200,
    maxScanSamples: 300,
    reentryAltitudeKm: 100,
    crossingToleranceS: 1,
  };
  assert.notDeepEqual(numerical.contextDecay.fixed, numerical.contextDecay.baselineFixed);
  assert.notDeepEqual(numerical.contextDecay.table, numerical.contextDecay.baselineTable);
  for (const [result, kind] of [
    [orbit.estimateDecay(drag, { ...request, maxscansamples: 0 }), "baselineFixed"],
    [orbit.estimateDecayWithContext(drag, request, context), "fixed"],
    [orbit.estimateDecayWithSourceAndContext(drag, table, request, context), "table"],
  ]) {
    assert.ok(result.timeToDecayS > 0);
    const expected = numerical.contextDecay[kind];
    assert.deepEqual(bits([result.timeToDecayS, result.reentryEpochS, result.reentryAltitudeKm]), [
      expected.timeToDecayS,
      expected.reentryEpochS,
      expected.reentryAltitudeKm,
    ]);
    assert.deepEqual(bits(result.reentryPositionKm), expected.reentryState.positionKm);
    assert.deepEqual(bits(result.reentryVelocityKmS), expected.reentryState.velocityKmS);
  }
});

test("context-aware decay rejects unknown request and policy keys", () => {
  using weather = new orbit.SpaceWeather(150, 150, 12);
  using drag = orbit.DragForce.fromBcFactor(0.02, weather, 100);
  using table = orbit.parseSpaceWeather(fixture("space_weather/SW-All-20260702-trim.csv"));
  using context = new orbit.PropagationContext();
  const request = { epochS: 0, positionKm: [6478.3, 0, 0], velocityKmS: [0, 7.85, 0] };
  const keys =
    "epochS, positionKm, velocityKmS, forceModel, integrator, reentryAltitudeKm, scanStepS, crossingToleranceS, maxDurationS, maxScanSamples";
  assert.throws(
    () => orbit.estimateDecayWithContext(drag, { ...request, scanSteps: 30 }, context),
    {
      name: "TypeError",
      message: `unknown decay request property 'scanSteps'; expected one of ${keys}`,
    },
  );
  const sourced = (extra) =>
    orbit.estimateDecayWithSourceAndContext(drag, table, { ...request, ...extra }, context);
  assert.throws(() => sourced({ scanSteps: 30 }), {
    name: "TypeError",
    message: `unknown sourced decay request property 'scanSteps'; expected one of ${keys}, policy`,
  });
  assert.throws(() => sourced({ policy: { requireGeomagnetc: false } }), {
    name: "TypeError",
    message:
      "unknown space-weather policy property 'requireGeomagnetc'; expected one of allowInterpolated, allowNotObserved, allowDailyPredicted, allowMonthlyPredicted, requireGeomagnetic",
  });
});
