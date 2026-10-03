// CCSDS OMM binding reproduces core KVN/XML/JSON parse and encode, against
// omm.json plus the committed CelesTrak OMM files (near-Earth + deep-space).

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  Omm,
  OmmEpoch,
  Sgp4Satellite,
  Tle,
  parseOmmCsv,
  parseOmmCsvArray,
  parseOmmJson,
  parseOmmJsonArray,
  parseOmmKvn,
  parseOmmXml,
  parseOmmXmlAll,
} from "../pkg-node/sidereon.js";
import { fixtureText, fixtureJson, hexToF64, f64Bits } from "./helpers.mjs";

const FX = fixtureJson("omm.json");
const eqBits = (value, hex) => assert.equal(f64Bits(value), BigInt(hex));
const load = (rel) => fixtureText(`omm/${rel.split("/").pop()}`);

const assertEpoch = (epoch, ref) => {
  assert.equal(epoch.year, ref.year);
  assert.equal(epoch.month, ref.month);
  assert.equal(epoch.day, ref.day);
  assert.equal(epoch.hour, ref.hour);
  assert.equal(epoch.minute, ref.minute);
  assert.equal(epoch.second, ref.second);
  assert.equal(epoch.microsecond, ref.microsecond);
  assert.equal(epoch.iso8601, ref.iso8601);
};

// The core uses None for an absent optional field, which wasm-bindgen surfaces
// as `undefined`; the JSON fixture stores it as `null`. Normalize before
// comparing so the two spellings of "absent" match.
const opt = (v) => v ?? null;

const assertOmm = (omm, ref) => {
  assert.equal(opt(omm.ccsdsOmmVers), ref.ccsds_omm_vers);
  assert.equal(opt(omm.classification), opt(ref.classification));
  assert.equal(opt(omm.creationDate), ref.creation_date);
  assert.equal(opt(omm.originator), ref.originator);
  assert.equal(opt(omm.messageId), opt(ref.message_id));
  assert.equal(opt(omm.objectName), ref.object_name);
  assert.equal(opt(omm.objectId), ref.object_id);
  assert.equal(opt(omm.centerName), ref.center_name);
  assert.equal(opt(omm.refFrame), ref.ref_frame);
  assert.equal(opt(omm.refFrameEpoch), opt(ref.ref_frame_epoch));
  assert.equal(opt(omm.timeSystem), ref.time_system);
  assert.equal(opt(omm.meanElementTheory), ref.mean_element_theory);
  assertEpoch(omm.epoch, ref.epoch);
  eqBits(omm.meanMotion, ref.mean_motion_hex);
  assert.equal(opt(omm.semiMajorAxisKm), opt(ref.semi_major_axis_km));
  eqBits(omm.eccentricity, ref.eccentricity_hex);
  eqBits(omm.inclinationDeg, ref.inclination_deg_hex);
  eqBits(omm.raOfAscNodeDeg, ref.ra_of_asc_node_deg_hex);
  eqBits(omm.argOfPericenterDeg, ref.arg_of_pericenter_deg_hex);
  eqBits(omm.meanAnomalyDeg, ref.mean_anomaly_deg_hex);
  assert.equal(opt(omm.gmKm3S2), opt(ref.gm_km3_s2));
  assert.deepEqual(opt(omm.spacecraft), opt(ref.spacecraft));
  assert.equal(omm.ephemerisType, ref.ephemeris_type);
  assert.equal(omm.classificationType, ref.classification_type);
  assert.equal(omm.noradCatId, ref.norad_cat_id);
  assert.equal(omm.elementSetNo, ref.element_set_no);
  assert.equal(omm.revAtEpoch, BigInt(ref.rev_at_epoch));
  eqBits(omm.bstar, ref.bstar_hex);
  assert.equal(opt(omm.btermM2Kg), opt(ref.bterm_m2_kg));
  eqBits(omm.meanMotionDot, ref.mean_motion_dot_hex);
  eqBits(omm.meanMotionDdot, ref.mean_motion_ddot_hex);
  assert.equal(opt(omm.agomM2Kg), opt(ref.agom_m2_kg));
  assert.deepEqual(opt(omm.covariance), opt(ref.covariance));
  assert.deepEqual(omm.userDefined, ref.user_defined ?? []);
  assert.deepEqual(
    omm.comments,
    ref.comments ?? {
      header: [],
      metadata: [],
      meanElements: [],
      tleParameters: [],
      userDefined: [],
    },
  );
};

