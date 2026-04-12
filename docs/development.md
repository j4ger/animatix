# Development and Debugging Utilities

This document describes the internal and contributor-facing utilities that exist today for inspecting, debugging, and validating Animatix scenes.

## Current CLI Utilities

Animatix currently exposes four practical CLI workflows.

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

## About Keyframes and Timeline Debugging

Animatix is built on an internal keyframed timeline system. Keyframes are created in the timeline layer and evaluated during rendering, but there is currently **no dedicated public CLI command** to export compiled keyframes or dump timeline tracks directly.

That means the current practical debugging loop is:

1. inspect the AST with `ast`
2. render targeted frames with `image --time ...`
3. render a full clip with `video` when validating final behavior

If a dedicated keyframe export or track dump tool is added later, this document should be updated to include it.

## Recommended Validation Workflow

For parser or language changes:

```bash
cargo run -- ast path/to/scene.amx
cargo test
```

For runtime, layout, rendering, or timeline changes:

```bash
cargo run -- image path/to/scene.amx --time 0.0 --output /tmp/frame0.png
cargo run -- image path/to/scene.amx --time 1.5 --output /tmp/frame1.png
cargo run -- image examples/reactive_runtime.amx --time 0.5 --output /tmp/reactive_a.png
cargo run -- image examples/reactive_runtime.amx --time 1.5 --output /tmp/reactive_b.png
cargo run -- video path/to/scene.amx --output /tmp/check.mp4 --fps 30
cargo test
```

For demo work:

- keep runnable demos under `examples/`
- keep future-only sketches under `examples/planned/`
- verify runnable demos with both `ast` and `image`/`video`

## Source Areas Worth Knowing

- `crates/animatix/src/main.rs` — CLI entrypoint
- `crates/animatix/src/parser.rs` — parser
- `crates/animatix/src/ast.rs` — AST types
- `crates/animatix/src/module.rs` — import/module loading
- `crates/animatix/src/timeline/` — keyframed runtime evaluation, actions, morphing, plotting
- `crates/animatix/src/renderer/` — rendering backend integration

These are the main places to inspect when debugging a mismatch between language syntax, timeline behavior, and rendered output.
