import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import {
  CarrierBand,
  DtedTerrain,
  ExactEpochQuery,
  GnssSystem,
  SbasCorrectionStore,
  SsrCorrectionStore,
  SsrCorrectedEphemeris,
  SsrSource,
  carrierBandName,
  decodeSbasMessage,
  decodeSsr,
  gnssSystemLabel,
  loadAntex,
  loadBiasSinex,
  loadBiasSinexLossy,
  loadCodeDcb,
  loadCodeDcbLossy,
  loadRinexNav,
  loadSp3,
  pppCorrectionsWithCodeBias,
  sampleBroadcastEphemeris,
  sampleSp3Ephemeris,
  sbasCorrectedState,
  solveSppSbas,
  ssrCorrectedState,
  ssrSourceLabel,
  ssrStoreFromRtcm,
  ssrStoreFromRtcmStrict,
} from "../pkg-node/sidereon.js";
import { coreGoldens, fixture, fixtureJson, hexToF64 } from "./helpers.mjs";

const CORE_FIXTURES = fileURLToPath(new URL("./fixtures", import.meta.url));
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
const readBits = (bytes, offset, count) => {
  let value = 0;
  for (let i = 0; i < count; i += 1) {
    const bit = offset + i;
    value = (value << 1) | ((bytes[3 + Math.floor(bit / 8)] >> (7 - (bit % 8))) & 1);
  }
  return value;
};
const writeBits = (bytes, offset, count, value) => {
  for (let i = 0; i < count; i += 1) {
    const bit = offset + i;
    const mask = 1 << (7 - (bit % 8));
    const index = 3 + Math.floor(bit / 8);
    const set = (value >> (count - i - 1)) & 1;
    bytes[index] = set ? bytes[index] | mask : bytes[index] & ~mask;
  }
};
const crc24q = (bytes) => {
  let crc = 0;
  for (const byte of bytes.subarray(0, -3)) {
    crc ^= byte << 16;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = (crc << 1) ^ (crc & 0x800000 ? 0x1864cfb : 0);
    }
  }
  crc &= 0xffffff;
  bytes[bytes.length - 3] = (crc >> 16) & 0xff;
  bytes[bytes.length - 2] = (crc >> 8) & 0xff;
  bytes[bytes.length - 1] = crc & 0xff;
};
const frameWithLargeG30Orbit = (frame) => {
  const changed = frame.slice();
  const deltaRadialOffset = 82;
  assert.equal(readBits(changed, deltaRadialOffset, 22), 807);
  writeBits(changed, deltaRadialOffset, 22, 150000);
  crc24q(changed);
  return changed;
};
const frameWithChangedG30Clock = (frame) => {
  const changed = frame.slice();
  const c0Offset = 203;
  assert.equal(readBits(changed, c0Offset, 22), 166);
  writeBits(changed, c0Offset, 22, 10166);
  crc24q(changed);
  return changed;
};
const frameWithRegionalG30Datum = (frame) => {
  const changed = frame.slice();
  // RTCM SSR places this flag immediately after multipleMessage and before IOD SSR.
  const datumOffset = 37;
  assert.equal(readBits(changed, datumOffset, 1), 0);
  writeBits(changed, datumOffset, 1, 1);
  crc24q(changed);
  return changed;
};
const frameAtGpsTow = (frame, originalTow, changedTow) => {
  const changed = frame.slice();
  const epochOffset = 12;
  assert.equal(readBits(changed, epochOffset, 20), originalTow);
  writeBits(changed, epochOffset, 20, changedTow);
  crc24q(changed);
  return changed;
};
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
  assert.equal(ssrSourceLabel(SsrSource.RtcmSsr), "rtcmSsr");
  assert.equal(ssrSourceLabel(SsrSource.GalileoHas), "galileoHas");
});

