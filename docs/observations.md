# RINEX observation surface reference

The RINEX observation bindings carry a parsed RINEX 2, 3 or 4 observation
product, the header in effect at each epoch, the strict writer and the version
2 downgrade between JavaScript/TypeScript and `sidereon-core`. Every reading,
writing and downgrade rule is the engine's; this module converts
representations and hands back what the engine holds.

This document describes the settled 3.0 surface. It does not move the package
version pin.

## 1. Absence conventions

- A scalar getter on a class (`markerName`, `intervalS`, `ObsPhaseShift.code`,
  `ObsEpoch.rcvClockOffsetS`, `ObsLeapSeconds.timeSystem`) returns `undefined`
  where the product holds no value. This is the convention the observation
  classes already used.
- A structured value (a plain object or array of plain objects) holds `null`
  for an absent field: `ObsValue.value`, `ObsGlonassBias.biasM`,
  `ObsPrnObsCount.counts[i]`, and a getter such as `receiver` or
  `glonassCodPhsBis` that returns `null` where the header carries no record.
- A numeric typed array holds `NaN` where a field is blank. The reader refuses
  a non-finite observation value, so `NaN` in `values`, `valueCycles`, `lli` or
  `ssi` always means a blank field. Where `NaN` alone would not say why a value
  is missing, a status and a validity array stand beside it (section 4).
- Nothing absent is reported as `0`.

## 2. Epochs

`RinexObs.epochs` and `RinexObs.epoch(i)` return every record in file order,
events and cycle slip records included, so indices are stable.

| Getter | Meaning |
| --- | --- |
| `epoch` | Civil epoch in the file time scale, or `undefined` for an event whose epoch fields are blank. RINEX 2.11 and 3.05 let an event without a significant epoch leave them blank. |
| `flag` | `0` observations, `1` power failure, `6` cycle slips (`rinexObsCycleSlipFlag()`), any other flag above 1 an event. |
| `rcvClockOffsetS`, `epochPicoseconds` | Optional epoch-line fields. |
| `declaredRecordCount` | The count the epoch line declared. |
| `specialRecords` | The records an event carried, verbatim, in order. |
| `satellites`, `satelliteCount`, `observations` | Satellites with observations and every field, as `ObsSatelliteValues[]`. |
| `cycleSlipSatellites`, `cycleSlips` | The slips a flag 6 epoch reports, as `ObsSatelliteValues[]`. |

`observations` and `cycleSlips` are index-aligned to `header.obsCodes(system)`
for each satellite's constellation: the union of every list the file
declares. A field under a code the list in effect at that epoch does not
declare is `{ value: null, lli: null, ssi: null }`. Cycle slips are held apart
from observations; `satellites` and `observations` are empty for a slip epoch,
so no measurement consumer takes a slip for a measurement.

## 3. Headers

`RinexObs.header` is the file header. `RinexObs.headerAt(i)` is the header in
effect at epoch `i`: the file header with the header records of every event at
or before `i` laid over it. Its `obsCodes` is the product's union and its
`declaredObsCodes(system)` the list in effect at the epoch. `headerAt` refuses
an index past the last epoch with a `RangeError`.

`RinexObs.headerTimeline()` builds every header once. `at(i)` looks one up
(past the last epoch it returns the header after every event), `segmentIndex(i)`
gives its position, and `segments` lists each header with the first epoch it is
in effect at, starting with the file header at 0. An event at epoch 0 gives a
second segment at 0; segments are neither merged nor reordered.

Header getters added in 3.0: `declaredObsCodes(system)` (`undefined` where the
header declares none), `rinex2Types`, `rinex2System`, `programRunByDate`,
`comments`, `markerNumber`, `markerType`, `observer`, `agency`, `receiver`,
`antenna`, `timeOfLastObsEpoch`, `timeOfLastObsScale`,
`declaredSatelliteCount`, `prnObsCounts`, `scaleFactors`, `glonassCodPhsBis`,
`glonassCodePhaseBias(code)`, `signalStrengthUnit`, `leapSeconds` and
`unretainedHeaderLabels`. `RinexObs.skippedRecords` counts records the reader
skipped or kept as contradictory.

