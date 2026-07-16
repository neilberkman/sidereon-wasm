import { buildExactCacheCommit, verifyExactCacheCommit } from "./pkg/sidereon.js";

const MARKERS = "markers";
const ENTRIES = "entries";

function request(request) {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

function complete(transaction) {
  return new Promise((resolve, reject) => {
    transaction.oncomplete = () => resolve();
    transaction.onerror = () => reject(transaction.error);
    transaction.onabort = () => reject(transaction.error ?? new Error("exact-cache transaction aborted"));
  });
}

function bytes(value) {
  return value instanceof Uint8Array ? value : new Uint8Array(value);
}

function entryId() {
  const random = new Uint8Array(16);
  globalThis.crypto.getRandomValues(random);
  return Array.from(random, (value) => value.toString(16).padStart(2, "0")).join("");
}

function baseKey(identity, source) {
  return `${source}\0${identity.cacheKey}`;
}

function entryKey(base, entry) {
  return `${base}\0${entry}`;
}

async function openDatabase(name) {
  if (typeof globalThis.indexedDB === "undefined") {
    throw new Error("BrowserExactProductCache requires IndexedDB");
  }
  const open = globalThis.indexedDB.open(name, 3);
  open.onupgradeneeded = () => {
    if (!open.result.objectStoreNames.contains(MARKERS)) {
      open.result.createObjectStore(MARKERS);
    }
    if (!open.result.objectStoreNames.contains(ENTRIES)) {
      open.result.createObjectStore(ENTRIES);
    }
  };
  return request(open);
}

class LockedExactCache {
  constructor(owner, identity, source, base) {
    this.owner = owner;
    this.identity = identity;
    this.source = source;
    this.base = base;
  }

  read() {
    return this.owner._read(this.identity, this.source, this.base);
  }

  async publish(product, archive, provenance) {
    product = bytes(product);
    archive = bytes(archive);
    provenance = bytes(provenance);
    const entry = entryId();
    const marker = buildExactCacheCommit(
      this.identity,
      this.source,
      entry,
      product,
      archive,
      provenance,
    );
    const transaction = this.owner.database.transaction([MARKERS, ENTRIES], "readwrite", {
      durability: "strict",
    });
    transaction.objectStore(ENTRIES).add(
      {
        product: product.slice(),
        archive: archive.slice(),
        provenance: provenance.slice(),
      },
      entryKey(this.base, entry),
    );
    transaction.objectStore(MARKERS).put({ entry, marker: marker.slice() }, this.base);
    await complete(transaction);
    return { entryId: entry, product, archive, provenance, marker };
  }

  async cleanupAbandoned() {
    const transaction = this.owner.database.transaction([MARKERS, ENTRIES], "readwrite", {
      durability: "strict",
    });
    const marker = await request(transaction.objectStore(MARKERS).get(this.base));
    const current = marker?.entry;
    const store = transaction.objectStore(ENTRIES);
    const range = globalThis.IDBKeyRange.bound(`${this.base}\0`, `${this.base}\0\uffff`);
    await new Promise((resolve, reject) => {
      const cursor = store.openCursor(range);
      cursor.onerror = () => reject(cursor.error);
      cursor.onsuccess = () => {
        const item = cursor.result;
        if (!item) {
          resolve();
          return;
        }
        if (item.key !== entryKey(this.base, current)) item.delete();
        item.continue();
      };
    });
    await complete(transaction);
  }
}

/**
 * Browser exact-product cache using Web Locks for cross-tab/worker acquisition
 * coordination and one durable IndexedDB transaction for entry publication.
 */
export class BrowserExactProductCache {
  static async open({ name = "sidereon-exact-products-v3" } = {}) {
    return new BrowserExactProductCache(await openDatabase(name));
  }

  constructor(database) {
    this.database = database;
  }

  async withLock(identity, source, operation, { timeoutMs = 30_000 } = {}) {
    if (!Number.isFinite(timeoutMs) || timeoutMs < 0) {
      throw new TypeError("timeoutMs must be finite and non-negative");
    }
    if (!globalThis.navigator?.locks) {
      throw new Error("BrowserExactProductCache requires the Web Locks API");
    }
    const base = baseKey(identity, source);
    const lockName = `sidereon-exact-cache:${base}`;
    if (timeoutMs === 0) {
      return globalThis.navigator.locks.request(
        lockName,
        { mode: "exclusive", ifAvailable: true },
        (lock) => {
          if (lock === null) throw new Error("timed out waiting for exact-cache lock");
          return operation(new LockedExactCache(this, identity, source, base));
        },
      );
    }
    const controller = new globalThis.AbortController();
    const timer = globalThis.setTimeout(() => controller.abort(), timeoutMs);
    let acquired = false;
    try {
      return await globalThis.navigator.locks.request(
        lockName,
        { mode: "exclusive", signal: controller.signal },
        () => {
          acquired = true;
          globalThis.clearTimeout(timer);
          return operation(new LockedExactCache(this, identity, source, base));
        },
      );
    } catch (error) {
      if (!acquired && controller.signal.aborted) {
        throw new Error("timed out waiting for exact-cache lock", { cause: error });
      }
      throw error;
    } finally {
      globalThis.clearTimeout(timer);
    }
  }

  read(identity, source) {
    return this._read(identity, source, baseKey(identity, source));
  }

  _read(identity, source, base) {
    return new Promise((resolve, reject) => {
      const transaction = this.database.transaction([MARKERS, ENTRIES], "readonly");
      const markerRequest = transaction.objectStore(MARKERS).get(base);
      let result;
      markerRequest.onerror = () => reject(markerRequest.error);
      markerRequest.onsuccess = () => {
        const committed = markerRequest.result;
        if (!committed) {
          result = null;
          return;
        }
        const entryRequest = transaction
          .objectStore(ENTRIES)
          .get(entryKey(base, committed.entry));
        entryRequest.onerror = () => reject(entryRequest.error);
        entryRequest.onsuccess = () => {
          const entry = entryRequest.result;
          if (!entry) {
            reject(new Error("committed exact-cache entry is incomplete"));
            return;
          }
          const product = bytes(entry.product);
          const archive = bytes(entry.archive);
          const provenance = bytes(entry.provenance);
          const marker = bytes(committed.marker);
          let verified;
          try {
            verified = verifyExactCacheCommit(
              identity,
              source,
              marker,
              product,
              archive,
              provenance,
            );
          } catch (error) {
            reject(error);
            return;
          }
          if (verified !== committed.entry) {
            reject(new Error("exact-cache entry identifier mismatch"));
            return;
          }
          result = {
            entryId: verified,
            product,
            archive,
            provenance,
            marker,
          };
        };
      };
      transaction.onerror = () => reject(transaction.error);
      transaction.onabort = () => reject(transaction.error ?? new Error("exact-cache read aborted"));
      transaction.oncomplete = () => resolve(result);
    });
  }

  close() {
    this.database.close();
  }
}
