// Sequential RTK baseline arc driver delegates to
// sidereon_core::rtk_filter::arc::solve_rtk_arc. The driver takes raw rover+base
// observations per epoch (not the SD-prepared rows the batch solveRtkFloat takes)
// and returns one reported baseline/ambiguity solution per epoch plus the carried
// filter state. We synthesize the arc epochs from the committed static WTZR/WTZZ
// RTK fixture used by rtk.test.mjs.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  buildDualFrequencyRinexRtkArc,
  buildRinexRtkArc,
  fixWideLaneRtkArc,
  loadSp3,
  parseRinexObs,
  prepareIonosphereFreeRtkArc,
  solveRtkArc,
  solveStaticReferenceStationRinex,
  solveStaticRinexRtkBaseline,
  solveStaticRtkArc,
  solveWideLaneFixedRinexRtkBaseline,
} from "../pkg-node/sidereon.js";
import { f64Bits, fixture, fixtureJson, norm } from "./helpers.mjs";

// GPS L1 wavelength (metres) the fixture's ambiguities use.
const L1_WAVELENGTH_M = 0.19029367279836487;
const F_L1_HZ = 1575.42e6;
const F_L2_HZ = 1227.6e6;
const WTZR_MARKER_M = [4075580.3111, 931854.0543, 4801568.2808];
const WTZZ_MARKER_M = [4075579.1913, 931853.3696, 4801569.1897];
const WTZR_OBS = "WTZR00DEU_R_20201770000_01D_30S_MO_120epoch.rnx";
const WTZZ_OBS = "WTZZ00DEU_R_20201770000_01D_30S_MO_120epoch.rnx";
const WTZR_WTZZ_SP3 = "GBM0MGXRAP_20201770000_01D_05M_ORB_120epoch.sp3";

const fx = fixtureJson("rtk_wtzr.json");

// Build raw arc epochs from the batch fixture's per-satellite code/phase rows.
const arcEpochs = fx.epochs.map((epoch) => {
  const rows = [...epoch.references, ...epoch.nonref];
  const satellitePositionsM = {};
  const baseSatellitePositionsM = {};
  const roverSatellitePositionsM = {};
  for (const r of rows) {
    satellitePositionsM[r.sat] = r.pos;
    baseSatellitePositionsM[r.sat] = r.base_tx_pos;
    roverSatellitePositionsM[r.sat] = r.rover_tx_pos;
  }
  return {
    base: rows.map((r) => ({
      satelliteId: r.sat,
      ambiguityId: r.sat,
      codeM: r.base_code_m,
      phaseM: r.base_phase_m,
    })),
    rover: rows.map((r) => ({
      satelliteId: r.sat,
      ambiguityId: r.sat,
      codeM: r.rover_code_m,
      phaseM: r.rover_phase_m,
    })),
    satellitePositionsM,
    baseSatellitePositionsM,
    roverSatellitePositionsM,
  };
});

// Every satellite token that appears across the arc needs a wavelength/offset.
const tokens = new Set();
for (const epoch of fx.epochs) {
  for (const r of [...epoch.references, ...epoch.nonref]) tokens.add(r.sat);
}
const wavelengthsM = {};
const offsetsM = {};
for (const t of tokens) {
  wavelengthsM[t] = L1_WAVELENGTH_M;
  offsetsM[t] = 0.0;
}

const config = {
  baseM: fx.base_arp_m,
  model: {
    codeSigmaM: fx.model.code_sigma_m,
    phaseSigmaM: fx.model.phase_sigma_m,
    sagnac: fx.model.sagnac,
    stochastic: fx.model.stochastic.kind,
    elevationWeighting: fx.model.stochastic.elevation_weighting ?? false,
  },
  baselinePriorSigmaM: 100.0,
  ambiguityPriorSigmaM: 1000.0,
  wavelengthsM,
  offsetsM,
};

const SLIP_SATELLITE = "G05";
const SLIP_EPOCH_INDEX = 3;
const MASK_DEG = 15.0;
const MASKED_SATS_AT_15_DEG = ["G08", "G09", "G15", "G18", "G21", "G27"];
const SPLIT_SCALE_IDS = [`${SLIP_SATELLITE}@rover#1`, `${SLIP_SATELLITE}@rover#2`];

