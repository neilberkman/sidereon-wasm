// RINEX clock products through the WASM binding: series and interpolation on
// the committed synthetic fixture, and the lossless model (byte-for-byte
// restatement, typed header and record views, edits, the write policy and its
// departures) on the RINEX clock 3.04 specification examples.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  parseRinexClock,
  loadRinexClock,
  parseRinexClockLossy,
  loadRinexClockLossy,
  ClockEpoch,
  RinexClock,
  TimeScale,
} from "../pkg-node/sidereon.js";

import { coreGoldens, fixture, f64Bits } from "./helpers.mjs";

const CLK = "clk/synthetic_rinex_clock.clk";
const SPEC_304_A17 = "clk/lossless/rinex_clock304_table_a17.clk";
const SPEC_304_A18 = "clk/lossless/rinex_clock304_table_a18.clk";

const bytes = (text) => Buffer.from(text, "utf8");
const header = (payload, label) => payload.padEnd(60) + label;

function assertClockError(call, name, kind) {
  let detail;
  assert.throws(call, (err) => {
    assert.equal(err.name, name);
    assert.equal(err.detail.kind, kind);
    assert.equal(err.detail.message, err.message);
    detail = err.detail;
    return true;
  });
  return detail;
}

test("rinex clock series and interpolation parse bit-exact", () => {
  const clock = parseRinexClock(fixture(CLK));

  assert.deepEqual(clock.satellites, ["G05", "G24"]);
  assert.equal(clock.satelliteCount, 2);
  assert.equal(clock.sampleCount, 5);
  assert.equal(clock.timeScale, TimeScale.Gpst);

  const bySat = new Map(clock.series.map((s) => [s.satellite, s]));
  const g05 = bySat.get("G05");
  const g24 = clock.seriesFor("G24");
  assert.notEqual(g24, undefined);

  assert.ok(g05.gpsSeconds instanceof Float64Array);
  assert.equal(g05.gpsSeconds.length, 3);
  assert.deepEqual(Array.from(g05.hasGpsSeconds), [1, 1, 1]);
  assert.equal(g05.biasS.length, 3);
  assert.equal(g05.length, 3);
  assert.equal(g05.timeScale, TimeScale.Gpst);
  assert.equal(g24.length, 2);
  // The G05 records are at 00:00:00, 00:00:30 and 00:01:00 GPS time; each
  // sample's time is the one the same civil label gives, to the bit.
  assert.deepEqual(
    Array.from(g05.gpsSeconds, f64Bits),
    [0, 30, 60].map((s) =>
      f64Bits(new ClockEpoch(2026, 5, 13, 0, Math.floor(s / 60), s % 60).gpsSeconds),
    ),
  );
  assert.equal(f64Bits(g05.biasS[1]), 0xbf2a36e36f0d4275n);
  // Each G05 record declares two values; the second is its bias sigma.
  assert.deepEqual(g05.additionalValues, [[4.0e-11], [2.0e-11], [2.0e-11]]);
  assert.deepEqual(g24.additionalValues, [[], []]);
  const epochs = g05.epochs;
  assert.equal(epochs.length, 3);
  assert.equal(epochs[1].scale, "GPST");
  assert.equal(epochs[1].jdWhole, g05.jdWhole[1]);
  assert.equal(epochs[1].jdFraction, g05.jdFraction[1]);
  assert.equal(epochs[1].nanos, null);
  assert.equal(epochs[1].gpsSeconds, g05.gpsSeconds[1]);

  const epoch = new ClockEpoch(2026, 5, 13, 0, 0, 30.0);
  assert.equal(epoch.gpsSeconds, g05.gpsSeconds[1]);
  assert.equal(f64Bits(clock.clockS("G05", epoch)), 0xbf2a36e36f0d4275n);

  const g24Exact = new ClockEpoch(2026, 5, 13, 0, 0, 0.0);
  const g24Mid = new ClockEpoch(2026, 5, 13, 0, 0, 15.0);
  assert.equal(f64Bits(clock.clockS("G24", g24Exact)), 0x3f0a36e2eb1c432dn);
  assert.equal(f64Bits(clock.clockSAtGpsSeconds("G24", g24Mid.gpsSeconds)), 0x3f0a36e4a2ea40can);
  assert.equal(clock.clockS("G99", epoch), undefined);
  assert.equal(clock.clockS("G05", new ClockEpoch(2026, 5, 13, 1, 0, 0.0)), undefined);

  assert.throws(() => clock.clockSAtGpsSeconds("G05", NaN), RangeError);
});

