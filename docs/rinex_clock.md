# RINEX clock surface reference

The RINEX clock bindings carry a parsed clock product between
JavaScript/TypeScript and `sidereon-core`: every header and data record with
how it was read, the per-satellite clock-bias series, the edits the engine
defines, and the writer with its policy. Every reading, editing and writing
rule is the engine's; this module converts representations and hands back
what the engine holds.

This document describes the settled 3.0 surface. It does not move the package
version pin.

## 1. The text is the authority

A product read from text keeps every line: header records with their exact
label and payload, blank lines, `AR`, `AS`, `CR`, `DR` and `MS` records,
continuation lines, and in a lossy read the lines that do not read.
`toRinexString()` on an unedited product restates its input byte for byte,
line terminators included.

| Entry point | Result |
| --- | --- |
| `parseRinexClock(bytes)`, `loadRinexClock(bytes)` | A `RinexClock`; the first line that does not read throws a `RinexClockParseError`. |
| `parseRinexClockLossy(bytes)`, `loadRinexClockLossy(bytes)` | A `RinexClock` that keeps unread lines verbatim, each reported in `diagnostics`. |
| `RinexClock.fromClockPoints(timeScale, rows)` | A product built from `{ satellite, points }` rows, each point `{ epoch, biasS, additionalValues? }` with `epoch` read in `timeScale`. |
| `RinexClock.fromSeriesRows(rows)` | A GPST product from `{ satellite, gpsSeconds, biasS }` rows, the form `ClockSeries` exports. |

Records are read at the columns of the file's declared version (the 80-column
layout before 3.04, the 85-column layout from 3.04), then at the other
version's columns, then as whitespace-separated values. Record epochs keep
every digit the seconds field states.

## 2. Views

| Getter or method | Meaning |
| --- | --- |
| `version`, `layout` | Declared version and the column layout, `"v300"` or `"v304"`; `undefined` when the text declares none. |
| `satelliteSystem` | System code of `RINEX VERSION / TYPE`, or `undefined`. |
| `timeSystem` | The `TIME SYSTEM ID` label (`GPS`, `GLO`, `GAL`, `QZS`, `BDS`, `IRN`, `UTC`, `TAI`) declared, defaulted or built in. |
| `timeSystemStatus` | `DECLARED`, `DEFAULTED` (the 3.00 default applied), `UNRECOGNIZED` with the label, `CONFLICTING` with the labels, or `CONSTRUCTED`. |
| `timeScale` | The `TimeScale` epochs are read in, or `undefined` when the system is missing, unrecognised, conflicting or `IRN`. `GLO` is UTC. |
| `headerRecords()` | Every header line: `line` (null for a line written by an edit), `text`, `label`, `labelColumn`, `payload`, `reading`, and `field`, the typed reading or `null`. |
| `records()` | Every data record in order, with `index`, `recordType`, `name`, `satellite`, `civilEpoch`, `epoch`, `declaredCount`, `values`, `surplusValues`, `line`, `lineCount`, `reading`, `continuationReading` and `sourceLines`. |
| `recordCount`, `sourceLine(n)` | Number of records; one source line by one-based number. |
| `series`, `seriesFor(sat)`, `satellites` | Per-satellite `AS` samples whose epoch resolves to an instant; where records repeat one satellite and epoch, the last is the sample. |
| `skippedRecords` | `{ line, recordType }` for each record outside the satellite series. The record itself is in `records()`. |
| `diagnostics` | `{ line, error }` for each line a lossy read kept without reading, and for header time-system errors. |
| `notices` | Findings that do not stop the product being read: a defaulted or missing time system, a system without a scale, nonconforming, uninterpreted or unknown header records, surplus values, records read at the other layout's columns or as whitespace. |

A record's `values` are its declared values, bias first. A value present
beyond the declared count, such as the bias sigma a one-value `AS` record
carries in some products, is kept in `surplusValues` with its position in the
value sequence (0 bias, 1 bias sigma, 2 rate, ...). `civilEpoch.second` is the
nearest double to the stated second; `sourceLines` holds the record's lines as
read, the exact seconds text included, and is empty for a record built or
edited through the API.

An instant crosses as `RinexClockInstant`: `scale` (`"GPST"`, `"UTC"`, ...),
the split Julian date `jdWhole` + `jdFraction`, or `nanos` as an exact decimal
string for an instant held as a nanosecond count, and `gpsSeconds`, `null` off
the GPS timeline. GPST and QZSST project onto it.

A header `i64` (`LEAP SECONDS`, `SOLN STA NAME / NUM` coordinates) crosses as an
exact decimal string beside a `number` that is `null` where the integer is not
exactly representable as one.

## 3. Series

