import { test } from "node:test";
import assert from "node:assert/strict";

import { EmissionMediaStatus, emissionMediaStatusLabel, loadSp3 } from "../pkg-node/sidereon.js";
import { coreGoldens, fixture, f64Bits, geodeticToEcef, hexToF64 } from "./helpers.mjs";

// Expected values are the core's own batch for the same inputs
// (test/golden-gen `emissionMedia`); the receiver and epoch are the golden's
// exact bits, so the binding and the engine see identical inputs.
const golden = coreGoldens().emissionMedia;
const goldenEpoch = hexToF64(golden.epochJ2000S);
const goldenReceiver = Float64Array.from(golden.receiverEcefM.map(hexToF64));
const bitsOrNaN = (values) =>
  Array.from(values).map((value) => (Number.isNaN(value) ? "NaN" : f64Bits(value)));
const goldenBits = (rows) => rows.map((value) => (value === null ? "NaN" : BigInt(value)));

test("emissionMediaBatch returns contiguous arrays and typed row statuses", () => {
  const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const epoch = sp3.epochsJ2000Seconds()[40];
  assert.equal(f64Bits(epoch), f64Bits(goldenEpoch));
  const batch = sp3.emissionMediaBatch(
    ["G16", "E01", "C01"],
    Float64Array.from([epoch, epoch, epoch]),
    goldenReceiver,
    { troposphere: true },
  );

  assert.equal(batch.count, 3);
  assert.equal(emissionMediaStatusLabel(EmissionMediaStatus.Valid), "valid");
  assert.equal(emissionMediaStatusLabel(EmissionMediaStatus.Gap), "gap");
  assert.deepEqual(batch.statusLabels, ["valid", "valid", "gap"]);
  assert.deepEqual(batch.statuses, [
    EmissionMediaStatus.Valid,
    EmissionMediaStatus.Valid,
    EmissionMediaStatus.Gap,
  ]);
  assert.deepEqual(
    bitsOrNaN(batch.positionEcefM),
    golden.all.positionsEcefM.flatMap((row) =>
      row === null ? ["NaN", "NaN", "NaN"] : row.map((value) => BigInt(value)),
    ),
  );
  assert.deepEqual(bitsOrNaN(batch.clockS), goldenBits(golden.all.clocksS));
  assert.deepEqual(
    bitsOrNaN(batch.ionosphereSlantDelayM),
    goldenBits(golden.all.ionosphereSlantDelaysM),
  );
  assert.deepEqual(bitsOrNaN(batch.troposphereDelayM), goldenBits(golden.all.troposphereDelaysM));
  assert.deepEqual(batch.elementResults, [
    { ok: true, error: undefined },
    { ok: true, error: undefined },
    { ok: false, error: "unknown satellite: C01" },
  ]);
  assert.equal(batch.error(2), "unknown satellite: C01");
});

test("emissionMediaBatch preserves state rows below an elevation cutoff", () => {
  const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const batch = sp3.emissionMediaBatch(["G16"], Float64Array.from([goldenEpoch]), goldenReceiver, {
    minElevationRad: 1.5,
    troposphere: true,
  });

  assert.deepEqual(batch.statusLabels, ["belowElevationCutoff"]);
  assert.deepEqual(
    bitsOrNaN(batch.positionEcefM),
    golden.belowCutoff.positionsEcefM[0].map((value) => BigInt(value)),
  );
  assert.deepEqual(bitsOrNaN(batch.clockS), goldenBits(golden.belowCutoff.clocksS));
  assert.deepEqual(golden.belowCutoff.troposphereDelaysM, [null]);
  assert.equal(Number.isNaN(batch.troposphereDelayM[0]), true);
});

test("emissionMediaBatch rejects ionosphere media without an IONEX product", () => {
  const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const epoch = sp3.epochsJ2000Seconds()[40];
  const receiver = Float64Array.from(geodeticToEcef(48.0, 11.0, 600.0));

  assert.throws(
    () =>
      sp3.emissionMediaBatch(["G16"], Float64Array.from([epoch]), receiver, { ionosphere: true }),
    TypeError,
  );
});
