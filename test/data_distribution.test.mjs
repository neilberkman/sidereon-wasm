import assert from "node:assert/strict";
import test from "node:test";

import {
  buildExactCacheCommit,
  defaultSampleForDate,
  distributionLocation,
  GnssExactProductSet,
  productIdentity,
  productSolutionClass,
  Sp3ContentStartConvention,
  sp3ContentStartConvention,
  sp3ContentStartOffsetSeconds,
  supportedSamples,
  verifyExactCacheCommit,
} from "../pkg-node/sidereon.js";

test("product-aware classification distinguishes IGS final orbit and broadcast navigation", () => {
  assert.equal(productSolutionClass("igs", "sp3"), "final");
  assert.equal(productSolutionClass("igs", "nav"), "broadcast");
  assert.equal(defaultSampleForDate("gfz", "sp3", 2021, 5, 17), "15M");
  assert.equal(defaultSampleForDate("gfz", "sp3", 2021, 5, 18), "05M");
});

test("final and ultra-rapid SP3 catalogs preserve their evidenced start dates", () => {
  assert.throws(() => productIdentity("esa", "sp3", 2014, 1, 4));
  assert.equal(
    productIdentity("esa", "sp3", 2014, 1, 5).officialFilename,
    "ESA0MGNFIN_20140050000_01D_05M_ORB.SP3",
  );

  assert.throws(() => productIdentity("gfz", "sp3", 2020, 5, 12));
  assert.equal(productIdentity("gfz", "sp3", 2020, 5, 13).sample, "15M");

  assert.throws(() => productIdentity("igs_ult", "sp3", 2022, 11, 26, undefined, "0600"));
  assert.equal(productIdentity("igs_ult", "sp3", 2022, 11, 27, undefined, "0600").sample, "15M");

  assert.throws(() => productIdentity("cod_ult", "sp3", 2022, 11, 26, undefined, "0000"));
  assert.equal(productIdentity("cod_ult", "sp3", 2022, 11, 27, undefined, "0000").sample, "05M");

  assert.throws(() => productIdentity("esa_ult", "sp3", 2022, 10, 3, undefined, "0600"));
  assert.equal(productIdentity("esa_ult", "sp3", 2022, 10, 4, undefined, "0600").sample, "15M");

  assert.throws(() => productIdentity("gfz_ult", "sp3", 2020, 10, 5, undefined, "0600"));
  assert.equal(productIdentity("gfz_ult", "sp3", 2020, 10, 6, undefined, "0600").sample, "15M");

  assert.throws(() => productIdentity("esa", "clk", 2014, 1, 4));
  assert.equal(productIdentity("esa", "clk", 2014, 1, 5).sample, "30S");
  assert.throws(() => productIdentity("gfz", "clk", 2020, 5, 12));
  assert.equal(productIdentity("gfz", "clk", 2020, 5, 13).sample, "30S");
});

test("ultra-rapid SP3 defaults follow the evidenced cadence boundaries", () => {
  assert.equal(defaultSampleForDate("esa_ult", "sp3", 2024, 9, 3), "15M");
  assert.equal(defaultSampleForDate("esa_ult", "sp3", 2025, 2, 2), "15M");
  assert.equal(defaultSampleForDate("esa_ult", "sp3", 2025, 2, 3), "05M");
  assert.equal(productIdentity("esa_ult", "sp3", 2024, 9, 3, undefined, "0600").sample, "15M");
  assert.equal(productIdentity("esa_ult", "sp3", 2025, 2, 2, undefined, "0600").sample, "15M");
  assert.equal(productIdentity("esa_ult", "sp3", 2025, 2, 2, undefined, "1200").sample, "05M");

  assert.equal(defaultSampleForDate("gfz_ult", "sp3", 2021, 5, 15), "15M");
  assert.equal(defaultSampleForDate("gfz_ult", "sp3", 2021, 5, 16), "05M");
  assert.equal(productIdentity("gfz_ult", "sp3", 2021, 5, 15, undefined, "0600").sample, "15M");
  assert.equal(productIdentity("gfz_ult", "sp3", 2021, 5, 16, undefined, "0600").sample, "05M");
});

