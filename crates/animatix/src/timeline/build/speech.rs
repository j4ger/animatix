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
        fn visit_expr(&mut self, expr: &'ast crate::ast::Expr) {
            match expr {
                crate::ast::Expr::Timeline(plan)
                | crate::ast::Expr::Sequence(plan)
                | crate::ast::Expr::Stagger { plan, .. } => {
                    for statement in &plan.statements {
                        self.visit_stmt(statement);
                    }
                },
                _ => animatix_syntax::visit::walk_expr(self, expr),
            }
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
                | InlineItem::SlotMarker
                | InlineItem::Expr(..) => {
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
/// A transcript's timestamps are relative to an Audio clip, or to the scene
/// when a standalone transcript is used.
pub(super) struct SpeechTrack {
    transcript: SpeechTranscript,
    start_ms: Option<f64>,
}

struct SpeechScan<'a> {
    assets: &'a mut AssetCache,
    tracks: &'a mut HashMap<String, SpeechTrack>,
    overridden: &'a std::collections::HashSet<String>,
    beat_ms: f64,
}

pub(super) fn collect_transcripts_from_ast(
    ast: &[Stmt],
    assets: &mut AssetCache,
    tracks: &mut HashMap<String, SpeechTrack>,
    env: &mut crate::timeline::Environment,
    overridden: &std::collections::HashSet<String>,
    beat_ms: f64,
) {
    SpeechScan {
        assets,
        tracks,
        overridden,
        beat_ms,
    }
    .walk(ast, env, Some(0.0));
}

impl SpeechScan<'_> {
    fn walk(
        &mut self,
        ast: &[Stmt],
        env: &mut crate::timeline::Environment,
        mut time_ms: Option<f64>,
    ) -> Option<f64> {
        for stmt in ast {
            match stmt {
                Stmt::LetDecl { name, value, .. } if !self.overridden.contains(name) => {
                    match crate::timeline::evaluate_expr(value, env) {
                        Ok(value) => env.set(name, value),
                        Err(error) => tracing::debug!(
                            "transcript pre-scan could not evaluate let '{name}': {error}"
                        ),
                    }
                },
                Stmt::ActorDecl {
                    label,
                    ty,
                    props,
                    modifiers,
                    ..
                } if has_transcript_property(ty, props) => {
                    let start_ms = if ty == "Audio" {
                        let timing = crate::timeline::timing::parse_timing_modifiers(
                            modifiers,
                            crate::timeline::timing::ModifierHost::ActorDeclaration,
                            Some(label),
                            &mut Vec::new(),
                        );
                        time_ms.map(|time| time + timing.delay_ms)
                    } else {
                        Some(0.0)
                    };
                    self.load(label, props, env, start_ms);
                },
                Stmt::Keyframe { time, body, .. } => {
                    time_ms = self.timestamp(time, env);
                    self.walk(body, env, time_ms);
                },
                Stmt::RelativeKeyframe { offset, body, .. } => {
                    time_ms = time_ms.zip(self.timestamp(offset, env)).map(|(t, step)| t + step);
                    self.walk(body, env, time_ms);
                },
                Stmt::Sequence { body, .. } | Stmt::Stagger { body, .. } => {
                    // The mounted plan determines the final placement of its actors.
                    self.walk(body, env, None);
                },
                Stmt::Always { body, .. } | Stmt::Scene { body, .. } => {
                    self.walk(body, env, time_ms);
                },
                Stmt::Block { body, .. } => {
                    self.walk(body, &mut env.clone(), time_ms);
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
                        time_ms = self.walk(branch, env, time_ms);
                    }
                },
                _ => {}, // No transcript-bearing declarations to inspect here.
            }
        }
        time_ms
    }

    fn timestamp(&self, time: &Time, env: &crate::timeline::Environment) -> Option<f64> {
        match time {
            Time::Expression(expr) => match crate::timeline::evaluate_expr(expr, env).ok()? {
                crate::timeline::Value::Duration(seconds) => Some(seconds * 1000.0),
                _ => None,
            },
            Time::SpeechMarker { .. } | Time::SpeechWord { .. } => {
                let mut errors = Vec::new();
                let Time::Milliseconds(ms) = lower_time(time, self.tracks, &mut errors) else {
                    return None;
                };
                errors.is_empty().then_some(ms as f64)
            },
            _ => crate::timeline::time_to_ms_at_beat(time, self.beat_ms).ok(),
        }
    }

    fn load(
        &mut self,
        label: &str,
        props: &[Property],
        env: &crate::timeline::Environment,
        start_ms: Option<f64>,
    ) {
        for prop in props.iter().filter(|prop| prop.name == "transcript") {
            match crate::timeline::evaluate_expr(&prop.value, env) {
                Ok(crate::timeline::Value::Str(path)) if !path.trim().is_empty() => {
                    if let Some(transcript) =
                        load_transcript_path(label, &path, self.assets, &mut Vec::new())
                    {
                        self.tracks.insert(
                            label.to_string(),
                            SpeechTrack {
                                transcript,
                                start_ms,
                            },
                        );
                    }
                },
                Ok(value) => tracing::debug!(
                    "transcript pre-scan skipped non-string path for '{label}': {value:?}"
                ),
                Err(error) => tracing::debug!(
                    "transcript pre-scan could not evaluate path for '{label}': {error}"
                ),
            }
        }
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
pub(super) fn lower_speech_keyframes(
    ast: &[Stmt],
    transcripts: &HashMap<String, SpeechTrack>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<Stmt> {
    ast.iter().map(|stmt| lower_stmt(stmt, transcripts, diagnostics)).collect()
}

fn lower_time(
    time: &Time,
    transcripts: &HashMap<String, SpeechTrack>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Time {
    let (label, description, offset_ms, position) = match time {
        Time::SpeechMarker {
            track,
            marker,
            offset_ms,
        } => (
            track,
            format!("@{marker}"),
            *offset_ms,
            transcripts.get(track).and_then(|track| track.transcript.find_marker(marker)),
        ),
        Time::SpeechWord {
            track,
            word,
            occurrence,
            offset_ms,
        } => (
            track,
            format!("[{word:?}, {occurrence}]"),
            *offset_ms,
            transcripts.get(track).and_then(|track| {
                track.transcript.find_word(word, *occurrence).map(|(start, _)| start)
            }),
        ),
        other => return other.clone(),
    };

    let error = match (transcripts.get(label), position) {
        (
            Some(SpeechTrack {
                start_ms: Some(start),
                ..
            }),
            Some(position),
        ) => {
            let total_ms = (start + position * 1000.0 + offset_ms as f64).round();
            return Time::Milliseconds(total_ms.max(0.0) as u64);
        },
        (Some(SpeechTrack { start_ms: None, .. }), Some(_)) => format!(
            "Start time of audio track '{label}' cannot be resolved before its speech anchor '{description}'. Declare the audio at a fixed scene time."
        ),
        (Some(_), None) => {
            format!("Speech anchor '{description}' not found in transcript for track '{label}'")
        },
        (None, _) => format!(
            "Audio track '{label}' with transcript not found for speech anchor '{description}'"
        ),
    };
    diagnostics.push(
        Diagnostic::error(DiagnosticCode::InvalidConfigValue, DiagnosticPhase::Build, error)
            .with_subject(label),
    );
    Time::Milliseconds(0)
}

fn lower_stmt(
    stmt: &Stmt,
    transcripts: &HashMap<String, SpeechTrack>,
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
