// Sans-IO NTRIP request construction, stream state, events, and sourcetable parsing.

import { test } from "node:test";
import assert from "node:assert/strict";

import {
  NtripClientMachine,
  NtripState,
  ntripRequestBytes,
  parseNtripSourcetable,
} from "../pkg-node/sidereon.js";

const decoder = new TextDecoder();
const encoder = new TextEncoder();

const CONFIG = {
  host: "caster.example.test",
  port: 2101,
  mountpoint: "MOUNT",
  version: "rev2",
  credentials: { username: "user", password: "pass" },
  userAgentProduct: "sidereon-wasm/0.10.1",
  ggaIntervalS: 10,
};

test("ntripRequestBytes builds the core HTTP request bytes", () => {
  assert.equal(
    decoder.decode(ntripRequestBytes(CONFIG)),
    [
      "GET /MOUNT HTTP/1.1",
      "Host: caster.example.test:2101",
      "Ntrip-Version: Ntrip/2.0",
      "User-Agent: NTRIP sidereon-wasm/0.10.1",
      "Authorization: Basic dXNlcjpwYXNz",
      "Connection: close",
      "",
      "",
    ].join("\r\n"),
  );
});

test("NtripClientMachine emits connection, payload, and stream-end events", () => {
  const machine = new NtripClientMachine(CONFIG);
  assert.equal(machine.state, NtripState.Idle);
  assert.equal(machine.stateLabel, "idle");
  assert.equal(
    decoder.decode(machine.connectionRequest()),
    decoder.decode(ntripRequestBytes(CONFIG)),
  );

  const events = machine.push(
    encoder.encode(
      "HTTP/1.1 200 OK\r\nContent-Type: gnss/data\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n0\r\n\r\n",
    ),
  );
  assert.equal(events.length, 3);
  assert.equal(events[0].kind, "connected");
  assert.equal(events[0].version, "rev2");
  assert.equal(events[0].chunked, true);
  assert.deepEqual(events[0].headers, [
    { name: "Content-Type", value: "gnss/data" },
    { name: "Transfer-Encoding", value: "chunked" },
  ]);
  assert.equal(events[1].kind, "payload");
  assert.deepEqual(events[1].payload, [97, 98, 99]);
  assert.equal(events[2].kind, "streamEnded");
  assert.equal(machine.stateLabel, "closed");
});

test("parseNtripSourcetable exposes stream rows from Uint8Array input", () => {
  const table = parseNtripSourcetable(
    encoder.encode(
      "STR;MOUNT;Ident;RTCM 3.3;1004(1),1012(1);2;GPS+GLO;NET;USA;40.1;-105.2;1;0;GEN;none;B;N;9600;misc\r\nENDSOURCETABLE\r\n",
    ),
  );

  assert.equal(table.recordCount, 1);
  assert.equal(table.streamCount, 1);
  assert.equal(table.streams[0].mountpoint, "MOUNT");
  assert.equal(table.streams[0].navSystem, "GPS+GLO");
  assert.deepEqual(table.streams[0].latDeg, { kind: "parsed", value: 40.1 });
  assert.deepEqual(table.streams[0].lonDeg, { kind: "parsed", value: -105.2 });
  assert.deepEqual(table.streams[0].nmeaRequired, { kind: "parsed", value: true });
  assert.deepEqual(table.streams[0].networkSolution, { kind: "parsed", value: false });
  assert.equal(table.streams[0].authentication, "basic");
  assert.deepEqual(table.streams[0].bitrate, { kind: "parsed", value: 9600 });
});
