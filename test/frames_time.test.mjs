// Frames + time binding reproduces the engine numbers bit-for-bit, against the
// same fixture (frames_time.json) the Rust core and the Python binding assert
// on. Every scale, sidereal angle, nutation/precession matrix, and frame
// transform is compared by exact IEEE-754 bit pattern.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  Instant,
  GnssWeekTow,
  TimeScale,
  timeScaleAbbrev,
  leapSeconds,
  leapSecondsBatch,
  leapSecondTableInfo,
  ut1CoverageInfo,
  temeToGcrs,
  gcrsToItrs,
  itrsToGcrs,
  geodeticToEcef,
  ecefToGeodetic,
} from "../pkg-node/sidereon.js";
import { fixtureJson, hexToF64, f64Bits, bigints } from "./helpers.mjs";

const FX = fixtureJson("frames_time.json");
const SAMPLE_POS = FX.sample.position_km_hex.map(hexToF64);
const SAMPLE_VEL = FX.sample.velocity_km_s_hex.map(hexToF64);

// Independent Skyfield 1.49 oracle. Unlike frames_time.json, these values
// were captured from Skyfield and were not emitted by Sidereon.
const SKYFIELD_TEME_POS = ["0x40ace86c23dffb6b", "0x409f7fa61c81cb47", "0x40b4bd8359159cde"];
const SKYFIELD_TEME_VEL = ["0xc00b2ffb7cf9ad7d", "0x401b7a8751f7fc4a", "0xbfceb36925f07cb4"];
const SKYFIELD_GCRS_POS = ["0x40ad0bd9193713e1", "0x409f41a3b2073733", "0x40b4b6ffad1289d1"];
const SKYFIELD_GCRS_VEL = ["0xc00af690723d6cb1", "0x401b88e06212f969", "0xbfcde8575471eaf0"];
const SKYFIELD_ITRS_POS = ["0xc092d5d32b319db8", "0x40af8b3b3a722474", "0x40b4bd8359159cdb"];

const eqBits = (value, hex) => assert.equal(f64Bits(value), BigInt(hex));
const epochsArray = () => bigints(FX.epochs.map((e) => e.unix_micros));
const tile = (vec, n) => {
  const out = new Float64Array(n * 3);
  for (let i = 0; i < n; i++) out.set(vec, i * 3);
  return out;
};

test("instant scales match reference bits", () => {
  for (const e of FX.epochs) {
    const inst = Instant.fromUnixMicros(BigInt(e.unix_micros));
    eqBits(inst.jdWhole, e.jd_whole_hex);
    eqBits(inst.ttJd, e.tt_jd_hex);
    eqBits(inst.ut1Jd, e.ut1_jd_hex);
    eqBits(inst.tdbJd, e.tdb_jd_hex);
    eqBits(inst.ttFraction, e.tt_fraction_hex);
    eqBits(inst.ut1Fraction, e.ut1_fraction_hex);
    eqBits(inst.tdbFraction, e.tdb_fraction_hex);
    eqBits(inst.deltaTSeconds, e.delta_t_seconds_hex);
    eqBits(inst.meanObliquityRadians, e.mean_obliquity_radians_hex);
  }
});

test("instant sidereal time matches reference bits", () => {
  for (const e of FX.epochs) {
    const inst = Instant.fromUnixMicros(BigInt(e.unix_micros));
    eqBits(inst.gmstRadians(), e.gmst_radians_hex);
    eqBits(inst.gastRadians(), e.gast_radians_hex);
  }
});

test("instant nutation + precession match reference bits", () => {
  for (const e of FX.epochs) {
    const inst = Instant.fromUnixMicros(BigInt(e.unix_micros));
    const [dpsi, deps] = inst.nutationAngles();
    eqBits(dpsi, e.nutation_dpsi_hex);
    eqBits(deps, e.nutation_deps_hex);

    const prec = inst.precessionMatrix();
    const nut = inst.nutationMatrix();
    assert.equal(prec.length, 9);
    assert.equal(nut.length, 9);
    for (let i = 0; i < 3; i++) {
      for (let j = 0; j < 3; j++) {
        eqBits(prec[i * 3 + j], e.precession_matrix_hex[i][j]);
        eqBits(nut[i * 3 + j], e.nutation_matrix_hex[i][j]);
      }
    }
  }
});

