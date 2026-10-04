// RINEX observation contracts the WASM binding carries from the core: phase
// shift statuses with every row kept, the header in effect at each epoch,
// untimed events and cycle slips, raw leap-second and GLONASS bias records,
// the strict writer's typed refusals and the version 2 downgrade's changes.
// Each text is laid out as the core's own tests lay it out.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  GnssSystem,
  parseRinexObs,
  repairRinexObs,
  rinexObsCycleSlipFlag,
} from "../pkg-node/sidereon.js";

import { fixture } from "./helpers.mjs";

const encoder = new TextEncoder();
const ESBC = "obs/ESBC00DNK_R_20201770000_01D_30S_MO_trim.rnx";

// A header record: content padded to sixty columns, then the label.
const headerLine = (body, label) => `${body.padEnd(60)}${label}`;
// One F14.3 observation field with blank LLI and SSI.
const obsField = (value) => `${value.toFixed(3).padStart(14)}  `;
// One F14.3 field with its LLI and SSI digits.
const obsFieldFlags = (value, lli, ssi) => `${value.toFixed(3).padStart(14)}${lli}${ssi}`;
const blankField = " ".repeat(16);
const obsRecord = (sat, values) =>
  `${sat}${values.map((value) => (value === null ? blankField : obsField(value))).join("")}`.trimEnd();
// A version 3 event record with blank epoch fields and the records after it.
const blankEvent = (flag, records) =>
  [`>${" ".repeat(30)}${flag}${String(records.length).padStart(3)}`, ...records].join("\n");
const obsText = (version, headers, body) =>
  [
    headerLine(
      `${version.toFixed(2).padStart(9)}           OBSERVATION DATA    M (MIXED)`,
      "RINEX VERSION / TYPE",
    ),
    ...headers,
    headerLine("", "END OF HEADER"),
    ...body,
  ].join("\n");
const parse = (text) => parseRinexObs(encoder.encode(text));

// The error `fn` throws, which must be `name` carrying a typed `detail`.
function thrown(fn, name) {
  let error;
  try {
    fn();
  } catch (caught) {
    error = caught;
  }
  assert.ok(error instanceof Error, `expected ${name} to be thrown`);
  assert.equal(error.name, name);
  assert.ok(error.detail !== null && typeof error.detail === "object", "typed detail attached");
  assert.equal(error.detail.message, error.message);
  return error;
}

// G01 L1W carries two corrections in one block, G02 L1W its own named one over
// the record for every satellite, G03 L1W only that record, L1C a blank named
// correction, L2W only the constellation-level record that declares the
// alignment unknown, and G03 L2W a blank beside 0.5.
const PHASE_SHIFT_HEADERS = [
  headerLine("G    3 L1C L1W L2W", "SYS / # / OBS TYPES"),
  headerLine("G L1W  0.25000  01 G01", "SYS / PHASE SHIFT"),
  headerLine("G L1W  0.50000  02 G02 G01", "SYS / PHASE SHIFT"),
  headerLine("G L1W  0.75000", "SYS / PHASE SHIFT"),
  headerLine("G L1C", "SYS / PHASE SHIFT"),
  headerLine("G", "SYS / PHASE SHIFT"),
  headerLine(`G L2W ${" ".repeat(8)}  01 G03`, "SYS / PHASE SHIFT"),
  headerLine("G L2W  0.50000  01 G03", "SYS / PHASE SHIFT"),
];
const PHASE_SHIFT_BODY = [
  "> 2020 01 01 00 00  0.0000000  0  3",
  obsRecord("G01", [100, 200, 300]),
  obsRecord("G02", [110, 210, 310]),
  obsRecord("G03", [120, 220, 320]),
];

