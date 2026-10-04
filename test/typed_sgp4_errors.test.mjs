// Public SGP4/TLE failures retain their core variants and payloads.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  DecayLatch,
  GroundStation,
  Tle,
  fitTle,
  parseTleFile,
  propagateBatch,
} from "../pkg-node/sidereon.js";
import { coreGoldens, fixtureJson, hexToF64 } from "./helpers.mjs";

const FX = fixtureJson("tle_roundtrip.json");

function thrown(run) {
  try {
    run();
  } catch (error) {
    return error;
  }
  assert.fail("expected the operation to throw");
}

function tleEpochUnixUs(year, dayOfYear) {
  const yearStart = BigInt(Date.UTC(year, 0, 1)) * 1000n;
  return yearStart + BigInt(Math.round((dayOfYear - 1) * 86_400_000_000));
}

function coreTleEpochSplit(year, dayOfYear) {
  // Mirrors days2mdhms_SGP4 + jday_SGP4 and the core's 1e-8-day rounding for
  // the parsed TLE epoch, rather than treating its Unix-microsecond surrogate
  // as the propagator's split epoch.
  const dayOfYearWhole = Math.floor(dayOfYear);
  const date = new Date(Date.UTC(year, 0, dayOfYearWhole));
  let temp = (dayOfYear - dayOfYearWhole) * 24;
  const hour = Math.floor(temp);
  temp = (temp - hour) * 60;
  const minute = Math.floor(temp);
  const second = (temp - minute) * 60;
  const month = date.getUTCMonth() + 1;
  const day = date.getUTCDate();
  const jd =
    367 * year -
    Math.floor(7 * (year + Math.floor((month + 9) / 12)) * 0.25) +
    Math.floor((275 * month) / 9) +
    day +
    1_721_013.5;
  const rawFraction = (second + minute * 60 + hour * 3600) / 86_400;
  const fraction = Math.round(rawFraction * 1e8) / 1e8;
  return [jd, fraction];
}

function coreUnixSplitFromMicroseconds(unixMicroseconds) {
  // Independently derives the split used by propagateWithDecayLatch's
  // unix_us_to_julian_date adapter: Euclidean days/remainder, then the
  // Unix-to-JD offset. This public path does not apply a TAI-UTC adjustment.
  const microsecondsPerDay = 86_400_000_000n;
  let days = unixMicroseconds / microsecondsPerDay;
  let remainder = unixMicroseconds % microsecondsPerDay;
  if (remainder < 0n) {
    days -= 1n;
    remainder += microsecondsPerDay;
  }

  let whole = 2_440_587 + Number(days);
  let fraction = 0.5 + Number(remainder) / Number(microsecondsPerDay);
  if (fraction >= 1) {
    whole += 1;
    fraction -= 1;
  }
  return [whole, fraction];
}

function coreMinutesSinceTleEpoch(unixMicroseconds, tleEpochSplit) {
  // Mirrors split_minutes_since_epoch: subtract each Julian-date part before
  // multiplying each difference by 1440.
  const [whole, fraction] = coreUnixSplitFromMicroseconds(unixMicroseconds);
  return (whole - tleEpochSplit[0]) * 1440 + (fraction - tleEpochSplit[1]) * 1440;
}

function exactFloatDetail(value) {
  const bytes = new ArrayBuffer(8);
  const view = new DataView(bytes);
  view.setFloat64(0, value, false);
  return {
    decimal: value.toString(),
    bitsHex: view.getBigUint64(0, false).toString(16).padStart(16, "0"),
  };
}

function assertNoUnencodedNumbers(value, path = "bestEffortFit") {
  if (typeof value === "number") {
    assert.fail(`${path} contains a number without exact string encoding`);
  }
  if (Array.isArray(value)) {
    value.forEach((entry, index) => assertNoUnencodedNumbers(entry, `${path}[${index}]`));
    return;
  }
  if (value !== null && typeof value === "object") {
    for (const [key, entry] of Object.entries(value)) {
      assertNoUnencodedNumbers(entry, `${path}.${key}`);
    }
  }
}

