// WASM parity for the accepted RINEX NAV, RINEX observation-code, and SBAS
// text-log subsets. Values are frozen from the public sidereon-core 1.1.1
// behavior and use the binding's public conversion/error surface.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  decodeSbasMessage,
  encodeSbasMessage,
  encodeRinexNav,
  GnssSystem,
  parseRinexNavLenient,
  parseRinexNavRecords,
  parseSbasEmsLines,
  parseSbasEmsLog,
  parseSbasRtklibLines,
  parseSbasRtklibLog,
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
  // The message type field states 2, the type SBAS_HEX carries at message
  // bits 8-13.
  const ems = parseSbasEmsLines(`ignored\n120,26,7,1,0,0,1,2,${SBAS_HEX}\n`);
  assert.equal(ems.length, 1);
  assert.equal(ems[0].satellite, "S20");
  assert.equal(ems[0].satelliteId, "S20");
  assert.equal(ems[0].week, 2_425);
  assert.equal(ems[0].towS, 259_201);
  assert.equal(ems[0].form, "body226");
  assert.deepEqual(Array.from(ems[0].bytes), Array.from(hexToBytes(SBAS_HEX)));
  assert.equal(ems[0].decode().messageType, 2);
  assert.equal(ems[0].declaredMessageType, 2);
  assert.equal(ems[0].messageType, 2);

  const rtklib = parseSbasRtklibLines(`ignored\n2360 259200 120 2 : ${SBAS_HEX}\n`);
  assert.equal(rtklib.length, 1);
  assert.equal(rtklib[0].satelliteId, "S20");
  assert.equal(rtklib[0].week, 2_360);
  assert.equal(rtklib[0].towS, 259_200);
  assert.equal(rtklib[0].form, "body226");
  assert.equal(rtklib[0].decode().messageType, 2);

  assert.deepEqual(parseSbasEmsLines("ignored\n"), []);
  assert.deepEqual(parseSbasRtklibLines("ignored\n"), []);
});

test("a declared message type that differs from the carried one is refused strict and reported lenient", () => {
  const text = `# comment\n\n120,26,7,1,0,0,1,1,${SBAS_HEX}\n`;
  assert.throws(
    () => parseSbasEmsLines(text),
    /declares SBAS message type 1 but its message carries type 2/,
  );
  assert.throws(() => parseSbasEmsLog(text), /declares SBAS message type 1/);

  const log = parseSbasEmsLog(text, { policy: "lenient" });
  assert.equal(log.blocks.length, 1);
  assert.equal(log.blocks[0].declaredMessageType, 1);
  assert.equal(log.blocks[0].messageType, 2);
  assert.deepEqual(log.skippedLines, [
    { line: 1, kind: "comment" },
    { line: 2, kind: "blank" },
  ]);
  assert.deepEqual(log.refusedLines, []);
  assert.equal(log.departures.length, 1);
  assert.equal(log.departures[0].kind, "declaredMessageType");
  assert.equal(log.departures[0].declared, 1);
  assert.equal(log.departures[0].carried, 2);
  assert.equal(log.departures[0].line, 3);

  const rtk = parseSbasRtklibLog(`2360 259200 120 1 : ${SBAS_HEX}\n`, { policy: "lenient" });
  assert.equal(rtk.blocks.length, 1);
  assert.equal(rtk.departures[0].kind, "declaredMessageType");
  assert.throws(() => parseSbasEmsLog(text, { polcy: "lenient" }), TypeError);
  assert.throws(() => parseSbasEmsLog(text, { policy: "loose" }), TypeError);
});

test("decodeSbasMessage reports the pad bits and refuses an unknown preamble unless lenient", () => {
  const bytes = hexToBytes(SBAS_HEX);
  const decoded = decodeSbasMessage(bytes, "body226");
  assert.equal(typeof decoded.padBits, "number");
  assert.deepEqual(decoded.departures, []);
  const altered = Uint8Array.from(bytes);
  altered[0] = 0x11;
  assert.throws(() => decodeSbasMessage(altered, "body226"), Error);
  const lenient = decodeSbasMessage(altered, "body226", "lenient");
  assert.equal(lenient.departures[0].kind, "unrecognizedPreamble");
  assert.equal(lenient.departures[0].preamble, 0x11);
});

test("encodeSbasMessage exposes the typed strict preamble refusal and lenient departure", () => {
  const altered = hexToBytes(SBAS_HEX);
  altered[0] = 0x11;

  assert.throws(
    () => encodeSbasMessage(altered, "body226"),
    (error) => {
      assert.equal(error.name, "SbasEncodeError");
      assert.deepEqual(error.detail, {
        kind: "SBAS_ENCODE",
        core: { kind: "unrecognizedPreamble", preamble: 0x11 },
        message: "SBAS encode error: SBAS preamble 0x11 is not 0x53, 0x9A or 0xC6",
      });
      return true;
    },
  );

  const lenient = encodeSbasMessage(altered, "body226", "lenient");
  assert.deepEqual(Array.from(lenient.bytes), Array.from(altered));
  assert.deepEqual(lenient.departures, [
    {
      kind: "unrecognizedPreamble",
      message: "SBAS preamble 0x11 is not 0x53, 0x9A or 0xC6",
      preamble: 0x11,
      declared: null,
      carried: null,
      line: null,
    },
  ]);
});

test("SBAS text parser errors remain engine errors", () => {
  assert.throws(() => parseSbasEmsLines("120,26,7,1,0,0,1,1,00"), /invalid SBAS hex block length/);
  assert.throws(
    () => parseSbasRtklibLines("2360 259200 120 1 : 00"),
    /invalid SBAS hex block length/,
  );
});
