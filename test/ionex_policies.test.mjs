// IONEX policy, header, warning, batch and standalone-grid surface.
//
// Every fixture here is written in the record grammar the engine's own reader
// takes: the data in columns 0..60 and the label at column 60, values in their
// I5 columns. Assertions are against what that reader produces, not against a
// shape this binding would like it to have.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  ionexFromNodeSamples,
  ionexFromSamples,
  ionexSlantDelayResults,
  loadIonex,
  loadIonexWithWarnings,
  IonexSlantPolicy,
  TecGrid,
  TecGridEpoch,
  TecGridEvalOptions,
  TecGridShellGeometry,
  tecXyz,
  tecXyzWithPolicy,
} from "../pkg-node/sidereon.js";

function ionexRecord(content, label) {
  return `${content.padEnd(60, " ")}${label.padEnd(20, " ")}\n`;
}

function i5Row(...values) {
  return values.map((value) => String(value).padStart(5, " ")).join("") + "\n";
}

function layoutHeader(mapCount, lastEpochStr, extraRecord = "") {
  return (
    ionexRecord("     1.0            I", "IONEX VERSION / TYPE") +
    ionexRecord("  2020     1     1     0     0     0", "EPOCH OF FIRST MAP") +
    ionexRecord(lastEpochStr, "EPOCH OF LAST MAP") +
    ionexRecord("  3600", "INTERVAL") +
    ionexRecord(String(mapCount).padStart(6, " "), "# OF MAPS IN FILE") +
    ionexRecord("   450.0 450.0   0.0", "HGT1 / HGT2 / DHGT") +
    ionexRecord("     1.0   0.0  -1.0", "LAT1 / LAT2 / DLAT") +
    ionexRecord("     0.0   1.0   1.0", "LON1 / LON2 / DLON") +
    extraRecord +
    ionexRecord("  6371.0", "BASE RADIUS") +
    ionexRecord("     2", "MAP DIMENSION") +
    ionexRecord("", "END OF HEADER")
  );
}

function layoutMap(kind, mapNo, epochStr, bands) {
  return (
    ionexRecord(String(mapNo).padStart(6, " "), `START OF ${kind} MAP`) +
    ionexRecord(epochStr, "EPOCH OF CURRENT MAP") +
    bands +
    ionexRecord(String(mapNo).padStart(6, " "), `END OF ${kind} MAP`)
  );
}

function layoutBands(northRow, southRow) {
  return (
    ionexRecord("     1.0   0.0   1.0   1.0 450.0", "LAT/LON1/LON2/DLON/H") +
    northRow +
    ionexRecord("     0.0   0.0   1.0   1.0 450.0", "LAT/LON1/LON2/DLON/H") +
    southRow
  );
}

function layoutEnd() {
  return ionexRecord("", "END OF FILE");
}

const EPOCH_0 = "  2020     1     1     0     0     0"; // 631108800 J2000 s
const EPOCH_1 = "  2020     1     1     1     0     0"; // 631112400 J2000 s
const EPOCH_0_S = 631108800;
const EPOCH_1_S = 631112400;
const L1_HZ = 1575.42e6;

const EXPONENT_0 = ionexRecord("     0", "EXPONENT");

// Every descriptive header record, so a supplied header can be checked field by
// field. The two mapping records are left to each test.
function baseHeader(overrides = {}) {
  return {
    version: 1.1,
    satelliteSystem: "MIX",
    program: "SIDEREON",
    runBy: "TESTAGENCY",
    date: "2026-09-22 12:00:00",
    descriptions: ["Line 1 description", "Line 2 description"],
    comments: ["Header comment A", "Header comment B"],
    intervalS: 3600,
    elevationCutoffDeg: 7.5,
    observablesUsed: "GPS L1/L2",
    stationCount: 50,
    satelliteCount: 31,
    mapsInFile: 1,
    ...overrides,
  };
}

const ONE_MAP_AXES = {
  mapEpochsJ2000S: [EPOCH_0_S],
  latNodesDeg: [1.0, 0.0],
  lonNodesDeg: [0.0, 1.0],
  dlatDeg: -1.0,
  dlonDeg: 1.0,
  shellHeightKm: 450.0,
  baseRadiusKm: 6371.0,
  exponent: 0,
};

// The same 2x2 single-map grid as `ONE_MAP_AXES`, one record per node, in the
// order `tecSamples()` emits: latitude 1.0 before 0.0, longitude ascending. A
// fresh array each call, so a test can rewrite one row.
function nodeSamples2x2() {
  return [
    [1.0, 0.0, 10.0],
    [1.0, 1.0, 20.0],
    [0.0, 0.0, 30.0],
    [0.0, 1.0, 40.0],
  ].map(([latDeg, lonDeg, vtecTecu]) => ({
    epochJ2000S: EPOCH_0_S,
    latDeg,
    lonDeg,
    vtecTecu,
    rmsTecu: vtecTecu / 10.0,
    heightOffsetKm: 0.0,
  }));
}

// A zenith query from inside the grid's latitude span, on the longitude-0 node
// column, which holds values in every fixture here. A receiver on the latitude-0
// edge would not do: the pierce-point recipe forms psi = pi/2 - E - asin(s), and
// at E = pi/2 the double cos(E) is about 6.1e-17, not 0, so the pierce point of a
// zenith ray lies about 3.3e-15 degrees south of the receiver, outside a grid
// whose southern node row is latitude 0. Longitude stays exactly 0: at azimuth 0
// the longitude offset is asin(-0.0), so no node of the other column is weighted.
function request(overrides = {}) {
  return {
    latDeg: 0.25,
    lonDeg: 0.0,
    azimuthDeg: 0.0,
    elevationDeg: 90.0,
    epochJ2000S: EPOCH_0_S,
    frequencyHz: L1_HZ,
    ...overrides,
  };
}

// --- Nullable grid cells ----------------------------------------------------

test("nullable maps round-trip, and an absent cube stays distinct from an all-null one", () => {
  const tec = [
    [
      [10.0, null],
      [null, 40.0],
    ],
  ];

  const ionexAbsent = ionexFromSamples({
    ...ONE_MAP_AXES,
    tecMaps: tec,
    rmsMaps: null,
    heightMaps: null,
  });
  assert.equal(ionexAbsent.hasRms, false);
  assert.equal(ionexAbsent.hasHeight, false);
  assert.equal(ionexAbsent.rmsMaps, null);
  assert.equal(ionexAbsent.heightMaps, null);
  assert.equal(ionexAbsent.tecMaps[0][0][1], null);
  assert.equal(ionexAbsent.tecMaps[0][0][0], 10.0);

  const extractedAbsent = ionexAbsent.tecGridSamples();
  assert.equal(extractedAbsent.rmsMaps, null);
  assert.equal(extractedAbsent.heightMaps, null);
  assert.equal(extractedAbsent.tecMaps[0][0][1], null);

  const allNullCube = [
    [
      [null, null],
      [null, null],
    ],
  ];
  const ionexAllNull = ionexFromSamples({
    ...ONE_MAP_AXES,
    tecMaps: tec,
    rmsMaps: allNullCube,
    heightMaps: allNullCube,
  });
  assert.equal(ionexAllNull.hasRms, true);
  assert.equal(ionexAllNull.hasHeight, true);
  assert.notEqual(ionexAllNull.rmsMaps, null);
  assert.notEqual(ionexAllNull.heightMaps, null);
  assert.equal(ionexAllNull.rmsMaps[0][0][0], null);
  assert.equal(ionexAllNull.heightMaps[0][0][0], null);

  const extractedAllNull = ionexAllNull.tecGridSamples();
  assert.notEqual(extractedAllNull.rmsMaps, null);
  assert.notEqual(extractedAllNull.heightMaps, null);
  assert.equal(extractedAllNull.rmsMaps[0][0][0], null);

  const nodeSamples = [
    {
      epochJ2000S: EPOCH_0_S,
      latDeg: 1.0,
      lonDeg: 0.0,
      vtecTecu: 10.0,
      rmsTecu: 1.5,
      heightOffsetKm: 0.0,
    },
    {
      epochJ2000S: EPOCH_0_S,
      latDeg: 1.0,
      lonDeg: 1.0,
      vtecTecu: null,
      rmsTecu: null,
      heightOffsetKm: null,
    },
    {
      epochJ2000S: EPOCH_0_S,
      latDeg: 0.0,
      lonDeg: 0.0,
      vtecTecu: null,
      rmsTecu: 2.5,
      heightOffsetKm: 10.0,
    },
    {
      epochJ2000S: EPOCH_0_S,
      latDeg: 0.0,
      lonDeg: 1.0,
      vtecTecu: 40.0,
      rmsTecu: null,
      heightOffsetKm: null,
    },
  ];

  const ionexNodes = ionexFromNodeSamples(nodeSamples, 450.0, 6371.0, 0);
  assert.equal(ionexNodes.hasRms, true);
  assert.equal(ionexNodes.hasHeight, true);
  // No header was supplied, so nothing is invented for the mapping record.
  assert.equal(ionexNodes.header.mappingFunction, null);
  assert.equal(ionexNodes.header.mappingDeclaration.kind, "ABSENT");

  const outSamples = ionexNodes.tecSamples();
  assert.equal(outSamples.length, 4);
  const s0 = outSamples.find((s) => s.latDeg === 1.0 && s.lonDeg === 0.0);
  assert.equal(s0.vtecTecu, 10.0);
  assert.equal(s0.rmsTecu, 1.5);
  assert.equal(s0.heightOffsetKm, 0.0);
  const s1 = outSamples.find((s) => s.latDeg === 1.0 && s.lonDeg === 1.0);
  assert.equal(s1.vtecTecu, null);
  assert.equal(s1.rmsTecu, null);
  assert.equal(s1.heightOffsetKm, null);
});

