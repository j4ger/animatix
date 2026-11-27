//! Compiled times for authored source locations, in composition-wide seconds.
use super::DocumentSession;
use animatix::composition::Composition;
use animatix::timeline::Timeline;
use animatix_syntax::ast::{Stmt, Time};
use animatix_syntax::to_source::ToSource;
use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub(crate) struct SourceTiming {
    // An unresolved header ends the preceding source interval.
    entries: BTreeMap<usize, (Option<String>, Option<f64>)>,
    named_cues: Vec<TimingMarker>,
}

impl SourceTiming {
    pub(super) fn from_document(doc: &DocumentSession) -> Self {
        let mut entries = BTreeMap::new();
        let mut markers = review_markers(
            doc.raw_statements.as_deref().unwrap_or_default(),
            doc.timeline.as_ref(),
            doc.composition.as_ref(),
            &doc.diagnostics,
        );
        locate(&doc.source_text, &mut markers);
        if let Some(composition) = &doc.composition {
            for (name, scene) in &composition.scenes {
                if let Some(span) = scene.source_span {
                    entries.insert(
                        span.start_line.saturating_sub(1),
                        (Some(name.clone()), composition.scene_start_times.get(name).copied()),
                    );
                }
            }
        }
        for marker in markers {
            if let Some(line) = marker.line {
                entries.insert(line, (marker.scene, marker.time_s));
            }
        }
        let named_cues = named_cue_markers(
            doc.raw_statements.as_deref().unwrap_or_default(),
            doc.timeline.as_ref(),
            doc.composition.as_ref(),
            &doc.source_text,
        );
        Self {
            entries,
            named_cues,
        }
    }

    pub(crate) fn named_cues(&self) -> &[TimingMarker] {
        &self.named_cues
    }

    pub(crate) fn at_header(&self, line: usize) -> Option<f64> {
        self.entries.get(&line)?.1
    }

    pub(crate) fn at_line(&self, line: usize) -> Option<f64> {
        self.entries.range(..=line).next_back()?.1.1
    }

