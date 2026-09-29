// CCSDS OEM and OPM bindings reproduce the engine parse/encode surface: parse
// from KVN/XML, read the typed blocks, re-encode (byte-stable round-trip), and
// build a message from scratch that re-parses to itself.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  Oem,
  OemMetadata,
  OemSegment,
  OemState,
  OemCovariance,
  Opm,
  OpmMetadata,
  OpmState,
  OpmKeplerian,
  OpmSpacecraft,
  OpmManeuver,
  parseOemKvn,
  parseOemXml,
  parseOpmKvn,
  parseOpmXml,
} from "../pkg-node/sidereon.js";

const OPM_KVN = `CCSDS_OPM_VERS = 2.0
CREATION_DATE = 2026-06-28T00:00:00
ORIGINATOR = SIDEREON
OBJECT_NAME = OSPREY
OBJECT_ID = 2026-001A
CENTER_NAME = EARTH
REF_FRAME = EME2000
TIME_SYSTEM = UTC
EPOCH = 2026-06-28T00:00:00
X = 7000
Y = 0
Z = 0
X_DOT = 0
Y_DOT = 7.5
Z_DOT = 1
SEMI_MAJOR_AXIS = 7000
ECCENTRICITY = 0.001
INCLINATION = 51.6
RA_OF_ASC_NODE = 120
ARG_OF_PERICENTER = 90
TRUE_ANOMALY = 42
GM = 398600.4418
MASS = 425
MAN_EPOCH_IGNITION = 2026-06-28T00:10:00
MAN_DURATION = 10
MAN_DELTA_MASS = -0.5
MAN_REF_FRAME = TNW
MAN_DV_1 = 0.001
MAN_DV_2 = 0
MAN_DV_3 = 0
`;

const OEM_KVN = `CCSDS_OEM_VERS = 2.0
CREATION_DATE = 2026-06-28T00:00:00
ORIGINATOR = SIDEREON
META_START
OBJECT_NAME = TEST
OBJECT_ID = 2026-001A
CENTER_NAME = EARTH
REF_FRAME = EME2000
TIME_SYSTEM = UTC
START_TIME = 2026-06-28T00:00:00
STOP_TIME = 2026-06-28T00:10:00
INTERPOLATION = LAGRANGE
INTERPOLATION_DEGREE = 5
META_STOP
2026-06-28T00:00:00 1 2 3 0.1 0.2 0.3
2026-06-28T00:05:00 1 2
2026-06-28T00:10:00 4 5 6 0.4 0.5 0.6
`;

test("parse OPM KVN exposes the typed blocks and round-trips", () => {
  const opm = parseOpmKvn(OPM_KVN);
  assert.equal(opm.ccsdsOpmVers, "2.0");
  assert.equal(opm.originator, "SIDEREON");
  assert.equal(opm.metadata.objectName, "OSPREY");
  assert.equal(opm.metadata.objectId, "2026-001A");
  assert.equal(opm.state.epoch, "2026-06-28T00:00:00");
  assert.deepEqual(Array.from(opm.state.positionKm), [7000, 0, 0]);
  assert.deepEqual(Array.from(opm.state.velocityKmS), [0, 7.5, 1]);
  assert.equal(opm.keplerian.trueAnomalyDeg, 42);
  assert.equal(opm.keplerian.meanAnomalyDeg, undefined);
  assert.equal(opm.keplerian.gmKm3S2, 398600.4418);
  assert.equal(opm.spacecraft.massKg, 425);
  assert.equal(opm.covariance, undefined);
  assert.equal(opm.maneuvers.length, 1);
  assert.equal(opm.maneuvers[0].refFrame, "TNW");
  assert.deepEqual(Array.from(opm.maneuvers[0].dvKmS), [0.001, 0, 0]);

  // KVN -> object -> KVN -> object is byte-stable.
  const encoded = opm.toKvnString();
  assert.equal(parseOpmKvn(encoded).toKvnString(), encoded);
  // The XML encoding re-parses to the same orbital content.
  assert.equal(parseOpmXml(opm.toXmlString()).state.positionKm[0], 7000);
});

