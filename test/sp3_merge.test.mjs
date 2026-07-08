// Multi-product SP3 merge through the WASM binding, mirroring
// sidereon-python/tests/test_sp3_merge.py against the same committed SP3
// products. The merge consensus is the engine's; this layer marshals the
// product array and options object and packages the merged product + report.
//
// Note: mergeSp3 consumes the Sp3 handles it is given (Vec<Sp3> by value), so
// the tests load a second handle of any product they still need to query after
// the merge. The Python binding clones inner products instead; the asserted
// values are identical.

import { test } from "node:test";
import assert from "node:assert/strict";

import { loadSp3, mergeSp3 } from "../pkg-node/sidereon.js";
import { fixture } from "./helpers.mjs";

const load = (name) => loadSp3(fixture(`sp3/${name}`));
const encode = new TextEncoder();

function miniSp3(label, records) {
  let sats = records.map(([sat]) => sat).join("");
  sats += "  0".repeat(17 - records.length);
  const lines = [
    `#cP2020  6 25  0  0  0.00000000       1 ORBIT ${label} FIT  TST`,
    "## 2111 432000.00000000   900.00000000 59025 0.0000000000000",
    `+   ${String(records.length).padStart(2)}   ${sats}`,
    "++         0  0  0  0  0  0  0  0  0  0  0  0  0  0  0  0  0",
    "%c G  cc GPS ccc cccc cccc cccc cccc ccccc ccccc ccccc ccccc",
    "%c cc cc ccc ccc cccc cccc cccc cccc ccccc ccccc ccccc ccccc",
    "%f  1.2500000  1.025000000  0.00000000000  0.000000000000000",
    "%f  0.0000000  0.000000000  0.00000000000  0.000000000000000",
    "%i    0    0    0    0      0      0      0      0         0",
    "%i    0    0    0    0      0      0      0      0         0",
    "/* TEST SP3-c FIXTURE",
    "*  2020  6 25  0  0  0.00000000",
  ];
  for (const [sat, positionKm, clockUs] of records) {
    const [x, y, z] = positionKm;
    lines.push(
      `P${sat}${x.toFixed(6).padStart(14)}${y.toFixed(6).padStart(14)}${z.toFixed(6).padStart(14)}${clockUs.toFixed(6).padStart(14)}`,
    );
  }
  lines.push("EOF");
  return loadSp3(encode.encode(`${lines.join("\n")}\n`));
}

test("merge real GBM products spans common coverage and interpolates", () => {
  const full = load("GBM0MGXRAP_20201770000_01D_05M_ORB_120epoch.sp3");
  const trim = load("GBM_BDS_C21_C08_trim.sp3");
  // Capture axes before the handles are consumed by the merge.
  const fullAxis = full.epochsJ2000Seconds();
  const trimAxis = trim.epochsJ2000Seconds();

  const { sp3: merged, report } = mergeSp3([full, trim], {
    positionToleranceM: 1e-6,
    clockMinCommon: 1,
    systems: ["C"],
  });

  const mergedAxis = merged.epochsJ2000Seconds();
  assert.equal(mergedAxis[0], Math.max(fullAxis[0], trimAxis[0]));
  assert.equal(
    mergedAxis[mergedAxis.length - 1],
    Math.min(fullAxis[fullAxis.length - 1], trimAxis[trimAxis.length - 1]),
  );

  const sats = merged.satellites;
  assert.ok(sats.includes("C08"));
  assert.ok(sats.includes("C21"));
  assert.ok(sats.every((s) => s.startsWith("C")));

  assert.equal(report.quarantinedCount, 0);
  assert.equal(report.positionOutlierCount, 0);
  assert.ok(report.singleSourceCount > 0);

  // Single-source C21 cells must match the source product exactly. Load a fresh
  // handle of the full product since the merge consumed the first.
  const fullRef = load("GBM0MGXRAP_20201770000_01D_05M_ORB_120epoch.sp3");
  const query = Float64Array.from([(mergedAxis[20] + mergedAxis[21]) / 2.0]);
  const expected = fullRef.interpolate("C21", query);
  const actual = merged.interpolate("C21", query);
  for (let i = 0; i < 3; i++) {
    assert.ok(Math.abs(actual.positionM[i] - expected.positionM[i]) <= 1e-6);
  }
  assert.ok(Math.abs(actual.clockS[0] - expected.clockS[0]) <= 1e-12);
});

