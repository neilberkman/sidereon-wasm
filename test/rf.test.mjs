// RF link-budget binding reproduces the engine numbers bit-for-bit, against
// rf_link_budget.json.

import { test } from "node:test";
import assert from "node:assert/strict";

import { fspl, eirp, cn0, wavelength, dishGain, LinkBudget } from "../pkg-node/sidereon.js";
import { fixtureJson, f64Bits } from "./helpers.mjs";

const FX = fixtureJson("rf_link_budget.json");
const eqBits = (value, hex) => assert.equal(f64Bits(value), BigInt(hex));

test("fspl matches reference bits", () => {
  const c = FX.fspl;
  eqBits(fspl(c.distance_km, c.frequency_mhz), c.value_hex);
});

test("eirp matches reference bits", () => {
  const c = FX.eirp;
  eqBits(eirp(c.tx_power_dbm, c.tx_antenna_gain_dbi), c.value_hex);
});

test("cn0 matches reference bits", () => {
  const c = FX.cn0;
  eqBits(cn0(c.eirp_dbw, c.fspl_db, c.receiver_gt_dbk, c.other_losses_db), c.value_hex);
});

test("wavelength matches reference bits", () => {
  const c = FX.wavelength;
  eqBits(wavelength(c.frequency_hz), c.value_hex);
});

test("dish gain matches reference bits", () => {
  const c = FX.dish_gain;
  eqBits(dishGain(c.diameter_m, c.frequency_hz, c.efficiency), c.value_hex);
});

test("link margin matches reference bits", () => {
  const c = FX.link_margin;
  const b = c.budget;
  const budget = new LinkBudget(
    b.eirp_dbw,
    b.fspl_db,
    b.receiver_gt_dbk,
    b.required_cn0_dbhz,
    b.other_losses_db,
  );
  eqBits(budget.margin, c.value_hex);
});

test("RF bad inputs throw", () => {
  assert.throws(() => fspl(0.0, 1616.0));
  assert.throws(() => wavelength(-1.0));
  assert.throws(() => dishGain(1.0, 1616.0e6, 0.0));
  assert.throws(() => new LinkBudget(Infinity, 165.0, -12.0, 35.0));
});
