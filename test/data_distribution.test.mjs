import assert from "node:assert/strict";
import test from "node:test";

import {
  buildExactCacheCommit,
  distributionLocation,
  GnssExactProductSet,
  productIdentity,
  verifyExactCacheCommit,
} from "../pkg-node/sidereon.js";

test("exact product identity remains independent of distributor", () => {
  const identity = productIdentity("cod", "sp3", 2026, 7, 12);
  assert.equal(identity.family, "sp3");
  assert.equal(identity.analysisCenter, "cod");
  assert.equal(identity.publisher, "COD");
  assert.equal(identity.solutionClass, "final");
  assert.equal(identity.campaign, "MGX");
  assert.equal(identity.sample, "05M");
  assert.equal(identity.officialFilename, "COD0MGXFIN_20261930000_01D_05M_ORB.SP3");
  assert.equal(identity.format, "SP3");
  assert.equal(identity.formatVersion, undefined);
  assert.equal(identity.cacheKey, "cod-final-a91258c21fa4860c34ce");

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

test("exact cache commits bind full identity, source, and all immutable bytes", () => {
  const identity = productIdentity("cod_prd1", "ionex", 2026, 7, 16);
  const entry = "0123456789abcdef0123456789abcdef";
  const product = new TextEncoder().encode("validated IONEX");
  const archive = new TextEncoder().encode("compressed distributor bytes");
  const provenance = new TextEncoder().encode('{"source":"direct"}');
  const marker = buildExactCacheCommit(identity, "direct", entry, product, archive, provenance);

  assert.equal(
    verifyExactCacheCommit(identity, "direct", marker, product, archive, provenance),
    entry,
  );
  assert.throws(
    () =>
      verifyExactCacheCommit(
        identity,
        "direct",
        marker,
        new TextEncoder().encode("different product"),
        archive,
        provenance,
      ),
    /identity, source, or bytes/,
  );
  assert.throws(
    () => verifyExactCacheCommit(identity, "nasa_cddis", marker, product, archive, provenance),
    /identity, source, or bytes/,
  );

  const otherTier = productIdentity("cod_prd2", "ionex", 2026, 7, 16);
  assert.equal(identity.officialFilename, otherTier.officialFilename);
  assert.throws(
    () => verifyExactCacheCommit(otherTier, "direct", marker, product, archive, provenance),
    /identity, source, or bytes/,
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

test("exact product sets fail closed and retain prediction metadata", () => {
  const first = productIdentity("cod", "sp3", 2026, 7, 12);
  const second = productIdentity("cod", "sp3", 2026, 7, 13);
  const complete = new GnssExactProductSet();
  complete.addExpected(first);
  complete.addExpected(second);
  complete.addAvailable(second);
  complete.addAvailable(first);
  assert.equal(complete.expectedCount, 2);
  assert.equal(complete.availableCount, 2);
  complete.validate();

  const partial = new GnssExactProductSet();
  partial.addExpected(first);
  partial.addExpected(second);
  partial.addAvailable(first);
  assert.throws(() => partial.validate(), /missing:/);

  const duplicated = new GnssExactProductSet();
  duplicated.addExpected(first);
  duplicated.addExpected(first);
  duplicated.addAvailable(first);
  duplicated.addAvailable(second);
  duplicated.addAvailable(second);
  assert.throws(() => duplicated.validate(), /duplicate expected:/);

  const oneDay = productIdentity("cod_prd1", "ionex", 2026, 7, 16);
  const twoDay = productIdentity("cod_prd2", "ionex", 2026, 7, 16);
  assert.equal(oneDay.officialFilename, twoDay.officialFilename);
  const wrongTier = new GnssExactProductSet();
  wrongTier.addExpected(oneDay);
  wrongTier.addAvailable(twoDay);
  assert.throws(() => wrongTier.validate(), /unexpected:/);
});

test("unsupported CDDIS families fail instead of changing product", () => {
  assert.throws(
    () => distributionLocation("igs", "nav", 2020, 6, 25, undefined, undefined, "nasa_cddis"),
    /does not serve nav/,
  );
});
