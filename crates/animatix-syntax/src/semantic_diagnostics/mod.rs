//! Canonical semantic diagnostics emitted from the syntax layer.
//!
//! The analyzer and LSP convert these into their transport DTOs instead of
//! re-implementing label/property/type checks.

use std::collections::HashSet;

use crate::ast::{InlineItem, Span, Stmt, is_array_member_label};
use crate::diagnostics::{
    Diagnostic, DiagnosticCode, DiagnosticLocation, DiagnosticPhase, DiagnosticSeverity,
};
use crate::symbol_table::{LabelKind, SymbolTable};
use crate::token::{Token, TokenKind, byte_to_line_col};
use crate::walk;

/// Plot family actors accept declaration-time runtime parameters — numeric
/// properties whose names match identifiers inside the actor's `func`
/// closure (e.g. `freq: 2` on `func: (x) => sin(freq * x)`). These cannot be
/// enumerated in the static property table, so the unknown-property info is
/// suppressed for them.
fn is_plot_runtime_param(ty: &str, prop_name: &str, props: &[crate::ast::Property]) -> bool {
    plot_runtime_params(ty, props).iter().any(|p| p == prop_name)
}

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
            // ordinary source, not dead code. This used to report as a warning
            // and fired 347 times across this repo's own `examples/`, `web/`
            // and `dogfood/` — every one of them on an actor, which trained
            // authors to skim the whole diagnostic list. It stays available as
            // a hint, because the case it genuinely catches ("this was meant to
            // be wired to something") is invisible to any other check:
            // `never-revealed` covers actors that never show up, and nothing
            // covers `clip_shape` declared as a mask source and never used.
            // A `let` nobody reads has no such defence — it is simply dead.
            let severity = if info.kind == LabelKind::Actor {
                DiagnosticSeverity::Hint
            } else {
                DiagnosticSeverity::Warning
            };
            diagnostics.push(span_diagnostic(
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
            ));
        }
    }

    walk::walk_stmts(stmts, &mut |stmt| {
        check_stmt(stmt, symbols, tokens, source, &mut diagnostics);
    });

    diagnostics
}

fn span_diagnostic(
    severity: DiagnosticSeverity,
    code: DiagnosticCode,
    message: String,
    line: usize,
    col: usize,
    end_col: usize,
) -> Diagnostic {
    Diagnostic {
        severity,
        phase: DiagnosticPhase::Build,
        code,
        message,
        location: DiagnosticLocation {
            line: Some(line),
            column: Some(col),
            end_line: Some(line),
            end_col: Some(end_col),
            span: None,
            path: None,
            subject: None,
        },
    }
}

fn range_diagnostic(
    severity: DiagnosticSeverity,
    code: DiagnosticCode,
    message: String,
    start: (usize, usize),
    end: (usize, usize),
) -> Diagnostic {
    Diagnostic {
        severity,
        phase: DiagnosticPhase::Build,
        code,
        message,
        location: DiagnosticLocation {
            line: Some(start.0 + 1),
            column: Some(start.1 + 1),
            end_line: Some(end.0 + 1),
            end_col: Some(end.1 + 1),
            span: None,
            path: None,
            subject: None,
        },
    }
}

fn span_positions(span: &Option<Span>) -> (usize, usize, usize, usize) {
    match span {
        Some(s) => (s.start_line, s.start_col, s.end_line, s.end_col),
        None => (1, 1, 1, 1),
    }
}

/// Positions for a property diagnostic: the property's own value span when
/// the parser recorded one, falling back to the actor declaration position.
///
/// Returns 1-based `(line, col, end_line, end_col)`, matching the diagnostic
/// location convention used by this module.
fn property_positions(
    prop: &crate::ast::Property,
    source: &str,
    actor_line: usize,
    actor_col: usize,
    actor_end_col: usize,
) -> (usize, usize, usize, usize) {
    let Some(byte_span) = prop.value_span else {
        return (actor_line, actor_col, actor_line, actor_end_col);
    };
    if source.is_empty() || byte_span.end > source.len() {
        return (actor_line, actor_col, actor_line, actor_end_col);
    }
    let span = Span::from_byte_span(source, byte_span);
    (span.start_line, span.start_col, span.end_line, span.end_col)
}

