// Calendar and six-by-six covariance parity against sidereon-core 1.1.1.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  covariance6KmToM,
  covariance6MToKm,
  dataDayOfYear,
  dayOfYear,
  eciToRtnCovariance6,
  interpolateCovariance6,
  rtnToEciCovariance6,
  secondOfDay,
} from "../pkg-node/sidereon.js";

import { f64Bits } from "./helpers.mjs";

const bits = (values) =>
  Array.from(values, (value) => `0x${f64Bits(value).toString(16).padStart(16, "0")}`);

const diagonal = (values) => {
  const covariance = new Float64Array(36);
  for (const [index, value] of values.entries()) covariance[index * 7] = value;
  return covariance;
};

const covarianceA = diagonal([1, 4, 9, 16, 25, 36]);
const covarianceB = diagonal([4, 9, 16, 25, 36, 49]);
const positionKm = Float64Array.from([7000, 1000, 2000]);
const velocityKmS = Float64Array.from([-1, 7.2, 2]);

test("raw civil helpers and validated product-date helper keep distinct semantics", () => {
  assert.equal(f64Bits(secondOfDay(1, 2, 3.5)), 0x40ad170000000000n);
  assert.equal(secondOfDay(24, 0, 0), 86400);
  assert.equal(f64Bits(dayOfYear(2024, 2, 29, 12, 0, 0.25)), 0x404e40001845c8a1n);
  assert.equal(dayOfYear(2023, 2, 29, 0, 0, 0), 60);
  assert.equal(dataDayOfYear(2020, 3, 1), 61);
  assert.throws(() => dataDayOfYear(2023, 2, 29), RangeError);
  assert.throws(() => dataDayOfYear(2024, 13, 1), RangeError);
});

test("six-by-six covariance unit conversion preserves frozen core values", () => {
  assert.deepEqual(bits(covariance6KmToM(covarianceA)), [
    "0x412e848000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x414e848000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x41612a8800000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x416e848000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x4177d78400000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x41812a8800000000",
  ]);
  assert.deepEqual(bits(covariance6MToKm(covariance6KmToM(covarianceA))), bits(covarianceA));
});

test("PSD covariance interpolation and RTN transforms preserve core golden values", () => {
  assert.deepEqual(bits(interpolateCovariance6(covarianceA, covarianceB, 0)), bits(covarianceA));
  assert.deepEqual(bits(interpolateCovariance6(covarianceA, covarianceB, 1)), bits(covarianceB));
  assert.deepEqual(
    bits(interpolateCovariance6(covarianceA, covarianceB, 0.5)).filter(
      (_, index) => index % 7 === 0,
    ),
    [
      "0x3ffffffffffffffe",
      "0x4017ffffffffffff",
      "0x4027fffffffffffd",
      "0x4033fffffffffffe",
      "0x403dfffffffffffd",
      "0x4044ffffffffffff",
    ],
  );

  assert.deepEqual(bits(eciToRtnCovariance6(covarianceA, positionKm, velocityKmS)), [
    "0x3ffa5ed097b425ed",
    "0x3fed78c3ec349524",
    "0x3ffe85b02614fbc3",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x3fed78c3ec349524",
    "0x4010b28cc0ee71fb",
    "0x3ff00ca5fc4d1fad",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x3ffe85b02614fbc3",
    "0x3ff00ca5fc4d1fad",
    "0x40205adf8c924245",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x4031a5ed097b425f",
    "0x4003f78a9b5fab42",
    "0x4012d703de61d83a",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x4003f78a9b5fab42",
    "0x40394845165261b3",
    "0x4000c4d8c1a7165a",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x4012d703de61d83a",
    "0x4000c4d8c1a7165a",
    "0x404108e6f0192df7",
  ]);

  assert.deepEqual(bits(rtnToEciCovariance6(covarianceA, positionKm, velocityKmS)), [
    "0x3ff879a80d80a1ec",
    "0xbfb056eba46c0f68",
    "0xbffd2714d21ed63e",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0xbfb056eba46c0f68",
    "0x4011749cf8364061",
    "0xbff756366474dd65",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0xbffd2714d21ed63e",
    "0xbff756366474dd65",
    "0x4020367c8234cb92",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x40316337767a936b",
    "0xbfdce5d9c7f17a50",
    "0xc01285d9ac74840e",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0xbfdce5d9c7f17a50",
    "0x4039c18f38df42b4",
    "0xc00a61ad9c036550",
    "0x0000000000000000",
    "0x0000000000000000",
    "0x0000000000000000",
    "0xc01285d9ac74840e",
    "0xc00a61ad9c036550",
    "0x4040ed9ca85314f1",
  ]);
});

test("six-by-six covariance delegates retain binding input and engine errors", () => {
  assert.throws(() => covariance6KmToM(new Float64Array(35)), TypeError);
  assert.throws(() => covariance6KmToM(diagonal([-1, 4, 9, 16, 25, 36])), RangeError);
  assert.throws(() => interpolateCovariance6(covarianceA, covarianceB, 1.1), RangeError);
  assert.throws(
    () =>
      eciToRtnCovariance6(
        diagonal([
          Number.MAX_VALUE,
          Number.MAX_VALUE,
          Number.MAX_VALUE,
          Number.MAX_VALUE,
          Number.MAX_VALUE,
          Number.MAX_VALUE,
        ]),
        positionKm,
        velocityKmS,
      ),
    (error) => {
      assert.equal(error.name, "RangeError");
      assert.equal(error.message, "invalid input for covariance: components must be finite");
      return true;
    },
  );
  assert.throws(
    () => eciToRtnCovariance6(covarianceA, Float64Array.from([0, 0, 0]), velocityKmS),
    /zero position vector/,
  );
  assert.throws(
    () => rtnToEciCovariance6(covarianceA, positionKm, Float64Array.from([1, 2])),
    TypeError,
  );
});
