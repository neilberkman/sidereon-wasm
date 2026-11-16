import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  CelestrakGpFormat as F,
  CelestrakGpSpecial,
  CelestrakTleRecord,
  GnssSystem,
  Omm,
  Tle,
  allowedHosts,
  celestrakConstellationUrl,
  celestrakGpFormatString,
  celestrakGpSourceEntry,
  celestrakGpSpecialString,
  celestrakGpUrl,
  celestrakGroup,
  navcenConstellationUrl,
  navcenSourceEntry,
  parseCelestrakGpResponse,
  parseOmmJsonArray,
  parseOmmJson,
  parseOmmKvn,
  parseOmmXmlAll,
  parseOmmXml,
  parseOmmCsvArray,
} from "../pkg-node/sidereon.js";

const bytes = (s) => new TextEncoder().encode(s);
const fixture = (name) => readFileSync(new URL(`./fixtures/omm/${name}`, import.meta.url), "utf8");

test("catalog entries, formats, selectors and refusals", () => {
  assert.deepEqual(celestrakGpSourceEntry(), {
    protocol: "https",
    host: "celestrak.org",
    rootUrl: "https://celestrak.org/NORAD/elements/gp.php",
  });
  assert.deepEqual(navcenSourceEntry(), {
    protocol: "https",
    host: "www.navcen.uscg.gov",
    rootUrl: "https://www.navcen.uscg.gov/gps-constellation",
  });
  assert.equal(celestrakGpSpecialString(CelestrakGpSpecial.Gpz), "GPZ");
  assert.equal(celestrakGpSpecialString(CelestrakGpSpecial.GpzPlus), "GPZ-PLUS");
  assert.equal(celestrakGpSpecialString(CelestrakGpSpecial.Decaying), "DECAYING");
  assert.equal(navcenConstellationUrl(), "https://www.navcen.uscg.gov/gps-constellation");
  assert.deepEqual(allowedHosts(), [
    "www.aiub.unibe.ch",
    "download.aiub.unibe.ch",
    "zhw-b.s3.cloud.switch.ch",
    "navigation-office.esa.int",
    "isdc-data.gfz.de",
    "igs.bkg.bund.de",
    "igs.gnsswhu.cn",
    "s3.amazonaws.com",
    "celestrak.org",
    "cddis.nasa.gov",
    "urs.earthdata.nasa.gov",
    "www.navcen.uscg.gov",
  ]);
  for (const [system, group] of [
    [GnssSystem.Gps, "gps-ops"],
    [GnssSystem.Glonass, "glo-ops"],
    [GnssSystem.Galileo, "galileo"],
    [GnssSystem.BeiDou, "beidou"],
    [GnssSystem.Qzss, "gnss"],
    [GnssSystem.Navic, "gnss"],
    [GnssSystem.Sbas, "sbas"],
  ])
    assert.equal(celestrakGroup(system), group);
  assert.equal(
    celestrakConstellationUrl(GnssSystem.Sbas, F.Csv),
    "https://celestrak.org/NORAD/elements/gp.php?GROUP=sbas&FORMAT=CSV",
  );
  const formats = [
    [F.Tle, "TLE"],
    [F.ThreeLine, "3LE"],
    [F.TwoLine, "2LE"],
    [F.Xml, "XML"],
    [F.Kvn, "KVN"],
    [F.Json, "JSON"],
    [F.JsonPretty, "JSON-PRETTY"],
    [F.Csv, "CSV"],
  ];
  for (const [format, wire] of formats) {
    assert.equal(celestrakGpFormatString(format), wire);
    assert.equal(
      celestrakGpUrl({ kind: "group", value: "active" }, format),
      `https://celestrak.org/NORAD/elements/gp.php?GROUP=active&FORMAT=${wire}`,
    );
  }
  for (const [query, parameter] of [
    [{ kind: "catalogNumber", value: 25544 }, "CATNR=25544"],
    [{ kind: "internationalDesignator", value: "1998-067" }, "INTDES=1998-067"],
    [{ kind: "name", value: "ISS ZARYA" }, "NAME=ISS%20ZARYA"],
    [{ kind: "special", value: CelestrakGpSpecial.GpzPlus }, "SPECIAL=GPZ-PLUS"],
  ]) {
    assert.equal(
      celestrakGpUrl(query, F.Json),
      `https://celestrak.org/NORAD/elements/gp.php?${parameter}&FORMAT=JSON`,
    );
  }
  assert.throws(
    () => celestrakGpUrl({ kind: "catalogNumber", value: -1 }, F.Tle),
    (e) =>
      e.name === "DataCatalogError" &&
      e.detail.kind === "invalidCelestrakGpSelector" &&
      e.detail.selector === "CATNR" &&
      e.detail.value === "-1",
  );
  assert.throws(
    () => celestrakGpUrl({ kind: "group", value: "bad group" }, F.Tle),
    (e) =>
      e.detail.kind === "invalidCelestrakGpSelector" &&
      e.detail.selector === "GROUP" &&
      e.detail.value === "bad group",
  );
  assert.throws(
    () => celestrakGpUrl({ kind: "special", value: 99 }, F.Tle),
    (e) => {
      assert.equal(e.message, 'invalid CelesTrak GP SPECIAL selector "99"');
      assert.deepEqual(e.detail, {
        kind: "invalidCelestrakGpSelector",
        selector: "SPECIAL",
        value: "99",
      });
      return true;
    },
  );
  for (const [query, selector, value] of [
    [{ kind: "catalogNumber", value: 4294967296 }, "CATNR", "4294967296"],
    [{ kind: "catalogNumber", value: Infinity }, "CATNR", "Infinity"],
    [{ kind: "catalogNumber", value: NaN }, "CATNR", "NaN"],
    [{ kind: "internationalDesignator", value: "1998-67" }, "INTDES", "1998-67"],
    [{ kind: "name", value: "" }, "NAME", ""],
    [{ kind: "special", value: 99 }, "SPECIAL", "99"],
  ]) {
    assert.throws(
      () => celestrakGpUrl(query, F.Tle),
      (e) =>
        e.name === "DataCatalogError" &&
        e.detail.kind === "invalidCelestrakGpSelector" &&
        e.detail.selector === selector &&
        e.detail.value === value,
    );
  }
});