### Phase-shift records

`ObsPhaseShift.code` is `undefined` for a record naming only its
constellation, which RINEX 3.05 section 5.2.12 uses to declare the alignment
unknown. `correctionCycles` is `undefined` for a blank correction.
`unrepresentableSatellites` lists designators such as `R28` that no satellite
id holds, kept so the record is written back whole; `coversEverySatellite` and
`satelliteCount` count them.

### GLONASS code-phase biases

`glonassCodPhsBis` is the record as written: `null` for no record, `[]` for a
blank record (the biases are unknown), otherwise `{ code, biasM }` entries with
`biasM` `null` for a blank bias. `glonassCodePhaseBias(code)` returns
`{ status: "available", biasM }`, `{ status: "none" }` where the header gives
the signal no bias (including every RINEX 4 header, which ignores the record),
`{ status: "unknown" }` for a blank record or bias, or
`{ status: "ambiguous", biasesM }` where one block gives the signal different
biases.

### Leap seconds

`leapSeconds` is an `ObsLeapSeconds` whose integer fields cross as `bigint`
(`current`, and `deltaFuture`, `week` and `day` or `undefined`), so every value
arrives exactly. `timeSystem` is the identifier as written (`GPS`, `BDS` or
`BDT`), or `undefined` where the field is blank; a blank field and an explicit
`GPS` stay distinct.

## 4. Carrier-phase rows

`carrierPhaseRows(i, filter?)` reads epoch `i` with the header in effect at it,
so a phase shift or GLONASS channel an earlier event declared applies. Every
carrier-phase row is kept, whatever its correction status.

| Array | Meaning |
| --- | --- |
| `phaseShiftCycles` | The `SYS / PHASE SHIFT` correction where the header gives one, `NaN` where it does not. |
| `phaseShiftAvailable` | `Uint8Array`, 1 where `phaseShiftCycles` holds a correction and 0 where it is `NaN`. |
| `phaseShiftStatus` | `available`, `unknown` or `ambiguous` per row. |
| `phaseShiftCorrections` | `CarrierPhaseShift` per row: `{ status: "available", cycles }`, `{ status: "unknown" }`, or `{ status: "ambiguous", corrections }` with every conflicting value in record order and `null` for a blank one. |

The statuses are the engine's: no record, or a blank correction, is an
available 0; a record naming a satellite applies over the record for every
satellite of its code; a record naming only the constellation makes a signal no
other record covers `unknown`; records in one block that give a signal
different corrections make it `ambiguous` (a blank beside 0 is one value, a
blank beside 0.5 two). From RINEX 4.00 the records are kept and written back
and every row is an available 0, as RINEX 4.00 Table A2 tells decoders to
ignore them.

## 5. Epoch indices

Every method taking an epoch index (`epoch`, `headerAt`, `observationValues`,
`carrierPhaseRows`, `pseudoranges`, `ObsHeaderTimeline.at`,
`ObsHeaderTimeline.segmentIndex`) reads the index exactly. A negative,
fractional, non-finite or out-of-range value is a `RangeError`, where the
default `number` to `usize` conversion would have wrapped it to another index.

## 6. Writing

`toRinexString()` writes version 2 records below 3.0 and version 3 records
otherwise, and returns text only when reading it back gives the product.
Otherwise it throws an `Error` named `RinexObsWriteError` whose `detail` is a
`RinexObsWriteErrorDetail`, a union on `kind` carrying the engine's payload
and `message`:

