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
mod verify;

#[cfg(test)]
mod tests;

pub use build::parse_set_pair;
pub use diagnostics::OutputFormat;
use extensions::load_cli_extensions;

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
        Commands::Timeline { input } => {
            commands::timeline::run(&input);
        },

        Commands::Ast {
            input,
            compact,
            force: _,
        } => {
            commands::ast::run(&input, compact);
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
                &set,
                &extensions,
            );
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
            commands::render::run_video(
                &input,
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
                export_preset.as_deref(),
                &set,
                &extensions,
            );
        },

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
                &set,
                &extensions,
            );
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
            commands::storyboard::run(
                &input,
                &set,
                cols,
                tile_width,
                tile_height,
                times,
                max_frames,
                debug_bounds,
                output,
                &extensions,
            );
        },

        Commands::Check {
            file,
            render_smoke,
            format,
            set,
        } => {
            commands::check::run(&file, render_smoke, &format, &set, &extensions);
        },

        Commands::Verify {
            input,
            checks,
            width,
            height,
            set,
            format,
        } => {
            commands::verify::run(
                &input,
                checks.as_ref(),
                width,
                height,
                &set,
                &format,
                &extensions,
            );
        },

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
            commands::lint::run(&paths, &format, deny_warnings, config.as_ref(), &extensions);
        },
    }
}
