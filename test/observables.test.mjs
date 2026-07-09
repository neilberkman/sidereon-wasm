// Observable-domain math through the WASM binding, mirroring
// sidereon-python/tests/test_observables.py. Inputs given as float64 bit
// patterns are reproduced bit-exact; the acquisition "metric" is the one value
// checked with a tolerance, because its golden input signal is built here with
// JS Math.cos/sin, which differs from NumPy's at the ULP level (the binding
// passes the core value through unchanged).

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  carrierFrequencyHz,
  wavelengthM,
  rinexBandFrequencyHz,
  rinexBandWavelengthM,
  glonassG1FrequencyHz,
  defaultSppFrequencyHz,
  defaultPair,
  gnssSystemLetter,
  carrierBandName,
  gamma,
  noiseAmplification,
  ionosphereFree,
  ionosphereFreePhaseM,
  ionosphereFreePhaseCycles,
  ionosphereFreePseudoranges,
  phaseMeters,
  geometryFree,
  wideLaneWavelength,
  narrowLaneCode,
  melbourneWubbena,
  wideLaneCycles,
  detectCycleSlips,
  smoothCode,
  smoothIonoFreeCode,
  slipReasonLabel,
  pseudorangeDropReasonLabel,
  dopplerToRangeRate,
  rangeRateToDoppler,
  solveVelocity,
  pseudorangeVariance,
  sigmas,
  weightVector,
  RaimWeights,
  caCode,
  caChip,
  autocorrelation,
  crossCorrelation,
  correlationAt,
  replica,
  correlate,
  correlateAgainst,
  acquire,
  coherentLoss,
  coherentLossDb,
  snrPostDb,
  loadSp3,
  GnssSystem,
  CarrierBand,
  SlipReason,
  PseudorangeDropReason,
} from "../pkg-node/sidereon.js";

import { fixture, hexToF64, f64Bits, hf } from "./helpers.mjs";

const C = 299792458.0;
const aBits = (arr) => Array.from(arr, f64Bits);
const expectBits = (hexes) => hexes.map((h) => BigInt(h));

const F1 = hexToF64("0x41D779C018000000");
const F2 = hexToF64("0x41D24AEC20000000");

const CARRIER_ARC_ROWS = [
  [
    0,
    "0x419AD7697CF35157",
    "0x4194F2CAD78DD8CA",
    "0x4174689C023D70A4",
    "0x4174689BFD06A506",
    0,
    0,
    "0x41D779C018000000",
    "0x41D24AEC20000000",
  ],
  [
    1,
    "0x419AD771514355CD",
    "0x4194F2D0F16095A4",
    "0x417468A1F420C49C",
    "0x417468A1F5C5F3EE",
    0,
    0,
    "0x41D779C018000000",
    "0x41D24AEC20000000",
  ],
  [
    2,
    "0x419AD779344A2977",
    "0x4194F2D716AA7F95",
    "0x417468A7F9374BC6",
    "0x417468A7F499BDB8",
    0,
    0,
    "0x41D779C018000000",
    "0x41D24AEC20000000",
  ],
  [
    3,
    "0x419AD7812607CC59",
    "0x4194F2DD476B96A2",
    "0x417468ADFFE76C8B",
    "0x417468AE036D8781",
    0,
    0,
    "0x41D779C018000000",
    "0x41D24AEC20000000",
  ],
  [
    4,
    "0x419AD7893E7C3E72",
    "0x4194F2E383A3DAC9",
    "0x417468B41A45A1CB",
    "0x417468B41574847E",
    0,
    0,
    "0x41D779C018000000",
    "0x41D24AEC20000000",
  ],
  [
    5,
    "0x419AD7914DA77FC0",
    "0x4194F2E9CB534C08",
    "0x417468BA38A3D70A",
    "0x417468BA3BA4773D",
    0,
    0,
    "0x41D779C018000000",
    "0x41D24AEC20000000",
  ],
  [
    6,
    "0x419AD7996B899045",
    "0x4194F2F01E79EA62",
    "0x417468C06AD91687",
    "0x417468C066CA2C8C",
    0,
    1,
    "0x41D779C018000000",
    "0x41D24AEC20000000",
  ],
  [
    7,
    "0x419AD7A198226FFF",
    "0x4194F2F67D17B5D4",
    "0x417468C6A06A7EF9",
    "0x417468C6A2BCAEA7",
    0,
    0,
    "0x41D779C018000000",
    "0x41D24AEC20000000",
  ],
  [
    8,
    "0x419AD7A9D3721EF1",
    "0x4194F2FCE72CAE62",
    "0x417468CCE5D2F1A9",
    "0x417468CCE471C01E",
    0,
    0,
    null,
    "0x41D24AEC20000000",
  ],
];