test("build an OPM from scratch and re-parse it", () => {
  const md = new OpmMetadata("SAT", "2026-9Z", "EARTH", "EME2000", "UTC");
  const st = new OpmState(
    "2026-06-28T00:00:00",
    Float64Array.from([7000, 0, 0]),
    Float64Array.from([0, 7.5, 1]),
  );
  const kep = new OpmKeplerian(7000, 0.001, 51.6, 120, 90, 398600.4418, undefined, 10);
  const sc = new OpmSpacecraft(500, undefined, undefined, undefined, 2.2);
  const man = new OpmManeuver(
    "2026-06-28T00:10:00",
    10,
    -0.5,
    "TNW",
    Float64Array.from([0.001, 0, 0]),
  );
  const opm = new Opm(md, st, kep, sc, undefined, [man], { originator: "BUILDER" });

  const parsed = parseOpmKvn(opm.toKvnString());
  assert.equal(parsed.originator, "BUILDER");
  assert.equal(parsed.metadata.objectName, "SAT");
  assert.equal(parsed.keplerian.meanAnomalyDeg, 10);
  assert.equal(parsed.keplerian.trueAnomalyDeg, undefined);
  assert.equal(parsed.spacecraft.massKg, 500);
  assert.equal(parsed.maneuvers.length, 1);
});

test("an OPM Keplerian block requires exactly one anomaly", () => {
  assert.throws(
    () => new OpmKeplerian(7000, 0.001, 51.6, 120, 90, 398600.4418, undefined, undefined),
    TypeError,
  );
  assert.throws(() => new OpmKeplerian(7000, 0.001, 51.6, 120, 90, 398600.4418, 42, 10), TypeError);
});

test("parse OEM KVN is forgiving and round-trips", () => {
  const oem = parseOemKvn(OEM_KVN);
  assert.equal(oem.ccsdsOemVers, "2.0");
  assert.equal(oem.segmentCount, 1);
  // The two-token middle line is skipped and reported, not fatal.
  assert.equal(oem.skippedStateCount, 1);
  assert.deepEqual(oem.skippedStates, [
    {
      line: 16,
      segment: 0,
      text: "2026-06-28T00:05:00 1 2",
      reason: "itemCount",
      itemCount: 3,
      field: null,
      issue: null,
    },
  ]);
  const seg = oem.segments[0];
  assert.equal(seg.metadata.objectName, "TEST");
  assert.equal(seg.metadata.interpolation, "LAGRANGE");
  assert.equal(seg.metadata.interpolationDegree, 5);
  assert.equal(seg.states.length, 2);
  assert.deepEqual(Array.from(seg.states[1].positionKm), [4, 5, 6]);

  const encoded = oem.toKvnString();
  assert.equal(parseOemKvn(encoded).toKvnString(), encoded);
  assert.equal(
    parseOemXml(oem.toXmlString()).segments[0].metadata.startTime,
    "2026-06-28T00:00:00",
  );
});

test("build an OEM with a covariance and re-parse it", () => {
  const md = new OemMetadata(
    "SAT",
    "2026-9Z",
    "EARTH",
    "EME2000",
    "UTC",
    "2026-06-28T00:00:00",
    "2026-06-28T00:10:00",
    { interpolation: "LAGRANGE", interpolationDegree: 5 },
  );
  const s0 = new OemState(
    "2026-06-28T00:00:00",
    Float64Array.from([1, 2, 3]),
    Float64Array.from([0.1, 0.2, 0.3]),
    undefined,
  );
  const diagonal = new Float64Array(36);
  [1, 2, 3, 4e-6, 5e-6, 6e-6].forEach((v, i) => {
    diagonal[i * 6 + i] = v;
  });
  const cov = new OemCovariance("2026-06-28T00:00:00", diagonal, "RTN");
  const seg = new OemSegment(md, [s0], [cov]);
  const oem = new Oem([seg], { originator: "BUILDER" });

  const parsed = parseOemKvn(oem.toKvnString());
  assert.equal(parsed.originator, "BUILDER");
  assert.deepEqual(parsed.skippedStates, []);
  assert.equal(parsed.skippedStateCount, 0);
  const pseg = parsed.segments[0];
  assert.equal(pseg.metadata.interpolationDegree, 5);
  assert.equal(pseg.covariances[0].covRefFrame, "RTN");
  assert.equal(pseg.covariances[0].matrix[0], 1);
  assert.equal(pseg.covariances[0].matrix[35], 6e-6);
});

