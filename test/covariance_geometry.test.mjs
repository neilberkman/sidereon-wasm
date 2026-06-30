// Jacobian-derived covariance / Hessian-trace and the 2x2 confidence ellipse,
// over sidereon_core::astro::math::least_squares and ::dop. The covariance
// values follow closed-form linear algebra, so they are checked everywhere at a
// tight tolerance.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  normalCovariance,
  hessianTrace,
  covarianceFromJacobian,
  errorEllipse2,
  leastSquares,
} from "../pkg-node/sidereon.js";

const close = (a, b, tol = 1e-12) => assert.ok(Math.abs(a - b) <= tol, `${a} != ${b}`);

// J = [[1,0],[0,1],[1,1]] (row-major), J^T J = [[2,1],[1,2]],
// (J^T J)^-1 = [[2,-1],[-1,2]] / 3.
const JAC = Float64Array.from([1, 0, 0, 1, 1, 1]);

test("normalCovariance returns the (J^T J)^-1 cofactor", () => {
  const cov = normalCovariance(JAC, 3, 2, 1.0);
  assert.equal(cov.length, 4);
  close(cov[0], 2 / 3);
  close(cov[1], -1 / 3);
  close(cov[2], -1 / 3);
  close(cov[3], 2 / 3);
});

test("normalCovariance scales by the variance scale", () => {
  const cov = normalCovariance(JAC, 3, 2, 2.0);
  close(cov[0], 4 / 3);
  close(cov[3], 4 / 3);
});

test("hessianTrace is the sum of squared column norms", () => {
  // columns [1,0,1] and [0,1,1]: 2 + 2 = 4 = trace(J^T J).
  close(hessianTrace(JAC, 3, 2), 4.0);
});

test("covarianceFromJacobian scales by the post-fit reduced chi-square", () => {
  // Solve a tiny problem, then feed the solve's Jacobian + cost back in.
  const result = leastSquares({
    kind: "linear",
    a: JAC,
    b: Float64Array.from([1, 2, 3.3]),
    m: 3,
    n: 2,
    x0: Float64Array.from([0, 0]),
  });
  const cov = covarianceFromJacobian(result.jac, result.m, result.n, result.cost);
  // s_sq = 2 cost / (m - n); covariance = s_sq * (J^T J)^-1.
  const sSq = (2 * result.cost) / (result.m - result.n);
  close(cov[0], sSq * (2 / 3), 1e-9);
  close(cov[3], sSq * (2 / 3), 1e-9);
});

test("covarianceFromJacobian rejects non-positive degrees of freedom", () => {
  // m == n: no redundancy.
  assert.throws(() => covarianceFromJacobian(Float64Array.from([1, 0, 0, 1]), 2, 2, 0.0), RangeError);
});

test("normalCovariance rejects a rank-deficient Jacobian", () => {
  // Two identical columns -> singular.
  assert.throws(() => normalCovariance(Float64Array.from([1, 1, 2, 2, 3, 3]), 3, 2, 1.0), Error);
});

test("errorEllipse2 scales the eigenvalues by the chi-square(2) quantile", () => {
  // Diagonal covariance [[4,0],[0,1]], confidence 0.95.
  const conf = 0.95;
  const ellipse = errorEllipse2(Float64Array.from([4, 0, 0, 1]), conf);
  const scale = -2 * Math.log(1 - conf);
  close(ellipse.chiSquareScale, scale);
  close(ellipse.semiMajor, Math.sqrt(4 * scale));
  close(ellipse.semiMinor, Math.sqrt(1 * scale));
  close(ellipse.orientationRad, 0.0);
  assert.equal(ellipse.confidence, conf);
});

test("errorEllipse2 rejects a bad shape and a non-PSD block", () => {
  assert.throws(() => errorEllipse2(Float64Array.from([1, 0, 0]), 0.95), TypeError);
  assert.throws(() => errorEllipse2(Float64Array.from([-1, 0, 0, -1]), 0.95), RangeError);
});
