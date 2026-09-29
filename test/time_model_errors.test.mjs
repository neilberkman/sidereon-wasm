import { test } from "node:test";
import assert from "node:assert/strict";

import {
  GnssWeekTow,
  JulianDate,
  TimeScale,
  galileoNequickDelay,
  siderealFilter,
} from "../pkg-node/sidereon.js";

function assertTimeModelDetail(error, { name, message, field, reason }) {
  assert.equal(error.name, name);
  assert.equal(error.message, message);
  assert.deepEqual(error.detail, {
    family: "TimeModelError",
    kind: "TIME_MODEL_INVALID_INPUT",
    message: message.replace(/^periodS: /, ""),
    field,
    reason,
  });
}

test("civil Julian-date construction retains its typed time refusal", () => {
  assert.throws(
    () => JulianDate.fromUtcCivil(2020, 6, 24, 12, 0, 86401),
    (error) => {
      assertTimeModelDetail(error, {
        name: "RangeError",
        message: "invalid time model fraction: must be within one residual day",
        field: "fraction",
        reason: "must be within one residual day",
      });
      return true;
    },
  );
});

test("GNSS week/TOW construction retains the exact invalid field", () => {
  assert.throws(
    () => new GnssWeekTow(TimeScale.Gpst, 1, Number.NaN),
    (error) => {
      assertTimeModelDetail(error, {
        name: "RangeError",
        message: "invalid time model tow_s: must be finite",
        field: "tow_s",
        reason: "must be finite",
      });
      return true;
    },
  );
});

test("GNSS week normalization and rollover keep their distinct typed causes", () => {
  const normalize = new GnssWeekTow(TimeScale.Gpst, 0xffff_ffff, 604800);
  assert.throws(
    () => normalize.normalized(),
    (error) => {
      assertTimeModelDetail(error, {
        name: "RangeError",
        message: "invalid time model tow_s: normalized week is out of range",
        field: "tow_s",
        reason: "normalized week is out of range",
      });
      return true;
    },
  );

  const unroll = new GnssWeekTow(TimeScale.Gpst, 0xffff_ffff, 0);
  assert.throws(
    () => unroll.unrolledWeek(1),
    (error) => {
      assertTimeModelDetail(error, {
        name: "RangeError",
        message: "invalid time model rollovers: unrolled week is out of range",
        field: "rollovers",
        reason: "unrolled week is out of range",
      });
      return true;
    },
  );
});

test("sidereal duration retains its legacy prefix and typed time refusal", () => {
  assert.throws(
    () => siderealFilter(new Float64Array([1]), Number.NaN, {}),
    (error) => {
      assertTimeModelDetail(error, {
        name: "RangeError",
        message: "periodS: invalid time model seconds: must be finite",
        field: "seconds",
        reason: "must be finite",
      });
      return true;
    },
  );
});

test("ionosphere civil split keeps the existing Error class and message", () => {
  assert.throws(
    () => galileoNequickDelay(0, 0, 0, 40, -3, 120, 30, 2020, 6, 24, 12, 0, 86401, 1.57542e9),
    (error) => {
      assertTimeModelDetail(error, {
        name: "Error",
        message: "invalid time model fraction: must be within one residual day",
        field: "fraction",
        reason: "must be within one residual day",
      });
      return true;
    },
  );
});
