use animatix::module::ModuleGraph;
use animatix::renderer;
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
    },
    /// Render a scene to a video file
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
    },
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
        Commands::Render { input } => {
            println!("Rendering Animatix file: {}", input.display());

            let ast = match ModuleGraph::new().load_entry(&input) {
                Ok(statements) => statements,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            renderer::run(&ast);
        }
        Commands::Image {
            input,
            width,
            height,
            time,
            output,
        } => {
            println!("Rendering Animatix image: {}", input.display());

            let ast = match ModuleGraph::new().load_entry(&input) {
                Ok(statements) => statements,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            let output_file =
                output.unwrap_or_else(|| PathBuf::from(format!("animatix_{}s.png", time)));
            println!(
                "Output image: {}x{} at {}s -> {}",
                width,
                height,
                time,
                output_file.display()
            );
            renderer::render_image(&ast, width, height, time, &output_file);
        }
        Commands::Video {
            input,
            width,
            height,
            fps,
            duration,
            output,
        } => {
            println!("Rendering Animatix video: {}", input.display());

            let ast = match ModuleGraph::new().load_entry(&input) {
                Ok(statements) => statements,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

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
        }
    }
}
