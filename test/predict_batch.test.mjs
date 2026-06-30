// Serial batch observable prediction over sidereon_core::observables::
// predict_batch. Cross-checked against the single-satellite observablesSp3 path:
// element i of the batch must be bit-identical to the scalar predict for the
// same (satellite, receiver, epoch).

import { test } from "node:test";
import assert from "node:assert/strict";

import { loadSp3, observablesSp3, predictBatchSp3 } from "../pkg-node/sidereon.js";
import { fixture } from "./helpers.mjs";

const setup = () => loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
const RECEIVER = [4_500_000.0, 500_000.0, 4_500_000.0];
const EPOCH = 646_272_000.0;

test("predictBatchSp3 matches the scalar observablesSp3 per request", () => {
  const sp3 = setup();
  const sats = ["G01", "G02"];
  const receivers = Float64Array.from([...RECEIVER, ...RECEIVER]);
  const epochs = Float64Array.from([EPOCH, EPOCH]);

  const batch = predictBatchSp3(sp3, sats, receivers, epochs, undefined);
  assert.equal(batch.count, 2);

  sats.forEach((sat, i) => {
    assert.equal(batch.isOk(i), true);
    assert.equal(batch.error(i), undefined);
    const single = observablesSp3(sp3, sat, Float64Array.from(RECEIVER), EPOCH, undefined);
    const obs = batch.observables(i);
    for (const attr of [
      "geometricRangeM",
      "rangeRateMS",
      "dopplerHz",
      "elevationDeg",
      "azimuthDeg",
    ]) {
      assert.equal(obs[attr], single[attr]);
    }
  });
});

test("predictBatchSp3 honors the predict options object", () => {
  const sp3 = setup();
  const batch = predictBatchSp3(
    sp3,
    ["G01"],
    Float64Array.from(RECEIVER),
    Float64Array.from([EPOCH]),
    { carrierHz: 1.57542e9, sagnac: true },
  );
  const single = observablesSp3(sp3, "G01", Float64Array.from(RECEIVER), EPOCH, {
    carrierHz: 1.57542e9,
    sagnac: true,
  });
  assert.equal(batch.observables(0).dopplerHz, single.dopplerHz);
});

test("predictBatchSp3 rejects mismatched input lengths and bad indices", () => {
  const sp3 = setup();
  assert.throws(
    () =>
      predictBatchSp3(
        sp3,
        ["G01", "G02"],
        Float64Array.from(RECEIVER),
        Float64Array.from([EPOCH]),
        undefined,
      ),
    TypeError,
  );
  const batch = predictBatchSp3(
    sp3,
    ["G01"],
    Float64Array.from(RECEIVER),
    Float64Array.from([EPOCH]),
    undefined,
  );
  assert.throws(() => batch.observables(5), RangeError);
});
