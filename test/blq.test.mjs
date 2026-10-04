// BLQ ocean-loading coefficient blocks through the WASM binding: the reader
// keeps every comment and column-order header with its placement, the writer
// restates them and refuses by name a block it could not read back unchanged.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  parseOceanLoadingBlqBlock,
  parseOceanLoadingBlqBlocks,
  writeOceanLoadingBlqBlock,
  writeOceanLoadingBlqBlocks,
} from "../pkg-node/sidereon.js";

const AMPLITUDES = [
  [
    0.00385, 0.00129, 0.00082, 0.00035, 0.00329, 0.00212, 0.00108, 0.00031, 0.00047, 0.00033,
    0.00031,
  ],
  [
    0.00117, 0.00045, 0.00024, 0.00012, 0.00077, 0.00055, 0.00025, 0.00008, 0.00013, 0.00005,
    0.00004,
  ],
  [
    0.00061, 0.00019, 0.00016, 0.00005, 0.00037, 0.0003, 0.00012, 0.00004, 0.00006, 0.00003,
    0.00002,
  ],
];
const PHASES = [
  [-54.6, -25.3, -72.6, -29.0, 55.9, -6.9, 54.5, -26.9, -16.6, -2.3, 1.5],
  [91.4, 126.6, 76.7, 126.0, 68.1, 36.4, 62.3, 30.4, -170.7, 172.7, 176.5],
  [-59.4, -21.2, -78.1, -15.6, 142.6, 101.7, 145.2, 98.0, -178.8, -179.2, -179.8],
];
const ORDER_HEADER = "$$ COLUMN ORDER:  M2  S2  N2  K2  K1  O1  P1  Q1  MF  MM SSA";
const row = (values, decimals) => values.map((v) => v.toFixed(decimals).padStart(7)).join(" ");

const TEXT = [
  "$$ Ocean loading displacement",
  "$$",
  ORDER_HEADER,
  "$$",
  "  ONSA",
  "$$ Onsala, before the first row",
  ...AMPLITUDES.map((values) => `  ${row(values, 5)}`),
  ...PHASES.map((values) => `  ${row(values, 1)}`),
  "$$ END TABLE",
  "",
].join("\n");

const COMMENTS = [
  { placement: "beforeStation", row: null, line: "$$ Ocean loading displacement" },
  { placement: "beforeStation", row: null, line: "$$" },
  { placement: "beforeStation", row: null, line: ORDER_HEADER },
  { placement: "beforeStation", row: null, line: "$$" },
  { placement: "beforeRow", row: 0, line: "$$ Onsala, before the first row" },
  { placement: "afterRows", row: null, line: "$$ END TABLE" },
];

function assertWriteRefused(call, expected) {
  assert.throws(call, (err) => {
    assert.equal(err.name, "BlqWriteError");
    assert.deepEqual({ ...err.detail, message: undefined }, { ...expected, message: undefined });
    assert.equal(err.detail.message, err.message);
    return true;
  });
}

test("a BLQ block keeps its coefficients and every comment with its placement", () => {
  const block = parseOceanLoadingBlqBlock(TEXT);
  assert.equal(block.station, "ONSA");
  assert.deepEqual(block.amplitudeM, AMPLITUDES);
  assert.deepEqual(block.phaseDeg, PHASES);
  assert.deepEqual(block.comments, COMMENTS);
  assert.deepEqual(parseOceanLoadingBlqBlocks(TEXT), [block]);
});

test("the writer restates the comments and reads back to an equal block", () => {
  const block = parseOceanLoadingBlqBlock(TEXT);
  const text = writeOceanLoadingBlqBlock(block);
  const lines = text.split("\n");
  // Comments at their placements; the station line from the third column.
  assert.deepEqual(lines.slice(0, 6), [
    "$$ Ocean loading displacement",
    "$$",
    ORDER_HEADER,
    "$$",
    "  ONSA",
    "$$ Onsala, before the first row",
  ]);
  assert.equal(lines[12], "$$ END TABLE");
  assert.deepEqual(parseOceanLoadingBlqBlock(text), block);
  assert.equal(writeOceanLoadingBlqBlock(parseOceanLoadingBlqBlock(text)), text);

  // Comments may be omitted from a block to write.
  const bare = writeOceanLoadingBlqBlock({
    station: "ONSA",
    amplitudeM: AMPLITUDES,
    phaseDeg: PHASES,
  });
  assert.equal(bare.split("\n")[0], "  ONSA");
  assert.deepEqual(parseOceanLoadingBlqBlock(bare).comments, []);
});

