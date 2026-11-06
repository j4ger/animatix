use std::collections::HashSet;

use crate::ast::{InlineItem, Stmt};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticSeverity};
use crate::symbol_table::{LabelKind, SymbolTable};
use crate::token::Token;
use crate::walk;

use super::checks::check_stmt;
use super::spans::span_diagnostic;

/// Collect the runtime-parameter names a plot-family actor's declaration
/// implies: every name that appears as a closure parameter or free identifier
/// inside its `func` property. Also consumed by `SymbolTable` (and the
/// analyzer's unresolved-variable check) so assignment targets and bare
/// references resolve against the same set.
pub fn plot_runtime_params(ty: &str, props: &[crate::ast::Property]) -> Vec<String> {
    let is_plot_host =
        animatix_std::caps_for_type(ty).is_some_and(|caps| caps.plot_geometry || caps.plot_host);
    if !is_plot_host {
        return Vec::new();
    }
    let Some(func_prop) = props.iter().find(|p| p.name == "func") else {
        return Vec::new();
    };
    // A declared property counts as a runtime parameter when the func closure
    // body references it by name (`freq: 2` on `func: (x) => sin(freq * x)`).
    // references_ident recurses through calls, so `sin(freq * x)` finds `freq`.
    props
        .iter()
        .filter(|p| p.name != "func")
        .filter(|p| func_prop.value.references_ident(&p.name))
        .map(|p| p.name.clone())
        .collect()
}

/// True for container types whose label may be purely structural.
fn is_structural_container_type(ty: &str) -> bool {
    animatix_std::caps_for_type(ty).is_some_and(|caps| caps.is_container || caps.is_effect_scope())
}

/// Collect semantic diagnostics for a parsed program.
///
/// This is the single emitter for analyzer-style lint checks. Build/typecheck
/// diagnostics remain produced by the typechecker and timeline build.
/// Duplicate labels are detected in the analyzer layer (`duplicates.rs`),
/// which knows the keyframe/morph scoping rules; a plain `HashMap` iteration
/// can never observe a duplicate.
pub fn collect_semantic_diagnostics(
    stmts: &[Stmt],
    symbols: &SymbolTable,
    tokens: &[Token],
    source: &str,
) -> Vec<Diagnostic> {
    let structural_containers = collect_structural_container_labels(stmts);
    let mut diagnostics = Vec::new();

    for (name, info) in &symbols.labels {
        if !symbols.referenced_labels.contains(name) {
            if name == "self"
                || info.kind == LabelKind::For
                || info.kind == LabelKind::Always
                || info.is_pub
                || symbols.array_labels.contains(name)
                || symbols.component_internal_labels.contains(name)
                || structural_containers.contains(name)
            {
                continue;
            }
            // An unreferenced *actor* is not a defect: every declared actor
            // paints, so `caption: Text, text: "…"` that nothing animates is
            // ordinary source, not dead code. Reporting it as a warning fires
            // on nearly every example in this repo, and only ever on an actor,
            // which trains authors to skim the whole diagnostic list. It stays
            // available as a hint, because the case it genuinely catches ("this
            // was meant to be wired to something") is invisible to any other
            // check: `never-revealed` covers actors that never show up, and
            // nothing covers `clip_shape` declared as a mask source and never
            // used.
            // A `let` nobody reads has no such defence — it is simply dead.
            let severity = if info.kind == LabelKind::Actor {
                DiagnosticSeverity::Hint
            } else {
                DiagnosticSeverity::Warning
            };
            diagnostics.push(
                span_diagnostic(
                    severity,
                    DiagnosticCode::UnusedLabel,
                    format!(
                        "Unused {}: '{}'",
                        match info.kind {
                            LabelKind::Actor => "actor",
                            LabelKind::Let => "binding",
                            LabelKind::Component => "component",
                            _ => "label",
                        },
                        name
                    ),
                    info.line,
                    info.col,
                    info.col + name.len(),
                )
                .with_subject(name.clone()),
            );
        }
    }

    walk::walk_stmts(stmts, &mut |stmt| {
        check_stmt(stmt, symbols, tokens, source, &mut diagnostics);
    });

    diagnostics
}

fn collect_structural_container_labels(stmts: &[Stmt]) -> HashSet<String> {
    let mut structural = HashSet::new();
    crate::walk::walk_stmts(stmts, &mut |stmt| {
        if let Stmt::ActorDecl {
            label,
            ty,
            children,
            ..
        } = stmt
        {
            if !children.is_empty() && is_structural_container_type(ty) {
                structural.insert(label.clone());
            }
            crate::walk::walk_inline_items(children, &mut |item| {
                if let InlineItem::Labeled {
                    label,
                    ty,
                    children,
                    ..
                } = item
                    && !children.is_empty()
                    && is_structural_container_type(ty)
                {
                    structural.insert(label.clone());
                }
            });
        }
    });
    structural
}
