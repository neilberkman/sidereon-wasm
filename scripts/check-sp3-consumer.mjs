import assert from "node:assert/strict";

// Shared by pre-publication tests and the clean npm consumer. Opposite clock
// differences keep the datum offset at zero while each mean falls half a
// column unit between the writable inputs.
export function checkSp3Consumer({ loadSp3, mergeSp3 }) {
  const encoder = new TextEncoder();
  const product = (clocks) => {
    const lines = [
      "#cP2020  6 24  0  0  0.00000000       1 ORBIT IGS14 FIT  TST",
      "## 2111 259200.00000000   900.00000000 59024 0.0000000000000",
      "+    3   G01G02G03",
      "++         0  0  0",
      "%c M  cc GPS ccc cccc cccc cccc cccc ccccc ccccc ccccc ccccc",
      "%c cc cc ccc ccc cccc cccc cccc cccc ccccc ccccc ccccc ccccc",
      "%f  1.2500000  1.025000000  0.00000000000  0.000000000000000",
      "%f  0.0000000  0.000000000  0.00000000000  0.000000000000000",
      "%i    0    0    0    0      0      0      0      0         0",
      "%i    0    0    0    0      0      0      0      0         0",
      "*  2020  6 24  0  0  0.00000000",
    ];
    for (let index = 0; index < clocks.length; index++) {
      const fields = [15600 + index, 20180, 21700, clocks[index]];
      lines.push(`PG0${index + 1}${fields.map((v) => v.toFixed(6).padStart(14)).join("")}`);
    }
    return loadSp3(encoder.encode(`${lines.join("\n")}\nEOF\n`));
  };
  const left = product([-598.410235, 138.983111, 0]);
  const right = product([-598.410234, 138.98311, 0]);
  assert.ok(left.toSp3String().endsWith("EOF\n"));
  assert.ok(right.toSp3String().endsWith("EOF\n"));
  const { sp3: merged } = mergeSp3([left, right], { clockMinCommon: 3 });
  try {
    const text = merged.toSp3String();
    const rounded = merged.toSp3StringWithRounding();
    assert.equal(rounded.text, text);
    assert.deepEqual(rounded.report, {
      positionXMaxChangeM: 0,
      positionYMaxChangeM: 0,
      positionZMaxChangeM: 0,
      velocityXMaxChangeMPerS: 0,
      velocityYMaxChangeMPerS: 0,
      velocityZMaxChangeMPerS: 0,
      clockMaxChangeS: 0,
      clockRateMaxChangeSPerS: 0,
    });
    const reread = loadSp3(encoder.encode(text));
    try {
      assert.equal(reread.toSp3String(), text);
      for (const sat of ["G01", "G02", "G03"]) {
        assert.equal(reread.state(sat, 0).clockS, merged.state(sat, 0).clockS);
      }
    } finally {
      reread.free();
    }
  } finally {
    merged.free();
  }
}