test("GP TLE records retain source bytes, fields, and conversions", () => {
  const source = fixture("25544.tle");
  const parsed = parseCelestrakGpResponse(bytes(source), F.Tle);
  assert.equal(parsed.records.length, 1);
  assert.equal(parsed.rejected.length, 0);
  assert.notEqual(parsed.records, parsed.records);
  assert.notEqual(parsed.rejected, parsed.rejected);
  const record = parsed.records[0];
  assert.equal(record.recordIndex, 1);
  assert.equal(record.lineNumber, 1);
  assert.deepEqual(record.raw, bytes(source));
  assert.equal(record.objectName, "ISS (ZARYA)");
  assert.ok(record.payload instanceof CelestrakTleRecord);
  assert.deepEqual(record.payload.elements, {
    catalogNumber: "25544",
    classification: "U",
    internationalDesignator: "98067A",
    epochYear: 2026,
    epochDayOfYear: 168.18949189,
    meanMotionDot: 0.00009113,
    meanMotionDoubleDot: 0,
    meanMotionDoubleDotText: " 00000+0",
    bstar: 0.17172 * 0.001,
    bstarText: " 17172-3",
    ephemerisType: 0,
    elsetNumber: 999,
    inclinationDeg: 51.6332,
    raanDeg: 300.0813,
    eccentricity: 0.0004737,
    argPerigeeDeg: 195.1146,
    meanAnomalyDeg: 164.9702,
    meanMotion: 15.49273435,
    revNumber: 57175,
  });
  assert.equal(record.payload.checksumWarnings.length, 0);
  assert.deepEqual(record.toElementSet(), {
    epoch: [2461208.5, 0.18949189],
    bstar: 0.17172 * 0.001,
    meanMotionDot: 0.00009113,
    meanMotionDoubleDot: 0,
    eccentricity: 0.0004737,
    argumentOfPerigeeDeg: 195.1146,
    inclinationDeg: 51.6332,
    meanAnomalyDeg: 164.9702,
    meanMotionRevPerDay: 15.49273435,
    rightAscensionDeg: 300.0813,
    catalogNumber: 25544,
    ommEpochDays: null,
  });
  const expectedOmm = {
    ccsdsOmmVers: undefined,
    classification: undefined,
    creationDate: undefined,
    originator: undefined,
    messageId: undefined,
    objectName: "ISS (ZARYA)",
    objectId: "1998-067A",
    centerName: "EARTH",
    refFrame: "TEME",
    refFrameEpoch: undefined,
    timeSystem: "UTC",
    meanElementTheory: "SGP4",
    meanMotion: 15.49273435,
    semiMajorAxisKm: undefined,
    gmKm3S2: undefined,
    btermM2Kg: undefined,
    agomM2Kg: undefined,
    spacecraft: undefined,
    covariance: undefined,
    userDefined: [],
    comments: { header: [], metadata: [], meanElements: [], tleParameters: [], userDefined: [] },
    eccentricity: 0.0004737,
    inclinationDeg: 51.6332,
    raOfAscNodeDeg: 300.0813,
    argOfPericenterDeg: 195.1146,
    meanAnomalyDeg: 164.9702,
    ephemerisType: 0,
    classificationType: "U",
    noradCatId: 25544,
    elementSetNo: 999,
    revAtEpoch: 57175n,
    bstar: 0.17172 * 0.001,
    meanMotionDot: 0.00009113,
    meanMotionDdot: 0,
  };
  const ommValues = (value) =>
    Object.fromEntries(Object.keys(expectedOmm).map((key) => [key, value[key]]));
  const converted = record.toOmm();
  assert.deepEqual(ommValues(converted), expectedOmm);
  assert.equal(converted.epoch.iso8601, "2026-06-17T04:32:52.099296");
  const fromTle = Omm.fromTle(record.payload.elements, "ISS (ZARYA)");
  assert.deepEqual(ommValues(fromTle), expectedOmm);
  assert.equal(fromTle.epoch.iso8601, "2026-06-17T04:32:52.099296");
  for (const epoch of [converted.epoch, fromTle.epoch]) {
    assert.deepEqual(
      [
        epoch.year,
        epoch.month,
        epoch.day,
        epoch.hour,
        epoch.minute,
        epoch.second,
        epoch.microsecond,
        epoch.femtosecond,
      ],
      [2026, 6, 17, 4, 32, 52, 99296, 0],
    );
  }
  assert.equal(Omm.fromTle(record.payload.elements, "ISS").objectName, "ISS");
  assert.throws(
    () => Omm.fromTle({ ...record.payload.elements, epochYear: 2026.5 }, "ISS"),
    (e) => {
      assert.deepEqual(e.detail, {
        family: "tle",
        cause: {
          kind: "invalidField",
          field: "epoch_year",
          reason: "expected an integer in i32 range",
        },
      });
      return true;
    },
  );
  const lines = source.trimEnd().split(/\r?\n/);
  const parsedTle = new Tle(lines[1], lines[2]);
  assert.equal(Omm.fromTle(parsedTle.elements, "ISS").noradCatId, 25544);
  assert.throws(
    () => Omm.fromTle({ ...record.payload.elements, epochDayOfYear: -1 }, "ISS"),
    (e) =>
      e.detail.family === "tle" &&
      e.detail.cause.kind === "invalidField" &&
      e.detail.cause.field === "epoch_day_of_year",
  );
  assert.equal(record.equals(record), true);
  assert.equal(parsed.equals(parseCelestrakGpResponse(bytes(source), F.Tle)), true);
});

