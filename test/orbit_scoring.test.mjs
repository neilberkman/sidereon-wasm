import { test } from "node:test";
import assert from "node:assert/strict";

import * as orbit from "../pkg-node/sidereon.js";
import { fixture, fixtureJson, fixtureText } from "./helpers.mjs";

const vector = (...values) => Float64Array.of(...values);
const exact = (hour, minute) => {
  using label = new orbit.ClockEpoch(2026, 4, 30, hour, minute, 0);
  return orbit.ClockInstant.fromCivil(orbit.TimeScale.Gpst, label);
};
const scoringProduct = () => {
  const header = fixtureText("sp3/g02_ecef_two_epoch.sp3")
    .split("\n*  ")[0]
    .replace("       2 d+D", "      16 d+D");
  const records = Array.from({ length: 16 }, (_, index) => {
    const minutes = 9 * 60 + 45 + 15 * index;
    const hour = String(Math.floor(minutes / 60)).padStart(2);
    const minute = String(minutes % 60).padStart(2);
    const fields = [26560, 1000 * index, 0, 89.172419 + 0.006984 * index]
      .map((value) => value.toFixed(6).padStart(14))
      .join("");
    return `*  2026  4 30 ${hour} ${minute}  0.00000000\nPG02${fields}`;
  });
  return `${header}\n${records.join("\n")}\nEOF\n`;
};
const typed = (detail) => (error) => {
  assert.equal(error.name, "OrbitScoreError");
  assert.equal(error.kind, detail.kind);
  assert.deepEqual(error.detail, detail);
  if (detail.kind === "invalidInput") {
    assert.equal(error.toString(), `invalid orbit scoring ${detail.field}: ${detail.reason}`);
  }
  return true;
};

test("RTN coordinates, inertial velocity and published SISRE weights", () => {
  using components = orbit.rtnComponents(vector(3, 4, 12), vector(8, 0, 0), vector(0, 4, 0));
  using expected = new orbit.RtnComponents(3, 4, 12);
  using other = new orbit.RtnComponents(3, 4, 13);
  assert.deepEqual([components.radialM, components.transverseM, components.normalM], [3, 4, 12]);
  assert.ok(components.equals(expected));
  assert.ok(!components.equals(other));
  assert.deepEqual(
    Array.from(orbit.inertialVelocityFromEarthFixed(vector(0, 0, 100), vector(1, 2, 3))),
    [1, 2, 3],
  );
  assert.equal(
    orbit.orbitRegimeFromInertialState(vector(26560000, 0, 0), vector(0, 3874, 0)),
    orbit.OrbitRegime.Meo,
  );
  assert.equal(orbit.orbitRegimeToString(orbit.OrbitRegime.Meo), "MEO");
  assert.equal(orbit.scoredProductToString(orbit.ScoredProduct.Source), "orbit source");
  using weights = new orbit.SisreWeights(1, 0);
  using same = new orbit.SisreWeights(1, 0);
  using published = orbit.SisreWeights.montenbruck2018(orbit.GnssSystem.Gps, orbit.OrbitRegime.Meo);
  assert.equal(weights.radial(), 1);
  assert.equal(weights.transverseNormalSquared(), 0);
  assert.equal(weights.orbitOnlyM(components), 3);
  assert.equal(weights.withClockM(components, 2), 1);
  assert.equal(published.radial(), 0.98);
  assert.equal(published.transverseNormalSquared(), 1 / 49);
  assert.ok(weights.equals(same));
  assert.ok(!weights.equals(published));
  assert.equal(orbit.DEFAULT_VELOCITY_HALF_STEP_S(), 0.5);
  assert.equal(orbit.MIN_VELOCITY_HALF_STEP_S(), 1e-8);
  assert.equal(orbit.triadVelocityDefault(), orbit.TriadVelocity.Inertial);
  assert.equal(orbit.clockDatumDefault(), orbit.ClockDatum.EpochSystemMedian);
  assert.equal(orbit.gapPolicyDefault(), orbit.GapPolicy.Count);
});

