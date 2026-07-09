// Shared test helpers: fixture loading and exact float64 bit decoding, so the
// WASM binding is checked against the same goldens the Rust core asserts on.

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

export const fixture = (rel) =>
  readFileSync(fileURLToPath(new URL(`./fixtures/${rel}`, import.meta.url)));

export const fixtureText = (rel) => fixture(rel).toString("utf8");

export const fixtureJson = (rel) => JSON.parse(fixtureText(rel));

export const coreFixtureRoot =
  process.env.SIDEREON_CORE_FIXTURES ??
  "/Users/neil/xuku/sidereon/crates/sidereon-core/tests/fixtures";

export const coreFixture = (...parts) => readFileSync(join(coreFixtureRoot, ...parts));

// Decode a big-endian IEEE-754 hex bit pattern ("0x417b...") to a JS number.
export function hexToF64(s) {
  const view = new DataView(new ArrayBuffer(8));
  view.setBigUint64(0, BigInt(s), false);
  return view.getFloat64(0, false);
}

// The big-endian uint64 bit pattern of a JS number, as a BigInt (Python `>Q`).
export function f64Bits(x) {
  const view = new DataView(new ArrayBuffer(8));
  view.setFloat64(0, x, false);
  return view.getBigUint64(0, false);
}

// Parse a C99 hex float literal ("0x1.f383p+20", "-0x1.8p-1") exactly. Every
// constant used here has a terminating hex fraction, so the dyadic arithmetic
// below is exact in float64.
export function hf(s) {
  let neg = false;
  if (s[0] === "-") {
    neg = true;
    s = s.slice(1);
  } else if (s[0] === "+") {
    s = s.slice(1);
  }
  s = s.slice(2); // strip "0x"
  const [mant, expPart] = s.split("p");
  const exp = parseInt(expPart, 10);
  const [ip, fp = ""] = mant.split(".");
  let val = parseInt(ip, 16);
  for (let i = 0; i < fp.length; i++) {
    val += parseInt(fp[i], 16) / Math.pow(16, i + 1);
  }
  val *= Math.pow(2, exp);
  return neg ? -val : val;
}

// Python str.splitlines(): split on newlines, drop a single trailing newline.
export function splitlines(text) {
  const lines = text.split(/\r\n|\r|\n/);
  if (lines.length > 0 && lines[lines.length - 1] === "") lines.pop();
  return lines;
}

export const norm = (a) => Math.hypot(...a);

export const C_M_S = 299792458.0;

const OMEGA_E = 7.2921151467e-5;

export const VELOCITY_OBS_BITS = [
  ["G07", "0xC0768A0B93C45F82"],
  ["G08", "0xC081BBF2879835FD"],
  ["G10", "0xC081C9B51570E844"],
  ["G16", "0xC045EB58A1B7B54E"],
  ["G18", "0x407EC07DD774B2F8"],
  ["G20", "0xC0689F0E9E24FBC3"],
  ["G21", "0x4063A9470C18C1A7"],
  ["G26", "0x4079EF7D9618F6B0"],
  ["G27", "0xC0775231A845D789"],
];

export function geodeticToEcef(latDeg, lonDeg, hM) {
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

export function synthSp3Pseudoranges(sp3, tRx, rx, rxClockS = 0, minElevationDeg = 10) {
  const rxRadius = norm(rx);
  const up = rx.map((c) => c / rxRadius);
  const out = [];
  for (const sat of sp3.satellites.filter((s) => s.startsWith("G"))) {
    let dtFlight = 0.075;
    let p;
    let dtSat;
    let range = 0;
    for (let it = 0; it < 4; it++) {
      const tTx = tRx - dtFlight;
      const interp = sp3.interpolate(sat, Float64Array.of(tTx));
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
      range = norm([p[0] - rx[0], p[1] - rx[1], p[2] - rx[2]]);
      dtFlight = range / C_M_S;
    }
    if (!p) continue;
    const los = [p[0] - rx[0], p[1] - rx[1], p[2] - rx[2]];
    const elDeg =
      (Math.asin((los[0] * up[0] + los[1] * up[1] + los[2] * up[2]) / range) * 180) / Math.PI;
    if (elDeg < minElevationDeg) continue;
    out.push({ satelliteId: sat, pseudorangeM: range + C_M_S * (rxClockS - dtSat) });
  }
  return out;
}

// A BigInt64Array of unix-microsecond epochs from a list of JS numbers (each
// well under 2^53, so the integer is exact before the BigInt cast).
export const bigints = (nums) => BigInt64Array.from(nums.map((n) => BigInt(n)));

// Cross-platform comparison for iterative/libm-derived values: wasm compute is
// deterministic, but JS-side Math.* reference values and iterative fit
// trajectories differ at ULP scale between platforms. Bands per the
// banded-pin rule; exact bits remain for closed-form outputs only.
export function assertCloseRel(actual, expected, relTol, label) {
  const a = Number(actual);
  const e = Number(expected);
  const tol = Math.abs(e) * relTol + Number.EPSILON;
  if (!(Math.abs(a - e) <= tol)) {
    throw new Error(`${label ?? "value"}: ${a} not within rel ${relTol} of ${e}`);
  }
}
export function assertCloseAbs(actual, expected, absTol, label) {
  const a = Number(actual);
  const e = Number(expected);
  if (!(Math.abs(a - e) <= absTol)) {
    throw new Error(`${label ?? "value"}: ${a} not within abs ${absTol} of ${e}`);
  }
}
export function f64FromBits(bits) {
  const buf = new DataView(new ArrayBuffer(8));
  buf.setBigUint64(0, BigInt(bits));
  return buf.getFloat64(0);
}
