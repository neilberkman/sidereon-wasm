// Shared test helpers: fixture loading and exact float64 bit decoding, so the
// WASM binding is checked against the same goldens the Rust core asserts on.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

export const fixture = (rel) =>
  readFileSync(fileURLToPath(new URL(`./fixtures/${rel}`, import.meta.url)));

export const fixtureText = (rel) => fixture(rel).toString("utf8");

export const fixtureJson = (rel) => JSON.parse(fixtureText(rel));

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

// A BigInt64Array of unix-microsecond epochs from a list of JS numbers (each
// well under 2^53, so the integer is exact before the BigInt cast).
export const bigints = (nums) => BigInt64Array.from(nums.map((n) => BigInt(n)));
