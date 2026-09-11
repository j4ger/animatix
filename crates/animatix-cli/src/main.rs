use std::path::{Path, PathBuf};

use animatix::composition::BuildTarget;
use animatix::extension_plugin::{NativePlugin, PluginDisposer, PluginLoader};
use animatix::renderer;
use animatix::timeline::{DebugRenderOptions, SceneDimensions};
use animatix::verify::{self, Box2, FrameView};
use animatix_analyzer::ExtensionManifest;
use animatix_syntax::diagnostics::{
    Diagnostic, DiagnosticCode, DiagnosticPhase, format_diagnostic, format_diagnostic_with_source,
};
use animatix_syntax::module::ModuleGraph;
use clap::{Parser as ClapParser, Subcommand, ValueEnum};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

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
    /// Render a scene to a video file
    #[cfg(feature = "video")]
    Video {
        /// The input Animatix scene file (.amx)
        input: PathBuf,

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
        threads: renderer::MaxRenderThreads,

        /// Video codec: auto, libx264, h264_nvenc, h264_vaapi, vp9
        #[arg(long, default_value = "auto")]
        codec: renderer::VideoCodec,

        /// libx264 preset: ultrafast, superfast, veryfast, faster, fast, medium, slow, slower,
        /// veryslow
        #[arg(long, default_value = "medium")]
        preset: renderer::H264Preset,

        /// Named export preset (720p30, 1080p30, 1080p60, 4k30). Overrides width/height/fps/codec/preset.
        #[arg(long)]
        export_preset: Option<String>,
    },
    /// Render a scene to an animated GIF file
    #[cfg(feature = "video")]
    Gif {
        /// The input Animatix scene file (.amx)
        input: PathBuf,

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
        threads: renderer::MaxRenderThreads,

        /// Named export preset (720p30, 1080p30, 1080p60, 4k30). Overrides width/height/fps.
        #[arg(long)]
        export_preset: Option<String>,
    },
    /// Parse an .amx file and print build diagnostics
    Check {
        /// Path to the .amx file (use "-" for stdin)
        file: String,

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

/// Loads an Animatix program from disk, expands components, and builds the
/// appropriate target (single-scene `Timeline` or multi-scene `Composition`).
/// Prints build diagnostics and exits on load failure.
fn load_and_build(
    input: &Path,
    extensions: &CliExtensions,
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
    let report = BuildTarget::from_ast_with_context(&ast, &namespaces, Some(input), context);
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
    use animatix::renderer::offscreen::OffscreenRenderer;
    use animatix::timeline::{DebugRenderOptions, SceneDimensions};

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
// Content-level verification (`animatix verify`)
// ---------------------------------------------------------------------------

/// One parsed line from a `verify.txt` checks file.
#[derive(Debug)]
struct VerifyCheck {
    kind: VerifyKind,
    time_s: f64,
    second_time_s: Option<f64>,
    label: Option<String>,
    threshold: Option<f64>,
    line: usize,
    raw: String,
}

/// The four supported check kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VerifyKind {
    /// The actor put ink on screen inside its evaluated bounds.
    Visible,
    /// The actor did not (e.g. a hidden-by-default declaration never revealed).
    Invisible,
    /// The whole frame is not blank.
    Ink,
    /// Two times render differently — the animation actually animates.
    Differs,
    /// The actor's region changed between a hidden time and a shown time — the
    /// robust "did it actually appear?" check for actors whose bounds contain
    /// other painted content (e.g. a curve inside a Graph that paints axes).
    Reveals,
}

fn verify_error(path: &Path, line_no: usize, line: &str, msg: &str) -> String {
    format!("{}:{line_no}: {msg} (in `{line}`)", path.display())
}

/// Parse a line-based checks file. One check per line following the grammar
/// documented in `dogfood/README.md`; blank lines and `#` comments are ignored.
fn parse_verify_checks(path: &Path) -> Result<Vec<VerifyCheck>, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("Cannot read checks file {}: {e}", path.display()))?;
    let mut checks = Vec::new();
    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line_no = idx + 1;
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let parse_time = |token: &str| -> Result<f64, String> {
            token.parse::<f64>().map_err(|_| {
                verify_error(path, line_no, line, &format!("`{token}` is not a number"))
            })
        };
        let mut check = VerifyCheck {
            kind: match tokens[0] {
                "visible" => VerifyKind::Visible,
                "invisible" => VerifyKind::Invisible,
                "ink" => VerifyKind::Ink,
                "differs" => VerifyKind::Differs,
                "reveals" => VerifyKind::Reveals,
                other => {
                    return Err(verify_error(
                        path,
                        line_no,
                        line,
                        &format!(
                            "unknown check kind `{other}` (expected visible|invisible|reveals|ink|differs)"
                        ),
                    ));
                },
            },
            time_s: 0.0,
            second_time_s: None,
            label: None,
            threshold: None,
            line: line_no,
            raw: line.to_string(),
        };
        match check.kind {
            VerifyKind::Visible | VerifyKind::Invisible => {
                if tokens.len() != 3 {
                    return Err(verify_error(
                        path,
                        line_no,
                        line,
                        "expected `<kind> <time_s> <actor-label>`",
                    ));
                }
                check.time_s = parse_time(tokens[1])?;
                check.label = Some(tokens[2].to_string());
            },
            VerifyKind::Ink => {
                if tokens.len() != 3 {
                    return Err(verify_error(
                        path,
                        line_no,
                        line,
                        "expected `ink <time_s> <min_frame_ink_fraction>`",
                    ));
                }
                check.time_s = parse_time(tokens[1])?;
                check.threshold = Some(tokens[2].parse().map_err(|_| {
                    verify_error(path, line_no, line, &format!("`{}` is not a number", tokens[2]))
                })?);
            },
            VerifyKind::Differs => {
                if tokens.len() < 3 || tokens.len() > 4 {
                    return Err(verify_error(
                        path,
                        line_no,
                        line,
                        "expected `differs <t1> <t2> [min_changed_fraction]`",
                    ));
                }
                check.time_s = parse_time(tokens[1])?;
                check.second_time_s = Some(parse_time(tokens[2])?);
                if let Some(token) = tokens.get(3) {
                    check.threshold = Some(token.parse().map_err(|_| {
                        verify_error(path, line_no, line, &format!("`{token}` is not a number"))
                    })?);
                }
            },
            VerifyKind::Reveals => {
                if tokens.len() < 4 || tokens.len() > 5 {
                    return Err(verify_error(
                        path,
                        line_no,
                        line,
                        "expected `reveals <actor-label> <t_before> <t_after> [min_changed_fraction]`",
                    ));
                }
                check.label = Some(tokens[1].to_string());
                check.time_s = parse_time(tokens[2])?;
                check.second_time_s = Some(parse_time(tokens[3])?);
                if let Some(token) = tokens.get(4) {
                    check.threshold = Some(token.parse().map_err(|_| {
                        verify_error(path, line_no, line, &format!("`{token}` is not a number"))
                    })?);
                }
            },
        }
        checks.push(check);
    }
    if checks.is_empty() {
        return Err(format!("{}: no checks found", path.display()));
    }
    Ok(checks)
}

