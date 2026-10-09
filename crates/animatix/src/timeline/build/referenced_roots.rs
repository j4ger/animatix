//! Pre-scan of the expanded AST for actor labels referenced by expressions.
//!
//! Build-time evaluation environments historically injected every property of
//! every declared track on every [`Timeline::build_eval_env`] call, making
//! environment construction O(declarations²). Expressions, however, can only
//! reference an actor label that appears textually in the program: member
//! access like `pulse.size.x` parses to `Expr::Path(["pulse", "size", "x"])`.
//!
//! Collecting those roots once per build lets the build environment inject
//! only referenced actors' properties:
//! - Over-injection (collecting too much) stays perfectly safe — those keys are simply unused.
//! - Under-injection fails loudly as an undefined-variable error instead of silently producing
//!   wrong values.
//!
//! The walk must stay exhaustive over [`Stmt`] and [`Expr`] variants (no
//! catch-all arm) so newly added syntax cannot silently escape collection.

use std::collections::{HashMap, HashSet};

use animatix_syntax::ast::{Expr, InlineItem, Property, Stmt, TargetSegment};

/// Pre-scan results of the expanded AST for property and state queries.
#[derive(Clone, Debug, Default)]
pub(crate) struct ReferenceScan {
    /// Actor labels referenced anywhere in the AST (statements, actions, expressions).
    pub roots: HashSet<String>,
    /// Whether `scene.stats.*` was referenced.
    pub stats_used: bool,
    /// Whether `is_animating` or a property reference (`&actor.prop`) was used.
    pub is_animating_used: bool,
    /// Actor labels that appeared as bare identifiers in expressions without a dot accessor.
    pub wildcard_actors: HashSet<String>,
    /// Map from actor label to the specific property names referenced in expressions.
    pub referenced_properties: HashMap<String, HashSet<String>>,
}

/// Normalize an identifier to its actor-label root: the first dotted segment
/// of `pulse.size.x` is the label `pulse`.
#[inline]
fn normalize_root(name: &str) -> String {
    match name.split_once('.') {
        Some((root, _)) => root.to_string(),
        None => name.to_string(),
    }
}

