#![allow(dead_code)] // Shared review/frame_source modules are compiled by both
// the bin and the test-library target; items reachable only
// from the latter are intentionally unused here (mirrors lib.rs).

use std::path::PathBuf;

pub use animatix::composition::BuildTarget;
#[cfg(feature = "video")]
use animatix_render::encode as export;
use clap::{Parser as ClapParser, Subcommand};
use tracing::error;
use tracing_subscriber::EnvFilter;

mod build;
mod commands;
mod diagnostics;
mod extensions;
mod frame_source;
mod review;

#[cfg(test)]
mod tests;

pub use build::parse_set_pair;
pub use diagnostics::OutputFormat;
use extensions::{CliExtensions, load_cli_extensions};

#[derive(clap::Args, Debug, Clone)]
struct SetArgs {
    /// Override a build-time variable: `--set NAME=VALUE` (repeatable).
    ///
    /// The value is parsed as an Animatix expression, so a template's own
    /// `let` is a default and the command line wins.
    #[arg(long = "set", value_parser = parse_set_pair)]
    set: Vec<(String, String)>,
}

#[cfg(feature = "video")]
#[derive(clap::Args, Debug)]
struct VideoArgs {
    /// The input Animatix scene file (.amx)
    input: PathBuf,
    #[command(flatten)]
    set: SetArgs,

    /// Export a named chapter from a multi-scene composition
    #[arg(long, value_name = "NAME")]
    scene: Option<String>,

    /// Omit the audio track
    #[arg(long)]
    no_audio: bool,

    /// Output video width (defaults to the configured canvas size, else 1280)
    #[arg(long)]
    width: Option<u32>,

    /// Output video height (defaults to the configured canvas size, else 720)
    #[arg(long)]
    height: Option<u32>,

    /// Output framerate
    #[arg(long, default_value_t = 30)]
    fps: u32,

    /// Full export duration in seconds. If omitted, auto-detects from timeline length.
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
}

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

