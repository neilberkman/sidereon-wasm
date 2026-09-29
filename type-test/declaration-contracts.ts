import * as NodeBindings from "../pkg-node/sidereon.js";
import * as WebBindings from "../pkg/sidereon.js";
import {
  type BrowserExactProductCache,
  type ExactCacheSingleFlightOpen,
  type ExactCacheSingleFlightOptions,
  ExactCacheSingleFlightOptionsError,
  ExactCacheSingleFlightOwnershipLostError,
  ExactCacheSingleFlightTimeoutError,
} from "../types/exact-cache.js";
import type {
  EcefToLla,
  AnomalyErrorDetail,
  AstroErrorDetail,
  DataCatalogErrorDetail,
  DomainErrorDetail,
  ElementsErrorDetail,
  EquinoctialErrorDetail,
  ExactCacheErrorDetail,
  ExactFloatDetail,
  FrameDomainErrorDetail,
  PropagationErrorDetail,
  PreciseObservableStateBatch,
  PreciseObservableStateElementResult,
  TimeModelErrorDetail,
  ScenarioErrorDetail,
  CoreErrorExactFloat,
  CoreDtedTileErrorDetail,
  CoreIonexEpochErrorDetail,
  CoreRtcmConversionCause,
  CoreRtcmDepartureDetail,
  CoreRtcmEncodeCause,
  CoreRtcmLnavRecordError,
  CoreRtcmMsmOptionalProblem,
  CoreSbasEncodeCause,
  IonexCoverageError,
  IonexCoveragePolicy,
  IonexDiagnosticEpoch,
  IonexHeader,
  IonexMapCube,
  IonexMappingDeclaration,
  IonexMappingFunction,
  IonexMissingNodePolicy,
  IonexMissingNodes,
  IonexNodeGap,
  IonexParseResult,
  IonexSlantBatchResult,
  IonexSlantDelayEvaluation,
  IonexSlantPolicyInput,
  IonexSlantPolicyLike,
  IonexSlantRefusal,
  IonexSlantRequest,
  IonexWarning,
  TecGridEvaluation,
  TecGridSamples,
  TecGridSamplesInput,
  TecSample,
  BiasError,
  BiasNoticeDetail,
} from "../types/sidereon-extra.js";
import type {
  InertialImuSampleInput,
  InertialImuSpec,
  InertialMechanizationInput,
  InertialNavState,
  InertialNavStateInput,
  InertialSimulatorOptions,
  StationDisplacementRequest,
  StationDisplacementResult,
  StationDisplacementBatchRequest,
  StationDisplacementBatchRow,
  InertialQuaternion,
  InertialErrorDetail,
  InertialMechanizationConfig,
  InertialRateRandomWalk,
  TideErrorDetail,
} from "../types/sidereon-extra.js";
import { ImuGrade, StationTideConstants } from "../pkg/sidereon.js";
import type {
  IonexHeader as NodeIonexHeader,
  IonexMapCube as NodeIonexMapCube,
  IonexParseResult as NodeIonexParseResult,
  IonexSlantBatchResult as NodeIonexSlantBatchResult,
  IonexSlantDelayEvaluation as NodeIonexSlantDelayEvaluation,
  IonexSlantRequest as NodeIonexSlantRequest,
  TecGridEvaluation as NodeTecGridEvaluation,
  TecGridSamples as NodeTecGridSamples,
  TecSample as NodeTecSample,
} from "../pkg-node/sidereon.js";

type Equal<Left, Right> =
  (<Value>() => Value extends Left ? 1 : 2) extends <Value>() => Value extends Right ? 1 : 2
    ? true
    : false;
type Assert<Condition extends true> = Condition;
type IsAny<Value> = 0 extends 1 & Value ? true : false;

type _ExactCacheOpenReturn = Assert<
  Equal<
    Awaited<ReturnType<BrowserExactProductCache["openSingleFlight"]>>,
    ExactCacheSingleFlightOpen
  >
>;
type _WebSp3ObservableBatch = Assert<
  Equal<ReturnType<WebBindings.Sp3["observableStatesAtJ2000S"]>, PreciseObservableStateBatch>
>;
type _NodeSp3ObservableBatch = Assert<
  Equal<ReturnType<NodeBindings.Sp3["observableStatesAtJ2000S"]>, PreciseObservableStateBatch>
>;
type _WebObservableElementDetail = Assert<
  Equal<PreciseObservableStateElementResult["detail"], WebBindings.ObservablesErrorDetail | null>
>;
type _TimeModelErrorDetail = Assert<
  Equal<
    TimeModelErrorDetail,
    {
      kind: "INVALID_INPUT";
      field: string;
      reason: string;
      message: string;
    }
  >
>;
type _TimeOffsetErrorDetail = Assert<
  Equal<
    WebBindings.TimeOffsetErrorDetail,
    | {
        readonly family: "TimeOffsetError";
        readonly kind: "EPOCH_REQUIRED";
        readonly message: string;
        readonly scale: "UTC" | "GLONASST";
      }
    | {
        readonly family: "TimeOffsetError";
        readonly kind: "UNSUPPORTED";
        readonly message: string;
        readonly scale: "TCG" | "TDB" | "TCB";
      }
    | {
        readonly family: "TimeOffsetError";
        readonly kind: "NON_FINITE_EPOCH";
        readonly message: string;
        readonly scale: "UTC" | "GLONASST";
      }
  >
>;
type _TimeModelCoreErrorDetail = Assert<
  Equal<
    WebBindings.TimeModelCoreErrorDetail,
    {
      readonly family: "TimeModelError";
      readonly kind: "TIME_MODEL_INVALID_INPUT";
      readonly message: string;
      readonly field: string;
      readonly reason: string;
    }
  >
>;
const inertialStateInput: InertialNavStateInput = {
  tJ2000S: 0,
  positionEcefM: [6_378_137, 0, 0],
  velocityEcefMps: [0, 0, 0],
  attitudeBodyToEcef: [
    [1, 0, 0],
    [0, 1, 0],
    [0, 0, 1],
  ],
};
const inertialSampleInput: InertialImuSampleInput = {
  kind: "increment",
  tJ2000S: 1,
  deltaVelocityMps: [0, 0, 0],
  deltaThetaRad: [0, 0, 0],
  dtS: 1,
};
const inertialSpec: InertialImuSpec = {
  accelVrwMpsSqrtS: 0,
  gyroArwRadSqrtS: 0,
  accelBiasInstabMps2: 0,
  gyroBiasInstabRps: 0,
  accelBiasTauS: 1,
  gyroBiasTauS: 1,
};
const inertialOptions: InertialSimulatorOptions = { output: "increment", seed: 7n };
const stationDisplacementRequest: StationDisplacementRequest = {
  stationEcefM: [6_378_137, 0, 0],
  year: 2020,
  month: 1,
  day: 1,
  hour: 0,
  minute: 0,
  second: 0,
  constants: StationTideConstants.Conventions,
  validity: "strict",
};
const stationDisplacementResult: StationDisplacementResult = {
  ecefM: [0, 0, 0],
  solidEarthTideEcefM: null,
  poleTideEcefM: null,
  oceanLoadingEcefM: null,
  degraded: null,
};
const inertialQuaternion: InertialQuaternion = { w: 1, x: 0, y: 0, z: 0 };
const inertialFailure: InertialErrorDetail = { kind: "NON_MONOTONIC_SAMPLE" };
const mechanizationInput: InertialMechanizationInput = {
  state: inertialStateInput,
  increment: { tJ2000S: 1, deltaVelocityMps: [0, 0, 0], deltaThetaRad: [0, 0, 0], dtS: 1 },
};
const tideBatchRequest: StationDisplacementBatchRequest = {
  stationEcefM: [6_378_137, 0, 0],
  epochs: [{ year: 2020, month: 1, day: 1, hour: 0, minute: 0, second: 0 }],
};
const tideBatchRow: StationDisplacementBatchRow = {
  index: 0,
  value: stationDisplacementResult,
  error: null,
};
const mechanizationConfig: InertialMechanizationConfig = { coningCorrection: "off" };
const randomWalk: InertialRateRandomWalk = { accelMps2SqrtS: 0.01, gyroRpsSqrtS: 0.001 };
const typedTideRefusal: TideErrorDetail = {
  kind: "TIME_SCALE",
  coverage: { kind: "OUTSIDE_COVERAGE", reason: "AFTER_COVERAGE" },
  message: "outside coverage",
};
void inertialStateInput;
void inertialSampleInput;
void inertialSpec;
void inertialOptions;
void stationDisplacementRequest;
void stationDisplacementResult;
void inertialQuaternion;
void inertialFailure;
void mechanizationInput;
void tideBatchRequest;
void tideBatchRow;
void mechanizationConfig;
void randomWalk;
void typedTideRefusal;
const inertialMechanizer = new WebBindings.StrapdownMechanizer(inertialStateInput);
const propagatedInertialState: InertialNavState = inertialMechanizer.propagate(inertialSampleInput);
inertialMechanizer.setImuErrorModel({
  bias: { accelMps2: [0, 0, 0], gyroRps: [0, 0, 0] },
  calibration: {
    accelScaleMisalignment: [
      [0, 0, 0],
      [0, 0, 0],
      [0, 0, 0],
    ],
    gyroScaleMisalignment: [
      [0, 0, 0],
      [0, 0, 0],
      [0, 0, 0],
    ],
  },
});
inertialMechanizer.setConfig(mechanizationConfig);
const imuSimulator = new WebBindings.ImuSimulator(inertialSpec, inertialOptions);
const simulatorBias: { accelMps2: [number, number, number]; gyroRps: [number, number, number] } =
  imuSimulator.bias();
const imuBatch = WebBindings.simulateImuSamplesFromIncrements(
  [{ tJ2000S: 1, deltaVelocityMps: [0, 0, 0], deltaThetaRad: [0, 0, 0], dtS: 1 }],
  inertialSpec,
  inertialOptions,
);
const stationResult: StationDisplacementResult = WebBindings.stationTideDisplacement(
  stationDisplacementRequest,
);
const attitudeQuaternion: InertialQuaternion = WebBindings.dcmToQuaternion(
  inertialStateInput.attitudeBodyToEcef,
);
const stateFromIncrement: InertialNavState = WebBindings.mechanizeEcef(mechanizationInput);
const tideBatch: StationDisplacementBatchRow[] =
  WebBindings.stationTideDisplacementBatch(tideBatchRequest);
const memsSpec: InertialImuSpec = WebBindings.imuSpecPreset(ImuGrade.Mems);
const gravityConstants = WebBindings.wgs84GravityConstants();
const generatedRandomWalk: InertialRateRandomWalk = WebBindings.imuRateRandomWalk(0.01, 0.001);
void propagatedInertialState;
void imuBatch;
void stationResult;
void attitudeQuaternion;
void stateFromIncrement;
void tideBatch;
void memsSpec;
void gravityConstants;
void generatedRandomWalk;
inertialMechanizer.free();
imuSimulator.free();
void simulatorBias;
const exactCacheSingleFlightOptions: ExactCacheSingleFlightOptions = {
  pollIntervalMs: 50,
  heartbeatIntervalMs: 5_000,
  livenessTimeoutMs: 30_000,
  waitTimeoutMs: 1_800_000,
};
const exactCacheOptionsError: TypeError = new ExactCacheSingleFlightOptionsError();
const exactCacheTimeoutError: Error = new ExactCacheSingleFlightTimeoutError();
const exactCacheOwnershipError: Error = new ExactCacheSingleFlightOwnershipLostError();
void exactCacheSingleFlightOptions;
void exactCacheOptionsError;
void exactCacheTimeoutError;
void exactCacheOwnershipError;

type WebNtripConfig = ConstructorParameters<typeof WebBindings.NtripClientMachine>[0];
type NodeNtripConfig = ConstructorParameters<typeof NodeBindings.NtripClientMachine>[0];
type WebTrackConfig = ConstructorParameters<typeof WebBindings.TrackFilterConfig>[0];
type NodeTrackConfig = ConstructorParameters<typeof NodeBindings.TrackFilterConfig>[0];

type _WebNtripDoesNotTakeFusionConfig = Assert<IsAny<WebNtripConfig>>;
type _NodeNtripDoesNotTakeFusionConfig = Assert<IsAny<NodeNtripConfig>>;
type _WebTrackDoesNotTakeFusionConfig = Assert<IsAny<WebTrackConfig>>;
type _NodeTrackDoesNotTakeFusionConfig = Assert<IsAny<NodeTrackConfig>>;

const webContentStart: WebBindings.Sp3ContentStartConvention =
  WebBindings.sp3ContentStartConvention("gfz_ult", 2022, 9, 7, "0300");
const nodeContentStart: NodeBindings.Sp3ContentStartConvention =
  NodeBindings.sp3ContentStartConvention("gfz_ult", 2022, 9, 7, "0300");
const webContentStartOffset: bigint = WebBindings.sp3ContentStartOffsetSeconds(webContentStart);
const nodeContentStartOffset: bigint = NodeBindings.sp3ContentStartOffsetSeconds(nodeContentStart);
const webSupportedSamples: string[] = WebBindings.supportedSamples(
  "gfz_ult",
  "sp3",
  2021,
  5,
  15,
  "0000",
);
const nodeSupportedSamples: string[] = NodeBindings.supportedSamples(
  "gfz_ult",
  "sp3",
  2021,
  5,
  15,
  "0000",
);
void webContentStartOffset;
void nodeContentStartOffset;
void webSupportedSamples;
void nodeSupportedSamples;

const sourceSensors: WebBindings.SourceSensor[] = [
  { positionM: [0, 0] },
  { positionM: new Float64Array([1, 0]), propagationSpeedMS: 343 },
  { positionM: [0, 1] },
];
const sourceArrivalTimes = new Float64Array([1, 2, 3]);
const sourceOptions: WebBindings.SourceLocateOptions = {
  mode: "toa",
  timingSigmaS: 0.001,
  includeInfluence: false,
};
const webSourceSolution: WebBindings.SourceSolution = WebBindings.locateSource(
  sourceSensors,
  sourceArrivalTimes,
  343,
  sourceOptions,
);
const nodeSourceSolution: NodeBindings.SourceSolution = NodeBindings.locateSource(
  sourceSensors,
  sourceArrivalTimes,
  343,
  { includeInfluence: true },
);
const webSourceSeed: WebBindings.SourceInitialGuess = WebBindings.closedFormInitialGuess(
  sourceSensors,
  sourceArrivalTimes,
  343,
  WebBindings.sourceSolveModeToa(),
);
const nodeLegacySourceSeed: NodeBindings.SourceInitialGuess = NodeBindings.chanHoInitialGuess(
  sourceSensors,
  sourceArrivalTimes,
  343,
  NodeBindings.sourceSolveModeTdoa(0),
);
const influenceScore: number = webSourceSolution.perSensorInfluence[0].score;
void webSourceSolution;
void nodeSourceSolution;
void webSourceSeed;
void nodeLegacySourceSeed;
void influenceScore;

type ExpectedStencilExtent = { beforeS: number; afterS: number };
type ExpectedContinuityVerdict = (
  fromJ2000S: number,
  throughJ2000S: number,
  orbitClass?: string | null,
  residualToleranceM?: number | null,
  gapThresholdFactor?: number | null,
) => WebBindings.WindowContinuityVerdict;
type ExpectedMergeContinuityVerdict = (
  fromJ2000S: number,
  throughJ2000S: number,
) => WebBindings.WindowContinuityVerdict | null;
type ExpectedNodeMergeContinuityVerdict = (
  fromJ2000S: number,
  throughJ2000S: number,
) => NodeBindings.WindowContinuityVerdict | null;
type ExpectedNextIssueDue = (
  center: string,
  content: string,
  now: Date,
) => WebBindings.NominalIssue;
type ExpectedNodeNextIssueDue = (
  center: string,
  content: string,
  now: Date,
) => NodeBindings.NominalIssue;

type _WebStencilExtent = Assert<
  Equal<ReturnType<WebBindings.Sp3["stencilExtent"]>, ExpectedStencilExtent>
>;
type _NodeStencilExtent = Assert<
  Equal<ReturnType<NodeBindings.Sp3["stencilExtent"]>, ExpectedStencilExtent>
>;
type _WebContinuityVerdict = Assert<
  Equal<WebBindings.Sp3["continuityVerdict"], ExpectedContinuityVerdict>
>;
type _NodeContinuityVerdict = Assert<
  Equal<NodeBindings.Sp3["continuityVerdict"], ExpectedContinuityVerdict>
>;
type _WebMergeContinuityVerdict = Assert<
  Equal<WebBindings.Sp3MergeReport["continuityVerdict"], ExpectedMergeContinuityVerdict>
>;
type _NodeMergeContinuityVerdict = Assert<
  Equal<NodeBindings.Sp3MergeReport["continuityVerdict"], ExpectedNodeMergeContinuityVerdict>
>;
type ExpectedClockOmissionReason =
  "datum_not_observable" | "preferred_source_without_clock" | "no_consensus";