`ClockSeries` keeps samples in every time scale. Its typed arrays are
index-aligned: `biasS`, `gpsSeconds` (holding `NaN` where the sample's scale
does not project onto GPS time, with `hasGpsSeconds` 0 there), `jdWhole` and
`jdFraction` (`NaN` for a nanosecond-count instant, which `epochs` states).
`epochs` holds each sample's `RinexClockInstant`, `additionalValues` the
declared values after the bias, and `timeScale` the samples' scale.

## 4. Epochs and queries

`ClockEpoch` is a civil label read in the scale of the product it is used with.
The constructor refuses fields that name no civil epoch in any scale; a
`23:59:60` label on a day that ends with a positive leap second is accepted, and
its `gpsSeconds` is `undefined`. The second is read as the shortest decimal of
the number given, with every digit kept.

`clockS(sat, epoch)` interpolates in the product's scale: on a UTC product a
leap-second label is a valid query, and interpolation across a leap second uses
elapsed time. `clockSAtGpsSeconds(sat, s)` answers GPST and QZSST series. A
product whose time system has no scale, or an epoch its scale does not have,
throws a `RinexClockQueryError`.

## 5. Edits

| Method | Change |
| --- | --- |
| `setTimeSystem(label)` | Replaces every `TIME SYSTEM ID` record with one at the layout's columns, or inserts one where Table A15 orders it. |
| `setRecordValues(index, values)` | Replaces a record's declared values; the record keeps its type, name and exact seconds text. |
| `insertRecord(index, record)` | Inserts `{ recordType, name, epoch, values }` before `index`, or after the last record when `index` is `recordCount`. |
| `removeRecord(index)` | Removes a record and every line it spans, returning it. |
| `retainRecords(keep)` | Keeps the records `keep` returns a truthy value for; returns the number removed. |
| `editRecords(edit)` | Replaces the values of every record for which `edit` returns an array; returns the number edited. |

Each edit is validated whole before anything changes: a value no 19-column
field states exactly, a name or year the layout cannot hold, an epoch the
product's scale does not have, or new values that would drop a record's
surplus values throws a `RinexClockEditError` and leaves the product as it
was. A callback that throws leaves it unchanged too, and its exception is
rethrown. An unknown time system label or record type is a `TypeError`.

## 6. Writing

`toRinexString()` writes under the strict policy. Records built or edited
through the API are written in the product's layout; a product built from rows
is written with a header stating its version, satellite system, time system
and data types. A value no 19-column field states exactly, or an epoch no
microsecond seconds text states exactly, is refused rather than rounded, and a
scale no RINEX clock time system names (GLONASS system time among them) is
`UNSUPPORTED_TIME_SCALE`.

`toRinexStringWithPolicy(policy?)` returns `{ text, value, departures }`.
`policy` is `"strict"`, `"lenient"`, or `{ nearestMicrosecondEpochs:
"strict" | "allow" }`. With `nearestMicrosecondEpochs` allowed, an epoch off the
microsecond grid is written at the nearest microsecond and reported as
`EPOCH_AT_NEAREST_MICROSECOND` with the record index, name, epoch and the
epoch text written. Values are never approximated under any policy.

## 7. Refusals

A refusal is an `Error` named for the operation, `RinexClockParseError`,
`RinexClockQueryError`, `RinexClockEditError`, `RinexClockWriteError` or
`RinexClockBuildError`, whose `detail` is a `RinexClockErrorDetail`:
`MALFORMED_AS_RECORD`, `MISSING_CONTINUATION`, `MALFORMED_CONTINUATION` and
`BAD_FIELD` carry the one-based `line`; `INVALID_INPUT` carries `field` and
`reason`; `UNSUPPORTED_TIME_SCALE` carries `scale`. Each carries the engine's
`message`.

## 8. TypeScript

The shapes are declared in the `typescript_custom_section` of
`src/rinex_clock.rs` and re-exported from `@neilberkman/sidereon/types`:
`RinexClockCivilEpoch`, `RinexClockInstant`, `RinexClockTimeSystemStatus`,
`RinexClockHeaderField`, `RinexClockHeaderRecord`, `RinexClockRecordReading`,
`RinexClockRecord`, `RinexClockSkip`, `RinexClockErrorDetail`,
`RinexClockDiagnostic`, `RinexClockNotice`, `RinexClockWriteDeparture`,
`RinexClockLeniency`, `RinexClockWritePolicyInput`, `RinexClockWritePolicyLike`,
`RinexClockWriteResult`, `RinexClockRecordInput`, `RinexClockPointInput`,
`RinexClockPointRowInput` and `RinexClockSeriesRowInput`.

## 9. Not bound here

`civil_to_clock_instant` and `clock_s_at_instant`: a query at a civil epoch
goes through `ClockEpoch` and `clockS`, which read the epoch in the product's
scale.
