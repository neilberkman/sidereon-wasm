import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { productIdentity, sp3MergeInputIdentity } from "../pkg-node/sidereon.js";

const golden = JSON.parse(
  readFileSync(new URL("./fixtures/sp3-merge-input-v1.json", import.meta.url), "utf8"),
);

function goldenIdentity(value) {
  const [year, month, day] = value.date.split("-").map(Number);
  return {
    family: value.family,
    analysisCenter: value.analysis_center,
    publisher: value.publisher,
    solutionClass: value.solution,
    campaign: value.campaign,
    filenameVersion: value.version,
    year,
    month,
    day,
    issue: value.issue ?? undefined,
    span: value.span,
    sample: value.sample,
    officialFilename: value.official_filename,
    format: value.format,
    formatVersion: value.format_version ?? undefined,
    predictionHorizonDays: value.prediction_horizon_days ?? undefined,
  };
}

function goldenArtifact(value) {
  return {
    requestedIdentity: goldenIdentity(value.requested_identity),
    resolvedIdentity: goldenIdentity(value.resolved_identity),
    distributionSource: value.distribution_source,
    officialFilename: value.official_filename,
    productSha256: value.product_sha256,
    productByteLength: value.product_byte_length,
    archiveSha256: value.archive_sha256,
    archiveByteLength: value.archive_byte_length,
    compression: value.compression,
  };
}

function goldenPolicy(combine) {
  const value = golden.complete_policy;
  return {
    positionToleranceM: value.position_tolerance_m,
    clockToleranceS: value.clock_tolerance_s,
    minAgree: value.min_agree,
    clockMinCommon: value.clock_min_common,
    combine,
    precedenceScope: value.precedence_scope,
    outlierReject: {
      positionToleranceM: value.outlier_reject.position_tolerance_m,
      clockToleranceS: value.outlier_reject.clock_tolerance_s,
    },
    targetEpochIntervalS: value.target_epoch_interval_s,
    systems: value.systems,
    assertedFrameLabelSets: value.frame_reconciliation.asserted_equivalent_label_sets,
    helmert: value.frame_reconciliation.helmert,
  };
}

function identityRecord(identity, formatVersion = identity.formatVersion) {
  return {
    family: identity.family,
    analysisCenter: identity.analysisCenter,
    publisher: identity.publisher,
    solutionClass: identity.solutionClass,
    campaign: identity.campaign,
    filenameVersion: identity.filenameVersion,
    year: identity.year,
    month: identity.month,
    day: identity.day,
    issue: identity.issue,
    span: identity.span,
    sample: identity.sample,
    officialFilename: identity.officialFilename,
    format: identity.format,
    formatVersion,
    predictionHorizonDays: identity.predictionHorizonDays,
  };
}

function artifact(center, day, digestByte) {
  const identity = productIdentity(center, "sp3", 2026, 7, day);
  return {
    requestedIdentity: identityRecord(identity),
    resolvedIdentity: identityRecord(identity, "d"),
    distributionSource: "direct",
    officialFilename: identity.officialFilename,
    productSha256: digestByte.repeat(64),
    productByteLength: 12345,
    archiveSha256: (digestByte === "1" ? "2" : "3").repeat(64),
    archiveByteLength: 6789,
    compression: "gzip",
  };
}

test("mean merged-SP3 identity is canonical across contributor and object ordering", () => {
  const first = artifact("esa", 12, "1");
  const second = artifact("cod", 13, "2");
  const forward = sp3MergeInputIdentity([first, second], {
    combine: "mean",
    systems: ["E", "G"],
  });
  const reverse = sp3MergeInputIdentity([second, first], {
    systems: ["G", "E"],
    combine: "mean",
  });

  assert.equal(forward.schemaVersion, 1);
  assert.match(forward.stableId, /^sidereon-sp3-merge-input-v1:[0-9a-f]{64}$/);
  assert.equal(forward.stableId, reverse.stableId);
  assert.deepEqual(forward.contributors, reverse.contributors);
  assert.equal(forward.contributors.length, 2);
  assert.equal(forward.precedenceContributors, undefined);
});