const carrierArc = (rows = CARRIER_ARC_ROWS) =>
  rows.map(([epoch, phi1, phi2, p1, p2, lli1, lli2, f1, f2]) => ({
    phi1Cycles: hexToF64(phi1),
    phi2Cycles: hexToF64(phi2),
    p1M: hexToF64(p1),
    p2M: hexToF64(p2),
    lli1,
    lli2,
    f1Hz: f1 === null ? undefined : hexToF64(f1),
    f2Hz: f2 === null ? undefined : hexToF64(f2),
    gapTimeS: epoch,
  }));

const VELOCITY_OBS_BITS = [
  ["G07", "0xC0768A0B93C45F82"],
  ["G08", "0xC081BBF2879835FD"],
  ["G10", "0xC081C9B51570E844"],
  ["G16", "0xC045EB58A1B7B54E"],
  ["G18", "0x407EC07DD774B2F8"],
  ["G20", "0xC0689F0E9E24FBC3"],
  ["G21", "0x4063A9470C18C1A7"],
  ["G26", "0x4079EF7D9618F6B0"],
  ["G27", "0xC0775231A845D789"],
];

const SIGNAL_PRN1_CHIPS = [
  -1, -1, 1, 1, -1, 1, 1, 1, 1, 1, -1, -1, -1, 1, 1, -1, 1, -1, 1, 1, -1, 1, 1, -1, -1, -1, -1, 1,
  1, -1, 1, -1,
];

const SIGNAL_REPLICA_SAMPLES = [
  1, 1, 1, 1, -1, -1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, -1, -1, 1, 1, 1, 1, -1, -1, -1, -1, 1, 1, -1,
  -1, 1, 1, 1, 1, -1, -1, 1, 1, -1, -1, 1, 1, 1, 1, 1, 1, -1, -1, 1, 1, -1, -1, -1, -1, 1, 1, -1,
  -1, -1, -1, 1, 1, -1, -1,
];

test("carrier frequency constants and default pair", () => {
  assert.equal(gnssSystemLetter(GnssSystem.Gps), "G");
  assert.equal(carrierBandName(CarrierBand.L1), "l1");
  assert.equal(carrierFrequencyHz(GnssSystem.Gps, CarrierBand.L1), 1_575_420_000.0);
  assert.equal(carrierFrequencyHz(GnssSystem.Gps, CarrierBand.L2), 1_227_600_000.0);
  assert.equal(carrierFrequencyHz(GnssSystem.Gps, CarrierBand.E1), undefined);
  assert.equal(defaultSppFrequencyHz(GnssSystem.Gps), 1_575_420_000.0);
  assert.equal(defaultSppFrequencyHz(GnssSystem.Glonass), undefined);
  assert.equal(glonassG1FrequencyHz(-7), 1_602_000_000.0 - 7.0 * 562_500.0);

  const pair = defaultPair(GnssSystem.Gps);
  assert.equal(pair.band1, CarrierBand.L1);
  assert.equal(pair.band2, CarrierBand.L2);
  assert.equal(defaultPair(GnssSystem.Glonass), undefined);
});

test("wavelength is c over frequency", () => {
  assert.equal(wavelengthM(GnssSystem.Gps, CarrierBand.L1), C / 1_575_420_000.0);
});