test("every carrier phase row is kept with its phase-shift status", () => {
  const obs = parse(obsText(3.05, PHASE_SHIFT_HEADERS, PHASE_SHIFT_BODY));
  const phase = obs.carrierPhaseRows(0);

  assert.equal(phase.length, 9);
  assert.deepEqual(phase.satellites, [
    "G01",
    "G01",
    "G01",
    "G02",
    "G02",
    "G02",
    "G03",
    "G03",
    "G03",
  ]);
  assert.deepEqual(phase.codes, ["L1C", "L1W", "L2W", "L1C", "L1W", "L2W", "L1C", "L1W", "L2W"]);
  // An unavailable correction never drops its row or its measurement.
  assert.deepEqual(Array.from(phase.valueCycles), [100, 200, 300, 110, 210, 310, 120, 220, 320]);
  assert.deepEqual(phase.phaseShiftStatus, [
    "available",
    "ambiguous",
    "unknown",
    "available",
    "available",
    "unknown",
    "available",
    "available",
    "ambiguous",
  ]);
  assert.deepEqual(Array.from(phase.phaseShiftAvailable), [1, 0, 0, 1, 1, 0, 1, 1, 0]);
  assert.ok(phase.phaseShiftAvailable instanceof Uint8Array);
  // NaN, never 0, where the header gives no one correction.
  assert.deepEqual(Array.from(phase.phaseShiftCycles), [0, NaN, NaN, 0, 0.5, NaN, 0, 0.75, NaN]);
  assert.deepEqual(phase.phaseShiftCorrections, [
    { status: "available", cycles: 0 },
    { status: "ambiguous", corrections: [0.25, 0.5] },
    { status: "unknown" },
    { status: "available", cycles: 0 },
    { status: "available", cycles: 0.5 },
    { status: "unknown" },
    { status: "available", cycles: 0 },
    { status: "available", cycles: 0.75 },
    { status: "ambiguous", corrections: [null, 0.5] },
  ]);

  // The records themselves are kept as written, and each contradiction, G01
  // L1W and G03 L2W, is counted.
  assert.equal(obs.skippedRecords, 2);
  const shifts = obs.header.phaseShifts;
  assert.equal(shifts.length, 7);
  assert.equal(shifts[0].correctionCycles, 0.25);
  assert.deepEqual(shifts[1].satellites, ["G02", "G01"]);
  assert.equal(shifts[1].satelliteCount, 2);
  assert.equal(shifts[1].coversEverySatellite, false);
  assert.equal(shifts[2].coversEverySatellite, true);
  assert.equal(shifts[3].code, "L1C");
  assert.equal(shifts[3].correctionCycles, undefined);
  assert.equal(shifts[4].code, undefined);
  assert.equal(shifts[4].correctionCycles, undefined);
  assert.equal(shifts[5].correctionCycles, undefined);
  assert.deepEqual(shifts[5].satellites, ["G03"]);
  assert.deepEqual(shifts[5].unrepresentableSatellites, []);

  // The contradictions are written back and read back the same way.
  const reparsed = parse(obs.toRinexString());
  assert.deepEqual(reparsed.carrierPhaseRows(0).phaseShiftCorrections, phase.phaseShiftCorrections);
});

test("a RINEX 4 file keeps its phase-shift records and applies none of them", () => {
  const obs = parse(obsText(4.02, PHASE_SHIFT_HEADERS, PHASE_SHIFT_BODY));
  const phase = obs.carrierPhaseRows(0);
  assert.equal(obs.header.phaseShifts.length, 7);
  assert.equal(obs.skippedRecords, 0, "RINEX 4 records are not checked for contradictions");
  assert.deepEqual(phase.phaseShiftStatus, Array(9).fill("available"));
  assert.deepEqual(Array.from(phase.phaseShiftCycles), Array(9).fill(0));
  assert.deepEqual(Array.from(phase.phaseShiftAvailable), Array(9).fill(1));
});

test("a blank named phase shift in a real header reads as an available 0", () => {
  const obs = parseRinexObs(fixture(ESBC));
  const record = obs.header.phaseShifts.find(
    (shift) => shift.system === GnssSystem.Gps && shift.code === "L1C",
  );
  assert.ok(record, "the fixture carries a G L1C record");
  assert.equal(record.correctionCycles, undefined);
  assert.equal(record.coversEverySatellite, true);

  const phase = obs.carrierPhaseRows(0);
  const row = phase.satellites.findIndex((sat, i) => sat === "G05" && phase.codes[i] === "L1C");
  assert.ok(row >= 0);
  assert.equal(phase.phaseShiftStatus[row], "available");
  assert.equal(phase.phaseShiftAvailable[row], 1);
  assert.equal(phase.phaseShiftCycles[row], 0);
});

