# IONEX surface reference

The IONEX bindings marshal vertical total electron content (VTEC) grid products,
degree-valued ray geometry, composite slant-delay policies, and structured
reader diagnostics between JavaScript/TypeScript and `sidereon-core`.

All modeling lives in the engine: pierce-point thin-shell intersection, bilinear
spatial interpolation, linear-in-time blending between map epochs, the obliquity
factor, and the dispersive group-delay scaling. This module validates inputs,
converts representations, and hands the result back with its status intact.

This document describes the settled 3.0 surface. It does not move the package
version pin.

## 1. Nullable grid cells, no validity masks

- `tecMaps`, `rmsMaps` and `heightMaps` are nested arrays `(number | null)[][][]`
  indexed `[mapIndex][latitudeIndex][longitudeIndex]`.
- A node the product gives as non-available is `null`. It is never coerced to
  `0` or to the file's `9999` sentinel, and no parallel mask is exposed.
- A present value is passed through unchanged: no wrapper scaling, no rounding.
- An optional cube the product does not carry is `null`, which stays distinct
  from a cube that is present with every node `null`. `hasRms` and `hasHeight`
  report the same distinction as booleans.
- `tecSamples()` returns one record per grid node,
  `{ epochJ2000S, latDeg, lonDeg, vtecTecu, rmsTecu?, heightOffsetKm? }`. Every
  node appears, including nodes whose values are all `null`.

### A misspelled property is refused, never dropped

An optional cube or field that the input object does not carry is an absence,
and there is no way to tell that absence apart from a name typed wrongly — so
the names are checked rather than assumed.

- `ionexFromSamples` accepts only the twelve `TecGridSamplesInput` names.
  `heightMap` for `heightMaps` throws a `TypeError` naming the property and
  listing the accepted names, instead of building a product with no height
  cube.
- `ionexFromNodeSamples` requires an array and walks it in index order, checking
  and converting each sample before moving to the next. `rmsTec` for `rmsTecu`
  throws a `TypeError` naming both the property and the index of the sample
  carrying it. Because the walk is sequential, the first row that fails is the
  row reported: a malformed sample at index 2 throws its own refusal even when
  index 5 carries an unknown property, and index 5 is never reached. Nothing is
  reserved up front from the array's stated `length`, so a sparse
  `new Array(100_000_000)` whose element `0` is malformed refuses with that
  catchable `TypeError`; a long array that really does carry its samples is not
  capped.
- `header`, wherever it is supplied, is checked the same way (§2).

The check is against own string property names, enumerable or not
(`Object.getOwnPropertyNames`). Enumerability does not decide whether a name is
a typo — `rmsTec` defined non-enumerably would be dropped just as silently as an
enumerable one — so an unknown own name is refused however it is defined, and
the refusal still names the container and, for a node sample, its index. A name
the container does declare stays usable however it is defined: a non-enumerable
`rmsTecu` is read normally. Symbol-keyed own properties are not string field
names and are not walked at all.

An inherited property is not refused, because these fields are read through the
prototype chain and an object holding its state there is a working input — a
class instance supplying its known fields from a prototype builds. Reading a
property is never absorbed either: an accessor that throws propagates.

Some properties are read by this module itself rather than by the deserializer,
because each carries a distinction the deserializer cannot make:
`header` on `ionexFromSamples`, the `mappingFunction` and `mappingDeclaration`
records on a supplied header (§2), and each axis of a supplied slant policy
(§4). At those reads, what a caller's accessor threw is re-raised *unchanged*
— the same object, with its class, its `message` and any custom properties
intact — rather than restated as a fresh `TypeError`. A `catch` matching on
`instanceof` or on a custom field still matches, and a thrown value that is not
an `Error` at all is not converted into one. The two mapping records are tested
for presence before their value is read, so a `Proxy` whose `has` trap throws
propagates there the same way a getter does.

A refusal this module writes is a different thing from a value a caller threw.
A property that is read intact and is then rejected — an unknown name, a value
that does not deserialize, a type mismatch, two records that contradict each
other — is a `TypeError` raised here with its own text. Only the caller's own
exception passes through untouched.

## 2. Header records

`ionex.header` returns every descriptive record the product carries:

| Property | Type |
| --- | --- |
| `version` | `number` |
| `satelliteSystem` | `string` |
| `program`, `runBy`, `date` | `string` |
| `descriptions`, `comments` | `string[]` |
| `intervalS` | `number` (`0` where the file says the interval may vary) |
| `mappingFunction` | `IonexMappingFunction \| null` |
| `mappingDeclaration` | `IonexMappingDeclaration` |
| `elevationCutoffDeg` | `number` |
| `observablesUsed` | `string` |
| `stationCount`, `satelliteCount`, `mapsInFile` | `number \| null` |

