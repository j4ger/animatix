use std::path::Path;

use animatix_syntax::module::ModuleGraph;
use tracing::{error, info};

pub fn run(input: &Path, compact: bool) {
    info!("Parsing Animatix file: {}", input.display());
    let ast = match ModuleGraph::new().load_entry(input) {
        Ok(statements) => statements,
        Err(e) => {
            error!("Error: {}", e);
            std::process::exit(1);
        },
    };
    if compact {
        println!("{:?}", ast);
    } else {
        for stmt in &ast {
            println!("{:#?}", stmt);
        }
    }
}
