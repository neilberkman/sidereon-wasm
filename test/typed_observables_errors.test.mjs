// Typed observables and velocity errors through the WASM binding.
// Tests scalar velocity refusal and success, scalar observables prediction
// invalid input and no ephemeris, mixed batch success/refusal, typed per-row
// detail getters with owned plain copied data, retention across batch.free(),
// and RangeError enforcement on all bad index forms.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  CarrierBand,
  carrierFrequencyHz,
  caCode,
  civilToJ2000Seconds,
  dopplerToRangeRate,
  gamma,
  GnssSystem,
  loadRinexNav,
  loadSp3,
  observablesBroadcast,
  observablesSp3,
  predictBatchBroadcast,
  predictBatchSp3,
  replica,
  solveVelocity,
  solveVelocityBroadcast,
  phaseMeters,
} from "../pkg-node/sidereon.js";

import { fixture, hexToF64 } from "./helpers.mjs";

const SP3_PATH = "sp3/GBM0MGXRAP_20201770000_01D_05M_ORB_120epoch.sp3";
const NAV_PATH = "nav/ESBC00DNK_R_20201770000_01D_MN.rnx";

const fL1 = carrierFrequencyHz(GnssSystem.Gps, CarrierBand.L1);
const RECEIVER = [3582105.291, 532589.731, 5232754.805];

function g01NavReferenceEpoch(nav) {
  const record = nav.records.find((candidate) => candidate.satellite === "G01");
  assert.ok(record, "the NAV fixture has a supported G01 broadcast record");
  const secondsOfDay = record.clock.tocSow % 86400;
  const hour = Math.floor(secondsOfDay / 3600);
  const minute = Math.floor((secondsOfDay - hour * 3600) / 60);
  const second = secondsOfDay - hour * 3600 - minute * 60;
  assert.equal(hour, 4, "the selected record is the fixture's 04:00 G01 record");
  return civilToJ2000Seconds(2020, 6, 25, hour, minute, second);
}

test("scalar solveVelocity refuses insufficient observations with typed TOO_FEW_SATELLITES", () => {
  const sp3 = loadSp3(fixture(SP3_PATH));
  const epochs = sp3.epochsJ2000Seconds();
  const tRx = epochs[Math.floor(epochs.length / 2)];

  // Only 2 satellite observations (4 required)
  const fewObservations = [
    { satelliteId: "G01", value: 1200.0, carrierHz: fL1 },
    { satelliteId: "G02", value: -850.0, carrierHz: fL1 },
  ];

  // Confirm that both rows have usable source states at this interior epoch;
  // the typed count below must describe rows actually built by the core.
  const predictions = fewObservations.map((observation) =>
    observablesSp3(sp3, observation.satelliteId, RECEIVER, tRx, undefined),
  );
  assert.equal(predictions.length, 2);
  assert.ok(predictions.every((prediction) => prediction.geometricRangeM > 0));

  let caught;
  try {
    solveVelocity(sp3, fewObservations, Float64Array.from(RECEIVER), tRx, undefined);
  } catch (e) {
    caught = e;
  }

  assert.ok(caught instanceof Error);
  assert.equal(caught.name, "Error");
  assert.ok(caught.detail !== undefined, "detail is attached");
  assert.equal(caught.detail.kind, "TOO_FEW_SATELLITES");
  assert.equal(caught.detail.used, 2);
  assert.equal(caught.detail.required, 4);
  assert.match(caught.detail.message, /too few satellites/i);
  assert.deepEqual(caught.cause, caught.detail);

  let duplicate;
  try {
    solveVelocity(
      sp3,
      [
        { satelliteId: "G01", value: 10.0, carrierHz: fL1 },
        { satelliteId: "G01", value: 11.0, carrierHz: fL1 },
      ],
      Float64Array.from(RECEIVER),
      tRx,
      undefined,
    );
  } catch (error) {
    duplicate = error;
  }
  assert.ok(duplicate instanceof Error);
  assert.equal(duplicate.detail.kind, "DUPLICATE_OBSERVATION");
  assert.equal(duplicate.detail.satelliteId, "G01");
});

