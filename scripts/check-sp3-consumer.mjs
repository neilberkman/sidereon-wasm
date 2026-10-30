import assert from "node:assert/strict";

// Shared by pre-publication tests and the clean npm consumer. Opposite clock
// differences keep the datum offset at zero while each mean falls half a
// column unit between the writable inputs.
export function checkSp3Consumer({ Sp3AccuracyRoundingPolicy, loadSp3, mergeSp3 }) {
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

  const accuracyProduct = (satellites, positionBase, clockBase, exponent) => {
    const satelliteHeader = satellites.join("") + "  0".repeat(17 - satellites.length);
    const code = (value) =>
      value === null
        ? ""
        : ` ${String(value).padStart(2)} ${String(value).padStart(2)} ${String(value).padStart(2)} ${String(value).padStart(3)}  `;
    const lines = [
      "#cV2020  6 25  0  0  0.00000000       2 ORBIT IGS14 FIT  TST",
      "## 2111 345600.00000000   300.00000000 59025 0.0000000000000",
      `+   ${String(satellites.length).padStart(2)}   ${satelliteHeader}`,
      `++       ${"  0".repeat(17)}`,
      "%c G  cc GPS ccc cccc cccc cccc cccc ccccc ccccc ccccc ccccc",
      "%c cc cc ccc ccc cccc cccc cccc cccc ccccc ccccc ccccc ccccc",
      `%f ${positionBase.toFixed(7).padStart(10)} ${clockBase.toFixed(9).padStart(12)}  0.00000000000  0.000000000000000`,
      "%f  0.0000000  0.000000000  0.00000000000  0.000000000000000",
      "%i    0    0    0    0      0      0      0      0         0",
      "%i    0    0    0    0      0      0      0      0         0",
      "/* PUBLIC SP3 ACCURACY WRITER FIXTURE",
    ];
    const positions = {
      G01: [15000, -20000, 5000],
      G02: [13000, -18000, 7000],
      G12: [16000, -19000, 4000],
    };
    for (let epoch = 0; epoch < 2; epoch += 1) {
      lines.push(`*  2020  6 25  0 ${String(epoch * 5).padStart(2)}  0.00000000`);
      for (const satellite of satellites) {
        const [x, y, z] = positions[satellite];
        const suffix = code(exponent);
        lines.push(
          `P${satellite}${(x + epoch).toFixed(6).padStart(14)}${(y - epoch).toFixed(6).padStart(14)}${(z + epoch).toFixed(6).padStart(14)}${(-50 + epoch).toFixed(6).padStart(14)}${suffix}`,
        );
        lines.push(
          `V${satellite}${(10000).toFixed(6).padStart(14)}${(-20000).toFixed(6).padStart(14)}${(30000).toFixed(6).padStart(14)}${(1).toFixed(6).padStart(14)}${suffix}`,
        );
      }
    }
    lines.push("EOF");
    return loadSp3(encoder.encode(`${lines.join("\n")}\n`));
  };

  const accuracyOptions = {
    combine: "precedence",
    precedenceScope: "cell",
    minAgree: 1,
    clockMinCommon: 1,
    positionToleranceM: 5,
    clockToleranceS: 50e-9,
    targetEpochIntervalS: 300,
    systems: ["G"],
  };
  const { sp3: noOp } = mergeSp3(
    [accuracyProduct(["G01", "G02"], 0, 0, null), accuracyProduct(["G02", "G12"], 1.25, 1.025, 13)],
    accuracyOptions,
  );
  try {
    const { text, report } = noOp.toSp3StringWithAccuracyRounding(
      Sp3AccuracyRoundingPolicy.Conservative,
    );
    assert.deepEqual(Object.values(report.accuracySigmas), Array(8).fill(0));
    const reread = loadSp3(encoder.encode(text));
    try {
      assert.equal(reread.recordAccuracy("G12", 0).p.positionSigmaM[0].value, 0.018189894035458565);
      assert.equal(reread.recordAccuracy("G01", 0).p.positionSigmaM[0].kind, "unknown");
    } finally {
      reread.free();
    }
  } finally {
    noOp.free();
  }

  const { sp3: changed } = mergeSp3(
    [accuracyProduct(["G01", "G02"], 2, 2, 1), accuracyProduct(["G02", "G12"], 1.25, 1.025, 13)],
    accuracyOptions,
  );
  try {
    const directSource = accuracyProduct(["G12"], 2, 2, 1);
    try {
      const { text: directText, report: directReport } =
        directSource.toSp3StringWithAccuracyRounding(Sp3AccuracyRoundingPolicy.Conservative);
      assert.deepEqual(Object.values(directReport.accuracySigmas), Array(8).fill(0));
      const directOutput = loadSp3(encoder.encode(directText));
      try {
        assert.equal(
          directOutput.recordAccuracy("G12", 0).v.velocitySigmaMS[0].value,
          directSource.recordAccuracy("G12", 0).v.velocitySigmaMS[0].value,
        );
      } finally {
        directOutput.free();
      }
    } finally {
      directSource.free();
    }
    const { text, report } = changed.toSp3StringWithAccuracyRounding(
      Sp3AccuracyRoundingPolicy.Conservative,
    );
    const reread = loadSp3(encoder.encode(text));
    try {
      const source = changed.recordAccuracy("G12", 0);
      const output = reread.recordAccuracy("G12", 0);
      const actualDeltas = [
        ...source.p.positionSigmaM.map(
          (value, index) => output.p.positionSigmaM[index].value - value.value,
        ),
        output.p.clockSigmaM.value - source.p.clockSigmaM.value,
      ];
      const reportDeltas = Object.values(report.accuracySigmas);
      assert.deepEqual([...reportDeltas.slice(0, 3), reportDeltas[6]], actualDeltas);
      assert.ok(reportDeltas.slice(0, 3).every((delta) => delta > 0));
      assert.deepEqual(reportDeltas.slice(3, 6), Array(3).fill(0));
      assert.ok(reportDeltas[6] > 0);
      assert.equal(reportDeltas[7], 0);
      assert.equal(source.v, undefined);
      assert.equal(output.v, undefined);
      assert.deepEqual(
        output.p.positionSigmaM.map((value) => value.value),
        [0.032, 0.032, 0.032],
      );
      assert.equal(output.p.clockSigmaM.value, 2 * (299792458 * 1e-12));
    } finally {
      reread.free();
    }
  } finally {
    changed.free();
  }

  const requiredPolicy = accuracyProduct(["G01"], 1.25, 1.025, 13);
  try {
    assert.throws(() => requiredPolicy.toSp3StringWithAccuracyRounding(), TypeError);
    assert.throws(() => requiredPolicy.toSp3StringWithAccuracyRounding(null), TypeError);
    assert.throws(() => requiredPolicy.toSp3StringWithAccuracyRounding("lossy"), TypeError);
  } finally {
    requiredPolicy.free();
  }

  const { sp3: unrepresentable } = mergeSp3(
    [accuracyProduct(["G01", "G02"], 1.25, 1.025, 1), accuracyProduct(["G02", "G12"], 2, 2, 98)],
    accuracyOptions,
  );
  try {
    assert.throws(
      () => unrepresentable.toSp3StringWithAccuracyRounding(Sp3AccuracyRoundingPolicy.Conservative),
      (error) =>
        error.name === "Sp3WriteError" && error.detail.kind === "ACCURACY_NOT_REPRESENTABLE",
    );
  } finally {
    unrepresentable.free();
  }
}
