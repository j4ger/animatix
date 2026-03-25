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
    }
}
