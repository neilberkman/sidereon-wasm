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
const NODE_PATH_ADAPTER_MARKER = "/* sidereon Node path-open adapters */";

const overlay = `${OVERLAY_MARKER}
export type Vec3 = [number, number, number] | Float64Array;
export type Vec4 = [number, number, number, number] | Float64Array;
export type Matrix3 = number[] | Float64Array;

export interface SourceSensor {
    /** Sensor position in a caller-chosen 2D or 3D Cartesian frame, in metres. */
    positionM: number[] | Float64Array;
    /** Optional propagation-speed override for this sensor, in metres per second. */
    propagationSpeedMS?: number;
}

export type SourceLocateMode = "toa" | "ToA" | "TOA" | "tdoa" | "TDOA";

export interface SourceSolveModeObject {
    mode?: SourceLocateMode;
    referenceSensor?: number;
}

export type SourceSolveMode = SourceLocateMode | SourceSolveModeObject;

export interface SourceLocateOptions {
    mode?: SourceLocateMode;
    /** Reference sensor index for TDOA mode; defaults to zero. */
    referenceSensor?: number;
    /** Timing standard deviation used for covariance, CRLB, and influence scores. */
    timingSigmaS?: number;
    /**
     * Whether to compute per-sensor leave-one-out influence diagnostics.
     * Defaults to true. Set to false to skip one nonlinear re-solve per sensor;
     * perSensorInfluence is then empty and every other output is bit-identical.
     */
    includeInfluence?: boolean;
    loss?: "linear" | "softL1" | "soft_l1" | "huber" | "cauchy" | "arctan";
    fScaleS?: number;
    ftol?: number;
    xtol?: number;
    gtol?: number;
    maxNfev?: number;
}

export interface SourceInitialGuess {
    positionM: number[];
    originTimeS: number | null;
    residualRmsS: number;
}

export interface SourceResidual {
    sensorIndex: number;
    referenceSensorIndex: number | null;
    residualS: number;
}

export interface SourceSensorInfluence {
    sensorIndex: number;
    residualS: number;
    leaveOneOutResidualS: number | null;
    positionDeltaM: number | null;
    originTimeDeltaS: number | null;
    /** First-derivative robust-loss weight for the full-solution residual. */
    lossWeight: number;
    /**
     * max(abs(residualS), abs(leaveOneOutResidualS)) / timingSigmaS, or
     * abs(residualS) / timingSigmaS when the leave-one-out solve is unavailable.
     * Robust-loss downweighting is reported separately in lossWeight.
     */
    score: number;
}

export interface SourceCovariance {
    state: number[][];
    positionM2: number[][];
    originTimeS2: number | null;
    timingSigmaS: number;
}

export interface SourceGeometryQuality {
    tier: "RankDeficient" | "ZeroRedundancy" | "Weak" | "Nominal";
    redundancy: number;
    rank: number;
    conditionNumber: number;
    gdop: number;
    raimCheckable: boolean;
    covarianceValidated: boolean;
}

export interface SourceSolution {
    positionM: number[];
    originTimeS: number | null;
    covariance: SourceCovariance | null;
    residuals: SourceResidual[];
    perSensorInfluence: SourceSensorInfluence[];
    geometryQuality: SourceGeometryQuality;
    initialGuess: SourceInitialGuess;
    status: number;
    nfev: number;
    njev: number;
    cost: number;
    optimality: number;
}

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
    verifyContinuity?: Sp3ContinuityOptions | null;
    /** Record per-epoch provenance; omitted or null records none. */
    provenance?: "summary" | "full" | null;
}

export type Sp3MergeOptions = Sp3MergeIdentityOptions;

export interface Sp3ContinuityOptions {
    orbitClass?: "meo_gnss" | "geosynchronous" | "leo" | null;
    residualToleranceM?: number | null;
    gapThresholdFactor?: number | null;
}

export interface ContinuityDefect {
    kind: "duplicate_epoch" | "single_sample_series" | "speed_bound" | "hold_out_residual";
    satellite: string;
    fromJ2000S: number | undefined;
    toJ2000S: number | undefined;
    magnitude: number | undefined;
    bound: number | undefined;
    /** Duplicate epoch or held-out sample epoch, seconds since J2000. */
    epochJ2000S: number | undefined;
    /** Duplicate epoch: how many times it occurs. */
    occurrences: number | undefined;
    /** Speed bound: the pair's interval, seconds. */
    intervalS: number | undefined;
    /** Speed bound: the pair's displacement, meters. */
    displacementM: number | undefined;
    /** Speed bound: displacement over interval, meters per second. */
    impliedSpeedMS: number | undefined;
    /** Speed bound: the bound it exceeded, meters per second. */
    boundMS: number | undefined;
    /** Hold-out residual: the preceding sample epoch, seconds since J2000. */
    precedingJ2000S: number | undefined;
    /** Hold-out residual: stored record to prediction, meters. */
    residualM: number | undefined;
    /** Hold-out residual: the tolerance it exceeded, meters. */
    toleranceM: number | undefined;
    /** Hold-out residual: epochs of the retained nodes the prediction used, ascending. */
    nodeEpochsJ2000S: number[] | undefined;
}

export interface ContinuityReport {
    attested: boolean;
    defects: ContinuityDefect[];
    pairsChecked: number;
    residualsChecked: number;
    residualsSkipped: number;
}

export type CellSelection =
    | { kind: "single_source"; source: number }
    | { kind: "precedence"; source: number; members: number[] }
    | { kind: "combined"; rule: "mean" | "median" | "precedence"; members: number[] };

export interface MergeContinuityCell {
    epochJ2000S: number;
    role: "held_out" | "interpolation_node" | "pair_end" | "repeated_epoch";
    selection: CellSelection | undefined;
}

export interface MergeContinuityViolation {
    defect: ContinuityDefect;
    fromSources: number[];
    toSources: number[];
    cells: MergeContinuityCell[];
    sources: number[];
    crossesContributors: boolean;
}

export interface MergeContinuityReport extends ContinuityReport {
    violations: MergeContinuityViolation[];
    splices: MergeContinuityViolation[];
}

export interface MergeCellProvenance {
    epochJ2000Seconds: number;
    satellite: string;
    position: CellSelection | undefined;
    clock: CellSelection | undefined;
}

export interface MergePrecedenceTransition {
    satellite: string;
    epochJ2000Seconds: number;
    fromSource: number | undefined;
    toSource: number | undefined;
    reason: "sole_availability" | "precedence" | "outlier_rejection" | "consensus_change";
}

export interface MergeContributorCoverage {
    source: number;
    cellsContributed: number;
    cellsSelected: number;
    firstEpochJ2000Seconds: number | undefined;
    lastEpochJ2000Seconds: number | undefined;
    cellsAbsent: number;
}

export interface MergeProvenance {
    mode: "summary" | "full";
    cells: MergeCellProvenance[];
    transitions: MergePrecedenceTransition[];
    coverage: MergeContributorCoverage[];
}

export interface WindowContinuityVerdict {
    decision: "accept" | "refuse";
    accepted: boolean;
    influencingDefects: ContinuityDefect[];
    influencingSplices: MergeContinuityViolation[];
    allDefects: ContinuityDefect[];
    allSplices: MergeContinuityViolation[];
}

export interface NominalCoverageInterval {
    from: Date;
    until: Date;
}

export interface NominalCoverage {
    observed: NominalCoverageInterval | null;
    predicted: NominalCoverageInterval | null;
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
    /** Defaults to "singleFrequency"; the broadcast group delay applies to single-frequency code only. */
    pseudorangeCode?: PseudorangeCode;
    qzssClock?: QzssClock;
    troposphereModel?: TroposphereModel;
}

export type PseudorangeCode = "singleFrequency" | "ionosphereFree";
export type QzssClock = "gps" | "separate";
export type TroposphereModel = "rtklib" | "saastamoinenNiell";

export interface FdeRequest extends SppRequest {
    pFa?: number;
    weights?: RaimWeightsInput | RaimWeights | RaimWeightsMode | {
        mode: "solution" | "unit" | "bySatellite";
        satelliteIds?: string[];
        weights?: number[] | Float64Array;
        values?: number[] | Float64Array;
    };
    weightsMode?: RaimWeightsMode;
    nSystems?: number;
    weightEntries?: Array<{ satelliteId: string; elevationDeg: number; cn0Dbhz?: number }>;
    varianceOptions?: { aM?: number; bM?: number; model?: "elevation" | "elevation_cn0"; cn0Dbhz?: number; cn0ScaleM2?: number };
    maxExclusions?: number;
    maxExclusionRmsM?: number;
}

export type RaimWeightsMode = "solution" | "unit" | "bySatellite";

export interface SppSolution {
    /** Per-used-satellite variance, aligned with usedSats and residualsM. */
    readonly pseudorangeVariancesM2: Float64Array;
    /** Effective inverse-variance weight, including robust factors. */
    readonly weights: Float64Array;
}

export interface FdeSolution {
    /** Complete accepted receiver solution and its solve diagnostics. */
    readonly solution: SppSolution;
    /** Core RAIM result retained for the accepted solution. */
    readonly raim: FdeRaimResult;
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
    qzssClock?: QzssClock;
    troposphereModel?: TroposphereModel;
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
    qzssClock: QzssClock;
    troposphereModel: TroposphereModel;
}

export interface StaticSolveOptions {
    initialPositionM?: [number, number, number];
    withGeodetic?: boolean;
    robust?: RobustOptions;
    qzssClock?: QzssClock;
    troposphereModel?: TroposphereModel;
}

export interface PppCorrectionOptions {
    solidEarthTide?: boolean;
    phaseWindup?: boolean;
    ut1Validity?: "strict" | "permissive";
    stationTideConstants?: StationTideConstants;
    satelliteAntenna?: {
        freq1Label: string;
        freq1Hz: number;
        freq2Label: string;
        freq2Hz: number;
        antennas: Array<{
            sat: string;
            validFrom?: PppCivil;
            validUntil?: PppCivil;
            frequencies: Array<{
                label: string;
                pcoM: [number, number, number];
                noaziPcvM: Array<[number, number]>;
            }>;
        }>;
    };
    poleTide?: { xpArcsec: number; ypArcsec: number };
    oceanLoading?: { amplitudeM: number[][]; phaseDeg: number[][] };
}

export interface PppCorrectionEpochVector {
    epochIndex: number;
    vectorM: [number, number, number];
}
export interface PppCorrectionSatelliteScalar {
    sat: string;
    epochIndex: number;
    valueM: number;
}
export interface PppCorrectionSatelliteVector {
    sat: string;
    epochIndex: number;
    vectorM: [number, number, number];
}
export interface PppCorrectionEpoch {
    year: number; month: number; day: number; hour: number; minute: number;
    second: number; tRxJ2000S: number;
    observations: Array<{ satelliteId: string; freq1Hz: number; freq2Hz: number; glonassChannel?: number | null }>;
}
export interface CodeBiasOptions {
    usedObservablesPerSat?: Array<{ sat: string; obs1: string; obs2: string }>;
    usedObservablesDefault?: Array<{ system: string; obs1: string; obs2: string }>;
    clockReference?: Array<{ system: string; obs1: string; obs2: string }>;
}
export interface PppCorrections {
    tide: PppCorrectionEpochVector[];
    poleTide: PppCorrectionEpochVector[];
    oceanLoading: PppCorrectionEpochVector[];
    windupM: PppCorrectionSatelliteScalar[];
    satPcoEcef: PppCorrectionSatelliteVector[];
    satPcvM: PppCorrectionSatelliteScalar[];
    codeBiasM: PppCorrectionSatelliteScalar[];
    ut1Degraded: "beforeCoverage" | "afterCoverage" | null;
    /** Legacy text view, retained for compatibility. */
    warnings: string[];
    /** Lossless typed record references and warning/skip variants. */
    diagnostics: NmeaDiagnostics;
}
export type PppCorrectionsErrorDetail =
    | { family: "PppCorrectionsError"; kind: "INVALID_INPUT"; field: string; reason: string; message: string }
    | { family: "PppCorrectionsError"; kind: "EPOCH"; epochIndex: number; cause: { kind: "INVALID_INPUT"; field: string; reason: string } | { kind: "OUTSIDE_COVERAGE"; reason: "BEFORE_COVERAGE" | "AFTER_COVERAGE" }; message: string }
    | { family: "PppCorrectionsError"; kind: "TIDE" | "POLE_TIDE" | "OCEAN_LOADING"; epochIndex: number; cause: TideErrorDetail; message: string }
    | { family: "PppCorrectionsError"; kind: "WINDUP_FREQUENCY"; epochIndex: number; satellite: string; field: string; reason: string; message: string }
    | { family: "PppCorrectionsError"; kind: "SATELLITE_ANTENNA_FREQUENCY"; field: string; reason: string; message: string }
    | { family: "PppCorrectionsError"; kind: "BIAS"; cause: BiasErrorDetail; message: string }
    | { family: "PppCorrectionsError"; kind: "CODE_BIAS_OBSERVABLE"; epochIndex: number; satellite: string; field: string; reason: string; message: string };

export interface RaimInput {
    usedSats: string[];
    residualsM: number[] | Float64Array;
    variancesM2?: number[] | Float64Array;
}

export type RaimWeightsInput =
    | { isUnit: true }
    | { satelliteIds: string[]; weights: number[] | Float64Array }
    | { satelliteIds: string[]; values: number[] | Float64Array }
    | Array<{ satelliteId: string; weight: number }>
    | Record<string, number>
    | RaimWeightsMode
    | { mode: RaimWeightsMode; satelliteIds?: string[]; weights?: number[] | Float64Array; values?: number[] | Float64Array };

export interface RaimOptions {
    pFa?: number;
    weights?: RaimWeightsInput | RaimWeights;
    weightsMode?: RaimWeightsMode;
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
    testable: boolean;
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
    maxExclusionRmsM?: number;
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
    ambiguityId: string;
    codeM: number;
    phaseM: number;
    lli?: number | null;
}

export interface RtkArcEpoch {
    base: RtkArcObservation[];
    rover: RtkArcObservation[];
    satellitePositionsM: Record<string, Vec3>;
    baseSatellitePositionsM?: Record<string, Vec3>;
    roverSatellitePositionsM?: Record<string, Vec3>;
    velocityMps?: Vec3 | null;
    predictionTimeS?: number | null;
    predictionEpoch?: ExactEpoch | null;
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
    ambiguityId: string;
    p1M: number;
    p2M: number;
    phi1Cycles: number;
    phi2Cycles: number;
    f1Hz: number;
    f2Hz: number;
    lli1?: number | null;
    lli2?: number | null;
}

export interface RtkDualFrequencySatelliteObservation {
    satelliteId: string;
    base: RtkDualFrequencyObservation;
    rover: RtkDualFrequencyObservation;
}

export interface RtkDualFrequencyArcEpoch {
    jdWhole: number;
    jdFraction: number;
    epochSortKey?: string | null;
    gapTimeS?: number | null;
    observations: RtkDualFrequencySatelliteObservation[];
    satellitePositionsM: Record<string, Vec3>;
    baseSatellitePositionsM?: Record<string, Vec3>;
    roverSatellitePositionsM?: Record<string, Vec3>;
    velocityMps?: Vec3 | null;
    predictionTimeS?: number | null;
    gapEpoch?: ExactEpoch | null;
    predictionEpoch?: ExactEpoch | null;
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
    /** RINEX 3.04 signal codes ("1C" or "C1C") of the two pseudoranges and two carrier phases. */
    signals?: PppObservationSignals;
}

export interface PppObservationSignals {
    code1: string;
    code2: string;
    phase1: string;
    phase2: string;
}

/** The iteration cap and convergence tolerances a PPP float solve ran with. */
export interface PppAppliedSolveOptions {
    maxIterations: number;
    positionToleranceM: number;
    clockToleranceM: number;
    ambiguityToleranceM: number;
    ztdToleranceM: number;
}

export interface PppObservationRef {
    epochIndex: number;
    ambiguityId: string;
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
    /** Input epoch index. */
    epochIndex: number;
    satelliteId: string;
    ambiguityId: string;
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

export type NdmTextIssue =
    | "lineBreak"
    | "surroundingWhitespace"
    | "interiorWhitespace"
    | "keywordSeparator"
    | "xmlIllegalCharacter"
    | "empty"
    | "detachedComment"
    | "repeatedParameter"
    | "commentNotCarried";

/** The detail of an OmmError, OpmError, OemError or CdmError. Fields the kind does not carry are null. */
export interface NdmErrorDetail {
    kind:
        | "MISSING_FIELD"
        | "INVALID_FIELD"
        | "FIELD"
        | "EPOCH"
        | "DUPLICATE_FIELD"
        | "UNKNOWN_FIELD"
        | "CSV_COLUMN_COUNT"
        | "CSV_EMPTY_BLOCK"
        | "MALFORMED_LINE"
        | "UNIT_MISMATCH"
        | "MULTIPLE_MESSAGES"
        | "IN_RECORD"
        | "CSV_COLUMN_ORDER"
        | "INCOMPATIBLE_METADATA"
        | "UNWRITABLE_TEXT"
        | "INCOMPLETE_STATE_VECTOR"
        | "MALFORMED_XML"
        | "UNEXPECTED_OBJECT_COUNT"
        | "UNKNOWN_OBJECT"
        | "REPEATED_OBJECT"
        | "HARD_BODY_RADIUS_COMMENT";
    message: string;
    field: string | null;
    value: string | null;
    /** The validation category of INVALID_FIELD, or an NdmTextIssue for UNWRITABLE_TEXT. */
    issue: string | null;
    line: number | null;
    unit: string | null;
    expectedUnit: string | null;
    first: string | null;
    second: string | null;
    count: number | null;
    expectedCount: number | null;
    index: number | null;
    source: NdmErrorDetail | null;
}

export interface OmmSpacecraft {
    comments?: string[];
    massKg?: number | null;
    solarRadAreaM2?: number | null;
    solarRadCoeff?: number | null;
    dragAreaM2?: number | null;
    dragCoeff?: number | null;
}

export interface OmmCovariance {
    comments?: string[];
    covRefFrame?: string | null;
    /** The 21 lower-triangle values CX_X, CY_X, CY_Y, ... CZ_DOT_Z_DOT as read. */
    lowerTriangle: number[];
}

export interface OmmUserDefined {
    parameter: string;
    value: string;
}

export interface OmmComments {
    header?: string[];
    metadata?: string[];
    meanElements?: string[];
    tleParameters?: string[];
    userDefined?: string[];
}

export interface OmmMeta {
    ccsdsOmmVers?: string;
    classification?: string;
    creationDate?: string;
    originator?: string;
    messageId?: string;
    objectName?: string;
    objectId?: string;
    centerName?: string;
    refFrame?: string;
    refFrameEpoch?: string;
    timeSystem?: string;
    meanElementTheory?: string;
    semiMajorAxisKm?: number;
    gmKm3S2?: number;
    spacecraft?: OmmSpacecraft;
    ephemerisType?: number;
    classificationType?: string;
    elementSetNo?: number;
    revAtEpoch?: number;
    bstar?: number;
    btermM2Kg?: number;
    meanMotionDot?: number;
    meanMotionDdot?: number;
    agomM2Kg?: number;
    covariance?: OmmCovariance;
    userDefined?: OmmUserDefined[];
    comments?: OmmComments;
}

export interface OmmArray {
    omms: Omm[];
    skipped: Array<{ index: number; reason: NdmErrorDetail }>;
}

export interface OpmMeta {
    ccsdsOpmVers?: string;
    classification?: string;
    creationDate?: string;
    originator?: string;
    messageId?: string;
    comments?: string[];
    userDefined?: OmmUserDefined[];
    userDefinedComments?: string[];
}

export interface OemMeta {
    ccsdsOemVers?: string;
    classification?: string;
    creationDate?: string;
    originator?: string;
    messageId?: string;
    comments?: string[];
}

export interface OemMetadataMeta {
    refFrameEpoch?: string;
    useableStartTime?: string;
    useableStopTime?: string;
    interpolation?: string;
    interpolationDegree?: number;
    comments?: string[];
}

/** A comment placed after \`position\` state lines or covariance matrices of its segment. */
export interface OemComment {
    position: number;
    text: string;
}

export interface OemSkippedState {
    /** One-based line number. */
    line: number;
    /** Zero-based segment index. */
    segment: number;
    text: string;
    reason: "itemCount" | "invalidField";
    itemCount: number | null;
    field: string | null;
    issue: string | null;
}

export interface CdmOdParameters {
    comments?: string[];
    timeLastobStart?: string | null;
    timeLastobEnd?: string | null;
    recommendedOdSpanD?: number | null;
    actualOdSpanD?: number | null;
    obsAvailable?: number | null;
    obsUsed?: number | null;
    tracksAvailable?: number | null;
    tracksUsed?: number | null;
    residualsAcceptedPct?: number | null;
    weightedRms?: number | null;
}

export interface CdmAdditionalParameters {
    comments?: string[];
    areaPcM2?: number | null;
    areaDrgM2?: number | null;
    areaSrpM2?: number | null;
    massKg?: number | null;
    cdAreaOverMassM2Kg?: number | null;
    crAreaOverMassM2Kg?: number | null;
    thrustAccelerationMS2?: number | null;
    sedrWKg?: number | null;
}

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
    velocityCovarianceRtn?: number[] | Float64Array;
    dragCovarianceRtn?: number[] | Float64Array;
    srpCovarianceRtn?: number[] | Float64Array;
    thrustCovarianceRtn?: number[] | Float64Array;
    metadataComments?: string[];
    odParameters?: CdmOdParameters;
    additionalParameters?: CdmAdditionalParameters;
    stateComments?: string[];
    covarianceComments?: string[];
}

export interface CdmMeta {
    ccsdsCdmVers?: string;
    comments?: string[];
    creationDate?: string;
    originator?: string;
    messageFor?: string;
    messageId?: string;
    relativeComments?: string[];
    tca?: string;
    missDistanceM?: number;
    relativeSpeedMS?: number;
    relativePositionRtnM?: [number | null, number | null, number | null];
    relativeVelocityRtnMS?: [number | null, number | null, number | null];
    startScreenPeriod?: string;
    stopScreenPeriod?: string;
    screenVolumeFrame?: string;
    screenVolumeShape?: string;
    screenVolumeM?: [number | null, number | null, number | null];
    screenEntryTime?: string;
    screenExitTime?: string;
    collisionProbability?: number;
    collisionProbabilityMethod?: string;
    hardBodyRadiusM?: number;
}

export interface BiasLookup {
    status:
        | "available"
        | "absent"
        | "unsupportedScale"
        | "ambiguous"
        | "carrierFrequencyRequired"
        | "invalidCarrierFrequency"
        | "carrierFrequencyUnknown"
        | "undefinedSlopeReference"
        | "invalidEpoch"
        | (string & {});
    /** Set only when status is "available". */
    value: number | null;
    /** Indices into BiasSet.records. */
    records: number[];
    overridden: number[];
    productScale: string | null;
    queryScale: string | null;
    observable: string | null;
}

export type BiasDepartureDetail =
    | { kind: "headerLayout"; reason: string }
    | { kind: "otherVersion"; version: string }
    | { kind: "missingFooter" }
    | { kind: "contentAfterFooter"; line: number }
    | { kind: "unexpectedControlLine"; line: number }
    | { kind: "unclosedBlock"; name: string; line: number }
    | { kind: "unopenedBlockEnd"; name: string; line: number }
    | { kind: "mismatchedBlockEnd"; open: string; close: string; line: number }
    | { kind: "nestedBlock"; open: string; inner: string; line: number }
    | { kind: "missingBlock"; name: string }
    | { kind: "unknownBlock"; name: string; line: number }
    | { kind: "blockStartSuffix"; line: number }
    | { kind: "dataOutsideBlock"; line: number }
    | { kind: "missingDeclaration"; keyword: string }
    | { kind: "unsupportedBiasMode"; line: number; label: string }
    | { kind: "nonStandardTimeSystem"; line: number; label: string }
    | { kind: "headerModeMismatch"; header: string; description: "absolute" | "relative" | "unspecified" }
    | { kind: "unknownDcbTimeSystem"; line: number; label: string }
    | { kind: "estimateCountMismatch"; declared: number; solutionRows: number }
    | { kind: "other"; message: string };

export type BiasNoticeDetail =
    | { kind: "departure"; departure: BiasDepartureDetail }
    | { kind: "invalidUtf8"; line: number }
    | { kind: "repeatedDeclaration"; line: number; keyword: string }
    | { kind: "conflictingDeclaration"; line: number; keyword: string }
    | { kind: "overlap"; first: number; second: number }
    | { kind: "dcbTimeSystemAssumed" }
    | { kind: "dcbTimeSystemAlias"; line: number; label: string }
    | { kind: "unknown"; message: string };

export type BiasErrorDetail =
    | { kind: "invalidInput"; field: string; reason: string }
    | { kind: "invalidEpoch" }
    | { kind: "unknownObservable"; code: string }
    | { kind: "unsupportedVersion"; version: string }
    | { kind: "missingDcbMetadata" }
    | { kind: "missingClockReference" }
    | { kind: "missingWriterMetadata"; field: string }
    | { kind: "utf8" }
    | { kind: "departure"; departure: BiasDepartureDetail }
    | { kind: "invalidUtf8Line"; line: number }
    | { kind: "unsupportedTimeSystem"; scale: string | null }
    | { kind: "dcbRecordMismatch"; record: number; field: string };

export interface BiasError extends Error {
    readonly name: "BiasError";
    readonly detail: BiasErrorDetail;
}

export interface BiasSet {
    /** Structured parser findings; notices remains the legacy string view. */
    readonly noticeDetails: BiasNoticeDetail[];
}

export interface RinexNavDiagnostic {
    /** One-based line number. */
    line: number;
    satellite: string;
    message: string;
}

export interface RinexNavOtherBlock {
    line: number;
    satellite: string;
    messageToken: string | null;
    kind: string;
}

export interface SbasSkippedLine {
    line: number;
    kind: "blank" | "comment" | "nonRecord";
}

export interface SbasRefusedLine {
    line: number;
    reason: "ambiguousWeek" | "checksumMismatch" | (string & {});
    week: number | null;
    written: number | null;
    computed: number | null;
}

export interface SbasDeparture {
    kind: "unrecognizedPreamble" | "declaredMessageType" | (string & {});
    message: string;
    preamble: number | null;
    declared: number | null;
    carried: number | null;
    line: number | null;
}

/** The side of the UT1 table an instant outside it lies on. */
export type Ut1DegradeReason = "beforeCoverage" | "afterCoverage";

/** A result computed under a UT1 validity policy with the departure from the UT1 table it accepted. */
export interface Ut1Validated<T> {
    value: T;
    ut1Degraded: Ut1DegradeReason | null;
}

/** The row class of a space-weather sample, least trusted last. */
export type SpaceWeatherClass =
    | "observed"
    | "interpolated"
    | "notObserved"
    | "dailyPredicted"
    | "monthlyPredicted";

/**
 * A space-weather lookup policy: "default", "lenient", or the default policy
 * with the named fields overridden. Unknown keys are refused.
 */
export type SpaceWeatherPolicyInput =
    | "default"
    | "lenient"
    | {
          allowInterpolated?: boolean;
          allowNotObserved?: boolean;
          allowDailyPredicted?: boolean;
          allowMonthlyPredicted?: boolean;
          requireGeomagnetic?: boolean;
      };

/** The seven-element NRLMSISE-00 Ap history at an epoch. */
export interface SpaceWeatherApHistory {
    ap: number[];
    class: SpaceWeatherClass;
    apDefaulted: boolean;
    binsFromDailyAp: number;
}

/** Count of every physical line of a bias product by what the reader made of it. */
export interface BiasLineCounts {
    lines: number;
    headerFooter: number;
    comments: number;
    blank: number;
    blockDelimiters: number;
    infoRows: number;
    records: number;
    skipped: number;
    blockBody: number;
    other: number;
}

/** Options of the SBAS log readers. */
export interface SbasLogOptions {
    policy?: "strict" | "lenient";
    referenceWeek?: number;
}

export type Ut1Validity = "strict" | "permissive";

/** A geodetic station for the observe and meridian-transit functions. */
export interface ObserveStation {
    latitudeDeg: number;
    longitudeDeg: number;
    /** Kilometres above the ellipsoid; 0 when omitted. */
    altitudeKm?: number;
}

/** Reduction options of observe; each omitted field takes the engine default. */
export interface ObserveOptions {
    polarMotion?: { xpArcsec: number; ypArcsec: number } | null;
    refraction?: { pressureMbar: number; temperatureC: number } | null;
    deflection?: boolean | null;
    aberration?: boolean | null;
}

/** An equatorial position: right ascension, declination and distance. */
export interface ObserveEquatorial {
    rightAscensionDeg: number;
    rightAscensionHours: number;
    declinationDeg: number;
    distanceKm: number;
}

/** The result of observe, observeSpkBody and their WithValidity variants. */
export interface ObserveResult {
    astrometric: ObserveEquatorial;
    apparentIcrs: ObserveEquatorial;
    apparent: ObserveEquatorial;
    horizontal: { azimuthDeg: number; elevationDeg: number; rangeKm: number };
    hourAngleDeg: number;
    hourAngleHours: number;
    ecliptic: { longitudeDeg: number; latitudeDeg: number; distanceKm: number };
    reduced: boolean;
}

/** One meridian transit: an upper or lower culmination. */
export interface MeridianTransitEvent {
    /** UTC unix microseconds. */
    timeUnixUs: number;
    /** Or, for a kind this binding does not name yet, the engine variant's name. */
    kind: "upper" | "lower" | (string & {});
    altitudeDeg: number;
}

export interface BodyAzEl {
    azimuthDeg: number;
    elevationDeg: number;
    rangeKm: number;
}

export interface MoonIlluminationResult {
    illuminatedFraction: number;
    phaseAngleDeg: number;
}

export interface RejectedTleRecord {
    /** One-based line number of the first rejected line (the name line when there was one). */
    lineNumber: number;
    name: string;
    issue: "invalid" | "missingLine2" | "orphanLine2" | "orphanName";
    message: string;
    detail: TleRecordIssueDetail;
}

export interface Sgp4ExactFloat { decimal: string; bitsHex: string; }
export interface Sgp4ExactInteger { decimal: string; }
export type Sgp4InputErrorKind =
    | "nonFinite" | "notPositive" | "negative" | "outOfRange" | "missing"
    | "floatParse" | "intParse" | "invalidCivilDate" | "invalidCivilTime";
export type Sgp4ErrorCause =
    | { kind: "invalidInput"; field: string; inputKind: Sgp4InputErrorKind; reason: string }
    | { kind: "nonFiniteOutput"; field: string }
    | { kind: "invalidTle"; message: string }
    | { kind: "sgp4"; code: number }
    | { kind: "resonanceStepBudget"; budget: string };
export interface Sgp4ErrorDetail { family: "sgp4"; cause: Sgp4ErrorCause; }
export type TleErrorCause =
    | { kind: "nonAscii" } | { kind: "format" } | { kind: "satelliteMismatch" }
    | { kind: "invalidCatalogNumber"; value: string; reason: string }
    | { kind: "catalogNumberOutOfRange"; catalogNumber: number }
    | { kind: "invalidField"; field: string; reason: string }
    | { kind: "field"; value: string }
    | { kind: "checksumMismatch"; lineLabel: string; expected: number; computed: number }
    | { kind: "checksumNotDigit"; lineLabel: string; found: string; computed: number };
export interface TleErrorDetail { family: "tle"; cause: TleErrorCause; }
export type TleRecordIssueDetail =
    | { kind: "invalid"; message: string; cause: Sgp4ErrorCause }
    | { kind: "missingLine2" } | { kind: "orphanLine2" } | { kind: "orphanName" };
export type DecayLatchedErrorCause =
    | { kind: "decayed"; firstFailingEpochMinutes: Sgp4ExactFloat; requestedEpochMinutes: Sgp4ExactFloat }
    | { kind: "propagation"; message: string; cause: Sgp4ErrorCause };
export interface DecayLatchedErrorDetail { family: "decayLatched"; cause: DecayLatchedErrorCause; }
export type LookAngleErrorCause =
    | { kind: "invalidInput"; field: string; reason: string }
    | { kind: "init" | "propagate"; message: string; cause: Sgp4ErrorCause }
    | { kind: "frameTransform"; message: string; cause: FrameTransformCause };
export interface LookAngleErrorDetail { family: "lookAngle"; cause: LookAngleErrorCause; }
export type PassErrorCause =
    | { kind: "invalidInput"; field: string; reason: string }
    | { kind: "ut1OutsideCoverage"; reason: "beforeCoverage" | "afterCoverage" };
export interface PassErrorDetail { family: "pass"; cause: PassErrorCause; }
export type TrfBackendErrorCause =
    | { kind: "failed"; message: string }
    | { kind: "badDimensions"; expectedM: number; expectedN: number; got: number };
export type TrfErrorCause =
    | { kind: "emptyResidual" } | { kind: "emptyParameters" }
    | { kind: "nonFiniteParameters" } | { kind: "nonFiniteInitialResidual" }
    | { kind: "insufficientRows"; m: number; n: number }
    | { kind: "sizeOverflow"; m: number; n: number }
    | { kind: "degreeOverflow"; degree: number } | { kind: "invalidMaxNfev" }
    | { kind: "invalidFScale"; fScale: Sgp4ExactFloat }
    | { kind: "invalidXScaleLength"; expected: number; got: number }
    | { kind: "invalidXScaleValue"; index: number; value: Sgp4ExactFloat }
    | { kind: "invalidJacobianLength"; expected: number; got: number }
    | { kind: "invalidResidualLength"; expected: number; got: number }
    | { kind: "invalidSliceLength"; what: string; expected: number; got: number }
    | { kind: "invalidSvdOutput"; message: string }
    | { kind: "backend"; cause: TrfBackendErrorCause };
export type TleFitErrorCause =
    | { kind: "arcTooShort"; samples: number; needed: number }
    | { kind: "invalidInput"; field: string; reason: string }
    | { kind: "epochsNotIncreasing"; index: number }
    | { kind: "epochOutsideArc" } | { kind: "mixedVelocityPresence" } | { kind: "notElliptical" }
    | { kind: "inclinationNearRetrograde"; inclinationDeg: Sgp4ExactFloat }
    | { kind: "seedPropagation"; epochIndex: number; message: string; cause: Sgp4ErrorCause }
    | { kind: "finalElements"; message: string; cause: Sgp4ErrorCause }
    | { kind: "solver"; message: string; cause: TrfErrorCause }
    | { kind: "solutionInfeasible" }
    | { kind: "didNotConverge"; bestEffortFit: Sgp4BestEffortFit }
    | { kind: "tleEncode"; message: string; cause: TleErrorCause };
export interface TleFitErrorDetail { family: "tleFit"; cause: TleFitErrorCause; }
export type Sgp4ExactJulianDate = [Sgp4ExactFloat, Sgp4ExactFloat];
export interface Sgp4FitElements {
    epoch: Sgp4ExactJulianDate;
    bstar: Sgp4ExactFloat;
    mean_motion_dot: Sgp4ExactFloat | null;
    mean_motion_double_dot: Sgp4ExactFloat | null;
    eccentricity: Sgp4ExactFloat;
    argument_of_perigee_deg: Sgp4ExactFloat;
    inclination_deg: Sgp4ExactFloat;
    mean_anomaly_deg: Sgp4ExactFloat;
    mean_motion_rev_per_day: Sgp4ExactFloat;
    right_ascension_deg: Sgp4ExactFloat;
    catalog_number: Sgp4ExactInteger | null;
    omm_epoch_days: Sgp4ExactFloat | null;
}
export interface Sgp4OmmEpoch {
    year: Sgp4ExactInteger;
    month: Sgp4ExactInteger;
    day: Sgp4ExactInteger;
    hour: Sgp4ExactInteger;
    minute: Sgp4ExactInteger;
    second: Sgp4ExactInteger;
    microsecond: Sgp4ExactInteger;
    femtosecond: Sgp4ExactInteger;
}
export interface Sgp4OmmComments {
    header: string[];
    metadata: string[];
    mean_elements: string[];
    tle_parameters: string[];
    user_defined: string[];
}
export interface Sgp4OmmSpacecraft {
    comments: string[];
    mass_kg: Sgp4ExactFloat | null;
    solar_rad_area_m2: Sgp4ExactFloat | null;
    solar_rad_coeff: Sgp4ExactFloat | null;
    drag_area_m2: Sgp4ExactFloat | null;
    drag_coeff: Sgp4ExactFloat | null;
}
export interface Sgp4OmmCovariance {
    comments: string[];
    cov_ref_frame: string | null;
    lower_triangle: Sgp4ExactFloat[];
}
export interface Sgp4OmmUserDefined { parameter: string; value: string; }
export interface Sgp4FitOmm {
    ccsds_omm_vers: string | null;
    classification: string | null;
    creation_date: string | null;
    originator: string | null;
    message_id: string | null;
    object_name: string | null;
    object_id: string | null;
    center_name: string | null;
    ref_frame: string | null;
    ref_frame_epoch: string | null;
    time_system: string | null;
    mean_element_theory: string | null;
    epoch: Sgp4OmmEpoch;
    mean_motion: Sgp4ExactFloat | null;
    semi_major_axis_km: Sgp4ExactFloat | null;
    eccentricity: Sgp4ExactFloat;
    inclination_deg: Sgp4ExactFloat;
    ra_of_asc_node_deg: Sgp4ExactFloat;
    arg_of_pericenter_deg: Sgp4ExactFloat;
    mean_anomaly_deg: Sgp4ExactFloat;
    gm_km3_s2: Sgp4ExactFloat | null;
    spacecraft: Sgp4OmmSpacecraft | null;
    ephemeris_type: Sgp4ExactInteger | null;
    classification_type: string | null;
    norad_cat_id: Sgp4ExactInteger | null;
    element_set_no: Sgp4ExactInteger | null;
    rev_at_epoch: Sgp4ExactInteger | null;
    bstar: Sgp4ExactFloat | null;
    bterm_m2_kg: Sgp4ExactFloat | null;
    mean_motion_dot: Sgp4ExactFloat | null;
    mean_motion_ddot: Sgp4ExactFloat | null;
    agom_m2_kg: Sgp4ExactFloat | null;
    covariance: Sgp4OmmCovariance | null;
    user_defined: Sgp4OmmUserDefined[];
    comments: Sgp4OmmComments;
    exact_sgp4_epoch: Sgp4ExactJulianDate | null;
    quantize_tle_derived_fields: boolean;
}
export interface Sgp4FitStatistics {
    rms_position_km: Sgp4ExactFloat;
    max_position_km: Sgp4ExactFloat;
    rms_position_axes_km: [Sgp4ExactFloat, Sgp4ExactFloat, Sgp4ExactFloat];
    rms_velocity_km_s: Sgp4ExactFloat | null;
    tle_rms_position_km: Sgp4ExactFloat;
    status: Sgp4ExactInteger;
    nfev: Sgp4ExactInteger;
    njev: Sgp4ExactInteger;
    cost: Sgp4ExactFloat;
    optimality: Sgp4ExactFloat;
    bstar_observable: boolean;
    seed_refine_passes: Sgp4ExactInteger;
}
export interface Sgp4BestEffortFit {
    elements: Sgp4FitElements;
    line1: string;
    line2: string;
    omm: Sgp4FitOmm;
    stats: Sgp4FitStatistics;
}
export interface Sgp4BatchErrorCause {
    kind: "satellitePropagation";
    satelliteIndex: number;
    message: string;
    cause: Sgp4ErrorCause;
}
export interface Sgp4BatchErrorDetail {
    family: "sgp4Batch";
    cause: Sgp4BatchErrorCause;
}
export type Sgp4OperationErrorDetail = Sgp4ErrorDetail | TleErrorDetail | DecayLatchedErrorDetail | LookAngleErrorDetail | PassErrorDetail | TleFitErrorDetail | Sgp4BatchErrorDetail;
export type Sgp4OperationError = Error & { detail: Sgp4OperationErrorDetail };

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
/** The earlier name of \`TerrainHeightBatchEntry\`. */
export type TerrainHeightBatchResult = TerrainHeightBatchEntry;
export type TerrainOrthometricBatchResult =
  | { ok: true; orthometricHeightM: { valueM: number }; error: null; detail: null }
  | { ok: false; orthometricHeightM: null; error: string; detail: TerrainLookupErrorDetail };

`;

