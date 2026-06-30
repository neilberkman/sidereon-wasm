// ANTEX antenna-calibration binding: parse a committed ANTEX product and read
// satellite / receiver PCO and PCV, mirroring tests/test_antex.py. PCO/PCV are
// metres; the fixture comments are millimetres, so the goldens are divided by
// 1000 exactly as the Python test does.

import { test } from "node:test";
import assert from "node:assert/strict";

import { loadAntex, AntexDateTime } from "../pkg-node/sidereon.js";
import { fixture } from "./helpers.mjs";

const mm = (values) => values.map((v) => v / 1000.0);
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
  assert.equal(g05.pcv("G01", 9.0), -9.5 / 1000.0);
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
  assert.equal(receiver.pcv("G01", 10.0), 0.99 / 1000.0);
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
  assert.equal(receiver.pcv("G01", 20.0), -0.99 / 1000.0);
});

test("ANTEX validation and lookup errors throw", () => {
  const antex = loadAntex(fixture("antex/igs20_wettzell_trim.atx"));
  const receiver = antex.antenna("LEIAR25.R3      LEIT");
  assert.ok(receiver);
  assert.throws(() => new AntexDateTime(2020, 2, 30));
  assert.throws(() => receiver.pco("UNKNOWN"));
  assert.throws(() => receiver.pcv("G01", NaN));
});

test("toAntexString re-parses to the same antennas and is byte-stable", () => {
  const antex = loadAntex(fixture("antex/igs20_wettzell_trim.atx"));
  const text = antex.toAntexString();
  const reparsed = loadAntex(Buffer.from(text, "utf8"));
  assert.equal(reparsed.antennaCount, antex.antennaCount);
  assert.deepEqual(reparsed.antennaIds, antex.antennaIds);
  assert.equal(reparsed.toAntexString(), text);
});