test("the receiver record is kept, reported as skipped and read", () => {
  const clock = parseRinexClock(fixture(CLK));
  assert.equal(clock.recordCount, 6);
  assert.deepEqual(clock.skippedRecords, [{ line: 4, recordType: "AR" }]);
  const records = clock.records();
  assert.equal(records.length, 6);
  const ar = records[0];
  assert.equal(ar.index, 0);
  assert.equal(ar.recordType, "AR");
  assert.equal(ar.name, "ONSA");
  assert.equal(ar.satellite, null);
  assert.deepEqual(ar.values, [1.0e-6, 0.0]);
  assert.equal(ar.line, 4);
  assert.equal(ar.lineCount, 1);
  assert.deepEqual(ar.sourceLines, [clock.sourceLine(4)]);
  assert.equal(records[1].satellite, "G05");
  assert.deepEqual(records[1].civilEpoch, {
    year: 2026,
    month: 5,
    day: 13,
    hour: 0,
    minute: 0,
    second: 0,
  });
  assert.equal(clock.sourceLine(0), undefined);
  assert.equal(clock.sourceLine(100), undefined);
});

test("trailing-text records preserve their notice counts and column layout", () => {
  const source = fixture(CLK).toString("utf8");
  const lines = source.split("\n");
  const index = lines.findIndex((line) => line.startsWith("AS "));
  assert.notEqual(index, -1);
  lines[index] = lines[index].padEnd(80) + " trailing";

  const clock = parseRinexClock(bytes(lines.join("\n")));
  const notice = clock.notices.find((entry) => entry.kind === "TRAILING_TEXT_RECORDS");
  assert.equal(notice.records, 1);
  assert.equal(notice.firstLine, index + 1);
  assert.match(notice.message, /text after their last column/i);
  const record = clock.records().find((entry) => entry.line === index + 1);
  assert.equal(record.reading, "columnsTrailingText");
  assert.deepEqual(record.readingDetail, {
    kind: "columnsTrailingText",
    layout: "v300",
  });
});

test("load accepts bytes", () => {
  assert.equal(loadRinexClock(fixture(CLK)).sampleCount, 5);
  assert.deepEqual(loadRinexClock(fixture(CLK)).satellites, ["G05", "G24"]);
});

test("strict parse errors are typed and the lossy read keeps the bad line", () => {
  const shortAs = "AS G05  2026 05 13 00 00  0.000000  1\n";
  assertClockError(
    () => parseRinexClock(bytes(shortAs)),
    "RinexClockParseError",
    "MALFORMED_AS_RECORD",
  );

  const text =
    "AS G05  2026 05 13 00 00  0.000000  1   1.0e-04\n" +
    "AS G06  2026 05 13 00 00  bad-second  1   2.0e-04\n";
  assert.throws(
    () => parseRinexClock(bytes(text)),
    (err) => err.name === "RinexClockParseError" && err.detail.line === 2,
  );

  const lossy = parseRinexClockLossy(bytes(text));
  assert.deepEqual(lossy.satellites, ["G05"]);
  assert.equal(
    f64Bits(lossy.clockS("G05", new ClockEpoch(2026, 5, 13, 0, 0, 0.0))),
    f64Bits(1.0e-4),
  );
  const diagnostics = lossy.diagnostics;
  assert.equal(diagnostics.length, 1);
  assert.equal(diagnostics[0].line, 2);
  assert.equal(typeof diagnostics[0].error.kind, "string");
  assert.equal(diagnostics[0].error.line, 2);
  // Nothing is dropped: the unread line is written back as read.
  assert.equal(lossy.toRinexString(), text);
  assert.equal(loadRinexClockLossy(bytes(shortAs)).sampleCount, 0);
});

test("toRinexString restates the source byte for byte", () => {
  const source = fixture(CLK).toString("utf8");
  const clock = parseRinexClock(fixture(CLK));
  assert.equal(clock.toRinexString(), source);
  const written = clock.toRinexStringWithPolicy();
  assert.deepEqual(Object.keys(written).sort(), ["departures", "text", "value"]);
  assert.equal(written.text, source);
  assert.equal(written.value, source);
  assert.deepEqual(written.departures, []);
});

test("records repeated for one satellite and epoch all remain", () => {
  const text =
    "AS G05  2026 05 13 00 00  0.000000  1   1.0e-04\n" +
    "AS G05  2026 05 13 00 00  0.000000  1   2.0e-04\n";
  const clock = parseRinexClock(bytes(text));
  assert.equal(clock.seriesFor("G05").length, 1);
  assert.equal(
    f64Bits(clock.clockS("G05", new ClockEpoch(2026, 5, 13, 0, 0, 0.0))),
    f64Bits(2.0e-4),
  );
  assert.deepEqual(
    clock.records().map((r) => r.values[0]),
    [1.0e-4, 2.0e-4],
  );
  assert.equal(clock.toRinexString(), text);
});