test("TLE parser and rejected-record diagnostics retain complete causes", () => {
  const checksum = FX.checksum_case;
  const strictError = thrown(() => new Tle(checksum.line1, checksum.line2));
  assert.equal(strictError.name, "Error");
  assert.equal(strictError.detail.family, "tle");
  assert.deepEqual(strictError.detail.cause, {
    kind: "checksumMismatch",
    lineLabel: "line 1",
    expected: 0,
    computed: 3,
  });
  assert.match(strictError.message, /checksum digit 0 does not match the computed checksum 3/);

  const nonDigitLine1 = `${FX.tle.line1.slice(0, 68)}X`;
  const nonDigitError = thrown(() => new Tle(nonDigitLine1, FX.tle.line2));
  assert.deepEqual(nonDigitError.detail.cause, {
    kind: "checksumNotDigit",
    lineLabel: "line 1",
    found: "X",
    computed: 3,
  });

  const parsed = parseTleFile(
    "BROKEN\n1 00001U 00000A   18184.80969102  .00000000  00000-0  00000-0 0  0001\n2 00001 not a valid line two",
  );
  try {
    const [rejected] = parsed.rejected;
    assert.equal(parsed.count, 0);
    assert.equal(rejected.issue, "invalid");
    assert.equal(rejected.detail.kind, "invalid");
    assert.equal(rejected.detail.message, rejected.message);
    assert.equal(rejected.detail.cause.kind, "invalidTle");
    assert.equal(rejected.message, `invalid TLE: ${rejected.detail.cause.message}`);

    const orphaned = parseTleFile(
      "2 00001 orphan\nORPHAN NAME\n1 00002U 00000A   18184.80969102  .00000000  00000-0  00000-0 0  0001\nLONE",
    );
    try {
      assert.deepEqual(
        orphaned.rejected.map(({ issue, lineNumber, name }) => ({ issue, lineNumber, name })),
        [
          { issue: "orphanLine2", lineNumber: 1, name: "" },
          { issue: "missingLine2", lineNumber: 2, name: "ORPHAN NAME" },
          { issue: "orphanName", lineNumber: 4, name: "LONE" },
        ],
      );
      assert.deepEqual(
        orphaned.rejected.map((row) => row.detail.kind),
        ["orphanLine2", "missingLine2", "orphanName"],
      );
    } finally {
      orphaned.free();
    }

    const missingLine2 = parseTleFile(
      "MISSING LINE 2\n1 00002U 00000A   18184.80969102  .00000000  00000-0  00000-0 0  0001",
    );
    try {
      assert.deepEqual(
        missingLine2.rejected.map(({ issue, lineNumber, name }) => ({ issue, lineNumber, name })),
        [{ issue: "missingLine2", lineNumber: 1, name: "MISSING LINE 2" }],
      );
      assert.equal(missingLine2.rejected[0].detail.kind, "missingLine2");
    } finally {
      missingLine2.free();
    }

    const unterminatedLine1 = parseTleFile(
      "1 00002U 00000A   18184.80969102  .00000000  00000-0  00000-0 0  0001",
    );
    try {
      assert.deepEqual(
        unterminatedLine1.rejected.map(({ issue, lineNumber, name }) => ({
          issue,
          lineNumber,
          name,
        })),
        [{ issue: "missingLine2", lineNumber: 1, name: "" }],
      );
      assert.equal(unterminatedLine1.rejected[0].detail.kind, "missingLine2");
    } finally {
      unterminatedLine1.free();
    }
  } finally {
    parsed.free();
  }

  const valid = new Tle(FX.tle.line1, FX.tle.line2);
  try {
    assert.equal(valid.checksumWarnings.length, 0);
    const prediction = valid.propagate(BigInt64Array.of(1_530_000_000_000_000n));
    try {
      assert.equal(prediction.positionKm.length, 3);
    } finally {
      prediction.free();
    }
  } finally {
    valid.free();
  }
});

