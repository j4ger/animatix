//! Duplicate-label detection with language-aware scoping.
//!
//! Redeclaring a label at a **later** keyframe is legal (automatic morph, see
//! `docs/spec.md` §7), so a duplicate is only flagged when two declarations
//! land in the same region: the same keyframe (or two keyframes with the same
//! absolute time), the same scene's unkeyed declaration region, or the same
//! container's children. Cross-scene redeclaration and lexical `let` shadowing
//! are legal and never flagged.

use std::collections::{HashMap, HashSet, VecDeque};

use animatix_syntax::ast::{Stmt, Time};
use animatix_syntax::occurrence::{Occurrence, OccurrenceKind};
use animatix_syntax::token::byte_to_line_col;

use crate::diagnostics::{Diagnostic, DiagnosticSeverity};

/// Collect duplicate-label warnings for a parsed program.
///
/// Declaration positions come from the parser-recorded occurrence stream:
/// actor-label declarations are exactly the `declaration` occurrences of
/// [`OccurrenceKind::Label`], and the walk consumes them per name in source
/// order (special declarations such as `Svg`/`Image` use plain identifiers
/// and record nothing, so per-name queues stay aligned).
pub fn collect_duplicate_labels(
    stmts: &[Stmt],
    occurrences: &[Occurrence],
    source: &str,
) -> Vec<Diagnostic> {
    let mut queues: HashMap<&str, VecDeque<&Occurrence>> = HashMap::new();
    for occurrence in occurrences {
        if occurrence.kind == OccurrenceKind::Label && occurrence.declaration {
            queues.entry(occurrence.name.as_str()).or_default().push_back(occurrence);
        }
    }

    let mut walker = Walker {
        queues,
        source,
        diagnostics: Vec::new(),
    };
    walker.walk_statements(stmts, &mut Namespace::default(), true);
    walker.diagnostics
}

/// One declaration namespace: an unkeyed region (scene top level or container
/// children) plus the absolute-time-keyed regions of its keyframes.
#[derive(Default)]
struct Namespace {
    unkeyed: HashSet<String>,
    keyed: HashSet<(String, u64)>,
}

