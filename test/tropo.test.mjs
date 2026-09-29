// Standalone troposphere reproduces the core Saastamoinen + Niell reference
// values bit-for-bit (0 ULP), the same goldens the Elixir binding pins in
// troposphere_test.exs (which are the core's own reference-fixture numbers).

import { test } from "node:test";
import assert from "node:assert/strict";

import { tropoZenithDelay, tropoMappingFactors, tropoSlantDelay } from "../pkg-node/sidereon.js";

// Standard sea-level surface meteorology used by the troposphere reference
// fixtures: 1013.25 hPa, 288.15 K, 50% relative humidity.
const MET = { pressureHpa: 1013.25, temperatureK: 288.15, relativeHumidity: 0.5 };

// Day-of-year 28.0 (the fixtures' seasonal argument), Jan 28 2021 00:00:00 UTC,
// as a split Julian date (JD = 2459242.5).
const JD_WHOLE = 2459242.5;
const JD_FRACTION = 0.0;

test("zenith delay matches the reference fixture bit-for-bit", () => {
  const z = tropoZenithDelay(45.0, 0.0, MET);
  assert.equal(z.dryM, 2.3069675999999997);
  assert.equal(z.wetM, 0.08601004964012601);
});

test("mapping is unity at the zenith", () => {
  const m = tropoMappingFactors(90.0, 45.0, 0.0, JD_WHOLE, JD_FRACTION);
  assert.equal(m.dry, 1.0);
  assert.equal(m.wet, 1.0);
});

test("mapping grows toward lower elevation", () => {
  const m90 = tropoMappingFactors(90.0, 45.0, 0.0, JD_WHOLE, JD_FRACTION);
  const m30 = tropoMappingFactors(30.0, 45.0, 0.0, JD_WHOLE, JD_FRACTION);
  const m10 = tropoMappingFactors(10.0, 45.0, 0.0, JD_WHOLE, JD_FRACTION);
  assert.ok(m30.dry > m90.dry);
  assert.ok(m10.dry > m30.dry);
});

test("zenith slant equals the reference fixture bit-for-bit", () => {
  // Niell mapping is unity at 90 deg, so the slant delay is the sum of the
  // zenith delays; this is the 'zenith_midlat' troposphere reference value.
  const slant = tropoSlantDelay(90.0, 45.0, 10.0, 0.0, MET, JD_WHOLE, JD_FRACTION);
  assert.equal(slant, 2.392977649640126);
});

test("slant is zero at and below the horizon", () => {
  assert.equal(tropoSlantDelay(0.0, 45.0, 10.0, 0.0, MET, JD_WHOLE, JD_FRACTION), 0.0);
  assert.equal(tropoSlantDelay(-5.0, 45.0, 10.0, 0.0, MET, JD_WHOLE, JD_FRACTION), 0.0);
});

test("slant grows as elevation drops", () => {
  const s90 = tropoSlantDelay(90.0, 45.0, 10.0, 0.0, MET, JD_WHOLE, JD_FRACTION);
  const s30 = tropoSlantDelay(30.0, 45.0, 10.0, 0.0, MET, JD_WHOLE, JD_FRACTION);
  const s10 = tropoSlantDelay(10.0, 45.0, 10.0, 0.0, MET, JD_WHOLE, JD_FRACTION);
  assert.ok(s30 > s90);
  assert.ok(s10 > s30);
});

test("malformed meteorology throws", () => {
  assert.throws(() => tropoZenithDelay(45.0, 0.0, { pressureHpa: 1000.0 }));
});

test("core meteorology refusal keeps its full typed error", () => {
  assert.throws(
    () => tropoZenithDelay(45.0, 0.0, { ...MET, pressureHpa: -1.0 }),
    (error) => {
      assert.ok(error instanceof RangeError);
      assert.equal(error.detail.kind, "INVALID_INPUT");
      assert.equal(error.detail.message, "pressure_hpa not positive");
      assert.equal(error.message, `invalid input: ${error.detail.message}`);
      return true;
    },
  );
});

test("core mapping refusal keeps its full typed error", () => {
  assert.throws(
    () => tropoMappingFactors(1.0, 45.0, 0.0, JD_WHOLE, JD_FRACTION),
    (error) => {
      assert.ok(error instanceof RangeError);
      assert.equal(error.detail.kind, "INVALID_INPUT");
      assert.equal(error.detail.message, "elevation_rad below mapping validity");
      assert.equal(error.message, `invalid input: ${error.detail.message}`);
      return true;
    },
  );
});

test("core split-date refusal keeps its full typed error", () => {
  assert.throws(
    () => tropoMappingFactors(45.0, 45.0, 0.0, Infinity, 0.0),
    (error) => {
      assert.ok(error instanceof RangeError);
      assert.equal(error.detail.family, "TimeModelError");
      assert.equal(error.detail.kind, "TIME_MODEL_INVALID_INPUT");
      assert.equal(error.detail.message, error.message);
      assert.equal(error.detail.field, "jd_whole");
      assert.equal(error.detail.reason, "must be finite");
      return true;
    },
  );
});

test("invalid receiver keeps its full frame-value error", () => {
  assert.throws(
    () => tropoZenithDelay(100.0, 0.0, MET),
    (error) => {
      assert.ok(error instanceof RangeError);
      assert.equal(error.detail.family, "FrameValueError");
      assert.equal(error.detail.kind, "FRAME_VALUE_INVALID_INPUT");
      assert.equal(error.detail.message, error.message);
      assert.equal(error.detail.field, "lat_rad");
      assert.equal(error.detail.reason, "must be in [-pi/2, pi/2]");
      return true;
    },
  );
});

test("a non-finite elevation is rejected, not folded to the horizon zero", () => {
  // -Infinity is bad input, not a sub-horizon geometry; it must throw rather
  // than silently return the below-horizon 0.0.
  assert.throws(
    () => tropoSlantDelay(-Infinity, 45.0, 10.0, 0.0, MET, JD_WHOLE, JD_FRACTION),
    RangeError,
  );
  assert.throws(
    () => tropoSlantDelay(NaN, 45.0, 10.0, 0.0, MET, JD_WHOLE, JD_FRACTION),
    RangeError,
  );
});
