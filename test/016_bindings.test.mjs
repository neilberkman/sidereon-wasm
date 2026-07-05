import { test } from "node:test";
import assert from "node:assert/strict";

import {
  DecayLatch,
  EarthShadowModel,
  Egm2008GridSpacing,
  GeoidGrid,
  TerrestrialFrame,
  Tle,
  fitSp3EcefPreciseOrbit,
  frameCatalogEntry,
  frameCatalogPropagatePosition,
  frameCatalogTransform,
  geodesicDirect,
  geodesicInverse,
  loadSp3,
  parseTdmKvn,
  propagateState,
  shadowFraction,
  shadowFractionWithModel,
  tropoMappingFactors,
} from "../pkg-node/sidereon.js";

import { f64Bits, fixture, fixtureText, hexToF64 } from "./helpers.mjs";

function assertClose(actual, expected, tol, label) {
  assert.ok(Math.abs(actual - expected) <= tol, `${label}: ${actual} vs ${expected}`);
}

function assertBits(actual, expectedHex, label) {
  assert.equal(f64Bits(actual), BigInt(expectedHex), label);
}

function assertArrayBitsEqual(actual, expected, label) {
  assert.equal(actual.length, expected.length, `${label}: length`);
  for (let i = 0; i < actual.length; i++) {
    assert.equal(f64Bits(actual[i]), f64Bits(expected[i]), `${label}[${i}]`);
  }
}

function geodTestRow() {
  return fixtureText("geodesic/geodtest_row.dat").trim().split(/\s+/).map(Number);
}

test("016 geodesic: vendored GeodTest row matches Karney WGS84 reference", () => {
  const [lat1, lon1, azi1, lat2, lon2, azi2, distanceM] = geodTestRow();

  const inverse = geodesicInverse(lat1, lon1, lat2, lon2);
  assertClose(inverse.distanceM, distanceM, 1e-6, "inverse distanceM");
  assertClose(inverse.initialAzimuthDeg, azi1, 5e-13, "inverse initialAzimuthDeg");
  assertClose(inverse.finalAzimuthDeg, azi2, 5e-13, "inverse finalAzimuthDeg");

  const direct = geodesicDirect(lat1, lon1, azi1, distanceM);
  assertClose(direct.lat2Deg, lat2, 5e-13, "direct lat2Deg");
  assertClose(direct.lon2Deg, lon2, 5e-13, "direct lon2Deg");
  assertClose(direct.finalAzimuthDeg, azi2, 5e-13, "direct finalAzimuthDeg");
});

test("016 frame catalog: ITRF2020 to ITRF2014 transform is pinned", () => {
  const positionM = [3657660.66, 255768.55, 5201382.11];
  const velocityMPerYear = [0.001, 0.002, 0.003];

  const entry = frameCatalogEntry(TerrestrialFrame.Itrf2020, TerrestrialFrame.Itrf2014);
  assert.ok(entry);
  assert.equal(entry.referenceEpochYear, 2015);
  assert.deepEqual(Array.from(entry.parametersAt(2020).translationMm), [-1.4, -1.4, 2.4]);
  assert.equal(entry.parametersAt(2020).scalePpb, -0.42);
  assert.deepEqual(Array.from(entry.parametersAt(2020).rotationMas), [0, 0, 0]);

  const propagated = frameCatalogPropagatePosition(positionM, velocityMPerYear, 2020, 2025);
  assert.deepEqual(Array.from(propagated), [3657660.665, 255768.56, 5201382.125]);

  const transformed = frameCatalogTransform(
    positionM,
    velocityMPerYear,
    TerrestrialFrame.Itrf2020,
    TerrestrialFrame.Itrf2014,
    2020,
  );
  ["0x414be7de541aaa81", "0x410f38c463501389", "0x4153d779870dc4f9"].forEach((bits, index) =>
    assertBits(transformed.positionM[index], bits, `positionM[${index}]`),
  );
  ["0x3f50624dd2f1a9fc", "0x3f5f212d77318fc5", "0x3f6a36e2eb1c432d"].forEach((bits, index) =>
    assertBits(transformed.velocityMPerYear[index], bits, `velocityMPerYear[${index}]`),
  );
});

