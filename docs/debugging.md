# Debugging Animatix

This document outlines the available debugging tools and techniques for the Animatix repository.

## CLI Debugging Commands

The Animatix CLI provides specific subcommands and flags to help inspect the internal state of scene files.

### Abstract Syntax Tree (AST) Inspection

To verify how an `.amx` file is being parsed, use the `ast` subcommand.

```bash
# Pretty-print the AST for a file
cargo run -- ast path/to/scene.amx

# Print AST on a single line
cargo run -- ast path/to/scene.amx --compact

# Force print AST even if there are parsing errors
cargo run -- ast path/to/scene.amx --force
```

### Static Image Rendering

Instead of rendering a full video, you can render a specific frame to a PNG image to debug visual issues.

```bash
# Render frame at 2.5 seconds
cargo run -- image path/to/scene.amx --time 2.5 --output debug_frame.png
```

## Internal Debug Features

### Text Rendering Debugging

The repository contains a specialized `text_debug.rs` module located in `crates/animatix/src/renderer/`. This module implements `fmt::Debug` for internal text rendering structures:

- `ExtractedGlyph`
- `ExtractedShape`

These implementations currently provide simplified labels ("ExtractedGlyph", "ExtractedShape") when printed via `{:?}`.

### Hardcoded Traces

Several shell scripts in the `crates/` directory are used to inject temporary debug print statements into the source code for deep inspection:

- `print_instances.sh`: Injects prints for text instance counts and data in the video renderer.
- `print_bg.sh`: Injects prints for background color evaluation.
- `print_markup.sh`: Injects prints for raw markup strings in the text renderer.
- `print_fonts.sh`: Injects prints for font family information.

These scripts use `sed` to modify the source code. If you see unexpected `println!` output in the console, check if these patches have been applied to `animatix/src/renderer/video.rs` or `animatix/src/renderer/text.rs`.

## Logging

Animatix currently relies on standard output (`println!`) and standard error (`eprintln!`) for reporting status and errors. There is no formal logging framework (like `log` or `tracing`) implemented at this time.

- **Parser Errors**: Displayed on `stderr` when parsing fails.
- **Render Status**: Basic progress and configuration info is printed to `stdout` during `render` or `video` commands.
