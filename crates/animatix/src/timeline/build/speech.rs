//! Speech transcript pre-pass and speech-anchored keyframe lowering.
//!
//! Scans the AST for audio/captions declarations to ingest transcripts,
//! and lowers `#track@marker` and `#track["word", occ]` keyframe times
//! statically to `Time::Milliseconds(ms)` before main build processing.

use std::collections::HashMap;

use animatix_text::transcript::SpeechTranscript;

use crate::ast::{Expr, Stmt, Time};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::timeline::assets::AssetCache;

/// Scan AST for declarations with `transcript:` property and load them.
pub fn collect_transcripts_from_ast(
    ast: &[Stmt],
    asset_cache: &AssetCache,
    transcripts: &mut HashMap<String, SpeechTranscript>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for stmt in ast {
        match stmt {
            Stmt::ActorDecl {
                label, ty, props, ..
            } => {
                if ty == "Audio" || ty == "Captions" {
                    for prop in props {
                        if prop.name == "transcript" {
                            if let Expr::Str(ref path) = prop.value {
                                let normalized = asset_cache.normalize_asset_url(path);
                                match std::fs::read_to_string(&normalized) {
                                    Ok(content) => {
                                        match animatix_text::transcript::parse_transcript_auto(
                                            &content,
                                        ) {
                                            Ok(parsed) => {
                                                transcripts.insert(label.clone(), parsed);
                                            },
                                            Err(e) => {
                                                diagnostics.push(
                                                    Diagnostic::warning(
                                                        DiagnosticCode::InvalidConfigValue,
                                                        DiagnosticPhase::Build,
                                                        format!(
                                                            "Failed to parse speech transcript for '{label}': {e}"
                                                        ),
                                                    )
                                                    .with_subject(label),
                                                );
                                            },
                                        }
                                    },
                                    Err(e) => {
                                        diagnostics.push(
                                            Diagnostic::warning(
                                                DiagnosticCode::InvalidConfigValue,
                                                DiagnosticPhase::Build,
                                                format!(
                                                    "Failed to read transcript file '{path}' for '{label}': {e}"
                                                ),
                                            )
                                            .with_subject(label),
                                        );
                                    },
                                }
                            }
                        }
                    }
                }
            },
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. } => {
                collect_transcripts_from_ast(body, asset_cache, transcripts, diagnostics);
            },
            _ => {},
        }
    }
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
