import { test } from "node:test";
import assert from "node:assert/strict";

import * as orbit from "../pkg-node/sidereon.js";
import { fixture, fixtureJson } from "./helpers.mjs";

const golden = fixtureJson("astrodynamics.json").attitudeTrajectories;
const vector = (...values) => Float64Array.of(...values);
const values = (array) => Array.from(array);
const Trajectory = orbit.OrbitAttitudeTrajectory;
const satellite = "G03";

const typed = (name, detail) => (error) => {
  assert.equal(error.name, name);
  assert.equal(error.kind, detail.kind);
  assert.deepEqual(error.detail, detail);
  return true;
};

function checkAttitude(name, trajectory) {
  const expected = golden[name];
  using sample = trajectory.sample(expected.epoch);
  assert.deepEqual(
    [values(sample.positionM), values(sample.velocityMS), values(sample.sunPositionM)],
    expected.sample,
  );
  using law = orbit.GnssAttitudeLaw.orbitNormal();
  using attitude = orbit.gnssAttitude(law, trajectory, expected.epoch);
  using dyb = attitude.dyb;
  assert.equal(attitude.mode, orbit.AttitudeMode.OrbitNormal);
  assert.deepEqual(
    [attitude.yawRad, attitude.nominalYawRad, attitude.betaRad, attitude.orbitAngleRad],
    expected.angles,
  );
  assert.deepEqual(
    [attitude.bodyX, attitude.bodyY, attitude.bodyZ, dyb.d, dyb.y, dyb.b].map(values),
    expected.axes,
  );
}

test("GNSS attitude on parsed SP3 and broadcast orbits with IERS and default orientation", () => {
  const product = orbit.loadSp3(fixture("sp3/GBM0MGXRAP_20201770000_01D_05M_ORB_120epoch.sp3"));
  const nav = orbit.loadRinexNav(fixture("nav/ESBC00DNK_R_20201770000_01D_MN.rnx"));
  using orientation = new orbit.Iers2010EarthOrientationProvider(
    [59025, 59026].map((day) => new orbit.Iers2010EopSample(day, 0.1, 0.2, -0.2, 0.001, 0, 0)),
  );
  using sp3 = Trajectory.fromSp3(product, satellite, orientation);
  using broadcast = Trajectory.fromBroadcast(nav, "G01");
  using broadcastWithOrientation = Trajectory.fromBroadcast(nav, "G01", orientation);
  using origin = orbit.ClockInstant.fromNanos(orbit.TimeScale.Gpst, 0n);
  const predicted = orbit.PredictedSp3.fromPredictionFlags(product);
  const broadcastOrbit = new orbit.BroadcastOrbit(nav, origin);
  const broadcastSource = broadcastOrbit.asOrbitSource();
  using predictedTrajectory = Trajectory.fromOrbitSource(predicted, satellite);
  using predictedWithOrientation = Trajectory.fromOrbitSource(predicted, satellite, orientation);
  using sameSp3 = Trajectory.fromSp3(product, satellite, orientation);
  using broadcastTrajectory = Trajectory.fromOrbitSource(broadcastOrbit, "G01");
  using broadcastFromHandle = Trajectory.fromOrbitSource(broadcastSource, "G01");
  product.free();
  nav.free();
  predicted.free();
  broadcastOrbit.free();
  broadcastSource.free();

  checkAttitude("sp3", sp3);
  checkAttitude("sp3", sameSp3);
  checkAttitude("broadcast", broadcast);
  checkAttitude("broadcastIers", broadcastWithOrientation);
  checkAttitude("predictedIers", predictedWithOrientation);
  checkAttitude("predicted", predictedTrajectory);
  checkAttitude("broadcastSource", broadcastTrajectory);
  checkAttitude("broadcastSource", broadcastFromHandle);
});

test("GNSS attitude on a sampled Earth-fixed orbit with a one-sided velocity difference", () => {
  using origin = orbit.ClockInstant.fromNanos(orbit.TimeScale.Tdb, 0n);
  using next = orbit.ClockInstant.fromNanos(orbit.TimeScale.Tdb, 43_200_000_000_000n);
  using nodes = new orbit.OrbitTrajectory("ITRF2020", origin, [
    new orbit.TrajectorySample(satellite, origin, vector(20e6, 10e6, 2e6)),
    new orbit.TrajectorySample(satellite, next, vector(20e6, 10.02e6, 2e6)),
  ]);
  using trajectory = Trajectory.fromOrbitSource(nodes, satellite);
  assert.equal(trajectory.sample(0), undefined);
  using withStep = trajectory.withVelocityHalfStepS(43200);
  checkAttitude("sampled", withStep);
  for (const [step, reason] of [
    [0, "not positive and finite"],
    [NaN, "not positive and finite"],
    [1e-9, "below the 1e-8 s resolution of SP3 epochs"],
  ]) {
    assert.throws(
      () => trajectory.withVelocityHalfStepS(step),
      typed("OrbitScoreError", { kind: "invalidInput", field: "velocity_half_step_s", reason }),
    );
  }
});

function ecksteinHechlerTrajectory() {
  using circular = new orbit.CircularElements(26560, 0, 0, 0.4, 0, 4.9);
  using gravity = orbit.ZonalHarmonics.earth();
  using propagator = orbit.EcksteinHechlerPropagator.fromMeanElements(0, circular, gravity);
  return Trajectory.fromEcksteinHechler(propagator);
}

