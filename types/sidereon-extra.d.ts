// Hand-written companion types for the shapes wasm-bindgen exposes as `any`.
//
// `Sp3.solveSpp(request)` takes a plain object; wasm-bindgen types its argument
// as `any` because it deserializes through serde. This is the precise shape it
// accepts. Import it alongside the generated types:
//
//   import { loadSp3 } from "@neilberkman/sidereon";
//   import type { SppRequest } from "@neilberkman/sidereon/types";

/** One pseudorange observation for an SPP solve. */
export interface SppObservation {
  /** RINEX/IGS satellite token, e.g. "G01", "E12", "C08". */
  satelliteId: string;
  /** Pseudorange in metres. */
  pseudorangeM: number;
}

/** Which atmospheric corrections to apply. Both default to false. */
export interface SppCorrections {
  ionosphere?: boolean;
  troposphere?: boolean;
}

/** GPS Klobuchar ionosphere coefficients (each a 4-element tuple). */
export interface SppKlobuchar {
  alpha?: [number, number, number, number];
  beta?: [number, number, number, number];
}

/** Surface meteorology for the troposphere model. */
export interface SppSurfaceMet {
  pressureHpa: number;
  temperatureK: number;
  relativeHumidity: number;
}

/** Opt-in Huber/IRLS robust-reweighting tuning. Including the `robust` key on an
 * SPP request (even as `{}`) enables the engine outer reweighting loop on top of
 * the static elevation weighting; every field is optional and falls back to the
 * engine default. Omitting `robust` runs the static reference solve unchanged. */
export interface SppRobust {
  /** Huber tuning constant `k`; residuals scaled below this keep full weight.
   * Must be finite and positive. Defaults to the engine constant (~1.345). */
  huberK?: number;
  /** Floor (metres) on the MAD scale, so a near-perfect fit cannot down-weight
   * every satellite. Must be finite and positive. Engine default applies. */
  scaleFloorM?: number;
  /** Maximum total outer solves (the warm start plus reweighted resolves). Must
   * be at least 1. Engine default applies. */
  maxOuter?: number;
  /** Outer-loop position L2 step tolerance (metres). Must be finite and
   * non-negative. Engine default applies. */
  outerTolM?: number;
}

/** The object passed to `Sp3.solveSpp`. */
export interface SppRequest {
  /** Pseudorange observations; at least one is required. */
  observations: SppObservation[];
  /** Receive epoch, seconds since J2000 in the ephemeris time scale. */
  tRxJ2000S: number;
  /** Receive second-of-day, seconds. */
  tRxSecondOfDayS: number;
  /** Day of year. */
  dayOfYear: number;
  /** Initial state guess [x, y, z, clock]; defaults to [0, 0, 0, 0]. */
  initialGuess?: [number, number, number, number];
  corrections?: SppCorrections;
  klobuchar?: SppKlobuchar;
  met?: SppSurfaceMet;
  /**
   * GLONASS FDMA channel numbers as `[slot, channel]` pairs, e.g.
   * `[[1, 1], [2, -4]]`. `slot` is the GLONASS satellite slot/PRN and `channel`
   * its FDMA frequency channel `k` (valid `[-7, +6]`). GLONASS is FDMA, so its
   * per-satellite carrier is resolved from this map to scale the L1 Klobuchar
   * ionosphere delay by `(f_L1 / f_k)^2`. Omit (or pass `[]`) when there is no
   * GLONASS observation; every other constellation is unaffected. A GLONASS
   * observation solved with `corrections.ionosphere` on but no entry here (or a
   * channel outside `[-7, +6]`) is rejected with an `Error`. Channels are
   * available from broadcast nav (`GlonassRecordJs.freqChannel`) or a RINEX obs
   * header (`ObsHeader.glonassSlots`).
   */
  glonassChannels?: [number, number][];
  /** Populate WGS84 lat/lon/height in the result. Defaults to true. */
  withGeodetic?: boolean;
  /** Opt-in Huber/IRLS robust reweighting. Omit for the static
   * elevation-weighted reference solve; include (even as `{}`) to route through
   * the engine outer reweighting loop. Honored on the SP3, broadcast-only, and
   * fallback paths, since it is a property of the solve inputs. */
  robust?: SppRobust;
  /** Cold-start convergence-basin widening: the number of near-surface
   * golden-spiral seeds the engine tries (plus `initialGuess`), selecting the
   * best redundant converged fix. Must be at least 1. Omit for the single exact
   * solve from `initialGuess`. Honored only by `Sp3.solveSpp` (the policy-bearing
   * path), not the broadcast-only or fallback paths. */
  coarseSearchSeeds?: number;
  /** Optional positive PDOP ceiling: a fix whose geometry is rank-deficient or
   * exceeds this ceiling is refused with an `Error`. Honored only by
   * `Sp3.solveSpp`. */
  maxPdop?: number;
}

/** Shared options for `Sp3.solveSppBatch(epochs, options)`, applied to every
 * epoch of the batch. The batch shares one `withGeodetic` flag and one solve
 * policy across all epochs (each epoch entry is itself an `SppRequest`, but its
 * own `withGeodetic` / `maxPdop` / `coarseSearchSeeds` are ignored in favour of
 * these). Every field is optional. */
export interface SppBatchOptions {
  /** Populate WGS84 lat/lon/height in each epoch's result. Defaults to true. */
  withGeodetic?: boolean;
  /** Cold-start convergence-basin widening applied to every epoch. Must be at
   * least 1. Omit for the single exact solve from each epoch's `initialGuess`. */
  coarseSearchSeeds?: number;
  /** Positive PDOP ceiling applied to every epoch; a fix that exceeds it (or is
   * rank-deficient) becomes that epoch's error. */
  maxPdop?: number;
}

/** Serialized observability tier label. Handle getters return the generated
 * `ObservabilityTier` enum; serde-returning APIs use these stable labels. */
export type ObservabilityTierLabel = "RankDeficient" | "ZeroRedundancy" | "Weak" | "Nominal";

/** Geometry observability and covariance-validation diagnostics.
 * `ZeroRedundancy` marks a full-rank design with no residual degrees of freedom,
 * so snapshot covariance bounds are unvalidated unless a propagated prior is
 * present. `Weak` means a condition-number or GDOP cutoff was exceeded; the
 * returned bounds are reported as computed and are not clamped. */
export interface GeometryQualityObject {
  tier: ObservabilityTierLabel;
  /** Observation redundancy, `nObs - nParams`. */
  redundancy: number;
  /** Rank of the design matrix used by the solve. */
  rank: number;
  /** Singular-value condition number of the design matrix. */
  conditionNumber: number;
  /** Geometric dilution of precision for the solved state. */
  gdop: number;
  /** Whether residual-based RAIM can test the solve. */
  raimCheckable: boolean;
  /** Whether residuals or a propagated prior validated the covariance bound. */
  covarianceValidated: boolean;
}

// --- Source localization ----------------------------------------------------
//
// Source-localization functions deserialize inputs and serialize outputs through
// serde, so wasm-bindgen types them as `any`. Import:
//
//   import { locateSource } from "@neilberkman/sidereon";
//   import type { SourceSensor, SourceLocateOptions, SourceSolution } from "@neilberkman/sidereon/types";

/** One source-localization sensor. Coordinates are caller-chosen Cartesian metres. */
export interface SourceSensor {
  positionM: number[];
  /** Per-sensor propagation speed, metres per second. */
  propagationSpeedMS?: number;
}

/** Source solve mode selector. */
export type SourceSolveMode = "toa" | { mode: "tdoa"; referenceSensor: number };

/** Options for `locateSource`. */
export interface SourceLocateOptions {
  mode?: "toa" | "tdoa";
  referenceSensor?: number;
  timingSigmaS?: number;
  loss?: "linear" | "softL1" | "soft_l1" | "huber" | "cauchy" | "arctan";
  fScaleS?: number;
  ftol?: number;
  xtol?: number;
  gtol?: number;
  maxNfev?: number;
}

/** Closed-form seed used by source localization. */
export interface SourceInitialGuess {
  positionM: number[];
  originTimeS?: number;
  residualRmsS: number;
}

/** One source-localization residual row. */
export interface SourceResidual {
  sensorIndex: number;
  referenceSensorIndex?: number;
  residualS: number;
}

/** Per-sensor source-localization influence diagnostic. */
export interface SourceSensorInfluence {
  sensorIndex: number;
  residualS: number;
  leaveOneOutResidualS?: number;
  positionDeltaM?: number;
  originTimeDeltaS?: number;
  lossWeight: number;
  score: number;
}

/** Source-localization covariance. */
export interface SourceCovariance {
  state: number[][];
  positionM2: number[][];
  originTimeS2?: number;
  timingSigmaS: number;
}

/** Source solution returned by `locateSource`. */
export interface SourceSolution {
  positionM: number[];
  originTimeS?: number;
  covariance?: SourceCovariance;
  residuals: SourceResidual[];
  perSensorInfluence: SourceSensorInfluence[];
  geometryQuality: GeometryQualityObject;
  initialGuess: SourceInitialGuess;
  status: number;
  nfev: number;
  njev: number;
  cost: number;
  optimality: number;
}

/** DOP scalars returned by `sourceDop` and nested in `sourceCrlb`. */
export interface SourceDop {
  gdop: number;
  pdop: number;
  hdop: number;
  vdop: number;
  tdop: number;
  systemTdops: { system: string; tdop: number }[];
}

/** Cramer-Rao lower bound returned by `sourceCrlb`. */
export interface SourceCrlb {
  dop: SourceDop;
  covariance: SourceCovariance;
}

// --- Observable-domain plain-object inputs ----------------------------------
//
// The wasm-bindgen surface types these arguments as `any` because they cross
// the boundary through serde. These are the precise shapes accepted. Import:
//
//   import { detectCycleSlips } from "@neilberkman/sidereon";
//   import type { ArcEpoch } from "@neilberkman/sidereon/types";

/** One epoch in a single-satellite carrier-phase arc. Phases are cycles, code
 * values metres, carrier frequencies hertz, `gapTimeS` any comparable seconds
 * coordinate. Any field may be omitted. */
export interface ArcEpoch {
  phi1Cycles?: number;
  phi2Cycles?: number;
  p1M?: number;
  p2M?: number;
  lli1?: number;
  lli2?: number;
  f1Hz?: number;
  f2Hz?: number;
  gapTimeS?: number;
}

/** Options controlling carrier-phase cycle-slip classification. */
export interface CycleSlipOptions {
  /** Geometry-free step threshold, metres. Defaults to 0.05. */
  gfThresholdM?: number;
  /** Melbourne-Wubbena step threshold, wide-lane cycles. Defaults to 4.0. */
  mwThresholdCycles?: number;
  /** Data-gap threshold, seconds. Defaults to 300.0. */
  minArcGapS?: number;
}

/** One satellite observation for a receiver velocity solve. */
export interface VelocityObservation {
  /** RINEX satellite token, e.g. "G07". */
  satelliteId: string;
  /** Pseudorange rate (m/s) for range-rate solves, or Doppler (Hz) for Doppler. */
  value: number;
  /** Carrier frequency, hertz, used for Doppler conversion. */
  carrierHz: number;
  /** Satellite clock drift, seconds per second. Defaults to 0. */
  satClockDriftSS?: number;
}

/** Options controlling receiver velocity solving. */
export interface VelocitySolveOptions {
  /** Observation value convention. Defaults to "range_rate". */
  observable?: "range_rate" | "doppler";
  /** Apply fixed-point light-time correction. Defaults to true. */
  lightTime?: boolean;
  /** Apply Earth-rotation Sagnac correction. Defaults to true. */
  sagnac?: boolean;
}

/** One satellite/elevation row for sigma or weight construction. */
export interface WeightEntry {
  satelliteId: string;
  /** Topocentric elevation, degrees. */
  elevationDeg: number;
  /** Optional carrier-to-noise density, dB-Hz. */
  cn0Dbhz?: number;
}

/** Options for pseudorange variance weighting. */
export interface PseudorangeVarianceOptions {
  /** Zenith-floor term, metres. Defaults to 0.3. */
  aM?: number;
  /** Elevation-scaled term, metres. Defaults to 0.3. */
  bM?: number;
  /** Variance model. Defaults to "elevation". */
  model?: "elevation" | "elevation_cn0";
  /** Carrier-to-noise density, dB-Hz (required for "elevation_cn0"). */
  cn0Dbhz?: number;
  /** C/N0 variance scale, square metres. Defaults to 1.0. */
  cn0ScaleM2?: number;
}

/** One satellite-keyed pseudorange for ionosphere-free combination. */
export interface PseudorangeObservation {
  satelliteId: string;
  /** Pseudorange, metres. */
  valueM: number;
}

/** A per-constellation RINEX band override for pseudorange combination. */
export interface PseudorangeBandOverride {
  /** Single RINEX system character, e.g. "G". */
  system: string;
  /** Band-1 RINEX observation code. */
  band1: string;
  /** Band-2 RINEX observation code. */
  band2: string;
}

/** GPS C/A replica generation options. */
export interface ReplicaOptions {
  /** Sampling rate, hertz. Defaults to 2_046_000. */
  sampleRateHz?: number;
  /** Output sample count. Defaults to 2046. */
  numSamples?: number;
  /** Initial C/A code phase, chips. Defaults to 0. */
  codePhaseChips?: number;
  /** Code-rate Doppler, hertz. Defaults to 0. */
  codeDopplerHz?: number;
}

/** Coherent GPS C/A correlation options. */
export interface CorrelateOptions {
  sampleRateHz?: number;
  dopplerHz?: number;
  codePhaseChips?: number;
  codeDopplerHz?: number;
}

/** GPS C/A acquisition search options. */
export interface AcquisitionOptions {
  sampleRateHz?: number;
  dopplerMinHz?: number;
  dopplerMaxHz?: number;
  dopplerStepHz?: number;
}

// --- Orbit propagation ------------------------------------------------------
//
// `propagateState(request)` deserializes its argument through serde, so
// wasm-bindgen types it as `any`. This is the precise shape it accepts. Import:
//
//   import { propagateState } from "@neilberkman/sidereon";
//   import type { PropagateStateRequest } from "@neilberkman/sidereon/types";

/** Space-weather constants for a drag request. */
export interface SpaceWeatherConfig {
  f107?: number;
  f107a?: number;
  ap?: number;
}

/** Atmospheric drag parameterization. */
export interface DragConfig {
  bcFactorM2Kg?: number;
  ballisticCoefficientKgM2?: number;
  cd?: number;
  areaM2?: number;
  massKg?: number;
  cutoffAltitudeKm?: number;
  spaceWeather?: SpaceWeatherConfig;
}

/** Cannonball solar-radiation-pressure parameters. */
export interface SolarRadiationPressureConfig {
  cr: number;
  areaToMassM2Kg?: number;
  areaM2?: number;
  massKg?: number;
  pressureNM2?: number;
  auKm?: number;
}

/** Zonal harmonic gravity selector. */
export interface ZonalForceConfig {
  maxDegree?: 2 | 3 | 4 | 5 | 6;
  j2?: boolean;
  j3?: boolean;
  j4?: boolean;
  j5?: boolean;
  j6?: boolean;
  muKm3S2?: number;
  reKm?: number;
  coefficients?: {
    j2?: number;
    j3?: number;
    j4?: number;
    j5?: number;
    j6?: number;
  };
}

/** Sun/Moon third-body force selector. */
export interface ThirdBodyForceConfig {
  sun?: boolean;
  moon?: boolean;
  gmSunKm3S2?: number;
  gmMoonKm3S2?: number;
}

/** Schwarzschild relativity force selector. */
export interface RelativityForceConfig {
  muKm3S2?: number;
  cKmS?: number;
}

