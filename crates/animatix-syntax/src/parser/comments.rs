//! Trailing-comment attachment for parsed properties.
//!
//! The tokenizer keeps `//` comments, but the grammar filters them; this module
//! restores `Property::trailing_comment` from the token stream.

use crate::ast::{InlineItem, Property, Stmt};

/// Attach `//` line comments to the properties they follow on the same line.
///
/// The tokenizer keeps comments in its lossless stream, but the parser filters
/// them out. This restores the previous parser's `Property::trailing_comment`
/// behavior from the token stream without reintroducing comments into the
/// grammar.
pub(super) fn attach_trailing_comments(
    statements: &mut [Stmt],
    source: &str,
    tokens: &[crate::token::Token],
) {
    let comments: Vec<(usize, String)> = tokens
        .iter()
        .filter_map(|t| match &t.kind {
            crate::token::TokenKind::Comment(text) => Some((t.span.start, text.clone())),
            _ => None,
        })
        .collect();
    if comments.is_empty() {
        return;
    }

    let mut props: Vec<&mut Property> = Vec::new();
    collect_property_spans(statements, &mut props);

    for (comment_start, text) in comments {
        let mut best: Option<usize> = None;
        let mut best_end = 0usize;
        for (i, prop) in props.iter().enumerate() {
            if let Some(span) = prop.value_span {
                if span.end <= comment_start
                    && !source[span.end..comment_start].contains('\n')
                    && span.end >= best_end
                {
                    best = Some(i);
                    best_end = span.end;
                }
            }
        }
        if let Some(i) = best {
            props[i].trailing_comment = Some(text);
        }
    }
}

/// Collect mutable references to every property that can carry a trailing
/// comment (those produced by [`common::property`], all of which set
/// `value_span`).
fn collect_property_spans<'a>(stmts: &'a mut [Stmt], out: &mut Vec<&'a mut Property>) {
    for stmt in stmts.iter_mut() {
        collect_stmt_property_spans(stmt, out);
    }
}

fn collect_stmt_property_spans<'a>(stmt: &'a mut Stmt, out: &mut Vec<&'a mut Property>) {
    match stmt {
        Stmt::ActorDecl {
            props, children, ..
        } => {
            out.extend(props.iter_mut());
            for child in children {
                collect_inline_property_spans(child, out);
            }
        },
        Stmt::Keyframe { body, .. }
        | Stmt::RelativeKeyframe { body, .. }
        | Stmt::Sequence { body, .. }
        | Stmt::Always { body, .. } => collect_property_spans(body, out),
        Stmt::Stagger { body, .. } => collect_property_spans(body, out),
        Stmt::Conditional {
            then_branch,
            else_branch,
            ..
        } => {
            collect_property_spans(then_branch, out);
            if let Some(else_branch) = else_branch {
                collect_property_spans(else_branch, out);
            }
        },
        Stmt::Match { arms, .. } => {
            for (_, body) in arms {
                collect_property_spans(body, out);
            }
        },
        Stmt::ForLoop { body, .. } => collect_property_spans(body, out),
        Stmt::ComponentDef(def, _) => collect_property_spans(&mut def.body, out),
        Stmt::FnDecl { body, .. } => collect_property_spans(body, out),
        Stmt::Config { settings, .. } => out.extend(settings.iter_mut()),
        Stmt::Scene { config, body, .. } => {
            out.extend(config.iter_mut());
            collect_property_spans(body, out);
        },
        _ => {},
    }
}

fn collect_inline_property_spans<'a>(item: &'a mut InlineItem, out: &mut Vec<&'a mut Property>) {
    match item {
        InlineItem::Anonymous {
            props, children, ..
        }
        | InlineItem::Labeled {
            props, children, ..
        } => {
            out.extend(props.iter_mut());
            for child in children {
                collect_inline_property_spans(child, out);
            }
        },
        InlineItem::ForLoop { body, .. } => {
            for child in body {
                collect_inline_property_spans(child, out);
            }
        },
        InlineItem::SlotFill { items, .. } => {
            for child in items {
                collect_inline_property_spans(child, out);
            }
        },
        InlineItem::SlotMarker => {},
    }
}
