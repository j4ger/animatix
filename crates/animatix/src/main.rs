use animatix::parser::parser;
use animatix::renderer;
use chumsky::Parser;
use clap::{Parser as ClapParser, Subcommand};
use std::fs;
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

        /// Render time in seconds
        #[arg(long, default_value_t = 5.0)]
        duration: f32,

        /// Output filename
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

fn main() {
    let args = Args::parse();

    match args.command {
        Commands::Ast {
            input,
            compact,
            force,
        } => {
            let src = match fs::read_to_string(&input) {
                Ok(content) => content,
                Err(e) => {
                    eprintln!(
                        "Error: Failed to read input file '{}': {}",
                        input.display(),
                        e
                    );
                    std::process::exit(1);
                }
            };

            println!("Parsing Animatix file: {}", input.display());

            let (ast, errs) = parser().parse(src.as_str()).into_output_errors();

            let has_errors = !errs.is_empty();

            if let Some(ast) = ast {
                if !has_errors || force {
                    println!("\nAbstract Syntax Tree:");
                    if compact {
                        println!("{:?}", ast);
                    } else {
                        for stmt in ast {
                            println!("{:#?}", stmt);
                        }
                    }
                }
            }

            if has_errors {
                eprintln!("\nErrors:");
                for err in errs {
                    eprintln!("{:?}", err);
                }
                std::process::exit(1);
            }
        }
        Commands::Render { input } => {
            let src = match fs::read_to_string(&input) {
                Ok(content) => content,
                Err(e) => {
                    eprintln!(
                        "Error: Failed to read input file '{}': {}",
                        input.display(),
                        e
                    );
                    std::process::exit(1);
                }
            };

            println!("Rendering Animatix file: {}", input.display());

            let (ast, errs) = parser().parse(src.as_str()).into_output_errors();

            if !errs.is_empty() {
                eprintln!("\nParse Errors:");
                for err in errs {
                    eprintln!("{:?}", err);
                }
                std::process::exit(1);
            }

            if let Some(ast) = ast {
                renderer::run(&ast);
            } else {
                eprintln!("Failed to generate AST.");
                std::process::exit(1);
            }
        }
        Commands::Video {
            input,
            width,
            height,
            fps,
            duration,
            output,
        } => {
            let src = match fs::read_to_string(&input) {
                Ok(content) => content,
                Err(e) => {
                    eprintln!(
                        "Error: Failed to read input file '{}': {}",
                        input.display(),
                        e
                    );
                    std::process::exit(1);
                }
            };

            println!("Rendering Animatix video: {}", input.display());

            let (ast, errs) = parser().parse(src.as_str()).into_output_errors();

            if !errs.is_empty() {
                eprintln!("\nParse Errors:");
                for err in errs {
                    eprintln!("{:?}", err);
                }
                std::process::exit(1);
            }

            if let Some(ast) = ast {
                let output_file = output.unwrap_or_else(|| {
                    let now = chrono::Local::now();
                    PathBuf::from(format!("animatix_{}.mp4", now.format("%y%m%d_%H%M_%S")))
                });
                println!(
                    "Output configuration: {}x{} at {} FPS for {} seconds -> {}",
                    width,
                    height,
                    fps,
                    duration,
                    output_file.display()
                );
                renderer::render_video(&ast, width, height, fps, duration, &output_file);
            } else {
                eprintln!("Failed to generate AST.");
                std::process::exit(1);
            }
        }
    }
}
