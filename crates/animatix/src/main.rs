use animatix::composition::BuildTarget;
use animatix::diagnostics::format_diagnostic;
use animatix::module::ModuleGraph;
use animatix::renderer;
use animatix::timeline::DebugRenderOptions;
use clap::{Parser as ClapParser, Subcommand};
use std::path::Path;
use std::path::PathBuf;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

#[derive(ClapParser, Debug)]
#[command(author, version, about, long_about = "Animatix CLI Tool")]
struct Args {
    /// Increase verbosity (-v for debug, -vv for trace)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Commands,
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
    /// Render a static scene from a given file
    Render {
        /// The input Animatix scene file (.amx)
        input: PathBuf,

        /// Loop the authored timeline in the preview window instead of holding on the last frame
        #[arg(long)]
        r#loop: bool,

        /// Draw per-node content bounding boxes for debugging
        #[arg(long)]
        debug_bounds: bool,
    },
    /// Render a specific frame to an image file (PNG)
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

        /// Video codec: auto, libx264, h264_nvenc, h264_vaapi
        #[arg(long, default_value = "auto")]
        codec: renderer::VideoCodec,

        /// libx264 preset: ultrafast, superfast, veryfast, faster, fast, medium, slow, slower, veryslow
        #[arg(long, default_value = "medium")]
        preset: renderer::H264Preset,
    },
    /// Render a scene to an animated GIF file
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
        /// Path to the .amx file
        file: String,
    },
}

// ----------------------------------------------------------------------------
// Shared helpers
// ----------------------------------------------------------------------------

/// Loads an Animatix program from disk, expands components, and builds the
/// appropriate target (single-scene `Timeline` or multi-scene `Composition`).
/// Prints build diagnostics and exits on load failure.
fn load_and_build(input: &Path) -> (BuildTarget, Vec<animatix::diagnostics::Diagnostic>) {
    let (ast, namespaces) = match ModuleGraph::new().load_program(input) {
        Ok(program) => (program.expand_components(), program.namespaces),
        Err(e) => {
            error!("Error: {}", e);
            std::process::exit(1);
        }
    };

    let report = BuildTarget::from_ast(&ast, &namespaces);
    print_build_diagnostics(&report.diagnostics);
    (report.output, report.diagnostics)
}

/// Resolves export duration for a `BuildTarget` (single or multi-scene).
fn resolve_duration(duration: Option<f32>, target: &BuildTarget, hold: f32, min_duration: f32) -> f32 {
    duration.unwrap_or_else(|| {
        let d = target.duration_s() as f32 + hold.max(0.0);
        d.max(min_duration)
    })
}

/// Generates a timestamped default filename when `--output` is omitted.
fn default_output_file(ext: &str) -> PathBuf {
    let now = chrono::Local::now();
    PathBuf::from(format!(
        "animatix_{}.{}",
        now.format("%y%m%d_%H%M%S"),
        ext
    ))
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
        .init();

    match args.command {
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
                    DebugRenderOptions { compute_hit_regions: false,
                        draw_bounds: debug_bounds,
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
                    DebugRenderOptions { compute_hit_regions: false,
                        draw_bounds: debug_bounds,
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
        }

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
                    DebugRenderOptions { compute_hit_regions: false,
                        draw_bounds: debug_bounds,
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
                        DebugRenderOptions { compute_hit_regions: false,
                            draw_bounds: debug_bounds,
                        },
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

        Commands::Render {
            input,
            r#loop,
            debug_bounds,
        } => {
            info!("Rendering Animatix file: {}", input.display());
            let (target, _) = load_and_build(&input);
            match target {
                BuildTarget::MultiScene(comp) => {
                    // Live preview shows the first scene for multi-scene compositions.
                    // The full composition timeline is available in the GUI.
                    if let Some(first_scene) = comp.scenes.values().next() {
                        info!(
                            "Multi-scene composition ({} scenes). Previewing first scene: '{}'.",
                            comp.scenes.len(),
                            first_scene.name
                        );
                        if let Err(e) = renderer::run_timeline_with_options(
                            first_scene.timeline.clone(),
                            r#loop,
                            DebugRenderOptions { compute_hit_regions: false,
                                draw_bounds: debug_bounds,
                            },
                        ) {
                            error!("Preview failed: {e}");
                            std::process::exit(1);
                        }
                    } else {
                        error!("Error: Composition has no scenes.");
                        std::process::exit(1);
                    }
                }
                BuildTarget::SingleScene(timeline) => {
                    if let Err(e) = renderer::run_timeline_with_options(
                        timeline,
                        r#loop,
                        DebugRenderOptions { compute_hit_regions: false,
                            draw_bounds: debug_bounds,
                        },
                    ) {
                        error!("Preview failed: {e}");
                        std::process::exit(1);
                    }
                }
            }
        }

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
                    DebugRenderOptions { compute_hit_regions: false,
                        draw_bounds: debug_bounds,
                    },
                ),
            };
            if let Err(e) = result {
                error!("Error: {e}");
                std::process::exit(1);
            }
        }

        Commands::Check { file } => {
            let source = match std::fs::read_to_string(&file) {
                Ok(s) => s,
                Err(e) => {
                    error!("Cannot read {}: {}", file, e);
                    std::process::exit(1);
                }
            };
            let mut module_graph = ModuleGraph::new();
            let (ast, namespaces) = match module_graph
                .load_program_with_source(std::path::Path::new(&file), Some(&source))
            {
                Ok(program) => (program.expand_components(), program.namespaces),
                Err(e) => {
                    error!("Error: {}", e);
                    std::process::exit(1);
                }
            };
            let report = BuildTarget::from_ast(&ast, &namespaces);
            let diagnostics = report.diagnostics;

            if diagnostics.is_empty() {
                println!("{}: OK (no diagnostics)", file);
            } else {
                for diag in &diagnostics {
                    let prefix = match diag.phase {
                        animatix::diagnostics::DiagnosticPhase::Parse => "[parse]",
                        animatix::diagnostics::DiagnosticPhase::Build => "[build]",
                        animatix::diagnostics::DiagnosticPhase::Render => "[render]",
                    };
                    let severity = if diag.is_error() { "ERROR" } else { "WARNING" };
                    println!("{prefix} {severity}: {}", diag.message);
                    if let Some(line) = diag.location.line {
                        if let Some(col) = diag.location.column {
                            println!("  at {}:{}", line, col);
                        }
                    }
                    if let Some(subject) = &diag.location.subject {
                        println!("  subject: {subject}");
                    }
                }
                if diagnostics.iter().any(|d| d.is_error()) {
                    std::process::exit(1);
                }
            }
        }
    }
}

fn print_build_diagnostics(diagnostics: &[animatix::diagnostics::Diagnostic]) {
    for diagnostic in diagnostics {
        let formatted = format_diagnostic(diagnostic);
        if diagnostic.is_error() {
            error!("{}", formatted);
        } else {
            warn!("{}", formatted);
        }
    }
}
