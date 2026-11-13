//! Walk the statements and lower frame-time `always` bodies.

use super::*;

/// Build-walk cursor shared by nested conditional branches, so a keyframe in a
/// branch advances the same build clock a top-level keyframe would.
struct WalkState {
    has_seen_keyframe: bool,
    current_build_time_ms: f64,
}

/// Walk the (speech-lowered) AST, lowering actor declarations, keyframes and
/// actions into the timeline.
///
/// Actors declared before the first keyframe are hidden by default until an
/// entrance reveals them; the saved/restored `default_opacity` is what encodes
/// that. Transcript data is attached during each transcript-capable actor's
/// actual declaration build.
pub(super) fn walk_statements(
    timeline: &mut Timeline,
    ast: &[Stmt],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut state = WalkState {
        has_seen_keyframe: false,
        current_build_time_ms: 0.0,
    };
    walk_statements_inner(timeline, ast, &mut state, diagnostics);

    // Names written by always blocks — a plot closure capturing one of these
    // must resample per frame (see ProceduralPlot::is_dynamic).
    timeline.frame_written_vars = Timeline::collect_frame_written_vars(&timeline.modifiers);
}

/// The statement walk proper. `if`/`match` blocks are build-time structural
/// forms: evaluate the condition here, then walk only the branch that is taken
/// so its declarations, keyframes and loop expansions are lowered exactly like
/// top-level statements.
fn walk_statements_inner(
    timeline: &mut Timeline,
    ast: &[Stmt],
    state: &mut WalkState,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for stmt in ast {
        match stmt {
            Stmt::Config { .. } => {},
            Stmt::Keyframe { time, body, .. } => {
                state.has_seen_keyframe = true;
                timeline.default_opacity = 1.0;
                state.current_build_time_ms = time_to_ms_at_beat(time, timeline.beat_duration_ms());
                timeline.process_body(state.current_build_time_ms, body, None, diagnostics);
            },
            Stmt::RelativeKeyframe { offset, body, .. } => {
                state.has_seen_keyframe = true;
                timeline.default_opacity = 1.0;
                state.current_build_time_ms +=
                    time_to_ms_at_beat(offset, timeline.beat_duration_ms());
                timeline.process_body(state.current_build_time_ms, body, None, diagnostics);
            },
            Stmt::Conditional {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let eval_env = timeline.build_eval_env(state.current_build_time_ms as u64);
                let branch = match evaluate_expr(condition, &eval_env) {
                    Ok(value) if value.is_truthy() => Some(then_branch.as_slice()),
                    Ok(_) => else_branch.as_deref(),
                    Err(e) => {
                        diagnostics.push(Diagnostic::error(
                            DiagnosticCode::InvalidPropertyValue,
                            DiagnosticPhase::Build,
                            format!("Failed to evaluate `if` condition: {e}; skipping the block."),
                        ));
                        None
                    },
                };
                if let Some(branch) = branch {
                    walk_statements_inner(timeline, branch, state, diagnostics);
                }
            },
            Stmt::Match {
                scrutinee, arms, ..
            } => {
                let eval_env = timeline.build_eval_env(state.current_build_time_ms as u64);
                let value = match evaluate_expr(scrutinee, &eval_env) {
                    Ok(value) => value,
                    Err(e) => {
                        diagnostics.push(Diagnostic::error(
                            DiagnosticCode::ModuleExportEvalError,
                            DiagnosticPhase::Build,
                            format!("Failed to evaluate match scrutinee: {e}; skipping match."),
                        ));
                        continue;
                    },
                };
                let mut matched = false;
                for (pattern, body) in arms {
                    if pattern_matches(pattern, &value) {
                        walk_statements_inner(timeline, body, state, diagnostics);
                        matched = true;
                        break;
                    }
                }
                if !matched
                    && !arms
                        .iter()
                        .any(|(pattern, _)| matches!(pattern, crate::ast::MatchPattern::Wildcard))
                {
                    diagnostics.push(Diagnostic::warning(
                        DiagnosticCode::InvalidPropertyValue,
                        DiagnosticPhase::Build,
                        format!(
                            "match scrutinee evaluated to {value:?} but no arm matched and no `_` wildcard arm was provided"
                        ),
                    ));
                }
            },
            Stmt::ActorDecl { .. }
            | Stmt::Assignment { .. }
            | Stmt::Sequence { .. }
            | Stmt::Stagger { .. }
            | Stmt::LetDecl { .. }
            | Stmt::Always { .. }
            | Stmt::ForLoop { .. }
            | Stmt::Step { .. } => {
                let saved_opacity = timeline.default_opacity;
                if !state.has_seen_keyframe {
                    timeline.default_opacity = 0.0;
                }
                timeline.process_body(
                    state.current_build_time_ms,
                    std::slice::from_ref(stmt),
                    None,
                    diagnostics,
                );
                timeline.default_opacity = saved_opacity;
            },
            _ => {}, // Config/import/type aliases are handled by earlier phases
        }
    }
}