type ExpectedDroppedEpochReason = "off_target_grid" | "not_on_tick_axis";
type _WebClockOmissionReason = Assert<
  Equal<WebBindings.Sp3ClockOmission["reason"], ExpectedClockOmissionReason>
>;
type _NodeClockOmissionReason = Assert<
  Equal<NodeBindings.Sp3ClockOmission["reason"], ExpectedClockOmissionReason>
>;
type _WebDroppedEpochReason = Assert<
  Equal<WebBindings.Sp3DroppedInputEpoch["reason"], ExpectedDroppedEpochReason>
>;
type _NodeDroppedEpochReason = Assert<
  Equal<NodeBindings.Sp3DroppedInputEpoch["reason"], ExpectedDroppedEpochReason>
>;
type _WebOmittedEpochs = Assert<
  Equal<WebBindings.Sp3MergeReport["omittedEpochsJ2000Seconds"], Float64Array>
>;
type _NodeOmittedEpochs = Assert<
  Equal<NodeBindings.Sp3MergeReport["omittedEpochsJ2000Seconds"], Float64Array>
>;
type _WebMergeContinuity = Assert<
  Equal<WebBindings.Sp3MergeReport["continuity"], WebBindings.MergeContinuityReport | null>
>;
type _NodeMergeContinuity = Assert<
  Equal<NodeBindings.Sp3MergeReport["continuity"], NodeBindings.MergeContinuityReport | null>
>;
type _WebMergeProvenance = Assert<
  Equal<WebBindings.Sp3MergeReport["provenance"], WebBindings.MergeProvenance | null>
>;
type _NodeMergeProvenance = Assert<
  Equal<NodeBindings.Sp3MergeReport["provenance"], NodeBindings.MergeProvenance | null>
>;
type _WebMergeProvenanceOption = Assert<
  Equal<WebBindings.Sp3MergeOptions["provenance"], "summary" | "full" | null | undefined>
>;
type _WebNextIssueDue = Assert<Equal<typeof WebBindings.nextIssueDue, ExpectedNextIssueDue>>;
type _NodeNextIssueDue = Assert<Equal<typeof NodeBindings.nextIssueDue, ExpectedNodeNextIssueDue>>;

const webNominalIssue: WebBindings.NominalIssue = WebBindings.nextIssueDue(
  "igs_ult",
  "sp3",
  new Date("2026-08-04T02:59:59Z"),
);
const nodeNominalIssue: NodeBindings.NominalIssue = NodeBindings.nextIssueDue(
  "igs_ult",
  "sp3",
  new Date("2026-08-04T02:59:59Z"),
);
const webDueAt: Date = webNominalIssue.dueAt;
const nodeDueAt: Date = nodeNominalIssue.dueAt;
const webIdentity: WebBindings.GnssProductIdentity = webNominalIssue.identity;
const nodeIdentity: NodeBindings.GnssProductIdentity = nodeNominalIssue.identity;
const webObserved: WebBindings.NominalCoverageInterval | null = webNominalIssue.covers.observed;
const nodePredicted: NodeBindings.NominalCoverageInterval | null =
  nodeNominalIssue.covers.predicted;
const mergeOptions: WebBindings.Sp3MergeOptions = {
  verifyContinuity: { orbitClass: null, residualToleranceM: 0.5 },
};
void webDueAt;
void nodeDueAt;
void webIdentity;
void nodeIdentity;
void webObserved;
void nodePredicted;
void mergeOptions;

const webExactEpoch = WebBindings.ExactEpoch.fromCivil(2000, 1, 1, 12, 0, 0.1);
const nodeExactEpoch = WebBindings.ExactEpoch.fromCivil(2000, 1, 1, 12, 0, 0.1);
const webExactQuery = WebBindings.ExactEpochQuery.fromEpoch(webExactEpoch).addBinarySeconds(0.25);
const nodeExactQuery =
  NodeBindings.ExactEpochQuery.fromEpoch(nodeExactEpoch).subtractBinarySeconds(0.25);
const webExactParts: [bigint, bigint, bigint, number] = [
  webExactEpoch.wholeSeconds,
  webExactEpoch.attoseconds,
  webExactEpoch.subAttosecondDigits,
  webExactEpoch.subAttosecondPlaces,
];
const nodeExactElapsed: number = nodeExactQuery.secondsSince(nodeExactEpoch.asQuery());
const webEpochCompare: number = webExactEpoch.compare(nodeExactEpoch);
const webEpochFromJ2000: WebBindings.ExactEpoch = WebBindings.ExactEpoch.j2000();
const webDecimalOffset: WebBindings.ExactEpoch = webEpochFromJ2000.checkedAddSeconds(0.125);
const nodeEpochElapsed: number = nodeExactQuery.secondsSinceEpoch(nodeExactEpoch);
const attosecondsPerSecond: bigint = WebBindings.exactEpochAttosecondsPerSecond();
const webExactSolve: typeof WebBindings.solveWithExactEpoch = WebBindings.solveWithExactEpoch;
const webSsrExactSpp: typeof WebBindings.solveSppWithSsrExactEpoch =
  WebBindings.solveSppWithSsrExactEpoch;
const webSsrPppFloat: typeof WebBindings.solvePppFloatWithSsr = WebBindings.solvePppFloatWithSsr;
const webSsrPppFixed: typeof WebBindings.solvePppFixedWithSsr = WebBindings.solvePppFixedWithSsr;
const webAccuracySidecars: typeof WebBindings.sp3PreciseEphemerisAccuracySamples =
  WebBindings.sp3PreciseEphemerisAccuracySamples;
const navicMessage: WebBindings.RtcmMessageInput = {
  type: "navicEphemeris",
  satelliteId: 1,
  weekNumber: 0,
  aF0: 0,
  aF1: 0,
  aF2: 0,
  ura: 0,
  tOc: 0,
  tGd: 0,
  deltaN: 0,
  iodec: 0,
  reserved: 0,
  l5Flag: false,
  sFlag: false,
  cUc: 0,
  cUs: 0,
  cIc: 0,
  cIs: 0,
  cRc: 0,
  cRs: 0,
  idot: 0,
  m0: 0n,
  tOe: 0,
  eccentricity: 0n,
  sqrtA: 0n,
  omega0: 0n,
  omega: 0n,
  omegaDot: 0,
  i0: 0n,
  spareDf544: 0,
  spareDf545: 0,
};
const legacyMessage: WebBindings.RtcmMessageInput = {
  type: "legacyObservations",
  messageNumber: 1002,
  referenceStationId: 0,
  epochTime: 0,
  synchronousGnss: false,
  satelliteCount: 0,
  divergenceFreeSmoothing: false,
  smoothingInterval: 0,
  satellites: [],
};
const projectionMessage: WebBindings.RtcmMessageInput = {
  type: "projection",
  systemId: 1,
  projectionType: 3,
  parameters: {
    kind: "naturalOrigin",
    latitude: 0n,
    longitude: 0n,
    addScale: 0,
    falseEasting: 0n,
    falseNorthing: 0n,
  },
};
const nodeAccuracyFactory: typeof NodeBindings.preciseEphemerisSamplesFromSamplesWithAccuracy =
  NodeBindings.preciseEphemerisSamplesFromSamplesWithAccuracy;
const webSsrPolicy: typeof WebBindings.SsrCorrectionSizePolicy.Strict =
  WebBindings.SsrCorrectionSizePolicy.Strict;
type _WebSp3Accuracy = Assert<
  Equal<ReturnType<WebBindings.Sp3["recordAccuracy"]>, Sp3RecordAccuracy>
>;
type _NodeSp3RawAccuracy = Assert<
  Equal<ReturnType<NodeBindings.Sp3["recordAccuracyCodes"]>, Sp3RawRecordAccuracy>
>;
type _AccuracyOutcome = Assert<
  Equal<Extract<Sp3AccuracyValue, { kind: "known" }>["value"], number>
>;
type _AccuracySampleEpoch = Assert<Equal<Sp3PreciseEphemerisAccuracySample["epoch"], number>>;
type _AccuracySampleHasLosslessInstant = Assert<
  Equal<
    Sp3PreciseEphemerisAccuracySample["instant"]["representation"]["kind"],
    "julianDate" | "nanos"
  >
>;
type _SampleInstantNanosAreExactText = Assert<
  Equal<
    Extract<
      NonNullable<Sp3PreciseEphemerisSample["instant"]>["representation"],
      { kind: "nanos" }
    >["nanos"],
    string
  >
>;
type _SamplePositionAxes = Assert<
  Equal<Sp3PreciseEphemerisSample["positionEcefM"], [number, number, number]>
>;
type _AccuracyErrorKind = Assert<
  Equal<
    PreciseSamplesErrorDetail["kind"],
    | "EMPTY"
    | "SINGLE_SAMPLE_SATELLITE"
    | "NON_MONOTONIC_EPOCHS"
    | "MIXED_TIME_SCALES"
    | "EPOCH_NOT_REPRESENTABLE"
    | "NON_FINITE_SAMPLE"
    | "ACCURACY_SAMPLES_MISMATCH"
    | "INVALID_ACCURACY_VALUE"
    | "UNKNOWN"
  >
>;
type _SsrOversizedSource = Assert<
  Equal<SsrOversizedCorrection["source"], "rtcmSsr" | "galileoHas" | "igsSsr">
>;
type _SsrCorrectionExceeded = Assert<Equal<SsrCorrectionSize["exceedsLimit"], boolean>>;
type _SppSsrRefusalCarriesSize = Assert<
  Equal<
    NonNullable<
      Extract<SppRejectedSatellite, { reason: "ssrCorrectionExceedsLimit" }>["ssrCorrectionSize"]
    >,
    SsrCorrectionSize
  >
>;
type _MsmSignalOptional = Assert<
  Equal<WebBindings.RtcmMsmSignalInput["finePhaseRange"], number | null | undefined>
>;
type _MsmRoughRangeOptional = Assert<
  Equal<NodeBindings.RtcmMsmSatellite["roughRangeMs"], number | undefined>
>;
void webExactParts;
void nodeExactElapsed;
void webEpochCompare;
void webDecimalOffset;
void nodeEpochElapsed;
void attosecondsPerSecond;
void navicMessage;
void legacyMessage;
void projectionMessage;
void webSsrExactSpp;
void webSsrPppFloat;
void webSsrPppFixed;
void webExactSolve;
void webAccuracySidecars;
void nodeAccuracyFactory;
void webSsrPolicy;

const webTerrainAttested: WebBindings.MmapTerrain = WebBindings.MmapTerrain.fromPathAttested(
  "terrain.tmm",
  0xffff_ffff_ffff_ffffn,
);
const nodeTerrainAttested: NodeBindings.MmapTerrain = NodeBindings.MmapTerrain.fromPathAttested(
  "terrain.tmm",
  0xffff_ffff_ffff_ffffn,
);
const webPreciseAttested: WebBindings.PreciseInterpolantArtifact =
  WebBindings.PreciseInterpolantArtifact.fromPathAttested("precise.spi", 1n);
const nodePreciseAttested: NodeBindings.PreciseInterpolantArtifact =
  NodeBindings.PreciseInterpolantArtifact.fromPathAttested("precise.spi", 1n);
const webTerrainDigestProvenance: "verified" | "attested" = webTerrainAttested.digestProvenance;
const nodeTerrainDigestProvenance: "verified" | "attested" = nodeTerrainAttested.digestProvenance;
const webPreciseDigestProvenance: "verified" | "attested" = webPreciseAttested.digestProvenance;
const nodePreciseDigestProvenance: "verified" | "attested" = nodePreciseAttested.digestProvenance;
void webTerrainDigestProvenance;
void nodeTerrainDigestProvenance;
void webPreciseDigestProvenance;
void nodePreciseDigestProvenance;

type _WebNmeaEpochsAreNotFusionEpochs = Assert<IsAny<WebBindings.NmeaParseResult["epochs"]>>;
type _NodeNmeaEpochsAreNotFusionEpochs = Assert<IsAny<NodeBindings.NmeaParseResult["epochs"]>>;
type _WebTrackEpochsAreNotFusionEpochs = Assert<IsAny<WebBindings.TrackRtsHistory["epochs"]>>;
type _NodeTrackEpochsAreNotFusionEpochs = Assert<IsAny<NodeBindings.TrackRtsHistory["epochs"]>>;
type _WebSmoothedTrackEpochsAreNotFusionEpochs = Assert<IsAny<WebBindings.SmoothedTrack["epochs"]>>;
type _NodeSmoothedTrackEpochsAreNotFusionEpochs = Assert<
  IsAny<NodeBindings.SmoothedTrack["epochs"]>
>;

type _WebStaticResidualsAreNotPppResiduals = Assert<IsAny<WebBindings.StaticSolution["residuals"]>>;
type _NodeStaticResidualsAreNotPppResiduals = Assert<
  IsAny<NodeBindings.StaticSolution["residuals"]>
>;
type _WebRtkAmbiguitiesRemainIndependent = Assert<
  IsAny<WebBindings.RtkFloatSolution["ambiguitiesM"]>
>;
type _NodeRtkAmbiguitiesRemainIndependent = Assert<
  IsAny<NodeBindings.RtkFloatSolution["ambiguitiesM"]>
>;

interface WebFusionConfigExtension extends WebBindings.FusionConfig {
  extension: true;
}
interface WebFusionTimeSyncConfigExtension extends WebBindings.FusionTimeSyncConfig {
  extension: true;
}
interface WebImuSampleInputExtension extends WebBindings.ImuSampleInput {
  extension: true;
}
interface WebFusionLooseMeasurementExtension extends WebBindings.FusionLooseMeasurement {
  extension: true;
}
interface WebFusionTightEpochExtension extends WebBindings.FusionTightEpoch {
  extension: true;
}
interface WebFusionUpdateExtension extends WebBindings.FusionUpdate {
  extension: true;
}
interface WebFusionStateExtension extends WebBindings.FusionState {
  extension: true;
}
interface WebFusionTimeSyncStatusExtension extends WebBindings.FusionTimeSyncStatus {
  extension: true;
}
interface WebFusionRtsEpochExtension extends WebBindings.FusionRtsEpoch {
  extension: true;
}

interface NodeFusionConfigExtension extends NodeBindings.FusionConfig {
  extension: true;
}
interface NodeFusionTimeSyncConfigExtension extends NodeBindings.FusionTimeSyncConfig {
  extension: true;
}
interface NodeImuSampleInputExtension extends NodeBindings.ImuSampleInput {
  extension: true;
}
interface NodeFusionLooseMeasurementExtension extends NodeBindings.FusionLooseMeasurement {
  extension: true;
}
interface NodeFusionTightEpochExtension extends NodeBindings.FusionTightEpoch {
  extension: true;
}
interface NodeFusionUpdateExtension extends NodeBindings.FusionUpdate {
  extension: true;
}
interface NodeFusionStateExtension extends NodeBindings.FusionState {
  extension: true;
}
interface NodeFusionTimeSyncStatusExtension extends NodeBindings.FusionTimeSyncStatus {
  extension: true;
}
interface NodeFusionRtsEpochExtension extends NodeBindings.FusionRtsEpoch {
  extension: true;
}

type _WebFusionConfigAllowsArbitraryProperties = Assert<
  IsAny<WebBindings.FusionConfig["arbitraryProperty"]>
>;
type _WebFusionTimeSyncConfigAllowsArbitraryProperties = Assert<
  IsAny<WebBindings.FusionTimeSyncConfig["arbitraryProperty"]>
>;
type _WebImuSampleInputAllowsArbitraryProperties = Assert<
  IsAny<WebBindings.ImuSampleInput["arbitraryProperty"]>
>;
type _WebFusionLooseMeasurementAllowsArbitraryProperties = Assert<
  IsAny<WebBindings.FusionLooseMeasurement["arbitraryProperty"]>
>;
type _WebFusionTightEpochAllowsArbitraryProperties = Assert<
  IsAny<WebBindings.FusionTightEpoch["arbitraryProperty"]>
>;
type _WebFusionUpdateAllowsArbitraryProperties = Assert<
  IsAny<WebBindings.FusionUpdate["arbitraryProperty"]>
>;
type _WebFusionStateAllowsArbitraryProperties = Assert<
  IsAny<WebBindings.FusionState["arbitraryProperty"]>
>;
type _WebFusionTimeSyncStatusAllowsArbitraryProperties = Assert<
  IsAny<WebBindings.FusionTimeSyncStatus["arbitraryProperty"]>
>;
type _WebFusionRtsEpochAllowsArbitraryProperties = Assert<
  IsAny<WebBindings.FusionRtsEpoch["arbitraryProperty"]>
>;

type _NodeFusionConfigAllowsArbitraryProperties = Assert<
  IsAny<NodeBindings.FusionConfig["arbitraryProperty"]>
>;
type _NodeFusionTimeSyncConfigAllowsArbitraryProperties = Assert<
  IsAny<NodeBindings.FusionTimeSyncConfig["arbitraryProperty"]>
>;
type _NodeImuSampleInputAllowsArbitraryProperties = Assert<
  IsAny<NodeBindings.ImuSampleInput["arbitraryProperty"]>
>;
type _NodeFusionLooseMeasurementAllowsArbitraryProperties = Assert<
  IsAny<NodeBindings.FusionLooseMeasurement["arbitraryProperty"]>
