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

test("OPM KVN exposes every header and typed-block getter", () => {
  const fixture = `CCSDS_OPM_VERS = 2.0
COMMENT header note
CLASSIFICATION = UNCLASSIFIED
CREATION_DATE = 2026-06-28T00:00:00
ORIGINATOR = SIDEREON
MESSAGE_ID = OPM-COMPLETE
COMMENT metadata note
OBJECT_NAME = SAT
OBJECT_ID = 2026-001A
CENTER_NAME = EARTH
REF_FRAME = EME2000
REF_FRAME_EPOCH = 2000-01-01T12:00:00
TIME_SYSTEM = UTC
COMMENT state note
EPOCH = 2026-06-28T00:00:00
X = 7000
Y = 0
Z = 0
X_DOT = 0
Y_DOT = 7.5
Z_DOT = 1
COMMENT keplerian note
SEMI_MAJOR_AXIS = 7000
ECCENTRICITY = 0.001
INCLINATION = 51.6
RA_OF_ASC_NODE = 120
ARG_OF_PERICENTER = 90
TRUE_ANOMALY = 42
GM = 398600.4418
COMMENT spacecraft note
MASS = 425
SOLAR_RAD_AREA = 12.5
SOLAR_RAD_COEFF = 1.7
DRAG_AREA = 8.25
DRAG_COEFF = 2.2
COMMENT covariance note
COV_REF_FRAME = RTN
CX_X = 1
CY_X = 2
CY_Y = 3
CZ_X = 4
CZ_Y = 5
CZ_Z = 6
CX_DOT_X = 7
CX_DOT_Y = 8
CX_DOT_Z = 9
CX_DOT_X_DOT = 10
CY_DOT_X = 11
CY_DOT_Y = 12
CY_DOT_Z = 13
CY_DOT_X_DOT = 14
CY_DOT_Y_DOT = 15
CZ_DOT_X = 16
CZ_DOT_Y = 17
CZ_DOT_Z = 18
CZ_DOT_X_DOT = 19
CZ_DOT_Y_DOT = 20
CZ_DOT_Z_DOT = 21
MAN_EPOCH_IGNITION = 2026-06-28T00:10:00
COMMENT maneuver note
MAN_DURATION = 10
MAN_DELTA_MASS = -0.5
MAN_REF_FRAME = TNW
MAN_DV_1 = 0.001
MAN_DV_2 = 0
MAN_DV_3 = 0
COMMENT user-defined note
USER_DEFINED_OWNER = SIDEREON
USER_DEFINED_PURPOSE = COVERAGE
`;
  const opm = parseOpmKvn(fixture);
  assert.equal(opm.ccsdsOpmVers, "2.0");
  assert.deepEqual(opm.comments, ["header note"]);
  assert.equal(opm.classification, "UNCLASSIFIED");
  assert.equal(opm.creationDate, "2026-06-28T00:00:00");
  assert.equal(opm.originator, "SIDEREON");
  assert.equal(opm.messageId, "OPM-COMPLETE");
  assert.deepEqual(opm.userDefinedComments, ["user-defined note"]);
  assert.deepEqual(opm.userDefined, [
    { parameter: "OWNER", value: "SIDEREON" },
    { parameter: "PURPOSE", value: "COVERAGE" },
  ]);

  assert.deepEqual(
    {
      comments: opm.metadata.comments,
      objectName: opm.metadata.objectName,
      objectId: opm.metadata.objectId,
      centerName: opm.metadata.centerName,
      refFrame: opm.metadata.refFrame,
      refFrameEpoch: opm.metadata.refFrameEpoch,
      timeSystem: opm.metadata.timeSystem,
    },
    {
      comments: ["metadata note"],
      objectName: "SAT",
      objectId: "2026-001A",
      centerName: "EARTH",
      refFrame: "EME2000",
      refFrameEpoch: "2000-01-01T12:00:00",
      timeSystem: "UTC",
    },
  );
  assert.deepEqual(
    {
      comments: opm.state.comments,
      epoch: opm.state.epoch,
      positionKm: Array.from(opm.state.positionKm),
      velocityKmS: Array.from(opm.state.velocityKmS),
    },
    {
      comments: ["state note"],
      epoch: "2026-06-28T00:00:00",
      positionKm: [7000, 0, 0],
      velocityKmS: [0, 7.5, 1],
    },
  );
  assert.deepEqual(
    {
      comments: opm.keplerian.comments,
      semiMajorAxisKm: opm.keplerian.semiMajorAxisKm,
      eccentricity: opm.keplerian.eccentricity,
      inclinationDeg: opm.keplerian.inclinationDeg,
      raOfAscNodeDeg: opm.keplerian.raOfAscNodeDeg,
      argOfPericenterDeg: opm.keplerian.argOfPericenterDeg,
      trueAnomalyDeg: opm.keplerian.trueAnomalyDeg,
      meanAnomalyDeg: opm.keplerian.meanAnomalyDeg,
      gmKm3S2: opm.keplerian.gmKm3S2,
    },
    {
      comments: ["keplerian note"],
      semiMajorAxisKm: 7000,
      eccentricity: 0.001,
      inclinationDeg: 51.6,
      raOfAscNodeDeg: 120,
      argOfPericenterDeg: 90,
      trueAnomalyDeg: 42,
      meanAnomalyDeg: undefined,
      gmKm3S2: 398600.4418,
    },
  );
  assert.deepEqual(
    {
      comments: opm.spacecraft.comments,
      massKg: opm.spacecraft.massKg,
      solarRadAreaM2: opm.spacecraft.solarRadAreaM2,
      solarRadCoeff: opm.spacecraft.solarRadCoeff,
      dragAreaM2: opm.spacecraft.dragAreaM2,
      dragCoeff: opm.spacecraft.dragCoeff,
    },
    {
      comments: ["spacecraft note"],
      massKg: 425,
      solarRadAreaM2: 12.5,
      solarRadCoeff: 1.7,
      dragAreaM2: 8.25,
      dragCoeff: 2.2,
    },
  );
  const lowerTriangle = Array.from({ length: 21 }, (_, index) => index + 1);
  assert.deepEqual(
    {
      comments: opm.covariance.comments,
      covRefFrame: opm.covariance.covRefFrame,
      lowerTriangle: Array.from(opm.covariance.lowerTriangle),
      matrix: Array.from(opm.covariance.matrix),
    },
    {
      comments: ["covariance note"],
      covRefFrame: "RTN",
      lowerTriangle,
      matrix: [
        1, 2, 4, 7, 11, 16, 2, 3, 5, 8, 12, 17, 4, 5, 6, 9, 13, 18, 7, 8, 9, 10, 14, 19, 11, 12, 13,
        14, 15, 20, 16, 17, 18, 19, 20, 21,
      ],
    },
  );
  assert.equal(opm.maneuvers.length, 1);
  assert.deepEqual(
    {
      comments: opm.maneuvers[0].comments,
      epochIgnition: opm.maneuvers[0].epochIgnition,
      durationS: opm.maneuvers[0].durationS,
      deltaMassKg: opm.maneuvers[0].deltaMassKg,
      refFrame: opm.maneuvers[0].refFrame,
      dvKmS: Array.from(opm.maneuvers[0].dvKmS),
    },
    {
      comments: ["maneuver note"],
      epochIgnition: "2026-06-28T00:10:00",
      durationS: 10,
      deltaMassKg: -0.5,
      refFrame: "TNW",
      dvKmS: [0.001, 0, 0],
    },
  );
  assert.equal(parseOpmKvn(opm.toKvnString()).toKvnString(), opm.toKvnString());
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

test("OEM KVN exposes every header, segment, state and covariance getter", () => {
  const fixture = `CCSDS_OEM_VERS = 2.0
COMMENT header note
CLASSIFICATION = UNCLASSIFIED
CREATION_DATE = 2026-06-28T00:00:00
ORIGINATOR = SIDEREON
MESSAGE_ID = OEM-COMPLETE
META_START
COMMENT metadata note
OBJECT_NAME = SAT
OBJECT_ID = 2026-001A
CENTER_NAME = EARTH
REF_FRAME = EME2000
REF_FRAME_EPOCH = 2000-01-01T12:00:00
TIME_SYSTEM = UTC
START_TIME = 2026-06-28T00:00:00
USEABLE_START_TIME = 2026-06-28T00:00:30
USEABLE_STOP_TIME = 2026-06-28T00:09:30
STOP_TIME = 2026-06-28T00:10:00
INTERPOLATION = LAGRANGE
INTERPOLATION_DEGREE = 5
META_STOP
COMMENT before first state
2026-06-28T00:00:00 1 2 3 0.1 0.2 0.3 0.01 0.02 0.03
COMMENT between states
2026-06-28T00:10:00 4 5 6 0.4 0.5 0.6
COVARIANCE_START
COMMENT before covariance
EPOCH = 2026-06-28T00:00:00
COV_REF_FRAME = RTN
CX_X = 1
CY_X = 2
CY_Y = 3
CZ_X = 4
CZ_Y = 5
CZ_Z = 6
CX_DOT_X = 7
CX_DOT_Y = 8
CX_DOT_Z = 9
CX_DOT_X_DOT = 10
CY_DOT_X = 11
CY_DOT_Y = 12
CY_DOT_Z = 13
CY_DOT_X_DOT = 14
CY_DOT_Y_DOT = 15
CZ_DOT_X = 16
CZ_DOT_Y = 17
CZ_DOT_Z = 18
CZ_DOT_X_DOT = 19
CZ_DOT_Y_DOT = 20
CZ_DOT_Z_DOT = 21
EPOCH = 2026-06-28T00:10:00
COMMENT between covariance matrices
COV_REF_FRAME = RTN
CX_X = 101
CY_X = 102
CY_Y = 103
CZ_X = 104
CZ_Y = 105
CZ_Z = 106
CX_DOT_X = 107
CX_DOT_Y = 108
CX_DOT_Z = 109
CX_DOT_X_DOT = 110
CY_DOT_X = 111
CY_DOT_Y = 112
CY_DOT_Z = 113
CY_DOT_X_DOT = 114
CY_DOT_Y_DOT = 115
CZ_DOT_X = 116
CZ_DOT_Y = 117
CZ_DOT_Z = 118
CZ_DOT_X_DOT = 119
CZ_DOT_Y_DOT = 120
CZ_DOT_Z_DOT = 121
COVARIANCE_STOP
`;
  const oem = parseOemKvn(fixture);
  assert.equal(oem.ccsdsOemVers, "2.0");
  assert.deepEqual(oem.comments, ["header note"]);
  assert.equal(oem.classification, "UNCLASSIFIED");
  assert.equal(oem.creationDate, "2026-06-28T00:00:00");
  assert.equal(oem.originator, "SIDEREON");
  assert.equal(oem.messageId, "OEM-COMPLETE");
  assert.equal(oem.segmentCount, 1);
  assert.deepEqual(oem.skippedStates, []);
  assert.equal(oem.skippedStateCount, 0);

  const [segment] = oem.segments;
  assert.deepEqual(
    {
      dataComments: segment.dataComments,
      covarianceComments: segment.covarianceComments,
      states: segment.states.map((state) => ({
        epoch: state.epoch,
        positionKm: Array.from(state.positionKm),
        velocityKmS: Array.from(state.velocityKmS),
        accelerationKmS2: state.accelerationKmS2 && Array.from(state.accelerationKmS2),
      })),
    },
    {
      dataComments: [
        { position: 0, text: "before first state" },
        { position: 1, text: "between states" },
      ],
      covarianceComments: [
        { position: 0, text: "before covariance" },
        { position: 1, text: "between covariance matrices" },
      ],
      states: [
        {
          epoch: "2026-06-28T00:00:00",
          positionKm: [1, 2, 3],
          velocityKmS: [0.1, 0.2, 0.3],
          accelerationKmS2: [0.01, 0.02, 0.03],
        },
        {
          epoch: "2026-06-28T00:10:00",
          positionKm: [4, 5, 6],
          velocityKmS: [0.4, 0.5, 0.6],
          accelerationKmS2: undefined,
        },
      ],
    },
  );
  assert.deepEqual(
    {
      comments: segment.metadata.comments,
      objectName: segment.metadata.objectName,
      objectId: segment.metadata.objectId,
      centerName: segment.metadata.centerName,
      refFrame: segment.metadata.refFrame,
      refFrameEpoch: segment.metadata.refFrameEpoch,
      timeSystem: segment.metadata.timeSystem,
      startTime: segment.metadata.startTime,
      stopTime: segment.metadata.stopTime,
      useableStartTime: segment.metadata.useableStartTime,
      useableStopTime: segment.metadata.useableStopTime,
      interpolation: segment.metadata.interpolation,
      interpolationDegree: segment.metadata.interpolationDegree,
    },
    {
      comments: ["metadata note"],
      objectName: "SAT",
      objectId: "2026-001A",
      centerName: "EARTH",
      refFrame: "EME2000",
      refFrameEpoch: "2000-01-01T12:00:00",
      timeSystem: "UTC",
      startTime: "2026-06-28T00:00:00",
      stopTime: "2026-06-28T00:10:00",
      useableStartTime: "2026-06-28T00:00:30",
      useableStopTime: "2026-06-28T00:09:30",
      interpolation: "LAGRANGE",
      interpolationDegree: 5,
    },
  );
  assert.deepEqual(
    segment.covariances.map((covariance) => ({
      epoch: covariance.epoch,
      covRefFrame: covariance.covRefFrame,
      lowerTriangle: Array.from(covariance.lowerTriangle),
      matrix: Array.from(covariance.matrix),
    })),
    [
      {
        epoch: "2026-06-28T00:00:00",
        covRefFrame: "RTN",
        lowerTriangle: Array.from({ length: 21 }, (_, index) => index + 1),
        matrix: [
          1, 2, 4, 7, 11, 16, 2, 3, 5, 8, 12, 17, 4, 5, 6, 9, 13, 18, 7, 8, 9, 10, 14, 19, 11, 12,
          13, 14, 15, 20, 16, 17, 18, 19, 20, 21,
        ],
      },
      {
        epoch: "2026-06-28T00:10:00",
        covRefFrame: "RTN",
        lowerTriangle: Array.from({ length: 21 }, (_, index) => index + 101),
        matrix: [
          101, 102, 104, 107, 111, 116, 102, 103, 105, 108, 112, 117, 104, 105, 106, 109, 113, 118,
          107, 108, 109, 110, 114, 119, 111, 112, 113, 114, 115, 120, 116, 117, 118, 119, 120, 121,
        ],
      },
    ],
  );
  assert.equal(parseOemKvn(oem.toKvnString()).toKvnString(), oem.toKvnString());
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
