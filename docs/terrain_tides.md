# Terrain and BLQ surface reference

The terrain bindings answer orthometric heights from DTED tiles and terrain
stores; the BLQ bindings read and write ocean-loading coefficient blocks. Every
rule is the engine's; this module converts representations.

This document describes the settled 3.0 surface. It does not move the package
version pin.

## 1. Unknown elevations

A DTED posting holding the null value (all bits set, MIL-PRF-89020B 3.11.3.1)
is an unknown elevation, and a terrain store keeps it as `-32767`. A lookup
that gives such a posting nonzero weight has no height: it throws an `Error`
named `TerrainLookupError` whose `detail` is a `TerrainLookupErrorDetail`,

```ts
{ kind: "UNKNOWN_TERRAIN_ELEVATION", latIndex, lonIndex, latitudePosting, longitudePosting, message }
```

with the tile id and the zero-based posting indices. A query exactly on a
known posting next to a null returns that posting. When the posting a nearest
lookup selects is a null on a tile edge, a neighbouring tile's posting at the
same coordinates answers, and a bilinear query exactly on a shared edge is
answered by the neighbouring tile.

The other kinds are `NON_WGS84_TERRAIN_TILE` (`datum`, a
`DtedHorizontalDatum`: `WGS84`, `WGS72`, `UNSTATED` or `OTHER` with its
`text`), `MISSING_TERRAIN_TILE` (a point no stored tile covers),
`INVALID_INPUT`, `PARSE` and `UNKNOWN`. `DtedTerrain` reads a missing tile as
sea level, `0.0`; `MmapTerrain` refuses it.

`heightBatch` entries are `{ ok: true, heightM, error: null, detail: null }`
or `{ ok: false, heightM: null, error, detail }`, with the same `detail`.
`orthometricHeightBatch` entries have the same shape with
`orthometricHeightM: { valueM }` in place of `heightM`. An
ellipsoidal lookup whose terrain step fails throws the `Terrain` datum error
with the refusal in `detail.terrain`.

## 2. Terrain stores

`dtedTreeToMmapStore` refuses a tile whose DSI states a horizontal datum other
than WGS84 with `NonWgs84Tile` (`detail.path`, `detail.datum`), since the store
records no datum. The store parser refuses an index record whose tile id lies
outside the coordinate domain (latitude ids `-90..=89`, longitude ids
`-180..=179`) as `TileIdOutOfRange`, and one whose bounds are not the edges of
the one-degree cell its id names as `TileBoundsMismatch` (`detail.field`
names the bound). Both carry `latIndex` and `lonIndex`.

## 3. BLQ blocks

| Entry point | Result |
| --- | --- |
| `parseOceanLoadingBlqBlock(text)` | One `OceanLoadingBlqBlock`; text holding more than one block is `MULTIPLE_BLOCKS`. |
| `parseOceanLoadingBlqBlocks(text)` | Every block in file order. |
| `writeOceanLoadingBlqBlock(block)` | The block as BLQ text. |
| `writeOceanLoadingBlqBlocks(blocks)` | Several blocks as one file. |

A block is `{ station, amplitudeM, phaseDeg, comments }`. The grids are 3 x 11:
rows radial, EW (west positive), NS (south positive); columns M2 S2 N2 K2 K1 O1
P1 Q1 Mf Mm Ssa, whatever order the file declared. `comments` holds every
comment and column-order header line exactly as read, each with its
`placement`: `"beforeStation"`, `"beforeRow"` with the zero-based `row` it
precedes, or `"afterRows"`; `row` is `null` for the other two.

A column-order header is a `COLUMN ORDER` declaration, a line of constituent
labels only, or a comment in which a word `ORDER` is followed to the end of the
line by labels. It sets the order of every later row, across blocks. A header
naming a label that is not one of the eleven, repeating one, or holding another
count is refused.

The writer restates the comments at their placements, writes the station line
from the third column and each row in the order its retained header declares,
so the output reads back to equal blocks. What would not read back unchanged is
refused with a `BlqWriteError` whose `detail` is a `BlqWriteErrorDetail` with
the zero-based `block` index: a station that is empty, holds a line break or
surrounding whitespace, or would read as a comment, header or row; a non-finite
coefficient (`row`, `constituent`); a comment with a line break, one that would
not read as a comment, one placed past the sixth row, a header the reader
refuses (`header`, a `BlqParseReason`), comments not in placement order, and a
comment after the rows of a block that is not the last. A malformed block object
is a `TypeError`.

The reader's refusals are `BlqParseError`s whose `detail` is a
`BlqParseErrorDetail`: the reason, its one-based `line` (0 for a whole-input
failure) and the engine's `message`.

## 4. TypeScript

The shapes are declared in the `typescript_custom_section` blocks of
`src/terrain.rs` and `src/tides.rs` and re-exported from
`@neilberkman/sidereon/types`: `DtedHorizontalDatum`,
`TerrainLookupErrorDetail`, `TerrainHeightBatchEntry`, `OceanLoadingBlqBlock`,
`OceanLoadingBlqBlockInput`, `OceanLoadingBlqComment`, `BlqParseReason`,
`BlqParseErrorDetail` and `BlqWriteErrorDetail`.
