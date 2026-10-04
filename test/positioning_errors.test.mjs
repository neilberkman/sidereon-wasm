// Typed positioning failures through the WASM binding. Every SPP, static,
// broadcast, fallback, FDE, DGNSS and batch failure throws an `Error` named
// `PositioningError` whose `detail` is a `PositioningErrorDetail`: `kind` names
// the engine variant and the fields are the ones that variant carries. Each
// test below provokes one variant through the public API.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  ExactEpochQuery,
  loadRinexObs,
  loadRinexNav,
  loadSp3,
  parseRinexNav,
  parseRinexObs,
  solveSppFromRinexObs,
  solveStatic,
  solveWithFallback,
  sppInputsFromRinexObs,
} from "../pkg-node/sidereon.js";
import { fixture, fixtureText, geodeticToEcef, synthSp3Pseudoranges } from "./helpers.mjs";

const sp3 = loadSp3(fixture("sp3/GBM0MGXRAP_20201770000_01D_05M_ORB_120epoch.sp3"));
const nav = loadRinexNav(fixture("nav/ESBC00DNK_R_20201770000_01D_MN.rnx"));
const rx = geodeticToEcef(55.69, 12.43, 50.0);
const tRx = sp3.epochsJ2000Seconds()[12];

function request(observations = synthSp3Pseudoranges(sp3, tRx, rx)) {
  return {
    observations,
    tRxJ2000S: tRx,
    tRxSecondOfDayS: 3600,
    dayOfYear: 177,
    initialGuess: [...rx, 0.0],
    corrections: { ionosphere: false, troposphere: false },
    withGeodetic: true,
  };
}

/** Run `call`, require a PositioningError, and return its detail. */
function positioningDetail(call) {
  let caught;
  try {
    call();
  } catch (e) {
    caught = e;
  }
  assert.ok(caught instanceof Error, "a PositioningError is thrown");
  assert.equal(caught.name, "PositioningError");
  assert.equal(caught.detail.message, caught.message);
  return caught.detail;
}

test("core source failures keep their detail and typed cause attached", () => {
  const query = ExactEpochQuery.fromBinaryJ2000Seconds(tRx);

  assert.throws(
    () => sp3.selectedPositionClockAtExactQueries("G99", query, query),
    (error) => {
      assert.ok(error instanceof Error);
      assert.equal(error.name, "Error");
      assert.equal(typeof error.message, "string");
      assert.ok(error.message.length > 0);
      assert.equal(error.detail.kind, "UNKNOWN_SATELLITE");
      assert.equal(error.detail.satelliteId, "G99");
      assert.deepEqual(error.cause, error.detail);
      return true;
    },
  );
});

test("a failed typed-cause attachment reports the source error and attachment failure", () => {
  const query = ExactEpochQuery.fromBinaryJ2000Seconds(tRx);
  const originalSet = Reflect.set;
  let interceptedCause = false;

  try {
    Reflect.set = function (target, property, value, ...rest) {
      if (target instanceof Error && property === "cause") {
        interceptedCause = true;
        throw new Error("cause attachment blocked");
      }
      return Reflect.apply(originalSet, Reflect, [target, property, value, ...rest]);
    };

    assert.throws(
      () => sp3.selectedPositionClockAtExactQueries("G99", query, query),
      (error) => {
        assert.ok(error instanceof Error);
        assert.match(error.message, /unknown satellite:?\s+G99/i);
        assert.match(error.message, /attaching the typed cause/i);
        assert.match(error.message, /cause attachment blocked/i);
        return true;
      },
    );
  } finally {
    Reflect.set = originalSet;
  }

  assert.equal(interceptedCause, true, "the actual source-error mapper attempted cause attachment");
});

test("too few satellites names the counts", () => {
  const obs = synthSp3Pseudoranges(sp3, tRx, rx).slice(0, 3);
  const detail = positioningDetail(() => sp3.solveSpp(request(obs)));
  assert.equal(detail.kind, "TOO_FEW_SATELLITES");
  assert.equal(detail.used, 3);
  assert.equal(detail.required, 4);
});

test("a satellite listed twice is a DUPLICATE_OBSERVATION naming it", () => {
  const obs = synthSp3Pseudoranges(sp3, tRx, rx);
  const detail = positioningDetail(() => sp3.solveSpp(request([...obs, obs[0]])));
  assert.equal(detail.kind, "DUPLICATE_OBSERVATION");
  assert.equal(detail.satelliteId, obs[0].satelliteId);
  assert.equal(detail.epochIndex, null);
});

test("a PDOP ceiling no geometry meets rejects the solution with its PDOP", () => {
  const detail = positioningDetail(() => sp3.solveSpp({ ...request(), maxPdop: 1e-6 }));
  assert.equal(detail.kind, "SOLUTION_REJECTED");
  assert.equal(detail.validation.kind, "DEGENERATE_GEOMETRY_PDOP");
  assert.ok(detail.validation.pdop > 1e-6);
});

