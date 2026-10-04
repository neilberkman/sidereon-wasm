// CCSDS TDM contracts the WASM binding carries from the core: positioned
// comments kept through a round trip, metadata built and replaced from ordered
// raw fields with every derived property following them, reader and writer
// policies that return each forgiven departure, and typed refusals.

import { test } from "node:test";
import assert from "node:assert/strict";

import { TdmField, TdmMetadata, parseTdmKvn, parseTdmKvnWithPolicy } from "../pkg-node/sidereon.js";

import { fixtureText } from "./helpers.mjs";

const ANNEX = "tdm/annex_e_01.kvn";

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

const plainFields = (metadata) =>
  metadata.fields.map((field) => ({ key: field.key, value: field.value }));

test("comments carry their positions and are written back where they were read", () => {
  const tdm = parseTdmKvn(fixtureText(ANNEX));
  assert.deepEqual(tdm.comments, [
    { text: "TDM example created by yyyyy-nnnA Nav Team (NASA/JPL)", beforeRecord: 1 },
    { text: "StarTrek 1-way data, Ka band down", beforeRecord: 1 },
  ]);
  const segment = tdm.segments[0];
  assert.deepEqual(segment.metadata.comments, [
    { text: "Data quality degraded by antenna pointing problem...", beforeRecord: 0 },
    { text: "Slightly noisy data", beforeRecord: 0 },
  ]);
  assert.deepEqual(segment.data.comments, [
    { text: "TRANSMIT_FREQ_2 is spacecraft reference downlink", beforeRecord: 0 },
  ]);

  const encoded = tdm.toKvnString();
  const reparsed = parseTdmKvn(encoded);
  assert.deepEqual(reparsed.comments, tdm.comments);
  assert.deepEqual(reparsed.segments[0].metadata.comments, segment.metadata.comments);
  assert.deepEqual(reparsed.segments[0].data.comments, segment.data.comments);
});

test("metadata is built from ordered raw fields and replaced atomically", () => {
  const initial = TdmMetadata.fromRaw(
    [
      { key: "TIME_SYSTEM", value: "UTC" },
      new TdmField("PARTICIPANT_1", "DSS-14"),
      { key: "MODE", value: "SEQUENTIAL" },
    ],
    [{ text: "initial", beforeRecord: 0 }],
  );
  assert.equal(initial.timeSystem, "UTC");
  assert.equal(initial.mode, "SEQUENTIAL");
  assert.deepEqual(
    initial.participants.map((participant) => [participant.index, participant.name]),
    [[1, "DSS-14"]],
  );
  assert.deepEqual(initial.comments, [{ text: "initial", beforeRecord: 0 }]);

  // A candidate without a participant is refused and changes nothing.
  const refused = thrown(
    () => initial.replaceRaw([{ key: "TIME_SYSTEM", value: "UTC" }]),
    "TdmValidationError",
  );
  assert.deepEqual(refused.detail, {
    kind: "MISSING_KEYWORD",
    keyword: "PARTICIPANT_n",
    segment: 1,
    message: refused.message,
  });
  assert.deepEqual(
    initial.fields.map((field) => field.key),
    ["TIME_SYSTEM", "PARTICIPANT_1", "MODE"],
  );
  assert.equal(initial.timeSystem, "UTC");
  assert.deepEqual(initial.comments, [{ text: "initial", beforeRecord: 0 }]);

  // A valid candidate replaces the fields, the comments and every derived
  // property together.
  const replacement = [
    { key: "TIME_SYSTEM", value: "TAI" },
    { key: "PARTICIPANT_1", value: "PARKES" },
    { key: "PARTICIPANT_2", value: "GOLDSTONE" },
    { key: "MODE", value: "SINGLE_DIRECTION" },
    { key: "PATH", value: "1,2" },
    { key: "TIMETAG_REF", value: "RECEIVE" },
    { key: "RANGE_UNITS", value: "RU" },
  ];
  initial.replaceRaw(replacement, [{ text: "replaced", beforeRecord: 0 }]);
  assert.deepEqual(plainFields(initial), replacement);
  assert.deepEqual(initial.comments, [{ text: "replaced", beforeRecord: 0 }]);
  assert.equal(initial.timeSystem, "TAI");
  assert.deepEqual(
    initial.participants.map((participant) => [participant.index, participant.name]),
    [
      [1, "PARKES"],
      [2, "GOLDSTONE"],
    ],
  );
  assert.equal(initial.mode, "SINGLE_DIRECTION");
  assert.deepEqual(
    initial.paths.map((path) => [path.key, path.index, Array.from(path.participants)]),
    [["PATH", undefined, [1, 2]]],
  );
  assert.equal(initial.timetagRef, "RECEIVE");
  assert.equal(initial.rangeUnits, "RU");
});

