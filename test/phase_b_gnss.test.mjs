import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import {
  CarrierBand,
  DtedTerrain,
  GnssSystem,
  SbasCorrectionStore,
  SsrCorrectionStore,
  carrierBandName,
  decodeSbasMessage,
  decodeSsr,
  gnssSystemLabel,
  loadBiasSinex,
  loadBiasSinexLossy,
  loadCodeDcb,
  loadRinexNav,
  loadSp3,
  pppCorrectionsWithCodeBias,
  sampleBroadcastEphemeris,
  sampleSp3Ephemeris,
  sbasCorrectedState,
  solveSppSbas,
  ssrCorrectedState,
} from "../pkg-node/sidereon.js";
import { fixture, hexToF64 } from "./helpers.mjs";

const CORE_FIXTURES = "/tmp/sid-integration/crates/sidereon-core/tests/fixtures";
const C_M_S = 299792458.0;
const F_L1_HZ = 1575.42e6;
const F_L2_HZ = 1227.6e6;

const coreFixture = (rel) => readFileSync(`${CORE_FIXTURES}/${rel}`);
const coreJson = (rel) => JSON.parse(coreFixture(rel).toString("utf8"));
const hexToBytes = (hex) =>
  Uint8Array.from(
    hex
      .trim()
      .match(/.{2}/g)
      .map((b) => parseInt(b, 16)),
  );
const close = (actual, expected, tol, label) =>
  assert.ok(Math.abs(actual - expected) <= tol, `${label}: ${actual} vs ${expected}`);
const norm3 = (v) => Math.hypot(v[0], v[1], v[2]);
const sub3 = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const satToken = (systemLetter, prn) => `${systemLetter}${String(prn).padStart(2, "0")}`;
const j2000FromUtc = (year, month, day, hour = 0, minute = 0, second = 0) =>
  Date.UTC(year, month - 1, day, hour, minute, second) / 1000 -
  Date.UTC(2000, 0, 1, 12, 0, 0) / 1000;
const gpsWeekTowFromUtc = (year, month, day, hour = 0, minute = 0, second = 0) => {
  const gpsSeconds =
    Date.UTC(year, month - 1, day, hour, minute, second) / 1000 -
    Date.UTC(1980, 0, 6, 0, 0, 0) / 1000 +
    18.0;
  const week = Math.floor(gpsSeconds / 604800.0);
  return { week, towS: gpsSeconds - week * 604800.0 };
};
const gpsJ2000FromWeekTow = (week, towS) =>
  week * 604800.0 + towS - (Date.UTC(2000, 0, 1, 12) / 1000 - Date.UTC(1980, 0, 6) / 1000);

test("GNSS system and carrier labels come from the core label tables", () => {
  assert.equal(gnssSystemLabel(GnssSystem.Gps), "GPS");
  assert.equal(gnssSystemLabel(GnssSystem.Glonass), "GLONASS");
  assert.equal(gnssSystemLabel(GnssSystem.Galileo), "Galileo");
  assert.equal(carrierBandName(CarrierBand.L1), "l1");
  assert.equal(carrierBandName(CarrierBand.E5a), "e5a");
});

test("Bias-SINEX and CODE DCB loaders expose oracle bias values", () => {
  const sinex = loadBiasSinex(coreFixture("bias/CODE.BIA"));
  const sinexGz = loadBiasSinexLossy(coreFixture("bias/COD0OPSFIN_20261330000_01D_01D_OSB.BIA.gz"));
  assert.equal(sinex.recordCount, 351);
  assert.ok(sinex.skippedRecords > 0);
  assert.ok(sinexGz.recordCount > 0);

  const osbEpoch = j2000FromUtc(2026, 6, 30);
  close(sinex.codeOsbSeconds("G01", "C1C", osbEpoch, "gpst"), -6.2069e-9, 1e-16, "G01 C1C OSB");
  close(sinex.codeOsbSeconds("G01", "C1W", osbEpoch, "gpst"), -5.2579e-9, 1e-16, "G01 C1W OSB");
  close(sinex.codeOsbSeconds("R02", "C1P", osbEpoch, "gpst"), 1.784e-9, 1e-16, "R02 C1P OSB");

  const dcb = loadCodeDcb(coreFixture("bias/P1C1_RINEX.DCB"), null);
  assert.equal(dcb.recordCount, 496);
  assert.equal(dcb.skippedRecords, 2);

  const dcbEpoch = j2000FromUtc(2026, 6, 2);
  close(dcb.codeDsbSeconds("G01", "C1W", "C1C", dcbEpoch, "gpst"), 0.626e-9, 1e-16, "G01 DCB");
  close(
    dcb.codeDsbSeconds("G01", "C1C", "C1W", dcbEpoch, "gpst"),
    -0.626e-9,
    1e-16,
    "G01 inverse DCB",
  );

  const model = dcb.codeBiasModelM(
    "G01",
    "C1C",
    "C2W",
    F_L1_HZ,
    F_L2_HZ,
    null,
    "C1W",
    "C2W",
    dcbEpoch,
    "gpst",
  );
  const alpha = (F_L1_HZ * F_L1_HZ) / (F_L1_HZ * F_L1_HZ - F_L2_HZ * F_L2_HZ);
  close(model, alpha * -0.626e-9 * C_M_S, 1e-12, "DCB model");
});

