// ANTEX antenna-calibration binding: parse a committed ANTEX product and read
// satellite / receiver PCO and PCV, every record the file carries, and the
// exact validity seconds. PCO/PCV are metres; the fixture values are
// millimetres, converted as `mm * 1e-3`, the arithmetic of RTKLIB `readantex`
// that the engine applies, so the goldens match bit for bit.

import { test } from "node:test";
import assert from "node:assert/strict";

import { loadAntex, AntexDateTime } from "../pkg-node/sidereon.js";
import { fixture } from "./helpers.mjs";

const mm = (values) => values.map((v) => v * 1e-3);
const eqVec = (got, want) => {
  assert.equal(got.length, want.length);
  want.forEach((v, i) => assert.equal(got[i], v));
};

test("load ANTEX and look up satellite PCO/PCV", () => {
  const antex = loadAntex(fixture("antex/igs20_wettzell_trim.atx"));
  const epoch = new AntexDateTime(2020, 6, 25);

  const g05 = antex.satelliteAntenna("G05", epoch);
  assert.equal(antex.antennaCount, 10);
  assert.ok(g05);
  assert.equal(g05.kind, "satellite");
  assert.equal(g05.serial, "G05");
  assert.ok(g05.validAt(epoch));
  assert.ok(g05.validFrom);
  assert.equal(g05.validFrom.year, 2009);
  assert.equal(g05.validFrom.month, 8);
  assert.equal(g05.validFrom.day, 17);
  assert.equal(g05.validUntil, undefined);
  assert.ok(g05.frequencies.includes("G01"));
  eqVec(g05.pco("G01"), mm([-3.3, -0.3, 742.63]));
  assert.equal(g05.pcv("G01", 9.0), -9.5 * 1e-3);
  assert.equal(antex.satelliteAntenna("G99", epoch), undefined);
});

test("ANTEX receiver lookup from bytes", () => {
  const antex = loadAntex(fixture("antex/igs20_wettzell_trim.atx"));
  const receiver = antex.antenna("LEIAR25.R3      LEIT");
  assert.ok(receiver);
  assert.equal(receiver.kind, "receiver");
  assert.equal(receiver.antennaType, "LEIAR25.R3      LEIT");
  assert.equal(receiver.serial, "");
  assert.equal(receiver.validFrom, undefined);
  eqVec(receiver.pco("G01"), mm([-0.05, 0.95, 160.96]));
  assert.equal(receiver.pcv("G01", 10.0), 0.99 * 1e-3);
});

test("second ANTEX fixture receiver PCO", () => {
  const antex = loadAntex(fixture("antex/igs20_pasa_scoa_gps.atx"));
  const id = antex.antennaIds.find((x) => x.startsWith("LEIAR20"));
  const receiver = antex.antenna(id);
  assert.ok(receiver);
  assert.equal(receiver.kind, "receiver");
  assert.equal(receiver.antennaType, "LEIAR20         LEIM");
  assert.equal(receiver.serial, "");
  eqVec(receiver.pco("G01"), mm([0.5, 0.13, 124.88]));
  assert.equal(receiver.pcv("G01", 20.0), -0.99 * 1e-3);
});

test("ANTEX validation and lookup errors throw", () => {
  const antex = loadAntex(fixture("antex/igs20_wettzell_trim.atx"));
  const receiver = antex.antenna("LEIAR25.R3      LEIT");
  assert.ok(receiver);
  assert.throws(() => new AntexDateTime(2020, 2, 30), RangeError);
  assert.throws(
    () => receiver.pco("UNKNOWN"),
    (err) => {
      assert.equal(err.name, "AntexLookupError");
      assert.deepEqual(
        { ...err.detail, message: undefined },
        {
          kind: "UNKNOWN_FREQUENCY",
          antennaId: "LEIAR25.R3      LEIT",
          frequency: "UNKNOWN",
          message: undefined,
        },
      );
      assert.equal(err.detail.message, err.message);
      return true;
    },
  );
  assert.throws(() => receiver.pcv("G01", NaN), RangeError);
  // A zenith outside the block's ZEN1..ZEN2 grid.
  assert.throws(
    () => receiver.pcv("G01", 95.0),
    (err) => err.name === "AntexLookupError" && err.detail.kind === "INVALID_INPUT",
  );
});