test("GP OMM formats and response errors", () => {
  for (const [format, name] of [
    [F.Xml, "24876.xml"],
    [F.Kvn, "24876.kvn"],
    [F.Json, "24876.json"],
    [F.JsonPretty, "24876-pretty.json"],
  ]) {
    const response = parseCelestrakGpResponse(bytes(fixture(name)), format);
    assert.equal(response.records.length, 1);
    assert.ok(response.records[0].payload instanceof Omm);
    assert.equal(response.records[0].toOmm().noradCatId, 24876);
    const source = fixture(name);
    assert.deepEqual(response.records[0].raw, bytes(source));
  }
  for (const [body, kind] of [
    ["", "emptyCelestrakGpResponse"],
    ["No GP data found", "celestrakGpDataNotFound"],
    ["<html>bad</html>", "celestrakGpHtmlResponse"],
  ]) {
    assert.throws(
      () => parseCelestrakGpResponse(bytes(body), F.Tle),
      (e) => e.name === "DataCatalogError" && e.detail.kind === kind,
    );
  }
  const response = parseCelestrakGpResponse(bytes("unexpected\n" + fixture("25544.tle")), F.Tle);
  assert.equal(response.rejected[0].reason.kind, "structural");
  assert.equal(response.rejected[0].reason.detail.kind, "invalidTleStructure");
  assert.equal(response.rejected[0].equals(response.rejected[0]), true);
  assert.equal(
    response.equals(parseCelestrakGpResponse(bytes(fixture("25544.tle")), F.Tle)),
    false,
  );
  assert.deepEqual(response.rejected[0].raw, bytes("unexpected\n"));
});

