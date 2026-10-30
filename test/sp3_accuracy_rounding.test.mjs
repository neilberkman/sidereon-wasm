import assert from "node:assert/strict";
import test from "node:test";

import { Sp3AccuracyRoundingPolicy, loadSp3, mergeSp3 } from "../pkg-node/sidereon.js";
import { fixture } from "./helpers.mjs";

const encoder = new TextEncoder();
const load = (name) => loadSp3(fixture(`sp3/accuracy-rounding/${name}`));
const options = {
  combine: "precedence",
  precedenceScope: "cell",
  minAgree: 1,
  clockMinCommon: 1,
  positionToleranceM: 5,
  clockToleranceS: 50e-9,
  outlierReject: { positionToleranceM: 5, clockToleranceS: 50e-9 },
  targetEpochIntervalS: 300,
  systems: ["G"],
};

function replacePositionXExponent(text, satellite, exponent) {
  const suffix = ` ${String(exponent).padStart(2)} ${"  "} ${"  "} ${"   "} `;
  return `${text
    .split("\n")
    .map((line) => (line.startsWith(`P${satellite}`) ? line.slice(0, 60) + suffix : line))
    .join("\n")}`;
}


for (const order of ["A.sp3", "B.sp3"]) {
  test(`accuracy rounding preserves sigmas after merge with ${order} first`, () => {
    const sources = order === "A.sp3" ? [load("A.sp3"), load("B.sp3")] : [load("B.sp3"), load("A.sp3")];
    for (const source of sources) assert.equal(typeof source.toSp3String(), "string");

    const { sp3 } = mergeSp3(sources, options);
    const { text, report } = sp3.toSp3StringWithAccuracyRounding(Sp3AccuracyRoundingPolicy.Conservative);
    const restored = loadSp3(encoder.encode(text));

    assert.equal(typeof report.recordValues.positionXMaxChangeM, "number");
    assert.equal(typeof report.accuracySigmas.positionSigmaXMaxChangeM, "number");
    assert.equal(typeof report.accuracySigmas.clockSigmaMaxChangeM, "number");
    assert.deepEqual(Object.values(report.accuracySigmas), Array(8).fill(0));
    const sourceSigma = sp3.recordAccuracy("G12", 0).p.positionSigmaM[0];
    const outputSigma = restored.recordAccuracy("G12", 0).p.positionSigmaM[0];
    assert.equal(sourceSigma.kind, "known");
    assert.equal(outputSigma.kind, "known");
    assert.ok(outputSigma.value >= sourceSigma.value);
    assert.deepEqual(restored.state("G12", 0).positionM, sp3.state("G12", 0).positionM);
    assert.equal(restored.recordAccuracy("G01", 0).p.positionSigmaM[0].kind, "unknown");
  });
}

test("accuracy rounding requires the explicit recognized policy", () => {
  const sp3 = load("B.sp3");
  assert.throws(() => sp3.toSp3StringWithAccuracyRounding(), TypeError);
  assert.throws(() => sp3.toSp3StringWithAccuracyRounding(undefined), TypeError);
  assert.throws(() => sp3.toSp3StringWithAccuracyRounding(null), TypeError);
  assert.throws(() => sp3.toSp3StringWithAccuracyRounding("lossy"), TypeError);
  assert.equal(
    typeof sp3.toSp3StringWithAccuracyRounding(Sp3AccuracyRoundingPolicy.Conservative).text,
    "string",
  );
});

test("accuracy rounding reports a nonzero conservative sigma increase", () => {
  let first = new TextDecoder().decode(fixture("sp3/accuracy-rounding/A.sp3"));
  first = first.replace("%f  0.0000000", "%f  2.0000000");
  first = replacePositionXExponent(first, "G01", 1);
  const a = loadSp3(encoder.encode(first));
  const b = load("B.sp3");
  assert.equal(typeof a.toSp3String(), "string");
  assert.equal(typeof b.toSp3String(), "string");
  const { sp3 } = mergeSp3([a, b], options);

  const { text, report } = sp3.toSp3StringWithAccuracyRounding(Sp3AccuracyRoundingPolicy.Conservative);
  const restored = loadSp3(encoder.encode(text));
  assert.equal(restored.header.posVelBase, 2);
  assert.equal(restored.recordAccuracy("G12", 0).p.positionSigmaM[0].value, 0.032);
  assert.ok(
    Math.abs(report.accuracySigmas.positionSigmaXMaxChangeM - 0.013810105964541435) < 1e-15,
  );
});