test("parse OMM KVN/XML/JSON match reference fields and re-encode", () => {
  for (const fx of FX.fixtures) {
    const kvn = parseOmmKvn(load(fx.kvn_fixture));
    const xml = parseOmmXml(load(fx.xml_fixture));
    const json = parseOmmJson(load(fx.json_fixture));

    assertOmm(kvn, fx.from_kvn);
    assertOmm(xml, fx.from_xml);
    assertOmm(json, fx.from_json);

    assert.equal(kvn.toKvnString(), fx.encoded_kvn);
    assert.equal(xml.toXmlString(), fx.encoded_xml);
    assert.deepEqual(JSON.parse(json.toJsonString()), JSON.parse(fx.encoded_json));
  }
});

test("OMM encodings share orbital content", () => {
  for (const fx of FX.fixtures) {
    const kvn = parseOmmKvn(load(fx.kvn_fixture));
    const xml = parseOmmXml(load(fx.xml_fixture));
    const json = parseOmmJson(load(fx.json_fixture));
    for (const other of [xml, json]) {
      assert.equal(other.noradCatId, kvn.noradCatId);
      assert.equal(f64Bits(other.meanMotion), f64Bits(kvn.meanMotion));
      assert.equal(f64Bits(other.eccentricity), f64Bits(kvn.eccentricity));
      assert.equal(f64Bits(other.bstar), f64Bits(kvn.bstar));
    }
  }
});

test("constructed OMM matches parsed KVN encoding", () => {
  const ref = FX.fixtures[0].from_kvn;
  const e = ref.epoch;
  const epoch = new OmmEpoch(e.year, e.month, e.day, e.hour, e.minute, e.second, e.microsecond);
  const omm = new Omm(
    epoch,
    hexToF64(ref.mean_motion_hex),
    hexToF64(ref.eccentricity_hex),
    hexToF64(ref.inclination_deg_hex),
    hexToF64(ref.ra_of_asc_node_deg_hex),
    hexToF64(ref.arg_of_pericenter_deg_hex),
    hexToF64(ref.mean_anomaly_deg_hex),
    ref.norad_cat_id,
    {
      ccsdsOmmVers: ref.ccsds_omm_vers,
      creationDate: ref.creation_date,
      originator: ref.originator,
      objectName: ref.object_name,
      objectId: ref.object_id,
      centerName: ref.center_name,
      refFrame: ref.ref_frame,
      timeSystem: ref.time_system,
      meanElementTheory: ref.mean_element_theory,
      ephemerisType: ref.ephemeris_type,
      classificationType: ref.classification_type,
      elementSetNo: ref.element_set_no,
      revAtEpoch: ref.rev_at_epoch,
      bstar: hexToF64(ref.bstar_hex),
      meanMotionDot: hexToF64(ref.mean_motion_dot_hex),
      meanMotionDdot: hexToF64(ref.mean_motion_ddot_hex),
    },
  );
  assert.equal(omm.toKvnString(), FX.fixtures[0].encoded_kvn);
});

