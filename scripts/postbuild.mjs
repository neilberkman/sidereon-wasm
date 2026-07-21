// The root package.json declares "type": "module" for the ESM (web) build under
// pkg/. The nodejs build under pkg-node/ is CommonJS (it uses __dirname and
// require('fs') to load the wasm), so it must be marked accordingly or Node
// loads it as ESM and the wasm path resolution breaks. Emit a package.json that
// scopes pkg-node/ to commonjs.
//
// wasm-bindgen cannot infer TypeScript record types from serde_wasm_bindgen
// JsValue arguments/results. Patch the generated declarations with the public
// object contracts for the audited high-level APIs.
import { readFileSync, writeFileSync } from "node:fs";

const OVERLAY_MARKER = "/* sidereon typed JsValue overlay */";

const overlay = `${OVERLAY_MARKER}
export type Vec3 = [number, number, number] | Float64Array;
export type Vec4 = [number, number, number, number] | Float64Array;
export type Matrix3 = number[] | Float64Array;

export interface ExactProductIdentityInput {
    family: "sp3" | "ionex" | "clk" | "nav";
    analysisCenter: string;
    publisher: "IGS" | "COD" | "ESA" | "GFZ";
    solutionClass: "final" | "rapid" | "ultra_rapid" | "predicted" | "broadcast";
    campaign: "OPS" | "MGN" | "MGX" | "BRD";
    filenameVersion: number;
    year: number;
    month: number;
    day: number;
    issue?: string | null;
    span: string;
    sample: string;
    officialFilename: string;
    format: "SP3" | "IONEX" | "RINEX_CLK" | "RINEX_NAV";
    formatVersion?: string | null;
    predictionHorizonDays?: number | null;
}

export interface Sp3ArtifactIdentityInput {
    requestedIdentity: ExactProductIdentityInput;
    resolvedIdentity: ExactProductIdentityInput;
    distributionSource: "direct" | "nasa_cddis" | "local_file" | "in_memory";
    officialFilename: string;
    productSha256: string;
    /** Positive integer no greater than Number.MAX_SAFE_INTEGER. */
    productByteLength: number;
    archiveSha256: string;
    /** Positive integer no greater than Number.MAX_SAFE_INTEGER. */
    archiveByteLength: number;
    compression: "none" | "gzip" | "unix_compress";
}

/** Validated canonical artifact record returned by merged-SP3 identity APIs. */
export type Sp3ArtifactIdentity = Sp3ArtifactIdentityInput;

export interface Sp3MergeIdentityOptions {
    /** Finite, non-negative position agreement tolerance in meters. */
    positionToleranceM?: number;
    /** Finite, non-negative clock agreement tolerance in seconds. */
    clockToleranceS?: number;
    minAgree?: number;
    clockMinCommon?: number;
    combine?: "mean" | "median" | "precedence";
    precedenceScope?: "cell" | "satellite_arc";
    outlierReject?: { positionToleranceM: number; clockToleranceS: number };
    targetEpochIntervalS?: number;
    systems?: string[];
    assertedFrameLabelSets?: string[][];
    helmert?: boolean;
}

export interface SurfaceMetInput {
    pressureHpa: number;
    temperatureK: number;
    relativeHumidity: number;
}

export interface RobustOptions {
    huberK?: number;
    scaleFloorM?: number;
    maxOuter?: number;
    outerTolM?: number;
}

export interface SppObservation {
    satelliteId: string;
    pseudorangeM: number;
}

export interface SppCorrections {
    ionosphere?: boolean;
    troposphere?: boolean;
}

export interface SppRequest {
    observations: SppObservation[];
    tRxJ2000S: number;
    tRxSecondOfDayS: number;
    dayOfYear: number;
    initialGuess?: Vec4;
    corrections?: SppCorrections;
    klobuchar?: { alpha?: Vec4; beta?: Vec4 };
    met?: SurfaceMetInput;
    glonassChannels?: Array<[number, number]>;
    withGeodetic?: boolean;
    robust?: RobustOptions;
    coarseSearchSeeds?: number;
    maxPdop?: number;
}

export interface FdeRequest extends SppRequest {
    pFa?: number;
    weights?: Array<{ satelliteId: string; weight: number }>;
    nSystems?: number;
    maxIterations?: number;
}

export interface SppBatchOptions {
    withGeodetic?: boolean;
    coarseSearchSeeds?: number;
    maxPdop?: number;
}

export interface RinexSppOptions {
    signalPolicy?: Record<string, string[]>;
    corrections?: SppCorrections;
    initialGuess?: Vec4;
    satellites?: string[];
    met?: SurfaceMetInput;
    robust?: RobustOptions;
}

export type RinexSppSolveOptions = SppBatchOptions;

export interface RinexSppEpochTime {
    year: number;
    month: number;
    day: number;
    hour: number;
    minute: number;
    second: number;
}

export interface RinexSppEpochInputs {
    epochIndex: number;
    epoch: RinexSppEpochTime;
    observations: SppObservation[];
    tRxJ2000S: number;
    tRxSecondOfDayS: number;
    dayOfYear: number;
    initialGuess: [number, number, number, number];
    corrections: Required<SppCorrections>;
    glonassChannels: Array<[number, number]>;
}

export interface RaimInput {
    usedSats: string[];
    residualsM: number[] | Float64Array;
}

export type RaimWeightsInput =
    | { satelliteIds: string[]; weights: number[] | Float64Array }
    | { satelliteIds: string[]; values: number[] | Float64Array }
    | Array<{ satelliteId: string; weight: number }>
    | Record<string, number>;

export interface RaimOptions {
    pFa?: number;
    weights?: RaimWeightsInput | RaimWeights;
    weightEntries?: Array<{ satelliteId: string; elevationDeg: number; cn0Dbhz?: number }>;
    varianceOptions?: { aM?: number; bM?: number; model?: "elevation" | "elevation_cn0"; cn0Dbhz?: number; cn0ScaleM2?: number };
    nSystems?: number;
}

export interface RaimResult {
    faultDetected: boolean;
    testStatistic: number;
    threshold: number | null;
    worstSat: string | null;
    reducedChiSquare: number | null;
    normalizedResiduals: Record<string, number>;
    rmsM: number;
    dof: number;
}

export interface RangeFdeRow {
    id: string;
    residualM: number;
    designRow: number[] | Float64Array;
    weight: number;
}

export interface RangeFdeOptions {
    pFa?: number;
    maxExclusions?: number;
    minRedundancy?: number;
}

export interface RangeFdeResult {
    stateCorrection: number[];
    stateCovariance: number[][];
    globalTest: { weightedSumSquares: number; dof: number; threshold: number | null; testable: boolean; faultDetected: boolean };
    excluded: string[];
    diagnostics: Array<{ id: string; excluded: boolean; postFitResidualM: number; normalizedResidual: number }>;
    iterations: number;
}

export interface AraimReceiver {
    latRad: number;
    lonRad: number;
    heightM: number;
}

export interface AraimRow {
    id: string;
    lineOfSight: [number, number, number] | Float64Array;
    system?: string;
    elevationRad: number;
}

export interface AraimGeometry {
    rows: AraimRow[];
    receiver: AraimReceiver;
    clockSystems: string[];
}

export interface AraimSatelliteIsmModel {
    sigmaUraM: number;
    sigmaUreM: number;
    effectiveSigmaIntM?: number;
    effectiveSigmaAccM?: number;
    bNomM: number;
    pSat: number;
}

export interface AraimConstellationIsm {
    system: string;
    pConst: number;
    defaultSat: AraimSatelliteIsmModel;
}

export interface AraimSatelliteIsm extends AraimSatelliteIsmModel {
    id: string;
}

export interface AraimIsm {
    constellations: AraimConstellationIsm[];
    satellites?: AraimSatelliteIsm[];
}

export interface AraimAllocation {
    phmiTotal: number;
    phmiVert: number;
    phmiHor: number;
    pfaVert: number;
    pfaHor: number;
    pThresholdUnmonitored: number;
    pEmt?: number;
    maxFaultOrder: number;
}

export interface AraimFaultHypothesis {
    excluded: string[];
    excludedConstellation: string | null;
    prior: number;
}

export interface AraimFaultMode extends AraimFaultHypothesis {
    sigmaIntEnuM: [number, number, number];
    biasEnuM: [number, number, number];
    thresholdEnuM: [number, number, number];
    monitorable: boolean;
}

export interface AraimResult {
    available: boolean;
    hplM: number;
    vplM: number;
    sigmaAccHM: number;
    sigmaAccVM: number;
    emtM: number;
    faultModes: AraimFaultMode[];
    pUnmonitored: number;
    availability: boolean;
}

export interface RtkSignalPair {
    system?: string;
    codeObservable: string;
    phaseObservable: string;
}

export interface RtkDualSignalPair {
    system?: string;
    code1Observable: string;
    phase1Observable: string;
    code2Observable: string;
    phase2Observable: string;
}

export interface RtkRinexArcOptions {
    signalPairs?: RtkSignalPair[];
    maxEpochs?: number;
    minCommonSatellites?: number;
    includePredictionTime?: boolean;
}

export interface RtkRinexDualArcOptions {
    signalPairs?: RtkDualSignalPair[];
    maxEpochs?: number;
    minCommonSatellites?: number;
    includePredictionTime?: boolean;
}

export interface RtkArcObservation {
    satelliteId: string;
    roverCodeM: number;
    baseCodeM: number;
    roverPhaseCycles: number;
    basePhaseCycles: number;
    wavelengthM: number;
    elevationRad?: number;
}

export interface RtkArcEpoch {
    tRxJ2000S: number;
    observations: RtkArcObservation[];
}

export interface RtkMeasModel {
    codeSigmaM?: number;
    phaseSigmaM?: number;
    sagnac?: boolean;
    stochastic?: string | { kind: string; elevationWeighting?: boolean };
}

export interface RtkArcConfig {
    baseM: Vec3;
    reference?: string | { kind: string; satelliteId?: string };
    model?: RtkMeasModel;
    baselinePriorSigmaM?: number;
    ambiguityPriorSigmaM?: number;
    initialBaselineM?: Vec3;
    wavelengthsM?: Record<string, number>;
    offsetsM?: Record<string, number>;
    updateOpts?: Record<string, number | boolean>;
    preprocessing?: Record<string, number | boolean | string[]>;
}

export interface RtkStaticArcConfig {
    arc: RtkArcConfig;
    opts?: Record<string, number | boolean>;
}

export interface RtkArcSolution {
    epochs: Array<Record<string, number | string | boolean | string[] | number[] | null>>;
    finalState: Record<string, number | number[] | Record<string, number>>;
    references: Record<string, string>;
}

export interface RtkStaticArcSolution {
    float: Record<string, number | string | boolean | string[] | number[] | Record<string, number> | null>;
    fixed: Record<string, number | string | boolean | string[] | number[] | Record<string, number> | null>;
}

export interface RtkDualFrequencyObservation {
    satelliteId: string;
    roverCode1M: number;
    baseCode1M: number;
    roverPhase1Cycles: number;
    basePhase1Cycles: number;
    roverCode2M: number;
    baseCode2M: number;
    roverPhase2Cycles: number;
    basePhase2Cycles: number;
    freq1Hz: number;
    freq2Hz: number;
    elevationRad?: number;
}

export interface RtkDualFrequencyArcEpoch {
    tRxJ2000S: number;
    observations: RtkDualFrequencyObservation[];
}

export interface RtkWideLaneArcConfig extends RtkArcConfig {
    options?: Record<string, number | boolean>;
}

export interface RtkWideLaneFixedResult {
    wideLaneCycles: Record<string, number>;
    metadata: Record<string, number | string | boolean>;
    solutions?: Array<Record<string, number | string | boolean | number[] | Record<string, number>>>;
}

export interface RtkIonosphereFreeArcConfig extends RtkArcConfig {
    options?: Record<string, number | boolean>;
}

export interface RtkIonosphereFreeArcResult {
    epochs: RtkArcEpoch[];
    wavelengthsM: Record<string, number>;
    offsetsM: Record<string, number>;
}

export interface PppCivil {
    year: number;
    month: number;
    day: number;
    hour: number;
    minute: number;
    second: number;
}

export interface PppObservation {
    satelliteId: string;
    ambiguityId: string;
    codeM: number;
    phaseM: number;
    freq1Hz?: number;
    freq2Hz?: number;
    glonassChannel?: number;
}

export interface PppEpoch {
    civil: PppCivil;
    jdWhole: number;
    jdFraction: number;
    tRxJ2000S: number;
    observations: PppObservation[];
}

export interface PppFloatState {
    positionM: Vec3;
    clocksM: number[];
    ambiguitiesM: Record<string, number>;
    ztdM?: number;
    tropoGradientNorthM?: number;
    tropoGradientEastM?: number;
    residualIonosphereM?: Record<string, number>;
}

export interface PppWeights {
    code?: number;
    phase?: number;
    elevationWeighting?: boolean;
}

export interface PppTroposphere {
    enabled?: boolean;
    estimateZtd?: boolean;
    estimateTropoGradients?: boolean;
    pressureHpa?: number;
    temperatureK?: number;
    relativeHumidity?: number;
    vmf1?: Array<{ mjd: number; ah: number; aw: number }>;
}

export interface PppSolveOptions {
    maxIterations?: number;
    positionToleranceM?: number;
    clockToleranceM?: number;
    ambiguityToleranceM?: number;
    ztdToleranceM?: number;
}

export interface PppFloatConfig {
    weights?: PppWeights;
    tropo?: PppTroposphere;
    options?: PppSolveOptions;
    elevationCutoffDeg?: number;
    residualScreen?: boolean;
    estimateResidualIonosphere?: boolean;
}

export interface PppFixedAmbiguity {
    wavelengthsM: Record<string, number>;
    offsetsM: Record<string, number>;
    ratioThreshold?: number;
}

export interface PppFixedConfig {
    ambiguity: PppFixedAmbiguity;
    weights?: PppWeights;
    tropo?: PppTroposphere;
    options?: PppSolveOptions;
    elevationCutoffDeg?: number;
    estimateResidualIonosphere?: boolean;
}

export interface PppAutoInitOptions {
    initialGuess?: { positionM: Vec3; clockM: number };
    sppInitialGuess?: Vec4;
    sppTroposphere?: boolean;
    sppMet?: SurfaceMetInput;
}

export interface PppResidual {
    epochIndex: number;
    satelliteId: string;
    codeM: number;
    phaseM: number;
    codeWeight: number;
    phaseWeight: number;
}

export interface PppTemporalCorrelation {
    lag1Autocorrelation: number;
    decorrelationTimeEpochs: number;
    decorrelationTimeS: number | null;
    nominalSampleCount: number;
    effectiveSampleCount: number;
    varianceInflationFactor: number;
    arcsUsed: number;
}

export type PppScalarMap = Record<string, number>;

/**
 * Fusion serde contracts are intentionally unrefined until their complete
 * current input and serialized-output surfaces are audited together.
 */
export interface FusionConfig {
    [key: string]: any;
}

export interface FusionTimeSyncConfig {
    [key: string]: any;
}

export interface ImuSampleInput {
    [key: string]: any;
}

export interface FusionLooseMeasurement {
    [key: string]: any;
}

export interface FusionTightEpoch {
    [key: string]: any;
}

export interface FusionUpdate {
    [key: string]: any;
}

export interface FusionState {
    [key: string]: any;
}

export interface FusionTimeSyncStatus {
    [key: string]: any;
}

export interface FusionRtsEpoch {
    [key: string]: any;
}

export interface DllJitterOptions {
    cn0DbHz: number;
    receiverBandwidthHz: number;
    earlyLateSpacingChips: number;
    integrationTimeS?: number;
}

export interface DllJitterResult {
    sigmaChips: number;
    sigmaM: number;
}

export interface MultipathEnvelopeOptions {
    earlyLateSpacingChips: number;
    receiverBandwidthHz: number;
    relativeAmplitude?: number;
    carrierPhaseRad?: number;
}

export interface MultipathEnvelopeResult {
    delayChips: Float64Array;
    errorChips: Float64Array;
}

export interface TerrainLookupOptions {
    interpolation?: "bilinear" | "nearest" | "nearestPosting";
}

export type TerrainPoint = [number, number] | { longitudeDeg: number; latitudeDeg: number };
export type TerrainHeightBatchResult = { ok: true; heightM: number } | { ok: false; error: string };
export type TerrainOrthometricBatchResult = { ok: true; orthometricHeightM: OrthometricHeightM } | { ok: false; error: string };

`;

