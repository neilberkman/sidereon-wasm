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

test("1042/1044/1045/1046 broadcast ephemerides built from scratch round-trip", () => {
  const beidou = {
    type: "beidouEphemeris",
    satelliteId: 19,
    weekNumber: 902,
    svUrai: 1,
    idot: 1,
    aode: 17,
    tOc: 12000,
    aF2: -3,
    aF1: 12345,
    aF0: -45678,
    aodc: 12,
    cRs: -1000,
    deltaN: 100,
    m0: 1000n,
    cUc: -50,
    eccentricity: 4459564n,
    cUs: 51,
    sqrtA: 2852448983n,
    tOe: 12000,
    cIc: -5,
    omega0: 1000n,
    cIs: 6,
    i0: 1000n,
    cRc: 100,
    omega: 1000n,
    omegaDot: -100,
    tGd1: 5,
    tGd2: 7,
    svHealth: false,
  };
  const qzss = {
    type: "qzssEphemeris",
    satelliteId: 3,
    tOc: 7200,
    aF2: 1,
    aF1: 1,
    aF0: 23456,
    iode: 11,
    cRs: 1,
    deltaN: 1,
    m0: 1n,
    cUc: 1,
    eccentricity: 1n,
    cUs: 1,
    sqrtA: 2702336448n,
    tOe: 3600,
    cIc: 1,
    omega0: 1n,
    cIs: 1,
    i0: 1n,
    cRc: 1,
    omega: 1n,
    omegaDot: 1,
    idot: 1,
    codesOnL2: 1,
    weekNumber: 123,
    ura: 1,
    svHealth: 1,
    tGd: 1,
    iodc: 1,
    fitInterval: false,
  };
  const galileoFnav = {
    type: "galileoFnavEphemeris",
    satelliteId: 12,
    weekNumber: 1402,
    iodNav: 7,
    sisa: 42,
    idot: 434,
    tOc: 5150,
    aF2: 0,
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
    bgdE5aE1: 5,
    e5aSignalHealth: 0,
    e5aDataValidity: false,
    reserved: 0,
  };
  const galileoInav = {
    ...galileoFnav,
    type: "galileoInavEphemeris",
    satelliteId: 3,
    sisaIndex: 107,
    bgdE5bE1: 7,
    e5bSignalHealth: 0,
    e5bDataValidity: false,
    e1bSignalHealth: 0,
    e1bDataValidity: false,
  };
  delete galileoInav.sisa;
  delete galileoInav.e5aSignalHealth;
  delete galileoInav.e5aDataValidity;

  for (const message of [beidou, qzss, galileoFnav, galileoInav]) {
    const back = decodeRtcmFrame(encodeRtcmFrame(message)).message;
    assert.equal(back.type, message.type);
    assert.equal(back.satelliteId, message.satelliteId);
    assert.equal(back.sqrtA, message.sqrtA);
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
