# Changelog

## Unreleased

## 0.34.0 - 2026-07-21

- Adds `supportedSamples`, returning the core's complete date- and issue-aware
  cadence set as a JavaScript array. `productIdentity` enforces that same set,
  including the GFZ ultra-rapid overlap and ESA ultra-rapid issue transition.
- Adds the `Sp3ContentStartConvention` enum,
  `sp3ContentStartConvention`, and `sp3ContentStartOffsetSeconds` with strict
  issue validation. Historical GFZ ultra-rapid identity-derived exact requests
  now inherit the cataloged one-day content-start offset, including across a
  GPS week boundary.
- Exact SP3 parsing now inherits the core's complete-record terminal validation:
  standards-compatible ASCII-space padding and LF/CRLF endings are accepted,
  while malformed, missing, premature, or followed-by-data `EOF` records still
  fail closed. The shared cross-interface corpus exercises all accepted and
  rejected forms through `parseExactSp3`; numerical behavior is unchanged.
- Caller-built exact identities now reject a span that is syntactically valid
  but not cataloged for that product family. This is an integrity-policy change
  only; JavaScript/TypeScript APIs and numerical calculations are unchanged.
- Builds against `sidereon` and `sidereon-core` 0.34.0.

## 0.33.1 - 2026-07-20

- Node ESM and CommonJS now both select the synchronous Node build; browser and
  bundler imports continue to select the fetch-initialized web build. Release
  checks install the packed tarball and compile and execute clean consumers for
  all three resolution paths.
- The legacy `@neilberkman/sidereon/types` subpath now has a harmless runtime
  target while preserving its type-only declaration target.
- The npm package now includes `THIRD-PARTY-NOTICES.md`, complete Apache-2.0,
  ISC, ERFA, SciPy, and IERS license texts, and the exact public
  `sidereon-core` 0.33.1 non-test tide sources required for the distributed
  IERS-derived routines.
- Adds product-aware solution classification and date-aware default sampling,
  including the historical GFZ rapid and ultra-rapid cadence changes and the
  issue-sensitive ESA ultra-rapid transition.
- Preserves both official IGS final-SP3 naming eras and exposes legacy CDDIS
  Unix-compress (`.Z`) packaging without substituting another product.
- Rejects SP3/clock dates before each evidenced family start, including the
  CODE ultra long-name boundary, and rejects unmodeled pre-week-2238 CDDIS
  long-name SP3/IONEX locations. ESA `ESA0MGNFIN` final SP3 remains direct-only
  instead of being substituted at CDDIS.
- Adds `ExactSp3Request`, `parseExactSp3`, and `validateExactSp3`, accepting
  both official half-open and inclusive grids while rejecting malformed,
  irregular, cadence-mismatched, span-mismatched, and identity-invalid bytes.
- Exposes the independently declared SP3 epoch count and start epoch.
- Adds `unix_compress` to merged-SP3 provenance without changing the existing
  `none` and `gzip` spellings.
- Builds against `sidereon` and `sidereon-core` 0.33.1 and
  `trust-region-least-squares` 0.9.2.

## 0.32.0 - 2026-07-18

- Adds `parseNavcenAt` and `mergeNavcenAt` for deterministic NAVCEN usability
  decisions at explicit UTC Unix microseconds supplied as JavaScript `bigint`.
  Assessments preserve NANU type,
  subject, raw Outage Start text, evaluation time, and parsed/unparseable/not-
  applicable interval provenance.
- Keeps `parseNavcen` and `mergeNavcen` unchanged for compatibility. The new
  path applies active forecasts only on their validated half-open intervals and
  additionally recognizes active `UNUSUFN` notices as immediately unusable.
- Builds against `sidereon` and `sidereon-core` 0.32.0.

## 0.31.2 - 2026-07-16

- Returns canonical contributors and ordered precedence contributors alongside
  the merged-SP3 stable ID.
- Rejects artifact byte lengths that are not positive exact JavaScript safe
  integers and enforces whole-second target epoch intervals.