// --- Header -----------------------------------------------------------------

test("a complete header round-trips, and a raw OTHER code keeps its exact text", () => {
  const cosz = { kind: "COSZ", code: "COSZ" };
  const ionex = ionexFromSamples({
    ...ONE_MAP_AXES,
    tecMaps: [
      [
        [10.0, 20.0],
        [30.0, 40.0],
      ],
    ],
    header: baseHeader({
      mappingFunction: cosz,
      mappingDeclaration: { kind: "DECLARED", function: cosz },
    }),
  });

  const header = ionex.header;
  assert.equal(header.version, 1.1);
  assert.equal(header.satelliteSystem, "MIX");
  assert.equal(header.program, "SIDEREON");
  assert.equal(header.runBy, "TESTAGENCY");
  assert.equal(header.date, "2026-09-22 12:00:00");
  assert.deepEqual(header.descriptions, ["Line 1 description", "Line 2 description"]);
  assert.deepEqual(header.comments, ["Header comment A", "Header comment B"]);
  assert.equal(header.intervalS, 3600);
  assert.equal(header.elevationCutoffDeg, 7.5);
  assert.equal(header.observablesUsed, "GPS L1/L2");
  assert.equal(header.stationCount, 50);
  assert.equal(header.satelliteCount, 31);
  assert.equal(header.mapsInFile, 1);
  assert.equal(header.mappingFunction.kind, "COSZ");
  assert.equal(header.mappingDeclaration.kind, "DECLARED");
  assert.equal(header.mappingDeclaration.function.code, "COSZ");

  const reparsed = loadIonex(Buffer.from(ionex.toIonexString(), "utf8"));
  const roundTripped = reparsed.header;
  assert.equal(roundTripped.version, 1.1);
  assert.equal(roundTripped.satelliteSystem, "MIX");
  assert.equal(roundTripped.runBy, "TESTAGENCY");
  assert.equal(roundTripped.mappingFunction.kind, "COSZ");
  assert.equal(roundTripped.mappingDeclaration.kind, "DECLARED");
});

test("an OTHER code the writer can represent survives a text round trip", () => {
  const mod = { kind: "OTHER", code: "MOD" };
  const ionex = ionexFromSamples({
    ...ONE_MAP_AXES,
    tecMaps: [
      [
        [10.0, 20.0],
        [30.0, 40.0],
      ],
    ],
    header: baseHeader({ mappingFunction: mod }),
  });
  assert.equal(ionex.header.mappingFunction.code, "MOD");
  assert.equal(ionex.header.mappingDeclaration.function.code, "MOD");

  const reparsed = loadIonex(Buffer.from(ionex.toIonexString(), "utf8"));
  assert.equal(reparsed.header.mappingFunction.kind, "OTHER");
  assert.equal(reparsed.header.mappingFunction.code, "MOD");
});

test("an OTHER code with trailing blanks is kept as written and the writer refuses it", () => {
  const padded = { kind: "OTHER", code: "MOD " };
  const ionex = ionexFromSamples({
    ...ONE_MAP_AXES,
    tecMaps: [
      [
        [10.0, 20.0],
        [30.0, 40.0],
      ],
    ],
    header: baseHeader({
      mappingFunction: padded,
      mappingDeclaration: { kind: "DECLARED", function: padded },
    }),
  });
  // Kept byte for byte, trailing blank included: the wrapper never trims.
  assert.equal(ionex.header.mappingFunction.code, "MOD ");

  // A MAPPING FUNCTION record would read back trimmed, so the writer refuses to
  // write a code that would not survive its own reader.
  assert.throws(
    () => ionex.toIonexString(),
    (err) => {
      assert.equal(err.name, "IonexWriterError");
      assert.equal(err.detail.kind, "UNWRITABLE");
      assert.match(err.detail.message, /MAPPING FUNCTION/);
      return true;
    },
  );
});

test("mappingFunction and mappingDeclaration must agree, null against DECLARED included", () => {
  const tecMaps = [
    [
      [10.0, 20.0],
      [30.0, 40.0],
    ],
  ];
  const build = (mapping) =>
    ionexFromSamples({ ...ONE_MAP_AXES, tecMaps, header: baseHeader(mapping) });

  const cosz = { kind: "COSZ", code: "COSZ" };
  const none = { kind: "NO_MAPPING", code: "NONE" };

  // An explicit null beside a DECLARED declaration is a contradiction, and an
  // ordinary optional field could not tell it from the property being absent.
  assert.throws(
    () =>
      build({ mappingFunction: null, mappingDeclaration: { kind: "DECLARED", function: cosz } }),
    (err) => {
      assert.equal(err.name, "TypeError");
      assert.match(err.message, /mappingFunction is null/);
      return true;
    },
  );

  assert.throws(
    () => build({ mappingFunction: cosz, mappingDeclaration: { kind: "ABSENT" } }),
    (err) => {
      assert.match(err.message, /mappingDeclaration is ABSENT/);
      return true;
    },
  );

  assert.throws(
    () => build({ mappingFunction: cosz, mappingDeclaration: null }),
    (err) => {
      assert.match(err.message, /mappingDeclaration is null/);
      return true;
    },
  );

  assert.throws(
    () =>
      build({ mappingFunction: cosz, mappingDeclaration: { kind: "DECLARED", function: none } }),
    (err) => {
      assert.match(err.message, /COSZ.*NONE/);
      return true;
    },
  );

  // A known tag carries the code the spec fixes for it; a contradicting code is
  // refused rather than dropped on the floor.
  assert.throws(
    () => build({ mappingFunction: { kind: "COSZ", code: "MOD" } }),
    (err) => {
      assert.match(err.message, /COSZ carries the code 'COSZ'/);
      return true;
    },
  );

  // Omitting the property entirely states nothing, and the declaration alone is
  // enough.
  const declaredOnly = build({ mappingDeclaration: { kind: "DECLARED", function: none } });
  assert.equal(declaredOnly.header.mappingFunction.kind, "NO_MAPPING");

  const absentOnly = build({ mappingDeclaration: { kind: "ABSENT" } });
  assert.equal(absentOnly.header.mappingFunction, null);
  assert.equal(absentOnly.header.mappingDeclaration.kind, "ABSENT");

  const bothNull = build({ mappingFunction: null, mappingDeclaration: null });
  assert.equal(bothNull.header.mappingFunction, null);

  const agreeing = build({
    mappingFunction: cosz,
    mappingDeclaration: { kind: "DECLARED", function: cosz },
  });
  assert.equal(agreeing.header.mappingFunction.kind, "COSZ");
});

// --- Unknown input properties -----------------------------------------------

const TEC_2X2 = [
  [
    [10.0, 20.0],
    [30.0, 40.0],
  ],
];

const ALL_NULL_2X2 = [
  [
    [null, null],
    [null, null],
  ],
];

test("a misspelled property on grid samples is refused rather than read as an absence", () => {
  // The singular spellings are the dangerous ones: the real property is
  // optional, so dropping the typo would build a product with no cube at all
  // and say nothing about it.
  assert.throws(
    () => ionexFromSamples({ ...ONE_MAP_AXES, tecMaps: TEC_2X2, heightMap: ALL_NULL_2X2 }),
    (err) => {
      assert.ok(err instanceof TypeError);
      assert.match(err.message, /unknown IONEX TEC grid samples property 'heightMap'/);
      // The accepted names are listed, so the correction is in the message.
      assert.match(err.message, /heightMaps/);
      return true;
    },
  );

  assert.throws(
    () => ionexFromSamples({ ...ONE_MAP_AXES, tecMaps: TEC_2X2, rmsMap: ALL_NULL_2X2 }),
    (err) => {
      assert.ok(err instanceof TypeError);
      assert.match(err.message, /unknown IONEX TEC grid samples property 'rmsMap'/);
      return true;
    },
  );

  // A required property misspelled is refused by name too, rather than being
  // reported only as the resulting missing field.
  const { mapEpochsJ2000S, ...withoutEpochs } = ONE_MAP_AXES;
  assert.throws(
    () =>
      ionexFromSamples({
        ...withoutEpochs,
        mapEpochsJ2000s: mapEpochsJ2000S,
        tecMaps: TEC_2X2,
      }),
    (err) => {
      assert.ok(err instanceof TypeError);
      assert.match(err.message, /unknown IONEX TEC grid samples property 'mapEpochsJ2000s'/);
      return true;
    },
  );

  // A property that belongs on the header does not belong on the container.
  assert.throws(
    () => ionexFromSamples({ ...ONE_MAP_AXES, tecMaps: TEC_2X2, mappingFunction: null }),
    (err) => {
      assert.ok(err instanceof TypeError);
      assert.match(err.message, /unknown IONEX TEC grid samples property 'mappingFunction'/);
      return true;
    },
  );

  // Nothing unknown: the same input without the typo builds.
  const built = ionexFromSamples({
    ...ONE_MAP_AXES,
    tecMaps: TEC_2X2,
    heightMaps: ALL_NULL_2X2,
  });
  assert.equal(built.hasHeight, true);
});

test("a misspelled header property is refused wherever the header is supplied", () => {
  for (const [typo, corrected] of [
    ["stationCounts", "stationCount"],
    ["mapsInFiles", "mapsInFile"],
    ["satelliteSystems", "satelliteSystem"],
    ["mappingDecl", "mappingDeclaration"],
  ]) {
    // Nested inside whole-grid samples.
    assert.throws(
      () =>
        ionexFromSamples({
          ...ONE_MAP_AXES,
          tecMaps: TEC_2X2,
          header: baseHeader({ [typo]: 50 }),
        }),
      (err) => {
        assert.ok(err instanceof TypeError);
        assert.match(err.message, new RegExp(`unknown IONEX header property '${typo}'`));
        assert.match(err.message, new RegExp(corrected));
        return true;
      },
      `header typo ${typo} must be refused on grid samples`,
    );

    // And as the fifth argument to the node-sample constructor.
    assert.throws(
      () => ionexFromNodeSamples(nodeSamples2x2(), 450.0, 6371.0, 0, baseHeader({ [typo]: 50 })),
      (err) => {
        assert.ok(err instanceof TypeError);
        assert.match(err.message, new RegExp(`unknown IONEX header property '${typo}'`));
        return true;
      },
      `header typo ${typo} must be refused on node samples`,
    );
  }
});