test("two-line, three-line and CSV framing", () => {
  const [name, line1, line2] = fixture("25544.tle").trimEnd().split(/\r?\n/);
  for (const [format, source, expectedName] of [
    [F.ThreeLine, `0 ${name.trim()}\n${line1}\n${line2}\n`, "ISS (ZARYA)"],
    [F.TwoLine, `${line1}\n${line2}\n`, undefined],
  ]) {
    const record = parseCelestrakGpResponse(bytes(source), format).records[0];
    assert.equal(record.objectName, expectedName);
    assert.deepEqual(record.raw, bytes(source));
    assert.equal(record.payload.line1, line1);
    assert.equal(record.payload.line2, line2);
  }
  const csv =
    'OBJECT_NAME,EPOCH,MEAN_MOTION,ECCENTRICITY,INCLINATION,RA_OF_ASC_NODE,ARG_OF_PERICENTER,MEAN_ANOMALY,BSTAR\nISS "ZARYA",2026-01-01T00:00:00,1,0,0,0,0,0,0';
  const record = parseCelestrakGpResponse(bytes(csv), F.Csv).records[0];
  assert.equal(record.objectName, 'ISS "ZARYA"');
  assert.deepEqual(record.raw, bytes(csv));
});

test("repeat download and malformed response retain details", () => {
  const repeat =
    "GP data has not updated since your last successful\ndownload of GROUP=active at 2026-03-26 08:10:22 UTC.\nData is updated once every 2 hours.\n";
  assert.throws(
    () => parseCelestrakGpResponse(bytes(repeat), F.Tle),
    (e) => {
      assert.equal(e.detail.kind, "celestrakGpNotModified");
      assert.equal(e.detail.query, "GROUP=active");
      assert.equal(e.detail.lastSuccessfulDownload, "2026-03-26 08:10:22 UTC");
      return true;
    },
  );
  assert.throws(
    () => parseCelestrakGpResponse(bytes("\n\n\nnot json"), F.Json),
    (e) => {
      assert.equal(e.detail.kind, "invalidCelestrakGpResponse");
      assert.equal(e.detail.line, 4);
      assert.equal(e.detail.column, 2);
      assert.equal(
        e.detail.reason,
        'malformed JSON: expected ident at line 4 column 2; response excerpt: "\\n\\n\\nnot json"',
      );
      return true;
    },
  );
});