/// A rendered verification frame: pixels plus the per-actor bounds from the
/// same evaluation.
struct VerifyFrame {
    frame: animatix::renderer::offscreen::RenderedFrame,
    bounds: std::collections::HashMap<String, Box2>,
}

fn verify_time_key(time_s: f64) -> u64 {
    (time_s * 1000.0).round() as u64
}

/// Render one frame at global `time_s`, resolving the active scene (and any
/// transition blend) for multi-scene files.
fn render_verify_frame(
    renderer: &mut animatix::renderer::offscreen::OffscreenRenderer,
    target: &BuildTarget,
    dimensions: SceneDimensions,
    time_s: f64,
) -> Result<VerifyFrame, String> {
    match target {
        BuildTarget::SingleScene(timeline) => {
            let (frame, program) = renderer.render_timeline_observable(
                timeline,
                time_s,
                dimensions,
                DebugRenderOptions::default(),
            )?;
            Ok(VerifyFrame {
                frame,
                bounds: verify::bounds_map(&program),
            })
        },
        BuildTarget::MultiScene(composition) => {
            if !composition.has_scenes() {
                return Err("Composition has no scenes to render".into());
            }
            let (scene_name, local_time_s, blend) = composition.evaluate(time_s);
            if let Some(blend) = blend {
                let from_scene = composition
                    .scenes
                    .get(&blend.from_scene)
                    .ok_or_else(|| format!("From scene '{}' not found", blend.from_scene))?;
                let to_scene = composition
                    .scenes
                    .get(&blend.to_scene)
                    .ok_or_else(|| format!("To scene '{}' not found", blend.to_scene))?;
                let frame = renderer.render_transition(
                    &from_scene.timeline,
                    blend.from_local,
                    &to_scene.timeline,
                    blend.to_local,
                    blend.progress as f32,
                    blend.id.clone(),
                    blend.easing,
                    dimensions,
                    DebugRenderOptions::default(),
                )?;
                // The compositor does not hand back bounds, so a `visible`
                // check during a blend consults both scenes and accepts a hit
                // in either (the actor may be in either side of the crossfade).
                let mut fb = None;
                let from_program = from_scene.timeline.evaluate_program_with_debug(
                    blend.from_local,
                    dimensions,
                    DebugRenderOptions::default(),
                    &mut fb,
                );
                let mut bounds = verify::bounds_map(&from_program);
                let mut fb = None;
                let to_program = to_scene.timeline.evaluate_program_with_debug(
                    blend.to_local,
                    dimensions,
                    DebugRenderOptions::default(),
                    &mut fb,
                );
                for (label, rect) in verify::bounds_map(&to_program) {
                    bounds.entry(label).or_insert(rect);
                }
                Ok(VerifyFrame { frame, bounds })
            } else {
                let scene = composition
                    .scenes
                    .get(&scene_name)
                    .ok_or_else(|| format!("Scene '{scene_name}' not found"))?;
                let (frame, program) = renderer.render_timeline_observable(
                    &scene.timeline,
                    local_time_s,
                    dimensions,
                    DebugRenderOptions::default(),
                )?;
                Ok(VerifyFrame {
                    frame,
                    bounds: verify::bounds_map(&program),
                })
            }
        },
    }
}

