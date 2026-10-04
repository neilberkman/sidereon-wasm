// GLONASS single-point positioning through the WASM binding.
//
// GLONASS is FDMA: its per-satellite carrier is resolved from the
// `glonassChannels` map so the L1 Klobuchar ionosphere delay scales by
// (f_L1/f_k)^2. These tests prove (a) GLONASS observations solve end-to-end,
// (b) with the ionosphere correction on, a GLONASS satellite with no channel,
// or a channel outside the -7..=6 allocation, is left out of the solve and
// reported in `rejectedSats` as "ionosphereCarrierUnresolved" while the rest of
// the epoch solves, (c) supplying the channel map brings GLONASS back into the
// solve with the ionosphere on, and (d) the field is a no-op for a GPS-only
// solve.
//
// Pseudoranges are synthesized from the committed multi-GNSS SP3 product itself
// (geometric range to each satellite plus its broadcast clock term), so the
// recovered position is checked against the known synthesis truth rather than any
// fabricated number. Neglected light-time / Sagnac terms leave a few-hundred-metre
// residual, well inside the loose bound asserted here.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import init, { loadSp3 } from "../pkg/sidereon.js";
import { coreGoldens, hexToF64 } from "./helpers.mjs";

const here = (rel) => fileURLToPath(new URL(rel, import.meta.url));
const wasmBytes = await readFile(here("../pkg/sidereon_bg.wasm"));
await init({ module_or_path: wasmBytes });

const C_M_S = 299792458.0;
const norm3 = (a, i = 0) => Math.hypot(a[i], a[i + 1], a[i + 2]);
const sub3 = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];

// WGS84 geodetic -> ECEF, metres.
function geodeticToEcef(latDeg, lonDeg, hM) {
  const a = 6378137.0;
  const f = 1 / 298.257223563;
  const e2 = f * (2 - f);
  const lat = (latDeg * Math.PI) / 180;
  const lon = (lonDeg * Math.PI) / 180;
  const N = a / Math.sqrt(1 - e2 * Math.sin(lat) ** 2);
  return [
    (N + hM) * Math.cos(lat) * Math.cos(lon),
    (N + hM) * Math.cos(lat) * Math.sin(lon),
    (N * (1 - e2) + hM) * Math.sin(lat),
  ];
}

// Synthesize a self-consistent GLONASS observation set at one SP3 epoch for a
// receiver at `rx`, keeping satellites above a 10-degree elevation mask.
function glonassScenario(sp3) {
  const epochs = sp3.epochsJ2000Seconds();
  const tRx = epochs[48];
  const rx = geodeticToEcef(55.75, 37.62, 200.0); // Moscow: GLONASS-favourable latitude
  const rxRadius = norm3(rx);
  const up = rx.map((c) => c / rxRadius);

  const observations = [];
  const channels = [];
  for (const sat of sp3.satellites.filter((s) => s.startsWith("R"))) {
    const interp = sp3.interpolate(sat, Float64Array.of(tRx));
    const p = interp.positionM;
    const dtSat = interp.clockS[0];
    if (!Number.isFinite(p[0]) || !Number.isFinite(dtSat)) continue;
    const los = sub3(p, rx);
    const range = norm3(los);
    const elDeg =
      (Math.asin((los[0] * up[0] + los[1] * up[1] + los[2] * up[2]) / range) * 180) / Math.PI;
    if (elDeg < 10) continue;
    observations.push({ satelliteId: sat, pseudorangeM: range - C_M_S * dtSat });
    channels.push([parseInt(sat.slice(1), 10), 0]); // a valid FDMA channel for each slot
  }

  return {
    rx,
    request: {
      observations,
      tRxJ2000S: tRx,
      tRxSecondOfDayS: 0,
      dayOfYear: 176,
      klobuchar: { alpha: [1e-8, 0, 0, 0], beta: [1e5, 0, 0, 0] },
      // Generic Earth-surface seed (equator/prime meridian), thousands of km from
      // truth; the ionosphere model needs a non-degenerate receiver radius at the
      // first iteration.
      initialGuess: [6378137.0, 0, 0, 0],
      withGeodetic: true,
    },
    channels,
  };
}

