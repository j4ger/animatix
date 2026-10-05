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
    const PLOT_HOSTS: [&str; 4] = ["PlotCurve", "VectorField", "Heatmap", "ContourSet"];
    if !PLOT_HOSTS.contains(&ty) {
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

/// Built-in container types whose label may be purely structural.
const STRUCTURAL_CONTAINER_TYPES: &[&str] =
    &["Row", "Col", "Grid", "Stack", "Group", "Filter", "Mask"];

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
            if !children.is_empty() && STRUCTURAL_CONTAINER_TYPES.contains(&ty.as_str()) {
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
                    if !children.is_empty() && STRUCTURAL_CONTAINER_TYPES.contains(&ty.as_str()) {
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

fn check_stmt(
    stmt: &Stmt,
    symbols: &SymbolTable,
    tokens: &[Token],
    source: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match stmt {
        Stmt::Action(action, span) => {
            let (line, col, end_line, end_col) = span_positions(span);
            let byte_range = action.byte_span.map(|s| s.start..s.end);

            if !symbols.actions.contains(&action.verb) {
                let (vstart, vend) =
                    find_ident_range(tokens, source, &action.verb, byte_range.clone())
                        .unwrap_or_else(|| ((line - 1, col - 1), (end_line - 1, end_col - 1)));
                diagnostics.push(range_diagnostic(
                    DiagnosticSeverity::Warning,
                    DiagnosticCode::UnknownAction,
                    format!("Unknown action: {}", action.verb),
                    vstart,
                    vend,
                ));
            }

            for target in &action.targets {
                // Dotted targets (`eq.lhs`, `row.b[j]` projected to `row.b`) are
                // resolved by walking the scene hierarchy at build time, so only
                // the root label needs to be defined here — mirroring the
                // assignment-target check.
                let check_label = target.split('.').next().unwrap_or(target);
                // `persist camera` / `remove camera` address the reserved scene
                // camera, which is never a declared actor. Gated on the verb
                // because no other action has an engine path for it.
                let is_persisted_camera = check_label == animatix_core::property::CAMERA_TARGET
                    && matches!(action.verb.as_str(), "persist" | "remove");
                let is_defined = is_persisted_camera
                    || symbols.labels.contains_key(check_label)
                    || is_array_member_label(check_label)
                        .is_some_and(|base| symbols.array_labels.contains(base))
                    || is_component_array_member(symbols, check_label);
                if !is_defined {
                    let source_text = target_source_text(target);
                    let (tstart, tend) =
                        find_ident_range(tokens, source, source_text, byte_range.clone())
                            .unwrap_or_else(|| ((line - 1, col - 1), (end_line - 1, end_col - 1)));
                    diagnostics.push(range_diagnostic(
                        DiagnosticSeverity::Warning,
                        DiagnosticCode::UndefinedLabel,
                        format!("Undefined label: {}", check_label),
                        tstart,
                        tend,
                    ));
                }
            }
        },

        Stmt::Assignment {
            target,
            property,
            value,
            span,
            ..
        } => {
            let (line, col, _end_line, end_col) = span_positions(span);

            if let Some(seg) = target.first() {
                let label = seg.label_str();
                // `camera` is the reserved scene-camera target, never a declared
                // actor; the engine routes its assignments without a track.
                let is_defined = label == animatix_core::property::CAMERA_TARGET
                    || symbols.labels.contains_key(label)
                    || is_array_member_label(label)
                        .is_some_and(|base| symbols.array_labels.contains(base))
                    || is_component_array_member(symbols, label);
                if !is_defined {
                    diagnostics.push(span_diagnostic(
                        DiagnosticSeverity::Warning,
                        DiagnosticCode::UndefinedLabel,
                        format!("Undefined label: {}", label),
                        line,
                        col,
                        end_col,
                    ));
                }
            }

            // `scope.stage.param = value` addresses an effect stage parameter,
            // which is not on the scope's property table. Validate it against
            // the stage's declared effect — built-in or manifest-declared —
            // resolved through the stage label's collected type. A bare
            // `stage.param = value` resolves the same way when the stage is
            // the direct target.
            let stage_label = target.last().map(|seg| seg.label_str());
            let effect_ty = stage_label
                .and_then(|label| symbols.inline_child_type(label))
                .filter(|ty| symbols.is_effect_type(ty))
                .map(str::to_string);
            if let Some(effect_ty) = effect_ty {
                if let Some(known_props) = symbols.properties.get(&effect_ty) {
                    if !known_props.contains(property) {
                        diagnostics.push(span_diagnostic(
                            DiagnosticSeverity::Info,
                            DiagnosticCode::UnknownProperty,
                            format!("Effect '{}' has no parameter '{}'", effect_ty, property),
                            line,
                            col,
                            end_col,
                        ));
                    }

                    let key = (effect_ty.clone(), property.to_string());
                    if let Some(expected_type) = symbols.property_types.get(&key) {
                        let actual_type = symbols.infer_expr_type(value);
                        if !crate::typing::is_subtype(&actual_type, expected_type) {
                            diagnostics.push(span_diagnostic(
                                DiagnosticSeverity::Warning,
                                DiagnosticCode::TypeMismatch,
                                format!(
                                    "Type mismatch for '{}.{}': expected {:?}, found {:?}",
                                    effect_ty, property, expected_type, actual_type
                                ),
                                line,
                                col,
                                end_col,
                            ));
                        }
                    }
                }
            } else if let Some(param) = if target.len() >= 2 {
                crate::schema::effect_specs().iter().find_map(|spec| {
                    spec.params.iter().find(|param| param.name.as_ref() == property)
                })
            } else {
                None
            } {
                if let Some(expected_type) = crate::symbol_table::effect_param_type(param.kind) {
                    let actual_type = symbols.infer_expr_type(value);
                    if !crate::typing::is_subtype(&actual_type, &expected_type) {
                        diagnostics.push(span_diagnostic(
                            DiagnosticSeverity::Warning,
                            DiagnosticCode::TypeMismatch,
                            format!(
                                "Type mismatch for effect parameter '{}': expected {:?}, found {:?}",
                                property, expected_type, actual_type
                            ),
                            line,
                            col,
                            end_col,
                        ));
                    }
                }
            } else if let Some(stage_label) = stage_param_segment(target, symbols, property) {
                // A `scope.stage.param` target whose stage type is not a known
                // effect (typically a plugin effect whose library is not
                // loaded) is still *stage-parameter* position, not a host
                // property assignment. Checking it against the host actor's
                // property table produced false `type-mismatch` warnings — e.g.
                // `panel.pix.size = 6` compared against `Filter.size` (Vec2)
                // because `Pixelate` was unknown. Stay silent instead: the
                // unknown effect type is already reported elsewhere.
                let _ = stage_label;
            } else {
                let resolved = target.first().and_then(|seg| symbols.labels.get(seg.label_str()));
                let label = target.first().map(|seg| seg.label_str()).unwrap_or("");
                if let Some(info) = resolved {
                    if let Some(ty) = &info.ty {
                        if let Some(known_props) = symbols.properties.get(ty) {
                            if !known_props.contains(property)
                                && !info.plot_params.iter().any(|p| p == property)
                            {
                                diagnostics.push(span_diagnostic(
                                    DiagnosticSeverity::Info,
                                    DiagnosticCode::UnknownProperty,
                                    format!(
                                        "Property '{}' not commonly used on {} (may still be valid)",
                                        property, ty
                                    ),
                                    line,
                                    col,
                                    end_col,
                                ));
                            }

                            let key = (ty.clone(), property.clone());
                            if let Some(expected_type) = symbols.property_types.get(&key) {
                                let actual_type = symbols.infer_expr_type(value);
                                if !crate::typing::is_subtype(&actual_type, expected_type) {
                                    diagnostics.push(span_diagnostic(
                                        DiagnosticSeverity::Warning,
                                        DiagnosticCode::TypeMismatch,
                                        format!(
                                            "Type mismatch for '{}.{}': expected {:?}, found {:?}",
                                            label, property, expected_type, actual_type
                                        ),
                                        line,
                                        col,
                                        end_col,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        },

        Stmt::ActorDecl {
            label,
            ty,
            props,
            span,
            ..
        } => {
            let (line, col, _end_line, end_col) = span_positions(span);
            // Actor declaration spans are not populated by the parser, so a
            // diagnostic anchored on the declaration would fall back to the
            // file start. Locate this declaration's own `label: Type` token
            // pair instead, so each of several same-typed declarations points
            // at its own line rather than the first one in the file.
            let type_range = find_decl_type_range(tokens, source, label, ty);

            // Component instances are valid actor types too: accept local and
            // imported (`pub component`) components as well as namespaced ones
            // (`import as` + `alias.Component`), not just builtin/primitive types.
            let ty_known = symbols.types.contains(ty)
                || symbols.components.contains_key(ty)
                || symbols.resolve_namespaced_component(ty).is_some();
            if !ty_known {
                // A genuinely unknown type cannot render, and the build layer
                // rejects it as an error (`unknown-actor-type`). Report the
                // same severity here so the editor and the CLI agree; only a
                // registered primitive whose extension plugin is missing stays
                // a warning, and that case is not visible at this layer.
                let (ty_line, ty_col, ty_end_col) = match type_range {
                    Some((start, end)) => (start.0 + 1, start.1 + 1, end.1 + 1),
                    None => (line, col, end_col),
                };
                diagnostics.push(span_diagnostic(
                    DiagnosticSeverity::Error,
                    DiagnosticCode::UnknownType,
                    format!("Unknown type: {}", ty),
                    ty_line,
                    ty_col,
                    ty_end_col,
                ));
            }

            if let Some(known_props) = symbols.properties.get(ty) {
                for prop in props {
                    // Point at the property's own value span so the squiggle
                    // lands on the offending property rather than the actor
                    // declaration's first character.
                    let (prop_line, prop_col, prop_end_line, prop_end_col) =
                        property_positions(prop, source, line, col, end_col);
                    if !known_props.contains(&prop.name)
                        && !is_plot_runtime_param(ty, &prop.name, props)
                    {
                        diagnostics.push(span_diagnostic(
                            DiagnosticSeverity::Info,
                            DiagnosticCode::UnknownProperty,
                            format!(
                                "Property '{}' not commonly used on {} (may still be valid)",
                                prop.name, ty
                            ),
                            prop_line,
                            prop_col,
                            prop_end_col,
                        ));
                        if let Some(last) = diagnostics.last_mut() {
                            last.location.end_line = Some(prop_end_line);
                        }
                    }

                    let key = (ty.clone(), prop.name.clone());
                    if let Some(expected_type) = symbols.property_types.get(&key) {
                        let actual_type = symbols.infer_expr_type(&prop.value);
                        if !crate::typing::is_subtype(&actual_type, expected_type) {
                            diagnostics.push(span_diagnostic(
                                DiagnosticSeverity::Warning,
                                DiagnosticCode::TypeMismatch,
                                format!(
                                    "Type mismatch for '{}.{}': expected {:?}, found {:?}",
                                    ty, prop.name, expected_type, actual_type
                                ),
                                prop_line,
                                prop_col,
                                prop_end_col,
                            ));
                            if let Some(last) = diagnostics.last_mut() {
                                last.location.end_line = Some(prop_end_line);
                            }
                        }
                    }
                }
            }
        },

        _ => {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Expr;

    fn unused_labels(stmts: &[Stmt]) -> Vec<String> {
        let symbols = SymbolTable::build_from_ast(stmts);
        collect_semantic_diagnostics(stmts, &symbols, &[], "")
            .into_iter()
            .filter(|d| d.code == DiagnosticCode::UnusedLabel)
            .map(|d| d.message.clone())
            .collect()
    }

    fn unused_label_severity(stmts: &[Stmt], name: &str) -> Option<DiagnosticSeverity> {
        let symbols = SymbolTable::build_from_ast(stmts);
        collect_semantic_diagnostics(stmts, &symbols, &[], "")
            .into_iter()
            .find(|d| d.code == DiagnosticCode::UnusedLabel && d.message.contains(name))
            .map(|d| d.severity)
    }

    fn bare_actor(ty: &str, label: &str, props: Vec<crate::ast::Property>) -> Stmt {
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: label.to_string(),
            array_index: None,
            ty: ty.to_string(),
            props,
            modifiers: vec![],
            children: vec![],
            span: None,
        }
    }

    /// A standing caption is content, not a loose end: nothing animates it and
    /// nothing has to. It stays a hint so editors can still offer the removal,
    /// but it must not sit in the warning column with real defects — 347 of
    /// this repo's own files said "Unused actor" and the lint stopped being
    /// readable.
    #[test]
    fn an_unreferenced_but_visible_actor_is_a_hint_not_a_warning() {
        let stmts = vec![bare_actor(
            "Text",
            "col_cap",
            vec![
                crate::ast::Property::new("text", Expr::Str("column space".into())),
                crate::ast::Property::new("opacity", Expr::Num(1.0)),
            ],
        )];
        assert_eq!(
            unused_label_severity(&stmts, "'col_cap'"),
            Some(DiagnosticSeverity::Hint),
            "a static caption must not be reported as a warning"
        );
    }

    /// The counterpart: a binding nothing reads has no defence, and this is the
    /// case the diagnostic was actually built for.
    #[test]
    fn an_unread_binding_is_still_a_warning() {
        let stmts = vec![Stmt::LetDecl {
            is_pub: false,
            name: "leftover".to_string(),
            value: Expr::Num(3.0),
            span: None,
        }];
        assert_eq!(
            unused_label_severity(&stmts, "'leftover'"),
            Some(DiagnosticSeverity::Warning),
            "demoting actors must not have demoted the case that means something"
        );
    }

    #[test]
    fn structural_container_with_children_is_not_unused() {
        let stmts = vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "cards".to_string(),
            array_index: None,
            ty: "Row".to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![InlineItem::Labeled {
                label: "card".to_string(),
                array_index: None,
                ty: "Rect".to_string(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        }];
        let labels = unused_labels(&stmts);
        assert!(
            !labels.iter().any(|m| m.contains("'cards'")),
            "structural container label should be exempt: {labels:?}"
        );
    }

    #[test]
    fn empty_container_is_still_unused() {
        let stmts = vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "holder".to_string(),
            array_index: None,
            ty: "Group".to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![],
            span: None,
        }];
        let labels = unused_labels(&stmts);
        assert!(labels.iter().any(|m| m.contains("holder")));
    }

    #[test]
    fn non_container_with_children_is_still_unused() {
        let stmts = vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "wrapper".to_string(),
            array_index: None,
            ty: "Rect".to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![InlineItem::Labeled {
                label: "child".to_string(),
                array_index: None,
                ty: "Rect".to_string(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        }];
        let labels = unused_labels(&stmts);
        assert!(labels.iter().any(|m| m.contains("wrapper")));
    }

    fn unknown_types(stmts: &[Stmt], symbols: &SymbolTable) -> Vec<String> {
        collect_semantic_diagnostics(stmts, symbols, &[], "")
            .into_iter()
            .filter(|d| d.code == DiagnosticCode::UnknownType)
            .map(|d| d.message.clone())
            .collect()
    }

    fn actor_decl_of_type(ty: &str) -> Stmt {
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "instance".to_string(),
            array_index: None,
            ty: ty.to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![],
            span: None,
        }
    }

    #[test]
    fn imported_component_instance_is_not_unknown_type() {
        // Regression: a statement-position instance of an imported `pub
        // component` used to warn `unknown-type` because only builtin types
        // were consulted; Group-wrapping "fixed" it only by escaping the lint.
        let mut symbols = SymbolTable::build_from_ast(&[]);
        symbols.components.insert(
            "MetricCard".to_string(),
            crate::symbol_table::ComponentInfo {
                name: "MetricCard".to_string(),
                is_pub: true,
                params: vec![],
                line: 1,
                col: 1,
                span: None,
            },
        );
        let stmts = vec![actor_decl_of_type("MetricCard")];
        let diags = unknown_types(&stmts, &symbols);
        assert!(
            diags.is_empty(),
            "imported component instance must not warn unknown-type: {diags:?}"
        );
    }

    #[test]
    fn namespaced_component_instance_is_not_unknown_type() {
        // `import "lib/ui.amx" as ui` + `ui.MetricCard` resolves through the
        // namespace tables, not `types`.
        let mut symbols = SymbolTable::build_from_ast(&[]);
        symbols.namespaces.insert(
            "ui".to_string(),
            SymbolTable {
                components: [(
                    "MetricCard".to_string(),
                    crate::symbol_table::ComponentInfo {
                        name: "MetricCard".to_string(),
                        is_pub: true,
                        params: vec![],
                        line: 1,
                        col: 1,
                        span: None,
                    },
                )]
                .into_iter()
                .collect(),
                ..Default::default()
            },
        );
        let stmts = vec![actor_decl_of_type("ui.MetricCard")];
        let diags = unknown_types(&stmts, &symbols);
        assert!(
            diags.is_empty(),
            "namespaced component instance must not warn unknown-type: {diags:?}"
        );
    }

    #[test]
    fn genuinely_unknown_type_still_warns() {
        let symbols = SymbolTable::build_from_ast(&[]);
        let stmts = vec![actor_decl_of_type("NotAType")];
        let diags = unknown_types(&stmts, &symbols);
        assert!(
            diags.iter().any(|m| m.contains("NotAType")),
            "bogus type must still warn unknown-type: {diags:?}"
        );
    }
}