test("seven-digit seconds are read as stated, not carried into the next minute", () => {
  const text = "AS G05  2026 05 13 00 00 59.9999996  1   1.0e-04\n";
  const clock = parseRinexClock(bytes(text));
  const nextMinute = new ClockEpoch(2026, 5, 13, 0, 1, 0.0).gpsSeconds;
  const sample = clock.seriesFor("G05").gpsSeconds[0];
  // 2026-05-13 00:00:59.9999996 GPST is exactly 1462665659.9999996 s after the
  // GPS epoch. The doubles either side are 1462665659.99999952316... and
  // 1462665659.99999976158...; the first is nearer, so the correctly rounded
  // value has the bits 0x41d5cba06efffffe. The record's time is that value.
  assert.equal(f64Bits(sample), 0x41d5cba06efffffen);
  assert.ok(sample < nextMinute);
  // The same civil label built directly gives the same correctly rounded value.
  const stated = new ClockEpoch(2026, 5, 13, 0, 0, 59.9999996);
  assert.equal(f64Bits(stated.gpsSeconds), 0x41d5cba06efffffen);
  // The exact seconds text stays with the record.
  assert.match(clock.records()[0].sourceLines[0], /59\.9999996/);
  assert.equal(clock.toRinexString(), text);
  assert.ok(stated.gpsSeconds < nextMinute);
  assert.equal(f64Bits(clock.clockS("G05", stated)), f64Bits(1.0e-4));
});

const UTC_TEXT = [
  header("     3.00           CLOCK DATA          GPS", "RINEX VERSION / TYPE"),
  header("   UTC", "TIME SYSTEM ID"),
  header("", "END OF HEADER"),
  "AS G05  2016 12 31 23 59 59.000000  1    0.100000000000E-03",
  "AS G05  2016 12 31 23 59 60.000000  1    0.200000000000E-03",
  "AS G05  2017 01 01 00 00  0.000000  1    0.300000000000E-03",
  "",
].join("\n");

test("a UTC product answers at a leap-second label and keeps samples off the GPS timeline", () => {
  const clock = parseRinexClock(bytes(UTC_TEXT));
  assert.equal(clock.timeSystem, "UTC");
  assert.equal(clock.timeScale, TimeScale.Utc);
  assert.deepEqual(clock.timeSystemStatus, { kind: "DECLARED" });

  const g05 = clock.seriesFor("G05");
  assert.equal(g05.length, 3);
  assert.equal(g05.timeScale, TimeScale.Utc);
  assert.deepEqual(Array.from(g05.hasGpsSeconds), [0, 0, 0]);
  assert.ok(Array.from(g05.gpsSeconds).every(Number.isNaN));
  assert.ok(g05.epochs.every((e) => e.scale === "UTC" && e.gpsSeconds === null));

  // 23:59:60 names an epoch in UTC on a day that ends with a leap second; it
  // has no GPS-time reading.
  const leap = new ClockEpoch(2016, 12, 31, 23, 59, 60.0);
  assert.equal(leap.gpsSeconds, undefined);
  assert.equal(clock.clockS("G05", leap), 2.0e-4);
  // Interpolation across the leap second uses elapsed time.
  // The value is the engine's own for the same product and label, reproduced
  // natively by test/golden-gen.
  const inside = clock.clockS("G05", new ClockEpoch(2016, 12, 31, 23, 59, 60.5));
  assert.equal(f64Bits(inside), BigInt(coreGoldens().clockLeapInterp.biasAt60p5));
  // No such label on an ordinary day, in any scale.
  assert.throws(() => new ClockEpoch(2016, 12, 30, 23, 59, 60.0), RangeError);
  assert.equal(clock.toRinexString(), UTC_TEXT);

  // GLO epochs are UTC in every version.
  const glo = parseRinexClock(bytes(UTC_TEXT.replace("   UTC", "   GLO")));
  assert.equal(glo.timeSystem, "GLO");
  assert.equal(glo.timeScale, TimeScale.Utc);
  assert.equal(glo.clockS("G05", leap), 2.0e-4);

  // A GPS product has no 23:59:60, and says so by name.
  const gps = parseRinexClock(fixture(CLK));
  assertClockError(
    () => gps.clockS("G05", new ClockEpoch(2016, 12, 31, 23, 59, 60.0)),
    "RinexClockQueryError",
    "INVALID_INPUT",
  );
});

