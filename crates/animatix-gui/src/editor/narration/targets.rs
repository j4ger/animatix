use std::ops::Range;

use animatix_syntax::ast::{Expr, Span, Stmt, Time};
use animatix_syntax::narration::{TRACK, decode_anchor, is_implicit_anchor};
use animatix_syntax::parser::parse_source_complete;
use animatix_syntax::token::{LineIndex, Token, TokenKind};

#[derive(Clone)]
pub(super) struct TimingTarget {
    pub scene: Option<String>,
    pub line: usize,
    pub label: String,
    pub marker: Option<String>,
    pub marker_span: Option<Range<usize>>,
    edit: TargetEdit,
}

#[derive(Clone)]
enum TargetEdit {
    Header(Range<usize>),
    Insert { offset: usize, newline: String },
}

impl TimingTarget {
    pub fn change(&self, marker: &str) -> (Range<usize>, String) {
        let header = format!("#{TRACK}@{marker}");
        match &self.edit {
            TargetEdit::Header(range) => (range.clone(), header),
            TargetEdit::Insert { offset, newline } => {
                (*offset..*offset, format!("{header}{newline}"))
            },
        }
    }

    pub fn current_timing<'a>(&self, source: &'a str) -> Option<&'a str> {
        match &self.edit {
            TargetEdit::Header(range) => source.get(range.clone()),
            TargetEdit::Insert { .. } => None,
        }
    }
}

pub(super) fn inspect_source(source: &str) -> (Vec<TimingTarget>, Vec<(String, usize)>) {
    let parsed = parse_source_complete(source);
    let mut targets = Vec::new();
    let mut scenes = Vec::new();
    if !parsed.errors.is_empty() || !parsed.token_issues.is_empty() {
        return (targets, scenes);
    }
    let Some(statements) = parsed.ast else {
        return (targets, scenes);
    };
    let index = LineIndex::new(source);
    for stmt in &statements {
        if let Stmt::Scene {
            name,
            span: Some(span),
            body,
            ..
        } = stmt
        {
            scenes
                .push((name.clone(), index.line_col_to_byte(span.start_line.saturating_sub(1), 0)));
            collect_scope(source, &index, &parsed.tokens, body, Some(name), &mut targets);
        }
    }
    if scenes.is_empty() {
        collect_scope(source, &index, &parsed.tokens, &statements, None, &mut targets);
    } else {
        targets.retain(|target| {
            scenes.iter().filter(|(name, _)| Some(name) == target.scene.as_ref()).count() == 1
        });
    }
    (targets, scenes)
}

fn collect_scope(
    source: &str,
    index: &LineIndex<'_>,
    tokens: &[Token],
    statements: &[Stmt],
    scene: Option<&str>,
    targets: &mut Vec<TimingTarget>,
) {
    for (ordinal, stmt) in statements.iter().enumerate() {
        let next = statements[ordinal + 1..].iter().find(|s| !matches!(s, Stmt::Comment(..)));
        let next_is_absolute = next.is_none_or(|next| {
            matches!(next,
                Stmt::Keyframe { time, span: Some(_), .. } if !is_implicit(time)
            )
        });
        let (time, body, span) = match stmt {
            Stmt::Keyframe { time, body, span } => (time, body.as_slice(), *span),
            Stmt::RelativeKeyframe { offset, body, span } => (offset, body.as_slice(), *span),
            Stmt::Assignment { .. } if next_is_absolute => {
                let Some(span) = action_span(stmt) else {
                    continue;
                };
                if let Some(target) = action_target(source, index, span, scene) {
                    targets.push(target);
                }
                continue;
            },
            _ => continue,
        };
        let actions: Vec<_> = body.iter().filter_map(action_span).collect();
        let direct_body =
            body.iter().all(|s| action_span(s).is_some() || matches!(s, Stmt::Comment(..)));
        if is_implicit(time) {
            // Inserting an explicit header at the first action rebinds the
            // entire group. The original paragraph still supplies narration.
            if direct_body
                && let Some(action) = actions.first()
                && let Some(mut target) = action_target(source, index, *action, scene)
            {
                target.marker = narration_marker(time);
                if actions.len() > 1 {
                    target.label = format!("Paragraph group · {} actions", actions.len());
                }
                targets.push(target);
            }
        } else if let Some(span) = span {
            let start = span_start(index, span);
            let Some(header) = timing_header(tokens, start, span_end(index, span), time) else {
                continue;
            };
            let label = match actions.as_slice() {
                [] => "Keyframe".to_owned(),
                [action] => format!("Keyframe · {}", snippet(source, index, *action)),
                _ => format!("Keyframe group · {} actions", actions.len()),
            };
            targets.push(TimingTarget {
                scene: scene.map(str::to_owned),
                line: span.start_line.saturating_sub(1),
                label,
                marker: narration_marker(time),
                marker_span: header.marker,
                edit: TargetEdit::Header(header.range),
            });
        } else {
            // A synthetic 0s keyframe can acquire an explicit header when
            // the following source statement cannot join the new keyframe.
            if next_is_absolute
                && let [action] = actions.as_slice()
                && let Some(target) = action_target(source, index, *action, scene)
            {
                targets.push(target);
            }
            continue;
        }
        // Splitting the final direct action leaves the original group intact.
        // A relative successor would inherit the new marker instead, so the
        // individual action is unavailable in that case.
        if next_is_absolute
            && actions.len() > 1
            && direct_body
            && let Some(action) = actions.last()
            && let Some(target) = action_target(source, index, *action, scene)
        {
            targets.push(target);
        }
    }
}