/** Additive numerical force-model composition. */
export interface CompositeForceModelConfig {
  kind: "composite";
  twoBody?: boolean;
  twoBodyMuKm3S2?: number;
  muKm3S2?: number;
  zonal?: boolean | "none" | "j2" | "j2_j6" | "j2ThroughJ6" | ZonalForceConfig;
  thirdBody?: boolean | "none" | "sun" | "moon" | "sun_moon" | "sunMoon" | ThirdBodyForceConfig;
  solarRadiationPressure?: false | "none" | SolarRadiationPressureConfig;
  srp?: false | "none" | SolarRadiationPressureConfig;
  relativity?: boolean | "none" | "schwarzschild" | RelativityForceConfig;
}

/** Canonical Earth Phase A perturbation set. */
export interface EarthPhaseAForceModelConfig {
  kind: "earth_phase_a" | "earthPhaseA";
  solarRadiationPressure?: false | "none" | SolarRadiationPressureConfig;
  srp?: false | "none" | SolarRadiationPressureConfig;
}

/** Numerical state-propagation force model selector. */
export type ForceModel =
  | "two_body"
  | "two_body_j2"
  | "composite"
  | "earth_phase_a"
  | CompositeForceModelConfig
  | EarthPhaseAForceModelConfig;

/** Numerical state-propagation integrator selector. */
export type Integrator = "dp54" | "rk4";

/** The object passed to `propagateState`. Position/velocity are length-3,
 * km / km/s; `timesS` are absolute TDB epochs (seconds). */
export interface PropagateStateRequest {
  /** Initial-state epoch, TDB seconds. */
  epochS: number;
  /** Initial ECI position [x, y, z], km. */
  positionKm: [number, number, number];
  /** Initial ECI velocity [vx, vy, vz], km/s. */
  velocityKmS: [number, number, number];
  /** Output sample epochs, TDB seconds, monotonic in the propagation direction. */
  timesS: number[];
  /** Force model. Defaults to "two_body". */
  forceModel?: ForceModel;
  /** Integrator. Defaults to "dp54". */
  integrator?: Integrator;
  /** Absolute tolerance (DP54). Defaults to 1e-9. */
  absTol?: number;
  /** Relative tolerance (DP54). Defaults to 1e-12. */
  relTol?: number;
  /** Initial step, seconds. Defaults to 60. Must be positive. */
  initialStepS?: number;
  /** Minimum step, seconds. Defaults to 1e-6. */
  minStepS?: number;
  /** Maximum step, seconds. Defaults to 3600. */
  maxStepS?: number;
  /** Maximum integrator steps. Defaults to 1_000_000. */
  maxSteps?: number;
  /** Gravitational parameter, km^3/s^2. Defaults to Earth's MU_EARTH. */
  muKm3S2?: number;
  /** Atmospheric drag layered on the selected force model. */
  drag?: DragConfig;
}

// --- 0.15 capability payloads ----------------------------------------------

/** Sidereal filter template selector. */
export type SiderealTemplateMethod =
  "mean" | "robustMad" | "robust_mad" | { method: "ewma"; alpha: number };

/** Options for `siderealFilter(series, periodS, options)`. */
export interface SiderealFilterOptions {
  sampleIntervalS?: number;
  priorPeriods?: number;
  minCoverage?: number;
  templateMethod?: SiderealTemplateMethod;
}

/** Result returned by `siderealFilter`. */
export interface SiderealFilterOutput {
  filtered: number[];
  template: number[];
  coverage: number[];
  underCovered: boolean[];
}

/** One period score returned by `periodicityStrength`. */
export interface PeriodicityStrength {
  periodS: number;
  strength: number;
}

/** WGS84 geodetic coordinate in radians and metres. */
export interface GeodeticCoordinate {
  latRad: number;
  lonRad: number;
  heightM?: number;
}

/** One geodetic time-series sample. */
export interface PositionTimeSeriesSample {
  epochYear: number;
  positionM: [number, number, number];
  covarianceM2?: [[number, number, number], [number, number, number], [number, number, number]];
}

/** Position-series frame selector. */
export type PositionSeriesFrame =
  "enu" | { kind: "enu" } | { kind: "ecef"; reference: GeodeticCoordinate };

/** Station position time series passed to geodetic time-series functions. */
export interface PositionTimeSeries {
  samples: PositionTimeSeriesSample[];
  frame?: PositionSeriesFrame;
}

/** MIDAS velocity options. */
export interface MidasOptions {
  dominantPeriodYears?: number;
  periodToleranceYears?: number;
  minPairs?: number;
}

/** One MIDAS component diagnostic. */
export interface MidasComponentStats {
  pairCount: number;
  retainedPairCount: number;
  slopeSigmaMPerYr: number;
  effectivePairCount: number;
}

/** Result returned by `velocityMidas`. */
export interface MidasVelocity {
  rateEnuMPerYr: [number, number, number];
  sigmaEnuMPerYr: [number, number, number];
  covarianceEnuM2PerYr2: [
    [number, number, number],
    [number, number, number],
    [number, number, number],
  ];
  componentStats: [MidasComponentStats, MidasComponentStats, MidasComponentStats];
  sampleCount: number;
  spanYears: number;
  quality: "nominal" | "shortSpan";
}

/** Trajectory model controls for `fitTrajectory`. */
export interface TrajectoryModelOptions {
  referenceEpochYear?: number;
  includeAnnual?: boolean;
  includeSemiannual?: boolean;
  offsetEpochsYear?: number[];
}

/** Trajectory fit options. */
export interface TrajectoryFitOptions {
  loss?: "linear" | "softL1" | "soft_l1" | "huber" | "cauchy" | "arctan";
  fScaleM?: number;
  maxNfev?: number;
}

/** One term in a fitted trajectory model. */
export interface TrajectoryTerm {
  kind:
    | "position"
    | "velocity"
    | "annualSin"
    | "annualCos"
    | "semiannualSin"
    | "semiannualCos"
    | "offset";
  index?: number;
  epochYear?: number;
}

/** One ENU component in a trajectory fit. */
export interface TrajectoryComponent {
  positionM: number;
  velocityMPerYr: number;
  annualSinM?: number;
  annualCosM?: number;
  semiannualSinM?: number;
  semiannualCosM?: number;
  offsetsM: number[];
}

/** Result returned by `fitTrajectory`. */
export interface TrajectoryFit {
  referenceEpochYear: number;
  terms: TrajectoryTerm[];
  components: [TrajectoryComponent, TrajectoryComponent, TrajectoryComponent];
  parameterCovariance: number[][];
  residualRmsEnuM: [number, number, number];
  geometryQuality: GeometryQualityObject;
  status: number;
  nfev: number;
  njev: number;
  cost: number;
  optimality: number;
}

/** Step detection options. */
export interface StepDetectionOptions {
  windowYears?: number;
  scoreThreshold?: number;
  minOffsetM?: number;
  minSamplesEachSide?: number;
  minSeparationYears?: number;
  midas?: MidasOptions;
}

/** Candidate returned by `detectSteps`. */
export interface StepCandidate {
  epochYear: number;
  offsetEnuM: [number, number, number];
  score: number;
  beforeCount: number;
  afterCount: number;
  heuristic: "detrendedSlidingMedian";
}

/** Network field request passed to `networkField`. */
export interface NetworkFieldRequest {
  frame: { origin: GeodeticCoordinate; removeCommonMode?: boolean };
  stations: { id: string; reference: GeodeticCoordinate; series: PositionTimeSeries }[];
}

/** Result returned by `networkField`. */
export interface NetworkField {
  frame: { origin: Required<GeodeticCoordinate>; removeCommonMode: boolean };
  stations: {
    id: string;
    rateEnuMPerYr: [number, number, number];
    rawRateEnuMPerYr: [number, number, number];
    sigmaEnuMPerYr: [number, number, number];
    localVelocity: MidasVelocity;
  }[];
  commonModeEnuMPerYr: [number, number, number];
}

/** Minimal kinematic solution accepted by `metricsFromKinematicSolution`. */
export interface KinematicMetricInput {
  positionM: [number, number, number];
  positionCovarianceM2: [
    [number, number, number],
    [number, number, number],
    [number, number, number],
  ];
  clockM?: number;
  ztdResidualM?: number;
  usedSats?: string[];
  innovationRmsM?: number;
}

/** Allan-family deviation curve used by `fitPowerLawNoise`. */
export interface AllanDeviationCurve {
  tauS: number[];
  deviation: number[];
  n: number[];
}

/** Power-law clock-noise fit options. */
export interface PowerLawNoiseOptions {
  minPointsPerOctave?: number;
  slopeTolerance?: number;
  scatterTolerance?: number;
  basicTauS?: number;
  measurementBandwidthHz?: number;
}

/** Power-law octave decision. */
export type PowerLawOctaveDominance =
  | {
      kind: "dominant";
      noiseType: "randomWalkFM" | "flickerFM" | "whiteFM" | "flickerPM" | "whitePM";
    }
  | { kind: "ambiguous" }
  | { kind: "flagged"; flag: "underSampled" | "degenerateDeviation" | "missingModifiedAllan" };

/** One classified clock-noise tau octave. */
export interface PowerLawOctave {
  tauStartS: number;
  tauEndS: number;
  pointCount: number;
  adevSlope?: number;
  mdevSlope?: number;
  slopeScatter?: number;
  dominance: PowerLawOctaveDominance;
}

/** One fitted clock-noise coefficient region. */
export interface PowerLawNoiseRegion {
  noiseType: "randomWalkFM" | "flickerFM" | "whiteFM" | "flickerPM" | "whitePM";
  tauStartS: number;
  tauEndS: number;
  octaveCount: number;
  pointCount: number;
  meanSlope: number;
  coefficient: number;
}

/** Result returned by `fitPowerLawNoise`. */
export interface PowerLawNoiseFit {
  dominantPerOctave: PowerLawOctave[];
  coefficients: [number, number, number, number, number];
  regions: PowerLawNoiseRegion[];
}

/** Numerical orbit-fit options. */
export interface OrbitFitOptions {
  forceModel?: ForceModel;
  muKm3S2?: number;
  integrator?: Integrator;
  integratorOptions?: Pick<
    PropagateStateRequest,
    "absTol" | "relTol" | "initialStepS" | "minStepS" | "maxStepS" | "maxSteps"
  >;
  solverOptions?: { gtol?: number; ftol?: number; xtol?: number; maxNfev?: number };
  linearSolve?: "nalgebraLu" | "nalgebra_lu" | "ownedGaussianFirstTie" | "owned_gaussian_first_tie";
  minLedgerSamples?: number;
  drag?: DragConfig;
}

/** Fitted orbit covariance marker. */
export type OrbitFitCovariance = { kind: "estimated"; matrix: number[][] } | { kind: "unbounded" };

/** One satellite solution in an orbit-fit report. */
export interface OrbitFitSolution {
  satellite: string;
  initialEpochS: number;
  initialPositionKm: [number, number, number];
  initialVelocityKmS: [number, number, number];
  covariance: OrbitFitCovariance;
  geometryQuality: GeometryQualityObject;
  seedRms3dM: number;
  fitRms3dM: number;
  iterations: number;
}

/** One RTN residual ledger entry. */
export interface OrbitResidualStats {
  radialRmsM: number;
  alongRmsM: number;
  crossRmsM: number;
  rms3dM: number;
  n: number;
  lowSampleCount: boolean;
}

/** Orbit residual ledger returned in an orbit-fit report. */
export interface OrbitResidualLedger {
  perSatellite: { satellite: string; stats: OrbitResidualStats }[];
  perConstellation: { system: string; stats: OrbitResidualStats }[];
  arcSpan: { timeScale: string; startJ2000S: number; endJ2000S: number; durationS: number };
}

/** Report returned by `fitSp3PreciseOrbit` and `fitPreciseEphemerisSampleOrbit`. */
export interface OrbitFitReport {
  fits: OrbitFitSolution[];
  ledger: OrbitResidualLedger;
}

// --- Covariance propagation -------------------------------------------------
//
// `propagateCovariance(request)` and `transportCovariance(covariance, segments,
// options)` deserialize through serde. These interfaces document the accepted
// JS objects and plain object fields returned by the related table APIs.

/** Covariance reference frame label. */
export type CovarianceFrameLabel = "inertial" | "rtn";

/** RTN acceleration process-noise power spectral densities. */
export interface CovarianceProcessNoise {
  qRadialKm2S3?: number;
  qTransverseKm2S3?: number;
  qNormalKm2S3?: number;
}

/** Space-weather constants embedded in a covariance drag request. */
export interface CovarianceDragSpaceWeather {
  f107?: number;
  f107a?: number;
  ap?: number;
}

/** Drag parameterization for covariance propagation. */
export interface CovarianceDragConfig {
  bcFactorM2Kg?: number;
  ballisticCoefficientKgM2?: number;
  cd?: number;
  areaM2?: number;
  massKg?: number;
  cutoffAltitudeKm?: number;
  spaceWeather?: CovarianceDragSpaceWeather;
}

/** The object passed to `propagateCovariance`. */
export interface CovariancePropagationRequest {
  epochS: number;
  positionKm: [number, number, number];
  velocityKmS: [number, number, number];
  covariance: number[] | Float64Array;
  timesS: number[];
  covarianceFrame?: CovarianceFrameLabel;
  outputFrame?: CovarianceFrameLabel;
  processNoise?: CovarianceProcessNoise;
  forceModel?: ForceModel;
  integrator?: Integrator;
  absTol?: number;
  relTol?: number;
  initialStepS?: number;
  minStepS?: number;
  maxStepS?: number;
  maxSteps?: number;
  muKm3S2?: number;
  drag?: CovarianceDragConfig;
}

/** One STM segment passed to `transportCovariance`. */
export interface CovarianceTransportSegment {
  stateTransitionMatrix: number[] | Float64Array;
  dtS: number;
  qRotationEpochS: number;
  qRotationPositionKm: [number, number, number];
  qRotationVelocityKmS: [number, number, number];
}

/** Options passed as the third argument to `transportCovariance`. */
export interface CovarianceTransportOptions {
  processNoise?: CovarianceProcessNoise;
}

// --- SGP4 fitting -----------------------------------------------------------
//
// `fitTle(samples, config)` accepts TEME state samples plus solver controls and
// returns `TleFit.elements` / `TleFit.stats` as serde plain objects.

/** One TEME state sample for `fitTle`. */
export interface TleFitSample {
  epoch: [number, number];
  positionTemeKm: [number, number, number];
  velocityTemeKmS?: [number, number, number];
}

/** Metadata copied into the fitted TLE and OMM encodings. */
export interface TleFitMetadata {
  catalogNumber?: number;
  classification?: "U" | "C" | "S";
  internationalDesignator?: string;
  elementSetNumber?: number;
  revAtEpoch?: number;
  objectName?: string;
}

/** Solver controls for `fitTle`. */
export interface TleFitConfig {
  epoch?: "midpoint" | "first" | "last";
  epochSampleIndex?: number;
  epochJd?: [number, number];
  fitBstar?: boolean;
  bstarSeed?: number;
  useVelocity?: boolean;
  velocityWeightS?: number;
  weights?: number[];
  opsMode?: "improved" | "afspc";
  ftol?: number;
  xtol?: number;
  gtol?: number;
  maxNfev?: number;
  xScale?: "unit" | "jac" | number[];
  loss?: "linear" | "softL1" | "soft_l1" | "huber" | "cauchy" | "arctan";
  fScale?: number;
  metadata?: TleFitMetadata;
}