test("precedence merged-SP3 identity binds contributor priority order", () => {
  const first = artifact("esa", 12, "1");
  const second = artifact("cod", 13, "2");
  const forward = sp3MergeInputIdentity([first, second], { combine: "precedence" });
  const reverse = sp3MergeInputIdentity([second, first], { combine: "precedence" });

  assert.notEqual(forward.stableId, reverse.stableId);
  assert.equal(forward.precedenceContributors[0].productSha256, first.productSha256);
  assert.equal(reverse.precedenceContributors[0].productSha256, second.productSha256);
});

test("shared literal golden fixture matches every canonical identity", () => {
  const esa = goldenArtifact(golden.artifacts.esa);
  const cod = goldenArtifact(golden.artifacts.cod);
  const mean = sp3MergeInputIdentity([esa, cod], goldenPolicy("mean"));
  const reversedMean = sp3MergeInputIdentity([cod, esa], goldenPolicy("mean"));
  const median = sp3MergeInputIdentity([esa, cod], goldenPolicy("median"));
  const precedence = sp3MergeInputIdentity([esa, cod], goldenPolicy("precedence"));
  const reversedPrecedence = sp3MergeInputIdentity([cod, esa], goldenPolicy("precedence"));
  const single = sp3MergeInputIdentity([esa], goldenPolicy("mean"));

  assert.equal(mean.stableId, golden.expected.mean_esa_cod);
  assert.equal(reversedMean.stableId, golden.expected.mean_esa_cod);
  assert.equal(median.stableId, golden.expected.median_esa_cod);
  assert.equal(precedence.stableId, golden.expected.precedence_esa_cod);
  assert.equal(reversedPrecedence.stableId, golden.expected.precedence_cod_esa);
  assert.equal(single.stableId, golden.expected.single_mean_esa);
  assert.deepEqual(mean.contributors, reversedMean.contributors);
  assert.equal(mean.precedenceContributors, undefined);
  assert.deepEqual(precedence.precedenceContributors, [esa, cod]);
  assert.deepEqual(reversedPrecedence.precedenceContributors, [cod, esa]);

  const negativeZeroPolicy = goldenPolicy("mean");
  negativeZeroPolicy.positionToleranceM = -0;
  negativeZeroPolicy.clockToleranceS = -0;
  assert.equal(
    sp3MergeInputIdentity([esa, cod], negativeZeroPolicy).stableId,
    sp3MergeInputIdentity([esa, cod], {
      ...negativeZeroPolicy,
      positionToleranceM: 0,
      clockToleranceS: 0,
    }).stableId,
  );

  const changedBytes = structuredClone(cod);
  changedBytes.productSha256 = golden.required_mutations.changed_product_sha256;
  assert.notEqual(
    sp3MergeInputIdentity([esa, changedBytes], goldenPolicy("mean")).stableId,
    mean.stableId,
  );
  const changedFormat = structuredClone(cod);
  changedFormat.resolvedIdentity.formatVersion =
    golden.required_mutations.changed_resolved_format_version;
  assert.notEqual(
    sp3MergeInputIdentity([esa, changedFormat], goldenPolicy("mean")).stableId,
    mean.stableId,
  );
  const changedPolicy = goldenPolicy("mean");
  changedPolicy.clockToleranceS = golden.required_mutations.changed_clock_tolerance_s;
  assert.notEqual(sp3MergeInputIdentity([esa, cod], changedPolicy).stableId, mean.stableId);

  const malformed = structuredClone(cod);
  malformed.productSha256 = golden.required_mutations.malformed_product_sha256;
  assert.throws(() => sp3MergeInputIdentity([esa, malformed], goldenPolicy("mean")), /SHA-256/);
  assert.throws(
    () =>
      sp3MergeInputIdentity([esa, cod], {
        ...goldenPolicy("mean"),
        targetEpochIntervalS: golden.required_mutations.fractional_target_epoch_interval_s,
      }),
    /whole number of seconds/,
  );
  assert.throws(
    () => sp3MergeInputIdentity([esa, cod], { ...goldenPolicy("mean"), systems: [] }),
    /must not be empty/,
  );
});

