// RINEX OBS parsing through the WASM binding, mirroring
// sidereon-python/tests/test_rinex_obs.py against the same committed fixtures.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  parseRinexObs,
  loadRinexObs,
  GnssSystem,
  TimeScale,
  ObservationKind,
  SignalPolicy,
  ObservationFilter,
  parseRinexNav,
  sppInputsFromRinexObs,
  solveSppFromRinexObs,
} from "../pkg-node/sidereon.js";

import { fixture, fixtureText } from "./helpers.mjs";

const ESBC = "obs/ESBC00DNK_R_20201770000_01D_30S_MO_trim.rnx";

const rowIndex = (series, sat, code) =>
  series.satellites.findIndex((s, i) => s === sat && series.codes[i] === code);

test("rinex obs header and epochs parse from the fixture", () => {
  const obs = parseRinexObs(fixture(ESBC));

  assert.equal(obs.epochCount, 2);
  assert.equal(obs.epochs.length, 2);

  const header = obs.header;
  assert.equal(header.version, 3.05);
  assert.equal(header.markerName, "ESBC00DNK");
  assert.equal(header.intervalS, 30.0);

  const approx = header.approxPositionM;
  const expectedApprox = [3582105.291, 532589.7313, 5232754.8054];
  for (let i = 0; i < 3; i++) assert.ok(Math.abs(approx[i] - expectedApprox[i]) < 1e-4);

  const hen = header.antennaDeltaHenM;
  assert.deepEqual(Array.from(hen), [0.216, 0.0, 0.0]);

  assert.ok(header.systems.includes(GnssSystem.Gps));
  assert.deepEqual(header.obsCodes(GnssSystem.Gps).slice(0, 5), [
    "C1C",
    "C1W",
    "C2L",
    "C2W",
    "C5Q",
  ]);
  assert.equal(header.obsCodes(GnssSystem.BeiDou)[0], "C2I");
  assert.ok(header.phaseShifts.length >= 20);

  assert.equal(header.timeOfFirstObsEpoch.year, 2020);
  assert.equal(header.timeOfFirstObsScale, TimeScale.Gpst);

  // GLONASS slot/channel pairs are flattened [slot0, chan0, slot1, chan1, ...].
  const slots = header.glonassSlots;
  let has11 = false;
  for (let i = 0; i < slots.length; i += 2) {
    if (slots[i] === 1 && slots[i + 1] === 1) has11 = true;
  }
  assert.ok(has11, "(1, 1) present in glonass slots");

  const epoch0 = obs.epoch(0);
  assert.equal(epoch0.flag, 0);
  assert.equal(epoch0.satelliteCount, 43);
  assert.equal(epoch0.epoch.second, 0.0);
  assert.ok(epoch0.satellites.includes("G05"));
  assert.equal(obs.epoch(1).epoch.second, 30.0);
});

test("pseudoranges are float64 series, exact to the fixture", () => {
  const obs = parseRinexObs(fixture(ESBC));
  const ranges = obs.pseudoranges(0);

  assert.ok(ranges.rangesM instanceof Float64Array);
  assert.equal(ranges.rangesM.length, 39);

  const bySat = new Map(ranges.satellites.map((s, i) => [s, ranges.rangesM[i]]));
  assert.equal(bySat.get("C05"), 40715949.461);
  assert.equal(bySat.get("E01"), 27616185.992);
  assert.equal(bySat.get("G05"), 20947300.931);
  assert.equal(bySat.get("R01"), 19307563.721);

  const gpsPolicy = new SignalPolicy().withSystem(GnssSystem.Gps, ["C1C"]);
  const gpsRanges = obs.pseudoranges(0, gpsPolicy);
  assert.ok(gpsRanges.satellites.every((s) => s.startsWith("G")));
  assert.equal(gpsRanges.length, 12);
});