test("a misspelled node-sample property is refused and names the sample it is on", () => {
  const withTypo = (index, overrides) => {
    const samples = nodeSamples2x2();
    samples[index] = { ...samples[index], ...overrides };
    delete samples[index].rmsTecu;
    delete samples[index].heightOffsetKm;
    return samples;
  };

  assert.throws(
    () => ionexFromNodeSamples(withTypo(2, { rmsTec: 2.5 }), 450.0, 6371.0, 0),
    (err) => {
      assert.ok(err instanceof TypeError);
      assert.match(err.message, /unknown IONEX TEC node sample 2 property 'rmsTec'/);
      assert.match(err.message, /rmsTecu/);
      return true;
    },
  );

  assert.throws(
    () => ionexFromNodeSamples(withTypo(0, { heightOffset: 10.0 }), 450.0, 6371.0, 0),
    (err) => {
      assert.ok(err instanceof TypeError);
      assert.match(err.message, /unknown IONEX TEC node sample 0 property 'heightOffset'/);
      assert.match(err.message, /heightOffsetKm/);
      return true;
    },
  );

  assert.throws(
    () => ionexFromNodeSamples(withTypo(3, { vtec: 40.0 }), 450.0, 6371.0, 0),
    (err) => {
      assert.match(err.message, /unknown IONEX TEC node sample 3 property 'vtec'/);
      return true;
    },
  );

  // The container itself and each row must be the documented shapes.
  assert.throws(
    () => ionexFromNodeSamples({ 0: nodeSamples2x2()[0] }, 450.0, 6371.0, 0),
    (err) => {
      assert.ok(err instanceof TypeError);
      assert.match(err.message, /must be an array of sample objects/);
      return true;
    },
  );
  assert.throws(
    () => ionexFromNodeSamples([nodeSamples2x2()[0], 7], 450.0, 6371.0, 0),
    (err) => {
      assert.ok(err instanceof TypeError);
      assert.match(err.message, /IONEX TEC node sample 1 must be an object/);
      return true;
    },
  );

  // No typo: the same four samples build a product.
  const built = ionexFromNodeSamples(nodeSamples2x2(), 450.0, 6371.0, 0);
  assert.equal(built.tecMaps[0][0][0], 10.0);
});

test("an inherited property is not refused and a throwing accessor is not absorbed", () => {
  // Own string property names are what the check walks; an inherited property
  // is not one. An object carrying its state on a prototype is read through
  // that chain, so it stays a usable input.
  const { mapEpochsJ2000S: epochs, ...axesWithoutEpochs } = ONE_MAP_AXES;
  // `note` is an inherited unknown key: not refused, and never read either.
  const inherited = Object.create({ mapEpochsJ2000S: epochs, note: "not an own property" });
  Object.assign(inherited, axesWithoutEpochs, { tecMaps: TEC_2X2 });
  const built = ionexFromSamples(inherited);
  assert.deepEqual(Array.from(built.mapEpochsJ2000S), [EPOCH_0_S]);

  // A known property whose accessor throws propagates the thrown value; it is
  // never read as an absence and never replaced by a default.
  const hostile = { ...ONE_MAP_AXES, tecMaps: TEC_2X2 };
  Object.defineProperty(hostile, "header", {
    enumerable: true,
    get() {
      throw new Error("header accessor exploded");
    },
  });
  assert.throws(
    () => ionexFromSamples(hostile),
    (err) => {
      assert.match(err.message, /header accessor exploded/);
      return true;
    },
  );
});

test("a throwing header accessor re-raises what it threw, identity intact", () => {
  // Propagating the text is not the contract; propagating the value is. A
  // caller that throws its own error class and matches on `instanceof` or on a
  // custom field has nothing to match if the text is restated as a fresh
  // `TypeError`.
  class HeaderUnavailable extends Error {
    constructor() {
      super("header unavailable");
      this.name = "HeaderUnavailable";
      this.code = "E_HEADER";
    }
  }
  const thrown = new HeaderUnavailable();
  const withError = { ...ONE_MAP_AXES, tecMaps: TEC_2X2 };
  Object.defineProperty(withError, "header", {
    enumerable: true,
    get() {
      throw thrown;
    },
  });
  assert.throws(
    () => ionexFromSamples(withError),
    (err) => {
      assert.equal(err, thrown); // the same object, not a copy of its message
      assert.ok(err instanceof HeaderUnavailable);
      assert.equal(err.name, "HeaderUnavailable");
      assert.equal(err.code, "E_HEADER");
      return true;
    },
  );

  // A thrown value need not be an `Error` at all, and is not converted into one.
  const bare = { reason: "no header", attempt: 3 };
  const withObject = { ...ONE_MAP_AXES, tecMaps: TEC_2X2 };
  Object.defineProperty(withObject, "header", {
    enumerable: true,
    get() {
      throw bare;
    },
  });
  assert.throws(
    () => ionexFromSamples(withObject),
    (err) => {
      assert.equal(err, bare);
      assert.equal(err instanceof Error, false);
      assert.equal(err.attempt, 3);
      return true;
    },
  );
});

test("a non-enumerable unknown property is refused, and a non-enumerable known one is read", () => {
  // Enumerability does not decide whether a name is a typo. `rmsTec` hidden
  // from `Object.keys` would be dropped exactly as silently as a plain one, so
  // the check has to see it.
  const hide = (object, key, value) => {
    Object.defineProperty(object, key, {
      value,
      enumerable: false,
      writable: true,
      configurable: true,
    });
    return object;
  };

  const rows = nodeSamples2x2();
  delete rows[2].rmsTecu;
  hide(rows[2], "rmsTec", 2.5);
  assert.equal(Object.keys(rows[2]).includes("rmsTec"), false);
  assert.throws(
    () => ionexFromNodeSamples(rows, 450.0, 6371.0, 0),
    (err) => {
      assert.ok(err instanceof TypeError);
      // The container and the index the typo is on are still named.
      assert.match(err.message, /unknown IONEX TEC node sample 2 property 'rmsTec'/);
      assert.match(err.message, /rmsTecu/);
      return true;
    },
  );

  assert.throws(
    () => ionexFromSamples(hide({ ...ONE_MAP_AXES, tecMaps: TEC_2X2 }, "heightMap", ALL_NULL_2X2)),
    (err) => {
      assert.ok(err instanceof TypeError);
      assert.match(err.message, /unknown IONEX TEC grid samples property 'heightMap'/);
      return true;
    },
  );

  assert.throws(
    () =>
      ionexFromSamples({
        ...ONE_MAP_AXES,
        tecMaps: TEC_2X2,
        header: hide(baseHeader(), "stationCounts", 50),
      }),
    (err) => {
      assert.ok(err instanceof TypeError);
      assert.match(err.message, /unknown IONEX header property 'stationCounts'/);
      return true;
    },
  );

  assert.throws(
    () =>
      ionexFromNodeSamples(
        nodeSamples2x2(),
        450.0,
        6371.0,
        0,
        hide(baseHeader(), "mapsInFiles", 1),
      ),
    (err) => {
      assert.ok(err instanceof TypeError);
      assert.match(err.message, /unknown IONEX header property 'mapsInFiles'/);
      return true;
    },
  );

  // A declared name stays usable however it is defined: `rmsTecu` hidden from
  // enumeration is read normally and still produces the RMS cube.
  const hiddenRms = nodeSamples2x2().map(({ rmsTecu, ...rest }) => hide(rest, "rmsTecu", rmsTecu));
  assert.equal(Object.keys(hiddenRms[0]).includes("rmsTecu"), false);
  const withRms = ionexFromNodeSamples(hiddenRms, 450.0, 6371.0, 0);
  assert.equal(withRms.hasRms, true);
  assert.equal(withRms.rmsMaps[0][0][0], 1.0);

  // An inherited known field is read the same way: a class instance supplying
  // its header fields from the prototype has no own names to refuse and builds.
  class HeaderLike {}
  Object.assign(HeaderLike.prototype, baseHeader());
  const built = ionexFromSamples({
    ...ONE_MAP_AXES,
    tecMaps: TEC_2X2,
    header: new HeaderLike(),
  });
  assert.equal(built.header.stationCount, 50);
  assert.equal(built.header.program, "SIDEREON");

  // A symbol-keyed own property is not a string field name. It is not walked,
  // so nothing here says a symbol typo is caught — it cannot be one.
  const symbolled = {
    ...ONE_MAP_AXES,
    tecMaps: TEC_2X2,
    [Symbol("heightMap")]: ALL_NULL_2X2,
  };
  assert.equal(ionexFromSamples(symbolled).hasHeight, false);
});

test("a stated array length is not reserved before the first row is checked", () => {
  // `length` is caller-controlled and need not describe real rows. This array
  // states a hundred million and holds one element; nothing beyond it is
  // materialized. A reservation sized from the stated length would ask wasm32
  // for gigabytes and abort where nothing can catch it, so what is asserted is
  // simply that the ordinary bad-row `TypeError` is what comes back.
  const sparse = new Array(100_000_000);
  sparse[0] = 7;
  assert.equal(sparse.length, 100_000_000);
  assert.throws(
    () => ionexFromNodeSamples(sparse, 450.0, 6371.0, 0),
    (err) => {
      assert.ok(err instanceof TypeError);
      assert.match(err.message, /IONEX TEC node sample 0 must be an object/);
      return true;
    },
  );

  // A hole at index 0 reads as `undefined` and refuses the same way.
  assert.throws(
    () => ionexFromNodeSamples(new Array(100_000_000), 450.0, 6371.0, 0),
    (err) => {
      assert.ok(err instanceof TypeError);
      assert.match(err.message, /IONEX TEC node sample 0 must be an object/);
      return true;
    },
  );

  // Validation order is all the two above prove. No length is refused for being
  // long: the four real samples still build.
  const built = ionexFromNodeSamples(nodeSamples2x2(), 450.0, 6371.0, 0);
  assert.equal(built.tecMaps[0][0][0], 10.0);
});