    pub(crate) fn playing_line(&self, time: f64, scene: Option<&str>) -> Option<usize> {
        self.entries
            .iter()
            .filter(|(_, (scope, _))| scope.as_deref() == scene)
            .filter_map(|(&line, (_, t))| t.filter(|t| *t <= time).map(|t| (line, t)))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(line, _)| line)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TimingMarker {
    pub expression: String,
    pub line: Option<usize>,
    pub scene: Option<String>,
    pub time_s: Option<f64>,
    pub action_count: usize,
    pub named_cue: bool,
}

pub(crate) fn collect_markers(
    statements: &[Stmt],
    timeline: Option<&Timeline>,
    composition: Option<&Composition>,
) -> Vec<TimingMarker> {
    let mut result = Vec::new();
    collect_scope(statements, timeline, composition, None, Some(0.0), &mut result);
    result
}

pub(crate) fn review_markers(
    statements: &[Stmt],
    timeline: Option<&Timeline>,
    composition: Option<&Composition>,
    diagnostics: &[animatix_syntax::diagnostics::Diagnostic],
) -> Vec<TimingMarker> {
    use animatix_syntax::diagnostics::DiagnosticCode;
    let mut markers = collect_markers(statements, timeline, composition);
    for problem in diagnostics
        .iter()
        .filter(|d| d.code == DiagnosticCode::NarrationBindingUnresolved)
    {
        if let Some(line) = problem.location.line {
            for marker in markers.iter_mut().filter(|m| m.line == line.checked_sub(1)) {
                marker.time_s = None;
            }
        }
    }
    markers
}

/// Use the same compiled named anchors and scene offsets as the Web SDK.
fn named_cue_markers(
    statements: &[Stmt],
    timeline: Option<&Timeline>,
    composition: Option<&Composition>,
    source: &str,
) -> Vec<TimingMarker> {
    let mut result = Vec::new();
    if let Some(composition) = composition {
        for (name, start) in &composition.scene_start_times {
            let Some(scene) = composition.scenes.get(name) else {
                continue;
            };
            let local_source = statements.iter().find_map(|stmt| match stmt {
                Stmt::Scene {
                    name: local, body, ..
                } if local == name => Some(body.as_slice()),
                _ => None,
            });
            append_cue_markers(
                &scene.timeline,
                Some(name),
                *start,
                local_source,
                source,
                &mut result,
            );
        }
    } else if let Some(timeline) = timeline {
        append_cue_markers(timeline, None, 0.0, Some(statements), source, &mut result);
    }
    result.sort_by(|a, b| {
        a.time_s
            .unwrap_or_default()
            .total_cmp(&b.time_s.unwrap_or_default())
            .then_with(|| a.scene.cmp(&b.scene))
            .then_with(|| a.expression.cmp(&b.expression))
    });
    result
}

fn append_cue_markers(
    timeline: &Timeline,
    scene: Option<&str>,
    start: f64,
    local_statements: Option<&[Stmt]>,
    source: &str,
    out: &mut Vec<TimingMarker>,
) {
    let mut lines = BTreeMap::new();
    if let Some(statements) = local_statements
        && timeline.named_cues().next().is_some()
    {
        let source_lines: Vec<_> = source.lines().collect();
        for statement in statements {
            let (group, span) = match statement {
                Stmt::Keyframe {
                    cue_group: Some(group),
                    span: Some(span),
                    ..
                }
                | Stmt::RelativeKeyframe {
                    cue_group: Some(group),
                    span: Some(span),
                    ..
                } => (group, span),
                _ => continue,
            };
            for name in &group.names {
                if timeline.named_cue_span(name) != Some(*span) {
                    continue;
                }
                // The span covers the header group and its body. Locate the
                // actual #cue line in the current source, not in an import.
                let Some(header) =
                    source_lines.get(span.start_line.saturating_sub(1)..span.end_line)
                else {
                    continue;
                };
                let line = header.iter().position(|line| {
                    use animatix_syntax::token::{TokenKind, tokenize};
                    let tokens = tokenize(line);
                    matches!(tokens.as_slice(), [hash, cue, colon, id, ..]
                        if hash.kind == TokenKind::Hash && colon.kind == TokenKind::Colon
                        && matches!(&cue.kind, TokenKind::Ident(keyword) if keyword == "cue")
                        && matches!(&id.kind, TokenKind::Ident(value) if value == name))
                });
                if let Some(line) = line {
                    lines.insert(name.as_str(), span.start_line.saturating_sub(1) + line);
                }
            }
        }
    }
    out.extend(timeline.named_cues().map(|(name, time)| TimingMarker {
        expression: format!("#cue:{name}"),
        line: lines.get(name).copied(),
        scene: scene.map(str::to_owned),
        time_s: Some(animatix::time::add_seconds(start, time)),
        action_count: 0,
        named_cue: true,
    }));
}

fn collect_scope(
    statements: &[Stmt],
    timeline: Option<&Timeline>,
    composition: Option<&Composition>,
    scene: Option<&str>,
    global_offset: Option<f64>,
    out: &mut Vec<TimingMarker>,
) {
    let mut previous = Some(0.0);
    for statement in statements {
        match statement {
            Stmt::Scene { name, body, .. } => {
                let timeline = composition.and_then(|c| c.scenes.get(name)).map(|s| &s.timeline);
                let offset = composition.and_then(|c| c.scene_start_times.get(name)).copied();
                collect_scope(body, timeline, composition, Some(name), offset, out);
            },
            statement if statement.is_cue_only_keyframe() => {},
            Stmt::Keyframe {
                time, body, span, ..
            }
            | Stmt::RelativeKeyframe {
                offset: time,
                body,
                span,
                ..
            } => {
                if let Time::SpeechMarker { marker, .. } = time
                    && animatix_syntax::narration::is_implicit_anchor(marker)
                    && body.is_empty()
                {
                    continue;
                }
                let relative = matches!(statement, Stmt::RelativeKeyframe { .. });
                let compiled =
                    span.and_then(|span| timeline?.marker_time(span)).map(|ms| ms / 1000.0);
                let value = resolve_time(time);
                let local = compiled.or_else(|| {
                    if relative {
                        previous.zip(value).map(|(a, b)| animatix::time::add_seconds(a, b))
                    } else {
                        value
                    }
                });
                previous = local;
                out.push(TimingMarker {
                    expression: format!("#{}{}", if relative { "+" } else { "" }, time.to_source()),
                    line: span.map(|span| span.start_line.saturating_sub(1)),
                    scene: scene.map(str::to_owned),
                    time_s: global_offset
                        .zip(local)
                        .map(|(a, b)| animatix::time::add_seconds(a, b)),
                    action_count: body.len(),
                    named_cue: false,
                });
            },
            // These structural forms can change the engine's current build time.
            // Do not invent the anchor of a subsequent relative marker.
            Stmt::Conditional { .. } | Stmt::Match { .. } | Stmt::ForLoop { .. } => previous = None,
            _ => {}, // Generated component/loop timing is not uniquely source-addressable.
        }
    }
}

fn resolve_time(time: &Time) -> Option<f64> {
    let value = match time {
        Time::Seconds(s) => *s,
        Time::Milliseconds(ms) => *ms as f64 / 1000.0,
        // Tempo may be expression-driven. Do not silently assume the default BPM.
        Time::Beats(_) | Time::Expression(_) => return None,
        // Expressions execute in their lexical build environment. Re-evaluating
        // them against the final environment could place a marker incorrectly.
        // The compiler owns clip offsets, lexical scope and paragraph alignment.
        // With no compiled span we can still offer source navigation.
        Time::SpeechMarker { .. } | Time::SpeechWord { .. } => return None,
    };
    value.is_finite().then_some(value)
}

/// Validate the entire direct-header sequence against the current source.
/// Parsed spans can belong to a last-good AST, and implicit #0 keyframes or
/// other mismatches must never steal a later authored header's location.
pub(crate) fn locate(source: &str, markers: &mut [TimingMarker]) {
    use animatix_syntax::ast::Span;
    use animatix_syntax::token::{TokenKind, tokenize};
    for marker in markers.iter_mut() {
        marker.line = None;
    }
    let paragraphs = animatix_syntax::narration::paragraphs(source);
    let mut authored = Vec::new();
    for marker in markers {
        if let Some((_, name)) = marker.expression.split_once('@')
            && animatix_syntax::narration::is_implicit_anchor(name)
        {
            let key =
                animatix_syntax::narration::decode_anchor(name).expect("validated implicit marker");
            if let Some(paragraph) =
                paragraphs.iter().find(|p| p.scene == marker.scene && key.matches_paragraph(p))
            {
                marker.line =
                    Some(source[..paragraph.span.start].bytes().filter(|b| *b == b'\n').count());
            }
        } else {
            authored.push(marker);
        }
    }
    let tokens: Vec<_> = tokenize(source)
        .into_iter()
        .filter(|token| !matches!(token.kind, TokenKind::Comment(_)))
        .collect();
    let expected: Vec<Vec<_>> = authored
        .iter()
        .map(|marker| tokenize(&marker.expression).into_iter().map(|token| token.kind).collect())
        .collect();
    let mut found = Vec::new();
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::LBrace => depth += 1,
            TokenKind::RBrace => depth = depth.saturating_sub(1),
            TokenKind::Hash if depth == 0 => {
                let candidate = expected
                    .iter()
                    .filter(|pattern| {
                        tokens.get(index..index + pattern.len()).is_some_and(|actual| {
                            actual.iter().zip(pattern.iter()).all(|(a, b)| &a.kind == b)
                        })
                    })
                    .max_by_key(|pattern| pattern.len());
                if let Some(pattern) = candidate {
                    found.push((pattern, Span::from_byte_span(source, token.span).start_line - 1));
                }
            },
            _ => {},
        }
    }
    if found.len() == expected.len()
        && found.iter().zip(&expected).all(|((actual, _), expected)| *actual == expected)
    {
        for (marker, (_, line)) in authored.into_iter().zip(found) {
            marker.line = Some(line);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn named_cues_resolve_and_keep_their_distinct_source_lines_without_adding_keyframes() {
        let text = "config { duration: 6 }
#1s
a: Rect
#+500ms
#cue:memory
#cue:alias
#2s
#cue:memory
#8s
#cue:out_of_range
#+1s
a.opacity = 1";
        let mut store = crate::app::test_support::make_document_store(text);
        let timing = &store.source.document.source_timing;
        let rows: Vec<_> = timing
            .named_cues()
            .iter()
            .map(|m| (m.expression.as_str(), m.time_s, m.line))
            .collect();
        assert_eq!(
            rows,
            [
                ("#cue:alias", Some(1.5), Some(5)),
                ("#cue:memory", Some(2.0), Some(7)),
            ]
        );
        assert_eq!(timing.at_header(5), None);
        assert_eq!(timing.at_header(10), Some(2.0));
        store.replace_text(format!(
            "// draft
{text}"
        ));
        assert!(store.source.document.source_timing.named_cues().is_empty());
    }

    #[test]
    fn named_cues_in_compositions_are_scene_scoped_and_seek_global_time() {
        let text = "# First
config { duration: 3 }
#1s
#cue:start
play Second
# Second
config { duration: 2 }
#500ms
#cue:start";
        let store = crate::app::test_support::make_document_store(text);
        let markers = store.source.document.source_timing.named_cues();
        let rows: Vec<_> = markers
            .iter()
            .map(|m| (m.scene.as_deref(), m.expression.as_str(), m.time_s, m.line))
            .collect();
        assert_eq!(
            rows,
            [
                (Some("First"), "#cue:start", Some(1.0), Some(3)),
                (Some("Second"), "#cue:start", Some(3.5), Some(8)),
            ]
        );
    }

    #[test]
    fn combined_fixture_resolves_authored_relative_headers_across_scenes() {
        let text = include_str!("../../editor/fixtures/cell-highlighting.amx");
        let store = crate::app::test_support::make_document_store(text);
        let timing = &store.source.document.source_timing;
        assert_eq!(timing.at_header(3), None, "a raw string contains #99s");
        assert_eq!(timing.at_header(10), Some(1.0));
        assert_eq!(timing.at_header(13), Some(2.0));
        assert_eq!(timing.at_header(22), Some(5.0));
    }
    #[test]
    fn repeated_local_times_seek_each_scene_and_expire_on_edit() {
        let text = "# Intro
config { duration: 4 }
#0s
a: Rect
#+1s
a.opacity = 1
# Outro
config { duration: 3 }
#0s
b: Rect
#+1s
b.opacity = 1";
        let mut store = crate::app::test_support::make_document_store(text);
        let doc = &store.source.document;
        assert_eq!(doc.source_timing.at_header(2), Some(0.0));
        assert_eq!(doc.source_timing.at_header(8), Some(4.0));
        assert_eq!(doc.source_timing.at_line(11), Some(5.0));
        assert_eq!(doc.find_keyframe_line_at(5.1), Some(10));
        store.replace_text(format!("// draft\n{text}"));
        assert_eq!(store.source.document.source_timing.at_header(8), None);
        store.replace_text(text.to_owned());
        store.source.document.rebuild().expect("restore original build");
        assert_eq!(store.source.document.source_timing.at_header(8), Some(4.0));
    }
}
