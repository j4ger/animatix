//! Every AST variant must survive the formatter with its authored values intact.
//!
//! This file replaces three `assert_eq!(arms, arms)` tautologies that used to
//! live in `format_core::variant_coverage_guardrails`. Those could never fail —
//! and one of them had drifted: it still counted 20 `Stmt` variants and named a
//! `ComponentAction` that does not exist, while claiming to guard a formatter
//! that was at that moment silently deleting every authored `ease:` from `play`
//! transitions.
//!
//! The guard here is behavioural, and it has three layers:
//!
//! 1. **Compiler-enforced variant discovery.** [`variant_of`] and [`scrub_in_place`] match
//!    exhaustively on `Stmt`. Adding a variant is a build error here until the new arm is written —
//!    which is the bookkeeping the count tests only pretended to do.
//! 2. **Coverage.** [`every_stmt_variant_has_a_probe`] fails if a variant has no probe document.
//! 3. **Fidelity.** Each probe is parsed, formatted, re-parsed, and the two ASTs are compared for
//!    *exact structural equality* (spans aside), plus the formatter must be stable on its own
//!    output. A field that is read but not printed, or printed in a form that comes back
//!    differently, fails here.

use animatix_syntax::ast::{
    BinaryOp, Expr, InlineItem, LoopPattern, MatchPattern, Property, Stmt, Transition, UnaryOp,
};
use animatix_syntax::format_core::{
    format_expr, format_inline_item, format_stmt_raw, format_stmts_raw,
};

fn parse(src: &str) -> Vec<Stmt> {
    let (stmts, errors) = animatix_syntax::parser::parse_source(src);
    assert!(
        errors.is_empty(),
        "parse errors {:?} for:\n{src}",
        errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
    stmts.expect("no statements")
}

// ---------------------------------------------------------------------------
// Layer 1 — exhaustive plumbing over `Stmt`
// ---------------------------------------------------------------------------

/// The variant name of a statement, as used by the probe table.
///
/// Exhaustive on purpose: a new `Stmt` variant breaks this match, and the arm
/// added to satisfy it is what forces a new entry in [`PROBES`] /
/// [`AST_ONLY_VARIANTS`].
fn variant_of(stmt: &Stmt) -> &'static str {
    match stmt {
        Stmt::Action(..) => "Action",
        Stmt::LetDecl { .. } => "LetDecl",
        Stmt::ActorDecl { .. } => "ActorDecl",
        Stmt::TypeAlias { .. } => "TypeAlias",
        Stmt::Import { .. } => "Import",
        Stmt::Keyframe { .. } => "Keyframe",
        Stmt::RelativeKeyframe { .. } => "RelativeKeyframe",
        Stmt::Assignment { .. } => "Assignment",
        Stmt::Sequence { .. } => "Sequence",
        Stmt::Stagger { .. } => "Stagger",
        Stmt::Always { .. } => "Always",
        Stmt::ReactiveBinding { .. } => "ReactiveBinding",
        Stmt::Conditional { .. } => "Conditional",
        Stmt::Match { .. } => "Match",
        Stmt::ForLoop { .. } => "ForLoop",
        Stmt::ComponentDef(..) => "ComponentDef",
        Stmt::FnDecl { .. } => "FnDecl",
        Stmt::Block { .. } => "Block",
        Stmt::Return { .. } => "Return",
        Stmt::Expr(..) => "Expr",
        Stmt::Config { .. } => "Config",
        Stmt::Scene { .. } => "Scene",
        Stmt::Play { .. } => "Play",
        Stmt::Comment(..) => "Comment",
        Stmt::Step { .. } => "Step",
    }
}

fn scrub_props(props: &mut [Property]) {
    for prop in props {
        prop.value_span = None;
    }
}

fn scrub_inline(items: &mut [InlineItem]) {
    for item in items {
        match item {
            InlineItem::Anonymous {
                props, children, ..
            }
            | InlineItem::Labeled {
                props, children, ..
            } => {
                scrub_props(props);
                scrub_inline(children);
            },
            InlineItem::ForLoop { body, .. } | InlineItem::SlotFill { items: body, .. } => {
                scrub_inline(body)
            },
            InlineItem::SlotMarker => {},
        }
    }
}

