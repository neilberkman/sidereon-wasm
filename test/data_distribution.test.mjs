import assert from "node:assert/strict";
import test from "node:test";

import { distributionLocation, productIdentity } from "../pkg-node/sidereon.js";

test("exact product identity remains independent of distributor", () => {
  const identity = productIdentity("cod", "sp3", 2026, 7, 12);
  assert.equal(identity.family, "sp3");
  assert.equal(identity.publisher, "COD");
  assert.equal(identity.solutionClass, "final");
  assert.equal(identity.campaign, "MGX");
  assert.equal(identity.sample, "05M");
  assert.equal(identity.officialFilename, "COD0MGXFIN_20261930000_01D_05M_ORB.SP3");

  const direct = distributionLocation("cod", "sp3", 2026, 7, 12, undefined, undefined, "direct");
  const cddis = distributionLocation("cod", "sp3", 2026, 7, 12, undefined, undefined, "nasa_cddis");
  assert.equal(direct.archiveFilename, "COD0MGXFIN_20261930000_01D_05M_ORB.SP3.gz");
  assert.equal(cddis.archiveFilename, "COD0MGXFIN_20261930000_01D_05M_ORB.SP3.gz");
  assert.equal(cddis.compression, "gzip");
  assert.equal(
    cddis.originalUrl,
    "https://cddis.nasa.gov/archive/gnss/products/2427/COD0MGXFIN_20261930000_01D_05M_ORB.SP3.gz",
  );
});

test("IONEX uses the public CDDIS year/day-of-year layout", () => {
  const location = distributionLocation(
    "esa",
    "ionex",
    2020,
    6,
    24,
    undefined,
    undefined,
    "nasa_cddis",
  );
  assert.equal(
    location.originalUrl,
    "https://cddis.nasa.gov/archive/gnss/products/ionex/2020/176/ESA0OPSFIN_20201760000_01D_02H_GIM.INX.gz",
  );
});

test("unsupported CDDIS families fail instead of changing product", () => {
  assert.throws(
    () => distributionLocation("igs", "nav", 2020, 6, 25, undefined, undefined, "nasa_cddis"),
    /does not serve nav/,
  );
});
