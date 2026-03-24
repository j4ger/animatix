use animatix::parser::parser;
use chumsky::Parser;
use clap::Parser as ClapParser;
use std::fs;
use std::path::PathBuf;

#[derive(ClapParser, Debug)]
#[command(
    author,
    version,
    about,
    long_about = "Animatix Language Compiler and AST Viewer"
)]
struct Args {
    /// The input Animatix scene file (.amx)
    input: PathBuf,

    /// Format AST output on a single line instead of pretty-printing
    #[arg(short, long)]
    compact: bool,

    /// Print AST even if parsing errors occurred
    #[arg(short, long)]
    force: bool,
}

fn main() {
    let args = Args::parse();

    let src = match fs::read_to_string(&args.input) {
        Ok(content) => content,
        Err(e) => {
            eprintln!(
                "Error: Failed to read input file '{}': {}",
                args.input.display(),
                e
            );
            std::process::exit(1);
        }
    };

    println!("Parsing Animatix file: {}", args.input.display());

    // Parse the source code using the parser defined in the library
    let (ast, errs) = parser().parse(src.as_str()).into_output_errors();

    let has_errors = !errs.is_empty();

    if let Some(ast) = ast {
        if !has_errors || args.force {
            println!("\nAbstract Syntax Tree:");
            if args.compact {
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
