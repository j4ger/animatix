// Minifies the <amx-player> web component source into ../amx-player.js (the
// distributable, committed). Run from web/tools: npm run build:embed
//
// The source check runs first on purpose: esbuild bundles a bare identifier as
// a free global, so a module that forgot to import a `core.js` export builds
// clean and fails in the browser (see check-embed-globals.mjs).
import { execFileSync } from "node:child_process";
import * as esbuild from "esbuild";

execFileSync(process.execPath, ["check-embed-globals.mjs"], { stdio: "inherit" });

await esbuild.build({
  entryPoints: ["../embed/src/amx-player.js"],
  outfile: "../embed/amx-player.js",
  bundle: true,
  format: "esm",
  minify: true,
  target: "es2022",
});

console.log("embed bundle ready: web/embed/amx-player.js");