test("016 geoid: EGM2008 crop fixture loads through the WASM grid entry", () => {
  const grid = GeoidGrid.fromEgm2008RasterWindow(
    fixture("geoid/egm2008_25_norcal_crop.bin"),
    Egm2008GridSpacing.TwoPointFiveMinute,
    37.0,
    -123.0,
    25,
    25,
  );

  const sanFrancisco = grid.undulationDeg(37.7749, -122.4194);
  assertBits(sanFrancisco, "0xc04014ef7b122a23", "San Francisco EGM2008 undulation");
  assertClose(sanFrancisco, -32.163558372373, 5e-14, "San Francisco oracle");
  assertBits(grid.undulationDeg(37.5, -122.75), "0xc040cd8cc0000000", "crop node 1");
  assertBits(grid.undulationDeg(37.875, -122.125), "0xc03fd8ed40000000", "crop node 2");
});

test("016 TDM: annex E KVN parses and re-encodes canonically", () => {
  const message = parseTdmKvn(fixtureText("tdm/annex_e_01.kvn"));
  assert.equal(message.version, "2.0");
  assert.equal(message.segmentCount, 1);
  const segment = message.segments[0];
  assert.deepEqual(
    segment.metadata.participants.map((participant) => [participant.index, participant.name]),
    [
      [1, "DSS-25"],
      [2, "yyyy-nnnA"],
    ],
  );
  assert.deepEqual(
    segment.metadata.paths.map((path) => [path.key, path.index, Array.from(path.participants)]),
    [["PATH", undefined, [2, 1]]],
  );
  assert.equal(segment.data.records.length, 31);
  const first = segment.data.records[0];
  assert.equal(first.keyword, "TRANSMIT_FREQ_2");
  assert.equal(first.observableKind, "transmitFreq");
  assert.equal(first.observableParticipant, 2);
  assert.equal(first.epoch, "2005-159T17:41:00");
  assert.equal(first.valueText, "32023442781.733");
  assert.equal(first.unit, "Hz");

  const encoded = message.toKvnString();
  assert.equal(encoded.length, 2217);
  assert.equal(parseTdmKvn(encoded).toKvnString(), encoded);
});

test("016 ECEF SP3 fit: two-epoch mini product reports unbounded covariance", () => {
  const sp3 = loadSp3(fixture("sp3/g02_ecef_two_epoch.sp3"));
  const report = fitSp3EcefPreciseOrbit(sp3, "G02", {
    forceModel: "two_body",
    integrator: "rk4",
    integratorOptions: { initialStepS: 60, maxStepS: 60 },
    solverOptions: { maxNfev: 200 },
    minLedgerSamples: 3,
  });

  assert.equal(report.fits.length, 1);
  assert.equal(report.fits[0].satellite, "G02");
  assert.equal(report.fits[0].covariance.kind, "unbounded");
  assert.equal(report.fits[0].geometryQuality.tier, "ZeroRedundancy");
  assert.equal(report.ledger.perSatellite[0].stats.n, 2);
  assert.equal(report.ledger.perSatellite[0].stats.lowSampleCount, true);
  // Re-pinned after the core parsed-epoch-axis hardening shifted this
  // synthetic micrometre-scale fit by parts in 1e9.
  assertClose(report.fits[0].fitRms3dM, 9.869725288539434e-7, 1e-18, "fit RMS");
});

