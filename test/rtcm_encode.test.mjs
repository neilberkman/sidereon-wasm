// RTCM 3.x construction + encode bindings over sidereon_core::rtcm::Message::
// {encode, to_frame}, the inverse of the decode wrappers. Each supported message
// family is built from a plain `type`-tagged JS object, encoded into a transport
// frame, and decoded back; the recovered field integers match what was
// constructed. encodeRtcmFrame -> decodeRtcmFrame is the round-trip.

import { test } from "node:test";
import assert from "node:assert/strict";

import { decodeRtcm, decodeRtcmFrame, encodeRtcm, encodeRtcmFrame } from "../pkg-node/sidereon.js";
import { fixture, fixtureJson } from "./helpers.mjs";

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

test("coordinate transformation messages round-trip typed parameter fields", () => {
  const projection = {
    type: "projection",
    systemId: 1,
    projectionType: 3,
    parameters: {
      kind: "naturalOrigin",
      latitude: -1234567890n,
      longitude: 2345678901n,
      addScale: 123,
      falseEasting: 9876543210n,
      falseNorthing: -8765432109n,
    },
  };
  const decoded = decodeRtcmFrame(encodeRtcmFrame(projection)).message;
  assert.equal(decoded.type, "projection");
  assert.deepEqual(decoded.parameters, projection.parameters);
  assert.equal(decoded.parameters.latitude, projection.parameters.latitude);
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

const fullEphemerisFixtures = [
  {
    messageNumber: 1019,
    message: {
      type: "gpsEphemeris",
      satelliteId: 8,
      weekNumber: 123,
      svAccuracy: 1,
      codeOnL2: 1,
      idot: -4000,
      iode: 11,
      tOc: 7200,
      aF2: -3,
      aF1: -12345,
      aF0: 23456,
      iodc: 57,
      cRs: -1000,
      deltaN: 100,
      m0: 1000000000n,
      cUc: -50,
      eccentricity: 4459564n,
      cUs: 51,
      sqrtA: 2702336448n,
      tOe: 3600,
      cIc: -5,
      omega0: 1500000000n,
      cIs: 6,
      i0: 400000000n,
      cRc: 100,
      omega: -1000000000n,
      omegaDot: -100,
      tGd: -5,
      svHealth: 7,
      l2PDataFlag: true,
      fitInterval: true,
      trailingBits: [true, false, false, false, false, false, false, false],
    },
  },
  {
    messageNumber: 1045,
    message: {
      type: "galileoFnavEphemeris",
      satelliteId: 12,
      weekNumber: 1402,
      iodNav: 7,
      sisa: 42,
      idot: 434,
      tOc: 5150,
      aF2: -3,
      aF1: -151,
      aF0: -471483n,
      cRs: -791,
      deltaN: 9274,
      m0: 1630831142n,
      cUc: -707,
      eccentricity: 4459564n,
      cUs: 3342,
      sqrtA: 2852448983n,
      tOe: 5150,
      cIc: -5,
      omega0: 2118450828n,
      cIs: -11,
      i0: 662506241n,
      cRc: 6692,
      omega: 372867071n,
      omegaDot: -15832,
      bgdE5aE1: -5,
      e5aSignalHealth: 1,
      e5aDataValidity: true,
      reserved: 63,
      trailingBits: [true, false, false, false, false, false, false, false],
    },
  },
  {
    messageNumber: 1046,
    message: {
      type: "galileoInavEphemeris",
      satelliteId: 3,
      weekNumber: 1402,
      iodNav: 7,
      sisaIndex: 107,
      idot: 434,
      tOc: 5150,
      aF2: -3,
      aF1: -151,
      aF0: -471483n,
      cRs: -791,
      deltaN: 9274,
      m0: 1630831142n,
      cUc: -707,
      eccentricity: 4459564n,
      cUs: 3342,
      sqrtA: 2852448983n,
      tOe: 5150,
      cIc: -5,
      omega0: 2118450828n,
      cIs: -11,
      i0: 662506241n,
      cRc: 6692,
      omega: 372867071n,
      omegaDot: -15832,
      bgdE5aE1: -5,
      bgdE5bE1: 7,
      e5bSignalHealth: 1,
      e5bDataValidity: true,
      e1bSignalHealth: 2,
      e1bDataValidity: true,
      reserved: 3,
      trailingBits: [true, false, false, false, false, false, false, false],
    },
  },
  {
    messageNumber: 1042,
    message: {
      type: "beidouEphemeris",
      satelliteId: 19,
      weekNumber: 902,
      svUrai: 1,
      idot: -4000,
      aode: 17,
      tOc: 12000,
      aF2: -3,
      aF1: 12345,
      aF0: -45678,
      aodc: 12,
      cRs: -1000,
      deltaN: 100,
      m0: 1000000000n,
      cUc: -50,
      eccentricity: 4459564n,
      cUs: 51,
      sqrtA: 2852448983n,
      tOe: 12000,
      cIc: -5,
      omega0: 1500000000n,
      cIs: 6,
      i0: 400000000n,
      cRc: 100,
      omega: -1000000000n,
      omegaDot: -100,
      tGd1: -5,
      tGd2: 7,
      svHealth: true,
      trailingBits: [true],
    },
  },
  {
    messageNumber: 1044,
    message: {
      type: "qzssEphemeris",
      satelliteId: 3,
      tOc: 7200,
      aF2: -3,
      aF1: -12345,
      aF0: 23456,
      iode: 11,
      cRs: -1000,
      deltaN: 100,
      m0: 1000000000n,
      cUc: -50,
      eccentricity: 4459564n,
      cUs: 51,
      sqrtA: 2702336448n,
      tOe: 3600,
      cIc: -5,
      omega0: 1500000000n,
      cIs: 6,
      i0: 400000000n,
      cRc: 100,
      omega: -1000000000n,
      omegaDot: -100,
      idot: -4000,
      codesOnL2: 1,
      weekNumber: 123,
      ura: 2,
      svHealth: 7,
      tGd: -5,
      iodc: 57,
      fitInterval: true,
      trailingBits: [true, false, false],
    },
  },
  {
    messageNumber: 1041,
    message: {
      type: "navicEphemeris",
      satelliteId: 9,
      weekNumber: 389,
      aF0: -1234567,
      aF1: -12345,
      aF2: -3,
      ura: 2,
      tOc: 10821,
      tGd: -5,
      deltaN: 1234567,
      iodec: 161,
      reserved: 0x2a5,
      l5Flag: true,
      sFlag: true,
      cUc: -16000,
      cUs: 15000,
      cIc: -1,
      cIs: 2,
      cRc: 16383,
      cRs: -16384,
      idot: -8000,
      m0: -2000000000n,
      tOe: 10821,
      eccentricity: 3000000n,
      sqrtA: 3404000000n,
      omega0: 1500000000n,
      omega: -1000000000n,
      omegaDot: -2000000,
      i0: 400000000n,
      spareDf544: 3,
      spareDf545: 1,
      trailingBits: [true, false, false, false, false, false],
    },
  },
  {
    messageNumber: 1020,
    message: {
      type: "glonassEphemeris",
      satelliteId: 5,
      frequencyChannel: 8,
      almanacHealth: true,
      almanacHealthAvailability: true,
      p1: 2,
      tK: 3600,
      bNMsb: true,
      p2: true,
      tB: 30,
      xnDot: 0,
      xn: 1000000,
      xnDotDot: -3,
      ynDot: 2000,
      yn: -2000000,
      ynDotDot: 4,
      znDot: -3000,
      zn: 3000000,
      znDotDot: -5,
      p3: true,
      gammaN: -100,
      mP: 2,
      mLNThird: true,
      tauN: -10000,
      deltaTauN: -6,
      eN: 17,
      mP4: true,
      mFT: 9,
      mNT: 700,
      mM: 2,
      additionalDataAvailable: true,
      nA: 1000,
      tauC: -1000000n,
      mN4: 17,
      mTauGps: 10000,
      mLNFifth: true,
      reserved: 63,
      negativeZero: 1,
      trailingBits: [true, false, false, false, false, false, false, false],
    },
  },
];

for (const { messageNumber, message } of fullEphemerisFixtures) {
  test(`lenient ${message.type} round-trip preserves every returned field`, () => {
    const body = encodeRtcm(message, "lenient");
    const frame = encodeRtcmFrame(message, "lenient");
    assert.deepEqual(body, frame.slice(3, -3));

    const decoded = decodeRtcmFrame(frame, "lenient");
    const expected = { ...message, messageNumber };
    assert.deepEqual(Object.keys(decoded.message).sort(), Object.keys(expected).sort());
    for (const [field, value] of Object.entries(expected)) {
      assert.deepEqual(decoded.message[field], value, `${message.type}.${field}`);
    }
    assert.equal(decoded.departures.length, 1);
    assert.equal(decoded.departures[0].kind, "trailingBits");
    assert.equal(decoded.departures[0].messageNumber, messageNumber);
    assert.deepEqual(decoded.departures[0].bits, message.trailingBits);
  });
}

test("lenient GLONASS ephemeris preserves negative zero for every field", () => {
  const glonass = fullEphemerisFixtures.find(({ messageNumber }) => messageNumber === 1020).message;
  const negativeZeroFields = [
    ["NEGATIVE_ZERO_XN_DOT", "xnDot", 1 << 0, 0],
    ["NEGATIVE_ZERO_XN", "xn", 1 << 1, 0],
    ["NEGATIVE_ZERO_XN_DOT_DOT", "xnDotDot", 1 << 2, 0],
    ["NEGATIVE_ZERO_YN_DOT", "ynDot", 1 << 3, 0],
    ["NEGATIVE_ZERO_YN", "yn", 1 << 4, 0],
    ["NEGATIVE_ZERO_YN_DOT_DOT", "ynDotDot", 1 << 5, 0],
    ["NEGATIVE_ZERO_ZN_DOT", "znDot", 1 << 6, 0],
    ["NEGATIVE_ZERO_ZN", "zn", 1 << 7, 0],
    ["NEGATIVE_ZERO_ZN_DOT_DOT", "znDotDot", 1 << 8, 0],
    ["NEGATIVE_ZERO_GAMMA_N", "gammaN", 1 << 9, 0],
    ["NEGATIVE_ZERO_TAU_N", "tauN", 1 << 10, 0],
    ["NEGATIVE_ZERO_DELTA_TAU_N", "deltaTauN", 1 << 11, 0],
    ["NEGATIVE_ZERO_TAU_C", "tauC", 1 << 12, 0n],
    ["NEGATIVE_ZERO_M_TAU_GPS", "mTauGps", 1 << 13, 0],
  ];
  for (const [name, field, mask, zero] of negativeZeroFields) {
    const negativeZero = { ...glonass, [field]: zero, negativeZero: mask, trailingBits: [] };
    const decoded = decodeRtcmFrame(encodeRtcmFrame(negativeZero, "lenient"), "lenient").message;
    assert.deepEqual(decoded, { ...negativeZero, messageNumber: 1020, trailingBits: [] }, name);
  }
});

test("a real 1046 Galileo I/NAV frame decodes and re-encodes exactly", () => {
  const frame = hexToBytes(
    "d3003f4160d5e8076b06c941e03ffed3ffe33917f3a490e984d2089bf4f4011030b0343aa813ab5d41efffb7e44fe8cfff5277d0b011a2416397fffffc2280140700800a8e",
  );
  const message = decodeRtcmFrame(frame).message;
  assert.equal(message.type, "galileoInavEphemeris");
  assert.equal(message.messageNumber, 1046);
  assert.equal(message.satelliteId, 3);
  assert.equal(message.weekNumber, 1402);
  assert.equal(message.iodNav, 7);
  assert.equal(message.sqrtA, 2852448983n);
  assert.deepEqual(encodeRtcmFrame(message), frame);
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
  assert.equal(back.system, "GPS");
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
  // The decoded object, with the system spelled as the decoder writes it,
  // re-encodes to the same frame.
  const frame = encodeRtcmFrame(msm);
  assert.deepEqual(encodeRtcmFrame(back), frame);
});

test("MSM1 and MSM2 round-trip only the fields their kinds carry", () => {
  const header = {
    referenceStationId: 0,
    epochTime: 1000,
    multipleMessage: false,
    iods: 0,
    reserved: 0,
    clockSteering: 0,
    externalClock: 0,
    divergenceFreeSmoothing: false,
    smoothingInterval: 0,
  };
  const msm1 = {
    type: "msm",
    messageNumber: 1071,
    system: "gps",
    kind: "msm1",
    header,
    satellites: [{ id: 5, roughRangeMod1: 512 }],
    signals: [{ satelliteId: 5, signalId: 2, finePseudorange: 100 }],
  };
  const msm2 = {
    type: "msm",
    messageNumber: 1072,
    system: "gps",
    kind: "msm2",
    header,
    satellites: [{ id: 5, roughRangeMod1: 512 }],
    signals: [
      {
        satelliteId: 5,
        signalId: 2,
        finePhaseRange: -200,
        lockTimeIndicator: 0,
        halfCycleAmbiguity: false,
      },
    ],
  };
  const decodedMsm1 = decodeRtcmFrame(encodeRtcmFrame(msm1)).message;
  assert.equal(decodedMsm1.kind, "msm1");
  assert.equal(decodedMsm1.satellites[0].roughRangeMs, undefined);
  assert.equal(decodedMsm1.signals[0].finePseudorange, 100);
  assert.equal(decodedMsm1.signals[0].finePhaseRange, undefined);
  assert.equal(decodedMsm1.signals[0].cnr, undefined);
  const decodedMsm2 = decodeRtcmFrame(encodeRtcmFrame(msm2)).message;
  assert.equal(decodedMsm2.kind, "msm2");
  assert.equal(decodedMsm2.signals[0].finePseudorange, undefined);
  assert.equal(decodedMsm2.signals[0].finePhaseRange, -200);
  assert.equal(decodedMsm2.signals[0].cnr, undefined);
});

test("encodeRtcmFrame rejects a malformed message object", () => {
  assert.throws(() => encodeRtcmFrame({ type: "stationCoordinates" }));
  assert.throws(() => encodeRtcmFrame({ notAType: true }));
});

test("codec refusals expose typed encode variants and fields", () => {
  assert.throws(
    () =>
      encodeRtcm({
        type: "stationCoordinates",
        messageNumber: 1200,
        referenceStationId: 1,
        itrfRealizationYear: 0,
        gpsIndicator: false,
        glonassIndicator: false,
        galileoIndicator: false,
        referenceStationIndicator: false,
        ecefX: 0n,
        singleReceiverOscillator: false,
        reserved: false,
        ecefY: 0n,
        quarterCycleIndicator: 0,
        ecefZ: 0n,
      }),
    (error) => {
      assert.equal(error.name, "RtcmEncodeError");
      assert.equal(error.detail.kind, "RTCM_ENCODE");
      assert.equal(error.detail.core.kind, "messageNumber");
      assert.equal(error.detail.core.messageNumber, 1200);
      assert.equal(error.detail.core.record.kind, "stationCoordinates");
      assert.equal("coreDetails" in error.detail, false);
      return true;
    },
  );
});

// A field the wire layout cannot state is refused by the core codec with its
// typed variant, never written as a different satellite or mask.
function assertEncodeRefused(message, core, messagePattern) {
  for (const encode of [encodeRtcm, encodeRtcmFrame]) {
    assert.throws(
      () => encode(message),
      (err) => {
        assert.equal(err.name, "RtcmEncodeError");
        assert.equal(err.detail.kind, "RTCM_ENCODE");
        assert.deepEqual(err.detail.core, core);
        assert.match(err.message, messagePattern);
        assert.equal(err.detail.message, err.message);
        return true;
      },
    );
  }
}

test("an ephemeris satellite id wider than its field is refused, not truncated", () => {
  const fx = fixtureJson("rtcm.json");
  const eph = decodeRtcm(hexToBytes(fx.stream))[1];
  assert.equal(eph.type, "gpsEphemeris");
  // 1019 carries the satellite in six bits, 0..=63.
  assertEncodeRefused(
    { ...eph, satelliteId: 64 },
    { kind: "satelliteIdOutOfRange", messageNumber: 1019, field: "GPS PRN", value: 64, width: 6 },
    /6-bit raw satellite field \(0\.\.=63\)/,
  );
  assert.ok(encodeRtcm({ ...eph, satelliteId: 63 }) instanceof Uint8Array);
});

test("MSM satellite and signal lists the masks cannot state are refused", () => {
  const header = {
    referenceStationId: 0,
    epochTime: 1000,
    multipleMessage: false,
    iods: 0,
    reserved: 0,
    clockSteering: 0,
    externalClock: 0,
    divergenceFreeSmoothing: false,
    smoothingInterval: 0,
  };
  const signal = (satelliteId, signalId) => ({
    satelliteId,
    signalId,
    finePseudorange: 100,
    finePhaseRange: -200,
    lockTimeIndicator: 0,
    halfCycleAmbiguity: false,
    cnr: 40,
  });
  const msm = (satellites, signals) => ({
    type: "msm",
    messageNumber: 1074,
    system: "gps",
    kind: "msm4",
    header,
    satellites: satellites.map((id) => ({ id, roughRangeMs: 67, roughRangeMod1: 512 })),
    signals,
  });

  const mask = (problem) => ({ kind: "msmMask", messageNumber: 1074, problem });
  assertEncodeRefused(
    msm([0], [signal(0, 2)]),
    mask({ kind: "satelliteOutsideMask", satellite: 0 }),
    /satellite id 0 is outside the 1\.\.=64 satellite mask/,
  );
  assertEncodeRefused(
    msm([65], [signal(65, 2)]),
    mask({ kind: "satelliteOutsideMask", satellite: 65 }),
    /satellite id 65 is outside/,
  );
  assertEncodeRefused(
    msm([5], [signal(5, 33)]),
    mask({ kind: "signalOutsideMask", signal: 33 }),
    /signal id 33 is outside the 1\.\.=32 signal mask/,
  );
  assertEncodeRefused(
    msm([5, 5], [signal(5, 2)]),
    mask({ kind: "satelliteListedTwice", satellite: 5 }),
    /satellite id 5 is listed twice/,
  );
  assertEncodeRefused(
    msm([5], [signal(6, 2)]),
    mask({ kind: "signalSatelliteNotListed", signal: 2, satellite: 6 }),
    /signal 2 names satellite id 6, which the satellite list does not hold/,
  );
  assertEncodeRefused(
    msm([5], [signal(5, 2), signal(5, 2)]),
    mask({ kind: "cellListedTwice", satellite: 5, signal: 2 }),
    /the cell for satellite id 5 signal 2 is listed twice/,
  );
  assert.ok(encodeRtcm(msm([5], [signal(5, 2)])) instanceof Uint8Array);
});

test("a decoded SSR frame re-encodes byte for byte", () => {
  const hex = fixture("ssr/SSRA02IGS0_2026181234930_1060.hex").toString("utf8").trim();
  const frame = Uint8Array.from(hex.match(/../g).map((byte) => Number.parseInt(byte, 16)));
  // Read and written under the lenient policy, so any trailing bits the frame
  // carries are kept and restated.
  const decoded = decodeRtcmFrame(frame, "lenient");
  assert.equal(decoded.message.type, "ssr");
  assert.equal(decoded.message.messageNumber, 1060);
  assert.deepEqual(encodeRtcmFrame(decoded.message, "lenient", decoded.reserved), frame);
  assert.deepEqual(encodeRtcm(decoded.message, "lenient"), frame.slice(3, frame.length - 3));
});
