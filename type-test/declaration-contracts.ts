import * as NodeBindings from "../pkg-node/sidereon.js";
import * as WebBindings from "../pkg/sidereon.js";

type Equal<Left, Right> =
  (<Value>() => Value extends Left ? 1 : 2) extends <Value>() => Value extends Right ? 1 : 2
    ? true
    : false;
type Assert<Condition extends true> = Condition;
type IsAny<Value> = 0 extends 1 & Value ? true : false;

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