test("accuracy rounding refuses when the output exponent range cannot preserve sigma", () => {
  let first = new TextDecoder().decode(fixture("sp3/accuracy-rounding/A.sp3"));
  first = first.replace("%f  0.0000000", "%f  1.2500000");
  first = replacePositionXExponent(first, "G01", 1);
  let second = new TextDecoder().decode(fixture("sp3/accuracy-rounding/B.sp3"));
  second = second.replace("%f  1.2500000", "%f  2.0000000");
  second = replacePositionXExponent(second, "G12", 98);
  const a = loadSp3(encoder.encode(first));
  const b = loadSp3(encoder.encode(second));
  assert.equal(typeof a.toSp3String(), "string");
  assert.equal(typeof b.toSp3String(), "string");
  const { sp3 } = mergeSp3([a, b], options);
  assert.throws(
    () => sp3.toSp3StringWithAccuracyRounding(Sp3AccuracyRoundingPolicy.Conservative),
    (error) => error.name === "Sp3WriteError" && error.detail.kind === "ACCURACY_NOT_REPRESENTABLE",
  );
});

test("accuracy rounding preserves sigma when a shared clock anchor is present", () => {
  const { sp3 } = mergeSp3([load("A-anchor.sp3"), load("B-anchor.sp3")], options);
  const { text, report } = sp3.toSp3StringWithAccuracyRounding(Sp3AccuracyRoundingPolicy.Conservative);
  const restored = loadSp3(encoder.encode(text));
  const source = sp3.recordAccuracy("G12", 0).p.clockSigmaM;
  const output = restored.recordAccuracy("G12", 0).p.clockSigmaM;

  assert.equal(source.kind, "known");
  assert.equal(output.kind, "known");
  assert.ok(output.value >= source.value);
  assert.equal(restored.state("G12", 0).clockS, sp3.state("G12", 0).clockS);
  assert.ok(report.accuracySigmas.clockSigmaMaxChangeM >= 0);
});

test("accuracy rounding report maps all eight sigma fields", () => {
  const direct = load("B-velocity.sp3");
  const directResult = direct.toSp3StringWithAccuracyRounding(Sp3AccuracyRoundingPolicy.Conservative);
  const directRestored = loadSp3(encoder.encode(directResult.text));
  assert.deepEqual(Object.values(directResult.report.accuracySigmas), Array(8).fill(0));
  const directSource = direct.recordAccuracy("G12", 0);
  const directOutput = directRestored.recordAccuracy("G12", 0);
  assert.deepEqual(
    directOutput.v.velocitySigmaMS.map((value) => value.value),
    directSource.v.velocitySigmaMS.map((value) => value.value),
  );

  const a = load("A-velocity.sp3");
  const b = load("B-velocity.sp3");
  assert.equal(typeof a.toSp3String(), "string");
  assert.equal(typeof b.toSp3String(), "string");
  const { sp3 } = mergeSp3([a, b], options);
  const { text, report } = sp3.toSp3StringWithAccuracyRounding(Sp3AccuracyRoundingPolicy.Conservative);
  const restored = loadSp3(encoder.encode(text));
  const deltas = Object.values(report.accuracySigmas);

  assert.equal(deltas.length, 8);
  const source = sp3.recordAccuracy("G12", 0);
  const output = restored.recordAccuracy("G12", 0);
  assert.equal(source.v, null);
  assert.equal(output.v, null);
  for (let index = 0; index < 3; index += 1) {
    assert.equal(source.p.positionSigmaM[index].kind, "known");
    assert.equal(output.p.positionSigmaM[index].kind, "known");
    assert.ok(output.p.positionSigmaM[index].value >= source.p.positionSigmaM[index].value);
  }
  assert.ok(output.p.clockSigmaM.value >= source.p.clockSigmaM.value);
  const actualDeltas = [
    ...source.p.positionSigmaM.map(
      (value, index) => output.p.positionSigmaM[index].value - value.value,
    ),
    output.p.clockSigmaM.value - source.p.clockSigmaM.value,
  ];
  assert.deepEqual([...deltas.slice(0, 3), deltas[6]], actualDeltas);
  assert.ok(deltas.slice(0, 3).every((delta) => delta > 0));
  assert.deepEqual(deltas.slice(3, 6), Array(3).fill(0));
  assert.ok(deltas[6] > 0);
  assert.equal(deltas[7], 0);
  assert.deepEqual(output.p.positionSigmaM.map((value) => value.value), [0.032, 0.032, 0.032]);
  assert.equal(output.p.clockSigmaM.value, 2 * (299792458 * 1e-12));
  directRestored.free();
  direct.free();
  a.free();
  b.free();
  sp3.free();
  restored.free();
});
