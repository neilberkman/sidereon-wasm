// CCSDS CDM binding reproduces the engine parse/encode surface, against cdm.json
// plus the committed CCSDS example KVN/XML files.

import { test } from "node:test";
import assert from "node:assert/strict";

import { Cdm, CdmObject, parseCdmKvn, parseCdmXml } from "../pkg-node/sidereon.js";
import { fixtureText, fixtureJson, hexToF64, f64Bits } from "./helpers.mjs";

const FX = fixtureJson("cdm.json");
const KVN = fixtureText(`cdm/${FX.kvn_fixture.split("/").pop()}`);
const XML = fixtureText(`cdm/${FX.xml_fixture.split("/").pop()}`);
const eqBits = (value, hex) => assert.equal(f64Bits(value), BigInt(hex));
const vec = (hexList) => Float64Array.from(hexList.map(hexToF64));

const assertObject = (obj, ref) => {
  assert.equal(obj.objectDesignator, ref.object_designator);
  assert.equal(obj.catalogName, ref.catalog_name);
  assert.equal(obj.objectName, ref.object_name);
  assert.equal(obj.internationalDesignator, ref.international_designator);
  assert.equal(obj.objectType, ref.object_type);
  assert.equal(obj.refFrame, ref.ref_frame);
  obj.positionKm.forEach((v, i) => eqBits(v, ref.position_km_hex[i]));
  obj.velocityKmS.forEach((v, i) => eqBits(v, ref.velocity_km_s_hex[i]));
  obj.covarianceRtn.forEach((v, i) => eqBits(v, ref.covariance_rtn_hex[i]));
};

const assertCdm = (cdm, ref) => {
  assert.equal(cdm.creationDate, ref.creation_date);
  assert.equal(cdm.originator, ref.originator);
  assert.equal(cdm.messageId, ref.message_id);
  assert.equal(cdm.tca, ref.tca);
  eqBits(cdm.missDistanceM, ref.miss_distance_m_hex);
  eqBits(cdm.relativeSpeedMS, ref.relative_speed_m_s_hex);
  eqBits(cdm.collisionProbability, ref.collision_probability_hex);
  assert.equal(cdm.collisionProbabilityMethod, ref.collision_probability_method);
  assert.equal(cdm.hardBodyRadiusM, undefined);
  assertObject(cdm.object1, ref.object1);
  assertObject(cdm.object2, ref.object2);
};

test("parse CDM KVN matches reference fields and re-encodes", () => {
  const cdm = parseCdmKvn(KVN);
  assertCdm(cdm, FX.from_kvn);
  assert.equal(cdm.toKvnString(), FX.encoded_kvn);
});

test("parse CDM XML matches reference fields and re-encodes", () => {
  const cdm = parseCdmXml(XML);
  assertCdm(cdm, FX.from_xml);
  assert.equal(cdm.toXmlString(), FX.encoded_xml);
});

test("KVN and XML share the same orbital content", () => {
  const kvn = parseCdmKvn(KVN);
  const xml = parseCdmXml(XML);
  kvn.object1.positionKm.forEach((v, i) =>
    assert.equal(f64Bits(v), f64Bits(xml.object1.positionKm[i])),
  );
  kvn.object2.velocityKmS.forEach((v, i) =>
    assert.equal(f64Bits(v), f64Bits(xml.object2.velocityKmS[i])),
  );
  assert.equal(kvn.collisionProbability, xml.collisionProbability);
});