test("rinex band frequency lookup", () => {
  assert.equal(rinexBandFrequencyHz(GnssSystem.Gps, "1"), 1_575_420_000.0);
  assert.equal(rinexBandFrequencyHz(GnssSystem.Galileo, "5"), 1_176_450_000.0);
  assert.equal(rinexBandFrequencyHz(GnssSystem.Glonass, "1", 1), 1_602_562_500.0);
  assert.equal(rinexBandFrequencyHz(GnssSystem.Glonass, "1"), undefined);
  assert.equal(
    rinexBandWavelengthM(GnssSystem.Glonass, "1", -7),
    C / (1_602_000_000.0 - 7.0 * 562_500.0),
  );
});

test("rinex band requires one character", () => {
  assert.throws(() => rinexBandFrequencyHz(GnssSystem.Gps, "12"), TypeError);
  assert.throws(() => rinexBandWavelengthM(GnssSystem.Gps, ""), TypeError);
});

test("linear combination scalars match the rust oracle bits", () => {
  assert.equal(f64Bits(phaseMeters(123_456_789.25, F1)), 0x4176679b5dbb7fd0n);
  assert.equal(f64Bits(gamma(F1, F2)), 0x40045da686c28e3cn);
  assert.equal(f64Bits(noiseAmplification(F1, F2)), 0x4007d3777c503ebcn);
  assert.equal(
    f64Bits(ionosphereFree(hexToF64("0x4175EF3C40772A36"), hexToF64("0x4175EF3C6A2BCBB5"), F1, F2)),
    0x4175ef3c00000000n,
  );
  assert.equal(
    f64Bits(
      ionosphereFreePhaseM(hexToF64("0x4175F4F80DDD7ECD"), hexToF64("0x4175FD37D057D184"), F1, F2),
    ),
    0x4175e837d93b3cban,
  );
  assert.equal(
    f64Bits(
      ionosphereFreePhaseCycles(
        hexToF64("0x419CD8990A6A993B"),
        hexToF64("0x419682AD3BEA73B9"),
        F1,
        F2,
      ),
    ),
    0x4175e837d93b3cban,
  );
  assert.equal(f64Bits(geometryFree(100.0, 60.0)), 0x4044000000000000n);
  assert.equal(f64Bits(wideLaneWavelength(F1, F2)), 0x3feb94d5e5a6844dn);
  assert.equal(f64Bits(narrowLaneCode(10.0, 12.0, F1, F2)), 0x4025c077975b8fe2n);
  assert.equal(f64Bits(melbourneWubbena(5.0, 3.0, 10.0, 12.0, F1, F2)), 0xc0224ddcdaa6bf58n);
  const wlCycles = hexToF64("0xC0224DDCDAA6BF58") / hexToF64("0x3FEB94D5E5A6844D");
  assert.equal(f64Bits(wideLaneCycles(5.0, 3.0, 10.0, 12.0, F1, F2)), f64Bits(wlCycles));
});

test("linear combination errors are range errors", () => {
  assert.throws(() => gamma(F1, F1), RangeError);
  assert.throws(() => ionosphereFreePhaseCycles(1.0, 2.0, 0.0, F2), RangeError);
  assert.throws(() => phaseMeters(1.0, 0.0), RangeError);
  assert.throws(() => wideLaneWavelength(F1, F1), RangeError);
});

