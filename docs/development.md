# Development and Debugging Utilities

This document describes the internal and contributor-facing utilities that exist today for inspecting, debugging, and validating Animatix scenes.

## Current CLI Utilities

Animatix currently exposes five practical CLI workflows.

### 1. AST inspection

```bash
cargo run -- ast examples/showcase.amx
```

Use this to inspect the parsed, module-expanded AST before runtime evaluation.

Useful flags:

- `--compact` for one-line output
- `--force` exists as a CLI/debug flag, but is not meaningfully wired beyond argument parsing yet

### 2. Live render preview

```bash
cargo run -- render examples/showcase.amx
```

Use this for fast visual iteration when you want to inspect the current scene interactively.

### 3. Frame export

```bash
cargo run -- image examples/showcase.amx --time 1.5 --output /tmp/showcase.png
```

Use this when debugging timeline behavior at a specific moment. This is the best current tool for validating whether a change really appears at the expected keyframe/time.

Common flags:

- `--time` to select the exact render time
- `--width` and `--height` to validate fixed-size output
- `--output` to control the output path

### 4. Video export

```bash
cargo run -- video examples/showcase.amx --output /tmp/showcase.mp4 --fps 30 --duration 5
```

Use this for end-to-end animation validation.

Common flags:

- `--fps`
- `--duration`
- `--width`
- `--height`
- `--output`

### 5. GIF export

```bash
cargo run -- gif examples/showcase.amx --output /tmp/showcase.gif --fps 15 --duration 5
```

Use this for web-friendly animation exports and sharing.

Common flags:

- `--fps` (default: 15, optimized for file size)
- `--duration`
- `--width` (default: 640)
- `--height` (default: 360)
- `--output`

## About Keyframes and Timeline Debugging

Animatix is built on an internal keyframed timeline system. Keyframes are created in the timeline layer and evaluated during rendering, but there is currently **no dedicated public CLI command** to export compiled keyframes or dump timeline tracks directly.

That means the current practical debugging loop is:

1. inspect the AST with `ast`
2. render targeted frames with `image --time ...`
3. render a full clip with `video` when validating final behavior
4. export a web-friendly `gif` for sharing

If a dedicated keyframe export or track dump tool is added later, this document should be updated to include it.

## Recommended Validation Workflow

For parser or language changes:

```bash
cargo run -- ast path/to/scene.amx
cargo test
```

If the change affects accepted syntax or syntax-highlighting structure:

```bash
cd tree-sitter-animatix
tree-sitter generate
tree-sitter test
tree-sitter highlight ../examples/reactive_runtime.amx
```

For runtime, layout, rendering, or timeline changes:

```bash
cargo run -- image path/to/scene.amx --time 0.0 --output /tmp/frame0.png
cargo run -- image path/to/scene.amx --time 1.5 --output /tmp/frame1.png
cargo run -- image examples/reactive_runtime.amx --time 0.5 --output /tmp/reactive_a.png
cargo run -- image examples/reactive_runtime.amx --time 1.5 --output /tmp/reactive_b.png
cargo run -- image examples/effects_demo.amx --time 3.0 --output /tmp/effects_frame.png
cargo run -- video path/to/scene.amx --output /tmp/check.mp4 --fps 30
cargo run -- gif path/to/scene.amx --output /tmp/check.gif --fps 15
cargo test
```

For demo work:

- keep runnable demos under `examples/`
- verify runnable demos with both `ast` and `image`/`video`
- key examples: `showcase.amx`, `reactive_runtime.amx`, `effects_demo.amx`

For effects actions:

```bash
cargo run -- render examples/effects_demo.amx
cargo run -- image examples/effects_demo.amx --time 3.0 --output /tmp/effects_frame.png
cargo run -- video examples/effects_demo.amx --output /tmp/effects.mp4 --fps 30 --duration 10
```

Available effects actions:
- `shake target [intensity: N, frequency: F, duration]` - Rapid oscillating horizontal motion
- `pulse target [intensity: N, duration]` - Scale up then return to normal
- `bounce target [intensity: N, duration]` - Elastic bounce motion

## Source Areas Worth Knowing

- `crates/animatix/src/main.rs` — CLI entrypoint
- `crates/animatix/src/parser.rs` — parser
- `crates/animatix/src/ast.rs` — AST types
- `crates/animatix/src/module.rs` — import/module loading
- `crates/animatix/src/timeline/` — keyframed runtime evaluation, actions, morphing, plotting, primitive descriptors, and timeline build orchestration
- `crates/animatix/src/timeline/modifier_runtime/` — modifier IR and bytecode VM implementation
- `crates/animatix/src/renderer/` — rendering backend integration
- `crates/animatix-gui/src/app/` — GUI runtime, workspace, persistence, file tree, and preview UI helpers
- `tree-sitter-animatix/` — parser-derived editor/tooling grammar package for `.amx`

These are the main places to inspect when debugging a mismatch between language syntax, timeline behavior, and rendered output.