/** `TleFit.stats` plain object. */
export interface TleFitStatistics {
  rms_position_km: number;
  max_position_km: number;
  rms_position_axes_km: [number, number, number];
  rms_velocity_km_s?: number;
  tle_rms_position_km: number;
  status: number;
  nfev: number;
  njev: number;
  cost: number;
  optimality: number;
  bstar_observable: boolean;
  seed_refine_passes: number;
}

/** `TleFit.elements` plain object. */
export interface TleFitElements {
  epoch: [number, number];
  bstar: number;
  mean_motion_dot: number;
  mean_motion_double_dot: number;
  eccentricity: number;
  argument_of_perigee_deg: number;
  inclination_deg: number;
  mean_anomaly_deg: number;
  mean_motion_rev_per_day: number;
  right_ascension_deg: number;
  catalog_number: number;
}

// --- NMEA -------------------------------------------------------------------
//
// `parseNmea`, `NmeaAccumulator.push/finish`, and `nmeaWriteGga` use serde
// plain objects for sentence bodies, diagnostics, epochs, and writer requests.

/** Calendar date used by NMEA epoch recovery. */
export interface NmeaDate {
  year: number;
  month: number;
  day: number;
}

/** NMEA time of day. */
export interface NmeaTimeOfDay {
  hour: number;
  minute: number;
  second: number;
  nanos: number;
  decimals: number;
}

/** NMEA latitude, longitude, and ellipsoidal height. */
export interface NmeaPosition {
  latDeg: number;
  lonDeg: number;
  heightM: number;
}

/** Shared NMEA parser diagnostics. */
export interface NmeaDiagnostics {
  skipCount: number;
  warningCount: number;
  skips: unknown[];
  warnings: unknown[];
}

/** One satellite ID used by an NMEA GSA epoch. */
export interface NmeaUsedSatellite {
  raw: number;
  resolved?: string;
}

/** One parsed or accumulated NMEA epoch. */
export interface NmeaEpoch {
  timeOfDay: NmeaTimeOfDay;
  date?: NmeaDate;
  position: NmeaPosition;
  instantUtcJ2000S?: number;
  pdop?: number;
  hdop?: number;
  vdop?: number;
  usedSatellites: NmeaUsedSatellite[];
  satellitesInView: number;
  sentenceCount: number;
  diagnostics: NmeaDiagnostics;
  gga?: Record<string, unknown>;
  rmc?: Record<string, unknown>;
  gll?: Record<string, unknown>;
  gst?: Record<string, unknown>;
  vtg?: Record<string, unknown>;
  zda?: Record<string, unknown>;
  gsa: Record<string, unknown>[];
  gsv: Record<string, unknown>[];
}

/** Plain object returned by `NmeaAccumulator.push/finish`. */
export interface NmeaAccumulatorResult {
  sentences: Record<string, unknown>[];
  epochs: NmeaEpoch[];
  diagnostics: NmeaDiagnostics;
  retainedLength: number;
}

/** Constructor options for `NmeaAccumulator`. */
export interface NmeaAccumulatorOptions {
  date?: NmeaDate;
  maxSentencesPerEpoch?: number;
}

/** The object passed to `nmeaWriteGga`. */
export interface NmeaGgaRequest {
  talker?: string;
  timeSecondsOfDay: number;
  latDeg: number;
  lonDeg: number;
  coordinateDecimals?: number;
  quality?: number;
  satellitesUsed?: number;
  hdop?: number;
  altitudeMslM?: number;
  geoidSeparationM?: number;
  differentialAgeS?: number;
  differentialStationId?: string;
}

// --- NTRIP sans-IO ----------------------------------------------------------

/** Basic authentication credentials for NTRIP request construction. */
export interface NtripCredentials {
  username: string;
  password: string;
}

/** NTRIP revision label accepted by request and state-machine configs. */
export type NtripVersionLabel = "rev1" | "rev2";

/** Config object for `ntripRequestBytes` and `new NtripClientMachine(config)`. */
export interface NtripRequestConfig {
  host: string;
  port?: number;
  mountpoint?: string;
  version?: NtripVersionLabel;
  credentials?: NtripCredentials;
  userAgentProduct?: string;
  ggaIntervalS?: number;
}

/** Header row surfaced on NTRIP connection events. */
export interface NtripHeader {
  name: string;
  value: string;
}

/** Event emitted by `NtripClientMachine.push/finish`. */
export type NtripClientEvent =
  | {
      kind: "connected";
      version?: NtripVersionLabel;
      chunked?: boolean;
      headers?: NtripHeader[];
      payload?: undefined;
      sourcetable?: undefined;
      rejection?: undefined;
      detail?: undefined;
    }
  | {
      kind: "payload";
      payload: number[];
      version?: undefined;
      chunked?: undefined;
      headers?: undefined;
      sourcetable?: undefined;
      rejection?: undefined;
      detail?: undefined;
    }
  | {
      kind: "sourcetable";
      sourcetable: NtripSourcetable;
      version?: undefined;
      chunked?: undefined;
      headers?: undefined;
      payload?: undefined;
      rejection?: undefined;
      detail?: undefined;
    }
  | {
      kind: "rejected" | "streamCorrupted" | "streamEnded";
      detail?: string;
      rejection?: unknown;
      version?: undefined;
      chunked?: undefined;
      headers?: undefined;
      payload?: undefined;
      sourcetable?: undefined;
    };

/** Parsed scalar wrapper used by NTRIP sourcetable fields. */
export type NtripParsed<T> =
  { kind: "parsed"; value: T } | { kind: "missing" } | { kind: "invalid"; raw: string };

/** One STR row in an NTRIP sourcetable. */
export interface NtripStreamRecord {
  typeTag: "STR";
  mountpoint: string;
  identifier: string;
  format: string;
  formatDetails: string;
  carrier: NtripParsed<number>;
  navSystem: string;
  network: string;
  country: string;
  latDeg: NtripParsed<number>;
  lonDeg: NtripParsed<number>;
  nmeaRequired: NtripParsed<boolean>;
  networkSolution: NtripParsed<boolean>;
  generator: string;
  compression: string;
  authentication: string;
  fee: NtripParsed<boolean>;
  bitrate: NtripParsed<number>;
  misc: string;
}

/** Parsed NTRIP sourcetable. */
export interface NtripSourcetable {
  recordCount: number;
  streamCount: number;
  streams: NtripStreamRecord[];
  records: Record<string, unknown>[];
  diagnostics: unknown[];
}

// --- Space weather ----------------------------------------------------------

/** CelesTrak CSSI table coverage. */
export interface SpaceWeatherCoverage {
  firstJ2000S: number;
  lastObservedJ2000S: number;
  lastDailyPredictedJ2000S: number;
  endJ2000S: number;
}

/** One daily CSSI space-weather row. */
export interface SpaceWeatherDay {
  year: number;
  month: number;
  day: number;
  class: "observed" | "dailyPredicted";
  bsrn: number;
  nd: number;
  kp: number[];
  kpSum: number;
  ap: number[];
  apAvg: number;
  cp: number;
  c9: number;
  isn: number;
  fluxQualifier?: string;
  f107Obs: number;
  f107Adj: number;
  f107ObsCenter81: number;
  f107ObsLast81: number;
  f107AdjCenter81: number;
  f107AdjLast81: number;
}

/** One monthly predicted CSSI row. */
export interface SpaceWeatherMonthly {
  year: number;
  month: number;
  f107: number;
  f107a: number;
  ap?: number;
}

/** Sample returned by `SpaceWeatherTable.sampleAt`. */
export interface SpaceWeatherSample {
  f107: number;
  f107a: number;
  ap: number;
  class: "observed" | "dailyPredicted" | "monthlyPredicted";
  apDefaulted: boolean;
}

/** Optional table sampling policy. */
export interface SpaceWeatherSamplePolicy {
  defaultMonthlyAp?: number;
}

/** Decay request passed to `estimateDecayWithSpaceWeather`. */
export interface SpaceWeatherDecayRequest {
  epochS: number;
  positionKm: [number, number, number];
  velocityKmS: [number, number, number];
  scanStepS?: number;
  maxDurationS?: number;
  maxScanSamples?: number;
  reentryAltitudeKm?: number;
  crossingToleranceS?: number;
}

/** Decay result returned by `estimateDecayWithSpaceWeather`. */
export interface SpaceWeatherDecayResult {
  timeToDecayS: number;
  reentryEpochS: number;
  reentryPositionKm: [number, number, number];
  reentryVelocityKmS: [number, number, number];
  reentryAltitudeKm: number;
}

// --- RINEX QC and repair ----------------------------------------------------

/** Location attached to a RINEX lint finding. */
export interface RinexFindingRef {
  epochIndex?: number;
  satellite?: string;
  field?: string;
}

/** One lint finding from `lintRinexObs` or `lintRinexNav`. */
export interface RinexFinding {
  code: string;
  severity: "fatal" | "error" | "warning" | "info";
  specRef: string;
  repairable: boolean;
  at: RinexFindingRef;
  detail: string;
}

/** Shared RINEX lint report. */
export interface RinexLintReport {
  clean: boolean;
  decodedFromCrinex: boolean;
  findingCount: number;
  counts: { fatal: number; error: number; warning: number; info: number };
  findings: RinexFinding[];
}

/** Options accepted by `repairRinexObs`. */
export interface RinexObsRepairOptions {
  fileStamp?: string;
  setInterval?: boolean;
  setTimeOfLastObs?: boolean;
  setObsCounts?: boolean;
  dropEmptyRecords?: boolean;
  sortRecords?: boolean;
}

/** Options accepted by `repairRinexNav`. */
export interface RinexNavRepairOptions {
  fileStamp?: string;
  dropUnsupported?: boolean;
  sortRecords?: boolean;
}

/** Options accepted by `observationQc`. */
export interface ObservationQcOptions {
  intervalOverrideS?: number;
  gapFactor?: number;
  clockJumpThresholdS?: number;
}

/** Civil epoch used inside observation QC report rows. */
export interface ObservationQcEpochTime {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
  second: number;
}

/** One detected gap between adjacent observation epochs. */
export interface ObservationQcDataGap {
  startEpoch: ObservationQcEpochTime;
  endEpoch: ObservationQcEpochTime;
  nominalIntervalS: number;
  observedDeltaS: number;
  missingEpochs: number;
}

/** One detected receiver-clock jump. */
export interface ObservationQcClockJump {
  epochIndex: number;
  deltaS: number;
}

/** One satellite summary in an observation QC report. */
export interface ObservationQcSatellite {
  satellite: string;
  epochsWithObservations: number;
  valueObservations: number;
}

/** Signal-strength indicator histogram. */
export interface ObservationQcSsi {
  counts: number[];
}

/** SNR summary for one signal. */
export interface ObservationQcSnr {
  n: number;
  mean: number;
  min: number;
  max: number;
  std: number;
}

/** One satellite/signal summary in an observation QC report. */
export interface ObservationQcSatelliteSignal {
  satellite: string;
  code: string;
  valueObservations: number;
  ssi: ObservationQcSsi;
  snr?: ObservationQcSnr;
}

/** One system/signal summary in an observation QC report. */
export interface ObservationQcSystemSignal {
  system: string;
  code: string;
  valueObservations: number;
  ssi: ObservationQcSsi;
  snr?: ObservationQcSnr;
}

/** Per-system cycle-slip summary in an observation QC report. */
export interface ObservationQcSystemCycleSlip {
  system: string;
  observations: number;
  slips: number;
  observationsPerSlip?: number;
}

/** Cycle-slip summary in an observation QC report. */
export interface ObservationQcCycleSlips {
  observations: number;
  totalSlips: number;
  observationsPerSlip?: number;
  bySystem: ObservationQcSystemCycleSlip[];
}

/** MP1 or MP2 multipath RMS statistics. */
export interface ObservationQcMpStats {
  n: number;
  rmsM: number;
}

/** Per-satellite multipath summary in an observation QC report. */
export interface ObservationQcSatelliteMultipath {
  satellite: string;
  mp1?: ObservationQcMpStats;
  mp2?: ObservationQcMpStats;
}

/** Per-system multipath summary in an observation QC report. */
export interface ObservationQcSystemMultipath {
  system: string;
  mp1?: ObservationQcMpStats;
  mp2?: ObservationQcMpStats;
}

/** Multipath summary in an observation QC report. */
export interface ObservationQcMultipath {
  satellites: ObservationQcSatelliteMultipath[];
  systems: ObservationQcSystemMultipath[];
}

/** Non-fatal observation QC note. */
export interface ObservationQcNote {
  kind: string;
  epochIndex?: number;
}

/** Report object returned by `observationQc`. */
export interface ObservationQcReport {
  totalEpochRecords: number;
  observationEpochs: number;
  eventRecords: number;
  powerFailureEpochs: number;
  skippedRecords: number;
  intervalS?: number;
  intervalSource?: string;
  missingEpochs: number;
  dataGaps: ObservationQcDataGap[];
  clockJumps: ObservationQcClockJump[];
  cycleSlips: ObservationQcCycleSlips;
  multipath: ObservationQcMultipath;
  satellites: ObservationQcSatellite[];
  satelliteSignals: ObservationQcSatelliteSignal[];
  systemSignals: ObservationQcSystemSignal[];
  notes: ObservationQcNote[];
  renderText(): string;
  renderHtml(): string;
  toJson(): string;
}

// --- RTK baseline solving ---------------------------------------------------
//
// `solveRtkFloat(config)` / `solveRtkFixed(config)` deserialize through serde.
// These are the precise shapes accepted. Import:
//
//   import { solveRtkFloat } from "@neilberkman/sidereon";
//   import type { RtkFloatConfig } from "@neilberkman/sidereon/types";

/** RTK stochastic weighting model selector. */
export type RtkStochasticModel = "simple" | "rtklib";

/** One satellite's base/rover measurements for an RTK epoch. */
export interface RtkSatMeasurement {
  sat: string;
  sdAmbiguityId: string;
  baseCodeM: number;
  basePhaseM: number;
  roverCodeM: number;
  roverPhaseM: number;
  baseTxPos: [number, number, number];
  roverTxPos: [number, number, number];
  pos: [number, number, number];
}

/** One RTK epoch with reference and non-reference satellite rows. */
export interface RtkEpoch {
  references: RtkSatMeasurement[];
  nonref: RtkSatMeasurement[];
  dtS: number;
  velocityMps?: [number, number, number];
}

/** RTK measurement weighting and correction model. */
export interface RtkMeasurementModel {
  codeSigmaM: number;
  phaseSigmaM: number;
  /** Apply the Earth-rotation Sagnac correction. Defaults to true. */
  sagnac?: boolean;
  /** Stochastic model. Defaults to "simple". */
  stochastic?: RtkStochasticModel;
  /** Elevation-weight measurements (simple model only). Defaults to false. */
  elevationWeighting?: boolean;
}

/** Iteration controls for an RTK float solve. */
export interface RtkFloatOptions {
  positionTolM?: number;
  ambiguityTolM?: number;
  maxIterations?: number;
}

/** Iteration and integer-search controls for RTK fixed solving. */
export interface RtkFixedOptions {
  positionTolM?: number;
  ambiguityTolM?: number;
  maxIterations?: number;
  ratioThreshold?: number;
  partialAmbiguityResolution?: boolean;
  partialMinAmbiguities?: number;
}

/** Residual validation controls for RTK fixed solving. */
export interface RtkResidualValidationOptions {
  /** Per-residual rejection threshold in sigma, or null/omitted to disable. */
  thresholdSigma?: number | null;
  maxExclusions?: number;
}

