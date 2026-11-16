import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const registry = "registry+https://github.com/rust-lang/crates.io-index";
const coreVersion = "4.0.0";
const coreRevision = "3132eeac13ebf30f63766fc608aa9aae9258dd3d";

for (const manifest of ["Cargo.toml", "test/golden-gen/Cargo.toml"]) {
  const metadata = JSON.parse(
    execFileSync(
      "cargo",
      ["metadata", "--locked", "--format-version", "1", "--manifest-path", manifest],
      { cwd: root, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 },
    ),
  );
  const owner = metadata.packages.find((pkg) => pkg.manifest_path === join(root, manifest));
  assert.ok(owner, `${manifest}: missing package`);
  for (const name of ["sidereon", "sidereon-core"]) {
    const requirement = owner.dependencies.find((dependency) => dependency.name === name);
    assert.equal(requirement.req, `=${coreVersion}`, `${manifest}: ${name} exact version`);
    assert.equal(requirement.source, registry, `${manifest}: ${name} registry requirement`);
  }
  for (const [name, version] of [
    ["sidereon", coreVersion],
    ["sidereon-core", coreVersion],
    ["trust-region-least-squares", "0.11.0"],
  ]) {
    const packages = metadata.packages.filter((pkg) => pkg.name === name);
    assert.equal(packages.length, 1, `${manifest}: ${name} package count`);
    const pkg = packages[0];
    assert.equal(pkg.version, version, `${manifest}: ${name} version`);
    assert.equal(pkg.source, registry, `${manifest}: ${name} source`);
    if (name === "trust-region-least-squares") continue;
    const directory = dirname(pkg.manifest_path);
    const vcs = JSON.parse(readFileSync(join(directory, ".cargo_vcs_info.json"), "utf8"));
    assert.equal(vcs.git.sha1, coreRevision, `${name}: published revision`);
    if (name !== "sidereon-core") continue;
    for (const filename of ["mod.rs", "ocean.rs", "pole.rs"]) {
      assert.deepEqual(
        readFileSync(join(root, `third_party_source/sidereon-core-${coreVersion}/tides`, filename)),
        readFileSync(join(directory, "src/tides", filename)),
        `published tide source: ${filename}`,
      );
    }
  }
}

const fixture = JSON.parse(readFileSync(join(root, "test/fixtures/core_goldens.json"), "utf8"));
assert.equal(fixture.coreRev, coreRevision, "fixture core revision");
const generator = readFileSync(join(root, "test/golden-gen/src/main.rs"), "utf8");
assert.ok(generator.includes(`const CORE_REV: &str = "${coreRevision}";`));
const notices = readFileSync(join(root, "THIRD-PARTY-NOTICES.md"), "utf8");
assert.ok(notices.includes(`sidereon-core\` ${coreVersion}`));
assert.ok(notices.includes(coreRevision));
process.stdout.write(
  "Published core versions, registry sources, revisions, and bundled tide sources match.\n",
);