/// Drop position data in place, so two ASTs from different source texts can be
/// compared for structural equality.
///
/// Byte offsets legitimately change when a formatter rewrites a document;
/// nothing else may. The match is exhaustive for the same reason as
/// [`variant_of`]: a variant that gains a span, or gains a nested statement /
/// property list this guard should walk, cannot be added without updating it.
fn scrub_in_place(stmts: &mut [Stmt]) {
    for stmt in stmts {
        match stmt {
            Stmt::Action(action, span) => {
                *span = None;
                action.byte_span = None;
            },
            Stmt::LetDecl { span, .. }
            | Stmt::TypeAlias { span, .. }
            | Stmt::Import { span, .. }
            | Stmt::Return { span, .. } => *span = None,
            Stmt::ActorDecl {
                props,
                children,
                span,
                ..
            } => {
                *span = None;
                scrub_props(props);
                scrub_inline(children);
            },
            Stmt::Keyframe { body, span, .. }
            | Stmt::RelativeKeyframe { body, span, .. }
            | Stmt::Sequence { body, span, .. }
            | Stmt::Stagger { body, span, .. }
            | Stmt::Always { body, span, .. }
            | Stmt::ForLoop { body, span, .. }
            | Stmt::Block { body, span, .. }
            | Stmt::FnDecl { body, span, .. } => {
                *span = None;
                scrub_in_place(body);
            },
            Stmt::Assignment {
                value_span, span, ..
            }
            | Stmt::ReactiveBinding {
                value_span, span, ..
            } => {
                *value_span = None;
                *span = None;
            },
            Stmt::Conditional {
                then_branch,
                else_branch,
                span,
                ..
            } => {
                *span = None;
                scrub_in_place(then_branch);
                if let Some(else_branch) = else_branch {
                    scrub_in_place(else_branch);
                }
            },
            Stmt::Match { arms, span, .. } => {
                *span = None;
                for (_pattern, body) in arms {
                    scrub_in_place(body);
                }
            },
            Stmt::ComponentDef(def, span) => {
                *span = None;
                scrub_in_place(&mut def.body);
            },
            Stmt::Expr(_expr, span) => *span = None,
            Stmt::Config { settings, span } => {
                *span = None;
                scrub_props(settings);
            },
            Stmt::Scene {
                config, body, span, ..
            } => {
                *span = None;
                scrub_props(config);
                scrub_in_place(body);
            },
            Stmt::Play { span, .. } => *span = None,
            Stmt::Comment(_text, span) => *span = None,
            Stmt::Step { span, .. } => *span = None,
        }
    }
}

fn scrubbed(stmts: &[Stmt]) -> Vec<Stmt> {
    let mut owned = stmts.to_vec();
    scrub_in_place(&mut owned);
    owned
}

/// Every variant name reachable in a document, at any depth.
///
/// Needed because the parser is free to wrap a statement: a top-level `move`
/// lands inside an implicit `#0s` keyframe, so "the first statement must be the
/// probe's variant" would be a lie. Exhaustive for the same reason as
/// [`variant_of`].
fn collect_variants(stmts: &[Stmt], out: &mut Vec<&'static str>) {
    for stmt in stmts {
        out.push(variant_of(stmt));
        match stmt {
            Stmt::Action(..)
            | Stmt::LetDecl { .. }
            | Stmt::TypeAlias { .. }
            | Stmt::Import { .. }
            | Stmt::Assignment { .. }
            | Stmt::ReactiveBinding { .. }
            | Stmt::Return { .. }
            | Stmt::Expr(..)
            | Stmt::Config { .. }
            | Stmt::Play { .. }
            | Stmt::Comment(..)
            | Stmt::Step { .. }
            | Stmt::ActorDecl { .. } => {},
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::ForLoop { body, .. }
            | Stmt::Block { body, .. }
            | Stmt::FnDecl { body, .. } => collect_variants(body, out),
            Stmt::Conditional {
                then_branch,
                else_branch,
                ..
            } => {
                collect_variants(then_branch, out);
                if let Some(else_branch) = else_branch {
                    collect_variants(else_branch, out);
                }
            },
            Stmt::Match { arms, .. } => {
                for (_pattern, body) in arms {
                    collect_variants(body, out);
                }
            },
            Stmt::ComponentDef(def, ..) => collect_variants(&def.body, out),
            Stmt::Scene { body, .. } => collect_variants(body, out),
        }
    }
}

