import { test } from "node:test";
import assert from "node:assert/strict";

import {
  loadSp3,
  mergeSp3,
  openPreciseInterpolantArtifact,
  preciseEphemerisSamplesFromSamples,
  sp3PreciseEphemerisSamples,
  PreciseEphemerisInterpolant,
} from "../pkg-node/sidereon.js";
import { fixture } from "./helpers.mjs";

const MID_HOLE_J2000_S = 646_260_300.0;

function captureThrow(fn, expected) {
  let thrown;
  assert.throws(fn, (error) => {
    thrown = error;
    return expected === undefined || expected.test(error.message);
  });
  return thrown;
}

function gappedSp3Bytes() {
  const text = fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3").toString("utf8");
  const lines = text.split("\n");
  const gappedLines = [];
  let inGap = false;
  for (const line of lines) {
    if (line.startsWith("*  2020  6 24  7 30")) inGap = true;
    if (line.startsWith("*  2020  6 24 10 15")) inGap = false;
    if (inGap && line.startsWith("PG01")) continue;
    gappedLines.push(line);
  }
  return new TextEncoder().encode(gappedLines.join("\n"));
}

test("Sp3 public DTO routes retain literal fixture metadata and state values", () => {
  const source = fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3");
  const sp3 = loadSp3(source);

  assert.equal(sp3.epochCount, 96);
  assert.equal(sp3.declaredEpochCount, 96);
  assert.equal(sp3.declaredStartJ2000Seconds, 646228800);

  const header = sp3.header;
  assert.deepEqual(header, {
    version: "c",
    dataType: "position",
    numEpochs: 96,
    dataUsed: "TRACK",
    coordinateSystem: "IGb14",
    orbitType: "FIT",
    agency: "GRGS",
    gnssWeek: 2111,
    secondsOfWeek: 259200,
    epochIntervalS: 900,
    mjd: 59024,
    mjdFraction: 0,
    fileType: "M",
    timeSystem: "GPS",
    timeScale: "gpst",
    posVelBase: 0,
    clockRateBase: 0,
    satellites: [
      "E01",
      "E02",
      "E03",
      "E04",
      "E05",
      "E07",
      "E08",
      "E09",
      "E11",
      "E12",
      "E13",
      "E14",
      "E15",
      "E18",
      "E19",
      "E21",
      "E24",
      "E25",
      "E26",
      "E27",
      "E30",
      "E31",
      "E33",
      "E36",
      "R01",
      "R02",
      "R03",
      "R04",
      "R05",
      "R07",
      "R08",
      "R09",
      "R11",
      "R12",
      "R13",
      "R14",
      "R15",
      "R16",
      "R17",
      "R18",
      "R19",
      "R20",
      "R21",
      "R23",
      "R24",
      "G01",
      "G02",
      "G03",
      "G05",
      "G06",
      "G07",
      "G08",
      "G09",
      "G10",
      "G11",
      "G12",
      "G13",
      "G14",
      "G15",
      "G16",
      "G17",
      "G18",
      "G19",
      "G20",
      "G21",
      "G22",
      "G24",
      "G25",
      "G26",
      "G27",
      "G28",
      "G29",
      "G30",
      "G31",
      "G32",
    ],
    satelliteAccuracyCodes: [
      4, 4, 4, 4, 4, 5, 4, 4, 5, 5, 5, 4, 5, 4, 4, 4, 4, 4, 5, 4, 5, 5, 5, 5, 5, 5, 5, 5, 6, 5, 5,
      5, 6, 6, 6, 6, 6, 5, 5, 6, 6, 5, 4, 5, 5, 4, 5, 4, 4, 5, 4, 4, 4, 4, 4, 5, 4, 4, 3, 4, 4, 4,
      4, 4, 5, 5, 4, 5, 4, 4, 4, 4, 4, 4, 3,
    ],
  });
  assert.deepEqual(sp3.comments, [
    "CNES/CLS/GRGS - TOULOUSE,FRANCE - Contact : igs-ac@cls.fr",
    "PCV:IGS14_2108 OL/AL:FES2012  NONE     NN ORB:CoN CLK:CoN",
    "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC",
    "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC",
  ]);
  assert.equal(sp3.skippedRecords, 0);
  assert.deepEqual(
    Array.from(sp3.epochsJ2000Seconds()),
    Array.from({ length: 96 }, (_, index) => 646228800 + 900 * index),
  );
  assert.deepEqual(sp3.satellites.slice(0, 10), [
    "E01",
    "E02",
    "E03",
    "E04",
    "E05",
    "E07",
    "E08",
    "E09",
    "E11",
    "E12",
  ]);
  assert.equal(sp3.gapThresholdFactor, 1.5);
  assert.deepEqual(
    Array.from(sp3.epochsJ2000Seconds()).slice(0, 6),
    [646228800, 646229700, 646230600, 646231500, 646232400, 646233300],
  );
  const state = sp3.state("G01", 0);
  assert.deepEqual(
    Array.from(state.positionM),
    [-10438032.216, 19508882.933000002, -14665718.188000001],
  );
  assert.equal(state.clockS, 0.000015315889);
  assert.equal(state.velocityMS, undefined);
  assert.deepEqual(sp3.recordAccuracyCodes("G01", 0), {
    p: {
      axisExponents: [undefined, undefined, undefined],
      clockExponent: undefined,
      positionVelocityBase: 0,
      clockRateBase: 0,
    },
    v: undefined,
  });
  assert.deepEqual(sp3.recordAccuracy("G01", 0), {
    p: {
      positionSigmaM: [{ kind: "unknown" }, { kind: "unknown" }, { kind: "unknown" }],
      clockSigmaM: { kind: "unknown" },
      positionVarianceM2: [{ kind: "unknown" }, { kind: "unknown" }, { kind: "unknown" }],
      clockVarianceM2: { kind: "unknown" },
    },
    v: undefined,
  });
  const summary = sp3.predictionSummary();
  assert.equal(summary.epochs.length, 96);
  assert.deepEqual(summary.epochs[0], {
    epochJ2000Seconds: 646228800,
    observed: true,
    orbitPredictedSatellites: [],
    clockPredictedSatellites: [],
  });
  assert.equal(summary.observedThroughJ2000Seconds, 646314300);

  const adjusted = sp3.withInterpolationOptions(2.0);
  assert.equal(sp3.gapThresholdFactor, 1.5);
  assert.equal(adjusted.gapThresholdFactor, 2.0);
  assert.deepEqual(adjusted.epochsJ2000Seconds(), sp3.epochsJ2000Seconds());
  const written = sp3.toSp3String();
  assert.equal(typeof written, "string");
  const reread = loadSp3(Buffer.from(written, "utf8"));
  assert.equal(reread.epochCount, sp3.epochCount);
  assert.deepEqual(reread.satellites, sp3.satellites);
  assert.deepEqual(reread.state("G01", 0).positionM, state.positionM);
});

