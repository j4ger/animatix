//! Rename support: decide whether a symbol can be renamed and where.
//!
//! Rename safety rests on three things the analyzer already has:
//! scope-aware reference resolution ([`crate::references::find_references_at`]),
//! the parser-recorded occurrence role of the token under the cursor, and the
//! symbol table's visibility flags. This module combines them into one
//! decision so the LSP (and any other consumer) does not re-derive the rules.

use animatix_syntax::occurrence::{Occurrence, OccurrenceKind};
use animatix_syntax::token::{byte_to_line_col, line_col_to_byte};

use crate::symbol_table::SymbolTable;

/// A rename request resolved to concrete source ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameTarget {
    /// The current name.
    pub name: String,
    /// The range of the identifier under the cursor: `(line, col, end_line,
    /// end_col)`, 0-based.
    pub range: (usize, usize, usize, usize),
    /// Every occurrence to rewrite, including the declaration, as 0-based
    /// `(line, col, end_line, end_col)` ranges.
    pub references: Vec<(usize, usize, usize, usize)>,
    /// Whether the symbol is exported (`pub`), so a cross-file rename may be
    /// needed and a single-file edit could leave importers broken.
    pub is_public: bool,
}

/// Why a symbol cannot be renamed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameRejection {
    /// The cursor is not on an identifier.
    NotAnIdentifier,
    /// The cursor is on a kind whose name is language vocabulary, not a
    /// user-chosen binding.
    NotRenameable {
        /// The role that made it non-renameable, for the message.
        role: &'static str,
    },
}

impl RenameRejection {
    /// A short human-readable reason.
    pub fn message(&self) -> String {
        match self {
            RenameRejection::NotAnIdentifier => "No symbol under the cursor".to_string(),
            RenameRejection::NotRenameable { role } => {
                format!("Cannot rename {role}: it is language vocabulary, not a declared name")
            },
        }
    }
}

/// Check whether `new_name` is usable as a rename target.
///
/// The DSL's identifier shape is a leading letter followed by letters, digits,
/// `_`, or `-` (actions are hyphenated), which is what this enforces.
pub fn validate_new_name(new_name: &str) -> Result<(), String> {
    if new_name.is_empty() {
        return Err("New name must not be empty".to_string());
    }
    let mut chars = new_name.chars();
    let first = chars.next().expect("non-empty checked above");
    if !(first.is_alphabetic() || first == '_') {
        return Err(format!("New name must start with a letter or `_`, got `{first}`"));
    }
    if let Some(bad) = chars.find(|c| !(c.is_alphanumeric() || *c == '_' || *c == '-')) {
        return Err(format!("New name must not contain `{bad}`"));
    }
    Ok(())
}

/// Map an occurrence role to a rejection reason when it is not renameable.
fn non_renameable_role(kind: OccurrenceKind) -> Option<&'static str> {
    match kind {
        OccurrenceKind::Label
        | OccurrenceKind::Variable
        | OccurrenceKind::Component
        | OccurrenceKind::Parameter
        | OccurrenceKind::ImportAlias
        | OccurrenceKind::TypeAlias => None,
        OccurrenceKind::Type => Some("a type name"),
        OccurrenceKind::Property => Some("a property name"),
        OccurrenceKind::Function => Some("a function name"),
        OccurrenceKind::Action => Some("an action verb"),
        OccurrenceKind::Scene => Some("a scene name"),
        OccurrenceKind::Wildcard => Some("a wildcard"),
    }
}