- Adds the shared literal provenance fixture and builds against `sidereon` and
  `sidereon-core` 0.31.2.

## 0.31.0 - 2026-07-16

- Adds `sp3MergeInputIdentity`, which validates complete exact SP3 artifact
  records plus the full merge policy and returns the shared versioned stable
  identity. Incomplete, malformed, mismatched, duplicate, non-SP3, and unknown
  fields fail closed.
- Builds against `sidereon` and `sidereon-core` 0.31.0.

## 0.30.0 - 2026-07-16

- Exposes analysis center, parsed format version, and the canonical all-field
  cache key on exact product identities.
- Adds the shared schema-v3 commit builder and verifier, binding the full
  identity, explicit source, and all immutable byte objects.
- Adds `@neilberkman/sidereon/exact-cache`, using Web Locks for bounded
  same-origin tab/worker coordination and one strict-durability IndexedDB
  transaction for atomic immutable-entry publication.
- Builds against `sidereon` and `sidereon-core` 0.30.0.

## 0.29.2 - 2026-07-16

- Adds `GnssExactProductSet`, a fail-closed gate for a declared exact identity
  inventory. Empty declarations, duplicates, missing products, and undeclared
  products are rejected.
- Preserves prediction-tier identity during exact-set comparison. SP3
  observed/predicted timing remains available from the parser's authoritative
  record-flag summary.
- Builds against `sidereon` and `sidereon-core` 0.29.2.

## 0.29.1 - 2026-07-15

- Derives CODE predicted IONEX P1 and P2 direct locations from their current
  official tier-specific HTTPS directories, including identity-year rollover.
- Keeps same-filename P1 and P2 exact product cache keys distinct.
- Builds against `sidereon` and `sidereon-core` 0.29.1.

## 0.29.0 - 2026-07-15

- Adds pure exact GNSS product identity and explicit distribution-location
  derivation for direct archives, NASA CDDIS/Earthdata, local files, and
  in-memory input. The WASM package performs no hidden network or credential IO.
- Builds against `sidereon` and `sidereon-core` 0.29.0.

## 0.28.1 - 2026-07-15

- Builds against `sidereon` and `sidereon-core` 0.28.1, inheriting the repaired
  official HTTPS source for CODE ultra-rapid products and the symmetric RTK
  candidate-selection fixes.

## 0.28.0 - 2026-07-13

- Adds per-cell SP3 precedence, optional deterministic outlier rejection,
  clock-outlier report access, and observed/predicted epoch summaries.
- Builds against `sidereon` and `sidereon-core` 0.28.0.

## 0.27.1 - 2026-07-13

- Builds against `sidereon` and `sidereon-core` 0.27.1.
- Rejects finite LAMBDA ambiguity inputs outside the signed 64-bit integer
  search domain with a `RangeError`, instead of returning saturated integers
  and non-finite scores.

## 0.27.0 - 2026-07-12

- Builds against `sidereon` and `sidereon-core` 0.27.0.
- Adds `GeoidGrid.fromProjEgm96Gtx` for PROJ's public EGM96 15-arcminute GTX
  grid.
- Adds `GeoidGrid.undulationProjRad` with explicit fused-versus-separately
  rounded arithmetic and typed `RangeError` coordinate failures. Existing geoid
  lookup functions retain their previous bits.

## 0.26.1 - 2026-07-12

- Builds against `sidereon` and `sidereon-core` 0.26.1.
- Fixes a process/VM denial of service when parsing malicious RINEX 2
  observation input with an oversized declared epoch satellite count. npm
  releases 0.11.1 through 0.26.0 are affected; upgrade to 0.26.1 or later.

## 0.26.0 - 2026-07-12

- Builds against `sidereon` and `sidereon-core` 0.26.0.
- Removes `updateOpts.innovationScreen` and the per-epoch `innovationScreen`
  result. The underlying sequential RTK screen was unsound and was removed from
  core 0.26.0; this is an intentional breaking JavaScript interface change.
- Inherits the core fix that keeps near-polar TEC coordinates finite.
