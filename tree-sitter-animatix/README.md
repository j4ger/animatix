# tree-sitter-animatix

Tree-sitter grammar for the [Animatix](https://github.com/your-org/animatix) animation DSL.

## Overview

Animatix is a layout-first animation language for creating animated scenes. This grammar provides syntax highlighting, code folding, and structural editing support for `.amx` files in editors that support tree-sitter.

## Grammar Features

The grammar supports all Animatix language constructs:

- **Actors**: `label: Type, prop: value`
- **Keyframes**: `#0.5s`, `#+0.3s`
- **Scenes**: `# SceneName`
- **Actions**: `fade-in target [800ms, ease: ease-out]`
- **Control flow**: `if`, `for`, `sequence`, `stagger`, `always`
- **Components**: `component Name(params) { ... }`
- **Expressions**: numbers, strings, booleans, tuples, arrays, sets, paths, calls, closures, if-expressions
- **Modifiers**: `[duration, delay: 500ms, ease: bounce]`
- **Slots**: `@slot`, `@name { ... }`
- **Imports**: `import "path" as alias`
- **Reactive bindings**: `prop := expression`

## Regenerating the Parser

After modifying `grammar.js`, regenerate the C parser:

```bash
tree-sitter generate
```

## Running Tests

```bash
tree-sitter test
```

Test cases are in `test/corpus/` and cover:
- `statements.txt` — all statement types
- `expressions.txt` — expression types and precedence
- `keyframes.txt` — keyframes, scenes, and play statements
- `control_flow.txt` — if, for, sequence, stagger, always
- `slots_and_actions.txt` — slots and action invocations

## Known Limitations

- **Modifier block ambiguity**: `[...]` after a property value is ambiguous with index expressions. Use commas to separate properties from modifier blocks when needed.
- **Hyphens in identifiers**: `a-b` is a single identifier, not subtraction. Always use spaces around the `-` operator: `a - b`.
- **No escape sequences**: Strings do not support `\n`, `\"`, etc. Use the other quote delimiter to include quotes: `"it's fine"` or `'he said "hi"'`.

## License

MIT OR Apache-2.0
