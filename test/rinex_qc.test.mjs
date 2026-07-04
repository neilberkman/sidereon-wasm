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

function assertClose(actual, expected, tolerance = 1e-12) {
  assert.ok(
    Math.abs(actual - expected) <= tolerance,
    `expected ${actual} to be within ${tolerance} of ${expected}`,
  );
}

test("lintRinexObs reports the core diagnostics for RINEX 2 OBS", () => {
  const report = lintRinexObs(fixture("obs/algo0010_2015001_v1_trim.rnx"));

  assert.equal(report.clean, false);
  assert.equal(report.decodedFromCrinex, false);
  assert.equal(report.findingCount, 9);
  assert.deepEqual(report.counts, { fatal: 0, error: 8, warning: 0, info: 1 });
  assert.deepEqual(
    report.findings.map((f) => f.code),
    [
      "OBS-H90",
      "OBS-H12",
      "OBS-H12",
      "OBS-H12",
      "OBS-H12",
      "OBS-H12",
      "OBS-H12",
      "OBS-H12",
      "OBS-H12",
    ],
  );
  assert.deepEqual(
    report.findings.slice(1).map((f) => f.at.satellite),
    ["R05", "R06", "R07", "R09", "R15", "R16", "R17", "R24"],
  );
  assert.equal(report.findings[0].severity, "info");
  assert.equal(report.findings[0].repairable, false);
  assert.match(report.findings[0].detail, /WAVELENGTH FACT L1\/2/);
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
    gapFactor: 1.5,
    clockJumpThresholdS: 0.0005,
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
  assert.deepEqual(qc.clockJumps, []);
  assert.equal(qc.cycleSlips.observations, 68);
  assert.equal(qc.cycleSlips.totalSlips, 0);
  assert.equal(qc.cycleSlips.observationsPerSlip, undefined);
  assert.deepEqual(
    qc.cycleSlips.bySystem.map((s) => ({
      system: s.system,
      observations: s.observations,
      slips: s.slips,
      observationsPerSlip: s.observationsPerSlip,
    })),
    [
      { system: "GPS", observations: 22, slips: 0, observationsPerSlip: undefined },
      { system: "GLONASS", observations: 14, slips: 0, observationsPerSlip: undefined },
      { system: "Galileo", observations: 16, slips: 0, observationsPerSlip: undefined },
      { system: "BeiDou", observations: 16, slips: 0, observationsPerSlip: undefined },
    ],
  );
  assert.equal(qc.multipath.satellites.length, 34);
  assert.equal(qc.multipath.systems.length, 4);

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

  const g08Multipath = qc.multipath.satellites.find((s) => s.satellite === "G08");
  assert.equal(g08Multipath.mp1.n, 2);
  assertClose(g08Multipath.mp1.rmsM, 0.29432116710497774);
  assert.equal(g08Multipath.mp2.n, 2);
  assertClose(g08Multipath.mp2.rmsM, 0.0019879508256253303);

  const gpsMultipath = qc.multipath.systems.find((s) => s.system === "GPS");
  assert.equal(gpsMultipath.mp1.n, 22);
  assertClose(gpsMultipath.mp1.rmsM, 0.1069865674510667);
  assert.equal(gpsMultipath.mp2.n, 22);
  assertClose(gpsMultipath.mp2.rmsM, 0.059282645631154554);

  const beidouMultipath = qc.multipath.systems.find((s) => s.system === "BeiDou");
  assert.equal(beidouMultipath.mp1.n, 16);
  assertClose(beidouMultipath.mp1.rmsM, 0.1387371512830529);
  assert.equal(beidouMultipath.mp2.n, 16);
  assertClose(beidouMultipath.mp2.rmsM, 0.07335406047173548);

  const rendered = qc.renderText();
  assert.match(rendered, /^G {3}GPS/m);
  assert.match(rendered, /^R {3}GLONASS/m);
  assert.match(rendered, /^E {3}Galileo/m);
  assert.match(rendered, /^C {3}BeiDou/m);
  assert.match(rendered, /^S {3}SBAS/m);
  assert.match(qc.renderHtml(), /RINEX Observation QC/);

  const json = JSON.parse(qc.toJson());
  assert.equal(json.cycleSlips.totalSlips, 0);
  assertClose(json.multipath.systems.find((s) => s.system === "GPS").mp1.rmsM, 0.1069865674510667);
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