test("degenerate single source reports single-source cells", () => {
  const sp3 = load("degenerate_coincident_5sat.sp3");
  const ref = load("degenerate_coincident_5sat.sp3");
  const epochCount = sp3.epochCount;
  const satellites = sp3.satellites;

  const { sp3: merged, report } = mergeSp3([sp3], { minAgree: 1, clockMinCommon: 1 });

  assert.equal(merged.epochCount, epochCount);
  assert.deepEqual(merged.satellites, satellites);
  assert.equal(report.singleSourceCount, epochCount * satellites.length);
  assert.equal(report.quarantinedCount, 0);

  const firstFlag = report.singleSource[0];
  assert.ok(satellites.includes(firstFlag.satellite));
  assert.deepEqual(Array.from(firstFlag.sources), [0]);
  assert.ok(Number.isFinite(firstFlag.epochJ2000Seconds));

  // Agreement metrics (B2): one per accepted cell. With a single source every
  // consensus has one member and zero spread; clock fields are present here
  // because the degenerate product carries clocks.
  assert.equal(report.agreementCount, epochCount * satellites.length);
  assert.equal(report.agreement.length, report.agreementCount);
  const a = report.agreement[0];
  assert.ok(satellites.includes(a.satellite));
  assert.ok(Number.isFinite(a.epochJ2000Seconds));
  assert.equal(a.positionMembers, 1);
  assert.equal(a.positionRmsM, 0);
  assert.equal(a.positionMaxM, 0);
  assert.ok(
    report.agreement.every(
      (m) => m.positionMembers >= 1 && m.positionRmsM <= m.positionMaxM + 1e-12,
    ),
  );

  const axis = ref.epochsJ2000Seconds();
  const query = Float64Array.from([(axis[0] + axis[1]) / 2.0]);
  const expected = ref.interpolate("G01", query);
  const actual = merged.interpolate("G01", query);
  for (let i = 0; i < 3; i++) {
    assert.ok(Math.abs(actual.positionM[i] - expected.positionM[i]) <= 1e-9);
  }
  assert.ok(Math.abs(actual.clockS[0] - expected.clockS[0]) <= 1e-15);
});

test("merge rejects empty sources and mismatched frames", () => {
  assert.throws(() => mergeSp3([], undefined), TypeError);

  const cod = load("COD0MGXFIN_20201770000_01D_05M_ORB.SP3");
  const gbm = load("GBM0MGXRAP_20201770000_01D_05M_ORB_120epoch.sp3");
  assert.throws(() => mergeSp3([cod, gbm], undefined), Error);
});

test("merge accepts asserted frame equivalence and reports it", () => {
  const a = miniSp3("IGS14", [["G01", [15000.0, -20000.0, 5000.0], 100.0]]);
  const b = miniSp3("ITRF2", [["G02", [16000.0, -21000.0, 6000.0], 200.0]]);

  const { sp3: merged, report } = mergeSp3([a, b], {
    assertedFrameLabelSets: [["IGS14", "ITRF2"]],
  });

  assert.deepEqual(new Set(merged.satellites), new Set(["G01", "G02"]));
  assert.equal(report.frameReconciliationCount, 1);
  const reconciliation = report.frameReconciliations[0];
  assert.equal(reconciliation.method, "asserted_equivalence");
  assert.equal(reconciliation.sourceIndex, 1);
  assert.equal(reconciliation.sourceLabel, "ITRF2");
  assert.equal(reconciliation.targetLabel, "IGS14");
  assert.deepEqual(reconciliation.assertedLabelSet, ["IGS14", "ITRF2"]);
  assert.deepEqual(Array.from(reconciliation.translationMm), []);
  assert.equal(reconciliation.recordsAffected, 1);
});