/** The object passed to `solveRtkFloat`. */
export interface RtkFloatConfig {
  epochs: RtkEpoch[];
  /** Known base receiver ECEF position [x, y, z], metres. */
  base: [number, number, number];
  ambiguityIds: string[];
  model: RtkMeasurementModel;
  /** Rover-minus-base ECEF baseline seed, metres. Defaults to [0, 0, 0]. */
  initialBaselineM?: [number, number, number];
  options?: RtkFloatOptions;
}

/** The object passed to `solveRtkFixed`. */
export interface RtkFixedConfig {
  epochs: RtkEpoch[];
  base: [number, number, number];
  ambiguityIds: string[];
  /** Ambiguity id -> satellite token. */
  ambiguitySatellites: Record<string, string>;
  /** Ambiguity id -> wavelength, metres. */
  wavelengthsM: Record<string, number>;
  /** Ambiguity id -> offset, metres. */
  offsetsM: Record<string, number>;
  model: RtkMeasurementModel;
  floatOptions?: RtkFloatOptions;
  fixedOptions?: RtkFixedOptions;
  residualOptions?: RtkResidualValidationOptions;
  /** Systems kept float-only (never integer-fixed), e.g. ["R"]. */
  floatOnlySystems?: string[];
  initialBaselineM?: [number, number, number];
}

// --- PPP solving ------------------------------------------------------------
//
// `solvePppFloat(sp3, epochs, initialState, config)` / `solvePppFixed(...)`
// take serde-`any` arrays and objects. Import:
//
//   import { solvePppFloat } from "@neilberkman/sidereon";
//   import type { PppEpoch, PppFloatConfig } from "@neilberkman/sidereon/types";

/** Civil epoch timestamp for a PPP epoch. */
export interface PppCivilDateTime {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
  second: number;
}

/** One ionosphere-free code/phase observation in a PPP epoch. */
export interface PppObservation {
  satelliteId: string;
  ambiguityId: string;
  /** Ionosphere-free code, metres. */
  codeM: number;
  /** Ionosphere-free carrier phase, metres. */
  phaseM: number;
  freq1Hz?: number;
  freq2Hz?: number;
}

/** One static PPP epoch. */
export interface PppEpoch {
  civil: PppCivilDateTime;
  jdWhole: number;
  jdFraction: number;
  tRxJ2000S: number;
  observations: PppObservation[];
}

/** Initial PPP state. */
export interface PppFloatState {
  positionM: [number, number, number];
  /** Per-epoch receiver clocks, metres. */
  clocksM: number[];
  /** Ambiguity id -> initial ambiguity, metres. */
  ambiguitiesM: Record<string, number>;
  /** Initial zenith tropospheric delay, metres. Defaults to 0. */
  ztdM?: number;
}

/** PPP measurement weights. */
export interface PppMeasurementWeights {
  code?: number;
  phase?: number;
  elevationWeighting?: boolean;
}

/** One VMF1 site-wise `a`-coefficient sample (00/06/12/18 UT node). */
export interface VmfSiteSample {
  /** Modified Julian date of the sample. */
  mjd: number;
  /** Hydrostatic `a` coefficient. */
  ah: number;
  /** Wet `a` coefficient. */
  aw: number;
}

/** PPP troposphere controls. */
export interface PppTroposphereOptions {
  enabled?: boolean;
  estimateZtd?: boolean;
  pressureHpa?: number;
  temperatureK?: number;
  relativeHumidity?: number;
  /**
   * Vienna Mapping Function 1 site-wise `a`-coefficient series. When one or
   * more strictly-ascending samples are supplied the zenith delays are mapped
   * with VMF1; otherwise the climatological Niell (1996) mapping is used.
   */
  vmf1?: VmfSiteSample[];
}

/** Iteration and convergence controls for PPP. */
export interface PppFloatOptions {
  maxIterations?: number;
  positionToleranceM?: number;
  clockToleranceM?: number;
  ambiguityToleranceM?: number;
  ztdToleranceM?: number;
}

/** The config object passed to `solvePppFloat`. */
export interface PppFloatConfig {
  weights?: PppMeasurementWeights;
  tropo?: PppTroposphereOptions;
  options?: PppFloatOptions;
  residualScreen?: boolean;
}

/** Integer ambiguity controls for PPP fixed solving. */
export interface PppFixedAmbiguityOptions {
  /** Ambiguity id -> wavelength, metres. */
  wavelengthsM: Record<string, number>;
  /** Ambiguity id -> offset, metres. */
  offsetsM: Record<string, number>;
  ratioThreshold?: number;
}

/** The config object passed to `solvePppFixed`. */
export interface PppFixedConfig {
  ambiguity: PppFixedAmbiguityOptions;
  weights?: PppMeasurementWeights;
  tropo?: PppTroposphereOptions;
  options?: PppFloatOptions;
}

// --- Static PPP correction precompute ---------------------------------------
//
// `pppCorrections(sp3, epochs, receiverEcefM, options?)` deserializes `epochs`
// and `options` through serde and returns the correction tables as a plain
// object (typed `any` by wasm-bindgen). Import:
//
//   import { pppCorrections } from "@neilberkman/sidereon";
//   import type { PppCorrectionsOptions, PppCorrections } from "@neilberkman/sidereon/types";

/** One satellite observation row (carrier frequencies) for the precompute. */
export interface PppCorrectionObservation {
  satelliteId: string;
  freq1Hz: number;
  freq2Hz: number;
}

/** One receiver epoch for the correction precompute: civil UTC date/time, the
 * receive time as continuous seconds since J2000, and the visible-satellite rows. */
export interface PppCorrectionEpoch {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
  second: number;
  tRxJ2000S: number;
  observations: PppCorrectionObservation[];
}

/** Solid-earth pole-tide options: the IERS polar motion of the date (arcsec),
 * sourced from IERS EOP (the engine does not embed polar motion). */
export interface PoleTideOptions {
  xpArcsec: number;
  ypArcsec: number;
}

/** Per-station ocean-loading BLQ coefficients (Bos-Scherneck / HARDISP format).
 *
 * `amplitudeM` (metres) and `phaseDeg` (degrees, positive lag) are each a
 * `3 x 11` nested array indexed `[component][constituent]`: component order is
 * radial/up (0), tangential EW/west (1), tangential NS/south (2); constituent
 * order is the BLQ columns M2 S2 N2 K2 K1 O1 P1 Q1 Mf Mm Ssa. */
export interface OceanLoadingBlq {
  amplitudeM: number[][];
  phaseDeg: number[][];
}

/** Frequency-dependent satellite antenna calibration. */
export interface PppSatelliteAntennaFrequency {
  label: string;
  /** Body-frame phase-centre offset `[x, y, z]`, metres. */
  pcoM: [number, number, number];
  /** Nadir-angle no-azimuth PCV samples as `[nadirDeg, pcvM]` pairs. */
  noaziPcvM: [number, number][];
}

/** One satellite's antenna block, selected by PRN and an optional validity window. */
export interface PppSatelliteAntenna {
  sat: string;
  validFrom?: PppCivilDateTime;
  validUntil?: PppCivilDateTime;
  frequencies: PppSatelliteAntennaFrequency[];
}

/** Satellite-antenna correction options. */
export interface PppSatelliteAntennaOptions {
  freq1Label: string;
  freq1Hz: number;
  freq2Label: string;
  freq2Hz: number;
  antennas: PppSatelliteAntenna[];
}

/** The optional options object passed to `pppCorrections`. Omit a field to leave
 * that correction off. */
export interface PppCorrectionsOptions {
  /** Compute the per-epoch solid-earth tide displacement. */
  solidEarthTide?: boolean;
  /** Compute the per-satellite carrier-phase wind-up. */
  phaseWindup?: boolean;
  /** Compute the satellite antenna PCO/PCV projection. */
  satelliteAntenna?: PppSatelliteAntennaOptions;
  /** Compute the solid-earth pole tide (needs the date's IERS polar motion). */
  poleTide?: PoleTideOptions;
  /** Compute ocean tide loading from the station's BLQ block. */
  oceanLoading?: OceanLoadingBlq;
}

/** A per-epoch ECEF displacement correction. */
export interface PppEpochVectorCorrection {
  /** Index into the input `epochs` array. */
  epochIndex: number;
  /** ECEF displacement `[dx, dy, dz]`, metres. */
  vectorM: [number, number, number];
}

/** A per-satellite, per-epoch scalar correction (metres). */
export interface PppSatScalarCorrection {
  sat: string;
  epochIndex: number;
  valueM: number;
}

/** A per-satellite, per-epoch ECEF vector correction (metres). */
export interface PppSatVectorCorrection {
  sat: string;
  epochIndex: number;
  vectorM: [number, number, number];
}

/** The correction tables returned by `pppCorrections`. Each list is keyed by the
 * input epoch index; lists for corrections that were not requested are empty. */
export interface PppCorrections {
  /** Solid-earth tide displacement per epoch. */
  tide: PppEpochVectorCorrection[];
  /** Solid-earth pole-tide displacement per epoch. */
  poleTide: PppEpochVectorCorrection[];
  /** Ocean tide loading displacement per epoch. */
  oceanLoading: PppEpochVectorCorrection[];
  /** Carrier-phase wind-up per satellite per epoch, metres. */
  windupM: PppSatScalarCorrection[];
  /** Satellite antenna PCO projected into ECEF per satellite per epoch, metres. */
  satPcoEcef: PppSatVectorCorrection[];
  /** Satellite antenna nadir PCV per satellite per epoch, metres. */
  satPcvM: PppSatScalarCorrection[];
}

// --- SP3 merge --------------------------------------------------------------
//
// `mergeSp3(sources, options?)` deserializes `options` through serde. Import:
//
//   import { mergeSp3 } from "@neilberkman/sidereon";
//   import type { Sp3MergeOptions } from "@neilberkman/sidereon/types";

/** How agreeing SP3 sources are combined in `mergeSp3`. */
export type Sp3MergeCombine = "mean" | "median" | "precedence";

/** The options object passed to `mergeSp3`. All fields optional. */
export interface Sp3MergeOptions {
  /** Max agreeing-source 3D position difference, metres. Defaults to 0.5. */
  positionToleranceM?: number;
  /** Max agreeing-source clock difference after alignment, seconds. Defaults to 5e-9. */
  clockToleranceS?: number;
  /** Minimum agreeing sources for a multi-source cell. Defaults to 2. */
  minAgree?: number;
  /** Minimum common clocked satellites for clock-datum alignment. Defaults to 5. */
  clockMinCommon?: number;
  /** Consensus combination policy. Defaults to "mean". */
  combine?: Sp3MergeCombine;
  /** Output epoch spacing, seconds, or null for the coarsest input grid. */
  targetEpochIntervalS?: number | null;
  /** Optional system filter as RINEX letters/names (e.g. ["G", "C"]). */
  systems?: string[] | null;
}

// --- DOP series -------------------------------------------------------------
//
// `gnssDopSeries(sp3, stationEcefM, j2000Seconds, options?)` deserializes
// `options` through serde. Import:
//
//   import { gnssDopSeries } from "@neilberkman/sidereon";
//   import type { GnssDopSeriesOptions } from "@neilberkman/sidereon/types";

/** DOP row weighting selector for `gnssDopSeries`. */
export type DopWeighting = "unit" | "elevation";

/** The options object passed to `gnssDopSeries`. All fields optional. */
export interface GnssDopSeriesOptions {
  /** Explicit satellite tokens (e.g. ["G01", "G02"]); omit for a visibility scan. */
  satellites?: string[];
  /** Minimum topocentric elevation, degrees. Defaults to 5. */
  elevationMaskDeg?: number;
  /** Constellation filter as RINEX letters/names (e.g. ["G"]). */
  systems?: string[];
  /** Row weighting policy. Defaults to "unit". */
  weighting?: DopWeighting;
  /** Apply light-time + Sagnac corrections forming the line of sight. Defaults to false. */
  lightTime?: boolean;
}

// --- CDM construction -------------------------------------------------------
//
// `new CdmObject(positionKm, velocityKmS, covarianceRtn, meta?)` and
// `new Cdm(object1, object2, meta?)` deserialize their `meta` object through
// serde. Import:
//
//   import { Cdm, CdmObject } from "@neilberkman/sidereon";
//   import type { CdmObjectMeta, CdmMeta } from "@neilberkman/sidereon/types";

/** Optional metadata for a `CdmObject`: the full CCSDS 508.0-B-1 object metadata
 * block plus the optional RTN velocity-covariance rows. Every string field is the
 * verbatim textual value; omitted fields are not emitted on encode. */
export interface CdmObjectMeta {
  objectDesignator?: string;
  catalogName?: string;
  objectName?: string;
  internationalDesignator?: string;
  objectType?: string;
  operatorContactPosition?: string;
  operatorOrganization?: string;
  operatorPhone?: string;
  operatorEmail?: string;
  ephemerisName?: string;
  covarianceMethod?: string;
  maneuverable?: string;
  orbitCenter?: string;
  refFrame?: string;
  gravityModel?: string;
  atmosphericModel?: string;
  nBodyPerturbations?: string;
  solarRadPressure?: string;
  earthTides?: string;
  intrackThrust?: string;
  /** RTN velocity-covariance rows completing the 6x6 matrix, a length-15 array in
   * CCSDS order (`CRDOT_R` .. `CNDOT_NDOT`). Omit when only the position
   * covariance block is carried. */
  velocityCovarianceRtn?: number[];
}

/** Optional message-level fields for a `Cdm`. */
export interface CdmMeta {
  creationDate?: string;
  originator?: string;
  messageId?: string;
  tca?: string;
  missDistanceM?: number;
  relativeSpeedMS?: number;
  collisionProbability?: number;
  collisionProbabilityMethod?: string;
  hardBodyRadiusM?: number;
}

// --- OMM construction -------------------------------------------------------
//
// `new Omm(epoch, meanMotion, ..., noradCatId, meta?)` deserializes its `meta`
// object through serde. Import:
//
//   import { Omm } from "@neilberkman/sidereon";
//   import type { OmmMeta } from "@neilberkman/sidereon/types";

/** Optional OMM fields; each defaults to the CCSDS-standard value. */
export interface OmmMeta {
  ccsdsOmmVers?: string;
  creationDate?: string;
  originator?: string;
  objectName?: string;
  objectId?: string;
  centerName?: string;
  refFrame?: string;
  timeSystem?: string;
  meanElementTheory?: string;
  ephemerisType?: number;
  classificationType?: string;
  elementSetNo?: number;
  revAtEpoch?: number;
  bstar?: number;
  meanMotionDot?: number;
  meanMotionDdot?: number;
}

// --- DGNSS ------------------------------------------------------------------
//
// `Sp3.dgnssCorrections(request)`, `Sp3.dgnssSolve(request)`, and
// `dgnssApply(roverObservations, corrections)` deserialize their plain-object
// arguments through serde, so wasm-bindgen types them as `any`. Import:
//
//   import { dgnssApply } from "@neilberkman/sidereon";
//   import type { DgnssSolveRequest } from "@neilberkman/sidereon/types";

/** One code (pseudorange) observation. */
export interface DgnssCodeObservation {
  satelliteId: string;
  pseudorangeM: number;
}

/** One per-satellite pseudorange correction; the element type of the array
 * returned by `Sp3.dgnssCorrections` and accepted by `dgnssApply`. */
export interface DgnssCorrectionEntry {
  satelliteId: string;
  correctionM: number;
}

/** The object passed to `Sp3.dgnssCorrections`. */
export interface DgnssCorrectionsRequest {
  /** Surveyed base ECEF position [x, y, z], metres. */
  basePositionM: [number, number, number];
  baseObservations: DgnssCodeObservation[];
  /** Receive epoch, seconds since J2000 in the ephemeris time scale. */
  tRxJ2000S: number;
}

