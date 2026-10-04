import { test } from "node:test";
import assert from "node:assert/strict";

import { fitSp3EcefPreciseOrbit, fitSp3EcefPreciseOrbits, loadSp3 } from "../pkg-node/sidereon.js";
import { fixture } from "./helpers.mjs";

test("precise-orbit fit refusals preserve typed core variants and fields", () => {
  const sp3 = loadSp3(fixture("sp3/g02_ecef_two_epoch.sp3"));
  const cases = [
    [
      "EMPTY_SELECTION",
      () => fitSp3EcefPreciseOrbits(sp3, []),
      "no satellites selected for precise-orbit fitting",
      { kind: "EMPTY_SELECTION", message: "no satellites selected for precise-orbit fitting" },
    ],
    [
      "TOO_FEW_SAMPLES",
      () => fitSp3EcefPreciseOrbit(sp3, "G63"),
      "satellite G63 has 0 samples; need at least 2",
      {
        kind: "TOO_FEW_SAMPLES",
        message: "satellite G63 has 0 samples; need at least 2",
        satellite: "G63",
        got: 0,
        required: 2,
      },
    ],
    [
      "INVALID_OPTION",
      () => fitSp3EcefPreciseOrbit(sp3, "G02", { minLedgerSamples: 0 }),
      "invalid orbit-fit min_ledger_samples: not positive",
      {
        kind: "INVALID_OPTION",
        message: "invalid orbit-fit min_ledger_samples: not positive",
        field: "min_ledger_samples",
        reason: "not positive",
      },
    ],
    [
      "LEAST_SQUARES",
      () => fitSp3EcefPreciseOrbit(sp3, "G02", { solverOptions: { maxNfev: 0 } }),
      "satellite G02 least-squares failed: invalid least-squares max_nfev: not positive",
      {
        kind: "LEAST_SQUARES",
        message: "satellite G02 least-squares failed: invalid least-squares max_nfev: not positive",
        satellite: "G02",
        source: { kind: "invalidInput", field: "max_nfev", reason: "not positive" },
      },
    ],
  ];

  try {
    for (const [kind, call, message, detail] of cases) {
      assert.throws(call, (error) => {
        assert.equal(error.name, "OrbitFitError", kind);
        assert.equal(error.message, message, kind);
        assert.deepEqual(error.detail, detail, kind);
        return true;
      });
    }
  } finally {
    sp3.free();
  }
});
