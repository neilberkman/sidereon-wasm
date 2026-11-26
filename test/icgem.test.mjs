import { test } from "node:test";
import assert from "node:assert/strict";

import {
  IcgemErrors,
  IcgemFormat,
  IcgemGravityField,
  IcgemNormalization,
  IcgemSigmas,
  IcgemTermKind,
} from "../pkg-node/sidereon.js";

const header = [
  "product_type gravity_field",
  "modelname M",
  "earth_gravity_constant 3.986004415e14",
  "radius 6378136.3",
  "max_degree 2",
  "errors formal",
  "tide_system tide_free",
  "end_of_head",
  "",
].join("\n");
const body =
  "gfc 2 0 -4.8e-4 0.0 1e-11 0.0\ngfc 2 1 0.0 0.0 1e-11 1e-11\ngfc 2 2 2.4e-6 -1.4e-6 1e-11 1e-11\n";
const text = header + body;
const variableHeader = header
  .replace("errors formal", "errors no")
  .replace("end_of_head", "format icgem2.0\nend_of_head");
const t0 = -43200;
const variable =
  variableHeader +
  "gfct 2 0 -4.8e-4 0.0 20000101.0000 20300101.0000\n" +
  "gfc 2 1 0 0\ngfc 2 2 2.4e-6 -1.4e-6\n";

const typed = (detail) => (error) => {
  assert.equal(error.name, "IcgemError");
  assert.equal(error.kind, detail.kind);
  assert.deepEqual(error.detail, detail);
  if (detail.kind === "missingEndOfHead")
    assert.equal(error.toString(), "ICGEM file has no end_of_head line");
  return true;
};

test("ICGEM field metadata, static coefficients, sigmas and truncation", () => {
  using field = IcgemGravityField.parse(text);
  using same = IcgemGravityField.fromBytes(new TextEncoder().encode(text));
  using truncated = IcgemGravityField.parseTruncated(text, 2, 0);
  assert.ok(field.equals(same));
  assert.ok(!field.equals(truncated));
  using metadata = field.header();
  using sameMetadata = same.header();
  using otherField = IcgemGravityField.parse(text.replace("modelname M", "modelname Other"));
  using otherMetadata = otherField.header();
  assert.equal(metadata.modelName, "M");
  assert.equal(metadata.gravityConstantM3S2, 3.986004415e14);
  assert.equal(metadata.radiusM, 6378136.3);
  assert.equal(metadata.maxDegree, 2);
  assert.equal(metadata.errors, IcgemErrors.Formal);
  assert.equal(metadata.normalization, IcgemNormalization.FullyNormalized);
  assert.equal(metadata.format, IcgemFormat.Icgem1_0);
  assert.equal(metadata.tideSystem, "tideFree");
  assert.ok(metadata.equals(sameMetadata));
  assert.ok(!metadata.equals(otherMetadata));
  assert.equal(field.storedMaxDegree(), 2);
  assert.equal(field.storedMaxOrder(), 2);
  assert.equal(truncated.storedMaxOrder(), 0);
  assert.equal(field.isTimeVariable(), false);
  assert.equal(field.staticCoefficient(3, 0), undefined);
  using coefficient = field.staticCoefficient(2, 2);
  using later = field.coefficientAt(2, 2, 1);
  using otherCoefficient = field.staticCoefficient(2, 0);
  assert.deepEqual(
    [coefficient.degree, coefficient.order, coefficient.c, coefficient.s],
    [2, 2, 2.4e-6, -1.4e-6],
  );
  assert.ok(coefficient.equals(later));
  assert.ok(!coefficient.equals(otherCoefficient));
  using sigmas = field.staticSigmas(2, 2);
  using sameSigmas = field.staticSigmas(2, 1);
  using none = new IcgemSigmas();
  assert.equal(sigmas.calibrated, undefined);
  assert.deepEqual(Array.from(sigmas.formal), [1e-11, 1e-11]);
  assert.ok(sigmas.equals(sameSigmas));
  assert.ok(!sigmas.equals(none));
  assert.equal(none.formal, undefined);
  assert.deepEqual(field.timeVariableTerms(), []);

  using assumed = IcgemGravityField.parse(text.replace("tide_system tide_free\n", ""));
  using labeled = assumed.withAssumedTideSystem("tideFree");
  using labeledHeader = labeled.header();
  assert.equal(labeledHeader.tideSystem, "tideFree");
  using gravity = field.gravity(2, 2);
  using equivalent = field.gravityInTideSystem(2, 2, "tideFree");
  using differing = field.gravityInTideSystem(2, 2, "zeroTide");
  assert.ok(gravity.equals(equivalent));
  assert.ok(!gravity.equals(differing));
  assert.equal(gravity.isTimeVariable(), false);
  using staticAt = gravity.coefficientAt(2, 2, 10);
  assert.ok(staticAt.equals(coefficient));
  using zeroField = IcgemGravityField.parse(
    header + "gfc 2 0 0 0 0 0\ngfc 2 1 0 0 0 0\ngfc 2 2 0 0 0 0\n",
  );
  using base = zeroField.gravity(2, 2);
  const position = new Float64Array([7000, 300, 200]);
  assert.equal(base.bodyFixedPotentialKm2S2(position), 0);
  assert.equal(base.bodyFixedPotentialAtKm2S2(position, 1), 0);
  assert.deepEqual(Array.from(base.bodyFixedAccelerationKmS2(position)), [0, 0, 0]);
  assert.deepEqual(Array.from(base.bodyFixedAccelerationAtKmS2(position, 1)), [0, 0, 0]);
});

