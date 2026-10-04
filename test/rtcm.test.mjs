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
  decodeRtcmStream,
  encodeRtcmFrame,
  FrameScanner,
  RtcmLockTimeTracker,
  rtcmDeriveLli,
  rtcmLliBits,
  rtcmMessageNumber,
  rtcmMinimumLockTimeMs,
  rtcmMsmEpochDtMs,
  rtcmMsmSignalRinexCode,
} from "../pkg-node/sidereon.js";
import { fixtureJson } from "./helpers.mjs";

const hexToBytes = (hex) => Uint8Array.from(hex.match(/.{2}/g).map((b) => parseInt(b, 16)));

const fx = fixtureJson("rtcm.json");
const stream = hexToBytes(fx.stream);
const single = hexToBytes(fx.single1005);

const concatBytes = (...chunks) => {
  const out = new Uint8Array(chunks.reduce((sum, chunk) => sum + chunk.length, 0));
  let offset = 0;
  for (const chunk of chunks) {
    out.set(chunk, offset);
    offset += chunk.length;
  }
  return out;
};

test("decodeRtcm decodes both frames in a valid stream", () => {
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
  assert.equal(eph.satelliteId, 8);
  assert.equal(eph.weekNumber, 123);
  // Large signed/unsigned raw fields cross as BigInt.
  assert.equal(eph.m0, -1234567n);
  assert.equal(eph.sqrtA, 2700000000n);
  assert.equal(eph.eccentricity, 4000000n);
  assert.equal(eph.fitInterval, false);
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

test("decodeRtcm refuses bytes outside CRC-valid frames; decodeRtcmStream reports them", () => {
  const noise = Uint8Array.from([0x00, 0xd3, 0x01, 0x02, 0x03, 0x04]);
  assert.throws(() => decodeRtcm(noise), /6 bytes outside CRC-valid frames/);
  const stream = decodeRtcmStream(noise);
  assert.deepEqual(stream.messages, []);
  assert.equal(stream.diagnostics.resyncBytes, 6);
  assert.equal(stream.diagnostics.crcFailures, 0);
  assert.deepEqual(stream.diagnostics.skippedFrames, []);
  assert.deepEqual(stream.diagnostics.departures, []);
});

// RTCM 3 transport frame around a raw body: preamble, six reserved bits and
// the 10-bit length, the body, and the CRC-24Q of everything before it.
function rawFrame(body, reserved = 0) {
  const head = [0xd3, ((reserved & 0x3f) << 2) | ((body.length >> 8) & 0x03), body.length & 0xff];
  const bytes = [...head, ...body];
  let crc = 0;
  for (const byte of bytes) {
    crc ^= byte << 16;
    for (let i = 0; i < 8; i++) {
      crc <<= 1;
      if (crc & 0x1000000) crc ^= 0x1864cfb;
    }
  }
  crc &= 0xffffff;
  return Uint8Array.from([...bytes, (crc >> 16) & 0xff, (crc >> 8) & 0xff, crc & 0xff]);
}

test("decodeRtcmStream reports resync bytes and skipped CRC-valid frames", () => {
  // A 1077 is decoded into its typed variant, so the encoder refuses to
  // write one as an unsupported body; the truncated frame is framed by hand.
  assert.throws(
    () => encodeRtcmFrame({ type: "unsupported", messageNumber: 1077, body: [0x43, 0x50] }),
    (e) => e.name === "RtcmEncodeError" && /typed variant/.test(e.message),
  );
  const truncatedMsm = rawFrame([0x43, 0x50]);
  const bytes = concatBytes(Uint8Array.from([0x11, 0x22]), truncatedMsm, stream);

  const decoded = decodeRtcmStream(bytes);
  assert.equal(decoded.messages.length, 2);
  assert.equal(decoded.diagnostics.resyncBytes, 2);
  assert.equal(decoded.diagnostics.crcFailures, 0);
  assert.deepEqual(decoded.diagnostics.skippedFrames, [
    {
      offset: 2,
      messageNumber: 1077,
      reason: "truncated",
    },
  ]);
  assert.throws(() => decodeRtcm(bytes), Error);
});

test("frame reserved bits are refused strict and kept lenient", () => {
  const body = Array.from(new FrameScanner(stream).next().body);
  const frame = rawFrame(body, 0x05);
  const strict = decodeRtcmStream(frame);
  assert.equal(strict.messages.length, 0);
  assert.equal(strict.diagnostics.skippedFrames[0].reason, "departure");
  assert.throws(() => decodeRtcmFrame(frame), /reserved bits/);
  const lenient = decodeRtcmStream(frame, "lenient");
  assert.equal(lenient.messages.length, 1);
  assert.equal(lenient.diagnostics.departures.length, 1);
  assert.equal(lenient.diagnostics.departures[0].kind, "frameReservedBits");
  assert.equal(lenient.diagnostics.departures[0].reserved, 5);
  assert.equal(lenient.diagnostics.departures[0].offset, 0);
  const framed = decodeRtcmFrame(frame, "lenient");
  assert.equal(framed.reserved, 5);
  assert.equal(framed.departures[0].kind, "frameReservedBits");
  // Written back with its reserved bits, the frame is the one read.
  assert.deepEqual(
    Array.from(encodeRtcmFrame(framed.message, "lenient", framed.reserved)),
    Array.from(frame),
  );
  assert.throws(() => decodeRtcmStream(frame, "loose"), TypeError);
});

test("RTCM LLI helpers expose lock-time rules and RINEX signal codes", () => {
  assert.deepEqual(rtcmLliBits(), { lossOfLock: 1, halfCycle: 2 });
  assert.equal(rtcmMinimumLockTimeMs("msm7", 64), 64);
  assert.equal(rtcmMinimumLockTimeMs("msm7", 705), undefined);
  assert.equal(rtcmDeriveLli({ minLockTimeMs: 512, elapsedMs: 1000 }, 512, true), 3);
  assert.equal(rtcmMsmEpochDtMs("gps", 604799000, 500), 1500);
  assert.equal(rtcmMsmSignalRinexCode("gps", 2), "1C");
});

test("RtcmLockTimeTracker derives per-cell LLI rows from decoded MSM messages", () => {
  const msm = {
    type: "msm",
    messageNumber: 1077,
    system: "gps",
    kind: "msm7",
    header: {
      referenceStationId: 2003,
      epochTime: 100000,
      multipleMessage: false,
      iods: 0,
      reserved: 0,
      clockSteering: 0,
      externalClock: 0,
      divergenceFreeSmoothing: false,
      smoothingInterval: 0,
    },
    satellites: [
      {
        id: 8,
        roughRangeMs: 75,
        roughRangeMod1: 512,
        extendedInfo: 3,
        roughPhaseRangeRateMS: -100,
      },
    ],
    signals: [
      {
        satelliteId: 8,
        signalId: 2,
        finePseudorange: 1234,
        finePhaseRange: -5678,
        lockTimeIndicator: 200,
        halfCycleAmbiguity: false,
        cnr: 720,
        finePhaseRangeRate: 42,
      },
    ],
  };

  const tracker = new RtcmLockTimeTracker();
  assert.deepEqual(tracker.observe(msm), [
    {
      satelliteId: 8,
      signalId: 2,
      lli: 0,
      minLockTimeMs: 1280,
    },
  ]);

  const slipped = structuredClone(msm);
  slipped.header.epochTime += 1000;
  slipped.signals[0].lockTimeIndicator = 100;
  assert.equal(tracker.observe(slipped)[0].lli, 1);

  tracker.reset();
  assert.equal(tracker.observe(slipped)[0].lli, 0);
});
