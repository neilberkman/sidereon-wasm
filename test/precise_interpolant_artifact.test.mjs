import { test } from "node:test";
import assert from "node:assert/strict";

import {
  loadSp3,
  openPreciseInterpolantArtifact,
  preciseInterpolantArtifactChecksum64,
} from "../pkg-node/sidereon.js";
import { coreGoldens, fixture, f64Bits } from "./helpers.mjs";

const STORE_ALIGNMENT = 4096n;
const alignUp = (value) => ((value + STORE_ALIGNMENT - 1n) / STORE_ALIGNMENT) * STORE_ALIGNMENT;

// FNV-1a over the store with the header checksum field (bytes 40..48) read as
// zero, as the store format defines its checksum.
function fnv1a64(bytes) {
  let hash = 0xcbf29ce484222325n;
  for (let index = 0; index < bytes.length; index++) {
    const byte = index >= 40 && index < 48 ? 0 : bytes[index];
    hash = ((hash ^ BigInt(byte)) * 0x100000001b3n) & 0xffffffffffffffffn;
  }
  return hash;
}

// The version 2 layout: a 64-byte header and a 96-byte index record per
// satellite, then each satellite's payload at the next 4096-byte boundary:
// position nodes (x plus three coordinates, 8 bytes each), their three
// accuracy values (16 bytes each), clock accuracy values (16 bytes) and clock
// nodes (24 bytes), 64-byte clock-arc records, and per arc its node abscissae
// and four coefficient arrays of one fewer entry (8 bytes each).
function checkLayout(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const satCount = view.getUint32(12, true);
  let expectedOffset = alignUp(64n + 96n * BigInt(satCount));
  for (let sat = 0; sat < satCount; sat++) {
    const record = 64 + 96 * sat;
    const positions = BigInt(view.getUint32(record + 4, true));
    const clockNodes = BigInt(view.getUint32(record + 8, true));
    const arcCount = view.getUint32(record + 12, true);
    const arcOffset = Number(view.getBigUint64(record + 56, true));
    const dataOffset = view.getBigUint64(record + 64, true);
    const dataLength = view.getBigUint64(record + 72, true);
    assert.equal(dataOffset, alignUp(expectedOffset));
    let length =
      positions * 32n +
      positions * 48n +
      clockNodes * 16n +
      clockNodes * 24n +
      BigInt(arcCount) * 64n;
    for (let arc = 0; arc < arcCount; arc++) {
      const arcNodes = BigInt(view.getUint32(arcOffset + 64 * arc, true));
      const coefficients = BigInt(view.getUint32(arcOffset + 64 * arc + 4, true));
      assert.equal(coefficients, arcNodes - 1n);
      length += arcNodes * 8n + 4n * coefficients * 8n;
    }
    assert.equal(dataLength, length);
    expectedOffset = dataOffset + dataLength;
  }
  return { satCount, totalLength: expectedOffset };
}

// Expected evaluations, length and checksum are the core's own for the same
// product (test/golden-gen `preciseArtifact`).
const golden = coreGoldens().preciseArtifact;

test("precise interpolant artifact bytes open and match SP3 interpolation bits", () => {
  const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const bytes = sp3.preciseInterpolantArtifactBytes();
  const second = sp3.preciseInterpolantArtifactBytes();
  assert.deepEqual(bytes, second);

  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  assert.equal(new TextDecoder().decode(bytes.subarray(0, 8)), "PEMAP001");
  assert.equal(view.getUint16(8, true), 2);
  assert.equal(view.getBigUint64(32, true), BigInt(bytes.length));
  const { satCount, totalLength } = checkLayout(bytes);
  assert.equal(totalLength, BigInt(bytes.length));
  assert.equal(bytes.length, golden.byteLength);

  const artifact = openPreciseInterpolantArtifact(bytes);
  assert.equal(artifact.byteLength, bytes.length);
  assert.equal(artifact.satellites.length, satCount);
  assert.equal(artifact.checksum64, fnv1a64(bytes));
  assert.equal(artifact.checksum64, view.getBigUint64(40, true));
  assert.equal(artifact.checksum64, BigInt(golden.checksum64));
  assert.equal(preciseInterpolantArtifactChecksum64(bytes), artifact.checksum64);
  assert.equal(artifact.timeScale, "Gpst");
  assert.deepEqual(artifact.satellites.slice(0, 5), ["G01", "G02", "G03", "G05", "G06"]);

  const epochs = sp3.epochsJ2000Seconds();
  const queries = [epochs[10], 0.5 * (epochs[10] + epochs[11])];
  queries.forEach((query, index) => {
    const expected = golden.g16[index];
    assert.equal(f64Bits(query), BigInt(expected.queryJ2000S));
    const mapped = artifact.evaluate("G16", query);
    const interpolated = sp3.interpolate("G16", Float64Array.of(query));
    const position = expected.positionM.map((value) => BigInt(value));
    assert.deepEqual(Array.from(mapped.positionM, f64Bits), position);
    assert.equal(f64Bits(mapped.clockS), BigInt(expected.clockS));
    assert.deepEqual(Array.from(interpolated.positionM, f64Bits), position);
    assert.equal(f64Bits(interpolated.clockS[0]), BigInt(expected.clockS));
  });
});