test("cycle slips detect clean and injected arc bits", () => {
  const clean = detectCycleSlips(carrierArc(CARRIER_ARC_ROWS.slice(0, 4)), undefined);
  assert.deepEqual(
    clean.map((r) => r.slip),
    [false, false, false, false],
  );
  assert.deepEqual(
    clean.map((r) => r.skipped),
    [false, false, false, false],
  );

  const actual = detectCycleSlips(carrierArc(), undefined);
  const expected = [
    [false, [], "0xC0E07FD931E60E00", "0xC0F7618A9FB55C00", false],
    [false, [], "0xC0E07FD93C7F8A00", "0xC0F76189F4FDF000", false],
    [false, [], "0xC0E07FD947190400", "0xC0F7618B8B4D9C00", false],
    [false, [], "0xC0E07FD951B28000", "0xC0F76189D67F0600", false],
    [
      true,
      [SlipReason.GeometryFree, SlipReason.MelbourneWubbena],
      "0xC0E07FB4D2FB7200",
      "0xC0F76136A660BE00",
      false,
    ],
    [false, [], "0xC0E07FB4DD94EC00", "0xC0F76136155F9C00", false],
    [true, [SlipReason.Lli], "0xC0E07FB4E82E6800", "0xC0F76137A3E94C00", false],
    [false, [], "0xC0E07FB4F2C7E200", "0xC0F761373E423D00", false],
    [false, [], null, null, true],
  ];

  assert.equal(actual[4].reasons[0], SlipReason.GeometryFree);
  assert.equal(slipReasonLabel(actual[4].reasons[0]), "geometry_free");
  assert.equal(actual[6].reasons[0], SlipReason.Lli);
  assert.equal(actual.length, expected.length);
  actual.forEach((got, i) => {
    const [slip, reasons, gf, mw, skipped] = expected[i];
    assert.equal(got.slip, slip);
    assert.deepEqual(Array.from(got.reasons), reasons);
    assert.equal(got.gfM === undefined ? null : f64Bits(got.gfM), gf === null ? null : BigInt(gf));
    assert.equal(got.mwM === undefined ? null : f64Bits(got.mwM), mw === null ? null : BigInt(mw));
    assert.equal(got.skipped, skipped);
  });
});

test("hatch smoothing matches the rust oracle bits", () => {
  const actual = smoothCode(carrierArc(), undefined, 100);
  const expected = [
    ["0x4174689C023D70A4", 1, false],
    ["0x417468A1F6000000", 2, false],
    ["0x417468A7F7A06D39", 3, false],
    ["0x417468AE02B851EB", 4, false],
    ["0x417468B41A45A1CB", 1, true],
    ["0x417468BA3AAC0831", 2, false],
    ["0x417468C06AD91687", 1, true],
    ["0x417468C6A20C49BA", 2, false],
    [null, 0, false],
  ];
  assert.equal(actual.length, expected.length);
  actual.forEach((got, i) => {
    const [p, window, reset] = expected[i];
    assert.equal(
      got.pSmoothM === undefined ? null : f64Bits(got.pSmoothM),
      p === null ? null : BigInt(p),
    );
    assert.equal(got.window, window);
    assert.equal(got.reset, reset);
  });
});

test("ionosphere-free hatch smoothing matches the rust oracle bits", () => {
  const actual = smoothIonoFreeCode(carrierArc(), undefined, 100);
  const expected = [
    ["0x4174689C0A4CAB98", "0x4174689C0A4CAB98", "0x41746197D93B3CB8", 1, false],
    ["0x417468A1F8BE0026", "0x417468A1F195BB1B", "0x4174619DCED4D652", 2, false],
    ["0x417468A7FBCFC0CC", "0x417468A80059A882", "0x417461A3CFA1A31E", 3, false],
    ["0x417468AE0479118A", "0x417468ADFA7503C6", "0x417461A9DBA1A31E", 4, false],
    ["0x417468B421B7B0F7", "0x417468B421B7B0F7", "0x417461B021565566", 1, true],
    ["0x417468BA3C0EEC2C", "0x417468BA33FFC0F9", "0x417461B643BCBBCE", 2, false],
    ["0x417468C0711EF759", "0x417468C0711EF759", "0x417461BC71565568", 1, true],
    ["0x417468C6A35FE7EF", "0x417468C69CD40BB8", "0x417461C2AA232235", 2, false],
    [null, null, null, 0, false],
  ];
  assert.equal(actual.length, expected.length);
  const bitsOrNull = (v, h) =>
    assert.equal(v === undefined ? null : f64Bits(v), h === null ? null : BigInt(h));
  actual.forEach((got, i) => {
    const [p, pIf, lIf, window, reset] = expected[i];
    bitsOrNull(got.pSmoothM, p);
    bitsOrNull(got.pIfM, pIf);
    bitsOrNull(got.lIfM, lIf);
    assert.equal(got.window, window);
    assert.equal(got.reset, reset);
  });
});

