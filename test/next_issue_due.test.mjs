import { test } from "node:test";
import assert from "node:assert/strict";

import { GnssProductIdentity, NominalIssue, nextIssueDue } from "../pkg-node/sidereon.js";

test("nextIssueDue maps exact identity, UTC deadline, and split coverage", () => {
  const issue = nextIssueDue("igs_ult", "sp3", new Date("2026-08-04T02:59:59Z"));

  assert.ok(issue instanceof NominalIssue);
  assert.ok(issue.identity instanceof GnssProductIdentity);
  assert.equal(issue.identity.analysisCenter, "igs_ult");
  assert.equal(issue.identity.issue, "0000");
  assert.equal(issue.dueAt.toISOString(), "2026-08-04T03:00:00.000Z");
  assert.deepEqual(
    {
      observed: {
        from: issue.covers.observed.from.toISOString(),
        until: issue.covers.observed.until.toISOString(),
      },
      predicted: {
        from: issue.covers.predicted.from.toISOString(),
        until: issue.covers.predicted.until.toISOString(),
      },
    },
    {
      observed: {
        from: "2026-08-03T00:00:00.000Z",
        until: "2026-08-04T00:00:00.000Z",
      },
      predicted: {
        from: "2026-08-04T00:00:00.000Z",
        until: "2026-08-05T00:00:00.000Z",
      },
    },
  );
});

test("nextIssueDue rounds fractional instants up and crosses the final GPS week", () => {
  const afterBoundary = nextIssueDue("igs_ult", "sp3", new Date("2026-08-04T03:00:00.001Z"));
  assert.equal(afterBoundary.identity.issue, "0600");
  assert.equal(afterBoundary.dueAt.toISOString(), "2026-08-04T09:00:00.000Z");

  const nextFinal = nextIssueDue("igs", "sp3", new Date("2026-08-22T00:00:00Z"));
  assert.equal(nextFinal.identity.year, 2026);
  assert.equal(nextFinal.identity.month, 8);
  assert.equal(nextFinal.identity.day, 15);
  assert.equal(nextFinal.dueAt.toISOString(), "2026-08-28T23:59:59.000Z");
  assert.equal(nextFinal.covers.predicted, null);
});

test("nextIssueDue rejects invalid dates and unsupported schedules", () => {
  assert.throws(() => nextIssueDue("igs_ult", "sp3", new Date(Number.NaN)), TypeError);
  assert.throws(
    () => nextIssueDue("wum_nrt", "sp3", new Date("2026-08-04T00:00:00Z")),
    /nominal due-time schedule/,
  );
});
