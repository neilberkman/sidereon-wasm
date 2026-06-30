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
  // The two-token middle line is skipped and counted, not fatal.
  assert.equal(oem.skippedStates, 1);
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
  assert.equal(parsed.skippedStates, 0);
  const pseg = parsed.segments[0];
  assert.equal(pseg.metadata.interpolationDegree, 5);
  assert.equal(pseg.covariances[0].covRefFrame, "RTN");
  assert.equal(pseg.covariances[0].matrix[0], 1);
  assert.equal(pseg.covariances[0].matrix[35], 6e-6);
});

test("an OEM covariance must be positive semidefinite", () => {
  const bad = new Float64Array(36);
  bad[0] = -1;
  assert.throws(() => new OemCovariance("e", bad, undefined), RangeError);
});

test("constructing an OEM with no segments throws", () => {
  assert.throws(() => new Oem([], undefined), TypeError);
});