>;
type _NodeFusionTightEpochAllowsArbitraryProperties = Assert<
  IsAny<NodeBindings.FusionTightEpoch["arbitraryProperty"]>
>;
type _NodeFusionUpdateAllowsArbitraryProperties = Assert<
  IsAny<NodeBindings.FusionUpdate["arbitraryProperty"]>
>;
type _NodeFusionStateAllowsArbitraryProperties = Assert<
  IsAny<NodeBindings.FusionState["arbitraryProperty"]>
>;
type _NodeFusionTimeSyncStatusAllowsArbitraryProperties = Assert<
  IsAny<NodeBindings.FusionTimeSyncStatus["arbitraryProperty"]>
>;
type _NodeFusionRtsEpochAllowsArbitraryProperties = Assert<
  IsAny<NodeBindings.FusionRtsEpoch["arbitraryProperty"]>
>;
type _WebFusionConstructorAvoidsStaleOverlay = Assert<
  IsAny<ConstructorParameters<typeof WebBindings.GnssInsFilter>[0]>
>;
type _NodeFusionConstructorAvoidsStaleOverlay = Assert<
  IsAny<ConstructorParameters<typeof NodeBindings.GnssInsFilter>[0]>
>;
type _WebFusionStateAvoidsStaleOverlay = Assert<
  IsAny<ReturnType<WebBindings.GnssInsFilter["state"]>>
>;
type _NodeFusionStateAvoidsStaleOverlay = Assert<
  IsAny<ReturnType<NodeBindings.GnssInsFilter["state"]>>
>;
type _WebFusionEpochsAvoidStaleOverlay = Assert<IsAny<WebBindings.FusionRtsHistory["epochs"]>>;
type _NodeFusionEpochsAvoidStaleOverlay = Assert<IsAny<NodeBindings.FusionRtsHistory["epochs"]>>;
type _WebSmoothedFusionEpochsAvoidStaleOverlay = Assert<
  IsAny<WebBindings.SmoothedFusionTrajectory["epochs"]>
>;
type _NodeSmoothedFusionEpochsAvoidStaleOverlay = Assert<
  IsAny<NodeBindings.SmoothedFusionTrajectory["epochs"]>
>;
type _WebPppResidualsRemainTyped = Assert<
  Equal<WebBindings.PppFloatSolution["residuals"], WebBindings.PppResidual[]>
>;
type _NodePppResidualsRemainTyped = Assert<
  Equal<NodeBindings.PppFloatSolution["residuals"], NodeBindings.PppResidual[]>
>;

function compileOnlyConstructorExamples() {
  new WebBindings.NtripClientMachine({ host: "caster.example.test" });
  new NodeBindings.NtripClientMachine({ host: "caster.example.test" });

  const trackConfig = {
    frame: "callerDefinedCartesian",
    initialTS: 0,
    initialPositionM: [0],
    initialVelocityMS: [1],
    initialCovariance: [
      [1, 0],
      [0, 1],
    ],
    accelerationVarianceSpectralDensityM2S3: 0.1,
  };

  new WebBindings.TrackFilterConfig(trackConfig);
  new NodeBindings.TrackFilterConfig(trackConfig);
}
// --- IONEX -----------------------------------------------------------------
//
// The IONEX declarations live in the `typescript_custom_section` of
// `src/ionex.rs`, so `wasm-pack` writes them into both `sidereon.d.ts` targets
// and the `unchecked_return_type` / `unchecked_param_type` attributes resolve
// against them. `types/sidereon-extra.js` re-exports the browser target's
// copies, which is what the unprefixed names below are.
//
// Nothing here casts: a boundary that came back as `any` would satisfy every
// assignment and fail the `IsAny` assertions instead.

function compileOnlyIonexTypeContracts(
  batchResult: IonexSlantBatchResult,
  warning: IonexWarning,
  decl: IonexMappingDeclaration,
) {
  if (batchResult.isOk) {
    const index: number = batchResult.index;
    const evaluation: IonexSlantDelayEvaluation = batchResult.evaluation;
    const delayM: number = evaluation.delayM;
    const isValid: boolean = evaluation.status.isValid;
    const isNominal: boolean = evaluation.status.isNominal;
    const isHeld: boolean = evaluation.status.isHeld;
    const isDegraded: boolean = evaluation.status.isDegraded;
    const isAssumedMapping: boolean = evaluation.status.isAssumedMapping;
    // The success arm carries no refusal at all.
    const refusal: null = batchResult.refusal;
    void index;
    void delayM;
    void isValid;
    void isNominal;
    void isHeld;
    void isDegraded;
    void isAssumedMapping;
    void refusal;
  } else {
    const refusal: IonexSlantRefusal = batchResult.refusal;
    const evaluation: null = batchResult.evaluation;
    void evaluation;
    if (refusal.kind === "COVERAGE") {
      const coverageError: IonexCoverageError = refusal.coverageError;
      void coverageError;
    } else if (refusal.kind === "MISSING_NODES") {
      const gap: IonexNodeGap = refusal.nodeGap;
      const earlier: IonexMissingNodes | null = gap.earlier;
      const later: IonexMissingNodes | null = gap.later;
      void earlier;
      void later;
    } else if (refusal.kind === "VARYING_HEIGHTS") {
      const mapNumber: number = refusal.mapNumber;
      const latIndex: number = refusal.latIndex;
      const lonIndex: number = refusal.lonIndex;
      void mapNumber;
      void latIndex;
      void lonIndex;
    } else if (refusal.kind === "HEIGHT_NOT_AVAILABLE") {
      const mapNumber: number = refusal.mapNumber;
      void mapNumber;
    } else if (refusal.kind === "MAPPING_FUNCTION") {
      const mappingDeclaration: IonexMappingDeclaration = refusal.mappingDeclaration;
      void mappingDeclaration;
    } else if (refusal.kind === "INVALID_INPUT") {
      const message: string = refusal.message;
      void message;
    } else {
      const message: string = refusal.message;
      void message;
    }
  }

  if (warning.kind === "MISSING_RECORD") {
    const label: string = warning.label;
    void label;
  } else if (warning.kind === "VERSION_RECORD_NOT_FIRST") {
    const line: number = warning.line;
    void line;
  } else if (warning.kind === "EPOCH_MISMATCH") {
    const label: string = warning.label;
    const declaredEpoch: IonexDiagnosticEpoch = warning.declaredEpoch;
    const mapsEpoch: IonexDiagnosticEpoch = warning.mapsEpoch;
    const declaredSeconds: string | null = declaredEpoch.j2000Seconds;
    const declaredNumber: number | null = declaredEpoch.j2000SecondsNumber;
    const declaredF64: number = declaredEpoch.j2000SecondsF64;
    void label;
    void mapsEpoch;
    void declaredSeconds;
    void declaredNumber;
    void declaredF64;
  } else if (warning.kind === "MAP_COUNT_MISMATCH") {
    // The count is an exact decimal string; the `number` beside it is null
    // where the engine's u64 would not survive the conversion.
    const declaredCount: string = warning.declaredCount;
    const declaredCountNumber: number | null = warning.declaredCountNumber;
    const tecMaps: number = warning.tecMaps;
    const allMaps: number = warning.allMaps;
    void declaredCount;
    void declaredCountNumber;
    void tecMaps;
    void allMaps;
  } else if (warning.kind === "NOT_A_NUMBER_VALUE") {
    const dataKind: string = warning.dataKind;
    const latDeg: number = warning.latDeg;
    const lonDeg: number = warning.lonDeg;
    void dataKind;
    void latDeg;
    void lonDeg;
  } else if (warning.kind === "INTERVAL_MISMATCH") {
    const declaredS: number = warning.declaredS;
    const spacingS: string = warning.spacingS;
    const spacingSNumber: number | null = warning.spacingSNumber;
    void declaredS;
    void spacingS;
    void spacingSNumber;
  } else if (warning.kind === "EXPONENT_CARRIED_INTO_MAP") {
    const exponent: number = warning.exponent;
    const setByLine: number = warning.setByLine;
    void exponent;
    void setByLine;
  } else {
    const message: string = warning.message;
    void message;
  }

  if (decl.kind === "DECLARED") {
    const func: IonexMappingFunction = decl.function;
    const code: string = func.code;
    void code;
  }
}

function compileOnlyIonexCallSites(ionex: WebBindings.Ionex, bytes: Uint8Array) {
  // Every policy shape the binding documents, including omitting it entirely.
  const policyObject: IonexSlantPolicyInput = { coverage: "hold", missingNodes: "renormalize" };
  const policyClass: WebBindings.IonexSlantPolicy = WebBindings.IonexSlantPolicy.coverageHold()
    .withMissingRenormalize()
    .withMappingDeclared();
  const policyLike: IonexSlantPolicyLike = policyObject;

  const requests: IonexSlantRequest[] = [
    {
      latDeg: 0,
      lonDeg: 0,
      azimuthDeg: 0,
      elevationDeg: 90,
      epochJ2000S: 0,
      frequencyHz: 1575.42e6,
    },
  ];

  const batch: IonexSlantBatchResult[] = ionex.slantDelaysBatchResults(requests);
  const batchWithPolicy: IonexSlantBatchResult[] = ionex.slantDelaysBatchResults(
    requests,
    policyClass,
  );
  const evaluation: IonexSlantDelayEvaluation = ionex.slantDelayWithPolicy(
    0,
    0,
    0,
    90,
    0,
    1575.42e6,
    policyLike,
  );
  const scalar: number = ionex.slantDelay(0, 0, 0, 90, 0, 1575.42e6);
  const header: IonexHeader = ionex.header;
  const tecMaps: IonexMapCube = ionex.tecMaps;
  const rmsMaps: IonexMapCube | null = ionex.rmsMaps;
  const heightMaps: IonexMapCube | null = ionex.heightMaps;
  const mappingFunction: IonexMappingFunction | null = ionex.mappingFunction;
  const mappingDeclaration: IonexMappingDeclaration = ionex.mappingDeclaration;
  const gridSamples: TecGridSamples = ionex.tecGridSamples();
  const nodeSamples: TecSample[] = ionex.tecSamples();
  const text: string = ionex.toIonexString();
  const parsed: IonexParseResult = WebBindings.loadIonexWithWarnings(bytes);
  const warnings: IonexWarning[] = parsed.warnings;

  const samplesInput: TecGridSamplesInput = {
    mapEpochsJ2000S: gridSamples.mapEpochsJ2000S,
    latNodesDeg: gridSamples.latNodesDeg,
    lonNodesDeg: gridSamples.lonNodesDeg,
    dlatDeg: gridSamples.dlatDeg,
    dlonDeg: gridSamples.dlonDeg,
    shellHeightKm: gridSamples.shellHeightKm,
    baseRadiusKm: gridSamples.baseRadiusKm,
    exponent: gridSamples.exponent,
    tecMaps: gridSamples.tecMaps,
    rmsMaps: gridSamples.rmsMaps,
    heightMaps: gridSamples.heightMaps,
    header: gridSamples.header,
  };
  const rebuilt: WebBindings.Ionex = WebBindings.ionexFromSamples(samplesInput);
  const fromNodes: WebBindings.Ionex = WebBindings.ionexFromNodeSamples(nodeSamples, 450, 6371, 0);

  // The standalone grid: an exact bigint epoch beside f64 coordinate axes.
  const epoch: WebBindings.TecGridEpoch = new WebBindings.TecGridEpoch(0n, 1);
  const unixNanos: bigint = epoch.unixNanos;
  const unixNanosString: string = epoch.unixNanosString;
  const dayOfYear: number = epoch.dayOfYear;
  const grid = new WebBindings.TecGrid([0, 1], [0, 1], [0, 1], [1, null, 1, 1, 1, 1, 1, 1]);
  const gridValues: (number | null)[] = grid.values;
  const vtec: number = grid.vtecAtPiercePoint(epoch, 0.5, 0.5);
  const policyVtec: TecGridEvaluation<number> = grid.vtecAtPiercePointWithPolicy(
    epoch,
    0.5,
    0.5,
    "renormalize",
  );
  const options = WebBindings.TecGridEvalOptions.l1(epoch);
  const convert: EcefToLla = (xyz: number[]) => [xyz[0], xyz[1], xyz[2]];
  const delay: number = WebBindings.ionoDelayXyz(grid, options, [1, 0, 0], [1, 0, 0], convert);
  const tec: Float64Array = WebBindings.tecXyz(grid, options, [1, 0, 0], [1, 0, 0]);
  const tecPolicy: TecGridEvaluation<[number, number]> = WebBindings.tecXyzWithPolicy(
    grid,
    options,
    [1, 0, 0],
    [1, 0, 0],
    "strict",
  );
  const coverage: IonexCoveragePolicy = policyClass.coverage;
  const missingNodes: IonexMissingNodePolicy = policyClass.missingNodes;

  void batch;
  void batchWithPolicy;
  void evaluation;
  void scalar;
  void header;
  void tecMaps;
  void rmsMaps;
  void heightMaps;
  void mappingFunction;
  void mappingDeclaration;
  void text;
  void warnings;
  void rebuilt;
  void fromNodes;
  void unixNanos;
  void unixNanosString;
  void dayOfYear;
  void gridValues;
  void vtec;
  void policyVtec;
  void delay;
  void tec;
  void tecPolicy;
  void coverage;
  void missingNodes;
}

// Nothing on the IONEX boundary may be inferred as `any`.
type _WarningNotAny = Assert<Equal<IsAny<IonexWarning>, false>>;
type _BatchResultNotAny = Assert<Equal<IsAny<IonexSlantBatchResult>, false>>;
type _RefusalNotAny = Assert<Equal<IsAny<IonexSlantRefusal>, false>>;
type _HeaderNotAny = Assert<Equal<IsAny<IonexHeader>, false>>;
type _MappingFunctionNotAny = Assert<Equal<IsAny<IonexMappingFunction>, false>>;
type _MappingDeclarationNotAny = Assert<Equal<IsAny<IonexMappingDeclaration>, false>>;

// The browser declaration target, compared name for name against the types
// `sidereon/types` re-exports.
type _WebHeaderGetter = Assert<Equal<WebBindings.Ionex["header"], IonexHeader>>;
type _WebTecMapsGetter = Assert<Equal<WebBindings.Ionex["tecMaps"], IonexMapCube>>;
type _WebRmsMapsGetter = Assert<Equal<WebBindings.Ionex["rmsMaps"], IonexMapCube | null>>;
type _WebHeightMapsGetter = Assert<Equal<WebBindings.Ionex["heightMaps"], IonexMapCube | null>>;
type _WebMappingFunctionGetter = Assert<
  Equal<WebBindings.Ionex["mappingFunction"], IonexMappingFunction | null>
>;
type _WebMappingDeclarationGetter = Assert<
  Equal<WebBindings.Ionex["mappingDeclaration"], IonexMappingDeclaration>
>;
type _WebSlantWithPolicyReturn = Assert<
  Equal<ReturnType<WebBindings.Ionex["slantDelayWithPolicy"]>, IonexSlantDelayEvaluation>
>;
type _WebBatchReturn = Assert<
  Equal<ReturnType<WebBindings.Ionex["slantDelaysBatchResults"]>, IonexSlantBatchResult[]>
>;
type _WebBatchRequestParam = Assert<
  Equal<Parameters<WebBindings.Ionex["slantDelaysBatchResults"]>[0], IonexSlantRequest[]>
>;
type _WebGridSamplesReturn = Assert<
  Equal<ReturnType<WebBindings.Ionex["tecGridSamples"]>, TecGridSamples>
>;
type _WebTecSamplesReturn = Assert<Equal<ReturnType<WebBindings.Ionex["tecSamples"]>, TecSample[]>>;
type _WebParseWithWarningsReturn = Assert<
  Equal<ReturnType<typeof WebBindings.loadIonexWithWarnings>, IonexParseResult>
>;
type _WebPolicyCoverageGetter = Assert<
  Equal<WebBindings.IonexSlantPolicy["coverage"], IonexCoveragePolicy>
>;
type _WebPolicyMissingGetter = Assert<
  Equal<WebBindings.IonexSlantPolicy["missingNodes"], IonexMissingNodePolicy>
>;
type _WebEpochBigInt = Assert<Equal<WebBindings.TecGridEpoch["unixNanos"], bigint>>;
type _WebGridValues = Assert<Equal<WebBindings.TecGrid["values"], (number | null)[]>>;
// The three axes are `Vec<f64>` in the engine and cross as typed arrays;
// only `values` can hold `null`, which no typed array expresses.
type _WebGridEpochsAxis = Assert<Equal<WebBindings.TecGrid["epochsNs"], Float64Array>>;
type _WebGridLatitudeAxis = Assert<Equal<WebBindings.TecGrid["latitudesDeg"], Float64Array>>;
type _WebGridLongitudeAxis = Assert<Equal<WebBindings.TecGrid["longitudesDeg"], Float64Array>>;
type _WebTecXyzReturn = Assert<Equal<ReturnType<typeof WebBindings.tecXyz>, Float64Array>>;
type _WebTecXyzPolicyReturn = Assert<
  Equal<ReturnType<typeof WebBindings.tecXyzWithPolicy>, TecGridEvaluation<[number, number]>>
