// Inverse SGP4 fitting from a TEME sample arc through the WASM binding.

import { test } from "node:test";
import assert from "node:assert/strict";

import { Tle, fitTle } from "../pkg-node/sidereon.js";

import { coreGoldens, f64Bits, hexToF64 } from "./helpers.mjs";

const L1 = "1 25544U 98067A   18183.80969102  .00002605  00000-0  48194-4 0  9999";
const L2 = "2 25544  51.6418 282.1100 0003956 227.7591 296.3436 15.54198036120477";

const unixUsToJdParts = (us) => {
  const jd = 2440587.5 + Number(us) / 86400_000000;
  const whole = Math.trunc(jd);
  return [whole, jd - whole];
};

test("fitTle reproduces the engine fit of propagated samples as a TLE and an OMM", () => {
  // Line 2 of the pinned element set carries checksum digit 7 where its
  // columns 1-68 give 6: the strict default refuses it, so the element set is
  // read under the lenient policy, which reports the mismatch.
  assert.throws(
    () => new Tle(L1, L2),
    /line 2 checksum digit 7 does not match the computed checksum 6/,
  );
  const truth = new Tle(L1, L2, undefined, "lenient");
  assert.deepEqual(
    truth.checksumWarnings.map((w) => [w.lineLabel, w.kind, w.expected, w.computed]),
    [["line 2", "mismatch", 7, 6]],
  );
  const baseUs =
    BigInt(Date.UTC(2018, 0, 1, 0, 0, 0)) * 1000n + BigInt(Math.round(183.80969102 * 86400_000000));
  const epochs = BigInt64Array.from(
    [-180, -120, -60, 0, 60, 120, 180].map((dt) => baseUs + BigInt(dt) * 1_000_000n),
  );
  const truthArc = truth.propagate(epochs);

  // The engine's own fit of the same samples, reproduced natively by
  // test/golden-gen. SGP4 and the fit run on the portable libm and a pure-Rust
  // linear algebra, so the samples, lines and elements agree to the bit.
  const golden = coreGoldens().tleFit;
  const bits = (values) => values.map((text) => BigInt(text));
  assert.deepEqual(
    Array.from(truthArc.positionKm, f64Bits),
    bits(golden.samples.flatMap((sample) => sample.positionTemeKm)),
  );
  assert.deepEqual(
    Array.from(truthArc.velocityKmS, f64Bits),
    bits(golden.samples.flatMap((sample) => sample.velocityTemeKmS)),
  );
  const samples = golden.samples.map((sample) => ({
    epoch: sample.epoch.map(hexToF64),
    positionTemeKm: sample.positionTemeKm.map(hexToF64),
    velocityTemeKmS: sample.velocityTemeKmS.map(hexToF64),
  }));
  // The test's own epoch split gives the same parts.
  assert.deepEqual(
    Array.from(epochs, (epoch) => unixUsToJdParts(epoch).map(f64Bits)),
    golden.samples.map((sample) => bits(sample.epoch)),
  );

  const fit = fitTle(samples, {
    fitBstar: true,
    useVelocity: true,
    velocityWeightS: 60,
    loss: "softL1",
    fScale: 1,
    xScale: "jac",
    maxNfev: 80,
    metadata: {
      catalogNumber: 25544,
      classification: "U",
      internationalDesignator: "98067A",
      elementSetNumber: 999,
      revAtEpoch: 12047,
      objectName: "ISS (ZARYA)",
    },
  });

  assert.equal(fit.line1, golden.line1);
  assert.equal(fit.line2, golden.line2);
  assert.deepEqual(fit.toLines(), [fit.line1, fit.line2]);
  assert.equal(fit.omm.noradCatId, 25544);
  assert.equal(fit.omm.classificationType, "U");
  assert.equal(fit.omm.revAtEpoch, 12047n);
  assert.equal(fit.omm.objectName, "ISS (ZARYA)");
  assert.equal(fit.omm.objectId, "1998-067A");
  const ommEpoch = fit.omm.epoch;
  assert.deepEqual(
    [
      ommEpoch.year,
      ommEpoch.month,
      ommEpoch.day,
      ommEpoch.hour,
      ommEpoch.minute,
      ommEpoch.second,
      ommEpoch.microsecond,
      ommEpoch.femtosecond,
    ],
    golden.ommEpoch,
  );

  // Every numeric element and statistic is the engine's, to the bit.
  const sameNumbers = (got, want, label) => {
    for (const [key, value] of Object.entries(want)) {
      if (typeof value === "number") {
        assert.equal(f64Bits(got[key]), f64Bits(value), `${label}.${key}`);
      } else if (Array.isArray(value) && value.every((v) => typeof v === "number")) {
        assert.deepEqual(Array.from(got[key], f64Bits), value.map(f64Bits), `${label}.${key}`);
      }
    }
  };
  sameNumbers(fit.elements, golden.elements, "elements");
  sameNumbers(fit.stats, golden.stats, "stats");
  assert.equal(fit.stats.bstar_observable, golden.stats.bstar_observable);
  assert.equal(fit.elements.catalog_number, 25544);

  // The printed TLE propagates as the engine propagates it.
  const fitted = new Tle(fit.line1, fit.line2);
  const check = fitted.propagate(BigInt64Array.from([epochs[3]]));
  assert.deepEqual(Array.from(check.positionKm, f64Bits), bits(golden.check.positionKm));
  assert.deepEqual(Array.from(check.velocityKmS, f64Bits), bits(golden.check.velocityKmS));
});