const dualBaseM = [3512900.0, 780500.0, 5248700.0];
const dualSatellitePositionsM = {
  G01: [14350000.0, 3190000.0, 21440000.0],
  G02: [20000000.0, 3000000.0, 18000000.0],
  G03: [9000000.0, 9000000.0, 22000000.0],
  G04: [16000000.0, -4000000.0, 21000000.0],
};

const dualArcEpochTemplate = [
  {
    satelliteId: "G01",
    base: {
      ambiguityId: "G01",
      p1M: 20000020.0,
      p2M: 20000022.0,
      phi1Cycles: 2.0,
      phi2Cycles: 0.0,
      f1Hz: F_L1_HZ,
      f2Hz: F_L2_HZ,
    },
    rover: {
      ambiguityId: "G01",
      p1M: 20000050.0,
      p2M: 20000052.5,
      phi1Cycles: 5.0,
      phi2Cycles: 0.0,
      f1Hz: F_L1_HZ,
      f2Hz: F_L2_HZ,
    },
  },
  {
    satelliteId: "G02",
    base: {
      ambiguityId: "G02",
      p1M: 20000010.0,
      p2M: 20000012.0,
      phi1Cycles: 1.0,
      phi2Cycles: 0.0,
      f1Hz: F_L1_HZ,
      f2Hz: F_L2_HZ,
    },
    rover: {
      ambiguityId: "G02",
      p1M: 20000042.0,
      p2M: 20000044.5,
      phi1Cycles: 7.0,
      phi2Cycles: 0.0,
      f1Hz: F_L1_HZ,
      f2Hz: F_L2_HZ,
    },
  },
  {
    satelliteId: "G03",
    base: {
      ambiguityId: "G03",
      p1M: 19999980.0,
      p2M: 19999982.0,
      phi1Cycles: -2.0,
      phi2Cycles: 0.0,
      f1Hz: F_L1_HZ,
      f2Hz: F_L2_HZ,
    },
    rover: {
      ambiguityId: "G03",
      p1M: 20000005.0,
      p2M: 20000007.5,
      phi1Cycles: 0.0,
      phi2Cycles: 0.0,
      f1Hz: F_L1_HZ,
      f2Hz: F_L2_HZ,
    },
  },
  {
    satelliteId: "G04",
    base: {
      ambiguityId: "G04",
      p1M: 20000040.0,
      p2M: 20000042.0,
      phi1Cycles: 4.0,
      phi2Cycles: 0.0,
      f1Hz: F_L1_HZ,
      f2Hz: F_L2_HZ,
    },
    rover: {
      ambiguityId: "G04",
      p1M: 20000073.0,
      p2M: 20000075.5,
      phi1Cycles: 8.0,
      phi2Cycles: 0.0,
      f1Hz: F_L1_HZ,
      f2Hz: F_L2_HZ,
    },
  },
];

const dualArcEpochs = ["000", "001", "002"].map((epochSortKey) => ({
  jdWhole: 2460100.5,
  jdFraction: 0.25,
  epochSortKey,
  observations: dualArcEpochTemplate,
  satellitePositionsM: dualSatellitePositionsM,
}));

const wideLaneConfig = {
  baseM: dualBaseM,
  options: {
    minEpochs: 2,
    toleranceCycles: 0.5,
    skipShortFragments: false,
  },
};

function withExtraScale(ids) {
  const nextWavelengthsM = { ...config.wavelengthsM };
  const nextOffsetsM = { ...config.offsetsM };
  for (const id of ids) {
    nextWavelengthsM[id] = L1_WAVELENGTH_M;
    nextOffsetsM[id] = 0.0;
  }
  return {
    ...config,
    wavelengthsM: nextWavelengthsM,
    offsetsM: nextOffsetsM,
  };
}

