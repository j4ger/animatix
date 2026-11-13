//! Speech transcript pre-pass and speech-anchored keyframe lowering.
//!
//! Scans the AST for audio/captions declarations to ingest transcripts,
//! and lowers `#track@marker` and `#track["word", occ]` keyframe times
//! statically to `Time::Milliseconds(ms)` before main build processing.

use std::collections::HashMap;

use animatix_text::transcript::SpeechTranscript;

use crate::ast::{InlineItem, Property, Stmt, Time};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::timeline::assets::AssetCache;

/// Whether this AST contains both a transcript declaration and a speech-anchored
/// keyframe. Only that combination needs transcript I/O before the actor build.
pub(super) fn needs_speech_transcript_prepass(ast: &[Stmt]) -> bool {
    struct Finder {
        has_declaration: bool,
        has_anchor: bool,
    }

    impl Finder {
        fn done(&self) -> bool {
            self.has_declaration && self.has_anchor
        }
    }

    impl<'ast> animatix_syntax::visit::AstVisitor<'ast> for Finder {
        fn visit_expr(&mut self, _expr: &'ast crate::ast::Expr) {
            // Expressions cannot declare an actor or a speech-anchored keyframe;
            // skip large plot-data/closure ASTs on the ordinary build path.
        }

        fn visit_stmt(&mut self, stmt: &'ast Stmt) {
            match stmt {
                Stmt::Keyframe { time, .. } => self.has_anchor |= is_speech_anchor(time),
                Stmt::RelativeKeyframe { offset, .. } => {
                    self.has_anchor |= is_speech_anchor(offset);
                },
                _ => {},
            }
            if let Stmt::ActorDecl { ty, props, .. } = stmt {
                self.has_declaration |= has_transcript_property(ty, props);
            }
            if !self.done() {
                animatix_syntax::visit::walk_stmt(self, stmt);
            }
        }

        fn visit_inline_item(&mut self, item: &'ast InlineItem) {
            let (ty, props) = match item {
                InlineItem::Labeled { ty, props, .. } | InlineItem::Anonymous { ty, props, .. } => {
                    (ty, props)
                },
                InlineItem::ForLoop { .. }
                | InlineItem::SlotFill { .. }
                | InlineItem::SlotMarker => {
                    animatix_syntax::visit::walk_inline_item(self, item);
                    return;
                },
            };
            self.has_declaration |= has_transcript_property(ty, props);
            if !self.done() {
                animatix_syntax::visit::walk_inline_item(self, item);
            }
        }
    }

    let mut finder = Finder {
        has_declaration: false,
        has_anchor: false,
    };
    for statement in ast {
        if finder.done() {
            break;
        }
        animatix_syntax::visit::AstVisitor::visit_stmt(&mut finder, statement);
    }
    finder.done()
}

fn is_speech_anchor(time: &Time) -> bool {
    matches!(time, Time::SpeechMarker { .. } | Time::SpeechWord { .. })
}

