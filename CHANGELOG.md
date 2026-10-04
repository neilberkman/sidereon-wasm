# Changelog

## Unreleased

## 3.0.0 - 2026-10-04

Engine update: sidereon / sidereon-core 3.0.0. Every entry below
restates an engine change as it reaches the JavaScript API.

### Labels

- **Breaking.** A variant a later engine adds to an enum this binding labels
  crosses under its own name, the engine variant's name in the case of the
  labels around it (`lowerCamelCase`, or `UPPER_SNAKE_CASE` for TDM input
  error kinds), where 2.1.1 gave `"unknown"` or `"UNKNOWN"`. This covers the
  almanac season, Moon phase, planet, planetary event, culmination and eclipse
  kinds; the RINEX clock header and record readings; the TDM input error
  kinds; and the new PPP unplaced reasons, Bias lookup statuses, RTCM and SBAS
  departures and SBAS refused-line reasons. Where the declarations state these
  as unions of literals, the unions end in
  `| (string & {})`.

### Positioning

- **Breaking.** SPP, static, DGNSS, PPP and tight-fusion solves place each
  satellite at the transmission epoch of its measured pseudorange,
  `t_tx = (t_rx - P / c) - dts`, as RTKLIB `satposs` places it. This moves
  solutions by the receiver clock times each satellite's range rate: nothing
  measurable on a steered receiver clock, decimetres of range on a receiver
  clock of half a millisecond.
- **Breaking.** The SPP, DGNSS and tight-fusion code models apply to a precise
  (SP3 or interpolated precise) satellite clock the relativistic term
  `-2 r.v / c^2`, as RTKLIB `peph2pos` does; they applied none. Their
  solutions on precise sources move by metres to tens of metres. PPP rows
  already applied the term and do not change for it.
- **Breaking.** `buildRinexRtkArc` and `buildDualFrequencyRinexRtkArc` place
  each receiver's satellites from that receiver's own pseudoranges, as RTKLIB
  `satposs` places them, where they used the reception epoch less the
  pseudorange over `c` rounded to whole microseconds with no satellite clock.
  A satellite is skipped for an epoch where the source has no clock for it or
  its pseudorange is not a positive distance. A satellite whose phase
  observable has no carrier no longer fails the arc: its measurement is formed
  from the first configured pair whose carriers resolve, and a satellite no
  pair resolves for is left out of that epoch and listed in the new
  `unresolvedCarriers` as `{ receiver, epochIndex, satelliteId,
  observableCode }`.
- **Breaking.** A PPP observation whose code is zero or negative is no longer
  solved at a meaningless transmission epoch. It is left out and reported in
  the new `unplacedObservations` on `PppFloatSolution` and
  `PppFixedSolution`, each `{ epochIndex, satelliteId, ambiguityId, reason }`
  with `reason` `"codeNotPositive"`.
- **Breaking.** An ionosphere-corrected SPP or static solve no longer fails a
  whole epoch because one satellite has no resolvable carrier (a GLONASS
  observation without a valid `glonassChannels` entry). The satellite is left
  out and reported in the new `rejectedSats` of `SppSolution` and
  `StaticSolution`, each `{ satelliteId, reason }`, with `reason` one of
  `"noEphemeris"`, `"lowElevation"`, `"sbasWithdrawn"`, `"sbasIonoUncovered"`
  and `"ionosphereCarrierUnresolved"`.
- **Breaking.** A broadcast satellite clock no longer includes the
  single-frequency group delay. SPP, FDE and DGNSS requests take
  `pseudorangeCode`, `"singleFrequency"` (the default) or `"ionosphereFree"`;
  the broadcast group delay (GPS and QZSS TGD, Galileo BGD, BeiDou TGD1) applies
  to single-frequency code only, as RTKLIB `prange` applies it.
- PPP observations take `signals: { code1, code2, phase1, phase2 }`, the RINEX
  3 tracking codes the ionosphere-free code and phase were formed from. SSR and
  HAS biases apply only to an observation that states the signals they are for.
- PPP residuals carry `ambiguityId`. `PppFloatSolution` gains
  `solvedEpochIndices`, `residualScreen`, `residualScreenRemovals` and
  `solveOptions` (the iteration cap and tolerances the solve ran with);
  `PppFixedSolution` gains `solvedEpochIndices`.
- `solveStaticReferenceStationRinex` reports a reference or rover epoch outside
  the UT1 table as a mode error with `kind` `"ut1OutsideCoverage"`, and the
  code, carrier and station solutions carry `ut1Degraded`.
  `sbasProtectionLevels` refuses such an epoch with the error label
  `"Ut1OutsideCoverage"`, and DGNSS solves throw an `Error` naming it.

- **Breaking.** SPP and the static solve select, mask and weight the
  satellites at every iterate, as RTKLIB `estpos` re-runs `rescode`, instead of
  once at the initial guess. A pass whose selection repeats the previous one
  takes RTKLIB's least-squares step, and the solve ends with the first such
  step below 1e-4 m. Solutions move wherever the selection or the weights at
  the solution differ from those at the initial guess; a coarse cold start that
  failed now solves. A solve whose selection does not settle throws an `Error`
  naming the passes it ran.
- **Breaking.** `StaticSolution.metadata.status` is `"SelectionSettled"` for a
  converged solve and `"OuterBudgetExhausted"` for a robust solve that spent
  its outer budget first, and `converged` describes how the whole solve ended:
  a spent robust budget reports `converged: false`. The status is typed as the
  new `SolveStatus` union, and the metadata as `StaticSolveMetadata`.
- `SppSolution.metadata` reports how the solve ran and ended, `{ iterations,
  converged, status, outerIterations, finalRobustScaleM, ionosphereApplied,
  troposphereApplied, usedCount }` (`SppSolveMetadata`).
- The SPP carrier check applies only when a frequency-dependent term is
  applied, so a solve without the ionosphere correction no longer leaves out a
  satellite for want of a carrier. The PPP auto-init SPP seed passes each
  GLONASS observation's channel.

- **Breaking.** Every SPP-family failure throws an `Error` named
  `PositioningError` whose `detail` is a typed `PositioningErrorDetail`: a
  union on `kind` with one member per engine variant and the fields it
  carries. This covers `solveSpp`, `solveSppWithDopplerVelocity`,
  `solveSppBatch`, `solveSppFromRinexObs`, `solveBroadcast`,
  `solveWithFallback`, `fde`, `solveSppSbas`, `solveStatic` and the DGNSS
  solves, including the new `SELECTION_UNSETTLED { passes }` and
  `UT1_OUTSIDE_COVERAGE { reason }`. A failure that wraps another nests it as
  `cause` (`EPOCH_INPUT`, `PRECISE_SOLVE_FAILED`, `BROADCAST_SOLVE_FAILED`); a
  rejected solution names its validation failure (`SOLUTION_REJECTED` with
  `validation.kind` such as `DEGENERATE_GEOMETRY_PDOP { pdop }`); a RINEX SPP
  variant a later engine adds crosses as `OTHER` with its own name in
  `variant`. They were plain `Error`s carrying only a message, and a
  fallback failure was named `PreciseSolveError` or `BroadcastSolveError`.
  `SppBatchSolution.error(i)` and `RinexSppSolutionBatch.error(i)` return the
  detail instead of a message string, and `broadcastReason.preciseError` is a
  detail. A non-finite DGNSS base position throws a `PositioningError`
  (`DGNSS_INVALID_INPUT`) instead of a `RangeError`.

### UT1 coverage

- **Breaking.** Every function that reads UT1 (sidereal times, GCRS/ITRS
  transforms, look angles, passes, ground tracks, visibility, observe, Sun and
  Moon az/el and illumination, rise/set and meridian transits, coverage grids,
  PPP corrections, SSR-corrected states) refuses an instant outside the UT1
  table by default and throws. It previously held TT-UT1 at the table edge; a
  pass search scored the failed look angle as -90 degrees and returned no
  passes. Where an instant outside the table is accepted, the engine follows
  the Skyfield 1.54 delta-T curve.
- `WithValidity` variants take an optional UT1 policy, `"strict"` (the
  default) or `"permissive"`, and return `{ value, ut1Degraded }`, where
  `ut1Degraded` is `"beforeCoverage"`, `"afterCoverage"` or `null`:
  `gmstRadiansWithValidity`, `gastRadiansWithValidity`,
  `gcrsToItrsWithValidity`, `itrsToGcrsWithValidity`, `sunAzElWithValidity`,
  `moonAzElWithValidity`, `moonIlluminationWithValidity`,
  `findMoonElevationCrossingsWithValidity`, `findMoonTransitsWithValidity`,
  `meridianTransitsWithValidity`, `meridianTransitsSpkWithValidity`,
  `observeWithValidity`, `observeSpkBodyWithValidity`,
  `Tle.lookAnglesWithValidity`, `Tle.findPassesWithValidity`,
  `Tle.groundTrackWithValidity` and `visibleFromSatellitesWithValidity`.