test("supported samples preserve every date and issue boundary", () => {
  assert.deepEqual(supportedSamples("esa", "sp3", 2026, 6, 15), ["05M"]);
  assert.deepEqual(supportedSamples("gfz", "sp3", 2021, 5, 17), ["15M"]);
  assert.deepEqual(supportedSamples("gfz", "sp3", 2021, 5, 18), ["05M"]);
  assert.deepEqual(supportedSamples("esa_ult", "sp3", 2025, 2, 2, "0600"), ["15M"]);
  assert.deepEqual(supportedSamples("esa_ult", "sp3", 2025, 2, 2, "1200"), ["05M"]);
  assert.deepEqual(supportedSamples("gfz_ult", "sp3", 2021, 5, 15, "0000"), ["15M", "05M"]);
  assert.deepEqual(supportedSamples("gfz_ult", "sp3", 2021, 5, 15, "2100"), ["15M"]);
  assert.deepEqual(supportedSamples("cod", "clk", 2026, 6, 15), ["30S"]);
  assert.deepEqual(supportedSamples("cod", "ionex", 2026, 6, 15), ["01H"]);
  assert.deepEqual(supportedSamples("igs", "nav", 2026, 6, 15), ["01D"]);
  assert.throws(() => supportedSamples("gfz_ult", "sp3", 2021, 5, 15, "0130"));

  assert.throws(() => productIdentity("esa", "sp3", 2026, 6, 15, "15M"));
  assert.throws(() => productIdentity("gfz", "sp3", 2021, 5, 17, "05M"));
  assert.throws(() => productIdentity("esa_ult", "sp3", 2025, 2, 2, "05M", "0600"));
  assert.throws(() => productIdentity("gfz_ult", "sp3", 2021, 5, 15, "05M", "2100"));
  assert.equal(productIdentity("gfz_ult", "sp3", 2021, 5, 15, "05M", "0000").sample, "05M");
});

test("SP3 content-start conventions preserve the complete GFZ transition catalog", () => {
  const issues = ["0000", "0300", "0600", "0900", "1200", "1500", "1800", "2100"];
  const minusOneDay = Sp3ContentStartConvention.FilenameEpochMinusOneDay;
  const filenameEpoch = Sp3ContentStartConvention.FilenameEpoch;
  const expected = new Map([
    [
      "2022-09-07",
      [
        filenameEpoch,
        minusOneDay,
        minusOneDay,
        minusOneDay,
        minusOneDay,
        minusOneDay,
        minusOneDay,
        minusOneDay,
      ],
    ],
    [
      "2022-09-08",
      [
        filenameEpoch,
        minusOneDay,
        minusOneDay,
        filenameEpoch,
        filenameEpoch,
        filenameEpoch,
        filenameEpoch,
        filenameEpoch,
      ],
    ],
  ]);

  assert.equal(sp3ContentStartConvention("gfz_ult", 2022, 9, 6, "2100"), minusOneDay);
  assert.equal(sp3ContentStartOffsetSeconds(minusOneDay), -86400n);
  assert.equal(sp3ContentStartConvention("gfz_ult", 2022, 9, 9, "0000"), filenameEpoch);
  assert.equal(sp3ContentStartOffsetSeconds(filenameEpoch), 0n);
  assert.equal(sp3ContentStartConvention("igs", 2022, 9, 7), filenameEpoch);

  for (const [date, conventions] of expected) {
    const day = Number(date.slice(-2));
    for (let index = 0; index < issues.length; index += 1) {
      const convention = sp3ContentStartConvention("gfz_ult", 2022, 9, day, issues[index]);
      assert.equal(convention, conventions[index], `${date} ${issues[index]}`);
      assert.equal(
        sp3ContentStartOffsetSeconds(convention),
        convention === filenameEpoch ? 0n : -86400n,
        `${date} ${issues[index]} offset`,
      );
    }
  }

  assert.throws(() => sp3ContentStartConvention("gfz_ult", 2022, 9, 7, "0130"));
  assert.throws(() => sp3ContentStartConvention("gfz_ult", 2022, 9, 7));
  assert.throws(() => sp3ContentStartConvention("gfz", 2022, 9, 7, "0000"));
});

test("IGS final identity and CDDIS packaging follow the official naming eras", () => {
  const legacy = productIdentity("igs", "sp3", 2022, 11, 26);
  assert.equal(legacy.solutionClass, "final");
  assert.equal(legacy.officialFilename, "igs22376.sp3");
  const legacyCddis = distributionLocation(
    "igs",
    "sp3",
    2022,
    11,
    26,
    undefined,
    undefined,
    "nasa_cddis",
  );
  assert.equal(legacyCddis.compression, "unix_compress");
  assert.equal(legacyCddis.archiveFilename, "igs22376.sp3.Z");
  assert.equal(
    legacyCddis.originalUrl,
    "https://cddis.nasa.gov/archive/gnss/products/2237/igs22376.sp3.Z",
  );

  const current = productIdentity("igs", "sp3", 2022, 11, 27);
  assert.equal(current.officialFilename, "IGS0OPSFIN_20223310000_01D_15M_ORB.SP3");
  const navigation = productIdentity("igs", "nav", 2022, 11, 26);
  assert.equal(navigation.solutionClass, "broadcast");

  assert.throws(() => productIdentity("igs", "sp3", 1994, 1, 1), /no cataloged naming convention/);
  assert.throws(() => productIdentity("cod_prd1", "sp3", 2026, 7, 12), /does not serve sp3/);
});

