use animatix::diagnostics::format_diagnostic;
use animatix::module::ModuleGraph;
use animatix::renderer;
use animatix::timeline::{DebugRenderOptions, Timeline};
use clap::{Parser as ClapParser, Subcommand};
use std::path::PathBuf;

#[derive(ClapParser, Debug)]
#[command(author, version, about, long_about = "Animatix CLI Tool")]
struct Args {
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

/// Loads an Animatix program from disk, expands components, and builds a Timeline.
/// Prints build diagnostics and exits on load failure.
fn load_and_build(input: &PathBuf) -> (Timeline, Vec<animatix::diagnostics::Diagnostic>) {
    let (ast, namespaces) = match ModuleGraph::new().load_program(input) {
        Ok(program) => (program.expand_components(), program.namespaces),
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    let report = Timeline::build_with_diagnostics(&ast, &namespaces);
    print_build_diagnostics(&report.diagnostics);
    (report.output, report.diagnostics)
}

/// Resolves export duration.
/// - If `duration` is `Some`, uses that value directly.
/// - Otherwise, computes `timeline.duration_seconds() + hold`, floored at `min_duration`.
fn resolve_duration(duration: Option<f32>, timeline: &Timeline, hold: f32, min_duration: f32) -> f32 {
    duration.unwrap_or_else(|| {
        let d = timeline.duration_seconds() as f32 + hold.max(0.0);
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
        } => {
            println!("Rendering Animatix GIF: {}", input.display());
            let (timeline, _) = load_and_build(&input);
            let effective_duration = resolve_duration(duration, &timeline, hold, 0.5);
            let output_file = output.unwrap_or_else(|| default_output_file("gif"));
            println!(
                "Output configuration: {}x{} at {} FPS for {:.2}s -> {}",
                width, height, fps, effective_duration, output_file.display()
            );
            renderer::render_gif_timeline_with_debug(
                timeline,
                width,
                height,
                fps,
                effective_duration,
                &output_file,
                DebugRenderOptions {
                    draw_bounds: debug_bounds,
                },
            );
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
        } => {
            println!("Rendering Animatix video: {}", input.display());
            let (timeline, _) = load_and_build(&input);
            let effective_duration = resolve_duration(duration, &timeline, hold, 0.5);
            let output_file = output.unwrap_or_else(|| default_output_file("mp4"));
            println!(
                "Output configuration: {}x{} at {} FPS for {:.2}s -> {}",
                width, height, fps, effective_duration, output_file.display()
            );
            renderer::render_video_timeline_with_debug(
                timeline,
                width,
                height,
                fps,
                effective_duration,
                &output_file,
                DebugRenderOptions {
                    draw_bounds: debug_bounds,
                },
            );
        }

        Commands::Ast {
            input,
            compact,
            force: _,
        } => {
            println!("Parsing Animatix file: {}", input.display());
            let ast = match ModuleGraph::new().load_entry(&input) {
                Ok(statements) => statements,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };
            println!("\nAbstract Syntax Tree:");
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
            println!("Rendering Animatix file: {}", input.display());
            let (timeline, _) = load_and_build(&input);
            renderer::run_timeline_with_options(
                timeline,
                r#loop,
                DebugRenderOptions {
                    draw_bounds: debug_bounds,
                },
            );
        }

        Commands::Image {
            input,
            width,
            height,
            time,
            output,
            debug_bounds,
        } => {
            println!("Rendering Animatix image: {}", input.display());
            let output_file =
                output.unwrap_or_else(|| PathBuf::from(format!("animatix_{}s.png", time)));
            println!(
                "Output image: {}x{} at {}s -> {}",
                width, height, time, output_file.display()
            );
            let (timeline, _) = load_and_build(&input);
            renderer::render_image_timeline_with_debug(
                timeline,
                width,
                height,
                time,
                &output_file,
                DebugRenderOptions {
                    draw_bounds: debug_bounds,
                },
            );
        }

        Commands::Check { file } => {
            let source = match std::fs::read_to_string(&file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Cannot read {}: {}", file, e);
                    std::process::exit(1);
                }
            };
            let mut module_graph = ModuleGraph::new();
            let (ast, namespaces) = match module_graph
                .load_program_with_source(std::path::Path::new(&file), Some(&source))
            {
                Ok(program) => (program.expand_components(), program.namespaces),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };
            let report = Timeline::build_with_diagnostics(&ast, &namespaces);

            if report.diagnostics.is_empty() {
                println!("{}: OK (no diagnostics)", file);
            } else {
                for diag in &report.diagnostics {
                    let prefix = match diag.phase {
                        animatix::diagnostics::DiagnosticPhase::Parse => "[parse]",
                        animatix::diagnostics::DiagnosticPhase::Build => "[build]",
                        animatix::diagnostics::DiagnosticPhase::Render => "[render]",
                    };
                    let severity = if diag.is_error() { "ERROR" } else { "WARNING" };
                    println!("{prefix} {severity}: {}", diag.message);
                    if let Some(subject) = &diag.location.subject {
                        println!("  subject: {subject}");
                    }
                }
                if report.diagnostics.iter().any(|d| d.is_error()) {
                    std::process::exit(1);
                }
            }
        }
    }
}

fn print_build_diagnostics(diagnostics: &[animatix::diagnostics::Diagnostic]) {
    for diagnostic in diagnostics {
        eprintln!("{}", format_diagnostic(diagnostic));
    }
}
