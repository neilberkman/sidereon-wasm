import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  ExactSp3Coverage,
  ExactSp3Request,
  parseExactSp3,
  productIdentity,
  validateExactSp3,
} from "../pkg-node/sidereon.js";

const encoder = new TextEncoder();
const P_G01 = "PG01  15000.000000 -20000.000000   5000.000000    123.456789\n";
const P_G02 = "PG02  16000.000000 -21000.000000   6000.000000    124.456789\n";
const terminalRecordCorpus = JSON.parse(
  readFileSync(new URL("./fixtures/sp3-terminal-record-v1.json", import.meta.url), "utf8"),
);

function regularOffsets(count, cadenceSeconds = 300) {
  return Array.from({ length: count }, (_, index) => index * cadenceSeconds);
}

function exactSp3At(
  offsets,
  declaredCount,
  headerCadence,
  { year, month, day, gnssWeek, secondsOfWeek, mjd, agency },
) {
  const dt = `${String(year).padStart(4)} ${String(month).padStart(2)} ${String(day).padStart(2)}  0  0  0.00000000`;
  let text = `#dP${dt} ${String(declaredCount).padStart(7)} ${"ORBIT".padEnd(5)}${"IGS20".padStart(6)}${"FIT".padStart(4)} ${agency}\n`;
  text += `## ${String(gnssWeek).padStart(4)} ${secondsOfWeek.toFixed(8).padStart(15)} ${headerCadence.padStart(14)} ${String(mjd).padStart(5)} 0.0000000000000\n`;
  text += "+    2   G01G02" + "  0".repeat(15) + "\n";
  text += ("+        " + "  0".repeat(17) + "\n").repeat(4);
  text += ("++       " + "  0".repeat(17) + "\n").repeat(5);
  text += "%c M  cc GPS ccc cccc cccc cccc cccc ccccc ccccc ccccc ccccc\n";
  text += "%c cc cc ccc ccc cccc cccc cccc cccc ccccc ccccc ccccc ccccc\n";
  text += "%f  1.2500000  1.025000000  0.00000000000  0.000000000000000\n";
  text += "%f  0.0000000  0.000000000  0.00000000000  0.000000000000000\n";
  text += "%i    0    0    0    0      0      0      0      0         0\n";
  text += "%i    0    0    0    0      0      0      0      0         0\n";
  text += "/* EXACT VALIDATION TEST FIXTURE\n".repeat(4);

  for (const offset of offsets) {
    const dayOffset = Math.floor(offset / 86400);
    const secondOfDay = offset % 86400;
    const hour = Math.floor(secondOfDay / 3600);
    const minute = Math.floor((secondOfDay % 3600) / 60);
    const second = secondOfDay % 60;
    text += `*  ${String(year).padStart(4)} ${String(month).padStart(2)} ${String(day + dayOffset).padStart(2)} ${String(hour).padStart(2)} ${String(minute).padStart(2)} ${second.toFixed(8).padStart(11)}\n`;
    text += P_G01 + P_G02;
  }
  return text + "EOF\n";
}

function exactSp3(offsets, declaredCount = offsets.length, headerCadence = "300.00000000") {
  return exactSp3At(offsets, declaredCount, headerCadence, {
    year: 2020,
    month: 1,
    day: 1,
    gnssWeek: 2086,
    secondsOfWeek: 259200,
    mjd: 58849,
    agency: "TST",
  });
}

function request() {
  return new ExactSp3Request(2020, 1, 1, "01D", "05M", "0000");
}

function hexBytes(value) {
  assert.equal(value.length % 2, 0, `odd-length fixture hex ${value}`);
  return Uint8Array.from(value.match(/../g)?.map((pair) => Number.parseInt(pair, 16)) ?? []);
}

function terminalCaseBytes(base, terminalCase) {
  assert.ok(base.endsWith("EOF\n"));
  const prefix = encoder.encode(base.slice(0, -4));
  const marker =
    terminalCase.marker === null ? new Uint8Array() : encoder.encode(terminalCase.marker);
  const padding = new Uint8Array(terminalCase.padding_spaces).fill(0x20);
  const components = [
    prefix,
    hexBytes(terminalCase.leading_hex),
    marker,
    padding,
    hexBytes(terminalCase.suffix_hex),
    hexBytes(terminalCase.separator_hex),
    hexBytes(terminalCase.trailing_hex),
  ];
  const bytes = new Uint8Array(components.reduce((length, item) => length + item.length, 0));
  let offset = 0;
  for (const item of components) {
    bytes.set(item, offset);
    offset += item.length;
  }
  return bytes;
}

