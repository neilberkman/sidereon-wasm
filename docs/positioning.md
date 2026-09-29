# Positioning, RTCM and SBAS changes in the 3.0 surface

This document describes the settled 3.0 surface for satellites left out of a
solve, RTK arcs built from RINEX, the RTCM encoder's refusals, the SBAS
correction store, transmission-epoch placement and UT1 coverage. It does not move the package version pin.

## 1. Satellites left out of a solve

`SppSolution.rejectedSats` lists each satellite left out of the solve as
`{ satelliteId, reason }`, in observation order; `StaticSolution.rejectedSats`
lists them per input epoch. Selection reports the first reason that applies,
tested in RTKLIB `rescode` order: `"noEphemeris"`, `"lowElevation"`,
`"sbasIonoUncovered"`, `"ionosphereCarrierUnresolved"`. `"sbasWithdrawn"`
comes from the augmentation paths.

With the ionosphere correction on, a satellite whose carrier the delay cannot be
scaled to is `"ionosphereCarrierUnresolved"`: a GLONASS satellite with no entry
in `glonassChannels`, or a channel outside the `-7..=6` FDMA allocation (the
`7` real headers give `R28`). It is left out and the rest of the epoch is
solved. An epoch left with too few satellites fails for that reason. GPS, QZSS,
SBAS, Galileo, BeiDou and NavIC have fixed carriers.

## 2. RTK arcs from RINEX

`buildRinexRtkArc` and `buildDualFrequencyRinexRtkArc` form a satellite's
measurement from the first configured pair whose values are present and whose
carriers resolve, so a GLONASS slot without a carrier on `L1C` falls back to a
configured CDMA `L3Q`. A satellite no configured pair resolves for is left out
of that epoch and listed in `unresolvedCarriers` as
`{ receiver: "base" | "rover", epochIndex, satelliteId, observableCode }`, with
`epochIndex` counted in that receiver's file. The arc no longer fails for it.

## 3. RTCM encoder

`encodeRtcm` and `encodeRtcmFrame` refuse a message the wire layout cannot
state, never writing it as a different mask or satellite: an MSM satellite id
outside `1..=64`, a signal id outside `1..=32`, a satellite or satellite/signal
cell listed twice, a signal whose satellite is not listed, and an ephemeris
satellite id wider than the message's field (four bits for QZSS 1044, six
otherwise). The refusal is an `Error` named `RtcmEncodeError` whose `detail` is
`{ kind: "INVALID_INPUT", reason, message }`, or `{ kind: "UNKNOWN", message }`
for any other engine error.

## 4. SBAS

`SbasCorrectionStore.unassignedMaskCorrections(geo)` returns, per 1-based PRN
mask number, the corrections a GEO addressed to active mask bits that name no
satellite held here, as `{ maskNumber, count }` with `count` an exact `bigint`,
or `null` when the GEO has no partition. The mask follows the RTCA DO-229
layout (1..37 GPS, 38..61 GLONASS slots 1..24, 120..158 SBAS); an unassigned
bit keeps its place among the active bits, so the corrections after it reach
their own satellites.

`satToSbasPrn` converts only the slots a broadcast PRN exists for, `S20`
through `S58`, and returns `null` for any other slot.

## 5. Frequencies

`rinexBandFrequencyHz` and `rinexObservationFrequencyHz` resolve a GLONASS G1
or G2 carrier only for a channel in `-7..=6`, and resolve the GLONASS CDMA
carriers G3, G1a and G2a (bands `3`, `4`, `6`), SBAS L1 and L5, NavIC L5, S and
L1, and QZSS L6.

## 6. Transmission epoch and satellite clock

SPP, static, DGNSS and PPP solves place each satellite at the transmission
epoch of its measured pseudorange, `t_tx = (t_rx - P / c) - dts`, as RTKLIB
`satposs` places it. The SPP, DGNSS and tight-fusion code models add to a
precise product's satellite clock the relativistic term `-2 r.v / c^2` RTKLIB
`peph2pos` applies; the PPP rows already applied it. A broadcast
satellite clock is the polynomial and the relativistic term, without the
single-frequency group delay; the SPP, FDE and DGNSS requests apply that delay
through `pseudorangeCode`:

| `pseudorangeCode` | Broadcast group delay |
| --- | --- |
| `"singleFrequency"` (default) | GPS and QZSS TGD, Galileo BGD, BeiDou TGD1 subtracted, as RTKLIB `prange` subtracts it |
| `"ionosphereFree"` | none, as `prange` applies none under `IFLC` |

An SP3 source carries no group delay, so the choice changes nothing there.

A PPP observation whose code is zero or negative has no transmission epoch. It
is left out of the solve and listed in `unplacedObservations` of
`PppFloatSolution` and `PppFixedSolution` as
`{ epochIndex, satelliteId, ambiguityId, reason }`, with `reason`
`"codeNotPositive"`. A reason a later engine adds before this binding names
it crosses as the engine variant's name in lowerCamelCase.

PPP observations take `signals: { code1, code2, phase1, phase2 }`, each a RINEX
3 signal code (`"1C"` or `"C1C"`), naming the codes and phases the
ionosphere-free combination was formed from. An SSR or HAS bias applies only to
an observation that states its signal. An invalid code is a `TypeError`.

## 7. UT1 coverage

Every function that reads UT1 refuses an instant outside the UT1 table by
default. Its `WithValidity` variant takes `"strict"` (the default) or
`"permissive"` and returns `{ value, ut1Degraded }`, `ut1Degraded` being
`"beforeCoverage"`, `"afterCoverage"` or `null`. `Instant.ut1Degraded`,
`SppSolution.ut1Degraded` and `CoverageGrid.ut1Degraded` report the departure
a permissive policy accepted, and `solveStaticReferenceStationRinex` reports a
refused epoch as the mode error `"ut1OutsideCoverage"`.

## 8. TypeScript

`SppRejectionReason` and `SppRejectedSatellite` (`src/spp.rs`),
`RtcmEncodeErrorDetail` (`src/rtcm.rs`) and `SbasUnassignedMaskCorrections`
(`src/sbas.rs`) are declared in `typescript_custom_section` blocks and
re-exported from `@neilberkman/sidereon/types`. `PppUnplacedObservation`,
`PppObservationSignals`, `Ut1Validated<T>` and `Ut1DegradeReason` are declared
in the typed overlay of `scripts/postbuild.mjs`. The SP3 writer's
`SATELLITE_NOT_REPRESENTABLE` refusal is in `writers.md`.