test("the 3.04 example reads every header record at its version's columns", () => {
  const source = fixture(SPEC_304_A17).toString("utf8");
  assert.ok(source.includes("\r\n"));
  const clock = parseRinexClock(fixture(SPEC_304_A17));
  assert.equal(clock.toRinexString(), source);
  assert.equal(clock.version, 3.04);
  assert.equal(clock.layout, "v304");
  assert.equal(clock.satelliteSystem, "G");
  assert.equal(clock.timeSystem, "GPS");
  assert.equal(clock.timeScale, TimeScale.Gpst);
  // The example prints four records at the 3.00 columns against its own table.
  assert.deepEqual(
    clock.notices.map(({ kind, line }) => ({ kind, line })),
    [9, 10, 13, 15].map((line) => ({ kind: "HEADER_RECORD_NONCONFORMING", line })),
  );
  assert.ok(clock.notices.every((notice) => typeof notice.message === "string"));

  const records = clock.headerRecords();
  assert.ok(records.every((record) => record.labelColumn === 65));
  assert.equal(records[0].line, 1);
  assert.equal(records[0].text, source.split("\r\n")[0]);
  assert.equal(records[12].reading, "otherVersionColumns");
  assert.deepEqual(records[12].field, {
    kind: "CLOCK_REF_COUNT",
    count: 1,
    start: { year: 1994, month: 7, day: 14, hour: 0, minute: 0, second: 0 },
    stop: { year: 1994, month: 7, day: 14, hour: 20, minute: 59, second: 0 },
  });
  const field = (label, prefix) =>
    records.find((r) => r.label === label && r.payload.startsWith(prefix)).field;
  assert.deepEqual(field("SOLN STA NAME / NUM", "GOLD"), {
    kind: "SOLUTION_STATION",
    name: "GOLD",
    identifier: "40405S031",
    xyzMm: ["1234567890", "-1234567890", "-1234567890"],
    xyzMmNumber: [1234567890, -1234567890, -1234567890],
  });
  assert.deepEqual(field("ANALYSIS CLK REF", "USNO"), {
    kind: "ANALYSIS_CLOCK_REF",
    name: "USNO",
    identifier: "40451S003",
    constraintS: -0.123456789012,
  });
  assert.deepEqual(field("SYS / # / OBS TYPES", "G"), {
    kind: "OBSERVATION_TYPES",
    system: "G",
    count: 4,
    descriptors: ["C1W", "L1W", "C2W", "L2W"],
  });
  assert.deepEqual(field("LEAP SECONDS", ""), {
    kind: "LEAP_SECONDS",
    seconds: "10",
    secondsNumber: 10,
  });
  assert.equal(field("PRN LIST", "G01").prns.length, 16);

  // Pin every field of every typed header record emitted by this real
  // specification example, including repeated records and both PRN chunks.
  assert.deepEqual(
    records.map(({ field }) => field),
    [
      { kind: "VERSION_TYPE", version: 3.04, fileType: "C", satelliteSystem: "G" },
      {
        kind: "PROGRAM_RUN_BY_DATE",
        program: "TORINEXC V9.9",
        runBy: "USNO",
        date: "19960403  001000 UTC",
      },
      { kind: "COMMENT", text: "EXAMPLE OF A CLOCK DATA ANALYSIS FILE" },
      { kind: "COMMENT", text: "IN THIS CASE ANALYSIS RESULTS FROM GPS ONLY ARE INCLUDED" },
      { kind: "COMMENT", text: "No re-alignment of the clocks has been applied." },
      {
        kind: "OBSERVATION_TYPES",
        system: "G",
        count: 4,
        descriptors: ["C1W", "L1W", "C2W", "L2W"],
      },
      { kind: "TIME_SYSTEM", label: "GPS" },
      { kind: "LEAP_SECONDS", seconds: "10", secondsNumber: 10 },
      {
        kind: "DCBS_APPLIED",
        system: "G",
        program: "CC2NONCC",
        source: "p1c1bias.hist @ goby.nrl.navy.mil",
      },
      {
        kind: "PCVS_APPLIED",
        system: "G",
        program: "PAGES",
        source: "igs05.atx @ igscb.jpl.nasa.gov",
      },
      { kind: "TYPES_OF_DATA", count: 2, types: ["AS", "AR"] },
      { kind: "ANALYSIS_CENTER", designator: "USN", name: "USNO USING GIPSY/OASIS-II" },
      {
        kind: "CLOCK_REF_COUNT",
        count: 1,
        start: { year: 1994, month: 7, day: 14, hour: 0, minute: 0, second: 0 },
        stop: { year: 1994, month: 7, day: 14, hour: 20, minute: 59, second: 0 },
      },
      {
        kind: "ANALYSIS_CLOCK_REF",
        name: "USNO",
        identifier: "40451S003",
        constraintS: -0.123456789012,
      },
      {
        kind: "CLOCK_REF_COUNT",
        count: 1,
        start: { year: 1994, month: 7, day: 14, hour: 21, minute: 0, second: 0 },
        stop: { year: 1994, month: 7, day: 14, hour: 21, minute: 59, second: 0 },
      },
      {
        kind: "ANALYSIS_CLOCK_REF",
        name: "TIDB",
        identifier: "50103M108",
        constraintS: -0.123456789012,
      },
      { kind: "SOLUTION_STATION_COUNT", count: 4, frame: "ITRF96" },
      {
        kind: "SOLUTION_STATION",
        name: "GOLD",
        identifier: "40405S031",
        xyzMm: ["1234567890", "-1234567890", "-1234567890"],
        xyzMmNumber: [1234567890, -1234567890, -1234567890],
      },
      {
        kind: "SOLUTION_STATION",
        name: "AREQ",
        identifier: "42202M005",
        xyzMm: ["-1234567890", "1234567890", "-1234567890"],
        xyzMmNumber: [-1234567890, 1234567890, -1234567890],
      },
      {
        kind: "SOLUTION_STATION",
        name: "TIDB",
        identifier: "50103M108",
        xyzMm: ["1234567890", "-1234567890", "1234567890"],
        xyzMmNumber: [1234567890, -1234567890, 1234567890],
      },
      {
        kind: "SOLUTION_STATION",
        name: "HARK",
        identifier: "30302M007",
        xyzMm: ["-1234567890", "1234567890", "-1234567890"],
        xyzMmNumber: [-1234567890, 1234567890, -1234567890],
      },
      {
        kind: "SOLUTION_STATION",
        name: "USNO",
        identifier: "40451S003",
        xyzMm: ["1234567890", "-1234567890", "-1234567890"],
        xyzMmNumber: [1234567890, -1234567890, -1234567890],
      },
      { kind: "SOLUTION_SATELLITE_COUNT", count: 27 },
      {
        kind: "PRN_LIST",
        prns: [
          "G01",
          "G02",
          "G03",
          "G04",
          "G05",
          "G06",
          "G07",
          "G08",
          "G09",
          "G10",
          "G13",
          "G14",
          "G15",
          "G16",
          "G17",
          "G18",
        ],
      },
      {
        kind: "PRN_LIST",
        prns: ["G19", "G21", "G22", "G23", "G24", "G25", "G26", "G27", "G29", "G30", "G31"],
      },
      { kind: "END_OF_HEADER" },
    ],
  );
  assert.equal(new Set(records.map(({ field }) => field.kind)).size, 17);

  const areq = clock.records().find((r) => r.name === "AREQ00USA");
  assert.equal(areq.recordType, "AR");
  assert.equal(areq.reading, "columnsV304");
  assert.equal(areq.continuationReading, "columnsV304");
  assert.equal(areq.lineCount, 2);
  assert.equal(areq.declaredCount, 6);
  assert.equal(areq.sourceLines.length, 2);
  assert.equal(areq.epoch.scale, "GPST");

  const g16 = clock.seriesFor("G16");
  assert.equal(g16.length, 1);
  assert.deepEqual(Array.from(g16.biasS), [-0.123456789012]);
  assert.deepEqual(g16.additionalValues, [[-0.0123456789012]]);
});