>;

// The Node declaration target carries the same generated types.
type _NodeHeaderGetter = Assert<Equal<NodeBindings.Ionex["header"], NodeIonexHeader>>;
type _NodeTecMapsGetter = Assert<Equal<NodeBindings.Ionex["tecMaps"], NodeIonexMapCube>>;
type _NodeRmsMapsGetter = Assert<Equal<NodeBindings.Ionex["rmsMaps"], NodeIonexMapCube | null>>;
type _NodeSlantWithPolicyReturn = Assert<
  Equal<ReturnType<NodeBindings.Ionex["slantDelayWithPolicy"]>, NodeIonexSlantDelayEvaluation>
>;
type _NodeBatchReturn = Assert<
  Equal<ReturnType<NodeBindings.Ionex["slantDelaysBatchResults"]>, NodeIonexSlantBatchResult[]>
>;
type _NodeBatchRequestParam = Assert<
  Equal<Parameters<NodeBindings.Ionex["slantDelaysBatchResults"]>[0], NodeIonexSlantRequest[]>
>;
type _NodeGridSamplesReturn = Assert<
  Equal<ReturnType<NodeBindings.Ionex["tecGridSamples"]>, NodeTecGridSamples>
>;
type _NodeTecSamplesReturn = Assert<
  Equal<ReturnType<NodeBindings.Ionex["tecSamples"]>, NodeTecSample[]>
>;
type _NodeParseWithWarningsReturn = Assert<
  Equal<ReturnType<typeof NodeBindings.loadIonexWithWarnings>, NodeIonexParseResult>
>;
type _NodeEpochBigInt = Assert<Equal<NodeBindings.TecGridEpoch["unixNanos"], bigint>>;
type _NodeGridValues = Assert<Equal<NodeBindings.TecGrid["values"], (number | null)[]>>;
type _NodeGridEpochsAxis = Assert<Equal<NodeBindings.TecGrid["epochsNs"], Float64Array>>;
type _NodeGridLatitudeAxis = Assert<Equal<NodeBindings.TecGrid["latitudesDeg"], Float64Array>>;
type _NodeGridLongitudeAxis = Assert<Equal<NodeBindings.TecGrid["longitudesDeg"], Float64Array>>;
type _NodeTecXyzReturn = Assert<Equal<ReturnType<typeof NodeBindings.tecXyz>, Float64Array>>;
type _NodeTecXyzPolicyReturn = Assert<
  Equal<ReturnType<typeof NodeBindings.tecXyzWithPolicy>, NodeTecGridEvaluation<[number, number]>>
>;
type _NodeHeaderNotAny = Assert<Equal<IsAny<NodeBindings.Ionex["header"]>, false>>;
type _NodeBatchNotAny = Assert<
  Equal<IsAny<ReturnType<NodeBindings.Ionex["slantDelaysBatchResults"]>>, false>
>;
type _NodeParseNotAny = Assert<
  Equal<IsAny<ReturnType<typeof NodeBindings.loadIonexWithWarnings>>, false>
>;

// The discriminated unions narrow to one payload each.
type _EpochMismatchDeclaredEpoch = Assert<
  Equal<Extract<IonexWarning, { kind: "EPOCH_MISMATCH" }>["declaredEpoch"], IonexDiagnosticEpoch>
>;
type _MapCountMismatchDeclaredCount = Assert<
  Equal<Extract<IonexWarning, { kind: "MAP_COUNT_MISMATCH" }>["declaredCount"], string>
>;
type _MapCountMismatchDeclaredNumber = Assert<
  Equal<Extract<IonexWarning, { kind: "MAP_COUNT_MISMATCH" }>["declaredCountNumber"], number | null>
>;
type _IntervalMismatchSpacing = Assert<
  Equal<Extract<IonexWarning, { kind: "INTERVAL_MISMATCH" }>["spacingS"], string>
>;
// The two variants carry disjoint payloads: no property is a diagnostic epoch
// in one and a count in the other.
type _MapCountHasNoEpoch = Assert<
  Equal<
    "declaredEpoch" extends keyof Extract<IonexWarning, { kind: "MAP_COUNT_MISMATCH" }>
      ? true
      : false,
    false
  >
>;
type _EpochHasNoCount = Assert<
  Equal<
    "declaredCount" extends keyof Extract<IonexWarning, { kind: "EPOCH_MISMATCH" }> ? true : false,
    false
  >
>;
type _CoverageRefusalNarrow = Assert<
  Equal<Extract<IonexSlantRefusal, { kind: "COVERAGE" }>["coverageError"], IonexCoverageError>
>;
type _MissingNodesRefusalNarrow = Assert<
  Equal<Extract<IonexSlantRefusal, { kind: "MISSING_NODES" }>["nodeGap"], IonexNodeGap>
>;
type _InvalidInputHasNoNodeGap = Assert<
  Equal<
    "nodeGap" extends keyof Extract<IonexSlantRefusal, { kind: "INVALID_INPUT" }> ? true : false,
    false
  >
>;
type _DeclaredMappingNarrow = Assert<
  Equal<Extract<IonexMappingDeclaration, { kind: "DECLARED" }>["function"], IonexMappingFunction>
>;
type _OtherMappingCode = Assert<
  Equal<Extract<IonexMappingFunction, { kind: "OTHER" }>["code"], string>
>;
type _CoszMappingCode = Assert<
  Equal<Extract<IonexMappingFunction, { kind: "COSZ" }>["code"], "COSZ">
>;
type _NoMappingCode = Assert<
  Equal<Extract<IonexMappingFunction, { kind: "NO_MAPPING" }>["code"], "NONE">
>;
type _QFactorMappingCode = Assert<
  Equal<Extract<IonexMappingFunction, { kind: "Q_FACTOR" }>["code"], "QFAC">
>;

// --- RINEX OBS, TDM, SP3 and ANTEX writers ------------------------------------
//
// These declarations live in the `typescript_custom_section` blocks of
// `src/rinex_obs.rs`, `src/tdm.rs`, `src/sp3.rs` and `src/antex.rs`. The
// aliases below read them through `types/sidereon-extra.js`, the browser
// target's copies, as the IONEX section's imports do.

type CarrierPhaseShift = import("../types/sidereon-extra.js").CarrierPhaseShift;
type CorrectionStatus = import("../types/sidereon-extra.js").CorrectionStatus;
type GlonassCodePhaseBias = import("../types/sidereon-extra.js").GlonassCodePhaseBias;
type ObsDowngradeChange = import("../types/sidereon-extra.js").ObsDowngradeChange;
type ObsGlonassBias = import("../types/sidereon-extra.js").ObsGlonassBias;
type ObsPrnObsCount = import("../types/sidereon-extra.js").ObsPrnObsCount;
type ObsSatelliteValues = import("../types/sidereon-extra.js").ObsSatelliteValues;
type ObsScaleFactor = import("../types/sidereon-extra.js").ObsScaleFactor;
type ObsValue = import("../types/sidereon-extra.js").ObsValue;
type RinexObsDowngrade = import("../types/sidereon-extra.js").RinexObsDowngrade;
type RinexObsWriteErrorDetail = import("../types/sidereon-extra.js").RinexObsWriteErrorDetail;
type Sp3WriteErrorDetail = import("../types/sidereon-extra.js").Sp3WriteErrorDetail;
type Sp3AccuracyValue = import("../types/sidereon-extra.js").Sp3AccuracyValue;
type Sp3RawRecordAccuracy = import("../types/sidereon-extra.js").Sp3RawRecordAccuracy;
type Sp3RecordAccuracy = import("../types/sidereon-extra.js").Sp3RecordAccuracy;
type Sp3PreciseEphemerisAccuracySample =
  import("../types/sidereon-extra.js").Sp3PreciseEphemerisAccuracySample;
type Sp3PreciseEphemerisSample = import("../types/sidereon-extra.js").Sp3PreciseEphemerisSample;
type PreciseSamplesErrorDetail = import("../types/sidereon-extra.js").PreciseSamplesErrorDetail;
type SsrCorrectionSize = import("../types/sidereon-extra.js").SsrCorrectionSize;
type SsrOversizedCorrection = import("../types/sidereon-extra.js").SsrOversizedCorrection;
type AntexWriteErrorDetail = import("../types/sidereon-extra.js").AntexWriteErrorDetail;
type TdmComment = import("../types/sidereon-extra.js").TdmComment;
type TdmDeparture = import("../types/sidereon-extra.js").TdmDeparture;
type TdmErrorDetail = import("../types/sidereon-extra.js").TdmErrorDetail;
type TdmFieldInput = import("../types/sidereon-extra.js").TdmFieldInput;
type TdmMetadataResult = import("../types/sidereon-extra.js").TdmMetadataResult;
type TdmParseResult = import("../types/sidereon-extra.js").TdmParseResult;
type TdmPolicyLike = import("../types/sidereon-extra.js").TdmPolicyLike;
type TdmWarning = import("../types/sidereon-extra.js").TdmWarning;
type TdmWritePolicyLike = import("../types/sidereon-extra.js").TdmWritePolicyLike;
type TdmWriteResult = import("../types/sidereon-extra.js").TdmWriteResult;
type NodeRinexObsDowngrade = import("../pkg-node/sidereon.js").RinexObsDowngrade;
type NodeTdmComment = import("../pkg-node/sidereon.js").TdmComment;
type NodeTdmParseResult = import("../pkg-node/sidereon.js").TdmParseResult;
type NodeCarrierPhaseShift = import("../pkg-node/sidereon.js").CarrierPhaseShift;

function compileOnlyObservationContracts(
  obs: WebBindings.RinexObs,
  change: ObsDowngradeChange,
  detail: RinexObsWriteErrorDetail,
  shift: CarrierPhaseShift,
  bias: GlonassCodePhaseBias,
) {
  const text: string = obs.toRinexString();
  const downgrade: RinexObsDowngrade = obs.downgradeToRinex2(2.11);
  const downgraded: WebBindings.RinexObs = downgrade.obs;
  const sameProduct: WebBindings.RinexObs = downgrade.value;
  const changes: ObsDowngradeChange[] = downgrade.changes;
  const skipped: number = obs.skippedRecords;
  const epoch: WebBindings.ObsEpoch = obs.epoch(0);
  // An untimed event epoch has no civil time.
  const civil: WebBindings.ObsEpochTime | undefined = epoch.epoch;
  const specialRecords: string[] = epoch.specialRecords;
  const slips: ObsSatelliteValues[] = epoch.cycleSlips;
  const observations: ObsSatelliteValues[] = epoch.observations;
  const firstValue: ObsValue = observations[0].values[0];
  const value: number | null = firstValue.value;
  const lli: number | null = firstValue.lli;
  const picoseconds: number | undefined = epoch.epochPicoseconds;
  const clock: number | undefined = epoch.rcvClockOffsetS;
  const header: WebBindings.ObsHeader = obs.headerAt(0);
  const timeline: WebBindings.ObsHeaderTimeline = obs.headerTimeline();
  const segments: WebBindings.ObsHeaderSegment[] = timeline.segments;
  const firstEpochIndex: number = segments[0].firstEpochIndex;
  const declared: string[] | undefined = header.declaredObsCodes(WebBindings.GnssSystem.Gps);
  const rinex2System: WebBindings.GnssSystem | undefined = header.rinex2System;
  const leap: WebBindings.ObsLeapSeconds | undefined = header.leapSeconds;
  const leapCurrent: bigint | undefined = leap?.current;
  const leapWeek: bigint | undefined = leap?.week;
  const leapSystem: string | undefined = leap?.timeSystem;
  const scaleFactors: ObsScaleFactor[] = header.scaleFactors;
  const prnCounts: ObsPrnObsCount[] = header.prnObsCounts;
  const blankCount: number | null = prnCounts[0].counts[0];
  const biases: ObsGlonassBias[] | null = header.glonassCodPhsBis;
  const phaseShift: WebBindings.ObsPhaseShift = header.phaseShifts[0];
  const shiftCode: string | undefined = phaseShift.code;
  const shiftCorrection: number | undefined = phaseShift.correctionCycles;
  const unrepresentable: string[] = phaseShift.unrepresentableSatellites;
  const coversEvery: boolean = phaseShift.coversEverySatellite;
  const phase: WebBindings.CarrierPhaseSeries = obs.carrierPhaseRows(0);
  const shiftCycles: Float64Array = phase.phaseShiftCycles;
  const shiftAvailable: Uint8Array = phase.phaseShiftAvailable;
  const statuses: CorrectionStatus[] = phase.phaseShiftStatus;
  const corrections: CarrierPhaseShift[] = phase.phaseShiftCorrections;
  const glonassBias: GlonassCodePhaseBias = header.glonassCodePhaseBias("C1C");
  const cycleSlipFlag: number = WebBindings.rinexObsCycleSlipFlag();

  if (change.kind === "IN_EVENT_LISTS") {
    const nested: ObsDowngradeChange = change.change;
    const eventEpoch: number = change.epochIndex;
    void nested;
    void eventEpoch;
  } else if (change.kind === "VALUE_ROUNDED") {
    const from: number = change.from;
    const satellite: string = change.satellite;
    void from;
    void satellite;
  } else if (change.kind === "DEPRECATED_RECORDS_REMOVED") {
    const where: number | null = change.epochIndex;
    void where;
  }

  if (detail.kind === "OBSERVABLE_NOT_REPRESENTABLE") {
    const system: string = detail.system;
    const code: string = detail.code;
    const version: number = detail.version;
    void system;
    void code;
    void version;
  } else if (detail.kind === "VALUE_OUTSIDE_DECLARED_LIST") {
    const code: string | null = detail.code;
    void code;
  } else if (detail.kind === "EVENT_RECORDS_UNREADABLE") {
    const readerError: string = detail.readerError;
    void readerError;
  } else if (detail.kind === "LEAP_SECONDS_TIME_SYSTEM_NOT_IN_VERSION") {
    const timeSystem: string = detail.timeSystem;
    void timeSystem;
  }

  if (shift.status === "ambiguous") {
    const conflicting: (number | null)[] = shift.corrections;
    void conflicting;
  } else if (shift.status === "available") {
    const cycles: number = shift.cycles;
    void cycles;
  }

  if (bias.status === "available") {
    const biasM: number = bias.biasM;
    void biasM;
  } else if (bias.status === "ambiguous") {
    const biasesM: (number | null)[] = bias.biasesM;
    void biasesM;
  }

  void text;
  void downgraded;
  void sameProduct;
  void changes;
  void skipped;
  void civil;
  void specialRecords;
  void slips;
  void value;
  void lli;
  void picoseconds;
  void clock;
  void firstEpochIndex;
  void declared;
  void rinex2System;
  void leapCurrent;
  void leapWeek;
  void leapSystem;
  void scaleFactors;
  void blankCount;
  void biases;
  void shiftCode;
  void shiftCorrection;
  void unrepresentable;
  void coversEvery;
  void shiftCycles;
  void shiftAvailable;
  void statuses;
  void corrections;
  void glonassBias;
  void cycleSlipFlag;
}

function compileOnlyTdmContracts(
  tdm: WebBindings.Tdm,
  metadata: WebBindings.TdmMetadata,
  detail: TdmErrorDetail,
  warning: TdmWarning,
  departure: TdmDeparture,
) {
  const headerComments: TdmComment[] = tdm.comments;
  const beforeRecord: number = headerComments[0].beforeRecord;
  const metadataComments: TdmComment[] = metadata.comments;
  const dataComments: TdmComment[] = tdm.segments[0].data.comments;
  const fields: TdmFieldInput[] = [
    { key: "TIME_SYSTEM", value: "UTC" },
    new WebBindings.TdmField("PARTICIPANT_1", "DSS-25"),
  ];
  const built: WebBindings.TdmMetadata = WebBindings.TdmMetadata.fromRaw(fields, [
    { text: "note", beforeRecord: 0 },
  ]);
  const withoutComments: WebBindings.TdmMetadata = WebBindings.TdmMetadata.fromRaw(fields);
  const readPolicy: TdmPolicyLike = { keywordOrder: "forgive" };
  const writePolicy: TdmWritePolicyLike = { repeatedKeywords: "forgive", keywordOrder: "strict" };
  const result: TdmMetadataResult = WebBindings.TdmMetadata.fromRawWithPolicy(
    fields,
    null,
    writePolicy,
  );
  const resultMetadata: WebBindings.TdmMetadata = result.metadata;
  const resultDepartures: TdmDeparture[] = result.departures;
  metadata.replaceRaw(fields, metadataComments);
  const replaced: TdmDeparture[] = metadata.replaceRawWithPolicy(fields, undefined, "lenient");
  tdm.setSegmentMetadata(0, metadata);
  const parsed: TdmParseResult = WebBindings.parseTdmKvnWithPolicy("", readPolicy);
  const parsedTdm: WebBindings.Tdm = parsed.tdm;
  const warnings: TdmWarning[] = parsed.warnings;
  const written: TdmWriteResult = tdm.toKvnStringWithPolicy(writePolicy);
  const writtenText: string = written.text;
  const strictText: string = tdm.toKvnString();

  if (detail.kind === "CONFLICTING_KEYWORD") {
    const line: number | null = detail.line;
    const first: string = detail.first;
    const second: string = detail.second;
    void line;
    void first;
    void second;
  } else if (detail.kind === "INVALID_FIELD") {
    const inputErrorKind: string = detail.inputErrorKind;
    void inputErrorKind;
  } else if (detail.kind === "MISSING_KEYWORD") {
    const segment: number | null = detail.segment;
    void segment;
  }
  if (warning.kind === "KEYWORD_OUT_OF_ORDER") {
    const line: number = warning.line;
    const section: string = warning.section;
    void line;
    void section;
  }
  if (departure.kind === "KEYWORD_OUT_OF_ORDER") {
    const keyword: string = departure.keyword;
    void keyword;
  }

  void beforeRecord;
  void dataComments;
  void built;
  void withoutComments;
  void resultMetadata;
  void resultDepartures;
  void replaced;
  void parsedTdm;
  void warnings;
  void writtenText;
  void strictText;
}