test("toAntexString re-parses to the same antennas and is byte-stable", () => {
  const antex = loadAntex(fixture("antex/igs20_wettzell_trim.atx"));
  const text = antex.toAntexString();
  const reparsed = loadAntex(Buffer.from(text, "utf8"));
  assert.equal(reparsed.antennaCount, antex.antennaCount);
  assert.deepEqual(reparsed.antennaIds, antex.antennaIds);
  assert.equal(reparsed.toAntexString(), text);
});

const G03 = "antex/igs20_block_i_g03_trim.atx";
const RELATIVE = "antex/igs_01_relative_trim.atx";

test("the header, calibration records and exact validity seconds are retained", () => {
  const antex = loadAntex(fixture(G03));
  assert.deepEqual(antex.header, {
    version: { version: 1.4, system: "M" },
    pcvType: {
      pcvType: "absolute",
      referenceAntennaType: "",
      referenceAntennaSerial: "",
      referenceAntenna: null,
    },
    comments: [
      "###########################################################",
      "General hint for satellite antenna corrections:",
      "###########################################################",
    ],
    endOfHeader: true,
  });
  assert.deepEqual(antex.outerComments, []);
  assert.equal(antex.skippedRecords, 0);
  assert.equal(antex.blockCount, 1);

  const [g03] = antex.antennaBlocks();
  assert.equal(g03.kind, "satellite");
  assert.equal(g03.antennaType, "BLOCK I");
  assert.equal(g03.serial, "G03");
  assert.deepEqual(g03.leadingComments, []);
  assert.deepEqual(g03.comments, []);
  assert.deepEqual(g03.calibrations, [
    { method: "", agency: "", antennasCalibrated: 0, date: "29-JAN-17" },
  ]);
  assert.equal(g03.daziDeg, 0);
  assert.equal(g03.zenithStartDeg, 0);
  assert.equal(g03.zenithEndDeg, 14);
  assert.equal(g03.zenithStepDeg, 1);
  assert.equal(g03.hasFrequencyCount, true);
  assert.equal(g03.sinexCode, "IGS20_2434");
  assert.deepEqual(g03.frequencies, ["G01", "G02"]);

  // VALID UNTIL states 59.9999999: every digit is kept, not rounded to 59 or
  // carried into the next minute.
  const until = g03.validUntil;
  assert.deepEqual(
    [until.year, until.month, until.day, until.hour, until.minute, until.second],
    [1994, 4, 17, 23, 59, 59],
  );
  assert.equal(until.fractionDigits, "9999999");
  assert.equal(until.nanosecond, 999999900);
  assert.equal(g03.validFrom.fractionDigits, "");
  assert.equal(g03.validFrom.nanosecond, 0);
  assert.ok(g03.validAt(new AntexDateTime(1994, 4, 17, 23, 59, 59, "9999999")));
  assert.ok(!g03.validAt(new AntexDateTime(1994, 4, 17, 23, 59, 59, "99999991")));
  assert.equal(
    antex.satelliteAntenna("G03", new AntexDateTime(1994, 4, 17, 23, 59, 59, "99999991")),
    undefined,
  );
  assert.equal(antex.antennaAt(g03.id, new AntexDateTime(1990, 1, 1)).serial, "G03");
  assert.equal(antex.antennaIntervals(g03.id).length, 1);

  const [g01] = g03.frequencySections();
  assert.equal(g01.frequency, "G01");
  assert.deepEqual(g01.pcoM, mm([210.0, 0.0, 1745.98]));
  assert.equal(g01.rms, null);
  assert.equal(g01.pcvSamples.length, 15);
  assert.deepEqual(g01.pcvSamples[0], {
    grid: "noAzimuth",
    azimuthDeg: null,
    zenithDeg: 0,
    valueM: -1.0 * 1e-3,
  });
  assert.deepEqual(g03.frequency("G01"), g01);

  // The writer restates the seconds exactly and reads back to equal records.
  const text = antex.toAntexString();
  assert.match(text, /^ {2}1994 {5}4 {4}17 {4}23 {4}59 {3}59\.9999999 +VALID UNTIL$/m);
  const reread = loadAntex(Buffer.from(text, "utf8"));
  assert.deepEqual(reread.header, antex.header);
  assert.equal(reread.antennaBlocks()[0].validUntil.fractionDigits, "9999999");
  assert.equal(reread.toAntexString(), text);
});