test("doppler range-rate conversions match the formula", () => {
  const fL1 = carrierFrequencyHz(GnssSystem.Gps, CarrierBand.L1);
  assert.equal(dopplerToRangeRate(-1250.0, fL1), (1250.0 * C) / fL1);
  assert.equal(rangeRateToDoppler(42.0, fL1), (-42.0 * fL1) / C);
});

test("velocity range-rate solve matches the rust oracle bits", () => {
  const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const fL1 = carrierFrequencyHz(GnssSystem.Gps, CarrierBand.L1);
  const observations = VELOCITY_OBS_BITS.map(([sat, bits]) => ({
    satelliteId: sat,
    value: hexToF64(bits),
    carrierHz: fL1,
  }));
  const receiver = Float64Array.from([4_500_000.0, 500_000.0, 4_500_000.0]);
  const solution = solveVelocity(sp3, observations, receiver, 646_272_000.0, undefined);

  assert.deepEqual(
    solution.usedSats,
    VELOCITY_OBS_BITS.map(([sat]) => sat),
  );
  assert.equal(solution.velocityMS.length, 3);
  assert.equal(solution.stateCovariance.length, 16);
  assert.ok(Array.from(solution.stateCovariance).every(Number.isFinite));
  for (const idx of [0, 5, 10, 15]) assert.ok(solution.stateCovariance[idx] > 0);
  assert.equal(solution.residualsMS.length, observations.length);
  assert.deepEqual(
    aBits(solution.velocityMS),
    expectBits(["0x4028000000000000", "0xc01c000000000016", "0x4007ffffffffff00"]),
  );
  assert.deepEqual(
    aBits(solution.stateCovariance),
    expectBits([
      "0x3ff0906b12ade753",
      "0xbfd3507feaeb34da",
      "0x3fe4b8aaad393152",
      "0x3e2653d2334473f0",
      "0xbfd3507feaeb34dc",
      "0x3fe06337a5bee55f",
      "0x3f9ceec75f8410a1",
      "0xbdfba852d0276899",
      "0x3fe4b8aaad39314d",
      "0x3f9ceec75f8410c1",
      "0x3ffc72af9d76e44d",
      "0x3e30eae3e3aecb8c",
      "0x3e2653d2334473f2",
      "0xbdfba852d027689e",
      "0x3e30eae3e3aecb8b",
      "0x3c6ae29fdfe7f6ff",
    ]),
  );
  assert.equal(f64Bits(solution.speedMS), 0x402c6ce322982a37n);
  assert.equal(f64Bits(solution.clockDriftSS), 0x3e112e0be826d2een);
  assert.deepEqual(
    aBits(solution.residualsMS),
    expectBits([
      "0xbd01000000000000",
      "0xbd24000000000000",
      "0x3cfc000000000000",
      "0xbd16000000000000",
      "0xbd1a800000000000",
      "0x3cf0000000000000",
      "0xbd14000000000000",
      "0x3d31800000000000",
      "0x3d18000000000000",
    ]),
  );
});