`IonexMappingFunction` is a discriminated union whose tag carries the code the
spec fixes for it:

```ts
{ kind: "NO_MAPPING"; code: "NONE" }
{ kind: "COSZ";       code: "COSZ" }
{ kind: "Q_FACTOR";   code: "QFAC" }
{ kind: "OTHER";      code: string }
```

`IonexMappingDeclaration` names the two cases a file can be in:
`{ kind: "DECLARED", function }` or `{ kind: "ABSENT" }`. A file with no
`MAPPING FUNCTION` record reads as `mappingFunction: null` and
`mappingDeclaration: { kind: "ABSENT" }`; nothing is invented for it, and
`ionexFromNodeSamples` without a header does the same.

### Supplying a header

A constructor reads `mappingFunction` and `mappingDeclaration` presence-aware:
an absent property, an explicit `null`, and a value are three distinct
statements.

- Omitting both states nothing about the mapping function.
- Supplying either alone states it.
- Supplying both means they must agree. `mappingFunction: null` beside
  `mappingDeclaration: { kind: "DECLARED", ... }` is a contradiction and is
  refused, as are `{ kind: "ABSENT" }` or `null` beside a declared function, and
  two declared functions that differ.
- A known tag carries the code the spec fixes for it, so
  `{ kind: "COSZ", code: "MOD" }` is refused rather than having its `code`
  quietly dropped.
- An `OTHER` code is kept byte for byte, leading and trailing blanks included.
- Any own string property other than the fifteen `IonexHeaderInput` names is a
  `TypeError` naming it and listing the accepted names, so `stationCounts` is
  refused rather than read as an absent `stationCount`. Defining it
  non-enumerably does not hide it (§1).

`mappingFunction` and `mappingDeclaration` are the two records read
presence-aware, so each has its own presence test and read against the supplied
object. Both re-raise what the caller threw, unchanged (§1): a getter that
throws, and a `Proxy` `has` trap that throws, both surface the caller's own
value. A record that is read and is then rejected — `{ kind: "COSZ", code: "MOD" }`,
a value that does not deserialize, a contradiction between the two records — is
a `TypeError` written here. A getter returning `undefined` is an absence, and
records supplied on a prototype are read through the chain.

### Raw codes against writer representability

Retention and writability are separate questions. `{ kind: "OTHER", code: "MOD " }`
is kept exactly as supplied and readable back as `"MOD "`, but a
`MAPPING FUNCTION` record would read back trimmed, so `toIonexString()` refuses
it rather than writing a code that would not survive its own reader. A code that
does read back, such as `"MOD"`, round-trips through the text.

## 3. `toIonexString()` is fallible

- Returns `string` on success.
- Throws an `IonexWriterError` where a value or a code cannot be put on a
  standard record as it stands, for example a value no single or per-block
  exponent expresses in an `I5` field.
- The thrown error carries `.detail` of `{ kind: "UNWRITABLE", message }`.

## 4. Slant policies

`IonexSlantPolicy` carries three independent axes:

| Axis | Values | Default |
| --- | --- | --- |
| `coverage` | `"strict"`, `"hold"` | `"strict"` |
| `missingNodes` | `"strict"`, `"renormalize"` | `"strict"` |
| `mapping` | `"declared"`, `"singleLayer"` | `"singleLayer"` |

Constructors: `defaultPolicy()`, `coverageStrict()`, `coverageHold()`,
`missingStrict()`, `missingRenormalize()`, `mappingDeclared()`,
`mappingSingleLayer()`, and `new IonexSlantPolicy(coverage?, missingNodes?, mapping?)`.
Each static constructor sets its own axis and leaves the other two at their
defaults.

Instance modifiers carry the other two axes through: `withCoverage`,
`withCoverageStrict`, `withCoverageHold`, `withMissingNodes`,
`withMissingStrict`, `withMissingRenormalize`, `withMapping`,
`withMappingDeclared`, `withMappingSingleLayer`. So
`IonexSlantPolicy.coverageHold().withMissingRenormalize().withMappingDeclared()`
holds all three.

Anywhere a policy is taken, an `IonexSlantPolicy` instance and a plain
`{ coverage?, missingNodes?, mapping? }` object are interchangeable, and
`missing_nodes` is accepted as an alias of `missingNodes`.