function withRoverLli(epochs, satelliteId, epochIndex) {
  return epochs.map((epoch, i) => ({
    ...epoch,
    base: epoch.base.map((obs) => ({ ...obs })),
    rover: epoch.rover.map((obs) =>
      i === epochIndex && obs.satelliteId === satelliteId ? { ...obs, lli: 1 } : { ...obs },
    ),
  }));
}

function arpPosition(markerM, obs) {
  const [heightM, eastM, northM] = obs.header.antennaDeltaHenM;
  assert.equal(eastM, 0.0);
  assert.equal(northM, 0.0);
  const radiusM = norm(markerM);
  return markerM.map((component) => component + (component / radiusM) * heightM);
}

function vectorErrorM(actual, expected) {
  return norm(actual.map((value, i) => value - expected[i]));
}

function squareCovarianceLength(flat, n) {
  assert.equal(flat.length, n * n);
  assert.ok(flat.every(Number.isFinite));
}

function wettzellRinexInputs() {
  const sp3 = loadSp3(fixture(`sp3/${WTZR_WTZZ_SP3}`));
  const baseObs = parseRinexObs(fixture(`obs/${WTZR_OBS}`));
  const roverObs = parseRinexObs(fixture(`obs/${WTZZ_OBS}`));
  const baseArpM = arpPosition(WTZR_MARKER_M, baseObs);
  const roverArpM = arpPosition(WTZZ_MARKER_M, roverObs);
  const truthBaselineM = roverArpM.map((component, i) => component - baseArpM[i]);
  return { sp3, baseObs, roverObs, baseArpM, truthBaselineM };
}

const realArcModel = {
  codeSigmaM: 2.0,
  phaseSigmaM: 0.01,
  sagnac: true,
  stochastic: "simple",
  elevationWeighting: true,
};

const realArcSolveOptions = {
  positionTolM: 1.0e-4,
  ambiguityTolM: 1.0e-4,
  maxIterations: 10,
};

test("solveRtkArc reports one solution per epoch and carries the filter state", () => {
  const sol = solveRtkArc(arcEpochs, config);

  assert.equal(sol.epochs.length, fx.epochs.length);
  // The highest-elevation GPS satellite (G30) is the per-system reference.
  assert.equal(sol.references.G, "G30");

  const last = sol.epochs[sol.epochs.length - 1];
  for (const v of last.reportedBaselineM) assert.ok(Number.isFinite(v));
  for (const v of last.floatBaselineM) assert.ok(Number.isFinite(v));
  assert.ok(["Weak", "Nominal"].includes(last.geometryQuality.tier));
  assert.equal(last.geometryQuality.covarianceValidated, true);
  assert.equal(last.geometryQuality.raimCheckable, true);
  // The static arc converges near the batch float baseline (the sequential
  // filter with integer holds is not identical, so a loose metre-scale check).
  const exp = fx.expected.float_baseline_m;
  for (let i = 0; i < 3; i++) {
    assert.ok(Math.abs(last.reportedBaselineM[i] - exp[i]) <= 0.1, `baseline[${i}]`);
  }

  // Final carried state is well-formed: n = 3 + sdAmbiguityIds.length.
  const n = 3 + sol.finalState.sdAmbiguityIds.length;
  assert.equal(sol.finalState.information.length, n * n);
  assert.equal(sol.finalState.epochCount, fx.epochs.length);
  assert.equal(sol.finalState.baselineM.length, 3);
});