test("velocity doppler solve matches the rust oracle bits", () => {
  const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const fL1 = carrierFrequencyHz(GnssSystem.Gps, CarrierBand.L1);
  const dopplerObs = VELOCITY_OBS_BITS.map(([sat, bits], idx) => {
    const channel = (idx % 14) - 7;
    const carrier = rinexBandFrequencyHz(GnssSystem.Glonass, "1", channel);
    return {
      satelliteId: sat,
      value: rangeRateToDoppler(hexToF64(bits), carrier),
      carrierHz: carrier,
    };
  });
  void fL1;
  const receiver = Float64Array.from([4_500_000.0, 500_000.0, 4_500_000.0]);
  const solution = solveVelocity(sp3, dopplerObs, receiver, 646_272_000.0, {
    observable: "doppler",
  });

  assert.deepEqual(
    aBits(solution.velocityMS),
    expectBits(["0x402800000000000c", "0xc01c00000000000f", "0x4007ffffffffff60"]),
  );
  assert.deepEqual(
    aBits(solution.stateCovariance),
    expectBits([
      "0x3ff0906b12ade753",
      "0xbfd3507feaeb34da",
      "0x3fe4b8aaad393152",
      "0x3e2653d2334473f0",
      "0xbfd3507feaeb34dc",
      "0x3fe06337a5bee55f",
      "0x3f9ceec75f8410a1",
      "0xbdfba852d0276899",
      "0x3fe4b8aaad39314d",
      "0x3f9ceec75f8410c1",
      "0x3ffc72af9d76e44d",
      "0x3e30eae3e3aecb8c",
      "0x3e2653d2334473f2",
      "0xbdfba852d027689e",
      "0x3e30eae3e3aecb8b",
      "0x3c6ae29fdfe7f6ff",
    ]),
  );
  assert.equal(f64Bits(solution.speedMS), 0x402c6ce322982a44n);
  assert.equal(f64Bits(solution.clockDriftSS), 0x3e112e0be826d4b8n);
});

test("ionosphere-free pseudoranges report drop reasons", () => {
  const band1 = [
    { satelliteId: "G01", valueM: 23_000_000.0 },
    { satelliteId: "G01", valueM: 23_000_010.0 },
    { satelliteId: "G02", valueM: 22_000_000.0 },
    { satelliteId: "G03", valueM: 21_000_000.0 },
    { satelliteId: "X01", valueM: 20_000_000.0 },
  ];
  const band2 = [
    { satelliteId: "G01", valueM: 23_000_000.0 },
    { satelliteId: "G02", valueM: 22_000_000.0 },
    { satelliteId: "G04", valueM: 24_000_000.0 },
    { satelliteId: "X01", valueM: 20_000_000.0 },
  ];

  const result = ionosphereFreePseudoranges(band1, band2, undefined);
  assert.deepEqual(result.combinedSats, ["G02"]);
  assert.ok(Math.abs(result.combinedM[0] - 22_000_000.0) < 1e-6);
  assert.deepEqual(result.droppedSats, ["G01", "G03", "G04", "X01"]);
  assert.deepEqual(Array.from(result.droppedReasons), [
    PseudorangeDropReason.DuplicateObservation,
    PseudorangeDropReason.MissingBand2,
    PseudorangeDropReason.MissingBand1,
    PseudorangeDropReason.UnknownSystem,
  ]);
  assert.equal(pseudorangeDropReasonLabel(result.droppedReasons[0]), "duplicate_observation");
});

test("pseudorange override system requires one character", () => {
  assert.throws(
    () =>
      ionosphereFreePseudoranges(
        [{ satelliteId: "G01", valueM: 23_000_000.0 }],
        [{ satelliteId: "G01", valueM: 23_000_000.0 }],
        [{ system: "GPS", band1: "l1", band2: "l2" }],
      ),
    TypeError,
  );
});

test("quality variance and weight vectors match the rust oracle", () => {
  assert.ok(Math.abs(pseudorangeVariance(30.0, undefined) - 0.45) < 1e-15);

  const entries = [
    { satelliteId: "G02", elevationDeg: 15.0 },
    { satelliteId: "G01", elevationDeg: 75.0 },
    { satelliteId: "G03", elevationDeg: 0.0 },
  ];
  const sig = sigmas(entries, undefined);
  const wts = weightVector(entries, undefined);

  assert.deepEqual(sig.satelliteIds, ["G01", "G02"]);
  assert.deepEqual(wts.satelliteIds, sig.satelliteIds);
  assert.equal(sig.values.length, 2);
  assert.equal(wts.values.length, 2);
  assert.ok(sig.values[1] > sig.values[0]);
  assert.ok(wts.values[1] < wts.values[0]);
  for (let i = 0; i < 2; i++) {
    assert.ok(
      Math.abs(wts.values[i] - 1.0 / (sig.values[i] * sig.values[i])) < 1e-15 * wts.values[i],
    );
  }
});

