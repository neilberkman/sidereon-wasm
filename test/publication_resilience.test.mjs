// Publication-lag resilience surface (core 0.36.0) through the WASM binding:
// the cross-line predicted-IONEX walk, the closed-dialect listing parsers,
// and newest-published-issue selection, checked against recorded archive
// listings (the same fixtures the core pins): the GFZ ultra-rapid listing
// from the 2026-08-04 publication lag and the AIUB whole-tree listing of the
// CODE/IONO/PRD predicted maps recorded on 2026-09-23.

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  newestPublishedProduct,
  parseArchiveListing,
  predictedIonexLineCandidates,
  productSolutionClass,
  publicationListingUrls,
  publishedIssueAgeMinutes,
  resolveFirstPublishedPredictedIonex,
} from "../pkg-node/sidereon.js";
import { fixtureText } from "./helpers.mjs";

const listing = (name) => fixtureText(`listings/${name}`);

test("cross-line candidates share the map date and name their line", () => {
  const candidates = predictedIonexLineCandidates(2026, 8, 5, undefined);
  assert.equal(candidates.length, 2);
  assert.equal(candidates[0].center, "cod_prd1");
  assert.equal(candidates[1].center, "cod_prd2");
  for (const candidate of candidates) {
    assert.equal(candidate.date, "2026-08-05");
  }
  // Same map date and archive directory, one filename token per line.
  assert.equal(candidates[0].filename, "COD0OPSP0D_20262170000_01D_01H_GIM.INX");
  assert.equal(candidates[1].filename, "COD0OPSP1D_20262170000_01D_01H_GIM.INX");
  assert.equal(
    candidates[0].url,
    "https://www.aiub.unibe.ch/download/CODE/IONO/PRD/COD0OPSP0D_20262170000_01D_01H_GIM.INX.gz",
  );
  assert.equal(
    candidates[1].url,
    "https://www.aiub.unibe.ch/download/CODE/IONO/PRD/COD0OPSP1D_20262170000_01D_01H_GIM.INX.gz",
  );
});

test("the recorded one-day gap resolves to the two-day line", () => {
  // In the state recorded on 2026-09-23 the one-day map for day 266 is not
  // yet published while the two-day map is; for day 265 both are, and the
  // walk keeps its one-day preference.
  const body = listing("aiub-iono-prd-20260923.csv");
  assert.equal(resolveFirstPublishedPredictedIonex(2026, 9, 23, undefined, body), 1);
  assert.equal(resolveFirstPublishedPredictedIonex(2026, 9, 22, undefined, body), 0);
});

test("the AIUB whole-tree listing separates the predicted lines", () => {
  const body = listing("aiub-iono-prd-20260923.csv");
  assert.deepEqual(newestPublishedProduct("cod_prd1", "ionex", body), {
    date: "2026-09-22",
    issue: "0000",
    filename: "COD0OPSP0D_20262650000_01D_01H_GIM.INX",
    observedAt: "2026-09-22T10:00:02Z",
  });
  assert.deepEqual(newestPublishedProduct("cod_prd2", "ionex", body), {
    date: "2026-09-23",
    issue: "0000",
    filename: "COD0OPSP1D_20262660000_01D_01H_GIM.INX",
    observedAt: "2026-09-22T10:00:02Z",
  });

  // The rolling copies CODE keeps at the tree root are not the archived
  // objects and are attributed to neither line.
  const rootCopies = body
    .split("\n")
    .filter((row) => row.length > 0 && !row.startsWith("CODE/IONO/"))
    .join("\n");
  assert.ok(
    rootCopies
      .split("\n")
      .some((row) => row.startsWith("CODE/COD0OPSP1D_20262660000_01D_01H_GIM.INX.gz;")),
  );
  for (const center of ["cod_prd1", "cod_prd2"]) {
    assert.equal(newestPublishedProduct(center, "ionex", rootCopies), null);
  }
});

test("newest published product reports the recorded GFZ lag", () => {
  const newest = newestPublishedProduct("gfz_ult", "sp3", listing("gfz-ultra-w2430-20260804.html"));
  assert.deepEqual(newest, {
    date: "2026-08-03",
    issue: "0300",
    filename: "GFZ0OPSULT_20262150300_02D_05M_ORB.SP3",
    observedAt: "2026-08-04 08:20",
  });
  assert.equal(
    publishedIssueAgeMinutes(2026, 8, 3, "0300", newest.filename, 2026, 8, 4, 7, 8, 0),
    BigInt(28 * 60 + 8),
  );
});

test("an unrecognizable listing body throws, never an empty parse", () => {
  for (const body of ["", "This mirror has moved.", "<html><h1>503</h1></html>"]) {
    assert.throws(() => parseArchiveListing(body));
  }
});

test("publication listing URLs are bounded", () => {
  assert.deepEqual(publicationListingUrls("gfz_ult", "sp3", 2026, 8, 4), [
    "https://isdc-data.gfz.de/gnss/products/ultra/w2430/",
    "https://isdc-data.gfz.de/gnss/products/ultra/w2429/",
  ]);
  assert.deepEqual(publicationListingUrls("cod_prd1", "ionex", 2026, 8, 4), [
    "https://www.aiub.unibe.ch/download/full_listing.csv",
  ]);
});

test("the WUM near-real-time line is cataloged", () => {
  assert.equal(productSolutionClass("wum_nrt", "sp3"), "near_real_time");
});
