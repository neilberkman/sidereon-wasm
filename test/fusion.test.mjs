// GNSS/INS fusion binding parity: stateful filter updates, time-sync replay,
// UKF selection, tight SP3 rows, and binary state codec round-trip.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";

import { GnssInsFilter, fusionStateBytesRoundTrip, loadSp3 } from "../pkg-node/sidereon.js";
import { fixture, f64Bits } from "./helpers.mjs";

const WGS84_A_M = 6378137.0;
const OMEGA_E = 7.2921151467e-5;

const eqBits = (value, hex) => assert.equal(f64Bits(value), BigInt(hex));
const sha256 = (bytes) => createHash("sha256").update(Buffer.from(bytes)).digest("hex");

function baseConfig(filterKind = "ekf") {
  return {
    initialState: {
      tJ2000S: 0,
      positionEcefM: [WGS84_A_M, 0, 0],
      velocityEcefMps: [0, 0, 0],
      attitudeBodyToEcef: [1, 0, 0, 0, 1, 0, 0, 0, 1],
    },
    layout: "fifteen",
    covarianceDiagonal: Array(15).fill(1),
    imuSpec: {
      accelVrwMpsSqrtS: 0,
      gyroArwRadSqrtS: 0,
      accelBiasInstabMps2: 0,
      gyroBiasInstabRps: 0,
      accelBiasTauS: Infinity,
      gyroBiasTauS: Infinity,
    },
    filterKind,
    timeSync: { imuCapacity: 4, checkpointCapacity: 4 },
  };
}

function increment(tJ2000S, dtS) {
  return {
    kind: "increment",
    tJ2000S,
    deltaVelocityMps: [0.015625 * dtS, -0.0078125 * dtS, 0.00390625 * dtS],
    deltaThetaRad: [OMEGA_E * dtS, 0.0009765625 * dtS, -0.00048828125 * dtS],
    dtS,
  };
}

function looseFix(tJ2000S, positionEcefM) {
  return {
    tJ2000S,
    positionEcefM,
    covariance: [
      [4, 0, 0],
      [0, 4, 0],
      [0, 0, 4],
    ],
    satellitesUsed: 8,
  };
}

test("fusion time-sync replay and state bytes match reference bits", () => {
  const filter = new GnssInsFilter(baseConfig());
  filter.propagate(increment(1, 1));
  const update = filter.updateLooseTimeSync(looseFix(0.75, [WGS84_A_M + 0.125, -0.0625, 0.03125]));
  const state = filter.state();
  const encoded = filter.encodeState();
  const roundTrip = fusionStateBytesRoundTrip(encoded);
  const restored = GnssInsFilter.fromStateBytes(baseConfig(), encoded).state();

  assert.equal(update.lateMeasurement, true);
  assert.equal(update.replayedImuSegments, 2);
  assert.equal(update.restoredCheckpointEpochJ2000S, 0);
  assert.deepEqual(Buffer.from(encoded), Buffer.from(roundTrip));
  assert.equal(encoded.length, 13798);
  assert.equal(sha256(encoded), "4276d2501010d57c3d4afba23bacefadcf4da359e9806cf1175457c0bcaf550f");

  eqBits(update.update.nis, "0x3FF76514A5737228");
  eqBits(state.positionEcefM[0], "0x415854A5451D0C2D");
  eqBits(state.positionEcefM[1], "0xBF98A921BD076F90");
  eqBits(state.covariance[0][0], "0x3FF9DCE1941815DA");
  eqBits(restored.positionEcefM[0], "0x415854A5451D0C2D");
});

test("fusion UKF option applies the same loose measurement surface", () => {
  const filter = new GnssInsFilter(baseConfig("ukf"));
  const update = filter.updateLoose(looseFix(0, [WGS84_A_M + 0.5, -0.25, 0.125]));

  assert.equal(update.applied, true);
  assert.equal(update.rows, 3);
  eqBits(update.nis, "0x3FB0CCCCCCCCCCCC");
});

test("fusion tight SP3 observation update matches reference bits", () => {
  const sp3 = loadSp3(fixture("GRG0MGXFIN_20201760000_01D_15M_ORB.SP3"));
  const filter = new GnssInsFilter({
    initialState: {
      tJ2000S: 646272000,
      positionEcefM: [4484128, 550582, 4487561],
      velocityEcefMps: [0, 0, 0],
      attitudeBodyToEcef: [1, 0, 0, 0, 1, 0, 0, 0, 1],
    },
    layout: "fifteen",
    covarianceDiagonal: Array(15).fill(10),
    imuSpec: "mems",
    filterKind: "ekf",
    tight: {
      lightTime: true,
      sagnac: true,
      initialClockBiasVarianceM2: 1e8,
      initialClockDriftVarianceM2S2: 1e4,
    },
  });

  const update = filter.updateTightSp3(sp3, {
    tJ2000S: 646272000,
    observations: [{ satelliteId: "G08", pseudorangeM: 23825519.8, pseudorangeSigmaM: 3.0 }],
  });
  const clock = filter.tightClockState();

  assert.equal(update.applied, true);
  assert.equal(update.rows, 1);
  eqBits(update.nis, "0x4021FFF5A609DA6D");
  eqBits(clock.biasM, "0x40DD4BF764C1C30C");
  eqBits(clock.covariance[0], "0x4032FFFFC36F2BC8");
});
