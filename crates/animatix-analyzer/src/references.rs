//! Find-references provider.

use tree_sitter::Tree;

/// Find all references to a symbol name in a file.
/// Returns a list of (start_line, start_col, end_line, end_col) ranges.
pub fn find_references(
    tree: Option<&Tree>,
    source: &str,
    symbol_name: &str,
) -> Vec<(usize, usize, usize, usize)> {
    let mut refs = Vec::new();

    if let Some(tree) = tree {
        let mut cursor = tree.walk();
        collect_references(&mut cursor, source, symbol_name, &mut refs);
    }

    refs
}

fn collect_references(
    cursor: &mut tree_sitter::TreeCursor,
    source: &str,
    symbol_name: &str,
    refs: &mut Vec<(usize, usize, usize, usize)>,
) {
    let node = cursor.node();

    if node.kind() == "identifier" && node.utf8_text(source.as_bytes()).unwrap_or("") == symbol_name
    {
        refs.push((
            node.start_position().row,
            node.start_position().column,
            node.end_position().row,
            node.end_position().column,
        ));
    }

    if cursor.goto_first_child() {
        loop {
            collect_references(cursor, source, symbol_name, refs);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}