test("node samples are walked in order, and the first failing row is the one reported", () => {
  // Each row is checked and converted before the next is looked at, so an
  // earlier malformed row wins over a later unknown property.
  const rows = nodeSamples2x2();
  rows[1] = 7;
  rows[3] = { ...rows[3], vtec: 40.0 };
  assert.throws(
    () => ionexFromNodeSamples(rows, 450.0, 6371.0, 0),
    (err) => {
      assert.ok(err instanceof TypeError);
      assert.match(err.message, /IONEX TEC node sample 1 must be an object/);
      assert.doesNotMatch(err.message, /sample 3/);
      return true;
    },
  );
});

test("an emitted header, grid samples and node samples feed straight back in", () => {
  const cosz = { kind: "COSZ", code: "COSZ" };
  const original = ionexFromSamples({
    ...ONE_MAP_AXES,
    tecMaps: TEC_2X2,
    header: baseHeader({
      mappingFunction: cosz,
      mappingDeclaration: { kind: "DECLARED", function: cosz },
    }),
  });

  // What `tecGridSamples()` emits carries both mapping records and every
  // header scalar, and every one of those names is accepted on the way back in.
  const emitted = original.tecGridSamples();
  assert.equal(emitted.header.mappingFunction.kind, "COSZ");
  assert.equal(emitted.header.mappingDeclaration.kind, "DECLARED");
  const rebuilt = ionexFromSamples(emitted);
  assert.deepEqual(rebuilt.tecGridSamples(), emitted);
  assert.equal(rebuilt.toIonexString(), original.toIonexString());

  // The emitted header on its own is accepted as the constructor's header.
  const headerOnly = ionexFromSamples({
    ...ONE_MAP_AXES,
    tecMaps: TEC_2X2,
    header: original.header,
  });
  assert.equal(headerOnly.header.mappingFunction.kind, "COSZ");
  assert.equal(headerOnly.header.mappingDeclaration.kind, "DECLARED");

  // And what `tecSamples()` emits rebuilds the same product through the node
  // constructor when the header travels with it.
  const fromNodes = ionexFromNodeSamples(
    original.tecSamples(),
    original.shellHeightKm,
    original.baseRadiusKm,
    original.exponent,
    original.header,
  );
  assert.deepEqual(fromNodes.tecGridSamples(), emitted);
  assert.equal(fromNodes.toIonexString(), original.toIonexString());

  // The undeclared case round-trips too: an emitted header with
  // `mappingFunction: null` beside `{ kind: "ABSENT" }` is the agreeing pair,
  // not a contradiction.
  const undeclared = ionexFromSamples({ ...ONE_MAP_AXES, tecMaps: TEC_2X2 });
  const undeclaredEmitted = undeclared.tecGridSamples();
  assert.equal(undeclaredEmitted.header.mappingFunction, null);
  assert.equal(undeclaredEmitted.header.mappingDeclaration.kind, "ABSENT");
  const undeclaredRebuilt = ionexFromSamples(undeclaredEmitted);
  assert.equal(undeclaredRebuilt.header.mappingFunction, null);
  assert.equal(undeclaredRebuilt.header.mappingDeclaration.kind, "ABSENT");
  assert.deepEqual(undeclaredRebuilt.tecGridSamples(), undeclaredEmitted);
});

// --- Policies ---------------------------------------------------------------

test("policy axes are independent and composable on a real IonexSlantPolicy", () => {
  const composed = IonexSlantPolicy.coverageHold().withMissingRenormalize().withMappingDeclared();
  assert.equal(composed.coverage, "hold");
  assert.equal(composed.missingNodes, "renormalize");
  assert.equal(composed.mapping, "declared");

  // Each single-axis constructor leaves the other two at the engine defaults.
  const holdOnly = IonexSlantPolicy.coverageHold();
  assert.equal(holdOnly.coverage, "hold");
  assert.equal(holdOnly.missingNodes, "strict");
  assert.equal(holdOnly.mapping, "singleLayer");

  // A later modifier carries the earlier axes through rather than resetting them.
  const stillHeld = holdOnly.withMappingSingleLayer();
  assert.equal(stillHeld.coverage, "hold");
  assert.equal(stillHeld.mapping, "singleLayer");

  const fromConstructor = new IonexSlantPolicy("hold", "renormalize", "declared");
  assert.equal(fromConstructor.coverage, "hold");
  assert.equal(fromConstructor.missingNodes, "renormalize");
  assert.equal(fromConstructor.mapping, "declared");

  const defaults = IonexSlantPolicy.defaultPolicy();
  assert.equal(defaults.coverage, "strict");
  assert.equal(defaults.missingNodes, "strict");
  assert.equal(defaults.mapping, "singleLayer");
});

test("a malformed policy is refused rather than replaced by the default", () => {
  const text =
    layoutHeader(1, EPOCH_0, EXPONENT_0) +
    layoutMap("TEC", 1, EPOCH_0, layoutBands(i5Row(10, 20), i5Row(30, 40))) +
    layoutEnd();
  const ionex = loadIonex(Buffer.from(text, "utf8"));
  const req = [request()];

  for (const badPolicy of [
    { coverage: "sideways" },
    { missingNodes: "approximately" },
    { mapping: "whatever" },
    { coverage: 7 },
    { banana: "hold" },
    "hold",
    42,
  ]) {
    assert.throws(
      () => ionex.slantDelaysBatchResults(req, badPolicy),
      (err) => err instanceof TypeError,
      `policy ${JSON.stringify(badPolicy)} must be refused`,
    );
  }

  // Two spellings of one axis that disagree pick neither.
  assert.throws(
    () =>
      ionex.slantDelaysBatchResults(req, {
        missingNodes: "renormalize",
        missing_nodes: "strict",
      }),
    (err) => {
      assert.match(err.message, /missingNodes/);
      return true;
    },
  );

  // Two spellings that agree are accepted.
  const agreeing = ionex.slantDelaysBatchResults(req, {
    missingNodes: "renormalize",
    missing_nodes: "renormalize",
  });
  assert.equal(agreeing[0].isOk, true);

  // A property whose accessor throws is propagated, never read as an absence.
  const hostile = {};
  Object.defineProperty(hostile, "coverage", {
    enumerable: true,
    get() {
      throw new Error("policy accessor exploded");
    },
  });
  assert.throws(
    () => ionex.slantDelaysBatchResults(req, hostile),
    (err) => {
      assert.match(err.message, /policy accessor exploded/);
      return true;
    },
  );

  // Nothing at all is the documented way to ask for the default.
  assert.equal(ionex.slantDelaysBatchResults(req)[0].isOk, true);
  assert.equal(ionex.slantDelaysBatchResults(req, null)[0].isOk, true);
  assert.equal(ionex.slantDelaysBatchResults(req, undefined)[0].isOk, true);
});

// --- Missing nodes ----------------------------------------------------------

test("strict refusal and renormalization report the same nodes", () => {
  const text =
    layoutHeader(2, EPOCH_1, EXPONENT_0) +
    layoutMap("TEC", 1, EPOCH_0, layoutBands(i5Row(10, 9999), i5Row(30, 40))) +
    layoutMap("TEC", 2, EPOCH_1, layoutBands(i5Row(10, 20), i5Row(9999, 40))) +
    layoutEnd();

  const ionex = loadIonex(Buffer.from(text, "utf8"));
  const midEpoch = (EPOCH_0_S + EPOCH_1_S) / 2;
  const req = request({ latDeg: 0.5, lonDeg: 0.5, epochJ2000S: midEpoch });

  const strict = ionex.slantDelaysBatchResults([req], IonexSlantPolicy.missingStrict());
  assert.equal(strict[0].index, 0);
  assert.equal(strict[0].isOk, false);
  assert.equal(strict[0].evaluation, null);
  const refusal = strict[0].refusal;
  assert.equal(refusal.kind, "MISSING_NODES");
  assert.equal(refusal.nodeGap.earlier.mapNumber, 1);
  assert.equal(refusal.nodeGap.earlier.latIndex, 0);
  assert.equal(refusal.nodeGap.earlier.lonIndex, 0);
  assert.equal(refusal.nodeGap.earlier.lonIndexNext, 1);
  assert.deepEqual(refusal.nodeGap.earlier.missing, [false, true, false, false]);
  assert.equal(refusal.nodeGap.later.mapNumber, 2);
  assert.deepEqual(refusal.nodeGap.later.missing, [false, false, true, false]);

  assert.throws(
    () => ionex.slantDelay(0.5, 0.5, 0.0, 90.0, midEpoch, L1_HZ),
    (err) => {
      assert.equal(err.name, "IonexMissingNodesError");
      assert.equal(err.detail.kind, "MISSING_NODES");
      assert.equal(err.detail.nodeGap.earlier.mapNumber, 1);
      return true;
    },
  );

  const renormalized = ionex.slantDelaysBatchResults([req], IonexSlantPolicy.missingRenormalize());
  assert.equal(renormalized[0].isOk, true);
  assert.equal(renormalized[0].refusal, null);
  const evaluation = renormalized[0].evaluation;
  assert(evaluation.delayM > 0.0);
  assert.equal(evaluation.status.isValid, false);
  assert.equal(evaluation.status.isNominal, false);
  assert.equal(evaluation.status.isDegraded, true);
  assert.equal(evaluation.status.degraded.earlier.mapNumber, 1);
  assert.equal(evaluation.status.degraded.later.mapNumber, 2);
});