const topLevelReplacements = [
  [
    "export function sp3MergeInputIdentity(contributors: any, options: any): Sp3MergeInputIdentity;",
    "export function sp3MergeInputIdentity(contributors: Sp3ArtifactIdentityInput[], options?: Sp3MergeIdentityOptions | null): Sp3MergeInputIdentity;",
  ],
  [
    "export function araim(geometry: any, ism: any, allocation: any): any;",
    "export function araim(geometry: AraimGeometry, ism: AraimIsm, allocation?: AraimAllocation | null): AraimResult;",
  ],
  [
    "export function araimFaultModes(geometry: any, ism: any, allocation: any): any;",
    "export function araimFaultModes(geometry: AraimGeometry, ism: AraimIsm, allocation?: AraimAllocation | null): AraimFaultHypothesis[];",
  ],
  [
    "export function araimLpv200Allocation(): any;",
    "export function araimLpv200Allocation(): AraimAllocation;",
  ],
  [
    "export function raim(input: any, options: any): any;",
    "export function raim(input: RaimInput, options?: RaimOptions | null): RaimResult;",
  ],
  [
    "export function raimForSolution(solution: SppSolution, options: any): any;",
    "export function raimForSolution(solution: SppSolution, options?: RaimOptions | null): RaimResult;",
  ],
  [
    "export function raimFdeDesign(rows: any, options: any): any;",
    "export function raimFdeDesign(rows: RangeFdeRow[], options?: RangeFdeOptions | null): RangeFdeResult;",
  ],
  [
    "export function sppInputsFromRinexObs(source: BroadcastEphemeris, obs: RinexObs, options: any): any;",
    "export function sppInputsFromRinexObs(source: BroadcastEphemeris, obs: RinexObs, options?: RinexSppOptions | null): RinexSppEpochInputs[];",
  ],
  [
    "export function solveSppFromRinexObs(source: BroadcastEphemeris, obs: RinexObs, rinex_options: any, solve_options: any): RinexSppSolutionBatch;",
    "export function solveSppFromRinexObs(source: BroadcastEphemeris, obs: RinexObs, rinex_options?: RinexSppOptions | null, solve_options?: RinexSppSolveOptions | null): RinexSppSolutionBatch;",
  ],
  [
    "export function buildRinexRtkArc(ephemeris: Sp3, base_obs: RinexObs, rover_obs: RinexObs, options?: any | null): any;",
    "export function buildRinexRtkArc(ephemeris: Sp3, base_obs: RinexObs, rover_obs: RinexObs, options?: RtkRinexArcOptions | null): { epochs: RtkArcEpoch[]; wavelengthsM: Record<string, number>; offsetsM: Record<string, number> };",
  ],
  [
    "export function buildDualFrequencyRinexRtkArc(ephemeris: Sp3, base_obs: RinexObs, rover_obs: RinexObs, options?: any | null): any;",
    "export function buildDualFrequencyRinexRtkArc(ephemeris: Sp3, base_obs: RinexObs, rover_obs: RinexObs, options?: RtkRinexDualArcOptions | null): { epochs: RtkDualFrequencyArcEpoch[] };",
  ],
  [
    "export function solveRtkArc(epochs: any, config: any): any;",
    "export function solveRtkArc(epochs: RtkArcEpoch[], config: RtkArcConfig): RtkArcSolution;",
  ],
  [
    "export function solveStaticRtkArc(epochs: any, config: any): any;",
    "export function solveStaticRtkArc(epochs: RtkArcEpoch[], config: RtkStaticArcConfig): RtkStaticArcSolution;",
  ],
  [
    "export function fixWideLaneRtkArc(epochs: any, config: any): any;",
    "export function fixWideLaneRtkArc(epochs: RtkDualFrequencyArcEpoch[], config: RtkWideLaneArcConfig): RtkWideLaneFixedResult;",
  ],
  [
    "export function prepareIonosphereFreeRtkArc(epochs: any, wide_lane_cycles: any, config: any): any;",
    "export function prepareIonosphereFreeRtkArc(epochs: RtkDualFrequencyArcEpoch[], wide_lane_cycles: Record<string, number>, config: RtkIonosphereFreeArcConfig): RtkIonosphereFreeArcResult;",
  ],
  [
    "export function solvePppAutoInitFixed(sp3: Sp3, epochs: any, options: any, float_config: any, fixed_config: any): PppFixedSolution;",
    "export function solvePppAutoInitFixed(sp3: Sp3, epochs: PppEpoch[], options: PppAutoInitOptions | null | undefined, float_config: PppFloatConfig, fixed_config: PppFixedConfig): PppFixedSolution;",
  ],
  [
    "export function solvePppAutoInitFloat(sp3: Sp3, epochs: any, options: any, config: any): PppFloatSolution;",
    "export function solvePppAutoInitFloat(sp3: Sp3, epochs: PppEpoch[], options: PppAutoInitOptions | null | undefined, config: PppFloatConfig): PppFloatSolution;",
  ],
  [
    "export function solvePppFixed(sp3: Sp3, epochs: any, float_solution: PppFloatSolution, config: any): PppFixedSolution;",
    "export function solvePppFixed(sp3: Sp3, epochs: PppEpoch[], float_solution: PppFloatSolution, config: PppFixedConfig): PppFixedSolution;",
  ],
  [
    "export function solvePppFloat(sp3: Sp3, epochs: any, initial_state: any, config: any): PppFloatSolution;",
    "export function solvePppFloat(sp3: Sp3, epochs: PppEpoch[], initial_state: PppFloatState, config: PppFloatConfig): PppFloatSolution;",
  ],
];

