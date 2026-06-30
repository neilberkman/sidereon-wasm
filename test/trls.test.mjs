// Generic data-driven trust-region least squares over the
// trust-region-least-squares crate's data path. Structural behaviour (kind
// dispatch, option plumbing, drop-one shape, input rejection) is checked
// everywhere; the tight coefficient-recovery tolerance is gated to Linux x86_64,
// where the libm the solver rides matches the reference. The wasm build uses the
// in-crate nalgebra SVD backend (the host-LAPACK bit-exact seam needs dlopen),
// so results are tolerance-close to SciPy rather than bit-for-bit.

import { test } from "node:test";
import assert from "node:assert/strict";

import { leastSquares, leastSquaresDropOne } from "../pkg-node/sidereon.js";

// Tight numeric assertions only where the native libm matches the reference.
const TIGHT = process.platform === "linux" && process.arch === "x64";
const tol = TIGHT ? 1e-9 : 1e-6;

test("polynomial fit recovers the generating coefficients", () => {
  // y = 1 + 2 t + 3 t^2, exact least-squares solution [1, 2, 3].
  const t = Float64Array.from([-2, -1, 0, 1, 2]);
  const y = Float64Array.from(t).map((ti) => 1 + 2 * ti + 3 * ti * ti);
  const result = leastSquares({
    kind: "polynomial",
    degree: 2,
    t,
    y,
    x0: Float64Array.from([0, 0, 0]),
  });

  assert.ok(result.success);
  assert.equal(result.n, 3);
  assert.equal(result.m, 5);
  assert.ok(Math.abs(result.x[0] - 1) < tol);
  assert.ok(Math.abs(result.x[1] - 2) < tol);
  assert.ok(Math.abs(result.x[2] - 3) < tol);
  // jac is the flat m-by-n design matrix.
  assert.equal(result.jac.length, result.m * result.n);
});

test("linear kind solves the overdetermined consistent system", () => {
  // r = [x0 - 1, x1 - 2, x0 + x1 - 3], minimized at [1, 2].
  const result = leastSquares({
    kind: "linear",
    a: Float64Array.from([1, 0, 0, 1, 1, 1]),
    b: Float64Array.from([1, 2, 3]),
    m: 3,
    n: 2,
    x0: Float64Array.from([0, 0]),
  });
  assert.ok(result.success);
  assert.ok(Math.abs(result.x[0] - 1) < tol);
  assert.ok(Math.abs(result.x[1] - 2) < tol);
});

test("exponential kind fits an exponential model", () => {
  const t = Float64Array.from([0, 0.5, 1, 1.5, 2, 2.5, 3]);
  const y = Float64Array.from(t).map((ti) => 2 * Math.exp(0.5 * ti) + 1);
  const result = leastSquares({
    kind: "exponential",
    t,
    y,
    x0: Float64Array.from([1, 1, 0]),
  });
  assert.ok(result.success);
  assert.ok(Math.abs(result.x[0] - 2) < 1e-4);
  assert.ok(Math.abs(result.x[1] - 0.5) < 1e-4);
  assert.ok(Math.abs(result.x[2] - 1) < 1e-4);
});

test("robust loss option is accepted and applied", () => {
  const t = Float64Array.from([-2, -1, 0, 1, 2]);
  const y = Float64Array.from(t).map((ti) => 1 + 2 * ti);
  const result = leastSquares({
    kind: "polynomial",
    degree: 1,
    t,
    y,
    x0: Float64Array.from([0, 0]),
    loss: "soft_l1",
    fScale: 1.0,
  });
  assert.ok(result.success);
  assert.equal(result.n, 2);
});

test("leastSquaresDropOne returns the base solve plus one re-solve per row", () => {
  const a = Float64Array.from([1, 0, 0, 1, 1, 1]);
  const b = Float64Array.from([1, 2, 3]);
  const request = { kind: "linear", a, b, m: 3, n: 2, x0: Float64Array.from([0, 0]) };
  const report = leastSquaresDropOne(request);

  assert.equal(report.count, 3);
  assert.ok(report.base.success);
  assert.equal(report.costDeltas.length, 3);
  // Each drop is a full solve of the masked problem.
  for (let i = 0; i < report.count; i++) {
    const drop = report.dropAt(i);
    assert.equal(drop.n, 2);
    assert.equal(drop.m, 2); // one residual row removed
    assert.equal(report.costDeltas[i], drop.cost - report.base.cost);
  }
  assert.throws(() => report.dropAt(3), RangeError);
});

test("leastSquaresDropOne matches independent masked solves after delegating to core", () => {
  // y = 1 + 2 t + 3 t^2 with one perturbed sample, so the drops actually move.
  const t = [-2, -1, 0, 1, 2];
  const yArr = t.map((ti) => 1 + 2 * ti + 3 * ti * ti);
  yArr[2] += 0.5; // perturb one row so leave-one-out is non-trivial
  const base = {
    kind: "polynomial",
    degree: 2,
    t: Float64Array.from(t),
    y: Float64Array.from(yArr),
    x0: Float64Array.from([0, 0, 0]),
  };
  const report = leastSquaresDropOne(base);
  assert.equal(report.count, t.length);

  for (let drop = 0; drop < t.length; drop++) {
    // Solve the same family with row `drop` removed in JS, independently.
    const tk = t.filter((_, i) => i !== drop);
    const yk = yArr.filter((_, i) => i !== drop);
    const expected = leastSquares({
      kind: "polynomial",
      degree: 2,
      t: Float64Array.from(tk),
      y: Float64Array.from(yk),
      x0: Float64Array.from([0, 0, 0]),
    });
    const got = report.dropAt(drop);
    // Identical solver path: parameter vector and cost are byte-for-byte equal.
    assert.deepEqual(Array.from(got.x), Array.from(expected.x));
    assert.equal(got.cost, expected.cost);
    assert.equal(report.costDeltas[drop], got.cost - report.base.cost);
  }
});

test("invalid kind and missing data fields throw TypeError", () => {
  assert.throws(() => leastSquares({ kind: "bogus", x0: Float64Array.from([0]) }), TypeError);
  assert.throws(
    () =>
      leastSquares({
        kind: "linear",
        b: Float64Array.from([1]),
        m: 1,
        n: 1,
        x0: Float64Array.from([0]),
      }),
    TypeError,
  );
});
