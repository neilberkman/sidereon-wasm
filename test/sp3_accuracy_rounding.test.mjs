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

function replaceCodes(text, satellite, { position, velocity }) {
  const format = (value, width) =>
    value === null ? " ".repeat(width) : String(value).padStart(width);
  return `${text
    .split("\n")
    .map((line) => {
      const codes = line.startsWith(`P${satellite}`)
        ? position
        : line.startsWith(`V${satellite}`)
          ? velocity
          : null;
      return codes
        ? line.slice(0, 60) +
            ` ${format(codes[0], 2)} ${format(codes[1], 2)} ${format(codes[2], 2)} ${format(codes[3], 3)}  `
        : line;
    })
    .join("\n")}`;
}

function replaceBases(text, { position, clock }) {
  return `${text
    .split("\n")
    .map((line) => {
      if (!line.startsWith("%f ")) return line;
      const fields = line.split(/\s+/);
      if (position !== undefined) fields[1] = Number(position).toFixed(7);
      if (clock !== undefined) fields[2] = Number(clock).toFixed(9);
      return `%f ${String(fields[1]).padStart(10)} ${String(fields[2]).padStart(12)} ${fields[3]} ${fields[4]}`;
    })
    .join("\n")}`;
}

for (const order of ["A.sp3", "B.sp3"]) {
  test(`accuracy rounding preserves sigmas after merge with ${order} first`, () => {
    const sources =
      order === "A.sp3" ? [load("A.sp3"), load("B.sp3")] : [load("B.sp3"), load("A.sp3")];
    for (const source of sources) assert.equal(typeof source.toSp3String(), "string");

    const { sp3 } = mergeSp3(sources, options);
    const { text, report } = sp3.toSp3StringWithAccuracyRounding(
      Sp3AccuracyRoundingPolicy.Conservative,
    );
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

for (const reverse of [false, true]) {
  test(`compatible defined accuracy bases preserve values in either order (reverse=${reverse})`, () => {
    let aText = replaceBases(
      new TextDecoder().decode(fixture("sp3/accuracy-rounding/A-anchor.sp3")),
      {
        position: 2,
        clock: 2,
      },
    );
    aText = replaceCodes(aText, "G01", { position: [2, null, null, 2] });
    let bText = replaceBases(
      new TextDecoder().decode(fixture("sp3/accuracy-rounding/B-anchor.sp3")),
      {
        position: 4,
        clock: 4,
      },
    );
    bText = replaceCodes(bText, "G12", { position: [1, null, null, 1] });
    const a = loadSp3(encoder.encode(aText));
    const b = loadSp3(encoder.encode(bText));
    const aRoundtrip = loadSp3(encoder.encode(a.toSp3String()));
    const bRoundtrip = loadSp3(encoder.encode(b.toSp3String()));
    for (const [source, restored, satellite] of [
      [a, aRoundtrip, "G01"],
      [b, bRoundtrip, "G12"],
    ]) {
      assert.deepEqual(restored.recordAccuracy(satellite, 0), source.recordAccuracy(satellite, 0));
    }
    const { sp3 } = mergeSp3(reverse ? [b, a] : [a, b], options);
    const strictText = sp3.toSp3String();
    const strict = loadSp3(encoder.encode(strictText));
    const { text, report } = sp3.toSp3StringWithAccuracyRounding(
      Sp3AccuracyRoundingPolicy.Conservative,
    );
    const rounded = loadSp3(encoder.encode(text));
    assert.equal(rounded.toSp3String(), strict.toSp3String());
    assert.deepEqual(Object.values(report.accuracySigmas), Array(8).fill(0));
    for (const satellite of ["G01", "G12"]) {
      assert.deepEqual(rounded.recordAccuracy(satellite, 0), strict.recordAccuracy(satellite, 0));
    }
    assert.equal(
      sp3.recordAccuracy("G01", 0).p.positionSigmaM[0].value,
      sp3.recordAccuracy("G12", 0).p.positionSigmaM[0].value,
    );
    assert.equal(
      sp3.recordAccuracy("G01", 0).p.clockSigmaM.value,
      sp3.recordAccuracy("G12", 0).p.clockSigmaM.value,
    );
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

  const { text, report } = sp3.toSp3StringWithAccuracyRounding(
    Sp3AccuracyRoundingPolicy.Conservative,
  );
  const restored = loadSp3(encoder.encode(text));
  assert.equal(restored.header.posVelBase, 2);
  assert.equal(restored.recordAccuracy("G12", 0).p.positionSigmaM[0].value, 0.032);
  assert.ok(
    Math.abs(report.accuracySigmas.positionSigmaXMaxChangeM - 0.013810105964541435) < 1e-15,
  );
});

test("accuracy rounding reports nonzero changes with the incompatible bases reversed", () => {
  let first = new TextDecoder().decode(fixture("sp3/accuracy-rounding/A.sp3"));
  first = first.replace("%f  0.0000000", "%f  2.0000000");
  first = replacePositionXExponent(first, "G01", 1);
  const a = loadSp3(encoder.encode(first));
  const b = load("B.sp3");
  const { sp3 } = mergeSp3([b, a], options);
  const { text, report } = sp3.toSp3StringWithAccuracyRounding(
    Sp3AccuracyRoundingPolicy.Conservative,
  );
  const restored = loadSp3(encoder.encode(text));
  const before = sp3.recordAccuracy("G01", 0).p.positionSigmaM[0];
  const after = restored.recordAccuracy("G01", 0).p.positionSigmaM[0];
  assert.ok(after.value > before.value);
  assert.ok(report.accuracySigmas.positionSigmaXMaxChangeM > 0);
  assert.equal(report.accuracySigmas.positionSigmaXMaxChangeM, after.value - before.value);
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
  const { text, report } = sp3.toSp3StringWithAccuracyRounding(
    Sp3AccuracyRoundingPolicy.Conservative,
  );
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
  const directResult = direct.toSp3StringWithAccuracyRounding(
    Sp3AccuracyRoundingPolicy.Conservative,
  );
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
  const { text, report } = sp3.toSp3StringWithAccuracyRounding(
    Sp3AccuracyRoundingPolicy.Conservative,
  );
  const restored = loadSp3(encoder.encode(text));
  const deltas = Object.values(report.accuracySigmas);

  assert.equal(deltas.length, 8);
  const source = sp3.recordAccuracy("G12", 0);
  const output = restored.recordAccuracy("G12", 0);
  assert.equal(source.v, undefined);
  assert.equal(output.v, undefined);
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
  assert.deepEqual(
    output.p.positionSigmaM.map((value) => value.value),
    [0.032, 0.032, 0.032],
  );
  assert.equal(output.p.clockSigmaM.value, 2 * (299792458 * 1e-12));
});

test("accuracy rounding respects I2 and I3 lower finite bounds and sentinels", () => {
  let aText = replaceBases(
    new TextDecoder().decode(fixture("sp3/accuracy-rounding/A-anchor.sp3")),
    {
      position: 2,
    },
  );
  aText = replaceCodes(aText, "G01", { position: [-9, null, null, null] });
  let bText = replaceBases(
    new TextDecoder().decode(fixture("sp3/accuracy-rounding/B-anchor.sp3")),
    {
      position: 4,
    },
  );
  bText = replaceCodes(bText, "G12", { position: [-5, null, null, null] });
  const lowerAxis = mergeSp3(
    [loadSp3(encoder.encode(aText)), loadSp3(encoder.encode(bText))],
    options,
  ).sp3;
  const axisResult = lowerAxis.toSp3StringWithAccuracyRounding(
    Sp3AccuracyRoundingPolicy.Conservative,
  );
  assert.equal(
    loadSp3(encoder.encode(axisResult.text)).recordAccuracyCodes("G12", 0).p.axisExponents[0],
    -9,
  );
  assert.ok(axisResult.report.accuracySigmas.positionSigmaXMaxChangeM > 0);

  let aClock = replaceBases(
    new TextDecoder().decode(fixture("sp3/accuracy-rounding/A-anchor.sp3")),
    { clock: 2 },
  );
  aClock = replaceCodes(aClock, "G01", { position: [null, null, null, -99] });
  let bClock = replaceBases(
    new TextDecoder().decode(fixture("sp3/accuracy-rounding/B-anchor.sp3")),
    { clock: 4 },
  );
  bClock = replaceCodes(bClock, "G12", { position: [null, null, null, -50] });
  const lowerClock = mergeSp3(
    [loadSp3(encoder.encode(aClock)), loadSp3(encoder.encode(bClock))],
    options,
  ).sp3;
  const clockResult = lowerClock.toSp3StringWithAccuracyRounding(
    Sp3AccuracyRoundingPolicy.Conservative,
  );
  assert.equal(
    loadSp3(encoder.encode(clockResult.text)).recordAccuracyCodes("G12", 0).p.clockExponent,
    -99,
  );
  assert.ok(clockResult.report.accuracySigmas.clockSigmaMaxChangeM > 0);

  let upperA = replaceBases(
    new TextDecoder().decode(fixture("sp3/accuracy-rounding/A-anchor.sp3")),
    { clock: 1.025 },
  );
  upperA = replaceCodes(upperA, "G01", { position: [null, null, null, 1] });
  let upperB = replaceBases(
    new TextDecoder().decode(fixture("sp3/accuracy-rounding/B-anchor.sp3")),
    { clock: 2 },
  );
  upperB = replaceCodes(upperB, "G12", { position: [null, null, null, 998] });
  const upperClock = mergeSp3(
    [loadSp3(encoder.encode(upperA)), loadSp3(encoder.encode(upperB))],
    options,
  ).sp3;
  assert.throws(
    () => upperClock.toSp3StringWithAccuracyRounding(Sp3AccuracyRoundingPolicy.Conservative),
    (error) => error.name === "Sp3WriteError" && error.detail.kind === "ACCURACY_NOT_REPRESENTABLE",
  );

  let sentinelText = replaceBases(
    new TextDecoder().decode(fixture("sp3/accuracy-rounding/B.sp3")),
    { position: 2, clock: 2 },
  );
  sentinelText = replaceCodes(sentinelText, "G12", { position: [99, 99, 99, 999] });
  const sentinel = loadSp3(encoder.encode(sentinelText));
  const sentinelResult = sentinel.toSp3StringWithAccuracyRounding(
    Sp3AccuracyRoundingPolicy.Conservative,
  );
  const sentinelCodes = loadSp3(encoder.encode(sentinelResult.text)).recordAccuracyCodes(
    "G12",
    0,
  ).p;
  assert.deepEqual(sentinelCodes.axisExponents, [99, 99, 99]);
  assert.equal(sentinelCodes.clockExponent, 999);
  assert.deepEqual(Object.values(sentinelResult.report.accuracySigmas), Array(8).fill(0));

  let invalidText = replaceBases(new TextDecoder().decode(fixture("sp3/accuracy-rounding/B.sp3")), {
    position: 0,
  });
  invalidText = replaceCodes(invalidText, "G12", { position: [1, 99, 99, 999] });
  const invalidBase = loadSp3(encoder.encode(invalidText));
  assert.equal(invalidBase.recordAccuracy("G12", 0).p.positionSigmaM[0].kind, "invalidBase");
  const invalidResult = invalidBase.toSp3StringWithAccuracyRounding(
    Sp3AccuracyRoundingPolicy.Conservative,
  );
  assert.equal(
    loadSp3(encoder.encode(invalidResult.text)).recordAccuracyCodes("G12", 0).p.axisExponents[0],
    1,
  );
  assert.equal(invalidResult.report.accuracySigmas.positionSigmaXMaxChangeM, 0);

  let invalidSource = new TextDecoder().decode(fixture("sp3/accuracy-rounding/A-anchor.sp3"));
  invalidSource = replaceCodes(invalidSource, "G01", { position: [13, 13, 13, 13] });
  let knownSource = new TextDecoder().decode(fixture("sp3/accuracy-rounding/B-anchor.sp3"));
  knownSource = replaceCodes(knownSource, "G12", { position: [13, 13, 13, 13] });
  for (const inputs of [
    [invalidSource, knownSource],
    [knownSource, invalidSource],
  ]) {
    const conflict = mergeSp3(
      inputs.map((text) => loadSp3(encoder.encode(text))),
      options,
    ).sp3;
    assert.throws(
      () => conflict.toSp3StringWithAccuracyRounding(Sp3AccuracyRoundingPolicy.Conservative),
      (error) =>
        error.name === "Sp3WriteError" && error.detail.kind === "ACCURACY_NOT_REPRESENTABLE",
    );
  }
});