test("prediction scoring on exact SP3 epochs and an explicit trajectory", () => {
  using cutoff = exact(9, 45);
  using next = exact(10, 0);
  using duplicate = exact(9, 45);
  using nano = orbit.ClockInstant.fromNanos(orbit.TimeScale.Tt, (1n << 100n) + 1n);
  using julian = orbit.ClockInstant.fromSplitJulianDate(orbit.TimeScale.Tt, 2451545, 0.125);
  assert.equal(nano.nanos, (1n << 100n) + 1n);
  assert.equal(nano.reprKind, "Nanos");
  assert.equal(nano.jdWhole, undefined);
  assert.equal(julian.jdWhole, 2451545);
  assert.equal(julian.fraction, 0.125);
  assert.equal(julian.nanos, undefined);
  assert.equal(julian.reprKind, "JulianDate");
  assert.equal(cutoff.scale, orbit.TimeScale.Gpst);
  assert.ok(cutoff.equals(duplicate));
  assert.ok(!cutoff.equals(next));
  using sp3 = orbit.loadSp3(new TextEncoder().encode(scoringProduct()));
  using predicted = orbit.PredictedSp3.withCutoff(sp3, cutoff);
  using flags = orbit.PredictedSp3.fromPredictionFlags(sp3);
  using copy = predicted.product();
  using frame = predicted.frame();
  using opts = new orbit.OrbitScoreOptions(vector(-1, 901));
  using sameOpts = new orbit.OrbitScoreOptions(vector(-1, 901));
  using differentOpts = new orbit.OrbitScoreOptions(vector(-1, 902));
  assert.ok(opts.equals(sameOpts));
  assert.ok(!opts.equals(differentOpts));
  using selection = orbit.SisreWeightSelection.MONTENBRUCK_2018();
  using defaultSelection = new orbit.SisreWeightSelection();
  assert.equal(defaultSelection.fixedWeights, undefined);
  assert.ok(selection.equals(defaultSelection));
  using fixedWeights = new orbit.SisreWeights(1, 0);
  using fixedSelection = orbit.SisreWeightSelection.fixed(fixedWeights);
  using extractedWeights = fixedSelection.fixedWeights;
  assert.ok(extractedWeights.equals(fixedWeights));
  assert.ok(!fixedSelection.equals(selection));
  using defaultEpochs = new orbit.ScoreEpochs();
  using referenceEpochs = orbit.ScoreEpochs.REFERENCE_RECORDS();
  using providedEpochs = orbit.ScoreEpochs.instants([exact(9, 45), exact(10, 0)]);
  const epochList = providedEpochs.epochInstants;
  try {
    assert.equal(epochList.length, 2);
    assert.ok(epochList[0].equals(cutoff));
    assert.ok(epochList[1].equals(next));
  } finally {
    for (const epoch of epochList) epoch.free();
  }
  assert.ok(defaultEpochs.equals(referenceEpochs));
  assert.ok(!defaultEpochs.equals(providedEpochs));
  opts.setEpochs(providedEpochs);
  opts.setSisreWeights(fixedSelection);
  opts.setTriad(orbit.TriadVelocity.EarthFixed);
  opts.setClockDatum(orbit.ClockDatum.Unadjusted);
  opts.setGaps(orbit.GapPolicy.Count);
  opts.setVelocityHalfStepS(0.5);
  using equivalentFrames = new orbit.Sp3FrameReconciliationOptions([["IGc20", "IGS20"]], false);
  using frames = new orbit.Sp3FrameReconciliationOptions([["IGc20", "IGS20"]], true);
  assert.deepEqual(frames.assertedEquivalentLabelSets, [["IGS20", "IGc20"]]);
  assert.equal(frames.helmert, true);
  assert.ok(!frames.equals(equivalentFrames));
  opts.setFrameReconciliation(frames);
  using storedWeights = opts.sisreWeights;
  using storedFrames = opts.frameReconciliation;
  using storedEpochs = opts.epochs;
  assert.ok(storedWeights.equals(fixedSelection));
  assert.ok(storedFrames.equals(frames));
  assert.ok(storedEpochs.equals(providedEpochs));
  assert.deepEqual(Array.from(opts.horizonBinEdgesS), [-1, 901]);
  assert.equal(opts.velocityHalfStepS, 0.5);
  assert.equal(opts.triad, orbit.TriadVelocity.EarthFixed);
  assert.equal(opts.clockDatum, orbit.ClockDatum.Unadjusted);
  assert.equal(opts.gaps, orbit.GapPolicy.Count);
  assert.equal(copy.epochCount, 16);
  assert.equal(frame.frameLabel, "IGc20");
  assert.equal(predicted.timeScale(), orbit.TimeScale.Gpst);
  assert.equal(predicted.sp3TimeSystem(), "GPS");
  assert.equal(predicted.referencePoint(), orbit.OrbitReferencePoint.CenterOfMass);
  assert.equal(predicted.clockIncludesRelativity(), false);
  assert.equal(flags.predictionOriginJ2000S("G02"), 830827800);
  assert.equal(predicted.predictionOriginJ2000S("G02"), 830814300);

  using report = orbit.scoreOrbit(predicted, sp3, ["G02"], opts);
  using handle = predicted.asOrbitSource();
  using another = orbit.scoreOrbit(handle, sp3, ["G02"], opts);
  using empty = orbit.scoreOrbit(handle, sp3, [], opts);
  assert.ok(report.equals(another));
  assert.ok(!report.equals(empty));
  assert.equal(report.sourceReferencePoint, orbit.OrbitReferencePoint.CenterOfMass);
  assert.deepEqual(Array.from(report.horizonBinEdgesS), [-1, 901]);
  using agreement = report.frame;
  using sameLabel = orbit.FrameAgreement.SAME_LABEL();
  using asserted = orbit.FrameAgreement.ASSERTED_EQUIVALENT();
  using helmert = orbit.FrameAgreement.helmert(
    orbit.TerrestrialFrame.Itrf2014,
    orbit.TerrestrialFrame.Itrf2020,
  );
  assert.ok(agreement.equals(sameLabel));
  assert.ok(!agreement.equals(asserted));
  using broadcastDatums = orbit.FrameAgreement.BROADCAST_DATUMS();
  assert.equal(broadcastDatums.fromFrame, undefined);
  assert.equal(broadcastDatums.toFrame, undefined);
  assert.ok(!broadcastDatums.equals(agreement));
  assert.ok(!broadcastDatums.equals(asserted));
  assert.equal(helmert.fromFrame, orbit.TerrestrialFrame.Itrf2014);
  assert.equal(helmert.toFrame, orbit.TerrestrialFrame.Itrf2020);
  using coverage = report.coverage;
  using defaultCoverage = new orbit.ScoreCoverage();
  assert.ok(!coverage.equals(defaultCoverage));
  assert.deepEqual(
    [
      defaultCoverage.epochs,
      defaultCoverage.scored,
      defaultCoverage.clocked,
      defaultCoverage.outsideHorizonBins,
      defaultCoverage.noPredictionOrigin,
      defaultCoverage.sourceSatelliteMissing,
      defaultCoverage.sourceEpochMissing,
      defaultCoverage.referenceSatelliteMissing,
      defaultCoverage.referenceEpochMissing,
    ],
    Array(9).fill(0),
  );
  assert.deepEqual(
    [
      coverage.epochs,
      coverage.scored,
      coverage.clocked,
      coverage.outsideHorizonBins,
      coverage.noPredictionOrigin,
      coverage.sourceSatelliteMissing,
      coverage.sourceEpochMissing,
      coverage.referenceSatelliteMissing,
      coverage.referenceEpochMissing,
    ],
    [2, 2, 2, 0, 0, 0, 0, 0, 0],
  );
  const [sat] = report.perSatellite;
  const [system] = report.perSystem;
  using satellite = sat;
  using constellation = system;
  assert.equal(satellite.satellite, "G02");
  assert.equal(satellite.predictionOriginJ2000S, 830814300);
  assert.equal(satellite.orbitRegime, undefined);
  assert.equal(constellation.system, orbit.GnssSystem.Gps);
  const [satCopy] = another.perSatellite;
  const [systemCopy] = another.perSystem;
  using otherSatellite = satCopy;
  using otherSystem = systemCopy;
  using satCoverage = satellite.coverage;
  using systemCoverage = constellation.coverage;
  assert.deepEqual([satCoverage.epochs, satCoverage.scored, satCoverage.clocked], [2, 2, 2]);
  assert.deepEqual(
    [systemCoverage.epochs, systemCoverage.scored, systemCoverage.clocked],
    [2, 2, 2],
  );
  assert.ok(satellite.equals(otherSatellite));
  assert.ok(constellation.equals(otherSystem));
  using usedWeights = satellite.sisreWeights;
  assert.deepEqual([usedWeights.radial(), usedWeights.transverseNormalSquared()], [1, 0]);
  const [bin] = satellite.bins;
  const [systemBin] = constellation.bins;
  using stats = bin;
  using statsBySystem = systemBin;
  assert.ok(stats.equals(statsBySystem));
  assert.deepEqual(
    [stats.horizonStartS, stats.horizonEndS, stats.count, stats.clockCount],
    [-1, 901, 2, 2],
  );
  using baseline = stats.radial;
  using filler = statsBySystem.radial;
  assert.ok(baseline.equals(filler));
  for (const key of ["radial", "transverse", "normal", "orbit3d", "sisreOrbit", "clock", "sisre"]) {
    using metric = stats[key];
    assert.deepEqual([metric.rmsM, metric.meanM, metric.maxAbsM].map(Math.abs), [0, 0, 0], key);
  }

  const firstPosition = vector(26560000, 0, 0);
  const secondPosition = vector(26560000, 1000000, 0);
  const t0 = new orbit.TrajectorySample("G02", cutoff, firstPosition, 89.172419e-6);
  const t1 = new orbit.TrajectorySample("G02", next, secondPosition, 89.179403e-6);
  using firstCopy = new orbit.TrajectorySample("G02", cutoff, firstPosition, 89.172419e-6);
  assert.equal(t0.satellite, "G02");
  assert.equal(t0.clockS, 89.172419e-6);
  assert.deepEqual(t0.positionEcefM, firstPosition);
  using sampleEpoch = t0.epoch;
  assert.ok(sampleEpoch.equals(cutoff));
  assert.ok(t0.equals(firstCopy));
  assert.ok(!t0.equals(t1));
  using trajectory = new orbit.OrbitTrajectory("IGc20", cutoff, [t0, t1]);
  using relativistic = trajectory.withClockRelativityIncluded(true);
  assert.deepEqual(trajectory.satellites(), ["G02"]);
  using trajectoryFrame = trajectory.frame();
  assert.equal(trajectoryFrame.frameLabel, "IGc20");
  assert.equal(trajectory.timeScale(), orbit.TimeScale.Gpst);
  assert.equal(trajectory.sp3TimeSystem(), undefined);
  assert.equal(trajectory.referencePoint(), orbit.OrbitReferencePoint.CenterOfMass);
  assert.equal(trajectory.clockIncludesRelativity(), false);
  const trajectoryEpochs = trajectory.epochs();
  try {
    assert.deepEqual(
      trajectoryEpochs.map((epoch) => epoch.gpsSeconds),
      [1461577500, 1461578400],
    );
  } finally {
    for (const epoch of trajectoryEpochs) epoch.free();
  }
  using sameTrajectory = trajectory.withClockRelativityIncluded(false);
  assert.ok(trajectory.equals(sameTrajectory));
  assert.ok(!trajectory.equals(relativistic));
  assert.equal(relativistic.clockIncludesRelativity(), true);
  assert.equal(trajectory.predictionOriginJ2000S("G02"), 830814300);
  using state = trajectory.stateAtJ2000S("G02", 830814300);
  assert.deepEqual(state.positionEcefM, firstPosition);
  assert.equal(state.clockS, 89.172419e-6);
  using otherState = trajectory.stateAtJ2000S("G02", 830815200);
  using sameState = sameTrajectory.stateAtJ2000S("G02", 830814300);
  using sp3State = predicted.stateAtJ2000S("G02", 830814300);
  assert.deepEqual(sp3State.positionEcefM, firstPosition);
  assert.equal(sp3State.clockS, 89.172419e-6);
  assert.ok(state.equals(sameState));
  assert.ok(!state.equals(otherState));
  using score = orbit.scoreOrbit(trajectory, sp3, ["G02"], opts);
  using trajectoryCoverage = score.coverage;
  assert.equal(trajectoryCoverage.scored, 2);
  using oneEpoch = orbit.ScoreEpochs.instants([exact(9, 45)]);
  opts.setEpochs(oneEpoch);
  using single = orbit.scoreOrbit(handle, sp3, ["G02"], opts);
  const [singleSat] = single.perSatellite;
  const [singleSys] = single.perSystem;
  using singleSatellite = singleSat;
  using singleSystem = singleSys;
  const [singleBin] = singleSatellite.bins;
  using singleStats = singleBin;
  assert.equal(singleStats.count, 1);
  assert.ok(!stats.equals(singleStats));
  assert.ok(!satellite.equals(singleSatellite));
  assert.ok(!constellation.equals(singleSystem));
  opts.setEpochs(providedEpochs);

  const flaggedText = scoringProduct()
    .split("\n")
    .map((line) => (line.startsWith("PG02") ? line.padEnd(79) + "P" : line))
    .join("\n");
  using flaggedProduct = orbit.loadSp3(new TextEncoder().encode(flaggedText));
  using noOrigin = orbit.PredictedSp3.fromPredictionFlags(flaggedProduct);

  opts.setGaps(orbit.GapPolicy.Refuse);
  assert.throws(
    () => orbit.scoreOrbit(predicted, sp3, ["G03"], opts),
    typed({ kind: "missingSatellite", product: orbit.ScoredProduct.Reference, satellite: "G03" }),
  );
  assert.throws(
    () => orbit.scoreOrbit(noOrigin, sp3, ["G02"], opts),
    typed({ kind: "noPredictionOrigin", satellite: "G02" }),
  );

  const equivalentFrame = scoringProduct().replace("IGc20", "IGS20");
  using equivalent = orbit.loadSp3(new TextEncoder().encode(equivalentFrame));
  using equivalentScore = orbit.scoreOrbit(predicted, equivalent, ["G02"], opts);
  using equivalentAgreement = equivalentScore.frame;
  assert.ok(equivalentAgreement.equals(asserted));
  assert.ok(!equivalentAgreement.equals(sameLabel));

  const wrongFrame = scoringProduct().replace("IGc20", "WRONG");
  using reference = orbit.loadSp3(new TextEncoder().encode(wrongFrame));
  assert.throws(
    () => orbit.scoreOrbit(predicted, reference, ["G02"], opts),
    typed({ kind: "frameMismatch", orbitSource: "IGc20", reference: "WRONG" }),
  );
});