// Epoch 0 is read by the file header's list; epoch 1 is an untimed flag 4
// event declaring a longer list and a phase shift; epoch 2 is read by the new
// list; epoch 3 reports a cycle slip.
function eventText() {
  const lines = [
    "     3.05           OBSERVATION DATA    M                   RINEX VERSION / TYPE",
    "G    2 C1C L1C                                              SYS / # / OBS TYPES",
    "                                                            END OF HEADER",
    "> 2020 01 01 00 00  0.0000000  0  1",
    `G01${obsFieldFlags(20_000_000, 0, 7)}${obsFieldFlags(100_000.125, 0, 7)}`,
    blankEvent(4, [
      headerLine("G    3 C1C L1C L2W", "SYS / # / OBS TYPES"),
      headerLine("G L1C  0.25000", "SYS / PHASE SHIFT"),
    ]),
    "> 2020 01 01 00 00 30.0000000  0  1",
    obsRecord("G01", [20_000_030, 100_030.25, 77_930.5]),
    "> 2020 01 01 00 01  0.0000000  6  1",
    `G01${blankField}${(1).toFixed(3).padStart(14)}`,
  ];
  return lines.join("\n");
}

const BLANK = { value: null, lli: null, ssi: null };

test("an untimed event, its header records and a cycle slip epoch keep their places", () => {
  const obs = parse(eventText());
  assert.equal(obs.epochCount, 4);
  assert.deepEqual(
    obs.epochs.map((epoch) => epoch.flag),
    [0, 4, 0, rinexObsCycleSlipFlag()],
  );

  const first = obs.epoch(0);
  assert.equal(first.epoch.second, 0);
  // Values are held under the union of every declared list, blank where the
  // list in effect at the epoch declares no code.
  assert.deepEqual(first.observations, [
    {
      satellite: "G01",
      values: [
        { value: 20_000_000, lli: 0, ssi: 7 },
        { value: 100_000.125, lli: 0, ssi: 7 },
        BLANK,
      ],
    },
  ]);

  const event = obs.epoch(1);
  assert.equal(event.epoch, undefined, "an event with blank epoch fields has no time");
  assert.equal(event.declaredRecordCount, 2);
  assert.deepEqual(event.specialRecords, [
    headerLine("G    3 C1C L1C L2W", "SYS / # / OBS TYPES"),
    headerLine("G L1C  0.25000", "SYS / PHASE SHIFT"),
  ]);
  assert.deepEqual(event.satellites, []);
  assert.deepEqual(event.observations, []);
  assert.deepEqual(event.cycleSlips, []);

  assert.deepEqual(
    obs.epoch(2).observations[0].values.map((field) => field.value),
    [20_000_030, 100_030.25, 77_930.5],
  );

  const slips = obs.epoch(3);
  assert.equal(slips.epoch.minute, 1);
  assert.deepEqual(slips.satellites, [], "a slip is not an observation");
  assert.equal(slips.satelliteCount, 0);
  assert.deepEqual(slips.specialRecords, []);
  assert.equal(slips.declaredRecordCount, 1);
  assert.deepEqual(slips.cycleSlipSatellites, ["G01"]);
  assert.deepEqual(slips.cycleSlips, [
    { satellite: "G01", values: [BLANK, { value: 1, lli: null, ssi: null }, BLANK] },
  ]);

  // The file header holds the union and its own declared list; the header in
  // effect changes at the event.
  const file = obs.header;
  assert.deepEqual(file.obsCodes(GnssSystem.Gps), ["C1C", "L1C", "L2W"]);
  assert.deepEqual(file.declaredObsCodes(GnssSystem.Gps), ["C1C", "L1C"]);
  assert.equal(file.declaredObsCodes(GnssSystem.Galileo), undefined);
  assert.deepEqual(obs.headerAt(0).declaredObsCodes(GnssSystem.Gps), ["C1C", "L1C"]);
  assert.deepEqual(obs.headerAt(1).declaredObsCodes(GnssSystem.Gps), ["C1C", "L1C", "L2W"]);
  assert.equal(obs.headerAt(0).phaseShifts.length, 0);
  assert.equal(obs.headerAt(2).phaseShifts[0].correctionCycles, 0.25);

  // Carrier rows read with the header in effect at their epoch.
  const before = obs.carrierPhaseRows(0);
  assert.deepEqual(before.codes, ["L1C", "L2W"]);
  assert.deepEqual(Array.from(before.phaseShiftCycles), [0, 0]);
  assert.ok(Number.isNaN(before.valueCycles[1]), "a blank value stays blank");
  const after = obs.carrierPhaseRows(2);
  assert.deepEqual(after.codes, ["L1C", "L2W"]);
  assert.deepEqual(Array.from(after.phaseShiftCycles), [0.25, 0]);

  const timeline = obs.headerTimeline();
  assert.equal(timeline.segmentCount, 2);
  assert.deepEqual(
    timeline.segments.map((segment) => segment.firstEpochIndex),
    [0, 1],
  );
  assert.equal(timeline.segmentIndex(0), 0);
  assert.equal(timeline.segmentIndex(3), 1);
  assert.deepEqual(timeline.at(3).declaredObsCodes(GnssSystem.Gps), ["C1C", "L1C", "L2W"]);
  // The timeline answers past the last epoch; headerAt refuses it.
  assert.deepEqual(timeline.at(99).declaredObsCodes(GnssSystem.Gps), ["C1C", "L1C", "L2W"]);
  assert.throws(() => obs.headerAt(4), RangeError);

  // The event is written back with blank epoch fields and its records.
  const text = obs.toRinexString();
  assert.ok(text.split("\n").includes(`>${" ".repeat(30)}4  2`), text);
  const reparsed = parse(text);
  assert.equal(reparsed.epoch(1).epoch, undefined);
  assert.deepEqual(reparsed.epoch(3).cycleSlips, slips.cycleSlips);
});