function compileOnlyWriterContracts(
  sp3: WebBindings.Sp3,
  antex: WebBindings.Antex,
  sp3Detail: Sp3WriteErrorDetail,
  antexDetail: AntexWriteErrorDetail,
  metric: WebBindings.Sp3AgreementMetric,
) {
  const sp3Text: string = sp3.toSp3String();
  const antexText: string = antex.toAntexString();
  // A clock-only merged cell carries no position, so its dispersion is absent.
  const positionRms: number | undefined = metric.positionRmsM;
  const positionMax: number | undefined = metric.positionMaxM;
  if (sp3Detail.kind === "PRECISION_NOT_REPRESENTABLE") {
    const columns: number = sp3Detail.columns;
    const decimals: number = sp3Detail.decimals;
    const value: number = sp3Detail.value;
    void columns;
    void decimals;
    void value;
  } else if (sp3Detail.kind === "EPOCH_NOT_RESTATABLE") {
    const residualS: number | null = sp3Detail.residualS;
    void residualS;
  } else if (sp3Detail.kind === "INTEGER_TOO_WIDE") {
    const exact: string = sp3Detail.value;
    const asNumber: number | null = sp3Detail.valueNumber;
    void exact;
    void asNumber;
  }
  if (antexDetail.kind === "UNWRITABLE") {
    const field: string = antexDetail.field;
    const reason: string = antexDetail.reason;
    void field;
    void reason;
  }
  void sp3Text;
  void antexText;
  void positionRms;
  void positionMax;
}

// Nothing on these boundaries may be inferred as `any`.
type _ObsDowngradeNotAny = Assert<Equal<IsAny<RinexObsDowngrade>, false>>;
type _ObsWriteDetailNotAny = Assert<Equal<IsAny<RinexObsWriteErrorDetail>, false>>;
type _ObsChangeNotAny = Assert<Equal<IsAny<ObsDowngradeChange>, false>>;
type _PhaseShiftNotAny = Assert<Equal<IsAny<CarrierPhaseShift>, false>>;
type _TdmDetailNotAny = Assert<Equal<IsAny<TdmErrorDetail>, false>>;
type _TdmCommentNotAny = Assert<Equal<IsAny<TdmComment>, false>>;
type _Sp3DetailNotAny = Assert<Equal<IsAny<Sp3WriteErrorDetail>, false>>;
type _Sp3AccuracyDetailFields = Assert<
  Equal<
    Extract<Sp3WriteErrorDetail, { kind: "ACCURACY_NOT_REPRESENTABLE" }>,
    {
      kind: "ACCURACY_NOT_REPRESENTABLE";
      satellite: string;
      epochIndex: number;
      component: string;
      exponent: number | null;
      message: string;
    }
  >
>;
type _Sp3AccuracyMismatchEpoch = Assert<
  Equal<Extract<Sp3WriteErrorDetail, { kind: "ACCURACY_RECORD_MISMATCH" }>["epochIndex"], number>
>;
type _Sp3AccuracyBasisEpoch = Assert<
  Equal<Extract<Sp3WriteErrorDetail, { kind: "ACCURACY_BASIS_MISSING" }>["epochIndex"], number>
>;
type _AntexDetailNotAny = Assert<Equal<IsAny<AntexWriteErrorDetail>, false>>;

// The browser declaration target.
type _WebDowngradeReturn = Assert<
  Equal<ReturnType<WebBindings.RinexObs["downgradeToRinex2"]>, RinexObsDowngrade>
>;
type _WebToRinexReturn = Assert<Equal<ReturnType<WebBindings.RinexObs["toRinexString"]>, string>>;
type _WebEpochTime = Assert<
  Equal<WebBindings.ObsEpoch["epoch"], WebBindings.ObsEpochTime | undefined>
>;
type _WebCycleSlips = Assert<Equal<WebBindings.ObsEpoch["cycleSlips"], ObsSatelliteValues[]>>;
type _WebLeapCurrent = Assert<Equal<WebBindings.ObsLeapSeconds["current"], bigint>>;
type _WebLeapTimeSystem = Assert<
  Equal<WebBindings.ObsLeapSeconds["timeSystem"], string | undefined>
>;
type _WebGlonassBiases = Assert<
  Equal<WebBindings.ObsHeader["glonassCodPhsBis"], ObsGlonassBias[] | null>
>;
type _WebPhaseShiftCode = Assert<Equal<WebBindings.ObsPhaseShift["code"], string | undefined>>;
type _WebPhaseShiftCorrection = Assert<
  Equal<WebBindings.ObsPhaseShift["correctionCycles"], number | undefined>
>;
type _WebPhaseStatus = Assert<
  Equal<WebBindings.CarrierPhaseSeries["phaseShiftStatus"], CorrectionStatus[]>
>;
type _WebPhaseCorrections = Assert<
  Equal<WebBindings.CarrierPhaseSeries["phaseShiftCorrections"], CarrierPhaseShift[]>
>;
type _WebPhaseAvailable = Assert<
  Equal<WebBindings.CarrierPhaseSeries["phaseShiftAvailable"], Uint8Array>
>;
type _WebGlonassBias = Assert<
  Equal<ReturnType<WebBindings.ObsHeader["glonassCodePhaseBias"]>, GlonassCodePhaseBias>
>;
type _WebTdmComments = Assert<Equal<WebBindings.Tdm["comments"], TdmComment[]>>;
type _WebTdmMetadataComments = Assert<Equal<WebBindings.TdmMetadata["comments"], TdmComment[]>>;
type _WebTdmDataComments = Assert<Equal<WebBindings.TdmDataSection["comments"], TdmComment[]>>;
type _WebTdmFromRawFields = Assert<
  Equal<Parameters<typeof WebBindings.TdmMetadata.fromRaw>[0], TdmFieldInput[]>
>;
type _WebTdmFromRawWithPolicy = Assert<
  Equal<ReturnType<typeof WebBindings.TdmMetadata.fromRawWithPolicy>, TdmMetadataResult>
>;
type _WebTdmReplaceWithPolicy = Assert<
  Equal<ReturnType<WebBindings.TdmMetadata["replaceRawWithPolicy"]>, TdmDeparture[]>
>;
type _WebTdmParseWithPolicy = Assert<
  Equal<ReturnType<typeof WebBindings.parseTdmKvnWithPolicy>, TdmParseResult>
>;
type _WebTdmWriteWithPolicy = Assert<
  Equal<ReturnType<WebBindings.Tdm["toKvnStringWithPolicy"]>, TdmWriteResult>
>;
type _WebAgreementRms = Assert<
  Equal<WebBindings.Sp3AgreementMetric["positionRmsM"], number | undefined>
>;
type _WebSp3Write = Assert<Equal<ReturnType<WebBindings.Sp3["toSp3String"]>, string>>;
type _WebAntexWrite = Assert<Equal<ReturnType<WebBindings.Antex["toAntexString"]>, string>>;

// The Node declaration target carries the same generated types.
type _NodeDowngradeReturn = Assert<
  Equal<ReturnType<NodeBindings.RinexObs["downgradeToRinex2"]>, NodeRinexObsDowngrade>
>;
type _NodeEpochTime = Assert<
  Equal<NodeBindings.ObsEpoch["epoch"], NodeBindings.ObsEpochTime | undefined>
>;
type _NodePhaseCorrections = Assert<
  Equal<NodeBindings.CarrierPhaseSeries["phaseShiftCorrections"], NodeCarrierPhaseShift[]>
>;
type _NodeTdmComments = Assert<Equal<NodeBindings.Tdm["comments"], NodeTdmComment[]>>;
type _NodeTdmParseWithPolicy = Assert<
  Equal<ReturnType<typeof NodeBindings.parseTdmKvnWithPolicy>, NodeTdmParseResult>
>;
type _NodeAgreementRms = Assert<
  Equal<NodeBindings.Sp3AgreementMetric["positionRmsM"], number | undefined>
>;
type _NodeDowngradeNotAny = Assert<
  Equal<IsAny<ReturnType<NodeBindings.RinexObs["downgradeToRinex2"]>>, false>
>;

// The discriminated unions narrow to one payload each.
type _InEventListsNests = Assert<
  Equal<Extract<ObsDowngradeChange, { kind: "IN_EVENT_LISTS" }>["change"], ObsDowngradeChange>
>;
type _ObservableNotRepresentableCode = Assert<
  Equal<Extract<RinexObsWriteErrorDetail, { kind: "OBSERVABLE_NOT_REPRESENTABLE" }>["code"], string>
>;
type _ScaleFactorsHasNoCode = Assert<
  Equal<
    "code" extends keyof Extract<RinexObsWriteErrorDetail, { kind: "SCALE_FACTORS_IN_VERSION_TWO" }>
      ? true
      : false,
    false
  >
>;
type _AmbiguousCorrections = Assert<
  Equal<Extract<CarrierPhaseShift, { status: "ambiguous" }>["corrections"], (number | null)[]>
>;
type _UnknownShiftHasNoCycles = Assert<
  Equal<
    "cycles" extends keyof Extract<CarrierPhaseShift, { status: "unknown" }> ? true : false,
    false
  >
>;
type _TdmConflictingSecond = Assert<
  Equal<Extract<TdmErrorDetail, { kind: "CONFLICTING_KEYWORD" }>["second"], string>
>;
type _Sp3ResidualNullable = Assert<
  Equal<Extract<Sp3WriteErrorDetail, { kind: "EPOCH_NOT_RESTATABLE" }>["residualS"], number | null>
>;
type _Sp3SatelliteNotRepresentablePrn = Assert<
  Equal<Extract<Sp3WriteErrorDetail, { kind: "SATELLITE_NOT_REPRESENTABLE" }>["prn"], number>
>;

// --- RINEX clock, ANTEX, terrain, BLQ, positioning, RTCM and SBAS ---------------
//
// These declarations live in the `typescript_custom_section` blocks of
// `src/rinex_clock.rs`, `src/antex.rs`, `src/terrain.rs`, `src/tides.rs`,
// `src/spp.rs`, `src/rtcm.rs` and `src/sbas.rs`, read here through
// `types/sidereon-extra.js` as the sections above read theirs.

type AntexCalibration = import("../types/sidereon-extra.js").AntexCalibration;
type AntexErrorDetail = import("../types/sidereon-extra.js").AntexErrorDetail;
type AntexFrequency = import("../types/sidereon-extra.js").AntexFrequency;
type AntexFrequencyRms = import("../types/sidereon-extra.js").AntexFrequencyRms;
type AntexHeader = import("../types/sidereon-extra.js").AntexHeader;
type AntexOuterComment = import("../types/sidereon-extra.js").AntexOuterComment;
type AntexPcvSample = import("../types/sidereon-extra.js").AntexPcvSample;
type AntexPcvType = import("../types/sidereon-extra.js").AntexPcvType;
type BlqParseErrorDetail = import("../types/sidereon-extra.js").BlqParseErrorDetail;
type BlqParseReason = import("../types/sidereon-extra.js").BlqParseReason;
type BlqWriteErrorDetail = import("../types/sidereon-extra.js").BlqWriteErrorDetail;
type DtedHorizontalDatum = import("../types/sidereon-extra.js").DtedHorizontalDatum;
type OceanLoadingBlqBlock = import("../types/sidereon-extra.js").OceanLoadingBlqBlock;
type OceanLoadingBlqComment = import("../types/sidereon-extra.js").OceanLoadingBlqComment;
type RinexClockDiagnostic = import("../types/sidereon-extra.js").RinexClockDiagnostic;
type RinexClockErrorDetail = import("../types/sidereon-extra.js").RinexClockErrorDetail;
type RinexClockHeaderField = import("../types/sidereon-extra.js").RinexClockHeaderField;
type RinexClockHeaderRecord = import("../types/sidereon-extra.js").RinexClockHeaderRecord;
type RinexClockInstant = import("../types/sidereon-extra.js").RinexClockInstant;
type RinexClockNotice = import("../types/sidereon-extra.js").RinexClockNotice;
type RinexClockRecord = import("../types/sidereon-extra.js").RinexClockRecord;
type RinexClockSkip = import("../types/sidereon-extra.js").RinexClockSkip;
type RinexClockTimeSystemStatus = import("../types/sidereon-extra.js").RinexClockTimeSystemStatus;
type RinexClockWriteDeparture = import("../types/sidereon-extra.js").RinexClockWriteDeparture;
type RinexClockWriteResult = import("../types/sidereon-extra.js").RinexClockWriteResult;
type RtcmEncodeErrorDetail = import("../types/sidereon-extra.js").RtcmEncodeErrorDetail;
type SbasUnassignedMaskCorrections =
  import("../types/sidereon-extra.js").SbasUnassignedMaskCorrections;
type SppRejectedSatellite = import("../types/sidereon-extra.js").SppRejectedSatellite;
type SppRejectionReason = import("../types/sidereon-extra.js").SppRejectionReason;
type TerrainHeightBatchEntry = import("../types/sidereon-extra.js").TerrainHeightBatchEntry;
type TerrainLookupErrorDetail = import("../types/sidereon-extra.js").TerrainLookupErrorDetail;
type NodeRinexClockRecord = import("../pkg-node/sidereon.js").RinexClockRecord;

function compileOnlyRinexClockContracts(
  clock: WebBindings.RinexClock,
  detail: RinexClockErrorDetail,
  notice: RinexClockNotice,
  field: RinexClockHeaderField,
  departure: RinexClockWriteDeparture,
) {
  const records: RinexClockRecord[] = clock.records();
  const headers: RinexClockHeaderRecord[] = clock.headerRecords();
  const status: RinexClockTimeSystemStatus = clock.timeSystemStatus;
  const scale: WebBindings.TimeScale | undefined = clock.timeScale;
  const layout: "v300" | "v304" | undefined = clock.layout;
  const version: number | undefined = clock.version;
  const timeSystem: string | undefined = clock.timeSystem;
  const skipped: RinexClockSkip[] = clock.skippedRecords;
  const diagnostics: RinexClockDiagnostic[] = clock.diagnostics;
  const notices: RinexClockNotice[] = clock.notices;
  const sourceLine: string | undefined = clock.sourceLine(1);
  const strict: RinexClockWriteResult = clock.toRinexStringWithPolicy();
  const lenient: RinexClockWriteResult = clock.toRinexStringWithPolicy("lenient");
  const allowed: RinexClockWriteResult = clock.toRinexStringWithPolicy({
    nearestMicrosecondEpochs: "allow",
  });
  clock.setTimeSystem("GLO");
  clock.setRecordValues(0, [1.0e-4, 1.0e-11]);
  clock.insertRecord(clock.recordCount, {
    recordType: "AS",
    name: "G01",
    epoch: new WebBindings.ClockEpoch(2020, 1, 1, 0, 0, 0),
    values: Float64Array.of(1.0e-4),
  });
  const removed: RinexClockRecord = clock.removeRecord(0);
  const removedCount: number = clock.retainRecords((record) => record.recordType === "AS");
  const editedCount: number = clock.editRecords((record) =>
    record.index === 0 ? [record.values[0]] : null,
  );
  const built: WebBindings.RinexClock = WebBindings.RinexClock.fromClockPoints(
    WebBindings.TimeScale.Utc,
    [
      {
        satellite: "G01",
        points: [
          {
            epoch: { year: 2016, month: 12, day: 31, hour: 23, minute: 59, second: 60 },
            biasS: 1.0e-4,
            additionalValues: [1.0e-11],
          },
        ],
      },
    ],
  );
  const fromRows: WebBindings.RinexClock = WebBindings.RinexClock.fromSeriesRows([
    { satellite: "G01", gpsSeconds: [0], biasS: Float64Array.of(1.0e-4) },
  ]);
  const series: WebBindings.ClockSeries | undefined = clock.seriesFor("G01");
  if (series) {
    const epochs: RinexClockInstant[] = series.epochs;
    const hasGps: Uint8Array = series.hasGpsSeconds;
    const gps: Float64Array = series.gpsSeconds;
    const additional: number[][] = series.additionalValues;
    const seriesScale: WebBindings.TimeScale | undefined = series.timeScale;
    const nanos: string | null = epochs[0].nanos;
    void hasGps;
    void gps;
    void additional;
    void seriesScale;
    void nanos;
  }
  const gpsSeconds: number | undefined = new WebBindings.ClockEpoch(2020, 1, 1, 0, 0, 0).gpsSeconds;

  const record = records[0];
  const epoch: RinexClockInstant | null = record.epoch;
  const lines: string[] = record.sourceLines;
  const surplus: { position: number; value: number }[] = record.surplusValues;
  const line: number | null = record.line;

  if (status.kind === "CONFLICTING") {
    const labels: string[] = status.labels;
    void labels;
  }
  if (detail.kind === "BAD_FIELD") {
    const badLine: number = detail.line;
    const value: string = detail.value;
    void badLine;
    void value;
  } else if (detail.kind === "UNSUPPORTED_TIME_SCALE") {
    const unsupported: string = detail.scale;
    void unsupported;
  }
  if (notice.kind === "SURPLUS_VALUES") {
    const firstLine: number = notice.firstLine;
    void firstLine;
  }
  if (field.kind === "SOLUTION_STATION") {
    const exact: [string, string, string] = field.xyzMm;
    const asNumbers: [number | null, number | null, number | null] = field.xyzMmNumber;
    void exact;
    void asNumbers;
  } else if (field.kind === "CLOCK_REF_COUNT") {
    const start = field.start;
    const year: number | undefined = start?.year;
    void year;
  }
  if (departure.kind === "EPOCH_AT_NEAREST_MICROSECOND") {
    const written: string = departure.written;
    const departed: RinexClockInstant | null = departure.epoch;
    void written;
    void departed;
  }
  void headers;
  void scale;
  void layout;
  void version;
  void timeSystem;
  void skipped;
  void diagnostics;
  void notices;
  void sourceLine;
  void strict;
  void lenient;
  void allowed;
  void removed;
  void removedCount;
  void editedCount;
  void built;
  void fromRows;
  void gpsSeconds;
  void epoch;
  void lines;
  void surplus;
  void line;
}