/// Lower the collected `always` bodies into IR for frame-time interpretation.
///
/// Also snapshots a hash of the modifier AST (for cross-rebuild IR reuse),
/// records which targets the programs write, and validates `&label.prop`
/// references against the declared tracks. Runs after the statement walk
/// because it consumes `timeline.modifiers`.
pub(super) fn lower_modifiers(timeline: &mut Timeline, diagnostics: &mut Vec<Diagnostic>) {
    // Compute modifier hash for cross-rebuild IR caching.
    //
    // We hash the Debug representation because the AST types cannot derive
    // `Hash` (several variants hold `f64`/`f32`, which do not implement
    // `Hash`). This is safe in practice: the hash is only compared within a
    // single process/session and is never persisted, Debug output is a
    // faithful rendering of every field, and a changed hash merely causes an
    // unnecessary re-lower rather than incorrect cache reuse.
    {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for stmt in &timeline.modifiers {
            format!("{:?}", stmt).hash(&mut hasher);
        }
        timeline.modifier_hash = hasher.finish();
    }

    // Skip lowering when no modifiers exist to avoid empty programs.
    if !timeline.modifiers.is_empty() {
        match crate::timeline::modifier_runtime::ir::lower_modifier_body(&timeline.modifiers) {
            Ok(program) => {
                timeline.modifier_programs.push(program);
            },
            Err(e) => {
                diagnostics.push(Diagnostic::warning(
                    DiagnosticCode::ModifierCompilationError,
                    DiagnosticPhase::Build,
                    format!(
                        "Always-block lowering failed: {e}. Modifier execution will be skipped."
                    ),
                ));
            },
        }
    }

    // Collect actor targets written by always blocks / modifiers so the
    // static subtree cache can safely distinguish static actors from dynamic ones.
    timeline.modifier_written_targets.clear();
    for program in &timeline.modifier_programs {
        program.collect_written_targets(&mut timeline.modifier_written_targets);
    }

    // Validate `&label.prop` references at build time: the label must be
    // a declared track and the property must be injectable. The reference
    // resolves at frame time today; this pass moves the common failure
    // (typo'd label or property) to build diagnostics. Runs after IR
    // lowering because it walks the lowered modifier programs.
    let mut prop_refs: Vec<(String, String)> = Vec::new();
    for program in &timeline.modifier_programs {
        program.collect_prop_refs(&mut prop_refs);
    }
    prop_refs.sort();
    prop_refs.dedup();
    for (label, prop) in prop_refs {
        if !timeline.world.tracks.contains_key(&label) {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::UnknownTargetPath,
                    DiagnosticPhase::Build,
                    format!(
                        "Property reference `&{label}.{prop}` targets an unknown actor '{label}'."
                    ),
                )
                .with_subject(label),
            );
            continue;
        }
        let injectable = crate::timeline::property_registry::resolve_property(prop.as_str())
            .is_some_and(|(schema, _)| {
                schema
                    .flags
                    .contains(crate::timeline::property_registry::PropertyFlags::INJECTABLE)
            });
        if !injectable {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::UnknownTargetPath,
                    DiagnosticPhase::Build,
                    format!(
                        "Property reference `&{label}.{prop}` names a property that is not injectable."
                    ),
                )
                .with_subject(label),
            );
        }
    }
}