`CODE_LISTS_NOT_VERSION_TWO`, `NOT_VERSION_TWO`,
`SCALE_FACTORS_IN_VERSION_TWO`, `VALUES_WITHOUT_CODES`, `COUNTS_WITHOUT_CODES`,
`CODE_LIST_NOT_STATED`, `EPOCH_FLAG_TOO_WIDE`, `EPOCH_TIME_MISSING`,
`EPOCH_PICOSECONDS_NOT_IN_VERSION`, `TOO_MANY_OBSERVATION_TYPES`,
`CODE_LISTS_NOT_UNION`, `VALUE_OUTSIDE_DECLARED_LIST`,
`DECLARED_LIST_NOT_STATED`, `EVENT_RECORDS_UNREADABLE`,
`OBSERVABLE_NOT_REPRESENTABLE`, `LEAP_SECONDS_TIME_SYSTEM_NOT_IN_VERSION`,
`INVALID_LEAP_SECONDS_TIME_SYSTEM` and `READ_BACK_MISMATCH`.

A constellation in a payload is its RINEX letter (`"C"`) and a satellite its
token (`"C01"`). The engine's `EventRecordsUnreadable.message` crosses as
`readerError`, beside the error's own `message`. A version 2 product holding
`SYS / SCALE FACTOR` records is refused with `SCALE_FACTORS_IN_VERSION_TWO`:
version 2 readers that do not know the record would read the scaled numbers as
physical ones.

## 7. Downgrade to version 2

`downgradeToRinex2(version)` returns `{ obs, value, changes }`: a new product
(`value` is the same instance) and every `ObsDowngradeChange`, in order. The
source product is not modified. The change kinds are `CODE_RENAMED`,
`CODE_MOVED`, `CODE_ADDED`, `CODE_LIST_REMOVED`, `VALUE_ROUNDED`,
`CYCLE_SLIP_ROUNDED`, `SCALE_FACTORS_REMOVED`, `EPOCH_PICOSECONDS_REMOVED`,
`CLOCK_OFFSET_ROUNDED`, `IN_EVENT_LISTS` (whose `change` is itself an
`ObsDowngradeChange` for the lists an event declares),
`DEPRECATED_RECORDS_REMOVED` (`epochIndex` `null` for the file header) and
`EVENT_RECORDS_REWRITTEN`.

What version 2 cannot state is refused with a `RinexObsWriteError` rather than
changed: a `version` outside [2.0, 3.0) (`NOT_VERSION_TWO`), a code on a
carrier version 2 has no name for, such as BeiDou B1C
(`OBSERVABLE_NOT_REPRESENTABLE`), and a `LEAP SECONDS` time system the version
does not support (`LEAP_SECONDS_TIME_SYSTEM_NOT_IN_VERSION`).

## 8. Repair and quality control

`repairRinexObs` returns a `RinexObsRepair` whether or not its product can be
written. `repaired`, `actions`, `remaining` and `decodedFromCrinex` are always
available. `repairedText` writes the product when first read and throws the
same typed `RinexObsWriteError` `toRinexString` throws; `toCrinexString`
composes that writer with the CRINEX encoder, so a writer refusal keeps its
typed detail and an encoder refusal is an `Error` with the encoder's message.

`ObservationQcReport.notes` gains the kind `eventHeaderRecordsUnread`, for a
product whose event header records do not read. A product read from text
always reads, so only a product built or changed in memory reaches it. The
notes keep this module's existing `undefined` for an absent `epochIndex`.

## 9. TypeScript

The plain-object shapes are declared in the `typescript_custom_section` of
`src/rinex_obs.rs` and re-exported from `@neilberkman/sidereon/types`:
`ObsValue`, `ObsSatelliteValues`, `ObsProgramRunByDate`, `ObsReceiver`,
`ObsAntenna`, `ObsScaleFactor`, `ObsPrnObsCount`, `ObsGlonassBias`,
`GlonassCodePhaseBias`, `CorrectionStatus`, `CarrierPhaseShift`,
`RinexObsWriteErrorDetail`, `ObsDowngradeChange` and `RinexObsDowngrade`.

## 10. Not bound here

The lint report's per-finding `detail` stays the engine's debug text; the lint
findings are not yet carried as a typed union. `ObservationQcReport` does not
carry the engine report's `header` and `systems` sections. The synthetic
observation scenario's RINEX serializer is not bound.
