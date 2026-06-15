//! AST utility functions: searching/filtering/transforming statement trees.
//!
//! These were extracted from the app module; they operate purely on the AST
//! and belong in the source_edit crate alongside other AST-manipulation helpers.

use animatix_syntax::ast::{Stmt, Time};
use super::apply::time_to_seconds;
use std::cell::RefCell;

thread_local! {
    /// Queue of keyframe absolute times (in seconds) that should be flashed
    /// in the timeline panel because their relative offset was rewritten.
    pub static ADJUST_FLASH_QUEUE: RefCell<Vec<f64>> = RefCell::new(Vec::new());
}

/// Compute the absolute time (in seconds) of the keyframe at `idx`.
pub fn compute_keyframe_abs_time(stmts: &[Stmt], idx: usize) -> f64 {
    let mut t = 0.0;
    for i in 0..=idx {
        match &stmts[i] {
            Stmt::Keyframe { time, .. } => t = time_to_seconds(time),
            Stmt::RelativeKeyframe { offset, .. } => t += time_to_seconds(offset),
            _ => {}
        }
    }
    t
}

/// Push a flash event for an absolute keyframe time.
pub fn push_adjust_flash_time(time_s: f64) {
    ADJUST_FLASH_QUEUE.with(|q| q.borrow_mut().push(time_s));
}

/// Clear the adjust flash queue.
pub fn clear_adjust_flash_queue() {
    ADJUST_FLASH_QUEUE.with(|q| q.borrow_mut().clear());
}

/// Drain the adjust flash queue, returning all accumulated flash times.
pub fn drain_adjust_flash_queue() -> Vec<f64> {
    ADJUST_FLASH_QUEUE.with(|q| q.borrow_mut().drain(..).collect())
}

// ---------------------------------------------------------------------------
// Keyframe discovery
// ---------------------------------------------------------------------------

/// Find all top-level keyframe or relative-keyframe statements whose body
/// contains an assignment targeting the given actor label.
pub fn find_keyframes_for_actor(stmts: &[Stmt], actor: &str) -> Vec<Stmt> {
    let mut result = Vec::new();
    for stmt in stmts {
        match stmt {
            Stmt::Keyframe { .. } | Stmt::RelativeKeyframe { .. } => {
                if keyframe_references_actor(stmt, actor) {
                    result.push(stmt.clone());
                }
            }
            _ => {}
        }
    }
    result
}

/// Check if a keyframe statement (or any nested child) references the given actor.
pub fn keyframe_references_actor(stmt: &Stmt, actor: &str) -> bool {
    let mut found = false;
    animatix_syntax::walk::walk_stmts(std::slice::from_ref(stmt), &mut |s| {
        if let Stmt::Assignment { target, .. } = s {
            if target.iter().any(|t| t == actor) {
                found = true;
            }
        }
    });
    found
}

// ---------------------------------------------------------------------------
// Keyframe insertion helpers (shared by keyframe_edits and action_edits)
// ---------------------------------------------------------------------------

/// Style of the keyframe immediately preceding a given time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyframeStyle {
    Absolute,
    Relative,
}

/// Determine the style (absolute vs relative) of the keyframe immediately
/// preceding `time_s`. New keyframes inherit this style so that relative
/// chains stay relative, absolute breakpoints stay absolute.
pub fn keyframe_style_before(stmts: &[Stmt], time_s: f64) -> KeyframeStyle {
    let mut current_time = 0.0f64;
    let mut style = KeyframeStyle::Absolute;

    for stmt in stmts {
        match stmt {
            Stmt::Keyframe { time, .. } => {
                current_time = time_to_seconds(time);
                if current_time <= time_s {
                    style = KeyframeStyle::Absolute;
                }
            }
            Stmt::RelativeKeyframe { offset, .. } => {
                current_time += time_to_seconds(offset);
                if current_time <= time_s {
                    style = KeyframeStyle::Relative;
                }
            }
            _ => {}
        }
        if current_time > time_s {
            break;
        }
    }
    style
}

/// Find the index after which a new keyframe at `time_s` should be inserted.
pub fn find_keyframe_insertion_point(stmts: &[Stmt], time_s: f64) -> usize {
    let mut last_kf_idx = 0usize;
    let mut current_time = 0.0f64;

    for (i, stmt) in stmts.iter().enumerate() {
        match stmt {
            Stmt::Keyframe { time, .. } => {
                current_time = time_to_seconds(time);
                if current_time <= time_s {
                    last_kf_idx = i + 1;
                }
            }
            Stmt::RelativeKeyframe { offset, .. } => {
                current_time += time_to_seconds(offset);
                if current_time <= time_s {
                    last_kf_idx = i + 1;
                }
            }
            _ => {}
        }
    }

    last_kf_idx
}