test("an OEM covariance keeps its values as stated and validates on request", () => {
  // A message holds the matrix as printed; a matrix that is not positive
  // semidefinite is kept, and only toValidatedMatrix refuses it.
  const bad = new Float64Array(36);
  bad[0] = -1;
  const cov = new OemCovariance("e", bad, undefined);
  assert.equal(cov.lowerTriangle.length, 21);
  assert.equal(cov.lowerTriangle[0], -1);
  assert.equal(cov.matrix[0], -1);
  assert.throws(() => cov.toValidatedMatrix(), RangeError);

  // The 21 lower-triangle values build the same block as the full matrix.
  const lower = new Float64Array(21).map((_, i) => i + 1);
  const fromLower = new OemCovariance("e", lower, undefined);
  assert.deepEqual(Array.from(fromLower.lowerTriangle), Array.from(lower));
  assert.equal(fromLower.matrix[1], 2);
  assert.equal(fromLower.matrix[6], 2);
  const again = new OemCovariance("e", fromLower.matrix, undefined);
  assert.deepEqual(Array.from(again.lowerTriangle), Array.from(lower));

  // An asymmetric full matrix is refused: only one of the two values could be kept.
  const asym = new Float64Array(36);
  asym[1] = 1;
  assert.throws(() => new OemCovariance("e", asym, undefined), RangeError);
  assert.throws(() => new OemCovariance("e", new Float64Array(20), undefined), TypeError);
});

