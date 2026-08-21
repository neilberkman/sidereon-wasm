import type { GnssProductIdentity } from "../pkg/sidereon.js";

export interface ExactCacheEntry {
  entryId: string;
  product: Uint8Array;
  archive: Uint8Array;
  provenance: Uint8Array;
  marker: Uint8Array;
}

export interface LockedExactCache {
  read(): Promise<ExactCacheEntry | null>;
  publish(
    product: Uint8Array,
    archive: Uint8Array,
    provenance: Uint8Array,
  ): Promise<ExactCacheEntry>;
  cleanupAbandoned(): Promise<void>;
}

export interface ExactCacheSingleFlightOptions {
  pollIntervalMs?: number;
  heartbeatIntervalMs?: number;
  livenessTimeoutMs?: number;
  waitTimeoutMs?: number;
}

export class ExactCacheSingleFlightOptionsError extends TypeError {}

export class ExactCacheSingleFlightTimeoutError extends Error {}

export class ExactCacheSingleFlightOwnershipLostError extends Error {}

export class ExactCacheSingleFlightOwner {
  private constructor();
  heartbeat(): Promise<void>;
  publish(
    product: Uint8Array,
    archive: Uint8Array,
    provenance: Uint8Array,
  ): Promise<ExactCacheEntry>;
  abandon(): Promise<void>;
}

export type ExactCacheSingleFlightOpen =
  { kind: "hit"; entry: ExactCacheEntry } | { kind: "owner"; owner: ExactCacheSingleFlightOwner };

export class BrowserExactProductCache {
  static open(options?: { name?: string }): Promise<BrowserExactProductCache>;
  withLock<T>(
    identity: GnssProductIdentity,
    source: string,
    operation: (cache: LockedExactCache) => Promise<T> | T,
    options?: { timeoutMs?: number },
  ): Promise<T>;
  read(identity: GnssProductIdentity, source: string): Promise<ExactCacheEntry | null>;
  openSingleFlight(
    identity: GnssProductIdentity,
    source: string,
    options?: ExactCacheSingleFlightOptions,
  ): Promise<ExactCacheSingleFlightOpen>;
  close(): void;
}