- `Instant.ut1Degraded`, `SppSolution.ut1Degraded` and
  `CoverageGrid.ut1Degraded` report the departure a permissive policy
  accepted. `coverageLookAngles` and `pppCorrections` (option `ut1Validity`)
  take the policy; `pppCorrections` also returns `ut1Degraded` and `warnings`.
  `ssrCorrectedState` takes an optional UT1 policy and returns `ut1Degraded`.

### TLE and SGP4

- `Sgp4Satellite.fromOmm(omm)` initializes a reusable SGP4 propagator through
  the engine's canonical OMM bridge. Its `propagate(epochs)` returns the same
  TEME arrays as the matching TLE, while incompatible metadata and missing
  `MEAN_MOTION` or `BSTAR` remain typed `OmmError` failures.
- **Breaking.** `new Tle(line1, line2, opsMode?, policy?)` and
  `parseTleFile(text, opsMode?, policy?)` take a checksum policy. `"strict"`,
  the default, refuses a column-69 digit that disagrees with the computed
  checksum and a column 69 that is not a digit; 2.1.1 read both. `"lenient"`
  reads them and reports each in `checksumWarnings`, now
  `{ kind, expected?, found?, computed, message }`.
- **Breaking.** `Tle.revNumber`, `elementSetNumber` and `ephemerisType` are
  `undefined` when the field is blank, instead of `0`.
- **Breaking.** `parseTleFile` returns `rejected`, one
  `{ lineNumber, name, issue, message }` per record it refused, and `skipped`
  is its length. Each `NamedTle` carries `lineNumber` and `checksumWarnings`.

- **Breaking.** `Tle.toLines` spells B\* and the second mean-motion derivative
  as python-sgp4's `export_tle` does: the five significant digits of
  `value * 10`, correctly rounded with ties to even (3.21675e-9 is now
  `" 32168-8"`, where it was `" 32167-8"`). A zero B\* is `" 00000+0"`, a
  negative zero `"-00000+0"`, and a zero second derivative `" 00000-0"`.
- **Breaking.** An OMM epoch of whole microseconds takes the SGP4 epoch
  python-sgp4 2.22 gives it, bit for bit, and a satellite built from such an
  OMM is initialised as python-sgp4's `sgp4.omm.initialize` does, with B\* and
  the second derivative as stated. Epochs of the NAVSTAR 43 and GALAXY 15
  fixtures move by about 0.3 microseconds. Other OMM element sets quantize B\*
  and the second derivative onto the TLE grid at exponent -9, as the TLE writer
  rounds them.
- **Breaking.** Passes and look angles propagate SGP4 at the split Julian date
  Skyfield 1.54 uses for the same UTC instant, bit for bit.

### SPK

- **Breaking.** SPK states always carry a velocity. Type 2 segments return
  the derivative of the position polynomial, where the velocity was
  `undefined`.
- `SpkKernels` holds several kernels and resolves a body chain across them
  (`new SpkKernels()`, `push`, `pushBytes`, `length`, `state`,
  `stateInFrame`). `Spk.stateInFrame`, `spkInertialFrameName` and
  `spkInertialFrameRotation` expose the kernel's inertial frames.

### Time

- **Breaking.** The embedded leap-second table dated the 1997 and 1999 leap
  seconds to 1997-01-01 and 1998-01-01. TAI - UTC became 31 s on 1997-07-01
  and 32 s on 1999-01-01, so every UTC conversion from 1997-01-01 to 1997-06-30
  and from 1998-01-01 to 1998-12-31 moves by one second, and `23:59:60` is
  accepted on 1997-06-30 and 1998-12-31 instead of 1996-12-31 and 1997-12-31.
- NMEA sentence seconds are read as the decimal stated, rounded once (`07.56`
  was 7.5600000000000005).

### Space weather

- **Breaking.** `sampleAt(epoch, policy?)` under the default policy refuses a
  monthly predicted row, which states no Ap, instead of substituting the quiet
  Ap of 4. `"lenient"`, or a policy object with `requireGeomagnetic: false`,
  substitutes it and reports it. Rows whose flux qualifier states no
  observation have the new class `"notObserved"`, refused unless the policy
  sets `allowNotObserved`. Unknown policy keys and names throw a `TypeError`.
- `apHistoryAt(epoch, policy?)` returns the seven-element NRLMSISE-00 Ap
  history `{ ap, class, apDefaulted, binsFromDailyAp }`. Decay requests take
  `policy`.

### CCSDS OMM, OPM, OEM and CDM

- **Breaking.** Reader and writer failures throw typed errors named
  `OmmError`, `OpmError`, `OemError` and `CdmError`, whose `detail` is an
  `NdmErrorDetail` (`kind`, `message` and the fields that variant carries).
  Writers (`toKvnString`, `toXmlString`, `toJsonString` and the new CSV
  writers) throw instead of writing text that would not read back.
- **Breaking.** A value a message does not carry is `undefined`, not a
  fabricated default: an OMM read from JSON without `CCSDS_OMM_VERS` has no
  version, and `new Omm(...)` takes `meanMotion` and `noradCatId` as optional.
- **Breaking.** OPM and OEM covariances are built from the 21-value lower
  triangle (`lowerTriangle`) or a symmetric 36-value matrix; an asymmetric
  matrix is refused. `matrix` returns the stored values unvalidated and
  `toValidatedMatrix()` checks positive semidefiniteness.
- **Breaking.** `OemSegment(metadata, states, covariances, dataComments?,
  covarianceComments?)` throws on invalid input. `skippedStates` is an array of
  `{ line, segment, text, reason, itemCount, field, issue }`, with the count in
  `skippedStateCount`.
- Every record the messages define is retained: OMM `classification`,
  `messageId`, `refFrameEpoch`, `semiMajorAxisKm`, `gmKm3S2`, spacecraft
  parameters, `btermM2Kg`, `agomM2Kg`, covariance, user-defined parameters and
  comments; OPM and OEM `classification`, `messageId`, `refFrameEpoch`,
  user-defined parameters and comments per block; CDM `ccsdsCdmVers`,
  `messageFor`, relative state, screening volume and period, comments, drag,
  SRP and thrust covariance rows, OD parameters and additional parameters,
  with `toCovarianceRtn()`.
- `parseOmmCsv`, `parseOmmXmlAll`, `parseOmmJsonArray` and `parseOmmCsvArray`
  read CSV and multi-record OMM, returning `{ omms, skipped }` for the array
  readers. `Omm.toCsvString`, `toCsvStringDiscardingComments` and
  `toJsonStringDiscardingComments` write them.

### Bias-SINEX and CODE DCB

- **Breaking.** `loadBiasSinex(bytes, policy?)` and
  `loadCodeDcb(bytes, options, policy?)` are strict by default: a file that
  departs from Bias-SINEX 1.00, or a DCB title whose time-system label is
  unknown when no options are given, is refused. `"lenient"` reads it and
  reports each departure in `notices`.
- **Breaking.** Bias lookups (`codeOsbSeconds`, `phaseOsbCycles`,
  `codeDsbSeconds`, `codeBiasModelM`) return a `BiasLookup`,
  `{ status, value, records, overridden, productScale, queryScale,
  observable }`, instead of a number or `undefined`, so an absent, ambiguous or
  out-of-scale lookup is distinct from a value. `phaseOsbCycles` takes
  `timeScale?` and `carrierHz?`.
- Bias records carry `family`, `unit`, `svn`, `validFrom`, `validUntil`,
  `slopeSigma` and `line`. `BiasSet` gains `mode`, `timeScale`,
  `timeSystemLabel`, `notices` and `lineCounts`, and writes with
  `toBiasSinex()` and `toCodeDcb()`.

### RINEX navigation

- **Breaking.** `BroadcastRecord.issue`, `issueMessage` and `svAccuracyM` are
  `undefined` where the message states none (a CNAV record has no IODE).
  `toRinexString` and `encodeRinexNav` throw instead of writing a record that
  would not read back.
- **Breaking.** `issueMessage` gains `"navic_lnav"` and
  `"galileo_unclassified"`. A Galileo record's message is read from its
  data-source word by RINEX 3.05 Table A8; a word that names no single message
  is `"galileo_unclassified"`, where it was read as I/NAV or F/NAV.