test("solveRtkArc exposes preprocessing metadata and covariance", () => {
  const slipped = withRoverLli(arcEpochs, SLIP_SATELLITE, SLIP_EPOCH_INDEX);

  const split = solveRtkArc(slipped, {
    ...withExtraScale(SPLIT_SCALE_IDS),
    preprocessing: {
      cycleSlip: "splitArc",
      hatchWindowCap: 100,
      elevationMaskDeg: MASK_DEG,
    },
  });

  assert.deepEqual(split.droppedSats, []);
  assert.deepEqual(split.elevationMaskedSats, MASKED_SATS_AT_15_DEG);
  assert.deepEqual(split.splitCycleSlipArcs, [
    {
      receiver: "rover",
      satelliteId: SLIP_SATELLITE,
      ambiguityId: `${SLIP_SATELLITE}@rover#1`,
      startEpochIndex: 0,
      endEpochIndex: SLIP_EPOCH_INDEX - 1,
      nEpochs: SLIP_EPOCH_INDEX,
    },
    {
      receiver: "rover",
      satelliteId: SLIP_SATELLITE,
      ambiguityId: `${SLIP_SATELLITE}@rover#2`,
      startEpochIndex: SLIP_EPOCH_INDEX,
      endEpochIndex: fx.epochs.length - 1,
      nEpochs: fx.epochs.length - SLIP_EPOCH_INDEX,
    },
  ]);
  assert.equal(split.measurementCovariance.length, split.finalState.information.length);
  assert.ok(split.measurementCovariance.every(Number.isFinite));
  assert.ok(!split.epochs.at(-1).usedSatelliteIds.includes("G08"));

  const dropped = solveRtkArc(slipped, {
    ...config,
    preprocessing: {
      cycleSlip: "dropSatellite",
      elevationMaskDeg: MASK_DEG,
    },
  });

  assert.deepEqual(dropped.droppedSats, [SLIP_SATELLITE]);
  assert.deepEqual(dropped.splitCycleSlipArcs, []);
  assert.deepEqual(dropped.elevationMaskedSats, MASKED_SATS_AT_15_DEG);
  assert.equal(dropped.measurementCovariance.length, dropped.finalState.information.length);
  assert.ok(dropped.measurementCovariance.every(Number.isFinite));
  assert.ok(!dropped.epochs.at(-1).usedSatelliteIds.includes(SLIP_SATELLITE));
});

test("solveRtkArc fixes integer ambiguities on the static arc", () => {
  const sol = solveRtkArc(arcEpochs, config);
  const last = sol.epochs[sol.epochs.length - 1];
  assert.equal(last.integerFixed, true);
  assert.ok(last.search);
  assert.equal(last.search.integerStatus, "Fixed");
  // The static arc clears the default LAMBDA acceptance ratio (3.0) comfortably.
  assert.ok(last.search.integerRatio > 3.0);
});

test("solveRtkArc surfaces the per-epoch innovation screen when enabled", () => {
  // The screen is off by default, so the per-epoch result carries no screen.
  const plain = solveRtkArc(arcEpochs, config);
  for (const epoch of plain.epochs) {
    assert.equal(epoch.innovationScreen, undefined);
  }

  // Enabling it via updateOpts surfaces the core InnovationScreen on every epoch.
  const screened = solveRtkArc(arcEpochs, {
    ...config,
    updateOpts: { innovationScreen: { thresholdSigma: 5.0, minRows: 1 } },
  });
  assert.equal(screened.epochs.length, arcEpochs.length);
  for (const epoch of screened.epochs) {
    const s = epoch.innovationScreen;
    assert.ok(s, "innovation screen present");
    assert.equal(s.thresholdSigma, 5.0);
    assert.equal(s.minRows, 1);
    assert.ok(Number.isInteger(s.inputRows) && s.inputRows > 0, "input rows counted");
    assert.equal(s.inputRows, s.acceptedRows + s.rejectedRows, "accepted + rejected = input");
    assert.equal(s.rejectedRows, s.rejectedCodeRows + s.rejectedPhaseRows, "code + phase rejected");
    assert.equal(typeof s.coasted, "boolean");
  }
});

test("solveStaticRtkArc returns one float and one fixed solution for the arc", () => {
  const sol = solveStaticRtkArc(arcEpochs, { arc: config });

  assert.equal(sol.references.G, "G30");
  assert.equal(sol.geometryQuality.tier, "Nominal");
  assert.equal(sol.geometryQuality.covarianceValidated, true);
  assert.ok(sol.ambiguityIds.length > 0);
  assert.equal(sol.floatSolution.baselineM.length, 3);
  assert.equal(sol.floatSolution.geometryQuality.tier, sol.geometryQuality.tier);
  assert.ok(sol.floatSolution.baselineM.every(Number.isFinite));
  assert.equal(sol.fixedSolution.fixedSolution.baselineM.length, 3);
  assert.ok(sol.fixedSolution.fixedSolution.baselineM.every(Number.isFinite));
  assert.ok(["Fixed", "NotFixed"].includes(sol.fixedSolution.fixedSolution.search.integerStatus));
  assert.deepEqual(sol.droppedSats, []);
  assert.deepEqual(sol.splitCycleSlipArcs, []);
  assert.deepEqual(sol.elevationMaskedSats, []);
});

