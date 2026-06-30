// CCSDS OMM binding reproduces core KVN/XML/JSON parse and encode, against
// omm.json plus the committed CelesTrak OMM files (near-Earth + deep-space).

import { test } from "node:test";
import assert from "node:assert/strict";

import { Omm, OmmEpoch, parseOmmKvn, parseOmmXml, parseOmmJson } from "../pkg-node/sidereon.js";
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
  assert.equal(omm.ccsdsOmmVers, ref.ccsds_omm_vers);
  assert.equal(opt(omm.creationDate), ref.creation_date);
  assert.equal(opt(omm.originator), ref.originator);
  assert.equal(opt(omm.objectName), ref.object_name);
  assert.equal(opt(omm.objectId), ref.object_id);
  assert.equal(opt(omm.centerName), ref.center_name);
  assert.equal(opt(omm.refFrame), ref.ref_frame);
  assert.equal(opt(omm.timeSystem), ref.time_system);
  assert.equal(opt(omm.meanElementTheory), ref.mean_element_theory);
  assertEpoch(omm.epoch, ref.epoch);
  eqBits(omm.meanMotion, ref.mean_motion_hex);
  eqBits(omm.eccentricity, ref.eccentricity_hex);
  eqBits(omm.inclinationDeg, ref.inclination_deg_hex);
  eqBits(omm.raOfAscNodeDeg, ref.ra_of_asc_node_deg_hex);
  eqBits(omm.argOfPericenterDeg, ref.arg_of_pericenter_deg_hex);
  eqBits(omm.meanAnomalyDeg, ref.mean_anomaly_deg_hex);
  assert.equal(omm.ephemerisType, ref.ephemeris_type);
  assert.equal(omm.classificationType, ref.classification_type);
  assert.equal(omm.noradCatId, ref.norad_cat_id);
  assert.equal(omm.elementSetNo, ref.element_set_no);
  assert.equal(omm.revAtEpoch, BigInt(ref.rev_at_epoch));
  eqBits(omm.bstar, ref.bstar_hex);
  eqBits(omm.meanMotionDot, ref.mean_motion_dot_hex);
  eqBits(omm.meanMotionDdot, ref.mean_motion_ddot_hex);
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

test("OMM parse and constructor errors throw", () => {
  assert.throws(() => parseOmmKvn("CCSDS_OMM_VERS = 2.0\n"));
  assert.throws(() => parseOmmXml("<not xml"));
  assert.throws(() => parseOmmJson("{}"));
  assert.throws(() => new OmmEpoch(2026, 13, 1, 0, 0, 0, 0));
  assert.throws(() => new Omm(new OmmEpoch(2026, 1, 1, 0, 0, 0, 0), NaN, 0, 0, 0, 0, 0, 1));
});
