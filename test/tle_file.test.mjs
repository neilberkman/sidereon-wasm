// parseTleFile over a CelesTrak-style multi-record file: a 3-line named record,
// a bare 2-line record (empty name), and a malformed record (skipped). Asserts
// the parsed count, names, skip count, and that a returned Tle actually
// propagates and produces look angles.

import { test } from "node:test";
import assert from "node:assert/strict";

import { parseTleFile, GroundStation } from "../pkg-node/sidereon.js";

// Valid ISS element set, reused as both a named (3-line) and a bare (2-line) record.
const L1 = "1 25544U 98067A   18184.80969102  .00001614  00000-0  31745-4 0  9993";
const L2 = "2 25544  51.6414 295.8524 0003435 262.6267 204.2868 15.54005638121106";

// A complete (line1, line2) record: the line-2 marker is present so it is picked
// up as a record, but the line is malformed (wrong length / non-numeric fields)
// so SGP4 init fails -> counted in `skipped`, not thrown.
const BAD_L1 = "1 00001U 00000A   18184.80969102  .00000000  00000-0  00000-0 0  0001";
const BAD_L2 = "2 00001 not a valid line two";

const FILE = [
  "ISS (ZARYA)",
  L1,
  L2,
  "",
  L1, // bare 2-line record (no preceding name line)
  L2,
  "0 BROKENSAT",
  BAD_L1,
  BAD_L2,
].join("\r\n");

test("parseTleFile parses names, skips malformed, and returns usable Tles", () => {
  const parsed = parseTleFile(FILE);

  assert.equal(parsed.skipped, 1);
  assert.equal(parsed.count, 2);

  const sats = parsed.satellites;
  assert.equal(sats.length, 2);

  // 3-line record keeps its name; bare 2-line record has an empty name.
  assert.equal(sats[0].name, "ISS (ZARYA)");
  assert.equal(sats[1].name, "");

  // The returned value is a real Tle: element getters and the kernels work.
  const tle = sats[0].tle;
  assert.equal(tle.catalogNumber, "25544");

  const epoch = 1530662400000000n; // 2018-07-04T00:00:00Z, unix microseconds
  const prop = tle.propagate([epoch]);
  assert.equal(prop.epochCount, 1);
  assert.ok(Number.isFinite(prop.positionKm[0]));
  assert.ok(prop.positionKm.some((v) => v !== 0));

  const station = new GroundStation(51.5, -0.1, 0);
  const look = tle.lookAngles(station, [epoch]);
  assert.equal(look.epochCount, 1);
  assert.ok(Number.isFinite(look.azimuthDeg[0]));
  assert.ok(Number.isFinite(look.elevationDeg[0]));
  assert.ok(look.rangeKm[0] > 0);
});

test("parseTleFile strips the CelesTrak '0 ' name marker", () => {
  const parsed = parseTleFile(["0 ISS (ZARYA)", L1, L2].join("\n"));
  assert.equal(parsed.count, 1);
  assert.equal(parsed.satellites[0].name, "ISS (ZARYA)");
});

test("parseTleFile rejects an invalid opsMode", () => {
  assert.throws(() => parseTleFile(`${L1}\n${L2}`, "bogus"));
});