test("setField and removeField change one keyword and keep the table order", () => {
  const meta = TdmMetadata.fromRaw(
    [
      { key: "TIME_SYSTEM", value: "UTC" },
      { key: "PARTICIPANT_1", value: "DSS-14" },
      { key: "MODE", value: "SEQUENTIAL" },
    ],
    // A metadata comment belongs at the beginning of its section (TDM 4.5.2).
    [{ text: "metadata note", beforeRecord: 0 }],
  );

  // A stated keyword takes the new value in place.
  meta.setField("MODE", "SINGLE_DIRECTION");
  assert.equal(meta.mode, "SINGLE_DIRECTION");
  assert.deepEqual(
    meta.fields.map((field) => field.key),
    ["TIME_SYSTEM", "PARTICIPANT_1", "MODE"],
  );

  // An absent keyword goes where tables 3-2 and 3-3 put it: START_TIME
  // after TIME_SYSTEM, TIMETAG_REF after MODE. The comment stays first.
  meta.setField("START_TIME", "2005-160T20:15:00");
  meta.setField("TIMETAG_REF", "RECEIVE");
  assert.deepEqual(plainFields(meta), [
    { key: "TIME_SYSTEM", value: "UTC" },
    { key: "START_TIME", value: "2005-160T20:15:00" },
    { key: "PARTICIPANT_1", value: "DSS-14" },
    { key: "MODE", value: "SINGLE_DIRECTION" },
    { key: "TIMETAG_REF", value: "RECEIVE" },
  ]);
  assert.deepEqual(meta.comments, [{ text: "metadata note", beforeRecord: 0 }]);
  assert.equal(meta.timetagRef, "RECEIVE");

  // Removing a keyword keeps the leading comment where it is.
  meta.removeField("START_TIME");
  assert.deepEqual(
    meta.fields.map((field) => field.key),
    ["TIME_SYSTEM", "PARTICIPANT_1", "MODE", "TIMETAG_REF"],
  );
  assert.deepEqual(meta.comments, [{ text: "metadata note", beforeRecord: 0 }]);
  meta.removeField("NOT_STATED");
  assert.equal(meta.fields.length, 4);

  // A keyword outside the metadata table, and a removal that leaves the block
  // without its time system, are refused and change nothing.
  thrown(() => meta.setField("NOT_A_KEYWORD", "x"), "TdmValidationError");
  const missing = thrown(() => meta.removeField("TIME_SYSTEM"), "TdmValidationError");
  assert.equal(missing.detail.kind, "MISSING_KEYWORD");
  assert.equal(meta.timeSystem, "UTC");
  assert.equal(meta.fields.length, 4);
});

test("comment positions are checked and kept, with every forgiven departure returned", () => {
  const base = [
    { key: "TIME_SYSTEM", value: "UTC" },
    { key: "PARTICIPANT_1", value: "DSS-14" },
  ];
  const outOfBounds = thrown(
    () => TdmMetadata.fromRaw(base, [{ text: "out of bounds", beforeRecord: 3 }]),
    "TdmValidationError",
  );
  assert.deepEqual(outOfBounds.detail, {
    kind: "UNWRITABLE",
    keyword: "COMMENT",
    reason: "comment position is out of bounds",
    message: outOfBounds.message,
  });
  const descending = thrown(
    () =>
      TdmMetadata.fromRawWithPolicy(
        base,
        [
          { text: "first", beforeRecord: 1 },
          { text: "second", beforeRecord: 0 },
        ],
        { keywordOrder: "forgive" },
      ),
    "TdmValidationError",
  );
  assert.equal(descending.detail.reason, "comment order cannot be emitted unchanged");

  // A comment after every field is out of table order: refused under the
  // strict policy, kept with a departure when keyword order is forgiven.
  const atEnd = [{ text: "at end of metadata", beforeRecord: 2 }];
  const strict = thrown(() => TdmMetadata.fromRaw(base, atEnd), "TdmValidationError");
  assert.deepEqual(strict.detail, {
    kind: "KEYWORD_OUT_OF_ORDER",
    line: null,
    keyword: "COMMENT",
    section: "metadata",
    message: strict.message,
  });
  const forgiven = TdmMetadata.fromRawWithPolicy(base, atEnd, { keywordOrder: "forgive" });
  assert.equal(forgiven.value, forgiven.metadata);
  assert.deepEqual(forgiven.metadata.comments, atEnd);
  assert.equal(forgiven.departures.length, 1);
  assert.equal(forgiven.departures[0].kind, "KEYWORD_OUT_OF_ORDER");
  assert.equal(forgiven.departures[0].keyword, "COMMENT");
  assert.equal(forgiven.departures[0].section, "metadata");
  assert.equal(typeof forgiven.departures[0].message, "string");

  // The binding reads a position exactly and refuses a misspelt property.
  assert.throws(() => TdmMetadata.fromRaw(base, [{ text: "x", beforeRecord: -1 }]), RangeError);
  assert.throws(() => TdmMetadata.fromRaw(base, [{ text: "x", beforeRecord: 0.5 }]), RangeError);
  assert.throws(() => TdmMetadata.fromRaw(base, [{ text: "x", beforeRecord: "0" }]), TypeError);
  assert.throws(() => TdmMetadata.fromRaw(base, [{ text: "x", before_record: 0 }]), TypeError);
  assert.throws(
    () => TdmMetadata.fromRaw([...base, { key: "MODE", value: "SEQUENTIAL", note: 1 }]),
    TypeError,
  );
  assert.throws(() => TdmMetadata.fromRaw("TIME_SYSTEM = UTC"), TypeError);
});