test("a 3.04 file without TIME SYSTEM ID takes the default and can declare it", () => {
  const source = fixture(SPEC_304_A18).toString("utf8");
  const clock = parseRinexClock(fixture(SPEC_304_A18));
  assert.equal(clock.toRinexString(), source);
  assert.equal(clock.timeSystem, "GPS");
  assert.deepEqual(clock.timeSystemStatus, { kind: "DEFAULTED" });
  assert.equal(clock.timeScale, TimeScale.Gpst);
  assert.deepEqual(
    clock.notices.map((notice) => ({ ...notice, message: undefined })),
    [
      { kind: "HEADER_RECORD_NONCONFORMING", line: 7, message: undefined },
      { kind: "TIME_SYSTEM_DEFAULTED", system: "GPS", message: undefined },
      { kind: "TIME_SYSTEM_MISSING", message: undefined },
    ],
  );
  assert.deepEqual(clock.skippedRecords, [
    { line: 10, recordType: "CR" },
    { line: 11, recordType: "CR" },
    { line: 12, recordType: "DR" },
    { line: 13, recordType: "CR" },
  ]);
  assert.deepEqual(clock.series, []);
  const records = clock.records();
  assert.equal(records.length, 4);
  const dr = records[2];
  assert.equal(dr.recordType, "DR");
  assert.equal(dr.civilEpoch.second, 14.5);
  assert.deepEqual(dr.values, [-1.23456789012, 0.123456789012]);
  assert.deepEqual(dr.sourceLines, [source.split("\r\n")[11]]);
  const drGps = new ClockEpoch(1995, 7, 14, 22, 23, 14.5).gpsSeconds;
  assert.equal(f64Bits(dr.epoch.gpsSeconds), f64Bits(drGps));

  // Declaring the time system writes one record at the 3.04 columns before
  // LEAP SECONDS GNSS, with the file's CRLF; every other line is unchanged.
  clock.setTimeSystem("GPS");
  const tsid = `${"   GPS".padEnd(65)}TIME SYSTEM ID\r\n`;
  let split = 0;
  for (let i = 0; i < 4; i++) split = source.indexOf("\r\n", split) + 2;
  const expected = source.slice(0, split) + tsid + source.slice(split);
  assert.equal(clock.toRinexString(), expected);
  assert.deepEqual(clock.timeSystemStatus, { kind: "DECLARED" });
  const inserted = clock.headerRecords()[4];
  assert.equal(inserted.line, null);
  assert.equal(inserted.reading, "columns");
  assert.deepEqual(inserted.field, { kind: "TIME_SYSTEM", label: "GPS" });

  assert.throws(() => clock.setTimeSystem("XYZ"), TypeError);
});