test("constructed CDM encodes like parsed KVN", () => {
  // Rebuild every object and message field from the parsed CDM's getters (the
  // full CCSDS metadata block plus the velocity covariance), so a from-scratch
  // construction reproduces the canonical encoding byte-for-byte.
  const parsed = parseCdmKvn(KVN);
  const mkObj = (o) =>
    new CdmObject(o.positionKm, o.velocityKmS, o.covarianceRtn, {
      objectDesignator: o.objectDesignator,
      catalogName: o.catalogName,
      objectName: o.objectName,
      internationalDesignator: o.internationalDesignator,
      objectType: o.objectType,
      operatorContactPosition: o.operatorContactPosition,
      operatorOrganization: o.operatorOrganization,
      operatorPhone: o.operatorPhone,
      operatorEmail: o.operatorEmail,
      ephemerisName: o.ephemerisName,
      covarianceMethod: o.covarianceMethod,
      maneuverable: o.maneuverable,
      orbitCenter: o.orbitCenter,
      refFrame: o.refFrame,
      gravityModel: o.gravityModel,
      atmosphericModel: o.atmosphericModel,
      nBodyPerturbations: o.nBodyPerturbations,
      solarRadPressure: o.solarRadPressure,
      earthTides: o.earthTides,
      intrackThrust: o.intrackThrust,
      velocityCovarianceRtn: o.velocityCovarianceRtn,
      dragCovarianceRtn: o.dragCovarianceRtn,
      srpCovarianceRtn: o.srpCovarianceRtn,
      thrustCovarianceRtn: o.thrustCovarianceRtn,
      metadataComments: o.metadataComments,
      odParameters: o.odParameters,
      additionalParameters: o.additionalParameters,
      stateComments: o.stateComments,
      covarianceComments: o.covarianceComments,
    });
  const cdm = new Cdm(mkObj(parsed.object1), mkObj(parsed.object2), {
    ccsdsCdmVers: parsed.ccsdsCdmVers,
    comments: parsed.comments,
    creationDate: parsed.creationDate,
    originator: parsed.originator,
    messageFor: parsed.messageFor,
    messageId: parsed.messageId,
    relativeComments: parsed.relativeComments,
    tca: parsed.tca,
    missDistanceM: parsed.missDistanceM,
    relativeSpeedMS: parsed.relativeSpeedMS,
    relativePositionRtnM: parsed.relativePositionRtnM,
    relativeVelocityRtnMS: parsed.relativeVelocityRtnMS,
    startScreenPeriod: parsed.startScreenPeriod,
    stopScreenPeriod: parsed.stopScreenPeriod,
    screenVolumeFrame: parsed.screenVolumeFrame,
    screenVolumeShape: parsed.screenVolumeShape,
    screenVolumeM: parsed.screenVolumeM,
    screenEntryTime: parsed.screenEntryTime,
    screenExitTime: parsed.screenExitTime,
    collisionProbability: parsed.collisionProbability,
    collisionProbabilityMethod: parsed.collisionProbabilityMethod,
    hardBodyRadiusM: parsed.hardBodyRadiusM,
  });
  assert.equal(cdm.toKvnString(), FX.encoded_kvn);
  assert.equal(cdm.toKvnString(), parsed.toKvnString());
});

test("CDM retains the relative state, screening volume, OD and additional parameters", () => {
  const cdm = parseCdmKvn(KVN);
  assert.equal(cdm.ccsdsCdmVers, "1.0");
  assert.equal(cdm.messageFor, "SATELLITE A");
  assert.deepEqual(cdm.relativeComments, ["Relative Metadata/Data"]);
  assert.deepEqual(cdm.relativePositionRtnM, [27.4, -70.2, 711.8]);
  assert.deepEqual(cdm.relativeVelocityRtnMS, [-7.2, -14692, -1437.2]);
  assert.equal(cdm.screenVolumeFrame, "RTN");
  assert.equal(cdm.screenVolumeShape, "ELLIPSOID");
  assert.deepEqual(cdm.screenVolumeM, [200, 1000, 1000]);
  assert.equal(cdm.screenEntryTime, "2010-03-13T22:37:52.222");
  assert.equal(cdm.screenExitTime, "2010-03-13T22:37:52.824");

  const od = cdm.object1.odParameters;
  assert.equal(od.timeLastobStart, "2010-03-12T02:14:12.746");
  assert.equal(od.recommendedOdSpanD, 7.88);
  assert.equal(od.obsAvailable, 592);
  assert.equal(od.obsUsed, 579);
  assert.equal(od.tracksAvailable, 123);
  assert.equal(od.tracksUsed, 119);
  assert.equal(od.residualsAcceptedPct, 97.8);
  assert.equal(od.weightedRms, 0.864);
  const extra = cdm.object1.additionalParameters;
  assert.equal(extra.areaPcM2, 5.2);
  assert.equal(extra.massKg, 251.6);
  assert.equal(extra.sedrWKg, 0.000045457);
  assert.equal(cdm.object2.additionalParameters.massKg, null);
  assert.equal(cdm.object1.velocityCovarianceRtn.length, 15);
  assert.equal(cdm.object1.dragCovarianceRtn, undefined);
});

