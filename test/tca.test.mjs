// Time-of-closest-approach finding / screening / Pc over
// sidereon_core::astro::tca. The two objects are the ISS and a synthetic copy in
// a different orbital plane (RAAN shifted), so their relative range oscillates
// and the finder brackets several local minima over the window. Window bounds
// cross as unix-microsecond UTC stamps (bigint).

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  findTcaCandidates,
  findTcaConjunctions,
  screenTcaCandidates,
  screenTcaConjunctions,
} from "../pkg-node/sidereon.js";

// TLE checksum: sum of digits (minus signs count 1) mod 10, in the last column.
function tleChecksum(body68) {
  let sum = 0;
  for (const ch of body68) {
    if (ch >= "0" && ch <= "9") sum += Number(ch);
    else if (ch === "-") sum += 1;
  }
  return sum % 10;
}
const fixTle = (line) => {
  const body = line.slice(0, 68);
  return body + tleChecksum(body);
};

const PRIMARY_L1 = "1 25544U 98067A   26168.18949189  .00009113  00000+0  17172-3 0  9996";
const PRIMARY_L2 = "2 25544  51.6332 300.0813 0004737 195.1146 164.9702 15.49273435571752";
// Same satellite, RAAN 300.0813 -> 200.0813 (columns 18-25), checksum fixed.
const SECONDARY_L1 = PRIMARY_L1;
const SECONDARY_L2 = fixTle(
  "2 25544  51.6332 200.0813 0004737 195.1146 164.9702 15.49273435571752",
);

const START = BigInt(Date.UTC(2026, 5, 17, 0, 0, 0)) * 1000n;
const END = BigInt(Date.UTC(2026, 5, 17, 6, 0, 0)) * 1000n;

test("findTcacandidates brackets the relative-range minima over the window", () => {
  const candidates = findTcaCandidates(
    PRIMARY_L1,
    PRIMARY_L2,
    SECONDARY_L1,
    SECONDARY_L2,
    START,
    END,
    undefined,
  );
  assert.ok(candidates.length >= 1);
  for (const c of candidates) {
    assert.equal(c.relativePositionKm.length, 3);
    assert.equal(c.relativeVelocityKmS.length, 3);
    assert.ok(Number.isFinite(c.missDistanceKm) && c.missDistanceKm >= 0);
    assert.ok(Number.isFinite(c.tcaJd));
    assert.ok(c.tcaSecondsSinceWindowStart >= 0);
    // miss distance is the norm of the relative position.
    const norm = Math.hypot(...c.relativePositionKm);
    assert.ok(Math.abs(norm - c.missDistanceKm) < 1e-6);
  }
});

test("findTcaConjunctions evaluates a finite Pc at each TCA", () => {
  const conjunctions = findTcaConjunctions(
    PRIMARY_L1,
    PRIMARY_L2,
    SECONDARY_L1,
    SECONDARY_L2,
    START,
    END,
    { hardBodyRadiusKm: 0.02, method: "foster_equal_area" },
    undefined,
  );
  assert.ok(conjunctions.length >= 1);
  for (const c of conjunctions) {
    assert.ok(Number.isFinite(c.pc) && c.pc >= 0 && c.pc <= 1);
    assert.ok(Number.isFinite(c.missKm));
    assert.ok(Number.isFinite(c.candidate.missDistanceKm));
  }
});

test("screenTcaCandidates catches hits below a large threshold and none below a tiny one", () => {
  const big = screenTcaCandidates(
    PRIMARY_L1,
    PRIMARY_L2,
    [{ line1: SECONDARY_L1, line2: SECONDARY_L2 }],
    START,
    END,
    1e9,
    undefined,
  );
  assert.ok(big.length >= 1);
  for (const hit of big) {
    assert.equal(hit.secondaryIndex, 0);
    assert.ok(hit.candidate.missDistanceKm <= 1e9);
  }

  const tiny = screenTcaCandidates(
    PRIMARY_L1,
    PRIMARY_L2,
    [{ line1: SECONDARY_L1, line2: SECONDARY_L2 }],
    START,
    END,
    1e-6,
    undefined,
  );
  assert.equal(tiny.length, 0);
});

test("screenTcaConjunctions returns Pc-tagged screening hits", () => {
  const hits = screenTcaConjunctions(
    PRIMARY_L1,
    PRIMARY_L2,
    [{ line1: SECONDARY_L1, line2: SECONDARY_L2 }],
    START,
    END,
    1e9,
    { hardBodyRadiusKm: 0.02 },
    undefined,
  );
  assert.ok(hits.length >= 1);
  for (const hit of hits) {
    assert.equal(hit.secondaryIndex, 0);
    assert.ok(Number.isFinite(hit.conjunction.pc));
    assert.ok(Number.isFinite(hit.conjunction.candidate.missDistanceKm));
  }
});

test("findTcaCandidates rejects a malformed TLE via the engine", () => {
  assert.throws(() =>
    findTcaCandidates("garbage", "garbage", SECONDARY_L1, SECONDARY_L2, START, END, undefined),
  );
});
