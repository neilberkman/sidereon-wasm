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

test("predicted IONEX direct locations preserve tier and identity year", () => {
  const p1 = distributionLocation("cod_prd1", "ionex", 2026, 7, 15, undefined, undefined, "direct");
  assert.equal(
    p1.originalUrl,
    "https://www.aiub.unibe.ch/download/CODE/IONO/P1/2026/COD0OPSPRD_20261960000_01D_01H_GIM.INX.gz",
  );

  const p2 = distributionLocation("cod_prd2", "ionex", 2026, 7, 16, undefined, undefined, "direct");
  assert.equal(
    p2.originalUrl,
    "https://www.aiub.unibe.ch/download/CODE/IONO/P2/2026/COD0OPSPRD_20261970000_01D_01H_GIM.INX.gz",
  );

  const boundary = distributionLocation(
    "cod_prd2",
    "ionex",
    2027,
    1,
    1,
    undefined,
    undefined,
    "direct",
  );
  assert.equal(
    boundary.originalUrl,
    "https://www.aiub.unibe.ch/download/CODE/IONO/P2/2027/COD0OPSPRD_20270010000_01D_01H_GIM.INX.gz",
  );
});

test("predicted tiers with the same filename retain distinct cache identities", () => {
  const p1 = productIdentity("cod_prd1", "ionex", 2026, 7, 16);
  const p2 = productIdentity("cod_prd2", "ionex", 2026, 7, 16);
  assert.equal(p1.officialFilename, p2.officialFilename);
  assert.notEqual(p1.cacheKey, p2.cacheKey);
});

test("unsupported CDDIS families fail instead of changing product", () => {
  assert.throws(
    () => distributionLocation("igs", "nav", 2020, 6, 25, undefined, undefined, "nasa_cddis"),
    /does not serve nav/,
  );
});