struct Walker<'a> {
    queues: HashMap<&'a str, VecDeque<&'a Occurrence>>,
    source: &'a str,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Walker<'a> {
    /// Pop the next declaration occurrence for `label`, if one was recorded.
    fn pop_occurrence(&mut self, label: &str) -> Option<&'a Occurrence> {
        self.queues.get_mut(label)?.pop_front()
    }

    fn visit_label(
        &mut self,
        label: &str,
        flaggable: bool,
        namespace: &mut Namespace,
        keyed_time: Option<u64>,
    ) {
        let occurrence = self.pop_occurrence(label);
        if !flaggable {
            return;
        }
        let duplicate = match keyed_time {
            Some(time) => !namespace.keyed.insert((label.to_string(), time)),
            None => !namespace.unkeyed.insert(label.to_string()),
        };
        if duplicate {
            let (line, col, end_line, end_col) = match occurrence {
                Some(occ) => {
                    let (start, end) = (
                        byte_to_line_col(self.source, occ.span.start),
                        byte_to_line_col(self.source, occ.span.end),
                    );
                    (start.0, start.1, end.0, end.1)
                },
                // No recorded occurrence (e.g. unnamed-asset default labels):
                // fall back to a file-level position.
                None => (0, 0, 0, 1),
            };
            self.diagnostics.push(Diagnostic {
                severity: DiagnosticSeverity::Warning,
                line,
                col,
                end_line,
                end_col,
                message: format!(
                    "Duplicate label '{label}' in this scope; rename one occurrence (re-declaration \
                     at a later keyframe is the supported morph form)"
                ),
                code: Some("duplicate-label".to_string()),
            });
        }
    }

    fn walk_statements(&mut self, stmts: &[Stmt], namespace: &mut Namespace, flagging: bool) {
        for stmt in stmts {
            match stmt {
                Stmt::Scene { body, .. } => {
                    // Each scene is its own label namespace.
                    self.walk_statements(body, &mut Namespace::default(), flagging);
                },
                Stmt::ActorDecl {
                    label,
                    is_anonymous,
                    array_index,
                    children,
                    ..
                } => {
                    if !is_anonymous {
                        self.visit_label(label, flagging && array_index.is_none(), namespace, None);
                    }
                    self.walk_children(children, flagging);
                },
                Stmt::Keyframe { time, body, .. } => {
                    // Declarations at the same absolute time as another
                    // keyframe's conflict; a different time is a legal morph.
                    let time_key = absolute_time_key(time);
                    for inner in body {
                        self.walk_statement_keyed(inner, namespace, flagging, time_key);
                    }
                },
                Stmt::ComponentDef(def, _) => {
                    // Component bodies are per-component namespaces: consume
                    // occurrences to keep queue order, never flag.
                    self.walk_statements(&def.body, &mut Namespace::default(), false);
                },
                other => {
                    // always/sequence/stagger/conditional/match/for/fn/block
                    // bodies: recurse into a fresh namespace so only
                    // intra-body duplicates are flagged.
                    if let Some(bodies) = statement_bodies(other) {
                        for body in bodies {
                            self.walk_statements(body, &mut Namespace::default(), flagging);
                        }
                    }
                },
            }
        }
    }

    fn walk_statement_keyed(
        &mut self,
        stmt: &Stmt,
        namespace: &mut Namespace,
        flagging: bool,
        time_key: u64,
    ) {
        match stmt {
            Stmt::ActorDecl {
                label,
                is_anonymous,
                array_index,
                children,
                ..
            } => {
                if !is_anonymous {
                    self.visit_label(
                        label,
                        flagging && array_index.is_none(),
                        namespace,
                        Some(time_key),
                    );
                }
                self.walk_children(children, flagging);
            },
            other => {
                if let Some(bodies) = statement_bodies(other) {
                    for body in bodies {
                        self.walk_statements(body, &mut Namespace::default(), flagging);
                    }
                }
            },
        }
    }

    fn walk_children(&mut self, children: &[animatix_syntax::ast::InlineItem], flagging: bool) {
        use animatix_syntax::ast::InlineItem;

        // Direct children of one container share a namespace; nested
        // containers recurse with their own.
        let mut namespace = Namespace::default();
        for item in children {
            match item {
                InlineItem::Labeled {
                    label,
                    array_index,
                    children,
                    ..
                } => {
                    self.visit_label(
                        label,
                        flagging && array_index.is_none(),
                        &mut namespace,
                        None,
                    );
                    self.walk_children(children, flagging);
                },
                InlineItem::Anonymous { children, .. } => {
                    self.walk_children(children, flagging);
                },
                InlineItem::ForLoop { body, .. } => {
                    // Loop bodies instantiate per iteration: labels repeat by
                    // design.
                    self.walk_children(body, false);
                },
                InlineItem::SlotFill { items, .. } => {
                    self.walk_children(items, flagging);
                },
                InlineItem::SlotMarker => {},
            }
        }
    }
}

/// Key an absolute keyframe time for duplicate detection.
fn absolute_time_key(time: &Time) -> u64 {
    match time {
        Time::Seconds(s) => s.to_bits(),
        Time::Milliseconds(ms) => (*ms as f64 / 1000.0).to_bits(),
    }
}

