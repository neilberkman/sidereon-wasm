// Standalone range RAIM / fault-detection-and-exclusion over a linearized
// measurement set delegates to sidereon_core::quality::raim_fde_design. The
// binding marshals the design rows and options and returns the protected state
// correction, covariance, global chi-square test, exclusion list, and
// per-measurement diagnostics.

import { test } from "node:test";
import assert from "node:assert/strict";

import { raimFdeDesign } from "../pkg-node/sidereon.js";

// A consistent 2-state linearization about dx = [1, -2]: every residual equals
// designRow . dx, so the protected solve recovers dx with no fault.
const CLEAN_ROWS = [
  { id: "m1", designRow: [1, 0], residualM: 1, weight: 1 },
  { id: "m2", designRow: [0, 1], residualM: -2, weight: 1 },
  { id: "m3", designRow: [1, 1], residualM: -1, weight: 1 },
  { id: "m4", designRow: [1, -1], residualM: 3, weight: 1 },
  { id: "m5", designRow: [2, 1], residualM: 0, weight: 1 },
];

test("raimFdeDesign recovers the state with no fault on a consistent set", () => {
  const r = raimFdeDesign(CLEAN_ROWS, undefined);
  assert.equal(r.globalTest.faultDetected, false);
  assert.equal(r.excluded.length, 0);
  assert.equal(r.iterations, 0);
  assert.equal(r.diagnostics.length, 5);
  assert.ok(Math.abs(r.stateCorrection[0] - 1) <= 1e-9);
  assert.ok(Math.abs(r.stateCorrection[1] + 2) <= 1e-9);
  // 2-by-2 protected covariance is symmetric.
  assert.equal(r.stateCovariance.length, 2);
  assert.ok(Math.abs(r.stateCovariance[0][1] - r.stateCovariance[1][0]) <= 1e-12);
});

test("raimFdeDesign detects and excludes a gross outlier", () => {
  const rows = CLEAN_ROWS.map((row) => ({ ...row }));
  rows[4] = { ...rows[4], residualM: 40 };
  const r = raimFdeDesign(rows, { pFa: 1e-3 });
  assert.ok(r.excluded.includes("m5"));
  assert.ok(r.iterations >= 1);
  // After excluding the outlier the surviving set is consistent again.
  assert.equal(r.globalTest.faultDetected, false);
  // The excluded row is flagged in its diagnostic, in input order.
  const m5 = r.diagnostics.find((d) => d.id === "m5");
  assert.equal(m5.excluded, true);
});

test("raimFdeDesign honours a maxExclusions budget of zero", () => {
  const rows = CLEAN_ROWS.map((row) => ({ ...row }));
  rows[4] = { ...rows[4], residualM: 40 };
  const r = raimFdeDesign(rows, { maxExclusions: 0 });
  // No exclusion allowed, so the fault is left unresolved.
  assert.equal(r.excluded.length, 0);
  assert.equal(r.globalTest.faultDetected, true);
});

test("raimFdeDesign rejects an empty measurement set", () => {
  assert.throws(() => raimFdeDesign([], undefined));
});
