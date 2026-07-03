// RINEX lint, repair, and observation QC surfaces over core fixtures.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  lintRinexNav,
  lintRinexObs,
  observationQc,
  parseRinexObs,
  repairRinexNav,
  repairRinexObs,
} from "../pkg-node/sidereon.js";

import { fixture } from "./helpers.mjs";

const encoder = new TextEncoder();

test("lintRinexObs reports the core fatal for unsupported RINEX 2 OBS", () => {
  const report = lintRinexObs(fixture("obs/algo0010_2015001_v1_trim.rnx"));

  assert.equal(report.clean, false);
  assert.equal(report.decodedFromCrinex, false);
  assert.equal(report.findingCount, 1);
  assert.deepEqual(report.counts, { fatal: 1, error: 0, warning: 0, info: 0 });
  assert.equal(report.findings[0].code, "OBS-H01");
  assert.equal(report.findings[0].severity, "fatal");
  assert.equal(report.findings[0].repairable, false);
  assert.match(report.findings[0].detail, /requires major version 3 or 4, got 2\.11/);
});

test("repairRinexObs fixes header-derived fields and leaves a clean report", () => {
  const report = lintRinexObs(fixture("obs/ESBC00DNK_R_20201770000_01D_30S_MO_trim.rnx"));
  assert.equal(report.findingCount, 1);
  assert.equal(report.findings[0].code, "OBS-H08");

  const repaired = repairRinexObs(fixture("obs/ESBC00DNK_R_20201770000_01D_30S_MO_trim.rnx"), {
    setInterval: true,
    setObsCounts: true,
    sortRecords: true,
    dropEmptyRecords: true,
  });

  assert.deepEqual(repaired.actions, [
    { id: "A4", message: "recomputed TIME OF LAST OBS" },
    { id: "A5", message: "recomputed observation count headers" },
  ]);
  assert.equal(repaired.decodedFromCrinex, false);
  assert.equal(repaired.repairedText.length, 31658);
  assert.equal(repaired.remaining.clean, true);
  assert.equal(repaired.remaining.findingCount, 0);
  assert.equal(repaired.repaired.epochCount, 2);
});

test("observationQc reports the core summary and selected signal statistics", () => {
  const repaired = repairRinexObs(fixture("obs/ESBC00DNK_R_20201770000_01D_30S_MO_trim.rnx"), {
    setInterval: true,
    setObsCounts: true,
  });
  const obs = parseRinexObs(encoder.encode(repaired.repairedText));
  const qc = observationQc(obs, {
    minSatellitesPerEpoch: 4,
    expectedIntervalS: 30,
    checkCycleSlips: true,
    geometryFreeThresholdM: 0.05,
    melbourneWubbenaThresholdCycles: 4,
    gapThresholdS: 300,
  });

  assert.equal(qc.totalEpochRecords, 2);
  assert.equal(qc.observationEpochs, 2);
  assert.equal(qc.eventRecords, 0);
  assert.equal(qc.intervalS, 30);
  assert.equal(qc.intervalSource, "header");
  assert.equal(qc.missingEpochs, 0);
  assert.equal(qc.satellites.length, 43);
  assert.equal(qc.satelliteSignals.length, 565);
  assert.equal(qc.dataGaps.length, 0);
  assert.equal(qc.notes.length, 0);

  assert.deepEqual(
    qc.satellites.find((s) => s.satellite === "G08"),
    { satellite: "G08", epochsWithObservations: 2, valueObservations: 36 },
  );
  assert.deepEqual(
    qc.satelliteSignals.find((s) => s.satellite === "G08" && s.code === "S1C"),
    {
      satellite: "G08",
      code: "S1C",
      valueObservations: 2,
      ssi: { counts: [2, 0, 0, 0, 0, 0, 0, 0, 0, 0] },
      snr: { n: 2, mean: 34.875, min: 33.25, max: 36.5, std: 2.2980970388562794 },
    },
  );
});

test("lintRinexNav and repairRinexNav expose core NAV diagnostics", () => {
  const lint = lintRinexNav(fixture("nav/BRD400DLR_S_20261800000_01H_MN_trim.rnx"));
  assert.equal(lint.clean, false);
  assert.equal(lint.findingCount, 7);
  assert.deepEqual(lint.counts, { fatal: 0, error: 4, warning: 0, info: 3 });
  assert.deepEqual(
    lint.findings.map((f) => f.code),
    ["NAV-B01", "NAV-B01", "NAV-B01", "NAV-B01", "NAV-B06", "NAV-B06", "NAV-B06"],
  );

  const repaired = repairRinexNav(fixture("nav/BRD400DLR_S_20261800000_01H_MN_trim.rnx"), {
    dropUnsupported: true,
    sortRecords: true,
  });
  assert.deepEqual(
    repaired.actions.map((a) => a.id),
    ["A12", "NAV-B06", "NAV-B06", "NAV-B06"],
  );
  assert.equal(repaired.records.length, 6);
  assert.equal(repaired.leapSeconds, 18);
  assert.equal(repaired.repairedText.length, 4545);
  assert.equal(repaired.remaining.clean, true);
  assert.equal(repaired.remaining.findingCount, 1);
  assert.equal(repaired.remaining.findings[0].code, "NAV-B05");
});
