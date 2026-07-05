import { test } from "node:test";
import assert from "node:assert/strict";

import {
  loadSp3,
  openPreciseInterpolantArtifact,
  preciseInterpolantArtifactChecksum64,
} from "../pkg-node/sidereon.js";
import { fixture, f64Bits } from "./helpers.mjs";

test("precise interpolant artifact bytes open and match SP3 interpolation bits", () => {
  const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const bytes = sp3.preciseInterpolantArtifactBytes();
  const second = sp3.preciseInterpolantArtifactBytes();
  assert.deepEqual(bytes, second);

  const artifact = openPreciseInterpolantArtifact(bytes);
  assert.equal(bytes.length, 926_752);
  assert.equal(artifact.byteLength, bytes.length);
  assert.equal(artifact.checksum64, 5250867419192089605n);
  assert.equal(preciseInterpolantArtifactChecksum64(bytes), artifact.checksum64);
  assert.equal(artifact.timeScale, "Gpst");
  assert.deepEqual(artifact.satellites.slice(0, 5), ["G01", "G02", "G03", "G05", "G06"]);

  const epochs = sp3.epochsJ2000Seconds();
  const queries = [epochs[10], 0.5 * (epochs[10] + epochs[11])];
  const expected = [
    {
      position: [0xc178d8dfe4b43959n, 0xc11fd0228624dd30n, 0xc158a4012116872bn],
      clock: 0xbf26d6277fd0e497n,
    },
    {
      position: [0xc1787e3fa9228724n, 0xc1243593360464a4n, 0xc15dd0bb39793458n],
      clock: 0xbf26d639ccc4382en,
    },
  ];

  queries.forEach((query, index) => {
    const mapped = artifact.evaluate("G16", query);
    const interpolated = sp3.interpolate("G16", Float64Array.of(query));
    assert.deepEqual(Array.from(mapped.positionM, f64Bits), expected[index].position);
    assert.equal(f64Bits(mapped.clockS), expected[index].clock);
    assert.deepEqual(Array.from(interpolated.positionM, f64Bits), expected[index].position);
    assert.equal(f64Bits(interpolated.clockS[0]), expected[index].clock);
  });
});

test("precise interpolant artifact rejects corrupt and truncated bytes with typed errors", () => {
  const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const bytes = sp3.preciseInterpolantArtifactBytes();

  const corrupt = Uint8Array.from(bytes);
  corrupt[corrupt.length - 1] ^= 0x80;
  assert.throws(
    () => openPreciseInterpolantArtifact(corrupt),
    (error) =>
      error instanceof Error &&
      error.kind === "Checksum" &&
      error.detail.expected === "0x48ded418f39ddc05" &&
      error.detail.found === "0x48df5418f39eb585",
  );

  assert.throws(
    () => openPreciseInterpolantArtifact(bytes.slice(0, bytes.length - 1)),
    (error) =>
      error instanceof Error &&
      error.kind === "Checksum" &&
      error.detail.expected === "0x48ded418f39ddc05" &&
      error.detail.found === "0xa35ac693c5b59f27",
  );
});
