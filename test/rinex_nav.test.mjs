// RINEX NAV parsing through the WASM binding, mirroring
// sidereon-python/tests/test_rinex_nav.py. Broadcast evaluation is checked
// bit-exact against the core broadcast_golden.json.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  parseRinexNav,
  loadRinexNav,
  parseRinexNavRecords,
  parseRinexGlonassRecords,
  parseRinexIonoCorrections,
  parseRinexLeapSeconds,
  BroadcastDelayTerm,
  CnavSignal,
  NavMessage,
  cnavUraNominalM,
  civilToJ2000Seconds,
  navMessageLabel,
} from "../pkg-node/sidereon.js";

import { fixture, fixtureJson, f64Bits, hexToF64, norm } from "./helpers.mjs";

const ESBC = "nav/ESBC00DNK_R_20201770000_01D_MN.rnx";
const BRDC = "nav/BRDC00GOP_R_20210010000_01D_MN.rnx";
const KMS = "nav/KMS300DNK_R_20221591000_01H_MN.rnx";
const BRD4 = "nav/BRD400DLR_S_20261800000_01H_MN_trim.rnx";

const MESSAGE_LABEL_BY_GOLDEN = {
  GPS_LNAV: "gps_lnav",
  GAL_INAV: "galileo_inav",
  GAL_FNAV: "galileo_fnav",
  BDS_D1: "beidou_d1",
  BDS_D2: "beidou_d2",
};

const countBy = (items, key) => {
  const counts = {};
  for (const item of items) counts[key(item)] = (counts[key(item)] ?? 0) + 1;
  return counts;
};

test("mixed nav records and default store parse from the fixture", () => {
  const records = parseRinexNavRecords(fixture(ESBC));
  assert.equal(records.length, 2216);
  assert.deepEqual(
    countBy(records, (r) => r.satellite[0]),
    { G: 257, E: 1602, C: 357 },
  );

  const galileoMessages = countBy(
    records.filter((r) => r.satellite.startsWith("E")),
    (r) => navMessageLabel(r.message),
  );
  assert.equal(galileoMessages["galileo_inav"], 821);
  assert.equal(galileoMessages["galileo_fnav"], 781);

  const g01 = records.find((r) => r.satellite === "G01");
  assert.equal(g01.message, NavMessage.GpsLnav);
  assert.equal(g01.week, 2111);
  assert.ok(g01.elements.sqrtA > 5100.0 && g01.elements.sqrtA < 5200.0);
  assert.ok(g01.elements.e > 0.0 && g01.elements.e < 0.05);
  assert.equal(g01.clock.tocSow, g01.elements.toeSow);
  assert.equal(g01.fitIntervalS, 14400.0);

  const store = parseRinexNav(fixture(ESBC));
  assert.equal(store.leapSeconds, 18.0);
  assert.ok(store.recordCount > 0);
  assert.equal(store.glonassRecordCount, 0);
  assert.ok(store.records.every((r) => r.svHealth === 0.0));
  assert.ok(store.records.every((r) => r.message !== NavMessage.GalileoFnav));
  assert.ok(store.records.some((r) => r.satellite === "C05"));

  const iono = store.ionoCorrections;
  assert.ok(iono.gps.alpha instanceof Float64Array);
  assert.equal(iono.gps.alpha.length, 4);
  assert.equal(iono.gps.beta.length, 4);
  assert.ok(Math.abs(iono.gps.alpha[0] - 4.6566e-9) < 1e-19);
  assert.ok(Math.abs(iono.gps.beta[0] - 8.192e4) < 1e-3);
  assert.equal(iono.beidou, undefined);
});

