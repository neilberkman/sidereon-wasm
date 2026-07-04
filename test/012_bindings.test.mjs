// sidereon-core 0.12 capability binding parity.
//
// Provenance: clock-stability reference values are from the public NIST SP 1065
// Section 12.4 Table 31 recurrence, terrain and IONEX fixtures are the core
// fixture copies in this binding repo, ARAIM inputs mirror the public WG-C ADD
// v3.0 Appendix D numerical example used by the core tests, and SBAS vectors are
// the same raw message bytes exercised by the existing binding smoke test.

import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import {
  DtedTerrain,
  Egm96FifteenMinuteGeoid,
  MmapTerrain,
  SbasCorrectionStore,
  TerrainGeoidModel,
  VerticalDatum,
  allanDeviation,
  angularSeparationCoords,
  araim,
  araimFaultModes,
  araimLpv200Allocation,
  computeAllanDeviations,
  decodeSbasMessage,
  dtedTreeToMmapStore,
  hadamardDeviation,
  ionexFromNodeSamples,
  ionexFromSamples,
  loadIonex,
  modifiedAdev,
  overlappingAdev,
  positionAngle,
  satToSbasPrn,
  sbasPrnToSat,
  terrainStoreChecksum64,
  timeDeviation,
} from "../pkg-node/sidereon.js";
import { fixture, fixtureJson, hexToF64 } from "./helpers.mjs";

const CORE_FIXTURES = fileURLToPath(new URL("./fixtures", import.meta.url));
const REPO_ROOT = fileURLToPath(new URL("..", import.meta.url));
const CORE_DTED_FIXTURES = `${CORE_FIXTURES}/dted`;
const CORE_DTED_TILES = `${CORE_DTED_FIXTURES}/tiles`;
const NIST_MODULUS = 2_147_483_647;
const NIST_MULTIPLIER = 16_807;
const NIST_SEED = 1_234_567_890;

const close = (actual, expected, tol, label) =>
  assert.ok(Math.abs(actual - expected) <= tol, `${label}: ${actual} vs ${expected}`);

const hexToBytes = (hex) =>
  Uint8Array.from(
    hex
      .trim()
      .match(/.{2}/g)
      .map((b) => parseInt(b, 16)),
  );

const coreDtedJson = () =>
  JSON.parse(readFileSync(`${CORE_DTED_FIXTURES}/dted_points.json`, "utf8"));

function dtedTreeStoreBytes(root) {
  try {
    return dtedTreeToMmapStore(root);
  } catch (err) {
    assert.equal(err.name, "Io");
    assert.equal(err.kind, "Io");
    assert.equal(err.detail.name, "Io");
    return Uint8Array.from(
      execFileSync("cargo", ["run", "--quiet", "--bin", "dted_tree_to_mmap_store", "--", root], {
        cwd: REPO_ROOT,
        maxBuffer: 16 * 1024 * 1024,
      }),
    );
  }
}

function nistFrequencyData(len) {
  let state = NIST_SEED;
  const values = [];
  for (let i = 0; i < len; i++) {
    values.push(state / NIST_MODULUS);
    state = (NIST_MULTIPLIER * state) % NIST_MODULUS;
  }
  return values;
}

function assertCurve(curve, expectedN, expectedDeviation, label) {
  assert.deepEqual(curve.tauS, [1, 10, 100], `${label} tau`);
  assert.deepEqual(curve.n, expectedN, `${label} n`);
  for (let i = 0; i < expectedDeviation.length; i++) {
    close(
      curve.deviation[i],
      expectedDeviation[i][0],
      expectedDeviation[i][1],
      `${label} dev ${i}`,
    );
  }
}

