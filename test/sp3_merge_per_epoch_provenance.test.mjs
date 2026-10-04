// Per-epoch merge provenance through the WASM binding: which contributor
// supplied each cell, where selection changed, and what each contributor
// covered. The sources are the engine's own provenance fixtures, built as
// text here, so every expectation is a property of an input this file states.

import { test } from "node:test";
import assert from "node:assert/strict";

import { loadSp3, mergeSp3 } from "../pkg-node/sidereon.js";

const encode = new TextEncoder();

function sp3(firstEpoch, records) {
  const lines = [
    `#cP2020  6 25  0 ${firstEpoch}  0.00000000       ${records.length} ORBIT IGS14 FIT  TST`,
    `## 2111 ${firstEpoch === " 0" ? "432000.00000000" : "432900.00000000"}   900.00000000 59025 0.0000000000000`,
    "+    1   G01  0  0  0  0  0  0  0  0  0  0  0  0  0  0  0  0",
    "++         0  0  0  0  0  0  0  0  0  0  0  0  0  0  0  0  0",
    "%c G  cc GPS ccc cccc cccc cccc cccc ccccc ccccc ccccc ccccc",
    "%c cc cc ccc ccc cccc cccc cccc cccc ccccc ccccc ccccc ccccc",
    "%f  1.2500000  1.025000000  0.00000000000  0.000000000000000",
    "%f  0.0000000  0.000000000  0.00000000000  0.000000000000000",
    "%i    0    0    0    0      0      0      0      0         0",
    "%i    0    0    0    0      0      0      0      0         0",
    "/* TEST SP3-c FIXTURE",
  ];
  for (const [minute, xKm] of records) {
    lines.push(`*  2020  6 25  0 ${minute}  0.00000000`);
    lines.push(`PG01${xKm.toFixed(6).padStart(14)} -20000.000000   5000.000000    100.000000`);
  }
  lines.push("EOF");
  return loadSp3(encode.encode(`${lines.join("\n")}\n`));
}

// G01 at 00:00 and 00:15 with the given X coordinates, kilometres.
const source = ([x0, x1]) =>
  sp3(" 0", [
    [" 0", x0],
    ["15", x1],
  ]);
// G01 at 00:00 only.
const earlySource = (x0) => sp3(" 0", [[" 0", x0]]);
// G01 at 00:15 only.
const lateSource = (x1) => sp3("15", [["15", x1]]);

const precedence = (provenance) => ({ combine: "precedence", minAgree: 1, provenance });
const selectedSource = (selection) =>
  selection.kind === "combined" ? undefined : selection.source;

test("provenance is null unless requested", () => {
  const { report } = mergeSp3([source([15_000, 15_100])], precedence(undefined));
  assert.equal(report.provenance, null);
});

test("a single-contributor merge records it for every epoch with no mid-arc transition", () => {
  const axis = source([15_000, 15_100]).epochsJ2000Seconds();
  const { report } = mergeSp3([source([15_000, 15_100])], precedence("full"));
  const provenance = report.provenance;

  assert.equal(provenance.mode, "full");
  assert.equal(provenance.cells.length, 2);
  for (const [i, cell] of provenance.cells.entries()) {
    assert.equal(cell.satellite, "G01");
    assert.equal(cell.epochJ2000Seconds, axis[i]);
    assert.equal(selectedSource(cell.position), 0);
  }

  // The arc's opening entry is a transition from no source; there is no
  // further change.
  assert.equal(provenance.transitions.length, 1);
  assert.equal(provenance.transitions[0].fromSource, undefined);
  assert.equal(provenance.transitions[0].toSource, 0);

  assert.deepEqual(provenance.coverage, [
    {
      source: 0,
      cellsContributed: 2,
      cellsSelected: 2,
      firstEpochJ2000Seconds: axis[0],
      lastEpochJ2000Seconds: axis[1],
      cellsAbsent: 0,
    },
  ]);
});

test("a forced precedence switch records one transition naming both sides", () => {
  // Source 0 carries only the first epoch and source 1 only the second, so
  // under cell precedence the supplier changes at the second epoch.
  const { report } = mergeSp3([earlySource(15_000), lateSource(15_100)], precedence("full"));
  const provenance = report.provenance;

  assert.equal(provenance.cells.length, 2);
  assert.equal(selectedSource(provenance.cells[0].position), 0);
  assert.equal(selectedSource(provenance.cells[1].position), 1);

  const changes = provenance.transitions.filter(
    (transition) => transition.fromSource !== undefined,
  );
  assert.equal(changes.length, 1);
  assert.equal(changes[0].fromSource, 0);
  assert.equal(changes[0].toSource, 1);
  assert.equal(changes[0].reason, "sole_availability");

  assert.equal(provenance.coverage[0].cellsContributed, 1);
  assert.equal(provenance.coverage[0].cellsAbsent, 1);
  assert.equal(provenance.coverage[1].cellsContributed, 1);
  assert.equal(provenance.coverage[1].cellsAbsent, 1);
});

test("outlier rejection is recorded as its own reason", () => {
  // Source 0 leaves the other two at the second epoch and the guard rejects
  // it while it is still present.
  const { report } = mergeSp3(
    [source([15_000, 25_000]), source([15_000, 15_100]), source([15_000, 15_100])],
    {
      combine: "precedence",
      minAgree: 2,
      positionToleranceM: 1,
      outlierReject: { positionToleranceM: 1, clockToleranceS: 1e-6 },
      provenance: "full",
    },
  );
  const provenance = report.provenance;

  assert.equal(selectedSource(provenance.cells[0].position), 0);
  assert.equal(selectedSource(provenance.cells[1].position), 1);
  const changes = provenance.transitions.filter(
    (transition) => transition.fromSource !== undefined,
  );
  assert.equal(changes.length, 1);
  assert.equal(changes[0].reason, "outlier_rejection");
  assert.ok(report.positionOutlierCount > 0);
});

test("a combined cell names no single supplier", () => {
  const { report } = mergeSp3([source([15_000, 15_100]), source([15_000, 15_100])], {
    provenance: "full",
  });
  const provenance = report.provenance;

  for (const cell of provenance.cells) {
    assert.deepEqual(cell.position, { kind: "combined", rule: "mean", members: [0, 1] });
  }
  for (const coverage of provenance.coverage) {
    assert.equal(coverage.cellsSelected, 0);
    assert.equal(coverage.cellsContributed, 2);
  }
});

test("summary and full modes agree on every transition and coverage they both describe", () => {
  const full = mergeSp3([source([15_000, 15_100]), lateSource(15_100)], precedence("full")).report
    .provenance;
  const summary = mergeSp3([source([15_000, 15_100]), lateSource(15_100)], precedence("summary"))
    .report.provenance;

  assert.equal(full.mode, "full");
  assert.equal(summary.mode, "summary");
  assert.deepEqual(full.transitions, summary.transitions);
  assert.deepEqual(full.coverage, summary.coverage);
  assert.deepEqual(summary.cells, []);
  assert.ok(full.cells.length > 0);
});

test("an unknown provenance mode or merge option is refused", () => {
  assert.throws(() => mergeSp3([source([15_000, 15_100])], { provenance: "all" }), TypeError);
  assert.throws(
    () => mergeSp3([source([15_000, 15_100])], { provenence: "full" }),
    /unknown field "provenence"/,
  );
});
