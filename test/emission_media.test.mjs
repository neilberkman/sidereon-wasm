import { test } from "node:test";
import assert from "node:assert/strict";

import { EmissionMediaStatus, emissionMediaStatusLabel, loadSp3 } from "../pkg-node/sidereon.js";
import { fixture, f64Bits, geodeticToEcef } from "./helpers.mjs";

test("emissionMediaBatch returns contiguous arrays and typed row statuses", () => {
  const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const epoch = sp3.epochsJ2000Seconds()[40];
  const receiver = Float64Array.from(geodeticToEcef(48.0, 11.0, 600.0));
  const batch = sp3.emissionMediaBatch(
    ["G16", "E01", "C01"],
    Float64Array.from([epoch, epoch, epoch]),
    receiver,
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
    Array.from(batch.positionEcefM).map((value) => (Number.isNaN(value) ? "NaN" : f64Bits(value))),
    [
      0x41528a3431db22d1n,
      0xc1704ee0fb2b020cn,
      0x417277c7bbae147an,
      0x4170eb2ded439581n,
      0xc154cf5f14ac0832n,
      0x4175fd6663604189n,
      "NaN",
      "NaN",
      "NaN",
    ],
  );
  assert.deepEqual(
    Array.from(batch.clockS).map((value) => (Number.isNaN(value) ? "NaN" : f64Bits(value))),
    [0xbf26da6e075bf537n, 0xbf4cfa1c57307076n, "NaN"],
  );
  assert.deepEqual(
    Array.from(batch.ionosphereSlantDelayM).map((value) =>
      Number.isNaN(value) ? "NaN" : f64Bits(value),
    ),
    [0x0000000000000000n, 0x0000000000000000n, "NaN"],
  );
  assert.deepEqual(
    Array.from(batch.troposphereDelayM).map((value) =>
      Number.isNaN(value) ? "NaN" : f64Bits(value),
    ),
    [0x40185de8aa0810f4n, 0x4004d46e00457c07n, "NaN"],
  );
  assert.deepEqual(batch.elementResults, [
    { ok: true, error: undefined },
    { ok: true, error: undefined },
    { ok: false, error: "unknown satellite: C01" },
  ]);
  assert.equal(batch.error(2), "unknown satellite: C01");
});

test("emissionMediaBatch preserves state rows below an elevation cutoff", () => {
  const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const epoch = sp3.epochsJ2000Seconds()[40];
  const receiver = Float64Array.from(geodeticToEcef(48.0, 11.0, 600.0));
  const batch = sp3.emissionMediaBatch(["G16"], Float64Array.from([epoch]), receiver, {
    minElevationRad: 1.5,
    troposphere: true,
  });

  assert.deepEqual(batch.statusLabels, ["belowElevationCutoff"]);
  assert.deepEqual(Array.from(batch.positionEcefM, f64Bits), [
    0x41528a3431db22d1n,
    0xc1704ee0fb2b020cn,
    0x417277c7bbae147an,
  ]);
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