test("look-angle and pass refusals retain their own typed validation records", () => {
  const tle = new Tle(FX.tle.line1, FX.tle.line2);
  const station = new GroundStation(91, 0, 0);
  try {
    const lookError = thrown(() =>
      tle.lookAngles(station, BigInt64Array.of(1_530_000_000_000_000n)),
    );
    assert.equal(lookError.detail.family, "lookAngle");
    assert.deepEqual(lookError.detail.cause, {
      kind: "invalidInput",
      field: "ground_station.latitude_deg",
      reason: "out of range",
    });

    const passError = thrown(() =>
      tle.findPasses(station, 1_530_000_000_000_000n, 1_530_000_060_000_000n),
    );
    assert.equal(passError.detail.family, "pass");
    assert.deepEqual(passError.detail.cause, {
      kind: "invalidInput",
      field: "ground_station.latitude_deg",
      reason: "out of range",
    });

    const validStation = new GroundStation(37, -122, 0);
    try {
      const validLook = tle.lookAngles(validStation, BigInt64Array.of(1_530_000_000_000_000n));
      try {
        assert.equal(validLook.rangeKm.length, 1);
      } finally {
        validLook.free();
      }
      const validPasses = tle.findPasses(
        validStation,
        1_530_000_000_000_000n,
        1_530_003_600_000_000n,
      );
      assert.ok(Array.isArray(validPasses));
      for (const pass of validPasses) pass.free();
    } finally {
      validStation.free();
    }
  } finally {
    station.free();
    tle.free();
  }
});

test("SGP4 decay and batch failures retain cause, exact epoch, and satellite index", () => {
  const line1 = "1 28872U 05037B   05333.02012661  .25992681  00000-0  24476-3 0  1534";
  const line2 = "2 28872  96.4736 157.9986 0303955 244.0492 110.6523 16.46015938 10708";
  const tle = new Tle(line1, line2);
  const epoch = tleEpochUnixUs(2005, 333.02012661);
  const tleEpochSplit = coreTleEpochSplit(2005, 333.02012661);
  const early = epoch + 1_000n * 60n * 1_000_000n;
  const decay = epoch + 1_440n * 60n * 1_000_000n;
  const later = epoch + 1_450n * 60n * 1_000_000n;
  const earlyPrediction = tle.propagate(BigInt64Array.of(early));
  try {
    assert.equal(earlyPrediction.positionKm.length, 3);
  } finally {
    earlyPrediction.free();
  }

  const rawFailure = thrown(() => tle.propagate(BigInt64Array.of(decay)));
  assert.equal(rawFailure.detail.family, "sgp4");
  assert.deepEqual(rawFailure.detail.cause, { kind: "sgp4", code: 6 });

  const batchFailure = thrown(() =>
    propagateBatch([new Tle(line1, line2)], BigInt64Array.of(decay)),
  );
  assert.equal(batchFailure.message, `satellite 0: ${rawFailure.message}`);
  assert.deepEqual(batchFailure.detail.cause, {
    kind: "satellitePropagation",
    satelliteIndex: 0,
    message: rawFailure.message,
    cause: { kind: "sgp4", code: 6 },
  });

  const latch = new DecayLatch();
  try {
    const propagationFailure = thrown(() =>
      tle.propagateWithDecayLatch(BigInt64Array.of(decay), latch),
    );
    assert.equal(propagationFailure.detail.family, "decayLatched");
    assert.deepEqual(propagationFailure.detail.cause, {
      kind: "decayed",
      firstFailingEpochMinutes: exactFloatDetail(coreMinutesSinceTleEpoch(decay, tleEpochSplit)),
      requestedEpochMinutes: exactFloatDetail(coreMinutesSinceTleEpoch(decay, tleEpochSplit)),
    });
    const latchedFailure = thrown(() =>
      tle.propagateWithDecayLatch(BigInt64Array.of(later), latch),
    );
    assert.deepEqual(latchedFailure.detail.cause, {
      kind: "decayed",
      firstFailingEpochMinutes: exactFloatDetail(coreMinutesSinceTleEpoch(decay, tleEpochSplit)),
      requestedEpochMinutes: exactFloatDetail(coreMinutesSinceTleEpoch(later, tleEpochSplit)),
    });
    const retained = structuredClone(latchedFailure.detail);
    latch.clear();
    const afterClear = tle.propagate(BigInt64Array.of(early));
    try {
      assert.equal(afterClear.positionKm.length, 3);
    } finally {
      afterClear.free();
    }
    assert.deepEqual(latchedFailure.detail, retained);
  } finally {
    latch.free();
    tle.free();
  }
});