// ---------------------------------------------------------------------------
// Layer 2 — the probe table
// ---------------------------------------------------------------------------

/// One document per `Stmt` variant. The probe's *first* statement must be the
/// variant being covered; the rest of the document supplies whatever that
/// variant needs to be meaningful (a declared actor, a scene to play).
///
/// Each probe also nests statements that other probes cover, so the structural
/// comparison in [`every_probe_survives_the_formatter`] sees them at depth too.
struct Probe {
    variant: &'static str,
    source: &'static str,
}

const PROBES: &[Probe] = &[
    Probe {
        variant: "Action",
        source: "\
#1s
move b to (300, 200) [700ms, ease: ease-in-out]
b: Rect, size: (20, 20), color: (1.0, 0.5, 0.2, 1.0), at: (60, 90)
",
    },
    Probe {
        variant: "LetDecl",
        source: "\
pub let ledger_total = 42
b: Rect, size: (20, 20), color: (1.0, 0.5, 0.2, 1.0), at: (60, 90)
",
    },
    Probe {
        variant: "ActorDecl",
        source: "\
row: Row {
  a: Rect, size: (6, 40), color: (0.2, 0.3, 0.4, 1.0), at: (0, 0)
  gap: 12
}
",
    },
    Probe {
        variant: "TypeAlias",
        source: "\
pub type LegendMode = Num | Str
",
    },
    Probe {
        variant: "Import",
        source: "\
import \"./lib/tokens.amx\" as tk
",
    },
    Probe {
        variant: "Keyframe",
        source: "\
#1.5s
  b.color = (0.9, 0.1, 0.1, 1.0) [300ms, ease: spring(6, 9)]
b: Rect, size: (20, 20), color: (1.0, 0.5, 0.2, 1.0), at: (60, 90)
",
    },
    Probe {
        variant: "RelativeKeyframe",
        source: "\
#+0.5s
  b.opacity = 0.25
",
    },
    Probe {
        variant: "Assignment",
        source: "\
b.color = (0.9, 0.1, 0.1, 1.0) [300ms, ease: expo-out]
",
    },
    Probe {
        variant: "Sequence",
        source: "\
sequence {
  b.opacity = 1.0
  b.rotation = 20
}
",
    },
    Probe {
        variant: "Stagger",
        source: "\
stagger [150ms] {
  b.opacity = 1.0
  b.rotation = 20
}
",
    },
    Probe {
        variant: "Always",
        source: "\
always {
  b.at = (100, 200)
}
",
    },
    Probe {
        variant: "ReactiveBinding",
        source: "\
b.rotation := ledger_total * 2
",
    },
    Probe {
        variant: "Conditional",
        source: "\
if ledger_total > 2 {
  b.opacity = 1.0
} else {
  b.opacity = 0.2
}
",
    },
    Probe {
        variant: "Match",
        source: "\
match ledger_total {
  0 => {
    b.opacity = 1.0
  },
  1..=3 => {
    b.opacity = 0.5
  },
  _ => {
    b.opacity = 0.1
  }
}
",
    },
    Probe {
        variant: "ForLoop",
        source: "\
for name, idx in {1, 2, 3} [step: 100ms] {
  b.at = (10, 10)
}
",
    },
    Probe {
        variant: "ComponentDef",
        source: "\
pub component MetricCard(title: Str = \"Metric\", value: Str = \"0\") {
  card: Rect, size: (120, 60), color: (0.1, 0.1, 0.12, 1.0), at: (0, 0)
  fn highlight {
    move self to (10, 10)
  }
}
",
    },
    Probe {
        variant: "FnDecl",
        source: "\
pub fn scaled_amount(v: Num, offset: Num = 1) -> Num {
  return v * 2
}
",
    },
    Probe {
        variant: "Return",
        source: "\
fn pick(v: Num) -> Num {
  return v
}
",
    },
    Probe {
        variant: "Expr",
        source: "\
fn tail_value(v: Num) -> Num {
  v * 2 + 1
}
",
    },
    Probe {
        variant: "Config",
        source: "\
config { resolution: (400, 300), duration: 3, fps: 30 }
",
    },
    Probe {
        variant: "Scene",
        source: "\
# Intro
config { resolution: (400, 300), duration: 1 }
b: Rect, size: (20, 20), color: (1.0, 0.5, 0.2, 1.0), at: (60, 90)
#0.5s
b.opacity = 1.0
",
    },
    Probe {
        variant: "Play",
        source: "\
play Intro [wipe-left, 500ms, ease: expo-out]
# Intro
config { resolution: (400, 300), duration: 1 }
b: Rect, size: (20, 20), color: (1.0, 0.5, 0.2, 1.0), at: (60, 90)
",
    },
    Probe {
        variant: "Step",
        source: "\
#1s
#step 1
b: Rect, size: (20, 20), color: (1.0, 0.5, 0.2, 1.0), at: (60, 90)
",
    },
];