// GPS pseudoranges plus GLONASS pseudoranges for the same receiver and epoch,
// each synthesized from the SP3 product (geometric range less the satellite
// clock term, light time and Earth rotation applied). GLONASS satellites are
// kept only above 20 degrees, well clear of the 10-degree mask, so selection
// at the truth seed reaches the carrier test for each of them.
function mixedScenario(sp3) {
  const OMEGA_E = 7.2921151467e-5;
  const epochs = sp3.epochsJ2000Seconds();
  const tRx = epochs[48];
  const rx = geodeticToEcef(48.0, 11.0, 600.0);
  const rxRadius = norm3(rx);
  const up = rx.map((c) => c / rxRadius);
  const observations = [];
  const glonass = [];
  for (const sat of sp3.satellites.filter((s) => s.startsWith("G") || s.startsWith("R"))) {
    let dtFlight = 0.075;
    let p = null;
    let dtSat = NaN;
    let range = 0;
    for (let it = 0; it < 4; it++) {
      const interp = sp3.interpolate(sat, Float64Array.of(tRx - dtFlight));
      const raw = interp.positionM;
      dtSat = interp.clockS[0];
      if (!Number.isFinite(raw[0]) || !Number.isFinite(dtSat)) {
        p = null;
        break;
      }
      const theta = OMEGA_E * dtFlight;
      p = [
        raw[0] * Math.cos(theta) + raw[1] * Math.sin(theta),
        -raw[0] * Math.sin(theta) + raw[1] * Math.cos(theta),
        raw[2],
      ];
      range = norm3(sub3(p, rx));
      dtFlight = range / C_M_S;
    }
    if (!p) continue;
    const los = sub3(p, rx);
    const elDeg =
      (Math.asin((los[0] * up[0] + los[1] * up[1] + los[2] * up[2]) / range) * 180) / Math.PI;
    const glonassSat = sat.startsWith("R");
    if (elDeg < (glonassSat ? 20 : 10)) continue;
    observations.push({ satelliteId: sat, pseudorangeM: range - C_M_S * dtSat });
    if (glonassSat) glonass.push(sat);
  }
  return {
    glonass,
    request: {
      observations,
      tRxJ2000S: tRx,
      tRxSecondOfDayS: 43200,
      dayOfYear: 176,
      corrections: { ionosphere: true, troposphere: false },
      initialGuess: [...rx, 0],
      withGeodetic: true,
    },
  };
}

async function loadFixtureSp3() {
  return loadSp3(await readFile(here("./fixtures/GRG0MGXFIN_20201760000_01D_15M_ORB.SP3")));
}

test("GLONASS pseudoranges solve end-to-end (ionosphere off, no channels needed)", async () => {
  const sp3 = await loadFixtureSp3();
  const { rx, request } = glonassScenario(sp3);
  assert.ok(request.observations.length >= 4, "enough visible GLONASS satellites");

  const sol = sp3.solveSpp({ ...request, corrections: { ionosphere: false, troposphere: false } });

  assert.ok(sol.usedSats.length >= 4, "solved with at least four satellites");
  for (const s of sol.usedSats) assert.ok(s.startsWith("R"), `${s} is a GLONASS satellite`);
  assert.equal(sol.usedSats.length, sol.residualsM.length);

  const err = norm3(sub3(Array.from(sol.positionM), rx));
  assert.ok(Number.isFinite(err) && err < 2000, `recovered within ${err.toFixed(1)} m of truth`);
});

test("GLONASS with ionosphere on but no channel map leaves every GLONASS satellite out", async () => {
  const sp3 = await loadFixtureSp3();
  const { request } = glonassScenario(sp3);

  // Every observation is GLONASS and none has a carrier, so no satellite is
  // left to solve with: the epoch fails for too few satellites, not for the
  // carrier.
  assert.throws(
    () => sp3.solveSpp({ ...request, corrections: { ionosphere: true, troposphere: false } }),
    (e) => e instanceof Error && /only 0 usable satellites/.test(e.message),
  );
});

