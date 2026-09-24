// CodeMirror 6 integration: .amx highlighting, dark theme, and diagnostic
// underline marks fed by the wasm build pipeline.
import {
  EditorState,
  EditorView,
  Decoration,
  StateEffect,
  StateField,
  basicSetup,
  HighlightStyle,
  syntaxHighlighting,
  indentUnit,
  tags as t,
} from "../vendor/cm.js";
import { amxLanguage } from "./amx-mode.js";

const amxHighlight = HighlightStyle.define([
  { tag: t.keyword, color: "#c792ea" },
  { tag: t.typeName, color: "#82aaff" },
  { tag: t.number, color: "#f78c6c" },
  { tag: t.string, color: "#c3e88d" },
  { tag: t.lineComment, color: "#5b6575", fontStyle: "italic" },
  { tag: t.comment, color: "#5b6575", fontStyle: "italic" },
  { tag: t.variableName, color: "#89ddff" },
  { tag: t.bool, color: "#f78c6c" },
  { tag: t.null, color: "#f78c6c" },
  { tag: t.operator, color: "#89ddff" },
  { tag: t.propertyName, color: "#ffcb6b" },
]);

const amxTheme = EditorView.theme({
  "&": { backgroundColor: "var(--bg)", color: "var(--text)", height: "100%" },
  ".cm-content": { caretColor: "var(--accent)", padding: "12px 0" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--accent)" },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection":
    { backgroundColor: "#2a3648" },
  ".cm-selectionMatch": { backgroundColor: "rgba(245,185,66,0.16)" },
  ".cm-searchMatch": { backgroundColor: "rgba(106,169,239,0.22)" },
});

const setDiagsEffect = StateEffect.define();

const diagField = StateField.define({
  create: () => Decoration.none,
  update(value, tr) {
    value = value.map(tr.changes);
    for (const effect of tr.effects) {
      if (effect.is(setDiagsEffect)) value = effect.value;
    }
    return value;
  },
  provide: (field) => EditorView.decorations.from(field),
});

const markClass = { error: "cm-diag-error", warning: "cm-diag-warning" };

/// UTF-8 byte offsets (Rust side) → JS string offsets (CodeMirror side).
/// Returns a lookup table; text is ASCII in the common case, so the fast
/// path short-circuits table construction entirely.
function byteOffsetTable(text) {
  let pureAscii = true;
  for (let i = 0; i < text.length; i++) {
    if (text.charCodeAt(i) > 0x7f) { pureAscii = false; break; }
  }
  if (pureAscii) return null;
  const bytes = new TextEncoder().encode(text);
  const map = new Uint32Array(bytes.length + 1);
  let b = 0;
  for (let i = 0; i < text.length; ) {
    const cp = text.codePointAt(i);
    const utf16 = cp > 0xffff ? 2 : 1;
    const utf8 = cp <= 0x7f ? 1 : cp <= 0x7ff ? 2 : cp <= 0xffff ? 3 : 4;
    for (let k = 0; k < utf8; k++) map[b + k] = i;
    b += utf8;
    i += utf16;
  }
  map[b] = text.length;
  return map;
}

export function createEditor(parent, { onEdit }) {
  const view = new EditorView({
    parent,
    state: EditorState.create({
      doc: "",
      extensions: [
        basicSetup,
        amxTheme,
        syntaxHighlighting(amxHighlight),
        diagField,
        indentUnit.of("    "),
        amxLanguage,
        EditorView.updateListener.of((update) => {
          if (update.docChanged) onEdit();
        }),
      ],
    }),
  });
  return view;
}

export const editorApi = {
  getDoc(view) {
    return view.state.doc.toString();
  },

  setDoc(view, text) {
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } });
  },

  /// Paint underline marks for diagnostics and return rows annotated with
  /// JS-side offsets for the diagnostics panel.
  setDiagnostics(view, diagnostics) {
    const text = view.state.doc.toString();
    const table = byteOffsetTable(text);
    const docLength = text.length;
    const toJs = (byteOffset) => {
      const off = Math.max(0, Math.min(byteOffset ?? -1, docLength));
      return table ? table[Math.min(off, table.length - 1)] : off;
    };

    // line starts for diags without a byte span
    const lineStarts = [0];
    for (let i = 0; i < text.length; i++) {
      if (text.charCodeAt(i) === 10) lineStarts.push(i + 1);
    }
    const lineOffset = (line, column) => {
      const start = lineStarts[Math.min(Math.max(line, 1) - 1, lineStarts.length - 1)];
      return start + Math.max((column ?? 1) - 1, 0);
    };

    const builder = [];
    for (const diag of diagnostics) {
      const cls = markClass[diag.severity];
      if (!cls) continue;
      let from;
      let to;
      if (diag.span && diag.span[1] > diag.span[0]) {
        from = toJs(diag.span[0]);
        to = Math.min(toJs(diag.span[1]), docLength);
      } else if (diag.line) {
        from = toJs(lineOffset(diag.line, diag.column));
        to = from;
      }
      if (from == null || from >= docLength) continue;
      if (to <= from) to = Math.min(from + 1, docLength);
      builder.push(Decoration.mark({ class: cls }).range(from, to));
    }

    view.dispatch({ effects: setDiagsEffect.of(builder.length > 0 ? Decoration.set(builder, true) : Decoration.none) });
  },

  jumpTo(view, diag) {
    const text = view.state.doc.toString();
    const line = Math.min(Math.max(diag.line ?? 1, 1), view.state.doc.lines);
    const lineInfo = view.state.doc.line(line);
    const anchor = Math.min(lineInfo.from + Math.max((diag.column ?? 1) - 1, 0), lineInfo.to);
    view.dispatch({
      selection: { anchor },
      scrollIntoView: true,
      effects: [], // selection covers repositioning
    });
    view.focus();
  },
};