test("quality cn0 model and errors", () => {
  assert.throws(() => pseudorangeVariance(30.0, { model: "elevation_cn0" }), RangeError);
  assert.throws(() => pseudorangeVariance(0.0, undefined), RangeError);

  const weak = pseudorangeVariance(30.0, { model: "elevation_cn0", cn0Dbhz: 30.0 });
  const strong = pseudorangeVariance(30.0, { model: "elevation_cn0", cn0Dbhz: 50.0 });
  assert.ok(strong < weak);
});

test("raim weights expose a sorted float64 vector", () => {
  const unit = RaimWeights.unit();
  assert.ok(unit.isUnit);
  assert.deepEqual(unit.satelliteIds, []);
  assert.equal(unit.weights.length, 0);

  const weights = RaimWeights.bySatellite(["G02", "G01"], Float64Array.from([0.25, 4.0]));
  assert.ok(!weights.isUnit);
  assert.deepEqual(weights.satelliteIds, ["G01", "G02"]);
  assert.deepEqual(Array.from(weights.weights), [4.0, 0.25]);

  assert.throws(() => RaimWeights.bySatellite(["G01"], Float64Array.from([1.0, 2.0])), TypeError);
  assert.throws(() => RaimWeights.bySatellite(["G01"], Float64Array.from([0.0])), RangeError);
});

test("signal ca code and replica match the rust oracle", () => {
  const code = caCode(1n);
  assert.ok(code instanceof Int8Array);
  assert.equal(code.length, 1023);
  assert.deepEqual(Array.from(code.slice(0, SIGNAL_PRN1_CHIPS.length)), SIGNAL_PRN1_CHIPS);
  assert.equal(caChip(1n, -1n), code[code.length - 1]);

  const r = replica(5n, {
    sampleRateHz: hf("0x1.f383000000000p+20"),
    numSamples: 64,
    codePhaseChips: hf("0x1.ff00000000000p+8"),
  });
  assert.ok(r instanceof Int8Array);
  assert.deepEqual(Array.from(r), SIGNAL_REPLICA_SAMPLES);

  assert.throws(() => caCode(33n), RangeError);
});

test("signal code correlation helpers match circular core semantics", () => {
  const codeA = Int8Array.from([1, -1, 1]);
  const codeB = Int8Array.from([1, 1, -1]);

  assert.ok(autocorrelation(codeA) instanceof Int32Array);
  assert.deepEqual(Array.from(autocorrelation(codeA)), [3, -1, -1]);
  assert.deepEqual(Array.from(crossCorrelation(codeA, codeB)), [-1, 3, -1]);
  assert.equal(correlationAt(codeA, codeB, 1n), 3);

  assert.throws(() => crossCorrelation(codeA, Int8Array.from([1, -1])), RangeError);
  assert.throws(() => correlationAt(codeA, codeB, 9223372036854775807n), RangeError);
});

