import { test } from "node:test";
import { loadSp3, mergeSp3 } from "../pkg-node/sidereon.js";
import { checkSp3Consumer } from "../scripts/check-sp3-consumer.mjs";

test("published consumer SP3 clock merge/write regression", () => {
  checkSp3Consumer({ loadSp3, mergeSp3 });
});