/** The object passed to `Sp3.dgnssSolve`. */
export interface DgnssSolveRequest {
  /** Surveyed base ECEF position [x, y, z], metres. */
  basePositionM: [number, number, number];
  baseObservations: DgnssCodeObservation[];
  roverObservations: DgnssCodeObservation[];
  tRxJ2000S: number;
  tRxSecondOfDayS: number;
  dayOfYear: number;
  /** Initial state guess [x, y, z, clock]; defaults to [0, 0, 0, 0]. */
  initialGuess?: [number, number, number, number];
  /** Populate WGS84 lat/lon/height in the result. Defaults to true. */
  withGeodetic?: boolean;
}

// --- Fault detection and exclusion (FDE) ------------------------------------
//
// `Sp3.fde(request)` deserializes its argument through serde. Import:
//
//   import { loadSp3 } from "@neilberkman/sidereon";
//   import type { FdeRequest } from "@neilberkman/sidereon/types";

/** One per-satellite RAIM residual weight. */
export interface RaimWeightEntry {
  satelliteId: string;
  weight: number;
}

/** The object passed to `Sp3.fde`: the SPP solve inputs plus the RAIM /
 * exclusion options. */
export interface FdeRequest {
  observations: SppObservation[];
  tRxJ2000S: number;
  tRxSecondOfDayS: number;
  dayOfYear: number;
  initialGuess?: [number, number, number, number];
  corrections?: SppCorrections;
  klobuchar?: SppKlobuchar;
  met?: SppSurfaceMet;
  glonassChannels?: [number, number][];
  withGeodetic?: boolean;
  /** RAIM false-alarm probability in the open interval (0, 1); defaults to the
   * core RAIM default. */
  pFa?: number;
  /** Per-satellite RAIM weights; absent or empty means unit weights. */
  weights?: RaimWeightEntry[];
  /** Override for the number of distinct GNSS clock systems. */
  nSystems?: number;
  /** Maximum exclusions; defaults to max(observationCount - 4, 0). */
  maxIterations?: number;
  /** Optional PDOP ceiling applied to each candidate solution. */
  maxPdop?: number;
}

// --- Broadcast-vs-precise comparison ----------------------------------------
//
// `BroadcastEphemeris.compareToSp3(...)` and the window-form
// `BroadcastEphemeris.compareWindowToSp3(...)` both return a plain object typed
// `any` with the `BroadcastCompareReport` shape below. Import:
//
//   import { loadRinexNav } from "@neilberkman/sidereon";
//   import type { BroadcastCompareReport } from "@neilberkman/sidereon/types";

/** Difference statistics for one satellite (or the overall set). Every metre
 * field is `null` when no compared epoch populated it. */
export interface BroadcastCompareStats {
  count: number;
  orbit3dRmsM: number | null;
  orbit3dMaxM: number | null;
  radialRmsM: number | null;
  radialMaxM: number | null;
  alongRmsM: number | null;
  alongMaxM: number | null;
  crossRmsM: number | null;
  crossMaxM: number | null;
  clockRmsM: number | null;
  clockMaxM: number | null;
  clockDatumRemovedRmsM: number | null;
  clockDatumRemovedMaxM: number | null;
}

/** The report returned by `BroadcastEphemeris.compareToSp3`. */
export interface BroadcastCompareReport {
  overall: BroadcastCompareStats;
  perSatellite: { satelliteId: string; stats: BroadcastCompareStats }[];
  missing: { satelliteId: string; count: number }[];
}

// --- GNSS constellation identity catalog -------------------------------------
//
// The constellation surface is JSON-in / JSON-out: every function takes and
// returns plain objects through serde, so wasm-bindgen types them as `any`.
// These are the precise shapes. Import:
//
//   import { fromCelestrakJson, mergeNavcen } from "@neilberkman/sidereon";
//   import type { ConstellationRecord } from "@neilberkman/sidereon/types";

/** Lower-case constellation label, e.g. "gps". GPS is supported today. */
export type ConstellationSystem =
  "gps" | "glonass" | "galileo" | "beidou" | "qzss" | "navic" | "sbas";

/** CelesTrak `gps-ops` provenance preserved on a record. */
export interface CelestrakSource {
  /** CelesTrak GP group the record came from ("gps-ops"). */
  group: string;
  /** The OMM OBJECT_NAME. */
  objectName?: string;
  /** The OMM OBJECT_ID (international designator). */
  objectId?: string;
  /** The OMM EPOCH, ISO-8601. */
  epoch?: string;
  /** Block type parsed from the object name ("IIF", "IIR", "IIR-M", "III"). */
  blockType?: string;
}

/** NAVCEN status provenance preserved on a record or recorded as a conflict. */
export interface NavcenSource {
  /** Space Vehicle Number. */
  svn?: number;
  blockType?: string;
  plane?: string;
  slot?: string;
  clock?: string;
  /** NANU type code ("FCSTSUMM", "UNUSABLE", "DECOM", ...). */
  nanuType?: string;
  nanuSubject?: string;
  /** Whether the row carried an active NANU. */
  activeNanu: boolean;
}

/** Per-source provenance kept on a record. */
export interface RecordSource {
  celestrak?: CelestrakSource;
  /** NAVCEN overlay merged into this record. */
  navcen?: NavcenSource;
  /** A NAVCEN row that matched the PRN but was not merged (a PRN transition). */
  navcenConflict?: NavcenSource;
}

/** A normalized GNSS satellite identity record, the element type of the array
 * returned by `fromCelestrakJson` / `mergeNavcen` and accepted by the validate
 * and diff functions. */
export interface ConstellationRecord {
  system: ConstellationSystem;
  /** Within-constellation PRN. */
  prn: number;
  /** Space Vehicle Number, when known (CelesTrak alone leaves this absent). */
  svn?: number;
  /** NORAD catalog id. */
  noradId: number;
  /** Canonical SP3/RINEX satellite token ("G03"). */
  sp3Id: string;
  /** GLONASS FDMA L1/L2 frequency-channel number (`k`, in -7..=6); absent for
   * the CDMA constellations. */
  fdmaChannel?: number;
  /** Present in the base identity source. */
  active: boolean;
  /** Advisory usability flag. */
  usable: boolean;
  source: RecordSource;
}

/** A parsed NAVCEN GPS constellation status row, the element type returned by
 * `parseNavcen` and accepted by `mergeNavcen`. */
export interface NavcenStatus {
  system: ConstellationSystem;
  prn: number;
  svn?: number;
  /** Usable per the active NANU (if any). */
  usable: boolean;
  activeNanu: boolean;
  nanuType?: string;
  nanuSubject?: string;
  plane?: string;
  slot?: string;
  blockType?: string;
  clock?: string;
}

/** A `(system, prn)` pair. Findings are keyed by system so a legitimate
 * multi-system catalog (GPS PRN 1 and Galileo PRN 1) is not a false collision. */
export interface ConstellationSystemPrn {
  system: ConstellationSystem;
  prn: number;
}

/** The report returned by `validate` / `validateAgainstSp3Ids`. */
export interface ConstellationValidation {
  /** Active+usable catalog SP3 ids absent from the compared product. */
  missingSp3Ids: string[];
  /** (system, PRN) pairs that appear in more than one record. */
  duplicatePrns: ConstellationSystemPrn[];
  /** NORAD ids that appear in more than one record. */
  duplicateNoradIds: number[];
  /** (system, PRN) pairs that are inactive or unusable. */
  inactiveUnusablePrns: ConstellationSystemPrn[];
  /** SP3 ids in the product absent from the active+usable catalog. */
  extraSp3Ids: string[];
}

/** A single field change on a PRN held across both diffed snapshots. */
export interface ConstellationFieldChange<T> {
  system: ConstellationSystem;
  prn: number;
  from: T;
  to: T;
}

/** The change report returned by `diff` and tested by `changed`. */
export interface ConstellationDiff {
  /** PRNs present only in the current snapshot. */
  added: ConstellationRecord[];
  /** PRNs present only in the previous snapshot. */
  removed: ConstellationRecord[];
  noradReassigned: ConstellationFieldChange<number>[];
  sp3IdChanged: ConstellationFieldChange<string>[];
  svnChanged: ConstellationFieldChange<number | null>[];
  /** GLONASS FDMA frequency-channel corrections on a held slot. */
  fdmaChannelChanged: ConstellationFieldChange<number | null>[];
  activityChanged: ConstellationFieldChange<boolean>[];
  usabilityChanged: ConstellationFieldChange<boolean>[];
}

/** An OMM entry `fromCelestrakJsonLenient` could not resolve to a record for the
 * requested system, carrying its identity so the caller can triage the skip. */
export interface SkippedOmm {
  /** The OMM OBJECT_NAME, when present. */
  objectName?: string;
  /** The OMM NORAD_CAT_ID. */
  noradId: number;
}

/** The result of `fromCelestrakJsonLenient`: the records that resolved for the
 * requested system, plus the entries that did not. */
export interface ConstellationCatalog {
  /** Records built from resolvable entries, sorted by `(system, prn)`. */
  records: ConstellationRecord[];
  /** Entries whose OBJECT_NAME did not resolve to a PRN for the system, in
   * input order. */
  skipped: SkippedOmm[];
}

// --- Product-staleness selection + broadcast fallback -----------------------
//
// `selectIonex` / `selectSp3` (+ `*OverRange`) and `solveWithFallback` take an
// optional `policy` object typed `any`, and the selection/source metadata cross
// out of the result handles as plain objects typed `any`. These are the precise
// shapes. Import:
//
//   import { selectSp3, solveWithFallback } from "@neilberkman/sidereon";
//   import type { StalenessPolicy, FixSource } from "@neilberkman/sidereon/types";

/** The optional staleness cap passed to the selection and fallback functions.
 * Set at most one of the two fields; omit the argument entirely for the engine
 * default cap of 3 days. */
export interface StalenessPolicy {
  /** Maximum tolerated staleness, seconds. */
  maxStalenessS?: number;
  /** Maximum tolerated staleness, days. */
  maxStalenessDays?: number;
}

/** How a selected product's source epoch relates to the requested epoch.
 * `"exact"` is no degradation; `"nearestPrior"` is the SP3 nearest-prior path;
 * `"diurnalShift"` is the IONEX whole-day persistence path. */
export type DegradationKind = "exact" | "nearestPrior" | "diurnalShift";

/** Staleness metadata attached to every selection and to a precise fix. Epochs
 * are J2000 seconds; `stalenessS` is `requested - source` and never negative. */
export interface StalenessMetadata {
  kind: DegradationKind;
  requestedEpochJ2000S: number;
  sourceEpochJ2000S: number;
  stalenessS: number;
  stalenessDays: number;
}

/** The interpolated state returned by `Sp3Selection.positionAtJ2000Seconds`. */
export interface Sp3SelectionState {
  positionM: [number, number, number];
  /** Clock offset, seconds, or `null` for the bad-clock sentinel. */
  clockS: number | null;
}

/** The variant `name` of a selection failure. A selection function throws an
 * `Error` whose `.name` is one of these and whose `.detail` is the matching
 * `SelectionErrorDetail`. */
export type SelectionErrorName =
  | "EmptyProductSet"
  | "InvalidRange"
  | "NoPriorProduct"
  | "BeyondStalenessCap"
  | "InvalidProduct"
  | "InvalidPolicy"
  | "Overflow";

/** The structured `.detail` attached to a thrown selection `Error`. Only the
 * fields the variant carries are present. */
export interface SelectionErrorDetail {
  name: SelectionErrorName;
  message: string;
  requestedEpochJ2000S?: number;
  startEpochJ2000S?: number;
  endEpochJ2000S?: number;
  sourceEpochJ2000S?: number;
  stalenessS?: number;
  maxStalenessS?: number;
  context?: string;
}

/** The structured `.detail` attached to a thrown `solveWithFallback` `Error`.
 * `name` names which path's solve failed and `message` carries the underlying
 * solve error. */
export interface FallbackErrorDetail {
  name: "PreciseSolveError" | "BroadcastSolveError";
  message: string;
}

/** Why `solveWithFallback` produced a fix from broadcast ephemeris. */
export interface BroadcastReason {
  kind: "preciseUnavailable" | "preciseDegradedUnusable";
  /** The precise selection's rejection, for `"preciseUnavailable"`. */
  selectionError: SelectionErrorDetail | null;
  /** The tried precise product's staleness, for `"preciseDegradedUnusable"`. */
  attemptedStaleness: StalenessMetadata | null;
  /** The precise solve error that triggered the fallback, for
   * `"preciseDegradedUnusable"`. */
  preciseError: string | null;
}

/** Which ephemeris source produced a `SourcedSolution`, read from
 * `SourcedSolution.source`. For a precise fix `staleness` is the product's
 * metadata and `broadcastReason` is `null`; for a broadcast fix `staleness` is
 * `null` and `broadcastReason` records why precise was not used. */
export interface FixSource {
  kind: "precise" | "broadcast";
  isPrecise: boolean;
  isBroadcast: boolean;
  isPreciseExact: boolean;
  staleness: StalenessMetadata | null;
  broadcastReason: BroadcastReason | null;
}

// --- Integer-ambiguity resolution (LAMBDA / bounded ILS) --------------------
//
// `lambdaIlsSearch(floatCycles, covariance, ratioThreshold)` and
// `boundedIlsSearch(floatCycles, covariance, radius, candidateLimit,
// ratioThreshold)` take the covariance as a row-major `number[][]` and return an
// `IlsResult` object (typed `any` by wasm-bindgen). Import:
//
//   import { lambdaIlsSearch } from "@neilberkman/sidereon";
//   import type { IlsResult } from "@neilberkman/sidereon/types";

/** The outcome of an integer-least-squares search. */
export interface IlsResult {
  /** Best integer vector, parallel to the input `floatCycles`. Cycle counts are
   * small integers and cross as plain numbers (not BigInt). */
  fixed: number[];
  /** Whether the ratio test passes at the requested threshold. */
  fixedStatus: boolean;
  /** Runner-up / best score ratio. Saturates to `Number.MAX_VALUE` when the best
   * score is exactly zero with a positive runner-up; `0` when there is no
   * runner-up. */
  ratio: number;
  /** Best (lowest) quadratic score. */
  bestScore: number;
  /** Runner-up score; absent when no second lattice point exists. */
  secondBestScore?: number;
  /** Number of lattice points evaluated. */
  candidatesEvaluated: number;
  /** Symmetrized covariance actually used, row-major. */
  covariance: number[][];
  /** Symmetrized inverse covariance, row-major. */
  covarianceInverse: number[][];
}

// --- SP3-backed visibility geometry -----------------------------------------
//
// `gnssVisible(sp3, stationEcefM, j2000Seconds, options?)`,
// `gnssVisibilitySeries(sp3, stationEcefM, startJ2000S, endJ2000S, stepSeconds,
// options?)`, and `gnssPasses(...)` deserialize `options` through serde. The
// returned classes (`GnssVisibleSatellite`, `GnssVisibilityCount`, `GnssPass`)
// are in the generated types. Import:
//
//   import { gnssVisible } from "@neilberkman/sidereon";
//   import type { GnssVisibilityOptions } from "@neilberkman/sidereon/types";

/** Visibility filters shared by `gnssVisible`, `gnssVisibilitySeries`, and
 * `gnssPasses`. All fields optional. */
export interface GnssVisibilityOptions {
  /** Minimum topocentric elevation, degrees. Defaults to 5. */
  elevationMaskDeg?: number;
  /** Constellation filter as RINEX letters/names (e.g. ["G", "E"]); omit to
   * admit every constellation in the product. */
  systems?: string[];
}

