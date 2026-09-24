// Bundles CodeMirror 6 into a single ESM file (../vendor/cm.js) so the demo
// site needs no CDN and no build step to serve. Run from web/tools:
//   npm install && npm run build
import * as esbuild from "esbuild";

const entry = `export * from "codemirror";
export { EditorState } from "@codemirror/state";
export { EditorView, Decoration, WidgetType, keymap } from "@codemirror/view";
export { StateEffect, StateField, RangeSetBuilder } from "@codemirror/state";
export { StreamLanguage, HighlightStyle, syntaxHighlighting } from "@codemirror/language";
export { tags } from "@lezer/highlight";
export { indentUnit } from "@codemirror/language";
export { searchKeymap, highlightSelectionMatches } from "@codemirror/search";
export { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
`;

await esbuild.build({
  stdin: { contents: entry, resolveDir: process.cwd(), sourcefile: "cm-entry.js" },
  bundle: true,
  format: "esm",
  minify: true,
  outfile: "../vendor/cm.js",
  target: "es2022",
});

console.log("vendor bundle ready: web/vendor/cm.js");
