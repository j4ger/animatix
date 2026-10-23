//! Code actions: diagnostic quick fixes and source-wide unused-declaration
//! cleanups.

use std::collections::HashMap;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::server::Backend;

pub(crate) async fn code_action(
    backend: &Backend,
    params: CodeActionParams,
) -> Result<Option<CodeActionResponse>> {
    let uri = params.text_document.uri.to_string();
    let requested = params.range;

    let analyzers = backend.analyzers.lock().await;
    let Some(analyzer) = analyzers.get(&uri) else {
        return Ok(None);
    };

    let mut actions: Vec<CodeActionOrCommand> = Vec::new();
    let url = match Url::parse(&uri) {
        Ok(url) => url,
        Err(_) => return Ok(None),
    };

    // Diagnostics are computed once per request and reused by every
    // action family below: each pass is a full re-check (~0.4ms on a
    // large file), and the cleanup actions need the same set.
    let diagnostics = analyzer.diagnostics();

    for diagnostic in diagnostics.iter() {
        // Only act on diagnostics the client actually asked about: the
        // request carries the visible range, and offering a fix for an
        // off-screen problem would apply an edit the user never saw.
        if !range_contains(requested, diagnostic) {
            continue;
        }
        let Some(code) = diagnostic.code.as_deref() else {
            continue;
        };

        match code {
            "undefined-label" => {
                if let Some(misspelled) = misspelled_name(diagnostic) {
                    let candidates =
                        animatix_analyzer::suggest_label_names(analyzer.symbols(), &misspelled, 3);
                    // A single unambiguous correction is the preferred fix;
                    // several candidates leave the choice to the author.
                    let unambiguous = candidates.len() == 1;
                    for candidate in candidates {
                        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                            title: format!("Replace with '{}'", candidate.name),
                            kind: Some(CodeActionKind::QUICKFIX),
                            diagnostics: None,
                            edit: Some(replace_range_edit(&url, diagnostic, &candidate.name)),
                            command: None,
                            is_preferred: unambiguous.then_some(true),
                            disabled: None,
                            data: None,
                        }));
                    }
                }
                if let Some(name) = undefined_label_name(diagnostic) {
                    if let Some(action) = declare_actor_action(analyzer.source(), &url, &name) {
                        actions.push(CodeActionOrCommand::CodeAction(action));
                    }
                }
            },
            "unknown-action" => {
                if let Some(misspelled) = misspelled_name(diagnostic) {
                    let candidates =
                        animatix_analyzer::suggest_action_names(analyzer.symbols(), &misspelled, 3);
                    let unambiguous = candidates.len() == 1;
                    for candidate in candidates {
                        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                            title: format!("Replace with '{}'", candidate.name),
                            kind: Some(CodeActionKind::QUICKFIX),
                            diagnostics: None,
                            edit: Some(replace_range_edit(&url, diagnostic, &candidate.name)),
                            command: None,
                            is_preferred: unambiguous.then_some(true),
                            disabled: None,
                            data: None,
                        }));
                    }
                }
            },
            "unused-label" => {
                if let Some(name) = unused_name(diagnostic) {
                    if let Some(action) = remove_declaration_action(analyzer, &url, &name) {
                        actions.push(CodeActionOrCommand::CodeAction(action));
                    }
                }
            },
            // Other codes have no quick fix to offer yet.
            _ => {},
        }
    }

    // A file-wide cleanup is offered when the document has several unused
    // top-level declarations: deleting them one at a time is tedious on
    // content like a dashboard with dozens of retired actors.
    if let Some(action) = remove_all_unused_action(analyzer, &diagnostics, &url) {
        actions.push(CodeActionOrCommand::CodeAction(action));
    }
    // A cleanup spanning several open documents gets its own action, so
    // the user does not have to run the per-file one in each tab.
    if let Some(action) = remove_all_unused_workspace_action(&analyzers, &diagnostics) {
        actions.push(CodeActionOrCommand::CodeAction(action));
    }

    if actions.is_empty() {
        Ok(None)
    } else {
        Ok(Some(actions))
    }
}

/// True when `diagnostic`'s range falls inside `range`.
///
/// A client asks for actions over the range the user is looking at; offering
/// an edit outside it would apply a change the user never saw.
fn range_contains(range: Range, diagnostic: &animatix_analyzer::Diagnostic) -> bool {
    let start = Position::new(diagnostic.line as u32, diagnostic.col as u32);
    let end = Position::new(diagnostic.end_line as u32, diagnostic.end_col as u32);
    position_le(range.start, start) && position_le(end, range.end)
}

