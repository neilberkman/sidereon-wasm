// RTCM 3.x construction + encode bindings over sidereon_core::rtcm::Message::
// {encode, to_frame}, the inverse of the decode wrappers. Each supported message
// family is built from a plain `type`-tagged JS object, encoded into a transport
// frame, and decoded back; the recovered field integers match what was
// constructed. encodeRtcmFrame -> decodeRtcmFrame is the round-trip.

import { test } from "node:test";
import assert from "node:assert/strict";

import { decodeRtcm, decodeRtcmFrame, encodeRtcm, encodeRtcmFrame } from "../pkg-node/sidereon.js";
import { fixtureJson } from "./helpers.mjs";

const hexToBytes = (hex) => Uint8Array.from(hex.match(/.{2}/g).map((b) => parseInt(b, 16)));

test("a decoded 1006 + 1019 stream re-encodes and round-trips byte-for-byte", () => {
  const fx = fixtureJson("rtcm.json");
  const stream = hexToBytes(fx.stream);
  const messages = decodeRtcm(stream);
  assert.deepEqual(
    messages.map((m) => m.type),
    ["stationCoordinates", "gpsEphemeris"],
  );

  for (const message of messages) {
    const frame = encodeRtcmFrame(message);
    assert.ok(frame instanceof Uint8Array);
    const back = decodeRtcmFrame(frame);
    assert.equal(back.message.type, message.type);
    assert.equal(back.message.messageNumber, message.messageNumber);
  }

  // The GPS ephemeris large raw fields survive the construct -> encode -> decode
  // trip as exact BigInts.
  const eph = messages[1];
  const ephBack = decodeRtcmFrame(encodeRtcmFrame(eph)).message;
  assert.equal(ephBack.m0, eph.m0);
  assert.equal(ephBack.sqrtA, eph.sqrtA);
  assert.equal(ephBack.eccentricity, eph.eccentricity);
  assert.equal(ephBack.satelliteId, eph.satelliteId);
});

test("a 1005 station message built from scratch round-trips", () => {
  const station = {
    type: "stationCoordinates",
    messageNumber: 1005,
    referenceStationId: 2003,
    itrfRealizationYear: 0,
    gpsIndicator: true,
    glonassIndicator: false,
    galileoIndicator: false,
    referenceStationIndicator: false,
    ecefX: 11446021400n,
    singleReceiverOscillator: false,
    reserved: false,
    ecefY: -4717211900n,
    quarterCycleIndicator: 0,
    ecefZ: 4296881700n,
  };
  // encodeRtcm yields the body; encodeRtcmFrame wraps it in a transport frame.
  assert.ok(encodeRtcm(station) instanceof Uint8Array);
  const back = decodeRtcmFrame(encodeRtcmFrame(station)).message;
  assert.equal(back.type, "stationCoordinates");
  assert.equal(back.messageNumber, 1005);
  assert.equal(back.referenceStationId, 2003);
  assert.equal(back.ecefX, 11446021400n);
  assert.equal(back.ecefY, -4717211900n);
  assert.equal(back.ecefZ, 4296881700n);
  // 1005 carries no antenna height.
  assert.equal(back.antennaHeight, undefined);
});

test("a 1007 antenna descriptor built from scratch round-trips", () => {
  const antenna = {
    type: "antennaDescriptor",
    messageNumber: 1007,
    referenceStationId: 5,
    antennaDescriptor: "TRM59800.00",
    antennaSetupId: 1,
  };
  const back = decodeRtcmFrame(encodeRtcmFrame(antenna)).message;
  assert.equal(back.type, "antennaDescriptor");
  assert.equal(back.messageNumber, 1007);
  assert.equal(back.antennaDescriptor, "TRM59800.00");
  assert.equal(back.antennaSetupId, 1);
});

test("a 1020 GLONASS ephemeris built from scratch round-trips", () => {
  const glonass = {
    type: "glonassEphemeris",
    satelliteId: 3,
    frequencyChannel: 5,
    almanacHealth: true,
    almanacHealthAvailability: true,
    p1: 0,
    tK: 100,
    bNMsb: false,
    p2: false,
    tB: 10,
    xnDot: 1000,
    xn: 2000,
    xnDotDot: 1,
    ynDot: -1000,
    yn: -2000,
    ynDotDot: -1,
    znDot: 500,
    zn: 1500,
    znDotDot: 0,
    p3: false,
    gammaN: 5,
    mP: 1,
    mLNThird: false,
    tauN: 123,
    deltaTauN: 2,
    eN: 7,
    mP4: false,
    mFT: 3,
    mNT: 50,
    mM: 1,
    additionalDataAvailable: false,
    nA: 0,
    tauC: 9999n,
    mN4: 1,
    mTauGps: 42,
    mLNFifth: false,
    reserved: 0,
  };
  const back = decodeRtcmFrame(encodeRtcmFrame(glonass)).message;
  assert.equal(back.type, "glonassEphemeris");
  assert.equal(back.messageNumber, 1020);
  assert.equal(back.satelliteId, 3);
  assert.equal(back.frequencyChannel, 5);
  assert.equal(back.xn, 2000);
  assert.equal(back.tauC, 9999n);
});

test("an MSM4 observation message built from scratch round-trips", () => {
  const msm = {
    type: "msm",
    messageNumber: 1074,
    system: "gps",
    kind: "msm4",
    header: {
      referenceStationId: 0,
      epochTime: 1000,
      multipleMessage: false,
      iods: 0,
      reserved: 0,
      clockSteering: 0,
      externalClock: 0,
      divergenceFreeSmoothing: false,
      smoothingInterval: 0,
    },
    satellites: [{ id: 5, roughRangeMs: 67, roughRangeMod1: 512 }],
    signals: [
      {
        satelliteId: 5,
        signalId: 2,
        finePseudorange: 100,
        finePhaseRange: -200,
        lockTimeIndicator: 0,
        halfCycleAmbiguity: false,
        cnr: 40,
      },
    ],
  };
  const back = decodeRtcmFrame(encodeRtcmFrame(msm)).message;
  assert.equal(back.type, "msm");
  assert.equal(back.messageNumber, 1074);
  assert.equal(back.system, "gps");
  assert.equal(back.kind, "msm4");
  assert.equal(back.satellites.length, 1);
  assert.equal(back.signals.length, 1);
  assert.equal(back.satellites[0].id, 5);
  assert.equal(back.satellites[0].roughRangeMs, 67);
  assert.equal(back.satellites[0].roughRangeMod1, 512);
  assert.equal(back.signals[0].signalId, 2);
  assert.equal(back.signals[0].finePseudorange, 100);
  assert.equal(back.signals[0].finePhaseRange, -200);
  assert.equal(back.signals[0].cnr, 40);
});

test("encodeRtcmFrame rejects a malformed message object", () => {
  assert.throws(() => encodeRtcmFrame({ type: "stationCoordinates" }));
  assert.throws(() => encodeRtcmFrame({ notAType: true }));
});
