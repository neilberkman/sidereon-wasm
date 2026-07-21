import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const packageRoot = dirname(dirname(fileURLToPath(import.meta.url)));
const packageJson = JSON.parse(readFileSync(join(packageRoot, "package.json"), "utf8"));
const scratch = mkdtempSync(join(tmpdir(), "sidereon-wasm-package-"));
const packDirectory = join(scratch, "pack");
const consumerDirectory = join(scratch, "consumer");
mkdirSync(packDirectory);
mkdirSync(consumerDirectory);

function run(command, args, cwd = packageRoot) {
  execFileSync(command, args, {
    cwd,
    encoding: "utf8",
    stdio: ["ignore", "inherit", "inherit"],
  });
}

function writeConsumer(path, contents) {
  writeFileSync(join(consumerDirectory, path), contents);
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

try {
  const packOutput = execFileSync(
    "npm",
    ["pack", "--json", "--ignore-scripts", "--pack-destination", packDirectory],
    {
      cwd: packageRoot,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "inherit"],
    },
  );
  const [packed] = JSON.parse(packOutput);

  if (packed.name !== packageJson.name || packed.version !== packageJson.version) {
    throw new Error(
      `packed identity ${packed.name}@${packed.version} does not match ${packageJson.name}@${packageJson.version}`,
    );
  }

  const packedPaths = new Set(packed.files.map(({ path }) => path));
  const requiredPackedPaths = [
    "package.json",
    "LICENSE",
    "README.md",
    "THIRD-PARTY-NOTICES.md",
    "licenses/Apache-2.0.txt",
    "licenses/ERFA-BSD-3-Clause.txt",
    "licenses/IERS-Conventions-Software-License.txt",
    "licenses/SciPy-BSD-3-Clause.txt",
    "licenses/libloading-ISC.txt",
    "third_party_source/sidereon-core-0.33.1/tides/mod.rs",
    "third_party_source/sidereon-core-0.33.1/tides/ocean.rs",
    "third_party_source/sidereon-core-0.33.1/tides/pole.rs",
    "pkg/sidereon.js",
    "pkg/sidereon.d.ts",
    "pkg/sidereon_bg.wasm",
    "pkg/sidereon_bg.wasm.d.ts",
    "pkg-node/package.json",
    "pkg-node/sidereon.js",
    "pkg-node/sidereon.d.ts",
    "pkg-node/sidereon_bg.wasm",
    "pkg-node/sidereon_bg.wasm.d.ts",
    "exact-cache.js",
    "types/exact-cache.d.ts",
    "types/sidereon-extra.js",
    "types/sidereon-extra.cjs",
    "types/sidereon-extra.d.ts",
  ];
  for (const path of requiredPackedPaths) {
    if (!packedPaths.has(path)) {
      throw new Error(`npm package is missing required file ${path}`);
    }
  }

  const coreSourceDigests = {
    "third_party_source/sidereon-core-0.33.1/tides/mod.rs":
      "7c71cb8facbd81af8473d3634e4c63d97dda8cb37a2f59888d3397cfdde4d39b",
    "third_party_source/sidereon-core-0.33.1/tides/ocean.rs":
      "6bd72d6647b634f979b670040d8c0b659e1f581fa41fdeec41b74b85d8c26c01",
    "third_party_source/sidereon-core-0.33.1/tides/pole.rs":
      "b4cc4c16bdd8ce1d8f04073602ab47dfb85a002b946ab192e8d4d2d600f0a1f8",
  };
  const exactThirdPartyDigests = {
    "licenses/ERFA-BSD-3-Clause.txt":
      "b1858f9a263f22c438a455a32945da51a31a0ae25a21055da13bb7ed57cc3b51",
    "licenses/IERS-Conventions-Software-License.txt":
      "a441d8ffe8151ddd5f1e0a9f82ce88ed54bd2f55e83fee6a519e50b006a8cba2",
    "licenses/SciPy-BSD-3-Clause.txt":
      "221e59f5e910fd7f94e44f0dac77436a11338c285c6346232e4a850a50da0e94",
    ...coreSourceDigests,
  };
  for (const [path, expected] of Object.entries(exactThirdPartyDigests)) {
    const actual = sha256(join(packageRoot, path));
    if (actual !== expected) {
      throw new Error(`bundled third-party material drift for ${path}: ${actual}`);
    }
  }

  const requiredExports = [
    "ExactSp3Coverage",
    "ExactSp3ParseResult",
    "ExactSp3Request",
    "defaultSampleForDate",
    "parseExactSp3",
    "productSolutionClass",
    "validateExactSp3",
  ];
  const requiredExportsLiteral = JSON.stringify(requiredExports);
  const requiredDeclarations = [
    "export enum ExactSp3Coverage",
    "export class ExactSp3ParseResult",
    "export class ExactSp3Request",
    "export function defaultSampleForDate(",
    "export function parseExactSp3(",
    "export function productSolutionClass(",
    "export function validateExactSp3(",
    'compression: "none" | "gzip" | "unix_compress";',
  ];
  for (const path of ["pkg/sidereon.d.ts", "pkg-node/sidereon.d.ts"]) {
    const declarations = readFileSync(join(packageRoot, path), "utf8");
    for (const declaration of requiredDeclarations) {
      if (!declarations.includes(declaration)) {
        throw new Error(`${path} is missing declaration ${declaration}`);
      }
    }
  }

  writeConsumer("package.json", JSON.stringify({ private: true, type: "module" }, null, 2) + "\n");
  const tarball = join(packDirectory, packed.filename);
  run(
    "npm",
    ["install", "--ignore-scripts", "--no-audit", "--no-fund", "--package-lock=false", tarball],
    consumerDirectory,
  );

  const installedPackage = join(consumerDirectory, "node_modules", "@neilberkman", "sidereon");
  const apacheLicense = readFileSync(join(installedPackage, "licenses", "Apache-2.0.txt"), "utf8");
  const erfaLicense = readFileSync(
    join(installedPackage, "licenses", "ERFA-BSD-3-Clause.txt"),
    "utf8",
  );
  const iersLicense = readFileSync(
    join(installedPackage, "licenses", "IERS-Conventions-Software-License.txt"),
    "utf8",
  );
  const scipyLicense = readFileSync(
    join(installedPackage, "licenses", "SciPy-BSD-3-Clause.txt"),
    "utf8",
  );
  const libloadingLicense = readFileSync(
    join(installedPackage, "licenses", "libloading-ISC.txt"),
    "utf8",
  );
  if (!apacheLicense.includes("Apache License") || !apacheLicense.includes("Version 2.0")) {
    throw new Error("installed package has an incomplete Apache-2.0 license text");
  }
  if (
    !erfaLicense.includes("Copyright (C) 2013-2021, NumFOCUS Foundation") ||
    !erfaLicense.includes("Standards of Fundamental Astronomy")
  ) {
    throw new Error("installed package has an incomplete ERFA 2.0.1 license text");
  }
  if (
    !iersLicense.includes("IERS Conventions Software License") ||
    !iersLicense.includes("e) The source code must be included for all routine(s)")
  ) {
    throw new Error("installed package has an incomplete IERS license text");
  }
  if (
    !scipyLicense.includes("Copyright (c) 2001-2002 Enthought, Inc. 2003, SciPy Developers") ||
    !scipyLicense.includes("Redistribution and use in source and binary forms")
  ) {
    throw new Error("installed package has an incomplete SciPy 1.18.0 license text");
  }
  if (
    !libloadingLicense.includes("Copyright © 2015, Simonas Kazlauskas") ||
    !libloadingLicense.includes("Permission to use, copy, modify, and/or distribute")
  ) {
    throw new Error("installed package has an incomplete libloading ISC license text");
  }
  for (const [path, expected] of Object.entries(exactThirdPartyDigests)) {
    const actual = sha256(join(installedPackage, path));
    if (actual !== expected) {
      throw new Error(`packed third-party material drift for ${path}: ${actual}`);
    }
  }

  writeConsumer(
    "node-esm.mjs",
    `import assert from "node:assert/strict";
import * as Sidereon from "@neilberkman/sidereon";
import { BrowserExactProductCache } from "@neilberkman/sidereon/exact-cache";
import * as LegacyTypes from "@neilberkman/sidereon/types";

assert.equal(typeof Sidereon.default, "object", "Node ESM loaded the web initializer");
for (const name of ${requiredExportsLiteral}) {
  assert.ok(name in Sidereon, \`Node ESM is missing runtime export \${name}\`);
}
assert.equal(Sidereon.defaultSampleForDate("gfz", "sp3", 2021, 5, 17), "15M");
assert.equal(typeof BrowserExactProductCache, "function");
assert.deepEqual(Object.keys(LegacyTypes), []);
`,
  );
  writeConsumer(
    "node-cjs.cjs",
    `const assert = require("node:assert/strict");
const Sidereon = require("@neilberkman/sidereon");
const LegacyTypes = require("@neilberkman/sidereon/types");

assert.equal(Object.hasOwn(Sidereon, "default"), false);
assert.deepEqual(Object.keys(LegacyTypes), []);
for (const name of ${requiredExportsLiteral}) {
  assert.ok(name in Sidereon, \`CommonJS is missing runtime export \${name}\`);
}
assert.equal(Sidereon.defaultSampleForDate("gfz", "sp3", 2021, 5, 17), "15M");
`,
  );
  writeConsumer(
    "browser-condition.mjs",
    `import assert from "node:assert/strict";
import initialize, * as Sidereon from "@neilberkman/sidereon";

assert.equal(typeof initialize, "function", "browser condition did not load the web build");
assert.equal(typeof Sidereon.initSync, "function");
for (const name of ${requiredExportsLiteral}) {
  assert.ok(name in Sidereon, \`browser condition is missing runtime export \${name}\`);
}
`,
  );
  run(process.execPath, ["node-esm.mjs"], consumerDirectory);
  run(process.execPath, ["node-cjs.cjs"], consumerDirectory);
  run(process.execPath, ["--conditions=browser", "browser-condition.mjs"], consumerDirectory);

  writeConsumer(
    "node-consumer.ts",
    `import * as Sidereon from "@neilberkman/sidereon";
import type { ExactCacheEntry } from "@neilberkman/sidereon/exact-cache";
import type {} from "@neilberkman/sidereon/types";

type Assert<Condition extends true> = Condition;
type HasWebInitializer = "initSync" extends keyof typeof Sidereon ? true : false;
type _NodeUsesNodeDeclarations = Assert<HasWebInitializer extends false ? true : false>;
const sample: string = Sidereon.defaultSampleForDate("gfz", "sp3", 2021, 5, 17);
const entry: ExactCacheEntry | undefined = undefined;
void sample;
void entry;
`,
  );
  writeConsumer(
    "node-consumer.cts",
    `import Sidereon = require("@neilberkman/sidereon");
import { defaultSampleForDate as namedDefaultSampleForDate } from "@neilberkman/sidereon";

type Assert<Condition extends true> = Condition;
type HasWebInitializer = "initSync" extends keyof typeof Sidereon ? true : false;
type _CommonJsUsesNodeDeclarations = Assert<HasWebInitializer extends false ? true : false>;
const sample: string = Sidereon.defaultSampleForDate("gfz", "sp3", 2021, 5, 17);
const namedSample: string = namedDefaultSampleForDate("gfz", "sp3", 2021, 5, 17);
void sample;
void namedSample;
`,
  );
  writeConsumer(
    "tsconfig.node.json",
    JSON.stringify(
      {
        compilerOptions: {
          target: "ESNext",
          module: "NodeNext",
          moduleResolution: "NodeNext",
          strict: true,
          noEmit: true,
          skipLibCheck: false,
          types: [],
        },
        files: ["node-consumer.ts", "node-consumer.cts"],
      },
      null,
      2,
    ) + "\n",
  );
  writeConsumer(
    "browser-consumer.ts",
    `import initialize, { initSync, defaultSampleForDate } from "@neilberkman/sidereon";
import type { BrowserExactProductCache } from "@neilberkman/sidereon/exact-cache";

const asyncInitializer: typeof initialize = initialize;
const syncInitializer: typeof initSync = initSync;
const sample: string = defaultSampleForDate("gfz", "sp3", 2021, 5, 17);
const cache: BrowserExactProductCache | undefined = undefined;
void asyncInitializer;
void syncInitializer;
void sample;
void cache;
`,
  );
  writeConsumer(
    "tsconfig.browser.json",
    JSON.stringify(
      {
        compilerOptions: {
          target: "ESNext",
          module: "ESNext",
          moduleResolution: "Bundler",
          customConditions: ["browser"],
          strict: true,
          noEmit: true,
          skipLibCheck: false,
          types: [],
        },
        files: ["browser-consumer.ts"],
      },
      null,
      2,
    ) + "\n",
  );

  const typeScript = join(packageRoot, "node_modules", "typescript", "bin", "tsc");
  run(process.execPath, [typeScript, "--project", "tsconfig.node.json"], consumerDirectory);
  run(process.execPath, [typeScript, "--project", "tsconfig.browser.json"], consumerDirectory);
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