test("artifact byte lengths require exact safe JavaScript integers", () => {
  const contributor = artifact("esa", 12, "1");
  for (const value of [0, -1, 1.5, Number.NaN, Number.POSITIVE_INFINITY, 2 ** 53]) {
    const changed = structuredClone(contributor);
    changed.productByteLength = value;
    assert.throws(
      () => sp3MergeInputIdentity([changed], undefined),
      /positive safe integer Number/,
    );
  }
  const bigint = structuredClone(contributor);
  bigint.archiveByteLength = 123n;
  assert.throws(() => sp3MergeInputIdentity([bigint], undefined), /positive safe integer Number/);
  const largestSafe = structuredClone(contributor);
  largestSafe.productByteLength = Number.MAX_SAFE_INTEGER;
  assert.equal(sp3MergeInputIdentity([largestSafe], undefined).schemaVersion, 1);
});

test("artifact bytes and the effective merge policy change the stable identity", () => {
  const first = artifact("esa", 12, "1");
  const second = artifact("cod", 13, "2");
  const original = sp3MergeInputIdentity([first, second], undefined);
  const changedArtifact = structuredClone(second);
  changedArtifact.productSha256 = "4".repeat(64);
  const changedBytes = sp3MergeInputIdentity([first, changedArtifact], undefined);
  const changedPolicy = sp3MergeInputIdentity([first, second], { combine: "median" });

  assert.notEqual(original.stableId, changedBytes.stableId);
  assert.notEqual(original.stableId, changedPolicy.stableId);
});

test("Unix-compress artifact provenance is accepted and bound", () => {
  const identity = productIdentity("igs", "sp3", 2022, 11, 26);
  const contributor = {
    requestedIdentity: identityRecord(identity),
    resolvedIdentity: identityRecord(identity, "d"),
    distributionSource: "nasa_cddis",
    officialFilename: identity.officialFilename,
    productSha256: "a".repeat(64),
    productByteLength: 12345,
    archiveSha256: "b".repeat(64),
    archiveByteLength: 6789,
    compression: "unix_compress",
  };

  const compressed = sp3MergeInputIdentity([contributor], undefined);
  const mislabeled = structuredClone(contributor);
  mislabeled.compression = "gzip";
  assert.equal(compressed.contributors[0].compression, "unix_compress");
  assert.notEqual(compressed.stableId, sp3MergeInputIdentity([mislabeled], undefined).stableId);
});

test("zero position and clock tolerances are valid identity policy", () => {
  const contributor = artifact("esa", 12, "1");
  const exact = sp3MergeInputIdentity([contributor], {
    positionToleranceM: 0,
    clockToleranceS: 0,
  });

  assert.equal(exact.schemaVersion, 1);
  assert.match(exact.stableId, /^sidereon-sp3-merge-input-v1:[0-9a-f]{64}$/);
  assert.throws(
    () => sp3MergeInputIdentity([contributor], { positionToleranceM: -1 }),
    /non-negative and finite/,
  );
  assert.throws(
    () => sp3MergeInputIdentity([contributor], { targetEpochIntervalS: 0 }),
    /positive and finite/,
  );
  assert.throws(
    () => sp3MergeInputIdentity([contributor], { targetEpochIntervalS: 1.5 }),
    /whole number of seconds/,
  );
});

test("single contributors work and incomplete, mismatched, or extra records fail closed", () => {
  const contributor = artifact("esa", 12, "1");
  assert.equal(sp3MergeInputIdentity([contributor], undefined).schemaVersion, 1);
  assert.throws(() => sp3MergeInputIdentity([], undefined), /at least one contributor/);

  const incomplete = structuredClone(contributor);
  delete incomplete.archiveSha256;
  assert.throws(() => sp3MergeInputIdentity([incomplete], undefined), /archiveSha256/);

  const mismatched = structuredClone(contributor);
  mismatched.resolvedIdentity = identityRecord(productIdentity("cod", "sp3", 2026, 7, 12), "d");
  assert.throws(() => sp3MergeInputIdentity([mismatched], undefined), /does not match/);

  const secretBearing = { ...contributor, authorization: "not-canonical-input" };
  assert.throws(() => sp3MergeInputIdentity([secretBearing], undefined), /unknown field/);
  assert.throws(
    () => sp3MergeInputIdentity([contributor], { unboundPolicyField: true }),
    /unknown field/,
  );
});