test("replaced metadata is written into the message at its position", () => {
  const tdm = parseTdmKvn(fixtureText(ANNEX));
  const metadata = tdm.segments[0].metadata;
  const fields = plainFields(metadata);
  assert.equal(fields.length, 11);
  const comments = [...metadata.comments, { text: "after every field", beforeRecord: 11 }];

  assert.throws(
    () => metadata.replaceRaw(fields, comments),
    (error) => {
      return error.name === "TdmValidationError" && error.detail.kind === "KEYWORD_OUT_OF_ORDER";
    },
  );
  const departures = metadata.replaceRawWithPolicy(fields, comments, { keywordOrder: "forgive" });
  assert.deepEqual(
    departures.map((departure) => [departure.kind, departure.keyword, departure.section]),
    [["KEYWORD_OUT_OF_ORDER", "COMMENT", "metadata"]],
  );
  assert.deepEqual(metadata.comments, comments);

  tdm.setSegmentMetadata(0, metadata);
  assert.deepEqual(tdm.segments[0].metadata.comments, comments);
  assert.throws(() => tdm.setSegmentMetadata(1, metadata), RangeError);
  assert.throws(() => tdm.setSegmentMetadata(-1, metadata), RangeError);

  // The strict writer refuses the comment's place rather than moving it.
  const refused = thrown(() => tdm.toKvnString(), "TdmWriteError");
  assert.equal(refused.detail.kind, "KEYWORD_OUT_OF_ORDER");
  assert.equal(refused.detail.keyword, "COMMENT");
  assert.equal(refused.detail.section, "metadata");

  const written = tdm.toKvnStringWithPolicy({ keywordOrder: "forgive" });
  assert.equal(written.value, written.text);
  assert.deepEqual(
    written.departures.map((departure) => [departure.kind, departure.keyword, departure.section]),
    [["KEYWORD_OUT_OF_ORDER", "COMMENT", "metadata"]],
  );
  const reread = parseTdmKvnWithPolicy(written.text, { keywordOrder: "forgive" });
  assert.equal(reread.value, reread.tdm);
  assert.deepEqual(
    reread.warnings.map((warning) => [
      warning.kind,
      warning.line,
      warning.keyword,
      warning.section,
    ]),
    [["KEYWORD_OUT_OF_ORDER", 20, "COMMENT", "metadata"]],
  );
  assert.deepEqual(reread.tdm.segments[0].metadata.comments, comments);
});

test("reader and writer policies return what they forgave; strict refusals are typed", () => {
  const text = fixtureText(ANNEX);
  assert.ok(text.endsWith("DATA_STOP\n"));
  const unterminated = text.slice(0, -1);
  const lastLine = unterminated.split("\n").length;

  const refused = thrown(() => parseTdmKvn(unterminated), "TdmParseError");
  assert.deepEqual(refused.detail, {
    kind: "UNTERMINATED_FINAL_LINE",
    line: lastLine,
    message: refused.message,
  });

  const forgiven = parseTdmKvnWithPolicy(unterminated, { finalTerminator: "forgive" });
  assert.deepEqual(
    forgiven.warnings.map((warning) => [warning.kind, warning.line]),
    [["UNTERMINATED_FINAL_LINE", lastLine]],
  );
  assert.deepEqual(parseTdmKvnWithPolicy(text, "lenient").warnings, []);
  assert.deepEqual(parseTdmKvnWithPolicy(text).warnings, []);

  // The writer terminates every line unless asked for the departure.
  const tdm = forgiven.tdm;
  assert.ok(tdm.toKvnString().endsWith("DATA_STOP\n"));
  const departed = tdm.toKvnStringWithPolicy({ finalTerminator: "forgive" });
  assert.ok(departed.text.endsWith("DATA_STOP"));
  assert.deepEqual(
    departed.departures.map((departure) => departure.kind),
    ["UNTERMINATED_FINAL_LINE"],
  );
  assert.deepEqual(tdm.toKvnStringWithPolicy("strict").departures, []);

  // A policy that cannot be read is refused rather than replaced by a default.
  assert.throws(() => parseTdmKvnWithPolicy(text, { finalTerminators: "forgive" }), TypeError);
  assert.throws(() => parseTdmKvnWithPolicy(text, { finalTerminator: "yes" }), TypeError);
  assert.throws(() => parseTdmKvnWithPolicy(text, { repeatedKeywords: "forgive" }), TypeError);
  assert.throws(() => parseTdmKvnWithPolicy(text, "permissive"), TypeError);
  assert.throws(() => tdm.toKvnStringWithPolicy(7), TypeError);
});
