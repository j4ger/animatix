// Registry of demo scenes. Files are copied from examples/ into pkg/examples/
// by scripts/build-web.sh, so the shown sources are exactly the repo's.
export const EXAMPLES = [
  { id: "hello",           file: "hello.amx",           title: "Hello, Animatix",       hint: "title card, theme import" },
  { id: "motion",          file: "motion.amx",          title: "Motion basics",         hint: "keyframes & easing" },
  { id: "expressions",     file: "expressions.amx",     title: "Expressions",           hint: "computed properties" },
  { id: "colors",          file: "colors.amx",          title: "Color & layout",        hint: "palettes, stacks" },
  { id: "code",            file: "code.amx",            title: "Code block",            hint: "typst text engine" },
  { id: "plots",           file: "plots.amx",           title: "Plots",                 hint: "data-driven curves" },
  { id: "for_loop",        file: "for_loop.amx",        title: "Generation loop",       hint: "procedural actors" },
  { id: "multiscene",      file: "multiscene.amx",      title: "Multi-scene",           hint: "scenes & transitions" },
  { id: "sorting_theatre", file: "sorting_theatre.amx", title: "Sorting Theatre",       hint: "gallery piece" },
  { id: "epicycles",       file: "epicycles.amx",       title: "Epicycles",             hint: "gallery piece" },
];

export function find(id) {
  return EXAMPLES.find((e) => e.id === id);
}