fn template_ids() -> [&'static str; 3] {
    animatix_render::delivery::ProjectTemplate::ALL.map(|template| template.id())
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Create a starter project in a new or empty directory.
    Init {
        #[arg(default_value = ".")]
        directory: PathBuf,
        /// Starter project to copy.
        #[arg(long, default_value = "narrated", value_parser = clap::builder::PossibleValuesParser::new(template_ids()))]
        template: String,
    },
    /// Diagnose GPU, font, audio muxing, and MP4 encoder availability.
    Doctor,
    /// Export caption cues to an SRT or WebVTT file without rendering.
    Subtitles {
        input: PathBuf,
        #[command(flatten)]
        set: SetArgs,
        /// Select a Captions actor by label.
        #[arg(long)]
        track: Option<String>,
        /// Destination ending in .srt or .vtt.
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Normalize transcript output from an external recognizer and apply optional corrections.
    Transcript {
        /// Whisper JSON, speech JSON, SRT, or WebVTT.
        input: PathBuf,
        /// JSON document with corrections by segment ID and word index.
        #[arg(long)]
        edits: Option<PathBuf>,
        /// Destination .speech.json file.
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Inspect the frame ranges and dependencies for incremental chapter video export.
    #[cfg(feature = "video")]
    ChapterPlan(commands::chapters::PlanArgs),
    /// Export the timeline's mixed audio to M4A without rendering video frames.
    #[cfg(feature = "video")]
    Audio {
        /// Input Animatix scene file (.amx).
        input: PathBuf,
        /// Audio duration in seconds; defaults to the video export duration.
        #[arg(long)]
        duration: Option<f32>,
        /// Output M4A filename.
        #[arg(short, long)]
        output: PathBuf,
    },
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
        #[command(flatten)]
        set: SetArgs,

        /// Output image width (defaults to the configured canvas size, else 1280)
        #[arg(long)]
        width: Option<u32>,

        /// Output image height (defaults to the configured canvas size, else 720)
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
        #[command(flatten)]
        set: SetArgs,
        /// Original scene file to show in the left column.
        #[arg(long, conflicts_with = "cols")]
        compare: Option<PathBuf>,
        /// Compare this named scene at scene-local times in both files.
        #[arg(long, requires = "compare")]
        scene: Option<String>,
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
        /// Maximum contact sheet width (default: 1536)
        #[arg(long, default_value_t = 1536)]
        max_width: u32,
        /// Minimum ink fraction for a frame to be included in the storyboard (default: 0.005)
        #[arg(long, default_value_t = 0.005)]
        min_ink: f64,
        /// Skip rendering and use time-in-interval midpoints
        #[arg(long)]
        fast: bool,
        /// Draw per-node content bounding boxes for debugging
        #[arg(long)]
        debug_bounds: bool,
        /// Output image filename (defaults to `<name>_storyboard.png`)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Render a scene to a video file
    #[cfg(feature = "video")]
    Video(VideoArgs),
    /// Render a scene to an animated GIF file
    #[cfg(feature = "video")]
    Gif {
        /// The input Animatix scene file (.amx)
        input: PathBuf,
        #[command(flatten)]
        set: SetArgs,

        /// Output GIF width (defaults to the configured canvas size, else 640)
        #[arg(long)]
        width: Option<u32>,

        /// Output GIF height (defaults to the configured canvas size, else 360)
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
        #[command(flatten)]
        set: SetArgs,

        /// Render one frame at time=0 to catch renderer bugs
        #[arg(long)]
        render_smoke: bool,

        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: OutputFormat,
    },
    /// Automatically audit a scene for layout, visibility, timing and ink defects
    Audit {
        #[command(flatten)]
        args: commands::audit::AuditArgs,
    },
    /// Emit measured scene facts as JSON, with no rule verdicts
    Measure {
        #[command(flatten)]
        args: commands::measure::MeasureArgs,
    },
    /// Compare or update visual regression baseline PNG snapshots
    Baseline {
        #[command(flatten)]
        args: commands::baseline::BaselineArgs,
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

fn main() {
    #[cfg(feature = "video")]
    let args = Args::parse();
    #[cfg(not(feature = "video"))]
    let args = parse_args_with_feature_help();

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
        .with_writer(std::io::stderr)
        .with_ansi(!args.no_color)
        .init();

    let extensions = match load_cli_extensions(&args.plugin) {
        Ok(extensions) => extensions,
        Err(e) => {
            error!("{e}");
            std::process::exit(2);
        },
    };

    dispatch(args.command, &extensions);
}

#[cfg(not(feature = "video"))]
fn parse_args_with_feature_help() -> Args {
    use clap::error::{ContextKind, ContextValue, ErrorKind};
    use clap::{CommandFactory, FromArgMatches};

    let mut command = Args::command();
    let mut matches = match command.try_get_matches_from_mut(std::env::args_os()) {
        Ok(matches) => matches,
        Err(mut error) => {
            let is_video_subcommand = matches!(
                error.get(ContextKind::InvalidSubcommand),
                Some(ContextValue::String(name))
                    if matches!(name.as_str(), "video" | "gif" | "audio" | "chapter-plan")
            );
            // The same spelling may occur as an invalid nested subcommand.
            if error.kind() == ErrorKind::InvalidSubcommand
                && is_video_subcommand
                && error.get(ContextKind::Usage)
                    == Some(&ContextValue::StyledStr(command.render_usage()))
            {
                error.insert(
                    ContextKind::Suggested,
                    ContextValue::StyledStrs(vec![
                        "This command requires the optional 'video' feature. Build it with 'nix develop --command cargo build -p animatix-cli --features video'.".into(),
                    ]),
                );
            }
            error.exit();
        },
    };
    Args::from_arg_matches_mut(&mut matches)
        .unwrap_or_else(|error| error.format(&mut command).exit())
}

#[cfg(feature = "video")]
fn dispatch_video(args: VideoArgs, extensions: &CliExtensions) {
    commands::render::run_video(
        &args.input,
        args.scene.as_deref(),
        args.no_audio,
        args.width,
        args.height,
        args.fps,
        args.duration,
        args.hold,
        args.output,
        args.debug_bounds,
        export::ExportSettings {
            max_render_threads: args.threads,
            video_codec: args.codec,
            h264_preset: args.preset,
        },
        args.export_preset.as_deref(),
        &args.set.set,
        extensions,
    );
}

fn exit_on_error(result: Result<(), String>) {
    if let Err(error) = result {
        tracing::error!("{error}");
        std::process::exit(1);
    }
}

fn dispatch(command: Commands, extensions: &CliExtensions) {
    match command {
        Commands::Init {
            directory,
            template,
        } => {
            let result = template
                .parse::<animatix_render::delivery::ProjectTemplate>()
                .and_then(|template| template.create(&directory));
            match result {
                Ok(scene) => println!("Created {}", scene.display()),
                Err(err) => {
                    eprintln!("Could not create a project in {}: {err}", directory.display());
                    std::process::exit(1);
                },
            }
        },
        Commands::Doctor => {
            if !animatix_render::delivery::print_doctor() {
                std::process::exit(1);
            }
        },
        Commands::Subtitles {
            input,
            set,
            track,
            output,
        } => {
            exit_on_error(commands::subtitles::run(
                &input,
                &set.set,
                track.as_deref(),
                &output,
                extensions,
            ));
        },
        Commands::Transcript {
            input,
            edits,
            output,
        } => exit_on_error(commands::transcript::run(&input, edits.as_deref(), &output)),
        #[cfg(feature = "video")]
        Commands::ChapterPlan(args) => {
            if let Err(err) = commands::chapters::run_plan(args, extensions) {
                error!("{err}");
                std::process::exit(1);
            }
        },
        #[cfg(feature = "video")]
        Commands::Audio {
            input,
            duration,
            output,
        } => {
            if let Err(err) = commands::chapters::run_audio(&input, duration, &output, extensions) {
                error!("{err}");
                std::process::exit(1);
            }
        },
        Commands::Timeline { input } => {
            commands::timeline::run(&input);
        },

        Commands::Ast {
            input,
            compact,
            force,
        } => {
            commands::ast::run(&input, compact, force);
        },

        Commands::Plugin { command } => match command {
            PluginCommands::Describe { library, output } => {
                if let Err(e) = commands::plugin::run_describe(&library, output.as_deref()) {
                    error!("{e}");
                    std::process::exit(2);
                }
            },
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
            commands::render::run_image(
                &input,
                width,
                height,
                time,
                output,
                debug_bounds,
                &set.set,
                extensions,
            );
        },

        #[cfg(feature = "video")]
        Commands::Video(args) => dispatch_video(args, extensions),

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
            commands::render::run_gif(
                &input,
                width,
                height,
                fps,
                duration,
                hold,
                output,
                debug_bounds,
                threads,
                export_preset.as_deref(),
                &set.set,
                extensions,
            );
        },

        Commands::Storyboard {
            input,
            set,
            compare,
            scene,
            cols,
            tile_width,
            tile_height,
            times,
            max_frames,
            max_width,
            min_ink,
            fast,
            debug_bounds,
            output,
        } => {
            commands::storyboard::run(
                &input,
                &set.set,
                compare.as_deref().map(|path| (path, scene.as_deref())),
                cols,
                tile_width,
                tile_height,
                max_width,
                min_ink,
                fast,
                times,
                max_frames,
                debug_bounds,
                output,
                extensions,
            );
        },

        Commands::Check {
            file,
            render_smoke,
            format,
            set,
        } => {
            commands::check::run(&file, render_smoke, &format, &set.set, extensions);
        },

        Commands::Audit { args } => commands::audit::run(args, extensions),
        Commands::Measure { args } => commands::measure::run(args, extensions),
        Commands::Baseline { args } => commands::baseline::run(args, extensions),

        Commands::Fmt {
            paths,
            check,
            indent,
        } => {
            commands::fmt::run(&paths, check, indent);
        },

        Commands::Lint {
            paths,
            format,
            deny_warnings,
            config,
        } => {
            commands::lint::run(&paths, &format, deny_warnings, config.as_ref(), extensions);
        },
    }
}