test("orbit scoring refuses incompatible time labels, missing epochs and non-finite results", () => {
  const encode = (input) => new TextEncoder().encode(input);
  using cutoff = exact(9, 45);
  using next = exact(10, 0);
  using utcLabel = new orbit.ClockEpoch(2026, 4, 30, 9, 45, 0);
  using utc = orbit.ClockInstant.fromCivil(orbit.TimeScale.Utc, utcLabel);
  using reference = orbit.loadSp3(encode(scoringProduct()));
  using irn = orbit.loadSp3(encode(scoringProduct().replace("cc GPS", "cc IRN")));
  using cutoffSource = orbit.PredictedSp3.withCutoff(reference, cutoff);
  using opts = new orbit.OrbitScoreOptions(vector(-1, 901));
  using epochs = orbit.ScoreEpochs.instants([exact(9, 45), exact(10, 0)]);
  opts.setEpochs(epochs);

  assert.throws(
    () => orbit.PredictedSp3.withCutoff(reference, utc),
    typed({
      kind: "instantTimeSystemMismatch",
      field: "cutoff",
      index: 0,
      found: orbit.TimeScale.Utc,
      expected: orbit.TimeScale.Gpst,
    }),
  );
  assert.throws(
    () => orbit.scoreOrbit(cutoffSource, irn, ["G02"], opts),
    typed({ kind: "timeSystemLabelMismatch", orbitSource: "GPS", reference: "IRN" }),
  );
  using utcSource = new orbit.OrbitTrajectory("IGc20", utc, [
    new orbit.TrajectorySample("G02", utc, vector(26560000, 0, 0)),
  ]);
  assert.throws(
    () => orbit.scoreOrbit(utcSource, reference, ["G02"], opts),
    typed({
      kind: "timeSystemMismatch",
      orbitSource: orbit.TimeScale.Utc,
      reference: orbit.TimeScale.Gpst,
    }),
  );
  using invalid = new orbit.OrbitScoreOptions(vector(0));
  assert.throws(
    () => orbit.scoreOrbit(cutoffSource, reference, ["G02"], invalid),
    typed({ kind: "invalidInput", field: "horizon_bin_edges_s", reason: "fewer than two edges" }),
  );
  opts.setGaps(orbit.GapPolicy.Refuse);
  using gap = new orbit.OrbitTrajectory("IGc20", cutoff, [
    new orbit.TrajectorySample("G02", cutoff, vector(26560000, 0, 0)),
  ]);
  assert.throws(
    () => orbit.scoreOrbit(gap, reference, ["G02"], opts),
    typed({
      kind: "missingEpoch",
      satellite: "G02",
      epochIndex: 1,
      product: orbit.ScoredProduct.Source,
      error: { kind: "EPOCH_OUT_OF_RANGE" },
    }),
  );
  using overflow = new orbit.OrbitTrajectory("IGc20", cutoff, [
    new orbit.TrajectorySample("G02", cutoff, vector(1.7e308, 1.7e308, 1.7e308)),
    new orbit.TrajectorySample("G02", next, vector(26560000, 1000000, 0)),
  ]);
  assert.throws(
    () => orbit.scoreOrbit(overflow, reference, ["G02"], opts),
    typed({ kind: "nonFiniteResult", satellite: "G02" }),
  );
  using helmert = new orbit.Sp3FrameReconciliationOptions(undefined, true);
  opts.setFrameReconciliation(helmert);
  using beyondFrame = new orbit.OrbitTrajectory("IGS14", cutoff, [
    new orbit.TrajectorySample("G02", cutoff, vector(Number.MAX_VALUE, 0, 0)),
  ]);
  assert.throws(
    () => orbit.scoreOrbit(beyondFrame, reference, ["G02"], opts),
    typed({
      kind: "frameTransform",
      error: { kind: "invalidInput", field: "x_m", reason: "must be finite" },
    }),
  );

  using undefinedRegime = orbit.loadSp3(
    encode(
      scoringProduct().replace(
        /\n(PG02.{14})(.{14})/g,
        (_, prefix, y) => "\n" + prefix + (Number(y) * 8).toFixed(6).padStart(14),
      ),
    ),
  );
  using forEscape = orbit.PredictedSp3.withCutoff(undefinedRegime, cutoff);
  assert.throws(
    () => orbit.scoreOrbit(forEscape, undefinedRegime, ["G02"], opts),
    typed({ kind: "undeterminedOrbitRegime", satellite: "G02" }),
  );
  using qzss = orbit.loadSp3(encode(scoringProduct().replaceAll("G02", "J02")));
  using qzssSource = orbit.PredictedSp3.withCutoff(qzss, cutoff);
  assert.throws(
    () => orbit.scoreOrbit(qzssSource, qzss, ["J02"], opts),
    typed({ kind: "noSisreWeights", satellite: "J02", regime: orbit.OrbitRegime.Meo }),
  );
  using staticProduct = orbit.loadSp3(
    encode(
      scoringProduct().replace(
        /\nPG02.{42}/g,
        "\nPG02" +
          ["0.000000", "0.000000", "26560.000000"].map((value) => value.padStart(14)).join(""),
      ),
    ),
  );
  using staticSource = orbit.PredictedSp3.withCutoff(staticProduct, cutoff);
  opts.setTriad(orbit.TriadVelocity.EarthFixed);
  opts.setVelocityHalfStepS(900);
  assert.throws(
    () => orbit.scoreOrbit(staticSource, staticProduct, ["G02"], opts),
    typed({ kind: "degenerateReferenceState", satellite: "G02", epochIndex: 0 }),
  );
});

