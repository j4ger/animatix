//! Conservative source navigation: only unique direct declarations in this file.
use animatix_syntax::ast::{Span, Stmt};

/// Do not guess from expanded entity labels. Component/loop instances and inline
/// children have no unique source-file identity in the runtime span component.
pub(super) fn direct_declaration(
    statements: &[Stmt],
    scene: Option<&str>,
    label: &str,
    actor_type: &str,
    runtime_span: Option<&Span>,
) -> Option<Span> {
    let mut found = Vec::new();
    collect(statements, scene, label, actor_type, &mut found);
    match found.as_slice() {
        [Some(span)] if runtime_span == Some(span) && span.start_line > 0 && span.start_col > 0 => {
            Some(*span)
        },
        _ => None,
    }
}

fn collect(
    statements: &[Stmt],
    scene: Option<&str>,
    label: &str,
    actor_type: &str,
    found: &mut Vec<Option<Span>>,
) {
    for statement in statements {
        match statement {
            Stmt::Scene { name, body, .. } if scene == Some(name.as_str()) => {
                collect(body, None, label, actor_type, found)
            },
            Stmt::ActorDecl {
                label: declared,
                ty,
                array_index: None,
                is_anonymous: false,
                span,
                ..
            } if scene.is_none() && declared == label && ty == actor_type => found.push(*span),
            Stmt::Keyframe { body, .. } | Stmt::RelativeKeyframe { body, .. }
                if scene.is_none() =>
            {
                collect(body, None, label, actor_type, found)
            },
            // No import, component, inline child or loop expansion: file/instance
            // provenance is not yet available and a guessed location is unsafe.
            _ => {},
        }
    }
}

/// Recover source positions from the parser's declaration occurrences when the
/// AST has no span. AST shape proves this is authored directly, occurrences prove
/// its exact position; neither generated entity names nor text search are used.
pub(super) fn parsed_declaration(
    source: &str,
    statements: &[Stmt],
    scene: Option<&str>,
    label: &str,
    actor_type: &str,
    runtime_span: Option<&Span>,
) -> Option<Span> {
    if let Some(span) = direct_declaration(statements, scene, label, actor_type, runtime_span) {
        return Some(span);
    }
    let mut candidates = Vec::new();
    collect(statements, scene, label, actor_type, &mut candidates);
    // Explicit but mismatching provenance must not fall back to inference.
    if candidates.as_slice() != [None] {
        return None;
    }
    let parsed = animatix_syntax::parser::parse_source_complete(source);
    if !parsed.errors.is_empty() {
        return None;
    }
    use animatix_syntax::occurrence::OccurrenceKind;
    let scenes: Vec<_> = parsed
        .occurrences
        .iter()
        .filter(|o| o.kind == OccurrenceKind::Scene && o.declaration)
        .collect();
    let range = if let Some(scene) = scene {
        let matches: Vec<_> = scenes.iter().enumerate().filter(|(_, o)| o.name == scene).collect();
        let [(index, occurrence)] = matches.as_slice() else {
            return None;
        };
        occurrence.span.end..scenes.get(index + 1).map_or(source.len(), |o| o.span.start)
    } else {
        0..scenes.first().map_or(source.len(), |o| o.span.start)
    };
    let occurrences: Vec<_> = parsed
        .occurrences
        .iter()
        .filter(|o| {
            o.kind == OccurrenceKind::Label
                && o.declaration
                && o.name == label
                && range.contains(&o.span.start)
        })
        .collect();
    let [occurrence] = occurrences.as_slice() else {
        return None;
    };
    let span = Span::from_byte_span(source, occurrence.span);
    if runtime_span.is_some_and(|runtime| {
        runtime.start_line != span.start_line || runtime.start_col != span.start_col
    }) {
        return None;
    }
    Some(span)
}