test("fit errors retain every core discriminator and a complete best-effort fit", () => {
  const shortArc = thrown(() =>
    fitTle(
      [
        { epoch: [2_460_000, 0.25], positionTemeKm: [7000, 0, 0] },
        { epoch: [2_460_000, 0.251], positionTemeKm: [7000, 1, 0] },
      ],
      {},
    ),
  );
  assert.equal(shortArc.detail.family, "tleFit");
  assert.deepEqual(shortArc.detail.cause, { kind: "arcTooShort", samples: 2, needed: 3 });

  const golden = coreGoldens().tleFit;
  const samples = golden.samples.map((sample) => ({
    epoch: sample.epoch.map(hexToF64),
    positionTemeKm: sample.positionTemeKm.map(hexToF64),
    velocityTemeKmS: sample.velocityTemeKmS.map(hexToF64),
  }));
  const nonConvergence = thrown(() =>
    fitTle(samples, {
      fitBstar: true,
      useVelocity: true,
      velocityWeightS: 60,
      loss: "softL1",
      fScale: 1,
      xScale: "jac",
      maxNfev: 1,
      metadata: { catalogNumber: 25544, classification: "U", internationalDesignator: "98067A" },
    }),
  );
  assert.equal(nonConvergence.detail.family, "tleFit");
  assert.equal(nonConvergence.detail.cause.kind, "didNotConverge");
  const best = nonConvergence.detail.cause.bestEffortFit;
  assert.deepEqual(Object.keys(best).sort(), ["elements", "line1", "line2", "omm", "stats"]);
  assert.deepEqual(Object.keys(best.elements).sort(), [
    "argument_of_perigee_deg",
    "bstar",
    "catalog_number",
    "eccentricity",
    "epoch",
    "inclination_deg",
    "mean_anomaly_deg",
    "mean_motion_dot",
    "mean_motion_double_dot",
    "mean_motion_rev_per_day",
    "omm_epoch_days",
    "right_ascension_deg",
  ]);
  assert.deepEqual(
    Object.keys(best.omm).sort(),
    [
      "agom_m2_kg",
      "arg_of_pericenter_deg",
      "bstar",
      "bterm_m2_kg",
      "ccsds_omm_vers",
      "center_name",
      "classification",
      "classification_type",
      "comments",
      "covariance",
      "creation_date",
      "eccentricity",
      "element_set_no",
      "ephemeris_type",
      "epoch",
      "exact_sgp4_epoch",
      "gm_km3_s2",
      "inclination_deg",
      "mean_motion",
      "mean_motion_ddot",
      "mean_motion_dot",
      "mean_anomaly_deg",
      "mean_element_theory",
      "message_id",
      "norad_cat_id",
      "object_id",
      "object_name",
      "originator",
      "ra_of_asc_node_deg",
      "ref_frame",
      "ref_frame_epoch",
      "rev_at_epoch",
      "semi_major_axis_km",
      "spacecraft",
      "time_system",
      "user_defined",
      "quantize_tle_derived_fields",
    ].sort(),
  );
  assert.deepEqual(Object.keys(best.stats).sort(), [
    "bstar_observable",
    "cost",
    "max_position_km",
    "nfev",
    "njev",
    "optimality",
    "rms_position_axes_km",
    "rms_position_km",
    "rms_velocity_km_s",
    "seed_refine_passes",
    "status",
    "tle_rms_position_km",
  ]);
  assert.ok(best.elements);
  assert.ok(best.line1.startsWith("1 "));
  assert.ok(best.line2.startsWith("2 "));
  assert.ok(best.omm);
  assert.ok(best.stats);
  assert.equal(typeof best.elements.bstar.bitsHex, "string");
  assert.equal(best.elements.bstar.bitsHex.length, 16);
  assert.equal(typeof best.elements.epoch[0].bitsHex, "string");
  assert.ok(Object.hasOwn(best.elements, "omm_epoch_days"));
  if (best.elements.omm_epoch_days !== null) {
    assert.equal(typeof best.elements.omm_epoch_days.bitsHex, "string");
  }
  assert.deepEqual(best.omm.exact_sgp4_epoch, best.elements.epoch);
  assert.equal(best.omm.quantize_tle_derived_fields, false);
  assert.equal(typeof best.omm.bstar.bitsHex, "string");
  assert.equal(typeof best.omm.epoch.femtosecond.decimal, "string");
  assert.ok(Object.hasOwn(best.omm, "rev_at_epoch"));
  if (best.omm.rev_at_epoch !== null) {
    assert.equal(typeof best.omm.rev_at_epoch.decimal, "string");
  }
  assert.equal(typeof best.stats.nfev.decimal, "string");
  assert.equal(typeof best.stats.status.decimal, "string");
  assertNoUnencodedNumbers(best);
});