/// Find the absolute time of the keyframe immediately before `time_s`.
pub fn find_prev_keyframe_time(stmts: &[Stmt], time_s: f64) -> f64 {
    let mut current_time = 0.0f64;
    let mut prev_time = 0.0f64;

    for stmt in stmts {
        match stmt {
            Stmt::Keyframe { time, .. } => {
                current_time = time_to_seconds(time);
            }
            Stmt::RelativeKeyframe { offset, .. } => {
                current_time += time_to_seconds(offset);
            }
            _ => continue,
        }
        if current_time > time_s {
            break;
        }
        prev_time = current_time;
    }

    prev_time
}

/// Wrap leading top-level declarations in a `#0s` keyframe so they don't get
/// shifted to a later time by a new relative keyframe.
///
/// Returns the (possibly adjusted) insertion index.
pub fn wrap_leading_decls_in_zero_keyframe(
    stmts: &mut Vec<Stmt>,
    insert_idx: usize,
    prev_time_s: f64,
) -> usize {
    if insert_idx != 0 || prev_time_s >= 0.001 || stmts.is_empty() {
        return insert_idx;
    }
    let first_is_keyframe = matches!(
        stmts[0],
        Stmt::Keyframe { .. } | Stmt::RelativeKeyframe { .. }
    );
    if first_is_keyframe {
        return insert_idx;
    }
    let decl_end = stmts
        .iter()
        .position(|s| matches!(s, Stmt::Keyframe { .. } | Stmt::RelativeKeyframe { .. }))
        .unwrap_or(stmts.len());
    if decl_end == 0 {
        return insert_idx;
    }
    let decls: Vec<Stmt> = stmts.drain(0..decl_end).collect();
    let zero_kf = Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: decls,
        span: None,
    };
    stmts.insert(0, zero_kf);
    insert_idx + 1
}

/// Subtract `delta_s` from the next relative keyframe's offset so that
/// subsequent keyframes keep their original absolute times.
pub fn adjust_following_relative_keyframe(
    stmts: &mut [Stmt],
    insert_idx: usize,
    delta_s: f64,
) {
    if insert_idx >= stmts.len() || delta_s < 0.001 {
        return;
    }
    if !matches!(stmts[insert_idx], Stmt::RelativeKeyframe { .. }) {
        return;
    }
    let flash_time = compute_keyframe_abs_time(stmts, insert_idx);
    if let Stmt::RelativeKeyframe { offset: ref mut next_offset, .. } = stmts[insert_idx] {
        let next_delta_s = time_to_seconds(next_offset);
        let new_next_delta_s = next_delta_s - delta_s;
        if new_next_delta_s >= 0.001 {
            push_adjust_flash_time(flash_time);
            *next_offset = if new_next_delta_s < 1.0 {
                Time::Milliseconds((new_next_delta_s * 1000.0).round() as u64)
            } else {
                Time::Seconds(new_next_delta_s)
            };
        }
    }
}

/// Append a statement to the keyframe at `time_s` (within ε tolerance).
/// Returns `true` if a matching keyframe was found.
pub fn append_to_keyframe_at_time(stmts: &mut [Stmt], time_s: f64, stmt: Stmt) -> bool {
    const EPSILON_S: f64 = 0.05;
    let mut current_time = 0.0f64;

    for kf in stmts.iter_mut() {
        match kf {
            Stmt::Keyframe { time, body, .. } => {
                current_time = time_to_seconds(time);
                if (current_time - time_s).abs() < EPSILON_S {
                    body.push(stmt);
                    return true;
                }
            }
            Stmt::RelativeKeyframe { offset, body, .. } => {
                current_time += time_to_seconds(offset);
                if (current_time - time_s).abs() < EPSILON_S {
                    body.push(stmt);
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Shift absolute keyframe times by `offset_s` seconds. Relative keyframes
/// and non-keyframe statements are left untouched.
pub fn shift_keyframe_times(stmts: &mut [Stmt], offset_s: f64) {
    if offset_s.abs() < 0.001 {
        return;
    }
    for stmt in stmts.iter_mut() {
        match stmt {
            Stmt::Keyframe { time, .. } => {
                let t = match time {
                    animatix_syntax::ast::Time::Seconds(s) => *s,
                    animatix_syntax::ast::Time::Milliseconds(ms) => *ms as f64 / 1000.0,
                };
                let new_t = t + offset_s;
                *time = animatix_syntax::ast::Time::Seconds(new_t);
            }
            Stmt::RelativeKeyframe { .. } => {
                // Relative keyframes keep their relative offset
            }
            Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::ComponentDef(animatix_syntax::ast::ComponentDef { body, .. }, _)
            | Stmt::ComponentAction { body, .. } => {
                shift_keyframe_times(body, offset_s);
            }
            Stmt::Conditional {
                then_branch,
                else_branch,
                ..
            } => {
                shift_keyframe_times(then_branch, offset_s);
                if let Some(eb) = else_branch {
                    shift_keyframe_times(eb, offset_s);
                }
            }
            Stmt::ForLoop { body, .. } => {
                shift_keyframe_times(body, offset_s);
            }
            _ => {}
        }
    }
}