fn has_transcript_property(ty: &str, props: &[Property]) -> bool {
    animatix_std::caps_for_type(ty).is_some_and(|caps| caps.transcript)
        && props.iter().any(|prop| prop.name == "transcript")
}
/// Scan declarations for transcript files and use the same expression environment as the build.
pub fn collect_transcripts_from_ast(
    ast: &[Stmt],
    asset_cache: &mut AssetCache,
    transcripts: &mut HashMap<String, SpeechTranscript>,
    _diagnostics: &mut Vec<Diagnostic>,
    env: &mut crate::timeline::Environment,
    overridden: &std::collections::HashSet<String>,
) {
    for stmt in ast {
        match stmt {
            Stmt::LetDecl { name, value, .. } if !overridden.contains(name) => {
                match crate::timeline::evaluate_expr(value, env) {
                    Ok(value) => env.set(name, value),
                    Err(error) => tracing::debug!(
                        "transcript pre-scan could not evaluate let '{name}': {error}"
                    ),
                }
            },
            Stmt::ActorDecl {
                label, ty, props, ..
            } if animatix_std::caps_for_type(ty).is_some_and(|caps| caps.transcript) => {
                for prop in props.iter().filter(|prop| prop.name == "transcript") {
                    match crate::timeline::evaluate_expr(&prop.value, env) {
                        Ok(crate::timeline::Value::Str(path)) if !path.trim().is_empty() => {
                            load_transcript(label, &path, asset_cache, transcripts);
                        },
                        Ok(value) => tracing::debug!(
                            "transcript pre-scan skipped non-string path for '{label}': {value:?}"
                        ),
                        Err(error) => tracing::debug!(
                            "transcript pre-scan could not evaluate path for '{label}': {error}"
                        ),
                    }
                }
            },
            Stmt::ActorDecl { .. } => {},
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::Scene { body, .. } => collect_transcripts_from_ast(
                body,
                asset_cache,
                transcripts,
                _diagnostics,
                env,
                overridden,
            ),
            Stmt::Block { body, .. } => {
                let saved = env.clone();
                collect_transcripts_from_ast(
                    body,
                    asset_cache,
                    transcripts,
                    _diagnostics,
                    env,
                    overridden,
                );
                *env = saved;
            },
            Stmt::Conditional {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let branch = match crate::timeline::evaluate_expr(condition, env) {
                    Ok(value) if value.is_truthy() => Some(then_branch.as_slice()),
                    Ok(_) => else_branch.as_deref(),
                    Err(error) => {
                        tracing::debug!(
                            "transcript pre-scan could not evaluate conditional: {error}"
                        );
                        None
                    },
                };
                if let Some(branch) = branch {
                    collect_transcripts_from_ast(
                        branch,
                        asset_cache,
                        transcripts,
                        _diagnostics,
                        env,
                        overridden,
                    );
                }
            },
            _ => {}, // Other statements cannot introduce transcript-bearing actors here.
        }
    }
}

fn load_transcript(
    label: &str,
    path: &str,
    asset_cache: &mut AssetCache,
    transcripts: &mut HashMap<String, SpeechTranscript>,
) {
    let mut ignored_diagnostics = Vec::new();
    if let Some(parsed) = load_transcript_path(label, path, asset_cache, &mut ignored_diagnostics) {
        transcripts.insert(label.to_string(), parsed);
    }
}

