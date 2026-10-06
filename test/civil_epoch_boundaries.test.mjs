import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import {
  Instant,
  Spk,
  findMoonTransits,
  observe,
  observeSpkBody,
  sunAzEl,
  sunMoonEci,
  temeToGcrs,
} from "../pkg-node/sidereon.js";

const BELOW_CIVIL_UTC = -62_167_219_200_000_001n;
const ABOVE_CIVIL_UTC = 253_402_300_800_000_000n;
const FIRST_CIVIL_UTC = -62_167_219_200_000_000n;
const LAST_CIVIL_UTC = 253_402_300_799_999_999n;
const VALID_EPOCH = 1_704_067_200_000_000n;
const STATION = { latitudeDeg: 51.4779, longitudeDeg: -0.0015, altitudeKm: 0.046 };

const civilRangeError = (fn) => {
  assert.throws(fn, (error) => {
    assert.equal(error instanceof RangeError, true);
    assert.match(error.message, /UTC civil year from 0 through 9999/);
    return true;
  });
};

test("unrestricted instants reject out-of-civil-range epochs only at civil conversion", () => {
  assert.doesNotThrow(() => Instant.fromUnixMicros(FIRST_CIVIL_UTC).ttJd);
  assert.doesNotThrow(() => Instant.fromUnixMicros(LAST_CIVIL_UTC).ttJd);

  for (const epoch of [BELOW_CIVIL_UTC, ABOVE_CIVIL_UTC]) {
    const instant = Instant.fromUnixMicros(epoch);
    assert.equal(instant.unixMicros, epoch);
    civilRangeError(() => instant.ttJd);
    civilRangeError(() => instant.gmstRadians());
    civilRangeError(() => instant.nutationAngles());
    civilRangeError(() => instant.precessionMatrix());
  }
});

test("civil epoch validation covers body, observation, event, and frame routes", () => {
  for (const epoch of [BELOW_CIVIL_UTC, ABOVE_CIVIL_UTC]) {
    civilRangeError(() => sunMoonEci(new BigInt64Array([epoch])));
    civilRangeError(() => sunAzEl(51.4779, -0.0015, 0.046, epoch));
    civilRangeError(() => observe(STATION, epoch, "sun"));
    civilRangeError(() => findMoonTransits(51.4779, -0.0015, 0.046, epoch, VALID_EPOCH, 300, 1));
    civilRangeError(() =>
      temeToGcrs(
        new Float64Array([7000, 0, 0]),
        new Float64Array([0, 7.5, 0]),
        new BigInt64Array([epoch]),
      ),
    );
  }
});

test("an invalid SPK observation does not prevent a later valid call", () => {
  const fixtureDir = fileURLToPath(new URL("./fixtures", import.meta.url));
  const spk = new Spk(new Uint8Array(readFileSync(`${fixtureDir}/bodies/observe_de.bsp`)));

  civilRangeError(() => observeSpkBody(STATION, ABOVE_CIVIL_UTC, spk, 4));
  const observation = observeSpkBody(STATION, VALID_EPOCH, spk, 4);
  assert.equal(Number.isFinite(observation.horizontal.rangeKm), true);
  spk.free();
});