test("held, degraded and assumedMapping coexist, and an assumption alone stays valid", () => {
  const text =
    layoutHeader(1, EPOCH_0, EXPONENT_0 + ionexRecord("NONE", "MAPPING FUNCTION")) +
    layoutMap("TEC", 1, EPOCH_0, layoutBands(i5Row(10, 9999), i5Row(30, 40))) +
    layoutEnd();
  const ionex = loadIonex(Buffer.from(text, "utf8"));

  const policy = IonexSlantPolicy.coverageHold().withMissingRenormalize().withMappingSingleLayer();
  assert.equal(policy.coverage, "hold");
  assert.equal(policy.missingNodes, "renormalize");
  assert.equal(policy.mapping, "singleLayer");

  // Latitude 2.0 is past the product's northern node, and the weighted cell
  // holds a non-available node.
  const heldBatch = ionex.slantDelaysBatchResults([request({ latDeg: 2.0, lonDeg: 0.5 })], policy);
  assert.equal(heldBatch[0].isOk, true);
  const held = heldBatch[0].evaluation;
  assert.equal(held.status.isHeld, true);
  assert.equal(held.status.isDegraded, true);
  assert.equal(held.status.isAssumedMapping, true);
  assert.equal(held.status.held.kind, "LATITUDE_OUT_OF_RANGE");
  assert.equal(held.status.assumedMapping.kind, "NO_MAPPING");
  assert.equal(held.status.isValid, false);

  // Inside coverage, on weighted nodes that all hold values (see `request`):
  // only the mapping is assumed, and the engine's own rule keeps that valid.
  const assumed = ionex.slantDelayWithPolicy(0.25, 0.0, 0.0, 90.0, EPOCH_0_S, L1_HZ, policy);
  assert.equal(assumed.status.isHeld, false);
  assert.equal(assumed.status.isDegraded, false);
  assert.equal(assumed.status.isAssumedMapping, true);
  assert.equal(assumed.status.assumedMapping.kind, "NO_MAPPING");
  assert.equal(assumed.status.isNominal, false);
  assert.equal(assumed.status.isValid, true);

  // A plain object states the same policy, and each axis acts on its own: hold
  // alone leaves the strict missing-node axis in place.
  const holdOnly = ionex.slantDelaysBatchResults([request({ latDeg: 2.0, lonDeg: 0.5 })], {
    coverage: "hold",
  });
  assert.equal(holdOnly[0].isOk, false);
  assert.equal(holdOnly[0].refusal.kind, "MISSING_NODES");
});

test("typed mapping and height refusals carry their own payloads", () => {
  const tecMaps = [
    [
      [10.0, 20.0],
      [30.0, 40.0],
    ],
  ];
  const declaredNone = ionexFromSamples({
    ...ONE_MAP_AXES,
    tecMaps,
    header: baseHeader({
      mappingDeclaration: { kind: "DECLARED", function: { kind: "NO_MAPPING", code: "NONE" } },
    }),
  });
  const req = request({ latDeg: 0.5, lonDeg: 0.5, elevationDeg: 45.0 });

  const declared = declaredNone.slantDelaysBatchResults([req], IonexSlantPolicy.mappingDeclared());
  assert.equal(declared[0].isOk, false);
  assert.equal(declared[0].refusal.kind, "MAPPING_FUNCTION");
  assert.equal(declared[0].refusal.mappingDeclaration.kind, "DECLARED");
  assert.equal(declared[0].refusal.mappingDeclaration.function.code, "NONE");

  const varying = ionexFromSamples({
    ...ONE_MAP_AXES,
    tecMaps,
    heightMaps: [
      [
        [450.0, 500.0],
        [450.0, 450.0],
      ],
    ],
  });
  const varyingResults = varying.slantDelaysBatchResults([req]);
  assert.equal(varyingResults[0].isOk, false);
  assert.equal(varyingResults[0].refusal.kind, "VARYING_HEIGHTS");
  assert.equal(varyingResults[0].refusal.mapNumber, 1);
  assert.equal(varyingResults[0].refusal.latIndex, 0);
  assert.equal(varyingResults[0].refusal.lonIndex, 1);

  const missingHeight = ionexFromSamples({
    ...ONE_MAP_AXES,
    tecMaps,
    heightMaps: [
      [
        [450.0, 450.0],
        [450.0, null],
      ],
    ],
  });
  const missingResults = missingHeight.slantDelaysBatchResults([req]);
  assert.equal(missingResults[0].isOk, false);
  assert.equal(missingResults[0].refusal.kind, "HEIGHT_NOT_AVAILABLE");
  assert.equal(missingResults[0].refusal.mapNumber, 1);
  assert.equal(missingResults[0].refusal.latIndex, 1);
  assert.equal(missingResults[0].refusal.lonIndex, 1);
});

// --- Warnings ---------------------------------------------------------------

test("all seven warning variants keep their own payloads at full precision", () => {
  const bands = layoutBands(i5Row(1, 2), i5Row(3, 4));
  const clean =
    layoutHeader(2, EPOCH_1, EXPONENT_0) +
    layoutMap("TEC", 1, EPOCH_0, bands) +
    layoutMap("TEC", 2, EPOCH_1, bands) +
    layoutEnd();

  // 1. MissingRecord. This header leaves out PGM / RUN BY / DATE,
  // MAPPING FUNCTION, ELEVATION CUTOFF and OBSERVABLES USED as well, each of
  // which is a legitimate warning of the same kind, so the one under test is
  // selected by kind and label together.
  const noEnd = clean.replace(layoutEnd(), "");
  const missing = loadIonexWithWarnings(Buffer.from(noEnd, "utf8")).warnings;
  const endOfFile = missing.find((w) => w.kind === "MISSING_RECORD" && w.label === "END OF FILE");
  assert.notEqual(endOfFile, undefined);
  assert.match(endOfFile.message, /END OF FILE/);
  const labels = missing.filter((w) => w.kind === "MISSING_RECORD").map((w) => w.label);
  assert(labels.includes("PGM / RUN BY / DATE"));
  assert(labels.includes("MAPPING FUNCTION"));
  assert(labels.includes("ELEVATION CUTOFF"));
  assert(labels.includes("OBSERVABLES USED"));

  // 2. VersionRecordNotFirst.
  const commentFirst = ionexRecord("a comment first", "COMMENT") + clean;
  const version = loadIonexWithWarnings(Buffer.from(commentFirst, "utf8")).warnings.find(
    (w) => w.kind === "VERSION_RECORD_NOT_FIRST",
  );
  assert.notEqual(version, undefined);
  assert.equal(version.line, 2);

  // 3. EpochMismatch, at the precision the engine holds the epoch.
  const wrongLast =
    layoutHeader(2, EPOCH_0, EXPONENT_0) +
    layoutMap("TEC", 1, EPOCH_0, bands) +
    layoutMap("TEC", 2, EPOCH_1, bands) +
    layoutEnd();
  const epochMismatch = loadIonexWithWarnings(Buffer.from(wrongLast, "utf8")).warnings.find(
    (w) => w.kind === "EPOCH_MISMATCH" && w.label === "EPOCH OF LAST MAP",
  );
  assert.notEqual(epochMismatch, undefined);
  assert.equal(epochMismatch.declaredEpoch.scale, "UTC");
  assert.equal(epochMismatch.declaredEpoch.jdWhole, 2458849.0);
  assert.equal(epochMismatch.declaredEpoch.fraction, 0.5);
  assert.equal(epochMismatch.declaredEpoch.j2000Seconds, String(EPOCH_0_S));
  assert.equal(epochMismatch.declaredEpoch.j2000SecondsNumber, EPOCH_0_S);
  assert.equal(epochMismatch.mapsEpoch.jdWhole, 2458849.0);
  assert(Math.abs(epochMismatch.mapsEpoch.fraction - (0.5 + 1.0 / 24.0)) < 1e-12);
  assert.equal(epochMismatch.mapsEpoch.j2000Seconds, String(EPOCH_1_S));
  assert.equal(epochMismatch.mapsEpoch.j2000SecondsF64, EPOCH_1_S);
  // The epoch payload is an epoch, not the number a map count would give.
  assert.equal(typeof epochMismatch.declaredEpoch, "object");

  // 4. MapCountMismatch. Its declared value is a count, carried exactly.
  const wrongCount =
    layoutHeader(3, EPOCH_1, EXPONENT_0) +
    layoutMap("TEC", 1, EPOCH_0, bands) +
    layoutMap("TEC", 2, EPOCH_1, bands) +
    layoutEnd();
  const mapCount = loadIonexWithWarnings(Buffer.from(wrongCount, "utf8")).warnings.find(
    (w) => w.kind === "MAP_COUNT_MISMATCH",
  );
  assert.notEqual(mapCount, undefined);
  assert.equal(mapCount.declaredCount, "3");
  assert.equal(mapCount.declaredCountNumber, 3);
  assert.equal(mapCount.tecMaps, 2);
  assert.equal(mapCount.allMaps, 2);
  assert.equal(typeof mapCount.line, "number");
  // The two variants do not share one property with two different types.
  assert.equal(mapCount.declaredEpoch, undefined);
  assert.equal(epochMismatch.declaredCount, undefined);

  // 5. NotANumberValue.
  const nanText =
    layoutHeader(1, EPOCH_0, EXPONENT_0) +
    layoutMap("TEC", 1, EPOCH_0, layoutBands(i5Row(1, "  nan"), i5Row(3, 4))) +
    layoutEnd();
  const notANumber = loadIonexWithWarnings(Buffer.from(nanText, "utf8")).warnings.find(
    (w) => w.kind === "NOT_A_NUMBER_VALUE",
  );
  assert.notEqual(notANumber, undefined);
  assert.equal(notANumber.dataKind, "TEC");
  assert.equal(notANumber.mapNumber, 1);
  assert.equal(notANumber.latDeg, 1.0);
  assert.equal(notANumber.lonDeg, 1.0);

  // 6. IntervalMismatch, whose spacing is carried exactly.
  const wrongInterval = clean.replace("  3600", "  1800");
  const interval = loadIonexWithWarnings(Buffer.from(wrongInterval, "utf8")).warnings.find(
    (w) => w.kind === "INTERVAL_MISMATCH",
  );
  assert.notEqual(interval, undefined);
  assert.equal(interval.declaredS, 1800);
  assert.equal(interval.spacingS, "3600");
  assert.equal(interval.spacingSNumber, 3600);
  assert.equal(interval.mapNumber, 2);

  // 7. ExponentCarriedIntoMap: map 1 states an exponent, map 2 inherits it.
  const carriedText =
    layoutHeader(2, EPOCH_1, "") +
    layoutMap(
      "TEC",
      1,
      EPOCH_0,
      ionexRecord("    -2", "EXPONENT") + layoutBands(i5Row(100, 200), i5Row(300, 400)),
    ) +
    layoutMap("TEC", 2, EPOCH_1, layoutBands(i5Row(100, 200), i5Row(300, 400))) +
    layoutEnd();
  const carried = loadIonexWithWarnings(Buffer.from(carriedText, "utf8")).warnings.find(
    (w) => w.kind === "EXPONENT_CARRIED_INTO_MAP",
  );
  assert.notEqual(carried, undefined);
  assert.equal(carried.exponent, -2);
  assert.equal(carried.mapNumber, 2);
  assert.equal(carried.dataKind, "TEC");
  assert.equal(typeof carried.setByLine, "number");
  assert(carried.setByLine < carried.line);

  // Warnings keep the reader's own order.
  const ordered = loadIonexWithWarnings(Buffer.from(noEnd, "utf8")).warnings;
  assert.equal(ordered[ordered.length - 1].label, "END OF FILE");
});