test("scalar observable RangeErrors retain exact domain variant details", () => {
  assert.ok(Number.isFinite(gamma(2.0, 1.0)));
  assert.throws(
    () => gamma(1.0, 1.0),
    (error) => {
      assert.ok(error instanceof RangeError);
      assert.equal(error.message, "equal carrier frequencies");
      assert.deepEqual(error.detail, {
        family: "IonosphereFreeError",
        kind: "equal_frequencies",
        message: error.message,
      });
      return true;
    },
  );

  assert.ok(Number.isFinite(phaseMeters(1.0, 1.57542e9)));
  assert.throws(
    () => phaseMeters(1.0, 0.0),
    (error) => {
      assert.ok(error instanceof RangeError);
      assert.equal(error.detail.family, "CarrierPhaseError");
      assert.equal(error.detail.kind, "invalid_frequency");
      assert.equal(error.detail.message, error.message);
      return true;
    },
  );

  assert.equal(caCode(1n).length, 1023);
  assert.throws(
    () => caCode(9_007_199_254_740_993n),
    (error) => {
      assert.ok(error instanceof RangeError);
      assert.equal(error.detail.family, "SignalError");
      assert.equal(error.detail.kind, "unsupported_prn");
      assert.equal(error.detail.prn, "9007199254740993");
      assert.equal(error.detail.message, error.message);
      return true;
    },
  );

  assert.ok(Number.isFinite(dopplerToRangeRate(10.0, 1.57542e9)));
  assert.throws(
    () => dopplerToRangeRate(10.0, 0.0),
    (error) => {
      assert.ok(error instanceof RangeError);
      assert.equal(error.detail.family, "VelocityError");
      assert.equal(error.detail.kind, "INVALID_INPUT");
      assert.equal(error.detail.field, "carrier_hz");
      assert.equal(error.detail.reason, "not positive");
      assert.equal(error.detail.message, error.message);
      return true;
    },
  );
  assert.throws(
    () =>
      replica(1n, { sampleRateHz: 0.0, numSamples: 4, codePhaseChips: 0.0, codeDopplerHz: 0.0 }),
    (error) => {
      assert.ok(error instanceof RangeError);
      assert.deepEqual(error.detail, {
        family: "SignalError",
        kind: "invalid_input",
        message: error.message,
        field: "sample_rate_hz",
        reason: "not positive",
      });
      return true;
    },
  );
});

test("scalar solveVelocityBroadcast refuses insufficient observations with typed TOO_FEW_SATELLITES", () => {
  const nav = loadRinexNav(fixture(NAV_PATH));
  const tRx = g01NavReferenceEpoch(nav);

  const singleObservation = [{ satelliteId: "G01", value: 500.0, carrierHz: fL1 }];
  const prediction = observablesBroadcast(nav, "G01", RECEIVER, tRx, undefined);
  assert.ok(prediction.geometricRangeM > 0, "the broadcast record predicts this interior epoch");

  let caught;
  try {
    solveVelocityBroadcast(nav, singleObservation, Float64Array.from(RECEIVER), tRx, undefined);
  } catch (e) {
    caught = e;
  }

  assert.ok(caught instanceof Error);
  assert.equal(caught.name, "Error");
  assert.ok(caught.detail !== undefined);
  assert.equal(caught.detail.kind, "TOO_FEW_SATELLITES");
  assert.equal(caught.detail.used, 1);
  assert.equal(caught.detail.required, 4);
  assert.deepEqual(caught.cause, caught.detail);
});

test("scalar solveVelocity succeeds on valid multi-satellite observations", () => {
  const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const observations = [
    { satelliteId: "G01", value: hexToF64("0x408785E0E0A3D70A"), carrierHz: fL1 },
    { satelliteId: "G03", value: hexToF64("0xC090AC416978D4FE"), carrierHz: fL1 },
    { satelliteId: "G09", value: hexToF64("0x409559F0A3D70A3D"), carrierHz: fL1 },
    { satelliteId: "G11", value: hexToF64("0x40954AE147AE147B"), carrierHz: fL1 },
  ];
  const receiver = Float64Array.from([4500000.0, 500000.0, 4500000.0]);
  const epochs = sp3.epochsJ2000Seconds();
  const tRx = epochs[Math.floor(epochs.length / 2)];

  // Establish usable SP3 predictions at this interior epoch before checking
  // the solver's used-satellite count; supplied rows can be skipped when the
  // source cannot predict them.
  const predictions = observations.map((observation) =>
    observablesSp3(sp3, observation.satelliteId, receiver, tRx, undefined),
  );
  assert.equal(predictions.length, observations.length);
  assert.ok(predictions.every((prediction) => prediction.geometricRangeM > 0));

  const solution = solveVelocity(sp3, observations, receiver, tRx, undefined);
  assert.equal(solution.velocityMS.length, 3);
  assert.ok(Array.from(solution.velocityMS).every(Number.isFinite));
  assert.ok(solution.usedSats.length >= 4);
  assert.ok(Number.isFinite(solution.speedMS));
  assert.ok(Number.isFinite(solution.clockDriftSS));
});