// --- Observable prediction --------------------------------------------------
//
// `observablesSp3(sp3, satellite, receiverEcefM, tRxJ2000S, options?)` and
// `observablesBroadcast(broadcast, satellite, tRxJ2000S, receiverEcefM,
// options?)` deserialize `options` through serde and return the generated
// `PredictedObservables` class. `solveVelocityBroadcast` reuses
// `VelocityObservation` / `VelocitySolveOptions` above. Import:
//
//   import { observablesSp3 } from "@neilberkman/sidereon";
//   import type { ObservablePredictOptions } from "@neilberkman/sidereon/types";

/** Options controlling observable prediction. All fields optional. */
export interface ObservablePredictOptions {
  /** Carrier frequency used to scale Doppler, hertz. Defaults to the GPS L1
   * frequency. */
  carrierHz?: number;
  /** Apply fixed-point light-time / transmit-time correction. Defaults to true. */
  lightTime?: boolean;
  /** Apply Earth-rotation Sagnac correction. Defaults to true. */
  sagnac?: boolean;
}

// --- Reduced-orbit fit/eval/drift -------------------------------------------
//
// `fitReducedOrbit(samples, scale, model)` deserializes `samples` through serde
// and returns the generated `ReducedOrbit` class, whose `position(query, frame)`
// / `positionVelocity(query, frame)` / `drift(truth, thresholdM)` methods take
// the same epoch/sample shapes. `scale` is a `TimeScale` enum value; `model` and
// `frame` are the string unions below. Import:
//
//   import { fitReducedOrbit } from "@neilberkman/sidereon";
//   import type { ReducedOrbitSample } from "@neilberkman/sidereon/types";

/** Reduced-orbit secular model selector. */
export type ReducedOrbitModelKind = "circular_secular" | "eccentric_secular";

/** Frame for reduced-orbit evaluation. */
export type ReducedOrbitFrame = "ecef" | "gcrs";

/** A civil calendar epoch, interpreted in the model's time scale. */
export interface ReducedOrbitCalendarEpoch {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
  /** Second of minute, fractional. */
  second: number;
}

/** One ECEF position sample for `fitReducedOrbit` and `ReducedOrbit.drift`. */
export interface ReducedOrbitSample {
  epoch: ReducedOrbitCalendarEpoch;
  /** ECEF X, metres. */
  xM: number;
  /** ECEF Y, metres. */
  yM: number;
  /** ECEF Z, metres. */
  zM: number;
}

// --- Source-backed reduced-orbit fit/drift ----------------------------------
//
// `fitReducedOrbitSp3(sp3, satellite, options)` and
// `fitReducedOrbitTle(tle, options)` sample the source over `options` and return
// the generated `ReducedOrbitSourceFit` class. The fitted `orbit` exposes
// `driftSp3(...)` and `driftTle(...)` methods using the drift options below.

/** Sampling window for source-backed reduced-orbit calls. */
export interface ReducedOrbitSourceSampling {
  /** Inclusive sampling start in the source time scale. */
  t0: ReducedOrbitCalendarEpoch;
  /** Inclusive sampling end in the source time scale. */
  t1: ReducedOrbitCalendarEpoch;
  /** Sampling cadence, seconds. */
  cadenceS: number;
}

/** Options for `fitReducedOrbitSp3` and `fitReducedOrbitTle`. */
export interface ReducedOrbitSourceFitOptions extends ReducedOrbitSourceSampling {
  model: ReducedOrbitModelKind;
}

/** Options for `ReducedOrbit.driftSp3` and `ReducedOrbit.driftTle`. */
export interface ReducedOrbitSourceDriftOptions extends ReducedOrbitSourceSampling {
  /** First crossing threshold, metres. */
  thresholdM: number;
}

// --- Piecewise reduced-orbit fit/eval/drift ---------------------------------
//
// `fitPiecewiseReducedOrbit(samples, scale, model, t0, t1, segmentSeconds)` tiles
// the `[t0, t1]` window (both `ReducedOrbitCalendarEpoch`) into
// `segmentSeconds`-long segments, fits each independently, and returns the
// generated `PiecewiseOrbit` class. Its `position(query, frame)` /
// `positionVelocity(query, frame)` / `drift(truth, thresholdM)` /
// `segmentIndexAt(query)` methods take the same `ReducedOrbitCalendarEpoch` /
// `ReducedOrbitSample` / `ReducedOrbitFrame` shapes as the single-segment model.
// `samples` is `ReducedOrbitSample[]`; `scale` is a `TimeScale` enum value;
// `model` is `ReducedOrbitModelKind`.

// `fitPiecewiseReducedOrbitSp3(sp3, satellite, options)` and
// `fitPiecewiseReducedOrbitTle(tle, options)` sample the source over `options`
// and return the generated `PiecewiseOrbitSourceFit` class. The fitted `orbit`
// exposes `driftSp3(...)` and `driftTle(...)` methods using
// `ReducedOrbitSourceDriftOptions`.

/** Options for source-backed piecewise reduced-orbit fitting. */
export interface PiecewiseReducedOrbitSourceFitOptions extends ReducedOrbitSourceFitOptions {
  /** Segment length, seconds. */
  segmentSeconds: number;
}

// --- GPS LNAV navigation-message codec --------------------------------------
//
// `lnavEncode(params, options)` deserializes both objects through serde and
// returns the generated `LnavSubframes` class. Import:
//
//   import { lnavEncode } from "@neilberkman/sidereon";
//   import type { LnavParams } from "@neilberkman/sidereon/types";

/** LNAV clock/ephemeris parameters in engineering units, the per-field input to
 * `lnavEncode`. Integer fields (week number, codes, health, IODC/IODE,
 * fit-interval flag, AODO) take whole numbers; the scaled fields take floats in
 * the documented physical units. */
export interface LnavParams {
  /** GPS week number. */
  weekNumber: number;
  /** L2 code indicator. */
  l2Code: number;
  /** L2 P data flag (encode-only; not recovered by `lnavDecode`). */
  l2PDataFlag: number;
  /** User range accuracy index. */
  uraIndex: number;
  /** SV health bits. */
  svHealth: number;
  /** Issue of data, clock. */
  iodc: number;
  /** Group delay differential, seconds. */
  tgd: number;
  /** Clock reference time, seconds. */
  toc: number;
  /** Clock bias coefficient, seconds. */
  af0: number;
  /** Clock drift coefficient, seconds per second. */
  af1: number;
  /** Clock drift-rate coefficient, seconds per second squared. */
  af2: number;
  /** Issue of data, ephemeris. */
  iode: number;
  /** Sine harmonic correction to orbit radius, metres. */
  crs: number;
  /** Mean motion difference, radians per second. */
  deltaN: number;
  /** Mean anomaly at reference time, radians. */
  m0: number;
  /** Cosine harmonic correction to argument of latitude, radians. */
  cuc: number;
  /** Eccentricity. */
  eccentricity: number;
  /** Sine harmonic correction to argument of latitude, radians. */
  cus: number;
  /** Square root of the semi-major axis, sqrt(metres). */
  sqrtA: number;
  /** Ephemeris reference time, seconds. */
  toe: number;
  /** Fit-interval flag. */
  fitIntervalFlag: number;
  /** Age of data offset. */
  aodo: number;
  /** Cosine harmonic correction to inclination, radians. */
  cic: number;
  /** Longitude of ascending node at weekly epoch, radians. */
  omega0: number;
  /** Sine harmonic correction to inclination, radians. */
  cis: number;
  /** Inclination at reference time, radians. */
  i0: number;
  /** Cosine harmonic correction to orbit radius, metres. */
  crc: number;
  /** Argument of perigee, radians. */
  omega: number;
  /** Rate of right ascension, radians per second. */
  omegaDot: number;
  /** Rate of inclination, radians per second. */
  idot: number;
}

/** Optional TLM/HOW options for `lnavEncode`. Every field is an integer and
 * defaults to 0 when omitted. */
export interface LnavOptions {
  /** Time-of-week count (17-bit). */
  tow?: number;
  /** Alert flag (1-bit). */
  alert?: number;
  /** Anti-spoof flag (1-bit). */
  antiSpoof?: number;
  /** Integrity status flag (1-bit). */
  integrity?: number;
  /** TLM message (14-bit). */
  tlmMessage?: number;
}

// ---------------------------------------------------------------------------
// Classical orbital elements (rv2coe / coe2rv)
// ---------------------------------------------------------------------------

/** Geometric classification of a two-body orbit. */
export type OrbitType =
  "ellipticalInclined" | "ellipticalEquatorial" | "circularInclined" | "circularEquatorial";

/**
 * Classical (Keplerian) orbital elements in the Vallado convention. Returned by
 * `rv2coe` and accepted by `coe2rv`. Angles are radians; `p` and `a` are km.
 * Undefined primary angles and inapplicable auxiliary angles are `NaN`. For
 * `coe2rv` only `p`, `ecc`, `incl`, `raan`, `argp`, and `nu` are required (an
 * ordinary elliptical-inclined orbit); `orbitType` and the auxiliary angles
 * default.
 */
export interface ClassicalElements {
  /** Semi-latus rectum p = h^2 / mu, km. */
  p: number;
  /** Semi-major axis a, km (Infinity for a parabolic orbit). Output only. */
  a?: number;
  ecc: number;
  incl: number;
  raan: number;
  argp: number;
  nu: number;
  /** Argument of latitude u = argp + nu, rad (circular inclined orbits). */
  arglat?: number;
  /** True longitude, rad (circular equatorial orbits). */
  truelon?: number;
  /** Longitude of perigee, rad (elliptical equatorial orbits). */
  lonper?: number;
  orbitType?: OrbitType;
}

/** An inertial Cartesian state, returned by `coe2rv`. */
export interface CartesianState {
  positionKm: [number, number, number];
  velocityKmS: [number, number, number];
}

// ---------------------------------------------------------------------------
// Observational-astronomy geometry
// ---------------------------------------------------------------------------

/** A point on a body surface, geocentric/planetocentric latitude and longitude. */
export interface SurfacePoint {
  /** Degrees on [-90, 90]. */
  latitudeDeg: number;
  /** Degrees on (-180, 180]. */
  longitudeDeg: number;
}

// ---------------------------------------------------------------------------
// RTCM 3.x decode
// ---------------------------------------------------------------------------

/** A 1005 / 1006 station antenna reference point. */
export interface RtcmStationCoordinates {
  type: "stationCoordinates";
  messageNumber: number;
  referenceStationId: number;
  itrfRealizationYear: number;
  gpsIndicator: boolean;
  glonassIndicator: boolean;
  galileoIndicator: boolean;
  referenceStationIndicator: boolean;
  ecefX: bigint;
  singleReceiverOscillator: boolean;
  reserved: boolean;
  ecefY: bigint;
  quarterCycleIndicator: number;
  ecefZ: bigint;
  /** Antenna height, raw 0.1 mm units (1006 only). */
  antennaHeight?: number;
  /** ECEF coordinates in metres. */
  xM: number;
  yM: number;
  zM: number;
  /** Antenna height in metres (1006 only). */
  antennaHeightM?: number;
}

/** A 1007 / 1008 / 1033 antenna or receiver descriptor. */
export interface RtcmAntennaDescriptor {
  type: "antennaDescriptor";
  messageNumber: number;
  referenceStationId: number;
  antennaDescriptor: string;
  antennaSetupId: number;
  antennaSerialNumber?: string;
  receiverType?: string;
  receiverFirmwareVersion?: string;
  receiverSerialNumber?: string;
}

/** MSM common header. */
export interface RtcmMsmHeader {
  referenceStationId: number;
  epochTime: number;
  multipleMessage: boolean;
  iods: number;
  reserved: number;
  clockSteering: number;
  externalClock: number;
  divergenceFreeSmoothing: boolean;
  smoothingInterval: number;
}

/** One MSM satellite record (raw transmitted integers). */
export interface RtcmMsmSatellite {
  id: number;
  roughRangeMs: number;
  roughRangeMod1: number;
  extendedInfo?: number;
  roughPhaseRangeRateMS?: number;
}

/** One MSM signal record (raw transmitted integers). */
export interface RtcmMsmSignal {
  satelliteId: number;
  signalId: number;
  finePseudorange: number;
  finePhaseRange: number;
  lockTimeIndicator: number;
  halfCycleAmbiguity: boolean;
  cnr: number;
  finePhaseRangeRate?: number;
}

/** Derived RINEX LLI for one MSM signal cell. */
export interface RtcmCellLli {
  satelliteId: number;
  signalId: number;
  /** RINEX LLI value; bit 0 is loss of lock, bit 1 is half-cycle ambiguity. */
  lli: number;
  minLockTimeMs?: number;
}

/** Previous lock-state input for `rtcmDeriveLli`. */
export interface RtcmPreviousLock {
  minLockTimeMs?: number;
  elapsedMs: number;
}

/** An MSM4 / MSM7 multi-signal observation message. */
export interface RtcmMsm {
  type: "msm";
  messageNumber: number;
  /** GNSS constellation label, e.g. "gps", "glonass". */
  system: string;
  /** "msm4" or "msm7". */
  kind: string;
  header: RtcmMsmHeader;
  satellites: RtcmMsmSatellite[];
  signals: RtcmMsmSignal[];
}

/** A 1019 GPS broadcast ephemeris (raw transmitted integers). */
export interface RtcmGpsEphemeris {
  type: "gpsEphemeris";
  messageNumber: number;
  satelliteId: number;
  weekNumber: number;
  svAccuracy: number;
  codeOnL2: number;
  idot: number;
  iode: number;
  tOc: number;
  aF2: number;
  aF1: number;
  aF0: number;
  iodc: number;
  cRs: number;
  deltaN: number;
  m0: bigint;
  cUc: number;
  eccentricity: bigint;
  cUs: number;
  sqrtA: bigint;
  tOe: number;
  cIc: number;
  omega0: bigint;
  cIs: number;
  i0: bigint;
  cRc: number;
  omega: bigint;
  omegaDot: number;
  tGd: number;
  svHealth: number;
  l2PDataFlag: boolean;
  fitInterval: boolean;
}

/** A 1020 GLONASS broadcast ephemeris (raw transmitted integers). */
export interface RtcmGlonassEphemeris {
  type: "glonassEphemeris";
  messageNumber: number;
  satelliteId: number;
  frequencyChannel: number;
  almanacHealth: boolean;
  almanacHealthAvailability: boolean;
  p1: number;
  tK: number;
  bNMsb: boolean;
  p2: boolean;
  tB: number;
  xnDot: number;
  xn: number;
  xnDotDot: number;
  ynDot: number;
  yn: number;
  ynDotDot: number;
  znDot: number;
  zn: number;
  znDotDot: number;
  p3: boolean;
  gammaN: number;
  mP: number;
  mLNThird: boolean;
  tauN: number;
  deltaTauN: number;
  eN: number;
  mP4: boolean;
  mFT: number;
  mNT: number;
  mM: number;
  additionalDataAvailable: boolean;
  nA: number;
  tauC: bigint;
  mN4: number;
  mTauGps: number;
  mLNFifth: boolean;
  reserved: number;
}

/** A recognized-but-undecoded message, preserved verbatim. */
export interface RtcmUnsupported {
  type: "unsupported";
  messageNumber: number;
  /** Undecoded body bytes. */
  body: number[];
}

/** The decoded RTCM 3 message IR, tagged by `type`. Returned by `decodeRtcm`. */
export type RtcmMessage =
  | RtcmMsm
  | RtcmStationCoordinates
  | RtcmAntennaDescriptor
  | RtcmGpsEphemeris
  | RtcmGlonassEphemeris
  | RtcmUnsupported;