test("clock stability estimators match the public reference table", () => {
  const series = { kind: "fractionalFrequency", values: nistFrequencyData(1000) };
  const m = [1, 10, 100];

  assertCurve(
    allanDeviation(series, 1.0, m),
    [999, 99, 9],
    [
      [2.922_319e-1, 5e-8],
      [9.965_736e-2, 5e-9],
      [3.897_804e-2, 5e-9],
    ],
    "ADEV",
  );
  assertCurve(
    overlappingAdev(series, 1.0, m),
    [999, 981, 801],
    [
      [2.922_319e-1, 5e-8],
      [9.159_953e-2, 5e-9],
      [3.241_343e-2, 5e-9],
    ],
    "OADEV",
  );
  assertCurve(
    modifiedAdev(series, 1.0, m),
    [999, 972, 702],
    [
      [2.922_319e-1, 5e-8],
      [6.172_376e-2, 5e-9],
      [2.170_921e-2, 5e-9],
    ],
    "MDEV",
  );
  assertCurve(
    hadamardDeviation(series, 1.0, m),
    [998, 971, 701],
    [
      [2.943_883e-1, 5e-8],
      [9.581_083e-2, 5e-9],
      [3.237_638e-2, 5e-9],
    ],
    "HDEV",
  );
  assertCurve(
    timeDeviation(series, 1.0, m),
    [999, 972, 702],
    [
      [1.687_202e-1, 5e-8],
      [3.563_623e-1, 5e-8],
      [1.253_382e0, 5e-7],
    ],
    "TDEV",
  );

  const gapped = computeAllanDeviations({
    series: {
      kind: "phaseSecondsWithGaps",
      values: [0, 1, 2, null, 4, 8, 16],
    },
    tau0S: 1.0,
    options: {
      estimators: { overlappingAdev: true },
      tauGrid: { kind: "explicit", averagingFactors: [1] },
      gapPolicy: "omitTerms",
    },
  });
  assert.deepEqual(gapped.overlappingAdev.n, [2]);
  assert.equal(gapped.overlappingAdev.deviation[0], 2.0);
});

test("DTED heightBatch matches scalar ORTHOMETRIC terrain lookups", () => {
  const terrain = new DtedTerrain(`${CORE_FIXTURES}/dted/tiles`);
  const points = fixtureJson("dted/dted_points.json");
  const cases = points.bilinear_cases.map((p) => [
    hexToF64(p.longitude_bits),
    hexToF64(p.latitude_bits),
  ]);

  const batch = terrain.heightBatch(cases, { interpolation: "bilinear" });
  assert.equal(batch.length, cases.length);
  for (let i = 0; i < cases.length; i++) {
    assert.equal(batch[i].ok, true);
    assert.equal(batch[i].heightM, terrain.heightMWithOptions(cases[i][0], cases[i][1], {}));
  }

  const withError = terrain.heightBatch([[Number.NaN, 36.5], cases[0]], {});
  assert.equal(withError[0].ok, false);
  assert.match(withError[0].error, /longitude/);
  assert.equal(withError[1].ok, true);
});

