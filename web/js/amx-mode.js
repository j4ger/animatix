// .amx syntax highlighting for CodeMirror, mirroring the single tokenizer's
// vocabulary (crates/animatix-syntax/src/token.rs / builtins.rs): keywords,
// reserved words, actor/scene markers, strings, numbers with time suffixes.
import { StreamLanguage } from "../vendor/cm.js";

const KEYWORDS = new Set([
  "config", "import", "as", "let", "pub", "type", "component", "fn", "return",
  "sequence", "stagger", "always", "for", "in", "if", "else", "match", "play",
  "actor", "keyframe", "scene", "at", "relative", "from", "to", "with", "drive",
  "conditional", "easing", "audio", "track", "persist", "on", "loop",
]);

const IDENT = /^[a-zA-Z_][a-zA-Z0-9_]*(?:-[a-zA-Z0-9_]+)*/;

export const amxLanguage = StreamLanguage.define({
  name: "amx",
  token(stream) {
    // scene marker: `# Name`
    if (stream.pos === 0 && stream.peek() === 0x23) {
      stream.skipToEnd();
      return "typeName";
    }
    if (stream.match("//")) {
      stream.skipToEnd();
      return "lineComment";
    }
    if (stream.match('"""')) {
      // doc/comment block — consume to closing triple quote
      while (stream.next() !== null) {
        if (stream.match('"""', false)) { stream.match('"""'); break; }
      }
      return "comment";
    }
    if (stream.match(/"(?:[^"\\]|\\.)*?"/)) return "string";
    if (stream.match(/\d+(?:\.\d+)?(?:ms|s)?/)) return "number";

    const m = stream.match(IDENT);
    if (m) {
      const word = m[0];
      if (stream.peek() === 0x28) return "variableName"; // call
      if (/^[A-Z]/.test(word)) return "typeName";        // primitive/component types
      if (KEYWORDS.has(word)) return "keyword";
      if (stream.eol()) return null;
      // `label:` declarations and `prop:` keys read as properties
      return null;
    }
    stream.next();
    return null;
  },
});