test("broadcast orbit reports its frame and time conventions", () => {
  using nav = orbit.loadRinexNav(fixture("nav/ESBC00DNK_R_20201770000_01D_MN.rnx"));
  using cutoff = exact(9, 45);
  using broadcast = new orbit.BroadcastOrbit(nav, cutoff);
  using frame = broadcast.frame();
  using datums = orbit.OrbitFrame.BROADCAST_DATUMS();
  using labeled = orbit.OrbitFrame.label("IGc20");
  assert.equal(broadcast.timeScale(), orbit.TimeScale.Gpst);
  assert.equal(broadcast.sp3TimeSystem(), undefined);
  assert.equal(broadcast.referencePoint(), orbit.OrbitReferencePoint.AntennaPhaseCenter);
  assert.equal(broadcast.clockIncludesRelativity(), true);
  assert.equal(broadcast.predictionOriginJ2000S("G02"), 830814300);
  assert.equal(frame.frameLabel, undefined);
  assert.equal(labeled.frameLabel, "IGc20");
  assert.ok(frame.equals(datums));
  assert.ok(!frame.equals(labeled));
  const golden = fixtureJson("astrodynamics.json").attitudeTrajectories.broadcastEcef;
  using state = broadcast.stateAtJ2000S("G01", golden.epoch);
  assert.deepEqual(Array.from(state.positionEcefM), golden.position);
  assert.equal(state.clockS, golden.clock);
});

test("numeric orbit scoring enums preserve their declared values", () => {
  assert.deepEqual([orbit.ClockDatum.EpochSystemMedian, orbit.ClockDatum.Unadjusted], [0, 1]);
  assert.deepEqual([orbit.GapPolicy.Count, orbit.GapPolicy.Refuse], [0, 1]);
  assert.deepEqual(
    [orbit.OrbitReferencePoint.CenterOfMass, orbit.OrbitReferencePoint.AntennaPhaseCenter],
    [0, 1],
  );
  assert.deepEqual(
    [orbit.OrbitRegime.Meo, orbit.OrbitRegime.Igso, orbit.OrbitRegime.Geo],
    [0, 1, 2],
  );
  assert.deepEqual([orbit.ScoredProduct.Source, orbit.ScoredProduct.Reference], [0, 1]);
  assert.deepEqual([orbit.TriadVelocity.Inertial, orbit.TriadVelocity.EarthFixed], [0, 1]);
});
