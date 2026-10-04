// IONEX serializer reproduces the engine round-trip: parse a vertical-TEC
// product, re-encode it, and confirm re-parsing yields the same grid geometry
// and map axis, byte-stable on the second pass.

import { test } from "node:test";
import assert from "node:assert/strict";

import { ionexFromSamples, loadIonex } from "../pkg-node/sidereon.js";
import { fixture } from "./helpers.mjs";

const INX = "synthetic_2map_7x7.20i";

test("toIonexString re-parses to the same product and is byte-stable", () => {
  const ionex = loadIonex(fixture(INX));
  const text = ionex.toIonexString();
  const reparsed = loadIonex(Buffer.from(text, "utf8"));

  assert.deepEqual(Array.from(reparsed.latNodesDeg), Array.from(ionex.latNodesDeg));
  assert.deepEqual(Array.from(reparsed.lonNodesDeg), Array.from(ionex.lonNodesDeg));
  assert.deepEqual(Array.from(reparsed.mapEpochsJ2000S), Array.from(ionex.mapEpochsJ2000S));
  assert.equal(reparsed.exponent, ionex.exponent);
  assert.equal(reparsed.shellHeightKm, ionex.shellHeightKm);

  // A slant delay query evaluates identically against the re-parsed product.
  const epoch = ionex.mapEpochsJ2000S[0];
  assert.equal(
    reparsed.slantDelay(12, 34, 45, 30, epoch, 1575.42e6),
    ionex.slantDelay(12, 34, 45, 30, epoch, 1575.42e6),
  );

  // Deterministic: re-encoding the re-parsed product is byte-identical.
  assert.equal(reparsed.toIonexString(), text);
});

test("structured grid and header properties expose complete metadata", () => {
  const ionex = loadIonex(fixture(INX));

  assert.equal(ionex.hasRms, false);
  assert.equal(ionex.hasHeight, false);
  assert.equal(ionex.rmsMaps, null);
  assert.equal(ionex.heightMaps, null);

  const tec = ionex.tecMaps;
  assert.equal(tec.length, 2);
  assert.equal(tec[0].length, 7);
  assert.equal(tec[0][0].length, 7);
  assert.equal(tec[0][0][0], 10.0); // 100 * 10^-1

  const header = ionex.header;
  assert.equal(header.version, 1.1);
  assert.equal(header.mapsInFile, 2);
  // The fixture carries no MAPPING FUNCTION record: the declaration names that
  // case and the function property is null, not missing.
  assert.equal(header.mappingFunction, null);
  assert.equal(header.mappingDeclaration.kind, "ABSENT");
  assert.equal(ionex.mappingFunction, null);
  assert.equal(ionex.mappingDeclaration.kind, "ABSENT");
  // Records the file leaves out read as their unstated values, not as absences.
  assert.equal(header.program, "");
  assert.equal(header.runBy, "");
  assert.equal(header.intervalS, 0);
  assert.equal(header.elevationCutoffDeg, 0);
  assert.equal(header.stationCount, null);
  assert.equal(header.satelliteCount, null);

  const gridSamples = ionex.tecGridSamples();
  assert.equal(gridSamples.rmsMaps, null);
  assert.equal(gridSamples.heightMaps, null);
  assert.equal(gridSamples.header.mappingDeclaration.kind, "ABSENT");

  const reconstructed = ionexFromSamples(gridSamples);
  assert.deepEqual(Array.from(reconstructed.latNodesDeg), Array.from(ionex.latNodesDeg));
  assert.deepEqual(Array.from(reconstructed.lonNodesDeg), Array.from(ionex.lonNodesDeg));
  assert.equal(reconstructed.toIonexString(), ionex.toIonexString());
});