test("Sp3 header projection preserves absent optional fields as undefined", () => {
  const lines = fixture("sp3/g02_ecef_two_epoch.sp3").toString("utf8").split(/\r?\n/);
  lines[0] = `${lines[0].slice(0, 40)}     ${lines[0].slice(45)}`;
  const cLine = lines.findIndex((line) => line.startsWith("%c"));
  assert.ok(cLine >= 0);
  lines[cLine] = `${lines[cLine].slice(0, 3)}  ${lines[cLine].slice(5)}`;
  const fLine = lines.findIndex((line) => line.startsWith("%f"));
  assert.ok(fLine >= 0);
  lines[fLine] = `${lines[fLine].slice(0, 3)}                       ${lines[fLine].slice(26)}`;

  const sp3 = loadSp3(new TextEncoder().encode(lines.join("\n")));
  assert.deepEqual(
    [sp3.header.dataUsed, sp3.header.fileType, sp3.header.posVelBase, sp3.header.clockRateBase],
    [undefined, undefined, undefined, undefined],
  );
  sp3.free();
});

test("Sp3 counts unsupported satellite position records it skips", () => {
  const source = fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3").toString("utf8");
  const lines = source.split("\n");
  const positionRecord = lines.findIndex((line) => line.startsWith("PE01"));
  assert.notEqual(positionRecord, -1);
  lines[positionRecord] = lines[positionRecord].replace(/^PE01/, "PL01");
  const sp3 = loadSp3(Buffer.from(lines.join("\n"), "utf8"));
  assert.equal(sp3.skippedRecords, 1);
});

