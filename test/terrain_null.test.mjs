// Terrain-store null postings and index checks through the WASM binding.
//
// The store is built here byte by byte in the TMMAP001 layout the core writes:
// a 64-byte header, one 80-byte index record, padding to 4096, then a 5 x 5
// payload of little-endian i16 postings stored longitude-major. The posting at
// longitude index 2, latitude index 2 holds -32767, the stored DTED null, so
// every lookup that weights it has no height.

import { test } from "node:test";
import assert from "node:assert/strict";

import { MmapTerrain, TerrainGeoidModel } from "../pkg-node/sidereon.js";

const NULL_POSTING = -32767;
const LAT_INDEX = 36;
const LON_INDEX = -107;
const COUNT = 5;

// Posting value at (longitude index, latitude index); the centre is the null.
const postingValue = (lon, lat) => (lon === 2 && lat === 2 ? NULL_POSTING : 100 + 10 * lon + lat);

function fnv1a64(bytes) {
  let hash = 0xcbf29ce484222325n;
  for (const byte of bytes) {
    hash ^= BigInt(byte);
    hash = (hash * 0x100000001b3n) & 0xffffffffffffffffn;
  }
  return hash;
}

function storeBytes({
  latIndex = LAT_INDEX,
  lonIndex = LON_INDEX,
  maxLatitudeDeg = LAT_INDEX + 1,
} = {}) {
  const dataOffset = 4096;
  const payload = new Uint8Array(COUNT * COUNT * 2);
  const payloadView = new DataView(payload.buffer);
  for (let lon = 0; lon < COUNT; lon++) {
    for (let lat = 0; lat < COUNT; lat++) {
      payloadView.setInt16(2 * (lon * COUNT + lat), postingValue(lon, lat), true);
    }
  }
  const bytes = new Uint8Array(dataOffset + payload.length);
  const view = new DataView(bytes.buffer);
  bytes.set(new TextEncoder().encode("TMMAP001"), 0);
  view.setUint16(8, 1, true); // version
  bytes[10] = 1; // EGM96 orthometric
  view.setUint32(12, 1, true); // tile count
  view.setBigUint64(16, 64n, true); // index offset
  view.setBigUint64(24, BigInt(dataOffset), true);
  view.setBigUint64(32, BigInt(bytes.length), true);
  const record = 64;
  view.setInt32(record, latIndex, true);
  view.setInt32(record + 4, lonIndex, true);
  view.setUint32(record + 8, COUNT, true);
  view.setUint32(record + 12, COUNT, true);
  view.setBigUint64(record + 16, BigInt(dataOffset), true);
  view.setBigUint64(record + 24, BigInt(payload.length), true);
  view.setBigUint64(record + 32, fnv1a64(payload), true);
  view.setFloat64(record + 40, latIndex, true);
  view.setFloat64(record + 48, lonIndex, true);
  view.setFloat64(record + 56, maxLatitudeDeg, true);
  view.setFloat64(record + 64, lonIndex + 1, true);
  bytes[record + 72] = 1;
  bytes.set(payload, dataOffset);
  return bytes;
}

function assertUnknownElevation(detail) {
  assert.deepEqual(
    { ...detail, message: undefined },
    {
      kind: "UNKNOWN_TERRAIN_ELEVATION",
      latIndex: LAT_INDEX,
      lonIndex: LON_INDEX,
      latitudePosting: 2,
      longitudePosting: 2,
      message: undefined,
    },
  );
  assert.equal(typeof detail.message, "string");
}

function assertLookupRefused(call) {
  assert.throws(call, (err) => {
    assert.equal(err.name, "TerrainLookupError");
    assertUnknownElevation(err.detail);
    assert.equal(err.detail.message, err.message);
    return true;
  });
}

test("a lookup that weights a null posting has no height", () => {
  const store = MmapTerrain.fromBytes(storeBytes());

  // The nearest posting to (-106.45, 36.55) is the null at (2, 2).
  assertLookupRefused(() =>
    store.heightMWithOptions(-106.45, 36.55, { interpolation: "nearestPosting" }),
  );
  // A bilinear cell with the null at one corner.
  assertLookupRefused(() => store.heightM(-106.6, 36.4));
  assertLookupRefused(() => store.orthometricHeightM(-106.6, 36.4));

  // Exactly on the known posting (1, 2) beside the null: that posting.
  assert.equal(store.heightM(-106.75, 36.5), postingValue(1, 2));
  assert.equal(
    store.heightMWithOptions(-106.95, 36.05, { interpolation: "nearest" }),
    postingValue(0, 0),
  );
});

test("batch lookups carry the typed refusal beside the message", () => {
  const store = MmapTerrain.fromBytes(storeBytes());
  const batch = store.heightBatch(
    [
      [-106.6, 36.4],
      [-106.75, 36.5],
      [10.5, 10.5],
    ],
    { interpolation: "bilinear" },
  );
  assert.equal(batch.length, 3);
  assert.equal(batch[0].ok, false);
  assert.equal(batch[0].heightM, null);
  assert.equal(typeof batch[0].error, "string");
  assertUnknownElevation(batch[0].detail);
  assert.deepEqual(batch[1], { ok: true, heightM: postingValue(1, 2), error: null, detail: null });
  assert.equal(batch[2].ok, false);
  assert.equal(batch[2].detail.kind, "MISSING_TERRAIN_TILE");
  assert.equal(batch[2].detail.latIndex, 10);
  assert.equal(batch[2].detail.lonIndex, 10);

  const typed = store.orthometricHeightBatch([[-106.6, 36.4]], {});
  assert.equal(typed[0].ok, false);
  assert.equal(typed[0].orthometricHeightM, null);
  assertUnknownElevation(typed[0].detail);
});

test("an ellipsoidal lookup at a null posting names the terrain refusal", () => {
  const store = MmapTerrain.fromBytes(storeBytes());
  assert.throws(
    () => store.ellipsoidalHeightMWithModel(-106.6, 36.4, {}, TerrainGeoidModel.egm96OneDegree()),
    (err) => {
      assert.equal(err.name, "Terrain");
      assert.equal(err.detail.name, "Terrain");
      assertUnknownElevation(err.detail.terrain);
      return true;
    },
  );
});

test("index records must name a tile in the coordinate domain with its own edges", () => {
  assert.throws(
    () => MmapTerrain.fromBytes(storeBytes({ maxLatitudeDeg: LAT_INDEX + 1.5 })),
    (err) => {
      assert.equal(err.name, "TileBoundsMismatch");
      assert.equal(err.detail.name, "TileBoundsMismatch");
      assert.equal(err.detail.latIndex, LAT_INDEX);
      assert.equal(err.detail.lonIndex, LON_INDEX);
      assert.equal(err.detail.field, "max_latitude_deg");
      return true;
    },
  );
  assert.throws(
    () => MmapTerrain.fromBytes(storeBytes({ latIndex: 90, maxLatitudeDeg: 91 })),
    (err) => {
      assert.equal(err.name, "TileIdOutOfRange");
      assert.equal(err.detail.latIndex, 90);
      assert.equal(err.detail.lonIndex, LON_INDEX);
      return true;
    },
  );
});
