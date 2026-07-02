import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import {
  Spk,
  angularSeparation,
  angularSeparationCoords,
  betaAngle,
  betaAngleFromState,
  coe2eq,
  coe2mee,
  coe2rv,
  cwPropagate,
  cwStm,
  eccentricToMean,
  eccentricToTrue,
  eq2coe,
  eq2rv,
  estimateDecay,
  lunarSolarEclipsesSpk,
  meanMotionCircular,
  meanMotionFromState,
  meanToEccentric,
  meanToTrue,
  mee2coe,
  mee2rv,
  meridianTransits,
  moonPhasesSpk,
  observe,
  observeSpkBody,
  planetaryEvents,
  positionAngle,
  propagateKepler,
  propagateState,
  relativeState,
  rtnRotation,
  rv2eq,
  rv2mee,
  rv2coe,
  seasonsSpk,
  solveKepler,
  trueToEccentric,
  trueToMean,
  DragForce,
  SpaceWeather,
} from "../pkg-node/sidereon.js";

const MU = 398600.4418;
const CORE_FIXTURES = fileURLToPath(new URL("./fixtures", import.meta.url));

const coreFixture = (rel) => readFileSync(`${CORE_FIXTURES}/${rel}`);
const close = (actual, expected, tol, label) =>
  assert.ok(Math.abs(actual - expected) <= tol, `${label}: ${actual} vs ${expected}`);
const wrapDiff = (a, b) => Math.atan2(Math.sin(a - b), Math.cos(a - b));
const closeAngle = (actual, expected, tol, label) =>
  assert.ok(Math.abs(wrapDiff(actual, expected)) <= tol, `${label}: ${actual} vs ${expected}`);
const finite3 = (values) => values.length === 3 && values.every(Number.isFinite);
const unixUs = (year, month, day, hour = 0, minute = 0, second = 0, micros = 0) =>
  BigInt(Date.UTC(year, month - 1, day, hour, minute, second)) * 1000n + BigInt(micros);
const secondsApart = (actualUs, expectedUs) =>
  Math.abs(Number(actualUs) - Number(expectedUs)) / 1e6;
const COE = {
  p: 6999.3,
  ecc: 0.01,
  incl: 0.9,
  raan: 0.5,
  argp: 1.0,
  nu: 2.0,
};

test("anomaly helpers solve and round-trip Kepler angles", () => {
  const mean = 0.75;
  const ecc = 0.1;
  const solved = solveKepler(mean, ecc);
  const eccentric = meanToEccentric(mean, ecc);
  close(solved.anomaly, eccentric, 1e-14, "solveKepler");
  assert.ok(solved.iterations > 0);

  close(eccentricToMean(eccentric, ecc), mean, 1e-14, "eccentricToMean");
  close(eccentricToTrue(trueToEccentric(1.25, ecc), ecc), 1.25, 1e-14, "true eccentric round trip");

  const nu = meanToTrue(mean, ecc);
  close(trueToMean(nu, ecc), mean, 1e-14, "mean true round trip");
  closeAngle(eccentricToTrue(eccentric, ecc), nu, 1e-14, "eccentricToTrue");
});

test("Kepler propagation and equinoctial conversions round-trip through core", () => {
  const propagated = propagateKepler(COE, MU, 60.0);
  assert.equal(propagated.orbitType, "ellipticalInclined");
  assert.notEqual(propagated.nu, COE.nu);

  const state = coe2rv(propagated, MU);
  const recovered = rv2coe(state.positionKm, state.velocityKmS, MU);
  close(recovered.a, propagated.a, 1e-6, "propagated a");
  close(recovered.ecc, propagated.ecc, 1e-12, "propagated ecc");
  closeAngle(recovered.nu, propagated.nu, 1e-10, "propagated nu");

  const eq = coe2eq(COE);
  assert.equal(eq.retrograde, "prograde");
  const coeFromEq = eq2coe(eq);
  close(coeFromEq.ecc, COE.ecc, 1e-12, "eq ecc");
  closeAngle(coeFromEq.raan, COE.raan, 1e-12, "eq raan");

  const mee = coe2mee(COE);
  assert.equal(mee.retrograde, "prograde");
  const coeFromMee = mee2coe(mee);
  close(coeFromMee.ecc, COE.ecc, 1e-12, "mee ecc");
  closeAngle(coeFromMee.nu, COE.nu, 1e-12, "mee nu");

  const eqFromRv = rv2eq(state.positionKm, state.velocityKmS, MU);
  const stateFromEq = eq2rv(eqFromRv, MU);
  const meeFromRv = rv2mee(state.positionKm, state.velocityKmS, MU);
  const stateFromMee = mee2rv(meeFromRv, MU);
  for (let i = 0; i < 3; i++) {
    close(stateFromEq.positionKm[i], state.positionKm[i], 1e-6, `eq r${i}`);
    close(stateFromEq.velocityKmS[i], state.velocityKmS[i], 1e-9, `eq v${i}`);
    close(stateFromMee.positionKm[i], state.positionKm[i], 1e-6, `mee r${i}`);
    close(stateFromMee.velocityKmS[i], state.velocityKmS[i], 1e-9, `mee v${i}`);
  }
});

