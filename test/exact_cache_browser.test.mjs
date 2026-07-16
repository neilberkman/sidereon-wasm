import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { IDBKeyRange, indexedDB } from "fake-indexeddb";

import { initSync, productIdentity } from "../pkg/sidereon.js";
import { BrowserExactProductCache } from "../exact-cache.js";

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
