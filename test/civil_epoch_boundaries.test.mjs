import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import {
  Instant,
  Spk,
  findMoonElevationCrossings,
  findMoonTransits,
  lunarSolarEclipses,
  meridianTransits,
  observe,
  observeSpkBody,
  seasons,
  sunAzEl,
  sunMoonEci,
  temeToGcrs,
} from "../pkg-node/sidereon.js";

const BELOW_CIVIL_UTC = -62_167_219_200_000_001n;
const ABOVE_CIVIL_UTC = 253_402_300_800_000_000n;
const FIRST_CIVIL_UTC = -62_167_219_200_000_000n;
const LAST_CIVIL_UTC = 253_402_300_799_999_999n;
const MIN_I64_UTC = -9_223_372_036_854_775_808n;
const MAX_I64_UTC = 9_223_372_036_854_775_807n;
const VALID_EPOCH = 1_704_067_200_000_000n;
const STATION = { latitudeDeg: 51.4779, longitudeDeg: -0.0015, altitudeKm: 0.046 };

const civilRangeError = (fn, argName) => {
  assert.throws(fn, (error) => {
    assert.equal(error instanceof RangeError, true);
    assert.match(error.message, /UTC civil year from 0 through 9999/);
    if (argName) {
      assert.ok(
        error.message.includes(argName),
        `expected RangeError message "${error.message}" to name ${argName}`,
      );
    }
    return true;
  });
};

test("unrestricted instants reject out-of-civil-range epochs only at civil conversion", () => {
  assert.doesNotThrow(() => Instant.fromUnixMicros(FIRST_CIVIL_UTC).ttJd);
  assert.doesNotThrow(() => Instant.fromUnixMicros(LAST_CIVIL_UTC).ttJd);

  for (const epoch of [BELOW_CIVIL_UTC, ABOVE_CIVIL_UTC, MIN_I64_UTC, MAX_I64_UTC]) {
    const instant = Instant.fromUnixMicros(epoch);
    assert.equal(instant.unixMicros, epoch);
    civilRangeError(() => instant.ttJd);
    civilRangeError(() => instant.ut1Jd);
    civilRangeError(() => instant.tdbJd);
    civilRangeError(() => instant.gmstRadians());
    civilRangeError(() => instant.nutationAngles());
    civilRangeError(() => instant.precessionMatrix());
  }
});

test("civil epoch validation covers body, observation, event, almanac, and frame routes", () => {
  for (const epoch of [BELOW_CIVIL_UTC, ABOVE_CIVIL_UTC, MIN_I64_UTC, MAX_I64_UTC]) {
    civilRangeError(() => sunMoonEci(new BigInt64Array([epoch])));
    civilRangeError(() =>
      temeToGcrs(
        new Float64Array([7000, 0, 0]),
        new Float64Array([0, 7.5, 0]),
        new BigInt64Array([epoch]),
      ),
    );
    civilRangeError(() => sunAzEl(51.4779, -0.0015, 0.046, epoch), "epochUnixUs");
    civilRangeError(() => observe(STATION, epoch, "sun"), "epochUnixUs");
  }

  for (const [start, end, expectedArg] of [
    [BELOW_CIVIL_UTC, VALID_EPOCH, "startUnixUs"],
    [VALID_EPOCH, ABOVE_CIVIL_UTC, "endUnixUs"],
    [MIN_I64_UTC, VALID_EPOCH, "startUnixUs"],
    [VALID_EPOCH, MAX_I64_UTC, "endUnixUs"],
  ]) {
    civilRangeError(
      () => findMoonTransits(51.4779, -0.0015, 0.046, start, end, 300, 1),
      expectedArg,
    );
    civilRangeError(
      () => findMoonElevationCrossings(51.4779, -0.0015, 0.046, start, end, undefined),
      expectedArg,
    );
    civilRangeError(() => seasons(start, end, 3600, 1), expectedArg);
    civilRangeError(() => meridianTransits("sun", STATION, start, end, 300, 1), expectedArg);
    civilRangeError(() => lunarSolarEclipses(start, end, 3600, 1), expectedArg);
  }
});

test("an invalid SPK observation does not prevent a later valid call", () => {
  const fixtureDir = fileURLToPath(new URL("./fixtures", import.meta.url));
  const spk = new Spk(new Uint8Array(readFileSync(`${fixtureDir}/bodies/observe_de.bsp`)));

  civilRangeError(() => observeSpkBody(STATION, ABOVE_CIVIL_UTC, spk, 4), "epochUnixUs");
  civilRangeError(() => observeSpkBody(STATION, MAX_I64_UTC, spk, 4), "epochUnixUs");
  const observation = observeSpkBody(STATION, VALID_EPOCH, spk, 4);
  assert.equal(Number.isFinite(observation.horizontal.rangeKm), true);
  spk.free();
});
