import { test } from "node:test";
import assert from "node:assert/strict";

import { loadSp3, mergeSp3 } from "../pkg-node/sidereon.js";

const encode = new TextEncoder();
const intervalS = 300;
const dayStartGpsS = 432_000;

function seriesSp3(startSecondOfDay, count, globalIndex, offsetKm) {
  const startHour = Math.floor(startSecondOfDay / 3_600);
  const startMinute = Math.floor((startSecondOfDay % 3_600) / 60);
  const lines = [
    `#cP2020  6 25 ${String(startHour).padStart(2)} ${String(startMinute).padStart(2)}  0.00000000${String(count).padStart(8)} ORBIT IGS20 FIT  TST`,
    `## 2111 ${(dayStartGpsS + startSecondOfDay).toFixed(8).padStart(15)}   300.00000000 59025 ${(startSecondOfDay / 86_400).toFixed(13)}`,
    "+    1   G01  0  0  0  0  0  0  0  0  0  0  0  0  0  0  0  0",
    "++         0  0  0  0  0  0  0  0  0  0  0  0  0  0  0  0  0",
    "%c G  cc GPS ccc cccc cccc cccc cccc ccccc ccccc ccccc ccccc",
    "%c cc cc ccc ccc cccc cccc cccc cccc ccccc ccccc ccccc ccccc",
    "%f  1.2500000  1.025000000  0.00000000000  0.000000000000000",
    "%f  0.0000000  0.000000000  0.00000000000  0.000000000000000",
    "%i    0    0    0    0      0      0      0      0         0",
    "%i    0    0    0    0      0      0      0      0         0",
    "/* WINDOW CONTINUITY MAPPING FIXTURE",
  ];

  for (let index = 0; index < count; index++) {
    const secondOfDay = startSecondOfDay + index * intervalS;
    const hour = Math.floor(secondOfDay / 3_600);
    const minute = Math.floor((secondOfDay % 3_600) / 60);
    const second = secondOfDay % 60;
    const xKm = 20_000 + globalIndex + index + offsetKm;
    lines.push(
      `*  2020  6 25 ${String(hour).padStart(2)} ${String(minute).padStart(2)} ${second.toFixed(8).padStart(11)}`,
    );
    lines.push(
      `PG01${xKm.toFixed(6).padStart(14)}${(-12_000).toFixed(6).padStart(14)}${(8_000).toFixed(6).padStart(14)}${(100).toFixed(6).padStart(14)}`,
    );
  }
  lines.push("EOF");
  return loadSp3(encode.encode(`${lines.join("\n")}\n`));
}

function seamProducts() {
  return [seriesSp3(9 * 3_600 + 30 * 60, 30, 0, 0), seriesSp3(12 * 3_600, 30, 30, 3_000)];
}