- Records expose the fields their message states: `iodc`, `l2Codes`,
  `l2pDataFlag`, `galileoDataSources`, `beidouAodc`, `transmissionTimeSow`,
  GLONASS `statedFreqChannel`, `ageDays`, `statusFlags`, `healthFlags`, group
  delays and flag words, `isHealthy`, `singleFrequencyGroupDelayS` and
  `clockBiasAtS`. Ionosphere corrections add QZSS, NavIC, Galileo NeQuick,
  Galileo disturbance flags and BeiDou BDGIM, with
  `BroadcastEphemeris.ionoCorrectionsAt`.
- `BroadcastEphemeris` and the NAV parse result report `skipped` blocks with
  their `line`, `departures` and `other` blocks. `parseRinexNavFile` returns a
  `RinexNavFile` that keeps every entry and writes it back.

### RTCM

- **Breaking.** `decodeRtcm` reads a buffer in full or throws: a stray byte, a
  CRC-24Q failure, a trailing partial frame, or a body that does not decode or
  departs from RTCM 3. It returned the frames it could read. Use
  `decodeRtcmStream` for noisy input.
- **Breaking.** `decodeRtcmStream(bytes, policy?)`, `decodeRtcmFrame(bytes,
  policy?)`, `encodeRtcm(message, policy?)` and `encodeRtcmFrame(message,
  policy?, reserved?)` take a policy, `"strict"` (the default) or
  `"lenient"`. Messages carry `trailingBits`, MSM messages `signalMask` and
  GLONASS fields `negativeZero`, so a lenient decode re-encodes byte for byte.
  Stream diagnostics gain `crcFailures` and `departures`, and skipped frames
  the reason `"departure"`. `decodeRtcmFrame` returns `reserved` and
  `departures`; `FrameScanner` gains `resyncBytes`, `crcFailures` and
  `reserved`.

- The decoded message IR is typed: `decodeRtcm`, `decodeRtcmMessage`,
  `decodeRtcmStream`, `decodeRtcmFrame` and `FrameScanner.next()` return
  `RtcmMessage` (a union on `type`), `RtcmStream`, `RtcmFrame` and
  `RtcmScannedFrame`, with `RtcmDeparture` and `RtcmFrameSkip` for the
  diagnostics; `encodeRtcm` and `encodeRtcmFrame` take `RtcmMessageInput`.
  Raw 64-bit fields are `bigint`.
- `encodeRtcm` and `encodeRtcmFrame` encode SSR messages (`type: "ssr"`), so
  every message type the decoder returns re-encodes.
- The encoder reads the GNSS system as the decoder writes it (`"GPS"`,
  `"GLONASS"`, `"Galileo"`, `"BeiDou"`, `"QZSS"`, `"NavIC"`, `"SBAS"`) as well
  as in lower case. A decoded MSM or SSR message was refused with
  `invalid GNSS system label "GPS"`.

### SBAS

- **Breaking.** `decodeSbasMessage(bytes, form?, policy?)` refuses by default
  a preamble other than `0x53`, `0x9A` and `0xC6`. `"lenient"` reads it and
  reports it in `departures`; the result carries `padBits`.
- `parseSbasEmsLog` and `parseSbasRtklibLog(text, { policy, referenceWeek })`
  return an `SbasLog` with `blocks`, `skippedLines`, `refusedLines` and
  `departures`. Log blocks carry `declaredMessageType` and `messageType` and
  decode with `decode(policy?)`. The ionospheric grid reports
  `unavailableIgps`.

### SSR

- SSR orbit and clock corrections carry `navMessage` (`"rtcm"` or `"has"`),
  `hasNavMessageIndex` and `transmittedEpochJ2000S`.

### IONEX

