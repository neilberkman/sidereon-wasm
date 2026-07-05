import { test } from "node:test";
import assert from "node:assert/strict";

import {
  Geofence,
  GeofenceCrossingKind,
  GeofenceError,
  GeofenceProbabilityMethod,
  geofenceContainmentProbability,
  geofenceCrossingKindLabel,
  geofenceErrorLabel,
  geofenceFromVertices3d,
  geofenceProbabilityMethodLabel,
} from "../pkg-node/sidereon.js";
import { f64Bits } from "./helpers.mjs";

const deg = (value) => (value * Math.PI) / 180;

const vertices = Float64Array.from([
  deg(-0.01),
  0,
  deg(-0.01),
  deg(0.02),
  deg(0.01),
  deg(0.02),
  deg(0.01),
  0,
]);

const uncertainty = {
  kind: "enuCovarianceM2",
  covarianceM2: [400, 0, 0, 0, 400, 0, 0, 0, 0],
};

test("geofence containment probability and crossing parity", () => {
  const fence = new Geofence(vertices);
  assert.equal(fence.edgeCount, 4);
  assert.equal(
    geofenceProbabilityMethodLabel(GeofenceProbabilityMethod.BoundaryNormal),
    "boundaryNormal",
  );
  assert.equal(geofenceCrossingKindLabel(GeofenceCrossingKind.Entered), "entered");
  assert.equal(geofenceErrorLabel(GeofenceError.TooFewVertices), "TooFewVertices");

  const center = [0, deg(0.01), 0];
  const nearNorthEdge = [deg(0.0098), deg(0.01), 0];
  const outsideNorth = [deg(0.012), deg(0.01), 0];

  assert.equal(fence.contains(...center), true);
  assert.equal(fence.contains(...outsideNorth), false);
  assert.equal(f64Bits(fence.distanceToBoundary(...center)), 0x409146f89a157d9an);
  assert.equal(f64Bits(fence.distanceToBoundary(...nearNorthEdge)), 0x40361d684277dc4bn);
  assert.equal(f64Bits(fence.distanceToBoundary(...outsideNorth)), 0xc06ba4c0cbfd9d52n);
  assert.equal(
    f64Bits(fence.containmentProbability(...nearNorthEdge, uncertainty, undefined)),
    0x3febb2d770633baan,
  );
  assert.equal(
    f64Bits(geofenceContainmentProbability(vertices, ...nearNorthEdge, uncertainty)),
    0x3febb2d770633baan,
  );

  const events = fence.crossingProbability(
    [
      { latRad: deg(0.012), lonRad: deg(0.01), uncertainty },
      { latRad: deg(0.009), lonRad: deg(0.01), uncertainty },
      { latRad: 0, lonRad: deg(0.01), uncertainty },
      { latRad: deg(0.012), lonRad: deg(0.01), uncertainty },
    ],
    { enterConfidence: 0.7, leaveConfidence: 0.6 },
  );
  assert.deepEqual(
    events.map((event) => event.kind),
    ["entered", "left"],
  );
  assert.deepEqual(
    events.map((event) => event.sampleIndex),
    [1, 3],
  );
  assert.deepEqual(
    events.map((event) => f64Bits(event.insideProbability)),
    [0x3feffffff7573576n, 0x0000000000000000n],
  );
});

test("geofence construction errors carry typed details", () => {
  assert.throws(
    () => new Geofence(Float64Array.from([0, 0, 0, 1])),
    (error) =>
      error instanceof Error &&
      error.kind === "TooFewVertices" &&
      error.detail.name === "TooFewVertices",
  );
});

test("geofence construction treats six-value flat input as a 2D triangle", () => {
  const triangle = new Geofence(Float64Array.from([0, 0, 0, deg(0.02), deg(0.02), 0]));
  assert.equal(triangle.edgeCount, 3);
  assert.equal(triangle.contains(deg(0.005), deg(0.005), 0), true);

  const withHeights = geofenceFromVertices3d(
    Float64Array.from([0, 0, 10, 0, deg(0.02), 20, deg(0.02), 0, 30]),
  );
  assert.equal(withHeights.edgeCount, 3);
  assert.equal(withHeights.contains(deg(0.005), deg(0.005), 0), true);
});
