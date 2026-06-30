// RTCM 3.x decode binding over sidereon_core::rtcm. The byte streams are
// generated from the crate's own encoders (see fixtures/rtcm.json), so decoding
// them recovers the exact transmitted field integers. Large fields (m0, sqrtA,
// eccentricity, ...) cross as BigInt to preserve precision.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  decodeRtcm,
  decodeRtcmFrame,
  decodeRtcmMessage,
  FrameScanner,
  rtcmMessageNumber,
} from "../pkg-node/sidereon.js";
import { fixtureJson } from "./helpers.mjs";

const hexToBytes = (hex) => Uint8Array.from(hex.match(/.{2}/g).map((b) => parseInt(b, 16)));

const fx = fixtureJson("rtcm.json");
const stream = hexToBytes(fx.stream);
const single = hexToBytes(fx.single1005);

test("decodeRtcm resyncs past a stray preamble and decodes both frames", () => {
  const messages = decodeRtcm(stream);
  assert.equal(messages.length, 2);

  const station = messages[0];
  assert.equal(station.type, "stationCoordinates");
  assert.equal(station.messageNumber, 1006);
  assert.equal(station.referenceStationId, 2003);
  assert.equal(station.ecefX, 11446021400n);
  assert.ok(Math.abs(station.xM - 1144602.14) < 1e-6);
  assert.ok(Math.abs(station.antennaHeightM - 1.5) < 1e-9);

  const eph = messages[1];
  assert.equal(eph.type, "gpsEphemeris");
  assert.equal(eph.messageNumber, 1019);
  assert.equal(eph.satelliteId, 14);
  assert.equal(eph.weekNumber, 1023);
  // Large signed/unsigned raw fields cross as BigInt.
  assert.equal(eph.m0, -1073741824n);
  assert.equal(eph.sqrtA, 2705000000n);
  assert.equal(eph.eccentricity, 21000000n);
  assert.equal(eph.fitInterval, true);
});

test("decodeRtcmFrame decodes a single 1005 frame and reports the frame length", () => {
  const frame = decodeRtcmFrame(single);
  assert.equal(frame.frameLen, single.length);
  assert.equal(frame.message.type, "stationCoordinates");
  assert.equal(frame.message.messageNumber, 1005);
  // 1005 carries no antenna height.
  assert.equal(frame.message.antennaHeight, undefined);
});

test("raw RTCM message body helpers decode the first scanned body", () => {
  const scanner = new FrameScanner(single);
  const frame = scanner.next();

  assert.equal(rtcmMessageNumber(frame.body), 1005);

  const message = decodeRtcmMessage(frame.body);
  assert.equal(message.type, "stationCoordinates");
  assert.equal(message.messageNumber, 1005);
  assert.equal(message.referenceStationId, 2003);
});

test("decodeRtcmFrame throws on a frame whose CRC is corrupted", () => {
  const corrupt = single.slice();
  corrupt[corrupt.length - 1] ^= 0xff;
  assert.throws(() => decodeRtcmFrame(corrupt));
});

test("FrameScanner walks every CRC-valid frame", () => {
  const scanner = new FrameScanner(stream);
  assert.equal(scanner.length, 2);

  const first = scanner.next();
  assert.ok(first.body instanceof Uint8Array);
  assert.ok(first.frameLen > 0);
  const second = scanner.next();
  assert.ok(second.body instanceof Uint8Array);
  // Exhausted.
  assert.equal(scanner.next(), undefined);
});

test("decodeRtcm returns an empty array for noise with no valid frame", () => {
  const noise = Uint8Array.from([0x00, 0xd3, 0x01, 0x02, 0x03, 0x04]);
  assert.deepEqual(decodeRtcm(noise), []);
});