test("time-variable ICGEM terms preserve periods, validity and coefficients", () => {
  using field = IcgemGravityField.parse(
    variable + "acos 2 0 1e-11 0.0 20000101.0000 20300101.0000 1.0\n",
  );
  assert.equal(field.isTimeVariable(), true);
  using metadata = field.header();
  assert.equal(metadata.format, IcgemFormat.Icgem2_0);
  using coefficient = field.coefficientAt(2, 0, t0);
  assert.equal(coefficient.c, -4.8e-4 + 1e-11);
  assert.equal(coefficient.s, 0);
  const [constant, cosine] = field.timeVariableTerms();
  const [anotherConstant, anotherCosine] = field.timeVariableTerms();
  try {
    assert.ok(constant.equals(anotherConstant));
    assert.ok(!constant.equals(anotherCosine));
    using kind = constant.kind;
    using cosineKind = cosine.kind;
    using constantKind = IcgemTermKind.CONSTANT();
    using trendKind = IcgemTermKind.TREND();
    using expectedCosine = IcgemTermKind.cosine(1);
    using sine = IcgemTermKind.sine(2);
    assert.ok(kind.equals(constantKind));
    assert.ok(cosineKind.equals(expectedCosine));
    assert.ok(!cosineKind.equals(sine));
    assert.equal(kind.label, "constant");
    assert.equal(trendKind.label, "trend");
    assert.equal(cosineKind.label, "cosine");
    assert.equal(sine.label, "sine");
    assert.equal(kind.periodYears, undefined);
    assert.equal(cosineKind.periodYears, 1);
    assert.equal(sine.periodYears, 2);
    assert.deepEqual(
      [
        constant.line,
        constant.degree,
        constant.order,
        constant.c,
        constant.s,
        constant.referenceTtSeconds,
        constant.validFromTtSeconds,
      ],
      [10, 2, 0, -4.8e-4, 0, t0, t0],
    );
    assert.equal(constant.validUntilTtSeconds, 946728000);
    using sigmas = constant.sigmas;
    assert.equal(sigmas.formal, undefined);
    using gravity = field.gravity(2, 0);
    assert.equal(gravity.isTimeVariable(), true);
  } finally {
    for (const item of [constant, cosine, anotherConstant, anotherCosine]) item.free();
  }
});