test("constructed OMM retains every public metadata field", () => {
  const ref = FX.fixtures[0].from_kvn;
  const e = ref.epoch;
  const epoch = new OmmEpoch(e.year, e.month, e.day, e.hour, e.minute, e.second, e.microsecond);
  const retained = {
    classification: "C",
    messageId: "OMM-RETAINED-METADATA",
    refFrameEpoch: "2026-06-17T06:00:00.000000Z",
    semiMajorAxisKm: 26_560.5,
    gmKm3S2: 398_600.5,
    spacecraft: {
      comments: ["spacecraft retained"],
      massKg: 1_500.5,
      solarRadAreaM2: 12.25,
      solarRadCoeff: 1.75,
      dragAreaM2: 8.5,
      dragCoeff: 2.25,
    },
    btermM2Kg: 0.125,
    agomM2Kg: 0.25,
    covariance: {
      comments: ["covariance retained"],
      covRefFrame: "RTN",
      lowerTriangle: Array.from({ length: 21 }, (_, index) => (index + 1) / 8),
    },
    userDefined: [
      { parameter: "OWNER", value: "SIDEREON" },
      { parameter: "PURPOSE", value: "ROUNDTRIP" },
    ],
    comments: {
      header: ["header retained"],
      metadata: ["metadata retained"],
      meanElements: ["mean elements retained"],
      tleParameters: ["TLE parameters retained"],
      userDefined: ["user-defined retained"],
    },
  };
  const omm = new Omm(
    epoch,
    hexToF64(ref.mean_motion_hex),
    hexToF64(ref.eccentricity_hex),
    hexToF64(ref.inclination_deg_hex),
    hexToF64(ref.ra_of_asc_node_deg_hex),
    hexToF64(ref.arg_of_pericenter_deg_hex),
    hexToF64(ref.mean_anomaly_deg_hex),
    ref.norad_cat_id,
    {
      ccsdsOmmVers: ref.ccsds_omm_vers,
      creationDate: ref.creation_date,
      originator: ref.originator,
      objectName: ref.object_name,
      objectId: ref.object_id,
      centerName: ref.center_name,
      refFrame: ref.ref_frame,
      timeSystem: ref.time_system,
      meanElementTheory: ref.mean_element_theory,
      ephemerisType: ref.ephemeris_type,
      classificationType: ref.classification_type,
      elementSetNo: ref.element_set_no,
      revAtEpoch: ref.rev_at_epoch,
      bstar: hexToF64(ref.bstar_hex),
      meanMotionDot: hexToF64(ref.mean_motion_dot_hex),
      meanMotionDdot: hexToF64(ref.mean_motion_ddot_hex),
      ...retained,
    },
  );

  assertOmm(omm, {
    ...ref,
    classification: retained.classification,
    message_id: retained.messageId,
    ref_frame_epoch: retained.refFrameEpoch,
    semi_major_axis_km: retained.semiMajorAxisKm,
    gm_km3_s2: retained.gmKm3S2,
    spacecraft: retained.spacecraft,
    bterm_m2_kg: retained.btermM2Kg,
    agom_m2_kg: retained.agomM2Kg,
    covariance: retained.covariance,
    user_defined: retained.userDefined,
    comments: retained.comments,
  });
});

test("OMM non-wire SGP4 side channels stay outside the public wrapper", () => {
  const ref = FX.fixtures[0].from_kvn;
  const e = ref.epoch;
  const epoch = new OmmEpoch(e.year, e.month, e.day, e.hour, e.minute, e.second, e.microsecond);
  const parsed = parseOmmKvn(load(FX.fixtures[0].kvn_fixture));
  const build = (extra) =>
    new Omm(
      epoch,
      hexToF64(ref.mean_motion_hex),
      hexToF64(ref.eccentricity_hex),
      hexToF64(ref.inclination_deg_hex),
      hexToF64(ref.ra_of_asc_node_deg_hex),
      hexToF64(ref.arg_of_pericenter_deg_hex),
      hexToF64(ref.mean_anomaly_deg_hex),
      ref.norad_cat_id,
      extra,
    );

  for (const [property, value] of [
    ["exactSgp4Epoch", [2_460_000, 0.25]],
    ["quantizeTleDerivedFields", false],
  ]) {
    assert.equal(property in parsed, false);
    assert.throws(
      () => build({ [property]: value }),
      (error) => error instanceof TypeError && error.message.includes(property),
    );
  }
});

