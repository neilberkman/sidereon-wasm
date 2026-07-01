// Sample-backed precise-ephemeris source and geometry-only batch range
// prediction, over sidereon_core::sp3::PreciseEphemerisSamples and
// sidereon_core::observables::predict_ranges.
//
// A source rebuilt from the samples extracted from a parsed SP3 product must
// interpolate states and predict ranges that match the SP3-parsed source. The
// core states the round-trip contract as byte-identical for samples that are the
// faithful SI image of the fit nodes, with a documented <= 1 ULP (a few
// nanometre) caveat from the non-injective km -> m reconstruction. The tolerances
// below sit comfortably above that few-nanometre floor (observed here: ~1.5e-8 m
// on position/range, exact transmit time, ~3e-21 s on clock).

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  loadSp3,
  sp3PreciseEphemerisSamples,
  preciseEphemerisSamplesFromSamples,
} from "../pkg-node/sidereon.js";
import { fixture } from "./helpers.mjs";

// Documented round-trip tolerance (see the header): meters for range/position,
// seconds for clock and transmit time.
const POS_TOL_M = 1e-6;
const CLOCK_TOL_S = 1e-15;
const TIME_TOL_S = 1e-6;

const setup = () => loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));

const RECEIVERS = [
  [4_027_894.0, 307_046.0, 4_919_474.0],
  [1_130_000.0, -4_830_000.0, 3_994_000.0],
];

// Interior query grid: nodes and midpoints, away from the coverage edges.
function queryGrid(sp3) {
  const epochs = sp3.epochsJ2000Seconds();
  const qs = [];
  for (let i = 3; i < epochs.length - 3; i += 8) {
    qs.push(epochs[i]);
    qs.push(0.5 * (epochs[i] + epochs[i + 1]));
  }
  return qs;
}

function gpsSats(sp3) {
  return sp3.satellites.filter((s) => s.startsWith("G")).slice(0, 3);
}

function assertPredictionEqual(a, b, msg) {
  assert.ok(
    Math.abs(a.geometricRangeM - b.geometricRangeM) <= POS_TOL_M,
    `${msg}: geometricRangeM ${a.geometricRangeM} vs ${b.geometricRangeM}`,
  );
  assert.ok(
    Math.abs(a.transmitTimeJ2000S - b.transmitTimeJ2000S) <= TIME_TOL_S,
    `${msg}: transmitTimeJ2000S ${a.transmitTimeJ2000S} vs ${b.transmitTimeJ2000S}`,
  );
  if (a.satClockS == null || b.satClockS == null) {
    assert.equal(a.satClockS, b.satClockS, `${msg}: satClockS null-ness`);
  } else {
    assert.ok(
      Math.abs(a.satClockS - b.satClockS) <= CLOCK_TOL_S,
      `${msg}: satClockS ${a.satClockS} vs ${b.satClockS}`,
    );
  }
  for (let k = 0; k < 3; k++) {
    assert.ok(
      Math.abs(a.satPosEcefM[k] - b.satPosEcefM[k]) <= POS_TOL_M,
      `${msg}: satPosEcefM[${k}] ${a.satPosEcefM[k]} vs ${b.satPosEcefM[k]}`,
    );
  }
}

test("sp3PreciseEphemerisSamples extracts the canonical samples", () => {
  const sp3 = setup();
  const samples = sp3PreciseEphemerisSamples(sp3);
  assert.ok(samples.length > 0);
  const s = samples[0];
  assert.equal(typeof s.sat, "string");
  assert.equal(typeof s.epoch, "number");
  assert.equal(s.positionEcefM.length, 3);
  assert.equal(typeof s.clockEvent, "boolean");
  // A source built from the extraction interpolates the same satellite set.
  const src = preciseEphemerisSamplesFromSamples(samples);
  assert.deepEqual([...src.satellites].sort(), [...sp3.satellites].sort());
});