test("OMM header and strict and lenient writers", () => {
  const omm = parseCelestrakGpResponse(bytes(fixture("25544.json")), F.Json).records[0].toOmm();
  assert.throws(
    () => omm.toKvnString(),
    (e) =>
      e.name === "OmmError" &&
      e.detail.kind === "MISSING_FIELD" &&
      e.detail.field === "CCSDS_OMM_VERS",
  );
  assert.throws(
    () => omm.toXmlString(),
    (e) =>
      e.name === "OmmError" &&
      e.detail.kind === "MISSING_FIELD" &&
      e.detail.field === "CCSDS_OMM_VERS",
  );
  const reference = JSON.parse(
    readFileSync(new URL("./fixtures/omm.json", import.meta.url), "utf8"),
  ).fixtures[0];
  const kvn = parseOmmKvn(fixture("25544.kvn"));
  const xml = parseOmmXml(fixture("25544.xml"));
  assert.equal(kvn.toKvnStringLenient(), reference.encoded_kvn);
  assert.equal(xml.toXmlStringLenient(), reference.encoded_xml);
  const strictKvn = reference.encoded_kvn
    .replace("CREATION_DATE = \n", "CREATION_DATE = 2026-06-17T05:00:00\n")
    .replace("ORIGINATOR = \n", "ORIGINATOR = TEST\n");
  const strictXml = reference.encoded_xml
    .replace(
      "<CREATION_DATE></CREATION_DATE>",
      "<CREATION_DATE>2026-06-17T05:00:00</CREATION_DATE>",
    )
    .replace("<ORIGINATOR></ORIGINATOR>", "<ORIGINATOR>TEST</ORIGINATOR>");
  assert.equal(kvn.withCcsdsHeader("2.0", "2026-06-17T05:00:00", "TEST").toKvnString(), strictKvn);
  assert.equal(xml.withCcsdsHeader("2.0", "2026-06-17T05:00:00", "TEST").toXmlString(), strictXml);
  const complete = parseOmmKvn(fixture("24876.kvn")).withCcsdsHeader(
    "2.0",
    "2026-06-17T05:00:00",
    "TEST",
  );
  assert.match(complete.toKvnString(), /ORIGINATOR\s*=\s*TEST/);
  assert.match(complete.toXmlString(), /<ORIGINATOR>TEST<\/ORIGINATOR>/);
  assert.match(omm.toKvnStringLenient(), /NORAD_CAT_ID\s*=\s*25544/);
  assert.match(omm.toXmlStringLenient(), /25544/);
  const withHeader = omm.withCcsdsHeader("2.0", "2026-06-17T05:00:00", "TEST");
  assert.equal(withHeader.creationDate, "2026-06-17T05:00:00");
  assert.equal(withHeader.originator, "TEST");
  assert.throws(
    () => omm.withCcsdsHeader("invalid", "2026-06-17T05:00:00", "TEST"),
    (e) => {
      assert.equal(e.name, "OmmError");
      assert.deepEqual(e.detail, {
        kind: "INVALID_FIELD",
        format: null,
        byteOffset: null,
        fields: null,
        message: "invalid OMM field CCSDS_OMM_VERS: out of range",
        field: "CCSDS_OMM_VERS",
        value: null,
        issue: "out of range",
        line: null,
        unit: null,
        expectedUnit: null,
        first: null,
        second: null,
        count: null,
        expectedCount: null,
        index: null,
        source: null,
      });
      return true;
    },
  );
});

test("fit OMM header failures retain the nested OMM error", async () => {
  const { fitTle } = await import("../pkg-node/sidereon.js");
  assert.throws(
    () => fitTle([], { ommHeader: { creationDate: "bad", originator: "TEST" } }),
    (e) => {
      assert.equal(e.detail.family, "tleFit");
      assert.equal(e.detail.cause.kind, "ommHeader");
      assert.equal(e.detail.cause.cause.kind, "INVALID_FIELD");
      assert.equal(e.detail.cause.cause.field, "CREATION_DATE");
      return true;
    },
  );
});