test("OMM parse and constructor errors throw", () => {
  assert.throws(() => parseOmmKvn("CCSDS_OMM_VERS = 2.0\n"));
  assert.throws(() => parseOmmXml("<not xml"));
  assert.throws(() => parseOmmJson("{}"));
  assert.throws(() => new OmmEpoch(2026, 13, 1, 0, 0, 0, 0));
  assert.throws(() => new Omm(new OmmEpoch(2026, 1, 1, 0, 0, 0, 0), NaN, 0, 0, 0, 0, 0, 1));
});

const isOmmError = (kind) => (e) =>
  e instanceof Error &&
  e.name === "OmmError" &&
  e.detail.kind === kind &&
  e.detail.message === e.message;

test("Sgp4Satellite.fromOmm matches each TLE fixture within source precision", () => {
  const epochs = new BigInt64Array([
    BigInt(Date.UTC(2026, 5, 17, 6)) * 1000n,
    BigInt(Date.UTC(2026, 5, 18, 6)) * 1000n,
  ]);
  const assertWithin = (actual, expected, tolerance, message) => {
    assert.equal(actual.length, expected.length, `${message} length`);
    actual.forEach((value, index) => {
      const delta = Math.abs(value - expected[index]);
      assert.ok(delta <= tolerance, `${message}[${index}] delta ${delta}`);
    });
  };

  for (const fx of FX.fixtures) {
    const omm = parseOmmKvn(load(fx.kvn_fixture));
    const [, line1, line2] = load(fx.kvn_fixture.replace(/\.kvn$/, ".tle")).split(/\r?\n/);
    const satellite = Sgp4Satellite.fromOmm(omm);
    const empty = satellite.propagate(new BigInt64Array());
    assert.deepEqual(empty.positionKm, new Float64Array(), `${fx.name} empty position`);
    assert.deepEqual(empty.velocityKmS, new Float64Array(), `${fx.name} empty velocity`);

    const fromOmm = satellite.propagate(epochs);
    const repeated = satellite.propagate(epochs);
    const fromTle = new Tle(line1, line2).propagate(epochs);

    assert.deepEqual(repeated.positionKm, fromOmm.positionKm, `${fx.name} repeat position`);
    assert.deepEqual(repeated.velocityKmS, fromOmm.velocityKmS, `${fx.name} repeat velocity`);

    // Some OMM fixtures retain more decimal digits than their fixed-width TLE
    // partners. The resulting spread stays below 1 mm and 0.2 micrometers/s.
    assertWithin(fromOmm.positionKm, fromTle.positionKm, 1e-6, `${fx.name} position km`);
    assertWithin(fromOmm.velocityKmS, fromTle.velocityKmS, 2e-10, `${fx.name} velocity km/s`);
  }
});

test("Sgp4Satellite.fromOmm preserves typed OMM bridge failures", () => {
  const ref = FX.fixtures[0].from_kvn;
  const epoch = new OmmEpoch(
    ref.epoch.year,
    ref.epoch.month,
    ref.epoch.day,
    ref.epoch.hour,
    ref.epoch.minute,
    ref.epoch.second,
    ref.epoch.microsecond,
  );
  const baseMeta = {
    centerName: "EARTH",
    refFrame: "TEME",
    timeSystem: "UTC",
    meanElementTheory: "SGP4",
    bstar: hexToF64(ref.bstar_hex),
  };
  const build = (meanMotion, meta) =>
    new Omm(
      epoch,
      meanMotion,
      hexToF64(ref.eccentricity_hex),
      hexToF64(ref.inclination_deg_hex),
      hexToF64(ref.ra_of_asc_node_deg_hex),
      hexToF64(ref.arg_of_pericenter_deg_hex),
      hexToF64(ref.mean_anomaly_deg_hex),
      ref.norad_cat_id,
      meta,
    );
  const expectedError = (kind, field) => (error) => {
    assert.equal(error.name, "OmmError");
    assert.equal(error.detail.kind, kind);
    assert.equal(error.detail.field, field);
    return true;
  };

  assert.throws(
    () =>
      Sgp4Satellite.fromOmm(
        build(hexToF64(ref.mean_motion_hex), { ...baseMeta, meanElementTheory: "DSST" }),
      ),
    expectedError("INCOMPATIBLE_METADATA", "MEAN_ELEMENT_THEORY"),
  );
  assert.throws(
    () => Sgp4Satellite.fromOmm(build(undefined, baseMeta)),
    expectedError("MISSING_FIELD", "MEAN_MOTION"),
  );
  const withoutBstar = Object.fromEntries(
    Object.entries(baseMeta).filter(([key]) => key !== "bstar"),
  );
  assert.throws(
    () => Sgp4Satellite.fromOmm(build(hexToF64(ref.mean_motion_hex), withoutBstar)),
    expectedError("MISSING_FIELD", "BSTAR"),
  );
});

