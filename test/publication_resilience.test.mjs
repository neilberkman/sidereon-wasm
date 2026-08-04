// Publication-lag resilience surface (core 0.36.0) through the WASM binding:
// the cross-line predicted-IONEX walk, the closed-dialect listing parsers,
// and newest-published-issue selection, checked against the archive listings
// recorded live during the 2026-08-04 publication lag (the same fixtures the
// core pins).

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
  assert.equal(candidates[0].filename, candidates[1].filename);
  assert.match(candidates[0].url, /\/IONO\/P1\/2026\//);
  assert.match(candidates[1].url, /\/IONO\/P2\/2026\//);
});

test("the recorded P1 gap resolves to P2 with the line named", () => {
  const body = listing("aiub-iono-p1p2-20260804.csv");
  assert.equal(resolveFirstPublishedPredictedIonex(2026, 8, 5, undefined, body), 1);
  assert.equal(resolveFirstPublishedPredictedIonex(2026, 8, 4, undefined, body), 0);
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