test("a leading event gives a second segment at epoch 0", () => {
  const obs = parse(
    obsText(
      3.05,
      [headerLine("G    2 C1C L1C", "SYS / # / OBS TYPES")],
      [
        blankEvent(4, [headerLine("G    2 L1C C1C", "SYS / # / OBS TYPES")]),
        "> 2020 01 01 00 00  0.0000000  0  1",
        obsRecord("G01", [100, 20_000_000]),
      ],
    ),
  );
  const timeline = obs.headerTimeline();
  assert.deepEqual(
    timeline.segments.map((segment) => segment.firstEpochIndex),
    [0, 0],
  );
  assert.deepEqual(timeline.at(0).declaredObsCodes(GnssSystem.Gps), ["L1C", "C1C"]);
  // Read by the event's list, held under the file header's order.
  assert.deepEqual(
    obs.epoch(1).observations[0].values.map((field) => field.value),
    [20_000_000, 100],
  );
});

const LEAP_BDT_305 = [
  "     3.05           OBSERVATION DATA    C                   RINEX VERSION / TYPE",
  "C    1 C2I                                                  SYS / # / OBS TYPES",
  "     4        1000     6BDT                                 LEAP SECONDS",
  "                                                            END OF HEADER",
  "> 2020 06 24 00 00  0.0000000  0  1",
  "C01  22000000.000  ",
  "",
].join("\n");

test("a LEAP SECONDS record keeps its raw time system and exact integers", () => {
  const bdt = parse(LEAP_BDT_305);
  const leap = bdt.header.leapSeconds;
  assert.equal(leap.current, 4n);
  assert.equal(leap.deltaFuture, undefined);
  assert.equal(leap.week, 1000n);
  assert.equal(leap.day, 6n);
  assert.equal(leap.timeSystem, "BDT");
  assert.equal(parse(bdt.toRinexString()).header.leapSeconds.timeSystem, "BDT");

  const gps = parse(
    LEAP_BDT_305.replace("     4        1000     6BDT", "    18        2300     1GPS"),
  );
  assert.equal(gps.header.leapSeconds.current, 18n);
  assert.equal(gps.header.leapSeconds.timeSystem, "GPS");
  const blank = parse(
    LEAP_BDT_305.replace("     4        1000     6BDT", "    18        2300     1   "),
  );
  assert.equal(blank.header.leapSeconds.timeSystem, undefined, "blank stays distinct from GPS");

  // Version 2 cannot state a BeiDou time system, so the downgrade refuses it.
  const error = thrown(() => bdt.downgradeToRinex2(2.11), "RinexObsWriteError");
  assert.deepEqual(error.detail, {
    kind: "LEAP_SECONDS_TIME_SYSTEM_NOT_IN_VERSION",
    timeSystem: "BDT",
    version: 2.11,
    message: error.message,
  });
});