test("Bias-SINEX and CODE DCB loaders expose oracle bias values", () => {
  const sinex = loadBiasSinex(coreFixture("bias/CODE.BIA"));
  const sinexGz = loadBiasSinexLossy(coreFixture("bias/COD0OPSFIN_20261330000_01D_01D_OSB.BIA.gz"));
  // Record and skip counts, reproduced natively by test/golden-gen.
  const counts = coreGoldens().bias;
  assert.equal(sinex.recordCount, counts.sinexRecordCount);
  assert.equal(sinex.skippedRecords, counts.sinexSkippedRecords);
  assert.ok(sinexGz.recordCount > 0);
  assert.equal(sinex.mode, "absolute");
  assert.equal(sinex.timeScale, "gpst");
  assert.ok(sinex.records.every((r) => ["code", "phase", "mixed"].includes(r.family)));
  assert.ok(sinex.records.every((r) => ["ns", "cyc"].includes(r.unit)));

  const osbEpoch = j2000FromUtc(2026, 6, 30);
  const osb = (sat, obs) => sinex.codeOsbSeconds(sat, obs, osbEpoch, "gpst");
  assert.equal(osb("G01", "C1C").status, "available");
  close(osb("G01", "C1C").value, -6.2069e-9, 1e-16, "G01 C1C OSB");
  close(osb("G01", "C1W").value, -5.2579e-9, 1e-16, "G01 C1W OSB");
  close(osb("R02", "C1P").value, 1.784e-9, 1e-16, "R02 C1P OSB");
  const absent = osb("G01", "C9Z");
  assert.equal(absent.status, "absent");
  assert.equal(absent.value, null);
  // A query on another time scale than the product's is not converted.
  const utc = sinex.codeOsbSeconds("G01", "C1C", osbEpoch, "utc");
  assert.equal(utc.status, "unsupportedScale");
  assert.equal(utc.productScale, "gpst");
  assert.equal(utc.queryScale, "utc");

  const dcb = loadCodeDcb(coreFixture("bias/P1C1_RINEX.DCB"), null);
  // G34 and R28 are satellite tokens in the shared 01..99 range, so their
  // records are kept with the others.
  assert.equal(dcb.recordCount, counts.dcbRecordCount);
  assert.equal(dcb.skippedRecords, counts.dcbSkippedRecords);

  const dcbEpoch = j2000FromUtc(2026, 6, 2);
  close(
    dcb.codeDsbSeconds("G01", "C1W", "C1C", dcbEpoch, "gpst").value,
    0.626e-9,
    1e-16,
    "G01 DCB",
  );
  close(
    dcb.codeDsbSeconds("G01", "C1C", "C1W", dcbEpoch, "gpst").value,
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
  assert.equal(model.status, "available");
  close(model.value, alpha * -0.626e-9 * C_M_S, 1e-12, "DCB model");
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
  close(corrections.codeBiasM[0].valueM, direct.value, 1e-12, "PPP code bias");
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

test("CRC-valid SSR phase-bias and orbit frames preserve every public field", () => {
  // These valid message bodies use literal values from the C-backed public
  // builders in the matching Go lane. Expected projections are fixed here.
  const phaseBias = hexToBytes("d30012fec2fc021c254000c381c0214ff3c9ffff40462b6c");
  assert.deepEqual(decodeSsr(phaseBias, true), {
    messageNumber: 4076,
    igsSsrVersion: 1,
    system: "SBAS",
    kind: "phaseBias",
    header: {
      epochTimeS: 4321,
      updateInterval: 2,
      multipleMessage: true,
      iodSsr: 4,
      providerId: 12,
      solutionId: 3,
      satelliteReferenceDatum: undefined,
      dispersiveBiasConsistency: true,
      mwConsistency: false,
      satelliteCount: 1,
    },
    orbit: [],
    clock: [],
    codeBias: [],
    phaseBias: [
      {
        satelliteId: 48,
        yawAngle: 20,
        yawRate: -1,
        biases: [
          {
            signalId: 7,
            integerIndicator: 1,
            wideLaneIntegerIndicator: 0,
            discontinuityCounter: 9,
            bias: -12,
          },
        ],
      },
    ],
    ura: [],
    paddingBitCount: 4,
  });

  const orbitFrame = hexToBytes("d3001a421186a01500030c10447ffffd00004ffffb000037fff9000100f41ef3");
  assert.deepEqual(decodeSsr(orbitFrame, true), {
    messageNumber: 1057,
    igsSsrVersion: undefined,
    system: "GPS",
    kind: "orbit",
    header: {
      epochTimeS: 100000,
      updateInterval: 1,
      multipleMessage: false,
      iodSsr: 4,
      providerId: 12,
      solutionId: 3,
      satelliteReferenceDatum: true,
      dispersiveBiasConsistency: undefined,
      mwConsistency: undefined,
      satelliteCount: 1,
    },
    orbit: [
      {
        satelliteId: 1,
        iode: 17,
        iodCrc: undefined,
        deltaRadial: -3,
        deltaAlong: 4,
        deltaCross: -5,
        dotDeltaRadial: 6,
        dotDeltaAlong: -7,
        dotDeltaCross: 8,
      },
    ],
    clock: [],
    codeBias: [],
    phaseBias: [],
    ura: [],
    paddingBitCount: 5,
  });

  const store = new SsrCorrectionStore();
  store.ingest(orbitFrame, true, 2400, 100000, "gpst");
  const expectedOrbit = {
    source: "rtcmSsr",
    providerId: 12,
    solutionId: 3,
    navMessage: "rtcm",
    hasNavMessageIndex: undefined,
    iode: 17,
    iodCrc: undefined,
    iodSsr: 4,
    basis: "velocityAligned",
    crsRegional: true,
    referencePoint: 0,
    radialM: 0.00030000000000000003,
    alongM: -0.0016,
    crossM: 0.002,
    radialRateMS: -0.000006,
    alongRateMS: 0.000028,
    crossRateMS: -0.000032,
    refEpochJ2000S: 820856801,
    transmittedEpochJ2000S: 820856800,
    updateIntervalS: 2,
  };
  assert.deepEqual(store.orbit("G01"), expectedOrbit);

  const comStore = new SsrCorrectionStore(1);
  comStore.ingest(orbitFrame, true, 2400, 100000, "gpst");
  assert.deepEqual(comStore.orbit("G01"), {
    ...expectedOrbit,
    referencePoint: 1,
  });

  const sbasOrbitFrame = hexToBytes(
    "d3001d4e4186a01500030c107ff579bdfffffe800027fffd80001bfffc800080014d49",
  );
  assert.deepEqual(decodeSsr(sbasOrbitFrame, true).orbit, [
    {
      satelliteId: 1,
      iode: 511,
      iodCrc: 0xabcdef,
      deltaRadial: -3,
      deltaAlong: 4,
      deltaCross: -5,
      dotDeltaRadial: 6,
      dotDeltaAlong: -7,
      dotDeltaCross: 8,
    },
  ]);
  const sbasStore = new SsrCorrectionStore();
  sbasStore.ingest(sbasOrbitFrame, true, 2400, 100000, "gpst");
  assert.deepEqual(sbasStore.orbit("S20"), {
    source: "rtcmSsr",
    providerId: 12,
    solutionId: 3,
    navMessage: "rtcm",
    hasNavMessageIndex: undefined,
    iode: 511,
    iodCrc: 0xabcdef,
    iodSsr: 4,
    basis: "velocityAligned",
    crsRegional: true,
    referencePoint: 0,
    radialM: 0.00030000000000000003,
    alongM: -0.0016,
    crossM: 0.002,
    radialRateMS: -0.000006,
    alongRateMS: 0.000028,
    crossRateMS: -0.000032,
    refEpochJ2000S: 820856801,
    transmittedEpochJ2000S: 820856800,
    updateIntervalS: 2,
  });

  for (const invalidTag of [-1, 1.5, Number.NaN, Number.POSITIVE_INFINITY]) {
    assert.throws(() => new SsrCorrectionStore(invalidTag), {
      name: "TypeError",
    });
  }
});

test("SSR decode, correction store, and corrected state route through core", () => {
  const frame = hexToBytes(coreFixture("ssr/SSRA02IGS0_2026181234930_1060.hex").toString("utf8"));
  const decoded = decodeSsr(frame, true);
  assert.deepEqual(decoded, {
    messageNumber: 1060,
    igsSsrVersion: undefined,
    system: "GPS",
    kind: "combinedOrbitClock",
    header: {
      epochTimeS: 344970,
      updateInterval: 3,
      multipleMessage: false,
      iodSsr: 1,
      providerId: 0,
      solutionId: 2,
      satelliteReferenceDatum: false,
      dispersiveBiasConsistency: undefined,
      mwConsistency: undefined,
      satelliteCount: 2,
    },
    orbit: [
      {
        satelliteId: 30,
        iode: 90,
        iodCrc: undefined,
        deltaRadial: 807,
        deltaAlong: 621,
        deltaCross: -349,
        dotDeltaRadial: 30,
        dotDeltaAlong: -10,
        dotDeltaCross: -8,
      },
      {
        satelliteId: 31,
        iode: 67,
        iodCrc: undefined,
        deltaRadial: -227,
        deltaAlong: -1752,
        deltaCross: 1423,
        dotDeltaRadial: -43,
        dotDeltaAlong: -7,
        dotDeltaCross: 3,
      },
    ],
    clock: [
      { satelliteId: 30, c0: 166, c1: 0, c2: 0 },
      { satelliteId: 31, c0: 4170, c1: 0, c2: 0 },
    ],
    codeBias: [],
    phaseBias: [],
    ura: [],
    paddingBitCount: 2,
  });
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
  assert.deepEqual(orbit, {
    source: "rtcmSsr",
    providerId: 0,
    solutionId: 2,
    navMessage: "rtcm",
    hasNavMessageIndex: undefined,
    iode: 90,
    iodCrc: undefined,
    iodSsr: 1,
    basis: "velocityAligned",
    crsRegional: false,
    referencePoint: 0,
    radialM: -0.08070000000000001,
    alongM: -0.2484,
    crossM: 0.1396,
    radialRateMS: -0.000029999999999999997,
    alongRateMS: 0.000039999999999999996,
    crossRateMS: 0.000032,
    refEpochJ2000S: 836221775,
    transmittedEpochJ2000S: 836221770,
    updateIntervalS: 10,
  });
  assert.deepEqual(clock, {
    source: "rtcmSsr",
    providerId: 0,
    solutionId: 2,
    navMessage: "rtcm",
    hasNavMessageIndex: undefined,
    iodSsr: 1,
    c0M: 0.0166,
    c1MS: 0,
    c2MS2: 0,
    highRateC0M: undefined,
    refEpochJ2000S: 836221775,
    transmittedEpochJ2000S: 836221770,
    updateIntervalS: 10,
  });
  assert.equal(orbit.source, "rtcmSsr");
  assert.equal(clock.source, "rtcmSsr");
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

test("owned SSR corrected source has an independent core numeric oracle and survives freed inputs", () => {
  const frame = hexToBytes(coreFixture("ssr/SSRA02IGS0_2026181234930_1060.hex").toString("utf8"));
  const store = new SsrCorrectionStore();
  store.ingest(frame, true, 2425, 344970, "gpst");
  const nav = loadRinexNav(coreFixture("ssr/BRDC00WRD_S_20261820000_G30_G31.rnx"));
  let source = new SsrCorrectedEphemeris(nav, store);
  const antex = loadAntex(coreFixture("antex/igs20_wettzell_trim.atx"));
  source = source.withSatelliteAntennas(antex);
  const epoch = ExactEpochQuery.fromBinaryJ2000Seconds(836221752);
  antex.free();
  nav.free();
  store.free();

  const state = source.correctedStateAtQueries("G30", epoch, epoch);
  assert.deepEqual(state, {
    positionEcefM: [-6296153.684045405, 15837450.006500244, -20103648.508187104],
    clockS: 0.0002800863540204331,
    ut1Degraded: undefined,
  });
  const selected = source.selectedPositionClockAtQueries("G30", epoch, epoch);
  assert.deepEqual(selected, {
    value: {
      positionEcefM: [-6296153.684045405, 15837450.006500244, -20103648.508187104],
      clockS: 0.0002800863540204331,
      groupDelayS: 4.19095158577e-9,
    },
    ut1Degraded: null,
  });
  assert.deepEqual(source.correctedStateWithGroupDelayAtQueries("G30", epoch, epoch), {
    value: {
      positionEcefM: [-6296153.684045405, 15837450.006500244, -20103648.508187104],
      clockS: 0.0002800863540204331,
      groupDelayS: 4.19095158577e-9,
    },
    ut1Degraded: null,
  });
  assert.deepEqual(source.transmitEpochClockAtQueries("G30", epoch, epoch), {
    value: 0.0002800929432812776,
    ut1Degraded: null,
  });
  assert.equal(source.ephemerisVarianceAtQueries("G30", epoch, epoch), 0.0225);
  assert.deepEqual(source.clockRelativityAtQuery("G30", epoch, state.positionEcefM), {
    kind: "notApplicable",
  });
  assert.equal(source.correctionSizePolicy, 0);
  assert.equal(source.ut1Departure, undefined);
  assert.deepEqual(source.oversizedCorrections, []);
});

test("owned SSR exposes velocity, applied solution status, and full group-delay results", () => {
  const frame = hexToBytes(coreFixture("ssr/SSRA02IGS0_2026181234930_1060.hex").toString("utf8"));
  const store = new SsrCorrectionStore();
  store.ingest(frame, true, 2425, 344970, "gpst");
  const nav = loadRinexNav(coreFixture("ssr/BRDC00WRD_S_20261820000_G30_G31.rnx"));
  const source = new SsrCorrectedEphemeris(nav, store);
  const epoch = 836221752;

  assert.deepEqual(
    source.correctedVelocityAtJ2000("G30", epoch),
    [-1733.2327608019114, -1962.167527526617, -1017.806526273489],
  );
  const solution = { source: "rtcmSsr", providerId: 0, solutionId: 2 };
  assert.deepEqual(source.appliedOrbitClockSolutionAtJ2000("G30", epoch), solution);
  assert.deepEqual(source.appliedOrbitClockStatusAtJ2000("G30", epoch), {
    status: "available",
    solution,
  });
  assert.deepEqual(source.correctedStateWithGroupDelayAtJ2000("G30", epoch), {
    positionEcefM: [-6296153.684045405, 15837450.006500244, -20103648.508187104],
    clockS: 0.0002800863540204331,
    groupDelayS: 4.19095158577e-9,
  });
  assert.equal(source.singleFrequencyGroupDelayAtJ2000("G30", epoch), 4.19095158577e-9);

  const emptyStore = new SsrCorrectionStore();
  const noRecord = new SsrCorrectedEphemeris(nav, emptyStore);
  assert.equal(noRecord.correctedVelocityAtJ2000("G01", epoch), null);
  assert.equal(noRecord.appliedOrbitClockSolutionAtJ2000("G01", epoch), null);
  assert.equal(noRecord.correctedStateWithGroupDelayAtJ2000("G01", epoch), null);
  assert.equal(noRecord.singleFrequencyGroupDelayAtJ2000("G01", epoch), null);
  const exactEpoch = ExactEpochQuery.fromBinaryJ2000Seconds(epoch);
  assert.equal(noRecord.correctedStateWithGroupDelayAtQueries("G01", exactEpoch, exactEpoch), null);
  noRecord.free();
  emptyStore.free();

  source.free();
  nav.free();
  store.free();
});

test("owned SSR applies regional provider, UT1, and nominal CoM attitude policies", () => {
  const frame = hexToBytes(coreFixture("ssr/SSRA02IGS0_2026181234930_1060.hex").toString("utf8"));
  const nav = loadRinexNav(coreFixture("ssr/BRDC00WRD_S_20261820000_G30_G31.rnx"));
  const epoch = 836221752;
  const regionalFrame = frameWithRegionalG30Datum(frame);
  assert.equal(decodeSsr(regionalFrame, true).header.satelliteReferenceDatum, true);

  const regionalStore = new SsrCorrectionStore();
  regionalStore.ingest(regionalFrame, true, 2425, 344970, "gpst");
  assert.equal(regionalStore.orbit("G30").crsRegional, true);
  const regionalDecline = new SsrCorrectedEphemeris(nav, regionalStore);
  assert.deepEqual(regionalDecline.appliedOrbitClockStatusAtJ2000("G30", epoch), {
    status: "unavailable",
    reason: { kind: "regionalProviderNotAllowed" },
  });
  assert.equal(regionalDecline.appliedOrbitClockSolutionAtJ2000("G30", epoch), null);

  const regionalAllowed = new SsrCorrectedEphemeris(nav, regionalStore, {
    allowRegionalProviders: [0],
  });
  assert.deepEqual(regionalAllowed.appliedOrbitClockStatusAtJ2000("G30", epoch), {
    status: "available",
    solution: { source: "rtcmSsr", providerId: 0, solutionId: 2 },
  });
  assert.deepEqual(regionalAllowed.correctedStateAtJ2000("G30", epoch), {
    positionEcefM: [-6296153.684045405, 15837450.006500244, -20103648.508187104],
    clockS: 0.0002800863540204331,
    ut1Degraded: undefined,
  });

  const comFrame = frame;
  const comStore = new SsrCorrectionStore(1);
  comStore.ingest(comFrame, true, 2425, 344970, "gpst");
  const antex = loadAntex(coreFixture("antex/igs20_wettzell_trim.atx"));
  const unavailableAttitude = new SsrCorrectedEphemeris(nav, comStore).withSatelliteAntennas(antex);
  assert.deepEqual(unavailableAttitude.appliedOrbitClockStatusAtJ2000("G30", 836221770), {
    status: "unavailable",
    reason: { kind: "centerOfMassUnresolved" },
  });
  const nominal = new SsrCorrectedEphemeris(nav, comStore, {
    maxStalenessS: 60,
    satelliteAttitude: "nominalSunFixed",
  }).withSatelliteAntennas(antex);
  assert.deepEqual(nominal.correctedStateAtJ2000("G30", 836221770), {
    positionEcefM: [-6327381.448161609, 15802128.916795386, -20121896.861226305],
    clockS: 0.0002800865527753679,
    ut1Degraded: undefined,
  });

  // Roll the real G30 RINEX record and RTCM epoch together beyond the pinned
  // UT1 table end (MJD 61589), retaining the record's broadcast parameters and IODE.
  const navLines = coreFixture("ssr/BRDC00WRD_S_20261820000_G30_G31.rnx")
    .toString("utf8")
    .split(/\r?\n/);
  const g30Line = navLines.findIndex((line) => line.startsWith("G30 2026 07 02"));
  assert.notEqual(g30Line, -1);
  navLines[g30Line] = navLines[g30Line].replace("2026 07 02", "2027 07 22");
  navLines[g30Line + 5] = navLines[g30Line + 5].replace("2.425000000000e+03", "2.480000000000e+03");
  const futureNav = loadRinexNav(new TextEncoder().encode(navLines.join("\n")));
  const futureFrame = frameAtGpsTow(frame, 344970, 345600);
  const futureWeek = 2480;
  const futureTow = 345600;
  const futureEpoch = gpsJ2000FromWeekTow(futureWeek, futureTow);
  const futureStore = new SsrCorrectionStore(1);
  futureStore.ingest(futureFrame, true, futureWeek, futureTow, "gpst");
  const futureAntex = loadAntex(coreFixture("antex/igs20_wettzell_trim.atx"));
  const strictUt1 = new SsrCorrectedEphemeris(futureNav, futureStore, {
    maxStalenessS: 60,
    satelliteAttitude: "nominalSunFixed",
    ut1Validity: "strict",
  }).withSatelliteAntennas(futureAntex);
  assert.deepEqual(strictUt1.appliedOrbitClockStatusAtJ2000("G30", futureEpoch), {
    status: "unavailable",
    reason: { kind: "ut1OutsideCoverage", reason: "afterCoverage" },
  });
  const permissiveUt1 = new SsrCorrectedEphemeris(futureNav, futureStore, {
    maxStalenessS: 60,
    satelliteAttitude: "nominalSunFixed",
    ut1Validity: "permissive",
  }).withSatelliteAntennas(futureAntex);
  const query = ExactEpochQuery.fromBinaryJ2000Seconds(futureEpoch);
  assert.throws(
    () => strictUt1.correctedStateWithGroupDelayAtQueries("G30", query, query),
    (error) => {
      assert.equal(error.name, "PositioningError");
      assert.deepEqual(error.detail, {
        kind: "UT1_OUTSIDE_COVERAGE",
        message: "UT1 outside the table: instant follows the UT1 table coverage",
        reason: "afterCoverage",
      });
      return true;
    },
  );
  const checkedDegraded = permissiveUt1.correctedStateWithGroupDelayAtQueries("G30", query, query);
  assert.equal(checkedDegraded.ut1Degraded, "afterCoverage");
  assert.ok(checkedDegraded.value.positionEcefM.every(Number.isFinite));
  assert.ok(Number.isFinite(checkedDegraded.value.clockS));
  assert.equal(checkedDegraded.value.groupDelayS, 4.19095158577e-9);
  const degraded = permissiveUt1.correctedStateAtQueries("G30", query, query);
  assert.ok(degraded);
  assert.ok(degraded.positionEcefM.every(Number.isFinite));
  assert.ok(Number.isFinite(degraded.clockS));
  assert.equal(degraded.ut1Degraded, "afterCoverage");
  assert.equal(permissiveUt1.ut1Departure, "afterCoverage");

  strictUt1.free();
  permissiveUt1.free();
  futureAntex.free();
  futureStore.free();
  futureNav.free();
  nominal.free();
  unavailableAttitude.free();
  antex.free();
  regionalAllowed.free();
  regionalDecline.free();
  comStore.free();
  regionalStore.free();
  nav.free();
});

test("owned SSR corrected source validates policy options before narrowing and owns a store snapshot", () => {
  const frame = hexToBytes(coreFixture("ssr/SSRA02IGS0_2026181234930_1060.hex").toString("utf8"));
  const nav = loadRinexNav(coreFixture("ssr/BRDC00WRD_S_20261820000_G30_G31.rnx"));
  const invalidOptions = [
    { maxStalenessS: -1 },
    { maxStalenessS: Number.NaN },
    { maxStalenessS: Number.POSITIVE_INFINITY },
    { allowRegionalProviders: [1.5] },
    { allowRegionalProviders: [-1] },
    { allowRegionalProviders: [65536] },
    { allowRegionalProviders: [Number.NaN] },
    { allowRegionalProviders: [Number.POSITIVE_INFINITY] },
    { fallback: "guess" },
    { ut1Validity: "guess" },
    { correctionSizePolicy: "guess" },
    { satelliteAttitude: "guess" },
  ];
  for (const options of invalidOptions) {
    const store = new SsrCorrectionStore();
    assert.throws(() => new SsrCorrectedEphemeris(nav, store, options));
    store.free();
  }

  const fallbackStore = new SsrCorrectionStore();
  const fallbackSource = new SsrCorrectedEphemeris(nav, fallbackStore, { fallback: "broadcast" });
  assert.ok(fallbackSource.correctedStateAtJ2000("G30", 836221752));
  fallbackSource.free();
  fallbackStore.free();

  const store = new SsrCorrectionStore();
  store.ingest(frame, true, 2425, 344970, "gpst");
  const source = new SsrCorrectedEphemeris(nav, store, {
    fallback: "broadcast",
    maxStalenessS: 120,
    ut1Validity: "strict",
    correctionSizePolicy: "lenient",
    satelliteAttitude: "unavailable",
    allowRegionalProviders: [7, 65535],
  });
  const epoch = ExactEpochQuery.fromBinaryJ2000Seconds(836221752);
  const before = source.correctedStateAtQueries("G30", epoch, epoch);
  const changedFrame = frameWithChangedG30Clock(frame);
  assert.equal(decodeSsr(changedFrame, true).clock[0].c0, 10166);
  store.ingest(changedFrame, true, 2425, 344970, "gpst");
  const after = source.correctedStateAtQueries("G30", epoch, epoch);
  assert.deepEqual(after, before);
  const updated = new SsrCorrectedEphemeris(nav, store);
  const updatedState = updated.correctedStateAtQueries("G30", epoch, epoch);
  assert.notDeepEqual(updatedState, before);

  const staleEpoch = ExactEpochQuery.fromBinaryJ2000Seconds(836222052);
  const staleFallback = new SsrCorrectedEphemeris(nav, store, {
    fallback: "broadcast",
    maxStalenessS: 120,
  });
  assert.ok(staleFallback.correctedStateAtQueries("G30", staleEpoch, staleEpoch));
  const staleDecline = new SsrCorrectedEphemeris(nav, store, {
    fallback: "decline",
    maxStalenessS: 120,
  });
  assert.equal(staleDecline.correctedStateAtQueries("G30", staleEpoch, staleEpoch), null);
  staleFallback.free();
  staleDecline.free();

  assert.equal(source.correctionSizePolicy, 1);
  nav.free();
  store.free();
  updated.free();
  assert.deepEqual(source.correctedStateAtQueries("G30", epoch, epoch), before);
});

test("owned SSR source returns strict size errors and retains lenient oversized reports", () => {
  const original = hexToBytes(
    coreFixture("ssr/SSRA02IGS0_2026181234930_1060.hex").toString("utf8"),
  );
  const frame = frameWithLargeG30Orbit(original);
  assert.equal(decodeSsr(frame, true).orbit[0].deltaRadial, 150000);
  const nav = loadRinexNav(coreFixture("ssr/BRDC00WRD_S_20261820000_G30_G31.rnx"));
  const strictStore = new SsrCorrectionStore();
  strictStore.ingest(frame, true, 2425, 344970, "gpst");
  const strict = new SsrCorrectedEphemeris(nav, strictStore);
  const epoch = ExactEpochQuery.fromBinaryJ2000Seconds(836221752);
  assert.throws(() => strict.correctedStateAtQueries("G30", epoch, epoch), {
    name: "SsrCorrectionSizeRefusal",
  });
  const refusal = strict.correctionSizeRefusalAtQueries("G30", epoch, epoch);
  assert.equal(refusal.satellite, "G30");
  assert.equal(refusal.size.orbitExceedsLimit, true);
  assert.equal(refusal.size.clockExceedsLimit, false);

  const lenientStore = new SsrCorrectionStore();
  lenientStore.ingest(frame, true, 2425, 344970, "gpst");
  const lenient = new SsrCorrectedEphemeris(nav, lenientStore, {
    correctionSizePolicy: "lenient",
  });
  assert.ok(lenient.correctedStateAtQueries("G30", epoch, epoch));
  const reports = lenient.oversizedCorrections;
  assert.equal(reports.length, 1);
  assert.equal(reports[0].satellite, "G30");
  assert.equal(reports[0].source, "rtcmSsr");
  assert.equal(reports[0].providerId, 0);
  assert.equal(reports[0].solutionId, 2);
  assert.equal(reports[0].size.orbitExceedsLimit, true);
  assert.equal(reports[0].size.clockExceedsLimit, false);
  assert.ok(reports[0].size.orbitM > 15);
  assert.ok(lenient.correctedStateAtQueries("G30", epoch, epoch));
  assert.deepEqual(lenient.oversizedCorrections, reports);
});

test("SSR stream constructors preserve lenient accounting and strict refusal", () => {
  const frame = hexToBytes(coreFixture("ssr/SSRA02IGS0_2026181234930_1060.hex").toString("utf8"));
  const nonSsr = hexToBytes(fixtureJson("rtcm.json").stream);
  const corrupt = frame.slice();
  corrupt[corrupt.length - 1] ^= 1;
  const trailing = frame.slice(0, -1);
  const bytes = Uint8Array.from([0x00, ...frame, ...nonSsr, ...corrupt, ...trailing]);

  const ingest = ssrStoreFromRtcm(bytes, 2425, 344970, "gpst");
  assert.equal(ingest.isComplete, false);
  assert.ok(ingest.store instanceof SsrCorrectionStore);
  assert.ok(ingest.store.orbit("G30"));
  assert.ok(ingest.store.clock("G30"));
  assert.ok(ingest.diagnostics.resyncBytes > 0);
  assert.equal(ingest.diagnostics.crcFailures, 1);
  assert.deepEqual(ingest.diagnostics.skippedFrames, []);
  assert.equal(ingest.trailingPartialFrameLen, trailing.length);
  assert.deepEqual(ingest.ingestRefusals, []);

  const cleanLenient = ssrStoreFromRtcm(frame, 2425, 344970, "gpst");
  assert.equal(cleanLenient.isComplete, true);
  assert.equal(cleanLenient.trailingPartialFrameLen, 0);
  assert.deepEqual(cleanLenient.diagnostics, {
    resyncBytes: 0,
    crcFailures: 0,
    skippedFrames: [],
    departures: [],
  });
  assert.deepEqual(cleanLenient.ingestRefusals, []);
  assert.ok(cleanLenient.store.orbit("G30"));

  const clean = ssrStoreFromRtcmStrict(frame, 2425, 344970, "gpst");
  assert.ok(clean instanceof SsrCorrectionStore);
  assert.ok(clean.orbit("G30"));
  assert.ok(clean.clock("G30"));
  assert.throws(() => ssrStoreFromRtcmStrict(bytes, 2425, 344970, "gpst"), Error);
  assert.throws(
    () => ssrStoreFromRtcmStrict(new Uint8Array([0x00]), 2425, 344970, "gpst"),
    (error) => {
      const detail = {
        kind: "PARSE",
        message:
          "RTCM input has 1 bytes outside CRC-valid frames (0 CRC-24Q failures, 0 bytes from an unfinished frame at the end)",
      };
      assert.equal(error.name, "Error");
      assert.deepEqual(error.detail, detail);
      assert.deepEqual(error.cause, detail);
      return true;
    },
  );
  assert.throws(() => ssrStoreFromRtcmStrict(trailing, 2425, 344970, "gpst"), Error);
});

test("Bias-SINEX and CODE DCB products write back what they read", () => {
  // The real CODE product carries one Latin-1 byte, 0xe4 ("ä" of "Jäggi") on
  // line 29 of its reference block, so it is not UTF-8 text.
  const bytes = coreFixture("bias/CODE.BIA");
  const latin1 = bytes.indexOf(0xe4);
  assert.notEqual(latin1, -1);
  assert.equal(bytes.indexOf(0xe4, latin1 + 1), -1);
  const sinex = loadBiasSinex(bytes);
  const sinexLossy = loadBiasSinexLossy(bytes);
  assert.deepEqual(sinexLossy.records, sinex.records);
  assert.deepEqual(sinexLossy.notices, sinex.notices);
  assert.deepEqual(loadBiasSinexLossy(bytes, "lenient").records, sinex.records);
  // Every line the reader kept is the writer's authority.
  assert.ok(Buffer.from(sinex.toBiasSinex()).equals(bytes));
  assert.ok(Buffer.from(sinex.toBiasSinexBytes()).equals(bytes));
  assert.throws(
    () => sinex.toBiasSinexText(),
    (error) => {
      assert.equal(error.name, "BiasError");
      assert.deepEqual(error.detail, { kind: "invalidUtf8Line", line: 29 });
      return true;
    },
  );
  assert.ok(Array.isArray(sinex.notices));
  for (const write of [() => sinex.toCodeDcbText(), () => sinex.toCodeDcbBytes()]) {
    assert.throws(write, (error) => {
      assert.equal(error.name, "BiasError");
      assert.deepEqual(error.detail, { kind: "missingWriterMetadata", field: "dcb_meta" });
      return true;
    });
  }

  // With that byte replaced by an ASCII letter of the same width the product
  // is UTF-8, and the text writer returns it unchanged.
  const ascii = Buffer.from(bytes);
  ascii[latin1] = 0x61;
  const asciiSinex = loadBiasSinex(ascii);
  assert.ok(Buffer.from(asciiSinex.toBiasSinexBytes()).equals(ascii));
  assert.equal(asciiSinex.toBiasSinexText(), ascii.toString("utf8"));

  const dcb_text = coreFixture("bias/P1C1_RINEX.DCB");
  const dcb = loadCodeDcb(dcb_text, null);
  const dcbLossy = loadCodeDcbLossy(dcb_text, null);
  assert.deepEqual(dcbLossy.records, dcb.records);
  assert.deepEqual(dcbLossy.notices, dcb.notices);
  assert.ok(Buffer.from(dcb.toCodeDcb()).equals(dcb_text));
  assert.ok(Buffer.from(dcb.toCodeDcbBytes()).equals(dcb_text));
  assert.equal(dcb.toCodeDcbText(), dcb_text.toString("utf8"));
  const unknownTimeSystem = Buffer.from(
    "# DCB P1-C1 2026-06 XYZ\nG01                           0.626       0.000\n",
    "ascii",
  );
  assert.throws(
    () => loadCodeDcb(unknownTimeSystem, null),
    (error) => error.name === "BiasError" && error.detail.kind === "departure",
  );
  const lenientDcb = loadCodeDcb(unknownTimeSystem, null, "lenient");
  const lenientDcbLossy = loadCodeDcbLossy(unknownTimeSystem, null, "lenient");
  assert.equal(lenientDcb.recordCount, 1);
  assert.equal(lenientDcb.timeScale, undefined);
  assert.deepEqual(lenientDcbLossy.records, lenientDcb.records);
  assert.deepEqual(lenientDcbLossy.noticeDetails, lenientDcb.noticeDetails);
  const again = loadCodeDcb(Buffer.from(dcb.toCodeDcbText(), "utf8"), null);
  assert.equal(again.recordCount, dcb.recordCount);
  const epoch = j2000FromUtc(2026, 6, 2);
  assert.equal(
    again.codeDsbSeconds("G01", "C1W", "C1C", epoch, "gpst").value,
    dcb.codeDsbSeconds("G01", "C1W", "C1C", epoch, "gpst").value,
  );

  // A product the strict reader accepts departs from nothing, so the lenient
  // reader reports the same notices.
  assert.deepEqual(loadBiasSinex(bytes, "lenient").notices, sinex.notices);
  assert.throws(() => loadBiasSinex(bytes, "loose"), TypeError);
});

test("Bias-SINEX departures retain typed details in strict and lenient modes", () => {
  const valid = coreFixture("bias/CODE.BIA").toString("utf8");
  const newline = valid.indexOf("\n");
  const malformed = `${valid.slice(0, newline - 1)}${valid.slice(newline)}`;
  const bytes = Buffer.from(malformed, "utf8");

  assert.throws(
    () => loadBiasSinex(bytes),
    (error) => {
      assert.equal(error.name, "BiasError");
      assert.equal(error.detail.kind, "departure");
      assert.deepEqual(error.detail.departure, {
        kind: "headerLayout",
        reason: "header line is not 74 columns",
      });
      return true;
    },
  );

  const parsed = loadBiasSinex(bytes, "lenient");
  const header_notice = parsed.noticeDetails.find(
    (notice) => notice.kind === "departure" && notice.departure.kind === "headerLayout",
  );
  assert.deepEqual(header_notice, {
    kind: "departure",
    departure: { kind: "headerLayout", reason: "header line is not 74 columns" },
  });
  assert.ok(parsed.notices.some((notice) => notice.includes("HeaderLayout")));
});

test("Bias-SINEX departure DTO maps every reachable variant field", () => {
  const base = coreFixture("bias/CODE.BIA").toString("utf8");
  const replaceOnce = (text, before, after) => {
    assert.ok(text.includes(before), `fixture text is missing ${before}`);
    return text.replace(before, after);
  };
  const insertBefore = (text, before, inserted) =>
    replaceOnce(text, before, `${inserted}${before}`);
  const removeBetween = (text, start, end) => {
    const a = text.indexOf(start);
    const b = text.indexOf(end, a);
    assert.ok(a >= 0 && b > a);
    return `${text.slice(0, a)}${text.slice(b)}`;
  };
  const cases = [
    [replaceOnce(base, "%=BIA 1.00", "%=BIA 2.00"), { kind: "otherVersion", version: "2.00" }],
    [base.replace("%=ENDBIA\n", ""), { kind: "missingFooter" }],
    [`${base}after-footer\n`, { kind: "contentAfterFooter", line: 424 }],
    [
      insertBefore(base, "%=ENDBIA", "%=UNEXPECTED\n"),
      { kind: "unexpectedControlLine", line: 423 },
    ],
    [
      replaceOnce(base, "-BIAS/SOLUTION\n", ""),
      { kind: "unclosedBlock", name: "BIAS/SOLUTION", line: 69 },
    ],
    [
      insertBefore(base, "%=ENDBIA", "-NO/SUCH/BLOCK\n"),
      { kind: "unopenedBlockEnd", name: "NO/SUCH/BLOCK", line: 423 },
    ],
    [
      replaceOnce(base, "-BIAS/SOLUTION\n", "-NO/SUCH/BLOCK\n"),
      { kind: "mismatchedBlockEnd", open: "BIAS/SOLUTION", close: "NO/SUCH/BLOCK", line: 422 },
    ],
    [
      insertBefore(base, "*BIAS SVN_", "+INNER/BLOCK\n"),
      { kind: "nestedBlock", open: "BIAS/SOLUTION", inner: "INNER/BLOCK", line: 70 },
    ],
    [
      removeBetween(base, "+FILE/REFERENCE", "-FILE/REFERENCE"),
      { kind: "missingBlock", name: "FILE/REFERENCE" },
    ],
    [
      insertBefore(base, "%=ENDBIA", "+UNKNOWN/BLOCK\n-UNKNOWN/BLOCK\n"),
      { kind: "unknownBlock", name: "UNKNOWN/BLOCK", line: 423 },
    ],
    [
      replaceOnce(base, "+BIAS/SOLUTION\n", "+BIAS/SOLUTION 351\n"),
      { kind: "blockStartSuffix", line: 69 },
    ],
    [insertBefore(base, "+BIAS/SOLUTION", "orphan row\n"), { kind: "dataOutsideBlock", line: 69 }],
    [
      replaceOnce(base, " BIAS_MODE                               ABSOLUTE\n", ""),
      { kind: "missingDeclaration", keyword: "BIAS_MODE" },
    ],
    [
      replaceOnce(
        base,
        " BIAS_MODE                               ABSOLUTE",
        " BIAS_MODE                               RELATIVE",
      ),
      { kind: "headerModeMismatch", header: "A", description: "relative" },
    ],
    [
      replaceOnce(
        base,
        " BIAS_MODE                               ABSOLUTE",
        " BIAS_MODE                               UNKNOWN",
      ),
      { kind: "unsupportedBiasMode", line: 60, label: "UNKNOWN" },
    ],
    [
      replaceOnce(
        base,
        " TIME_SYSTEM                             G  ",
        " TIME_SYSTEM                             XYZ",
      ),
      { kind: "nonStandardTimeSystem", line: 62, label: "XYZ" },
    ],
    [
      replaceOnce(
        base,
        "%=BIA 1.00 COD 2026:182:31588 IGS 2026:152:00000 2026:182:00000 A 00000351",
        "%=BIA 1.00 COD 2026:182:31588 IGS 2026:152:00000 2026:182:00000 A 00000352",
      ),
      { kind: "estimateCountMismatch", declared: 352, solutionRows: 351 },
    ],
  ];

  for (const [text, expected] of cases) {
    const parsed = loadBiasSinex(Buffer.from(text, "utf8"), "lenient");
    const notice = parsed.noticeDetails.find(
      (item) => item.kind === "departure" && item.departure.kind === expected.kind,
    );
    assert.deepEqual(notice, { kind: "departure", departure: expected });
  }

  const generated = "# DCB P1-C1 2026-06 MARS\nABMF97103M001                 -1.365       0.050\n";
  const parsedDcb = loadCodeDcbLossy(Buffer.from(generated, "utf8"), null, "lenient");
  const dcbNotice = parsedDcb.noticeDetails.find(
    (item) => item.kind === "departure" && item.departure.kind === "unknownDcbTimeSystem",
  );
  assert.deepEqual(dcbNotice, {
    kind: "departure",
    departure: { kind: "unknownDcbTimeSystem", line: 1, label: "MARS" },
  });
  assert.throws(
    () => loadCodeDcb(Buffer.from(generated, "utf8")),
    (error) => {
      assert.equal(error.name, "BiasError");
      assert.deepEqual(error.detail, {
        kind: "departure",
        departure: { kind: "unknownDcbTimeSystem", line: 1, label: "MARS" },
      });
      return true;
    },
  );
});

test("SSR 4076 store retains complete IGS orbit, clock, and high-rate records", () => {
  const baseClock = hexToBytes(
    "d30024fec22e30d40060123702088fffffa00009ffff600006ffff200023ffff60000a7ffffe20482165",
  );
  const highRateClock = hexToBytes("d3000efec23030d4006012370410001340495867");
  const store = new SsrCorrectionStore();

  store.ingest(baseClock, true, 2400, 100000, "gpst");
  store.ingest(highRateClock, true, 2400, 100000, "gpst");

  assert.deepEqual(store.orbit("G01"), {
    source: "igsSsr",
    providerId: 291,
    solutionId: 7,
    navMessage: "igsSsr",
    hasNavMessageIndex: undefined,
    iode: 17,
    iodCrc: undefined,
    iodSsr: 6,
    basis: "velocityAligned",
    crsRegional: false,
    referencePoint: 0,
    radialM: 0.00030000000000000003,
    alongM: -0.0016,
    crossM: 0.002,
    radialRateMS: -0.000006,
    alongRateMS: 0.000028,
    crossRateMS: -0.000032,
    refEpochJ2000S: 820856800,
    transmittedEpochJ2000S: 820856800,
    updateIntervalS: 1,
  });

  assert.deepEqual(store.clock("G01"), {
    source: "igsSsr",
    providerId: 291,
    solutionId: 7,
    navMessage: "igsSsr",
    hasNavMessageIndex: undefined,
    iodSsr: 6,
    c0M: -0.001,
    c1MS: 0.000019999999999999998,
    c2MS2: -6e-7,
    highRate: {
      solution: {
        source: "igsSsr",
        providerId: 291,
        solutionId: 7,
      },
      iodSsr: 6,
      c0M: 0.0077,
      refEpochJ2000S: 820856800,
      transmittedEpochJ2000S: 820856800,
      updateIntervalS: 1,
    },
    highRateC0M: 0.0077,
    refEpochJ2000S: 820856800,
    transmittedEpochJ2000S: 820856800,
    updateIntervalS: 1,
  });
});