test("scalar observablesSp3 and observablesBroadcast report typed INVALID_INPUT and NO_EPHEMERIS", () => {
  const sp3 = loadSp3(fixture(SP3_PATH));
  const nav = loadRinexNav(fixture(NAV_PATH));
  const tRx = sp3.epochsJ2000Seconds()[0];

  // Non-finite receiver position: INVALID_INPUT
  let caughtSp3Invalid;
  try {
    observablesSp3(sp3, "G01", [Number.NaN, 0, 0], tRx, undefined);
  } catch (e) {
    caughtSp3Invalid = e;
  }
  assert.ok(caughtSp3Invalid instanceof Error);
  assert.equal(caughtSp3Invalid.name, "Error");
  assert.equal(caughtSp3Invalid.detail.kind, "INVALID_INPUT");
  assert.equal(caughtSp3Invalid.detail.field, "receiver_ecef_m");
  assert.equal(caughtSp3Invalid.detail.reason, "not finite");
  assert.deepEqual(caughtSp3Invalid.cause, caughtSp3Invalid.detail);

  let caughtSp3Missing;
  try {
    observablesSp3(sp3, "G99", Float64Array.from(RECEIVER), tRx, undefined);
  } catch (e) {
    caughtSp3Missing = e;
  }
  assert.ok(caughtSp3Missing instanceof Error);
  assert.equal(caughtSp3Missing.detail.kind, "EPHEMERIS");
  assert.equal(caughtSp3Missing.detail.cause.kind, "UNKNOWN_SATELLITE");
  assert.equal(caughtSp3Missing.detail.cause.satelliteId, "G99");
  assert.deepEqual(caughtSp3Missing.cause, caughtSp3Missing.detail);

  // Missing satellite in broadcast ephemeris: NO_EPHEMERIS
  let caughtNavNoEph;
  try {
    observablesBroadcast(nav, "G99", Float64Array.from(RECEIVER), tRx, undefined);
  } catch (e) {
    caughtNavNoEph = e;
  }
  assert.ok(caughtNavNoEph instanceof Error);
  assert.equal(caughtNavNoEph.name, "Error");
  assert.equal(caughtNavNoEph.detail.kind, "NO_EPHEMERIS");
  assert.deepEqual(caughtNavNoEph.cause, caughtNavNoEph.detail);
});

test("mixed batch prediction preserves row order, reports per-row typed errorDetail and throws typed cause on observables()", () => {
  const sp3 = loadSp3(fixture(SP3_PATH));
  const epochsAvailable = sp3.epochsJ2000Seconds();
  const tRx = epochsAvailable[Math.floor(epochsAvailable.length / 2)];

  // The successful row is independently predicted at this interior epoch.
  const control = observablesSp3(sp3, "G01", RECEIVER, tRx, undefined);
  assert.ok(control.geometricRangeM > 0);

  // Request 0 is valid ("G01"), Request 1 is missing ("G99")
  const sats = ["G01", "G99"];
  const receivers = Float64Array.from([...RECEIVER, ...RECEIVER]);
  const epochs = Float64Array.from([tRx, tRx]);

  const batch = predictBatchSp3(sp3, sats, receivers, epochs, undefined);
  assert.equal(batch.count, 2);

  // Row 0 succeeds
  assert.equal(batch.isOk(0), true);
  assert.equal(batch.error(0), undefined);
  assert.equal(batch.errorDetail(0), undefined);
  assert.equal(batch.detail(0), undefined);
  const obs0 = batch.observables(0);
  assert.ok(obs0.geometricRangeM > 0);
  assert.ok(Number.isFinite(obs0.dopplerHz));

  // Row 1 fails
  assert.equal(batch.isOk(1), false);
  const errText = batch.error(1);
  assert.equal(typeof errText, "string");
  assert.ok(errText.length > 0);

  const detail1 = batch.errorDetail(1);
  assert.equal(typeof detail1, "object");
  assert.ok(detail1 !== null);
  assert.equal(detail1.kind, "EPHEMERIS");
  assert.equal(detail1.cause.kind, "UNKNOWN_SATELLITE");
  assert.equal(detail1.cause.satelliteId, "G99");
  assert.equal(detail1.message, errText);

  // detail alias returns equivalent plain object
  assert.deepEqual(batch.detail(1), detail1);

  // Calling observables(1) throws Error carrying that row's message, .detail and .cause
  let thrown;
  try {
    batch.observables(1);
  } catch (e) {
    thrown = e;
  }
  assert.ok(thrown instanceof Error);
  assert.equal(thrown.message, errText);
  assert.deepEqual(thrown.detail, detail1);
  assert.deepEqual(thrown.cause, detail1);
});

