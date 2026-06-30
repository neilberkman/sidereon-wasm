// Flat ESLint config. The only first-party JavaScript in this repo is the ESM
// test suite under test/ and the build helper in scripts/; the published surface
// is wasm-bindgen-generated and is not linted here (see ignores). Rules stay at
// the recommended baseline plus a few correctness guards (no stray console, no
// unused symbols) so the suite holds the same zero-warning bar as the Rust side.
import js from "@eslint/js";
import globals from "globals";

export default [
  {
    ignores: ["pkg/", "pkg-node/", "target/", "node_modules/"],
  },
  js.configs.recommended,
  {
    files: ["test/**/*.mjs", "scripts/**/*.mjs"],
    languageOptions: {
      ecmaVersion: "latest",
      sourceType: "module",
      globals: {
        ...globals.node,
        ...globals.browser,
      },
    },
    rules: {
      "no-console": "error",
      "no-unused-vars": ["error", { argsIgnorePattern: "^_" }],
    },
  },
];
