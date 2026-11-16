import { test } from "node:test";
import assert from "node:assert/strict";
import { araimSatelliteIsmModelAddV30 } from "../pkg-node/sidereon.js";

test("ADD v3.0 model uses the Appendix A local variance and rejects unsupported systems", () => {
  const model = araimSatelliteIsmModelAddV30("GPS", Math.PI / 2, 0.75, 0.5, 0.5, 1e-5);
  assert.ok(Math.abs(model.effectiveSigmaIntM ** 2 - 0.75 ** 2 - 0.27847436379754925) < 2e-15);
  assert.throws(
    () => araimSatelliteIsmModelAddV30("BeiDou", Math.PI / 6, 0.75, 0.5, 0.5, 1e-5),
    (error) => error.detail.kind === "unsupportedConstellation" && error.detail.system === "BeiDou",
  );
});