test("PPP correction precompute applies code-bias options", () => {
  const sp3 = loadSp3(fixture("sp3/GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const bias = loadBiasSinex(coreFixture("bias/CODE.BIA"));
  const t = j2000FromUtc(2026, 6, 30);
  const epochs = [
    {
      year: 2026,
      month: 6,
      day: 30,
      hour: 0,
      minute: 0,
      second: 0.0,
      tRxJ2000S: t,
      observations: [{ satelliteId: "G01", freq1Hz: F_L1_HZ, freq2Hz: F_L2_HZ }],
    },
  ];
  const codeBias = {
    usedObservablesDefault: [{ system: "gps", obs1: "C1C", obs2: "C2W" }],
    clockReference: [{ system: "gps", obs1: "C1W", obs2: "C2W" }],
  };
  const corrections = pppCorrectionsWithCodeBias(
    sp3,
    epochs,
    Float64Array.from([3512900.0, 780500.0, 5248700.0]),
    {},
    bias,
    codeBias,
  );
  assert.equal(corrections.codeBiasM.length, 1);
  const direct = bias.codeBiasModelM(
    "G01",
    "C1C",
    "C2W",
    F_L1_HZ,
    F_L2_HZ,
    null,
    "C1W",
    "C2W",
    t,
    "gpst",
  );
  close(corrections.codeBiasM[0].valueM, direct, 1e-12, "PPP code bias");
});

test("source-agnostic ephemeris sampler covers precise and broadcast sources", () => {
  const sp3 = loadSp3(fixture("sp3/GBM0MGXRAP_20201770000_01D_05M_ORB_120epoch.sp3"));
  const epochs = sp3.epochsJ2000Seconds();
  const precise = sampleSp3Ephemeris(sp3, ["G01"], epochs[2], epochs[3], epochs[3] - epochs[2]);
  assert.equal(precise.length, 2);
  assert.ok(precise.every((row) => row.sat === "G01" && row.status === "valid"));
  assert.ok(precise.every((row) => row.positionEcefM.length === 3));

  const nav = loadRinexNav(fixture("nav/ESBC00DNK_R_20201770000_01D_MN.rnx"));
  const sats = Array.from({ length: 32 }, (_, i) => satToken("G", i + 1));
  const broadcast = sampleBroadcastEphemeris(nav, sats, epochs[20], epochs[20] + 600.0, 300.0);
  assert.ok(broadcast.some((row) => row.status === "valid" && row.clockS != null));
  assert.ok(broadcast.some((row) => row.status === "gap"));
});

test("DTED terrain wrapper delegates lookup and validation to core", () => {
  const terrain = new DtedTerrain(`${CORE_FIXTURES}/dted/tiles`);
  const points = coreJson("dted/dted_points.json");
  const bilinear = points.bilinear_cases[0];

  assert.equal(
    terrain.heightM(hexToF64(bilinear.longitude_bits), hexToF64(bilinear.latitude_bits)),
    0.0,
  );
  assert.equal(
    terrain.heightMWithOptions(
      hexToF64(bilinear.longitude_bits),
      hexToF64(bilinear.latitude_bits),
      {
        interpolation: "nearest",
      },
    ),
    0.0,
  );
  assert.throws(
    () => terrain.heightMWithOptions(-106.5, 36.5, { interpolation: "cubic" }),
    TypeError,
  );
  assert.throws(() => terrain.heightM(Number.NaN, 36.5), Error);
});

test("SBAS decode, store, corrected state, and corrected SPP route through core", () => {
  const mt2 = hexToBytes("5308DFFC010005FFC00DFFC009FFDFFC001FFDFFDFFFBABBBBBB9BBB80");
  const decoded = decodeSbasMessage(mt2, "body226");
  assert.equal(decoded.messageType, 2);
  assert.equal(decoded.form, "body226");
  assert.match(decoded.kind, /FastCorrections/);

  const store = new SbasCorrectionStore();
  const mt9 = hexToBytes("9A25C80C8D3F574632853C69A015EEBFF2D7DF580018FE3FCFF79C38C0");
  const gps = gpsWeekTowFromUtc(2011, 1, 21, 0, 0, 0);
  store.ingest(mt9, "body226", "S29", gps.week, gps.towS, "gpst");

  const nav = loadRinexNav(fixture("nav/ESBC00DNK_R_20201770000_01D_MN.rnx"));
  const t = j2000FromUtc(2020, 6, 25, 12, 0, 0);
  const fallback = sbasCorrectedState(nav, store, "S29", "G01", t, "mixedAugmentation");
  assert.ok(fallback);
  assert.ok(fallback.positionEcefM.every(Number.isFinite));
  assert.equal(sbasCorrectedState(nav, store, "S29", "G01", t, "sbasOnly"), null);

  const sats = Array.from({ length: 32 }, (_, i) => satToken("G", i + 1));
  const states = sampleBroadcastEphemeris(nav, sats, t, t, 60.0).filter(
    (row) => row.status === "valid" && row.clockS != null,
  );
  assert.ok(states.length >= 6);
  const receiver = [3512900.0, 780500.0, 5248700.0];
  const observations = states.map((row) => ({
    satelliteId: row.sat,
    pseudorangeM: norm3(sub3(row.positionEcefM, receiver)) - C_M_S * row.clockS,
  }));
  const solution = solveSppSbas(
    nav,
    store,
    "S29",
    {
      observations,
      tRxJ2000S: t,
      tRxSecondOfDayS: 43200.0,
      dayOfYear: 177.0,
      initialGuess: [...receiver, 0.0],
      corrections: { ionosphere: false, troposphere: false },
      withGeodetic: true,
    },
    "mixedAugmentation",
  );
  assert.ok(solution.positionM.every(Number.isFinite));
  assert.ok(solution.usedSats.length >= 4);
});

test("SSR decode, correction store, and corrected state route through core", () => {
  const frame = hexToBytes(coreFixture("ssr/SSRA02IGS0_2026181234930_1060.hex").toString("utf8"));
  const decoded = decodeSsr(frame, true);
  assert.equal(decoded.messageNumber, 1060);
  assert.equal(decoded.system, "GPS");
  assert.equal(decoded.kind, "combinedOrbitClock");
  assert.ok(decoded.orbit.length > 0);
  assert.equal(decoded.clock.length, decoded.orbit.length);

  const store = new SsrCorrectionStore();
  const ssrWeek = 2425;
  const ssrTowS = 344970.0;
  store.ingest(frame, true, ssrWeek, ssrTowS, "gpst");
  const record =
    decoded.orbit.find((entry) => entry.satelliteId === 30 || entry.satelliteId === 31) ??
    decoded.orbit[0];
  const sat = satToken("G", record.satelliteId);
  const orbit = store.orbit(sat);
  const clock = store.clock(sat);
  assert.ok(orbit);
  assert.ok(clock);
  assert.ok(Number.isFinite(orbit.radialM));
  assert.ok(Number.isFinite(clock.c0M));

  const nav = loadRinexNav(coreFixture("ssr/BRDC00WRD_S_20261820000_G30_G31.rnx"));
  const state = ssrCorrectedState(
    nav,
    store,
    sat,
    gpsJ2000FromWeekTow(ssrWeek, ssrTowS),
    true,
    null,
  );
  assert.ok(state);
  assert.ok(state.positionEcefM.every(Number.isFinite));
});