test("several blocks write as one file that reads back block for block", () => {
  const onsa = parseOceanLoadingBlqBlock(TEXT);
  const first = { ...onsa, comments: onsa.comments.filter((c) => c.placement !== "afterRows") };
  const second = { station: "WTZR", amplitudeM: AMPLITUDES, phaseDeg: PHASES, comments: [] };
  const text = writeOceanLoadingBlqBlocks([first, second]);
  assert.deepEqual(parseOceanLoadingBlqBlocks(text), [first, second]);

  // A comment after the rows of any block but the last would read as part of
  // the next block.
  assertWriteRefused(() => writeOceanLoadingBlqBlocks([onsa, second]), {
    kind: "AFTER_ROWS_BEFORE_ANOTHER_BLOCK",
    index: 5,
    block: 0,
  });
});

test("the writer refuses by name what would not read back unchanged", () => {
  const block = parseOceanLoadingBlqBlock(TEXT);

  assertWriteRefused(() => writeOceanLoadingBlqBlock({ ...block, station: "" }), {
    kind: "EMPTY_STATION",
    block: 0,
  });

  const amplitudeM = AMPLITUDES.map((values) => [...values]);
  amplitudeM[1][4] = Number.NaN;
  assertWriteRefused(() => writeOceanLoadingBlqBlock({ ...block, amplitudeM }), {
    kind: "NON_FINITE_COEFFICIENT",
    row: 1,
    constituent: "K1",
    block: 0,
  });

  assertWriteRefused(
    () =>
      writeOceanLoadingBlqBlock({
        ...block,
        comments: [...block.comments, { placement: "beforeRow", row: 6, line: "$$ late" }],
      }),
    { kind: "COMMENT_PLACEMENT_OUT_OF_RANGE", index: 6, block: 0 },
  );

  const badHeader = block.comments.map((c) =>
    c.line === ORDER_HEADER ? { ...c, line: ORDER_HEADER.replace("SSA", " SA") } : c,
  );
  assertWriteRefused(() => writeOceanLoadingBlqBlock({ ...block, comments: badHeader }), {
    kind: "INVALID_HEADER",
    index: 2,
    header: { kind: "UNSUPPORTED_CONSTITUENT", constituent: "SA" },
    block: 0,
  });

  assertWriteRefused(
    () =>
      writeOceanLoadingBlqBlock({
        ...block,
        comments: [{ placement: "beforeStation", line: "no comment marker" }],
      }),
    { kind: "NOT_A_COMMENT_LINE", index: 0, block: 0 },
  );

  assertWriteRefused(
    () =>
      writeOceanLoadingBlqBlock({
        ...block,
        comments: [
          { placement: "afterRows", line: "$$ a" },
          { placement: "beforeStation", line: "$$ b" },
        ],
      }),
    { kind: "COMMENTS_OUT_OF_PLACEMENT_ORDER", index: 1, block: 0 },
  );
});

test("malformed block objects are TypeErrors", () => {
  const block = parseOceanLoadingBlqBlock(TEXT);
  assert.throws(
    () => writeOceanLoadingBlqBlock({ ...block, amplitudeM: AMPLITUDES.slice(0, 2) }),
    TypeError,
  );
  assert.throws(
    () => writeOceanLoadingBlqBlock({ ...block, phaseDeg: PHASES.map((r) => r.slice(1)) }),
    TypeError,
  );
  assert.throws(() => writeOceanLoadingBlqBlock({ ...block, stationName: "ONSA" }), TypeError);
  assert.throws(
    () =>
      writeOceanLoadingBlqBlock({
        ...block,
        comments: [{ placement: "beforeRow", line: "$$ missing row" }],
      }),
    TypeError,
  );
  assert.throws(
    () =>
      writeOceanLoadingBlqBlock({
        ...block,
        comments: [{ placement: "afterRows", row: 2, line: "$$ stray row" }],
      }),
    TypeError,
  );
  assert.throws(() => writeOceanLoadingBlqBlocks(block), TypeError);
});

test("the reader refuses a block that does not read, naming the line", () => {
  const truncated = TEXT.split("\n").slice(4, 8).join("\n");
  assert.throws(
    () => parseOceanLoadingBlqBlocks(truncated),
    (err) => {
      assert.equal(err.name, "BlqParseError");
      assert.deepEqual(
        { ...err.detail, message: undefined },
        {
          kind: "MISSING_COEFFICIENT_ROWS",
          station: "ONSA",
          expected: 6,
          found: 2,
          line: 1,
          message: undefined,
        },
      );
      return true;
    },
  );

  const block = parseOceanLoadingBlqBlock(TEXT);
  const twoBlocks = writeOceanLoadingBlqBlocks([
    { ...block, comments: [] },
    { ...block, station: "WTZR", comments: [] },
  ]);
  assert.throws(
    () => parseOceanLoadingBlqBlock(twoBlocks),
    (err) => {
      assert.equal(err.name, "BlqParseError");
      assert.equal(err.detail.kind, "MULTIPLE_BLOCKS");
      assert.equal(err.detail.found, 2);
      assert.equal(err.detail.line, 0);
      return true;
    },
  );
});