/// Variants the parser cannot produce, so no probe document exists for them.
///
/// `Block` is only ever synthesized by timeline-function expansion, and
/// `Comment` is printed by the formatter but never emitted by the parser —
/// that gap is the comment-fidelity item in `docs/roadmap.md`. Both are
/// covered AST-directly by [`ast_only_variants_still_print_their_content`],
/// and when the parser gains the syntax they should move into [`PROBES`].
const AST_ONLY_VARIANTS: &[&str] = &["Block", "Comment"];

const ALL_VARIANTS: &[&str] = &[
    "Action",
    "LetDecl",
    "ActorDecl",
    "TypeAlias",
    "Import",
    "Keyframe",
    "RelativeKeyframe",
    "Assignment",
    "Sequence",
    "Stagger",
    "Always",
    "ReactiveBinding",
    "Conditional",
    "Match",
    "ForLoop",
    "ComponentDef",
    "FnDecl",
    "Block",
    "Return",
    "Expr",
    "Config",
    "Scene",
    "Play",
    "Step",
    "Comment",
];

#[test]
fn every_stmt_variant_has_a_probe() {
    let covered: Vec<&str> = PROBES
        .iter()
        .map(|probe| probe.variant)
        .chain(AST_ONLY_VARIANTS.iter().copied())
        .collect();
    let missing: Vec<&str> =
        ALL_VARIANTS.iter().copied().filter(|name| !covered.contains(name)).collect();
    assert!(missing.is_empty(), "Stmt variants with no formatter probe: {missing:?}");

    let unknown: Vec<&&str> = covered.iter().filter(|name| !ALL_VARIANTS.contains(name)).collect();
    assert!(
        unknown.is_empty(),
        "probes name variants that no longer exist: {unknown:?} — either the variant was \
         renamed (update ALL_VARIANTS and the probe) or ALL_VARIANTS is stale"
    );
    let mut sorted = ALL_VARIANTS.to_vec();
    sorted.sort_unstable();
    let duplicates: Vec<&str> = sorted
        .windows(2)
        .filter(|pair| pair[0] == pair[1])
        .map(|pair| pair[0])
        .collect();
    assert!(
        duplicates.is_empty(),
        "ALL_VARIANTS lists {duplicates:?} twice, so coverage could hide a gap"
    );
}

#[test]
fn every_probe_survives_the_formatter() {
    for probe in PROBES {
        let stmts = parse(probe.source);
        let mut reachable: Vec<&'static str> = Vec::new();
        collect_variants(&stmts, &mut reachable);
        assert!(
            reachable.contains(&probe.variant),
            "probe for {} never produced one; this document reached {:?}:\n{}",
            probe.variant,
            reachable,
            probe.source
        );

        let once = format_stmts_raw(&stmts, 0, 2);
        let retyped = parse(&once);
        assert_eq!(
            scrubbed(&stmts),
            scrubbed(&retyped),
            "the formatter changed what the parser reads for {}.\n\
             authored source:\n{}\nformatted once:\n{}",
            probe.variant,
            probe.source,
            once
        );

        let twice = format_stmts_raw(&retyped, 0, 2);
        assert_eq!(
            once, twice,
            "formatting is not idempotent for {}\nfirst:\n{}\nsecond:\n{}",
            probe.variant, once, twice
        );
    }
}