test("CdmObject.toCovarianceRtn returns the validated rows the object holds", () => {
  const cdm = parseCdmKvn(KVN);
  // Both objects state position and velocity rows. Object 2's 6x6 matrix is
  // positive definite.
  const rows = cdm.object2.toCovarianceRtn();
  assert.equal(rows.length, 6);
  rows.forEach((row) => assert.equal(row.length, 6));
  assert.equal(rows[0][0], 1337);
  assert.equal(rows[3][3], 6.886e-5);
  for (let i = 0; i < 6; i++) {
    for (let j = 0; j < i; j++) assert.equal(rows[i][j], rows[j][i]);
  }
  // Object 1's 6x6 matrix in the CCSDS example is indefinite: eliminating in
  // exact arithmetic, the pivot of the RDOT row is -0.0061 (CRDOT_T = -5.476
  // against CT_T = 2533 and CRDOT_RDOT = 0.005744). The rows are kept as read
  // and the validated view refuses them.
  assert.equal(cdm.object1.velocityCovarianceRtn.length, 15);
  assert.throws(
    () => cdm.object1.toCovarianceRtn(),
    (e) =>
      e.name === "CdmError" &&
      e.detail.kind === "INVALID_FIELD" &&
      e.detail.field === "covariance_rtn",
  );
});

test("CDM readers and writers refuse with a typed CdmError", () => {
  const unknown = KVN.replace("MESSAGE_ID", "COMMENTS = x\nMESSAGE_ID");
  let caught;
  try {
    parseCdmKvn(unknown);
  } catch (e) {
    caught = e;
  }
  assert.ok(caught instanceof Error);
  assert.equal(caught.name, "CdmError");
  assert.equal(caught.detail.kind, "UNKNOWN_FIELD");
  assert.equal(caught.detail.field, "COMMENTS");

  const parsed = parseCdmKvn(KVN);
  const obj = (o) =>
    new CdmObject(o.positionKm, o.velocityKmS, o.covarianceRtn, { objectName: "A\nB" });
  const bad = new Cdm(obj(parsed.object1), obj(parsed.object2), {});
  try {
    bad.toKvnString();
    assert.fail("expected a refusal");
  } catch (e) {
    assert.equal(e.name, "CdmError");
    assert.equal(e.detail.kind, "UNWRITABLE_TEXT");
    assert.equal(e.detail.field, "OBJECT_NAME");
    assert.equal(e.detail.issue, "lineBreak");
  }
  assert.throws(
    () => new Cdm(obj(parsed.object1), obj(parsed.object2), { missDistance: 1 }),
    TypeError,
  );
});