function compileOnlyAntexContracts(antex: WebBindings.Antex, detail: AntexErrorDetail) {
  const header: AntexHeader = antex.header;
  const pcvType: AntexPcvType | null = header.pcvType;
  const outer: AntexOuterComment[] = antex.outerComments;
  const skipped: number = antex.skippedRecords;
  const blockCount: number = antex.blockCount;
  const blocks: WebBindings.Antenna[] = antex.antennaBlocks();
  const intervals: WebBindings.Antenna[] = antex.antennaIntervals("BLOCK I");
  const epoch = new WebBindings.AntexDateTime(1994, 4, 17, 23, 59, 59, "9999999");
  const at: WebBindings.Antenna | undefined = antex.antennaAt("BLOCK I", epoch);
  const block = blocks[0];
  const dazi: number | undefined = block.daziDeg;
  const zenithStart: number | undefined = block.zenithStartDeg;
  const calibrations: AntexCalibration[] = block.calibrations;
  const leading: string[] = block.leadingComments;
  const sections: AntexFrequency[] = block.frequencySections();
  const section: AntexFrequency = block.frequency("G01");
  const rms: AntexFrequencyRms | null = section.rms;
  const sample: AntexPcvSample = section.pcvSamples[0];
  const azimuth: number | null = sample.azimuthDeg;
  const until: WebBindings.AntexDateTime | undefined = block.validUntil;
  const digits: string | undefined = until?.fractionDigits;
  const nanosecond: number | undefined = epoch.nanosecond;
  if (detail.kind === "AMBIGUOUS_FREQUENCY") {
    const count: number = detail.sections;
    void count;
  } else if (detail.kind === "INVALID_FIELD") {
    const antennaId: string | null = detail.antennaId;
    void antennaId;
  }
  void pcvType;
  void outer;
  void skipped;
  void blockCount;
  void intervals;
  void at;
  void dazi;
  void zenithStart;
  void calibrations;
  void leading;
  void sections;
  void rms;
  void azimuth;
  void digits;
  void nanosecond;
}

function compileOnlyTerrainAndBlqContracts(
  store: WebBindings.MmapTerrain,
  terrain: WebBindings.DtedTerrain,
  lookup: TerrainLookupErrorDetail,
  parseDetail: BlqParseErrorDetail,
  writeDetail: BlqWriteErrorDetail,
) {
  const batch: TerrainHeightBatchEntry[] = store.heightBatch([[-106.5, 36.5]], {});
  const dted: TerrainHeightBatchEntry[] = terrain.heightBatch([[-106.5, 36.5]], {});
  const entry = batch[0];
  if (entry.ok) {
    const height: number = entry.heightM;
    const none: null = entry.detail;
    void height;
    void none;
  } else {
    const refused: TerrainLookupErrorDetail = entry.detail;
    void refused;
  }
  if (lookup.kind === "UNKNOWN_TERRAIN_ELEVATION") {
    const posting: number = lookup.latitudePosting;
    void posting;
  } else if (lookup.kind === "NON_WGS84_TERRAIN_TILE") {
    const datum: DtedHorizontalDatum = lookup.datum;
    void datum;
  }

  const block: OceanLoadingBlqBlock = WebBindings.parseOceanLoadingBlqBlock("");
  const blocks: OceanLoadingBlqBlock[] = WebBindings.parseOceanLoadingBlqBlocks("");
  const text: string = WebBindings.writeOceanLoadingBlqBlock(block);
  const file: string = WebBindings.writeOceanLoadingBlqBlocks([
    block,
    { station: "WTZR", amplitudeM: [[0]], phaseDeg: [Float64Array.of(0)] },
  ]);
  const comment: OceanLoadingBlqComment = block.comments[0];
  if (comment.placement === "beforeRow") {
    const row: number = comment.row;
    void row;
  } else {
    const row: null = comment.row;
    void row;
  }
  const parseLine: number = parseDetail.line;
  if (parseDetail.kind === "MISSING_COEFFICIENT_ROWS") {
    const found: number = parseDetail.found;
    void found;
  }
  const blockIndex: number = writeDetail.block;
  if (writeDetail.kind === "INVALID_HEADER") {
    const headerReason: BlqParseReason = writeDetail.header;
    void headerReason;
  } else if (writeDetail.kind === "NON_FINITE_COEFFICIENT") {
    const constituent: string = writeDetail.constituent;
    void constituent;
  }
  void dted;
  void blocks;
  void text;
  void file;
  void parseLine;
  void blockIndex;
}

function compileOnlyPositioningContracts(
  spp: WebBindings.SppSolution,
  solution: WebBindings.StaticSolution,
  rtcm: RtcmEncodeErrorDetail,
  sbas: WebBindings.SbasCorrectionStore,
) {
  const rejected: SppRejectedSatellite[] = spp.rejectedSats;
  const reason: SppRejectionReason = rejected[0].reason;
  const perEpoch: SppRejectedSatellite[][] = solution.rejectedSats;
  if (rtcm.kind === "INVALID_INPUT") {
    const why: string = rtcm.reason;
    void why;
  }
  const counts: SbasUnassignedMaskCorrections[] | null = sbas.unassignedMaskCorrections("S29");
  if (counts) {
    const count: bigint = counts[0].count;
    const maskNumber: number = counts[0].maskNumber;
    void count;
    void maskNumber;
  }
  void reason;
  void perEpoch;
}

// Nothing on these boundaries may be inferred as `any`.
type _ClockRecordsNotAny = Assert<
  Equal<IsAny<ReturnType<WebBindings.RinexClock["records"]>>, false>
>;
type _NodeClockRecords = Assert<
  Equal<ReturnType<NodeBindings.RinexClock["records"]>, NodeRinexClockRecord[]>
>;
type _ClockLayout = Assert<Equal<WebBindings.RinexClock["layout"], "v300" | "v304" | undefined>>;
type _ClockTimeScale = Assert<
  Equal<WebBindings.RinexClock["timeScale"], WebBindings.TimeScale | undefined>
>;
type _ClockEpochGpsSeconds = Assert<
  Equal<WebBindings.ClockEpoch["gpsSeconds"], number | undefined>
>;
type _ClockWriteResult = Assert<
  Equal<ReturnType<WebBindings.RinexClock["toRinexStringWithPolicy"]>, RinexClockWriteResult>
>;
type _AntexHeader = Assert<Equal<WebBindings.Antex["header"], AntexHeader>>;
type _AntennaDazi = Assert<Equal<WebBindings.Antenna["daziDeg"], number | undefined>>;
type _AntennaSections = Assert<
  Equal<ReturnType<WebBindings.Antenna["frequencySections"]>, AntexFrequency[]>
>;
type _SppRejected = Assert<Equal<WebBindings.SppSolution["rejectedSats"], SppRejectedSatellite[]>>;
type _SppVariances = Assert<Equal<WebBindings.SppSolution["pseudorangeVariancesM2"], Float64Array>>;
type _SppEffectiveWeights = Assert<Equal<WebBindings.SppSolution["weights"], Float64Array>>;
type _SppMetadataSystems = Assert<Equal<WebBindings.SppSolution["metadata"]["systems"], string[]>>;
type _FdeSolutionComplete = Assert<
  Equal<WebBindings.FdeSolution["solution"], WebBindings.SppSolution>
>;
type _FdeAcceptedRaim = Assert<Equal<WebBindings.FdeSolution["raim"], WebBindings.FdeRaimResult>>;
type _FdeRequestWeightsMode = Assert<
  Equal<WebBindings.FdeRequest["weightsMode"], WebBindings.RaimWeightsMode | undefined>
>;
type _FdeRequestBudget = Assert<Equal<WebBindings.FdeRequest["maxExclusions"], number | undefined>>;
type _FdeRequestCap = Assert<Equal<WebBindings.FdeRequest["maxExclusionRmsM"], number | undefined>>;
type _FdeRequestWeightEntries = Assert<
  Equal<
    WebBindings.FdeRequest["weightEntries"],
    Array<{ satelliteId: string; elevationDeg: number; cn0Dbhz?: number }> | undefined
  >
>;
type _FdeRequestVarianceOptions = Assert<
  Equal<
    WebBindings.FdeRequest["varianceOptions"],
    | {
        aM?: number;
        bM?: number;
        model?: "elevation" | "elevation_cn0";
        cn0Dbhz?: number;
        cn0ScaleM2?: number;
      }
    | undefined
  >
>;
type _FdeRequestRejectsOldBudget = Assert<
  Equal<"maxIterations" extends keyof WebBindings.FdeRequest ? true : false, false>
>;
type _RaimVariances = Assert<
  Equal<WebBindings.RaimInput["variancesM2"], number[] | Float64Array | undefined>
>;
type _RaimWeightModes = Assert<
  Equal<WebBindings.RaimOptions["weightsMode"], WebBindings.RaimWeightsMode | undefined>
>;
type _RaimTestable = Assert<Equal<WebBindings.RaimResult["testable"], boolean>>;
type _RangeFdeCap = Assert<
  Equal<WebBindings.RangeFdeOptions["maxExclusionRmsM"], number | undefined>
>;
type _QualityErrorKind = Assert<
  Equal<WebBindings.QualityError["detail"]["kind"], WebBindings.QualityErrorKind>
>;
type _UnresolvedSystems = Assert<Equal<WebBindings.FdeUnresolvedSolution["systems"], string[]>>;
type _UnresolvedUsedCount = Assert<Equal<WebBindings.FdeUnresolvedSolution["usedCount"], number>>;
type _StaticRejected = Assert<
  Equal<WebBindings.StaticSolution["rejectedSats"], SppRejectedSatellite[][]>
>;
type _StoreBatch = Assert<
  Equal<ReturnType<WebBindings.MmapTerrain["heightBatch"]>, TerrainHeightBatchEntry[]>
>;
type _DtedBatch = Assert<
  Equal<ReturnType<WebBindings.DtedTerrain["heightBatch"]>, TerrainHeightBatchEntry[]>
>;
type _SbasUnassigned = Assert<
  Equal<
    ReturnType<WebBindings.SbasCorrectionStore["unassignedMaskCorrections"]>,
    SbasUnassignedMaskCorrections[] | null
  >
>;
type _BlqParse = Assert<
  Equal<ReturnType<typeof WebBindings.parseOceanLoadingBlqBlock>, OceanLoadingBlqBlock>
>;
type _RtcmDetailNotAny = Assert<Equal<IsAny<RtcmEncodeErrorDetail>, false>>;
type _AntexDetailFullNotAny = Assert<Equal<IsAny<AntexErrorDetail>, false>>;
type _BlqWriteHeaderReason = Assert<
  Equal<Extract<BlqWriteErrorDetail, { kind: "INVALID_HEADER" }>["header"], BlqParseReason>
>;
type _BlqWriteHasBlock = Assert<
  Equal<Extract<BlqWriteErrorDetail, { kind: "EMPTY_STATION" }>["block"], number>
>;
type _ClockBadFieldLine = Assert<
  Equal<Extract<RinexClockErrorDetail, { kind: "BAD_FIELD" }>["line"], number>
>;
type _ClockInvalidInputHasNoLine = Assert<
  Equal<
    "line" extends keyof Extract<RinexClockErrorDetail, { kind: "INVALID_INPUT" }> ? true : false,
    false
  >
>;

// --- Engine-update surfaces -------------------------------------------------
//
// Results, policies and errors the engine update added. Each is declared by an
// `unchecked_*_type` attribute or the typed overlay, so a boundary that fell
// back to `any` or to a bare `string` fails here in both targets.

type _WebPppFloatUnplaced = Assert<
  Equal<WebBindings.PppFloatSolution["unplacedObservations"], WebBindings.PppUnplacedObservation[]>
>;
type _NodePppFloatUnplaced = Assert<
  Equal<
    NodeBindings.PppFloatSolution["unplacedObservations"],
    NodeBindings.PppUnplacedObservation[]
  >
>;
type _WebPppFixedUnplaced = Assert<
  Equal<WebBindings.PppFixedSolution["unplacedObservations"], WebBindings.PppUnplacedObservation[]>
>;
type _NodePppFixedUnplaced = Assert<
  Equal<
    NodeBindings.PppFixedSolution["unplacedObservations"],
    NodeBindings.PppUnplacedObservation[]
  >
>;
type _WebPppUnplacedShape = Assert<
  Equal<
    WebBindings.PppUnplacedObservation,
    {
      epochIndex: number;
      satelliteId: string;
      ambiguityId: string;
      reason: "codeNotPositive" | "ssrCorrectionExceedsLimit" | (string & {});
      ssrCorrectionSize?: WebBindings.SsrCorrectionSize;
    }
  >
>;
type _WebPppScreenRemovals = Assert<
  Equal<WebBindings.PppFloatSolution["residualScreenRemovals"], WebBindings.PppObservationRef[]>
>;
type _NodePppScreenRemovals = Assert<
  Equal<NodeBindings.PppFloatSolution["residualScreenRemovals"], NodeBindings.PppObservationRef[]>
>;

type ExpectedUt1Degraded = "beforeCoverage" | "afterCoverage" | undefined;
type _WebInstantUt1 = Assert<Equal<WebBindings.Instant["ut1Degraded"], ExpectedUt1Degraded>>;
type _NodeInstantUt1 = Assert<Equal<NodeBindings.Instant["ut1Degraded"], ExpectedUt1Degraded>>;
type _WebSppUt1 = Assert<Equal<WebBindings.SppSolution["ut1Degraded"], ExpectedUt1Degraded>>;
type _NodeSppUt1 = Assert<Equal<NodeBindings.SppSolution["ut1Degraded"], ExpectedUt1Degraded>>;
type _WebCoverageUt1 = Assert<Equal<WebBindings.CoverageGrid["ut1Degraded"], ExpectedUt1Degraded>>;
type _NodeCoverageUt1 = Assert<
  Equal<NodeBindings.CoverageGrid["ut1Degraded"], ExpectedUt1Degraded>
>;
type _WebCoverageCellError = Assert<
  Equal<
    ReturnType<WebBindings.CoverageGrid["cellError"]>,
    (Error & { detail: WebBindings.LookAngleErrorDetail }) | undefined
  >
>;
type _NodeCoverageCellError = Assert<
  Equal<
    ReturnType<NodeBindings.CoverageGrid["cellError"]>,
    (Error & { detail: NodeBindings.LookAngleErrorDetail }) | undefined
  >
>;
type _WebUt1ValidatedShape = Assert<
  Equal<
    WebBindings.Ut1Validated<number>,
    { value: number; ut1Degraded: "beforeCoverage" | "afterCoverage" | null }
  >
>;
type _WebGmstValidity = Assert<
  Equal<
    ReturnType<WebBindings.Instant["gmstRadiansWithValidity"]>,
    WebBindings.Ut1Validated<number>
  >
>;
type _NodeGmstValidity = Assert<
  Equal<
    ReturnType<NodeBindings.Instant["gmstRadiansWithValidity"]>,
    NodeBindings.Ut1Validated<number>
  >
