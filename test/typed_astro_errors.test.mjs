import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import {
  findMoonTransitsWithValidity,
  meridianTransits,
  planetaryEvents,
  seasons,
  Spk,
  observe,
  observeWithValidity,
  observeSpkBody,
  observeSpkBodyWithValidity,
  sunAzElWithValidity,
} from "../pkg-node/sidereon.js";

const here = (rel) => fileURLToPath(new URL(rel, import.meta.url));
const LAT = 51.4769;
const LON = 0.0;
const ALT_KM = 0.046;
const START = 1_704_067_200_000_000n;
const END = 1_704_153_600_000_000n;
const EROS = 20000433;
const SUN = 10;
const SPK_ET = 757339200.0;
const BEFORE_UT1_TABLE = BigInt(Date.UTC(1800, 0, 1)) * 1000n;

function thrown(call) {
  try {
    call();
  } catch (error) {
    return error;
  }
  assert.fail("expected the call to throw");
}

function floatBitsHex(value) {
  const bytes = new ArrayBuffer(8);
  new DataView(bytes).setFloat64(0, value, false);
  return Array.from(new Uint8Array(bytes), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function loadErosKernel() {
  return new Spk(new Uint8Array(await readFile(here("./fixtures/spk/horizons_eros_type21.bsp"))));
}

test("strict sky UT1 errors retain nested typed causes and permissive success", () => {
  const strict = thrown(() => sunAzElWithValidity(LAT, LON, ALT_KM, BEFORE_UT1_TABLE, "strict"));
  assert.equal(strict.name, "Error");
  assert.equal(strict.detail.family, "bodyObservation");
  assert.equal(strict.detail.cause.kind, "frameTransform");
  assert.equal(
    strict.detail.cause.message,
    "frame transform reads UT1, but the instant precedes the UT1 table coverage",
  );
  assert.deepEqual(strict.detail.cause.cause, {
    kind: "ut1OutsideCoverage",
    reason: "beforeCoverage",
  });
  assert.equal(typeof strict.message, "string");
  assert.equal(
    strict.message,
    "topocentric reduction failed: frame transform reads UT1, but the instant precedes the UT1 table coverage",
  );

  const permissive = sunAzElWithValidity(LAT, LON, ALT_KM, BEFORE_UT1_TABLE, "permissive");
  assert.equal(permissive.ut1Degraded, "beforeCoverage");
  assert.ok(Number.isFinite(permissive.value.azimuthDeg));
  assert.ok(Number.isFinite(permissive.value.elevationDeg));
  assert.ok(Number.isFinite(permissive.value.rangeKm));

  assert.throws(
    () => sunAzElWithValidity(NaN, LON, ALT_KM, BEFORE_UT1_TABLE, "strict"),
    RangeError,
  );
});

test("event finder and almanac refusals retain their exact current variants", () => {
  const finderError = thrown(() =>
    findMoonTransitsWithValidity(LAT, LON, ALT_KM, START, END, 0.0, 1.0, "strict"),
  );
  assert.equal(finderError.name, "Error");
  assert.equal(finderError.detail.family, "eventFinder");
  assert.deepEqual(finderError.detail.cause, {
    kind: "invalidInput",
    field: "step_seconds",
    reason: "not positive",
  });

  const scanError = thrown(() => seasons(START, END, 0.0, 1.0));
  assert.equal(scanError.detail.family, "almanac");
  assert.deepEqual(scanError.detail.cause, {
    kind: "invalidInput",
    field: "step_seconds",
    reason: "not positive",
  });
  assert.equal(scanError.message, "invalid almanac input step_seconds: not positive");

  const ephemerisError = thrown(() =>
    meridianTransits(
      "mars",
      { latitudeDeg: LAT, longitudeDeg: LON, altitudeKm: ALT_KM },
      START,
      END,
      300.0,
      1.0,
    ),
  );
  assert.equal(ephemerisError.detail.family, "almanac");
  assert.deepEqual(ephemerisError.detail.cause, { kind: "ephemerisRequired" });
  assert.equal(ephemerisError.message, "SPK ephemeris is required");

  assert.throws(() => meridianTransits("sun", null, START, END, 300.0, 1.0), TypeError);
});

test("SPK construction and state failures preserve parser fields and exact epochs", async () => {
  const unsupportedId = thrown(() => new Spk(new Uint8Array(1024)));
  assert.equal(unsupportedId.message, 'unsupported DAF identification word ""');
  assert.deepEqual(unsupportedId.detail.cause, { kind: "unsupportedDafId", idWord: "" });

  const unsupportedFormatBytes = new Uint8Array(1024);
  unsupportedFormatBytes.set(new TextEncoder().encode("DAF/SPK"), 0);
  unsupportedFormatBytes.set(new TextEncoder().encode("BINARY??"), 88);
  const unsupportedFormat = thrown(() => new Spk(unsupportedFormatBytes));
  assert.equal(unsupportedFormat.message, 'unsupported DAF binary format "BINARY??"');
  assert.deepEqual(unsupportedFormat.detail.cause, {
    kind: "unsupportedBinaryFormat",
    binaryFormat: "BINARY??",
  });

  const truncated = thrown(() => new Spk(new Uint8Array([1, 2, 3])));
  assert.equal(truncated.name, "Error");
  assert.equal(truncated.detail.family, "spk");
  assert.deepEqual(truncated.detail.cause, {
    kind: "truncated",
    context: "DAF file record",
    needed: 1024,
    actual: 3,
  });

  const spk = await loadErosKernel();
  try {
    const missing = thrown(() => spk.state(1234567, SUN, SPK_ET));
    assert.equal(missing.detail.family, "spk");
    assert.deepEqual(missing.detail.cause, { kind: "unknownBody", body: 1234567 });

    const coverageGap = thrown(() => spk.state(EROS, SUN, 1.0e99));
    assert.equal(coverageGap.detail.family, "spk");
    const { et, ...coverageFields } = coverageGap.detail.cause;
    assert.deepEqual(coverageFields, {
      kind: "coverageGap",
      target: EROS,
      center: SUN,
    });
    assert.deepEqual(Object.keys(et).sort(), ["bitsHex", "decimal"]);
    assert.equal(et.bitsHex, floatBitsHex(1.0e99));
    assert.equal(typeof et.decimal, "string");

    const native = spk.state(EROS, SUN, SPK_ET);
    try {
      const frameError = thrown(() => spk.stateInFrame(EROS, SUN, SPK_ET, 22));
      assert.equal(frameError.detail.cause.kind, "nonInertialFrameRotation");
      assert.equal(frameError.detail.cause.from, native.frame);
      assert.equal(frameError.detail.cause.to, 22);
    } finally {
      native.free();
    }

    const nested = thrown(() =>
      planetaryEvents(spk, "mars", "opposition", START, END, 3600.0, 1.0),
    );
    assert.equal(nested.detail.family, "almanac");
    assert.deepEqual(nested.detail.cause, {
      kind: "spk",
      message: "unknown SPK body 399",
      cause: { kind: "unknownBody", body: 399 },
    });
  } finally {
    spk.free();
  }
});

test("all general observation routes retain complete typed ObserveError causes", async () => {
  const validStation = { latitudeDeg: LAT, longitudeDeg: LON, altitudeKm: ALT_KM };
  const invalidStation = { ...validStation, latitudeDeg: 91 };
  const epochUnixUs = BigInt(Date.UTC(2024, 0, 1)) * 1000n;
  const invalidExpected = {
    family: "observation",
    cause: {
      kind: "frameTransform",
      message: "invalid frame transform latitude_deg: must be in [-90, 90]",
      cause: { kind: "invalidInput", field: "latitude_deg", reason: "must be in [-90, 90]" },
    },
  };
  for (const call of [
    () => observe(invalidStation, epochUnixUs, "sun"),
    () => observeWithValidity(invalidStation, epochUnixUs, "sun", undefined, "strict"),
  ]) {
    const error = thrown(call);
    assert.equal(error.name, "Error");
    assert.deepEqual(error.detail, invalidExpected);
    assert.equal(
      error.message,
      "frame transform failed: invalid frame transform latitude_deg: must be in [-90, 90]",
    );
  }

  const beforeTable = BigInt(Date.UTC(1800, 0, 1)) * 1000n;
  const ut1Error = thrown(() =>
    observeWithValidity(validStation, beforeTable, "sun", undefined, "strict"),
  );
  assert.equal(ut1Error.detail.family, "observation");
  assert.deepEqual(ut1Error.detail.cause, {
    kind: "frameTransform",
    message: "frame transform reads UT1, but the instant precedes the UT1 table coverage",
    cause: { kind: "ut1OutsideCoverage", reason: "beforeCoverage" },
  });

  const kernel = new Spk(new Uint8Array(await readFile(here("./fixtures/bodies/observe_de.bsp"))));
  const spkErrors = [
    () => observeSpkBody(invalidStation, epochUnixUs, kernel, 4),
    () => observeSpkBodyWithValidity(invalidStation, epochUnixUs, kernel, 4, "strict"),
  ];
  for (const call of spkErrors) {
    const error = thrown(call);
    assert.equal(error.name, "Error");
    assert.equal(error.detail.family, "observation");
    assert.deepEqual(error.detail.cause, invalidExpected.cause);
  }
});