const classMemberReplacements = [
  [
    "Sp3MergeInputIdentity",
    [
      ["readonly contributors: any;", "readonly contributors: Sp3ArtifactIdentity[];"],
      [
        "readonly precedenceContributors: any;",
        "readonly precedenceContributors: Sp3ArtifactIdentity[] | undefined;",
      ],
    ],
  ],
  [
    "BroadcastEphemeris",
    [
      ["fde(request: any): FdeSolution;", "fde(request: FdeRequest): FdeSolution;"],
      [
        "solveBroadcast(request: any): SppSolution;",
        "solveBroadcast(request: SppRequest): SppSolution;",
      ],
    ],
  ],
  [
    "Sp3",
    [
      ["fde(request: any): FdeSolution;", "fde(request: FdeRequest): FdeSolution;"],
      ["solveSpp(request: any): SppSolution;", "solveSpp(request: SppRequest): SppSolution;"],
      [
        "solveSppBatch(epochs: any, options: any): SppBatchSolution;",
        "solveSppBatch(epochs: SppRequest[], options?: SppBatchOptions | null): SppBatchSolution;",
      ],
      [
        "sppRobustFdeDriver(request: any): FdeSolution;",
        "sppRobustFdeDriver(request: FdeRequest): FdeSolution;",
      ],
    ],
  ],
  [
    "PppFixedSolution",
    [
      [
        "readonly fixedAmbiguitiesCycles: any;",
        "readonly fixedAmbiguitiesCycles: Record<string, number>;",
      ],
      ["readonly fixedAmbiguitiesM: any;", "readonly fixedAmbiguitiesM: PppScalarMap;"],
      ["readonly residualIonosphereM: any;", "readonly residualIonosphereM: PppScalarMap;"],
      ["readonly residuals: any;", "readonly residuals: PppResidual[];"],
      [
        "readonly temporalCorrelation: any;",
        "readonly temporalCorrelation: PppTemporalCorrelation;",
      ],
    ],
  ],
  [
    "PppFloatSolution",
    [
      ["readonly ambiguitiesM: any;", "readonly ambiguitiesM: PppScalarMap;"],
      ["readonly residualIonosphereM: any;", "readonly residualIonosphereM: PppScalarMap;"],
      ["readonly residuals: any;", "readonly residuals: PppResidual[];"],
      [
        "readonly temporalCorrelation: any;",
        "readonly temporalCorrelation: PppTemporalCorrelation;",
      ],
    ],
  ],
  // Fusion JsValue members intentionally retain wasm-bindgen's `any` until
  // their current input and serialized-output contracts are audited together.
  [
    "SignalAnalysisModulation",
    [
      [
        "dllLowerBound(options: any): any;",
        "dllLowerBound(options: DllJitterOptions): DllJitterResult;",
      ],
      [
        "dllThermalNoiseJitter(options: any, processing: DllProcessing): any;",
        "dllThermalNoiseJitter(options: DllJitterOptions, processing: DllProcessing): DllJitterResult;",
      ],
      [
        "multipathErrorEnvelope(options: any, delay_chips: Float64Array): any;",
        "multipathErrorEnvelope(options: MultipathEnvelopeOptions, delay_chips: Float64Array): MultipathEnvelopeResult;",
      ],
    ],
  ],
  [
    "DtedTerrain",
    [
      [
        "heightBatch(points: any, options: any): any;",
        "heightBatch(points: TerrainPoint[], options?: TerrainLookupOptions | null): TerrainHeightBatchResult[];",
      ],
      [
        "heightMWithOptions(longitude_deg: number, latitude_deg: number, options: any): number;",
        "heightMWithOptions(longitude_deg: number, latitude_deg: number, options?: TerrainLookupOptions | null): number;",
      ],
    ],
  ],
  [
    "MmapTerrain",
    [
      [
        "heightBatch(points: any, options: any): any;",
        "heightBatch(points: TerrainPoint[], options?: TerrainLookupOptions | null): TerrainHeightBatchResult[];",
      ],
      [
        "heightMWithOptions(longitude_deg: number, latitude_deg: number, options: any): number;",
        "heightMWithOptions(longitude_deg: number, latitude_deg: number, options?: TerrainLookupOptions | null): number;",
      ],
      [
        "ellipsoidalHeightMWithModel(longitude_deg: number, latitude_deg: number, options: any, geoid: TerrainGeoidModel): EllipsoidalHeightM;",
        "ellipsoidalHeightMWithModel(longitude_deg: number, latitude_deg: number, options: TerrainLookupOptions | null | undefined, geoid: TerrainGeoidModel): EllipsoidalHeightM;",
      ],
      [
        "ellipsoidalHeightMWithOptions(longitude_deg: number, latitude_deg: number, options: any): EllipsoidalHeightM;",
        "ellipsoidalHeightMWithOptions(longitude_deg: number, latitude_deg: number, options?: TerrainLookupOptions | null): EllipsoidalHeightM;",
      ],
      [
        "orthometricHeightBatch(points: any, options: any): any;",
        "orthometricHeightBatch(points: TerrainPoint[], options?: TerrainLookupOptions | null): TerrainOrthometricBatchResult[];",
      ],
      [
        "orthometricHeightMWithOptions(longitude_deg: number, latitude_deg: number, options: any): OrthometricHeightM;",
        "orthometricHeightMWithOptions(longitude_deg: number, latitude_deg: number, options?: TerrainLookupOptions | null): OrthometricHeightM;",
      ],
    ],
  ],
];