test("brdc header ionosphere and glonass records parse", () => {
  const records = parseRinexNavRecords(fixture(BRDC));
  assert.deepEqual(
    countBy(records, (r) => r.satellite[0]),
    { C: 1, E: 1 },
  );

  const glonass = parseRinexGlonassRecords(fixture(BRDC));
  assert.equal(glonass.length, 1);
  const r10 = glonass[0];
  assert.equal(r10.satellite, "R10");
  assert.equal(r10.svHealth, 0.0);
  assert.equal(r10.positionM.length, 3);
  assert.equal(r10.velocityMS.length, 3);
  assert.equal(r10.accelerationMS2.length, 3);
  assert.ok(norm(r10.positionM) > 25_000_000.0 && norm(r10.positionM) < 26_000_000.0);

  const iono = parseRinexIonoCorrections(fixture(BRDC));
  assert.notEqual(iono.gps, undefined);
  assert.notEqual(iono.beidou, undefined);
  const beAlpha = [1.118e-8, 2.98e-8, -4.172e-7, 6.557e-7];
  const beBeta = [1.413e5, -5.243e5, 1.638e6, -4.588e5];
  for (let i = 0; i < 4; i++) {
    assert.ok(
      Math.abs(iono.beidou.alpha[i] - beAlpha[i]) <
        1e-15 * Math.max(1, Math.abs(beAlpha[i])) + 1e-18,
    );
    assert.ok(Math.abs(iono.beidou.beta[i] - beBeta[i]) < 1e-3);
  }
  assert.equal(parseRinexLeapSeconds(fixture(BRDC)), 18.0);

  const fromBytes = loadRinexNav(fixture(BRDC));
  assert.equal(fromBytes.glonassRecordCount, 1);
  assert.equal(fromBytes.glonassRecords[0].satellite, "R10");
});

test("rinex v4 nav records parse", () => {
  const records = parseRinexNavRecords(fixture(KMS));
  assert.equal(records.length, 175);
  assert.deepEqual(
    countBy(records, (r) => r.satellite[0]),
    { G: 30, E: 108, C: 36, J: 1 },
  );
  assert.deepEqual(
    countBy(records, (r) => navMessageLabel(r.message)),
    {
      gps_lnav: 30,
      galileo_inav: 55,
      galileo_fnav: 53,
      beidou_d1: 33,
      beidou_d2: 3,
      qzss_lnav: 1,
    },
  );

  const store = parseRinexNav(fixture(KMS));
  assert.ok(store.recordCount > 0);
  assert.equal(store.leapSeconds, 18.0);
});

test("CNAV/RINEX-4 record evaluation exposes URA and ISC terms", () => {
  const records = parseRinexNavRecords(fixture(BRD4));
  assert.equal(records.length, 7);
  assert.deepEqual(
    countBy(records, (r) => navMessageLabel(r.message)),
    {
      gps_lnav: 2,
      gps_cnav: 2,
      qzss_lnav: 1,
      qzss_cnav: 1,
      qzss_cnav2: 1,
    },
  );

  const record = records.find(
    (r) => r.satellite === "G01" && navMessageLabel(r.message) === "gps_cnav",
  );
  assert.equal(record.week, 2425);
  assert.equal(record.elements.toeSow, 91800);
  assert.equal(record.issue, 306);
  assert.equal(record.issueMessage, NavMessage.GpsCnav);
  assert.notEqual(record.cnav, undefined);

  assert.equal(f64Bits(record.cnav.adotMS), 0x3f629ffffffffb7fn);
  assert.equal(f64Bits(record.cnav.deltaN0DotRadS2), 0xbd2006eb857c7c91n);
  assert.equal(record.cnav.topWeek, 2424);
  assert.equal(record.cnav.topTowS, 603900);
  assert.equal(record.cnav.uraEdIndex, 0);
  assert.equal(f64Bits(record.cnav.uraNedM(record.week, record.cnav.topTowS)), 0x406f1fc2e2000000n);
  assert.equal(cnavUraNominalM(15), undefined);
  assert.equal(cnavUraNominalM(3), 5.7);

  assert.equal(
    f64Bits(record.groupDelays.cnavSingleFrequencyCorrectionS(CnavSignal.L1Ca)),
    0xbe425ffffffffffan,
  );
  assert.equal(
    f64Bits(record.groupDelays.get(BroadcastDelayTerm.CnavIscL1Ca)),
    0xbdf3fffffffffd34n,
  );

  const state = record.evaluate(record.elements.toeSow);
  assert.equal(f64Bits(state.clockS), 0x3f2ee71f5f4100cdn);
  assert.deepEqual(
    [state.xM, state.yM, state.zM].map((x) => `0x${f64Bits(x).toString(16).padStart(16, "0")}`),
    ["0xc1746a00f3ea856e", "0xc16e037eb4cce2ab", "0xc1364141082327d0"],
  );
});