test("OMM failures are typed OmmErrors whose detail names the engine variant", () => {
  assert.throws(
    () => parseOmmKvn("CCSDS_OMM_VERS = 2.0\n"),
    (e) => {
      assert.equal(e.name, "OmmError");
      assert.equal(typeof e.detail.kind, "string");
      assert.equal(e.detail.message, e.message);
      return true;
    },
  );
  // A JSON array of several records is not one OMM.
  const records = FX.fixtures.map((fx) => JSON.parse(load(fx.json_fixture))).flat();
  assert.throws(() => parseOmmJson(JSON.stringify(records)), isOmmError("MULTIPLE_MESSAGES"));
});

test("the array readers keep every record they can read and report the rest", () => {
  const records = FX.fixtures.map((fx) => JSON.parse(load(fx.json_fixture))).flat();
  const parsed = parseOmmJsonArray(JSON.stringify([...records, { OBJECT_NAME: "BROKEN" }]));
  assert.deepEqual(
    parsed.omms.map((omm) => omm.noradCatId),
    FX.fixtures.map((fx) => fx.from_json.norad_cat_id),
  );
  assert.equal(parsed.skipped.length, 1);
  assert.equal(parsed.skipped[0].index, records.length);
  assert.equal(typeof parsed.skipped[0].reason.kind, "string");

  const xml = parseOmmXmlAll(load(FX.fixtures[0].xml_fixture));
  assert.equal(xml.omms.length, 1);
  assert.deepEqual(xml.skipped, []);
  assert.equal(
    xml.omms[0].toXmlString(),
    parseOmmXml(load(FX.fixtures[0].xml_fixture)).toXmlString(),
  );
});

test("GP CSV writes and reads back one record and a table of several", () => {
  const omms = FX.fixtures.map((fx) => parseOmmJson(load(fx.json_fixture)));
  const csv = omms.map((omm) => omm.toCsvString());
  const one = parseOmmCsv(csv[0]);
  assert.equal(one.toCsvString(), csv[0]);
  assert.equal(one.noradCatId, omms[0].noradCatId);
  eqBits(one.meanMotion, FX.fixtures[0].from_json.mean_motion_hex);
  assert.equal(omms[0].toCsvStringDiscardingComments(), csv[0]);

  // One header, then each record's data row.
  const lines = (text) => text.trimEnd().split(/\r?\n/);
  const [header] = lines(csv[0]);
  for (const text of csv) assert.equal(lines(text)[0], header);
  const rows = csv.map((text) => lines(text)[1]);
  const table = [header, ...rows].join("\n") + "\n";
  const parsed = parseOmmCsvArray(table);
  assert.deepEqual(
    parsed.omms.map((omm) => omm.noradCatId),
    omms.map((omm) => omm.noradCatId),
  );
  assert.deepEqual(parsed.skipped, []);
  assert.throws(() => parseOmmCsv(table), isOmmError("MULTIPLE_MESSAGES"));
});
