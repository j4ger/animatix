// Minifies the <amx-player> web component source into ../amx-player.js (the
// distributable, committed). Run from web/tools: npm run build:embed
import * as esbuild from "esbuild";

await esbuild.build({
  entryPoints: ["../embed/src/amx-player.js"],
  outfile: "../embed/amx-player.js",
  bundle: false,
  format: "esm",
  minify: true,
  target: "es2022",
});

console.log("embed bundle ready: web/embed/amx-player.js");