test("loadSp3 interpolation policy", () => {
  const bytes = gappedSp3Bytes();
  const sp3Default = loadSp3(bytes);
  assert.equal(sp3Default.gapThresholdFactor, 1.5);

  // Midpoint of 12-spacing G01 hole is refused under default policy.
  const midpoint = Float64Array.of(MID_HOLE_J2000_S);
  assert.throws(() => sp3Default.interpolate("G01", midpoint), /epoch out of range/);

  // Factor 13 spans the 12-spacing hole and serves the midpoint.
  const sp3Wide = loadSp3(bytes, 13.0);
  assert.equal(sp3Wide.gapThresholdFactor, 13.0);
  const interp = sp3Wide.interpolate("G01", midpoint);
  assert.ok(interp.positionM.every((v) => Number.isFinite(v)));

  // A contiguous satellite produces identical interpolation under either policy.
  const queryG02 = Float64Array.of(MID_HOLE_J2000_S + 450.0);
  const interpDefault = sp3Default.interpolate("G02", queryG02);
  const interpWide = sp3Wide.interpolate("G02", queryG02);
  assert.deepEqual(interpWide.positionM, interpDefault.positionM);

  // Invalid factor (<= 1.0 or non-finite) raises Error / RangeError.
  for (const invalid of [1.0, 0.5, Number.NaN, Number.POSITIVE_INFINITY]) {
    assert.throws(() => loadSp3(bytes, invalid), /greater than 1\.0/);
  }
});

test("Sp3.withInterpolationOptions", () => {
  const bytes = gappedSp3Bytes();
  const sp3 = loadSp3(bytes);
  const midpoint = Float64Array.of(MID_HOLE_J2000_S);

  assert.throws(() => sp3.interpolate("G01", midpoint), /epoch out of range/);

  const sp3Wide = sp3.withInterpolationOptions(13.0);
  assert.equal(sp3Wide.gapThresholdFactor, 13.0);
  assert.ok(sp3Wide.interpolate("G01", midpoint).positionM.every((v) => Number.isFinite(v)));

  assert.throws(() => sp3.withInterpolationOptions(1.0), /greater than 1\.0/);
});

test("Sp3.stencilExtent follows gap threshold", () => {
  const bytes = gappedSp3Bytes();
  const sp3Default = loadSp3(bytes);
  assert.deepEqual(sp3Default.stencilExtent(), {
    beforeS: 11.0 * 900.0,
    afterS: 11.0 * 900.0,
  });

  const sp3Wide = loadSp3(bytes, 13.0);
  assert.deepEqual(sp3Wide.stencilExtent(), {
    beforeS: 19800.0,
    afterS: 19800.0,
  });
});

test("Sp3.checkContinuity interpolation policy", () => {
  const bytes = gappedSp3Bytes();
  const sp3 = loadSp3(bytes);

  // Default policy checks residuals without bridging the gap.
  const resDef = sp3.checkContinuity(null, 1.0);
  const dDef = resDef.defects.find((d) => d.satellite === "G01" && d.fromJ2000S === 646_254_900.0);
  assert.ok(dDef);
  assert.ok(dDef.magnitude > 20.0);
  // Every field of the hold-out residual under the engine's name, beside the
  // summary fields it fills.
  assert.equal(dDef.kind, "hold_out_residual");
  assert.equal(dDef.precedingJ2000S, dDef.fromJ2000S);
  assert.equal(dDef.epochJ2000S, dDef.toJ2000S);
  assert.equal(dDef.residualM, dDef.magnitude);
  assert.equal(dDef.toleranceM, 1.0);
  assert.equal(dDef.bound, dDef.toleranceM);
  assert.ok(dDef.nodeEpochsJ2000S.length > 0);
  assert.ok(dDef.nodeEpochsJ2000S.every((epoch, i, all) => i === 0 || all[i - 1] < epoch));
  assert.equal(dDef.intervalS, undefined);

  // Wide policy bridges the gap, altering the hold-out replay.
  const resWide = sp3.checkContinuity(null, 1.0, 13.0);
  const dWide = resWide.defects.find(
    (d) => d.satellite === "G01" && d.fromJ2000S === 646_254_900.0,
  );
  assert.ok(dWide);
  assert.ok(dWide.magnitude < 15.0);
  assert.notEqual(dWide.magnitude, dDef.magnitude);

  assert.throws(() => sp3.checkContinuity(null, 1.0, 1.0), /greater than 1\.0/);
  const invalidTolerance = captureThrow(() => sp3.checkContinuity(null, Number.NaN));
  assert.equal(invalidTolerance.name, "ContinuityOptionsError");
  assert.deepEqual(invalidTolerance.detail, {
    field: "residual_tolerance_m",
    value: "NaN",
    reason: "notFinite",
  });
});