/// Result of running one check.
struct VerifyOutcome {
    line: usize,
    raw: String,
    passed: bool,
    detail: String,
}

/// Render every time a check needs (once) and evaluate all checks against the
/// cached frames.
fn run_verify_checks(
    renderer: &mut animatix::renderer::offscreen::OffscreenRenderer,
    target: &BuildTarget,
    checks: &[VerifyCheck],
    dimensions: SceneDimensions,
) -> Result<Vec<VerifyOutcome>, String> {
    let mut cache: std::collections::HashMap<u64, VerifyFrame> = std::collections::HashMap::new();
    for check in checks {
        for time_s in std::iter::once(check.time_s).chain(check.second_time_s) {
            let key = verify_time_key(time_s);
            if let std::collections::hash_map::Entry::Vacant(slot) = cache.entry(key) {
                slot.insert(render_verify_frame(renderer, target, dimensions, time_s)?);
            }
        }
    }

    let mut outcomes = Vec::with_capacity(checks.len());
    for check in checks {
        let entry = cache
            .get(&verify_time_key(check.time_s))
            .expect("frame rendered in the pass above");
        let view = FrameView::new(entry.frame.width, entry.frame.height, &entry.frame.rgba)
            .ok_or_else(|| "rendered frame buffer has an unexpected size".to_string())?;
        let reference = view.modal_color();
        let (passed, detail) = match check.kind {
            VerifyKind::Visible | VerifyKind::Invisible => {
                let label = check.label.as_deref().unwrap_or_default();
                let bounds = entry.bounds.get(label).copied();
                let visibility = verify::actor_visibility(
                    &view,
                    bounds,
                    reference,
                    verify::DEFAULT_TOLERANCE,
                    verify::DEFAULT_MIN_INK_PIXELS,
                );
                let want_visible = check.kind == VerifyKind::Visible;
                let detail = match bounds {
                    Some(bounds) => format!(
                        "`{label}` ink {} px ({:.2}%) in {:.0}x{:.0}px bounds",
                        visibility.ink_pixels,
                        visibility.ink_fraction * 100.0,
                        bounds.x1 - bounds.x0,
                        bounds.y1 - bounds.y0
                    ),
                    None => format!("`{label}` was not evaluated this frame (no bounds)"),
                };
                (want_visible == visibility.visible, detail)
            },
            VerifyKind::Ink => {
                let fraction = verify::frame_ink_fraction(&view, verify::DEFAULT_TOLERANCE);
                let min = check.threshold.unwrap_or(0.005);
                (
                    f64::from(fraction) >= min,
                    format!("frame ink {:.3}% (min {:.3}%)", fraction * 100.0, min * 100.0),
                )
            },
            VerifyKind::Differs => {
                let second = check.second_time_s.expect("differs checks carry a second time");
                let other =
                    cache.get(&verify_time_key(second)).expect("frame rendered in the pass above");
                let other_view =
                    FrameView::new(other.frame.width, other.frame.height, &other.frame.rgba)
                        .ok_or_else(|| {
                            "rendered frame buffer has an unexpected size".to_string()
                        })?;
                let changed =
                    verify::differing_fraction(&view, &other_view, verify::DEFAULT_TOLERANCE);
                let min = check.threshold.unwrap_or(0.001);
                (
                    f64::from(changed) >= min,
                    format!(
                        "{:.2}% of pixels changed between {:.3}s and {:.3}s (min {:.2}%)",
                        changed * 100.0,
                        check.time_s,
                        second,
                        min * 100.0
                    ),
                )
            },
            VerifyKind::Reveals => {
                let label = check.label.as_deref().unwrap_or_default();
                let after_time = check.second_time_s.expect("reveals checks carry a second time");
                let after = cache
                    .get(&verify_time_key(after_time))
                    .expect("frame rendered in the pass above");
                let after_view =
                    FrameView::new(after.frame.width, after.frame.height, &after.frame.rgba)
                        .ok_or_else(|| {
                            "rendered frame buffer has an unexpected size".to_string()
                        })?;
                let min = check.threshold.unwrap_or(0.01);
                match after.bounds.get(label).copied() {
                    Some(bounds) => {
                        let region = bounds.padded(verify::BOUNDS_PAD);
                        let changed = verify::region_differing_fraction(
                            &view,
                            &after_view,
                            region,
                            verify::DEFAULT_TOLERANCE,
                        );
                        (
                            f64::from(changed) >= min,
                            format!(
                                "`{label}` region changed {:.2}% between {:.3}s and {:.3}s (min {:.2}%)",
                                changed * 100.0,
                                check.time_s,
                                after_time,
                                min * 100.0
                            ),
                        )
                    },
                    None => (
                        false,
                        format!(
                            "`{label}` has no bounds at {after_time:.3}s; cannot verify reveal"
                        ),
                    ),
                }
            },
        };
        outcomes.push(VerifyOutcome {
            line: check.line,
            raw: check.raw.clone(),
            passed,
            detail,
        });
    }
    Ok(outcomes)
}

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
        } => {
            info!("Rendering Animatix GIF: {}", input.display());
            let (mut width, mut height, mut fps) = (width, height, fps);
            if let Some(name) = export_preset.as_deref() {
                let Some(preset_values) = renderer::ExportPreset::by_name(name) else {
                    error!("Unknown export preset '{name}'");
                    std::process::exit(2);
                };
                width = Some(preset_values.width);
                height = Some(preset_values.height);
                fps = preset_values.fps;
            }
            let (target, _) = load_and_build(&input, &extensions);
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
                    let Some(preset_values) = renderer::ExportPreset::by_name(name) else {
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
                BuildTarget::MultiScene(comp) => renderer::render_gif_composition_with_settings(
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
                    renderer::ExportSettings {
                        max_render_threads: threads,
                        ..Default::default()
                    },
                ),
                BuildTarget::SingleScene(timeline) => renderer::render_gif_timeline_with_settings(
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
                    renderer::ExportSettings {
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
        } => {
            info!("Rendering Animatix video: {}", input.display());
            let (mut width, mut height, mut fps, mut codec, mut preset) =
                (width, height, fps, codec, preset);
            if let Some(name) = export_preset.as_deref() {
                let Some(preset_values) = renderer::ExportPreset::by_name(name) else {
                    error!("Unknown export preset '{name}'");
                    std::process::exit(2);
                };
                width = Some(preset_values.width);
                height = Some(preset_values.height);
                fps = preset_values.fps;
                codec = preset_values.video_codec;
                preset = preset_values.h264_preset;
            }
            let (target, _) = load_and_build(&input, &extensions);
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
                    let Some(preset_values) = renderer::ExportPreset::by_name(name) else {
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
                BuildTarget::MultiScene(comp) => renderer::render_video_composition_with_settings(
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
                    renderer::ExportSettings {
                        max_render_threads: threads,
                        video_codec: codec,
                        h264_preset: preset,
                    },
                ),
                BuildTarget::SingleScene(timeline) => {
                    renderer::render_video_timeline_with_settings(
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
                        renderer::ExportSettings {
                            max_render_threads: threads,
                            video_codec: codec,
                            h264_preset: preset,
                        },
                    )
                },
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
        } => {
            info!("Rendering Animatix image: {}", input.display());
            let (target, _) = load_and_build(&input, &extensions);
            let configured_resolution = configured_resolution(&target);
            let width = width.or(configured_resolution.map(|(w, _)| w)).unwrap_or(1280);
            let height = height.or(configured_resolution.map(|(_, h)| h)).unwrap_or(720);
            let output_file =
                output.unwrap_or_else(|| PathBuf::from(format!("animatix_{}s.png", time)));
            info!("Output image: {}x{} at {}s -> {}", width, height, time, output_file.display());
            let result = match &target {
                BuildTarget::MultiScene(comp) => {
                    renderer::render_image_composition(comp, width, height, time, &output_file)
                },
                BuildTarget::SingleScene(timeline) => renderer::render_image_timeline_with_debug(
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

        Commands::Check {
            file,
            render_smoke,
            format,
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
            let report = BuildTarget::from_ast_with_context(
                &ast,
                &namespaces,
                if file_label == "-" {
                    None
                } else {
                    Some(std::path::Path::new(&file_label))
                },
                context,
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

            match format {
                OutputFormat::Json => {
                    if diagnostics.is_empty() && semantic.is_empty() {
                        println!(r#"{{"passed":true}}"#);
                    } else {
                        let mut errors: Vec<String> =
                            diagnostics.iter().map(diagnostic_to_json).collect();
                        for diag in &semantic {
                            let line = diag.line.to_string();
                            let col = diag.col.to_string();
                            let severity = format!("{:?}", diag.severity).to_lowercase();
                            let code = diag.code.as_deref().unwrap_or("");
                            errors.push(format!(
                                r#"{{"line":{},"col":{},"message":"{}","code":"{}","severity":"{}"}}"#,
                                line,
                                col,
                                escape_json(&diag.message),
                                code,
                                severity,
                            ));
                        }
                        println!(r#"{{"passed":false,"errors":[{}]}}"#, errors.join(","));
                        if diagnostics.iter().any(|d| d.is_error())
                            || semantic.iter().any(|d| d.is_error())
                        {
                            std::process::exit(1);
                        }
                    }
                },
                OutputFormat::Text => {
                    if diagnostics.is_empty() && semantic.is_empty() {
                        println!("{}: OK (no diagnostics)", file_label);
                    } else {
                        for diag in &diagnostics {
                            println!("{}", format_diagnostic_with_source(diag, &source));
                        }
                        for diag in &semantic {
                            println!("{}:{}", file_label, diag);
                        }
                        if diagnostics.iter().any(|d| d.is_error())
                            || semantic.iter().any(|d| d.is_error())
                        {
                            std::process::exit(1);
                        }
                    }
                },
            }
        },

        Commands::Verify {
            input,
            checks,
            width,
            height,
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
            let (target, _) = load_and_build(&input, &extensions);
            let configured = configured_resolution(&target);
            let dimensions = SceneDimensions {
                width: width.or(configured.map(|(w, _)| w)).unwrap_or(1280),
                height: height.or(configured.map(|(_, h)| h)).unwrap_or(720),
            };
            let mut renderer = match animatix::renderer::offscreen::OffscreenRenderer::new() {
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
                                for diag in &diagnostics {
                                    println!("{}:{}", file.display(), diag);
                                }
                            },
                            OutputFormat::Json => {
                                for diag in &diagnostics {
                                    all_diagnostics.push(serde_json::json!({
                                        "file": file.display().to_string(),
                                        "line": diag.line,
                                        "col": diag.col,
                                        "severity": format!("{:?}", diag.severity).to_lowercase(),
                                        "code": diag.code,
                                        "message": diag.message,
                                    }));
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

/// Escapes a string for safe inclusion in JSON output.
fn escape_json(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c.is_control() => {
                result.push_str(&format!("\\u{:04x}", c as u32));
            },
            c => result.push(c),
        }
    }
    result
}

/// Serializes a single diagnostic as a JSON object string.
fn diagnostic_to_json(d: &Diagnostic) -> String {
    let line = match d.location.line {
        Some(l) => l.to_string(),
        None => "null".to_string(),
    };
    let col = match d.location.column {
        Some(c) => c.to_string(),
        None => "null".to_string(),
    };
    format!(
        r#"{{"line":{},"col":{},"message":"{}","code":"{}","severity":"{}","phase":"{}"}}"#,
        line,
        col,
        escape_json(&d.message),
        d.code,
        d.severity,
        d.phase,
    )
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
}
