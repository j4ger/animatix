//! Build-time Wasm partitioning and font metadata, never linked into a player.

mod fonts;
mod split;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("split") if args.len() == 4 => split::emit(&args[1], &args[2], &args[3]),
        Some("fonts") if args.len() == 2 => fonts::emit(&args[1]),
        _ => {
            Err("usage: animatix-web-build split ORIGINAL BINDGEN_DIR OUT | fonts FONT_DIR".into())
        },
    }
}