test("window continuity maps inside-day, straddling, and stencil-boundary cases", () => {
  const [first, second] = seamProducts();
  const seam = first.epochsJ2000Seconds().at(-1);
  const { sp3: merged, report } = mergeSp3([first, second], {
    combine: "precedence",
    minAgree: 1,
    verifyContinuity: { orbitClass: "meo_gnss", residualToleranceM: null },
  });

  assert.deepEqual(merged.stencilExtent(), { beforeS: 3_300, afterS: 3_300 });

  const insideOneDay = report.continuityVerdict(seam - 7_200, seam - 3_600);
  assert.equal(insideOneDay.decision, "accept");
  assert.equal(insideOneDay.accepted, true);
  assert.deepEqual(insideOneDay.influencingDefects, []);
  assert.equal(insideOneDay.allDefects.length, 1);
  assert.equal(insideOneDay.allSplices.length, 1);

  const straddling = report.continuityVerdict(seam - 600, seam + 600);
  assert.equal(straddling.decision, "refuse");
  assert.equal(straddling.accepted, false);
  assert.equal(straddling.influencingDefects.length, 1);
  assert.equal(straddling.influencingSplices.length, 1);
  assert.equal(straddling.influencingDefects[0].kind, "speed_bound");
  assert.deepEqual(straddling.influencingSplices[0].fromSources, [0]);
  assert.deepEqual(straddling.influencingSplices[0].toSources, [1]);

  // A merge verdict reads the nodes the window's interpolations select, as
  // RTKLIB pephpos selects them: the last node strictly before the query and
  // five either side of it. A window ending on the node five before the seam
  // reaches the node before the seam; any later end reaches the seam record,
  // a pair end of the violation. The stencil extent no longer decides a merge
  // verdict, so a window ending at it is accepted.
  const atStencilExtent = report.continuityVerdict(seam - 7_200, seam - 3_300);
  assert.equal(atStencilExtent.decision, "accept");

  const missesSeam = report.continuityVerdict(seam - 7_200, seam - 5 * intervalS);
  assert.equal(missesSeam.decision, "accept");

  const reachesSeam = report.continuityVerdict(seam - 7_200, seam - 5 * intervalS + 0.001);
  assert.equal(reachesSeam.decision, "refuse");

  // The nodes those verdicts read: the window ending five nodes before the
  // seam reaches the node before it, and any later end reaches the seam
  // record. They are the merged product's own node selection.
  const missNodes = report.continuitySelectedNodes("G01", seam - 7_200, seam - 5 * intervalS);
  assert.equal(missNodes.at(-1), seam - intervalS);
  const reachNodes = report.continuitySelectedNodes(
    "G01",
    seam - 7_200,
    seam - 5 * intervalS + 0.001,
  );
  assert.equal(reachNodes.at(-1), seam);
  assert.deepEqual(
    reachNodes,
    merged.selectedNodes("G01", seam - 7_200, seam - 5 * intervalS + 0.001),
  );
  assert.ok(reachNodes.every((node, i) => i === 0 || reachNodes[i - 1] < node));

  const direct = merged.continuityVerdict(seam - 600, seam + 600, "meo_gnss", null);
  assert.equal(direct.decision, "refuse");
  assert.equal(direct.influencingDefects.length, 1);
  assert.deepEqual(direct.influencingSplices, []);

  const defaulted = merged.continuityVerdict(seam - 600, seam + 600);
  assert.equal(defaulted.decision, "refuse");
});

test("merge continuity verdict preserves not-requested as null", () => {
  const [first, second] = seamProducts();
  const { sp3: merged, report } = mergeSp3([first, second], {
    combine: "precedence",
    minAgree: 1,
  });
  const start = merged.epochsJ2000Seconds()[0];
  assert.equal(report.continuityVerdict(start, start + intervalS), null);
  assert.equal(report.continuity, null);
  assert.equal(report.continuitySelectedNodes("G01", start, start + intervalS), undefined);
});

test("the merge continuity report names each finding's cells, contributors and measurements", () => {
  const [first, second] = seamProducts();
  const seam = first.epochsJ2000Seconds().at(-1);
  const { report } = mergeSp3([first, second], {
    combine: "precedence",
    minAgree: 1,
    verifyContinuity: { orbitClass: "meo_gnss", residualToleranceM: null },
  });
  const continuity = report.continuity;

  // With the residual check off, the speed gate finds the one seam pair: X
  // steps from 20,029 km to 23,030 km in 300 s, and Y and Z do not move.
  assert.equal(continuity.attested, false);
  assert.equal(continuity.defects.length, 1);
  assert.equal(continuity.violations.length, 1);
  assert.deepEqual(continuity.splices, continuity.violations);

  const [splice] = continuity.splices;
  const defect = splice.defect;
  assert.equal(defect.kind, "speed_bound");
  assert.equal(defect.satellite, "G01");
  assert.equal(defect.fromJ2000S, seam);
  assert.equal(defect.toJ2000S, seam + intervalS);
  assert.equal(defect.intervalS, intervalS);
  assert.equal(defect.displacementM, 3_001_000);
  assert.equal(defect.impliedSpeedMS, 3_001_000 / intervalS);
  assert.equal(defect.magnitude, defect.impliedSpeedMS);
  assert.equal(defect.bound, defect.boundMS);
  assert.ok(defect.impliedSpeedMS > defect.boundMS);
  assert.equal(defect.nodeEpochsJ2000S, undefined);
  assert.deepEqual(continuity.defects[0], defect);

  assert.deepEqual(splice.fromSources, [0]);
  assert.deepEqual(splice.toSources, [1]);
  assert.deepEqual(splice.sources, [0, 1]);
  assert.equal(splice.crossesContributors, true);
  assert.deepEqual(splice.cells, [
    { epochJ2000S: seam, role: "pair_end", selection: { kind: "single_source", source: 0 } },
    {
      epochJ2000S: seam + intervalS,
      role: "pair_end",
      selection: { kind: "single_source", source: 1 },
    },
  ]);
});
