// sidereon-core 0.14 GeometryQuality parity smoke.
//
// The well-posed SPP case uses this repo's existing golden trace and checks the
// new WASM getters exactly. The rank-deficient case uses the vendored
// coincident-satellite SP3 fixture so the binding proves core's singular-geometry
// error crosses as an Error, not as a partial solution.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  loadSp3,
  ObservabilityTier,
  observabilityTierLabel,
  sourceDop,
} from "../pkg-node/sidereon.js";
import { fixture, fixtureJson, hexToF64 } from "./helpers.mjs";

function l0Request() {
  const fx = fixtureJson("spp_trace_L0_minimal.json").fixture;
  const inp = fx.inputs;
  return {
    sp3File: inp.sp3_file,
    request: {
      observations: inp.observations.map((o) => ({
        satelliteId: o.sat_id,
        pseudorangeM: hexToF64(o.p_meas_m),
      })),
      tRxJ2000S: hexToF64(inp.t_rx_j2000_s),
      tRxSecondOfDayS: hexToF64(inp.t_rx_sod_s),
      dayOfYear: hexToF64(inp.doy),
      initialGuess: fx.frozen.initial_guess_x0.map(hexToF64),
      corrections: { ionosphere: false, troposphere: false },
      klobuchar: {
        alpha: inp.klobuchar_alpha.map(hexToF64),
        beta: inp.klobuchar_beta.map(hexToF64),
      },
      met: {
        pressureHpa: hexToF64(inp.met.pressure_hpa),
        temperatureK: hexToF64(inp.met.temperature_k),
        relativeHumidity: hexToF64(inp.met.relative_humidity),
      },
      withGeodetic: true,
    },
  };
}

test("0.14 SPP geometryQuality exposes nominal diagnostics", () => {
  const { sp3File, request } = l0Request();
  const sp3 = loadSp3(fixture(sp3File));
  const sol = sp3.solveSpp(request);
  const q = sol.geometryQuality;

  assert.equal(q.tier, ObservabilityTier.Nominal);
  assert.equal(observabilityTierLabel(q.tier), "Nominal");
  assert.equal(q.covarianceValidated, true);
  assert.equal(q.raimCheckable, true);
  assert.equal(sol.raimCheckable, q.raimCheckable);
  assert.equal(sol.redundancy, q.redundancy);
  assert.ok(Number.isInteger(q.rank) && q.rank > 0);
  assert.ok(Number.isFinite(q.conditionNumber) && q.conditionNumber > 0.0);
  assert.ok(Number.isFinite(q.gdop) && q.gdop > 0.0);
});

test("0.14 rank-deficient SPP fixture throws singular geometry error", () => {
  const sp3 = loadSp3(fixture("sp3/degenerate_coincident_5sat.sp3"));
  const request = {
    observations: [1, 2, 3, 4, 5].map((prn) => ({
      satelliteId: `G${String(prn).padStart(2, "0")}`,
      pseudorangeM: 20_181_863.0,
    })),
    tRxJ2000S: 646_229_000.0,
    tRxSecondOfDayS: 200.0,
    dayOfYear: 176.0,
    initialGuess: [6_378_137.0, 0.0, 0.0, 0.0],
    corrections: { ionosphere: false, troposphere: false },
    withGeodetic: false,
  };

  assert.throws(
    () => sp3.solveSpp(request),
    (err) => {
      assert.ok(err instanceof Error);
      assert.match(err.message, /SPP solve failed: degenerate geometry/i);
      assert.match(err.message, /singular/i);
      return true;
    },
  );
});

test("0.14 source collinear geometry throws singular geometry error", () => {
  const sensors = [
    { positionM: [0.0, 0.0] },
    { positionM: [100.0, 0.0] },
    { positionM: [200.0, 0.0] },
    { positionM: [300.0, 0.0] },
  ];

  assert.throws(
    () => sourceDop(sensors, [50.0, 0.0], 300.0),
    (err) => {
      assert.ok(err instanceof Error);
      assert.match(err.message, /source geometry failed: singular/i);
      return true;
    },
  );
});