fn collect_expr_roots(expr: &Expr, scan: &mut ReferenceScan) {
    match expr {
        Expr::Num(_) | Expr::Percent(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Null => {},
        Expr::Ident(name) => {
            if name.starts_with("scene.stats") {
                scan.stats_used = true;
            }
            if let Some((actor, rest)) = name.split_once('.') {
                let actor_root = normalize_root(actor);
                scan.roots.insert(actor_root.clone());
                let prop = match rest.split_once('.') {
                    Some((p, _)) => p,
                    None => rest,
                };
                let entry = scan.referenced_properties.entry(actor_root).or_default();
                entry.insert(prop.to_string());
                match prop {
                    "x" | "y" => {
                        entry.insert("at".to_string());
                    },
                    "radius" => {
                        entry.insert("size".to_string());
                    },
                    "r" | "g" | "b" | "a" => {
                        entry.insert("color".to_string());
                    },
                    _ => {},
                }
            } else {
                scan.roots.insert(name.clone());
                scan.wildcard_actors.insert(name.clone());
            }
        },
        Expr::Path(parts) => {
            if parts.len() >= 2 && parts[0] == "scene" && parts[1] == "stats" {
                scan.stats_used = true;
            }
            if parts.len() >= 2 {
                let actor = normalize_root(&parts[0]);
                let prop = &parts[1];
                scan.roots.insert(actor.clone());
                let entry = scan.referenced_properties.entry(actor).or_default();
                entry.insert(prop.clone());
                match prop.as_str() {
                    "x" | "y" => {
                        entry.insert("at".to_string());
                    },
                    "radius" => {
                        entry.insert("size".to_string());
                    },
                    "r" | "g" | "b" | "a" => {
                        entry.insert("color".to_string());
                    },
                    _ => {},
                }
            } else if let Some(first) = parts.first() {
                let root = normalize_root(first);
                scan.roots.insert(root.clone());
                scan.wildcard_actors.insert(root);
            }
        },
        Expr::Index(container, index) => {
            collect_expr_roots(container, scan);
            collect_expr_roots(index, scan);
        },
        Expr::Tuple(items) | Expr::List(items) => {
            for item in items {
                collect_expr_roots(item, scan);
            }
        },
        Expr::Binary(a, _, b) => {
            collect_expr_roots(a, scan);
            collect_expr_roots(b, scan);
        },
        Expr::Unary(op, e) => {
            if *op == animatix_syntax::ast::UnaryOp::Ref {
                scan.is_animating_used = true;
            }
            collect_expr_roots(e, scan);
        },
        // The function name resolves against the stdlib / user fns in the base
        // environment, not against actor tracks; arguments may reference actors.
        Expr::Call(name, args) => {
            if name == "is_animating" {
                scan.is_animating_used = true;
            }
            for arg in args {
                collect_expr_roots(arg, scan);
            }
        },
        Expr::Method(receiver, _, args) => {
            collect_expr_roots(receiver, scan);
            for arg in args {
                collect_expr_roots(arg, scan);
            }
        },
        Expr::Closure(_, body) => collect_expr_roots(body, scan),
        Expr::LetChain(bindings, tail) => {
            for (_, value) in bindings {
                collect_expr_roots(value, scan);
            }
            collect_expr_roots(tail, scan);
        },
        Expr::Conditional(condition, then_expr, else_expr) => {
            collect_expr_roots(condition, scan);
            collect_expr_roots(then_expr, scan);
            collect_expr_roots(else_expr, scan);
        },
        Expr::Match(scrutinee, arms) => {
            collect_expr_roots(scrutinee, scan);
            for (_, body) in arms {
                collect_expr_roots(body, scan);
            }
        },
        Expr::Construct(_, props) => collect_property_roots(props, scan),
    }
}

fn collect_property_roots(props: &[Property], scan: &mut ReferenceScan) {
    for prop in props {
        collect_expr_roots(&prop.value, scan);
    }
}

fn collect_target_roots(target: &[TargetSegment], scan: &mut ReferenceScan) {
    for segment in target {
        match segment {
            TargetSegment::Static(name) => {
                scan.roots.insert(normalize_root(name));
            },
            TargetSegment::Indexed { base, index } => {
                scan.roots.insert(normalize_root(base));
                collect_expr_roots(index, scan);
            },
        }
    }
}

fn collect_inline_item_roots(items: &[InlineItem], scan: &mut ReferenceScan) {
    for item in items {
        match item {
            InlineItem::Anonymous {
                props,
                modifiers,
                children,
                ..
            } => {
                collect_property_roots(props, scan);
                for modifier in modifiers {
                    collect_expr_roots(&modifier.value, scan);
                }
                collect_inline_item_roots(children, scan);
            },
            InlineItem::Labeled {
                array_index,
                props,
                modifiers,
                children,
                ..
            } => {
                if let Some(index) = array_index {
                    collect_expr_roots(index, scan);
                }
                collect_property_roots(props, scan);
                for modifier in modifiers {
                    collect_expr_roots(&modifier.value, scan);
                }
                collect_inline_item_roots(children, scan);
            },
            InlineItem::ForLoop { iterable, body, .. } => {
                collect_expr_roots(iterable, scan);
                collect_inline_item_roots(body, scan);
            },
            // A slot marker references no actors; fills carry the slotted items.
            InlineItem::SlotMarker => {},
            InlineItem::SlotFill { items, .. } => {
                collect_inline_item_roots(items, scan);
            },
        }
    }
}

fn collect_stmt_roots(stmts: &[Stmt], scan: &mut ReferenceScan) {
    for stmt in stmts {
        match stmt {
            Stmt::Action(action, _) => {
                // Action targets name actors (`move btn to ...`).
                for target in &action.targets {
                    scan.roots.insert(normalize_root(target));
                }
                for index in action.target_index.iter().flatten() {
                    collect_expr_roots(index, scan);
                }
                for arg in &action.args {
                    collect_expr_roots(arg, scan);
                }
            },
            Stmt::LetDecl { value, .. } => collect_expr_roots(value, scan),
            Stmt::ActorDecl {
                array_index,
                props,
                modifiers,
                children,
                ..
            } => {
                if let Some(index) = array_index {
                    collect_expr_roots(index, scan);
                }
                collect_property_roots(props, scan);
                for modifier in modifiers {
                    collect_expr_roots(&modifier.value, scan);
                }
                collect_inline_item_roots(children, scan);
            },
            // Type aliases, imports, and comments carry no expressions that
            // resolve against actor tracks.
            Stmt::TypeAlias { .. } | Stmt::Import { .. } | Stmt::Comment(..) => {},
            Stmt::Keyframe { body, .. } | Stmt::RelativeKeyframe { body, .. } => {
                collect_stmt_roots(body, scan);
            },
            Stmt::Assignment {
                target,
                value,
                modifiers,
                ..
            } => {
                collect_target_roots(target, scan);
                collect_expr_roots(value, scan);
                for modifier in modifiers {
                    collect_expr_roots(&modifier.value, scan);
                }
            },
            Stmt::Sequence { body, .. } | Stmt::Always { body, .. } => {
                collect_stmt_roots(body, scan);
            },
            Stmt::Stagger {
                modifiers, body, ..
            } => {
                for modifier in modifiers {
                    collect_expr_roots(&modifier.value, scan);
                }
                collect_stmt_roots(body, scan);
            },
            Stmt::ReactiveBinding {
                target,
                property: _,
                value,
                value_span: _,
                span: _,
            } => {
                collect_target_roots(target, scan);
                collect_expr_roots(value, scan);
            },
            Stmt::Conditional {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                collect_expr_roots(condition, scan);
                collect_stmt_roots(then_branch, scan);
                if let Some(else_branch) = else_branch {
                    collect_stmt_roots(else_branch, scan);
                }
            },
            Stmt::Match {
                scrutinee, arms, ..
            } => {
                collect_expr_roots(scrutinee, scan);
                for (_, body) in arms {
                    collect_stmt_roots(body, scan);
                }
            },
            Stmt::ForLoop {
                iterable,
                body,
                modifiers,
                ..
            } => {
                collect_expr_roots(iterable, scan);
                for modifier in modifiers {
                    collect_expr_roots(&modifier.value, scan);
                }
                collect_stmt_roots(body, scan);
            },
            Stmt::ComponentDef(def, _) => collect_stmt_roots(&def.body, scan),
            Stmt::FnDecl { body, .. } => collect_stmt_roots(body, scan),
            Stmt::Block { body, .. } => collect_stmt_roots(body, scan),
            Stmt::Return { value, .. } => {
                if let Some(value) = value {
                    collect_expr_roots(value, scan);
                }
            },
            Stmt::Expr(expr, _) => collect_expr_roots(expr, scan),
            Stmt::Config { settings, .. } => collect_property_roots(settings, scan),
            Stmt::Scene { config, body, .. } => {
                collect_property_roots(config, scan);
                collect_stmt_roots(body, scan);
            },
            // Play transitions are static scene-graph metadata.
            Stmt::Play { .. } => {},
        }
    }
}

/// Collect every actor label root referenced by any expression in `stmts`,
/// whether `scene.stats.*` was referenced, whether `is_animating` was used,
/// and selective per-actor property access mappings.
pub(crate) fn scan_references(stmts: &[Stmt]) -> ReferenceScan {
    let mut scan = ReferenceScan::default();
    collect_stmt_roots(stmts, &mut scan);
    scan
}

/// Collect every actor label root referenced by any expression in `stmts`.
#[cfg(test)]
pub(crate) fn collect_referenced_roots(stmts: &[Stmt]) -> HashSet<String> {
    scan_references(stmts).roots
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots_of(source: &str) -> HashSet<String> {
        let (stmts, _) = animatix_syntax::parser::parse_source(source);
        collect_referenced_roots(&stmts.expect("test fixture should parse"))
    }

    #[test]
    fn cross_actor_references_are_collected() {
        let roots = roots_of(
            r#"
#0s
pulse: Rect, size: (120, 120), at: (280, 390)
echo: Ellipse, size: (40, 40), at: pulse.at

always {
  echo.size = (pulse.size.x / 3, pulse.size.y / 3)
}
"#,
        );
        assert!(roots.contains("pulse"), "referenced actor must be collected");
        assert!(roots.contains("echo"), "assignment target must be collected");
    }

    #[test]
    fn unreferenced_actors_are_not_collected() {
        let roots = roots_of(
            r#"
#0s
a: Rect, size: (100, 100), at: (200, 200)
b: Ellipse, size: (50, 50), at: (600, 300)
"#,
        );
        // Declarations themselves do not make an actor referenced; only
        // expressions naming them do.
        assert!(!roots.contains("a"), "literal-only declarations add no roots");
        assert!(!roots.contains("b"));
    }

    #[test]
    fn action_targets_and_closures_are_collected() {
        let roots = roots_of(
            r#"
#0s
btn: Rect, size: (80, 80)
plot1: Plot, func: (x) => sin(x * gain)

#1s
move btn to (400, 300) [500ms]
"#,
        );
        assert!(roots.contains("btn"), "action targets are actor references");
        // Closure bodies are walked; `gain` may or may not be an actor but
        // collecting it is the safe direction.
        assert!(roots.contains("gain"));
    }

    #[test]
    fn selective_property_scan_tracks_properties_and_animating_state() {
        let (stmts, _) = animatix_syntax::parser::parse_source(
            r#"
#0s
pulse: Rect, size: (120, 120), at: (280, 390)
echo: Ellipse, size: (40, 40), at: pulse.at

always {
  echo.size = (pulse.size.x / 3, pulse.size.y / 3)
}
"#,
        );
        let scan = scan_references(&stmts.unwrap());
        assert!(!scan.is_animating_used);
        let pulse_props = scan.referenced_properties.get("pulse").unwrap();
        assert!(pulse_props.contains("size"));
        assert!(pulse_props.contains("at"));
        // Never referenced properties should not be present
        assert!(!pulse_props.contains("color"));
        assert!(!pulse_props.contains("opacity"));
        // echo is only an assignment target, not read in expressions
        assert!(!scan.referenced_properties.contains_key("echo"));
    }

    #[test]
    fn is_animating_and_ref_are_detected() {
        let (stmts, _) = animatix_syntax::parser::parse_source(
            r#"
#0s
box: Rect, size: (80, 40), at: (100, 100)
always {
  let moving = is_animating(&box.at)
}
"#,
        );
        let scan = scan_references(&stmts.unwrap());
        assert!(scan.is_animating_used);
        let box_props = scan.referenced_properties.get("box").unwrap();
        assert!(box_props.contains("at"));
    }
}