/// Bodies of block statements that may contain further declarations.
fn statement_bodies(stmt: &Stmt) -> Option<Vec<&[Stmt]>> {
    use animatix_syntax::ast::Stmt as S;
    match stmt {
        S::RelativeKeyframe { body, .. }
        | S::Sequence { body, .. }
        | S::Stagger { body, .. }
        | S::Always { body, .. }
        | S::FnDecl { body, .. }
        | S::Block { body, .. }
        | S::ForLoop { body, .. } => Some(vec![body]),
        S::Conditional {
            then_branch,
            else_branch,
            ..
        } => {
            let mut bodies = vec![then_branch.as_slice()];
            if let Some(else_body) = else_branch {
                bodies.push(else_body);
            }
            Some(bodies)
        },
        S::Match { arms, .. } => Some(arms.iter().map(|(_, body)| body.as_slice()).collect()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::Analyzer;

    fn duplicate_labels(source: &str) -> Vec<String> {
        let analyzer = Analyzer::new(source);
        analyzer
            .diagnostics()
            .into_iter()
            .filter(|d| d.code.as_deref() == Some("duplicate-label"))
            .map(|d| format!("{}:{}: {}", d.line, d.col, d.message))
            .collect()
    }

    #[test]
    fn duplicate_top_level_labels_flagged() {
        let source = r#"title: Text, text: "one"
title: Text, text: "two"
"#;
        let diags = duplicate_labels(source);
        assert_eq!(diags.len(), 1, "second declaration flagged: {diags:?}");
        // 0-based position of the second `title` (line 1, col 0).
        assert!(diags[0].starts_with("1:0"), "position on second decl: {diags:?}");
    }

    #[test]
    fn morph_redeclaration_at_later_keyframe_is_legal() {
        let source = r#"title: Text, text: "one"
#0s
fade-in title [100ms]
#1s
title: Text, text: "two"
"#;
        assert!(duplicate_labels(source).is_empty(), "morph redeclaration must not be flagged");
    }

    #[test]
    fn duplicate_in_same_keyframe_flagged() {
        let source = r#"#0s
title: Text, text: "one"
title: Text, text: "two"
"#;
        let diags = duplicate_labels(source);
        assert_eq!(diags.len(), 1, "same-keyframe duplicate flagged: {diags:?}");
    }

    #[test]
    fn same_time_keyframes_conflict() {
        let source = r#"#0s
title: Text, text: "one"
#0s
title: Text, text: "two"
"#;
        let diags = duplicate_labels(source);
        assert_eq!(diags.len(), 1, "same-time duplicate flagged: {diags:?}");
    }

    #[test]
    fn cross_scene_redeclaration_is_legal() {
        let source = r#"# SceneA
#0s
title: Text, text: "A"

# SceneB
#0s
title: Text, text: "B"
"#;
        assert!(duplicate_labels(source).is_empty(), "scenes are separate namespaces");
    }

    #[test]
    fn duplicate_children_in_one_container_flagged() {
        let source = r#"row: Row {
  card: Text, text: "a",
  card: Text, text: "b"
}
"#;
        let diags = duplicate_labels(source);
        assert_eq!(diags.len(), 1, "duplicate children flagged: {diags:?}");
    }

    #[test]
    fn same_child_name_in_different_containers_is_legal() {
        let source = r#"left: Row {
  card: Text, text: "a"
}
right: Row {
  card: Text, text: "b"
}
"#;
        assert!(
            duplicate_labels(source).is_empty(),
            "sibling containers are separate namespaces"
        );
    }

    #[test]
    fn let_shadowing_actor_label_is_not_flagged() {
        let source = r#"let title = "unused"
title: Text, text: "hello"
"#;
        assert!(duplicate_labels(source).is_empty(), "let bindings are a separate kind");
    }

    #[test]
    fn array_and_loop_declarations_are_not_flagged() {
        let source = r#"#0s
dots[3]: Circle, radius: 4
for i in {0, 1, 2} {
    box[i]: Rect, size: (10, 10)
}
"#;
        assert!(duplicate_labels(source).is_empty());
    }

    #[test]
    fn lint_config_suppresses_duplicate_label() {
        let source = "// lint-disable: duplicate-label\ntitle: Text, text: \"one\"\ntitle: Text, text: \"two\"\n";
        let analyzer = Analyzer::new(source);
        let diagnostics: Vec<_> = analyzer
            .diagnostics()
            .into_iter()
            .filter(|d| d.code.as_deref() == Some("duplicate-label"))
            .collect();
        assert!(diagnostics.is_empty(), "lint-disable should suppress");
    }
}