test("sky angles and relative-frame helpers expose core geometry", () => {
  close(
    angularSeparation(Float64Array.from([1, 0, 0]), Float64Array.from([0, 1, 0])),
    90.0,
    1e-12,
    "angularSeparation",
  );
  close(angularSeparationCoords(0, 0, 90, 0), 90.0, 1e-12, "angularSeparationCoords");
  close(positionAngle(0, 0, 90, 0), 90.0, 1e-12, "positionAngle");
  close(betaAngle(Float64Array.from([0, 0, 1]), Float64Array.from([0, 0, 1])), 90.0, 1e-12, "beta");
  close(
    betaAngleFromState(
      Float64Array.from([7000, 0, 0]),
      Float64Array.from([0, 7.5, 0]),
      Float64Array.from([0, 0, 1]),
    ),
    90.0,
    1e-12,
    "beta from state",
  );

  const r = Float64Array.from([7000, 0, 0]);
  const v = Float64Array.from([0, Math.sqrt(MU / 7000), 0]);
  const rot = rtnRotation(r, v);
  assert.equal(rot.length, 9);
  close(rot[0], 1.0, 1e-12, "RTN radial axis");

  const chief = { epochS: 0, positionKm: [7000, 0, 0], velocityKmS: [0, Math.sqrt(MU / 7000), 0] };
  const deputy = {
    epochS: 0,
    positionKm: [7000.1, 0.2, -0.1],
    velocityKmS: [0.0001, Math.sqrt(MU / 7000) + 0.0002, -0.0001],
  };
  const rel = relativeState(chief, deputy);
  assert.ok(finite3(rel.positionKm));
  assert.ok(finite3(rel.velocityKmS));

  const n = meanMotionCircular(7000.0);
  close(meanMotionFromState(r, v), n, 1e-12, "mean motion");
  const stm = cwStm(n, 10.0);
  assert.equal(stm.length, 36);
  const propagated = cwPropagate(rel, n, 10.0);
  assert.ok(finite3(propagated.positionKm));
  assert.ok(finite3(propagated.velocityKmS));
});

test("general body observation returns analytic and SPK apparent quantities", () => {
  const station = { latitudeDeg: 51.4779, longitudeDeg: -0.0015, altitudeKm: 0.046 };
  const epoch = unixUs(2024, 1, 1, 0, 0, 0);

  const sun = observe(station, epoch, "sun", {
    refraction: { pressureMbar: 1013.25, temperatureC: 15.0 },
  });
  assert.ok(Number.isFinite(sun.apparent.rightAscensionDeg));
  assert.ok(Number.isFinite(sun.horizontal.azimuthDeg));
  assert.ok(Number.isFinite(sun.ecliptic.longitudeDeg));

  const spk = new Spk(new Uint8Array(coreFixture("bodies/observe_de.bsp")));
  const mars = observeSpkBody(station, epoch, spk, 4);
  close(mars.apparent.rightAscensionDeg, 267.0545293733468, 1 / 3600, "Mars apparent RA");
  close(mars.apparent.declinationDeg, -23.96185179890772, 1 / 3600, "Mars apparent Dec");
  close(mars.horizontal.rangeKm, 362601889.4988805, 5.0, "Mars range");
});