test("mmap terrain store built from DTED fixtures matches DTED terrain", () => {
  const bytes = dtedTreeStoreBytes(CORE_DTED_TILES);
  assert.ok(bytes instanceof Uint8Array);
  assert.ok(bytes.length > 0);

  const checksum = terrainStoreChecksum64(bytes);
  assert.equal(typeof checksum, "bigint");

  const store = MmapTerrain.fromBytes(bytes);
  assert.equal(store.checksum64(), checksum);
  assert.equal(MmapTerrain.fromVec(store.toBytes()).checksum64(), checksum);
  assert.equal(store.verticalDatum, VerticalDatum.Egm96MslOrthometric);

  const index = store.tileIndex();
  assert.equal(index.length, 2);
  assert.ok(index.some((tile) => tile.latIndex === 36 && tile.lonIndex === -107));
  assert.ok(index.some((tile) => tile.latIndex === 36 && tile.lonIndex === -106));
  assert.ok(index.every((tile) => tile.verticalDatum === VerticalDatum.Egm96MslOrthometric));

  const cases = coreDtedJson().multi_tile_cases.map((p) => ({
    longitudeDeg: hexToF64(p.longitude_bits),
    latitudeDeg: hexToF64(p.latitude_bits),
    bilinearM: hexToF64(p.bilinear_bits),
    nearestM: hexToF64(p.nearest_bits),
  }));

  const points = cases.map((p) => [p.longitudeDeg, p.latitudeDeg]);
  const batch = store.heightBatch(points, { interpolation: "bilinear" });
  const typedBatch = store.orthometricHeightBatch(points, { interpolation: "bilinear" });
  assert.equal(batch.length, cases.length);
  assert.equal(typedBatch.length, cases.length);

  for (let i = 0; i < cases.length; i++) {
    const p = cases[i];
    assert.equal(batch[i].ok, true);
    assert.equal(typedBatch[i].ok, true);
    assert.equal(batch[i].heightM, p.bilinearM);
    assert.equal(store.heightM(p.longitudeDeg, p.latitudeDeg), batch[i].heightM);
    assert.equal(store.orthometricHeightM(p.longitudeDeg, p.latitudeDeg).valueM, batch[i].heightM);
    assert.equal(typedBatch[i].orthometricHeightM.valueM, batch[i].heightM);
    assert.equal(
      store.heightMWithOptions(p.longitudeDeg, p.latitudeDeg, {
        interpolation: "nearestPosting",
      }),
      p.nearestM,
    );
  }

  const oneDegree = TerrainGeoidModel.egm96OneDegree();
  const p = cases[0];
  const ellipsoidal = store.ellipsoidalHeightM(p.longitudeDeg, p.latitudeDeg);
  assert.equal(typeof ellipsoidal.valueM, "number");
  assert.equal(
    store.ellipsoidalHeightMWithModel(p.longitudeDeg, p.latitudeDeg, {}, oneDegree).valueM,
    ellipsoidal.valueM,
  );
  assert.equal(
    store
      .orthometricHeightM(p.longitudeDeg, p.latitudeDeg)
      .toEllipsoidalHeightDeg(p.latitudeDeg, p.longitudeDeg, oneDegree).valueM,
    ellipsoidal.valueM,
  );

  assert.throws(
    () => Egm96FifteenMinuteGeoid.fromWw15mghDacPath(`${CORE_DTED_FIXTURES}/WW15MGH.DAC`),
    (err) => {
      assert.equal(err.name, "MissingEgm96Dac");
      assert.equal(err.kind, "MissingEgm96Dac");
      assert.match(err.path, /WW15MGH\.DAC$/);
      assert.match(err.remediation, /WW15MGH\.DAC/);
      assert.equal(err.detail.name, "MissingEgm96Dac");
      assert.match(err.detail.path, /WW15MGH\.DAC$/);
      assert.match(err.detail.remediation, /WW15MGH\.DAC/);
      return true;
    },
  );
});

test("IONEX samples rebuild parsed products through both sample constructors", () => {
  const ionex = loadIonex(fixture("synthetic_2map_7x7.20i"));
  const gridSamples = ionex.tecGridSamples();
  const fromGrid = ionexFromSamples(gridSamples);
  const fromNodes = ionexFromNodeSamples(
    ionex.tecSamples(),
    ionex.shellHeightKm,
    ionex.baseRadiusKm,
    ionex.exponent,
  );

  assert.deepEqual(fromGrid.tecGridSamples(), gridSamples);
  assert.deepEqual(fromNodes.tecGridSamples(), gridSamples);
  const epoch = ionex.mapEpochsJ2000S[0];
  assert.equal(
    fromGrid.slantDelay(12, 34, 45, 30, epoch, 1575.42e6),
    ionex.slantDelay(12, 34, 45, 30, epoch, 1575.42e6),
  );
  assert.equal(fromNodes.toIonexString(), fromGrid.toIonexString());
});

test("SBAS decode payload and store accessors expose core message data", () => {
  const mt2 = hexToBytes("5308DFFC010005FFC00DFFC009FFDFFC001FFDFFDFFFBABBBBBB9BBB80");
  const decoded = decodeSbasMessage(mt2, "body226");
  assert.equal(decoded.messageType, 2);
  assert.equal(decoded.form, "body226");
  assert.match(decoded.kind, /FastCorrections/);
  assert.equal(decoded.message.kind, "fastCorrections");
  assert.equal(decoded.message.prc.length, 13);
  assert.equal(decoded.message.udrei.length, 13);

  assert.equal(sbasPrnToSat(129), "S29");
  assert.equal(satToSbasPrn("S29"), 129);

  const store = new SbasCorrectionStore();
  const mt9 = hexToBytes("9A25C80C8D3F574632853C69A015EEBFF2D7DF580018FE3FCFF79C38C0");
  store.ingest(mt9, "body226", "S29", 1619, 432018.0, "gpst");
  const geo = store.geoNavState("S29");
  assert.ok(geo);
  assert.equal(geo.positionEcefM.length, 3);
  assert.ok(geo.positionEcefM.every(Number.isFinite));
  assert.equal(store.fastCorrection("S29", "G01"), null);
  assert.equal(store.ionoGrid("S29"), null);
});