test("relative values name their reference antenna", () => {
  const antex = loadAntex(fixture(RELATIVE));
  assert.deepEqual(antex.header.version, { version: 1.3, system: "M" });
  assert.deepEqual(antex.header.pcvType, {
    pcvType: "relative",
    referenceAntennaType: "AOAD/M_T",
    referenceAntennaSerial: "",
    referenceAntenna: "AOAD/M_T",
  });
  assert.equal(antex.blockCount, 2);
  const blocks = antex.antennaBlocks();
  assert.deepEqual(
    blocks.map((block) => block.antennaType),
    ["BLOCK I", "ASH700699.L1    NONE"],
  );
  assert.deepEqual(blocks[1].calibrations, [
    { method: "FIELD", agency: "IGEX", antennasCalibrated: null, date: "12-NOV-98" },
  ]);
  assert.equal(blocks[1].validFrom, undefined);
  assert.equal(blocks[1].validUntil, undefined);
});

test("a repeated frequency label with differing sections is refused as ambiguous", () => {
  const source = fixture(G03).toString("utf8");
  // Relabel the G02 section as G01 and change its up offset.
  const relabeled = source.replaceAll("   G02 ", "   G01 ");
  const at = relabeled.lastIndexOf("1745.98");
  const text = `${relabeled.slice(0, at)}1745.99${relabeled.slice(at + 7)}`;
  const antex = loadAntex(Buffer.from(text, "utf8"));
  assert.equal(antex.skippedRecords, 0);
  const [block] = antex.antennaBlocks();
  assert.deepEqual(block.frequencies, ["G01", "G01"]);
  assert.equal(block.frequencySections().length, 2);
  for (const call of [
    () => block.pco("G01"),
    () => block.frequency("G01"),
    () => block.pcv("G01", 5),
  ]) {
    assert.throws(call, (err) => {
      assert.equal(err.name, "AntexLookupError");
      assert.equal(err.detail.kind, "AMBIGUOUS_FREQUENCY");
      assert.equal(err.detail.antennaId, block.id);
      assert.equal(err.detail.frequency, "G01");
      assert.equal(err.detail.sections, 2);
      return true;
    });
  }
});

test("a record that does not read is refused by name", () => {
  const source = fixture(G03).toString("utf8");
  const text = source.replace(/^ {5}0\.0(?= +DAZI)/m, "     x.x");
  assert.notEqual(text, source);
  assert.throws(
    () => loadAntex(Buffer.from(text, "utf8")),
    (err) => {
      assert.equal(err.name, "AntexParseError");
      assert.equal(err.detail.kind, "INVALID_FIELD");
      assert.equal(err.detail.record, "DAZI");
      assert.equal(err.detail.field, "dazi");
      assert.equal(err.detail.value, "x.x");
      assert.match(err.detail.antennaId, /^BLOCK I +G03/);
      return true;
    },
  );
});

test("AntexDateTime takes the exact fraction of the second", () => {
  const exact = new AntexDateTime(2020, 1, 1, 0, 0, 1, "0000000012345678");
  assert.equal(exact.fractionDigits, "0000000012345678");
  assert.equal(exact.nanosecond, undefined);
  assert.equal(new AntexDateTime(2020, 1, 1, 0, 0, 1, "500").fractionDigits, "5");
  assert.equal(new AntexDateTime(2020, 1, 1, 0, 0, 1, "000000001").nanosecond, 1);
  assert.equal(new AntexDateTime(2020, 1, 1, 0, 0, 1, "0000").fractionDigits, "");
  // GPS time has no leap-second label.
  assert.throws(() => new AntexDateTime(2016, 12, 31, 23, 59, 60), RangeError);
  assert.throws(() => new AntexDateTime(2020, 1, 1, 0, 0, 0, "1.5"), TypeError);
  assert.throws(() => new AntexDateTime(2020, 1, 1, 0, 0, 0, "1".repeat(20)), RangeError);
});