#[test]
fn ast_only_variants_still_print_their_content() {
    let comment = Stmt::Comment(" a standalone note".into(), None);
    assert_eq!(format_stmt_raw(&comment, 0, 2), "// a standalone note");

    let block = Stmt::Block {
        body: vec![Stmt::Comment(" inside".into(), None)],
        span: None,
    };
    let printed = format_stmt_raw(&block, 0, 2);
    assert!(printed.contains("// inside"), "a Block lost its body: {printed}");
}

// ---------------------------------------------------------------------------
// Layer 3 — the same fidelity test for expressions
// ---------------------------------------------------------------------------

/// Expressions are compared exactly, because `Expr` carries no position data:
/// anything `format_expr` fails to print comes back as a *different* AST node.
///
/// The third field is a prefix the value position needs before the printed form
/// parses at all — a `LetChain` is only writable as a closure body, since the
/// grammar reads a bare `{ let … }` as no expression here.
fn probes_expr() -> Vec<(&'static str, Expr, &'static str)> {
    vec![
        ("Num", Expr::Num(42.5), ""),
        ("Percent", Expr::Percent(37.0), ""),
        ("Str", Expr::Str("hello there".into()), ""),
        ("Bool", Expr::Bool(true), ""),
        ("Null", Expr::Null, ""),
        ("Ident", Expr::Ident("ledger_total".into()), ""),
        ("Path", Expr::Path(vec!["palette".into(), "accent".into()]), ""),
        (
            "Index",
            Expr::Index(Box::new(Expr::Ident("items".into())), Box::new(Expr::Num(2.0))),
            "",
        ),
        ("List", Expr::List(vec![Expr::Num(1.0), Expr::Num(2.0), Expr::Num(3.0)]), ""),
        ("Tuple", Expr::Tuple(vec![Expr::Num(1.5), Expr::Num(2.5)]), ""),
        (
            "Binary",
            Expr::Binary(
                Box::new(Expr::Ident("left_side".into())),
                BinaryOp::Add,
                Box::new(Expr::Ident("right_side".into())),
            ),
            "",
        ),
        (
            "Unary",
            Expr::Unary(UnaryOp::Neg, Box::new(Expr::Ident("ledger_total".into()))),
            "",
        ),
        (
            "Call",
            Expr::Call("lerp".into(), vec![Expr::Num(0.0), Expr::Num(1.0), Expr::Num(0.5)]),
            "",
        ),
        (
            "Method",
            Expr::Method(
                Box::new(Expr::Ident("curve".into())),
                "scale".into(),
                vec![Expr::Num(2.0)],
            ),
            "",
        ),
        (
            "Closure",
            Expr::Closure(
                vec!["amt".into()],
                Box::new(Expr::Binary(
                    Box::new(Expr::Ident("amt".into())),
                    BinaryOp::Mul,
                    Box::new(Expr::Num(2.0)),
                )),
            ),
            "",
        ),
        (
            "LetChain",
            Expr::Closure(
                vec!["amt".into()],
                Box::new(Expr::LetChain(
                    vec![(
                        "twice".into(),
                        Expr::Binary(
                            Box::new(Expr::Ident("amt".into())),
                            BinaryOp::Mul,
                            Box::new(Expr::Num(2.0)),
                        ),
                    )],
                    Box::new(Expr::Ident("twice".into())),
                )),
            ),
            "",
        ),
        (
            "Conditional",
            Expr::Conditional(
                Box::new(Expr::Ident("flag_a".into())),
                Box::new(Expr::Num(1.0)),
                Box::new(Expr::Num(2.0)),
            ),
            "",
        ),
        (
            "Match",
            Expr::Match(
                Box::new(Expr::Ident("flag_a".into())),
                vec![
                    (MatchPattern::Bool(true), Box::new(Expr::Num(1.0))),
                    (MatchPattern::Wildcard, Box::new(Expr::Num(2.0))),
                ],
            ),
            "",
        ),
        (
            "Construct",
            Expr::Construct("Badge".into(), vec![Property::new("size", Expr::Num(3.0))]),
            "",
        ),
    ]
}