test("every ICGEM refusal includes its record location and typed payload", () => {
  const invalid = [
    [text.replace("radius 6378136.3\n", ""), { kind: "missingHeaderKeyword", keyword: "radius" }],
    [text.replace("end_of_head\n", ""), { kind: "missingEndOfHead" }],
    [
      text.replace("radius 6378136.3\n", "radius 6378136.3\nradius 6378137\n"),
      { kind: "repeatedHeaderKeyword", line: 5, firstLine: 4, keyword: "radius" },
    ],
    [
      text.replace("0.0 1e-11 1e-11", "0.0"),
      { kind: "missingField", line: 10, keyword: "gfc", field: "sigma C" },
    ],
    [
      text.replace("2.4e-6", "2.4x-6"),
      { kind: "invalidField", line: 11, keyword: "gfc", field: "C", value: "2.4x-6" },
    ],
    [
      text + "gfc 3 0 1e-6 0 0 0\n",
      { kind: "degreeAboveMaximum", line: 12, degree: 3, maxDegree: 2 },
    ],
    [text + "gfc 1 2 0 0 0 0\n", { kind: "orderAboveDegree", line: 12, degree: 1, order: 2 }],
    [
      text + "gfc 2 2 0 0 0 0\n",
      { kind: "repeatedCoefficient", line: 12, firstLine: 11, degree: 2, order: 2 },
    ],
    [
      text.replace("gfc 2 2", "gfx 2 2"),
      { kind: "unknownRecordKeyword", line: 11, keyword: "gfx" },
    ],
    [text.replace("gfc 2 2", "2 2"), { kind: "missingRecordKeyword", line: 11 }],
    [
      text.replace("max_degree 2", "max_degree 60000"),
      { kind: "tooFewRecords", maxDegree: 60000, required: 1800089998, lines: 3 },
    ],
    [
      text.replace("gfc 2 2 2.4e-6 -1.4e-6 1e-11 1e-11", "comment"),
      { kind: "missingCoefficient", degree: 2, order: 2 },
    ],
    [
      text + "trnd 1 0 1e-12 0 0 0\n",
      { kind: "uncoveredTerm", line: 12, keyword: "trnd", degree: 1, order: 0 },
    ],
    [variable + "dot 2 0 1e-12 0.0\n", { kind: "keywordNotInFormat", line: 13, keyword: "dot" }],
    [
      variable + "gfct 2 0 1e-12 0.0 20050101.0000 20310101.0000\n",
      { kind: "overlappingTerm", line: 13, firstLine: 10, keyword: "gfct", degree: 2, order: 0 },
    ],
  ];
  for (const [source, detail] of invalid) {
    assert.throws(() => IcgemGravityField.parse(source), typed(detail), detail.kind);
  }

  const unnormalized = header
    .replace("max_degree 2", "max_degree 170")
    .replace("errors formal", "errors no\nnorm unnormalized");
  const records = [];
  for (let degree = 2; degree <= 170; degree++) {
    for (let order = 0; order <= degree; order++) {
      records.push(`gfc ${degree} ${order} ${degree === 170 && order === 170 ? 1 : 0} 0`);
    }
  }
  assert.throws(
    () => IcgemGravityField.parse(unnormalized + records.join("\n")),
    typed({ kind: "normalizationOutOfRange", line: 14712, degree: 170, order: 170 }),
  );

  using field = IcgemGravityField.parse(text);
  using missingTide = IcgemGravityField.parse(text.replace("tide_system tide_free\n", ""));
  using temporal = IcgemGravityField.parse(variable);
  using shortPeriod = IcgemGravityField.parse(
    variable + "acos 2 0 1e-11 0.0 20000101.0000 20300101.0000 1e-320\n",
  );
  assert.throws(
    () => field.gravity(3, 0),
    typed({ kind: "truncationOutOfRange", degree: 3, order: 0, maxDegree: 2 }),
  );
  assert.throws(
    () => missingTide.gravityInTideSystem(2, 0, "meanTide"),
    typed({ kind: "unknownTideSystem" }),
  );
  assert.throws(
    () => field.withAssumedTideSystem("meanTide"),
    typed({ kind: "tideSystemConflict", stated: "tideFree", assumed: "meanTide" }),
  );
  assert.throws(
    () => temporal.coefficientAt(2, 0, t0 - 1),
    typed({ kind: "epochOutsideValidity", degree: 2, order: 0, epochTtSeconds: t0 - 1 }),
  );
  assert.throws(() => temporal.coefficientAt(2, 0, NaN), typed({ kind: "nonFiniteEpoch" }));
  assert.throws(
    () => shortPeriod.coefficientAt(2, 0, t0 + 1),
    typed({ kind: "nonFiniteCoefficient", degree: 2, order: 0, epochTtSeconds: t0 + 1 }),
  );
});

test("ICGEM normalization and error-column enums use their declared values", () => {
  assert.deepEqual(
    [IcgemErrors.No, IcgemErrors.Calibrated, IcgemErrors.Formal, IcgemErrors.CalibratedAndFormal],
    [0, 1, 2, 3],
  );
  assert.deepEqual([IcgemFormat.Icgem1_0, IcgemFormat.Icgem2_0], [0, 1]);
  assert.deepEqual([IcgemNormalization.FullyNormalized, IcgemNormalization.Unnormalized], [0, 1]);
});