fn position_le(a: Position, b: Position) -> bool {
    (a.line, a.character) <= (b.line, b.character)
}

/// A single-range replace edit wrapped in a `WorkspaceEdit`.
fn replace_range_edit(
    url: &Url,
    diagnostic: &animatix_analyzer::Diagnostic,
    new_text: &str,
) -> WorkspaceEdit {
    let edit = TextEdit {
        range: Range::new(
            Position::new(diagnostic.line as u32, diagnostic.col as u32),
            Position::new(diagnostic.end_line as u32, diagnostic.end_col as u32),
        ),
        new_text: new_text.to_string(),
    };
    let mut changes = HashMap::new();
    changes.insert(url.clone(), vec![edit]);
    WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    }
}

/// Extract the offending name from a spelling-related diagnostic message.
///
/// The analyzer phrases these as `Undefined label: NAME` and `Unknown action:
/// NAME`; anything else yields `None` so unrelated diagnostics are skipped.
fn misspelled_name(diagnostic: &animatix_analyzer::Diagnostic) -> Option<String> {
    const PREFIXES: [&str; 2] = ["Undefined label: ", "Unknown action: "];
    for prefix in PREFIXES {
        if let Some(rest) = diagnostic.message.strip_prefix(prefix) {
            let name = rest.trim();
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// Extract the name from an `undefined-label` diagnostic.
fn undefined_label_name(diagnostic: &animatix_analyzer::Diagnostic) -> Option<String> {
    diagnostic
        .message
        .strip_prefix("Undefined label: ")
        .map(|rest| rest.trim().to_string())
        .filter(|name| !name.is_empty())
}

/// Extract the name from an `unused-label` diagnostic (`Unused actor: 'x'`,
/// `Unused binding: 'x'`, …).
fn unused_name(diagnostic: &animatix_analyzer::Diagnostic) -> Option<String> {
    for kind in ["actor", "binding", "label", "component"] {
        let prefix = format!("Unused {kind}: '");
        if let Some(rest) = diagnostic.message.strip_prefix(prefix.as_str()) {
            return rest.strip_suffix('\'').map(str::to_string);
        }
    }
    None
}

/// Build the "declare the missing actor" quick fix.
///
/// Inserts the declaration before the first keyframe (or `always` block, or
/// end of file) so it precedes every use the author is reaching for.
fn declare_actor_action(source: &str, url: &Url, name: &str) -> Option<CodeAction> {
    let line = animatix_analyzer::declaration_insertion_line(source);
    let edit = TextEdit {
        range: Range::new(Position::new(line as u32, 0), Position::new(line as u32, 0)),
        new_text: animatix_analyzer::missing_actor_statement(name),
    };
    let mut changes = HashMap::new();
    changes.insert(url.clone(), vec![edit]);
    Some(CodeAction {
        title: format!("Declare actor '{name}'"),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: None,
        disabled: None,
        data: None,
    })
}

/// Build the "remove the unused declaration" quick fix.
///
/// The removal range covers the whole statement (brace-matched), and is only
/// offered for actors and `let` bindings — unused components and other kinds
/// may be intentional scaffolding a quick fix should not delete.
fn remove_declaration_action(
    analyzer: &animatix_analyzer::Analyzer,
    url: &Url,
    name: &str,
) -> Option<CodeAction> {
    let decl_byte = animatix_analyzer::first_declaration_byte(analyzer.occurrences(), name)?;
    // Only top-level declarations are safe to remove: an inline child (an
    // effect stage inside a Filter, an actor inside a Row) renders as part of
    // its parent even when its label is never referenced, so deleting it
    // would silently change the output.
    if !animatix_analyzer::is_top_level_position(analyzer.source(), decl_byte) {
        return None;
    }
    let (start_byte, end_byte) =
        animatix_analyzer::statement_removal_range(analyzer.source(), decl_byte)?;
    let (start_line, start_col) =
        animatix_syntax::token::byte_to_line_col(analyzer.source(), start_byte);
    let (end_line, end_col) = animatix_syntax::token::byte_to_line_col(analyzer.source(), end_byte);
    let edit = TextEdit {
        range: Range::new(
            Position::new(start_line as u32, start_col as u32),
            Position::new(end_line as u32, end_col as u32),
        ),
        new_text: String::new(),
    };
    let mut changes = HashMap::new();
    changes.insert(url.clone(), vec![edit]);
    Some(CodeAction {
        title: format!("Remove unused '{name}'"),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: None,
        disabled: None,
        data: None,
    })
}

/// Build the file-wide "remove all unused declarations" action.
///
/// Only offered when more than one top-level declaration is unused, and only
/// for the removals the analyzer confirms are safe (top-level statements with
/// a determinable extent). Edits are emitted in reverse source order so a
/// client applying them sequentially does not shift the later ranges.
/// How many `unused-label` diagnostics a document reports.
fn unused_label_count(diagnostics: &[animatix_analyzer::Diagnostic]) -> usize {
    diagnostics.iter().filter(|d| d.code.as_deref() == Some("unused-label")).count()
}

/// Removal edits for one document's unused top-level declarations.
///
/// Empty when the document has fewer than two removable declarations.
fn unused_removal_edits(
    analyzer: &animatix_analyzer::Analyzer,
    diagnostics: &[animatix_analyzer::Diagnostic],
) -> Vec<TextEdit> {
    let names: Vec<String> = diagnostics
        .iter()
        .filter(|d| d.code.as_deref() == Some("unused-label"))
        .filter_map(unused_name)
        .collect();
    if names.len() < 2 {
        return Vec::new();
    }
    let mut ranges =
        animatix_analyzer::batch_removal_ranges(analyzer.occurrences(), analyzer.source(), &names);
    if ranges.len() < 2 {
        return Vec::new();
    }
    // Reverse order: each edit's range stays valid as earlier ones are applied.
    ranges.sort_unstable_by(|a, b| b.0.cmp(&a.0));

    let source = analyzer.source();
    ranges
        .into_iter()
        .map(|(start, end)| {
            let (start_line, start_col) = animatix_syntax::token::byte_to_line_col(source, start);
            let (end_line, end_col) = animatix_syntax::token::byte_to_line_col(source, end);
            TextEdit {
                range: Range::new(
                    Position::new(start_line as u32, start_col as u32),
                    Position::new(end_line as u32, end_col as u32),
                ),
                new_text: String::new(),
            }
        })
        .collect()
}

/// Build the "remove all unused declarations" action for one document.
fn remove_all_unused_action(
    analyzer: &animatix_analyzer::Analyzer,
    diagnostics: &[animatix_analyzer::Diagnostic],
    url: &Url,
) -> Option<CodeAction> {
    let edits = unused_removal_edits(analyzer, diagnostics);
    if edits.is_empty() {
        return None;
    }
    let count = edits.len();
    Some(source_action(
        format!("Remove all {count} unused declarations"),
        single_file_edit(url, edits),
    ))
}

/// Build the workspace-wide "remove all unused declarations" action.
///
/// Offered only when the cleanup spans more than one document, so it does not
/// duplicate the per-file action in the common single-file case.
fn remove_all_unused_workspace_action(
    analyzers: &HashMap<String, animatix_analyzer::Analyzer>,
    current_diagnostics: &[animatix_analyzer::Diagnostic],
) -> Option<CodeAction> {
    // The action only exists when the cleanup spans documents, so the current
    // document has to qualify first. Checking that before touching any other
    // file keeps the common case (nothing to clean here) at the cost of one
    // document instead of the whole session — diagnostics are ~0.4ms per
    // file, which is ~11ms across 30 open documents.
    if unused_label_count(current_diagnostics) < 2 {
        return None;
    }

    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    let mut total = 0usize;
    for (uri, analyzer) in analyzers {
        let document_diagnostics = analyzer.diagnostics();
        let edits = unused_removal_edits(analyzer, &document_diagnostics);
        if edits.is_empty() {
            continue;
        }
        let Ok(url) = Url::parse(uri) else {
            continue;
        };
        total += edits.len();
        changes.insert(url, edits);
    }
    if changes.len() < 2 {
        return None;
    }
    Some(source_action(
        format!("Remove all {total} unused declarations in {} files", changes.len()),
        WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        },
    ))
}

/// Wrap an edit in a `source`-kind code action.
fn source_action(title: String, edit: WorkspaceEdit) -> CodeAction {
    CodeAction {
        title,
        // A source action so it is reachable from the file-level menu rather
        // than only from a diagnostic's lightbulb.
        kind: Some(CodeActionKind::SOURCE),
        diagnostics: None,
        edit: Some(edit),
        command: None,
        is_preferred: None,
        disabled: None,
        data: None,
    }
}

/// A `WorkspaceEdit` touching one document.
fn single_file_edit(url: &Url, edits: Vec<TextEdit>) -> WorkspaceEdit {
    let mut changes = HashMap::new();
    changes.insert(url.clone(), edits);
    WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    }
}
