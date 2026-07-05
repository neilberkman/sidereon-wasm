// Signal-analysis binding parity: spectrum, SSC, effective C/N0, DLL jitter,
// and multipath envelopes are pinned to core-computed float64 bit patterns.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  DllProcessing,
  SignalAnalysisModulation,
  effectiveCn0Degradation,
  spectralSeparationCoefficientDbHz,
  spectralSeparationCoefficientHz,
} from "../pkg-node/sidereon.js";
import { f64Bits } from "./helpers.mjs";

const eqBits = (value, hex) => assert.equal(f64Bits(value), BigInt(hex));

test("signal analysis closed forms match reference bits", () => {
  const bpsk = SignalAnalysisModulation.bpsk(1);
  const boc = SignalAnalysisModulation.sineBoc(1, 1);
  const dll = {
    cn0DbHz: 45,
    loopBandwidthHz: 1.0,
    integrationTimeS: 0.02,
    correlatorSpacingChips: 0.1,
    receiverBandwidthHz: 24e6,
  };
  const multipath = {
    multipathToDirectRatio: 0.5,
    correlatorSpacingChips: 0.1,
    receiverBandwidthHz: 24e6,
  };

  eqBits(bpsk.psdHz(0), "0x3EB066676CCCDD33");
  eqBits(boc.psdHz(1.0e6), "0x3E9BC121ED0E8D46");
  eqBits(bpsk.fractionPowerInBand(24e6), "0x3FEFBA305BC2217B");
  eqBits(boc.rmsBandwidthHz(24e6), "0x413E31009B5CBEC3");
  eqBits(spectralSeparationCoefficientHz(bpsk, boc, 24e6), "0x3E85DDCA164B7F18");
  eqBits(spectralSeparationCoefficientDbHz(bpsk, boc, 24e6), "0xC050F8575FE9E076");

  const cn0 = effectiveCn0Degradation(bpsk, boc, 45, 24e6, 0.2);
  eqBits(cn0.effectiveCn0DbHz, "0x40467F6C2F08F4A0");
  eqBits(cn0.degradationDb, "0x3F727A1EE16C0000");

  eqBits(bpsk.dllThermalNoiseJitter(dll, DllProcessing.Coherent).meters, "0x3FD373A385AD2B5A");
  eqBits(bpsk.dllThermalNoiseJitter(dll, DllProcessing.NonCoherent).meters, "0x3FD377C46E0BDEDB");
  eqBits(bpsk.dllLowerBound(dll).meters, "0x3FCE6F95C8606022");

  const envelope = bpsk.multipathErrorEnvelope(multipath, Float64Array.of(0, 0.1, 0.25));
  eqBits(envelope[2].inPhaseM, "0x401AC969157AFB64");
  eqBits(envelope[2].antiPhaseM, "0xC01ACB7E2480D0D8");
  eqBits(envelope[2].runningAverageM, "0x4015DD8C8A972938");
});

test("signal analysis rejects invalid domains", () => {
  assert.throws(() => SignalAnalysisModulation.bpsk(0));
  assert.throws(() => SignalAnalysisModulation.sineBoc(1, 0));
  const bpsk = SignalAnalysisModulation.bpsk(1);
  assert.throws(() => bpsk.fractionPowerInBand(0));
});
