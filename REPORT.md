# Sidereon Wasm Parity Report

Audit: `/Users/neil/xuku/sidereon_docs/parity-audit-2026-07-08.md`

## #26/#27 ARAIM typed wasm surface

Closed for the wasm TypeScript declaration surface. `araim`, `araimFaultModes`, and
`araimLpv200Allocation` now expose first-class TS interfaces for geometry, rows,
receiver geodetic input, ISM/default and per-satellite models, allocation, fault
hypotheses, monitored fault modes, and result objects. Runtime behavior is
unchanged.

Proof:
- `direct araim returns protection levels from geometry`
- `direct araim returns unavailable for sparse GPS geometry`
- `direct araim reports a clear bad lineOfSight length`

## #8/#9/#13 RTK arc and wide-lane typed wasm surface

Closed for the existing wasm RTK arc exports. The generated declarations are
post-processed to type RINEX arc builders, sequential/static arc solve inputs,
wide-lane fixing, and ionosphere-free prep inputs/results. Runtime behavior is
unchanged.

Proof:
- `solveRtkArc reports one solution per epoch and carries the filter state`
- `solveRtkArc exposes preprocessing metadata and covariance`
- `solveStaticRtkArc returns one float and one fixed solution for the arc`
- `buildRinexRtkArc and solveStaticRinexRtkBaseline solve the real WTZR/WTZZ arc`
- `buildDualFrequencyRinexRtkArc and solveWideLaneFixedRinexRtkBaseline fix the real WTZR/WTZZ arc`
- `fixWideLaneRtkArc fixes wide-lane ambiguities over a dual-frequency arc`
- `prepareIonosphereFreeRtkArc prepares single-frequency RTK arc inputs`

## #16/#17 PPP auto-init typed wasm surface

Closed for the existing wasm PPP float/fixed and SPP auto-init exports. The
generated declarations now name PPP epochs, observations, float state, float and
fixed configs, ambiguity controls, auto-init options, residual rows, temporal
correlation, and scalar maps. Runtime behavior is unchanged.

Proof:
- `solvePppAutoInitFloat (SPP seed) matches the engine float reference`
- `solvePppAutoInitFloat honours an explicit initial guess`
- `solvePppAutoInitFixed (SPP seed) matches the engine fixed reference`
- `PPP float position matches the engine reference`
- `PPP fixed position and integer fix match the engine reference`

## #2/#4 RINEX OBS to SPP convenience

Partially closed. Added `sppInputsFromRinexObs` and `solveSppFromRinexObs` for
parsed RINEX OBS plus `BroadcastEphemeris`, matching the broadcast navigation
workflow used by the real RINEX fixture. The facade's generic source form also
permits SP3-backed assembly; a wasm `Sp3` source variant remains.

Proof:
- `RINEX OBS convenience assembles and solves SPP through broadcast NAV`

## #21 RAIM for existing SPP solution

Closed. Added `raimForSolution(solution, options)` over `SppSolution`, delegating
to `sidereon_core::quality::raim_for_solution` and returning the same typed
result envelope as `raim`.

Proof:
- `raimForSolution runs over a real SPP solution`
- Existing direct RAIM tests also cover the shared option/result typing.

## #24 Broadcast FDE peer

Closed. Added `BroadcastEphemeris.fde(request)`, sharing the existing FDE
marshaling and delegating to the same core FDE driver over the broadcast
ephemeris source.

Proof:
- `BroadcastEphemeris.fde excludes a faulty broadcast SPP observation`
- Existing SP3 FDE tests continue to pass.

## #90 Fusion/inertial surface completion

Partially closed. The committed subset is declaration parity for the existing
wasm GNSS/INS filter, RTS smoother, and state codec surface: config, IMU sample,
loose/tight update epochs, state, update result, time-sync status, and RTS epoch
interfaces. No runtime expansion was committed.

Proof:
- `fusion time-sync replay and state bytes match reference bits`
- `fusion robust loose recorded RTS smoothing matches reference bits`
- `fusion tight SP3 observation update matches reference bits`

Remaining:
- Broader Python-only typed model details not already represented by current
wasm runtime methods.

## #92 Signal analysis

Partially closed. The committed subset is declaration parity for the existing
DLL jitter and multipath-envelope object protocols. Runtime behavior is
unchanged.

Proof:
- `signal analysis closed forms match reference bits`
- `signal analysis rejects invalid domains`

Remaining:
- Additional Python signal/acquisition class-model breadth not present as wasm
runtime exports.

## #93 Terrain/geoid store

Partially closed. The committed subset is declaration parity for the existing
DTED terrain and mmap terrain-store point/options/result protocols. Runtime
behavior is unchanged.

Proof:
- `DTED heightBatch matches scalar ORTHOMETRIC terrain lookups`
- `mmap terrain store built from DTED fixtures matches DTED terrain`
- `DTED terrain wrapper delegates lookup and validation to core`

Remaining:
- Python data acquisition/cache helpers and broader data catalog/fetch workflows.

## Gates

All required gates passed with exit code 0:

- `npm run build`
- `npm test`
- `npm run typecheck`
- `npm run lint`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`

## Run 2

### #90 Fusion/inertial surface completion

Partially closed, unchanged in this run. The existing wasm runtime already
covers the committed GNSS/INS filter, time-sync replay, RTS smoother, state
codec, loose/tight update, stationary update, and non-holonomic update subset
from Run 1. No additional fusion runtime surface was committed in Run 2.

Proof retained:
- `fusion time-sync replay and state bytes match reference bits`
- `fusion robust loose recorded RTS smoothing matches reference bits`
- `fusion tight SP3 observation update matches reference bits`

Remaining:
- Python's broader constructor/class-model surface for inertial config and
  state objects is still not mirrored as first-class wasm runtime classes.

### #92 Signal analysis

Partially closed. Added the missing GPS C/A correlation helper runtime exports
that Python exposes over `sidereon_core::signal`: `autocorrelation`,
`crossCorrelation`, `correlationAt`, and `correlateAgainst`. The new exports use
typed array inputs/outputs in the generated declarations, so no postbuild
overlay replacement was required.

Proof:
- `signal code correlation helpers match circular core semantics`
- `signal correlate and acquire match the rust oracle bits`
- Existing `signal analysis closed forms match reference bits`

Remaining:
- Python option helper classes remain represented in wasm as JS options objects
  for the existing `replica`, `correlate`, and `acquire` calls.

### #93 Terrain/geoid store

Partially closed, unchanged in committed runtime surface. No new terrain export
was committed in Run 2. The Python `DtedTile.from_path` surface is core-backed,
but the wasm target cannot exercise that path because the core implementation
uses `std::fs::read`, which returns "operation not supported" in this build, and
core 0.24.0 does not expose a DTED tile byte parser. Adding it here would either
be untestable or require reimplementing parser logic in the binding, so it was
left out.

Proof retained:
- `DTED heightBatch matches scalar ORTHOMETRIC terrain lookups`
- `mmap terrain store built from DTED fixtures matches DTED terrain`
- `DTED terrain wrapper delegates lookup and validation to core`

Remaining:
- Single-tile DTED wrapper requires a core byte-constructor or another
  testable core-backed path for wasm.
- Python data-acquisition/network helpers remain intentionally out of scope.

### Run 2 Gates

All required gates passed with exit code 0:

- `npm run build`
- `npm test`
- `npm run typecheck`
- `npm run lint`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`
