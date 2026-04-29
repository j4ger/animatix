fn main() {
    let src_dir = std::path::Path::new("../../tree-sitter-animatix/src");

    let mut cc_build = cc::Build::new();
    cc_build.include(src_dir);
    cc_build.file(src_dir.join("parser.c"));

    // Suppress warnings from generated C code
    cc_build.flag_if_supported("-Wno-unused-parameter");
    cc_build.flag_if_supported("-Wno-unused-but-set-variable");

    cc_build.compile("tree-sitter-animatix");
}
