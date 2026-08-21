import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { IDBKeyRange, indexedDB } from "fake-indexeddb";

import { initSync, productIdentity } from "../pkg/sidereon.js";
import {
  BrowserExactProductCache,
  ExactCacheSingleFlightOptionsError,
  ExactCacheSingleFlightTimeoutError,
} from "../exact-cache.js";

initSync({
  module: readFileSync(new URL("../pkg/sidereon_bg.wasm", import.meta.url)),
});

function idbRequest(request) {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

function idbComplete(transaction) {
  return new Promise((resolve, reject) => {
    transaction.oncomplete = () => resolve();
    transaction.onerror = () => reject(transaction.error);
    transaction.onabort = () => reject(transaction.error);
  });
}

class TestLockManager {
  #tails = new Map();

  request(name, { signal, ifAvailable = false }, operation) {
    if (ifAvailable && this.#tails.has(name)) return Promise.resolve(operation(null));
    const predecessor = this.#tails.get(name) ?? Promise.resolve();
    let release;
    const held = new Promise((resolve) => {
      release = resolve;
    });
    const tail = predecessor.catch(() => {}).then(() => held);
    this.#tails.set(name, tail);
    return predecessor
      .catch(() => {})
      .then(() => {
        if (signal?.aborted) throw signal.reason;
        return operation({ name });
      })
      .finally(() => {
        release();
        if (this.#tails.get(name) === tail) this.#tails.delete(name);
      });
  }
}

Object.defineProperty(globalThis, "indexedDB", {
  configurable: true,
  value: indexedDB,
});
Object.defineProperty(globalThis, "IDBKeyRange", {
  configurable: true,
  value: IDBKeyRange,
});
Object.defineProperty(globalThis.navigator, "locks", {
  configurable: true,
  value: new TestLockManager(),
});

let databaseSequence = 0;

function databaseName(label) {
  databaseSequence += 1;
  return `sidereon-exact-cache-${label}-${process.pid}-${Date.now()}-${databaseSequence}`;
}

function exactCacheFixture() {
  return {
    identity: productIdentity("cod_prd1", "ionex", 2026, 7, 16),
    source: "direct",
    product: new TextEncoder().encode("validated IONEX"),
    archive: new TextEncoder().encode("distributor archive"),
    provenance: new TextEncoder().encode('{"source":"direct"}'),
  };
}

test("browser cache coordinates one acquisition and rejects stored-byte corruption", async () => {
  const name = `sidereon-exact-cache-test-${process.pid}-${Date.now()}`;
  const first = await BrowserExactProductCache.open({ name });
  const second = await BrowserExactProductCache.open({ name });
  const identity = productIdentity("cod_prd1", "ionex", 2026, 7, 16);
  const source = "direct";
  const product = new TextEncoder().encode("validated IONEX");
  const archive = new TextEncoder().encode("distributor archive");
  const provenance = new TextEncoder().encode('{"source":"direct"}');
  let acquired = 0;
  let releaseFirst;
  const firstMayPublish = new Promise((resolve) => {
    releaseFirst = resolve;
  });
  let firstEntered;
  const entered = new Promise((resolve) => {
    firstEntered = resolve;
  });

  const firstTask = first.withLock(identity, source, async (cache) => {
    assert.equal(await cache.read(), null);
    acquired += 1;
    firstEntered();
    await firstMayPublish;
    return cache.publish(product, archive, provenance);
  });
  await entered;
  await assert.rejects(
    second.withLock(identity, source, () => undefined, { timeoutMs: 0 }),
    /timed out waiting for exact-cache lock/,
  );
  const secondTask = second.withLock(identity, source, async (cache) => {
    const hit = await cache.read();
    if (hit === null) acquired += 1;
    return hit;
  });
  releaseFirst();

  const [published, reused] = await Promise.all([firstTask, secondTask]);
  assert.equal(acquired, 1);
  assert.equal(reused.entryId, published.entryId);
  assert.equal(await first.withLock(identity, source, () => true, { timeoutMs: 0 }), true);
  await assert.rejects(
    first.withLock(
      identity,
      source,
      async () => {
        await new Promise((resolve) => setTimeout(resolve, 5));
        throw new Error("operation failed after lock acquisition");
      },
      { timeoutMs: 1 },
    ),
    /operation failed after lock acquisition/,
  );
  assert.deepEqual(reused.product, product);
  assert.deepEqual(reused.archive, archive);
  assert.deepEqual(reused.provenance, provenance);

  const base = `${source}\0${identity.cacheKey}`;
  const orphanId = "ffffffffffffffffffffffffffffffff";
  const orphanKey = `${base}\0${orphanId}`;
  const seedOrphan = first.database.transaction(["entries"], "readwrite", {
    durability: "strict",
  });
  seedOrphan.objectStore("entries").add(
    {
      product: new Uint8Array(),
      archive: new Uint8Array(),
      provenance: new Uint8Array(),
    },
    orphanKey,
  );
  await idbComplete(seedOrphan);
  await second.withLock(identity, source, (cache) => cache.cleanupAbandoned());
  const inspectCleanup = first.database.transaction(["entries"], "readonly");
  assert.equal(await idbRequest(inspectCleanup.objectStore("entries").get(orphanKey)), undefined);
  await idbComplete(inspectCleanup);

  const transaction = first.database.transaction(["entries"], "readwrite", {
    durability: "strict",
  });
  const entries = transaction.objectStore("entries");
  const key = `${base}\0${published.entryId}`;
  const stored = await idbRequest(entries.get(key));
  stored.product = new TextEncoder().encode("corrupt product");
  entries.put(stored, key);
  await idbComplete(transaction);

  await assert.rejects(first.read(identity, source), /identity, source, or bytes/);
  first.close();
  second.close();
  await idbRequest(indexedDB.deleteDatabase(name));
});

test("single-flight returns a pre-committed hit without entering acquisition", async () => {
  const name = databaseName("single-flight-hit");
  const cache = await BrowserExactProductCache.open({ name });
  const { identity, source, product, archive, provenance } = exactCacheFixture();
  const published = await cache.withLock(identity, source, (locked) =>
    locked.publish(product, archive, provenance),
  );
  let fetchInvoked = false;

  const opened = await cache.openSingleFlight(identity, source);
  if (opened.kind === "owner") {
    fetchInvoked = true;
    await opened.owner.abandon();
  }

  assert.equal(opened.kind, "hit");
  assert.equal(fetchInvoked, false);
  assert.equal(opened.entry.entryId, published.entryId);
  assert.deepEqual(opened.entry.product, product);
  assert.deepEqual(opened.entry.archive, archive);
  assert.deepEqual(opened.entry.provenance, provenance);

  cache.close();
  await idbRequest(indexedDB.deleteDatabase(name));
});

test("single-flight owner publishes and the next open returns a hit", async () => {
  const name = databaseName("single-flight-owner");
  const first = await BrowserExactProductCache.open({ name });
  const second = await BrowserExactProductCache.open({ name });
  const { identity, source, product, archive, provenance } = exactCacheFixture();

  const opened = await first.openSingleFlight(identity, source);
  assert.equal(opened.kind, "owner");
  await opened.owner.heartbeat();
  const published = await opened.owner.publish(product, archive, provenance);

  const reused = await second.openSingleFlight(identity, source);
  assert.equal(reused.kind, "hit");
  assert.equal(reused.entry.entryId, published.entryId);
  assert.deepEqual(reused.entry.product, product);

  first.close();
  second.close();
  await idbRequest(indexedDB.deleteDatabase(name));
});

test("single-flight maps a bounded wait on a held owner to its timeout error", async () => {
  const name = databaseName("single-flight-timeout");
  const first = await BrowserExactProductCache.open({ name });
  const second = await BrowserExactProductCache.open({ name });
  const { identity, source } = exactCacheFixture();
  const options = {
    pollIntervalMs: 1,
    heartbeatIntervalMs: 20,
    livenessTimeoutMs: 50,
    waitTimeoutMs: 5,
  };

  const held = await first.openSingleFlight(identity, source, options);
  assert.equal(held.kind, "owner");
  await assert.rejects(
    second.openSingleFlight(identity, source, options),
    ExactCacheSingleFlightTimeoutError,
  );
  await held.owner.abandon();

  first.close();
  second.close();
  await idbRequest(indexedDB.deleteDatabase(name));
});

test("single-flight rejects invalid duration options with its typed error", async () => {
  const name = databaseName("single-flight-options");
  const cache = await BrowserExactProductCache.open({ name });
  const { identity, source, product, archive, provenance } = exactCacheFixture();
  await cache.withLock(identity, source, (locked) => locked.publish(product, archive, provenance));

  for (const options of [
    { pollIntervalMs: 0 },
    { heartbeatIntervalMs: 10, livenessTimeoutMs: 10 },
    { waitTimeoutMs: Number.NaN },
    { pollIntervalMs: "soon" },
  ]) {
    await assert.rejects(
      cache.openSingleFlight(identity, source, options),
      ExactCacheSingleFlightOptionsError,
    );
  }

  cache.close();
  await idbRequest(indexedDB.deleteDatabase(name));
});