test("typed per-row errorDetail is owned plain data that survives batch.free() and subsequent calls", () => {
  const nav = loadRinexNav(fixture(NAV_PATH));
  const tRx = g01NavReferenceEpoch(nav);

  const control = observablesBroadcast(nav, "G01", RECEIVER, tRx, undefined);
  assert.ok(control.geometricRangeM > 0, "the selected G01 NAV record predicts its toc");

  const sats = ["G01", "G99"];
  const receivers = Float64Array.from([...RECEIVER, ...RECEIVER]);
  const epochs = Float64Array.from([tRx, tRx]);

  const batch = predictBatchBroadcast(nav, sats, receivers, epochs, undefined);
  assert.equal(batch.count, 2);
  assert.equal(batch.isOk(0), true);
  assert.equal(batch.errorDetail(0), undefined);
  assert.ok(batch.observables(0).geometricRangeM > 0);
  assert.equal(batch.isOk(1), false);

  // Extract owned plain detail object
  const detail = batch.errorDetail(1);
  assert.equal(detail.kind, "NO_EPHEMERIS");
  assert.equal(typeof detail.message, "string");
  const savedDetail = structuredClone(detail);

  // Free the underlying batch
  batch.free();

  // Subsequent unrelated operations
  const freq = carrierFrequencyHz(GnssSystem.Gps, CarrierBand.L1);
  assert.ok(freq > 1e9);

  // The complete owned object retains its contents after free and later calls.
  assert.deepEqual(detail, savedDetail);
});

test("all PredictBatch index methods enforce error::index_arg RangeError on bad indices", () => {
  const sp3 = loadSp3(fixture(SP3_PATH));
  const tRx = sp3.epochsJ2000Seconds()[0];

  const sats = ["G01", "G99"];
  const receivers = Float64Array.from([...RECEIVER, ...RECEIVER]);
  const epochs = Float64Array.from([tRx, tRx]);

  const batch = predictBatchSp3(sp3, sats, receivers, epochs, undefined);
  assert.equal(batch.count, 2);

  const badIndices = [
    -1,
    1.5,
    Number.NaN,
    Number.POSITIVE_INFINITY,
    Number.NEGATIVE_INFINITY,
    2,
    100,
  ];

  for (const idx of badIndices) {
    assert.throws(
      () => batch.isOk(idx),
      (err) => err instanceof RangeError,
      `isOk(${idx}) must throw RangeError`,
    );
    assert.throws(
      () => batch.observables(idx),
      (err) => err instanceof RangeError,
      `observables(${idx}) must throw RangeError`,
    );
    assert.throws(
      () => batch.error(idx),
      (err) => err instanceof RangeError,
      `error(${idx}) must throw RangeError`,
    );
    assert.throws(
      () => batch.errorDetail(idx),
      (err) => err instanceof RangeError,
      `errorDetail(${idx}) must throw RangeError`,
    );
    assert.throws(
      () => batch.detail(idx),
      (err) => err instanceof RangeError,
      `detail(${idx}) must throw RangeError`,
    );
  }

  // Valid indices 0 and 1 succeed
  assert.equal(typeof batch.isOk(0), "boolean");
  assert.equal(typeof batch.isOk(1), "boolean");
});
