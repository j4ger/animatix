fn main() {
    let src_dir = std::path::Path::new("../../tree-sitter-animatix/src");

    let mut cc_build = cc::Build::new();
    cc_build.include(src_dir);
    cc_build.file(src_dir.join("parser.c"));
    cc_build.file(src_dir.join("scanner.c"));

    // glibc's _FORTIFY_SOURCE guard warns when C is compiled without optimization.
    cc_build.opt_level(2);

    // Suppress warnings from generated C code
    cc_build.flag_if_supported("-Wno-unused-parameter");
    cc_build.flag_if_supported("-Wno-unused-but-set-variable");

    cc_build.compile("tree-sitter-animatix");

    // Ensure cargo rebuilds when the grammar or queries change
    println!("cargo:rerun-if-changed=../../tree-sitter-animatix/grammar.js");
    println!("cargo:rerun-if-changed=../../tree-sitter-animatix/src/parser.c");
    println!("cargo:rerun-if-changed=../../tree-sitter-animatix/src/scanner.c");
    println!("cargo:rerun-if-changed=../../tree-sitter-animatix/queries/highlights.scm");
}
