// The root package.json declares "type": "module" for the ESM (web) build under
// pkg/. The nodejs build under pkg-node/ is CommonJS (it uses __dirname and
// require('fs') to load the wasm), so it must be marked accordingly or Node
// loads it as ESM and the wasm path resolution breaks. Emit a package.json that
// scopes pkg-node/ to commonjs.
import { writeFileSync } from "node:fs";

writeFileSync("pkg-node/package.json", JSON.stringify({ type: "commonjs" }, null, 2) + "\n");
