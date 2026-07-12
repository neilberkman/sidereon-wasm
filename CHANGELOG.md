# Changelog

## 0.26.0 - 2026-07-12

- Builds against `sidereon` and `sidereon-core` 0.26.0.
- Removes `updateOpts.innovationScreen` and the per-epoch `innovationScreen`
  result. The underlying sequential RTK screen was unsound and was removed from
  core 0.26.0; this is an intentional breaking JavaScript interface change.
- Inherits the core fix that keeps near-polar TEC coordinates finite.
