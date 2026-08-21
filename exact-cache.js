import {
  buildExactCacheCommit,
  ExactCacheSingleFlightWait as CoreExactCacheSingleFlightWait,
  verifyExactCacheCommit,
} from "./pkg/sidereon.js";

const MARKERS = "markers";
const ENTRIES = "entries";
const IN_FLIGHT = "inFlight";
const HEARTBEATS = "heartbeats";
const REVISION_ENCODER = new globalThis.TextEncoder();
const MAX_TIMER_DELAY_MS = 2_147_483_647;

const SINGLE_FLIGHT_DEFAULTS = Object.freeze({
  pollIntervalMs: 50,
  heartbeatIntervalMs: 5_000,
  livenessTimeoutMs: 30_000,
  waitTimeoutMs: 30 * 60 * 1_000,
});

export class ExactCacheSingleFlightOptionsError extends TypeError {
  constructor(message = "invalid exact-cache single-flight options", options) {
    super(message, options);
    this.name = "ExactCacheSingleFlightOptionsError";
  }
}

export class ExactCacheSingleFlightTimeoutError extends Error {
  constructor(message = "timed out waiting for the exact-cache in-flight owner", options) {
    super(message, options);
    this.name = "ExactCacheSingleFlightTimeoutError";
  }
}

export class ExactCacheSingleFlightOwnershipLostError extends Error {
  constructor(message = "exact-cache single-flight ownership was lost", options) {
    super(message, options);
    this.name = "ExactCacheSingleFlightOwnershipLostError";
  }
}

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
    transaction.onabort = () =>
      reject(transaction.error ?? new Error("exact-cache transaction aborted"));
  });
}