test("raw values and carrier-phase rows are filtered float64 series", () => {
  const obs = parseRinexObs(fixture(ESBC));
  const filt = new ObservationFilter().withSystem(GnssSystem.Gps, ["C1C", "L1C"]);
  const rows = obs.observationValues(0, filt);

  assert.ok(rows.values instanceof Float64Array);
  assert.equal(rows.length, 24);

  const c = rowIndex(rows, "G05", "C1C");
  const l = rowIndex(rows, "G05", "L1C");
  assert.equal(rows.kinds[c], ObservationKind.Pseudorange);
  assert.equal(rows.kinds[l], ObservationKind.CarrierPhase);
  assert.equal(rows.values[c], 20947300.931);
  assert.equal(rows.values[l], 110078836.389);
  assert.equal(rows.ssi[c], 8.0);
  assert.ok(Number.isNaN(rows.lli[c]));

  const phase = obs.carrierPhaseRows(
    0,
    new ObservationFilter().withSystem(GnssSystem.Gps, ["L1C"]),
  );
  const p = rowIndex(phase, "G05", "L1C");
  assert.ok(phase.valueCycles instanceof Float64Array);
  assert.equal(phase.valueCycles[p], 110078836.389);
  assert.equal(phase.frequencyHz[p], 1575420000.0);
  assert.ok(Math.abs(phase.valueM[p] - phase.valueCycles[p] * phase.wavelengthM[p]) < 1e-9);
  assert.equal(phase.phaseShiftCycles[p], 0.0);
});

test("load accepts bytes and errors are typed", () => {
  const text = fixtureText(ESBC);
  assert.equal(loadRinexObs(Buffer.from(text, "utf8")).epochCount, 2);

  const navText =
    "     3.05           N: GNSS NAV DATA    M (MIXED)           RINEX VERSION / TYPE\n";
  assert.throws(() => parseRinexObs(Buffer.from(navText, "utf8")), Error);

  // A nav buffer is not an obs file; parsing it as obs throws, and an
  // out-of-range epoch is a RangeError.
  assert.throws(() => parseRinexObs(fixture(ESBC)).epoch(99), RangeError);
});

// Keep a reference to parseRinexNav so the import is exercised even though the
// obs surface does not need it; the nav suite covers it in depth.
test("parseRinexNav is exported", () => {
  assert.equal(typeof parseRinexNav, "function");
});

test("toRinexString re-parses to the same header and epochs", () => {
  const obs = parseRinexObs(fixture(ESBC));
  const text = obs.toRinexString();
  const reparsed = parseRinexObs(Buffer.from(text, "utf8"));
  assert.equal(reparsed.epochCount, obs.epochCount);
  assert.equal(reparsed.header.markerName, obs.header.markerName);
  // Deterministic: re-encoding the re-parsed product is byte-identical.
  assert.equal(reparsed.toRinexString(), text);
});

test("RINEX OBS convenience assembles and solves SPP through broadcast NAV", () => {
  const obs = parseRinexObs(fixture(ESBC));
  const nav = parseRinexNav(fixture("nav/ESBC00DNK_R_20201770000_01D_MN.rnx"));
  const rinexOptions = {
    corrections: { ionosphere: false, troposphere: false },
    signalPolicy: { G: ["C1C"], E: ["C1C"], C: ["C2I"], R: ["C1C"] },
  };

  const inputs = sppInputsFromRinexObs(nav, obs, rinexOptions);
  assert.equal(inputs.length, 2);
  assert.equal(inputs[0].epochIndex, 0);
  assert.equal(inputs[0].epoch.second, 0);
  assert.ok(inputs[0].observations.length >= 20);
  assert.ok(inputs[0].observations.some((row) => row.satelliteId === "G05"));

  const batch = solveSppFromRinexObs(nav, obs, rinexOptions, { withGeodetic: true });
  assert.equal(batch.count, 2);
  assert.equal(batch.epochIndex(0), 0);
  assert.equal(batch.isOk(0), true);
  const solution = batch.solution(0);
  assert.ok(solution.usedSats.length >= 4);
  assert.ok(Math.hypot(...solution.positionM) > 6.0e6);
  assert.equal(batch.error(0), undefined);
});