test("the 3.04 clock header also exposes the remaining typed fields and continuation values", () => {
  const clock = parseRinexClock(fixture(SPEC_304_A18));
  const fields = clock.headerRecords().map(({ field }) => field);
  assert.deepEqual(fields, [
    { kind: "VERSION_TYPE", version: 3.04, fileType: "C", satelliteSystem: "" },
    {
      kind: "PROGRAM_RUN_BY_DATE",
      program: "TORINEXC V9.9",
      runBy: "USNO",
      date: "19960403  001000 UTC",
    },
    { kind: "COMMENT", text: "EXAMPLE OF A CLOCK DATA FILE" },
    { kind: "COMMENT", text: "IN THIS CASE CALIBRATION/DISCONTINUITY DATA GIVEN" },
    { kind: "LEAP_SECONDS_GNSS", seconds: "10", secondsNumber: 10 },
    { kind: "TYPES_OF_DATA", count: 2, types: ["CR", "DR"] },
    { kind: "STATION_NAME_NUM", name: "USNO", identifier: "40451S003" },
    { kind: "STATION_CLOCK_REF", text: "UTC(USNO) MASTER CLOCK VIA CONTINUOUS CABLE MONITOR" },
    { kind: "END_OF_HEADER" },
  ]);
  assert.equal(new Set(fields.map(({ kind }) => kind)).size, 8);
  const a17Fields = parseRinexClock(fixture(SPEC_304_A17))
    .headerRecords()
    .map(({ field }) => field);
  const variantFields = new Map([...a17Fields, ...fields].map((field) => [field.kind, field]));
  assert.equal(variantFields.size, 20);
  assert.equal(
    [...variantFields.values()].reduce((count, field) => count + Object.keys(field).length - 1, 0),
    42,
  );

  const source = fixture(SPEC_304_A17).toString("utf8");
  const lines = source.split("\r\n");
  const observationTypes = lines.findIndex((line) => line.endsWith("SYS / # / OBS TYPES"));
  assert.notEqual(observationTypes, -1);
  lines[observationTypes] = header("G    5  C1W L1W C2W L2W", "SYS / # / OBS TYPES");
  lines.splice(observationTypes + 1, 0, header("        L5Q", "SYS / # / OBS TYPES"));
  const continued = parseRinexClock(bytes(lines.join("\r\n")));
  assert.deepEqual(
    continued
      .headerRecords()
      .filter(({ label }) => label === "SYS / # / OBS TYPES")
      .map(({ field }) => field),
    [
      {
        kind: "OBSERVATION_TYPES",
        system: "G",
        count: 5,
        descriptors: ["C1W", "L1W", "C2W", "L2W"],
      },
      { kind: "OBSERVATION_TYPES", system: null, count: null, descriptors: ["L5Q"] },
    ],
  );
});

