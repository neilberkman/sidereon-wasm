// Inverse SGP4 fitting from a TEME sample arc through the WASM binding.

import { test } from "node:test";
import assert from "node:assert/strict";

import { Tle, fitTle } from "../pkg-node/sidereon.js";

import { f64Bits, f64FromBits, assertCloseRel, assertCloseAbs } from "./helpers.mjs";

const L1 = "1 25544U 98067A   18183.80969102  .00002605  00000-0  48194-4 0  9999";
const L2 = "2 25544  51.6418 282.1100 0003956 227.7591 296.3436 15.54198036120477";

const unixUsToJdParts = (us) => {
  const jd = 2440587.5 + Number(us) / 86400_000000;
  const whole = Math.trunc(jd);
  return [whole, jd - whole];
};

test("fitTle recovers a core-pinned TLE and OMM from propagated samples", () => {
  const truth = new Tle(L1, L2);
  const baseUs =
    BigInt(Date.UTC(2018, 0, 1, 0, 0, 0)) * 1000n + BigInt(Math.round(183.80969102 * 86400_000000));
  const epochs = BigInt64Array.from(
    [-180, -120, -60, 0, 60, 120, 180].map((dt) => baseUs + BigInt(dt) * 1_000_000n),
  );
  const truthArc = truth.propagate(epochs);
  const samples = Array.from(epochs, (epoch, i) => ({
    epoch: unixUsToJdParts(epoch),
    positionTemeKm: Array.from(truthArc.positionKm.slice(i * 3, i * 3 + 3)),
    velocityTemeKmS: Array.from(truthArc.velocityKmS.slice(i * 3, i * 3 + 3)),
  }));

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

  {
    // B* is unobservable on this short arc (fit.stats.bstar_observable is
    // asserted false below); its printed field and the line checksum are
    // platform-fragile fit outputs. Compare everything before the B* field.
    const expectedLine1 = "1 25544U 98067A   18184.80969102  .00000000  00000-0  69039-5 0  9999";
    assert.equal(fit.line1.slice(0, 53), expectedLine1.slice(0, 53));
    assert.equal(fit.line1.length, 69);
  }
  assert.equal(fit.line2, "2 25544  51.6418 277.1225 0003956 231.4635 131.4746 15.54204685120470");
  assert.deepEqual(fit.toLines(), [fit.line1, fit.line2]);
  assert.equal(fit.omm.noradCatId, 25544);
  assert.equal(fit.omm.classificationType, "U");
  assert.equal(fit.omm.revAtEpoch, 12047n);
  assert.equal(fit.omm.objectName, "ISS (ZARYA)");
  assert.equal(fit.omm.objectId, "1998-067A");
  assert.equal(fit.omm.epoch.iso8601, "2018-07-03T19:25:57.304112613201141");

  assertCloseRel(
    fit.stats.rms_position_km,
    f64FromBits(0x3f16b6c5b2fdd699n),
    1e-2,
    "stats.rms_position_km",
  );
  assertCloseRel(
    fit.stats.max_position_km,
    f64FromBits(0x3f21f40358bd1e50n),
    1e-2,
    "stats.max_position_km",
  );
  {
    const expectedAxes = ["0x3f0ab48fe4491db2", "0x3ef562b5e3f709df", "0x3f119467e55ede03"];
    Array.from(fit.stats.rms_position_axes_km).forEach((v, i) =>
      assertCloseRel(v, f64FromBits(BigInt(expectedAxes[i])), 1e-2, `rms_position_axes_km[${i}]`),
    );
  }
  assertCloseRel(
    fit.stats.rms_velocity_km_s,
    f64FromBits(0x3e9c2faecae62509n),
    1e-2,
    "stats.rms_velocity_km_s",
  );
  assertCloseRel(
    fit.stats.tle_rms_position_km,
    f64FromBits(0x3f7ec36ba1b07a29n),
    1e-2,
    "stats.tle_rms_position_km",
  );
  assert.equal(fit.stats.status, 3);
  assert.ok(fit.stats.nfev > 0 && fit.stats.nfev < 200);
  assert.ok(fit.stats.njev > 0 && fit.stats.njev < 100);
  assertCloseRel(fit.stats.cost, f64FromBits(0x3e5e99e8d0000000n), 1e-2, "fit cost");
  // Optimality is the terminal gradient norm; the iteration path differs per
  // platform, so any reference value is fragile. Converged means small.
  assert.ok(fit.stats.optimality >= 0 && fit.stats.optimality < 0.1);
  assert.equal(fit.stats.bstar_observable, false);
  assert.ok(fit.stats.seed_refine_passes >= 1 && fit.stats.seed_refine_passes <= 10);

  assert.equal(f64Bits(fit.elements.epoch[0]), 0x4142c15f80000000n);
  assert.equal(f64Bits(fit.elements.epoch[1]), 0x3fd3d1fa48800000n);
  assert.ok(Number.isFinite(fit.elements.bstar)); // unobservable; value is platform-fragile
  assertCloseRel(fit.elements.eccentricity, f64FromBits(0x3f39ec4ed0714d2an), 1e-6, "eccentricity");
  assertCloseRel(
    fit.elements.argument_of_perigee_deg,
    f64FromBits(0x406ceed54ef04e01n),
    1e-6,
    "argument_of_perigee_deg",
  );
  assertCloseRel(
    fit.elements.inclination_deg,
    f64FromBits(0x4049d2268084f515n),
    1e-9,
    "inclination_deg",
  );
  assertCloseRel(
    fit.elements.mean_anomaly_deg,
    f64FromBits(0x40606f302ff8e5d7n),
    1e-6,
    "mean_anomaly_deg",
  );
  assertCloseRel(
    fit.elements.mean_motion_rev_per_day,
    f64FromBits(0x402f15872a417487n),
    1e-9,
    "mean_motion_rev_per_day",
  );
  assertCloseRel(
    fit.elements.right_ascension_deg,
    f64FromBits(0x407151f5ba40ee40n),
    1e-9,
    "right_ascension_deg",
  );
  assert.equal(fit.elements.catalog_number, 25544);

  const fitted = new Tle(fit.line1, fit.line2);
  const check = fitted.propagate(BigInt64Array.from([epochs[3]]));
  {
    const expected = ["0x409066f98d3f8924", "0xc0ba29d1a62fd437", "0x4070aa459703f1cc"];
    Array.from(check.positionKm).forEach((v, i) =>
      assertCloseAbs(v, f64FromBits(BigInt(expected[i])), 0.05, `positionKm[${i}]`),
    );
  }
  {
    const expected = ["0x4012a8eca8872010", "0x3fef32249896b8f0", "0x40180641d28932d3"];
    Array.from(check.velocityKmS).forEach((v, i) =>
      assertCloseAbs(v, f64FromBits(BigInt(expected[i])), 1e-4, `velocityKmS[${i}]`),
    );
  }
});