test("GLONASS code-phase biases keep absent, blank, zero and conflicting apart", () => {
  const bias = (value) => value.toFixed(3).padStart(8);
  const withRecord = (content) =>
    parse(
      obsText(
        3.05,
        [
          headerLine("G    1 C1C", "SYS / # / OBS TYPES"),
          headerLine(content, "GLONASS COD/PHS/BIS"),
        ],
        [],
      ),
    ).header;

  const header = withRecord(` C1C ${bias(-10)} C1P ${" ".repeat(8)} C2C ${bias(-10.432)}`);
  assert.deepEqual(header.glonassCodPhsBis, [
    { code: "C1C", biasM: -10 },
    { code: "C1P", biasM: null },
    { code: "C2C", biasM: -10.432 },
  ]);
  assert.deepEqual(header.glonassCodePhaseBias("C1C"), { status: "available", biasM: -10 });
  assert.deepEqual(header.glonassCodePhaseBias("C1P"), { status: "unknown" });
  assert.deepEqual(header.glonassCodePhaseBias("C2P"), { status: "none" });

  const zero = withRecord(` C1C ${bias(0)}`);
  assert.deepEqual(zero.glonassCodePhaseBias("C1C"), { status: "available", biasM: 0 });

  const conflicting = withRecord(` C1C ${bias(-9)} C1C ${bias(-10)}`);
  assert.deepEqual(conflicting.glonassCodePhaseBias("C1C"), {
    status: "ambiguous",
    biasesM: [-9, -10],
  });

  const blankRecord = withRecord("");
  assert.deepEqual(blankRecord.glonassCodPhsBis, []);
  assert.deepEqual(blankRecord.glonassCodePhaseBias("C1C"), { status: "unknown" });

  const absent = parse(obsText(3.05, [headerLine("G    1 C1C", "SYS / # / OBS TYPES")], [])).header;
  assert.equal(absent.glonassCodPhsBis, null);
  assert.deepEqual(absent.glonassCodePhaseBias("C1C"), { status: "none" });
});

// A version 2 file whose SYS / SCALE FACTOR record this reader applies and a
// version 2 reader that does not know the record would not.
const SCALED_V2 = [
  headerLine("     2.11           OBSERVATION DATA    G (GPS)", "RINEX VERSION / TYPE"),
  headerLine("     2    C1    L1", "# / TYPES OF OBSERV"),
  headerLine("G   10   0", "SYS / SCALE FACTOR"),
  headerLine("  2015     1     1     0     0    0.0000000     GPS", "TIME OF FIRST OBS"),
  headerLine("", "END OF HEADER"),
  " 15  1  1  0  0  0.0000000  0  1G 1",
  " 200000010.000        1230.001",
  "",
].join("\n");

test("the strict writer refuses a scaled version 2 product and the downgrade reports each change", () => {
  const obs = parse(SCALED_V2);
  assert.equal(obs.epoch(0).observations[0].values[0].value, 20_000_001);
  assert.deepEqual(obs.header.scaleFactors, [{ system: "G", factor: 10, codes: [] }]);

  const error = thrown(() => obs.toRinexString(), "RinexObsWriteError");
  assert.deepEqual(error.detail, {
    kind: "SCALE_FACTORS_IN_VERSION_TWO",
    count: 1,
    message:
      "RINEX OBS version 2 would carry 1 SYS / SCALE FACTOR records, which version 2 readers that do not apply them read as physical values; downgrade_to_rinex2 removes them",
  });

  const result = obs.downgradeToRinex2(2.11);
  assert.equal(result.value, result.obs);
  assert.deepEqual(result.changes, [
    { kind: "SCALE_FACTORS_REMOVED", count: 1 },
    {
      kind: "VALUE_ROUNDED",
      epochIndex: 0,
      satellite: "G01",
      code: "L1C",
      from: 1230.001 / 10,
      to: 123,
    },
  ]);
  const plain = result.obs.toRinexString();
  assert.ok(!plain.includes("SYS / SCALE FACTOR"));
  assert.ok(plain.includes("  20000001.000         123.000"), plain);
  // The source product is not changed.
  assert.equal(obs.header.scaleFactors.length, 1);

  // A repair of the same text is returned whole; writing it throws the same
  // typed refusal.
  const repair = repairRinexObs(encoder.encode(SCALED_V2), {});
  assert.equal(repair.repaired.header.scaleFactors.length, 1);
  const textError = thrown(() => repair.repairedText, "RinexObsWriteError");
  assert.equal(textError.detail.kind, "SCALE_FACTORS_IN_VERSION_TWO");
  assert.equal(textError.detail.count, 1);
  const crinexError = thrown(() => repair.toCrinexString(), "RinexObsWriteError");
  assert.deepEqual(crinexError.detail, textError.detail);
});

