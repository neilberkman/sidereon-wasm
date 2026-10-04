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
  assert.deepEqual(
    obs.obsCodes(GnssSystem.Gps),
    header.obsCodes(GnssSystem.Gps),
    "the owner-level obsCodes route exposes the complete header code list",
  );
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

  assert.equal(epoch0.declaredRecordCount, 43);
  assert.deepEqual(epoch0.specialRecords, []);
  assert.deepEqual(epoch0.cycleSlips, []);
  assert.equal(epoch0.observations.length, 43);
  assert.equal(obs.skippedRecords, 0);
  assert.deepEqual(header.declaredObsCodes(GnssSystem.Gps), header.obsCodes(GnssSystem.Gps));
  assert.deepEqual(header.rinex2Types, []);
  assert.equal(header.rinex2System, undefined);
});

test("OBS header DTO preserves the complete literal public projection", () => {
  const esbc = parseRinexObs(fixture(ESBC)).header;
  assert.equal(esbc.version, 3.05);
  assert.deepEqual(Array.from(esbc.approxPositionM), [3582105.291, 532589.7313, 5232754.8054]);
  assert.deepEqual(Array.from(esbc.antennaDeltaHenM), [0.216, 0, 0]);
  assert.deepEqual(esbc.systems, [0, 1, 2, 3, 4, 6]);
  assert.deepEqual(esbc.obsCodes(GnssSystem.Gps), [
    "C1C",
    "C1W",
    "C2L",
    "C2W",
    "C5Q",
    "D1C",
    "D2L",
    "D2W",
    "D5Q",
    "L1C",
    "L2L",
    "L2W",
    "L5Q",
    "S1C",
    "S1W",
    "S2L",
    "S2W",
    "S5Q",
  ]);
  assert.deepEqual(esbc.declaredObsCodes(GnssSystem.Gps), esbc.obsCodes(GnssSystem.Gps));
  assert.deepEqual(esbc.rinex2Types, []);
  assert.equal(esbc.rinex2System, undefined);
  assert.deepEqual(esbc.programRunByDate, {
    program: "sbf2rin-13.4.5",
    runBy: "",
    date: "20220706 130812 UTC",
  });
  assert.deepEqual(esbc.comments, [
    "gfzrnx-1.16-8177    FILE MERGE          20220706 132211 UTC",
    "INITIAL_RINEX_VERSION: 3.04",
    "SEPTENTRIO RECEIVERS OUTPUT ALIGNED CARRIER PHASES.",
    "NO FURTHER PHASE SHIFT APPLIED IN THE RINEX ENCODER.",
    "GFZRNX.NUM_EPOCHS: 0",
  ]);
  assert.equal(esbc.markerNumber, "10118M001");
  assert.equal(esbc.markerType, "GEODETIC");
  assert.equal(esbc.markerName, "ESBC00DNK");
  assert.equal(esbc.observer, "SDFE");
  assert.equal(esbc.agency, "SDFE");
  assert.deepEqual(esbc.receiver, {
    number: "3047937",
    receiverType: "SEPT POLARX5",
    version: "5.2.0",
  });
  assert.deepEqual(esbc.antenna, { number: "CR5200327016", antennaType: "ASH701945E_M    SCIS" });
  assert.equal(esbc.intervalS, 30);
  assert.deepEqual(
    [
      esbc.timeOfFirstObsEpoch.year,
      esbc.timeOfFirstObsEpoch.month,
      esbc.timeOfFirstObsEpoch.day,
      esbc.timeOfFirstObsEpoch.hour,
      esbc.timeOfFirstObsEpoch.minute,
      esbc.timeOfFirstObsEpoch.second,
      esbc.timeOfFirstObsScale,
    ],
    [2020, 6, 25, 0, 0, 0, TimeScale.Gpst],
  );
  assert.deepEqual(
    [
      esbc.timeOfLastObsEpoch.year,
      esbc.timeOfLastObsEpoch.month,
      esbc.timeOfLastObsEpoch.day,
      esbc.timeOfLastObsEpoch.hour,
      esbc.timeOfLastObsEpoch.minute,
      esbc.timeOfLastObsEpoch.second,
      esbc.timeOfLastObsScale,
    ],
    [2020, 6, 25, 23, 59, 30, TimeScale.Gpst],
  );
  assert.equal(esbc.declaredSatelliteCount, 0);
  assert.deepEqual(esbc.prnObsCounts, []);
  assert.equal(esbc.phaseShifts.length, 22);
  assert.equal(esbc.phaseShifts[0].system, GnssSystem.BeiDou);
  assert.equal(esbc.phaseShifts[0].code, "L2I");
  assert.deepEqual(esbc.phaseShifts[0].satellites, []);
  assert.deepEqual(esbc.phaseShifts[0].unrepresentableSatellites, []);
  assert.equal(esbc.phaseShifts[0].coversEverySatellite, true);
  assert.equal(esbc.phaseShifts[0].satelliteCount, 0);
  assert.deepEqual(esbc.scaleFactors, []);
  const scaledText = fixtureText(ESBC).replace(
    "                                                            END OF HEADER",
    "G   10   1 L1W".padEnd(60) +
      "SYS / SCALE FACTOR\n                                                            END OF HEADER",
  );
  assert.deepEqual(parseRinexObs(Buffer.from(scaledText, "utf8")).header.scaleFactors, [
    { system: "G", factor: 10, codes: ["L1W"] },
  ]);
  assert.deepEqual(
    Array.from(esbc.glonassSlots),
    [
      1, 1, 2, -4, 3, 5, 4, 6, 5, 1, 6, -4, 7, 5, 8, 6, 9, -2, 10, -7, 11, 0, 12, -1, 13, -2, 14,
      -7, 15, 0, 16, -1, 17, 4, 18, -3, 19, 3, 20, 2, 21, 4, 23, 3, 24, 2,
    ],
  );
  assert.equal(esbc.glonassCodPhsBis, null);
  assert.equal(esbc.signalStrengthUnit, "DBHZ");
  assert.equal(esbc.leapSeconds, undefined);
  assert.deepEqual(esbc.unretainedHeaderLabels, []);
  const wtzr = parseRinexObs(fixture("obs/WTZR00DEU_R_20201770000_01D_30S_MO_120epoch.rnx")).header;
  assert.deepEqual(Array.from(wtzr.approxPositionM), [4075580.8863, 931853.5784, 4801567.9707]);
  assert.equal(wtzr.declaredSatelliteCount, 111);
  assert.equal(wtzr.prnObsCounts.length, 111);
  assert.deepEqual(wtzr.prnObsCounts[0], {
    satellite: "G01",
    counts: [921, 920, 917, 935, 921, 920, 917, 935, 915, 920, 914, 935, 921, 920, 917, 935],
  });
  assert.equal(wtzr.phaseShifts.length, 3);
  assert.equal(wtzr.phaseShifts[0].system, GnssSystem.Gps);
  assert.equal(wtzr.phaseShifts[0].code, "L2S");
  assert.equal(wtzr.phaseShifts[0].correctionCycles, -0.25);
  assert.equal(wtzr.phaseShifts[0].coversEverySatellite, true);
  assert.deepEqual(wtzr.glonassCodPhsBis, [
    { code: "C1C", biasM: -71.94 },
    { code: "C1P", biasM: -71.94 },
    { code: "C2C", biasM: -71.94 },
    { code: "C2P", biasM: -71.94 },
  ]);
  assert.deepEqual(wtzr.glonassCodePhaseBias("C1C"), { status: "available", biasM: -71.94 });
  assert.deepEqual(
    [
      wtzr.leapSeconds.current,
      wtzr.leapSeconds.deltaFuture,
      wtzr.leapSeconds.week,
      wtzr.leapSeconds.day,
      wtzr.leapSeconds.timeSystem,
    ],
    [18n, 18n, 1929n, 7n, undefined],
  );
  assert.deepEqual(wtzr.unretainedHeaderLabels, ["RCV CLOCK OFFS APPL"]);
  const rinex2 = parseRinexObs(fixture("obs/algo0010_2015001_v1_trim.rnx")).header;
  assert.equal(rinex2.version, 2.11);
  assert.deepEqual(rinex2.rinex2Types, ["L1", "L2", "C1", "C2", "P2", "P1", "S1", "S2"]);
  assert.equal(rinex2.rinex2System, undefined);
  assert.equal(rinex2.markerName, "ALGO CACS-GSD 883160 Algonquin Park ON Canada");
  assert.deepEqual([rinex2.leapSeconds.current, rinex2.leapSeconds.deltaFuture], [16n, undefined]);
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
  assert.equal(phase.phaseShiftAvailable[p], 1);
  assert.equal(phase.phaseShiftStatus[p], "available");
  assert.deepEqual(phase.phaseShiftCorrections[p], { status: "available", cycles: 0 });
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
    qzssClock: "separate",
    troposphereModel: "saastamoinenNiell",
  };

  const inputs = sppInputsFromRinexObs(nav, obs, rinexOptions);
  assert.equal(inputs.length, 2);
  assert.equal(inputs[0].epochIndex, 0);
  assert.equal(inputs[0].epoch.second, 0);
  assert.ok(inputs[0].observations.length >= 20);
  assert.ok(inputs[0].observations.some((row) => row.satelliteId === "G05"));
  assert.equal(inputs[0].qzssClock, "separate");
  assert.equal(inputs[0].troposphereModel, "saastamoinenNiell");

  const batch = solveSppFromRinexObs(nav, obs, rinexOptions, { withGeodetic: true });
  assert.equal(batch.count, 2);
  assert.equal(batch.epochIndex(0), 0);
  assert.equal(batch.isOk(0), true);
  const solution = batch.solution(0);
  assert.ok(solution.usedSats.length >= 4);
  assert.ok(Math.hypot(...solution.positionM) > 6.0e6);
  assert.equal(batch.error(0), undefined);
});