test("OMM array spans retain source offsets", () => {
  const result = parseOmmJsonArray(fixture("25544.json"));
  assert.deepEqual(result.spans, [{ index: 0, byteStart: 1, byteEnd: 418, line: 1 }]);
  assert.deepEqual(parseOmmXmlAll(fixture("24876.xml")).spans, [
    { index: 0, byteStart: 207, byteEnd: 1191, line: 3 },
  ]);
  const csv =
    "OBJECT_NAME,EPOCH,MEAN_MOTION,ECCENTRICITY,INCLINATION,RA_OF_ASC_NODE,ARG_OF_PERICENTER,MEAN_ANOMALY,BSTAR\nISS,2026-01-01T00:00:00,1,0,0,0,0,0,0,0";
  assert.deepEqual(parseOmmCsvArray(csv).spans, [
    { index: 0, byteStart: 107, byteEnd: 146, line: 2 },
  ]);
  const kvn = fixture("24876.kvn");
  const kvnRecord = parseCelestrakGpResponse(bytes(kvn), F.Kvn).records[0];
  assert.equal(kvnRecord.lineNumber, 1);
  assert.deepEqual(kvnRecord.raw, bytes(kvn));
  const secondKvn = fixture("25544.kvn");
  const records = parseCelestrakGpResponse(bytes(kvn + secondKvn), F.Kvn).records;
  assert.equal(records.length, 2);
  assert.equal(records[0].lineNumber, 1);
  assert.equal(records[1].lineNumber, 29);
  assert.equal(records[0].raw.length, 627);
  assert.equal(records[1].raw.length, 634);
  assert.deepEqual(records[0].raw, bytes(kvn.slice(0, -2)));
  assert.deepEqual(records[1].raw, bytes("\r\n" + secondKvn));
});

test("record and OMM handles compare by core value", () => {
  const iss = parseCelestrakGpResponse(bytes(fixture("25544.tle")), F.Tle).records[0];
  const other = parseCelestrakGpResponse(bytes(fixture("24876.tle")), F.Tle).records[0];
  assert.equal(iss.equals(other), false);
  assert.equal(iss.payload.equals(iss.payload), true);
  assert.equal(iss.payload.equals(other.payload), false);
  assert.equal(iss.toOmm().equals(iss.toOmm()), true);
  assert.equal(iss.toOmm().equals(other.toOmm()), false);
  assert.equal(iss.toOmm().epoch.equals(iss.toOmm().epoch), true);
  assert.equal(iss.toOmm().epoch.equals(other.toOmm().epoch), false);
});

test("new OMM error variants preserve their fields", () => {
  assert.throws(
    () => parseOmmJson("["),
    (e) => {
      assert.equal(e.detail.kind, "MALFORMED_DOCUMENT");
      assert.equal(e.detail.format, "JSON");
      assert.equal(e.detail.byteOffset, 0);
      assert.equal(e.detail.value, "EOF while parsing a list at line 1 column 1");
      return true;
    },
  );
  const input =
    "EPOCH = 2026-01-01T00:00:00\nECCENTRICITY = 0\nINCLINATION = 0\nRA_OF_ASC_NODE = 0\nARG_OF_PERICENTER = 0\nMEAN_ANOMALY = 0\n";
  assert.throws(
    () => parseOmmKvn(input),
    (e) => {
      assert.equal(e.detail.kind, "MISSING_ONE_OF");
      assert.deepEqual(e.detail.fields, ["SEMI_MAJOR_AXIS", "MEAN_MOTION"]);
      return true;
    },
  );
});

test("checksum warnings retain their complete data and value equality", () => {
  const lines = fixture("25544.tle").trimEnd().split(/\r?\n/);
  lines[1] = lines[1].slice(0, -1);
  const bad = parseCelestrakGpResponse(bytes(lines.join("\n")), F.Tle).records[0].payload;
  assert.equal(bad.checksumWarnings.length, 1);
  assert.equal(bad.checksumWarnings[0].lineLabel, "line 1");
  assert.equal(bad.checksumWarnings[0].kind, "missing");
  assert.equal(bad.checksumWarnings[0].computed, 6);
  assert.equal(bad.checksumWarnings[0].equals(bad.checksumWarnings[0]), true);
  lines[2] = lines[2].slice(0, -1);
  const different = parseCelestrakGpResponse(bytes(lines.join("\n")), F.Tle).records[0].payload;
  assert.equal(bad.checksumWarnings[0].equals(different.checksumWarnings[1]), false);
});