Omitting the argument, or passing `null` or `undefined`, selects the defaults.
Anything else that cannot be read is refused: an unknown axis name, a value
outside an axis, a non-string axis value, a non-object policy, two spellings of
one axis that disagree, or a property whose accessor throws. A supplied policy
never falls back to the default.

The unknown-axis check is against own string property names, enumerable or not
(`Object.getOwnPropertyNames`), the same rule the other supplied objects use
(§1). `missingNode` beside `missingNodes` would leave the strict default in
place and read as a policy that was never asked for, and hiding it from
`Object.keys` would not make it any less of a typo, so it is refused however it
is defined. A name the policy does declare stays usable however it is defined.
Own symbol keys are not string field names and are not walked at all. An
inherited property is not refused: the axes are read through the prototype
chain, which is how an `IonexSlantPolicy` instance — whose own property is the
wasm-bindgen pointer, exempt from the walk — is read, and an ordinary object
holding its axes on a prototype works the same way.

A throwing axis accessor re-raises the caller's value unchanged (§1) through
every entry that takes a policy object: `slantDelayWithPolicy`,
`slantDelaysBatchResults` and `ionexSlantDelayResults`. A value that is read and
is then rejected — a non-string axis, a name outside the axis, two spellings
that disagree — is a `TypeError` written here.

## 5. Results and the engine's validity rule

`slantDelayWithPolicy` and `slantDelaysBatchResults` return a structured
evaluation:

- `delayM`: positive group delay in metres.
- `status.held`: `IonexCoverageError | null`, the coverage miss `"hold"` held
  the value through.
- `status.degraded`: `IonexNodeGap | null`, the non-available nodes
  `"renormalize"` interpolated around.
- `status.assumedMapping`: `IonexAssumedMapping | null`, naming what a product
  declaring anything other than `COSZ` declares, where the single-layer
  `1/cos(z')` was applied anyway.
- `status.isValid`: `held === null && degraded === null`. **An assumed mapping
  alone leaves a value valid**, which is the engine's own rule: most published
  global products declare `NONE` while their descriptions name the mapping
  function their maps were determined with.
- `status.isNominal`: all three of `held`, `degraded` and `assumedMapping` are
  `null`.
- `status.isHeld`, `isDegraded`, `isAssumedMapping`: the same three as booleans.

The three coexist: one result can be held, degraded and mapped by assumption at
once.

## 6. Typed refusals

A scalar query throws a typed error carrying `.detail`; a batch row carries the
same object as `refusal`. Both are a discriminated union on `kind`, and each
variant carries only its own fields:

| `kind` | Payload |
| --- | --- |
| `COVERAGE` | `coverageError`, `message` |
| `MISSING_NODES` | `nodeGap`, `message` |
| `VARYING_HEIGHTS` | `mapNumber`, `latIndex`, `lonIndex`, `message` |
| `HEIGHT_NOT_AVAILABLE` | `mapNumber`, `latIndex`, `lonIndex`, `message` |
| `MAPPING_FUNCTION` | `mappingDeclaration`, `message` |
| `INVALID_INPUT` | `message` |
| `UNKNOWN` | `message` |

`nodeGap` carries `earlier` and `later`, each `null` or
`{ mapNumber, latIndex, lonIndex, lonIndexNext, missing }`, where `missing` is
the four-boolean cell corner order `E00`, `E01`, `E10`, `E11`.

`UNKNOWN` is reached only by an engine variant this binding does not yet name,
and carries the engine's message in full. It is never folded into a known
category and never reported as a success.

## 7. Reader warnings

`loadIonexWithWarnings`, `Ionex.parseWithWarnings` and
`Ionex.parseStrWithWarnings` return `{ ionex, value, warnings }`, where `ionex`
and `value` are the same product and `warnings` keeps the reader's own order.
All seven engine variants are preserved, each with its own payload:

1. `MISSING_RECORD` — `label`, `message`. Several are legitimate for one file,
   so select on `kind` and `label` together.
2. `VERSION_RECORD_NOT_FIRST` — `line`, `message`.
3. `EPOCH_MISMATCH` — `label`, `line`, `declaredEpoch`, `mapsEpoch`, `message`.
4. `MAP_COUNT_MISMATCH` — `line`, `declaredCount`, `declaredCountNumber`,
   `tecMaps`, `allMaps`, `message`.