fn is_component_array_member(symbols: &SymbolTable, label: &str) -> bool {
    let Some(base) = is_array_member_label(label) else {
        return false;
    };
    let Some((instance, _)) = base.rsplit_once('.') else {
        return false;
    };
    let Some(info) = symbols.labels.get(instance) else {
        return false;
    };
    info.ty.as_deref().is_some_and(|ty| symbols.components.contains_key(ty))
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
                {
                    if !children.is_empty() && is_structural_container_type(ty) {
                        structural.insert(label.clone());
                    }
                }
            });
        }
    });
    structural
}

/// Detect a `scope.stage.param = value` assignment so the caller can skip
/// host-property validation.
///
/// The shape is: at least two target segments with the last naming an inline
/// child (a stage). Whatever that stage's effect declares, the property is a
/// *stage parameter*, not a property of the host actor — validating
/// `panel.pix.size = 6` against `Filter.size` (Vec2) produced false
/// `type-mismatch` warnings for every plugin effect whose parameter list is
/// not in the static schema (e.g. `Pixelate` before its library loads).
///
/// Returns the stage label when the shape matches.
fn stage_param_segment(
    target: &[crate::ast::TargetSegment],
    symbols: &SymbolTable,
    _property: &str,
) -> Option<String> {
    if target.len() < 2 {
        return None;
    }
    let label = target.last()?.label_str();
    let stage_ty = symbols.inline_child_type(label)?;
    // A recognized effect is validated by the effect branch above; an
    // unrecognized stage type is left to the unknown-type diagnostic.
    if symbols.is_effect_type(stage_ty) {
        return None;
    }
    Some(label.to_string())
}

fn find_ident_range(
    tokens: &[Token],
    source: &str,
    text: &str,
    range: Option<std::ops::Range<usize>>,
) -> Option<((usize, usize), (usize, usize))> {
    tokens
        .iter()
        .filter(|t| range.as_ref().is_none_or(|r| t.span.start >= r.start && t.span.end <= r.end))
        .find(|t| matches!(&t.kind, TokenKind::Ident(name) if name == text))
        .map(|t| {
            let start = byte_to_line_col(source, t.span.start);
            let end = byte_to_line_col(source, t.span.end);
            (start, end)
        })
}

/// Locate the `Type` identifier in a `label: Type` declaration pair.
///
/// Matching on the label first keeps several declarations that share a type
/// (`dot: Pulse`, `ring: Pulse`, `cross: Pulse`) pointing at their own line
/// instead of all resolving to the first occurrence of the type name.
/// Returns 0-based `((line, col), (line, col))`.
fn find_decl_type_range(
    tokens: &[Token],
    source: &str,
    label: &str,
    ty: &str,
) -> Option<((usize, usize), (usize, usize))> {
    let label_index = tokens
        .iter()
        .position(|t| matches!(&t.kind, TokenKind::Ident(name) if name == label))?;
    // Scan forward for `: Type` immediately after the label.
    let mut index = label_index + 1;
    while index + 1 < tokens.len() {
        if matches!(tokens[index].kind, TokenKind::Colon) {
            if let TokenKind::Ident(name) = &tokens[index + 1].kind {
                if name == ty {
                    let token = &tokens[index + 1];
                    return Some((
                        byte_to_line_col(source, token.span.start),
                        byte_to_line_col(source, token.span.end),
                    ));
                }
            }
            break; // a different type name on this declaration
        }
        if matches!(tokens[index].kind, TokenKind::Ident(_)) {
            break; // moved past the label without finding a colon
        }
        index += 1;
    }
    None
}

/// Return the source-level identifier text for a resolved target string.
fn target_source_text(target: &str) -> &str {
    let first = target.split('.').next().unwrap_or(target);
    is_array_member_label(first).unwrap_or(first)
}

mod checks;
#[cfg(test)]
mod tests;

use checks::check_stmt;