test("predictRanges over a sample source matches the SP3-parsed source", () => {
  const sp3 = setup();
  const src = preciseEphemerisSamplesFromSamples(sp3PreciseEphemerisSamples(sp3));

  const qs = queryGrid(sp3);
  const sats = gpsSats(sp3);
  assert.ok(qs.length > 0 && sats.length > 0);

  for (const options of [undefined, { lightTime: false, sagnac: false }, { sagnac: true }]) {
    const requests = [];
    for (const q of qs)
      for (const rx of RECEIVERS)
        for (const sat of sats) requests.push({ sat, receiverEcefM: rx, tRxJ2000S: q });

    const fromSp3 = sp3.predictRanges(requests, options);
    const fromSamples = src.predictRanges(requests, options);
    assert.equal(fromSamples.length, requests.length);
    for (let i = 0; i < requests.length; i++) {
      assertPredictionEqual(
        fromSp3[i],
        fromSamples[i],
        `options=${JSON.stringify(options)} request ${i}`,
      );
    }
  }
});

test("sample-source interpolated states match Sp3.interpolate", () => {
  const sp3 = setup();
  const src = preciseEphemerisSamplesFromSamples(sp3PreciseEphemerisSamples(sp3));
  const qs = queryGrid(sp3);

  for (const sat of gpsSats(sp3)) {
    // Range prediction with light-time and Sagnac off exposes the raw
    // interpolated state at the query epoch.
    const requests = qs.map((q) => ({ sat, receiverEcefM: RECEIVERS[0], tRxJ2000S: q }));
    const predicted = src.predictRanges(requests, { lightTime: false, sagnac: false });
    const interp = sp3.interpolate(sat, Float64Array.from(qs));
    for (let i = 0; i < qs.length; i++) {
      for (let k = 0; k < 3; k++) {
        assert.ok(
          Math.abs(predicted[i].satPosEcefM[k] - interp.positionM[i * 3 + k]) <= POS_TOL_M,
          `${sat} q${i} pos[${k}]`,
        );
      }
      assert.ok(
        Math.abs(predicted[i].satClockS - interp.clockS[i]) <= CLOCK_TOL_S,
        `${sat} q${i} clock`,
      );
    }
  }
});

test("predictRanges batch equals per-request calls", () => {
  const sp3 = setup();
  const src = preciseEphemerisSamplesFromSamples(sp3PreciseEphemerisSamples(sp3));
  const qs = queryGrid(sp3);
  const sats = gpsSats(sp3);

  const requests = [];
  for (const q of qs)
    for (const sat of sats) requests.push({ sat, receiverEcefM: RECEIVERS[0], tRxJ2000S: q });

  const batch = src.predictRanges(requests, undefined);
  assert.equal(batch.length, requests.length);
  requests.forEach((req, i) => {
    const single = src.predictRanges([req], undefined);
    assert.equal(single.length, 1);
    // The batch is amortization of the call boundary only, so a single-request
    // call is bit-identical to the batch element.
    assert.equal(single[0].geometricRangeM, batch[i].geometricRangeM);
    assert.equal(single[0].transmitTimeJ2000S, batch[i].transmitTimeJ2000S);
    assert.equal(single[0].satClockS, batch[i].satClockS);
    assert.deepEqual(single[0].satPosEcefM, batch[i].satPosEcefM);
  });
});

test("preciseEphemerisSamplesFromSamples throws on a validation failure", () => {
  const sp3 = setup();
  const samples = sp3PreciseEphemerisSamples(sp3);
  // A satellite with a single sample cannot be interpolated: the source builder
  // must reject it (RangeError).
  const oneG01 = samples.filter((s) => s.sat === "G01").slice(0, 1);
  assert.throws(() => preciseEphemerisSamplesFromSamples(oneG01), RangeError);
  // Empty input is rejected too.
  assert.throws(() => preciseEphemerisSamplesFromSamples([]), RangeError);
});

test("predictRanges rejects a non-finite receiver", () => {
  const sp3 = setup();
  const q = sp3.epochsJ2000Seconds()[4];
  assert.throws(
    () => sp3.predictRanges([{ sat: "G01", receiverEcefM: [NaN, 0, 0], tRxJ2000S: q }], undefined),
    RangeError,
  );
});