test("signal correlate and acquire match the rust oracle bits", () => {
  const prn = 5n;
  const fs = hf("0x1.f383000000000p+20");
  const doppler = hf("0x1.f400000000000p+9");
  const codePhase = hf("0x1.ff00000000000p+8");

  const clean = (n) => {
    const code = Array.from(
      replica(prn, { sampleRateHz: fs, numSamples: n, codePhaseChips: codePhase }),
      Number,
    );
    const w = (2.0 * Math.PI * doppler) / fs;
    const iq = new Float64Array(n * 2);
    for (let k = 0; k < n; k++) {
      const theta = w * k;
      iq[2 * k] = code[k] * Math.cos(theta);
      iq[2 * k + 1] = code[k] * Math.sin(theta);
    }
    return iq;
  };

  const correlation = correlate(clean(64), prn, {
    sampleRateHz: fs,
    dopplerHz: doppler,
    codePhaseChips: codePhase,
  });
  assert.equal(f64Bits(correlation.i), f64Bits(hf("0x1.0000000000000p+6")));
  assert.equal(f64Bits(correlation.q), f64Bits(hf("0x0.0p+0")));
  assert.equal(f64Bits(correlation.power), f64Bits(hf("0x1.0000000000000p+12")));

  const explicit = correlateAgainst(
    clean(64),
    replica(prn, { sampleRateHz: fs, numSamples: 64, codePhaseChips: codePhase }),
    fs,
    doppler,
  );
  assert.equal(f64Bits(explicit.i), f64Bits(hf("0x1.0000000000000p+6")));
  assert.equal(f64Bits(explicit.q), f64Bits(hf("0x0.0p+0")));
  assert.equal(f64Bits(explicit.power), f64Bits(hf("0x1.0000000000000p+12")));

  const acquisition = acquire(clean(2046), prn, { sampleRateHz: fs });
  assert.equal(f64Bits(acquisition.codePhaseChips), f64Bits(codePhase));
  assert.equal(f64Bits(acquisition.dopplerHz), f64Bits(doppler));
  assert.equal(f64Bits(acquisition.peakPower), f64Bits(hf("0x1.ff00200000000p+21")));
  assert.equal(f64Bits(acquisition.peakMetric), f64Bits(acquisition.metric));
  // The metric's golden input signal is built here with JS cos/sin, which
  // differs from NumPy at the ULP level, so this single value is tolerance
  // checked rather than bit-exact.
  assert.ok(Math.abs(acquisition.metric - hexToF64("0x409369E276358FF0")) < 1e-9);
  assert.equal(acquisition.grid.codePhaseBins, 2046);
  assert.equal(f64Bits(acquisition.grid.samplesPerChip), f64Bits(hf("0x1.0p+1")));
  assert.equal(acquisition.grid.dopplerStepHz, 500.0);
  const dopplerBins = [];
  for (let v = -2500.0; v <= 2500.0 + 1e-9; v += 500.0) dopplerBins.push(v);
  assert.deepEqual(Array.from(acquisition.grid.dopplerHz), dopplerBins);

  assert.throws(() => correlate(Float64Array.from([1, 2, 3]), prn), TypeError);
  assert.throws(
    () => correlateAgainst(Float64Array.from([1, 2]), Int8Array.from([]), fs, 0),
    RangeError,
  );
});

test("signal loss helpers match the rust oracle bits", () => {
  const cases = [
    ["0x0.0p+0", "0x1.0624dd2f1a9fcp-10", "0x1.0000000000000p+0", "0x0.0p+0"],
    [
      "0x1.f400000000000p+7",
      "0x1.0624dd2f1a9fcp-10",
      "0x1.9f02f6222c71fp-1",
      "-0x1.d2fe745bc3f62p-1",
    ],
    [
      "0x1.f400000000000p+8",
      "0x1.0624dd2f1a9fcp-9",
      "0x1.f8f7171d21750p-110",
      "-0x1.482ecab293e4ep+8",
    ],
    [
      "0x1.ec00000000000p+6",
      "0x1.0624dd2f1a9fcp-8",
      "0x1.ac58d00563d6dp-2",
      "-0x1.e47c4a524edbdp+1",
    ],
  ];
  for (const [freqErr, intTime, loss, lossDb] of cases) {
    assert.equal(f64Bits(coherentLoss(hf(freqErr), hf(intTime))), f64Bits(hf(loss)));
    assert.equal(f64Bits(coherentLossDb(hf(freqErr), hf(intTime))), f64Bits(hf(lossDb)));
  }
  assert.equal(f64Bits(snrPostDb(40.0, 1.0e-3)), f64Bits(10.0));
});
