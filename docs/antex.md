# ANTEX surface reference

The ANTEX bindings carry a parsed ANTEX 1.4 product between
JavaScript/TypeScript and `sidereon-core`: every record the format defines,
the phase-center lookups, and the writer. Every reading, lookup and writing
rule is the engine's; this module converts representations.

This document describes the settled 3.0 surface. It does not move the package
version pin. The writer's refusals are in `writers.md`.

## 1. Absent records stay absent

A record the file does not carry is `undefined` from a class getter
(`Antenna.daziDeg`, `zenithStartDeg`, `zenithEndDeg`, `zenithStepDeg`,
`sinexCode`, `validFrom`, `validUntil`) and `null` in a plain object
(`header.version`, `header.pcvType`, `AntexFrequency.rms`,
`AntexCalibration.antennasCalibrated`), never `0`. The writer writes no record
the source did not carry.

## 2. Header and blocks

| Getter or method | Meaning |
| --- | --- |
| `Antex.header` | `{ version, pcvType, comments, endOfHeader }`. `version` is `ANTEX VERSION / SYST` as `{ version, system }`; `pcvType` is `PCV TYPE / REFANT` as `{ pcvType: "absolute" \| "relative", referenceAntennaType, referenceAntennaSerial, referenceAntenna }`, where `referenceAntenna` is the antenna relative values refer to (`AOAD/M_T` when a relative file leaves it blank) and `null` for absolute values. |
| `Antex.outerComments` | Comments after `END OF HEADER` outside every block, each `{ blocksBefore, text }`. |
| `Antex.skippedRecords` | Records the reader stepped over: a corrupt PCV value, a line outside any record, a `# OF FREQUENCIES` count that disagrees with the sections, a header record after `END OF HEADER`, a block or section its own end record does not close. |
| `Antex.antennaBlocks()`, `blockCount` | Every block in file order. |
| `Antex.antennaIntervals(id)`, `antennaAt(id, epoch)` | Every validity block of one `TYPE / SERIAL NO` id; the one valid at an epoch. |
| `Antex.antenna(id)`, `antennaCount`, `antennaIds` | The latest block of each id. |
| `Antenna.leadingComments`, `comments` | Comments before `TYPE / SERIAL NO`, and after it. |
| `Antenna.calibrations` | Every `METH / BY / # / DATE` record, each `{ method, agency, antennasCalibrated, date }`. |
| `Antenna.hasFrequencyCount` | Whether the block carries `# OF FREQUENCIES`. |
| `Antenna.frequencies` | Frequency labels in file order; a label with several sections appears once per section. |
| `Antenna.frequencySections()` | Every section in file order: `{ frequency, pcoM, pcvSamples, rms }`, grid values as `{ grid, azimuthDeg, zenithDeg, valueM }`. |
| `Antenna.frequency(label)`, `pco(label)`, `pcv(label, zenithDeg, azimuthDeg?)` | One section and its lookups. Several sections with the label must be identical; differing ones throw `AMBIGUOUS_FREQUENCY`. |

Millimetre fields are converted to metres as `mm * 1e-3`, the arithmetic of
RTKLIB `readantex`.

## 3. Validity seconds

`VALID FROM` and `VALID UNTIL` are GPS-time instants, second `0..=59`. The
fraction of the second is exact: `AntexDateTime.fractionDigits` is the decimal
the file states after the point, leading zeros kept and no trailing zero, so
`59.9999999` is `second` 59 and `fractionDigits` `"9999999"`, and
`1.2345678E-9` is `"0000000012345678"`. `nanosecond` is the fraction in whole
nanoseconds, or `undefined` when it is not a whole number of them.

`new AntexDateTime(year, month, day, hour?, minute?, second?, fractionDigits?)`
takes the same digit string; a date outside the GPS calendar, a second of 60
included, is a `RangeError`, and anything but digits is a `TypeError`.
`validAt` compares with every digit.

## 4. Refusals

`loadAntex` throws an `AntexParseError` and the lookups an `AntexLookupError`,
each with an `AntexErrorDetail`: `INVALID_FIELD` (`antennaId`, `null` for a
header record, `record`, `field`, `value`), `REPEATED_RECORD`,
`DEGENERATE_GRID`, `MISSING_PCO`, `INVALID_INPUT`, `UNKNOWN_FREQUENCY`,
`AMBIGUOUS_FREQUENCY` (with `sections`), `EMPTY_PCV_GRID`,
`INVALID_DATE_TIME` and `UNWRITABLE`, each with the engine's `message`. A
non-finite zenith or azimuth passed to `pcv` is a `RangeError`.

## 5. TypeScript

The shapes are declared in the `typescript_custom_section` of `src/antex.rs`
and re-exported from `@neilberkman/sidereon/types`: `AntexHeader`,
`AntexVersion`, `AntexPcvType`, `AntexOuterComment`, `AntexCalibration`,
`AntexFrequency`, `AntexFrequencyRms`, `AntexPcvSample`, `AntexErrorDetail`
and `AntexWriteErrorDetail`.
