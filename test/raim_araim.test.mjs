import { test } from "node:test";
import assert from "node:assert/strict";

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import {
  araim,
  araimLpv200Allocation,
  loadSp3,
  RaimWeights,
  raim,
  raimForSolution,
} from "../pkg-node/sidereon.js";

const here = (rel) => fileURLToPath(new URL(rel, import.meta.url));

const close = (actual, expected, tol, label) =>
  assert.ok(Math.abs(actual - expected) <= tol, `${label}: ${actual} vs ${expected}`);

test("direct raim runs over post-fit residual lists", () => {
  const input = {
    usedSats: ["G01", "G02", "G03", "G04", "G05", "G06"],
    residualsM: [0.2, -0.1, 0.3, 0.2, 9.0, -0.2],
  };
  const result = raim(input, { pFa: 1e-3 });

  assert.equal(result.faultDetected, true);
  close(result.testStatistic, 81.22, 1e-12, "test statistic");
  assert.equal(result.dof, 2);
  assert.equal(result.worstSat, "G05");
  close(result.reducedChiSquare, 40.61, 1e-12, "reduced chi-square");
  close(result.rmsM, Math.sqrt(81.22 / 6), 1e-12, "RMS residual");
  assert.equal(result.normalizedResiduals.G05, 9.0);
});

test("direct raim builds weights from elevation and cn0 entries", () => {
  const input = {
    usedSats: ["G01", "G02", "G03", "G04", "G05", "G06"],
    residualsM: [0.2, -0.1, 0.3, 0.2, 2.0, -0.2],
  };
  const result = raim(input, {
    pFa: 1e-3,
    weightEntries: [
      { satelliteId: "G01", elevationDeg: 75.0, cn0Dbhz: 48.0 },
      { satelliteId: "G02", elevationDeg: 65.0, cn0Dbhz: 47.0 },
      { satelliteId: "G03", elevationDeg: 55.0, cn0Dbhz: 46.0 },
      { satelliteId: "G04", elevationDeg: 45.0, cn0Dbhz: 45.0 },
      { satelliteId: "G05", elevationDeg: 35.0, cn0Dbhz: 44.0 },
      { satelliteId: "G06", elevationDeg: 25.0, cn0Dbhz: 43.0 },
    ],
    varianceOptions: { model: "elevation_cn0" },
  });

  assert.equal(result.dof, 2);
  assert.equal(result.worstSat, "G05");
  assert.ok(Number.isFinite(result.testStatistic));
  assert.ok(result.normalizedResiduals.G05 > input.residualsM[4]);

  const weighted = raim(input, {
    weights: RaimWeights.bySatellite(["G05"], Float64Array.from([4.0])),
  });
  close(weighted.testStatistic, 16.22, 1e-12, "class weight statistic");
  assert.equal(weighted.normalizedResiduals.G05, 4.0);
});

test("raimForSolution runs over a real SPP solution", () => {
  const sp3 = loadSp3(
    readFileSync(here("./fixtures/sp3/GBM0MGXRAP_20201770000_01D_05M_ORB_120epoch.sp3")),
  );
  const tRx = sp3.epochsJ2000Seconds()[12];
  const rx = [3582105.291, 532589.7313, 5232754.8054];
  const observations = ["G05", "G07", "G08", "G10", "G13", "G15"].map((satelliteId) => {
    const state = sp3.interpolate(satelliteId, Float64Array.of(tRx));
    const range = Math.hypot(
      state.positionM[0] - rx[0],
      state.positionM[1] - rx[1],
      state.positionM[2] - rx[2],
    );
    return { satelliteId, pseudorangeM: range - 299792458.0 * state.clockS[0] };
  });
  const solution = sp3.solveSpp({
    observations,
    tRxJ2000S: tRx,
    tRxSecondOfDayS: 3600,
    dayOfYear: 177,
    initialGuess: [rx[0], rx[1], rx[2], 0],
    corrections: { ionosphere: false, troposphere: false },
  });

  const result = raimForSolution(solution, { pFa: 1e-3 });
  assert.equal(result.faultDetected, true);
  assert.equal(result.dof, solution.usedSats.length - 4);
  assert.equal(result.worstSat, "G08");
  close(result.testStatistic, 19.00967387255123, 1e-9, "solution RAIM statistic");
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

test("direct araim returns protection levels from geometry", () => {
  const result = araim(wgCGeometry(), wgCIsm(), araimLpv200Allocation());

  assert.equal(result.available, true);
  assert.equal(result.availability, true);
  close(result.hplM, 14.5, 0.1, "HPL m");
  close(result.vplM, 19.2, 0.1, "VPL m");
  close(result.sigmaAccHM, 0.8695570268695054, 1e-12, "horizontal accuracy sigma m");
  close(result.sigmaAccVM, 1.47, 0.02, "vertical accuracy sigma m");
});

function sparseGpsGeometry() {
  const s = 1 / Math.sqrt(3);
  return {
    receiver: { latRad: 0, lonRad: 0, heightM: 0 },
    clockSystems: ["GPS"],
    rows: [
      { id: "G01", lineOfSight: [s, s, s], elevationRad: Math.PI / 2 },
      { id: "G02", lineOfSight: [s, -s, -s], elevationRad: Math.PI / 2 },
      { id: "G03", lineOfSight: [-s, s, -s], elevationRad: Math.PI / 2 },
      { id: "G04", lineOfSight: [-s, -s, s], elevationRad: Math.PI / 2 },
    ],
  };
}

function sparseGpsIsm() {
  return {
    constellations: [
      {
        system: "GPS",
        pConst: 0,
        defaultSat: { sigmaUraM: 0.75, sigmaUreM: 0.5, bNomM: 0.75, pSat: 1e-5 },
      },
    ],
  };
}

test("direct araim returns unavailable for sparse GPS geometry", () => {
  const result = araim(sparseGpsGeometry(), sparseGpsIsm(), araimLpv200Allocation());

  assert.equal(result.available, false);
  assert.equal(result.availability, false);
  assert.equal(result.hplM, Infinity);
  assert.equal(result.vplM, Infinity);
});

test("direct araim reports a clear bad lineOfSight length", () => {
  const geometry = sparseGpsGeometry();
  geometry.rows[0] = { ...geometry.rows[0], lineOfSight: [1, 0] };

  assert.throws(
    () => araim(geometry, sparseGpsIsm(), araimLpv200Allocation()),
    /lineOfSight|length 3|array of length 3/,
  );
});