test("merge applies Helmert frame reconciliation and reports table values", () => {
  const a = miniSp3("IGS14", [["G01", [14000.0, -19000.0, 4000.0], 100.0]]);
  const b = miniSp3("IGS20", [["G02", [15000.0, -20000.0, 5000.0], 200.0]]);

  const { sp3: merged, report } = mergeSp3([a, b], { minAgree: 1, helmert: true });

  const got = merged.state("G02", 0).positionM;
  const expected = [14_999_999.992_3, -19_999_999.993_048_087, 5_000_000.000_396_175];
  for (let i = 0; i < 3; i++) {
    assert.ok(Math.abs(got[i] - expected[i]) <= 2e-9);
  }
  const reconciliation = report.frameReconciliations[0];
  assert.equal(reconciliation.method, "helmert");
  assert.equal(reconciliation.sourceFrame, "ITRF2020");
  assert.equal(reconciliation.targetFrame, "ITRF2014");
  assert.equal(reconciliation.catalogSourceFrame, "ITRF2020");
  assert.equal(reconciliation.catalogTargetFrame, "ITRF2014");
  assert.equal(reconciliation.catalogInverse, false);
  assert.deepEqual(Array.from(reconciliation.translationMm), [-1.4, -0.9, 1.4]);
  assert.equal(reconciliation.scalePpb, -0.42);
  assert.deepEqual(Array.from(reconciliation.translationMmPerYear), [0.0, -0.1, 0.2]);
  assert.ok(reconciliation.provenance.includes("ITRF2020 to past ITRFs"));
});

test("merge inverse Helmert reports catalog direction", () => {
  const a = miniSp3("IGS20", [["G01", [14000.0, -19000.0, 4000.0], 100.0]]);
  const b = miniSp3("IGS14", [["G02", [15000.0, -20000.0, 5000.0], 200.0]]);

  const { report } = mergeSp3([a, b], { minAgree: 1, helmert: true });

  const reconciliation = report.frameReconciliations[0];
  assert.equal(reconciliation.method, "helmert");
  assert.equal(reconciliation.sourceFrame, "ITRF2014");
  assert.equal(reconciliation.targetFrame, "ITRF2020");
  assert.equal(reconciliation.catalogSourceFrame, "ITRF2020");
  assert.equal(reconciliation.catalogTargetFrame, "ITRF2014");
  assert.equal(reconciliation.catalogInverse, true);
  assert.deepEqual(Array.from(reconciliation.translationMm), [-1.4, -0.9, 1.4]);
});

test("merge Helmert identity reconciliation is bit equal", () => {
  const a = miniSp3("IGS20", [["G01", [14000.0, -19000.0, 4000.0], 100.0]]);
  const b = miniSp3("IGc20", [["G02", [15000.125, -20000.5, 5000.25], 200.0]]);
  const original = Array.from(b.state("G02", 0).positionM);

  const { sp3: merged, report } = mergeSp3([a, b], { minAgree: 1, helmert: true });

  assert.deepEqual(Array.from(merged.state("G02", 0).positionM), original);
  const reconciliation = report.frameReconciliations[0];
  assert.equal(reconciliation.identity, true);
  assert.deepEqual(Array.from(reconciliation.translationMm), []);
});

test("merge accepts string selectors and validates the system filter", () => {
  // String combine + name/letter system aliases are accepted and produce a
  // valid merge (mirrors the Python Sp3MergeOptions selector parsing).
  const sp3 = load("degenerate_coincident_5sat.sp3");
  const { sp3: merged } = mergeSp3([sp3], {
    combine: "precedence",
    minAgree: 1,
    clockMinCommon: 1,
    systems: ["GPS"],
  });
  assert.ok(merged.satellites.every((s) => s.startsWith("G")));

  const bad = load("degenerate_coincident_5sat.sp3");
  assert.throws(() => mergeSp3([bad], { systems: ["X"] }), TypeError);

  const badLabels = load("degenerate_coincident_5sat.sp3");
  assert.throws(() => mergeSp3([badLabels], { assertedFrameLabelSets: [["IGS14"]] }), TypeError);
});