test("SPK almanac events match core oracle windows", () => {
  const spk = new Spk(new Uint8Array(coreFixture("almanac/almanac_de421.spk")));

  const seasons = seasonsSpk(spk, unixUs(2025, 1, 1), unixUs(2026, 1, 1), 21600.0, 1.0);
  assert.deepEqual(
    seasons.map((event) => event.kind),
    ["marchEquinox", "juneSolstice", "septemberEquinox", "decemberSolstice"],
  );
  assert.ok(secondsApart(seasons[0].timeUnixUs, unixUs(2025, 3, 20, 9, 1, 28, 934474)) < 5.0);

  const phases = moonPhasesSpk(spk, unixUs(2025, 1, 1), unixUs(2025, 2, 1), 21600.0, 1.0);
  const full = phases.find((event) => event.kind === "full");
  const nextNew = phases.find((event) => event.kind === "new");
  assert.ok(full);
  assert.ok(nextNew);
  assert.ok(secondsApart(full.timeUnixUs, unixUs(2025, 1, 13, 22, 26, 54, 547309)) < 5.0);
  assert.ok(secondsApart(nextNew.timeUnixUs, unixUs(2025, 1, 29, 12, 35, 58, 908240)) < 5.0);

  const mars = planetaryEvents(
    spk,
    "mars",
    "opposition",
    unixUs(2025, 1, 1),
    unixUs(2025, 2, 1),
    21600.0,
    1.0,
  );
  assert.equal(mars.length, 1);
  assert.equal(mars[0].kind, "opposition");
  assert.ok(secondsApart(mars[0].timeUnixUs, unixUs(2025, 1, 16, 2, 38, 35, 259890)) < 5.0);
  close(mars[0].elongationDeg, 180.0, 0.01, "Mars elongation");

  const station = { latitudeDeg: 51.4769, longitudeDeg: 0.0, altitudeKm: 0.046 };
  const transits = meridianTransits(
    "sun",
    station,
    unixUs(2025, 3, 20),
    unixUs(2025, 3, 21),
    300.0,
    1.0,
  );
  assert.ok(
    transits.some(
      (event) =>
        event.kind === "upper" &&
        (Number(event.timeUnixUs) - Number(unixUs(2025, 3, 20))) / 3.6e9 > 11.5 &&
        (Number(event.timeUnixUs) - Number(unixUs(2025, 3, 20))) / 3.6e9 < 12.5,
    ),
  );

  const eclipses = lunarSolarEclipsesSpk(
    spk,
    unixUs(2025, 3, 14),
    unixUs(2025, 3, 15),
    21600.0,
    1.0,
  );
  const lunar = eclipses.find((event) => event.kind === "lunarTotal");
  assert.ok(lunar);
  assert.ok(secondsApart(lunar.timeMaximumUnixUs, unixUs(2025, 3, 14, 6, 59, 0)) < 300.0);
  assert.ok(lunar.magnitude > 1.1);
});

test("drag force, decay estimate, and propagator drag option are wired", () => {
  const weather = new SpaceWeather(150.0, 150.0, 12.0);
  const drag = DragForce.fromBcFactor(0.02, weather, 90.0);
  assert.equal(drag.bcFactorM2Kg, 0.02);
  assert.equal(drag.spaceWeather.f107, 150.0);

  const acceleration = drag.acceleration(
    0.0,
    Float64Array.from([6528.137, 0, 0]),
    Float64Array.from([0, 7.8, 0]),
  );
  assert.ok(finite3(acceleration));

  const decay = estimateDecay(drag, {
    epochS: 0.0,
    positionKm: [6500.0, 0.0, 0.0],
    velocityKmS: [0.0, 0.0, 0.0],
    reentryAltitudeKm: 120.0,
    scanStepS: 10.0,
    crossingToleranceS: 0.1,
    maxDurationS: 1800.0,
    maxScanSamples: 200,
  });
  assert.ok(decay.timeToDecayS > 0.0);
  assert.ok(finite3(decay.reentryPositionKm));

  const lowOrbitRadiusKm = 6528.137;
  const lowOrbitSpeedKmS = Math.sqrt(MU / lowOrbitRadiusKm);
  const base = propagateState({
    epochS: 0.0,
    positionKm: [lowOrbitRadiusKm, 0.0, 0.0],
    velocityKmS: [0.0, lowOrbitSpeedKmS, 0.0],
    timesS: [0.0, 60.0],
    forceModel: "two_body",
  });
  const withDrag = propagateState({
    epochS: 0.0,
    positionKm: [lowOrbitRadiusKm, 0.0, 0.0],
    velocityKmS: [0.0, lowOrbitSpeedKmS, 0.0],
    timesS: [0.0, 60.0],
    forceModel: "two_body",
    drag: { bcFactorM2Kg: 0.02, spaceWeather: { f107: 150.0, f107a: 150.0, ap: 12.0 } },
  });
  assert.equal(base.epochCount, 2);
  assert.equal(withDrag.epochCount, 2);
  assert.ok(withDrag.positionKm.every(Number.isFinite));
  const finalDeltaKm = Math.hypot(
    withDrag.positionKm[3] - base.positionKm[3],
    withDrag.positionKm[4] - base.positionKm[4],
    withDrag.positionKm[5] - base.positionKm[5],
  );
  assert.ok(finalDeltaKm > 0.0);
});
