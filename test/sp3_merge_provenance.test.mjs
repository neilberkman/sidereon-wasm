import assert from "node:assert/strict";
import test from "node:test";

import { productIdentity, sp3MergeInputIdentity } from "../pkg-node/sidereon.js";

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
});

test("precedence merged-SP3 identity binds contributor priority order", () => {
  const first = artifact("esa", 12, "1");
  const second = artifact("cod", 13, "2");
  const forward = sp3MergeInputIdentity([first, second], { combine: "precedence" });
  const reverse = sp3MergeInputIdentity([second, first], { combine: "precedence" });

  assert.notEqual(forward.stableId, reverse.stableId);
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
