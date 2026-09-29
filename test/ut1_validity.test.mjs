// UT1 coverage through the WASM binding. Every function that reads UT1 refuses
// an instant outside the UT1 table by default; its `WithValidity` variant takes
// "strict" (the default) or "permissive" and returns `{ value, ut1Degraded }`,
// where `ut1Degraded` names the side of the table a permissive result was
// computed on. The embedded table ends in 2027, so 2040 lies after it and 1950
// before it.

import { test } from "node:test";
import assert from "node:assert/strict";

import { GroundStation, Instant, Tle, sunAzEl, sunAzElWithValidity } from "../pkg-node/sidereon.js";
import { fixtureText } from "./helpers.mjs";

const unixUs = (year) => BigInt(Date.UTC(year, 0, 1, 12)) * 1000n;
const INSIDE = unixUs(2024);
const AFTER = unixUs(2040);
const BEFORE = unixUs(1950);

test("sidereal time is refused outside the UT1 table unless the policy is permissive", () => {
  const inside = Instant.fromUnixMicros(INSIDE);
  assert.equal(inside.ut1Degraded, undefined);
  assert.deepEqual(inside.gmstRadiansWithValidity(), {
    value: inside.gmstRadians(),
    ut1Degraded: null,
  });
  assert.deepEqual(inside.gastRadiansWithValidity("permissive"), {
    value: inside.gastRadians(),
    ut1Degraded: null,
  });

  const after = Instant.fromUnixMicros(AFTER);
  assert.equal(after.ut1Degraded, "afterCoverage");
  assert.throws(() => after.gmstRadians(), Error);
  assert.throws(() => after.gmstRadiansWithValidity(), Error);
  assert.throws(() => after.gmstRadiansWithValidity("strict"), Error);
  const permissive = after.gmstRadiansWithValidity("permissive");
  assert.equal(permissive.ut1Degraded, "afterCoverage");
  assert.ok(permissive.value >= 0 && permissive.value < 2 * Math.PI);

  const before = Instant.fromUnixMicros(BEFORE);
  assert.equal(before.gmstRadiansWithValidity("permissive").ut1Degraded, "beforeCoverage");

  assert.throws(() => inside.gmstRadiansWithValidity("loose"), TypeError);
});

test("sunAzElWithValidity matches sunAzEl inside the table and reports a departure outside it", () => {
  const site = [48.15, 11.58, 0.52];
  assert.deepEqual(sunAzElWithValidity(...site, INSIDE), {
    value: sunAzEl(...site, INSIDE),
    ut1Degraded: null,
  });
  assert.throws(() => sunAzEl(...site, AFTER), Error);
  assert.throws(() => sunAzElWithValidity(...site, AFTER, "strict"), Error);
  const outside = sunAzElWithValidity(...site, AFTER, "permissive");
  assert.equal(outside.ut1Degraded, "afterCoverage");
  assert.ok(Number.isFinite(outside.value.elevationDeg));
});

test("Tle.lookAnglesWithValidity refuses or reports an epoch outside the table", () => {
  const [, line1, line2] = fixtureText("omm/24876.tle").trimEnd().split(/\r?\n/);
  const tle = new Tle(line1, line2);
  const station = new GroundStation(48.15, 11.58, 520.0);
  const inside = tle.lookAnglesWithValidity(station, new BigInt64Array([INSIDE]), "strict");
  const plain = tle.lookAngles(station, new BigInt64Array([INSIDE]));
  assert.equal(inside.ut1Degraded, null);
  assert.deepEqual(Array.from(inside.value.azimuthDeg), Array.from(plain.azimuthDeg));
  assert.deepEqual(Array.from(inside.value.elevationDeg), Array.from(plain.elevationDeg));

  const later = new BigInt64Array([AFTER]);
  assert.throws(() => tle.lookAngles(station, later), Error);
  assert.throws(() => tle.lookAnglesWithValidity(station, later), Error);
  assert.equal(
    tle.lookAnglesWithValidity(station, later, "permissive").ut1Degraded,
    "afterCoverage",
  );
});
