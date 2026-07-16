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

export class BrowserExactProductCache {
  static open(options?: { name?: string }): Promise<BrowserExactProductCache>;
  withLock<T>(
    identity: GnssProductIdentity,
    source: string,
    operation: (cache: LockedExactCache) => Promise<T> | T,
    options?: { timeoutMs?: number },
  ): Promise<T>;
  read(identity: GnssProductIdentity, source: string): Promise<ExactCacheEntry | null>;
  close(): void;
}