test("instant from UTC equals unix path", () => {
  for (const e of FX.epochs) {
    const c = e.calendar;
    const second = c.second + c.microsecond / 1_000_000.0;
    const inst = Instant.fromUtc(c.year, c.month, c.day, c.hour, c.minute, second);
    assert.equal(inst.unixMicros, BigInt(e.unix_micros));
    eqBits(inst.ttJd, e.tt_jd_hex);
  }
});

test("two-part Julian date recombines", () => {
  const e = FX.epochs[0];
  const inst = Instant.fromUnixMicros(BigInt(e.unix_micros));
  const split = inst.ttJdSplit;
  assert.equal(split.whole, inst.jdWhole);
  assert.equal(split.fraction, inst.ttFraction);
  assert.equal(split.jd, split.whole + split.fraction);
  assert.equal(inst.ut1JdSplit.fraction, inst.ut1Fraction);
  assert.equal(inst.tdbJdSplit.fraction, inst.tdbFraction);
});

test("temeToGcrs matches reference bits (both compat paths)", () => {
  const epochs = epochsArray();
  const n = FX.epochs.length;
  const pos = tile(SAMPLE_POS, n);
  const vel = tile(SAMPLE_VEL, n);

  for (const [compat, key] of [
    [true, "teme_to_gcrs_skyfield"],
    [false, "teme_to_gcrs_direct"],
  ]) {
    const result = temeToGcrs(pos, vel, epochs, compat);
    assert.equal(result.epochCount, n);
    const gp = result.positionKm;
    const gv = result.velocityKmS;
    assert.equal(gp.length, n * 3);
    FX.epochs.forEach((e, idx) => {
      const ref = e[key];
      for (let axis = 0; axis < 3; axis++) {
        eqBits(gp[idx * 3 + axis], ref.position_hex[axis]);
        eqBits(gv[idx * 3 + axis], ref.velocity_hex[axis]);
      }
    });
  }
});

test("temeToGcrs matches Skyfield 1.49 at zero ULP", () => {
  const epoch = Instant.fromUtc(2018, 7, 4, 0, 0, 0).unixMicros;
  const result = temeToGcrs(
    Float64Array.from(SKYFIELD_TEME_POS.map(hexToF64)),
    Float64Array.from(SKYFIELD_TEME_VEL.map(hexToF64)),
    BigInt64Array.from([epoch]),
    true,
  );

  for (let axis = 0; axis < 3; axis++) {
    eqBits(result.positionKm[axis], SKYFIELD_GCRS_POS[axis]);
    eqBits(result.velocityKmS[axis], SKYFIELD_GCRS_VEL[axis]);
  }
});

test("gcrsToItrs matches reference bits (both compat paths)", () => {
  const epochs = epochsArray();
  const n = FX.epochs.length;
  const pos = tile(SAMPLE_POS, n);
  for (const [compat, key] of [
    [true, "gcrs_to_itrs_skyfield_hex"],
    [false, "gcrs_to_itrs_direct_hex"],
  ]) {
    const itrs = gcrsToItrs(pos, epochs, compat);
    assert.equal(itrs.length, n * 3);
    FX.epochs.forEach((e, idx) => {
      for (let axis = 0; axis < 3; axis++) eqBits(itrs[idx * 3 + axis], e[key][axis]);
    });
  }
});

test("gcrsToItrs matches Skyfield 1.49 at zero ULP", () => {
  const epoch = Instant.fromUtc(2018, 7, 4, 0, 0, 0).unixMicros;
  const itrs = gcrsToItrs(
    Float64Array.from(SKYFIELD_GCRS_POS.map(hexToF64)),
    BigInt64Array.from([epoch]),
    true,
  );

  for (let axis = 0; axis < 3; axis++) eqBits(itrs[axis], SKYFIELD_ITRS_POS[axis]);
});