test("parseWithWarnings returns the product beside its warnings", () => {
  const text =
    layoutHeader(1, EPOCH_0, EXPONENT_0) +
    layoutMap("TEC", 1, EPOCH_0, layoutBands(i5Row(10, 20), i5Row(30, 40))) +
    layoutEnd();
  const result = loadIonexWithWarnings(Buffer.from(text, "utf8"));
  assert.equal(result.ionex, result.value);
  assert.deepEqual(Array.from(result.ionex.lonNodesDeg), [0.0, 1.0]);
  assert(Array.isArray(result.warnings));
});

// --- Batch ------------------------------------------------------------------

test("a batch keeps one typed row per request in request order", () => {
  const text =
    layoutHeader(1, EPOCH_0, EXPONENT_0) +
    layoutMap("TEC", 1, EPOCH_0, layoutBands(i5Row(10, 9999), i5Row(30, 40))) +
    layoutEnd();
  const ionex = loadIonex(Buffer.from(text, "utf8"));

  const requests = [
    // 0. Inside the cell, weighting only nodes that hold values.
    request(),
    // 1. Receiver latitude outside [-90, 90].
    request({ latDeg: 95.0 }),
    // 2. A row that is not a request object at all, in the middle of the batch.
    "not a request",
    // 3. A row missing a required field.
    { latDeg: 0.0, lonDeg: 0.0 },
    // 4. Still evaluated after the malformed rows above. The geometry stays at
    // the zenith: this product spans one degree of latitude, and at 60 degrees
    // elevation the pierce point already lands about 1.5 degrees away, outside
    // its coverage.
    request({ frequencyHz: 1227.6e6 }),
    // 5. A non-finite coordinate.
    request({ latDeg: NaN }),
    // 6. A non-positive carrier frequency.
    request({ frequencyHz: -1.0 }),
    // 7. The weighted cell holds a non-available node.
    request({ latDeg: 0.5, lonDeg: 0.5 }),
    // 8. An epoch before the first map, under the strict default.
    request({ epochJ2000S: EPOCH_0_S - 3600 }),
    // 9. A fractional query second.
    request({ epochJ2000S: EPOCH_0_S + 0.5 }),
    // 10. An epoch past the safe-integer range, which a number cannot carry
    // exactly and must not be cast.
    request({ epochJ2000S: 2 ** 63 }),
    // 11. A last valid row, to show no earlier failure aborted the batch.
    request(),
  ];

  const results = ionex.slantDelaysBatchResults(requests);
  assert.equal(results.length, requests.length);
  results.forEach((row, index) => assert.equal(row.index, index));

  const expected = [
    ["ok"],
    ["refused", "INVALID_INPUT"],
    ["refused", "INVALID_INPUT"],
    ["refused", "INVALID_INPUT"],
    ["ok"],
    ["refused", "INVALID_INPUT"],
    ["refused", "INVALID_INPUT"],
    ["refused", "MISSING_NODES"],
    ["refused", "COVERAGE"],
    ["refused", "INVALID_INPUT"],
    ["refused", "INVALID_INPUT"],
    ["ok"],
  ];
  expected.forEach(([outcome, kind], index) => {
    const row = results[index];
    if (outcome === "ok") {
      assert.equal(row.isOk, true, `row ${index}`);
      assert(row.evaluation.delayM > 0.0, `row ${index}`);
      assert.equal(row.refusal, null, `row ${index}`);
    } else {
      assert.equal(row.isOk, false, `row ${index}`);
      assert.equal(row.evaluation, null, `row ${index}`);
      assert.equal(row.refusal.kind, kind, `row ${index}`);
      assert.equal(typeof row.refusal.message, "string", `row ${index}`);
    }
  });

  // The top-level convenience takes the same rows.
  const viaFreeFunction = ionexSlantDelayResults(ionex, [request()]);
  assert.equal(viaFreeFunction[0].isOk, true);

  // A container that is not an array is refused outright, not row by row.
  for (const badContainer of [{}, "requests", 5, null]) {
    assert.throws(
      () => ionex.slantDelaysBatchResults(badContainer),
      (err) => err instanceof TypeError,
      `container ${JSON.stringify(badContainer)} must be refused`,
    );
  }

  // An empty batch is an empty result, not an error.
  assert.deepEqual(ionex.slantDelaysBatchResults([]), []);
});

// --- Writer -----------------------------------------------------------------

test("the writer refuses a value it cannot represent and says why", () => {
  const ionex = ionexFromSamples({
    ...ONE_MAP_AXES,
    tecMaps: [
      [
        [12345.6, 20.0],
        [30.0, 40.0],
      ],
    ],
  });
  assert.throws(
    () => ionex.toIonexString(),
    (err) => {
      assert.equal(err.name, "IonexWriterError");
      assert.equal(err.detail.kind, "UNWRITABLE");
      assert.equal(typeof err.detail.message, "string");
      return true;
    },
  );
});

// --- Standalone regular grid -------------------------------------------------

function smallGrid() {
  // Two epochs, two latitudes, two longitudes; one node holds no value and one
  // holds an exact zero, which is a value and not an absence.
  return new TecGrid(
    [0.0, 3600.0 * 1e9],
    [0.0, 10.0],
    [0.0, 10.0],
    [10.0, null, 30.0, 40.0, 15.0, 25.0, 35.0, 0.0],
  );
}

test("a standalone grid exposes its axes and values exactly", () => {
  const grid = smallGrid();
  assert.deepEqual(Array.from(grid.epochsNs), [0.0, 3600.0 * 1e9]);
  assert.deepEqual(Array.from(grid.latitudesDeg), [0.0, 10.0]);
  assert.deepEqual(Array.from(grid.longitudesDeg), [0.0, 10.0]);
  const values = grid.values;
  assert.equal(values.length, 8);
  assert.equal(values[1], null);
  assert.equal(values[7], 0.0);
});

test("a standalone grid reports its policy results and typed error payloads", () => {
  const grid = smallGrid();
  const epoch = new TecGridEpoch(1800n * 1000000000n, 1);

  assert.throws(
    () => grid.vtecAtPiercePoint(epoch, 5.0, 5.0),
    (err) => {
      assert.equal(err.name, "TecGridError");
      assert.equal(err.detail.kind, "NODES_NOT_AVAILABLE");
      assert.notEqual(err.detail.nodeGap, null);
      assert.equal(err.detail.nodeGap.earlier.mapNumber, 1);
      return true;
    },
  );

  const renormalized = grid.vtecAtPiercePointWithPolicy(epoch, 5.0, 5.0, "renormalize");
  assert(renormalized.value > 0.0);
  assert.notEqual(renormalized.degraded, null);
  assert.equal(renormalized.degraded.earlier.mapNumber, 1);

  assert.throws(
    () => grid.vtecAtPiercePoint(epoch, 5.0, 95.0),
    (err) => {
      assert.equal(err.detail.kind, "OUT_OF_BOUNDS");
      assert.equal(typeof err.detail.name, "string");
      return true;
    },
  );

  assert.throws(
    () => new TecGrid([0.0, 1.0], [0.0, 10.0], [0.0, 10.0], [10.0, 20.0]),
    (err) => {
      assert.equal(err.detail.kind, "VALUE_COUNT_MISMATCH");
      assert.equal(err.detail.actual, 2);
      assert.equal(err.detail.expected, 8);
      return true;
    },
  );

  assert.throws(
    () => new TecGrid([0.0], [0.0, 10.0], [0.0, 10.0], [1.0, 2.0]),
    (err) => {
      assert.equal(err.detail.kind, "AXES_TOO_SHORT");
      return true;
    },
  );

  assert.throws(
    () => new TecGrid([1.0, 0.0], [0.0, 10.0], [0.0, 10.0], new Array(8).fill(1.0)),
    (err) => {
      assert.equal(err.detail.kind, "AXES_NOT_INCREASING");
      return true;
    },
  );

  const geometry = TecGridShellGeometry.defaultShell();
  assert.equal(geometry.shellHeightM, 450000.0);
  assert.equal(geometry.shellRadiusM(), geometry.earthRadiusM + geometry.shellHeightM);

  const options = TecGridEvalOptions.l1(epoch);
  assert.equal(options.epoch.unixNanos, 1800n * 1000000000n);
  assert(options.frequencyHz > 1e9);
});

