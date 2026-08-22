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

  assert.deepEqual(merged.stencilExtent(), { beforeS: 1_500, afterS: 1_500 });

  const insideOneDay = report.continuityVerdict(merged, seam - 7_200, seam - 3_600);
  assert.equal(insideOneDay.decision, "accept");
  assert.equal(insideOneDay.accepted, true);
  assert.deepEqual(insideOneDay.influencingDefects, []);
  assert.equal(insideOneDay.allDefects.length, 1);
  assert.equal(insideOneDay.allSplices.length, 1);

  const straddling = report.continuityVerdict(merged, seam - 600, seam + 600);
  assert.equal(straddling.decision, "refuse");
  assert.equal(straddling.accepted, false);
  assert.equal(straddling.influencingDefects.length, 1);
  assert.equal(straddling.influencingSplices.length, 1);
  assert.equal(straddling.influencingDefects[0].kind, "speed_bound");
  assert.deepEqual(straddling.influencingSplices[0].fromSources, [0]);
  assert.deepEqual(straddling.influencingSplices[0].toSources, [1]);

  const reachesSeam = report.continuityVerdict(merged, seam - 7_200, seam - 1_500);
  assert.equal(reachesSeam.decision, "refuse");

  const missesSeam = report.continuityVerdict(merged, seam - 7_200, seam - 1_500.001);
  assert.equal(missesSeam.decision, "accept");

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
  assert.equal(report.continuityVerdict(merged, start, start + intervalS), null);
});