test("the batch reports each epoch's failure as a detail and throws it on solution()", () => {
  const batch = sp3.solveSppBatch([request(), request()], { maxPdop: 1e-6 });
  for (let i = 0; i < batch.count; i++) {
    assert.equal(batch.isOk(i), false);
    const detail = batch.error(i);
    assert.equal(detail.kind, "SOLUTION_REJECTED");
    assert.equal(detail.validation.kind, "DEGENERATE_GEOMETRY_PDOP");
    const thrown = positioningDetail(() => batch.solution(i));
    assert.deepEqual(thrown, detail);
  }
  const ok = sp3.solveSppBatch([request()]);
  assert.equal(ok.error(0), undefined);
});

test("broadcast solves and the fallback nest the failing leg as cause", () => {
  const few = request(synthSp3Pseudoranges(sp3, tRx, rx).slice(0, 3));
  const broadcast = positioningDetail(() => nav.solveBroadcast(few));
  assert.equal(broadcast.kind, "TOO_FEW_SATELLITES");

  const fallback = positioningDetail(() =>
    solveWithFallback([], nav, few, { maxStalenessDays: 3 }),
  );
  assert.equal(fallback.kind, "BROADCAST_SOLVE_FAILED");
  assert.equal(fallback.cause.kind, "TOO_FEW_SATELLITES");
  assert.equal(fallback.cause.used, 3);
});

test("FDE unresolved errors retain reason, last solution, exclusions and the RAIM test", () => {
  const obs = synthSp3Pseudoranges(sp3, tRx, rx).map((o, i) => ({
    ...o,
    pseudorangeM: o.pseudorangeM + (i === 0 ? 5000 : 0),
  }));
  const detail = positioningDetail(() => sp3.fde({ ...request(obs), pFa: 1e-3, maxExclusions: 0 }));
  assert.equal(detail.kind, "FAULT_UNRESOLVED");
  assert.equal(detail.reason, "EXCLUSION_BUDGET_EXHAUSTED");
  assert.ok(detail.testStatistic > 0);
  assert.ok(detail.solution.positionM.every(Number.isFinite));
  assert.ok(detail.solution.usedSats.length >= 5);
  assert.equal(detail.solution.usedCount, detail.solution.usedSats.length);
  assert.ok(detail.solution.systems.length > 0);
  assert.equal(detail.solution.residualsM.length, detail.solution.usedSats.length);
  assert.deepEqual(detail.excluded, []);
  assert.equal(detail.raim.faultDetected, true);
  assert.equal(detail.raim.testable, true);
  assert.ok(Object.keys(detail.raim.normalizedResiduals).length > 0);

  // Assert ALL 7 structured fields of detail.solution.geometryQuality
  const gq = detail.solution.geometryQuality;
  assert.equal(typeof gq, "object");
  assert.ok(gq !== null);
  assert.equal(gq.tier, "Nominal");
  assert.equal(typeof gq.redundancy, "number");
  assert.ok(gq.redundancy >= 1);
  assert.equal(typeof gq.rank, "number");
  assert.ok(gq.rank >= 4);
  assert.equal(typeof gq.conditionNumber, "number");
  assert.ok(Number.isFinite(gq.conditionNumber) && gq.conditionNumber > 0);
  assert.equal(typeof gq.gdop, "number");
  assert.ok(Number.isFinite(gq.gdop) && gq.gdop > 0);
  assert.equal(typeof gq.raimCheckable, "boolean");
  assert.equal(gq.raimCheckable, true);
  assert.equal(typeof gq.covarianceValidated, "boolean");
  assert.equal(gq.covarianceValidated, true);
});

test("FDE refuses the removed maxIterations option", () => {
  assert.throws(
    () => sp3.fde({ ...request(), maxIterations: 0 }),
    (error) => error instanceof TypeError && /maxIterations is unsupported/.test(error.message),
  );
});

test("FDE distinguishes a fault with no admissible exclusion from an exhausted budget", () => {
  const obs = synthSp3Pseudoranges(sp3, tRx, rx).map((o, i) => ({
    ...o,
    pseudorangeM: o.pseudorangeM + (i === 0 ? 5000 : 0),
  }));
  const detail = positioningDetail(() =>
    sp3.fde({ ...request(obs), pFa: 1e-3, maxExclusions: 1, maxExclusionRmsM: 1e-6 }),
  );
  assert.equal(detail.kind, "FAULT_UNRESOLVED");
  assert.equal(detail.reason, "NO_ADMISSIBLE_EXCLUSION");
  assert.match(detail.message, /no exclusion was admissible/);
});

test("a static epoch listing one satellite twice names the epoch", () => {
  const obs = synthSp3Pseudoranges(sp3, tRx, rx);
  const epochs = [request(), request([...obs, obs[1]])];
  const detail = positioningDetail(() => solveStatic(sp3, epochs, {}));
  assert.equal(detail.kind, "DUPLICATE_OBSERVATION");
  assert.equal(detail.satelliteId, obs[1].satelliteId);
  assert.equal(detail.epochIndex, 1);
});