function occurrenceCount(text, needle) {
  return text.split(needle).length - 1;
}

function replaceExactly(text, from, to, context) {
  const fromCount = occurrenceCount(text, from);
  const toCount = occurrenceCount(text, to);
  if (fromCount === 1 && toCount === 0) {
    return text.replace(from, to);
  }
  if (fromCount === 0 && toCount === 1) {
    return text;
  }
  throw new Error(
    `${context}: expected exactly one source declaration (or one already-patched declaration), found source=${fromCount}, patched=${toCount}`,
  );
}

function replaceClassMember(text, className, from, to, path) {
  const classMarker = `export class ${className} {`;
  const classCount = occurrenceCount(text, classMarker);
  if (classCount !== 1) {
    throw new Error(`${path}: expected exactly one ${classMarker}, found ${classCount}`);
  }
  const start = text.indexOf(classMarker);
  const nextExport = text.indexOf("\nexport ", start + classMarker.length);
  const end = nextExport === -1 ? text.length : nextExport;
  const declaration = text.slice(start, end);
  const patched = replaceExactly(declaration, from, to, `${path}: ${className}`);
  return text.slice(0, start) + patched + text.slice(end);
}

function insertOverlay(text, path) {
  const anchor = "/* eslint-disable */\n";
  const anchorCount = occurrenceCount(text, anchor);
  if (anchorCount !== 1) {
    throw new Error(
      `${path}: expected exactly one declaration overlay anchor, found ${anchorCount}`,
    );
  }
  const markerCount = occurrenceCount(text, OVERLAY_MARKER);
  if (markerCount === 1) {
    return text;
  }
  if (markerCount !== 0) {
    throw new Error(`${path}: expected zero or one overlay markers, found ${markerCount}`);
  }
  const patched = text.replace(anchor, anchor + "\n" + overlay);
  if (occurrenceCount(patched, OVERLAY_MARKER) !== 1) {
    throw new Error(`${path}: declaration overlay insertion failed`);
  }
  return patched;
}

function patchDeclarations(path) {
  let text = insertOverlay(readFileSync(path, "utf8"), path);
  for (const [from, to] of topLevelReplacements) {
    text = replaceExactly(text, from, to, `${path}: top-level declaration`);
  }
  for (const [className, replacements] of classMemberReplacements) {
    for (const [from, to] of replacements) {
      text = replaceClassMember(text, className, from, to, path);
    }
  }
  return text;
}

const declarationPaths = ["pkg/sidereon.d.ts", "pkg-node/sidereon.d.ts"];
const patchedDeclarations = declarationPaths.map((path) => [path, patchDeclarations(path)]);

writeFileSync("pkg-node/package.json", JSON.stringify({ type: "commonjs" }, null, 2) + "\n");
for (const [path, text] of patchedDeclarations) {
  writeFileSync(path, text);
}