test("OEM and OPM retain header, metadata and data comments", () => {
  const kvn = `CCSDS_OEM_VERS = 2.0
COMMENT header note
CLASSIFICATION = UNCLASSIFIED
CREATION_DATE = 2026-06-28T00:00:00
ORIGINATOR = SIDEREON
MESSAGE_ID = OEM-1
META_START
COMMENT metadata note
OBJECT_NAME = TEST
OBJECT_ID = 2026-001A
CENTER_NAME = EARTH
REF_FRAME = EME2000
REF_FRAME_EPOCH = 2000-01-01T12:00:00
TIME_SYSTEM = UTC
START_TIME = 2026-06-28T00:00:00
STOP_TIME = 2026-06-28T00:10:00
META_STOP
COMMENT before the first state
2026-06-28T00:00:00 1 2 3 0.1 0.2 0.3
COMMENT between states
2026-06-28T00:10:00 4 5 6 0.4 0.5 0.6
`;
  const oem = parseOemKvn(kvn);
  assert.deepEqual(oem.comments, ["header note"]);
  assert.equal(oem.classification, "UNCLASSIFIED");
  assert.equal(oem.messageId, "OEM-1");
  const seg = oem.segments[0];
  assert.deepEqual(seg.metadata.comments, ["metadata note"]);
  assert.equal(seg.metadata.refFrameEpoch, "2000-01-01T12:00:00");
  assert.deepEqual(seg.dataComments, [
    { position: 0, text: "before the first state" },
    { position: 1, text: "between states" },
  ]);
  assert.equal(parseOemKvn(oem.toKvnString()).toKvnString(), oem.toKvnString());

  const md = new OemMetadata(
    "SAT",
    "2026-9Z",
    "EARTH",
    "EME2000",
    "UTC",
    "2026-06-28T00:00:00",
    "2026-06-28T00:10:00",
    { comments: ["built"], refFrameEpoch: "2000-01-01T12:00:00" },
  );
  // A segment takes ownership of the states it is given, so each segment gets
  // its own.
  const state0 = () =>
    new OemState(
      "2026-06-28T00:00:00",
      Float64Array.from([1, 2, 3]),
      Float64Array.from([0.1, 0.2, 0.3]),
      undefined,
    );
  const built = new Oem(
    [new OemSegment(md, [state0()], [], [{ position: 1, text: "after" }], [])],
    {
      comments: ["top"],
      messageId: "M",
    },
  );
  const round = parseOemKvn(built.toKvnString());
  assert.deepEqual(round.comments, ["top"]);
  assert.equal(round.messageId, "M");
  assert.deepEqual(round.segments[0].metadata.comments, ["built"]);
  assert.deepEqual(round.segments[0].dataComments, [{ position: 1, text: "after" }]);
  assert.throws(() => new Oem([new OemSegment(md, [state0()], [])], { originatr: "x" }), TypeError);

  const opmKvn = `${OPM_KVN.replace(
    "CCSDS_OPM_VERS = 2.0\n",
    "CCSDS_OPM_VERS = 2.0\nCOMMENT opm header\n",
  )
    .replace("ORIGINATOR = SIDEREON\n", "ORIGINATOR = SIDEREON\nMESSAGE_ID = OPM-1\n")
    .replace(
      "OBJECT_NAME = OSPREY\n",
      "COMMENT metadata\nOBJECT_NAME = OSPREY\n",
    )}USER_DEFINED_FOO = bar baz\n`;
  const opm = parseOpmKvn(opmKvn);
  assert.deepEqual(opm.comments, ["opm header"]);
  assert.equal(opm.messageId, "OPM-1");
  assert.deepEqual(opm.metadata.comments, ["metadata"]);
  assert.deepEqual(opm.userDefined, [{ parameter: "FOO", value: "bar baz" }]);
  assert.equal(parseOpmKvn(opm.toKvnString()).toKvnString(), opm.toKvnString());
});

test("OEM and OPM refuse with typed errors naming the field", () => {
  let caught;
  try {
    parseOpmKvn(OPM_KVN.replace("MASS = 425", "MASS = 425\nMASS = 426"));
  } catch (e) {
    caught = e;
  }
  assert.equal(caught.name, "OpmError");
  assert.equal(caught.detail.kind, "DUPLICATE_FIELD");
  assert.equal(caught.detail.field, "MASS");
  assert.equal(caught.detail.first, "425");
  assert.equal(caught.detail.second, "426");

  const md = new OpmMetadata("SAT\nX", "2026-9Z", "EARTH", "EME2000", "UTC");
  const st = new OpmState(
    "2026-06-28T00:00:00",
    Float64Array.from([7000, 0, 0]),
    Float64Array.from([0, 7.5, 1]),
  );
  const opm = new Opm(md, st, undefined, undefined, undefined, [], undefined);
  try {
    opm.toKvnString();
    assert.fail("expected a refusal");
  } catch (e) {
    assert.equal(e.name, "OpmError");
    assert.equal(e.detail.kind, "UNWRITABLE_TEXT");
    assert.equal(e.detail.field, "OBJECT_NAME");
    assert.equal(e.detail.issue, "lineBreak");
  }

  try {
    parseOemKvn(
      OEM_KVN.replace("INTERPOLATION = LAGRANGE", "INTERPOLATION = LAGRANGE\nBOGUS_KEY = 1"),
    );
    assert.fail("expected a refusal");
  } catch (e) {
    assert.equal(e.name, "OemError");
    assert.equal(e.detail.kind, "UNKNOWN_FIELD");
  }
});

test("constructing an OEM with no segments throws", () => {
  assert.throws(() => new Oem([], undefined), TypeError);
});
