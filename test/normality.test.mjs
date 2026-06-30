// Residual-distribution diagnostics over sidereon_core::quality::normality. The
// moment statistics (skewness, kurtosis, Jarque-Bera) are closed-form
// arithmetic with no libm, so they are checked everywhere at a tight tolerance.
// Shapiro-Wilk rides libm (erf / pow / inverse-normal), so its exact value is
// tolerance-gated to Linux x86_64 and only structural elsewhere; the wasm build
// is tolerance-close to SciPy, not bit-for-bit.

import { test } from "node:test";
import assert from "node:assert/strict";

import { skewness, kurtosis, moments, jarqueBera, shapiroWilk } from "../pkg-node/sidereon.js";

const close = (a, b, tol) => assert.ok(Math.abs(a - b) <= tol, `${a} != ${b}`);

// x = [1,2,3,4,5]: mean 3, m2 = 2, m3 = 0, m4 = 6.8.
const X = Float64Array.from([1, 2, 3, 4, 5]);

test("skewness of symmetric data is zero", () => {
  close(skewness(X), 0.0, 1e-12);
  close(skewness(X, false), 0.0, 1e-12); // bias-corrected, still zero
});

test("kurtosis matches the closed-form excess and Pearson values", () => {
  // excess = m4/m2^2 - 3 = 6.8/4 - 3 = -1.3; Pearson = 1.7.
  close(kurtosis(X), -1.3, 1e-12); // fisher (excess) default
  close(kurtosis(X, false), 1.7, 1e-12); // pearson
});

test("moments returns the mean/variance/skewness/excess-kurtosis bundle", () => {
  const m = moments(X);
  close(m.mean, 3.0, 1e-12);
  close(m.variance, 2.0, 1e-12); // biased second central moment
  close(m.skewness, 0.0, 1e-12);
  close(m.kurtosisExcess, -1.3, 1e-12);
});

test("jarqueBera matches its closed form and survival p-value", () => {
  // JB = n/6 (S^2 + K^2/4), S = 0, K = -1.3 -> 5/6 * (1.69/4).
  const jb = jarqueBera(X);
  const stat = (5 / 6) * ((-1.3) ** 2 / 4);
  close(jb.statistic, stat, 1e-12);
  close(jb.pValue, Math.exp(-stat / 2), 1e-12);
});

test("shapiroWilk returns W in (0,1] with a valid p-value", () => {
  // Shapiro-Wilk rides libm, so the wasm build is tolerance-close to SciPy
  // rather than bit-for-bit (the native bit-exact bar lives in the core suite);
  // assert the structural envelope plus the expected direction. A near-linear
  // ramp is fairly Gaussian-looking (high W); clearly skewed data scores lower.
  const ramp = shapiroWilk(X);
  assert.ok(ramp.w > 0 && ramp.w <= 1.0);
  assert.ok(ramp.pValue >= 0 && ramp.pValue <= 1.0);
  assert.ok(ramp.w > 0.9);

  const skewed = shapiroWilk(Float64Array.from([1, 1, 1, 1, 1, 1, 1, 2, 5, 40]));
  assert.ok(skewed.w < ramp.w);
});

test("normality stats reject too-small or degenerate sets", () => {
  // bias-corrected skewness needs >= 3 values (InsufficientData -> TypeError).
  assert.throws(() => skewness(Float64Array.from([1, 2]), false), TypeError);
  assert.throws(() => shapiroWilk(Float64Array.from([1, 2])), TypeError); // need >= 3
  assert.throws(() => skewness(Float64Array.from([2, 2, 2])), RangeError); // zero variance
});