/// Resolve a rename at `(line, col)`.
///
/// Returns the ranges to rewrite, or a rejection explaining why the symbol
/// cannot be renamed.
pub fn rename_at(
    occurrences: &[Occurrence],
    symbols: &SymbolTable,
    source: &str,
    line: usize,
    col: usize,
) -> Result<RenameTarget, RenameRejection> {
    let byte = line_col_to_byte(source, line, col);
    let Some(occurrence) = occurrences.iter().find(|o| byte >= o.span.start && byte < o.span.end)
    else {
        return Err(RenameRejection::NotAnIdentifier);
    };

    if let Some(role) = non_renameable_role(occurrence.kind) {
        return Err(RenameRejection::NotRenameable { role });
    }
    // A bare action verb in statement position (`fade-in box [1s]`) is recorded
    // as a `Label` declaration by the parser, so the role alone cannot tell it
    // apart from a real actor binding. Action verbs are language vocabulary —
    // renaming one would break every call site.
    if symbols.actions.contains(&occurrence.name) {
        return Err(RenameRejection::NotRenameable {
            role: "an action verb",
        });
    }

    let name = occurrence.name.clone();
    let references = crate::references::find_references_at(occurrences, source, line, col);
    if references.is_empty() {
        return Err(RenameRejection::NotAnIdentifier);
    }
    let (start_line, start_col) = byte_to_line_col(source, occurrence.span.start);
    let (end_line, end_col) = byte_to_line_col(source, occurrence.span.end);
    let is_public = symbols.labels.get(&name).is_some_and(|info| info.is_pub)
        || symbols.components.get(&name).is_some_and(|info| info.is_pub);

    Ok(RenameTarget {
        name,
        range: (start_line, start_col, end_line, end_col),
        references,
        is_public,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Analyzer;

    fn rename(source: &str, line: usize, col: usize) -> Result<RenameTarget, RenameRejection> {
        let analyzer = Analyzer::new(source);
        rename_at(analyzer.occurrences(), analyzer.symbols(), source, line, col)
    }

    #[test]
    fn renames_only_the_binding_under_the_cursor() {
        // Three separate `x` bindings; renaming one must not touch the others.
        let source = r#"let x = 1
let mapper = (x) => x + 1
outer = x
"#;
        // Cursor on the closure parameter (line 1, col 14).
        let target = rename(source, 1, 14).expect("closure param is renameable");
        assert_eq!(target.name, "x");
        assert_eq!(target.references.len(), 2, "param + its body reference");

        // Cursor on the top-level `let x` (line 0, col 4).
        let top = rename(source, 0, 4).expect("top-level let is renameable");
        assert_eq!(top.references.len(), 2, "declaration + the `outer = x` read");
    }

    #[test]
    fn rejects_language_vocabulary() {
        let source = "title: Text, text: \"hi\"\n";
        // On the type name `Text` (line 0, col 7).
        let err = rename(source, 0, 7).expect_err("type names are not renameable");
        assert!(
            matches!(err, RenameRejection::NotRenameable { role } if role.contains("type")),
            "unexpected rejection: {err:?}"
        );
        // On the property name `text` (line 0, col 13).
        let err = rename(source, 0, 13).expect_err("property names are not renameable");
        assert!(matches!(err, RenameRejection::NotRenameable { .. }), "{err:?}");
    }

    #[test]
    fn rejects_non_identifiers() {
        let source = "title: Text, text: \"hi\"\n";
        // On whitespace.
        let err = rename(source, 0, 6).expect_err("whitespace is not a symbol");
        assert_eq!(err, RenameRejection::NotAnIdentifier);
    }

    #[test]
    fn reports_public_symbols() {
        let source = "pub let accent = rgb(1, 0, 0)\nbox = accent\n";
        let target = rename(source, 0, 8).expect("pub let is renameable");
        assert!(target.is_public, "pub binding is flagged as exported");
    }

    #[test]
    fn action_verbs_are_not_renameable() {
        let source = "box: Rect, size: (10, 10)\n#0s\nfade-in box [1s]\n";
        // On `fade-in` (line 2, col 1).
        let err = rename(source, 2, 1).expect_err("action verbs are not renameable");
        assert!(
            matches!(err, RenameRejection::NotRenameable { role } if role.contains("action")),
            "unexpected rejection: {err:?}"
        );
    }

    #[test]
    fn new_name_validation() {
        assert!(validate_new_name("card_title").is_ok());
        assert!(validate_new_name("fade-in").is_ok(), "hyphenated names are legal");
        assert!(validate_new_name("").is_err());
        assert!(validate_new_name("1abc").is_err(), "must not start with a digit");
        assert!(validate_new_name("a b").is_err(), "spaces are not allowed");
        assert!(validate_new_name("a.b").is_err(), "dots are not allowed");
    }
}
