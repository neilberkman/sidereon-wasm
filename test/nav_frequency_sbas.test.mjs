// WASM parity for the accepted RINEX NAV, RINEX observation-code, and SBAS
// text-log subsets. Values are frozen from the public sidereon-core 1.1.1
// behavior and use the binding's public conversion/error surface.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  encodeRinexNav,
  GnssSystem,
  parseRinexNavLenient,
  parseRinexNavRecords,
  parseSbasEmsLines,
  parseSbasRtklibLines,
  rinexObservationFrequencyHz,
  rinexObservationWavelengthM,
} from "../pkg-node/sidereon.js";

import { fixture, fixtureText, f64Bits, splitlines } from "./helpers.mjs";

const NAV_FIXTURE = "nav/ESBC00DNK_R_20201770000_01D_MN.rnx";
const SBAS_HEX = "5308DFFC010005FFC00DFFC009FFDFFC001FFDFFDFFFBABBBBBB9BBB80";

const hexToBytes = (hex) => Uint8Array.from(hex.match(/.{2}/g).map((byte) => parseInt(byte, 16)));

test("lenient RINEX NAV parsing preserves records and skipped-block diagnostics", () => {
  const lines = splitlines(fixtureText(NAV_FIXTURE));
  const firstGps = lines.findIndex((line) => line.startsWith("G01"));
  assert.notEqual(firstGps, -1);

  // Clear the first record's fixed-width af0 field. The core parser should
  // drop this block and retain the other supported records.
  lines[firstGps] = `${lines[firstGps].slice(0, 23)}${" ".repeat(19)}${lines[firstGps].slice(42)}`;
  const parsed = parseRinexNavLenient(new TextEncoder().encode(`${lines.join("\n")}\n`));

  assert.equal(parsed.recordCount, 2_215);
  assert.equal(parsed.skippedCount, 1);
  assert.equal(parsed.records.length, parsed.recordCount);
  assert.equal(parsed.skipped.length, parsed.skippedCount);
  assert.equal(parsed.skipped[0].satellite, "G01");
  assert.equal(parsed.skipped[0].message, "bad/missing af0 field in record for G01");
});

test("lenient NAV parsing and arbitrary-list encoding retain JS errors and ownership", () => {
  assert.throws(() => parseRinexNavLenient(Uint8Array.from([0xff])), TypeError);
  assert.throws(() => parseRinexNavLenient(new TextEncoder().encode("not a RINEX NAV")), Error);

  const records = parseRinexNavRecords(fixture(NAV_FIXTURE));
  const firstSatellite = records[0].satellite;
  const encoded = encodeRinexNav([records[0]]);
  const reparsed = parseRinexNavLenient(new TextEncoder().encode(encoded));
  assert.equal(reparsed.recordCount, 1);
  assert.equal(reparsed.skippedCount, 0);
  assert.equal(reparsed.records[0].satellite, firstSatellite);
  assert.throws(() => encodeRinexNav([{}]), Error);
});

test("RINEX observation-code mappings are direct and version-aware", () => {
  assert.equal(rinexObservationFrequencyHz(GnssSystem.BeiDou, "C1I", 3.02), 1_561_098_000);
  assert.equal(rinexObservationFrequencyHz(GnssSystem.BeiDou, "C1I", 3.03), 1_575_420_000);
  assert.equal(rinexObservationFrequencyHz(GnssSystem.Glonass, "L1C", 3.04, 1), 1_602_562_500);
  assert.equal(rinexObservationFrequencyHz(GnssSystem.Gps, "C9X", 3.04), undefined);
  assert.equal(rinexObservationFrequencyHz(GnssSystem.Glonass, "L1C", 3.04), undefined);

  assert.equal(
    f64Bits(rinexObservationWavelengthM(GnssSystem.Gps, "L1C", 3.04)),
    0x3fc85b8b06a70079n,
  );
  assert.equal(
    f64Bits(rinexObservationWavelengthM(GnssSystem.BeiDou, "C1I", 3.02)),
    0x3fc894bff89f23b5n,
  );
});

test("EMS and RTKLIB parsers return timestamped, decodable engine blocks", () => {
  const ems = parseSbasEmsLines(`ignored\n120,26,7,1,0,0,1,1,${SBAS_HEX}\n`);
  assert.equal(ems.length, 1);
  assert.equal(ems[0].satellite, "S20");
  assert.equal(ems[0].satelliteId, "S20");
  assert.equal(ems[0].week, 2_425);
  assert.equal(ems[0].towS, 259_201);
  assert.equal(ems[0].form, "body226");
  assert.deepEqual(Array.from(ems[0].bytes), Array.from(hexToBytes(SBAS_HEX)));
  assert.equal(ems[0].decode().messageType, 2);

  const rtklib = parseSbasRtklibLines(`ignored\n2360 259200 120 1 : ${SBAS_HEX}\n`);
  assert.equal(rtklib.length, 1);
  assert.equal(rtklib[0].satelliteId, "S20");
  assert.equal(rtklib[0].week, 2_360);
  assert.equal(rtklib[0].towS, 259_200);
  assert.equal(rtklib[0].form, "body226");
  assert.equal(rtklib[0].decode().messageType, 2);

  assert.deepEqual(parseSbasEmsLines("ignored\n"), []);
  assert.deepEqual(parseSbasRtklibLines("ignored\n"), []);
});

test("SBAS text parser errors remain engine errors", () => {
  assert.throws(() => parseSbasEmsLines("120,26,7,1,0,0,1,1,00"), /invalid SBAS hex block length/);
  assert.throws(
    () => parseSbasRtklibLines("2360 259200 120 1 : 00"),
    /invalid SBAS hex block length/,
  );
});
