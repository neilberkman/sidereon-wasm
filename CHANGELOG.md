# Changelog

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
