import { test } from "node:test";
import assert from "node:assert/strict";

import { loadSp3 } from "../pkg-node/sidereon.js";
import { fixture } from "./helpers.mjs";

const shiftClockUs = (line, deltaUs) => {
  if (!line.startsWith("P")) return line;
  const clock = Number(line.slice(46, 60));
  const shifted = (clock + deltaUs).toFixed(6).padStart(14);
  return `${line.slice(0, 46)}${shifted}${line.slice(60)}`;
};

const shiftedProduct = (deltaUs) => {
  const text = fixture("sp3/degenerate_coincident_5sat.sp3").toString("utf8");
  return loadSp3(Buffer.from(text.split("\n").map((line) => shiftClockUs(line, deltaUs)).join("\n")));
};

test("SP3 clock reference offset and alignment delegate to the core pair", () => {
  const reference = loadSp3(fixture("sp3/degenerate_coincident_5sat.sp3"));
  const shifted = shiftedProduct(50.0);

  const offsets = reference.clockReferenceOffset(shifted, 3);
  assert.equal(offsets.length, reference.epochCount);
  for (const offset of offsets) {
    assert.equal(offset.satellites, 5);
    assert.ok(Math.abs(offset.offsetS - 5.0e-5) <= 1.0e-12);
    assert.ok(Number.isFinite(offset.epochJ2000Seconds));
  }

  const aligned = reference.alignClockReference(shifted, 3);
  const residuals = reference.clockReferenceOffset(aligned, 3);
  assert.equal(residuals.length, offsets.length);
  for (const residual of residuals) {
    assert.ok(Math.abs(residual.offsetS) <= 1.0e-12);
  }

  assert.throws(() => reference.clockReferenceOffset(shifted, 0), RangeError);
  assert.throws(() => reference.alignClockReference(shifted, 0), RangeError);
});