>;
type _WebSunValidity = Assert<
  Equal<
    ReturnType<typeof WebBindings.sunAzElWithValidity>,
    WebBindings.Ut1Validated<WebBindings.BodyAzEl>
  >
>;
type _NodeSunValidity = Assert<
  Equal<
    ReturnType<typeof NodeBindings.sunAzElWithValidity>,
    NodeBindings.Ut1Validated<NodeBindings.BodyAzEl>
  >
>;
type _WebLookValidity = Assert<
  Equal<
    ReturnType<WebBindings.Tle["lookAnglesWithValidity"]>,
    WebBindings.Ut1Validated<WebBindings.LookAngles>
  >
>;

type _WebBiasLookup = Assert<
  Equal<ReturnType<WebBindings.BiasSet["codeOsbSeconds"]>, WebBindings.BiasLookup>
>;
type _NodeBiasLookup = Assert<
  Equal<ReturnType<NodeBindings.BiasSet["codeOsbSeconds"]>, NodeBindings.BiasLookup>
>;
type _WebBiasLineCounts = Assert<
  Equal<WebBindings.BiasSet["lineCounts"], WebBindings.BiasLineCounts>
>;
type _NodeBiasLineCounts = Assert<
  Equal<NodeBindings.BiasSet["lineCounts"], NodeBindings.BiasLineCounts>
>;

type _WebApHistory = Assert<
  Equal<ReturnType<WebBindings.SpaceWeatherTable["apHistoryAt"]>, WebBindings.SpaceWeatherApHistory>
>;
type _NodeApHistory = Assert<
  Equal<
    ReturnType<NodeBindings.SpaceWeatherTable["apHistoryAt"]>,
    NodeBindings.SpaceWeatherApHistory
  >
>;
type _WebApHistoryParameters = Assert<
  Equal<
    Parameters<WebBindings.SpaceWeatherTable["apHistoryAt"]>,
    [epoch_j2000_s: number, policy?: WebBindings.SpaceWeatherPolicyInput]
  >
>;
type _WebSampleAtParameters = Assert<
  Equal<
    Parameters<WebBindings.SpaceWeatherTable["sampleAt"]>,
    [epoch_j2000_s: number, policy?: WebBindings.SpaceWeatherPolicyInput]
  >
>;

type _WebSbasEmsLogParameters = Assert<
  Equal<
    Parameters<typeof WebBindings.parseSbasEmsLog>,
    [text: string, options?: WebBindings.SbasLogOptions]
  >
>;
type _NodeSbasRtklibLogParameters = Assert<
  Equal<
    Parameters<typeof NodeBindings.parseSbasRtklibLog>,
    [text: string, options?: NodeBindings.SbasLogOptions]
  >
>;

type _WebOmmArray = Assert<
  Equal<ReturnType<typeof WebBindings.parseOmmJsonArray>, WebBindings.OmmArray>
>;
type _NodeOmmArray = Assert<
  Equal<ReturnType<typeof NodeBindings.parseOmmCsvArray>, NodeBindings.OmmArray>
>;
type _WebTleRejected = Assert<
  Equal<WebBindings.ParsedTleFile["rejected"], WebBindings.RejectedTleRecord[]>
>;
type _WebSppRejected = Assert<
  Equal<WebBindings.SppSolution["rejectedSats"], WebBindings.SppRejectedSatellite[]>
>;
type _WebOemSkippedStates = Assert<
  Equal<WebBindings.Oem["skippedStates"], WebBindings.OemSkippedState[]>
>;
type _WebSpkKernelsState = Assert<
  Equal<ReturnType<WebBindings.SpkKernels["stateInFrame"]>, WebBindings.SpkState>
>;
type _WebCdmCovarianceRtn = Assert<
  Equal<ReturnType<WebBindings.CdmObject["toCovarianceRtn"]>, number[][]>
>;

type _WebObserve = Assert<Equal<ReturnType<typeof WebBindings.observe>, WebBindings.ObserveResult>>;
type _NodeObserveSpkBody = Assert<
  Equal<ReturnType<typeof NodeBindings.observeSpkBody>, NodeBindings.ObserveResult>
>;
type _WebObserveValidity = Assert<
  Equal<
    ReturnType<typeof WebBindings.observeWithValidity>,
    WebBindings.Ut1Validated<WebBindings.ObserveResult>
  >
>;
type _NodeObserveSpkValidity = Assert<
  Equal<
    ReturnType<typeof NodeBindings.observeSpkBodyWithValidity>,
    NodeBindings.Ut1Validated<NodeBindings.ObserveResult>
  >
>;
type _WebObserveParameters = Assert<
  Equal<
    Parameters<typeof WebBindings.observe>,
    [
      station: WebBindings.ObserveStation,
      epoch_unix_us: bigint,
      target: string,
      options?: WebBindings.ObserveOptions,
    ]
  >
>;
type _WebMeridianTransits = Assert<
  Equal<ReturnType<typeof WebBindings.meridianTransits>, WebBindings.MeridianTransitEvent[]>
>;
type _NodeMeridianTransitsSpk = Assert<
  Equal<ReturnType<typeof NodeBindings.meridianTransitsSpk>, NodeBindings.MeridianTransitEvent[]>
>;
type _WebMeridianValidity = Assert<
  Equal<
    ReturnType<typeof WebBindings.meridianTransitsWithValidity>,
    WebBindings.Ut1Validated<WebBindings.MeridianTransitEvent[]>
  >
>;
type _NodeMeridianSpkValidity = Assert<
  Equal<
    ReturnType<typeof NodeBindings.meridianTransitsSpkWithValidity>,
    NodeBindings.Ut1Validated<NodeBindings.MeridianTransitEvent[]>
  >
>;
type _WebMeridianTransitShape = Assert<
  Equal<
    WebBindings.MeridianTransitEvent,
    { timeUnixUs: number; kind: "upper" | "lower" | (string & {}); altitudeDeg: number }
  >
>;
// A kind or reason a later engine adds crosses under its own name, so the
// declared unions stay open while still offering the named literals.
type _WebUnplacedReasonOpen = Assert<
  Equal<
    WebBindings.PppUnplacedObservation["reason"],
    "codeNotPositive" | "ssrCorrectionExceedsLimit" | (string & {})
  >
>;
type _WebPppSolveOptions = Assert<
  Equal<WebBindings.PppFloatSolution["solveOptions"], WebBindings.PppAppliedSolveOptions>
>;
type _NodePppSolveOptions = Assert<
  Equal<NodeBindings.PppFloatSolution["solveOptions"], NodeBindings.PppAppliedSolveOptions>
>;

type ExpectedSolveStatus =
  | "GradientTolerance"
  | "CostTolerance"
  | "StepTolerance"
  | "MaxEvaluations"
  | "SelectionSettled"
  | "OuterBudgetExhausted"
  | "OuterOscillation";
type _WebSolveStatus = Assert<Equal<WebBindings.SolveStatus, ExpectedSolveStatus>>;
type _WebSppMetadata = Assert<
  Equal<WebBindings.SppSolution["metadata"], WebBindings.SppSolveMetadata>
>;
type _NodeSppMetadata = Assert<
  Equal<NodeBindings.SppSolution["metadata"], NodeBindings.SppSolveMetadata>
>;
type _WebStaticMetadata = Assert<
  Equal<WebBindings.StaticSolution["metadata"], WebBindings.StaticSolveMetadata>
>;
type _WebSppMetadataStatus = Assert<
  Equal<WebBindings.SppSolveMetadata["status"], ExpectedSolveStatus>
>;

type ExpectedTroposphereFrameValueErrorDetail = {
  readonly family: "FrameValueError";
  readonly kind: "FRAME_VALUE_INVALID_INPUT";
  readonly message: string;
  readonly field: string;
  readonly reason: string;
};
type ExpectedTroposphereTimeModelErrorDetail = {
  readonly family: "TimeModelError";
  readonly kind: "TIME_MODEL_INVALID_INPUT";
  readonly message: string;
  readonly field: string;
  readonly reason: string;
};
type _TroposphereErrorDetail = Assert<
  Equal<
    WebBindings.TroposphereCoreError["detail"],
    | WebBindings.CoreErrorDetail
    | ExpectedTroposphereFrameValueErrorDetail
    | ExpectedTroposphereTimeModelErrorDetail
  >
>;

const selectorRequest: WebBindings.SppRequest = {
  observations: [{ satelliteId: "J01", pseudorangeM: 22_000_000 }],
  tRxJ2000S: 0,
  tRxSecondOfDayS: 0,
  dayOfYear: 1,
  qzssClock: "separate",
  troposphereModel: "saastamoinenNiell",
};
const rinexSelectors: WebBindings.RinexSppOptions = {
  qzssClock: "gps",
  troposphereModel: "rtklib",
};
const staticSelectors: WebBindings.StaticSolveOptions = {
  qzssClock: "separate",
  troposphereModel: "rtklib",
};
const pppTideConstants: WebBindings.PppCorrectionOptions = {
  stationTideConstants: StationTideConstants.IersRoutine,
  ut1Validity: "permissive",
};
void [selectorRequest, rinexSelectors, staticSelectors, pppTideConstants];

// Positioning failures: one detail union across every SPP-family solve.
type _WebBatchErrorDetail = Assert<
  Equal<
    ReturnType<WebBindings.SppBatchSolution["error"]>,
    WebBindings.PositioningErrorDetail | undefined
  >
>;
type _NodeRinexBatchErrorDetail = Assert<
  Equal<
    ReturnType<NodeBindings.RinexSppSolutionBatch["error"]>,
    NodeBindings.PositioningErrorDetail | undefined
  >
>;
type _WebPositioningErrorKinds = Assert<
  Equal<
    WebBindings.PositioningErrorDetail["kind"],
    | "INVALID_INPUT"
    | "TOO_FEW_SATELLITES"
    | "TOO_FEW_MEASUREMENTS"
    | "SINGULAR"
    | "DUPLICATE_OBSERVATION"
    | "EPHEMERIS_LOST"
    | "SELECTION_UNSETTLED"
    | "UT1_OUTSIDE_COVERAGE"
    | "SOLUTION_REJECTED"
    | "NO_COARSE_SOLUTION"
    | "EMPTY_EPOCHS"
    | "EPOCH_INPUT"
    | "PRECISE_SOLVE_FAILED"
    | "BROADCAST_SOLVE_FAILED"
    | "FAULT_UNRESOLVED"
    | "RAIM_CONFIGURATION"
    | "OBSERVATION"
    | "MISSING_APPROX_POSITION"
    | "DGNSS_INVALID_INPUT"
    | "RTK_FLOAT"
    | "RTK_FIXED"
    | "OTHER"
  >
>;
type SelectionUnsettledDetail = Extract<
  WebBindings.PositioningErrorDetail,
  { kind: "SELECTION_UNSETTLED" }
>;
type _WebSelectionUnsettledPasses = Assert<Equal<SelectionUnsettledDetail["passes"], number>>;
type EpochInputDetail = Extract<WebBindings.PositioningErrorDetail, { kind: "EPOCH_INPUT" }>;
type _WebEpochInputCause = Assert<
  Equal<EpochInputDetail["cause"], WebBindings.PositioningErrorDetail>
>;
type _WebPositioningErrorShape = Assert<
  Equal<WebBindings.PositioningError["detail"], WebBindings.PositioningErrorDetail>
>;
type FaultUnresolvedDetail = Extract<
  WebBindings.PositioningErrorDetail,
  { kind: "FAULT_UNRESOLVED" }
>;
type _WebFaultUnresolvedGeometryQuality = Assert<
  Equal<FaultUnresolvedDetail["solution"]["geometryQuality"], WebBindings.FdeGeometryQuality>
>;
type ObservationDetail = Extract<WebBindings.PositioningErrorDetail, { kind: "OBSERVATION" }>;
type _WebObservationCause = Assert<Equal<ObservationDetail["cause"], WebBindings.CoreErrorDetail>>;
type RtkFixedDetail = Extract<WebBindings.PositioningErrorDetail, { kind: "RTK_FIXED" }>;
type _WebRtkFixedCause = Assert<
  Equal<RtkFixedDetail["cause"], WebBindings.ValidatedFixedSolveErrorDetail>
>;
type RtkResidualDetail = Extract<
  WebBindings.ValidatedFixedSolveErrorDetail,
  { kind: "RESIDUAL_VALIDATION_FAILED" }
>;
type _WebRtkResidualPayload = Assert<
  Equal<RtkResidualDetail["outlier"], WebBindings.ResidualValidationOutlierDetail>
>;
type RtkFloatDetail = Extract<WebBindings.PositioningErrorDetail, { kind: "RTK_FLOAT" }>;
type _WebRtkFloatCause = Assert<Equal<RtkFloatDetail["cause"], WebBindings.FloatSolveErrorDetail>>;
type RtkFloatInvalidInput = Extract<WebBindings.FloatSolveErrorDetail, { kind: "INVALID_INPUT" }>;
type _WebRtkInputKind = Assert<
  Equal<
    RtkFloatInvalidInput["inputKind"],
    | "non_finite"
    | "not_positive"
    | "negative"
    | "out_of_range"
    | "missing"
    | "float_parse"
    | "int_parse"
    | "invalid_civil_date"
    | "invalid_civil_time"
  >
>;

function compileOnlyCoreErrorCauseContracts(detail: WebBindings.CoreErrorDetail) {
  if (detail.kind === "SP3_EPOCH_INTERVAL") {
    const decimal: string = detail.value.decimal;
    const bits: string = detail.value.bitsHex;
    // @ts-expect-error ExactFloat is an object, not a lossy number/string slot.
    const roundedValue: string = detail.value;
    void decimal;
    void bits;
    void roundedValue;
  }

  if (detail.kind === "TERRAIN_TILE") {
    const dted: CoreDtedTileErrorDetail = detail.cause;
    if (dted.kind === "outside") {
      const exact: CoreErrorExactFloat = dted.longitude;
      const bits: string = exact.bitsHex;
      // @ts-expect-error Outside carries exact floats, not a posting index.
      const absentIndex: number = dted.longitudeIndex;
      void bits;
      void absentIndex;
    }
    if (dted.kind === "negativePostingIndex") {
      const index: string = dted.index;
      // @ts-expect-error The signed index is decimal text to retain i64 exactly.
      const roundedIndex: number = dted.index;
      void index;
      void roundedIndex;
    }
  }

  if (detail.kind === "IONEX_OUT_OF_COVERAGE") {
    const coverage: IonexCoverageError = detail.cause;
    const coverageKind: IonexCoverageError["kind"] = coverage.kind;
    // @ts-expect-error Coverage errors do not carry a missing-node gap.
    const absentGap: IonexNodeGap = detail.cause.earlier;
    void coverageKind;
    void absentGap;
  }

  if (detail.kind === "IONEX_NODES_NOT_AVAILABLE") {
    const gap: IonexNodeGap = detail.cause;
    const earlier: IonexNodeGap["earlier"] = gap.earlier;
    // @ts-expect-error Node gaps do not have a top-level refusal kind.
    const absentKind: string = gap.kind;
    void earlier;
    void absentKind;
  }

  if (detail.kind === "IONEX_SLANT_UNAVAILABLE") {
    const refusal: IonexSlantRefusal = detail.cause;
    const refusalMessage: string = refusal.message;
    void refusalMessage;
  }

  if (detail.kind === "IONEX_EPOCH") {
    const epoch: CoreIonexEpochErrorDetail = detail.cause;
    if (epoch.kind === "yearOutOfField") {
      const seconds: string = epoch.utcJ2000S;
      // @ts-expect-error The i64 value is not a JavaScript number.
      const roundedSeconds: number = epoch.utcJ2000S;
      void seconds;
      void roundedSeconds;
    }
  }

  if (detail.kind === "SBAS_ENCODE") {
    const sbas: CoreSbasEncodeCause = detail.cause;
    if (sbas.kind === "fieldOutOfRange") {
      const value: string = sbas.value;
      const width: number = sbas.width;
      void value;
      void width;
    }
  }

  if (detail.kind === "RTCM_ENCODE") {
    const rtcm: CoreRtcmEncodeCause = detail.cause;
    if (rtcm.kind === "negativeZeroWithValue") {
      const value: string = rtcm.value;
      // @ts-expect-error The source i64 is preserved as decimal text.
      const roundedValue: number = rtcm.value;
      void value;
      void roundedValue;
    }
    if (rtcm.kind === "strictDeparture") {
      const departure: CoreRtcmDepartureDetail = rtcm.departure;
      const message: string = departure.message;
      void message;
    }
    if (rtcm.kind === "msmOptional") {
      const problem: CoreRtcmMsmOptionalProblem = rtcm.problem;
      if (problem.kind === "invalidValue") {
        const value: string = problem.value;
        void value;
      }
    }
  }

  if (detail.kind === "RTCM_CONVERSION") {
    const conversion: CoreRtcmConversionCause = detail.cause;
    if (conversion.kind === "fitInterval") {
      const record: CoreRtcmLnavRecordError = conversion.error;
      if (record.kind === "weekMismatch") {
        const fullWeek: number = record.fullWeek;
        const decodedWeek: string = record.decodedWeek;
        void fullWeek;
        void decodedWeek;
      }
    }
  }
}