fn action_span(stmt: &Stmt) -> Option<Span> {
    match stmt {
        Stmt::Expr(Expr::Call(..), span) => *span,
        Stmt::Assignment { span, target, .. }
            if target.iter().all(|segment| segment.as_static_str().is_some()) =>
        {
            *span
        },
        _ => None,
    }
}

fn action_target(
    source: &str,
    index: &LineIndex<'_>,
    span: Span,
    scene: Option<&str>,
) -> Option<TimingTarget> {
    let line = span.start_line.checked_sub(1)?;
    let offset = index.line_col_to_byte(line, 0);
    let start = span_start(index, span);
    if !source.get(offset..start)?.trim().is_empty() {
        return None;
    }
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    Some(TimingTarget {
        scene: scene.map(str::to_owned),
        line,
        label: format!("Action · {}", snippet(source, index, span)),
        marker: None,
        marker_span: None,
        edit: TargetEdit::Insert {
            offset,
            newline: newline.to_owned(),
        },
    })
}

fn snippet(source: &str, index: &LineIndex<'_>, span: Span) -> String {
    let line = source
        .get(span_start(index, span)..span_end(index, span))
        .and_then(|text| text.lines().next())
        .unwrap_or_default()
        .trim();
    let line_end = line.char_indices().nth(44).map(|(offset, _)| offset);
    line_end.map_or_else(|| line.to_owned(), |end| format!("{}…", &line[..end]))
}

fn span_start(index: &LineIndex<'_>, span: Span) -> usize {
    index.line_col_to_byte(span.start_line.saturating_sub(1), span.start_col.saturating_sub(1))
}

fn span_end(index: &LineIndex<'_>, span: Span) -> usize {
    index.line_col_to_byte(span.end_line.saturating_sub(1), span.end_col.saturating_sub(1))
}

fn is_implicit(time: &Time) -> bool {
    matches!(time, Time::SpeechMarker { marker, .. } if is_implicit_anchor(marker))
}

fn narration_marker(time: &Time) -> Option<String> {
    match time {
        Time::SpeechMarker { marker, .. } if decode_anchor(marker).is_some() => {
            Some(marker.clone())
        },
        _ => None,
    }
}

struct TimingHeader {
    range: Range<usize>,
    marker: Option<Range<usize>>,
}

