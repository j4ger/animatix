use std::path::{Path, PathBuf};

use animatix::composition::BuildTarget;
use animatix::extension_plugin::{NativePlugin, PluginDisposer, PluginLoader};
use animatix::timeline::{DebugRenderOptions, SceneDimensions};
use animatix_analyzer::ExtensionManifest;
// Export entry points (image/video/GIF) live in the render crate.
use animatix_render::encode as export;
use animatix_syntax::diagnostics::{
    Diagnostic, DiagnosticCode, DiagnosticPhase, format_diagnostic, format_diagnostic_with_source,
};
use animatix_syntax::module::ModuleGraph;
use clap::{Parser as ClapParser, Subcommand, ValueEnum};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

mod verify;
use verify::{parse_verify_checks, run_verify_checks};

#[derive(ClapParser, Debug)]
#[command(author, version, about, long_about = "Animatix CLI Tool")]
struct Args {
    /// Increase verbosity (-v for debug, -vv for trace)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Suppress ANSI color codes in output
    #[arg(long)]
    no_color: bool,

    /// Load an extension manifest or native plugin library (repeatable)
    #[arg(long, global = true, value_name = "PLUGIN", action = clap::ArgAction::Append)]
    plugin: Vec<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

/// Output format for diagnostic reporting.
#[derive(Clone, ValueEnum, Debug)]
enum OutputFormat {
    /// Human-readable text output
    Text,
    /// Structured JSON output
    Json,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Print the scene table for a multi-scene file (name/start/duration/transitions)
    Timeline {
        /// The input Animatix scene file (.amx)
        input: PathBuf,
    },
    /// Parse and display the AST for a given file
    Ast {
        /// The input Animatix scene file (.amx)
        input: PathBuf,

        /// Format AST output on a single line instead of pretty-printing
        #[arg(short, long)]
        compact: bool,

        /// Print AST even if parsing errors occurred
        #[arg(short, long)]
        force: bool,
    },

    /// Inspect extension libraries and generated analyzer metadata
    Plugin {
        #[command(subcommand)]
        command: PluginCommands,
    },

    /// Render a specific frame to an image file (PNG/WebP)
    Image {
        /// The input Animatix scene file (.amx)
        input: PathBuf,
        /// Override a build-time variable: `--set NAME=VALUE` (repeatable).
        ///
        /// The value is parsed as an Animatix expression, so a template's own
        /// `let` is a default and the command line wins.
        #[arg(long = "set", value_parser = parse_set_pair)]
        set: Vec<(String, String)>,

        /// Output image width (defaults to the file's `config { resolution: .. }`, else 1280)
        #[arg(long)]
        width: Option<u32>,

        /// Output image height (defaults to the file's `config { resolution: .. }`, else 720)
        #[arg(long)]
        height: Option<u32>,

        /// Render time in seconds
        #[arg(long, default_value_t = 0.0)]
        time: f32,

        /// Output filename
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Draw per-node content bounding boxes for debugging
        #[arg(long)]
        debug_bounds: bool,
    },