test("precise interpolant artifact maps public byte framing and index failures", () => {
  const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const bytes = sp3.preciseInterpolantArtifactBytes();
  const withChecksum = (mutate) => {
    const changed = Uint8Array.from(bytes);
    mutate(changed, new DataView(changed.buffer));
    const view = new DataView(changed.buffer);
    view.setBigUint64(40, 0n, true);
    view.setBigUint64(40, fnv1a64(changed), true);
    return changed;
  };
  const cases = [
    {
      name: "bad magic",
      bytes: Uint8Array.of(0, 1, 2, 3, 4, 5, 6, 7),
      kind: "BadMagic",
      check: (error) => {
        assert.equal(error.detail.found, "[00, 01, 02, 03, 04, 05, 06, 07]");
        assert.equal(
          error.message,
          "precise interpolant store parse error: missing store magic, found [00, 01, 02, 03, 04, 05, 06, 07]",
        );
      },
    },
    {
      name: "short fixed header",
      bytes: bytes.slice(0, 32),
      kind: "HeaderTruncated",
      check: (error) => {
        assert.equal(error.detail.available, "32");
        assert.equal(
          error.message,
          "precise interpolant store truncated: 32 bytes, shorter than the 64-byte header",
        );
      },
    },
    {
      name: "unsupported version",
      bytes: withChecksum((_changed, view) => view.setUint16(8, 99, true)),
      kind: "UnsupportedVersion",
      check: (error) => {
        assert.equal(error.detail.version, 99);
        assert.equal(error.message, "precise interpolant store version 99 is not supported");
      },
    },
    {
      name: "invalid index offset",
      bytes: withChecksum((_changed, view) => view.setBigUint64(16, 72n, true)),
      kind: "Parse",
      check: (error) => {
        assert.equal(error.detail.reason, "index offset must be 64, got 72");
        assert.equal(
          error.message,
          "precise interpolant store parse error: index offset must be 64, got 72",
        );
      },
    },
    {
      name: "unsupported time scale",
      bytes: withChecksum((changed) => {
        changed[10] = 255;
      }),
      kind: "UnsupportedTimeScale",
      check: (error) => {
        assert.equal(error.detail.tag, 255);
        assert.equal(
          error.message,
          "precise interpolant store time-scale tag 255 is not supported",
        );
      },
    },
    {
      name: "unsupported satellite system",
      bytes: withChecksum((changed) => {
        changed[64] = 255;
      }),
      kind: "UnsupportedSatelliteSystem",
      check: (error) => {
        assert.equal(error.detail.tag, 255);
        assert.equal(
          error.message,
          "precise interpolant store satellite-system tag 255 is not supported",
        );
      },
    },
    {
      name: "trailing bytes",
      bytes: withChecksum((changed, view) => {
        view.setBigUint64(32, BigInt(bytes.length - 1), true);
        changed[changed.length - 1] = 0;
      }),
      kind: "TrailingBytes",
      check: (error) => {
        assert.equal(error.detail.declared, String(bytes.length - 1));
        assert.equal(error.detail.available, String(bytes.length));
        assert.equal(
          error.message,
          `precise interpolant store has trailing bytes: header declares ${bytes.length - 1} bytes but ${bytes.length} are present`,
        );
      },
    },
    {
      name: "satellite range out of bounds",
      bytes: withChecksum((_changed, view) =>
        view.setBigUint64(64 + 72, BigInt(bytes.length), true),
      ),
      kind: "RangeOutOfBounds",
      check: (error) => {
        assert.equal(error.detail.region, "satellite data");
        assert.equal(error.detail.satelliteId, "G01");
        assert.equal(
          error.detail.offset,
          new DataView(bytes.buffer).getBigUint64(64 + 64, true).toString(),
        );
        assert.equal(error.detail.len, String(bytes.length));
        assert.equal(error.detail.available, String(bytes.length));
      },
    },
    {
      name: "satellite payload checksum",
      bytes: withChecksum((changed) => {
        const view = new DataView(changed.buffer);
        const offset = Number(view.getBigUint64(64 + 64, true));
        const length = Number(view.getBigUint64(64 + 72, true));
        changed[offset + length - 1] ^= 1;
      }),
      kind: "SatelliteChecksum",
      check: (error) => {
        assert.equal(error.detail.satelliteId, "G01");
        assert.equal(typeof error.detail.expected, "string");
        assert.equal(typeof error.detail.found, "string");
        assert.match(
          error.message,
          /^precise interpolant satellite G01 checksum expected 0x[0-9a-f]+ but found 0x[0-9a-f]+$/,
        );
      },
    },
  ];

  assert.equal(cases.length, 9);
  for (const entry of cases) {
    assert.throws(
      () => openPreciseInterpolantArtifact(entry.bytes),
      (error) => {
        assert.ok(error instanceof Error, entry.name);
        assert.equal(error.name, entry.kind, entry.name);
        assert.equal(error.kind, entry.kind, entry.name);
        assert.equal(error.detail.name, entry.kind, entry.name);
        entry.check(error);
        return true;
      },
      entry.name,
    );
  }
});

test("precise interpolant artifact rejects corrupt and truncated bytes with typed errors", () => {
  const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const bytes = sp3.preciseInterpolantArtifactBytes();
  const hex = (value) => `0x${value.toString(16).padStart(16, "0")}`;

  const corrupt = Uint8Array.from(bytes);
  corrupt[corrupt.length - 1] ^= 0x80;
  assert.throws(
    () => openPreciseInterpolantArtifact(corrupt),
    (error) =>
      error instanceof Error &&
      error.kind === "Checksum" &&
      error.detail.expected === hex(fnv1a64(bytes)) &&
      error.detail.found === hex(fnv1a64(corrupt)),
  );

  // A short store is reported by its declared and present lengths, never as a
  // checksum mismatch.
  assert.throws(
    () => openPreciseInterpolantArtifact(bytes.slice(0, bytes.length - 1)),
    (error) =>
      error instanceof Error &&
      error.kind === "Truncated" &&
      error.detail.declared === String(bytes.length) &&
      error.detail.available === String(bytes.length - 1),
  );
});