#[test]
fn every_expr_variant_round_trips_through_the_printer() {
    let printed_names: Vec<&str> = probes_expr().iter().map(|(name, _, _)| *name).collect();
    for name in [
        "Num",
        "Percent",
        "Str",
        "Bool",
        "Null",
        "Ident",
        "Path",
        "Index",
        "List",
        "Tuple",
        "Binary",
        "Unary",
        "Call",
        "Method",
        "Closure",
        "LetChain",
        "Conditional",
        "Match",
        "Construct",
    ] {
        assert!(
            printed_names.contains(&name),
            "no expression probe for {name} — Expr has no such variant any more, or the probe was deleted"
        );
    }

    for (name, expr, prefix) in probes_expr() {
        let printed = format_expr(&expr);
        let stmts = parse(&format!("probe_target = {prefix}{printed}\n"));
        let reparsed = match &stmts[0] {
            Stmt::Assignment { value, .. } => value.clone(),
            other => {
                panic!("{name}: probe text {printed:?} did not reparse as a value, got {other:?}")
            },
        };
        assert_eq!(
            expr, reparsed,
            "format_expr lost or rewrote something: {name}\nprinted: {printed:?}\nread back: {reparsed:?}"
        );
    }
}

#[test]
fn every_binary_and_unary_operator_round_trips() {
    let ops = [
        BinaryOp::Add,
        BinaryOp::Sub,
        BinaryOp::Mul,
        BinaryOp::Div,
        BinaryOp::Mod,
        BinaryOp::Pow,
        BinaryOp::Eq,
        BinaryOp::Neq,
        BinaryOp::Lt,
        BinaryOp::Gt,
        BinaryOp::Lte,
        BinaryOp::Gte,
        BinaryOp::And,
        BinaryOp::Or,
    ];
    assert_eq!(ops.len(), 14, "BinaryOp drifted");
    for op in ops {
        let expr = Expr::Binary(
            Box::new(Expr::Ident("left_side".into())),
            op.clone(),
            Box::new(Expr::Ident("right_side".into())),
        );
        let printed = format_expr(&expr);
        let stmts = parse(&format!("probe_target = {printed}\n"));
        let reparsed = match &stmts[0] {
            Stmt::Assignment { value, .. } => value,
            other => panic!("{printed:?} did not reparse as a value: {other:?}"),
        };
        assert_eq!(&expr, reparsed, "operator {:?} did not survive {:?}", op, printed);
    }

    for op in [UnaryOp::Neg, UnaryOp::Not] {
        let expr = Expr::Unary(op.clone(), Box::new(Expr::Ident("ledger_total".into())));
        let printed = format_expr(&expr);
        let stmts = parse(&format!("probe_target = {printed}\n"));
        let reparsed = match &stmts[0] {
            Stmt::Assignment { value, .. } => value,
            other => panic!("{printed:?} did not reparse as a value: {other:?}"),
        };
        assert_eq!(&expr, reparsed, "unary {:?} did not survive {:?}", op, printed);
    }
}

// ---------------------------------------------------------------------------
// Layer 3b — container items
// ---------------------------------------------------------------------------

#[test]
fn every_inline_item_variant_round_trips() {
    let items = vec![
        InlineItem::Anonymous {
            ty: "Rect".into(),
            props: vec![Property::new(
                "size",
                Expr::Tuple(vec![Expr::Num(6.0), Expr::Num(8.0)]),
            )],
            modifiers: vec![],
            children: vec![],
        },
        InlineItem::Labeled {
            label: "chip".into(),
            array_index: None,
            ty: "Text".into(),
            props: vec![Property::new("text", Expr::Str("hello".into()))],
            modifiers: vec![],
            children: vec![],
        },
        InlineItem::ForLoop {
            var: LoopPattern::Single("entry".into()),
            index_var: Some("idx".into()),
            iterable: Expr::List(vec![Expr::Num(1.0), Expr::Num(2.0)]),
            body: vec![InlineItem::Anonymous {
                ty: "Rect".into(),
                props: vec![Property::new("size", Expr::Num(4.0))],
                modifiers: vec![],
                children: vec![],
            }],
        },
        InlineItem::SlotMarker,
        InlineItem::SlotFill {
            slot: "header".into(),
            items: vec![InlineItem::Anonymous {
                ty: "Text".into(),
                props: vec![Property::new("text", Expr::Str("titled".into()))],
                modifiers: vec![],
                children: vec![],
            }],
        },
    ];

    let rendered: Vec<String> = items.iter().map(|item| format_inline_item(item, 0, 2)).collect();
    let source = format!(
        "board: Row {{\n{}\n}}\n",
        rendered.iter().map(|line| format!("  {line}")).collect::<Vec<_>>().join("\n")
    );
    let stmts = parse(&source);
    let children = match &stmts[0] {
        Stmt::ActorDecl { children, .. } => children,
        other => panic!("container probe did not parse as an actor declaration: {other:?}"),
    };
    let mut owned = children.clone();
    scrub_inline(&mut owned);
    assert_eq!(
        items, owned,
        "a container item lost something.\nsource:\n{source}\nread back: {owned:#?}"
    );
}