test("itrsToGcrs matches reference bits", () => {
  const epochs = epochsArray();
  const n = FX.epochs.length;
  const gcrs = itrsToGcrs(tile(SAMPLE_POS, n), epochs);
  assert.equal(gcrs.length, n * 3);
  FX.epochs.forEach((e, idx) => {
    for (let axis = 0; axis < 3; axis++) eqBits(gcrs[idx * 3 + axis], e.itrs_to_gcrs_hex[axis]);
  });
});

test("geodetic <-> ecef round matches reference bits", () => {
  const g2e = FX.geodetic_to_ecef;
  const ecef = geodeticToEcef(Float64Array.from(g2e.input_hex.map(hexToF64)));
  for (let axis = 0; axis < 3; axis++) eqBits(ecef[axis], g2e.ecef_km_hex[axis]);

  const e2g = FX.ecef_to_geodetic;
  const geo = ecefToGeodetic(Float64Array.from(e2g.input_km_hex.map(hexToF64)));
  for (let axis = 0; axis < 3; axis++) eqBits(geo[axis], e2g.geodetic_hex[axis]);
});

test("leap seconds match reference bits (scalar + batch)", () => {
  for (const c of FX.leap_seconds_cases) {
    eqBits(leapSeconds(c.year, c.month, c.day), c.value_hex);
  }
  const flat = [];
  for (const c of FX.leap_seconds_cases) flat.push(c.year, c.month, c.day);
  const values = leapSecondsBatch(Int32Array.from(flat));
  assert.equal(values.length, FX.leap_seconds_cases.length);
  FX.leap_seconds_cases.forEach((c, i) => eqBits(values[i], c.value_hex));
});

test("leap-second and UT1 table info match reference", () => {
  const ls = leapSecondTableInfo();
  const lref = FX.leap_second_table;
  assert.equal(ls.source, lref.source);
  assert.equal(ls.firstMjd, lref.first_mjd);
  assert.equal(ls.lastMjd, lref.last_mjd);
  assert.equal(ls.entries, lref.entries);

  const u = ut1CoverageInfo();
  const uref = FX.ut1_coverage;
  assert.equal(u.source, uref.source);
  assert.equal(u.firstMjd, uref.first_mjd);
  assert.equal(u.lastMjd, uref.last_mjd);
  assert.equal(u.entries, uref.entries);
  eqBits(u.firstJdTt, uref.first_jd_tt_hex);
  eqBits(u.lastJdTt, uref.last_jd_tt_hex);
});

test("GnssWeekTow normalizes and unrolls", () => {
  const ref = FX.gnss_week_tow;
  const wt = new GnssWeekTow(TimeScale.Gpst, ref.input_week, hexToF64(ref.input_tow_s_hex));
  assert.equal(wt.system, TimeScale.Gpst);
  assert.equal(wt.week, ref.input_week);

  const norm = wt.normalized();
  assert.equal(norm.week, ref.normalized_week);
  eqBits(norm.towS, ref.normalized_tow_s_hex);
  assert.equal(wt.unrolledWeek(2), ref.unrolled_week_2_rollovers);
});

test("TimeScale abbrev", () => {
  assert.equal(timeScaleAbbrev(TimeScale.Gpst), "GPST");
  assert.notEqual(TimeScale.Utc, TimeScale.Tai);
});

test("transform shape errors throw", () => {
  const epochs = epochsArray();
  const n = FX.epochs.length;
  assert.throws(() => gcrsToItrs(new Float64Array(0), new BigInt64Array(0)));
  assert.throws(() => gcrsToItrs(tile(SAMPLE_POS, n + 1), epochs));
  assert.throws(() => Instant.fromUtc(2020, 13, 1));
});