test("Sp3.continuityVerdict interpolation policy", () => {
  const bytes = gappedSp3Bytes();
  const sp3 = loadSp3(bytes);
  const axis = sp3.epochsJ2000Seconds();
  const start = axis[10];
  const stop = axis[20];

  const verdictDef = sp3.continuityVerdict(start, stop, null, 1.0);
  assert.ok(["accept", "refuse"].includes(verdictDef.decision));

  const verdictWide = sp3.continuityVerdict(start, stop, null, 1.0, 13.0);
  assert.ok(["accept", "refuse"].includes(verdictWide.decision));

  assert.throws(() => sp3.continuityVerdict(start, stop, null, 1.0, 1.0), /greater than 1\.0/);
});

test("mergeSp3 verifyContinuity interpolation policy", () => {
  const bytes = gappedSp3Bytes();
  const p1 = loadSp3(bytes);
  const p2 = loadSp3(bytes);

  const { sp3: merged, report } = mergeSp3([p1, p2], {
    combine: "precedence",
    minAgree: 1,
    verifyContinuity: { residualToleranceM: 1.0, gapThresholdFactor: 13.0 },
  });
  const axis = merged.epochsJ2000Seconds();
  const verdict = report.continuityVerdict(axis[10], axis[20]);
  assert.ok(["accept", "refuse"].includes(verdict.decision));

  const p3 = loadSp3(bytes);
  const p4 = loadSp3(bytes);
  assert.throws(
    () =>
      mergeSp3([p3, p4], {
        combine: "precedence",
        minAgree: 1,
        verifyContinuity: { residualToleranceM: 1.0, gapThresholdFactor: 1.0 },
      }),
    /greater than 1\.0/,
  );
});

test("preciseEphemerisSamplesFromSamples interpolation policy", () => {
  const bytes = gappedSp3Bytes();
  const sp3 = loadSp3(bytes);
  const samples = sp3PreciseEphemerisSamples(sp3);

  const pesDef = preciseEphemerisSamplesFromSamples(samples);
  assert.equal(pesDef.gapThresholdFactor, 1.5);
  const statesDef = pesDef.observableStatesAtSharedJ2000S(["G01"], MID_HOLE_J2000_S);
  assert.equal(statesDef.statuses[0], "gap");
  assert.equal(statesDef.elementResults[0].error, "epoch out of range");

  const pesWide = preciseEphemerisSamplesFromSamples(samples, 13.0);
  assert.equal(pesWide.gapThresholdFactor, 13.0);
  const statesWide = pesWide.observableStatesAtSharedJ2000S(["G01"], MID_HOLE_J2000_S);
  assert.equal(statesWide.statuses[0], "valid");
  assert.ok(statesWide.positionsEcefM[0].every((v) => Number.isFinite(v)));

  const pesWide2 = pesDef.withInterpolationOptions(13.0);
  assert.equal(pesWide2.gapThresholdFactor, 13.0);
  const statesWide2 = pesWide2.observableStatesAtSharedJ2000S(["G01"], MID_HOLE_J2000_S);
  assert.equal(statesWide2.statuses[0], "valid");
  assert.ok(statesWide2.positionsEcefM[0].every((v) => Number.isFinite(v)));

  assert.throws(() => preciseEphemerisSamplesFromSamples(samples, 1.0), /greater than 1\.0/);
  assert.throws(() => pesDef.withInterpolationOptions(1.0), /greater than 1\.0/);
});

test("PreciseEphemerisInterpolant.fromSp3 interpolation policy", () => {
  const bytes = gappedSp3Bytes();
  const sp3 = loadSp3(bytes);

  const interpDef = PreciseEphemerisInterpolant.fromSp3(sp3);
  assert.equal(interpDef.gapThresholdFactor, 1.5);
  const statesDef = interpDef.observableStatesAtSharedJ2000S(["G01"], MID_HOLE_J2000_S);
  assert.equal(statesDef.statuses[0], "gap");
  assert.equal(statesDef.elementResults[0].error, "epoch out of range");

  const interpWide = PreciseEphemerisInterpolant.fromSp3(sp3, 13.0);
  assert.equal(interpWide.gapThresholdFactor, 13.0);
  const statesWide = interpWide.observableStatesAtSharedJ2000S(["G01"], MID_HOLE_J2000_S);
  assert.equal(statesWide.statuses[0], "valid");
  assert.ok(statesWide.positionsEcefM[0].every((v) => Number.isFinite(v)));

  const interpWide2 = interpDef.withInterpolationOptions(13.0);
  assert.equal(interpWide2.gapThresholdFactor, 13.0);
  const statesWide2 = interpWide2.observableStatesAtSharedJ2000S(["G01"], MID_HOLE_J2000_S);
  assert.equal(statesWide2.statuses[0], "valid");
  assert.ok(statesWide2.positionsEcefM[0].every((v) => Number.isFinite(v)));

  assert.throws(() => PreciseEphemerisInterpolant.fromSp3(sp3, 1.0), /greater than 1\.0/);
  assert.throws(() => interpDef.withInterpolationOptions(1.0), /greater than 1\.0/);
});

