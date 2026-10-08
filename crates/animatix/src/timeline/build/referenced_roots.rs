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

use std::collections::HashSet;

use animatix_syntax::ast::{Expr, InlineItem, Property, Stmt, TargetSegment};

/// Normalize an identifier to its actor-label root: the first dotted segment
/// of `pulse.size.x` is the label `pulse`.
#[inline]
fn normalize_root(name: &str) -> String {
    match name.split_once('.') {
        Some((root, _)) => root.to_string(),
        None => name.to_string(),
    }
}

fn collect_expr_roots(expr: &Expr, roots: &mut HashSet<String>, stats_used: &mut bool) {
    match expr {
        Expr::Num(_) | Expr::Percent(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Null => {},
        Expr::Ident(name) => {
            if name.starts_with("scene.stats") {
                *stats_used = true;
            }
            roots.insert(normalize_root(name));
        },
        Expr::Path(parts) => {
            if parts.len() >= 2 && parts[0] == "scene" && parts[1] == "stats" {
                *stats_used = true;
            }
            if let Some(first) = parts.first() {
                roots.insert(normalize_root(first));
            }
        },
        Expr::Index(container, index) => {
            collect_expr_roots(container, roots, stats_used);
            collect_expr_roots(index, roots, stats_used);
        },
        Expr::Tuple(items) | Expr::List(items) => {
            for item in items {
                collect_expr_roots(item, roots, stats_used);
            }
        },
        Expr::Binary(a, _, b) => {
            collect_expr_roots(a, roots, stats_used);
            collect_expr_roots(b, roots, stats_used);
        },
        Expr::Unary(_, e) => collect_expr_roots(e, roots, stats_used),
        // The function name resolves against the stdlib / user fns in the base
        // environment, not against actor tracks; arguments may reference actors.
        Expr::Call(_, args) => {
            for arg in args {
                collect_expr_roots(arg, roots, stats_used);
            }
        },
        Expr::Method(receiver, _, args) => {
            collect_expr_roots(receiver, roots, stats_used);
            for arg in args {
                collect_expr_roots(arg, roots, stats_used);
            }
        },
        Expr::Closure(_, body) => collect_expr_roots(body, roots, stats_used),
        Expr::LetChain(bindings, tail) => {
            for (_, value) in bindings {
                collect_expr_roots(value, roots, stats_used);
            }
            collect_expr_roots(tail, roots, stats_used);
        },
        Expr::Conditional(condition, then_expr, else_expr) => {
            collect_expr_roots(condition, roots, stats_used);
            collect_expr_roots(then_expr, roots, stats_used);
            collect_expr_roots(else_expr, roots, stats_used);
        },
        Expr::Match(scrutinee, arms) => {
            collect_expr_roots(scrutinee, roots, stats_used);
            for (_, body) in arms {
                collect_expr_roots(body, roots, stats_used);
            }
        },
        Expr::Construct(_, props) => collect_property_roots(props, roots, stats_used),
    }
}

fn collect_property_roots(props: &[Property], roots: &mut HashSet<String>, stats_used: &mut bool) {
    for prop in props {
        collect_expr_roots(&prop.value, roots, stats_used);
    }
}

fn collect_target_roots(
    target: &[TargetSegment],
    roots: &mut HashSet<String>,
    stats_used: &mut bool,
) {
    for segment in target {
        match segment {
            TargetSegment::Static(name) => {
                roots.insert(normalize_root(name));
            },
            TargetSegment::Indexed { base, index } => {
                roots.insert(normalize_root(base));
                collect_expr_roots(index, roots, stats_used);
            },
        }
    }
}

fn collect_inline_item_roots(
    items: &[InlineItem],
    roots: &mut HashSet<String>,
    stats_used: &mut bool,
) {
    for item in items {
        match item {
            InlineItem::Anonymous {
                props,
                modifiers,
                children,
                ..
            } => {
                collect_property_roots(props, roots, stats_used);
                for modifier in modifiers {
                    collect_expr_roots(&modifier.value, roots, stats_used);
                }
                collect_inline_item_roots(children, roots, stats_used);
            },
            InlineItem::Labeled {
                array_index,
                props,
                modifiers,
                children,
                ..
            } => {
                if let Some(index) = array_index {
                    collect_expr_roots(index, roots, stats_used);
                }
                collect_property_roots(props, roots, stats_used);
                for modifier in modifiers {
                    collect_expr_roots(&modifier.value, roots, stats_used);
                }
                collect_inline_item_roots(children, roots, stats_used);
            },
            InlineItem::ForLoop { iterable, body, .. } => {
                collect_expr_roots(iterable, roots, stats_used);
                collect_inline_item_roots(body, roots, stats_used);
            },
            // A slot marker references no actors; fills carry the slotted items.
            InlineItem::SlotMarker => {},
            InlineItem::SlotFill { items, .. } => {
                collect_inline_item_roots(items, roots, stats_used)
            },
        }
    }
}

fn collect_stmt_roots(stmts: &[Stmt], roots: &mut HashSet<String>, stats_used: &mut bool) {
    for stmt in stmts {
        match stmt {
            Stmt::Action(action, _) => {
                // Action targets name actors (`move btn to ...`).
                for target in &action.targets {
                    roots.insert(normalize_root(target));
                }
                for index in action.target_index.iter().flatten() {
                    collect_expr_roots(index, roots, stats_used);
                }
                for arg in &action.args {
                    collect_expr_roots(arg, roots, stats_used);
                }
            },
            Stmt::LetDecl { value, .. } => collect_expr_roots(value, roots, stats_used),
            Stmt::ActorDecl {
                array_index,
                props,
                modifiers,
                children,
                ..
            } => {
                if let Some(index) = array_index {
                    collect_expr_roots(index, roots, stats_used);
                }
                collect_property_roots(props, roots, stats_used);
                for modifier in modifiers {
                    collect_expr_roots(&modifier.value, roots, stats_used);
                }
                collect_inline_item_roots(children, roots, stats_used);
            },
            // Type aliases, imports, and comments carry no expressions that
            // resolve against actor tracks.
            Stmt::TypeAlias { .. } | Stmt::Import { .. } | Stmt::Comment(..) => {},
            Stmt::Keyframe { body, .. } | Stmt::RelativeKeyframe { body, .. } => {
                collect_stmt_roots(body, roots, stats_used);
            },
            Stmt::Assignment {
                target,
                value,
                modifiers,
                ..
            } => {
                collect_target_roots(target, roots, stats_used);
                collect_expr_roots(value, roots, stats_used);
                for modifier in modifiers {
                    collect_expr_roots(&modifier.value, roots, stats_used);
                }
            },
            Stmt::Sequence { body, .. } | Stmt::Always { body, .. } => {
                collect_stmt_roots(body, roots, stats_used);
            },
            Stmt::Stagger {
                modifiers, body, ..
            } => {
                for modifier in modifiers {
                    collect_expr_roots(&modifier.value, roots, stats_used);
                }
                collect_stmt_roots(body, roots, stats_used);
            },
            Stmt::ReactiveBinding {
                target,
                property: _,
                value,
                value_span: _,
                span: _,
            } => {
                collect_target_roots(target, roots, stats_used);
                collect_expr_roots(value, roots, stats_used);
            },
            Stmt::Conditional {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                collect_expr_roots(condition, roots, stats_used);
                collect_stmt_roots(then_branch, roots, stats_used);
                if let Some(else_branch) = else_branch {
                    collect_stmt_roots(else_branch, roots, stats_used);
                }
            },
            Stmt::Match {
                scrutinee, arms, ..
            } => {
                collect_expr_roots(scrutinee, roots, stats_used);
                for (_, body) in arms {
                    collect_stmt_roots(body, roots, stats_used);
                }
            },
            Stmt::ForLoop {
                iterable,
                body,
                modifiers,
                ..
            } => {
                collect_expr_roots(iterable, roots, stats_used);
                for modifier in modifiers {
                    collect_expr_roots(&modifier.value, roots, stats_used);
                }
                collect_stmt_roots(body, roots, stats_used);
            },
            Stmt::ComponentDef(def, _) => collect_stmt_roots(&def.body, roots, stats_used),
            Stmt::FnDecl { body, .. } => collect_stmt_roots(body, roots, stats_used),
            Stmt::Block { body, .. } => collect_stmt_roots(body, roots, stats_used),
            Stmt::Return { value, .. } => {
                if let Some(value) = value {
                    collect_expr_roots(value, roots, stats_used);
                }
            },
            Stmt::Expr(expr, _) => collect_expr_roots(expr, roots, stats_used),
            Stmt::Config { settings, .. } => collect_property_roots(settings, roots, stats_used),
            Stmt::Scene { config, body, .. } => {
                collect_property_roots(config, roots, stats_used);
                collect_stmt_roots(body, roots, stats_used);
            },
            // Play transitions are static scene-graph metadata.
            Stmt::Play { .. } => {},
        }
    }
}

/// Collect every actor label root referenced by any expression in `stmts`,
/// and whether `scene.stats.*` was referenced.
pub(crate) fn scan_references(stmts: &[Stmt]) -> (HashSet<String>, bool) {
    let mut roots = HashSet::new();
    let mut stats_used = false;
    collect_stmt_roots(stmts, &mut roots, &mut stats_used);
    (roots, stats_used)
}

/// Collect every actor label root referenced by any expression in `stmts`.
#[cfg(test)]
pub(crate) fn collect_referenced_roots(stmts: &[Stmt]) -> HashSet<String> {
    scan_references(stmts).0
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
}