test("buildRinexRtkArc and solveStaticRinexRtkBaseline solve the real WTZR/WTZZ arc", () => {
  const { sp3, baseObs, roverObs, baseArpM, truthBaselineM } = wettzellRinexInputs();
  const arcOptions = { maxEpochs: 120, includePredictionTime: false };

  const arc = buildRinexRtkArc(sp3, baseObs, roverObs, arcOptions);
  assert.equal(arc.epochs.length, 120);
  assert.equal(arc.skippedEpochCount, 0);
  assert.ok(Object.keys(arc.wavelengthsM).length > 0);
  assert.ok(Object.values(arc.offsetsM).every((value) => value === 0.0));

  const sol = solveStaticRinexRtkBaseline(sp3, baseObs, roverObs, {
    baseM: baseArpM,
    model: realArcModel,
    arcOptions,
    preprocessing: { cycleSlip: "splitArc" },
    opts: {
      float: realArcSolveOptions,
      fixed: realArcSolveOptions,
    },
  });

  assert.deepEqual(sol.references, { G: "G30" });
  assert.equal(sol.splitCycleSlipArcs.length, 4);
  assert.equal(sol.floatSolution.converged, true);
  assert.ok(vectorErrorM(sol.floatSolution.baselineM, truthBaselineM) < 0.08);
  squareCovarianceLength(
    sol.floatSolution.ambiguityCovarianceM,
    Object.keys(sol.floatSolution.ambiguitiesM).length,
  );

  const fixed = sol.fixedSolution.fixedSolution;
  assert.equal(fixed.search.integerStatus, "NotFixed");
  assert.ok(fixed.search.integerRatio < 3.0);
  assert.ok(vectorErrorM(fixed.baselineM, truthBaselineM) < 0.01);
});

test("solveStaticReferenceStationRinex solves the real WTZR/WTZZ static coordinate", () => {
  const { sp3, baseObs, roverObs, baseArpM, truthBaselineM } = wettzellRinexInputs();
  const maxEpochs = 24;
  const sol = solveStaticReferenceStationRinex(sp3, baseObs, roverObs, {
    referencePositionM: baseArpM,
    enableCodeDgnss: false,
    enableCarrierRtk: true,
    withGeodetic: true,
    carrier: {
      arcOptions: { maxEpochs, includePredictionTime: false },
      model: realArcModel,
      preprocessing: { cycleSlip: "splitArc" },
      opts: {
        float: realArcSolveOptions,
        fixed: {
          ...realArcSolveOptions,
          ratioThreshold: 3.0,
          partialAmbiguityResolution: true,
          partialMinAmbiguities: 4,
        },
      },
    },
  });

  assert.equal(sol.mode, "carrierFixed");
  assert.equal(sol.fixStatus, "carrierFixed");
  assert.equal(sol.carrierSolution.integerStatus, "Fixed");
  assert.equal(sol.diagnostics.length, maxEpochs);
  assert.equal(sol.carrierSolution.diagnostics.length, maxEpochs);
  assert.equal(sol.modeReports.length, 1);
  assert.equal(sol.modeReports[0].mode, "carrierFixed");
  assert.equal(sol.modeReports[0].status, "solved");
  assert.equal(sol.modeReports[0].usedEpochs, maxEpochs);
  assert.equal(sol.modeReports[0].skippedEpochs, 0);
  assert.equal(sol.modeReports[0].usedMeasurements, 432);
  assert.ok(vectorErrorM(sol.baselineVectorM, truthBaselineM) < 0.005);
  assert.ok(sol.carrierSolution.integerRatio > 3.0);
  assert.deepEqual(sol.positionM.map(f64Bits), [
    0x414f181daf5efc9bn,
    0x412c701ad3584625n,
    0x4152510859bc4563n,
  ]);
  assert.deepEqual(sol.baselineVectorM.map(f64Bits), [
    0xbfef911d96f93d53n,
    0xbfe4dc7081330552n,
    0x3ff1159562cca4a5n,
  ]);
  assert.deepEqual(
    sol.covariance.positionEcefM2.map((row) => row.map(f64Bits)),
    [
      [0x3f04acaf48e915f5n, 0x3edf5da71e914413n, 0x3ef32e401d0c7caen],
      [0x3edf5da71e914413n, 0x3eec4a84fc5f2788n, 0x3ed882c671817361n],
      [0x3ef32e401d0c7cadn, 0x3ed882c671817361n, 0x3f08fce97d368dedn],
    ],
  );
  assert.deepEqual(sol.geodetic.heightM, 666.1751900247245);
});