5. `NOT_A_NUMBER_VALUE` — `dataKind`, `mapNumber`, `line`, `latDeg`, `lonDeg`,
   `message`.
6. `INTERVAL_MISMATCH` — `line`, `declaredS`, `mapNumber`, `spacingS`,
   `spacingSNumber`, `message`.
7. `EXPONENT_CARRIED_INTO_MAP` — `dataKind`, `mapNumber`, `line`, `exponent`,
   `setByLine`, `message`.

No property carries two different types across variants: a diagnostic epoch is
always `declaredEpoch` / `mapsEpoch`, and a count is always `declaredCount`.

### Diagnostic precision

`IonexDiagnosticEpoch` keeps what the engine holds rather than forcing a warning
epoch through an integer constructor:

- `scale`, `jdWhole`, `fraction`, `jd` for the split Julian date.
- `nanos`: exact integer nanoseconds as a decimal string, where the engine holds
  the epoch in that representation.
- `j2000Seconds`: exact whole J2000 seconds as a decimal string, where the epoch
  falls on a whole second.
- `j2000SecondsNumber`: the same value as a `number`, `null` where it is not
  exactly representable as one.
- `j2000SecondsF64`: the floating-point projection, which a fractional epoch has.

The engine carries `MAP_COUNT_MISMATCH.declared` as a `u64` and
`INTERVAL_MISMATCH.spacing_s` as an `i64`, neither of which a JavaScript
`number` holds exactly past 2^53. Both are given as an exact decimal string
(`declaredCount`, `spacingS`) with a `number` beside them
(`declaredCountNumber`, `spacingSNumber`) that is `null` where the conversion
would lose digits.

## 8. Batch evaluation

`slantDelaysBatchResults(requests, policy?)` and the top-level
`ionexSlantDelayResults(ionex, requests, policy?)` return one typed row per
request, in request order:

```ts
{ index: number; isOk: true;  evaluation: IonexSlantDelayEvaluation; refusal: null }
{ index: number; isOk: false; evaluation: null; refusal: IonexSlantRefusal }
```

- Each row carries its own `index`, and the result length always equals the
  request length. No row is dropped.
- A malformed row — not an object, missing a field, a non-finite coordinate, a
  non-positive frequency, a receiver latitude outside `[-90, 90]`, a fractional
  query second — becomes an `INVALID_INPUT` row of its own. The rows after it
  are still evaluated.
- A request container that is not an array is refused outright, as a thrown
  `TypeError`, rather than being read as a single row.
- Degrees reach the engine as one multiply by `pi/180`, the same operation order
  the scalar entry uses, so a batch row and the scalar call agree bit for bit.

### Integer query seconds

`epochJ2000S` is a whole number of seconds since J2000. It must be finite,
integral, and within the JavaScript safe-integer range
`[-9007199254740991, 9007199254740991]`; anything outside is refused rather than
cast. That bound is the range a `number` carries integers exactly in. It is
narrower than `i64`, and deliberately so: `i64::MAX as f64` is 2^63, one past the
largest `i64`, so a bound written against it accepts a value that then wraps.

## 9. Standalone regular grid

`TecGrid` interpolates a rectilinear grid over epoch, latitude and longitude:

- `new TecGrid(epochsNs, latitudesDeg, longitudesDeg, values)` — three strictly
  increasing axes and flat values in epoch-latitude-longitude order with
  longitude varying fastest, `null` marking a node without a value. Each axis
  takes a `number[]` or a `Float64Array`; `values` takes `(number | null)[]`.
- `epochsNs`, `latitudesDeg`, `longitudesDeg` — the stored axes, each returned
  as a `Float64Array`. The three axes are `Vec<f64>` in the engine and carry no
  nulls, so the binding returns them as the typed array rather than copying
  them into an ordinary one.
- `values` — the stored flat values, returned as an ordinary
  `(number | null)[]`. It is the one of the four that can hold `null`, which no
  typed array expresses.
- `vtecAtPiercePoint(epoch, lonDeg, latDeg)` — refuses a query that weights a
  node holding no value.
- `vtecAtPiercePointWithPolicy(epoch, lonDeg, latDeg, policy?)` — returns
  `{ value, degraded }` under `"strict"` or `"renormalize"`.
- `TecGridShellGeometry`, `TecGridEvalOptions` — shell radius/height and the
  epoch, elevation floor, NaN fallback height, carrier frequency and shell the
  ECEF entries evaluate under.
- `ionoDelayXyz` / `ionoDelayXyzWithPolicy` — group delay for an ECEF
  satellite/receiver pair.