pub(crate) fn load_transcript_path(
    label: &str,
    path: &str,
    asset_cache: &mut AssetCache,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<SpeechTranscript> {
    // The cache loader owns canonicalization of the caller's raw resource path.
    // Keep the normalized identity for usage tracking, but do not pass it back
    // to the loader or a relative document base would be applied twice.
    let normalized = asset_cache.normalize_asset_url(path);
    match asset_cache.load_transcript_text(path) {
        Ok(content) => match animatix_text::transcript::parse_transcript_auto(&content) {
            Ok(parsed) => {
                asset_cache.record_usage(&normalized, label);
                Some(parsed)
            },
            Err(error) => {
                diagnostics.push(transcript_warning(
                    label,
                    format!("failed to parse transcript: {error}"),
                ));
                None
            },
        },
        Err(error) => {
            diagnostics.push(transcript_warning(label, error));
            None
        },
    }
}

pub(super) fn attach_transcript_from_props(
    timeline: &mut crate::timeline::Timeline,
    label: &str,
    props: &[Property],
    env: &crate::timeline::Environment,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for prop in props.iter().filter(|prop| prop.name == "transcript") {
        let subject = format!("{label}.transcript");
        let Some(value) = crate::timeline::evaluate_expr_with_lookup_diagnostic(
            &prop.value,
            env,
            diagnostics,
            &subject,
        ) else {
            continue;
        };
        let crate::timeline::Value::Str(path) = value else {
            diagnostics.push(transcript_warning(
                label,
                format!("transcript path must be a string, got {value:?}"),
            ));
            continue;
        };
        if path.trim().is_empty() {
            continue;
        }
        let parsed = load_transcript_path(
            label,
            &path,
            std::sync::Arc::make_mut(&mut timeline.asset_cache),
            diagnostics,
        );
        if let Some(parsed) = parsed
            && !timeline.set_transcript(label, parsed)
        {
            tracing::warn!("parsed transcript for '{label}' was not attached: actor is missing");
        }
    }
}

fn transcript_warning(label: &str, message: String) -> Diagnostic {
    Diagnostic::warning(
        DiagnosticCode::InvalidConfigValue,
        DiagnosticPhase::Build,
        format!("Speech transcript for '{label}': {message}"),
    )
    .with_subject(label)
}

/// Lower any speech-anchored `Time` variants in the AST to `Time::Milliseconds(u64)`.
pub fn lower_speech_keyframes(
    ast: &[Stmt],
    transcripts: &HashMap<String, SpeechTranscript>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<Stmt> {
    ast.iter().map(|stmt| lower_stmt(stmt, transcripts, diagnostics)).collect()
}

fn lower_time(
    time: &Time,
    transcripts: &HashMap<String, SpeechTranscript>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Time {
    match time {
        Time::SpeechMarker {
            track,
            marker,
            offset_ms,
        } => {
            if let Some(transcript) = transcripts.get(track) {
                if let Some(pos_s) = transcript.find_marker(marker) {
                    let pos_ms = (pos_s * 1000.0).round() as i64;
                    let total_ms = (pos_ms + offset_ms).max(0) as u64;
                    Time::Milliseconds(total_ms)
                } else {
                    diagnostics.push(
                        Diagnostic::error(
                            DiagnosticCode::InvalidConfigValue,
                            DiagnosticPhase::Build,
                            format!(
                                "Speech marker '@{marker}' not found in transcript for track '{track}'"
                            ),
                        )
                        .with_subject(track),
                    );
                    Time::Milliseconds(0)
                }
            } else {
                diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode::InvalidConfigValue,
                        DiagnosticPhase::Build,
                        format!(
                            "Audio track '{track}' with transcript not found for speech marker '@{marker}'"
                        ),
                    )
                    .with_subject(track),
                );
                Time::Milliseconds(0)
            }
        },
        Time::SpeechWord {
            track,
            word,
            occurrence,
            offset_ms,
        } => {
            if let Some(transcript) = transcripts.get(track) {
                if let Some((start_s, _)) = transcript.find_word(word, *occurrence) {
                    let pos_ms = (start_s * 1000.0).round() as i64;
                    let total_ms = (pos_ms + offset_ms).max(0) as u64;
                    Time::Milliseconds(total_ms)
                } else {
                    diagnostics.push(
                        Diagnostic::error(
                            DiagnosticCode::InvalidConfigValue,
                            DiagnosticPhase::Build,
                            format!(
                                "Speech word '[\"{word}\", {occurrence}]' not found in transcript for track '{track}'"
                            ),
                        )
                        .with_subject(track),
                    );
                    Time::Milliseconds(0)
                }
            } else {
                diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode::InvalidConfigValue,
                        DiagnosticPhase::Build,
                        format!(
                            "Audio track '{track}' with transcript not found for speech word '[\"{word}\"]'"
                        ),
                    )
                    .with_subject(track),
                );
                Time::Milliseconds(0)
            }
        },
        other => other.clone(),
    }
}

fn lower_stmt(
    stmt: &Stmt,
    transcripts: &HashMap<String, SpeechTranscript>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Stmt {
    match stmt {
        Stmt::Keyframe { time, body, span } => Stmt::Keyframe {
            time: lower_time(time, transcripts, diagnostics),
            body: lower_speech_keyframes(body, transcripts, diagnostics),
            span: *span,
        },
        Stmt::RelativeKeyframe { offset, body, span } => Stmt::RelativeKeyframe {
            offset: lower_time(offset, transcripts, diagnostics),
            body: lower_speech_keyframes(body, transcripts, diagnostics),
            span: *span,
        },
        Stmt::Sequence { body, span } => Stmt::Sequence {
            body: lower_speech_keyframes(body, transcripts, diagnostics),
            span: *span,
        },
        Stmt::Stagger {
            modifiers,
            body,
            span,
        } => Stmt::Stagger {
            modifiers: modifiers.clone(),
            body: lower_speech_keyframes(body, transcripts, diagnostics),
            span: *span,
        },
        other => other.clone(),
    }
}