test("buildDualFrequencyRinexRtkArc and solveWideLaneFixedRinexRtkBaseline fix the real WTZR/WTZZ arc", () => {
  const { sp3, baseObs, roverObs, baseArpM, truthBaselineM } = wettzellRinexInputs();
  const arcOptions = { maxEpochs: 120, includePredictionTime: false };

  const arc = buildDualFrequencyRinexRtkArc(sp3, baseObs, roverObs, arcOptions);
  assert.equal(arc.epochs.length, 120);
  assert.equal(arc.skippedEpochCount, 0);

  const sol = solveWideLaneFixedRinexRtkBaseline(sp3, baseObs, roverObs, {
    baseM: baseArpM,
    model: realArcModel,
    arcOptions,
    opts: {
      float: realArcSolveOptions,
      fixed: realArcSolveOptions,
    },
  });

  assert.equal(sol.wideLaneFixed, true);
  assert.equal(sol.integerStatus, "Fixed");
  assert.ok(sol.integerRatio > 3.0);
  assert.ok(Object.keys(sol.wideLaneAmbiguitiesCycles).length > 0);
  assert.ok(vectorErrorM(sol.fixedBaselineM, truthBaselineM) < 0.01);
  assert.ok(vectorErrorM(sol.floatBaselineM, truthBaselineM) < 0.1);
  squareCovarianceLength(
    sol.solution.floatSolution.ambiguityCovarianceM,
    Object.keys(sol.solution.floatSolution.ambiguitiesM).length,
  );
});

test("fixWideLaneRtkArc fixes wide-lane ambiguities over a dual-frequency arc", () => {
  const sol = fixWideLaneRtkArc(dualArcEpochs, wideLaneConfig);

  assert.ok(sol.references.G);
  assert.equal(typeof sol.geometryQuality.tier, "string");
  assert.equal(typeof sol.geometryQuality.covarianceValidated, "boolean");
  assert.equal(sol.epochs.length, dualArcEpochs.length);
  assert.ok(Object.keys(sol.wideLaneCycles).length > 0);
  assert.deepEqual(sol.droppedSats, []);
  assert.deepEqual(sol.splitCycleSlipArcs, []);
});

test("prepareIonosphereFreeRtkArc prepares single-frequency RTK arc inputs", () => {
  const wideLane = fixWideLaneRtkArc(dualArcEpochs, wideLaneConfig);
  const sol = prepareIonosphereFreeRtkArc(dualArcEpochs, wideLane.wideLaneCycles, {
    baseM: dualBaseM,
    applyTroposphere: false,
  });

  assert.deepEqual(sol.references, wideLane.references);
  assert.equal(sol.epochs.length, dualArcEpochs.length);
  assert.ok(Object.keys(sol.wavelengthsM).length > 0);
  assert.ok(Object.keys(sol.offsetsM).length > 0);
  assert.ok(sol.epochs[0].base.every((obs) => Number.isFinite(obs.codeM)));
  assert.ok(sol.epochs[0].rover.every((obs) => Number.isFinite(obs.phaseM)));
});

test("solveRtkArc rejects an empty arc", () => {
  assert.throws(() => solveRtkArc([], config));
});
