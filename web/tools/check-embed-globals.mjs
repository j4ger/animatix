// Guard against the defect that shipped once: after the embed player was split
// into `core` + one module per concern, a module referenced a `core.js` export
// without destructuring/namespace access. esbuild bundles a bare identifier as
// a free global without complaint, so the site built, the gate passed, and
// every `<amx-player>` threw `ReferenceError: qualityStep is not defined`
// inside `_applyRenderScale` — the page showed "Failed to load scene" for every
// figure on every page.
//
// This walks each player module, collects the `core.js` export names it uses as
// bare identifiers, and fails when one is neither in that module's
// `const { … } = core;` binding list nor accessed as `core.<name>`.
//
// Note the namespace access: a destructured copy of a `let` export freezes at
// its module-init value, so mutable cross-module state must be read as
// `core.<name>` at the use site. The check accepts either form; the *runtime*
// value is the reviewer's call.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const srcRoot = path.join(here, "..", "embed", "src");

function stripCommentsAndStrings(source) {
  // Enough for identifier detection: comments, and single/double/backtick
  // string bodies. Template substitutions are not evaluated, which is fine —
  // an identifier only ever referenced inside a template hole is not the
  // failure mode this guards.
  return source
    .replace(/\/\*[\s\S]*?\*\//g, " ")
    .replace(/\/\/[^\n]*/g, " ")
    .replace(/`(?:\\.|[^`\\])*`/g, "``")
    .replace(/"(?:\\.|[^"\\])*"/g, '""')
    .replace(/'(?:\\.|[^'\\])*'/g, "''");
}

function coreExports(coreSource) {
  const match = coreSource.match(/export\s*\{([\s\S]*?)\};/);
  if (!match) throw new Error("core.js: could not find the `export { … };` list");
  return match[1]
    .split(",")
    .map((name) => name.trim())
    .filter(Boolean);
}

const files = [];
(function walk(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full);
    else if (entry.name.endsWith(".js") && full !== path.join(srcRoot, "player", "core.js")) files.push(full);
  }
})(srcRoot);

const core = fs.readFileSync(path.join(srcRoot, "player", "core.js"), "utf8");
const exports_ = coreExports(core);

const problems = [];
for (const file of files) {
  const raw = fs.readFileSync(file, "utf8");
  const source = stripCommentsAndStrings(raw);
  const destructured = new Set(
    (source.match(/const\s*\{([\s\S]*?)\}\s*=\s*core;/) ?? [, ""])[1]
      .split(",")
      .map((name) => name.trim())
      .filter(Boolean),
  );
  for (const name of exports_) {
    if (destructured.has(name)) continue;
    if (new RegExp(`core\\s*\\.\\s*${name}\\b`).test(source)) continue;
    if (new RegExp(`(?<![\\w.$])${name}(?![\\w$])`).test(source)) {
      problems.push(`${path.relative(here, file)} uses core export '${name}' without destructuring or core.${name}`);
    }
  }
}

if (problems.length) {
  console.error("embed source check failed:");
  for (const problem of problems) console.error(`  ${problem}`);
  process.exit(1);
}
console.log(`embed source check ok (${files.length} module(s), ${exports_.length} core exports)`);