test("record edits validate the whole change and change nothing when refused", () => {
  const source = fixture(SPEC_304_A18).toString("utf8");
  const lines = source.split("\r\n");

  // Removing a record removes the lines it spans and nothing else.
  const removed = parseRinexClock(fixture(SPEC_304_A18));
  const record = removed.removeRecord(0);
  assert.equal(record.recordType, "CR");
  assert.equal(record.line, 10);
  assert.equal(removed.recordCount, 3);
  assert.equal(removed.toRinexString(), [...lines.slice(0, 9), ...lines.slice(10)].join("\r\n"));

  // An inserted record is written in the product's layout and reads back.
  const inserted = parseRinexClock(fixture(SPEC_304_A18));
  inserted.insertRecord(inserted.recordCount, {
    recordType: "CR",
    name: "USNO",
    epoch: new ClockEpoch(1995, 7, 14, 23, 50, 0.0),
    values: [0.5, 0.25],
  });
  const last = inserted.records()[4];
  assert.equal(last.line, null);
  assert.equal(last.reading, "edited");
  assert.deepEqual(last.sourceLines, []);
  const reread = parseRinexClock(bytes(inserted.toRinexString())).records()[4];
  assert.equal(reread.recordType, "CR");
  assert.equal(reread.name, "USNO");
  assert.deepEqual(reread.values, [0.5, 0.25]);
  assert.deepEqual(reread.civilEpoch, {
    year: 1995,
    month: 7,
    day: 14,
    hour: 23,
    minute: 50,
    second: 0,
  });

  const clock = parseRinexClock(fixture(SPEC_304_A18));
  const before = clock.toRinexString();
  assertClockError(
    () =>
      clock.insertRecord(0, {
        recordType: "CR",
        name: "TENCHARSXX",
        epoch: { year: 1995, month: 7, day: 14, hour: 0, minute: 0, second: 0 },
        values: [1.0],
      }),
    "RinexClockEditError",
    "INVALID_INPUT",
  );
  assertClockError(
    () => clock.setRecordValues(1, [Number.NaN]),
    "RinexClockEditError",
    "INVALID_INPUT",
  );
  assertClockError(() => clock.removeRecord(4), "RinexClockEditError", "INVALID_INPUT");
  assert.throws(() => clock.insertRecord(0, { recordType: "XX", name: "USNO" }), TypeError);
  assert.throws(() => clock.setRecordValues(-1, [1.0]), RangeError);
  assert.equal(clock.toRinexString(), before);

  // A batch edit is checked whole: one refused value refuses them all.
  assertClockError(
    () => clock.editRecords((r) => (r.index === 3 ? [Number.NaN] : [r.values[0], r.values[1]])),
    "RinexClockEditError",
    "INVALID_INPUT",
  );
  assert.throws(
    () =>
      clock.editRecords(() => {
        throw new Error("stop");
      }),
    /stop/,
  );
  assert.throws(
    () =>
      clock.retainRecords(() => {
        throw new Error("stop");
      }),
    /stop/,
  );
  assert.equal(clock.toRinexString(), before);

  // Batches that pass apply in one step.
  assert.equal(
    clock.retainRecords((r) => r.recordType !== "DR"),
    1,
  );
  assert.deepEqual(
    clock.records().map((r) => r.recordType),
    ["CR", "CR", "CR"],
  );
  assert.equal(
    clock.editRecords((r) => (r.index === 0 ? [0.5, r.values[1]] : undefined)),
    1,
  );
  assert.deepEqual(clock.records()[0].values, [0.5, -0.0123456789012]);
  assert.equal(parseRinexClock(bytes(clock.toRinexString())).records()[0].values[0], 0.5);
});

test("values beyond a record's declared count are kept and must be restated", () => {
  const text = [
    header("     2.00           CLOCK DATA", "RINEX VERSION / TYPE"),
    header("   GPS", "TIME SYSTEM ID"),
    header("     1    AS", "# / TYPES OF DATA"),
    header("", "END OF HEADER"),
    "AS G01  2026 09 17 00 00  0.000000  1    0.170710878415E-03  5.556437046250E-12",
    "AS G01  2026 09 17 00 00 30.000000  1    0.170720878415E-03  5.556437046250E-12",
    "",
  ].join("\n");
  const clock = parseRinexClock(bytes(text));
  const first = clock.records()[0];
  assert.equal(first.declaredCount, 1);
  assert.deepEqual(first.values, [0.170710878415e-3]);
  assert.deepEqual(first.surplusValues, [{ position: 1, value: 5.55643704625e-12 }]);
  assert.deepEqual(clock.seriesFor("G01").additionalValues, [[], []]);
  const surplus = clock.notices.find((notice) => notice.kind === "SURPLUS_VALUES");
  assert.equal(surplus.records, 2);
  assert.equal(surplus.firstLine, 5);

  // Dropping the surplus sigma is refused; restating it is an edit.
  const refused = assertClockError(
    () => clock.setRecordValues(0, [0.170710878415e-3]),
    "RinexClockEditError",
    "INVALID_INPUT",
  );
  assert.equal(refused.field, "values");
  assert.equal(clock.toRinexString(), text);
  clock.setRecordValues(0, Float64Array.of(0.170710878415e-3, 5.55643704625e-12));
  const edited = clock.records()[0];
  assert.equal(edited.declaredCount, 2);
  assert.deepEqual(edited.surplusValues, []);
  assert.equal(edited.reading, "edited");
  const reread = parseRinexClock(bytes(clock.toRinexString())).records()[0];
  assert.deepEqual(reread.values, [0.170710878415e-3, 5.55643704625e-12]);
});