test("a GLONASS satellite with no channel is rejected and the rest of the epoch solves", async () => {
  const sp3 = await loadFixtureSp3();
  const { glonass, request } = mixedScenario(sp3);
  assert.ok(glonass.length >= 2, "at least two GLONASS satellites above 20 degrees");

  const sol = sp3.solveSpp(request);
  for (const s of sol.usedSats) assert.ok(s.startsWith("G"), `${s} is a GPS satellite`);
  const unresolved = sol.rejectedSats.filter((r) => r.reason === "ionosphereCarrierUnresolved");
  assert.deepEqual(
    unresolved.map((r) => r.satelliteId),
    glonass,
    "every GLONASS satellite, in observation order",
  );
  for (const row of sol.rejectedSats) {
    assert.deepEqual(Object.keys(row).sort(), ["reason", "satelliteId"]);
  }

  // A channel for every GLONASS satellite but the first: only that one is
  // left out for its carrier.
  const [first, ...rest] = glonass;
  const partial = sp3.solveSpp({
    ...request,
    glonassChannels: rest.map((sat) => [parseInt(sat.slice(1), 10), 0]),
  });
  assert.deepEqual(
    partial.rejectedSats
      .filter((r) => r.reason === "ionosphereCarrierUnresolved")
      .map((r) => r.satelliteId),
    [first],
  );
  assert.ok(!partial.usedSats.includes(first));
  assert.ok(partial.usedSats.some((s) => s.startsWith("R")));
});

test("the GLONASS channel map lifts the ionosphere gate and GLONASS solves", async () => {
  const sp3 = await loadFixtureSp3();
  const { rx, request, channels } = glonassScenario(sp3);

  const sol = sp3.solveSpp({
    ...request,
    corrections: { ionosphere: true, troposphere: false },
    glonassChannels: channels,
  });

  assert.ok(sol.usedSats.length >= 4, "solved with the ionosphere correction on");
  for (const s of sol.usedSats) assert.ok(s.startsWith("R"), `${s} is a GLONASS satellite`);
  const err = norm3(sub3(Array.from(sol.positionM), rx));
  assert.ok(Number.isFinite(err) && err < 2000, `recovered within ${err.toFixed(1)} m of truth`);
});

test("an out-of-range GLONASS channel is left out like a missing one", async () => {
  const sp3 = await loadFixtureSp3();
  const { glonass, request } = mixedScenario(sp3);

  // Channel 7 is outside the FDMA allocation [-7, +6] (it is the channel real
  // headers give R28), so the carrier is unresolvable and the satellite is
  // reported exactly as if it had no channel.
  const sol = sp3.solveSpp({
    ...request,
    glonassChannels: glonass.map((sat) => [parseInt(sat.slice(1), 10), 7]),
  });
  assert.deepEqual(
    sol.rejectedSats
      .filter((r) => r.reason === "ionosphereCarrierUnresolved")
      .map((r) => r.satelliteId),
    glonass,
  );
  for (const s of sol.usedSats) assert.ok(s.startsWith("G"), `${s} is a GPS satellite`);
});

test("glonassChannels is a no-op for a GPS-only solve (backward compatible)", async () => {
  const fx = JSON.parse(
    await readFile(here("./fixtures/spp_trace_L0_minimal.json"), "utf8"),
  ).fixture;
  const inp = fx.inputs;
  const sp3 = loadSp3(await readFile(here(`./fixtures/${inp.sp3_file}`)));

  const request = {
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
  };

  const without = sp3.solveSpp(request);
  // A populated GLONASS channel map must not perturb a solve that observes no
  // GLONASS satellite: same position and clock, bit-for-bit.
  const withChannels = sp3.solveSpp({
    ...request,
    glonassChannels: [
      [1, 0],
      [2, 3],
      [7, -7],
    ],
  });

  assert.deepEqual(Array.from(withChannels.positionM), Array.from(without.positionM));
  assert.equal(withChannels.rxClockS, without.rxClockS);

  // The engine's solve of the trace inputs with the precise-clock
  // relativistic term, reproduced natively by test/golden-gen.
  const expected = coreGoldens().sppTrace.positionM.map(hexToF64);
  assert.deepEqual(Array.from(without.positionM), expected);
});
