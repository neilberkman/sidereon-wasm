import test from "node:test";
import assert from "node:assert/strict";

import {
  ExactEpoch,
  PppCycleSlipConfig,
  PppRunningMeanVariance,
  PppSatelliteCycleSlipState,
  pppDefaultMinimumArcLength,
  pppDefaultRunningStatisticKFactor,
  pppDetectCycleSlips,
  pppGeometryFreeM,
  pppMelbourneWubbenaCycles,
  pppUpdateGeometryFree,
  pppUpdateMelbourneWubbena,
  preparePppWidelaneFixedEpochs,
  splitPppFloatCycleSlipEpochs,
} from "../pkg-node/sidereon.js";

const C = 299792458;
const F1 = 1575.42e6;
const F2 = 1227.6e6;

function observation(epoch, wideLaneCycles = 5) {
  const geometricM = 23000000 + epoch * 100;
  const phaseM = geometricM + (C / (F1 - F2)) * wideLaneCycles;
  return {
    satelliteId: "G01",
    ambiguityId: "G01",
    p1M: geometricM,
    p2M: geometricM,
    phi1Cyc: phaseM / (C / F1),
    phi2Cyc: phaseM / (C / F2),
    f1Hz: F1,
    f2Hz: F2,
  };
}

test("cycle-slip defaults, running statistics, and mutable state preserve core values", () => {
  assert.equal(pppDefaultMinimumArcLength(), 2);
  assert.equal(pppDefaultRunningStatisticKFactor(), 4);
  const config = PppCycleSlipConfig.default();
  assert.equal(config.minimumArcLength, 2);
  assert.equal(config.runningStatisticKFactor, 4);
  assert.ok(config.equals(PppCycleSlipConfig.default()));

  const statistic = new PppRunningMeanVariance();
  statistic.push(1);
  statistic.push(3);
  assert.equal(statistic.sampleCount, 2);
  assert.equal(statistic.mean, 2);
  assert.equal(statistic.variance, 2);
  assert.equal(statistic.standardDeviation(), Math.SQRT2);

  const state = new PppSatelliteCycleSlipState();
  const first = pppUpdateMelbourneWubbena(state, observation(0));
  assert.equal(first.melbourneWubbenaCycles, 4.999999999454808);
  assert.equal(first.slip, false);
  assert.equal(state.melbourneWubbena.sampleCount, 1);
  const gf = pppUpdateGeometryFree(state, observation(0), 0, ExactEpoch.j2000());
  assert.equal(gf.geometryFreeM, 0);
  assert.equal(gf.slip, false);
  assert.equal(gf.reset, false);
  assert.equal(state.previousEpoch.equals(ExactEpoch.j2000()), true);
  state.resetArc();
  assert.ok(state.equals(new PppSatelliteCycleSlipState()));
});

test("cycle-slip combinations and ordered detection return exact flags", () => {
  assert.equal(pppMelbourneWubbenaCycles(observation(0)), 4.999999999454808);
  assert.equal(pppGeometryFreeM(observation(0)), 0);
  const epochs = [0, 1, 2, 3].map((epoch) => ({
    gapTimeS: epoch * 30,
    gapEpoch: new ExactEpoch(BigInt(epoch * 30), 0n),
    observations: [observation(epoch, epoch >= 2 ? 9 : 5)],
  }));
  const config = new PppCycleSlipConfig(2, 0.05, 2, 4, 120);
  const flags = pppDetectCycleSlips(epochs, config);
  assert.equal(flags.length, 4);
  assert.deepEqual(flags[1].observations, [{ satelliteId: "G01", slip: false, reasons: [] }]);
  assert.deepEqual(flags[2].observations, [
    { satelliteId: "G01", slip: true, reasons: ["melbourneWubbena"] },
  ]);
  assert.equal(flags[2].gapEpoch.equals(new ExactEpoch(60n, 0n)), true);
  assert.ok(flags[2].equals(flags[2]));
});

test("wide-lane preparation and float arc splitting expose complete results", () => {
  const epochs = [0, 1].map((epoch) => ({
    gapTimeS: epoch * 30,
    observations: [observation(epoch)],
  }));
  const prepared = preparePppWidelaneFixedEpochs(epochs, 2, 0.25, "error", {});
  const value = prepared.toJSON();
  assert.equal(value.epochs.length, 2);
  assert.equal(value.epochs[0].epochIndex, 0n);
  assert.equal(value.epochs[0].observations[0].satelliteId, "G01");
  assert.equal(value.wideLaneCycles.G01, 5n);
  assert.deepEqual(value.droppedSats, []);
  assert.deepEqual(value.splitArcs, []);
  assert.ok(prepared.equals(prepared));

  const tagged = splitPppFloatCycleSlipEpochs(
    epochs.map((epoch) => ({
      gapTimeS: epoch.gapTimeS,
      observations: [{ satelliteId: "G01", ambiguityId: "original", raw: epoch.observations[0] }],
    })),
    {},
  );
  assert.deepEqual(tagged, [
    { observations: [{ satelliteId: "G01", ambiguityId: "original" }] },
    { observations: [{ satelliteId: "G01", ambiguityId: "original" }] },
  ]);
});

test("cycle-slip and preparation refusals retain exact typed details", () => {
  assert.throws(
    () => new PppCycleSlipConfig(0, 0.05, 2, 4, 120).validate(),
    (error) => {
      assert.deepEqual(error.details, {
        family: "CycleSlipError",
        kind: "INVALID_CONFIG",
        configKind: "INVALID_MELBOURNE_WUBBENA_THRESHOLD",
        reason: "Melbourne-Wubbena threshold must be positive and finite",
        message:
          "invalid cycle-slip config: Melbourne-Wubbena threshold must be positive and finite",
      });
      return true;
    },
  );
  assert.throws(
    () => preparePppWidelaneFixedEpochs([{ observations: [observation(0)] }], 2, 0.25, "error", {}),
    (error) => {
      assert.equal(error.details.family, "WideLanePrepError");
      assert.equal(error.details.kind, "TOO_FEW_WIDE_LANE_EPOCHS");
      assert.equal(error.details.ambiguityId, "G01");
      assert.equal(error.details.count, 1);
      assert.equal(error.details.minimum, 2);
      return true;
    },
  );
});
