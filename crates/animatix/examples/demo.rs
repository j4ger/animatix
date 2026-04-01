use animatix::parser::parser;
use chumsky::Parser;
use std::fs;
use std::path::Path;

fn main() {
    let paths_to_try = [
        "animatix/examples/demo.amx",        // Workspace root
        "crates/animatix/examples/demo.amx", // Project root
        "examples/demo.amx",                 // Crate root
        "demo.amx",                          // Examples directory
    ];

    let mut src = None;

    for path in paths_to_try {
        if Path::new(path).exists() {
            src = Some(fs::read_to_string(path).unwrap_or_else(|e| {
                panic!("Failed to read {}: {}", path, e);
            }));
            break;
        }
    }

    let src = src.expect("Could not find demo.amx in any of the expected locations.");

    println!("Parsing Animatix code:\n{}", src);

    let (ast, errs) = parser().parse(src.as_str()).into_output_errors();

    if let Some(ast) = ast {
        println!("\nAbstract Syntax Tree:");
        for stmt in &ast {
            println!("{:#?}", stmt);
        }

        println!("\nStarting renderer...");
        animatix::renderer::run(&ast);
    }

    if !errs.is_empty() {
        println!("\nErrors:");
        for err in errs {
            println!("{:?}", err);
        }
    }
}
