// NMEA parse, streaming accumulation, and GGA writing pinned to core outputs.

import { test } from "node:test";
import assert from "node:assert/strict";

import { NmeaAccumulator, nmeaWriteGga, parseNmea } from "../pkg-node/sidereon.js";

const SAMPLE = [
  "$GPRMC,123520,A,4807.038,N,01131.000,E,22.4,84.4,230394,3.1,W,A,S*72",
  "$GNGSA,A,3,01,02,03,04,,,,,,,,,1.5,0.9,1.2,1*3B",
  "$GPGSV,1,1,01,01,45,083,42,1*58",
  "$GPGGA,123521,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,*4C",
].join("\r\n");

const bytes = (text) => Buffer.from(text, "utf8");

test("parseNmea groups sentences into core epochs", () => {
  const parsed = parseNmea(bytes(SAMPLE));

  assert.equal(parsed.sentenceCount, 4);
  assert.equal(parsed.epochCount, 2);
  assert.deepEqual(parsed.diagnostics, { skipCount: 0, warningCount: 0, skips: [], warnings: [] });

  const first = parsed.epochs[0];
  assert.equal(first.date.year, 1994);
  assert.equal(first.date.month, 3);
  assert.equal(first.date.day, 23);
  assert.equal(first.timeOfDay.second, 20);
  assert.equal(first.instantUtcJ2000S, -182301880);
  assert.equal(first.position.latDeg, 48.1173);
  assert.equal(first.position.lonDeg, 11.516666666666667);
  assert.equal(first.pdop, 1.5);
  assert.equal(first.hdop, 0.9);
  assert.equal(first.vdop, 1.2);
  assert.deepEqual(
    first.usedSatellites.map((sat) => sat.resolved),
    ["G01", "G02", "G03", "G04"],
  );
  assert.equal(first.satellitesInView, 1);

  const second = parsed.epochs[1];
  assert.equal(second.timeOfDay.second, 21);
  assert.equal(second.position.heightM, 592.3);
  assert.equal(second.gga.quality, 1);
  assert.equal(second.gga.satellitesUsed, 8);
  assert.equal(second.gga.altitudeMslM, 545.4);
  assert.equal(second.gga.geoidSeparationM, 46.9);
});

test("NmeaAccumulator streams partial chunks without losing epochs", () => {
  const acc = new NmeaAccumulator({ date: { year: 1994, month: 3, day: 23 } });
  const stream = `${SAMPLE}\r\n`;
  const a = acc.push(bytes(stream.slice(0, 120)));
  const b = acc.push(bytes(stream.slice(120)));
  const finished = acc.finish();

  assert.equal(a.epochs.length, 0);
  assert.equal(b.epochs.length, 1);
  assert.equal(finished.epochs.length, 1);
  assert.equal(acc.retainedLength, 0);
  assert.equal(b.epochs[0].instantUtcJ2000S, -182301880);
  assert.equal(finished.epochs[0].instantUtcJ2000S, -182301879);
});

test("nmeaWriteGga returns the core checksum and sentence layout", () => {
  assert.equal(
    nmeaWriteGga({
      talker: "GP",
      timeSecondsOfDay: 45319,
      latDeg: 48.1173,
      lonDeg: 11.516666666666667,
      coordinateDecimals: 3,
      quality: 1,
      satellitesUsed: 8,
      hdop: 0.9,
      altitudeMslM: 545.4,
      geoidSeparationM: 46.9,
    }),
    "$GPGGA,123519.00,4807.038,N,01131.000,E,1,08,0.90,545.4,M,46.9,M,,*59\r\n",
  );
});