function terminalResultClass(bytes) {
  try {
    parseExactSp3(bytes, request());
    return "accept";
  } catch (error) {
    const message = String(error?.message ?? error);
    if (/malformed EOF record/.test(message)) return "malformed_eof_record";
    if (/missing its EOF record/.test(message)) return "missing_eof";
    if (/nonblank records after EOF/.test(message)) return "trailing_content_after_eof";
    assert.fail(`terminal corpus reached unrelated exact error: ${message}`);
  }
}

test("exact SP3 obeys the shared terminal-record contract", () => {
  assert.equal(terminalRecordCorpus.schema, "sidereon-sp3-terminal-record-v1");
  assert.equal(terminalRecordCorpus.record_width, 80);
  assert.equal(terminalRecordCorpus.record_width_authority, "sidereon-interoperability-policy");

  const base = exactSp3(regularOffsets(288));
  for (const terminalCase of terminalRecordCorpus.cases) {
    assert.equal(
      terminalResultClass(terminalCaseBytes(base, terminalCase)),
      terminalCase.expect,
      terminalCase.name,
    );
  }
});

test("exact SP3 accepts half-open and inclusive daily five-minute grids", () => {
  const halfOpen = parseExactSp3(encoder.encode(exactSp3(regularOffsets(288))), request());
  assert.equal(halfOpen.coverage, ExactSp3Coverage.HalfOpen);
  assert.equal(halfOpen.product.epochCount, 288);
  assert.equal(halfOpen.product.declaredEpochCount, 288);
  assert.equal(typeof halfOpen.product.declaredStartJ2000Seconds, "number");
  assert.equal(validateExactSp3(halfOpen.product, request()), ExactSp3Coverage.HalfOpen);

  const inclusive = parseExactSp3(encoder.encode(exactSp3(regularOffsets(289))), request());
  assert.equal(inclusive.coverage, ExactSp3Coverage.Inclusive);
  assert.equal(inclusive.product.epochCount, 289);
});

test("exact SP3 rejects short, irregular, cadence-invalid, and unsupported requests", () => {
  assert.throws(
    () => parseExactSp3(encoder.encode(exactSp3(regularOffsets(287))), request()),
    /span mismatch/,
  );
  assert.throws(
    () => parseExactSp3(encoder.encode(exactSp3(regularOffsets(290))), request()),
    /span mismatch/,
  );

  const irregular = regularOffsets(288);
  irregular[100] += 1;
  assert.throws(() => parseExactSp3(encoder.encode(exactSp3(irregular)), request()), /irregular/);
  assert.throws(
    () =>
      parseExactSp3(encoder.encode(exactSp3(regularOffsets(288), 288, "0.00000000")), request()),
    /positive/,
  );
  assert.throws(
    () => parseExactSp3(encoder.encode(exactSp3(regularOffsets(288), 288, "NaN")), request()),
    /finite/,
  );
  assert.throws(() => new ExactSp3Request(2020, 1, 1, "01D", "60M", "0000"), /noncanonical/);
  assert.throws(() => new ExactSp3Request(2020, 1, 1, "01D", "00U", "0000"), /unsupported/);
});

test("exact SP3 requests can bind a complete catalog identity", () => {
  const identity = productIdentity("igs", "sp3", 2026, 7, 19);
  const exact = ExactSp3Request.fromIdentity(identity);
  assert.equal(exact.span, identity.span);
  assert.equal(exact.sample, identity.sample);
  assert.equal(exact.expectedAgency, "IGS");
  assert.throws(() => exact.requireAgency("bad"), /agency/);
});

test("historical GFZ identity applies its cataloged content start across a GPS week", () => {
  const identity = productIdentity("gfz_ult", "sp3", 2022, 9, 4, "05M", "0000");
  const catalogRequest = ExactSp3Request.fromIdentity(identity);
  const bytes = encoder.encode(
    exactSp3At(regularOffsets(576), 576, "300.00000000", {
      year: 2022,
      month: 9,
      day: 3,
      gnssWeek: 2225,
      secondsOfWeek: 518400,
      mjd: 59825,
      agency: "GFZ",
    }),
  );

  assert.equal(parseExactSp3(bytes, catalogRequest).coverage, ExactSp3Coverage.HalfOpen);

  const literalFilenameEpoch = new ExactSp3Request(2022, 9, 4, "02D", "05M", "0000");
  assert.throws(() => parseExactSp3(bytes, literalFilenameEpoch), /start mismatch/);
});