test("GNSS attitude on analytic orbits with analytic and JPL DE Sun positions", () => {
  using eh = ecksteinHechlerTrajectory();
  using gravity = orbit.ZonalHarmonics.earth();
  using keplerian = new orbit.KeplerianElements(7000, 0.03, 0.7, 0.2, 0.3, 0.4);
  using propagator = orbit.BrouwerLyddanePropagator.fromMeanElements(0, keplerian, gravity, 1e-17);
  using bl = Trajectory.fromBrouwerLyddane(propagator);
  const de = orbit.DeEphemeris.fromBytes(fixture("bodies/de440.440"));
  using withDe = bl.withDeEphemeris(de);
  de.free();

  checkAttitude("ecksteinHechler", eh);
  checkAttitude("brouwerLyddane", bl);
  checkAttitude("analyticDe", withDe);
  assert.equal(withDe.sample(600), undefined);
});

test("GNSS attitude on continuous DP5(4) output and outside the integration arc", () => {
  using initial = new orbit.CartesianState(0, vector(7000, 1000, 200), vector(-1, 7.5, 0.8));
  const options = {
    forceModel: { muKm3S2: 398600.4418 },
  };
  using propagator = new orbit.StatePropagator(initial, options);
  const result = propagator.propagateTo(300, { denseOutput: true });
  using final = result.finalState;
  assert.equal(final.epochTdbSeconds, 300);
  assert.deepEqual([values(final.positionKm), values(final.velocityKmS)], golden.finalState);

  const dense = result.dense;
  assert.ok(dense instanceof orbit.DenseOutput);
  assert.deepEqual(dense.coverageTdbSeconds(), Float64Array.from(golden.denseCoverage.forward));
  using trajectory = Trajectory.fromDenseOutput(dense);
  dense.free();
  result.free();
  checkAttitude("dense", trajectory);
  using law = orbit.GnssAttitudeLaw.orbitNormal();
  using at123 = orbit.gnssAttitude(law, trajectory, 123);
  using alsoAt123 = orbit.gnssAttitude(law, trajectory, 123);
  using at124 = orbit.gnssAttitude(law, trajectory, 124);
  assert.ok(at123.equals(alsoAt123));
  assert.ok(!at123.equals(at124));
  for (const tS of [-1, 301]) {
    assert.equal(trajectory.sample(tS), undefined);
    assert.throws(
      () => orbit.gnssAttitude(law, trajectory, tS),
      (error) => {
        assert.equal(error.toString(), `trajectory has no state at t = ${tS} s`);
        return typed("GnssAttitudeError", { kind: "trajectoryUnavailable", tS })(error);
      },
    );
  }

  using context = new orbit.PropagationContext();
  using withContext = propagator.propagateToWithContext(300, context, { denseOutput: true });
  using withContextFinal = withContext.finalState;
  using withContextDense = withContext.dense;
  using withContextTrajectory = Trajectory.fromDenseOutput(withContextDense);
  assert.deepEqual(
    [values(withContextFinal.positionKm), values(withContextFinal.velocityKmS)],
    golden.finalState,
  );
  checkAttitude("dense", withContextTrajectory);
  using backward = propagator.propagateTo(-300, { denseOutput: true });
  using backwardDense = backward.dense;
  assert.deepEqual(
    backwardDense.coverageTdbSeconds(),
    Float64Array.from(golden.denseCoverage.backward),
  );
  using empty = propagator.propagateTo(0, { denseOutput: true });
  using emptyDense = empty.dense;
  assert.equal(emptyDense.coverageTdbSeconds(), undefined);
  using noDenseOutput = propagator.propagateTo(300, { denseOutput: false });
  assert.equal(noDenseOutput.dense, undefined);
  using fixedStep = new orbit.StatePropagator(initial, { ...options, integrator: "rk4" });
  using fixedResult = fixedStep.propagateTo(300, { denseOutput: true });
  assert.equal(fixedResult.dense, undefined);
});

test("GNSS attitude laws preserve the reason and epoch of each refusal", () => {
  using trajectory = ecksteinHechlerTrajectory();
  using block = orbit.GnssBlock.beidou3Meo();
  using requiresManufacturer = orbit.GnssAttitudeLaw.forBlock(block);
  using invalid = orbit.GnssAttitudeLaw.gpsIir(-0.1);
  using invalidNonFinite = orbit.GnssAttitudeLaw.gpsIir(Infinity);
  using unfinished = orbit.GnssAttitudeLaw.galileoIov(1.57, 1.56);
  using domain = orbit.GnssAttitudeLaw.gpsIia(100, 0.01, orbit.GPS_SHADOW_LIMIT_RAD());
  using normal = orbit.GnssAttitudeLaw.orbitNormal();
  for (const [law, time, detail] of [
    [requiresManufacturer, 0, { kind: "beidouManufacturerRequired", tS: 0 }],
    [invalid, 0, { kind: "invalidParameter", field: "yaw_rate_rad_s", value: -0.1 }],
    [invalidNonFinite, 0, { kind: "invalidParameter", field: "yaw_rate_rad_s", value: Infinity }],
    [unfinished, 0, { kind: "turnEndNotFound", tS: 0 }],
    [
      domain,
      -30000,
      {
        kind: "outsideValidatedDomain",
        tS: -30000,
        reason:
          "GPS IIA maximum yaw rate above 1e5 times the orbital rate, where " +
          "the shadow crossing's yaw is set only to the rate times the uncertainty of the shadow entry",
      },
    ],
    [normal, NaN, { kind: "nonFinite", field: "t_s" }],
  ]) {
    assert.throws(
      () => orbit.gnssAttitude(law, trajectory, time),
      typed("GnssAttitudeError", detail),
    );
  }
});
