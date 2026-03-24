use animatix::parser::parser;
use chumsky::Parser;

fn main() {
    let src = r#"
        #0s
        let x = 100
        btn: Button, text: "OK", at: (x, 200)

        #2s
        btn.color = "red"
    "#;

    println!("Parsing Animatix code:\n{}", src);

    // Parse the source code using the parser defined in the library
    let (ast, errs) = parser().parse(src).into_output_errors();

    if let Some(ast) = ast {
        println!("\nAbstract Syntax Tree:");
        for stmt in ast {
            println!("{:#?}", stmt);
        }
    }

    if !errs.is_empty() {
        println!("\nErrors:");
        for err in errs {
            println!("{:?}", err);
        }
    }
}
