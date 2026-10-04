// SP3 and ANTEX writers refuse, with a typed detail, a product whose fixed
// columns cannot restate a value it holds, instead of rounding or dropping it.
// Each text is one the core's own tests read and refuse to write the same way.

import { test } from "node:test";
import assert from "node:assert/strict";

import { loadAntex, loadSp3 } from "../pkg-node/sidereon.js";

import { fixture } from "./helpers.mjs";

const encoder = new TextEncoder();

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

const SP3C_FILE = [
  "#cP2020  6 24  0  0  0.00000000       2 ORBIT IGS14 FIT  TST",
  "## 2111 432000.00000000   900.00000000 59024 0.0000000000000",
  "+    2   G01G02  0  0  0  0  0  0  0  0  0  0  0  0  0  0  0",
  "++         0  0  0  0  0  0  0  0  0  0  0  0  0  0  0  0  0",
  "%c G  cc GPS ccc cccc cccc cccc cccc ccccc ccccc ccccc ccccc",
  "%c cc cc ccc ccc cccc cccc cccc cccc ccccc ccccc ccccc ccccc",
  "%f  1.2500000  1.025000000  0.00000000000  0.000000000000000",
  "%f  0.0000000  0.000000000  0.00000000000  0.000000000000000",
  "%i    0    0    0    0      0      0      0      0         0",
  "%i    0    0    0    0      0      0      0      0         0",
  "/* TEST SP3-c FIXTURE",
  "*  2020  6 24  0  0  0.00000000",
  "PG01  15000.000000 -20000.000000   5000.000000    123.456789",
  "PG02  -1234.567890   2345.678901  -3456.789012 999999.999999",
  "*  2020  6 24  0 15  0.00000000",
  "PG01  15100.000000 -20100.000000   5100.000000   -987.654321              E",
  "PG02      0.000000      0.000000      0.000000    100.000000",
  "EOF",
  "",
].join("\n");

test("an SP3 product the reader kept but its columns cannot restate is refused by name", () => {
  // The fixture itself writes.
  assert.equal(typeof loadSp3(encoder.encode(SP3C_FILE)).toSp3String(), "string");

  // A %f base finer than the canonical F10.7 column reads back exactly from
  // the source bytes, so the reader keeps it; the writer cannot restate it.
  const fine = loadSp3(encoder.encode(SP3C_FILE.replace(" 1.2500000", "1.25000001")));
  const precision = thrown(() => fine.toSp3String(), "Sp3WriteError");
  assert.deepEqual(precision.detail, {
    kind: "PRECISION_NOT_REPRESENTABLE",
    field: "pos/vel base",
    columns: 10,
    decimals: 7,
    value: 1.25000001,
    message: precision.message,
  });

  // A non-finite start is kept for exact validation to report; no column
  // states it.
  const nanStart = loadSp3(
    encoder.encode(
      [
        "#cP2020  6 24  0  0  0.00000000       1 ORBIT IGS14 FIT  TST",
        "## 2111             NaN   900.00000000 59024 0.0000000000000",
        "+    1   G01  0  0  0  0  0  0  0  0  0  0  0  0  0  0  0  0",
        "++         5  0  0  0  0  0  0  0  0  0  0  0  0  0  0  0  0",
        "%c G  cc GPS ccc cccc cccc cccc cccc ccccc ccccc ccccc ccccc",
        "%c cc cc ccc ccc cccc cccc cccc cccc ccccc ccccc ccccc ccccc",
        "%f  1.2500000  1.025000000  0.00000000000  0.000000000000000",
        "%f  0.0000000  0.000000000  0.00000000000  0.000000000000000",
        "%i    0    0    0    0      0      0      0      0         0",
        "%i    0    0    0    0      0      0      0      0         0",
        "/* TEST SP3-c FIXTURE",
        "*  2020  6 24  0  0  0.00000000",
        "PG01      1.000000      2.000000      3.000000    123.456789",
        "EOF",
        "",
      ].join("\n"),
    ),
  );
  const nonFinite = thrown(() => nanStart.toSp3String(), "Sp3WriteError");
  assert.deepEqual(nonFinite.detail, {
    kind: "NON_FINITE",
    field: "seconds-of-week",
    message: nonFinite.message,
  });
});

const antexLine = (prefix, label) => `${prefix.padEnd(60)}${label}`;
const antexBlock = (dazi) =>
  [
    antexLine("", "START OF ANTENNA"),
    antexLine("TESTANT             TESTSER", "TYPE / SERIAL NO"),
    antexLine(dazi, "DAZI"),
    antexLine("     0.0  10.0   5.0", "ZEN1 / ZEN2 / DZEN"),
    antexLine("IGS_TEST", "SINEX CODE"),
    antexLine("  2020     1     1     0     0    0.0000000", "VALID FROM"),
    antexLine("  2021    12    31    23    59   59.0000000", "VALID UNTIL"),
    antexLine("G01", "START OF FREQUENCY"),
    antexLine("      1.50      2.00      3.00", "NORTH / EAST / UP"),
    "   NOAZI    1.00    2.00    3.00",
    "     0.0    1.00    2.00    3.00",
    "    90.0    4.00    5.00    6.00",
    antexLine("", "END OF FREQUENCY"),
    antexLine("", "END OF ANTENNA"),
  ].join("\n");

test("an ANTEX value its fixed column cannot restate is refused as UNWRITABLE", () => {
  const plain = loadAntex(encoder.encode(antexBlock("     0.0")));
  assert.equal(typeof plain.toAntexString(), "string");

  // DAZI is F6.1, which cannot hold 5.25.
  const fine = loadAntex(encoder.encode(antexBlock("    5.25")));
  assert.equal(fine.antenna("TESTANT             TESTSER").daziDeg, 5.25);
  const error = thrown(() => fine.toAntexString(), "AntexWriteError");
  assert.equal(error.detail.kind, "UNWRITABLE");
  assert.equal(error.detail.field, "dazi_deg");
  assert.match(error.detail.reason, /precision/);

  // A real product still writes and reads back.
  const antex = loadAntex(fixture("antex/igs20_wettzell_trim.atx"));
  assert.equal(typeof antex.toAntexString(), "string");
});
