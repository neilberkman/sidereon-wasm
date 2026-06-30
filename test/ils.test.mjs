// Integer-ambiguity resolution kernels through the WASM binding: the standalone
// LAMBDA search (`lambdaIlsSearch`) and the bounded lattice search
// (`boundedIlsSearch`). The search math is `sidereon_core::ils`; these assert
// the marshalling and that the binding reproduces the core's RTKLIB goldens.

import { test } from "node:test";
import assert from "node:assert/strict";

import { lambdaIlsSearch, boundedIlsSearch } from "../pkg-node/sidereon.js";

// Build an n x n nested array from a flat row-major list.
const full = (flat, n) => Array.from({ length: n }, (_, i) => flat.slice(i * n, i * n + n));

// RTKLIB lambda() utest1 (sidereon-core ils.rs lambda_matches_rtklib_utest1).
const UTEST1_A = [1585184.171, -6716599.43, 3915742.905, 7627233.455, 9565990.879, 989457273.2];
const UTEST1_Q = full(
  [
    0.227134, 0.112202, 0.112202, 0.112202, 0.112202, 0.103473, 0.112202, 0.227134, 0.112202,
    0.112202, 0.112202, 0.103473, 0.112202, 0.112202, 0.227134, 0.112202, 0.112202, 0.103473,
    0.112202, 0.112202, 0.112202, 0.227134, 0.112202, 0.103473, 0.112202, 0.112202, 0.112202,
    0.112202, 0.227134, 0.103473, 0.103473, 0.103473, 0.103473, 0.103473, 0.103473, 0.434339,
  ],
  6,
);
const UTEST1_FIXED = [1585184, -6716599, 3915743, 7627234, 9565991, 989457273];

test("lambdaIlsSearch reproduces the RTKLIB utest1 golden", () => {
  const r = lambdaIlsSearch(Float64Array.from(UTEST1_A), UTEST1_Q, 3.0);
  assert.deepEqual(r.fixed, UTEST1_FIXED);
  // Integer cycles cross as plain JS numbers, not BigInt.
  assert.equal(typeof r.fixed[0], "number");
  assert.ok(Math.abs(r.bestScore - 3.5079844392) < 1e-4);
  assert.ok(Math.abs(r.secondBestScore - 3.70845619249) < 1e-4);
  assert.equal(typeof r.fixedStatus, "boolean");
  assert.ok(r.ratio > 1.0);
  // Diagnostic covariance is the symmetrized n x n input.
  assert.equal(r.covariance.length, 6);
  assert.equal(r.covarianceInverse.length, 6);
});

test("boundedIlsSearch agrees with LAMBDA on a near-diagonal case", () => {
  const r = boundedIlsSearch(Float64Array.from(UTEST1_A), UTEST1_Q, 2, 1_000_000, 3.0);
  assert.deepEqual(r.fixed, UTEST1_FIXED);
  assert.ok(Number.isFinite(r.bestScore));
});

test("a near-integer, well-conditioned case fixes to the rounded vector", () => {
  const a = Float64Array.from([5.08, 3.02, -2.04]);
  const q = [
    [0.01, 0.0, 0.0],
    [0.0, 0.01, 0.0],
    [0.0, 0.0, 0.01],
  ];
  const r = lambdaIlsSearch(a, q, 3.0);
  assert.deepEqual(r.fixed, [5, 3, -2]);
  assert.equal(r.fixedStatus, true);
  assert.ok(r.ratio > 3.0);
});

test("a singular covariance throws an Error", () => {
  const a = Float64Array.from([1.1, 2.2]);
  const zero = [
    [0.0, 0.0],
    [0.0, 0.0],
  ];
  assert.throws(
    () => lambdaIlsSearch(a, zero, 3.0),
    (e) => !(e instanceof TypeError),
  );
});

test("a non-square covariance throws a TypeError", () => {
  const a = Float64Array.from([1.0, 2.0, 3.0]);
  const wrong = [
    [0.01, 0.0],
    [0.0, 0.01],
  ];
  assert.throws(() => lambdaIlsSearch(a, wrong, 3.0), TypeError);
  assert.throws(() => boundedIlsSearch(a, wrong, 1, 1000, 3.0), TypeError);
});

test("boundedIlsSearch rejects a negative radius / candidate limit with RangeError", () => {
  const a = Float64Array.from([1.1, 2.2]);
  const q = [
    [0.01, 0.0],
    [0.0, 0.01],
  ];
  assert.throws(() => boundedIlsSearch(a, q, -1, 1000, 3.0), RangeError);
  assert.throws(() => boundedIlsSearch(a, q, 1, -5, 3.0), RangeError);
  assert.throws(() => boundedIlsSearch(a, q, 1.5, 1000, 3.0), RangeError);
});