test("PreciseEphemerisInterpolant.fromSamples interpolation policy", () => {
  const bytes = gappedSp3Bytes();
  const sp3 = loadSp3(bytes);
  const samples = sp3PreciseEphemerisSamples(sp3);

  const interpDef = PreciseEphemerisInterpolant.fromSamples(samples);
  assert.equal(interpDef.gapThresholdFactor, 1.5);
  const statesDef = interpDef.observableStatesAtSharedJ2000S(["G01"], MID_HOLE_J2000_S);
  assert.equal(statesDef.statuses[0], "gap");
  assert.equal(statesDef.elementResults[0].error, "epoch out of range");

  const interpWide = PreciseEphemerisInterpolant.fromSamples(samples, 13.0);
  assert.equal(interpWide.gapThresholdFactor, 13.0);
  const statesWide = interpWide.observableStatesAtSharedJ2000S(["G01"], MID_HOLE_J2000_S);
  assert.equal(statesWide.statuses[0], "valid");
  assert.ok(statesWide.positionsEcefM[0].every((v) => Number.isFinite(v)));

  assert.throws(() => PreciseEphemerisInterpolant.fromSamples(samples, 1.0), /greater than 1\.0/);
});

test("PreciseEphemerisInterpolant.fromPreciseEphemerisSamples interpolation policy", () => {
  const bytes = gappedSp3Bytes();
  const sp3 = loadSp3(bytes);
  const samplesSrc = preciseEphemerisSamplesFromSamples(sp3PreciseEphemerisSamples(sp3));

  const interpDef = PreciseEphemerisInterpolant.fromPreciseEphemerisSamples(samplesSrc);
  assert.equal(interpDef.gapThresholdFactor, 1.5);
  const statesDef = interpDef.observableStatesAtSharedJ2000S(["G01"], MID_HOLE_J2000_S);
  assert.equal(statesDef.statuses[0], "gap");
  assert.equal(statesDef.elementResults[0].error, "epoch out of range");

  const interpWide = PreciseEphemerisInterpolant.fromPreciseEphemerisSamples(samplesSrc, 13.0);
  assert.equal(interpWide.gapThresholdFactor, 13.0);
  const statesWide = interpWide.observableStatesAtSharedJ2000S(["G01"], MID_HOLE_J2000_S);
  assert.equal(statesWide.statuses[0], "valid");
  assert.ok(statesWide.positionsEcefM[0].every((v) => Number.isFinite(v)));

  assert.throws(
    () => PreciseEphemerisInterpolant.fromPreciseEphemerisSamples(samplesSrc, 1.0),
    /greater than 1\.0/,
  );
});

test("Sp3.preciseInterpolantArtifactBytes interpolation policy", () => {
  const bytes = gappedSp3Bytes();
  const sp3 = loadSp3(bytes);

  const bytesDef = sp3.preciseInterpolantArtifactBytes();
  const artifactDef = openPreciseInterpolantArtifact(bytesDef);
  assert.equal(artifactDef.gapThresholdFactor, 1.5);
  assert.throws(() => artifactDef.evaluate("G01", MID_HOLE_J2000_S), /epoch out of range/);

  const bytesWide = sp3.preciseInterpolantArtifactBytes(13.0);
  const artifactWide = openPreciseInterpolantArtifact(bytesWide);
  assert.equal(artifactWide.gapThresholdFactor, 13.0);
  const state = artifactWide.evaluate("G01", MID_HOLE_J2000_S);
  assert.ok(Array.from(state.positionM).every((v) => Number.isFinite(v)));

  assert.throws(() => sp3.preciseInterpolantArtifactBytes(1.0), /greater than 1\.0/);
});
