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
});