test("a static solve with no epochs is EMPTY_EPOCHS", () => {
  const detail = positioningDetail(() => solveStatic(sp3, [], {}));
  assert.equal(detail.kind, "EMPTY_EPOCHS");
});

test("a DGNSS base position that is not finite is DGNSS_INVALID_INPUT", () => {
  const detail = positioningDetail(() =>
    sp3.dgnssCorrections({
      basePositionM: [Number.NaN, 0, 0],
      baseObservations: [{ satelliteId: "G01", pseudorangeM: 2.3e7 }],
      tRxJ2000S: tRx,
    }),
  );
  assert.equal(detail.kind, "DGNSS_INVALID_INPUT");
  assert.equal(detail.field, "base_position_m[0]");
});

const ESBC_OBS_FIXTURE = "obs/ESBC00DNK_R_20201770000_01D_30S_MO_trim.rnx";
const ESBC_NAV_FIXTURE = "nav/ESBC00DNK_R_20201770000_01D_MN.rnx";

test("RINEX SPP missing approximate position refusal, assembly, solve and success control", () => {
  const navProduct = parseRinexNav(fixture(ESBC_NAV_FIXTURE));
  const rawObsText = fixtureText(ESBC_OBS_FIXTURE);

  // Strip APPROX POSITION XYZ from header
  const obsTextNoPos = rawObsText.replace(/^.*APPROX POSITION XYZ.*$/m, "");
  const obsNoPos = parseRinexObs(Buffer.from(obsTextNoPos, "utf8"));

  const rinexOptions = {
    corrections: { ionosphere: false, troposphere: false },
    signalPolicy: { G: ["C1C"], E: ["C1C"], C: ["C2I"], R: ["C1C"] },
    qzssClock: "separate",
    troposphereModel: "saastamoinenNiell",
  };

  // Assembly fails with MISSING_APPROX_POSITION
  const assemblyDetail = positioningDetail(() =>
    sppInputsFromRinexObs(navProduct, obsNoPos, rinexOptions),
  );
  assert.equal(assemblyDetail.kind, "MISSING_APPROX_POSITION");
  assert.match(assemblyDetail.message, /APPROX POSITION XYZ/);

  // Solve fails with MISSING_APPROX_POSITION
  const solveDetail = positioningDetail(() =>
    solveSppFromRinexObs(navProduct, obsNoPos, rinexOptions, { withGeodetic: true }),
  );
  assert.equal(solveDetail.kind, "MISSING_APPROX_POSITION");
  assert.match(solveDetail.message, /APPROX POSITION XYZ/);

  // Success control: supplying initialGuess allows both assembly and solve to succeed
  const optionsWithGuess = {
    ...rinexOptions,
    initialGuess: [3582105.291, 532589.731, 5232754.805, 0.0],
  };
  const inputs = sppInputsFromRinexObs(navProduct, obsNoPos, optionsWithGuess);
  assert.ok(inputs.length > 0);

  const batch = solveSppFromRinexObs(navProduct, obsNoPos, optionsWithGuess, {
    withGeodetic: true,
  });
  assert.ok(batch.count > 0);
  assert.equal(batch.isOk(0), true);
  const solution = batch.solution(0);
  assert.ok(solution.usedSats.length >= 4);
  assert.ok(Math.hypot(...solution.positionM) > 6.0e6);
});

test("RINEX OBS parser preserves typed refusal for a malformed fixed-column event record", () => {
  const rawObsText = fixtureText(ESBC_OBS_FIXTURE);
  const observationEpoch = "> 2020 06 25 00 00 00.0000000  0 43";
  const eventEpoch = "> 2020 06 25 00 00 00.0000000  4  1";
  const malformedHeaderRecord = `${"not a position".padEnd(60)}APPROX POSITION XYZ`.padEnd(80);

  // RINEX event records use the same fixed-width header-record syntax. The
  // parser refuses this malformed position while constructing its timeline.
  const obsTextBadEvent = rawObsText.replace(
    observationEpoch,
    `${eventEpoch}\n${malformedHeaderRecord}\n${observationEpoch}`,
  );
  assert.notEqual(obsTextBadEvent, rawObsText, "the actual first epoch is replaced with the event");
  assert.equal(malformedHeaderRecord.length, 80);

  for (const parse of [parseRinexObs, loadRinexObs]) {
    assert.throws(
      () => parse(Buffer.from(obsTextBadEvent, "utf8")),
      (error) => {
        assert.ok(error instanceof Error);
        assert.equal(error.name, "Error");
        assert.match(error.message, /APPROX POSITION XYZ/);
        assert.equal(error.detail.kind, "PARSE");
        assert.match(error.detail.message, /APPROX POSITION XYZ/);
        assert.deepEqual(error.cause, error.detail);
        return true;
      },
    );
  }
});
