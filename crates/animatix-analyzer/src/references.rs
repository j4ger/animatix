//! Find-references provider.

use animatix_syntax::occurrence::Occurrence;
use animatix_syntax::token::{byte_to_line_col, line_col_to_byte};

/// Find all references to the first declaration of `symbol_name`.
///
/// Callers that know the cursor position should prefer [`find_references_at`]
/// so shadowed bindings resolve to the declaration the user is actually on.
pub fn find_references(
    occurrences: &[Occurrence],
    source: &str,
    symbol_name: &str,
) -> Vec<(usize, usize, usize, usize)> {
    let target = occurrences
        .iter()
        .find(|o| o.declaration && o.name == symbol_name)
        .or_else(|| occurrences.iter().find(|o| o.name == symbol_name));
    let Some(target) = target else {
        return Vec::new();
    };
    references_for(occurrences, source, target)
}

/// Find all references to the binding at `(line, col)`, resolving through
/// lexical parent scopes when the occurrence is not itself a declaration.
pub fn find_references_at(
    occurrences: &[Occurrence],
    source: &str,
    line: usize,
    col: usize,
) -> Vec<(usize, usize, usize, usize)> {
    let byte = line_col_to_byte(source, line, col);
    let Some(target) = occurrences.iter().find(|o| byte >= o.span.start && byte < o.span.end)
    else {
        return Vec::new();
    };
    references_for(occurrences, source, target)
}

fn references_for(
    occurrences: &[Occurrence],
    source: &str,
    target: &Occurrence,
) -> Vec<(usize, usize, usize, usize)> {
    let binding_scope = resolve_scope(occurrences, target);
    occurrences
        .iter()
        .filter(|o| o.name == target.name && resolve_scope(occurrences, o) == binding_scope)
        .map(|o| {
            let (start_line, start_col) = byte_to_line_col(source, o.span.start);
            let (end_line, end_col) = byte_to_line_col(source, o.span.end);
            (start_line, start_col, end_line, end_col)
        })
        .collect()
}

/// Find the scope that declares `target.name`, walking outward through the
/// parser-recorded scope chain.
fn resolve_scope(occurrences: &[Occurrence], target: &Occurrence) -> Option<u32> {
    let mut scope = target.scope_id;
    let mut parent = target.parent_scope_id;
    while let Some(current) = scope {
        if occurrences
            .iter()
            .any(|o| o.declaration && o.scope_id == Some(current) && o.name == target.name)
        {
            return Some(current);
        }
        scope = parent;
        parent = occurrences.iter().find(|o| o.scope_id == scope).and_then(|o| o.parent_scope_id);
    }
    None
}