const topLevelReplacements = [
  ["export function sourceSolveModeToa(): string;", 'export function sourceSolveModeToa(): "toa";'],
  [
    "export function sourceSolveModeTdoa(reference_sensor: number): any;",
    'export function sourceSolveModeTdoa(referenceSensor: number): { mode: "tdoa"; referenceSensor: number };',
  ],
  [
    "export function locateSource(sensors: any, arrival_times_s: any, propagation_speed_m_s: number, options: any): any;",
    "export function locateSource(sensors: SourceSensor[], arrivalTimesS: number[] | Float64Array, propagationSpeedMS: number, options?: SourceLocateOptions | null): SourceSolution;",
  ],
  [
    "export function closedFormInitialGuess(sensors: any, arrival_times_s: any, propagation_speed_m_s: number, mode: any): any;",
    "export function closedFormInitialGuess(sensors: SourceSensor[], arrivalTimesS: number[] | Float64Array, propagationSpeedMS: number, mode: SourceSolveMode): SourceInitialGuess;",
  ],
  [
    "export function chanHoInitialGuess(sensors: any, arrival_times_s: any, propagation_speed_m_s: number, mode: any): any;",
    "export function chanHoInitialGuess(sensors: SourceSensor[], arrivalTimesS: number[] | Float64Array, propagationSpeedMS: number, mode: SourceSolveMode): SourceInitialGuess;",
  ],
  [
    "export function loadSp3(bytes: Uint8Array, gap_threshold_factor?: number | null): Sp3;",
    "export function loadSp3(bytes: Uint8Array, gapThresholdFactor?: number | null): Sp3;",
  ],
  [
    "export function preciseEphemerisSamplesFromSamples(samples: Sp3PreciseEphemerisSample[], gap_threshold_factor?: number | null): PreciseEphemerisSampleSource;",
    "export function preciseEphemerisSamplesFromSamples(samples: Sp3PreciseEphemerisSample[], gapThresholdFactor?: number | null): PreciseEphemerisSampleSource;",
  ],
  [
    "export function mergeSp3(sources: Sp3[], options: any): Sp3MergeResult;",
    "export function mergeSp3(sources: Sp3[], options?: Sp3MergeOptions | null): Sp3MergeResult;",
  ],
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
    "export function solveStatic(sp3: Sp3, epochs: any, options: any): StaticSolution;",
    "export function solveStatic(sp3: Sp3, epochs: SppRequest[], options?: StaticSolveOptions | null): StaticSolution;",
  ],
  [
    "export function pppCorrections(sp3: Sp3, epochs: any, receiver_ecef_m: Float64Array, options: any): any;",
    "export function pppCorrections(sp3: Sp3, epochs: PppCorrectionEpoch[], receiver_ecef_m: Float64Array, options?: PppCorrectionOptions | null): PppCorrections;",
  ],
  [
    "export function pppCorrectionsWithCodeBias(sp3: Sp3, epochs: any, receiver_ecef_m: Float64Array, options: any, bias_set: BiasSet, code_bias: any): any;",
    "export function pppCorrectionsWithCodeBias(sp3: Sp3, epochs: PppCorrectionEpoch[], receiver_ecef_m: Float64Array, options: PppCorrectionOptions | null | undefined, bias_set: BiasSet, code_bias: CodeBiasOptions): PppCorrections;",
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
      [
        "checkContinuity(orbit_class?: string | null, residual_tolerance_m?: number | null, gap_threshold_factor?: number | null): any;",
        "checkContinuity(orbitClass?: string | null, residualToleranceM?: number | null, gapThresholdFactor?: number | null): ContinuityReport;",
      ],
      [
        "continuityVerdict(from_j2000_s: number, through_j2000_s: number, orbit_class: any, residual_tolerance_m: any, gap_threshold_factor: any): any;",
        "continuityVerdict(fromJ2000S: number, throughJ2000S: number, orbitClass?: string | null, residualToleranceM?: number | null, gapThresholdFactor?: number | null): WindowContinuityVerdict;",
      ],
      [
        "withInterpolationOptions(gap_threshold_factor: number): Sp3;",
        "withInterpolationOptions(gapThresholdFactor: number): Sp3;",
      ],
      [
        "selectedNodes(satellite: string, from_j2000_s: number, through_j2000_s: number): Float64Array;",
        "selectedNodes(satellite: string, fromJ2000S: number, throughJ2000S: number): Float64Array;",
      ],
      [
        "preciseInterpolantArtifactBytes(gap_threshold_factor?: number | null): Uint8Array;",
        "preciseInterpolantArtifactBytes(gapThresholdFactor?: number | null): Uint8Array;",
      ],
      ["stencilExtent(): any;", "stencilExtent(): { beforeS: number; afterS: number };"],
      ["fde(request: any): FdeSolution;", "fde(request: FdeRequest): FdeSolution;"],
      ["solveSpp(request: any): SppSolution;", "solveSpp(request: SppRequest): SppSolution;"],
      [
        "solveSppBatch(epochs: any, options: any): SppBatchSolution;",
        "solveSppBatch(epochs: SppRequest[], options?: SppBatchOptions | null): SppBatchSolution;",
      ],
      [
        "solveStatic(epochs: any, options: any): StaticSolution;",
        "solveStatic(epochs: SppRequest[], options?: StaticSolveOptions | null): StaticSolution;",
      ],
      [
        "sppRobustFdeDriver(request: any): FdeSolution;",
        "sppRobustFdeDriver(request: FdeRequest): FdeSolution;",
      ],
    ],
  ],
  [
    "PreciseEphemerisSampleSource",
    [
      [
        "withInterpolationOptions(gap_threshold_factor: number): PreciseEphemerisSampleSource;",
        "withInterpolationOptions(gapThresholdFactor: number): PreciseEphemerisSampleSource;",
      ],
    ],
  ],
  [
    "PreciseEphemerisInterpolant",
    [
      [
        "static fromPreciseEphemerisSamples(source: PreciseEphemerisSampleSource, gap_threshold_factor?: number | null): PreciseEphemerisInterpolant;",
        "static fromPreciseEphemerisSamples(source: PreciseEphemerisSampleSource, gapThresholdFactor?: number | null): PreciseEphemerisInterpolant;",
      ],
      [
        "static fromSamples(samples: Sp3PreciseEphemerisSample[], gap_threshold_factor?: number | null): PreciseEphemerisInterpolant;",
        "static fromSamples(samples: Sp3PreciseEphemerisSample[], gapThresholdFactor?: number | null): PreciseEphemerisInterpolant;",
      ],
      [
        "static fromSp3(sp3: Sp3, gap_threshold_factor?: number | null): PreciseEphemerisInterpolant;",
        "static fromSp3(sp3: Sp3, gapThresholdFactor?: number | null): PreciseEphemerisInterpolant;",
      ],
      [
        "withInterpolationOptions(gap_threshold_factor: number): PreciseEphemerisInterpolant;",
        "withInterpolationOptions(gapThresholdFactor: number): PreciseEphemerisInterpolant;",
      ],
    ],
  ],
  [
    "Sp3MergeReport",
    [
      [
        "continuityVerdict(from_j2000_s: number, through_j2000_s: number): any;",
        "continuityVerdict(fromJ2000S: number, throughJ2000S: number): WindowContinuityVerdict | null;",
      ],
      ["readonly continuity: any;", "readonly continuity: MergeContinuityReport | null;"],
      [
        "continuitySelectedNodes(satellite: string, from_j2000_s: number, through_j2000_s: number): Float64Array | undefined;",
        "continuitySelectedNodes(satellite: string, fromJ2000S: number, throughJ2000S: number): Float64Array | undefined;",
      ],
      ["readonly provenance: any;", "readonly provenance: MergeProvenance | null;"],
    ],
  ],
  [
    "Sp3ClockOmission",
    [
      [
        "readonly reason: string;",
        'readonly reason: "datum_not_observable" | "preferred_source_without_clock" | "no_consensus";',
      ],
    ],
  ],
  [
    "Sp3DroppedInputEpoch",
    [["readonly reason: string;", 'readonly reason: "off_target_grid" | "not_on_tick_axis";']],
  ],
  ["NominalIssue", [["readonly covers: any;", "readonly covers: NominalCoverage;"]]],
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
        "heightBatch(points: any, options: any): TerrainHeightBatchEntry[];",
        "heightBatch(points: TerrainPoint[], options?: TerrainLookupOptions | null): TerrainHeightBatchEntry[];",
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
        "static __fromBytesAttested(bytes: Uint8Array, claimed_checksum64: any): MmapTerrain;",
        "private static fromBytesAttestedInternal(bytes: Uint8Array, claimed_checksum64: any): MmapTerrain;",
      ],
      [
        "static fromPathAttested(path: string, claimed_checksum64: any): MmapTerrain;",
        "static fromPathAttested(path: string, claimed_checksum64: bigint): MmapTerrain;",
      ],
      ["readonly digestProvenance: string;", 'readonly digestProvenance: "verified" | "attested";'],
      [
        "heightBatch(points: any, options: any): TerrainHeightBatchEntry[];",
        "heightBatch(points: TerrainPoint[], options?: TerrainLookupOptions | null): TerrainHeightBatchEntry[];",
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
  [
    "PreciseInterpolantArtifact",
    [
      [
        "static __fromBytesAttested(bytes: Uint8Array, claimed_checksum64: any): PreciseInterpolantArtifact;",
        "private static fromBytesAttestedInternal(bytes: Uint8Array, claimed_checksum64: any): PreciseInterpolantArtifact;",
      ],
      [
        "static fromPathAttested(path: string, claimed_checksum64: any): PreciseInterpolantArtifact;",
        "static fromPathAttested(path: string, claimed_checksum64: bigint): PreciseInterpolantArtifact;",
      ],
      ["readonly digestProvenance: string;", 'readonly digestProvenance: "verified" | "attested";'],
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
    `${context}: expected exactly one source declaration (or one already-patched declaration), found source=${fromCount}, patched=${toCount}; source declaration=${JSON.stringify(from)}`,
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

function patchNodePathAdapters(path) {
  const text = readFileSync(path, "utf8");
  const markerCount = occurrenceCount(text, NODE_PATH_ADAPTER_MARKER);
  if (markerCount === 1) {
    return text;
  }
  if (markerCount !== 0) {
    throw new Error(
      `${path}: expected zero or one Node path adapter markers, found ${markerCount}`,
    );
  }
  for (const className of ["MmapTerrain", "PreciseInterpolantArtifact"]) {
    if (!text.includes(`class ${className} {`)) {
      throw new Error(`${path}: missing ${className} for Node path adapters`);
    }
  }
  return `${text}
${NODE_PATH_ADAPTER_MARKER}
const __sidereonMaxU64 = 0xffff_ffff_ffff_ffffn;
const __mmapTerrainFromBytesAttested = MmapTerrain.__fromBytesAttested;
const __preciseInterpolantFromBytesAttested = PreciseInterpolantArtifact.__fromBytesAttested;

function __sidereonClaimedChecksum64(value) {
    if (typeof value !== 'bigint') {
        throw new TypeError('claimedChecksum64 must be a bigint');
    }
    if (value < 0n || value > __sidereonMaxU64) {
        throw new RangeError('claimedChecksum64 must be between 0n and 18446744073709551615n');
    }
    return value;
}

function __sidereonReadArtifactPath(path) {
    try {
        return require('fs').readFileSync(path);
    } catch (cause) {
        const reason = cause instanceof Error ? cause.message : String(cause);
        const message = \`\${path} failed: \${reason}\`;
        const error = new Error(message);
        error.name = 'Io';
        error.kind = 'Io';
        error.detail = { name: 'Io', message, path: String(path) };
        throw error;
    }
}

MmapTerrain.fromPath = function(path) {
    return MmapTerrain.fromBytes(__sidereonReadArtifactPath(path));
};
MmapTerrain.fromPathAttested = function(path, claimedChecksum64) {
    const claim = __sidereonClaimedChecksum64(claimedChecksum64);
    return __mmapTerrainFromBytesAttested(__sidereonReadArtifactPath(path), claim);
};
PreciseInterpolantArtifact.fromPath = function(path) {
    return openPreciseInterpolantArtifact(__sidereonReadArtifactPath(path));
};
PreciseInterpolantArtifact.fromPathAttested = function(path, claimedChecksum64) {
    const claim = __sidereonClaimedChecksum64(claimedChecksum64);
    return __preciseInterpolantFromBytesAttested(__sidereonReadArtifactPath(path), claim);
};
delete MmapTerrain.__fromBytesAttested;
delete PreciseInterpolantArtifact.__fromBytesAttested;
`;
}

const declarationPaths = ["pkg/sidereon.d.ts", "pkg-node/sidereon.d.ts"];
const patchedDeclarations = declarationPaths.map((path) => [path, patchDeclarations(path)]);
const patchedNodeJs = patchNodePathAdapters("pkg-node/sidereon.js");

writeFileSync("pkg-node/package.json", JSON.stringify({ type: "commonjs" }, null, 2) + "\n");
for (const [path, text] of patchedDeclarations) {
  writeFileSync(path, text);
}
writeFileSync("pkg-node/sidereon.js", patchedNodeJs);