const WG_C_ROWS = [
  ["G01", "GPS", [0.0225, 0.9951, -0.0966], 3.8865, 3.574],
  ["G02", "GPS", [0.675, -0.69, -0.2612], 1.4377, 1.1252],
  ["G03", "GPS", [0.0723, -0.6601, -0.7477], 0.8604, 0.5479],
  ["G04", "GPS", [-0.9398, 0.2553, -0.2269], 1.6383, 1.3258],
  ["G05", "GPS", [-0.5907, -0.7539, -0.2877], 1.3229, 1.0104],
  ["E01", "Galileo", [-0.3236, -0.0354, -0.9455], 0.8434, 0.5309],
  ["E02", "Galileo", [-0.6748, 0.4356, -0.5957], 0.8963, 0.5838],
  ["E03", "Galileo", [0.0938, -0.7004, -0.7075], 0.8669, 0.5544],
  ["E04", "Galileo", [0.5571, 0.3088, -0.7709], 0.8573, 0.5448],
  ["E05", "Galileo", [0.6622, 0.6958, -0.278], 1.3616, 1.0491],
];

function wgCGeometry() {
  const elevation = (cAccM2) => {
    const sigmaUre = 0.5;
    const a = 0.3;
    const b = 0.3;
    return Math.asin(Math.sqrt((b * b) / (cAccM2 - sigmaUre * sigmaUre - a * a)));
  };
  return {
    receiver: { latRad: 0, lonRad: 0, heightM: 0 },
    clockSystems: ["GPS", "Galileo"],
    rows: WG_C_ROWS.map(([id, system, designEnu, , cAccM2]) => {
      const east = -designEnu[0];
      const north = -designEnu[1];
      const up = -designEnu[2];
      return {
        id,
        system,
        lineOfSight: [up, east, north],
        elevationRad: elevation(cAccM2),
      };
    }),
  };
}

function wgCIsm() {
  const model = {
    sigmaUraM: 0.75,
    sigmaUreM: 0.5,
    bNomM: 0.5,
    pSat: 1e-5,
  };
  return {
    constellations: [
      { system: "GPS", pConst: 1e-4, defaultSat: model },
      { system: "Galileo", pConst: 1e-4, defaultSat: model },
    ],
    satellites: WG_C_ROWS.map(([id, , , cIntM2, cAccM2]) => ({
      id,
      sigmaUraM: 0.75,
      sigmaUreM: 0.5,
      bNomM: 0.5,
      pSat: 1e-5,
      effectiveSigmaIntM: Math.sqrt(cIntM2),
      effectiveSigmaAccM: Math.sqrt(cAccM2),
    })),
  };
}

test("ARAIM returns the public WG-C protection-level reference", () => {
  const geometry = wgCGeometry();
  const ism = wgCIsm();
  const allocation = araimLpv200Allocation();
  assert.equal(allocation.pEmt, 1e-5);
  const result = araim(geometry, ism, allocation);

  assert.equal(result.availability, true);
  close(result.vplM, 19.2, 0.1, "VPL m");
  close(result.hplM, 14.5, 0.1, "HPL m");
  close(result.emtM, 7.8, 0.1, "EMT m");
  close(result.sigmaAccVM, 1.47, 0.02, "vertical accuracy sigma m");
  assert.ok(result.faultModes.length > 1);

  const modes = araimFaultModes(geometry, ism, allocation);
  assert.equal(modes[0].excluded.length, 0);
  assert.ok(modes.some((mode) => mode.excludedConstellation === "Galileo"));
});

test("astro angle helpers keep longitude-first degree semantics", () => {
  close(angularSeparationCoords(0, 0, 90, 0), 90.0, 1e-12, "separation deg");
  close(positionAngle(0, 0, 90, 0), 90.0, 1e-12, "position angle deg");
  close(positionAngle(0, 0, 0, 45), 0.0, 1e-12, "northward position angle deg");
  assert.throws(() => angularSeparationCoords(0, 91, 90, 0), Error);
});