test("a TecGridEpoch keeps every nanosecond of a present-day timestamp", () => {
  // The last nanosecond of 2026-01-01T00:00:00Z. A JavaScript number holds
  // integers exactly only below 2^53, and this is about two hundred times that,
  // so it can only arrive and leave as a bigint or a decimal string.
  const exact = 1767225600999999999n;
  assert(exact > BigInt(Number.MAX_SAFE_INTEGER));

  const fromBigInt = new TecGridEpoch(exact, 1);
  assert.equal(fromBigInt.unixNanos, exact);
  assert.equal(fromBigInt.unixNanosString, "1767225600999999999");
  assert.equal(fromBigInt.dayOfYear, 1);
  // A number round trip would have lost the last digits; the binding did not.
  assert.notEqual(BigInt(Number(exact)), exact);

  const fromString = new TecGridEpoch("1767225600999999999", 1);
  assert.equal(fromString.unixNanos, exact);

  const negative = new TecGridEpoch(-1767225600999999999n, 0);
  assert.equal(negative.unixNanos, -1767225600999999999n);

  // A number is still accepted where it can carry the value exactly.
  const small = new TecGridEpoch(1800000000000, 5);
  assert.equal(small.unixNanos, 1800000000000n);

  // Past i64 in either direction, it is refused rather than wrapped.
  assert.throws(() => new TecGridEpoch(2n ** 63n, 1), RangeError);
  assert.throws(() => new TecGridEpoch(-(2n ** 63n) - 1n, 1), RangeError);
  // A number that cannot carry the value exactly is refused too.
  assert.throws(() => new TecGridEpoch(Number.MAX_SAFE_INTEGER + 2, 1), RangeError);
  assert.throws(() => new TecGridEpoch(1.5, 1), RangeError);
  assert.throws(() => new TecGridEpoch(NaN, 1), RangeError);
  assert.throws(() => new TecGridEpoch({}, 1), TypeError);

  // The day-of-year is checked in its own domain before it can be narrowed.
  assert.throws(() => new TecGridEpoch(0n, 65536), RangeError);
  assert.throws(() => new TecGridEpoch(0n, -1), RangeError);
  assert.throws(() => new TecGridEpoch(0n, 1.5), RangeError);
  assert.equal(new TecGridEpoch(0n, 65535).dayOfYear, 65535);
});

test("a coordinate callback that throws re-raises the value it threw", () => {
  const grid = new TecGrid([0.0, 3600.0 * 1e9], [0.0, 10.0], [0.0, 10.0], new Array(8).fill(12.0));
  const options = TecGridEvalOptions.l1(new TecGridEpoch(1800n * 1000000000n, 1));
  const receiver = [6371000.0, 0.0, 0.0];
  const satellite = [26000000.0, 0.0, 0.0];

  const sentinel = new Error("no conversion available here");
  assert.throws(
    () =>
      tecXyz(grid, options, satellite, receiver, () => {
        throw sentinel;
      }),
    (err) => err === sentinel,
  );

  // The same holds for the policy-aware entry, so no fabricated coordinate can
  // turn a thrown error into a delay.
  assert.throws(
    () =>
      tecXyzWithPolicy(grid, options, satellite, receiver, "renormalize", () => {
        throw sentinel;
      }),
    (err) => err === sentinel,
  );

  // A malformed return is a typed input error, not a fabricated coordinate.
  assert.throws(
    () => tecXyz(grid, options, satellite, receiver, () => [1.0, 2.0]),
    (err) => {
      assert(err instanceof TypeError);
      assert.match(err.message, /exactly 3 numbers/);
      return true;
    },
  );
  assert.throws(
    () => tecXyz(grid, options, satellite, receiver, () => ["5", "5", "450000"]),
    (err) => {
      assert(err instanceof TypeError);
      assert.match(err.message, /non-numeric component/);
      return true;
    },
  );
  assert.throws(
    () => tecXyz(grid, options, satellite, receiver, () => ({ lon: 5, lat: 5 })),
    (err) => {
      assert(err instanceof TypeError);
      assert.match(err.message, /array of three numbers/);
      return true;
    },
  );

  // A deliberately returned NaN is the engine's documented marker: the pierce
  // point falls back to the receiver position and a value comes back. That is a
  // different outcome from a thrown error.
  let call = 0;
  const value = tecXyz(grid, options, satellite, receiver, () => {
    call += 1;
    return call === 1 ? [NaN, NaN, NaN] : [5.0, 5.0, 450000.0];
  });
  assert.equal(value.length, 2);
  assert(Number.isFinite(value[0]));
  assert(Number.isFinite(value[1]));

  // With no callback at all the engine's own WGS84 conversion is used.
  const builtIn = tecXyz(grid, options, satellite, receiver);
  assert.equal(builtIn.length, 2);
  assert(Number.isFinite(builtIn[0]));
});

// --- Caller exceptions at the supplied-header boundary ----------------------

test("a throwing mapping-record accessor re-raises what it threw, identity intact", () => {
  // `mappingFunction` and `mappingDeclaration` are read presence-aware rather
  // than by serde, so they have their own `has` and `get` against the supplied
  // object. Both re-raise what JavaScript threw. Restating the text as a fresh
  // `TypeError` would leave a caller matching on `instanceof` or on a custom
  // field with nothing to match.
  class MappingUnavailable extends Error {
    constructor() {
      super("mapping record unavailable");
      this.name = "MappingUnavailable";
      this.code = "E_MAPPING";
    }
  }

  const build = (header) => ionexFromSamples({ ...ONE_MAP_AXES, tecMaps: TEC_2X2, header });

  const thrownFunction = new MappingUnavailable();
  const functionGetter = baseHeader();
  Object.defineProperty(functionGetter, "mappingFunction", {
    enumerable: true,
    configurable: true,
    get() {
      throw thrownFunction;
    },
  });
  assert.throws(
    () => build(functionGetter),
    (err) => {
      assert.equal(err, thrownFunction); // the same object, not a copy of its message
      assert.ok(err instanceof MappingUnavailable);
      assert.equal(err.name, "MappingUnavailable");
      assert.equal(err.code, "E_MAPPING");
      return true;
    },
  );

  // A thrown value need not be an `Error`, and is not converted into one.
  const bare = { reason: "no declaration", attempt: 7 };
  const declarationGetter = baseHeader();
  Object.defineProperty(declarationGetter, "mappingDeclaration", {
    enumerable: true,
    configurable: true,
    get() {
      throw bare;
    },
  });
  assert.throws(
    () => build(declarationGetter),
    (err) => {
      assert.equal(err, bare);
      assert.equal(err instanceof Error, false);
      assert.equal(err.attempt, 7);
      return true;
    },
  );

  // Enumerability does not change what a throwing accessor does: the name walk
  // sees the property without invoking it, and the read that follows throws.
  const hiddenGetter = baseHeader();
  Object.defineProperty(hiddenGetter, "mappingFunction", {
    enumerable: false,
    configurable: true,
    get() {
      throw thrownFunction;
    },
  });
  assert.equal(Object.keys(hiddenGetter).includes("mappingFunction"), false);
  assert.throws(
    () => build(hiddenGetter),
    (err) => err === thrownFunction,
  );

  // Presence is tested before the value is read, so a `Proxy` whose `has` trap
  // throws is the other half of the same boundary.
  const trapped = new Error("has trap refused");
  const proxied = new Proxy(baseHeader(), {
    has(target, key) {
      if (key === "mappingFunction") {
        throw trapped;
      }
      return Reflect.has(target, key);
    },
  });
  assert.throws(
    () => build(proxied),
    (err) => {
      assert.equal(err, trapped);
      assert.equal(err.message, "has trap refused");
      return true;
    },
  );
});

test("a mapping record that is read but malformed stays this module's own TypeError", () => {
  // Re-raising the caller's value covers what the caller threw. A value that
  // arrives intact and is then rejected is this module's judgement, and stays a
  // `TypeError` written here with its own text.
  const build = (mapping) =>
    ionexFromSamples({ ...ONE_MAP_AXES, tecMaps: TEC_2X2, header: baseHeader(mapping) });

  assert.throws(
    () => build({ mappingFunction: 7 }),
    (err) => {
      assert.ok(err instanceof TypeError);
      assert.match(err.message, /invalid 'mappingFunction'/);
      return true;
    },
  );

  assert.throws(
    () => build({ mappingDeclaration: { kind: "SOMETHING_ELSE" } }),
    (err) => {
      assert.ok(err instanceof TypeError);
      assert.match(err.message, /invalid 'mappingDeclaration'/);
      return true;
    },
  );

  // A getter that returns a malformed value, rather than throwing one, is read
  // and then refused the same way.
  const returning = baseHeader();
  Object.defineProperty(returning, "mappingFunction", {
    enumerable: true,
    configurable: true,
    get() {
      return { kind: "COSZ", code: "MOD" };
    },
  });
  assert.throws(
    () => ionexFromSamples({ ...ONE_MAP_AXES, tecMaps: TEC_2X2, header: returning }),
    (err) => {
      assert.ok(err instanceof TypeError);
      assert.match(err.message, /COSZ carries the code 'COSZ'/);
      return true;
    },
  );

  // Absent, null and a value stay three distinct statements through the same
  // reads: a getter returning `undefined` is an absence, and the inherited
  // records a prototype supplies are still read.
  const undefinedGetter = baseHeader();
  Object.defineProperty(undefinedGetter, "mappingFunction", {
    enumerable: true,
    configurable: true,
    get() {
      return undefined;
    },
  });
  const fromUndefined = ionexFromSamples({
    ...ONE_MAP_AXES,
    tecMaps: TEC_2X2,
    header: undefinedGetter,
  });
  assert.equal(fromUndefined.header.mappingFunction, null);
  assert.equal(fromUndefined.header.mappingDeclaration.kind, "ABSENT");

  const inheritedHeader = Object.create({
    mappingDeclaration: { kind: "DECLARED", function: { kind: "Q_FACTOR", code: "QFAC" } },
  });
  Object.assign(inheritedHeader, baseHeader());
  const fromInherited = ionexFromSamples({
    ...ONE_MAP_AXES,
    tecMaps: TEC_2X2,
    header: inheritedHeader,
  });
  assert.equal(fromInherited.header.mappingFunction.kind, "Q_FACTOR");
  assert.equal(fromInherited.header.mappingDeclaration.kind, "DECLARED");
});

