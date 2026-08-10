import { after, test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  MmapTerrain,
  PreciseInterpolantArtifact,
  loadSp3,
  terrainStoreChecksum64,
} from "../pkg-node/sidereon.js";
import { fixture, f64Bits } from "./helpers.mjs";

const HEADER_INDEX_OFFSET_OFFSET = 16;
const TERRAIN_HEADER_DATA_OFFSET_OFFSET = 24;
const PRECISE_HEADER_CHECKSUM_OFFSET = 40;
const PRECISE_POS_KX_OFFSET_OFFSET = 24;

const scratch = mkdtempSync(join(tmpdir(), "sidereon-wasm-attested-open-"));
after(() => rmSync(scratch, { recursive: true, force: true }));

const terrainBytes = Uint8Array.from(
  execFileSync(
    "cargo",
    ["run", "--quiet", "--bin", "dted_tree_to_mmap_store", "--", "test/fixtures/dted/tiles"],
    { maxBuffer: 16 * 1024 * 1024 },
  ),
);
const terrainClaim = terrainStoreChecksum64(terrainBytes);
const terrainPath = join(scratch, "terrain.tmm");
writeFileSync(terrainPath, terrainBytes);

const corruptTerrainBytes = Uint8Array.from(terrainBytes);
const terrainDataOffset = Number(
  new DataView(corruptTerrainBytes.buffer).getBigUint64(TERRAIN_HEADER_DATA_OFFSET_OFFSET, true),
);
corruptTerrainBytes[terrainDataOffset + 1] ^= 1;
const corruptTerrainPath = join(scratch, "terrain-corrupt.tmm");
writeFileSync(corruptTerrainPath, corruptTerrainBytes);

const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
const preciseBytes = sp3.preciseInterpolantArtifactBytes();
const preciseView = new DataView(
  preciseBytes.buffer,
  preciseBytes.byteOffset,
  preciseBytes.byteLength,
);
const preciseClaim = preciseView.getBigUint64(PRECISE_HEADER_CHECKSUM_OFFSET, true);
const precisePath = join(scratch, "precise.spi");
writeFileSync(precisePath, preciseBytes);

const corruptPreciseBytes = Uint8Array.from(preciseBytes);
const preciseIndexOffset = Number(preciseView.getBigUint64(HEADER_INDEX_OFFSET_OFFSET, true));
const precisePosKxOffset = Number(
  preciseView.getBigUint64(preciseIndexOffset + PRECISE_POS_KX_OFFSET_OFFSET, true),
);
corruptPreciseBytes[precisePosKxOffset + 1] ^= 1;
const corruptPrecisePath = join(scratch, "precise-corrupt.spi");
writeFileSync(corruptPrecisePath, corruptPreciseBytes);

const isPreciseChecksumError = (error) =>
  error instanceof Error && ["Checksum", "SatelliteChecksum"].includes(error.kind);

test("terrain attested path open skips corrupt payload verification until verify", () => {
  assert.throws(
    () => MmapTerrain.fromPath(corruptTerrainPath),
    (error) => error instanceof Error && error.kind === "Checksum",
  );

  const attested = MmapTerrain.fromPathAttested(corruptTerrainPath, terrainClaim);
  assert.equal(attested.digestProvenance, "attested");
  assert.equal(attested.checksum64(), terrainClaim);
  assert.throws(
    () => attested.verify(),
    (error) => error instanceof Error && error.kind === "Checksum",
  );
  assert.equal(attested.digestProvenance, "attested");
});

test("pristine terrain attested path open matches verified queries and escalates", () => {
  const verified = MmapTerrain.fromPath(terrainPath);
  const attested = MmapTerrain.fromPathAttested(terrainPath, terrainClaim);

  assert.equal(verified.digestProvenance, "verified");
  assert.equal(attested.digestProvenance, "attested");
  assert.equal(attested.checksum64(), terrainClaim);
  for (const [longitudeDeg, latitudeDeg] of [
    [-105.5, 36.5],
    [-106.5, 36.5],
    [-105.999, 36.001],
  ]) {
    assert.equal(
      f64Bits(attested.heightM(longitudeDeg, latitudeDeg)),
      f64Bits(verified.heightM(longitudeDeg, latitudeDeg)),
    );
  }

  attested.verify();
  assert.equal(attested.digestProvenance, "verified");
  assert.equal(attested.checksum64(), terrainClaim);

  const wrongClaim = terrainClaim ^ 1n;
  const wrong = MmapTerrain.fromPathAttested(terrainPath, wrongClaim);
  assert.equal(wrong.checksum64(), wrongClaim);
  assert.throws(
    () => wrong.verify(),
    (error) => error instanceof Error && error.kind === "AttestedChecksumMismatch",
  );
});

test("precise attested path open skips corrupt payload verification until verify", () => {
  assert.throws(
    () => PreciseInterpolantArtifact.fromPath(corruptPrecisePath),
    isPreciseChecksumError,
  );

  const attested = PreciseInterpolantArtifact.fromPathAttested(corruptPrecisePath, preciseClaim);
  assert.equal(attested.digestProvenance, "attested");
  assert.equal(attested.checksum64, preciseClaim);
  assert.throws(() => attested.verify(), isPreciseChecksumError);
  assert.equal(attested.digestProvenance, "attested");
});

test("precise attested path open rejects a claim different from the header", () => {
  const claimed = preciseClaim ^ 1n;
  assert.throws(
    () => PreciseInterpolantArtifact.fromPathAttested(precisePath, claimed),
    (error) =>
      error instanceof Error &&
      error.kind === "AttestedChecksumMismatch" &&
      error.detail.claimed === `0x${claimed.toString(16)}` &&
      error.detail.declared === `0x${preciseClaim.toString(16)}`,
  );
});

test("pristine precise attested path open matches verified queries and escalates", () => {
  const verified = PreciseInterpolantArtifact.fromPath(precisePath);
  const attested = PreciseInterpolantArtifact.fromPathAttested(precisePath, preciseClaim);
  const query = sp3.epochsJ2000Seconds()[10];
  const expected = verified.evaluate("G16", query);
  const found = attested.evaluate("G16", query);

  assert.equal(verified.digestProvenance, "verified");
  assert.equal(attested.digestProvenance, "attested");
  assert.equal(attested.checksum64, preciseClaim);
  assert.deepEqual(Array.from(found.positionM, f64Bits), Array.from(expected.positionM, f64Bits));
  assert.equal(f64Bits(found.clockS), f64Bits(expected.clockS));

  attested.verify();
  assert.equal(attested.digestProvenance, "verified");
  assert.equal(attested.checksum64, preciseClaim);
});

test("attested path open rejects malformed checksum claims before I/O", () => {
  for (const fromPathAttested of [
    MmapTerrain.fromPathAttested,
    PreciseInterpolantArtifact.fromPathAttested,
  ]) {
    assert.throws(() => fromPathAttested("missing", 1), TypeError);
    assert.throws(() => fromPathAttested("missing", "1"), TypeError);
    assert.throws(() => fromPathAttested("missing", -1n), RangeError);
    assert.throws(() => fromPathAttested("missing", 1n << 64n), RangeError);
  }
});

test("Node path open preserves typed I/O errors", () => {
  assert.throws(
    () => MmapTerrain.fromPath(join(scratch, "missing.tmm")),
    (error) =>
      error instanceof Error &&
      error.name === "Io" &&
      error.kind === "Io" &&
      error.detail.name === "Io",
  );
});
