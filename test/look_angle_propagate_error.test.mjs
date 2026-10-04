import { test } from "node:test";
import assert from "node:assert/strict";

import { GroundStation, Tle } from "../pkg-node/sidereon.js";

test("Tle.lookAngles retains SGP4 propagation details", () => {
  const line1 = "1 28872U 05037B   05333.02012661  .25992681  00000-0  24476-3 0  1534";
  const line2 = "2 28872  96.4736 157.9986 0303955 244.0492 110.6523 16.46015938 10708";
  const tle = new Tle(line1, line2);
  const station = new GroundStation(37, -122, 0);
  try {
    const epoch = BigInt(Date.UTC(2005, 0, 1)) * 1000n +
      BigInt(Math.round((333.02012661 - 1) * 86_400_000_000));
    const decay = epoch + 1440n * 60n * 1_000_000n;
    let failure;
    try {
      tle.lookAngles(station, BigInt64Array.of(decay));
    } catch (error) {
      failure = error;
    }
    assert.ok(failure instanceof Error);
    assert.equal(failure.message, "SGP4 propagation failed: SGP4 error code 6");
    assert.deepEqual(failure.detail, {
      family: "lookAngle",
      cause: {
        kind: "propagate",
        message: "SGP4 error code 6",
        cause: { kind: "sgp4", code: 6 },
      },
    });
  } finally {
    station.free();
    tle.free();
  }
});