test("products built from points or GPS-second rows state their header", () => {
  const source = parseRinexClock(fixture(CLK));
  const g05 = source.seriesFor("G05");
  const rebuilt = RinexClock.fromSeriesRows([
    { satellite: "G05", gpsSeconds: g05.gpsSeconds, biasS: g05.biasS },
  ]);
  assert.deepEqual(rebuilt.timeSystemStatus, { kind: "CONSTRUCTED" });
  assert.equal(rebuilt.timeScale, TimeScale.Gpst);
  assert.deepEqual(rebuilt.headerRecords(), []);
  const reread = parseRinexClock(bytes(rebuilt.toRinexString()));
  assert.equal(reread.timeSystem, "GPS");
  assert.deepEqual(
    Array.from(reread.seriesFor("G05").gpsSeconds, f64Bits),
    Array.from(g05.gpsSeconds, f64Bits),
  );
  assert.deepEqual(
    Array.from(reread.seriesFor("G05").biasS, f64Bits),
    Array.from(g05.biasS, f64Bits),
  );
  assertClockError(
    () =>
      RinexClock.fromSeriesRows([
        { satellite: "G05", gpsSeconds: [30.0, 0.0], biasS: [1.0e-4, 2.0e-4] },
      ]),
    "RinexClockBuildError",
    "INVALID_INPUT",
  );
  assert.throws(
    () => RinexClock.fromSeriesRows([{ satellite: "G05", gpsSeconds: [0.0], biasS: [] }]),
    TypeError,
  );

  // A UTC product built from points keeps its leap-second epoch.
  const utc = RinexClock.fromClockPoints(TimeScale.Utc, [
    {
      satellite: "G05",
      points: [
        {
          epoch: new ClockEpoch(2016, 12, 31, 23, 59, 60.0),
          biasS: 2.0e-4,
          additionalValues: [1.0e-11],
        },
        { epoch: { year: 2017, month: 1, day: 1, hour: 0, minute: 0, second: 0 }, biasS: 3.0e-4 },
      ],
    },
  ]);
  const utcText = utc.toRinexString();
  const utcBack = parseRinexClock(bytes(utcText));
  assert.equal(utcBack.timeSystem, "UTC");
  assert.equal(utcBack.clockS("G05", new ClockEpoch(2016, 12, 31, 23, 59, 60.0)), 2.0e-4);
  assert.deepEqual(utcBack.seriesFor("G05").additionalValues, [[1.0e-11], []]);

  // GLONASS system time is UTC(SU) + 3 h; GLO names UTC hours, so no RINEX
  // clock time system states it.
  const glonasst = RinexClock.fromClockPoints(TimeScale.Glonasst, [
    { satellite: "R01", points: [{ epoch: new ClockEpoch(2020, 1, 1, 0, 0, 0), biasS: 1.0e-4 }] },
  ]);
  const unsupported = assertClockError(
    () => glonasst.toRinexString(),
    "RinexClockWriteError",
    "UNSUPPORTED_TIME_SCALE",
  );
  assert.equal(unsupported.scale, "GLONASST");
});

test("an epoch off the microsecond grid is refused, or written at the nearest one under the policy", () => {
  const clock = RinexClock.fromClockPoints(TimeScale.Gpst, [
    {
      satellite: "G05",
      points: [
        {
          epoch: { year: 2026, month: 5, day: 13, hour: 0, minute: 0, second: 1e-7 },
          biasS: 1.0e-4,
        },
      ],
    },
  ]);
  assertClockError(() => clock.toRinexString(), "RinexClockWriteError", "INVALID_INPUT");
  assertClockError(
    () => clock.toRinexStringWithPolicy("strict"),
    "RinexClockWriteError",
    "INVALID_INPUT",
  );

  for (const policy of ["lenient", { nearestMicrosecondEpochs: "allow" }]) {
    const written = clock.toRinexStringWithPolicy(policy);
    assert.equal(written.text, written.value);
    assert.equal(written.departures.length, 1);
    const [departure] = written.departures;
    assert.equal(departure.kind, "EPOCH_AT_NEAREST_MICROSECOND");
    assert.equal(departure.record, 0);
    assert.equal(departure.name, "G05");
    assert.equal(departure.epoch.scale, "GPST");
    assert.equal(typeof departure.written, "string");
    assert.equal(typeof departure.message, "string");
    const back = parseRinexClock(bytes(written.text));
    assert.equal(back.seriesFor("G05").biasS[0], 1.0e-4);
  }

  assert.throws(() => clock.toRinexStringWithPolicy("loose"), TypeError);
  assert.throws(() => clock.toRinexStringWithPolicy({ nearestMicrosecond: "allow" }), TypeError);
  assert.throws(
    () => clock.toRinexStringWithPolicy({ nearestMicrosecondEpochs: "yes" }),
    TypeError,
  );
});