    /// Render a visual storyboard contact sheet of keyframe beats across the animation
    Storyboard {
        /// The input Animatix scene file (.amx)
        input: PathBuf,
        /// Override a build-time variable: `--set NAME=VALUE` (repeatable).
        #[arg(long = "set", value_parser = parse_set_pair)]
        set: Vec<(String, String)>,
        /// Number of columns in the grid (default: auto, up to 4)
        #[arg(short = 'c', long)]
        cols: Option<usize>,
        /// Tile width in pixels (defaults to 480 or half scene resolution)
        #[arg(long)]
        tile_width: Option<u32>,
        /// Tile height in pixels (defaults to 270 or half scene resolution)
        #[arg(long)]
        tile_height: Option<u32>,
        /// Comma-separated timestamps in seconds to sample (overrides automatic keyframe detection)
        #[arg(long, value_delimiter = ',')]
        times: Option<Vec<f64>>,
        /// Maximum number of frames to sample when auto-detecting (default: 8)
        #[arg(long, default_value_t = 8)]
        max_frames: usize,
        /// Draw per-node content bounding boxes for debugging
        #[arg(long)]
        debug_bounds: bool,
        /// Output image filename (defaults to `<name>_storyboard.png`)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Render a scene to a video file
    #[cfg(feature = "video")]
    Video {
        /// The input Animatix scene file (.amx)
        input: PathBuf,
        /// Override a build-time variable: `--set NAME=VALUE` (repeatable).
        ///
        /// The value is parsed as an Animatix expression, so a template's own
        /// `let` is a default and the command line wins.
        #[arg(long = "set", value_parser = parse_set_pair)]
        set: Vec<(String, String)>,

        /// Output video width (defaults to the file's `config { resolution: .. }`, else 1280)
        #[arg(long)]
        width: Option<u32>,

        /// Output video height (defaults to the file's `config { resolution: .. }`, else 720)
        #[arg(long)]
        height: Option<u32>,

        /// Output framerate
        #[arg(long, default_value_t = 30)]
        fps: u32,

        /// Render duration in seconds. If omitted, auto-detects from timeline length.
        #[arg(long)]
        duration: Option<f32>,

        /// Seconds to hold the last frame after the last animation ends. Applied only when
        /// --duration is omitted.
        #[arg(long, default_value_t = 1.0)]
        hold: f32,

        /// Output filename
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Draw per-node content bounding boxes for debugging
        #[arg(long)]
        debug_bounds: bool,

        /// Maximum render threads (auto or a number)
        #[arg(short = 'j', long, default_value = "auto")]
        threads: export::MaxRenderThreads,

        /// Video codec: auto, libx264, h264_nvenc, h264_vaapi, vp9
        #[arg(long, default_value = "auto")]
        codec: export::VideoCodec,

        /// libx264 preset: ultrafast, superfast, veryfast, faster, fast, medium, slow, slower,
        /// veryslow
        #[arg(long, default_value = "medium")]
        preset: export::H264Preset,

        /// Named export preset (720p30, 1080p30, 1080p60, 4k30). Overrides
        /// width/height/fps/codec/preset.
        #[arg(long)]
        export_preset: Option<String>,
    },
    /// Render a scene to an animated GIF file
    #[cfg(feature = "video")]
    Gif {
        /// The input Animatix scene file (.amx)
        input: PathBuf,
        /// Override a build-time variable: `--set NAME=VALUE` (repeatable).
        ///
        /// The value is parsed as an Animatix expression, so a template's own
        /// `let` is a default and the command line wins.
        #[arg(long = "set", value_parser = parse_set_pair)]
        set: Vec<(String, String)>,

        /// Output GIF width (defaults to the file's `config { resolution: .. }`, else 640)
        #[arg(long)]
        width: Option<u32>,

        /// Output GIF height (defaults to the file's `config { resolution: .. }`, else 360)
        #[arg(long)]
        height: Option<u32>,

        /// Output framerate
        #[arg(long, default_value_t = 15)]
        fps: u32,

        /// Render duration in seconds. If omitted, auto-detects from timeline length.
        #[arg(long)]
        duration: Option<f32>,

        /// Seconds to hold the last frame after the last animation ends. Applied only when
        /// --duration is omitted.
        #[arg(long, default_value_t = 1.0)]
        hold: f32,

        /// Output filename
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Draw per-node content bounding boxes for debugging
        #[arg(long)]
        debug_bounds: bool,

        /// Maximum render threads (auto or a number)
        #[arg(short = 'j', long, default_value = "auto")]
        threads: export::MaxRenderThreads,

        /// Named export preset (720p30, 1080p30, 1080p60, 4k30). Overrides width/height/fps.
        #[arg(long)]
        export_preset: Option<String>,
    },
    /// Parse an .amx file and print build diagnostics
    Check {
        /// Path to the .amx file (use "-" for stdin)
        file: String,
        /// Override a build-time variable: `--set NAME=VALUE` (repeatable).
        ///
        /// The value is parsed as an Animatix expression, so a template's own
        /// `let` is a default and the command line wins.
        #[arg(long = "set", value_parser = parse_set_pair)]
        set: Vec<(String, String)>,

        /// Render one frame at time=0 to catch renderer bugs
        #[arg(long)]
        render_smoke: bool,

        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: OutputFormat,
    },
    /// Verify rendered frames against declared visibility/ink checks
    ///
    /// Reads a line-based checks file (default: `verify.txt` beside the input)
    /// and asserts what is actually on screen — an actor is visible, a frame is
    /// not blank, or two times differ. Catches silent visual regressions that
    /// `check`/`lint` and a whole-frame smoke test both miss. See
    /// `dogfood/README.md` for the check grammar.
    Verify {
        /// Path to the .amx file
        input: PathBuf,
        /// Override a build-time variable: `--set NAME=VALUE` (repeatable).
        ///
        /// The value is parsed as an Animatix expression, so a template's own
        /// `let` is a default and the command line wins.
        #[arg(long = "set", value_parser = parse_set_pair)]
        set: Vec<(String, String)>,

        /// Checks file (default: `verify.txt` in the input's directory)
        #[arg(long)]
        checks: Option<PathBuf>,

        /// Output image width (defaults to the file's `config { resolution: .. }`, else 1280)
        #[arg(long)]
        width: Option<u32>,

        /// Output image height (defaults to the file's `config { resolution: .. }`, else 720)
        #[arg(long)]
        height: Option<u32>,

        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: OutputFormat,
    },
    /// Format .amx files in-place
    Fmt {
        /// Files or directories to format (default: current directory)
        #[arg(default_value = ".")]
        paths: Vec<PathBuf>,

        /// Check formatting without modifying files (exit 1 if not formatted)
        #[arg(long)]
        check: bool,

        /// Number of spaces per indentation level
        #[arg(long, default_value_t = 2)]
        indent: usize,
    },
    /// Run linter on .amx files
    Lint {
        /// Files or directories to lint (default: current directory)
        #[arg(default_value = ".")]
        paths: Vec<PathBuf>,

        /// Output format
        #[arg(long, default_value = "text")]
        format: OutputFormat,

        /// Treat warnings as errors
        #[arg(long)]
        deny_warnings: bool,

        /// Path to .amx.toml config file
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
enum PluginCommands {
    /// Load a native plugin and print its runtime descriptors as TOML
    Describe {
        /// Native plugin library to inspect
        library: PathBuf,

        /// Write the manifest to this path instead of stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

// ----------------------------------------------------------------------------
// Shared helpers
// ----------------------------------------------------------------------------

/// Turn a module-load failure into structured diagnostics so `check`/`lint`
/// report parse errors through the same stdout channel as semantic
/// diagnostics. Previously these printed only via `tracing` (stderr), which
/// meant `--format json` emitted nothing at all for an unparseable file.
///
/// `source` is the entry file's text, used to resolve each parse error's byte
/// span into an end position so JSON consumers get a full range.
fn module_error_diagnostics(
    err: &animatix_syntax::module::ModuleError,
    source: &str,
) -> Vec<Diagnostic> {
    use animatix_syntax::module::ModuleError;

    match err {
        ModuleError::ParseErrors(errors) => errors
            .iter()
            .map(|error| {
                let diagnostic = error.to_diagnostic();
                // The error carries a byte span; the end line/column is only
                // knowable with the source in hand.
                let span = error.span.clone();
                let end = animatix_syntax::ast::Span::from_range(source, span);
                diagnostic.with_end_location(end.end_line, end.end_col)
            })
            .collect(),
        other => vec![Diagnostic::error(
            DiagnosticCode::ParseError,
            DiagnosticPhase::Parse,
            other.to_string(),
        )],
    }
}

/// Print a `check` report for one file and exit 1 when any error is present.
///
/// Both diagnostic families share this path: `build` diagnostics (from
/// `ModuleGraph`/build, pretty-printed against the source) and analyzer
/// `semantic` diagnostics. JSON output puts them in one `errors` array so a
/// CI consumer sees a complete report on stdout.
fn emit_check_report(
    file_label: &str,
    source: &str,
    diagnostics: Vec<Diagnostic>,
    semantic: Vec<animatix_analyzer::Diagnostic>,
    format: &OutputFormat,
) {
    let has_error =
        diagnostics.iter().any(|d| d.is_error()) || semantic.iter().any(|d| d.is_error());

    match format {
        OutputFormat::Json => {
            if diagnostics.is_empty() && semantic.is_empty() {
                println!(r#"{{"passed":true}}"#);
            } else {
                // `passed` mirrors the exit code: warnings are reported but do
                // not fail the run, so CI consumers never see passed:false
                // alongside exit 0.
                let errors: Vec<serde_json::Value> = diagnostics
                    .iter()
                    .map(diagnostic_to_json)
                    .chain(semantic.iter().map(semantic_diagnostic_to_json))
                    .collect();
                let report = serde_json::json!({
                    "passed": !has_error,
                    "errors": errors,
                });
                println!("{report}");
            }
        },
        OutputFormat::Text => {
            if diagnostics.is_empty() && semantic.is_empty() {
                println!("{file_label}: OK (no diagnostics)");
            } else {
                for diag in &diagnostics {
                    println!("{}", format_diagnostic_with_source(diag, source));
                }
                let (shown, folded) = fold_hints(&semantic);
                for diag in shown {
                    println!("{}:{}", file_label, diag);
                }
                if folded > 0 {
                    println!("{file_label}: {folded} hint(s) folded — --format json lists them");
                }
            }
        },
    }

    if has_error {
        std::process::exit(1);
    }
}

/// Split semantic diagnostics into the ones a text report should print, and how
/// many hints it folded away.
///
/// A hint is an observation about source that is usually deliberate — an
/// unreferenced actor still renders every frame — and this repo's own corpus
/// carries 347 of them, which was enough to make the warning column unreadable
/// (see `docs/roadmap.md`, "Easing pass"). They are folded from the text
/// report rather than deleted: the count is stated, and `--format json` keeps
/// every one.
fn fold_hints(
    diagnostics: &[animatix_analyzer::Diagnostic],
) -> (Vec<&animatix_analyzer::Diagnostic>, usize) {
    let shown: Vec<&animatix_analyzer::Diagnostic> = diagnostics
        .iter()
        .filter(|d| d.severity != animatix_analyzer::DiagnosticSeverity::Hint)
        .collect();
    (shown.clone(), diagnostics.len() - shown.len())
}

/// Extensions loaded from CLI `--plugin` arguments.
struct CliExtensions {
    loader: PluginLoader,
    manifests: Vec<ExtensionManifest>,
}

impl CliExtensions {
    fn new() -> Self {
        Self {
            loader: PluginLoader::new(),
            manifests: Vec::new(),
        }
    }

    fn install_into(
        &self,
        ctx: &mut animatix::extension_context::ExtensionContext,
    ) -> Result<Vec<PluginDisposer>, String> {
        self.loader.install_all(ctx).map_err(|err| err.to_string())
    }

    fn merged_manifest(&self) -> ExtensionManifest {
        ExtensionManifest::merge(&self.manifests)
    }
}

/// Load manifests and native plugin libraries named on the command line.
fn load_cli_extensions(paths: &[PathBuf]) -> Result<CliExtensions, String> {
    let mut extensions = CliExtensions::new();
    for path in paths {
        if is_manifest_path(path) {
            let source = std::fs::read_to_string(path)
                .map_err(|err| format!("Cannot read {}: {err}", path.display()))?;
            let manifest = ExtensionManifest::from_toml(&source)
                .map_err(|err| format!("Invalid manifest {}: {err}", path.display()))?;
            if let Some(library) = manifest.library.as_deref() {
                let library_path = path.parent().unwrap_or_else(|| Path::new(".")).join(library);
                let plugin = NativePlugin::load(&library_path).map_err(|err| err.to_string())?;
                extensions.loader.register(Box::new(plugin));
            }
            extensions.manifests.push(manifest);
        } else if is_native_library_path(path) {
            let plugin = NativePlugin::load(path).map_err(|err| err.to_string())?;
            extensions.loader.register(Box::new(plugin));
        } else {
            return Err(format!(
                "Unsupported plugin path '{}': expected a .amx-plugin.toml manifest or native library",
                path.display()
            ));
        }
    }
    Ok(extensions)
}

/// Load a native plugin, collect its runtime descriptors, and emit a manifest.
fn run_plugin_describe(library: &Path, output: Option<&Path>) -> Result<(), String> {
    let toml = animatix_plugin_tooling::generate_manifest_toml(library, output)?;
    match output {
        Some(path) => {
            std::fs::write(path, &toml)
                .map_err(|err| format!("Cannot write {}: {err}", path.display()))?;
            info!("Wrote plugin manifest: {}", path.display());
            Ok(())
        },
        None => {
            print!("{toml}");
            Ok(())
        },
    }
}

fn is_manifest_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".amx-plugin.toml"))
}

fn is_native_library_path(path: &Path) -> bool {
    matches!(path.extension().and_then(|ext| ext.to_str()), Some("so" | "dylib" | "dll"))
}

/// Split a `--set NAME=VALUE` argument on the first `=`.
fn parse_set_pair(raw: &str) -> Result<(String, String), String> {
    let (name, value) =
        raw.split_once('=').ok_or_else(|| format!("expected NAME=VALUE, got '{raw}'"))?;
    Ok((name.trim().to_string(), value.trim().to_string()))
}

/// Loads an Animatix program from disk, expands components, and builds the
/// appropriate target (single-scene `Timeline` or multi-scene `Composition`).
/// Prints build diagnostics and exits on load failure.
fn load_and_build(
    input: &Path,
    extensions: &CliExtensions,
    defines: &[(String, String)],
) -> (BuildTarget, Vec<animatix_syntax::diagnostics::Diagnostic>) {
    let (ast, namespaces, type_diagnostics) = match ModuleGraph::new().load_program(input) {
        Ok(mut program) => {
            let diagnostics = program.typecheck();
            let mut expansion_errors = Vec::new();
            let expanded = program.expand_components(&mut expansion_errors);
            let mut diagnostics = diagnostics;
            for error in expansion_errors {
                diagnostics.push(animatix_syntax::diagnostics::Diagnostic::error(
                    animatix_syntax::diagnostics::DiagnosticCode::UnknownAction,
                    animatix_syntax::diagnostics::DiagnosticPhase::Build,
                    error,
                ));
            }
            (expanded, program.namespaces, diagnostics)
        },
        Err(e) => {
            error!("Error: {}", e);
            std::process::exit(1);
        },
    };

    let mut ctx = animatix::extension_context::ExtensionContext::new();
    let disposers = match extensions.install_into(&mut ctx) {
        Ok(disposers) => disposers,
        Err(e) => {
            error!("Plugin install failed: {e}");
            std::process::exit(1);
        },
    };
    let context = std::sync::Arc::new(ctx);
    let report = BuildTarget::from_ast_with_context_and_defines(
        &ast,
        &namespaces,
        Some(input),
        context,
        defines,
    );
    let mut all_diagnostics = type_diagnostics;
    all_diagnostics.extend(report.diagnostics);
    print_build_diagnostics(&all_diagnostics);
    let _disposers = disposers;
    (report.output, all_diagnostics)
}

/// Resolves export duration for a `BuildTarget` (single or multi-scene).
#[cfg(feature = "video")]
fn resolve_duration(
    duration: Option<f32>,
    target: &BuildTarget,
    hold: f32,
    min_duration: f32,
) -> f32 {
    duration.unwrap_or_else(|| {
        let d = target.duration_s() as f32 + hold.max(0.0);
        d.max(min_duration)
    })
}

/// Resolution declared by the file via top-level `config { resolution: (w, h) }`,
/// used as the default export canvas when `--width`/`--height` are omitted.
fn configured_resolution(target: &BuildTarget) -> Option<(u32, u32)> {
    match target {
        BuildTarget::SingleScene(timeline) => timeline.resolution(),
        BuildTarget::MultiScene(composition) => {
            composition.scenes.values().next().and_then(|scene| scene.timeline.resolution())
        },
    }
}

/// Automatically select a diverse set of keyframe beat timestamps and the resting frame.
fn compute_auto_storyboard_times(target: &BuildTarget, max_frames: usize) -> Vec<f64> {
    let max_frames = max_frames.max(2);
    let (mut key_times, duration_s) = match target {
        BuildTarget::SingleScene(timeline) => {
            let kt: Vec<f64> = timeline.keyframe_times_s().into_iter().collect();
            (kt, timeline.playback_duration_seconds())
        },
        BuildTarget::MultiScene(comp) => {
            let mut kt = Vec::new();
            for (name, scene) in &comp.scenes {
                let start_s = comp.scene_start_times.get(name).copied().unwrap_or(0.0);
                kt.push(start_s);
                for t in scene.timeline.keyframe_times_s() {
                    kt.push(start_s + t);
                }
            }
            (kt, comp.global_duration_s)
        },
    };

    if duration_s <= 0.001 {
        return vec![0.0];
    }

    key_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    key_times.dedup_by(|a, b| (*a - *b).abs() < 0.001);

    if key_times.is_empty() {
        let step = duration_s / (max_frames - 1) as f64;
        return (0..max_frames).map(|i| i as f64 * step).collect();
    }

    let step = duration_s / (max_frames - 1) as f64;
    let mut selected = Vec::with_capacity(max_frames);
    selected.push(0.0);

    let mut used_keys = std::collections::HashSet::new();

    for i in 1..(max_frames - 1) {
        let ideal_t = i as f64 * step;
        let mut best_k_idx = None;
        let mut best_key = None;
        let mut best_dist = step * 0.40;

        for (k_idx, &kt) in key_times.iter().enumerate() {
            if kt <= 0.05 || kt >= duration_s - 0.05 || used_keys.contains(&k_idx) {
                continue;
            }
            let dist = (kt - ideal_t).abs();
            if dist < best_dist {
                best_dist = dist;
                best_key = Some(kt);
                best_k_idx = Some(k_idx);
            }
        }

        if let Some(idx) = best_k_idx {
            used_keys.insert(idx);
        }

        selected.push(best_key.unwrap_or(ideal_t));
    }

    selected.push(duration_s);
    selected.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    selected.dedup_by(|a, b| (*a - *b).abs() < 0.02);

    while selected.len() < max_frames {
        let mut max_gap = 0.0;
        let mut insert_idx = 0;
        let mut midpoint = 0.0;
        for i in 0..(selected.len() - 1) {
            let gap = selected[i + 1] - selected[i];
            if gap > max_gap {
                max_gap = gap;
                insert_idx = i + 1;
                midpoint = selected[i] + gap / 2.0;
            }
        }
        if max_gap <= 0.05 {
            break;
        }
        selected.insert(insert_idx, midpoint);
    }

    selected
}

/// Generates a timestamped default filename when `--output` is omitted.
#[cfg(feature = "video")]
fn default_output_file(ext: &str) -> PathBuf {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    // Simple unix timestamp: animatix_1234567890.png
    PathBuf::from(format!("animatix_{}.{}", secs, ext))
}

/// Render a single frame at time=0 to catch renderer bugs early.
fn run_render_smoke(target: &BuildTarget) -> Result<(), String> {
    use animatix::timeline::{DebugRenderOptions, SceneDimensions};
    use animatix_render::offscreen::OffscreenRenderer;

    let mut renderer = OffscreenRenderer::new().map_err(|e| e.to_string())?;
    let dims = SceneDimensions {
        width: 320,
        height: 180,
    };

    match target {
        BuildTarget::SingleScene(timeline) => {
            renderer
                .render_timeline_with_debug(timeline, 0.0, dims, DebugRenderOptions::default())
                .map_err(|e| e.to_string())?;
        },
        BuildTarget::MultiScene(composition) => {
            if !composition.has_scenes() {
                return Err("Composition has no scenes to render".into());
            }
            let (scene_name, local_time_s, transition_blend) = composition.evaluate(0.0);
            if let Some(blend) = transition_blend {
                let from_scene = composition
                    .scenes
                    .get(&blend.from_scene)
                    .ok_or_else(|| format!("From scene '{}' not found", blend.from_scene))?;
                let to_scene = composition
                    .scenes
                    .get(&blend.to_scene)
                    .ok_or_else(|| format!("To scene '{}' not found", blend.to_scene))?;
                renderer
                    .render_transition(
                        &from_scene.timeline,
                        blend.from_local,
                        &to_scene.timeline,
                        blend.to_local,
                        blend.progress as f32,
                        blend.id.clone(),
                        blend.easing,
                        dims,
                        DebugRenderOptions::default(),
                    )
                    .map_err(|e| e.to_string())?;
            } else {
                let scene = composition
                    .scenes
                    .get(&scene_name)
                    .ok_or_else(|| format!("Scene '{}' not found", scene_name))?;
                renderer
                    .render_timeline_with_debug(
                        &scene.timeline,
                        local_time_s,
                        dims,
                        DebugRenderOptions::default(),
                    )
                    .map_err(|e| e.to_string())?;
            }
        },
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// ----------------------------------------------------------------------------

fn main() {
    let args = Args::parse();

    let filter = match args.verbose {
        0 => EnvFilter::new("warn"),
        1 => EnvFilter::new("info,animatix=debug"),
        2 => EnvFilter::new("info,animatix=trace"),
        _ => EnvFilter::new("trace"),
    };
    let filter = if let Ok(env_filter) = std::env::var("RUST_LOG") {
        EnvFilter::new(env_filter)
    } else {
        filter
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(!args.no_color)
        .init();

    let extensions = match load_cli_extensions(&args.plugin) {
        Ok(extensions) => extensions,
        Err(e) => {
            error!("{e}");
            std::process::exit(2);
        },
    };

    match args.command {
        #[cfg(feature = "video")]
        Commands::Gif {
            input,
            width,
            height,
            fps,
            duration,
            hold,
            output,
            debug_bounds,
            threads,
            export_preset,
            set,
        } => {
            info!("Rendering Animatix GIF: {}", input.display());
            let (mut width, mut height, mut fps) = (width, height, fps);
            if let Some(name) = export_preset.as_deref() {
                let Some(preset_values) = export::ExportPreset::by_name(name) else {
                    error!("Unknown export preset '{name}'");
                    std::process::exit(2);
                };
                width = Some(preset_values.width);
                height = Some(preset_values.height);
                fps = preset_values.fps;
            }
            let (target, _) = load_and_build(&input, &extensions, &set);
            if export_preset.is_none() {
                let configured = match &target {
                    BuildTarget::SingleScene(timeline) => timeline.export_preset(),
                    BuildTarget::MultiScene(composition) => composition
                        .scenes
                        .values()
                        .next()
                        .and_then(|scene| scene.timeline.export_preset()),
                };
                if let Some(name) = configured {
                    let Some(preset_values) = export::ExportPreset::by_name(name) else {
                        error!("Unknown export preset '{name}' from config");
                        std::process::exit(2);
                    };
                    width = Some(preset_values.width);
                    height = Some(preset_values.height);
                    fps = preset_values.fps;
                }
            }
            let configured_resolution = configured_resolution(&target);
            let width = width.or(configured_resolution.map(|(w, _)| w)).unwrap_or(640);
            let height = height.or(configured_resolution.map(|(_, h)| h)).unwrap_or(360);
            let effective_duration = resolve_duration(duration, &target, hold, 0.5);
            let output_file = output.unwrap_or_else(|| default_output_file("gif"));
            info!(
                "Output configuration: {}x{} at {} FPS for {:.2}s -> {}",
                width,
                height,
                fps,
                effective_duration,
                output_file.display()
            );
            let result = match &target {
                BuildTarget::MultiScene(comp) => export::render_gif_composition_with_settings(
                    comp,
                    width,
                    height,
                    fps,
                    effective_duration,
                    &output_file,
                    DebugRenderOptions {
                        compute_hit_regions: false,
                        draw_bounds: debug_bounds,
                        ..Default::default()
                    },
                    export::ExportSettings {
                        max_render_threads: threads,
                        ..Default::default()
                    },
                ),
                BuildTarget::SingleScene(timeline) => export::render_gif_timeline_with_settings(
                    timeline.clone(),
                    width,
                    height,
                    fps,
                    effective_duration,
                    &output_file,
                    DebugRenderOptions {
                        compute_hit_regions: false,
                        draw_bounds: debug_bounds,
                        ..Default::default()
                    },
                    export::ExportSettings {
                        max_render_threads: threads,
                        ..Default::default()
                    },
                ),
            };
            if let Err(e) = result {
                error!("Error: {e}");
                std::process::exit(1);
            }
        },

        #[cfg(feature = "video")]
        Commands::Video {
            input,
            width,
            height,
            fps,
            duration,
            hold,
            output,
            debug_bounds,
            threads,
            codec,
            preset,
            export_preset,
            set,
        } => {
            info!("Rendering Animatix video: {}", input.display());
            let (mut width, mut height, mut fps, mut codec, mut preset) =
                (width, height, fps, codec, preset);
            if let Some(name) = export_preset.as_deref() {
                let Some(preset_values) = export::ExportPreset::by_name(name) else {
                    error!("Unknown export preset '{name}'");
                    std::process::exit(2);
                };
                width = Some(preset_values.width);
                height = Some(preset_values.height);
                fps = preset_values.fps;
                codec = preset_values.video_codec;
                preset = preset_values.h264_preset;
            }
            let (target, _) = load_and_build(&input, &extensions, &set);
            if export_preset.is_none() {
                let configured = match &target {
                    BuildTarget::SingleScene(timeline) => timeline.export_preset(),
                    BuildTarget::MultiScene(composition) => composition
                        .scenes
                        .values()
                        .next()
                        .and_then(|scene| scene.timeline.export_preset()),
                };
                if let Some(name) = configured {
                    let Some(preset_values) = export::ExportPreset::by_name(name) else {
                        error!("Unknown export preset '{name}' from config");
                        std::process::exit(2);
                    };
                    width = Some(preset_values.width);
                    height = Some(preset_values.height);
                    fps = preset_values.fps;
                    codec = preset_values.video_codec;
                    preset = preset_values.h264_preset;
                }
            }
            let configured_resolution = configured_resolution(&target);
            let width = width.or(configured_resolution.map(|(w, _)| w)).unwrap_or(1280);
            let height = height.or(configured_resolution.map(|(_, h)| h)).unwrap_or(720);
            let effective_duration = resolve_duration(duration, &target, hold, 0.5);
            let output_file = output.unwrap_or_else(|| default_output_file("mp4"));
            info!(
                "Output configuration: {}x{} at {} FPS for {:.2}s -> {}",
                width,
                height,
                fps,
                effective_duration,
                output_file.display()
            );
            let result = match &target {
                BuildTarget::MultiScene(comp) => export::render_video_composition_with_settings(
                    comp,
                    width,
                    height,
                    fps,
                    effective_duration,
                    &output_file,
                    DebugRenderOptions {
                        compute_hit_regions: false,
                        draw_bounds: debug_bounds,
                        ..Default::default()
                    },
                    export::ExportSettings {
                        max_render_threads: threads,
                        video_codec: codec,
                        h264_preset: preset,
                    },
                ),
                BuildTarget::SingleScene(timeline) => export::render_video_timeline_with_settings(
                    timeline.clone(),
                    width,
                    height,
                    fps,
                    effective_duration,
                    &output_file,
                    DebugRenderOptions {
                        compute_hit_regions: false,
                        draw_bounds: debug_bounds,
                        ..Default::default()
                    },
                    export::ExportSettings {
                        max_render_threads: threads,
                        video_codec: codec,
                        h264_preset: preset,
                    },
                ),
            };
            if let Err(e) = result {
                error!("Error: {e}");
                std::process::exit(1);
            }
        },

        Commands::Plugin { command } => match command {
            PluginCommands::Describe { library, output } => {
                if let Err(e) = run_plugin_describe(&library, output.as_deref()) {
                    error!("{e}");
                    std::process::exit(2);
                }
            },
        },

        Commands::Timeline { input } => {
            let source = match std::fs::read_to_string(&input) {
                Ok(s) => s,
                Err(e) => {
                    error!("Cannot read {}: {}", input.display(), e);
                    std::process::exit(1);
                },
            };
            let mut module_graph = ModuleGraph::new();
            let (ast, namespaces) = match module_graph
                .load_program_with_source(std::path::Path::new(&input), Some(&source))
            {
                Ok(mut program) => {
                    let _ = program.typecheck();
                    let mut expansion_errors = Vec::new();
                    let expanded = program.expand_components(&mut expansion_errors);
                    (expanded, program.namespaces)
                },
                Err(e) => {
                    error!("Error: {}", e);
                    std::process::exit(1);
                },
            };
            let context = std::sync::Arc::new(animatix::extension_context::ExtensionContext::new());
            let report = BuildTarget::from_ast_with_context(
                &ast,
                &namespaces,
                Some(std::path::Path::new(&input)),
                context,
            );
            match report.output {
                BuildTarget::MultiScene(comp) => {
                    let summary = comp.summary();
                    println!(
                        "{:<4} {:<14} {:>8} {:>9}  transition -> next",
                        "#", "scene", "start(s)", "dur(s)"
                    );
                    for (i, (name, start, dur, explicit)) in summary.scenes.iter().enumerate() {
                        let next = summary
                            .edges
                            .iter()
                            .find(|(from, _, _, _)| from == name)
                            .map(|(_, to, id, ms)| format!("{} [{}, {}ms]", to, id, ms))
                            .unwrap_or_else(|| "-".to_string());
                        println!(
                            "{:<4} {:<14} {:>8.2} {:>9.2}  {}{}",
                            i + 1,
                            name,
                            start,
                            dur,
                            next,
                            if explicit.is_some() {
                                ""
                            } else {
                                " (inferred)"
                            }
                        );
                    }
                    println!("total: {:.2}s", summary.total_duration_s);
                },
                BuildTarget::SingleScene(_) => {
                    println!("single-timeline file (no `# Scene` declarations)");
                },
            }
        },
        Commands::Ast {
            input,
            compact,
            force: _,
        } => {
            info!("Parsing Animatix file: {}", input.display());
            let ast = match ModuleGraph::new().load_entry(&input) {
                Ok(statements) => statements,
                Err(e) => {
                    error!("Error: {}", e);
                    std::process::exit(1);
                },
            };
            if compact {
                println!("{:?}", ast);
            } else {
                for stmt in &ast {
                    println!("{:#?}", stmt);
                }
            }
        },

        Commands::Image {
            input,
            width,
            height,
            time,
            output,
            debug_bounds,
            set,
        } => {
            info!("Rendering Animatix image: {}", input.display());
            let (target, _) = load_and_build(&input, &extensions, &set);
            let configured_resolution = configured_resolution(&target);
            let width = width.or(configured_resolution.map(|(w, _)| w)).unwrap_or(1280);
            let height = height.or(configured_resolution.map(|(_, h)| h)).unwrap_or(720);
            let output_file =
                output.unwrap_or_else(|| PathBuf::from(format!("animatix_{}s.png", time)));
            info!("Output image: {}x{} at {}s -> {}", width, height, time, output_file.display());
            let result = match &target {
                BuildTarget::MultiScene(comp) => {
                    export::render_image_composition(comp, width, height, time, &output_file)
                },
                BuildTarget::SingleScene(timeline) => export::render_image_timeline_with_debug(
                    timeline.clone(),
                    width,
                    height,
                    time,
                    &output_file,
                    DebugRenderOptions {
                        compute_hit_regions: false,
                        draw_bounds: debug_bounds,
                        ..Default::default()
                    },
                ),
            };
            if let Err(e) = result {
                error!("Error: {e}");
                std::process::exit(1);
            }
        },

        Commands::Storyboard {
            input,
            set,
            cols,
            tile_width,
            tile_height,
            times,
            max_frames,
            debug_bounds,
            output,
        } => {
            info!("Generating storyboard for {}", input.display());
            let (target, _) = load_and_build(&input, &extensions, &set);
            let configured_resolution = configured_resolution(&target);
            let (default_tw, default_th) = configured_resolution
                .map(|(w, h)| ((w / 2).clamp(320, 640), (h / 2).clamp(180, 360)))
                .unwrap_or((480, 270));
            let tile_width = tile_width.unwrap_or(default_tw);
            let tile_height = tile_height.unwrap_or(default_th);
            let render_width = configured_resolution.map(|(w, _)| w);
            let render_height = configured_resolution.map(|(_, h)| h);

            let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("storyboard");
            let output_file =
                output.unwrap_or_else(|| PathBuf::from(format!("{}_storyboard.png", stem)));

            let sample_times = match times {
                Some(ts) => ts,
                None => compute_auto_storyboard_times(&target, max_frames),
            };

            let settings = export::StoryboardSettings {
                cols,
                tile_width,
                tile_height,
                render_width,
                render_height,
                times: sample_times,
                gutter: 8,
                margin: 12,
                header_height: 24,
            };

            let debug_options = DebugRenderOptions {
                compute_hit_regions: false,
                draw_bounds: debug_bounds,
                ..Default::default()
            };

            let result = match &target {
                BuildTarget::SingleScene(timeline) => export::render_storyboard_timeline(
                    timeline.clone(),
                    &settings,
                    &output_file,
                    debug_options,
                ),
                BuildTarget::MultiScene(comp) => export::render_storyboard_composition(
                    comp,
                    &settings,
                    &output_file,
                    debug_options,
                ),
            };

            match result {
                Ok(frame_count) => {
                    println!(
                        "Storyboard saved to {} ({} frames)",
                        output_file.display(),
                        frame_count
                    );
                },
                Err(e) => {
                    error!("Error: {e}");
                    std::process::exit(1);
                },
            }
        },

        Commands::Check {
            file,
            render_smoke,
            format,
            set,
        } => {
            let (source, file_label) = if file == "-" {
                let source = match std::io::read_to_string(std::io::stdin()) {
                    Ok(s) => s,
                    Err(e) => {
                        error!("Cannot read from stdin: {}", e);
                        std::process::exit(1);
                    },
                };
                (source, "-".to_string())
            } else {
                let source = match std::fs::read_to_string(&file) {
                    Ok(s) => s,
                    Err(e) => {
                        error!("Cannot read {}: {}", file, e);
                        std::process::exit(1);
                    },
                };
                (source, file.clone())
            };
            let mut module_graph = ModuleGraph::new();
            let (ast, namespaces, type_diagnostics) = match module_graph
                .load_program_with_source(std::path::Path::new(&file_label), Some(&source))
            {
                Ok(mut program) => {
                    let diagnostics = program.typecheck();
                    let mut expansion_errors = Vec::new();
                    let expanded = program.expand_components(&mut expansion_errors);
                    let mut diagnostics = diagnostics;
                    for error in expansion_errors {
                        diagnostics.push(animatix_syntax::diagnostics::Diagnostic::error(
                            animatix_syntax::diagnostics::DiagnosticCode::UnknownAction,
                            animatix_syntax::diagnostics::DiagnosticPhase::Build,
                            error,
                        ));
                    }
                    (expanded, program.namespaces, diagnostics)
                },
                Err(e) => {
                    // Report the parse failure through the normal diagnostic
                    // channel (stdout, honouring --format) instead of exiting
                    // on a tracing line. The branch only fires when the module
                    // graph could not parse, so the whole report is the parse
                    // failure — running the analyzer too would duplicate every
                    // error, since it re-parses the same source.
                    let parse_diagnostics = module_error_diagnostics(&e, &source);
                    emit_check_report(&file_label, &source, parse_diagnostics, Vec::new(), &format);
                    // Every parse failure is an error, so this always exits 1.
                    std::process::exit(1);
                },
            };
            let mut ctx = animatix::extension_context::ExtensionContext::new();
            let disposers = match extensions.install_into(&mut ctx) {
                Ok(disposers) => disposers,
                Err(e) => {
                    error!("Plugin install failed: {e}");
                    std::process::exit(1);
                },
            };
            let context = std::sync::Arc::new(ctx);
            let report = BuildTarget::from_ast_with_context_and_defines(
                &ast,
                &namespaces,
                if file_label == "-" {
                    None
                } else {
                    Some(std::path::Path::new(&file_label))
                },
                context,
                &set,
            );
            let _disposers = disposers;
            let mut diagnostics = type_diagnostics;
            diagnostics.extend(report.diagnostics);

            if render_smoke {
                if let Err(e) = run_render_smoke(&report.output) {
                    diagnostics.push(Diagnostic::error(
                        DiagnosticCode::RenderFailure,
                        DiagnosticPhase::Render,
                        format!("Render smoke test failed: {e}"),
                    ));
                }
            }

            // Run semantic analysis (mirrors lint command behavior)
            let mut analyzer = animatix_analyzer::Analyzer::new_with_path(
                &source,
                if file_label == "-" {
                    None
                } else {
                    Some(std::path::PathBuf::from(&file_label))
                },
            )
            .with_extension_manifest(extensions.merged_manifest());
            analyzer.merge_import_symbols();
            let lint_config = animatix_analyzer::LintConfig::from_source(&source);
            let semantic = analyzer.diagnostics_with_config(&lint_config);

            // Build-layer diagnostics that name an actor but carry no position
            // (they are produced from the runtime track table, which has no
            // source spans) are anchored at that actor's declaration, so the
            // report points somewhere useful instead of at 1:1.
            resolve_build_diagnostic_positions(&mut diagnostics, &analyzer);

            emit_check_report(&file_label, &source, diagnostics, semantic, &format);
        },

        Commands::Verify {
            input,
            checks,
            width,
            height,
            set,
            format,
        } => {
            let checks_path = checks.unwrap_or_else(|| {
                input.parent().unwrap_or_else(|| Path::new(".")).join("verify.txt")
            });
            if !checks_path.exists() {
                error!(
                    "Checks file not found: {} (pass --checks or add verify.txt beside the input)",
                    checks_path.display()
                );
                std::process::exit(1);
            }
            let specs = match parse_verify_checks(&checks_path) {
                Ok(specs) => specs,
                Err(e) => {
                    error!("{e}");
                    std::process::exit(1);
                },
            };
            let (target, _) = load_and_build(&input, &extensions, &set);
            let configured = configured_resolution(&target);
            let dimensions = SceneDimensions {
                width: width.or(configured.map(|(w, _)| w)).unwrap_or(1280),
                height: height.or(configured.map(|(_, h)| h)).unwrap_or(720),
            };
            let mut renderer = match animatix_render::offscreen::OffscreenRenderer::new() {
                Ok(renderer) => renderer,
                Err(e) => {
                    error!("Failed to create offscreen renderer: {e}");
                    std::process::exit(1);
                },
            };
            let outcomes = match run_verify_checks(&mut renderer, &target, &specs, dimensions) {
                Ok(outcomes) => outcomes,
                Err(e) => {
                    error!("Verification could not render: {e}");
                    std::process::exit(1);
                },
            };
            let failed = outcomes.iter().filter(|outcome| !outcome.passed).count();
            match format {
                OutputFormat::Json => {
                    let results: Vec<serde_json::Value> = outcomes
                        .iter()
                        .map(|outcome| {
                            serde_json::json!({
                                "line": outcome.line,
                                "check": outcome.raw,
                                "passed": outcome.passed,
                                "detail": outcome.detail,
                            })
                        })
                        .collect();
                    let value = serde_json::json!({
                        "input": input.display().to_string(),
                        "passed": failed == 0,
                        "failed": failed,
                        "total": outcomes.len(),
                        "results": results,
                    });
                    println!("{value}");
                },
                OutputFormat::Text => {
                    println!("verify: {}", input.display());
                    for outcome in &outcomes {
                        let status = if outcome.passed { "PASS" } else { "FAIL" };
                        println!(
                            "  L{:<4} {:<4} {} — {}",
                            outcome.line, status, outcome.raw, outcome.detail
                        );
                    }
                    println!(
                        "verify: {} of {} checks passed",
                        outcomes.len() - failed,
                        outcomes.len()
                    );
                },
            }
            if failed > 0 {
                std::process::exit(1);
            }
        },

        Commands::Fmt {
            paths,
            check,
            indent,
        } => {
            let config = animatix_syntax::formatter::FormatConfig {
                indent_size: indent,
                ..Default::default()
            };
            let formatter = animatix_syntax::formatter::Formatter::new(config);
            let mut has_changes = false;

            for path in &paths {
                if path.is_dir() {
                    // Format all .amx files in directory
                    for entry in walkdir::WalkDir::new(path)
                        .into_iter()
                        .filter_map(|e| e.ok())
                        .filter(|e| e.path().extension().is_some_and(|ext| ext == "amx"))
                    {
                        if let Err(e) = format_file(entry.path(), &formatter, check) {
                            error!("{}: {}", entry.path().display(), e);
                            has_changes = true;
                        }
                    }
                } else {
                    if let Err(e) = format_file(path, &formatter, check) {
                        error!("{}: {}", path.display(), e);
                        has_changes = true;
                    }
                }
            }

            if check && has_changes {
                error!("Some files are not formatted. Run 'animatix fmt' to fix.");
                std::process::exit(1);
            }
        },

        Commands::Lint {
            paths,
            format,
            deny_warnings,
            config,
        } => {
            // Load lint config from file if specified
            let file_config = config
                .as_ref()
                .map(|p| animatix_analyzer::LintConfig::from_file(p))
                .unwrap_or_default();

            let mut total_errors = 0;
            let mut total_warnings = 0;
            let mut all_diagnostics = Vec::new();

            for path in &paths {
                let files = if path.is_dir() {
                    walkdir::WalkDir::new(path)
                        .into_iter()
                        .filter_map(|e| e.ok())
                        .filter(|e| e.path().extension().is_some_and(|ext| ext == "amx"))
                        .map(|e| e.path().to_path_buf())
                        .collect::<Vec<_>>()
                } else {
                    vec![path.clone()]
                };

                for file in files {
                    let source = match std::fs::read_to_string(&file) {
                        Ok(s) => s,
                        Err(e) => {
                            error!("{}: Failed to read: {}", file.display(), e);
                            total_errors += 1;
                            continue;
                        },
                    };

                    let mut analyzer =
                        animatix_analyzer::Analyzer::new_with_path(&source, Some(file.clone()))
                            .with_extension_manifest(extensions.merged_manifest());
                    analyzer.merge_import_symbols();
                    // Merge inline config with file config
                    let mut config = animatix_analyzer::LintConfig::from_source(&source);
                    config.merge(&file_config);
                    let diagnostics = analyzer.diagnostics_with_config(&config);

                    if !diagnostics.is_empty() {
                        match format {
                            OutputFormat::Text => {
                                let (shown, folded) = fold_hints(&diagnostics);
                                for diag in shown {
                                    println!("{}:{}", file.display(), diag);
                                }
                                if folded > 0 {
                                    println!(
                                        "{}: {folded} hint(s) folded — --format json lists them",
                                        file.display()
                                    );
                                }
                            },
                            OutputFormat::Json => {
                                for diag in &diagnostics {
                                    // Same per-diagnostic shape as `check
                                    // --format json`, plus the file it came
                                    // from; lint sweeps many files.
                                    let mut value = semantic_diagnostic_to_json(diag);
                                    if let Some(object) = value.as_object_mut() {
                                        object.insert(
                                            "file".to_string(),
                                            serde_json::Value::String(file.display().to_string()),
                                        );
                                    }
                                    all_diagnostics.push(value);
                                }
                            },
                        }

                        total_errors += diagnostics.iter().filter(|d| d.is_error()).count();
                        total_warnings += diagnostics.iter().filter(|d| d.is_warning()).count();
                    }
                }
            }

            match format {
                OutputFormat::Json => match serde_json::to_string_pretty(&all_diagnostics) {
                    Ok(json) => println!("{json}"),
                    Err(err) => {
                        tracing::error!("Failed to serialize diagnostics JSON: {err}");
                        std::process::exit(1);
                    },
                },
                OutputFormat::Text => {
                    if total_errors > 0 || total_warnings > 0 {
                        println!("\n{} error(s), {} warning(s)", total_errors, total_warnings);
                    }
                },
            }

            if total_errors > 0 || (deny_warnings && total_warnings > 0) {
                std::process::exit(1);
            }
        },
    }
}

/// Anchor build diagnostics that name an actor but carry no position.
///
/// `never-revealed` and friends are produced while walking the runtime track
/// table, which keeps no source spans, so they reach the CLI with no location.
/// Their message quotes the actor label in backticks; when the analyzer can
/// resolve that label to a declaration, the diagnostic is pointed there.
fn resolve_build_diagnostic_positions(
    diagnostics: &mut [Diagnostic],
    analyzer: &animatix_analyzer::Analyzer,
) {
    for diagnostic in diagnostics.iter_mut() {
        if diagnostic.location.line.is_some() {
            continue;
        }
        let Some(label) = backticked_label(&diagnostic.message) else {
            continue;
        };
        let Some((line, col, end_line, end_col)) = analyzer.declaration_range(&label) else {
            continue;
        };
        // Diagnostics are 1-based on this side; the analyzer reports 0-based.
        diagnostic.location.line = Some(line + 1);
        diagnostic.location.column = Some(col + 1);
        diagnostic.location.end_line = Some(end_line + 1);
        diagnostic.location.end_col = Some(end_col + 1);
    }
}

/// Extract the first `` `label` `` from a diagnostic message.
fn backticked_label(message: &str) -> Option<String> {
    let start = message.find('`')? + 1;
    let rest = &message[start..];
    let end = rest.find('`')?;
    let label = &rest[..end];
    (!label.is_empty() && !label.contains(' ')).then(|| label.to_string())
}

/// Format a single .amx file.
///
/// If `check` is true, only checks if the file is formatted (doesn't modify).
/// Returns Ok(()) if the file is already formatted or was formatted successfully.
fn format_file(
    path: &Path,
    formatter: &animatix_syntax::formatter::Formatter,
    check: bool,
) -> Result<(), String> {
    let source = std::fs::read_to_string(path).map_err(|e| format!("Failed to read: {}", e))?;

    let (stmts, errors) = animatix_syntax::parser::parse_simple(&source);
    let stmts = stmts.ok_or_else(|| format!("Parse error: {:?}", errors))?;

    let formatted = formatter.format(&stmts);

    if check {
        if source != formatted {
            return Err("File is not formatted".into());
        }
    } else {
        if source != formatted {
            std::fs::write(path, &formatted).map_err(|e| format!("Failed to write: {}", e))?;
            info!("Formatted: {}", path.display());
        }
    }

    Ok(())
}

/// Serializes a single build diagnostic as a JSON value.
///
/// Both diagnostic families go through this shape so a consumer parses one
/// schema: the same keys are present whether the diagnostic came from the
/// build or from the analyzer, with `phase` naming which.
fn diagnostic_to_json(d: &Diagnostic) -> serde_json::Value {
    serde_json::json!({
        "line": d.location.line,
        "col": d.location.column,
        "end_line": d.location.end_line,
        "end_col": d.location.end_col,
        "message": d.message,
        "code": d.code.to_string(),
        "severity": d.severity.to_string(),
        "phase": d.phase.to_string(),
    })
}

/// Serializes a single analyzer (semantic) diagnostic as a JSON value.
///
/// Uses the same keys as [`diagnostic_to_json`]; the analyzer's checks run
/// alongside the build, which is what `phase` reports.
fn semantic_diagnostic_to_json(d: &animatix_analyzer::Diagnostic) -> serde_json::Value {
    serde_json::json!({
        "line": d.line,
        "col": d.col,
        "end_line": d.end_line,
        "end_col": d.end_col,
        "message": d.message,
        "code": d.code,
        "severity": format!("{:?}", d.severity).to_lowercase(),
        "phase": "build",
    })
}

fn print_build_diagnostics(diagnostics: &[Diagnostic]) {
    for diagnostic in diagnostics {
        let formatted = format_diagnostic(diagnostic);
        if diagnostic.is_error() {
            error!("{}", formatted);
        } else {
            warn!("{}", formatted);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_and_lint_json_share_one_diagnostic_shape() {
        // Consumers parse both commands with one schema, so a diagnostic from
        // either family must carry the same keys (`code`, `severity`, `phase`).
        let build = diagnostic_to_json(&Diagnostic::error(
            DiagnosticCode::ParseError,
            DiagnosticPhase::Parse,
            "boom",
        ));
        for key in [
            "line", "col", "end_line", "end_col", "message", "code", "severity", "phase",
        ] {
            assert!(build.get(key).is_some(), "build diagnostic is missing `{key}`");
        }

        let semantic = semantic_diagnostic_to_json(&animatix_analyzer::Diagnostic {
            severity: animatix_analyzer::DiagnosticSeverity::Warning,
            line: 3,
            col: 4,
            end_line: 3,
            end_col: 9,
            message: "Unused actor: 'x'".to_string(),
            code: Some("unused-label".to_string()),
        });
        for key in [
            "line", "col", "end_line", "end_col", "message", "code", "severity", "phase",
        ] {
            assert!(semantic.get(key).is_some(), "semantic diagnostic is missing `{key}`");
        }
        assert_eq!(semantic["severity"], "warning");
        assert_eq!(semantic["phase"], "build");
        assert_eq!(semantic["line"], 3);
    }

    #[test]
    fn module_parse_errors_become_structured_diagnostics() {
        // A module that fails to parse must produce one diagnostic per parse
        // error, tagged with the stable parse-error code — not a bare tracing
        // line, which `--format json` could not surface.
        let (ast, errors) =
            animatix_syntax::parser::parse_source("title: Text {\n  text: \"x\"\n#0s\n");
        assert!(ast.is_none() || !errors.is_empty(), "source must be malformed");
        let source = "title: Text {\n  text: \"x\"\n#0s\n";
        let err = animatix_syntax::module::ModuleError::ParseErrors(errors);
        let diagnostics = module_error_diagnostics(&err, source);
        assert!(!diagnostics.is_empty(), "parse failure must yield diagnostics");
        assert!(
            diagnostics.iter().all(|d| d.code == DiagnosticCode::ParseError && d.is_error()),
            "every parse diagnostic is a parse-error: {diagnostics:?}"
        );
    }

    #[test]
    fn non_parse_module_errors_become_one_diagnostic() {
        let err = animatix_syntax::module::ModuleError::FileNotFound(PathBuf::from("/nope.amx"));
        let diagnostics = module_error_diagnostics(&err, "");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].is_error());
        assert!(diagnostics[0].message.contains("/nope.amx"));
    }

    #[test]
    fn plugin_paths_are_classified() {
        assert!(is_manifest_path(Path::new("demo.amx-plugin.toml")));
        assert!(!is_manifest_path(Path::new("demo.toml")));
        assert!(is_native_library_path(Path::new("libdemo.so")));
        assert!(is_native_library_path(Path::new("libdemo.dylib")));
        assert!(is_native_library_path(Path::new("demo.dll")));
        assert!(!is_native_library_path(Path::new("demo.txt")));
    }

    #[test]
    fn relative_path_uses_common_ancestor() {
        let dir = std::env::temp_dir().join(format!(
            "animatix-cli-relative-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let from = dir.join("nested");
        let target = dir.join("libs").join("libdemo.so");
        std::fs::create_dir_all(&from).expect("create from dir");
        std::fs::create_dir_all(target.parent().expect("target parent"))
            .expect("create target dir");
        std::fs::write(&target, b"demo").expect("write target");

        assert_eq!(
            animatix_plugin_tooling::relative_path(&from, &target),
            Some(PathBuf::from("../libs/libdemo.so"))
        );

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn load_cli_extensions_reads_manifest_without_native_library() {
        let dir = std::env::temp_dir().join(format!(
            "animatix-cli-plugin-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let manifest_path = dir.join("demo.amx-plugin.toml");
        std::fs::write(
            &manifest_path,
            "[[primitives]]\ntype_name = \"Gauge\"\n\n[[properties]]\nactor_type = \"Gauge\"\nname = \"level\"\ntype = \"Num\"\n",
        )
        .expect("write manifest");

        let extensions =
            load_cli_extensions(std::slice::from_ref(&manifest_path)).expect("load manifest");
        assert_eq!(extensions.manifests.len(), 1);
        assert_eq!(extensions.manifests[0].primitives[0].type_name, "Gauge");
        assert_eq!(
            extensions.merged_manifest().properties[0].name,
            "level",
            "manifest should be usable by the analyzer"
        );

        std::fs::remove_dir_all(dir).ok();
    }
    #[test]
    fn backticked_label_extracts_actor_names() {
        assert_eq!(
            backticked_label("Actor `title` is declared before any keyframe"),
            Some("title".to_string())
        );
        // A label is a single token; a backticked phrase is not a label.
        assert_eq!(backticked_label("see `the docs here` for details"), None);
        assert_eq!(backticked_label("no backticks at all"), None);
        assert_eq!(backticked_label("empty `` label"), None);
    }

    #[test]
    fn unpositioned_build_diagnostics_get_anchored() {
        // A `never-revealed` warning reaches the CLI with no location; the
        // analyzer resolves the quoted label to its declaration.
        let source = "title: Text, text: \"hi\"\n#0s\nfade-in title [1s]\n";
        let analyzer = animatix_analyzer::Analyzer::new(source);
        let mut diagnostics = vec![Diagnostic::warning(
            DiagnosticCode::NeverRevealed,
            DiagnosticPhase::Build,
            "Actor `title` is declared before any keyframe".to_string(),
        )];
        assert!(diagnostics[0].location.line.is_none(), "starts unpositioned");

        resolve_build_diagnostic_positions(&mut diagnostics, &analyzer);

        assert_eq!(diagnostics[0].location.line, Some(1), "anchored to the declaration");
        assert_eq!(diagnostics[0].location.column, Some(1));
    }

    #[test]
    fn positioned_diagnostics_are_left_alone() {
        let source = "title: Text, text: \"hi\"\n";
        let analyzer = animatix_analyzer::Analyzer::new(source);
        let mut diagnostics = vec![
            Diagnostic::warning(
                DiagnosticCode::NeverRevealed,
                DiagnosticPhase::Build,
                "Actor `title` is declared before any keyframe".to_string(),
            )
            .with_location(9, 9, 0..1),
        ];

        resolve_build_diagnostic_positions(&mut diagnostics, &analyzer);

        assert_eq!(diagnostics[0].location.line, Some(9), "existing position is respected");
    }

    #[test]
    fn auto_storyboard_times_sample_start_cues_and_rest() {
        let source = r#"
#0s
box: Rect, at: (0, 0), size: (10, 10)
#1s
fade-in box [500ms]
#3s
fade-out box [500ms]
"#;
        let (ast, errors) = animatix_syntax::parser::parse_source(source);
        assert!(errors.is_empty());
        let report = animatix::timeline::Timeline::build_with_diagnostics(
            &ast.unwrap(),
            &std::collections::HashMap::new(),
        );
        let target = BuildTarget::SingleScene(report.output);
        let times = compute_auto_storyboard_times(&target, 8);
        assert!(!times.is_empty());
        assert_eq!(times[0], 0.0, "must include start");
        assert!(times.contains(&1.0), "must include intermediate keyframe");
        assert!(times.last().copied().unwrap() >= 3.0, "must include end/rest frame");
    }
}