// --- Caller exceptions and own names at the supplied-policy boundary --------

// A product every policy test below can query inside its coverage, so a refusal
// is the policy's doing and not an unrelated input being rejected first.
function policyFixture() {
  const text =
    layoutHeader(1, EPOCH_0, EXPONENT_0) +
    layoutMap("TEC", 1, EPOCH_0, layoutBands(i5Row(10, 20), i5Row(30, 40))) +
    layoutEnd();
  return loadIonex(Buffer.from(text, "utf8"));
}

test("a throwing policy accessor re-raises what it threw, identity intact", () => {
  const ionex = policyFixture();
  const req = [request()];

  class PolicyUnavailable extends Error {
    constructor() {
      super("policy axis unavailable");
      this.name = "PolicyUnavailable";
      this.code = "E_POLICY";
    }
  }

  const thrown = new PolicyUnavailable();
  const throwingCoverage = {};
  Object.defineProperty(throwingCoverage, "coverage", {
    enumerable: true,
    configurable: true,
    get() {
      throw thrown;
    },
  });

  const identity = (err) => {
    assert.equal(err, thrown); // the same object, not a copy of its message
    assert.ok(err instanceof PolicyUnavailable);
    assert.equal(err.name, "PolicyUnavailable");
    assert.equal(err.code, "E_POLICY");
    return true;
  };

  // Every public consumer of a supplied policy reaches the same read.
  assert.throws(() => ionex.slantDelaysBatchResults(req, throwingCoverage), identity);
  assert.throws(() => ionexSlantDelayResults(ionex, req, throwingCoverage), identity);
  assert.throws(
    () => ionex.slantDelayWithPolicy(0.0, 0.0, 0.0, 90.0, EPOCH_0_S, L1_HZ, throwingCoverage),
    identity,
  );

  // A thrown value need not be an `Error`, and is not converted into one.
  const bare = { reason: "no mapping axis", attempt: 2 };
  const throwingMapping = {};
  Object.defineProperty(throwingMapping, "mapping", {
    enumerable: true,
    configurable: true,
    get() {
      throw bare;
    },
  });
  assert.throws(
    () => ionex.slantDelaysBatchResults(req, throwingMapping),
    (err) => {
      assert.equal(err, bare);
      assert.equal(err instanceof Error, false);
      assert.equal(err.attempt, 2);
      return true;
    },
  );

  // The alias spelling is read through the same accessor path.
  const throwingAlias = {};
  Object.defineProperty(throwingAlias, "missing_nodes", {
    enumerable: true,
    configurable: true,
    get() {
      throw thrown;
    },
  });
  assert.throws(() => ionex.slantDelaysBatchResults(req, throwingAlias), identity);

  // A value that is read and is then rejected stays this module's own refusal.
  assert.throws(
    () => ionex.slantDelaysBatchResults(req, { coverage: 7 }),
    (err) => {
      assert.ok(err instanceof TypeError);
      assert.match(err.message, /policy\.coverage must be a string/);
      return true;
    },
  );
  assert.throws(
    () => ionex.slantDelaysBatchResults(req, { coverage: "sideways" }),
    (err) => {
      assert.ok(err instanceof TypeError);
      assert.match(err.message, /invalid coverage policy 'sideways'/);
      return true;
    },
  );
});

test("a policy's own string names are checked whether or not they are enumerable", () => {
  const ionex = policyFixture();
  const outside = [request({ latDeg: 2.0, lonDeg: 0.5 })];

  const hide = (object, key, value) => {
    Object.defineProperty(object, key, {
      value,
      enumerable: false,
      writable: true,
      configurable: true,
    });
    return object;
  };

  // A non-enumerable typo would be dropped exactly as silently as an enumerable
  // one: `missingNode` beside `missingNodes` would leave the strict default in
  // place and read as a policy the caller never asked for.
  const hiddenTypo = hide({}, "missingNode", "renormalize");
  assert.equal(Object.keys(hiddenTypo).length, 0);
  assert.throws(
    () => ionex.slantDelaysBatchResults([request()], hiddenTypo),
    (err) => {
      assert.ok(err instanceof TypeError);
      assert.match(err.message, /unknown policy property 'missingNode'/);
      return true;
    },
  );

  // A name the policy does declare stays usable however it is defined.
  const hiddenKnown = hide({}, "coverage", "hold");
  assert.equal(Object.keys(hiddenKnown).length, 0);
  const held = ionex.slantDelaysBatchResults(outside, hiddenKnown);
  assert.equal(held[0].isOk, true);
  assert.notEqual(held[0].evaluation.status.held, null);
  assert.equal(held[0].evaluation.status.held.kind, "LATITUDE_OUT_OF_RANGE");

  // The same axis supplied enumerably is the same policy, so the walk is what
  // changed and not the reading.
  const enumerableHeld = ionex.slantDelaysBatchResults(outside, { coverage: "hold" });
  assert.equal(enumerableHeld[0].isOk, true);
  assert.equal(enumerableHeld[0].evaluation.status.held.kind, "LATITUDE_OUT_OF_RANGE");

  // Strict is still the default the omitted axis keeps.
  const strict = ionex.slantDelaysBatchResults(outside, {});
  assert.equal(strict[0].isOk, false);
  assert.equal(strict[0].refusal.kind, "COVERAGE");

  // An own symbol key is not a string field name; nothing walks it and nothing
  // refuses it.
  const withSymbol = { coverage: "hold", [Symbol("tag")]: "ignored" };
  const symbolHeld = ionex.slantDelaysBatchResults(outside, withSymbol);
  assert.equal(symbolHeld[0].isOk, true);

  // An inherited property is not an own name and is not refused: the axes are
  // read through the prototype chain, so an object holding its state there is a
  // working policy.
  const inherited = Object.create({ coverage: "hold", note: "not an own property" });
  const inheritedHeld = ionex.slantDelaysBatchResults(outside, inherited);
  assert.equal(inheritedHeld[0].isOk, true);
  assert.equal(inheritedHeld[0].evaluation.status.held.kind, "LATITUDE_OUT_OF_RANGE");

  // A real `IonexSlantPolicy` carries its pointer as its own property and its
  // axes on the prototype; the walk skips the one and the reads find the other.
  const instance = IonexSlantPolicy.coverageHold();
  const ownNames = Object.getOwnPropertyNames(instance);
  assert.ok(ownNames.every((name) => name === "__wbg_ptr"));
  const instanceHeld = ionex.slantDelaysBatchResults(outside, instance);
  assert.equal(instanceHeld[0].isOk, true);
  assert.equal(instanceHeld[0].evaluation.status.held.kind, "LATITUDE_OUT_OF_RANGE");

  // Only the exact generated pointer name is ignored. Prefix lookalikes and
  // the former generic `ptr` exemption remain unknown fields.
  for (const [policy, key] of [
    [{ coverage: "hold", __wbg_typo: "renormalize" }, "__wbg_typo"],
    [{ coverage: "hold", ptr: "renormalize" }, "ptr"],
    [hide({ coverage: "hold" }, "__wbg_typo", "renormalize"), "__wbg_typo"],
    [hide(IonexSlantPolicy.coverageHold(), "__wbg_typo", "renormalize"), "__wbg_typo"],
  ]) {
    assert.throws(
      () => ionex.slantDelaysBatchResults([request()], policy),
      (err) => {
        assert.ok(err instanceof TypeError);
        assert.ok(err.message.includes(`unknown policy property '${key}'`));
        return true;
      },
    );
  }

  // The accepted alias still passes the walk, and two spellings that disagree
  // are still refused.
  const aliased = ionex.slantDelaysBatchResults([request()], { missing_nodes: "renormalize" });
  assert.equal(aliased[0].isOk, true);
  assert.throws(
    () =>
      ionex.slantDelaysBatchResults([request()], {
        missingNodes: "renormalize",
        missing_nodes: "strict",
      }),
    (err) => {
      assert.ok(err instanceof TypeError);
      assert.match(err.message, /policy\.missingNodes is 'renormalize'/);
      return true;
    },
  );

  // A non-enumerable unknown name on an instance is refused too, so the
  // pointer exemption is not a hole an arbitrary hidden name fits through.
  const taggedInstance = hide(IonexSlantPolicy.coverageHold(), "missingNode", "renormalize");
  assert.throws(
    () => ionex.slantDelaysBatchResults([request()], taggedInstance),
    (err) => {
      assert.ok(err instanceof TypeError);
      assert.match(err.message, /unknown policy property 'missingNode'/);
      return true;
    },
  );
});