test("BroadcastEphemeris.evaluate selects a store record by GPST-like J2000 seconds", () => {
  const store = parseRinexNav(fixture(BRD4));
  const record = store.records[0];
  const gpsEpoch = civilToJ2000Seconds(1980, 1, 6, 0, 0, 0);
  const query = gpsEpoch + record.week * 604800 + record.elements.toeSow;
  const state = store.evaluate(record.satellite, query);

  assert.equal(state.satellite, "G01");
  assert.equal(state.tJ2000S, 835970400);
  assert.equal(f64Bits(state.clockS), 0x3f2ee69457945987n);
  assert.deepEqual(
    Array.from(state.positionM, (x) => `0x${f64Bits(x).toString(16).padStart(16, "0")}`),
    ["0xc1735dc3f3f5a2d5", "0xc16dee83fc993d03", "0xc15ac9a1aad952c8"],
  );
});

test("broadcast record evaluate matches the core golden bit-exact", () => {
  const doc = fixtureJson("broadcast_golden.json");
  const records = parseRinexNavRecords(fixture(ESBC));

  for (const name of ["gps_at_toe", "gal_plus_2h", "bds_geo_plus_2h", "bds_meo_week_fold"]) {
    const c = doc.cases.find((entry) => entry.name === name);
    const label = MESSAGE_LABEL_BY_GOLDEN[c.message];
    const expectedElements = c.elements_hex;

    const matches = records.filter(
      (r) =>
        r.satellite === c.sat &&
        navMessageLabel(r.message) === label &&
        f64Bits(r.elements.toeSow) === BigInt(expectedElements.toe_sow) &&
        f64Bits(r.elements.sqrtA) === BigInt(expectedElements.sqrt_a) &&
        f64Bits(r.elements.e) === BigInt(expectedElements.e),
    );
    assert.equal(matches.length, 1, `exactly one record for ${name}`);
    const record = matches[0];

    const tSow = hexToF64(c.t_sow_hex);
    const state = record.evaluate(tSow);
    const expect = c.expect_hex;

    assert.ok(state.positionM instanceof Float64Array);
    assert.equal(state.positionM.length, 3);

    // Bit-exact: the echoed epoch, the satellite clock decomposition, and the
    // Kepler iteration count are all pure arithmetic and agree to the bit with
    // the natively generated golden.
    assert.equal(f64Bits(state.tSowS), BigInt(c.t_sow_hex));
    assert.equal(f64Bits(state.clockS), BigInt(expect.dt_clock_total_s));
    assert.equal(f64Bits(state.clockPolynomialS), BigInt(expect.dt_clock_poly_s));
    assert.equal(f64Bits(state.relativisticClockS), BigInt(expect.dt_rel_s));
    assert.equal(f64Bits(state.groupDelayS), BigInt(expect.tgd_s));
    assert.equal(state.keplerIterations, c.kepler_iterations);

    // The ECEF position is assembled through the orbital-plane rotation, whose
    // sin/cos/sqrt are evaluated by the wasm32 libm. That libm differs from the
    // native libm the golden was generated with by up to ~2 ULP, so the three
    // position components are checked to a sub-micron tolerance rather than the
    // bit (root cause is the wasm math library, not the binding marshalling).
    const expectedPos = [expect.x_m, expect.y_m, expect.z_m].map(hexToF64);
    const got = [state.xM, state.yM, state.zM];
    for (let i = 0; i < 3; i++) {
      assert.ok(Math.abs(got[i] - expectedPos[i]) < 1e-6, `position[${i}] within 1e-6 m`);
      assert.equal(state.positionM[i], got[i]);
    }
  }
});

test("evaluate rejects a non-finite epoch and parse errors are typed", () => {
  const record = parseRinexNavRecords(fixture(ESBC))[0];
  assert.throws(() => record.evaluate(NaN), RangeError);

  const bogus =
    "     3.05           OBSERVATION DATA   M                   RINEX VERSION / TYPE\n" +
    "                                                            END OF HEADER\n";
  assert.throws(() => parseRinexNavRecords(Buffer.from(bogus, "utf8")), Error);
});

test("toRinexString re-parses to the same broadcast records", () => {
  const nav = parseRinexNav(fixture(BRDC));
  const text = nav.toRinexString();
  const reparsed = parseRinexNav(Buffer.from(text, "utf8"));
  assert.equal(reparsed.recordCount, nav.recordCount);
  // Deterministic: re-encoding the re-parsed records is byte-identical.
  assert.equal(reparsed.toRinexString(), text);
});