/** A single decoded frame, returned by `decodeRtcmFrame`. */
export interface RtcmDecodedFrame {
  message: RtcmMessage;
  /** Total frame length in bytes (preamble, length, body, CRC). */
  frameLen: number;
}

/** One yielded frame from a `FrameScanner`. */
export interface RtcmScannedFrame {
  /** Message body (bytes between the length word and the CRC). */
  body: Uint8Array;
  frameLen: number;
}

/** One CRC-valid frame skipped by `decodeRtcmStream`. */
export interface RtcmFrameSkip {
  offset: number;
  messageNumber?: number;
  reason: "truncated" | "malformed";
  message?: string;
}

/** Stream diagnostics returned by `decodeRtcmStream`. */
export interface RtcmStreamDiagnostics {
  resyncBytes: number;
  skippedFrames: RtcmFrameSkip[];
}

/** RTCM stream decode result returned by `decodeRtcmStream`. */
export interface RtcmStream {
  messages: RtcmMessage[];
  diagnostics: RtcmStreamDiagnostics;
}

/** RINEX LLI bit constants used by RTCM MSM helpers. */
export interface RtcmLliBits {
  lossOfLock: number;
  halfCycle: number;
}

// ---------------------------------------------------------------------------
// Moving-baseline RTK
// ---------------------------------------------------------------------------

/** One moving-baseline epoch: a per-epoch base ECEF position plus an RTK epoch. */
export interface MovingBaselineEpoch extends RtkEpoch {
  /** Base receiver ECEF position [x, y, z] this epoch, metres. */
  basePositionM: [number, number, number];
}

/** The object passed to `solveMovingBaseline`. The ambiguity set is shared. */
export interface MovingBaselineConfig {
  epochs: MovingBaselineEpoch[];
  ambiguityIds: string[];
  ambiguitySatellites: Record<string, string>;
  wavelengthsM: Record<string, number>;
  offsetsM: Record<string, number>;
  floatOnlySystems?: string[];
  model: RtkMeasurementModel;
  floatOptions?: RtkFloatOptions;
  fixedOptions?: RtkFixedOptions;
  initialBaselineM?: [number, number, number];
  /** Carry each solved baseline into the next epoch. Defaults to true. */
  warmStart?: boolean;
}

// ---------------------------------------------------------------------------
// Time of closest approach (TCA)
// ---------------------------------------------------------------------------

/** Finder sampling controls (defaults: 60 s coarse step, 1e-3 s tolerance). */
export interface TcaFinderOptions {
  coarseStepSeconds?: number;
  timeToleranceSeconds?: number;
}

/** Collision-probability options for the TCA conjunction finders. */
export interface TcaPcOptions {
  hardBodyRadiusKm: number;
  /** Pc method; defaults to "foster_equal_area". */
  method?: "foster_equal_area" | "foster_numerical" | "alfano_2005";
  /** Primary-object 3x3 position covariance (flat row-major, km^2). */
  primaryCovarianceKm2?: number[];
  /** Secondary-object 3x3 position covariance (flat row-major, km^2). */
  secondaryCovarianceKm2?: number[];
}

/** A borrowed TLE line pair for the screening catalogs. */
export interface Tle {
  line1: string;
  line2: string;
}

/** One local time-of-closest-approach candidate. */
export interface TcaCandidate {
  /** Refined TCA split Julian date (whole boundary). */
  tcaJdWhole: number;
  tcaJdFraction: number;
  /** Recombined TCA Julian date. */
  tcaJd: number;
  tcaSecondsSinceWindowStart: number;
  missDistanceKm: number;
  /** Primary minus secondary TEME position [x, y, z], km. */
  relativePositionKm: [number, number, number];
  /** Primary minus secondary TEME velocity [vx, vy, vz], km/s. */
  relativeVelocityKmS: [number, number, number];
}

/** A TCA candidate with the collision probability evaluated at that TCA. */
export interface TcaConjunction {
  candidate: TcaCandidate;
  pc: number;
  missKm: number;
  relativeSpeedKmS: number;
  sigmaXKm: number;
  sigmaZKm: number;
}

/** One threshold-screening hit. */
export interface TcaScreeningHit {
  /** Index of the secondary in the supplied catalog. */
  secondaryIndex: number;
  candidate: TcaCandidate;
}

/** A threshold-screening hit with Pc evaluated at the returned TCA. */
export interface TcaScreeningConjunctionHit {
  secondaryIndex: number;
  conjunction: TcaConjunction;
}

// ---------------------------------------------------------------------------
// NeQuick-G full three-dimensional slant ionosphere
// ---------------------------------------------------------------------------
//
// `nequickGStecTecu(eval)` / `nequickGDelayM(eval, frequencyHz)` take a serde-
// `any` object. Import:
//
//   import { nequickGStecTecu } from "@neilberkman/sidereon";
//   import type { NequickGEval } from "@neilberkman/sidereon/types";

/** One receiver-to-satellite ray (geometry, epoch, and the Galileo broadcast
 * effective-ionisation coefficients) for the full NeQuick-G slant model. */
export interface NequickGEval {
  /** Galileo broadcast effective-ionisation coefficient a_i0. */
  ai0: number;
  ai1: number;
  ai2: number;
  /** Month of the year, 1..=12. */
  month: number;
  /** UTC time of day in hours, [0, 24]. */
  utcHours: number;
  /** Receiver geodetic longitude, degrees. */
  stationLonDeg: number;
  /** Receiver geodetic latitude, degrees. */
  stationLatDeg: number;
  /** Receiver height above the reference sphere, metres. */
  stationHeightM: number;
  /** Satellite geodetic longitude, degrees. */
  satelliteLonDeg: number;
  /** Satellite geodetic latitude, degrees. */
  satelliteLatDeg: number;
  /** Satellite height above the reference sphere, metres. */
  satelliteHeightM: number;
}

// ---------------------------------------------------------------------------
// Range RAIM / fault detection and exclusion (design-matrix form)
// ---------------------------------------------------------------------------
//
// `raimFdeDesign(rows, options?)` takes a serde-`any` array and object and
// returns the result as a plain object (typed `any` by wasm-bindgen). Import:
//
//   import { raimFdeDesign } from "@neilberkman/sidereon";
//   import type { RaimFdeRow, RaimFdeResult } from "@neilberkman/sidereon/types";

/** One linearized range measurement. */
export interface RaimFdeRow {
  /** Stable measurement identifier, e.g. a satellite token "G01". */
  id: string;
  /** Observed-minus-computed range residual, metres. */
  residualM: number;
  /** Design-matrix row (partials of predicted range w.r.t. each state
   * parameter). Length equals the estimated state dimension. */
  designRow: number[];
  /** Inverse-variance weight 1 / sigma^2; finite and strictly positive. */
  weight: number;
}

/** Options for `raimFdeDesign`. Every field is optional. */
export interface RaimFdeOptions {
  /** False-alarm probability for the global chi-square test. Default 1.0e-3. */
  pFa?: number;
  /** Maximum measurements the exclusion loop may remove. Default unbounded. */
  maxExclusions?: number;
  /** Minimum redundancy an exclusion must leave behind. Default 1. */
  minRedundancy?: number;
}

/** Global chi-square consistency test over the protected set. */
export interface RaimChiSquareTest {
  weightedSumSquares: number;
  /** Redundancy: nUsed - nState. */
  dof: number;
  /** Chi-square threshold, absent when dof <= 0. */
  threshold?: number;
  testable: boolean;
  faultDetected: boolean;
}

/** Per-measurement diagnostic, in input order. */
export interface RaimMeasurementDiagnostic {
  id: string;
  excluded: boolean;
  postFitResidualM: number;
  normalizedResidual: number;
}

/** Result of `raimFdeDesign`. */
export interface RaimFdeResult {
  /** Protected weighted-least-squares state correction, length nState. */
  stateCorrection: number[];
  /** Protected state covariance (H^T W H)^-1, nState-by-nState. */
  stateCovariance: number[][];
  globalTest: RaimChiSquareTest;
  /** Excluded measurement ids, in exclusion order. */
  excluded: string[];
  diagnostics: RaimMeasurementDiagnostic[];
  iterations: number;
}

// ---------------------------------------------------------------------------
// PPP auto-init (SPP-seeded) drivers
// ---------------------------------------------------------------------------
//
// `solvePppAutoInitFloat(sp3, epochs, options?, config)` and
// `solvePppAutoInitFixed(sp3, epochs, options?, floatConfig, fixedConfig)` seed
// the float state from the SPP solver, so no initial state is supplied. Import:
//
//   import { solvePppAutoInitFloat } from "@neilberkman/sidereon";
//   import type { PppAutoInitOptions } from "@neilberkman/sidereon/types";

/** Explicit static-position/clock seed that bypasses the SPP auto-init stages. */
export interface PppInitialGuess {
  positionM: [number, number, number];
  /** Receiver clock seed, metres (duplicated across every epoch). */
  clockM: number;
}

/** SPP surface meteorology for the auto-init seed troposphere. */
export interface PppSppSurfaceMet {
  pressureHpa: number;
  temperatureK: number;
  relativeHumidity: number;
}

/** Auto-initialization policy for the raw-epochs PPP drivers. Every field is
 * optional; omitting `initialGuess` runs the per-epoch SPP seed. */
export interface PppAutoInitOptions {
  /** Explicit seed; present skips the SPP/mean stages entirely. */
  initialGuess?: PppInitialGuess;
  /** SPP cold-start guess [x, y, z, b] for each per-epoch seed. Default zeros. */
  sppInitialGuess?: [number, number, number, number];
  /** Apply the troposphere correction in the SPP seed. Default false. */
  sppTroposphere?: boolean;
  /** Surface meteorology used by the SPP seed troposphere. */
  sppMet?: PppSppSurfaceMet;
}

// ---------------------------------------------------------------------------
// Sequential RTK baseline arc driver
// ---------------------------------------------------------------------------
//
// `solveRtkArc(epochs, config)` takes serde-`any` arrays/objects and returns
// the solution as a plain object (typed `any` by wasm-bindgen). Import:
//
//   import { solveRtkArc } from "@neilberkman/sidereon";
//   import type { RtkArcEpoch, RtkArcConfig, RtkArcSolution } from "@neilberkman/sidereon/types";

/** One raw single-frequency code/carrier observation at a receiver. */
export interface RtkArcObservation {
  satelliteId: string;
  /** Ambiguity-arc id (the satellite id for a clean arc; a distinct id splits
   * a cycle-slip arc so the single-difference key resets). */
  ambiguityId: string;
  codeM: number;
  phaseM: number;
  /** Loss-of-lock indicator. Bit 0 set marks a cycle-slip event when enabled. */
  lli?: number;
}

/** One raw RTK arc epoch: paired base/rover observations and satellite positions. */
export interface RtkArcEpoch {
  base: RtkArcObservation[];
  rover: RtkArcObservation[];
  /** Shared receive-time satellite ECEF positions (metres), satellite -> xyz. */
  satellitePositionsM: Record<string, [number, number, number]>;
  /** Transmit-time positions for the base; defaults to the shared map. */
  baseSatellitePositionsM?: Record<string, [number, number, number]>;
  /** Transmit-time positions for the rover; defaults to the shared map. */
  roverSatellitePositionsM?: Record<string, [number, number, number]>;
  velocityMps?: [number, number, number];
  /** Epoch time coordinate (seconds) for the prediction-delta computation. */
  predictionTimeS?: number;
}

/** Reference-satellite selection policy for the arc. */
export interface RtkArcReferenceSelection {
  /** "auto" (default), "satellite", or "perSystem". */
  mode?: "auto" | "satellite" | "perSystem";
  /** Fixed reference satellite token (mode "satellite", single-system only). */
  satellite?: string;
  /** Constellation letter -> reference satellite (mode "perSystem"). */
  references?: Record<string, string>;
}

/** Optional predicted-residual screen for one epoch update. */
export interface RtkArcInnovationScreen {
  thresholdSigma: number;
  minRows: number;
}

/** Per-epoch sequential-update controls. Every field is optional. */
export interface RtkArcUpdateOptions {
  holdSigmaM?: number;
  positionTolM?: number;
  ambiguityTolM?: number;
  maxIterations?: number;
  /** Kinematic baseline process-noise sigma (metres); 0 is the static filter. */
  processNoiseBaselineSigmaM?: number;
  /** Prediction dynamics. Default "constantPosition". */
  dynamicsModel?: "constantPosition" | "velocityPropagated";
  /** Constellation letters kept float-only (never integer-fixed), e.g. ["R"]. */
  floatOnlySystems?: string[];
  innovationScreen?: RtkArcInnovationScreen;
  /** Emit per-epoch residual diagnostics. Default false. */
  reportResiduals?: boolean;
  /** AR commitment arming gate (metres); omit to keep always-armed. */
  arArmingSigmaM?: number;
  /** LAMBDA acceptance ratio threshold. Default 3.0. */
  ratioThreshold?: number;
}

/** Cycle-slip handling policy for arc preprocessing. */
export type RtkArcCycleSlipPolicy = "error" | "dropSatellite" | "splitArc";

/** Optional preprocessing chained ahead of the RTK arc solve. */
export interface RtkArcPreprocessing {
  /** Reads `RtkArcObservation.lli`; omit to skip cycle-slip preprocessing. */
  cycleSlip?: RtkArcCycleSlipPolicy;
  /** Hatch code-smoothing window cap; omit to skip smoothing. */
  hatchWindowCap?: number;
  /** Base-receiver elevation mask in degrees; omit to skip masking. */
  elevationMaskDeg?: number;
}

/** The config object passed to `solveRtkArc`. */
export interface RtkArcConfig {
  /** Base-station ECEF position [x, y, z], metres. */
  baseM: [number, number, number];
  reference?: RtkArcReferenceSelection;
  model: RtkMeasurementModel;
  /** Baseline prior sigma (metres) for the initial information matrix. */
  baselinePriorSigmaM: number;
  /** Ambiguity prior sigma (metres) for each new single-difference column. */
  ambiguityPriorSigmaM: number;
  initialBaselineM?: [number, number, number];
  /** Per-ambiguity carrier wavelengths (metres) for the integer search. */
  wavelengthsM?: Record<string, number>;
  /** Per-ambiguity code-to-phase metre offsets for the integer search. */
  offsetsM?: Record<string, number>;
  updateOpts?: RtkArcUpdateOptions;
  preprocessing?: RtkArcPreprocessing;
}

/** Option groups used by the validated fixed solve inside `solveStaticRtkArc`. */
export interface RtkValidatedFixedOptions {
  float?: RtkFloatOptions;
  fixed?: RtkFixedOptions;
  residual?: RtkResidualValidationOptions;
}

/** The config object passed to `solveStaticRtkArc`. */
export interface RtkStaticArcConfig {
  arc: RtkArcConfig;
  opts?: RtkValidatedFixedOptions;
}

/** Scalar summary of one epoch's LAMBDA integer search. */
export interface RtkArcSearchSummary {
  integerStatus: "Fixed" | "NotFixed";
  integerMethod: string;
  integerRatio?: number;
  integerBestScore?: number;
  integerSecondBestScore?: number;
  integerCandidates: number;
  partialEnabled: boolean;
  partialFixed: boolean;
}

/** One public residual row at a reported epoch solution. */
export interface RtkArcResidual {
  epochIndex: number;
  satelliteId: string;
  referenceSatelliteId: string;
  ambiguityId: string;
  codeM: number;
  phaseM: number;
  codeSigmaM: number;
  phaseSigmaM: number;
  codeNormalized: number;
  phaseNormalized: number;
}