test("individual rejection variants retain exact nested causes", () => {
  const lines = fixture("25544.tle").trimEnd().split(/\r?\n/);
  lines[1] = `${lines[1].slice(0, -1)}0`;
  const source = lines.join("\n");
  const rejected = parseCelestrakGpResponse(bytes(source), F.Tle).rejected[0];
  assert.equal(rejected.recordIndex, 1);
  assert.equal(rejected.lineNumber, 2);
  assert.deepEqual(rejected.raw, bytes(source));
  assert.equal(rejected.reason.kind, "tle");
  assert.deepEqual(rejected.reason.detail, {
    message: "TLE line 1 checksum digit 0 does not match the computed checksum 6",
    cause: { kind: "checksumMismatch", lineLabel: "line 1", expected: 0, computed: 6 },
  });
  const missing = fixture("25544.json").replace('"EPOCH":"2026-06-17T04:32:52.099296",', "");
  const omm = parseCelestrakGpResponse(bytes(missing), F.Json).rejected[0];
  assert.equal(omm.reason.kind, "omm");
  assert.equal(omm.reason.detail.cause.kind, "MISSING_FIELD");
  assert.equal(omm.reason.detail.cause.field, "EPOCH");
  assert.equal(rejected.equals(omm), false);
  assert.equal(rejected.reason.equals(rejected.reason), true);
  assert.equal(rejected.reason.equals(omm.reason), false);
  const invalid = fixture("25544.json").replace("0.0004737", "2");
  const structural = parseCelestrakGpResponse(bytes(invalid), F.Json).rejected[0].reason;
  assert.equal(structural.kind, "structural");
  assert.deepEqual(structural.detail, {
    kind: "invalidElementSet",
    message: "invalid SGP4 input eccentricity: out of range",
    cause: {
      kind: "invalidInput",
      field: "eccentricity",
      inputKind: "outOfRange",
      reason: "out of range",
    },
  });
});

test("GP framing distinguishes stray text and an incomplete record", () => {
  const [name, line1, line2] = fixture("25544.tle").trimEnd().split(/\r?\n/);
  const stray = `${line1}\n${line2}\nSTRAY\n${line1}\n${line2}`;
  const rejected = parseCelestrakGpResponse(bytes(stray), F.TwoLine).rejected[0];
  assert.equal(rejected.lineNumber, 3);
  assert.equal(rejected.reason.kind, "structural");
  assert.deepEqual(rejected.reason.detail, { kind: "unexpectedContent" });
  const incomplete = `${name}\n${line1}\n${line2}\nNEXT SAT`;
  const last = parseCelestrakGpResponse(bytes(incomplete), F.Tle).rejected[0];
  assert.equal(last.lineNumber, 4);
  assert.deepEqual(last.raw, bytes("NEXT SAT"));
  assert.deepEqual(last.reason.detail, { kind: "incompleteRecord" });
});

test("GP response validates format and bytes", () => {
  assert.throws(
    () => parseCelestrakGpResponse(bytes(""), 999),
    (e) => e.name === "Error" && e.message === "invalid enum value passed",
  );
  assert.throws(
    () => parseCelestrakGpResponse(Uint8Array.of(255), F.Json),
    (e) => {
      assert.deepEqual(e.detail, {
        kind: "invalidCelestrakGpResponse",
        reason: "invalid utf-8 sequence of 1 bytes from index 0",
        line: null,
        column: null,
      });
      return true;
    },
  );
  const source = fixture("25544.tle");
  const bom = new Uint8Array([0xef, 0xbb, 0xbf, ...bytes(source)]);
  assert.deepEqual(parseCelestrakGpResponse(bom, F.Tle).records[0].raw, bytes(source));
});
