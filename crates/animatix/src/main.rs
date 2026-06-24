use animatix::composition::BuildTarget;
use animatix_syntax::diagnostics::{format_diagnostic_with_source, Diagnostic, DiagnosticCode, DiagnosticPhase};
#[cfg(feature = "video")]
use animatix_syntax::diagnostics::format_diagnostic;
use animatix_syntax::module::ModuleGraph;
#[cfg(feature = "video")]
use animatix::renderer;
#[cfg(feature = "video")]
use animatix::timeline::DebugRenderOptions;
use clap::{Parser as ClapParser, Subcommand, ValueEnum};
use std::path::Path;
use std::path::PathBuf;
use tracing::{error, info};
#[cfg(feature = "video")]
use tracing::warn;
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

    /// Render a specific frame to an image file (PNG)
    #[cfg(feature = "video")]
    Image {
        /// The input Animatix scene file (.amx)
        input: PathBuf,

        /// Output image width
        #[arg(long, default_value_t = 1280)]
        width: u32,

        /// Output image height
        #[arg(long, default_value_t = 720)]
        height: u32,

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

        /// Output video width
        #[arg(long, default_value_t = 1280)]
        width: u32,

        /// Output video height
        #[arg(long, default_value_t = 720)]
        height: u32,

        /// Output framerate
        #[arg(long, default_value_t = 30)]
        fps: u32,

        /// Render duration in seconds. If omitted, auto-detects from timeline length.
        #[arg(long)]
        duration: Option<f32>,

        /// Seconds to hold the last frame after the last animation ends. Applied only when --duration is omitted.
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

        /// libx264 preset: ultrafast, superfast, veryfast, faster, fast, medium, slow, slower, veryslow
        #[arg(long, default_value = "medium")]
        preset: renderer::H264Preset,
    },
    /// Render a scene to an animated GIF file
    #[cfg(feature = "video")]
    Gif {
        /// The input Animatix scene file (.amx)
        input: PathBuf,

        /// Output GIF width
        #[arg(long, default_value_t = 640)]
        width: u32,

        /// Output GIF height
        #[arg(long, default_value_t = 360)]
        height: u32,

        /// Output framerate
        #[arg(long, default_value_t = 15)]
        fps: u32,

        /// Render duration in seconds. If omitted, auto-detects from timeline length.
        #[arg(long)]
        duration: Option<f32>,

        /// Seconds to hold the last frame after the last animation ends. Applied only when --duration is omitted.
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

// ----------------------------------------------------------------------------
// Shared helpers
// ----------------------------------------------------------------------------

/// Loads an Animatix program from disk, expands components, and builds the
/// appropriate target (single-scene `Timeline` or multi-scene `Composition`).
/// Prints build diagnostics and exits on load failure.
#[cfg(feature = "video")]
fn load_and_build(input: &Path) -> (BuildTarget, Vec<animatix_syntax::diagnostics::Diagnostic>) {
    let (ast, namespaces, type_diagnostics) = match ModuleGraph::new().load_program(input) {
        Ok(mut program) => {
            let diagnostics = program.typecheck();
            (program.expand_components(), program.namespaces, diagnostics)
        }
        Err(e) => {
            error!("Error: {}", e);
            std::process::exit(1);
        }
    };

    let report = BuildTarget::from_ast(&ast, &namespaces, Some(input));
    let mut all_diagnostics = type_diagnostics;
    all_diagnostics.extend(report.diagnostics);
    print_build_diagnostics(&all_diagnostics);
    (report.output, all_diagnostics)
}

/// Resolves export duration for a `BuildTarget` (single or multi-scene).
#[cfg(feature = "video")]
fn resolve_duration(duration: Option<f32>, target: &BuildTarget, hold: f32, min_duration: f32) -> f32 {
    duration.unwrap_or_else(|| {
        let d = target.duration_s() as f32 + hold.max(0.0);
        d.max(min_duration)
    })
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
        }
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
        }
    }

    Ok(())
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
        } => {
            info!("Rendering Animatix GIF: {}", input.display());
            let (target, _) = load_and_build(&input);
            let effective_duration = resolve_duration(duration, &target, hold, 0.5);
            let output_file = output.unwrap_or_else(|| default_output_file("gif"));
            info!(
                "Output configuration: {}x{} at {} FPS for {:.2}s -> {}",
                width, height, fps, effective_duration, output_file.display()
            );
            let result = match &target {
                BuildTarget::MultiScene(comp) => renderer::render_gif_composition_with_settings(
                    comp,
                    width,
                    height,
                    fps,
                    effective_duration,
                    &output_file,
                    DebugRenderOptions { compute_hit_regions: false, draw_bounds: debug_bounds, ..Default::default() },
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
                    DebugRenderOptions { compute_hit_regions: false, draw_bounds: debug_bounds, ..Default::default() },
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
        }

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
        } => {
            info!("Rendering Animatix video: {}", input.display());
            let (target, _) = load_and_build(&input);
            let effective_duration = resolve_duration(duration, &target, hold, 0.5);
            let output_file = output.unwrap_or_else(|| default_output_file("mp4"));
            info!(
                "Output configuration: {}x{} at {} FPS for {:.2}s -> {}",
                width, height, fps, effective_duration, output_file.display()
            );
            let result = match &target {
                BuildTarget::MultiScene(comp) => renderer::render_video_composition_with_settings(
                    comp,
                    width,
                    height,
                    fps,
                    effective_duration,
                    &output_file,
                    DebugRenderOptions { compute_hit_regions: false, draw_bounds: debug_bounds, ..Default::default() },
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
                        DebugRenderOptions { compute_hit_regions: false, draw_bounds: debug_bounds, ..Default::default() },
                        renderer::ExportSettings {
                            max_render_threads: threads,
                            video_codec: codec,
                            h264_preset: preset,
                        },
                    )
                }
            };
            if let Err(e) = result {
                error!("Error: {e}");
                std::process::exit(1);
            }
        }

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
                }
            };
            if compact {
                println!("{:?}", ast);
            } else {
                for stmt in &ast {
                    println!("{:#?}", stmt);
                }
            }
        }



        #[cfg(feature = "video")]
        Commands::Image {
            input,
            width,
            height,
            time,
            output,
            debug_bounds,
        } => {
            info!("Rendering Animatix image: {}", input.display());
            let output_file =
                output.unwrap_or_else(|| PathBuf::from(format!("animatix_{}s.png", time)));
            info!(
                "Output image: {}x{} at {}s -> {}",
                width, height, time, output_file.display()
            );
            let (target, _) = load_and_build(&input);
            let result = match &target {
                BuildTarget::MultiScene(comp) => renderer::render_image_composition(
                    comp,
                    width,
                    height,
                    time,
                    &output_file,
                ),
                BuildTarget::SingleScene(timeline) => renderer::render_image_timeline_with_debug(
                    timeline.clone(),
                    width,
                    height,
                    time,
                    &output_file,
                    DebugRenderOptions { compute_hit_regions: false, draw_bounds: debug_bounds, ..Default::default() },
                ),
            };
            if let Err(e) = result {
                error!("Error: {e}");
                std::process::exit(1);
            }
        }

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
                    }
                };
                (source, "-".to_string())
            } else {
                let source = match std::fs::read_to_string(&file) {
                    Ok(s) => s,
                    Err(e) => {
                        error!("Cannot read {}: {}", file, e);
                        std::process::exit(1);
                    }
                };
                (source, file.clone())
            };
            let mut module_graph = ModuleGraph::new();
            let (ast, namespaces, type_diagnostics) = match module_graph
                .load_program_with_source(std::path::Path::new(&file_label), Some(&source))
            {
                Ok(mut program) => {
                    let diagnostics = program.typecheck();
                    (program.expand_components(), program.namespaces, diagnostics)
                }
                Err(e) => {
                    error!("Error: {}", e);
                    std::process::exit(1);
                }
            };
            let report = BuildTarget::from_ast(
                &ast,
                &namespaces,
                if file_label == "-" { None } else { Some(std::path::Path::new(&file_label)) },
            );
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

            match format {
                OutputFormat::Json => {
                    if diagnostics.is_empty() {
                        println!(r#"{{"passed":true}}"#);
                    } else {
                        let errors: Vec<String> = diagnostics
                            .iter()
                            .map(diagnostic_to_json)
                            .collect();
                        println!(
                            r#"{{"passed":false,"errors":[{}]}}"#,
                            errors.join(",")
                        );
                        if diagnostics.iter().any(|d| d.is_error()) {
                            std::process::exit(1);
                        }
                    }
                }
                OutputFormat::Text => {
                    if diagnostics.is_empty() {
                        println!("{}: OK (no diagnostics)", file_label);
                    } else {
                        for diag in &diagnostics {
                            println!("{}", format_diagnostic_with_source(diag, &source));
                        }
                        if diagnostics.iter().any(|d| d.is_error()) {
                            std::process::exit(1);
                        }
                    }
                }
            }
        }

        Commands::Fmt { paths, check, indent } => {
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
        }

        Commands::Lint { paths, format, deny_warnings, config } => {
            // Load lint config from file if specified
            let file_config = config.as_ref()
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
                        }
                    };

                    let analyzer = animatix_analyzer::Analyzer::new_with_path(&source, Some(file.clone()));
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
                            }
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
                            }
                        }

                        total_errors += diagnostics.iter().filter(|d| d.is_error()).count();
                        total_warnings += diagnostics.iter().filter(|d| d.is_warning()).count();
                    }
                }
            }

            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&all_diagnostics).unwrap());
                }
                OutputFormat::Text => {
                    if total_errors > 0 || total_warnings > 0 {
                        println!("\n{} error(s), {} warning(s)", total_errors, total_warnings);
                    }
                }
            }

            if total_errors > 0 || (deny_warnings && total_warnings > 0) {
                std::process::exit(1);
            }
        }
    }
}

/// Format a single .amx file.
///
/// If `check` is true, only checks if the file is formatted (doesn't modify).
/// Returns Ok(()) if the file is already formatted or was formatted successfully.
fn format_file(path: &Path, formatter: &animatix_syntax::formatter::Formatter, check: bool) -> Result<(), String> {
    use animatix_syntax::chumsky::Parser;

    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read: {}", e))?;

    let stmts = animatix_syntax::parser::parser_simple()
        .parse(&source)
        .into_result()
        .map_err(|e| format!("Parse error: {:?}", e))?;

    let formatted = formatter.format(&stmts);

    if check {
        if source != formatted {
            return Err("File is not formatted".into());
        }
    } else {
        if source != formatted {
            std::fs::write(path, &formatted)
                .map_err(|e| format!("Failed to write: {}", e))?;
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
            }
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

#[cfg(feature = "video")]
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