- **Breaking.** A grid node the file gives as `9999` is now `null`. At the default exponent it used to read as 999.9 TECU, and `slantDelay` interpolated it. `Ionex.tecGridSamples()` returns `tecMaps` and `rmsMaps` as `(number | null)[][][]`, `tecSamples()` returns `vtecTecu` and `rmsTecu` as `number | null`, and `ionexFromSamples` / `ionexFromNodeSamples` accept `null` at those positions.
- **Breaking.** `tecGridSamples().rmsMaps` is `null` for a product that carries no RMS maps, where it was `[]`. A declared RMS stack whose nodes are all `null` is kept, and it stays distinct from an absent stack. The result also carries `heightMaps` (`null` when absent) and `header`, and each `tecSamples()` row carries `heightOffsetKm`.
- `START OF HEIGHT MAP` blocks are now read (a height map's `END OF HEIGHT MAP` record used to fail the parse). They are exposed as `Ionex.heightMaps`, in km added to `HGT1`. New getters: `Ionex.tecMaps`, `rmsMaps`, `heightMaps` (each `IonexMapCube`, or `null` when absent), `hasRms`, `hasHeight` and `skippedRecords`.
- **Breaking.** `toIonexString()` throws an `IonexWriterError` whose `detail` is `{ kind: "UNWRITABLE", message }` when its IONEX field cannot state a value, axis or header field exactly. It used to round values to the header exponent, write a 0.25 degree step as 0.2 and overrun columns. The writer now uses the IONEX 1 layouts (`2X,3F6.1` axes, `2X,5F6.1` band records, `16I5` values) and picks an exponent that states every value exactly. It writes back the header records the product carries, plus `EPOCH OF FIRST MAP`, `EPOCH OF LAST MAP`, `# OF MAPS IN FILE`, `MAP DIMENSION` and `END OF FILE`, so its text differs from 2.1.1.
- **Breaking.** `Ionex.slantDelay`, `IonexSelection.slantDelay` and the new top-level `ionexSlantDelay` now throw a named error with a typed `.detail` for an engine refusal, where they threw a plain `Error`. The names are `IonexCoverageError`, `IonexMissingNodesError`, `IonexVaryingHeightsError`, `IonexHeightNotAvailableError`, `IonexMappingFunctionError`, `IonexInvalidInputError`, and `IonexSlantError` for a variant the binding does not name. A query whose interpolation weights a `null` node is refused (`MISSING_NODES`). On a product with height maps, the delay uses `HGT1` plus the common height, or is refused with `VARYING_HEIGHTS` / `HEIGHT_NOT_AVAILABLE`.
- **Breaking.** Every J2000-second input (`epochJ2000S` on the slant entries and batch requests, `mapEpochsJ2000S` on `ionexFromSamples`, `epochJ2000S` on `ionexFromNodeSamples` rows) must be an integer within ±9007199254740991. Values between 2^53 and the `i64` range used to be accepted and could wrap.
- **Breaking.** `ionexFromSamples` and `ionexFromNodeSamples` throw a `TypeError` for an own string property outside the accepted names, enumerable or not (for example `heightMap` or `rmsTec`), where the property used to be ignored. `ionexFromNodeSamples` requires an array, walks it in order and names the index of the first malformed sample. A throwing accessor on `header`, on the header's mapping records or on a policy axis re-raises the caller's own value unchanged.
- `Ionex.header` returns the descriptive header records as an `IonexHeader`: `version`, `satelliteSystem`, `program`, `runBy`, `date`, `descriptions`, `comments`, `intervalS`, `mappingFunction`, `mappingDeclaration`, `elevationCutoffDeg`, `observablesUsed`, `stationCount`, `satelliteCount` and `mapsInFile`. `Ionex.mappingFunction` and `Ionex.mappingDeclaration` read the same two records. `ionexFromSamples` accepts `header`, and `ionexFromNodeSamples` takes an optional fifth `header` argument. A supplied header whose `mappingFunction` and `mappingDeclaration` contradict each other is refused, as is a known tag with another code (`{ kind: "COSZ", code: "MOD" }`).
- New `IonexSlantPolicy` with three axes: `coverage` (`"strict"` / `"hold"`), `missingNodes` (`"strict"` / `"renormalize"`) and `mapping` (`"declared"` / `"singleLayer"`). It has static constructors and `with*` modifiers, and a plain `{ coverage?, missingNodes?, mapping? }` object is accepted in its place. It is taken by `Ionex.slantDelayWithPolicy`, `Ionex.slantDelaysBatchResults`, `ionexSlantDelayWithPolicy` and `ionexSlantDelayResults`. The results are `IonexSlantDelayEvaluation` objects whose `status` carries `held`, `degraded`, `assumedMapping`, `isValid` and `isNominal`, and a batch returns one `{ index, isOk, evaluation, refusal }` row per request.
- A product declaring a `MAPPING FUNCTION` other than `COSZ` is still mapped with the single-layer `1/cos(z')`, and the value is now flagged in `status.assumedMapping`. It stays `isValid`. `mapping: "declared"` refuses such a product with `MAPPING_FUNCTION`.
- `loadIonexWithWarnings`, `Ionex.parseWithWarnings` and `Ionex.parseStrWithWarnings` return `{ ionex, value, warnings }` with seven warning kinds: `MISSING_RECORD`, `VERSION_RECORD_NOT_FIRST`, `EPOCH_MISMATCH`, `MAP_COUNT_MISMATCH`, `NOT_A_NUMBER_VALUE`, `INTERVAL_MISMATCH` and `EXPONENT_CARRIED_INTO_MAP`. `Ionex.parse` and `Ionex.parseStr` are new static constructors.
- The reader accepts files it used to refuse:
  - an axis running in either direction, with `latNodesDeg` / `lonNodesDeg` now in file order (an ascending latitude axis was refused)
  - an epoch at hour 24 with zero minute and second
  - a `nan` value field, read as `null` with a `NOT_A_NUMBER_VALUE` warning
  - a map inheriting an earlier map's `EXPONENT`, reported as `EXPONENT_CARRIED_INTO_MAP`

  `slantDelay` also interpolates across the longitude seam of a grid that closes the circle, and returns a value along a line of sight through a pole.
- **Breaking.** The reader refuses what it used to misplace or pass over:
  - a `MAP DIMENSION 3` product, or an `HGT1 / HGT2 / DHGT` giving several heights
  - a band whose latitude, longitude or height is not a grid node
  - a map that gives a node twice or leaves one without a value
  - `START OF TEC MAP` numbers out of order
  - an RMS or height map that does not name a TEC map
  - a data record holding a non-ASCII character, or one outside a band
  - a `16I5` record with an empty field
  - an unclosed `AUX DATA` block
  - a file type other than `I`
  - two records of one header label that read as different values
- Values scale as `field * 10^EXPONENT` from an exact power of ten, and each band is placed by its own `LAT/LON1/LON2/DLON/H` record. An `EXPONENT` record inside a map applies to the data blocks after it. Previously, bands were placed in arrival order and an in-map `EXPONENT` before the first band was ignored.
- New standalone regular-grid surface: `TecGrid`, whose `values` are `(number | null)[]`, with `vtecAtPiercePoint` and `vtecAtPiercePointWithPolicy`. Alongside it are `TecGridEpoch` (exact `bigint` `unixNanos`), `TecGridShellGeometry` and `TecGridEvalOptions`, and the top-level `ionoDelayXyz`, `ionoDelayXyzWithPolicy`, `tecXyz` and `tecXyzWithPolicy` with an optional `ecefToLla` callback whose thrown value is re-raised unchanged. Failures throw a `TecGridError` whose `.detail` is a `TecGridErrorDetail`.

### TDM

- **Breaking.** `Tdm.comments`, `TdmMetadata.comments` and `TdmDataSection.comments` return `TdmComment[]` (`{ text, beforeRecord }`) instead of `string[]`. `toKvnString()` writes each comment back at its position rather than gathering comments to the top of their block, and comment text keeps its leading indentation.
- **Breaking.** `parseTdmKvn` throws a `TdmParseError` and `Tdm.toKvnString()` a `TdmWriteError`, where both threw a plain `Error`. Each carries a `detail` that is a `TdmErrorDetail` union on `kind`, with a one-based `line` that is `null` for a failure raised with no input line.
- **Breaking.** `parseTdmKvn` reads under the strict CCSDS 503.0-B-2 rules and now refuses:
  - a last line with no terminator (`UNTERMINATED_FINAL_LINE`)
  - a tab, control or non-ASCII character (`NON_PRINTABLE_CHARACTER`)
  - a line over 254 characters (`LINE_TOO_LONG`)
  - a missing `CREATION_DATE`, `ORIGINATOR`, `TIME_SYSTEM` or `PARTICIPANT_n` (`MISSING_KEYWORD`)
  - a data section with no records (`EMPTY_DATA_SECTION`), or `KEY =` with no value (`EMPTY_VALUE`)
  - a version outside `x.y` (`INVALID_VERSION`)
  - a timetag outside the 4.3.9 forms, one with `Z` under a time system other than UTC, one with second 60 other than at `23:59` UTC or `02:59` GLONASS, or one with more than 38 fraction digits (`MALFORMED_EPOCH`)
  - records out of chronological order (`RECORDS_OUT_OF_ORDER`), or a repeated keyword and timetag pair (`DUPLICATE_RECORD`)
  - keywords out of table order, or comments out of place (`KEYWORD_OUT_OF_ORDER`)
  - a `PATH` naming an undefined participant (`UNDEFINED_PARTICIPANT`)
  - a keyword outside its section's table (`UNDEFINED_KEYWORD`)
  - `PARTICIPANT_0`, `PARTICIPANT_6` and up, padded indices such as `PARTICIPANT_01`, or `PATH_3` (`INVALID_FIELD`)
  - a keyword repeated with another value (`CONFLICTING_KEYWORD`)
  - `COMMENT=value` or `META_START = 1` (`MALFORMED_LINE`)
- The reader ends a line at CR, LF, CRLF or LFCR, so a file written with carriage returns reads. It used to read as one line and was refused.
- **Breaking.** `Tdm.toKvnString()` applies the reader's rules to what it writes. It refuses under the strict write policy each departure listed above. It also refuses a keyword written twice with one value (`REPEATED_KEYWORD`), and a field or comment the KVN form cannot carry, such as an empty or padded key, a key holding `=`, or a line break (`UNWRITABLE`, `KEYWORD_NOT_ASSIGNABLE`). It now terminates the last line, which it did not.
- `parseTdmKvnWithPolicy(text, policy?)` returns `{ tdm, value, warnings }`. `policy` is `"strict"`, `"lenient"`, or an object setting `nonPrintable`, `missingKeywords`, `longLines`, `emptyDataSections`, `recordOrder`, `duplicateRecords`, `keywordOrder` or `finalTerminator` to `"strict"` or `"forgive"`. A misspelled axis is a `TypeError`.
- `Tdm.toKvnStringWithPolicy(policy?)` returns `{ text, value, departures }`. It takes the reader's axes plus `repeatedKeywords`, and reports each departure it emitted as a `TdmDeparture`.
- `TdmMetadata.fromRaw(fields, comments?)`, `TdmMetadata.fromRawWithPolicy`, `metadata.replaceRaw`, `metadata.replaceRawWithPolicy`, `new TdmField(key, value)` and `tdm.setSegmentMetadata(segmentIndex, metadata)` build and replace a metadata block from ordered raw fields and positioned comments. `participants`, `paths`, `mode`, `timetagRef`, `timeSystem` and `rangeUnits` are derived from those fields. A refused replacement throws a `TdmValidationError` and leaves the block unchanged.

- `TdmMetadata.setField(key, value)` sets one keyword: a stated keyword takes
  the value in place, an absent one is inserted where the keyword order of
  tables 3-2 and 3-3 puts it, and every comment keeps the field it precedes.
  `TdmMetadata.removeField(key)` removes every occurrence. Both validate as
  `replaceRaw` does and are atomic.

### RINEX observations

- **Breaking.** `RinexObs.toRinexString()` returns text only when reading it back gives the product. Otherwise it throws a `RinexObsWriteError` whose `detail` is a `RinexObsWriteErrorDetail` naming the first field that would change; it used to drop, round, wrap or truncate and return the text anyway. A version 2 product is written as a RINEX 2 file, where it used to be written with version 3 `>` epoch records under a version 2 header. A version 2 product holding `SYS / SCALE FACTOR` records is refused (`SCALE_FACTORS_IN_VERSION_TWO`), and so is picoseconds in a product below 4.02 (`EPOCH_PICOSECONDS_NOT_IN_VERSION`).
- **Breaking.** `RinexObsRepair.repairedText` now throws that same `RinexObsWriteError`, where it was a plain `string` getter, and `RinexObsRepair.toCrinexString()` throws it for a writer refusal. `repairRinexObs` returns the repair whether or not its product can be written.
- **Breaking.** `ObsEpoch.epoch` is `ObsEpochTime | undefined`. It is `undefined` for an event whose epoch fields are blank, which RINEX 2.11 and 3.05 allow and which was refused.
- **Breaking.** `ObsPhaseShift.code` is `string | undefined`: a record naming only its constellation (RINEX 3.05 section 5.2.12) is read, where it used to be refused. `ObsPhaseShift.correctionCycles` is `number | undefined` for a blank correction. New getters: `unrepresentableSatellites` (designators such as `R28`, kept and written back), `coversEverySatellite` and `satelliteCount`. Satellite lists continue past ten satellites.
- **Breaking.** `carrierPhaseRows(i, filter?)` reads epoch `i` with the header in effect at it. `phaseShiftCycles` holds `NaN` where no single correction applies, beside the new `phaseShiftAvailable`, `phaseShiftStatus` (`available` / `unknown` / `ambiguous`) and `phaseShiftCorrections`. From RINEX 4.00, every row is an available 0.
- **Breaking.** Header records an event epoch (flags 2 to 5) carries now take effect for the epochs after it; they used to be kept as text and never applied. `ObsHeader.obsCodes(system)` is the union of every list the file declares, and each epoch's `observations` and `cycleSlips` are index-aligned to it. The new `declaredObsCodes(system)` gives the list in effect. `RinexObs.headerAt(i)` and `RinexObs.headerTimeline()` (`ObsHeaderTimeline`, `ObsHeaderSegment`) give the header in effect at each epoch.
- **Breaking.** Every epoch-index argument (`epoch`, `headerAt`, `observationValues`, `carrierPhaseRows`, `pseudoranges`, `ObsHeaderTimeline.at`, `ObsHeaderTimeline.segmentIndex`) throws a `RangeError` for a negative, fractional, non-finite or out-of-range index. Previously a fractional index was truncated to another epoch.
- **Breaking.** One header block now refuses two records that read as different values for the same label. This applies to:
  - `INTERVAL`, `MARKER NAME/NUMBER/TYPE`, `APPROX POSITION XYZ`, `ANTENNA: DELTA H/E/N`, `ANT # / TYPE`, `REC # / TYPE / VERS`, `OBSERVER / AGENCY` and `SIGNAL STRENGTH UNIT`
  - in the file header only: `RINEX VERSION / TYPE`, `TIME OF FIRST OBS`, `TIME OF LAST OBS`, `LEAP SECONDS` and `# OF SATELLITES`

  Two scale factors for one code and two channels for one GLONASS slot are refused too. The later record used to win. Conflicting phase shifts or GLONASS biases are kept and read as `ambiguous` instead of being refused.
- Flag 6 epochs are read as cycle slips into `ObsEpoch.cycleSlips` and `cycleSlipSatellites`, with `satellites` and `observations` empty, and `rinexObsCycleSlipFlag()` returns the flag. New `ObsEpoch` getters: `observations`, `rcvClockOffsetS`, `epochPicoseconds`, `declaredRecordCount` and `specialRecords` (the records an event carried, verbatim; they used to be discarded).
- New `ObsHeader` getters: `rinex2Types`, `rinex2System`, `programRunByDate`, `comments`, `markerNumber`, `markerType`, `observer`, `agency`, `receiver`, `antenna`, `timeOfLastObsEpoch`, `timeOfLastObsScale`, `declaredSatelliteCount`, `prnObsCounts`, `scaleFactors`, `signalStrengthUnit` and `unretainedHeaderLabels`. `leapSeconds` is an `ObsLeapSeconds` with `bigint` fields and a `timeSystem` that keeps blank and `GPS` distinct. `RinexObs.skippedRecords` is also new.
- `ObsHeader.glonassCodPhsBis` returns the record as written, with `biasM: null` for a blank bias. A header with a blank bias used to be refused. `glonassCodePhaseBias(code)` returns `available`, `none`, `unknown` or `ambiguous`.
- `RinexObs.downgradeToRinex2(version)` returns `{ obs, value, changes }` with every `ObsDowngradeChange`. It throws a `RinexObsWriteError` for what version 2 cannot state: `NOT_VERSION_TWO`, a carrier version 2 has no name for such as BeiDou B1C (`OBSERVABLE_NOT_REPRESENTABLE`), and `LEAP_SECONDS_TIME_SYSTEM_NOT_IN_VERSION`.
- **Breaking.** Version 2 observation codes are read as the signals they name. GPS `C2` below 2.12 reads as L2C rather than `C2C`, BeiDou digits name the band their frequency slot gives, and 2.12 lettered names read as their signals. Galileo and BeiDou are no longer given `P` codes, and a Galileo `C2` is no longer read as E5a.
- The reader takes fixed-column records from their columns. An epoch of 100 or more satellites, a position component of -10,000,000 m and a `PRN / # OF OBS` record laid out as `3X,A1,I2,9I6` now read. A file declaring version 2 while carrying `>` epoch records also reads.
- **Breaking.** The reader refuses values its own fields cannot write back:
  - an `INTERVAL`, position, antenna delta or first/last-observation second below its field's resolution or too wide for its columns
  - a version 2 value wider than `F14.3` or with more than three decimals
  - a version 2 type code wider than two characters
  - a `GLONASS COD/PHS/BIS` code longer than three characters
  - a type-list continuation that continues no list
  - blank epoch fields on an observation or cycle slip record
- `lintRinexObs` no longer reports `OBS-B11` and reports `OBS-B10` for an event whose header records do not read. `ObservationQcReport.notes` gains the `eventHeaderRecordsUnread` kind.

- **Breaking.** A lint finding's `detail` is a typed `RinexLintFindingDetail`,
  a union on `kind` (`OBS_UNRETAINED_HEADER { label }`,
  `OBS_EPOCH_GAP { gapS, intervalS }` and so on, one member per engine
  variant), where it was the engine's debug text. `lintRinexObs`,
  `lintRinexNav` and the repairs' `remaining` are typed `RinexLintReport`.
- `ObservationQcReport` gains `header` (marker, receiver, antenna,
  approximate position, antenna delta, first and last observation times and
  duration) and `systems` (per-system completeness), and `toJson()` includes
  both.
- `simulateScenarioSet(scenario)` keeps the simulated observation set as a
  `ScenarioSimulation`, with `arrays`, `determinismFingerprintHex`,
  `toRinexObservationFile()`, `toRinexString()` and
  `sppObservationsForEpoch(i)`.

### RINEX clock

- **Breaking.** A parsed `RinexClock` keeps every line of its input. `toRinexString()` on an unedited product restates the input byte for byte, line terminators included. It used to write a three-line header, drop every other header record and every non-`AS` record, and write a `GLO` time system as `UTC`.
- **Breaking.** `parseRinexClock` / `loadRinexClock` read `AR`, `CR`, `DR` and `MS` records and refuse one with an unreadable epoch, bias or sigma or a blank name, where such records were skipped. The first line that does not read throws a `RinexClockParseError` whose `detail` is a `RinexClockErrorDetail`, where it threw a plain `Error`. A value beyond a record's declared count is kept in `surplusValues` instead of refused. `parseRinexClockLossy` keeps unread lines verbatim and reports each in `diagnostics`.
- **Breaking.** `TIME SYSTEM ID` reads `GPS`, `GLO`, `GAL`, `QZS`, `BDS`, `BDT`, `IRN`, `UTC` and `TAI`, with `GLO` read as UTC. A file without the record takes the RINEX clock 3.00 default and reports it in `notices`. An `IRN`, unrecognised or conflicting label leaves the product without a time scale, and `clockS` then throws a `RinexClockQueryError`.
- **Breaking.** `ClockEpoch` is a civil label read in the time scale of the product it is queried against. The constructor accepts `23:59:60` on a day that ends with a positive leap second, and `gpsSeconds` is `number | undefined`, `undefined` for such a label. The second is read as the shortest decimal of the number given with every digit kept, so `59.9999996` no longer rounds into the next minute. Record epochs keep every digit their seconds field states.
- **Breaking.** `series`, `seriesFor` and `sampleCount` include samples in every time scale. `ClockSeries.gpsSeconds` holds `NaN` for a sample off the GPS timeline, where such samples used to be dropped. New `ClockSeries` getters: `hasGpsSeconds`, `jdWhole`, `jdFraction`, `epochs` (`RinexClockInstant[]`), `additionalValues` and `timeScale`.
- `clockSAtGpsSeconds` answers QZSST series and throws a `RinexClockQueryError` outside civil years 1 through 9999. On a UTC product, interpolation across a leap second uses elapsed time.
- **Breaking.** `toRinexString()` refuses rather than rounds, throwing a `RinexClockWriteError`. It refuses a value no 19-column field states exactly, an epoch no microsecond seconds text states exactly, and a scale no RINEX clock time system names, GLONASS system time among them (`UNSUPPORTED_TIME_SCALE`). `toRinexStringWithPolicy(policy?)` returns `{ text, value, departures }`, and `{ nearestMicrosecondEpochs: "allow" }` writes an off-grid epoch at the nearest microsecond and reports it as `EPOCH_AT_NEAREST_MICROSECOND`.
- New views: `version`, `layout` (`"v300"` / `"v304"`), `satelliteSystem`, `timeSystem`, `timeSystemStatus`, `timeScale`, `headerRecords()`, `records()`, `recordCount`, `sourceLine(n)`, `skippedRecords`, `diagnostics` and `notices`.
- New edits: `setTimeSystem`, `setRecordValues`, `insertRecord`, `removeRecord`, `retainRecords` and `editRecords`. Each validates the whole change first; a refused edit throws a `RinexClockEditError` and leaves the product unchanged.
- New builders: `RinexClock.fromClockPoints(timeScale, rows)` and `RinexClock.fromSeriesRows(rows)`, which throw a `RinexClockBuildError`.

- **Breaking.** `ClockEpoch.gpsSeconds` and each sample's `gpsSeconds` are the
  double nearest the GPS second count the civil tag states, rounded once. They
  were rounded at each step, so 2026-05-13 00:00:59.9999996 came out one unit
  in the last place high.

### ANTEX

- **Breaking.** `Antenna.daziDeg`, `zenithStartDeg`, `zenithEndDeg` and `zenithStepDeg` are `number | undefined`, `undefined` when the block has no such record. A blank or non-numeric `DAZI` or `ZEN1 / ZEN2 / DZEN` value is refused as `INVALID_FIELD`, where it used to read as `0`.
- **Breaking.** `Antenna.frequencies` lists labels in file order, once per section. 2.1.1 listed each label once, sorted, with a repeated label's last section winning. `pco`, `pcv` and the new `frequency(label)` throw an `AntexLookupError` with `AMBIGUOUS_FREQUENCY` for a label whose sections differ, and `UNKNOWN_FREQUENCY` for an absent label. `pcv` also refuses a zenith outside the block's grid and an empty grid.
- **Breaking.** `loadAntex` throws an `AntexParseError` whose `detail` is an `AntexErrorDetail`, where it threw a plain `Error`.
- **Breaking.** `VALID FROM` / `VALID UNTIL` are read by their `5I6,F13.7` columns with an exact fraction of the second, so `59.9999999` no longer becomes 59. The following are refused as `INVALID_FIELD`: a blank, short or malformed field, and a second of 60, which GPS time does not have. `new AntexDateTime(...)` throws a `RangeError` for a second of 60 and takes an optional seventh `fractionDigits` argument. New getters `fractionDigits` and `nanosecond`; `validAt` compares every digit.
- **Breaking.** Millimetre fields convert to metres as `mm * 1e-3`, the arithmetic of RTKLIB `readantex`, so about one PCO or PCV value in seven moves by one unit in the last place.
- **Breaking.** A PCV row that would put two values on one zenith is refused with `DEGENERATE_GRID`, and a once-per-block record repeated with different content with `REPEATED_RECORD`. Lines outside any record, a `# OF FREQUENCIES` count that disagrees with the sections, and unclosed blocks or sections are counted in the new `Antex.skippedRecords`. Blank PCV cells are kept at their declared zenith positions.
- **Breaking.** `Antex.toAntexString()` throws an `AntexWriteError` whose `detail` is `{ kind: "UNWRITABLE", field, reason, message }`. It throws for field overflow, precision loss, validity seconds no 13-column form with a decimal point states, a malformed frequency label, sample coordinates the reader could not rebuild, and antenna fields that disagree with the retained validity intervals. It writes every record from a retained value and adds none the source did not carry, apart from block and section start and end records.
- New records and lookups:
  - `Antex.header` (`version`, and `pcvType` with `referenceAntenna`), `outerComments`, `antennaBlocks()`, `blockCount`, `antennaIntervals(id)` and `antennaAt(id, epoch)`
  - `Antenna.leadingComments`, `comments`, `calibrations`, `hasFrequencyCount`, `frequencySections()` and `frequency(label)`

  `Antex.antenna(id)` returns the id's latest block.

### SP3

- **Breaking.** `Sp3.toSp3String()` throws an `Sp3WriteError` whose `detail` is an `Sp3WriteErrorDetail` union on `kind`, where it always returned text. It writes a field only when reading its columns back gives the stored value bit for bit. It writes an epoch record only when that record restates the stored instant.
  - A value that would read back as an absence sentinel is refused (`RECORD_READS_AS_ABSENT`).
  - An integer-nanosecond epoch is refused (`EPOCH_REPRESENTATION_UNSUPPORTED`).
  - A satellite with no `01`..`99` token is refused (`SATELLITE_NOT_REPRESENTABLE`), where a PRN of 0 used to be written as `G00`.
- `Sp3WriteErrorDetail` carries each engine `u64` / `i64` as an exact decimal string beside a `number` that is `null` when not exactly representable: `value` / `valueNumber`, `year` / `yearNumber`, `declared` / `declaredNumber`. The writer restates the data-used and file-type descriptors and `%f` bases the product was read with.
- **Breaking.** `loadSp3` now fails on a non-finite value in the first `%f` line's base fields. A `V` record with no matching `P` record is counted in `skippedRecords` rather than dropped silently. Clock-only records survive reading, writing and merging without creating orbit samples.
- **Breaking.** `Sp3AgreementMetric.positionRmsM` and `positionMaxM` are `number | undefined`, `undefined` for a merged cell that carries a clock and no position. `positionMembers` is 0 for such a cell.
- `Sp3MergeReport` states what the merge did not write: `droppedInputEpochs` (input epochs that took no part, each `Sp3DroppedInputEpoch` with `source`, `epochIndex`, `epochJ2000Seconds` and `reason` `"off_target_grid"` or `"not_on_tick_axis"`), `omittedEpochsJ2000Seconds` (union-grid epochs with no accepted cell, which are not written), `arcWithheld` (cells whose preferred source carried no position, as `Sp3MergeFlag`s) and `clockOmissions` (each source clock left out, each `Sp3ClockOmission` with `epochJ2000Seconds`, `satellite`, `source`, `reason` `"datum_not_observable"`, `"preferred_source_without_clock"` or `"no_consensus"`, `preferred` and `cellHasClock`), with `droppedInputEpochCount`, `arcWithheldCount` and `clockOmissionCount`. The epochs of these lists and of every other merge report entry, of `Sp3.predictionSummary()` and of `Sp3ClockReferenceOffset` are reduced to seconds as the engine reduces an SP3 instant. The binding reduced a Julian date with one more rounding, which lands a whole-second epoch of many days a unit in the last place off its second, and gave `NaN` for an integer-nanosecond epoch.
- `Sp3MergeReport.continuity` returns the merge continuity report, `null` when `verifyContinuity` was not requested: `{ attested, defects, pairsChecked, residualsChecked, residualsSkipped, violations, splices }`. Only window verdicts reached it before, and they carried the splices, not the violations inside one contributor's arc.
- A merge continuity violation carries `sources` and `cells`, each `{ epochJ2000S, role, selection }` with `role` `"held_out"`, `"interpolation_node"`, `"pair_end"` or `"repeated_epoch"` and `selection` a `CellSelection` (`{ kind: "single_source", source }`, `{ kind: "precedence", source, members }` or `{ kind: "combined", rule, members }`) or `undefined`. A `ContinuityDefect` carries every field of its kind under the engine's name beside the summary fields: `epochJ2000S` and `occurrences`; `intervalS`, `displacementM`, `impliedSpeedMS` and `boundMS`; or `epochJ2000S`, `precedingJ2000S`, `residualM`, `toleranceM` and `nodeEpochsJ2000S`.
- `Sp3.selectedNodes(satellite, fromJ2000S, throughJ2000S)` and `Sp3MergeReport.continuitySelectedNodes(satellite, fromJ2000S, throughJ2000S)` return the position nodes the interpolations of a satellite in a window select, for a product and for the merged product a merge continuity report holds (`undefined` when `verifyContinuity` was not requested): the nodes window verdicts read.
- `Sp3MergeReport` carries the engine's whole-product and per-epoch agreement: `positionAgreementRmsM`, `positionAgreementMaxM`, `clockAgreementRmsS`, `clockAgreementMaxS`, `singleSourceFraction` and `perEpochAgreement` (`Sp3EpochAgreement` with `epochJ2000Seconds`, `satellites`, `positionRmsM`, `positionMaxM`, `clockRmsS` and `clockMaxS`).
- `mergeSp3` takes `provenance: "summary" | "full"` and `Sp3MergeReport.provenance` returns the per-epoch provenance the engine recorded, `{ mode, cells, transitions, coverage }`, or `null` when it was not requested.
- **Breaking.** `mergeSp3` refuses an option field it does not know with a `TypeError`, as `sp3MergeInputIdentity` does, where it ignored one.
- **Breaking.** `mergeSp3` takes any `targetEpochIntervalS` that is a whole number of the 10-nanosecond ticks an SP3 interval states, as the engine's merge does, where it threw a `RangeError` for an interval that was not a whole number of seconds. The engine refuses any other interval, and the binding throws its refusal as an `Error`. `sp3MergeInputIdentity` still binds whole seconds only; its refusal of any other interval is now the engine's `Error` ("invalid merged-SP3 policy: target epoch interval") where it was a `RangeError`. A zero, negative or non-finite interval is still a `RangeError`.
- **Breaking.** `Sp3MergeReport.continuityVerdict(fromJ2000S, throughJ2000S)` takes the window alone, as the engine's merge verdict does: the report holds the merged product's interpolation nodes, and a violation influences a window when the nodes its interpolations select include the violation's held-out, repeated or pair-end record or straddle a handover between its records. It took the merged `Sp3` first and read its stencil extent; in a merge whose source takes over twelve epochs before its run ends, single-epoch windows were refused from epoch 51 where the selected nodes accept them through epoch 67.

### Terrain and tides

- **Breaking.** A DTED posting holding the null value is now an unknown elevation. Before, a terrain store returned it as a -32767 m height. A `DtedTerrain` or `MmapTerrain` lookup that gives such a posting nonzero weight throws a `TerrainLookupError` whose `detail` is `UNKNOWN_TERRAIN_ELEVATION` with the tile and zero-based posting indices. A query exactly on a known posting next to a null returns that posting, and a neighbouring tile answers on a shared edge.
- **Breaking.** `DtedTerrain` and `MmapTerrain` lookups (`heightM`, `heightMWithOptions`, `orthometricHeightM*`) throw a `TerrainLookupError` whose `detail` is a `TerrainLookupErrorDetail`, where they threw a plain `Error`. The kinds are `UNKNOWN_TERRAIN_ELEVATION`, `NON_WGS84_TERRAIN_TILE`, `MISSING_TERRAIN_TILE`, `INVALID_INPUT`, `PARSE` and `UNKNOWN`. Failed `heightBatch` / `orthometricHeightBatch` entries gain `detail`. An ellipsoidal lookup whose terrain step fails carries the refusal in `detail.terrain`.
- **Breaking.** `DtedTerrain` refuses a tile whose DSI states a horizontal datum other than WGS84 (`NON_WGS84_TERRAIN_TILE`, with `datum` as a `DtedHorizontalDatum`). `dtedTreeToMmapStore` and `writeDtedTreeToMmapStore` refuse such a tile with `NonWgs84Tile`. A blank datum still reads as WGS84.
- **Breaking.** DTED tiles are checked against their UHL metadata, and a tile whose origin disagrees with its file name is refused; it used to read as sea level. A posting below -16000 m other than the null is read as two's complement, as GDAL reads it. Terrain stores built before this release keep the value decoded at conversion.
- **Breaking.** `MmapTerrain.fromBytes`, `fromVec` and `fromPath` refuse an index record whose tile id lies outside the coordinate domain (`TileIdOutOfRange`). They also refuse one whose bounds are not its one-degree cell's edges (`TileBoundsMismatch`, with `field`).
- Terrain-store bilinear lookup agrees bit for bit with raw DTED lookup west of the prime meridian and south of the equator.
- New `parseOceanLoadingBlqBlock(text)`, `parseOceanLoadingBlqBlocks(text)`, `writeOceanLoadingBlqBlock(block)` and `writeOceanLoadingBlqBlocks(blocks)`. They read and write `{ station, amplitudeM, phaseDeg, comments }` blocks in the standard constituent order. Each retained comment and column-order header line is kept with its `placement` (`beforeStation`, `beforeRow` with `row`, or `afterRows`).
- A BLQ column-order header is one of:
  - a `COLUMN ORDER` declaration
  - a line of labels only
  - a comment in which a word `ORDER` is followed by labels to the end of the line

  A header naming a label outside the eleven constituents, repeating one, or holding another count is refused with a `BlqParseError`.
- The BLQ writer throws a `BlqWriteError` with the zero-based `block` index for a block that would not read back unchanged:
  - a station that is empty, holds a line break or surrounding whitespace, or reads as a comment, header or row
  - a non-finite coefficient
  - a comment that is misplaced or would not read as a comment
  - a comment after the rows of any block but the last

### Fixed

- The 2.1.1 documentation of the `MmapTerrain` lookups said a missing tile
  evaluates to `0.0`. A lookup on a missing tile threw in 2.1.1 and throws now,
  as a `TerrainLookupError` with `MISSING_TERRAIN_TILE`; the documentation says
  so.

## 2.1.1 - 2026-09-22

### Fixed

- Engine update: sidereon 2.1.1 / sidereon-core 2.1.1. CODE predicted
  ionosphere maps resolve to the archive AIUB now serves them from: `cod_prd1`
  is `CODE/IONO/PRD/COD0OPSP0D_<date>0000_01D_01H_GIM.INX.gz` and `cod_prd2`
  is `CODE/IONO/PRD/COD0OPSP1D_<date>0000_01D_01H_GIM.INX.gz`. The
  `CODE/IONO/P1/<year>` and `CODE/IONO/P2/<year>` `COD0OPSPRD` trees they were
  read from stopped receiving issues after 2026-09-21 and are now empty, so
  every predicted-IONEX URL `distributionLocation` and
  `predictedIonexLineCandidates` returned pointed at an object that no longer
  exists. For the dates both layouts carried the objects decompress to the
  same bytes.
- The two predicted lines now carry distinct official filenames, so
  `productIdentity("cod_prd1", ...)` and `productIdentity("cod_prd2", ...)`
  for one map date differ in `officialFilename` as well as in
  `predictionHorizonDays` and `cacheKey`.
- `newestPublishedProduct` and `resolveFirstPublishedPredictedIonex` attribute
  only objects under `CODE/IONO/PRD/` to a predicted line, not the rolling
  copies CODE keeps at the tree root.
- The JavaScript API is unchanged.

## 2.1.0 - 2026-09-05

### Added

- Configurable SP3 coverage-gap interpolation policy (`gapThresholdFactor`, default 1.5):
  - `loadSp3(bytes, gapThresholdFactor?)` loads SP3 products with an explicit gap threshold factor.
  - `Sp3.gapThresholdFactor` reads the active factor; `Sp3.withInterpolationOptions(factor)` returns a copy with the updated policy.
  - `Sp3.checkContinuity(..., gapThresholdFactor?)` and `Sp3.continuityVerdict(..., gapThresholdFactor?)` accept an optional gap threshold factor override.
  - `mergeSp3` accepts `options.verifyContinuity.gapThresholdFactor` to configure continuity verification.
  - `preciseEphemerisSamplesFromSamples(samples, gapThresholdFactor?)` preserves or overrides the factor; `PreciseEphemerisSampleSource.gapThresholdFactor` and `withInterpolationOptions(factor)` inspect and update it.
  - `PreciseEphemerisInterpolant.fromSp3(sp3, gapThresholdFactor?)`, `fromSamples(samples, gapThresholdFactor?)`, and `fromPreciseEphemerisSamples(source, gapThresholdFactor?)` accept an optional factor; `PreciseEphemerisInterpolant.gapThresholdFactor` and `withInterpolationOptions(factor)` inspect and update it.
  - `Sp3.preciseInterpolantArtifactBytes(gapThresholdFactor?)` and `PreciseInterpolantArtifact.gapThresholdFactor` serialize and inspect precomputed artifacts with the interpolation policy.

### Changed

- Engine update: sidereon 2.1.0 / sidereon-core 2.1.0. Additive upstream release: the SP3 coverage-gap threshold is now a validated, product-carried policy (`Sp3InterpolationOptions`, default 1.5 and bit-identical to before), the SP3 window-scoped continuity reach is derived from the interpolator's actual selectable node spans, and RINEX 4 CNAV week/TOW round trips are stable at the week boundary.

## 2.0.0 - 2026-09-03

- **Breaking (upstream):** sidereon 2.0.0 / sidereon-core 2.0.0. Public input
  structs are `#[non_exhaustive]`, so they are built through `Default`/`new`
  plus field assignment, and `terrain`, `ionex::tec_grid` and
  `astro::propagator::dense_output` return typed error enums. Error message
  text is unchanged, so the strings this package surfaces to JavaScript are
  the same.
- The JavaScript API is unchanged.

## 1.4.1 - 2026-08-30

### Changed

- Engine update: sidereon 1.4.1 / sidereon-core 1.4.1 with
  trust-region-least-squares 0.11.0. Portable numerics now cover
  transcendental math, nalgebra decompositions, dynamic products, and fused
  multiply-add, keeping native results bit-identical; fit optimality and
  evaluation counts retain the 1.3.3 results where the fit path is unchanged.

## 1.3.3 - 2026-08-30

### Changed

- Engine update: sidereon 1.3.3 / sidereon-core 1.3.3. Archive-listing parsing
  is no longer quadratic (154 s to 0.23 s on AIUB's ~426k-row listing), and
  transcendental math is bit-identical across x86_64 and arm64. A large
  listing still parses synchronously on the calling thread; run it in a Web
  Worker when the caller must stay responsive. No interface API changes.

## 1.3.1 - 2026-08-29

### Changed

- Engine update: sidereon 1.3.1 / sidereon-core 1.3.1. This release keeps the
  shared release number across the language interfaces, which ships the Go
  interface relicense from Apache-2.0 to MIT. No interface API changes.

## 1.3.0 - 2026-08-29

### Changed

- Engine update: sidereon 1.3.0 / sidereon-core 1.3.0. This release keeps the
  shared release number across the language interfaces, which now include a Go
  interface. No interface API changes.

## 1.2.0 - 2026-08-28

### Added

- Calendar second-of-day and fractional day-of-year helpers, the five
  covariance-6 conversion/interpolation/ECI-RTN helpers, lenient NAV parsing
  with skipped-block diagnostics and arbitrary record-list encoding, RINEX
  observation-code and version-aware frequency/wavelength lookups, and the
  SBAS EMS and RTKLIB text-log parsers.

### Changed

- Engine update: sidereon 1.2.0 / sidereon-core 1.2.0, which corrects lenient
  RINEX 4 CNAV decoding and RTKLIB SBAS wire-form preservation.

## Unreleased

### Added

- Lenient RINEX NAV parsing with core skipped-block diagnostics and standalone
  encoding for arbitrary caller-supplied record lists.
- Direct, RINEX-version-aware observation-code frequency and wavelength
  mappings.
- EMS and RTKLIB SBAS text-log parsers returning timestamped raw message
  blocks with structured decode access.
- Added core-backed `secondOfDay`, `dayOfYear`, and `dataDayOfYear` calendar
  helpers.
- Added core-backed six-by-six covariance unit conversion, PSD interpolation,
  and ECI/RTN transform helpers using flat row-major arrays.

## 1.1.1 - 2026-08-26

### Changed

- Engine update: sidereon 1.1.1 / sidereon-core 1.1.1, the coordination
  release restoring the shared release number across the language interfaces.
  No numerical, algorithmic, or API changes.

### Fixed

- `THIRD-PARTY-NOTICES.md` names the vendored tide-source snapshot
  consistently (sidereon-core 0.36.3, byte-identical to the 0.35.0 sources).
  No API changes.

## 1.1.0 - 2026-08-24

### Added

- `locateSource` accepts `includeInfluence` (default `true`). Setting it to
  `false` skips the per-sensor leave-one-out re-solves and returns an empty
  `perSensorInfluence` array while keeping every other output bit-identical.
- `closedFormInitialGuess` exposes the Schau-Robinson spherical-intersection
  seed. `chanHoInitialGuess` remains available as a deprecated alias.

### Changed

- Source-sensor influence `score` is now
  `max(abs(residualS), abs(leaveOneOutResidualS)) / timingSigmaS`, falling back
  to the full residual when a leave-one-out solve is unavailable. Robust-loss
  downweighting is reported separately in `lossWeight`.

## 1.0.1 - 2026-08-22

### Changed

- engine update: sidereon-core 1.0.1 with trust-region-least-squares 0.10.0 (unified fail-closed HostNumerics backend seam; host power dispatch reproduces NumPy's stride-0 scalar-exponent fast paths bit-for-bit). No interface API changes.

## 1.0.0 - 2026-08-21

Sidereon 1.0.0 across every interface; additions arrive without breaking
existing callers from here.

### Added

- Exact-cache single-flight opens in the browser cache (IndexedDB
  transaction election; liveness rules stay in Rust).
- Window-scoped continuity verdicts (`Sp3.stencilExtent`,
  `continuityVerdict`, merge-report equivalent) and `nextIssueDue`.

### Changed

- Engine pinned to `sidereon-core` 1.0.0.

## 0.39.1 - 2026-08-11

### Fixed

- DTED terrain lookups compute the grid cell and intra-cell fraction in
  exact integer arithmetic (engine fix): the binary64 scaling product
  rounded away up to 4096 ULP of the fraction and could flip a
  coordinate strictly below a posting into the next cell's stencil.
  No API change; heights at dyadic-exact coordinates are byte-identical.

### Changed

- Engine pinned to `sidereon-core` 0.39.1.

## 0.39.0 - 2026-08-10

### Added

- `MmapTerrain.fromPathAttested` / `PreciseInterpolantArtifact` attested
  opens (Node), `digestProvenance` getters, and `verify()` escalation,
  mirroring the engine's attested-open contract. Claims are u64 BigInt;
  malformed claims throw, never silently fall back to hashing.

### Changed

- Engine pinned to `sidereon-core` 0.39.0.

## 0.38.0 - 2026-08-09

### Changed

- Engine pinned to `sidereon-core` 0.38.0.

  The engine's new `mmap` feature is deliberately **not** enabled here:
  memory mapping has no meaning in a WebAssembly sandbox, which has no
  filesystem to map. The wasm interface is unaffected by the change.

## 0.37.0 - 2026-08-09

### Added

- `Sp3.checkContinuity(orbitClass, residualToleranceM)` attests that a
  parsed or merged product is physically continuous, or reports each
  violation with its epochs and magnitude. Two checks with different jobs:
  a physical earth-fixed speed gate whose bound is a true upper bound for
  the orbit class, so it cannot false-positive and catches gross
  corruption; and a hold-out interpolation residual, which supplies the
  sensitivity a speed gate structurally cannot - adjacent GNSS MEO epochs
  are hundreds of kilometres apart, so a metre-scale splice moves the
  implied speed by a fraction of a percent. Reports rather than refuses.

### Changed

- Engine pinned to `sidereon-core` 0.37.0.

## 0.36.2 - 2026-08-04

- Builds against `sidereon` and `sidereon-core` 0.36.3: `parseArchiveListing`
  accepts AIUB whole-tree CSV rows with spaces in unrelated object paths
  instead of rejecting the entire live 426k-row listing over one such row.
  Found by downstream 0.36.1 verification. Vendored tides source relabeled
  to the matching engine version (bytes unchanged).

## 0.36.1 - 2026-08-04

- Builds against `sidereon` and `sidereon-core` 0.36.1 (version-alignment
  engine release; no engine changes), and the vendored IERS-derived tides
  source under `third_party_source/` is relabeled to the matching engine
  version - its bytes are identical to the 0.35.0 copies.

- Merge-provenance identity inputs now accept the `WUM` publisher token and
  the `near_real_time` solution class introduced by core 0.36.0, so
  `wum_nrt` artifacts round-trip through `sp3MergeInputIdentity`. 0.36.0
  rejected them at identity parsing.

## 0.36.0 - 2026-08-04

- Adds the publication-lag resilience surface over core 0.36.0:
  `predictedIonexLineCandidates` (the opt-in CODE `P1`/`P2` cross-line walk
  for one map date - never a neighboring day's map, each candidate keeping
  its own line identity), `parseArchiveListing` (closed dialect detection:
  an unrecognizable listing body throws, never a best-effort empty result;
  `observedAt` is the archive-reported modification text, verbatim),
  `newestPublishedProduct`, `publicationListingUrls` (bounded, at most two
  URLs, newest directory first), `publishedIssueAgeMinutes`, and
  `resolveFirstPublishedPredictedIonex`. Browser and Node callers own the
  fetch itself.
- The Wuhan MGEX near-real-time orbit line (`wum_nrt`, hourly `WUM0MGXNRT`
  02D/05M over anonymous FTP, archive-verified from 2024-07-03) flows
  through the catalog surface, including the `near_real_time` solution
  class and the `WUM` publisher token in merge provenance.
- Builds against `sidereon` and `sidereon-core` 0.36.0. The positioning and
  orbit numerical kernels are unchanged.

## 0.35.0 - 2026-07-24

- RINEX observation QC now treats a source `INTERVAL` of zero as
  standards-compatible unavailable metadata. `lintRinexObs` reports the
  informational `OBS-H19`; default `observationQc` infers cadence from regular
  epochs when possible and otherwise reports an unresolved interval. Explicit
  zero, negative, or non-finite caller overrides remain errors.
- Negative parsed source cadence metadata is reported separately as `OBS-H20`
  and is likewise excluded from QC calculations. Non-finite RINEX text remains
  a parse error; programmatically constructed non-finite headers receive
  `OBS-H20` in the core.
- When interval repair is requested, `repairRinexObs` replaces an unavailable
  source `INTERVAL` with an inferred cadence, or removes the record when
  cadence cannot be resolved.
- Adds `ObservationQcReport.lintFindings`, so QC consumers can inspect the
  compact core lint summary without making a separate lint call.
- Builds against `sidereon` and `sidereon-core` 0.35.0. Positioning and orbit
  propagation numerical kernels are unchanged.

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
