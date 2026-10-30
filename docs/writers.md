# SP3 and ANTEX writers, SP3 merge agreement

This document describes the settled 3.0 surface of the SP3 writer, the ANTEX
writer and the SP3 merge agreement metrics. It does not move the package
version pin.

## 1. SP3 writer

`Sp3.toSp3String()` writes a field only when reading its columns back the way
the parser reads them gives the value the product holds, bit for bit. A value
the columns cannot state, a value that would read back as one of the format's
absence sentinels, an epoch no record restates, or text that would not survive
the reader's trim is refused rather than rounded, shifted or dropped.

A refusal is an `Error` named `Sp3WriteError` whose `detail` is an
`Sp3WriteErrorDetail`: a union on `kind` carrying the engine's payload and
`message`. The kinds are `NO_EPOCHS`, `TEXT_NOT_COLUMN_SAFE`,
`TEXT_NOT_COLUMN_STABLE`, `BLANK_DESCRIPTOR`, `EMPTY_COMMENT`,
`TEXT_TOO_WIDE`, `INTEGER_TOO_WIDE`, `NON_FINITE`, `NUMBER_TOO_WIDE`,
`PRECISION_NOT_REPRESENTABLE`, `YEAR_NOT_REPRESENTABLE`,
`EPOCH_REPRESENTATION_UNSUPPORTED`, `EPOCH_NOT_RESTATABLE`,
`EPOCH_TIME_SCALE_MISMATCH`, `HEADER_TIME_SCALE_MISMATCH`,
`EPOCH_COUNT_MISMATCH`, `ACCURACY_CODE_COUNT_MISMATCH`, `DUPLICATE_SATELLITE`,
`SATELLITE_NOT_REPRESENTABLE`, `EPOCH_ARRAY_LENGTH_MISMATCH`, `UNDECLARED_SATELLITE_RECORD`,
`CONFLICTING_RECORDS`, `VELOCITY_STATE_IN_POSITION_PRODUCT`,
`RECORD_VALUE_NON_FINITE`, `RECORD_VALUE_TOO_WIDE`,
`RECORD_VALUE_NOT_REPRESENTABLE`, `RECORD_READS_AS_ABSENT`,
`RECORD_FIELDS_DISAGREE`, and `UNKNOWN` for a variant this binding does not yet
name.

Call `Sp3.toSp3StringWithRounding()` when SP3 column precision is an explicit
output choice. It returns `{ text, report }`, where `report` is a
`Sp3RoundingReport` containing maximum absolute changes in SI units. The JS
names map to the core report fields as follows: `positionXMaxChangeM` /
`positionYMaxChangeM` / `positionZMaxChangeM` are `position_x_m` /
`position_y_m` / `position_z_m`; `velocityXMaxChangeMPerS` /
`velocityYMaxChangeMPerS` / `velocityZMaxChangeMPerS` are `velocity_x_m_s` /
`velocity_y_m_s` / `velocity_z_m_s`; `clockMaxChangeS` is `clock_s`; and
`clockRateMaxChangeSPerS` is `clock_rate_s_s`. Rounding uses nearest decimal
with ties to even at the six places in each record column. Headers and epochs
are not rounded, the input `Sp3` is unchanged, and the same structural,
finite-value, width and absence-sentinel checks still apply. `toSp3String()`
remains strict and does not opt into rounding.

`Sp3.toSp3StringWithAccuracyRounding(Sp3AccuracyRoundingPolicy.Conservative)`
also adjusts encoded
finite accuracy standard deviations when their columns require it. Pass the
policy explicitly; omitted, null, or unknown policies throw `TypeError`. It
returns `{ text, report: { recordValues, accuracySigmas } }`. The accuracy
report gives the maximum absolute SI change for each standard deviation, and
conservative policy never lowers a finite value. Unknown and unspecified
accuracy states remain unchanged.

- A satellite is its token (`"G01"`), a time scale its short identifier
  (`"GPST"`), and an SP3 time system its three-character label (`"GPS"`).
  `SATELLITE_NOT_REPRESENTABLE` names a satellite that has no `01`..`99`
  token, such as a number of 0 or 100, so it carries the system letter as
  `system` and the number as `prn` instead.
- An engine `u64` or `i64` (`INTEGER_TOO_WIDE.value`,
  `YEAR_NOT_REPRESENTABLE.year`, `EPOCH_COUNT_MISMATCH.declared`) crosses as an
  exact decimal string beside a `number` (`valueNumber`, `yearNumber`,
  `declaredNumber`) that is `null` where the integer is not exactly
  representable as one.
- `EPOCH_NOT_RESTATABLE.residualS` is `null` where no candidate record could be
  read back at all, which the engine marks with NaN.
- `RECORD_FIELDS_DISAGREE.stored` and `.native` are `null` for the half the
  record does not hold.

## 2. ANTEX writer

`Antex.toAntexString()` writes every record from a retained value, in the
order ANTEX 1.4 lays records out, and no record the source did not carry apart
from the start and end records of blocks and sections (`antex.md`). It refuses
field overflow, precision loss, validity seconds no 13-column form with a
decimal point states, a frequency label that is not a system flag and a
two-column number, sample coordinates the reader could not reconstruct, and
disagreement between the public antenna fields and the validity intervals the
product retains. A refusal is an `Error` named `AntexWriteError` whose `detail`
is an `AntexWriteErrorDetail`: `{ kind: "UNWRITABLE", field, reason, message }`,
or `{ kind: "OTHER", message }` for any other ANTEX error the writer returns
(the writer raises only `UNWRITABLE` today).

## 3. SP3 merge agreement

A merged cell that carries a clock and no position, as a clock-only record
does, reports `positionMembers` 0 and `positionRmsM` and `positionMaxM`
`undefined`, as `clockRmsS` and `clockMaxS` are `undefined` for a cell without
a clock. An absent orbit has no dispersion, so none is reported rather than a
zero that would read as perfect agreement.

## 4. TypeScript

`Sp3WriteErrorDetail` and `AntexWriteErrorDetail` are declared in the
`typescript_custom_section` blocks of `src/sp3.rs` and `src/antex.rs` and
re-exported from `@neilberkman/sidereon/types`.