async function runTransaction(transaction, operation) {
  const completion = complete(transaction);
  try {
    const result = await operation();
    await completion;
    return result;
  } catch (error) {
    try {
      transaction.abort();
    } catch {
      // The request may already have aborted or completed the transaction.
    }
    await completion.catch(() => {});
    throw error;
  }
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

function monotonicNow() {
  if (typeof globalThis.performance?.now !== "function") {
    throw new Error("BrowserExactProductCache requires performance.now()");
  }
  return globalThis.performance.now();
}

function timerDelay(milliseconds) {
  return Math.min(milliseconds, MAX_TIMER_DELAY_MS);
}

function waitFor(milliseconds) {
  return new Promise((resolve) => globalThis.setTimeout(resolve, timerDelay(milliseconds)));
}

function ownerRevision(inFlight, heartbeat) {
  return JSON.stringify([inFlight, heartbeat]);
}

function singleFlightWait(options, startedMs) {
  if (options === undefined) options = {};
  if (options === null || typeof options !== "object" || Array.isArray(options)) {
    throw new ExactCacheSingleFlightOptionsError();
  }
  const normalized = {
    pollIntervalMs: options.pollIntervalMs ?? SINGLE_FLIGHT_DEFAULTS.pollIntervalMs,
    heartbeatIntervalMs: options.heartbeatIntervalMs ?? SINGLE_FLIGHT_DEFAULTS.heartbeatIntervalMs,
    livenessTimeoutMs: options.livenessTimeoutMs ?? SINGLE_FLIGHT_DEFAULTS.livenessTimeoutMs,
    waitTimeoutMs: options.waitTimeoutMs ?? SINGLE_FLIGHT_DEFAULTS.waitTimeoutMs,
  };
  try {
    return {
      heartbeatIntervalMs: normalized.heartbeatIntervalMs,
      wait: new CoreExactCacheSingleFlightWait(
        startedMs,
        normalized.pollIntervalMs,
        normalized.heartbeatIntervalMs,
        normalized.livenessTimeoutMs,
        normalized.waitTimeoutMs,
      ),
    };
  } catch (cause) {
    throw new ExactCacheSingleFlightOptionsError(undefined, { cause });
  }
}

async function readCommitted(transaction, identity, source, base) {
  const committed = await request(transaction.objectStore(MARKERS).get(base));
  if (committed === undefined) return null;
  const entry = await request(
    transaction.objectStore(ENTRIES).get(entryKey(base, committed.entry)),
  );
  if (entry === undefined) {
    throw new Error("committed exact-cache entry is incomplete");
  }
  const product = bytes(entry.product);
  const archive = bytes(entry.archive);
  const provenance = bytes(entry.provenance);
  const marker = bytes(committed.marker);
  const verified = verifyExactCacheCommit(identity, source, marker, product, archive, provenance);
  if (verified !== committed.entry) {
    throw new Error("exact-cache entry identifier mismatch");
  }
  return {
    entryId: verified,
    product,
    archive,
    provenance,
    marker,
  };
}

async function openDatabase(name) {
  if (typeof globalThis.indexedDB === "undefined") {
    throw new Error("BrowserExactProductCache requires IndexedDB");
  }
  const open = globalThis.indexedDB.open(name, 4);
  open.onupgradeneeded = () => {
    if (!open.result.objectStoreNames.contains(MARKERS)) {
      open.result.createObjectStore(MARKERS);
    }
    if (!open.result.objectStoreNames.contains(ENTRIES)) {
      open.result.createObjectStore(ENTRIES);
    }
    if (!open.result.objectStoreNames.contains(IN_FLIGHT)) {
      open.result.createObjectStore(IN_FLIGHT);
    }
    if (!open.result.objectStoreNames.contains(HEARTBEATS)) {
      open.result.createObjectStore(HEARTBEATS);
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

export class ExactCacheSingleFlightOwner {
  constructor(cache, identity, source, base, ownerToken, heartbeatIntervalMs) {
    this.cache = cache;
    this.identity = identity;
    this.source = source;
    this.base = base;
    this.ownerToken = ownerToken;
    this.heartbeatIntervalMs = heartbeatIntervalMs;
    this.state = "active";
    this.heartbeatError = null;
    this.heartbeatTail = Promise.resolve();
    this.heartbeatTimer = undefined;
    this._scheduleHeartbeat();
  }

  _ownershipLost(cause) {
    return cause instanceof ExactCacheSingleFlightOwnershipLostError
      ? cause
      : new ExactCacheSingleFlightOwnershipLostError(undefined, { cause });
  }

  _ensureActive() {
    if (this.state !== "active") {
      throw new ExactCacheSingleFlightOwnershipLostError();
    }
    if (this.heartbeatError !== null) throw this.heartbeatError;
  }

  _stopHeartbeat() {
    if (this.heartbeatTimer !== undefined) {
      globalThis.clearTimeout(this.heartbeatTimer);
      this.heartbeatTimer = undefined;
    }
  }

  _scheduleHeartbeat() {
    if (this.state !== "active" || this.heartbeatError !== null) return;
    this.heartbeatTimer = globalThis.setTimeout(() => {
      this.heartbeatTimer = undefined;
      void this._queueHeartbeat().then(
        () => this._scheduleHeartbeat(),
        () => {},
      );
    }, timerDelay(this.heartbeatIntervalMs));
  }

  _queueHeartbeat() {
    try {
      this._ensureActive();
    } catch (error) {
      return Promise.reject(error);
    }
    const attempt = this.heartbeatTail.then(() =>
      this.cache._heartbeatSingleFlight(this.base, this.ownerToken),
    );
    this.heartbeatTail = attempt.catch((error) => {
      this.heartbeatError = this._ownershipLost(error);
      this._stopHeartbeat();
    });
    return attempt;
  }

  heartbeat() {
    return this._queueHeartbeat();
  }

  async publish(product, archive, provenance) {
    if (this.state !== "active") {
      throw new ExactCacheSingleFlightOwnershipLostError();
    }
    this.state = "publishing";
    this._stopHeartbeat();
    try {
      await this.heartbeatTail;
      if (this.heartbeatError !== null) throw this.heartbeatError;
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
      const published = await this.cache._publishSingleFlight(
        this.base,
        this.ownerToken,
        entry,
        product,
        archive,
        provenance,
        marker,
      );
      this.state = "published";
      return published;
    } catch (error) {
      await this.cache._abandonSingleFlight(this.base, this.ownerToken).catch(() => {});
      this.state = "abandoned";
      throw error;
    }
  }

  async abandon() {
    if (this.state !== "active") {
      throw new ExactCacheSingleFlightOwnershipLostError();
    }
    this.state = "abandoning";
    this._stopHeartbeat();
    await this.heartbeatTail;
    try {
      await this.cache._abandonSingleFlight(this.base, this.ownerToken);
    } finally {
      this.state = "abandoned";
    }
  }
}

/**
 * Browser exact-product cache using IndexedDB transactions for single-flight
 * coordination and durable entry publication. `withLock` retains its legacy
 * Web Locks behavior.
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

  async _read(identity, source, base) {
    const transaction = this.database.transaction([MARKERS, ENTRIES], "readonly");
    return runTransaction(transaction, () => readCommitted(transaction, identity, source, base));
  }

  async openSingleFlight(identity, source, options) {
    const startedMs = monotonicNow();
    const { heartbeatIntervalMs, wait } = singleFlightWait(options, startedMs);
    try {
      const base = baseKey(identity, source);
      const ownerToken = entryId();
      let takeoverRevision;
      while (true) {
        const result = await this._openSingleFlightTransaction(
          identity,
          source,
          base,
          ownerToken,
          takeoverRevision,
        );
        takeoverRevision = undefined;
        if (result.kind === "hit") return result;
        if (result.kind === "owner") {
          return {
            kind: "owner",
            owner: new ExactCacheSingleFlightOwner(
              this,
              identity,
              source,
              base,
              ownerToken,
              heartbeatIntervalMs,
            ),
          };
        }

        const decision = wait.observe(monotonicNow(), REVISION_ENCODER.encode(result.revision));
        if (decision.action === "wait") {
          await waitFor(decision.delayMs);
        } else if (decision.action === "takeover") {
          takeoverRevision = result.revision;
        } else {
          throw new ExactCacheSingleFlightTimeoutError();
        }
      }
    } finally {
      wait.free();
    }
  }

  _openSingleFlightTransaction(identity, source, base, ownerToken, takeoverRevision) {
    const transaction = this.database.transaction(
      [MARKERS, ENTRIES, IN_FLIGHT, HEARTBEATS],
      "readwrite",
      { durability: "strict" },
    );
    return runTransaction(transaction, async () => {
      const committed = await readCommitted(transaction, identity, source, base);
      if (committed !== null) return { kind: "hit", entry: committed };

      const inFlightStore = transaction.objectStore(IN_FLIGHT);
      const heartbeatStore = transaction.objectStore(HEARTBEATS);
      const inFlight = await request(inFlightStore.get(base));
      const heartbeat = await request(heartbeatStore.get(base));
      const revision = ownerRevision(inFlight, heartbeat);
      if (
        inFlight === undefined ||
        (takeoverRevision !== undefined && takeoverRevision === revision)
      ) {
        inFlightStore.put({ ownerToken }, base);
        heartbeatStore.put({ ownerToken, revision: 0 }, base);
        return { kind: "owner" };
      }
      return { kind: "waiter", revision };
    });
  }

  _heartbeatSingleFlight(base, ownerToken) {
    const transaction = this.database.transaction([IN_FLIGHT, HEARTBEATS], "readwrite", {
      durability: "strict",
    });
    return runTransaction(transaction, async () => {
      const inFlight = await request(transaction.objectStore(IN_FLIGHT).get(base));
      const heartbeatStore = transaction.objectStore(HEARTBEATS);
      const heartbeat = await request(heartbeatStore.get(base));
      if (
        inFlight?.ownerToken !== ownerToken ||
        heartbeat?.ownerToken !== ownerToken ||
        !Number.isSafeInteger(heartbeat.revision) ||
        heartbeat.revision < 0 ||
        heartbeat.revision === Number.MAX_SAFE_INTEGER
      ) {
        throw new ExactCacheSingleFlightOwnershipLostError();
      }
      heartbeatStore.put({ ownerToken, revision: heartbeat.revision + 1 }, base);
    });
  }

  _publishSingleFlight(base, ownerToken, entry, product, archive, provenance, marker) {
    const transaction = this.database.transaction(
      [MARKERS, ENTRIES, IN_FLIGHT, HEARTBEATS],
      "readwrite",
      { durability: "strict" },
    );
    return runTransaction(transaction, async () => {
      const inFlight = await request(transaction.objectStore(IN_FLIGHT).get(base));
      if (inFlight?.ownerToken !== ownerToken) {
        throw new ExactCacheSingleFlightOwnershipLostError();
      }
      transaction.objectStore(ENTRIES).add(
        {
          product: product.slice(),
          archive: archive.slice(),
          provenance: provenance.slice(),
        },
        entryKey(base, entry),
      );
      transaction.objectStore(MARKERS).put({ entry, marker: marker.slice() }, base);
      transaction.objectStore(IN_FLIGHT).delete(base);
      transaction.objectStore(HEARTBEATS).delete(base);
      return { entryId: entry, product, archive, provenance, marker };
    });
  }

  _abandonSingleFlight(base, ownerToken) {
    const transaction = this.database.transaction([IN_FLIGHT, HEARTBEATS], "readwrite", {
      durability: "strict",
    });
    return runTransaction(transaction, async () => {
      const inFlightStore = transaction.objectStore(IN_FLIGHT);
      const inFlight = await request(inFlightStore.get(base));
      if (inFlight?.ownerToken === ownerToken) {
        inFlightStore.delete(base);
        transaction.objectStore(HEARTBEATS).delete(base);
      }
    });
  }

  close() {
    this.database.close();
  }
}