/** Static float RTK solution returned inside `RtkStaticArcSolution`. */
export interface RtkStaticArcFloatSolution {
  baselineM: [number, number, number];
  ambiguitiesM: Record<string, number>;
  ambiguityCovarianceM: number[];
  ambiguityCovarianceInverseM: number[];
  residuals: RtkArcResidual[];
  iterations: number;
  converged: boolean;
  status: "StateTolerance" | "MaxIterations";
  codeRmsM: number;
  phaseRmsM: number;
  weightedRmsM: number;
  nObservations: number;
  geometryQuality: GeometryQualityObject;
}

/** Static fixed RTK solution returned inside `RtkStaticArcSolution`. */
export interface RtkStaticArcFixedSolution {
  baselineM: [number, number, number];
  freeAmbiguitiesM: Record<string, number>;
  fixedAmbiguitiesCycles: Record<string, number>;
  fixedAmbiguitiesM: Record<string, number>;
  residuals: RtkArcResidual[];
  search: RtkArcSearchSummary;
  iterations: number;
  converged: boolean;
  status: "StateTolerance" | "MaxIterations";
  codeRmsM: number;
  phaseRmsM: number;
  weightedRmsM: number;
  nObservations: number;
}

/** One residual-validation outlier selected by fixed RTK validation. */
export interface RtkResidualValidationOutlier {
  epochIndex: number;
  satelliteId: string;
  referenceSatelliteId: string;
  ambiguityId: string;
  kind: "code" | "phase";
  residualM: number;
  sigmaM: number;
  normalizedResidual: number;
  thresholdSigma: number;
}

/** Residual-validation metadata for the accepted fixed RTK solution. */
export interface RtkResidualValidationResult {
  thresholdSigma: number;
  maxExclusions: number;
  excludedSats: string[];
  exclusions: RtkResidualValidationOutlier[];
}

/** Validated fixed RTK solution returned inside `RtkStaticArcSolution`. */
export interface RtkValidatedFixedSolution {
  floatSolution: RtkStaticArcFloatSolution;
  fixedSolution: RtkStaticArcFixedSolution;
  residualValidation?: RtkResidualValidationResult;
  ambiguityIds: string[];
  ambiguitySatellites: Record<string, string>;
}

/** The full static RTK arc solution returned by `solveStaticRtkArc`. */
export interface RtkStaticArcSolution {
  geometryQuality: GeometryQualityObject;
  references: Record<string, string>;
  ambiguityIds: string[];
  ambiguitySatellites: Record<string, string>;
  floatSolution: RtkStaticArcFloatSolution;
  fixedSolution: RtkValidatedFixedSolution;
  droppedSats: string[];
  splitCycleSlipArcs: RtkArcCycleSlipSplitArc[];
  elevationMaskedSats: string[];
}

/** One epoch's predicted-residual (innovation) screen outcome, present only when
 * the screen was enabled via `updateOpts.innovationScreen`. */
export interface RtkArcInnovationScreenResult {
  thresholdSigma: number;
  minRows: number;
  inputRows: number;
  acceptedRows: number;
  rejectedRows: number;
  rejectedCodeRows: number;
  rejectedPhaseRows: number;
  maxAbsNormalizedInnovation?: number;
  maxRejectedAbsNormalizedInnovation?: number;
  /** Whether the epoch was coasted (too few rows survived the screen). */
  coasted: boolean;
}

/** One epoch's reported baseline/ambiguity solution. */
export interface RtkArcEpochSolution {
  reportedBaselineM: [number, number, number];
  floatBaselineM: [number, number, number];
  integerFixed: boolean;
  integerRatio: number;
  newlyFixed: string[];
  fixedIds: string[];
  /** Reported single-difference ambiguities (id -> metres). */
  sdAmbiguitiesM: Record<string, number>;
  fixedDoubleDifferenceIds: string[];
  usedSatelliteIds: string[];
  search?: RtkArcSearchSummary;
  residuals: RtkArcResidual[];
  geometryQuality: GeometryQualityObject;
  /** Per-epoch innovation-screen result, present only when the screen is enabled
   * for the arc via `updateOpts.innovationScreen`. */
  innovationScreen?: RtkArcInnovationScreenResult;
}

/** The final carried filter state (the serializable streaming ABI). */
export interface RtkArcFilterState {
  version: number;
  references: Record<string, string>;
  sdAmbiguityIds: string[];
  baselineM: [number, number, number];
  sdAmbiguitiesM: number[];
  /** Row-major n-by-n information matrix, n = 3 + sdAmbiguityIds.length. */
  information: number[];
  ambiguityPriorSigmaM: number;
  epochCount: number;
  fixedCycles: Record<string, number>;
  fixedM: Record<string, number>;
}

/** Cycle-slip split-arc metadata emitted by preprocessing. */
export interface RtkArcCycleSlipSplitArc {
  receiver: "base" | "rover";
  satelliteId: string;
  ambiguityId: string;
  startEpochIndex: number;
  endEpochIndex: number;
  nEpochs: number;
}

/** The full sequential RTK arc solution returned by `solveRtkArc`. */
export interface RtkArcSolution {
  /** Per-constellation reference single-difference ambiguity ids. */
  references: Record<string, string>;
  epochs: RtkArcEpochSolution[];
  finalState: RtkArcFilterState;
  droppedSats: string[];
  splitCycleSlipArcs: RtkArcCycleSlipSplitArc[];
  elevationMaskedSats: string[];
  /** Row-major posterior covariance, n-by-n with n = finalState information size. */
  measurementCovariance: number[];
}

/** One receiver's dual-frequency code/carrier observation. */
export interface RtkDualFrequencyObservation {
  ambiguityId: string;
  p1M: number;
  p2M: number;
  phi1Cycles: number;
  phi2Cycles: number;
  f1Hz: number;
  f2Hz: number;
  lli1?: number;
  lli2?: number;
}

/** Paired base/rover dual-frequency observation for one satellite. */
export interface RtkDualFrequencySatelliteObservation {
  satelliteId: string;
  base: RtkDualFrequencyObservation;
  rover: RtkDualFrequencyObservation;
}

/** One dual-frequency RTK arc epoch. */
export interface RtkDualFrequencyArcEpoch {
  jdWhole: number;
  jdFraction: number;
  epochSortKey?: string;
  gapTimeS?: number;
  observations: RtkDualFrequencySatelliteObservation[];
  satellitePositionsM: Record<string, [number, number, number]>;
  baseSatellitePositionsM?: Record<string, [number, number, number]>;
  roverSatellitePositionsM?: Record<string, [number, number, number]>;
  velocityMps?: [number, number, number];
  predictionTimeS?: number;
}

/** Dual-frequency cycle-slip classifier thresholds. */
export interface RtkDualCycleSlipOptions {
  gfThresholdM?: number;
  mwThresholdCycles?: number;
  minArcGapS?: number;
}

/** Optional dual-frequency cycle-slip preprocessing for wide-lane fixing. */
export interface RtkDualCycleSlipConfig {
  policy: RtkArcCycleSlipPolicy;
  options?: RtkDualCycleSlipOptions;
}

/** Wide-lane integer estimation controls. */
export interface RtkWideLaneOptions {
  minEpochs: number;
  toleranceCycles: number;
  skipShortFragments: boolean;
}

/** The config object passed to `fixWideLaneRtkArc`. */
export interface RtkWideLaneArcConfig {
  baseM: [number, number, number];
  reference?: RtkArcReferenceSelection;
  options: RtkWideLaneOptions;
  cycleSlip?: RtkDualCycleSlipConfig;
}

/** The wide-lane RTK arc solution returned by `fixWideLaneRtkArc`. */
export interface RtkWideLaneArcSolution {
  geometryQuality: GeometryQualityObject;
  references: Record<string, string>;
  wideLaneCycles: Record<string, number>;
  epochs: RtkDualFrequencyArcEpoch[];
  droppedSats: string[];
  splitCycleSlipArcs: RtkArcCycleSlipSplitArc[];
}

/** The config object passed to `prepareIonosphereFreeRtkArc`. */
export interface RtkIonosphereFreeArcConfig {
  baseM: [number, number, number];
  initialBaselineM?: [number, number, number];
  reference?: RtkArcReferenceSelection;
  applyTroposphere?: boolean;
}

/** The ionosphere-free RTK arc setup returned by `prepareIonosphereFreeRtkArc`. */
export interface RtkIonosphereFreeArcSolution {
  references: Record<string, string>;
  epochs: RtkArcEpoch[];
  wavelengthsM: Record<string, number>;
  offsetsM: Record<string, number>;
}

// ---------------------------------------------------------------------------
// RTCM construction + encode (the inverse of decodeRtcm)
// ---------------------------------------------------------------------------
//
// `encodeRtcm(message)` / `encodeRtcmFrame(message)` take a `type`-tagged plain
// object carrying the raw transmitted field integers and return the encoded body
// / framed bytes. The accepted shapes mirror the `decodeRtcm` output minus the
// derived convenience fields. Import:
//
//   import { encodeRtcmFrame } from "@neilberkman/sidereon";
//   import type { RtcmMessageInput } from "@neilberkman/sidereon/types";

/** A 1005 / 1006 station antenna reference point for encoding. */
export type RtcmStationCoordinatesInput = Omit<
  RtcmStationCoordinates,
  "xM" | "yM" | "zM" | "antennaHeightM"
>;

/** A 1007 / 1008 / 1033 antenna or receiver descriptor for encoding. */
export type RtcmAntennaDescriptorInput = RtcmAntennaDescriptor;

/** An MSM4 / MSM7 observation message for encoding. */
export type RtcmMsmInput = RtcmMsm;

/** A 1019 GPS broadcast ephemeris for encoding (`messageNumber` is implied). */
export type RtcmGpsEphemerisInput = Omit<RtcmGpsEphemeris, "messageNumber">;

/** A 1020 GLONASS broadcast ephemeris for encoding (`messageNumber` is implied). */
export type RtcmGlonassEphemerisInput = Omit<RtcmGlonassEphemeris, "messageNumber">;

/** A recognized-but-undecoded message for encoding (body preserved verbatim). */
export type RtcmUnsupportedInput = RtcmUnsupported;

/** The message IR accepted by `encodeRtcm` / `encodeRtcmFrame`, tagged by `type`.
 * A `decodeRtcm` output object is also accepted (its extra derived fields are
 * ignored). */
export type RtcmMessageInput =
  | RtcmMsmInput
  | RtcmStationCoordinatesInput
  | RtcmAntennaDescriptorInput
  | RtcmGpsEphemerisInput
  | RtcmGlonassEphemerisInput
  | RtcmUnsupportedInput;

// ---------------------------------------------------------------------------
// Classical reliability and SBAS protection-level plain objects
// ---------------------------------------------------------------------------

/** Result returned by `wtestNoncentrality(alpha, power)`. */
export interface WtestNoncentrality {
  /** Two-sided false-alarm probability supplied by the caller. */
  alpha: number;
  /** Detection power supplied by the caller. */
  power: number;
  /** Missed-detection probability passed to the core calculation. */
  beta: number;
  /** Baarda w-test noncentrality distance. */
  delta0: number;
  /** Squared noncentrality distance. */
  lambda0: number;
}

/** One range-observation row accepted by `reliabilityDesign`. */
export interface RangeReliabilityRow {
  /** Observation identifier echoed into the report. */
  id: string;
  /** Linearized design row for this range observation. */
  designRow: number[];
  /** Externally supplied one-sigma range model, metres. */
  sigmaM: number;
}

/** Options accepted by `reliabilityDesign` and `reliabilityAraim`. */
export interface ReliabilityOptions {
  /** Two-sided false-alarm probability for the one-dimensional w-test. */
  alpha?: number;
  /** Detection power. Mutually exclusive with `beta`. Defaults to 0.80. */
  power?: number;
  /** Missed-detection probability. Mutually exclusive with `power`. */
  beta?: number;
  /** Optional precomputed noncentrality parameter. */
  lambda0Override?: number;
  /** Alias for `lambda0Override`. */
  lambda0?: number;
  /** Redundancy floor below which an observation is uncheckable. */
  minRedundancy?: number;
}

/** Reliability diagnostics for one observation. */
export interface ObservationReliability {
  /** Observation identifier echoed from the input row. */
  id: string;
  /** Redundancy number for this observation. */
  redundancy: number;
  /** Minimal detectable bias, metres, or `null` when uncheckable. */
  mdbM: number | null;
  /** External effect vector, metres, or `null` when unavailable. */
  externalEnuM: [number, number, number] | null;
  /** Bias-to-noise ratio in state space, or `null` when uncheckable. */
  biasToNoise: number | null;
  /** True when redundancy is below the reporting floor. */
  uncheckable: boolean;
}

/** Observation carrying the largest finite MDB in a reliability report. */
export interface ReliabilityMaxMdb {
  /** Observation identifier. */
  id: string;
  /** Largest finite MDB, metres. */
  mdbM: number;
}

/** Observation carrying the smallest redundancy number. */
export interface ReliabilityMinRedundancy {
  /** Observation identifier. */
  id: string;
  /** Smallest redundancy number. */
  redundancy: number;
}

/** Aggregate reliability diagnostics for a design. */
export interface ReliabilitySummary {
  /** Number of observations in the design. */
  nObs: number;
  /** Number of estimated parameters in the design. */
  nParams: number;
  /** Algebraic degrees of freedom, `nObs - nParams`. */
  dof: number;
  /** Sum of per-observation redundancy numbers. */
  sumRedundancy: number;
  /** Noncentrality parameter used for MDB calculations. */
  lambda0: number;
  /** Largest finite MDB, or `null` if no observation is checkable. */
  maxMdbM: ReliabilityMaxMdb | null;
  /** Smallest redundancy number and its observation identifier. */
  minRedundancy: ReliabilityMinRedundancy;
  /** Count of observations reported as uncheckable. */
  nUncheckable: number;
}

/** Full reliability design report. */
export interface ReliabilityReport {
  /** Per-observation reliability diagnostics, in input order. */
  perObservation: ObservationReliability[];
  /** Aggregate design diagnostics. */
  summary: ReliabilitySummary;
}

/** One SBAS or ARAIM protection-geometry row. */
export interface ProtectionRow {
  /** Satellite token such as `"G01"`. */
  id: string;
  /** ECEF line-of-sight unit vector. */
  lineOfSight: [number, number, number];
  /** Optional GNSS system label. Defaults to the system encoded in `id`. */
  system?: string;
  /** Satellite elevation angle, radians. */
  elevationRad: number;
}

/** Receiver coordinates used by SBAS and ARAIM protection geometry. */
export interface ProtectionReceiver {
  /** Geodetic latitude, radians. */
  latRad: number;
  /** Geodetic longitude, radians east. */
  lonRad: number;
  /** Ellipsoidal height above WGS84, metres. */
  heightM: number;
}

/** Protection geometry accepted by `sbasProtectionLevels` and `reliabilityAraim`. */
export interface ProtectionGeometry {
  /** Satellite rows in input order. */
  rows: ProtectionRow[];
  /** Receiver geodetic coordinates. */
  receiver: ProtectionReceiver;
  /** Active receiver clock systems, such as `["G"]`. */
  clockSystems: string[];
}

/** Plain-object row accepted by `new SbasErrorModel(rows)`. */
export interface SbasSisErrorInput {
  /** Satellite token matching a protection-geometry row. */
  id: string;
  /** Total one-sigma range term, metres. Mutually exclusive with components. */
  sigmaM?: number;
  /** Fast and long-term correction residual sigma, metres. */
  sigmaFltM?: number;
  /** User ionospheric range-error sigma, metres. */
  sigmaUireM?: number;
  /** Airborne receiver noise, divergence, and multipath sigma, metres. */
  sigmaAirM?: number;
  /** Tropospheric residual sigma, metres. */
  sigmaTropoM?: number;
}