test("pre-week-2238 CDDIS rejects unmodeled long-name SP3 products", () => {
  // Direct archives retain their independently evidenced historical products.
  assert.equal(productIdentity("esa", "sp3", 2020, 6, 24).sample, "05M");
  assert.equal(productIdentity("gfz", "sp3", 2020, 6, 24).sample, "15M");

  assert.throws(() =>
    distributionLocation("esa", "sp3", 2020, 6, 24, undefined, undefined, "nasa_cddis"),
  );
  assert.throws(() =>
    distributionLocation("gfz", "sp3", 2020, 6, 24, undefined, undefined, "nasa_cddis"),
  );

  for (const [center, year, month, day, issue] of [
    ["esa_ult", 2022, 10, 4, "0600"],
    ["gfz_ult", 2020, 10, 6, "0600"],
  ]) {
    assert.ok(distributionLocation(center, "sp3", year, month, day, undefined, issue, "direct"));
    assert.throws(() =>
      distributionLocation(center, "sp3", year, month, day, undefined, issue, "nasa_cddis"),
    );
  }

  // CODE ultra is itself unmodeled before the long-name transition.
  assert.throws(() =>
    distributionLocation("cod_ult", "sp3", 2022, 11, 26, undefined, "0000", "direct"),
  );

  // The separately modeled legacy IGS final remains available as .sp3.Z.
  assert.equal(
    distributionLocation("igs", "sp3", 2020, 6, 24, undefined, undefined, "nasa_cddis").compression,
    "unix_compress",
  );
});

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

test("CODE direct routing remains product-specific across current families", () => {
  const sp3 = distributionLocation("cod", "sp3", 2026, 4, 30, undefined, undefined, "direct");
  assert.equal(
    sp3.originalUrl,
    "https://www.aiub.unibe.ch/download/CODE_MGEX/CODE/2026/COD0MGXFIN_20261200000_01D_05M_ORB.SP3.gz",
  );

  const clock = distributionLocation("cod", "clk", 2026, 4, 30, undefined, undefined, "direct");
  assert.equal(
    clock.originalUrl,
    "https://www.aiub.unibe.ch/download/CODE_MGEX/CODE/2026/COD0MGXFIN_20261200000_01D_30S_CLK.CLK.gz",
  );

  const finalIonex = distributionLocation(
    "cod",
    "ionex",
    2026,
    4,
    30,
    undefined,
    undefined,
    "direct",
  );
  assert.equal(
    finalIonex.originalUrl,
    "https://www.aiub.unibe.ch/download/CODE/2026/COD0OPSFIN_20261200000_01D_01H_GIM.INX.gz",
  );

  const rapidIonex = distributionLocation(
    "cod_rap",
    "ionex",
    2026,
    4,
    30,
    undefined,
    undefined,
    "direct",
  );
  assert.equal(
    rapidIonex.originalUrl,
    "https://www.aiub.unibe.ch/download/CODE/COD0OPSRAP_20261200000_01D_01H_GIM.INX.gz",
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

test("IONEX uses the public current CDDIS layout without inventing a historical long name", () => {
  assert.throws(() =>
    distributionLocation("esa", "ionex", 2022, 11, 26, undefined, undefined, "nasa_cddis"),
  );
  const location = distributionLocation(
    "esa",
    "ionex",
    2024,
    6,
    24,
    undefined,
    undefined,
    "nasa_cddis",
  );
  assert.equal(
    location.originalUrl,
    "https://cddis.nasa.gov/archive/gnss/products/ionex/2024/176/ESA0OPSFIN_20241760000_01D_02H_GIM.INX.gz",
  );
});

test("CDDIS does not substitute for ESA MGEX final SP3", () => {
  assert.ok(distributionLocation("esa", "sp3", 2024, 6, 24, undefined, undefined, "direct"));
  assert.throws(() =>
    distributionLocation("esa", "sp3", 2024, 6, 24, undefined, undefined, "nasa_cddis"),
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