test("CdmObject round-trips the full metadata block and velocity covariance", () => {
  const ref = FX.from_kvn;
  const velocityCovarianceRtn = Array.from({ length: 15 }, (_, i) => (i + 1) * 1e-9);
  const meta = {
    objectDesignator: "1997-051A",
    catalogName: "SATCAT",
    objectName: "OBJECT ALPHA",
    internationalDesignator: "1997-051A",
    objectType: "PAYLOAD",
    operatorContactPosition: "Flight Dynamics",
    operatorOrganization: "Example Org",
    operatorPhone: "+1 555 0100",
    operatorEmail: "ops@example.test",
    ephemerisName: "EPHEM_A",
    covarianceMethod: "CALCULATED",
    maneuverable: "YES",
    orbitCenter: "EARTH",
    refFrame: "ITRF",
    gravityModel: "EGM-96: 36D 36O",
    atmosphericModel: "JACCHIA 70 DCA",
    nBodyPerturbations: "MOON, SUN",
    solarRadPressure: "YES",
    earthTides: "YES",
    intrackThrust: "NO",
    velocityCovarianceRtn,
  };
  const obj1 = new CdmObject(
    vec(ref.object1.position_km_hex),
    vec(ref.object1.velocity_km_s_hex),
    vec(ref.object1.covariance_rtn_hex),
    meta,
  );

  // Every newly exposed getter mirrors the constructed value.
  assert.equal(obj1.operatorContactPosition, meta.operatorContactPosition);
  assert.equal(obj1.operatorOrganization, meta.operatorOrganization);
  assert.equal(obj1.operatorPhone, meta.operatorPhone);
  assert.equal(obj1.operatorEmail, meta.operatorEmail);
  assert.equal(obj1.ephemerisName, meta.ephemerisName);
  assert.equal(obj1.covarianceMethod, meta.covarianceMethod);
  assert.equal(obj1.maneuverable, meta.maneuverable);
  assert.equal(obj1.orbitCenter, meta.orbitCenter);
  assert.equal(obj1.gravityModel, meta.gravityModel);
  assert.equal(obj1.atmosphericModel, meta.atmosphericModel);
  assert.equal(obj1.nBodyPerturbations, meta.nBodyPerturbations);
  assert.equal(obj1.solarRadPressure, meta.solarRadPressure);
  assert.equal(obj1.earthTides, meta.earthTides);
  assert.equal(obj1.intrackThrust, meta.intrackThrust);
  assert.equal(obj1.velocityCovarianceRtn.length, 15);
  obj1.velocityCovarianceRtn.forEach((v, i) =>
    assert.equal(f64Bits(v), f64Bits(velocityCovarianceRtn[i])),
  );

  // An object without a velocity-covariance block reports undefined for it.
  const obj2 = new CdmObject(
    vec(ref.object2.position_km_hex),
    vec(ref.object2.velocity_km_s_hex),
    vec(ref.object2.covariance_rtn_hex),
    { objectDesignator: ref.object2.object_designator, refFrame: ref.object2.ref_frame },
  );
  assert.equal(obj2.velocityCovarianceRtn, undefined);

  // The full metadata block survives a KVN encode -> parse round trip.
  const cdm = new Cdm(obj1, obj2, {
    creationDate: ref.creation_date,
    originator: ref.originator,
    messageId: ref.message_id,
    tca: ref.tca,
    missDistanceM: hexToF64(ref.miss_distance_m_hex),
    relativeSpeedMS: hexToF64(ref.relative_speed_m_s_hex),
    collisionProbability: hexToF64(ref.collision_probability_hex),
    collisionProbabilityMethod: ref.collision_probability_method,
  });
  const reparsed = parseCdmKvn(cdm.toKvnString());
  const ro = reparsed.object1;
  assert.equal(ro.operatorOrganization, meta.operatorOrganization);
  assert.equal(ro.atmosphericModel, meta.atmosphericModel);
  assert.equal(ro.maneuverable, meta.maneuverable);
  assert.ok(ro.velocityCovarianceRtn, "velocity covariance survives the round trip");
  ro.velocityCovarianceRtn.forEach((v, i) =>
    assert.equal(f64Bits(v), f64Bits(velocityCovarianceRtn[i])),
  );
});

test("CdmObject rejects a wrong-length velocity covariance", () => {
  const ref = FX.from_kvn.object1;
  assert.throws(
    () =>
      new CdmObject(
        vec(ref.position_km_hex),
        vec(ref.velocity_km_s_hex),
        vec(ref.covariance_rtn_hex),
        { velocityCovarianceRtn: [1, 2, 3] },
      ),
    TypeError,
  );
});

test("CDM parse and shape errors throw", () => {
  assert.throws(() => parseCdmKvn("OBJECT = OBJECT1\nX = 1.0 [km]\n"));
  assert.throws(() => parseCdmXml("<segment></segment><segment></segment>"));
  assert.throws(() => new CdmObject(new Float64Array(2), new Float64Array(3), new Float64Array(6)));
  assert.throws(() => new CdmObject(new Float64Array(3), new Float64Array(3), new Float64Array(5)));
});