fn timing_header(tokens: &[Token], start: usize, end: usize, time: &Time) -> Option<TimingHeader> {
    let first = tokens.partition_point(|token| token.span.start < start);
    let last = tokens.partition_point(|token| token.span.end <= end);
    let mut header = tokens.get(first..last)?.iter().peekable();
    let hash = header.next()?;
    if hash.span.start != start || hash.kind != TokenKind::Hash {
        return None;
    }
    header.next_if(|token| token.kind == TokenKind::Plus);
    let time_start = header.next()?;
    let mut marker_span = None;
    let mut last = match (&time_start.kind, time) {
        (TokenKind::Time { .. }, Time::Seconds(_) | Time::Milliseconds(_))
        | (TokenKind::BeatTime { .. }, Time::Beats(_)) => time_start,
        (TokenKind::Ident(_), Time::SpeechMarker { marker, .. }) => {
            header.next_if(|token| token.kind == TokenKind::At)?;
            let identifier = header
                .next_if(|token| matches!(&token.kind, TokenKind::Ident(name) if name == marker))?;
            marker_span = Some(identifier.span.start..identifier.span.end);
            identifier
        },
        (TokenKind::Ident(_), Time::SpeechWord { .. }) => {
            header.next_if(|token| token.kind == TokenKind::LBracket)?;
            header.next_if(|token| matches!(token.kind, TokenKind::Str(_)))?;
            if header.next_if(|token| token.kind == TokenKind::Comma).is_some() {
                header.next_if(|token| matches!(token.kind, TokenKind::Number(_)))?;
            }
            header.next_if(|token| token.kind == TokenKind::RBracket)?
        },
        _ => return None,
    };
    if matches!(time, Time::SpeechMarker { .. } | Time::SpeechWord { .. })
        && header
            .next_if(|token| matches!(token.kind, TokenKind::Plus | TokenKind::Minus))
            .is_some()
    {
        last = header.next_if(|token| matches!(token.kind, TokenKind::Time { .. }))?;
    }
    Some(TimingHeader {
        range: start..last.span.end,
        marker: marker_span,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_patch_changes_only_timing_tokens_and_leaves_colon_and_comment() {
        let source = "# Opening\n#  +  500ms: // 相对\nfade_in(target: title)\n";
        let (targets, _) = inspect_source(source);
        let target = &targets[0];
        assert_eq!(target.scene.as_deref(), Some("Opening"));
        let (range, replacement) = target.change("cue");
        assert_eq!(&source[range.clone()], "#  +  500ms");
        let mut edited = source.to_owned();
        edited.replace_range(range, &replacement);
        assert_eq!(edited, "# Opening\n#narration@cue: // 相对\nfade_in(target: title)\n");
    }

    #[test]
    fn only_trailing_direct_action_can_split_from_group() {
        let source = "#1s\na.opacity = 0.5\nb.opacity = 0.5 // keep\n#2s\na.opacity = 1\n";
        let (targets, _) = inspect_source(source);
        let target = targets.iter().find(|target| target.label.starts_with("Action")).unwrap();
        let (range, replacement) = target.change("cue");
        let mut edited = source.to_owned();
        edited.replace_range(range, &replacement);
        assert_eq!(
            edited,
            "#1s\na.opacity = 0.5\n#narration@cue\nb.opacity = 0.5 // keep\n#2s\na.opacity = 1\n"
        );

        let relative = source.replace("#2s", "#+2s");
        let (targets, _) = inspect_source(&relative);
        assert!(targets.iter().all(|target| !target.label.starts_with("Action")));
    }

    #[test]
    fn implicit_paragraph_group_can_bind_without_touching_its_prose() {
        let source = "/// 开场，欢迎。\na.opacity = 0.5\nb.opacity = 1\n#2s\n";
        let (targets, _) = inspect_source(source);
        let group = targets
            .iter()
            .find(|target| target.label.starts_with("Paragraph group"))
            .unwrap();
        let (range, replacement) = group.change("cue");
        let mut edited = source.to_owned();
        edited.replace_range(range, &replacement);
        assert_eq!(
            edited,
            "/// 开场，欢迎。\n#narration@cue\na.opacity = 0.5\nb.opacity = 1\n#2s\n"
        );
        let (targets, _) = inspect_source(&edited);
        assert!(targets.iter().any(|target| target.label == "Keyframe group · 2 actions"));
    }
}