test("the public downgrade reports nested changes made by an event list", () => {
  const source = obsText(
    3.05,
    [
      headerLine("G    2 C1C L1C", "SYS / # / OBS TYPES"),
      headerLine("R    1 C1C", "SYS / # / OBS TYPES"),
    ],
    [
      "> 2020 01 01 00 00  0.0000000  0  2",
      obsRecord("G01", [20_000_000, 100_000]),
      obsRecord("R02", [21_000_000]),
      blankEvent(4, [headerLine("G    3 L1C C1C S1C", "SYS / # / OBS TYPES")]),
      "> 2020 01 01 00 00 30.0000000  0  2",
      obsRecord("G01", [100_030, 20_000_030, 45]),
      obsRecord("R02", [21_000_030]),
      blankEvent(4, [headerLine("G    1 C1C", "SYS / # / OBS TYPES")]),
      "> 2020 01 01 00 01  0.0000000  0  1",
      obsRecord("G01", [20_000_060]),
    ],
  );
  const result = parse(source).downgradeToRinex2(2.11);

  assert.deepEqual(result.changes, [
    { kind: "CODE_ADDED", system: "R", code: "L1C" },
    {
      kind: "IN_EVENT_LISTS",
      epochIndex: 1,
      change: { kind: "CODE_ADDED", system: "R", code: "L1C" },
    },
    {
      kind: "IN_EVENT_LISTS",
      epochIndex: 1,
      change: { kind: "CODE_MOVED", system: "R", code: "C1C", from: 0, to: 1 },
    },
    {
      kind: "IN_EVENT_LISTS",
      epochIndex: 1,
      change: { kind: "CODE_ADDED", system: "R", code: "S1C" },
    },
    {
      kind: "EVENT_RECORDS_REWRITTEN",
      epochIndex: 1,
      from: ["G    3 L1C C1C S1C                                          SYS / # / OBS TYPES"],
      to: ["     3    L1    C1    S1                                    # / TYPES OF OBSERV"],
    },
    {
      kind: "EVENT_RECORDS_REWRITTEN",
      epochIndex: 3,
      from: ["G    1 C1C                                                  SYS / # / OBS TYPES"],
      to: ["     1    C1                                                # / TYPES OF OBSERV"],
    },
  ]);
  assert.equal(result.value, result.obs);
});

test("the downgrade refuses what version 2 cannot state and leaves the source alone", () => {
  const esbc = parseRinexObs(fixture(ESBC));
  const notTwo = thrown(() => esbc.downgradeToRinex2(3.05), "RinexObsWriteError");
  assert.deepEqual(notTwo.detail, {
    kind: "NOT_VERSION_TWO",
    version: 3.05,
    message: notTwo.message,
  });

  const b1c = parse(
    [
      "     3.05           OBSERVATION DATA    C                   RINEX VERSION / TYPE",
      "C    2 C1P L1P                                              SYS / # / OBS TYPES",
      "                                                            END OF HEADER",
      "> 2020 06 24 00 00  0.0000000  0  1",
      "C01  22000000.000          10.000  ",
      "",
    ].join("\n"),
  );
  for (const version of [2.11, 2.12]) {
    const error = thrown(() => b1c.downgradeToRinex2(version), "RinexObsWriteError");
    assert.deepEqual(error.detail, {
      kind: "OBSERVABLE_NOT_REPRESENTABLE",
      system: "C",
      code: "C1P",
      version,
      message: error.message,
    });
  }
  assert.equal(b1c.header.version, 3.05);
  assert.deepEqual(b1c.obsCodes(GnssSystem.BeiDou), ["C1P", "L1P"]);
});

test("an epoch index is read exactly or refused", () => {
  const obs = parseRinexObs(fixture(ESBC));
  for (const index of [-1, 1.5, NaN, Infinity, 2 ** 32, 2]) {
    assert.throws(() => obs.epoch(index), RangeError, `epoch(${index})`);
    assert.throws(() => obs.headerAt(index), RangeError, `headerAt(${index})`);
    assert.throws(() => obs.carrierPhaseRows(index), RangeError, `carrierPhaseRows(${index})`);
    assert.throws(() => obs.observationValues(index), RangeError, `observationValues(${index})`);
    assert.throws(() => obs.pseudoranges(index), RangeError, `pseudoranges(${index})`);
  }
  assert.equal(obs.epoch(0).flag, 0);
  assert.equal(obs.headerAt(1).version, 3.05);
  const timeline = obs.headerTimeline();
  assert.throws(() => timeline.at(-1), RangeError);
  assert.throws(() => timeline.segmentIndex(0.5), RangeError);
});
