//! Tree-sitter grammar for the Animatix animation DSL.

use tree_sitter::Language;

unsafe extern "C" {
    fn tree_sitter_animatix() -> Language;
}

/// Get the tree-sitter Language for Animatix.
pub fn language() -> Language {
    unsafe { tree_sitter_animatix() }
}

/// The syntax highlighting query for Animatix.
pub const HIGHLIGHTS_QUERY: &str = include_str!("../../../tree-sitter-animatix/queries/highlights.scm");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_load_language() {
        let lang = language();
        // Just verify we can load the language without panicking
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&lang).unwrap();
    }

    #[test]
    fn can_parse_simple_input() {
        let lang = language();
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&lang).unwrap();

        let source = r#"
# 0s
title: Text {
    content: "Hello",
    position: (400, 300),
}
"#;
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();
        assert!(!root.has_error());
    }
}