// ---------------------------------------------------------------------------
// The specific regressions this guard exists for
// ---------------------------------------------------------------------------

/// Action spellings that must survive formatting unchanged in structure.
///
/// The tuple-argument case shipped broken: `animatix fmt` rewrote
/// `move b to (300, 200)` into `move b, to 300, 200`, which the parser rejects
/// outright — so running the formatter on any file with a positioned `move`
/// destroyed it rather than tidying it.
#[test]
fn every_action_spelling_reparses() {
    for line in [
        "move b to (300, 200)",
        "move b to (300, 200) [700ms, ease: ease-in-out]",
        "fade-in b [500ms]",
        "fade-in a, b [500ms]",
        "scale b by (2, 3)",
        "shake b 20 [300ms]",
        "pulse bars[j] [2.3s, intensity: 190]",
        "highlight_key(bars, 3)",
        "say b \"hi there\" [1.2s]",
    ] {
        let stmts = parse(&format!("{line}\n"));
        let formatted = format_stmts_raw(&stmts, 0, 2);
        let reparsed = parse(&formatted);
        assert_eq!(
            scrubbed(&stmts),
            scrubbed(&reparsed),
            "an action did not survive formatting\nauthored: {line}\nformatted: {formatted}"
        );
        assert_eq!(
            format_stmts_raw(&reparsed, 0, 2),
            formatted,
            "action formatting is not idempotent for {line:?}"
        );
    }
}

/// `Transition` has three fields and the formatter used to print two of them.
/// Pinning the tuple directly keeps a future `format_transition` from quietly
/// going back to `id` + `duration`.
#[test]
fn a_transition_prints_all_three_of_its_fields() {
    let typed: &[(&str, Transition)] = &[
        (
            "EaseInOut",
            Transition {
                id: "wipe-left".into(),
                duration_ms: 500,
                easing: animatix_syntax::easing::Easing::EaseInOut,
            },
        ),
        (
            "ExpoOut",
            Transition {
                id: "wipe-left".into(),
                duration_ms: 500,
                easing: animatix_syntax::easing::Easing::ExpoOut,
            },
        ),
        (
            "CubicBezier",
            Transition {
                id: "fade".into(),
                duration_ms: 800,
                easing: animatix_syntax::easing::Easing::CubicBezier([0.16, 1.0, 0.3, 1.0]),
            },
        ),
        (
            "Spring",
            Transition {
                id: "fade".into(),
                duration_ms: 800,
                easing: animatix_syntax::easing::Easing::Spring {
                    damping: 6.0,
                    frequency: 9.0,
                },
            },
        ),
    ];

    for (label, transition) in typed {
        let stmt = Stmt::Play {
            scene_name: "Intro".into(),
            transition: Some(transition.clone()),
            span: None,
        };
        let printed = format_stmt_raw(&stmt, 0, 2);
        let reparsed = parse(&printed);
        let read_back = match &reparsed[0] {
            Stmt::Play { transition, .. } => transition
                .clone()
                .unwrap_or_else(|| panic!("play lost its transition: {printed:?}")),
            other => panic!("unexpected statement: {other:?}"),
        };
        assert_eq!(
            *transition, read_back,
            "a {label} transition did not survive formatting; printed {printed:?}"
        );
    }
}
