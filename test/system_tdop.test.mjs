// Per-system / per-clock-column time DOP through the WASM binding (core A3
// surface): the geometry `Dop.systemTdops` clock-column vector and the SPP
// solution's `systemTdops`, paired with the constellation ordering.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { Dop, Wgs84Geodetic, loadSp3 } from "../pkg-node/sidereon.js";
import { hexToF64 } from "./helpers.mjs";

const here = (rel) => fileURLToPath(new URL(rel, import.meta.url));

test("Dop.systemTdops is empty for a system-agnostic single-clock geometry", () => {
  const receiver = new Wgs84Geodetic(0.0, 0.0);
  const az = Float64Array.from([45.0, 225.0, 135.0, 315.0]);
  const el = Float64Array.from([
    35.264389682754654, 35.264389682754654, -35.264389682754654, -35.264389682754654,
  ]);
  const dop = Dop.fromAzEl(az, el, receiver);

  // The geometry-only `dop` path carries no constellation context, so it leaves
  // `systemTdops` empty (read the lone clock's value off `tdop`). Per-system
  // entries are only emitted by the multi-clock SPP path, where the solve knows
  // each clock column's constellation.
  assert.ok(Array.isArray(dop.systemTdops) || dop.systemTdops instanceof Float64Array);
  assert.equal(dop.systemTdops.length, 0);
  assert.ok(Number.isFinite(dop.tdop) && dop.tdop > 0);
});

test("SppSolution.systemTdops carries one finite per-system entry, ascending", async () => {
  const fx = JSON.parse(
    await readFile(here("./fixtures/spp_trace_L0_minimal.json"), "utf8"),
  ).fixture;
  const inp = fx.inputs;
  const sp3 = loadSp3(await readFile(here(`./fixtures/${inp.sp3_file}`)));

  const sol = sp3.solveSpp({
    observations: inp.observations.map((o) => ({
      satelliteId: o.sat_id,
      pseudorangeM: hexToF64(o.p_meas_m),
    })),
    tRxJ2000S: hexToF64(inp.t_rx_j2000_s),
    tRxSecondOfDayS: hexToF64(inp.t_rx_sod_s),
    dayOfYear: hexToF64(inp.doy),
    initialGuess: fx.frozen.initial_guess_x0.map(hexToF64),
    corrections: { ionosphere: false, troposphere: false },
    klobuchar: { alpha: inp.klobuchar_alpha.map(hexToF64), beta: inp.klobuchar_beta.map(hexToF64) },
    withGeodetic: false,
  });

  const tdops = sol.systemTdops;
  // GPS-only fixture: a single constellation clock.
  assert.equal(tdops.length, 1);
  assert.equal(tdops[0].system, "GPS");
  assert.ok(Number.isFinite(tdops[0].tdop) && tdops[0].tdop > 0);
});