- `tecXyz` — vertical and slant TEC for the same pair, as a two-element
  `Float64Array` holding `[vtecTecu, stecTecu]`.
- `tecXyzWithPolicy` — the same pair under an explicit missing-node policy,
  as `TecGridEvaluation<[number, number]>`: the `value` there is an ordinary
  array, because it crosses inside a serialized result object rather than as a
  bare `Vec<f64>` return.

A `TecGridError` thrown from any of these carries `.detail` as a discriminated
union on `kind`: `AXES_TOO_SHORT`, `AXES_NOT_INCREASING`, `DIMENSIONS_OVERFLOW`,
`VALUE_COUNT_MISMATCH`, `INVALID_FIELD`, `NODES_NOT_AVAILABLE`, `OUT_OF_BOUNDS`,
or `UNKNOWN` for a variant this binding does not yet name.

### Two different time precisions, deliberately

These are not the same and the difference is in the engine, not in this binding:

- **`TecGridEpoch` is exact.** `unixNanos` is a signed 64-bit nanosecond count.
  The constructor takes a `bigint`, a decimal string, or a `number` that is a
  safe integer, and refuses anything else: a present-day timestamp is about
  1.8e18 ns, two hundred times `Number.MAX_SAFE_INTEGER`, so a `number` cannot
  carry one without losing its last digits. A value outside `i64` is refused
  rather than wrapped. The getter returns a `bigint`, and `unixNanosString`
  gives the same value as a decimal string. `dayOfYear` is checked as a whole
  number in `[0, 65535]` before it is narrowed, so `65536` is an error rather
  than `0`.
- **The `TecGrid` epoch axis is `f64`.** That is the engine's own contract for
  this grid type: `epochsNs` is a `Float64Array` of Unix nanoseconds, and
  adjacent nanoseconds are not distinguishable at present-day magnitudes. A
  query converts the exact `TecGridEpoch` to that axis coordinate on the way in.

These two are not conflated anywhere: `TecGridEpoch` counts Unix nanoseconds,
while the IONEX product surface counts whole J2000 seconds as `epochJ2000S`.
Neither is widened to meet the other.

### The coordinate callback

`ionoDelayXyz`, `ionoDelayXyzWithPolicy`, `tecXyz` and `tecXyzWithPolicy` take an
optional `ecefToLla` callback, `(xyz: number[]) => [lonDeg, latDeg, altM]`, with
`xyz` in metres. Omitting it uses the engine's own WGS84 conversion, whose
failure is propagated rather than absorbed.

Three outcomes are kept apart:

- **A returned NaN component** is the engine's documented marker for "no pierce
  point": the evaluation falls back to the receiver position and the configured
  fallback altitude, and returns a value.
- **A thrown value** is not that marker. The first value thrown is captured and
  re-raised unchanged — the same object identity the callback threw — after the
  engine call returns. A thrown error never comes back as a delay, and never as
  the engine's own NaN fallback.
- **A malformed return** — not an array, not exactly three elements, or an
  element that is not a number — is a `TypeError` naming what was wrong.

## 10. TypeScript

The IONEX declarations are generated from the `typescript_custom_section` in
`src/ionex.rs`, so `wasm-pack` writes them into both `pkg/sidereon.d.ts` and
`pkg-node/sidereon.d.ts`, and the `unchecked_return_type` /
`unchecked_param_type` attributes on the bindings resolve against them. Every
public boundary that crosses as a `JsValue` — constructors, getters, methods,
free functions, inputs and error details — names a precise type rather than
being inferred as `any`.

`types/sidereon-extra.d.ts`, reachable as `@neilberkman/sidereon/types`,
re-exports those same declarations rather than restating them, so the two cannot
drift apart.

## 11. Methods not bound here

Kept out of this module on purpose, with the reason:

- `TecGrid::interpolate_vtec` and `interpolate_vtec_with_policy` are
  crate-private in the engine. `vtecAtPiercePoint` and
  `vtecAtPiercePointWithPolicy` are the public path to the same interpolation.
- `klobuchar`, `klobuchar_native`, `galileo_nequick_g_native` and
  `galileo_effective_ionisation_level` are broadcast-ephemeris ionosphere models
  owned by `src/ionosphere.rs`, not the IONEX grid path.
- `ionex_slant_delay` and `ionex_slant_delays` in the engine are the fixed-policy
  forms of the entries bound here; `slantDelay` and `slantDelaysBatchResults`
  reach the same kernels with the default policy.