type _DomainErrorFamilies = Assert<
  Equal<
    DomainErrorDetail["family"],
    | "anomaly"
    | "elements"
    | "equinoctial"
    | "propagation"
    | "frameTransform"
    | "scenario"
    | "exactCache"
  >
>;
type _AnomalyExactFloat = Assert<
  Equal<Extract<AnomalyErrorDetail["cause"], { kind: "beyondAsymptote" }>["nu"], ExactFloatDetail>
>;
type _ElementErrors = Assert<Equal<ElementsErrorDetail["family"], "elements">>;
type _EquinoctialNestedCause = Assert<
  Equal<
    Extract<EquinoctialErrorDetail["cause"], { kind: "elements" }>["cause"]["kind"],
    ElementsErrorDetail["cause"]["kind"]
  >
>;
type _PropagationReasons = Assert<
  Equal<
    Extract<PropagationErrorDetail["cause"], { kind: "ut1OutsideCoverage" }>["reason"],
    "beforeCoverage" | "afterCoverage"
  >
>;
type _FrameTransformFamily = Assert<Equal<FrameDomainErrorDetail["family"], "frameTransform">>;
type _ScenarioFamily = Assert<Equal<ScenarioErrorDetail["family"], "scenario">>;
type _CatalogDetailsAreTyped = Assert<
  Equal<Extract<DataCatalogErrorDetail, { kind: "invalidCoordinate" }>["latitudeBitsHex"], string>
>;
type _ExactCacheFamily = Assert<Equal<ExactCacheErrorDetail["family"], "exactCache">>;

type _WebObservablesErrorKinds = Assert<
  Equal<
    WebBindings.ObservablesErrorDetail["kind"],
    "INVALID_INPUT" | "NO_EPHEMERIS" | "EPHEMERIS" | "MEDIA"
  >
>;
type _WebVelocityErrorKinds = Assert<
  Equal<
    WebBindings.VelocityErrorDetail["kind"],
    | "NO_OBSERVATIONS"
    | "TOO_FEW_SATELLITES"
    | "SINGULAR_GEOMETRY"
    | "DUPLICATE_OBSERVATION"
    | "INVALID_CARRIER"
    | "INVALID_INPUT"
    | "INVALID_OBSERVATION"
    | "INVALID_RECEIVER_STATE"
  >
>;

// RTCM: the decoded message IR, stream diagnostics and departures are typed.
type _WebDecodeRtcm = Assert<
  Equal<ReturnType<typeof WebBindings.decodeRtcm>, WebBindings.RtcmMessage[]>
>;
type _WebDecodeRtcmStream = Assert<
  Equal<ReturnType<typeof WebBindings.decodeRtcmStream>, WebBindings.RtcmStream>
>;
type _NodeDecodeRtcmFrame = Assert<
  Equal<ReturnType<typeof NodeBindings.decodeRtcmFrame>, NodeBindings.RtcmFrame>
>;
type _WebStreamDepartures = Assert<
  Equal<WebBindings.RtcmStream["diagnostics"]["departures"], WebBindings.RtcmDeparture[]>
>;
type _WebRtcmMessageTypes = Assert<
  Equal<
    WebBindings.RtcmMessage["type"],
    | "msm"
    | "legacyObservations"
    | "networkAuxiliaryStation"
    | "networkCorrectionDifferences"
    | "networkResiduals"
    | "physicalReferenceStation"
    | "fkpGradients"
    | "stationCoordinates"
    | "antennaDescriptor"
    | "systemParameters"
    | "text"
    | "glonassCodePhaseBiases"
    | "gpsEphemeris"
    | "glonassEphemeris"
    | "beidouEphemeris"
    | "navicEphemeris"
    | "qzssEphemeris"
    | "galileoFnavEphemeris"
    | "galileoInavEphemeris"
    | "ssr"
    | "ssrVtec"
    | "helmertTransformation"
    | "residualGrid"
    | "projection"
    | "unsupported"
  >
>;
type GpsEph = Extract<WebBindings.RtcmMessage, { type: "gpsEphemeris" }>;
type _WebGpsEphRawBits = Assert<Equal<GpsEph["sqrtA"], bigint>>;
type _WebEncodeRtcmInput = Assert<
  Equal<Parameters<typeof WebBindings.encodeRtcm>[0], WebBindings.RtcmMessageInput>
>;
// Every decoded message is an acceptable encoder input.
type _WebDecodedIsInput = Assert<
  WebBindings.RtcmMessage extends WebBindings.RtcmMessageInput ? true : false
>;

// RINEX lint: every finding carries a typed detail.
type _WebLintReport = Assert<
  Equal<ReturnType<typeof WebBindings.lintRinexObs>, WebBindings.RinexLintReport>
>;
type _WebBiasNoticeDetails = Assert<
  Equal<NodeBindings.BiasSet["noticeDetails"], BiasNoticeDetail[]>
>;
type _WebBiasSinexTextWriter = Assert<
  Equal<ReturnType<NodeBindings.BiasSet["toBiasSinexText"]>, string>
>;
type _WebBiasSinexBytesWriter = Assert<
  Equal<ReturnType<NodeBindings.BiasSet["toBiasSinexBytes"]>, Uint8Array>
>;
type _WebCodeDcbTextWriter = Assert<
  Equal<ReturnType<NodeBindings.BiasSet["toCodeDcbText"]>, string>
>;
type _WebCodeDcbBytesWriter = Assert<
  Equal<ReturnType<NodeBindings.BiasSet["toCodeDcbBytes"]>, Uint8Array>
>;
type _WebChi2InverseCdf = Assert<Equal<ReturnType<typeof WebBindings.chi2Inv>, number>>;
type _NodeChi2InverseCdf = Assert<Equal<ReturnType<typeof NodeBindings.chi2Inv>, number>>;
type _WebBiasErrorDetail = Assert<Equal<BiasError["detail"], NodeBindings.BiasErrorDetail>>;
type _WebRtkPredictionEpoch = Assert<
  Equal<NodeBindings.RtkArcEpoch["predictionEpoch"], NodeBindings.ExactEpoch | null | undefined>
>;
type _WebRtkDualGapEpoch = Assert<
  Equal<
    NodeBindings.RtkDualFrequencyArcEpoch["gapEpoch"],
    NodeBindings.ExactEpoch | null | undefined
  >
>;
type LintGap = Extract<WebBindings.RinexLintFindingDetail, { kind: "OBS_EPOCH_GAP" }>;
type _WebLintGapFields = Assert<Equal<LintGap["gapS"] | LintGap["intervalS"], number>>;

type _RejectedTleTypedIssue = Assert<
  Equal<NodeBindings.RejectedTleRecord["detail"], NodeBindings.TleRecordIssueDetail>
>;
type _ResonanceBudgetIsExact = Assert<
  Equal<
    Extract<NodeBindings.Sgp4ErrorDetail["cause"], { kind: "resonanceStepBudget" }>["budget"],
    string
  >
>;
type _Sgp4InputErrorKinds = Assert<
  Equal<
    NodeBindings.Sgp4InputErrorKind,
    | "nonFinite"
    | "notPositive"
    | "negative"
    | "outOfRange"
    | "missing"
    | "floatParse"
    | "intParse"
    | "invalidCivilDate"
    | "invalidCivilTime"
  >
>;
type _Sgp4ErrorFamilies = Assert<
  Equal<
    NodeBindings.Sgp4OperationErrorDetail["family"],
    "sgp4" | "tle" | "decayLatched" | "lookAngle" | "pass" | "tleFit" | "sgp4Batch"
  >
>;
type _Sgp4FamilyPayloads = Assert<
  Equal<
    Extract<NodeBindings.Sgp4OperationErrorDetail, { family: "sgp4" }>,
    NodeBindings.Sgp4ErrorDetail
  >
> &
  Assert<
    Equal<
      Extract<NodeBindings.Sgp4OperationErrorDetail, { family: "tle" }>,
      NodeBindings.TleErrorDetail
    >
  > &
  Assert<
    Equal<
      Extract<NodeBindings.Sgp4OperationErrorDetail, { family: "decayLatched" }>,
      NodeBindings.DecayLatchedErrorDetail
    >
  > &
  Assert<
    Equal<
      Extract<NodeBindings.Sgp4OperationErrorDetail, { family: "lookAngle" }>,
      NodeBindings.LookAngleErrorDetail
    >
  > &
  Assert<
    Equal<
      Extract<NodeBindings.Sgp4OperationErrorDetail, { family: "pass" }>,
      NodeBindings.PassErrorDetail
    >
  > &
  Assert<
    Equal<
      Extract<NodeBindings.Sgp4OperationErrorDetail, { family: "tleFit" }>,
      NodeBindings.TleFitErrorDetail
    >
  > &
  Assert<
    Equal<
      Extract<NodeBindings.Sgp4OperationErrorDetail, { family: "sgp4Batch" }>,
      NodeBindings.Sgp4BatchErrorDetail
    >
  >;
type _Sgp4FamilyCauses = Assert<
  Equal<NodeBindings.Sgp4ErrorDetail["cause"], NodeBindings.Sgp4ErrorCause>
> &
  Assert<Equal<NodeBindings.TleErrorDetail["cause"], NodeBindings.TleErrorCause>> &
  Assert<
    Equal<NodeBindings.DecayLatchedErrorDetail["cause"], NodeBindings.DecayLatchedErrorCause>
  > &
  Assert<Equal<NodeBindings.LookAngleErrorDetail["cause"], NodeBindings.LookAngleErrorCause>> &
  Assert<Equal<NodeBindings.PassErrorDetail["cause"], NodeBindings.PassErrorCause>> &
  Assert<Equal<NodeBindings.TleFitErrorDetail["cause"], NodeBindings.TleFitErrorCause>> &
  Assert<Equal<NodeBindings.Sgp4BatchErrorDetail["cause"], NodeBindings.Sgp4BatchErrorCause>>;
type _Sgp4FitBestEffortRetained = Assert<
  Equal<
    Extract<NodeBindings.TleFitErrorCause, { kind: "didNotConverge" }>["bestEffortFit"],
    NodeBindings.Sgp4BestEffortFit
  >
>;
type _Sgp4FitAxesAreThreeExactValues = Assert<
  Equal<
    NodeBindings.Sgp4FitStatistics["rms_position_axes_km"],
    [NodeBindings.Sgp4ExactFloat, NodeBindings.Sgp4ExactFloat, NodeBindings.Sgp4ExactFloat]
  >
>;
type _Sgp4OperationErrorDetail = Assert<
  Equal<NodeBindings.Sgp4OperationError["detail"], NodeBindings.Sgp4OperationErrorDetail>
>;

type _NmeaSkipReasonKinds = Assert<
  Equal<
    NodeBindings.NmeaSkipReason["reasonKind"],
    | "unrepresentableSatellite"
    | "unsupportedRecordType"
    | "malformedField"
    | "outOfRangeEpoch"
    | "truncated"
    | "unsupportedUnit"
    | "unknownBlock"
    | "inconsistentRecord"
  >
>;
type _NmeaFieldErrorKinds = Assert<
  Equal<
    NodeBindings.NmeaFieldError["kind"],
    | "missing"
    | "nonFinite"
    | "notPositive"
    | "negative"
    | "outOfRange"
    | "floatParse"
    | "intParse"
    | "invalidCivilDate"
    | "invalidCivilTime"
  >
>;
type _NmeaWarningKinds = Assert<
  Equal<
    NodeBindings.NmeaWarningKind,
    "checksum" | "clamped" | "degraded" | "mismatch" | "overlap" | "missingMetadata"
  >
>;
type _NmeaSkipFieldCause = Assert<
  Equal<
    Extract<NodeBindings.NmeaSkipDiagnostic, { reasonKind: "malformedField" }>["cause"],
    NodeBindings.NmeaFieldError
  >
>;
type _NmeaDiagnosticPositions = Assert<
  Equal<NodeBindings.NmeaSkipDiagnostic["at"], NodeBindings.NmeaRecordRef>
>;
type _NmeaRecordRefOptionsAreExplicit = Assert<
  Equal<
    NodeBindings.NmeaRecordRef,
    {
      line: number | null | undefined;
      recordIndex: number | null | undefined;
      satellite: string | null | undefined;
    }
  >
>;
type _NmeaDiagnosticsGetter = Assert<
  Equal<NodeBindings.NmeaParseResult["diagnostics"], NodeBindings.NmeaDiagnostics>
>;
type _NmeaAccumulatorDiagnostics = Assert<
  Equal<NodeBindings.NmeaAccumulatorOutput["diagnostics"], NodeBindings.NmeaDiagnostics>
>;
type _NmeaCivilDateI64ValuesAreLossless = Assert<
  Equal<
    Extract<NodeBindings.NmeaFieldError, { kind: "invalidCivilDate" }>,
    { kind: "invalidCivilDate"; field: string; year: string; month: string; day: string }
  >
>;
type _NmeaCivilTimeI64ValuesAreLossless = Assert<
  Equal<
    Extract<NodeBindings.NmeaFieldError, { kind: "invalidCivilTime" }>,
    {
      kind: "invalidCivilTime";
      field: string;
      hour: string;
      minute: string;
      second: number;
    }
  >
>;

const astroError: AstroErrorDetail = {
  family: "spk",
  cause: { kind: "unknownBody", body: 10 },
};
const observeCause: WebBindings.ObserveCause = {
  kind: "frameTransform",
  message: "frame transform failed",
  cause: { kind: "invalidInput", field: "latitude_deg", reason: "must be in [-90, 90]" },
};
type _ObservationErrorDetail = Assert<
  Equal<Extract<AstroErrorDetail, { family: "observation" }>["cause"], WebBindings.ObserveCause>
>;
void observeCause;

void astroError;

type _NodeIodErrorKind = Assert<
  Equal<
    NodeBindings.IodErrorKind,
    | "determinant_too_small"
    | "orbit_not_possible"
    | "zero_vector"
    | "collinear_vectors"
    | "not_coplanar"
    | "invalid_time_geometry"
    | "no_positive_root"
    | "root_solve_failed"
    | "non_finite_value"
  >
>;
type _WebIodErrorKind = Assert<Equal<NodeBindings.IodErrorKind, WebBindings.IodErrorKind>>;
type _NodeIodErrorDetail = Assert<
  Equal<NodeBindings.IodErrorDetail, { family: "IodError"; kind: NodeBindings.IodErrorKind }>
>;

type _FleetLookAngleOutcomeIsTyped = Assert<
  Equal<IsAny<ReturnType<WebBindings.Constellation["lookAngleArcOutcomes"]>>, false>
>;
type _FleetGroundTrackOutcomeIsTyped = Assert<
  Equal<IsAny<ReturnType<WebBindings.Constellation["groundTrackOutcomes"]>>, false>
>;
type _FleetPassOutcomeIsTyped = Assert<
  Equal<ReturnType<WebBindings.Constellation["passOutcomes"]>, WebBindings.FleetPassOutcome[]>
>;
const exactFleetPassOutcome: WebBindings.FleetPassOutcome = {
  satelliteIndex: 0,
  value: [
    {
      aosUnixUs: 1_000_000n,
      losUnixUs: 2_000_000n,
      maxElevationDeg: 30,
      culminationUnixUs: 1_500_000n,
    },
  ],
  error: null,
};
void exactFleetPassOutcome;

// PPP correction declarations remain exact alongside the previously added
// SP3 writer contracts in this file.
const pppEpoch: WebBindings.PppCorrectionEpoch = {
  year: 2020,
  month: 6,
  day: 24,
  hour: 12,
  minute: 0,
  second: 0,
  tRxJ2000S: 0,
  observations: [{ satelliteId: "G01", freq1Hz: 1.57542e9, freq2Hz: 1.2276e9 }],
};
const pppResult: WebBindings.PppCorrections = {
  tide: [],
  poleTide: [],
  oceanLoading: [],
  windupM: [],
  satPcoEcef: [],
  satPcvM: [],
  codeBiasM: [],
  ut1Degraded: null,
  warnings: [],
  diagnostics: { skipCount: 0, warningCount: 0, skips: [], warnings: [] },
};
const codeBiasOptions: WebBindings.CodeBiasOptions = {
  usedObservablesPerSat: [{ sat: "G01", obs1: "C1C", obs2: "C2W" }],
};
type _PppDetailedErrorFamily = Assert<
  Equal<WebBindings.PppCorrectionsErrorDetail["family"], "PppCorrectionsError">
>;
type _PppDetailedErrorKinds = Assert<
  Equal<
    WebBindings.PppCorrectionsErrorDetail["kind"],
    | "INVALID_INPUT"
    | "EPOCH"
    | "TIDE"
    | "POLE_TIDE"
    | "OCEAN_LOADING"
    | "WINDUP_FREQUENCY"
    | "SATELLITE_ANTENNA_FREQUENCY"
    | "BIAS"
    | "CODE_BIAS_OBSERVABLE"
  >
>;
type _PppCorrectionsReturn = Assert<
  Equal<ReturnType<typeof WebBindings.pppCorrections>, WebBindings.PppCorrections>
>;
void [pppEpoch, pppResult, codeBiasOptions];