test("016 force model: spherical-harmonic geopotential option propagates with pinned bits", () => {
  const ephemeris = propagateState({
    epochS: 0,
    positionKm: [7000, -1210, 1300],
    velocityKmS: [1, 7.2, 0.5],
    timesS: [0, 60, 120],
    integrator: "rk4",
    initialStepS: 30,
    maxStepS: 30,
    forceModel: {
      kind: "composite",
      twoBody: true,
      sphericalHarmonic: { model: "earth", maxDegree: 2, maxOrder: 0 },
      thirdBody: false,
      solarRadiationPressure: false,
      relativity: false,
    },
  });

  assert.equal(ephemeris.epochCount, 3);
  assertBits(ephemeris.positionKm[8], "0x40951791acc3f85b", "harmonic z at 120s");
  assertBits(ephemeris.velocityKmS[8], "0x3fd522a88dabbd12", "harmonic vz at 120s");
});

test("016 SGP4 decay latch: raw propagation stays stateless after latch trips", () => {
  const line1 = "1 28872U 05037B   05333.02012661  .25992681  00000-0  24476-3 0  1534";
  const line2 = "2 28872  96.4736 157.9986 0303955 244.0492 110.6523 16.46015938 10708";
  const tle = new Tle(line1, line2);
  const latch = new DecayLatch();
  const epoch = tleEpochUnixUs(5, 333.02012661);
  const tEarly = epoch + 1000n * 60n * 1000000n;
  const tDecay = epoch + 1440n * 60n * 1000000n;
  const tLater = epoch + 1450n * 60n * 1000000n;

  const rawEarly = tle.propagate(BigInt64Array.from([tEarly]));
  const rawLater = tle.propagate(BigInt64Array.from([tLater]));
  assert.throws(() => tle.propagate(BigInt64Array.from([tDecay])), Error);
  assert.throws(() => tle.propagateWithDecayLatch(BigInt64Array.from([tDecay]), latch), Error);
  assert.equal(latch.firstFailingEpochMinutes, 1440);
  assert.throws(() => tle.propagateWithDecayLatch(BigInt64Array.from([tLater]), latch), Error);

  const rawLaterAfterLatch = tle.propagate(BigInt64Array.from([tLater]));
  assertArrayBitsEqual(rawLaterAfterLatch.positionKm, rawLater.positionKm, "raw later position");
  assertArrayBitsEqual(rawLaterAfterLatch.velocityKmS, rawLater.velocityKmS, "raw later velocity");

  const latchedEarly = tle.propagateWithDecayLatch(BigInt64Array.from([tEarly]), latch);
  assertArrayBitsEqual(latchedEarly.positionKm, rawEarly.positionKm, "latched early position");
  assert.equal(latch.firstFailingEpochMinutes, 1440);
  latch.clear();
  assert.equal(latch.firstFailingEpochMinutes, undefined);
});

test("016 troposphere and eclipse smalls: typed errors and oblate shadow option", () => {
  assert.throws(() => tropoMappingFactors(1.0, 45.0, 0.0, 2451545.0, 0.0), RangeError);

  const sun = [149597870.7, 0, 0];
  const polarGrazing = [-7000, 0, 6370];
  const spherical = shadowFractionWithModel(polarGrazing, sun, EarthShadowModel.Spherical)[0];
  const oblate = shadowFractionWithModel(polarGrazing, sun, EarthShadowModel.Wgs84Oblate)[0];
  assert.equal(f64Bits(spherical), f64Bits(shadowFraction(polarGrazing, sun)[0]));
  assert.equal(f64Bits(spherical), f64Bits(hexToF64("0x3fe0a32f08e7fb1f")));
  assert.equal(f64Bits(oblate), f64Bits(hexToF64("0x3fc8757648272b98")));
  assert.ok(oblate < spherical);
});

function tleEpochUnixUs(year2, dayOfYear) {
  const year = year2 < 57 ? 2000 + year2 : 1900 + year2;
  const jan1 = BigInt(Date.UTC(year, 0, 1)) * 1000n;
  return jan1 + BigInt(Math.round((dayOfYear - 1) * 86400 * 1e6));
}
