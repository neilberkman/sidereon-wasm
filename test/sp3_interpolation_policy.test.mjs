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

  // Wide policy bridges the gap, altering the hold-out replay.
  const resWide = sp3.checkContinuity(null, 1.0, 13.0);
  const dWide = resWide.defects.find(
    (d) => d.satellite === "G01" && d.fromJ2000S === 646_254_900.0,
  );
  assert.ok(dWide);
  assert.ok(dWide.magnitude < 15.0);
  assert.notEqual(dWide.magnitude, dDef.magnitude);

  assert.throws(() => sp3.checkContinuity(null, 1.0, 1.0), /greater than 1\.0/);
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
  const verdict = report.continuityVerdict(merged, axis[10], axis[20]);
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
