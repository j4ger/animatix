use crate::app::commands::{Effect, UndoLabel};
use crate::app::document_controller::DocumentController;
use crate::app::stores::{DocumentStore, PreviewStore, UiStore};

pub fn handle_create_actor(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    ty: String,
    label: String,
    position: [f32; 2],
    props: Vec<animatix_syntax::ast::Property>,
) -> Vec<Effect> {
    document_store.snapshot(UndoLabel::CreateActor {
        ty: ty.clone(),
        label: label.clone(),
        position,
        props: props.clone(),
    });
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_create_actor(&ty, &label, position, props);
    vec![]
}

pub fn handle_rename_actor(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    old_label: String,
    new_label: String,
) -> Vec<Effect> {
    if old_label == new_label {
        return vec![];
    }
    if new_label.is_empty() {
        preview_store.preview.status = "Rename failed — label cannot be empty".to_string();
        return vec![];
    }
    if let Some(timeline) = document_store.source.document.active_timeline() {
        if timeline.has_actor(&new_label) {
            preview_store.preview.status =
                format!("Rename failed — '{}' already exists", new_label);
            return vec![];
        }
    }

    document_store.snapshot(UndoLabel::RenameActor {
        old_label: old_label.clone(),
        new_label: new_label.clone(),
    });

    let old_label_for_edit = old_label.clone();
    let new_label_for_edit = new_label.clone();

    if let Some(ref mut stmts) = document_store.source.document.raw_statements {
        let edit = crate::source_edit::SourceEdit::RenameActor {
            old_label: old_label_for_edit,
            new_label: new_label_for_edit,
        };
        if crate::source_edit::apply_edit(stmts, edit).is_err() {
            preview_store.preview.status =
                format!("Rename failed — could not rename '{}' to '{}'", old_label, new_label);
            return vec![];
        }
        let (new_source, source_index) = (
            animatix_syntax::to_source::stmts_to_source(stmts),
            animatix_syntax::source_index::SourceIndex::build(stmts),
        );
        document_store.commit_source(new_source, source_index);
        preview_store.pending_rebuild_at =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(100));
        preview_store.preview.status = format!("Renamed {} → {}", old_label, new_label);
    } else {
        preview_store.preview.status = "Rename failed — no AST available".to_string();
        return vec![];
    }

    if ui_store.selection.selected_actors.contains(&old_label) {
        ui_store.selection.selected_actors.remove(&old_label);
        ui_store.selection.selected_actors.insert(new_label.clone());
    }
    preview_store.preview_dirty = true;
    vec![]
}

pub fn handle_duplicate_actor(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    original_label: String,
) -> Vec<Effect> {
    document_store.snapshot(UndoLabel::DuplicateActor(original_label.clone()));
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_duplicate_actor(&original_label);
    vec![]
}

pub fn handle_delete_selected_actors(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
) -> Vec<Effect> {
    document_store.snapshot(UndoLabel::DeleteSelectedActors);
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_delete_selected_actors();
    vec![]
}

pub fn handle_paste_actors(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
) -> Vec<Effect> {
    document_store.snapshot(UndoLabel::PasteActors);
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.paste_actors();
    vec![]
}

pub fn handle_reparent_actor(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    actor: String,
    new_parent: Option<String>,
) -> Vec<Effect> {
    document_store.snapshot(UndoLabel::ReparentActor {
        actor: actor.clone(),
        new_parent: new_parent.clone(),
    });
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_reparent_actor(&actor, new_parent);
    vec![]
}

pub fn handle_extract_scene(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    actor_labels: Vec<String>,
    new_scene_name: String,
) -> Vec<Effect> {
    document_store.snapshot(UndoLabel::ExtractScene {
        actor_labels: actor_labels.clone(),
        new_scene_name: new_scene_name.clone(),
    });
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_extract_scene(actor_labels, new_scene_name);
    vec![]
}

pub fn handle_move_to_scene(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    actor_labels: Vec<String>,
    target_scene: String,
) -> Vec<Effect> {
    document_store.snapshot(UndoLabel::MoveToScene {
        actor_labels: actor_labels.clone(),
        target_scene: target_scene.clone(),
    });
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_move_to_scene(actor_labels, target_scene);
    vec![]
}

/// Toggle actor visibility in the preview.
///
/// **Ephemeral by design.** Visibility is a UI-layer concern (like collapsed
/// nodes in an outline), not an animation-layer property. It mutates the
/// in-memory `Timeline` but intentionally does NOT persist to `.amx` source
/// — amx files must not be coupled with GUI state. The toggle is lost on
/// rebuild (source edit, file reopen, undo). Revisit if users need
/// durable visibility (e.g. for export-time layer control).
pub fn handle_toggle_actor_visibility(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    _ui_store: &mut UiStore,
    actor: String,
) -> Vec<Effect> {
    if let Some(at) = document_store.source.document.active_timeline_mut() {
        let timeline = &mut *at.timeline;
        if let Some(track) = timeline.get_track_mut(&actor) {
            track.visible = !track.visible;
            preview_store.preview_dirty = true;
            let status = if track.visible {
                format!("{actor} visible")
            } else {
                format!("{actor} hidden")
            };
            preview_store.preview.status = status;
        }
    }
    vec![]
}

/// Toggle actor lock (prevent selection/dragging in preview).
///
/// **Ephemeral by design.** See [`handle_toggle_actor_visibility`] — same
/// rationale. Lock state is UI-layer, not animation-layer.
pub fn handle_toggle_actor_lock(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    _ui_store: &mut UiStore,
    actor: String,
) -> Vec<Effect> {
    if let Some(at) = document_store.source.document.active_timeline_mut() {
        let timeline = &mut *at.timeline;
        if let Some(track) = timeline.get_track_mut(&actor) {
            track.locked = !track.locked;
            preview_store.preview_dirty = true;
            let status = if track.locked {
                format!("{actor} locked")
            } else {
                format!("{actor} unlocked")
            };
            preview_store.preview.status = status;
        }
    }
    vec![]
}

use crate::app::commands::{Align, Axis};

pub fn handle_align_actors(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    alignment: Align,
) -> Vec<Effect> {
    if ui_store.selection.selected_actors.len() < 2 {
        preview_store.preview.status = "Select 2+ actors to align".to_string();
        return vec![];
    }

    let bounds = &document_store.source.cached_actor_bounds;
    let mut rects = Vec::new();
    for actor in &ui_store.selection.selected_actors {
        if let Some(rect) = bounds.get(actor) {
            rects.push((actor.clone(), *rect));
        }
    }
    if rects.len() < 2 {
        preview_store.preview.status = "No spatial data for alignment".to_string();
        return vec![];
    }

    // Compute reference edge from the first actor (primary selection)
    let ref_value = match alignment {
        Align::Left => rects[0].1.x0,
        Align::Center => (rects[0].1.x0 + rects[0].1.x1) / 2.0,
        Align::Right => rects[0].1.x1,
        Align::Top => rects[0].1.y0,
        Align::Bottom => rects[0].1.y1,
        Align::Middle => (rects[0].1.y0 + rects[0].1.y1) / 2.0,
    };

    let time_ms = (preview_store.preview.playback.current_time_s() * 1000.0) as u64;
    let keyframe_mode = ui_store.keyframe_mode;
    let mut edits = Vec::new();

    if let Some(timeline) = document_store.source.document.active_timeline() {
        for (actor, rect) in &rects[1..] {
            if let Some(track) = timeline.get_track(actor) {
                let pos = track
                    .geometry
                    .position
                    .as_ref()
                    .map(|p| p.evaluate(time_ms))
                    .unwrap_or([0.0, 0.0]);
                let new_pos = match alignment {
                    Align::Left => [ref_value as f32 + (pos[0] - rect.x0 as f32), pos[1]],
                    Align::Center => [
                        ref_value as f32 + (pos[0] - ((rect.x0 + rect.x1) / 2.0) as f32),
                        pos[1],
                    ],
                    Align::Right => [ref_value as f32 + (pos[0] - rect.x1 as f32), pos[1]],
                    Align::Top => [pos[0], ref_value as f32 + (pos[1] - rect.y0 as f32)],
                    Align::Bottom => [pos[0], ref_value as f32 + (pos[1] - rect.y1 as f32)],
                    Align::Middle => [
                        pos[0],
                        ref_value as f32 + (pos[1] - ((rect.y0 + rect.y1) / 2.0) as f32),
                    ],
                };
                edits.push(crate::app::commands::PropertyEdit {
                    actor: actor.clone(),
                    property: "position".into(),
                    value: crate::app::commands::PropertyValue::Vec2(new_pos),
                    create_keyframe: keyframe_mode,
                    time_s: None,
                });
            }
        }
    }

    if edits.is_empty() {
        preview_store.preview.status = "Alignment failed".to_string();
        return vec![];
    }

    document_store.snapshot(UndoLabel::AlignActors(alignment));
    if let Some(ref mut stmts) = document_store.source.document.raw_statements {
        let snapshot = stmts.clone();
        for edit in &edits {
            let expr = match crate::app::commands::property_value_to_expr(edit.value.clone()) {
                Ok(e) => e,
                Err(e) => {
                    *stmts = snapshot;
                    preview_store.preview.status =
                        format!("Alignment failed: expression error for '{}': {}", edit.actor, e);
                    return vec![];
                },
            };
            let source_edit = crate::source_edit::SourceEdit::SetProperty {
                actor: edit.actor.clone(),
                property: "at".into(),
                value: expr,
            };
            if let Err(e) = crate::source_edit::apply_edit(stmts, source_edit) {
                *stmts = snapshot;
                preview_store.preview.status = format!("Alignment failed: {}", e);
                return vec![];
            }
        }
        let (new_source, source_index) = (
            animatix_syntax::to_source::stmts_to_source(stmts),
            animatix_syntax::source_index::SourceIndex::build(stmts),
        );
        document_store.commit_source(new_source, source_index);
        preview_store.pending_rebuild_at =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(100));
    }
    preview_store.preview.status = format!("Aligned {} actors", rects.len());
    vec![]
}

pub fn handle_distribute_actors(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    axis: Axis,
) -> Vec<Effect> {
    if ui_store.selection.selected_actors.len() < 3 {
        preview_store.preview.status = "Select 3+ actors to distribute".to_string();
        return vec![];
    }

    let bounds = &document_store.source.cached_actor_bounds;
    let mut rects: Vec<(String, kurbo::Rect)> = ui_store
        .selection
        .selected_actors
        .iter()
        .filter_map(|a| bounds.get(a).map(|r| (a.clone(), *r)))
        .collect();
    if rects.len() < 3 {
        preview_store.preview.status = "No spatial data for distribution".to_string();
        return vec![];
    }

    // Sort by position along the distribution axis
    match axis {
        Axis::Horizontal => rects.sort_by(|a, b| f64::total_cmp(&a.1.x0, &b.1.x0)),
        Axis::Vertical => rects.sort_by(|a, b| f64::total_cmp(&a.1.y0, &b.1.y0)),
    }

    let (Some(first), Some(last)) = (rects.first(), rects.last()) else {
        return vec![];
    };
    let (start, end) = match axis {
        Axis::Horizontal => (first.1.x0, last.1.x1),
        Axis::Vertical => (first.1.y0, last.1.y1),
    };
    let total_span = end - start;
    let count = rects.len();
    let step = total_span / (count - 1) as f64;

    let time_ms = (preview_store.preview.playback.current_time_s() * 1000.0) as u64;
    let keyframe_mode = ui_store.keyframe_mode;
    let mut edits = Vec::new();

    if let Some(timeline) = document_store.source.document.active_timeline() {
        for (i, (actor, rect)) in rects.iter().enumerate() {
            if let Some(track) = timeline.get_track(actor) {
                let pos = track
                    .geometry
                    .position
                    .as_ref()
                    .map(|p| p.evaluate(time_ms))
                    .unwrap_or([0.0, 0.0]);
                let target = start + step * i as f64;
                let new_pos = match axis {
                    Axis::Horizontal => [target as f32 + (pos[0] - rect.x0 as f32), pos[1]],
                    Axis::Vertical => [pos[0], target as f32 + (pos[1] - rect.y0 as f32)],
                };
                edits.push(crate::app::commands::PropertyEdit {
                    actor: actor.clone(),
                    property: "position".into(),
                    value: crate::app::commands::PropertyValue::Vec2(new_pos),
                    create_keyframe: keyframe_mode,
                    time_s: None,
                });
            }
        }
    }

    if edits.is_empty() {
        preview_store.preview.status = "Distribution failed".to_string();
        return vec![];
    }

    document_store.snapshot(UndoLabel::DistributeActors(axis));
    if let Some(ref mut stmts) = document_store.source.document.raw_statements {
        let snapshot = stmts.clone();
        for edit in &edits {
            let expr = match crate::app::commands::property_value_to_expr(edit.value.clone()) {
                Ok(e) => e,
                Err(e) => {
                    *stmts = snapshot;
                    preview_store.preview.status = format!(
                        "Distribution failed: expression error for '{}': {}",
                        edit.actor, e
                    );
                    return vec![];
                },
            };
            let source_edit = crate::source_edit::SourceEdit::SetProperty {
                actor: edit.actor.clone(),
                property: "at".into(),
                value: expr,
            };
            if let Err(e) = crate::source_edit::apply_edit(stmts, source_edit) {
                *stmts = snapshot;
                preview_store.preview.status = format!("Distribution failed: {}", e);
                return vec![];
            }
        }
        let (new_source, source_index) = (
            animatix_syntax::to_source::stmts_to_source(stmts),
            animatix_syntax::source_index::SourceIndex::build(stmts),
        );
        document_store.commit_source(new_source, source_index);
        preview_store.pending_rebuild_at =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(100));
    }
    preview_store.preview.status = format!("Distributed {} actors", rects.len());
    vec![]
}

pub fn handle_group_selected_actors(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
) -> Vec<Effect> {
    if ui_store.selection.selected_actors.len() < 2 {
        preview_store.preview.status = "Select 2+ actors to group".to_string();
        return vec![];
    }

    let labels: Vec<String> = ui_store.selection.selected_actors.iter().cloned().collect();
    let group_label = crate::app::utils::labels::unique_label(
        document_store.source.document.active_timeline(),
        "group",
    );

    // Compute center of selected actors for group position
    let bounds = &document_store.source.cached_actor_bounds;
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut has_any = false;
    for actor in &labels {
        if let Some(rect) = bounds.get(actor) {
            min_x = min_x.min(rect.x0);
            min_y = min_y.min(rect.y0);
            max_x = max_x.max(rect.x1);
            max_y = max_y.max(rect.y1);
            has_any = true;
        }
    }

    let center = if has_any {
        [
            ((min_x + max_x) / 2.0) as f32,
            ((min_y + max_y) / 2.0) as f32,
        ]
    } else {
        [0.0, 0.0]
    };

    document_store.snapshot(UndoLabel::GroupSelectedActors);

    // Insert Group actor
    if let Some(ref mut stmts) = document_store.source.document.raw_statements {
        let group_props = vec![animatix_syntax::ast::Property {
            name: "at".into(),
            value: animatix_syntax::ast::Expr::Tuple(vec![
                animatix_syntax::ast::Expr::Num(center[0] as f64),
                animatix_syntax::ast::Expr::Num(center[1] as f64),
            ]),
            value_span: None,
            trailing_comment: None,
        }];
        let edit = crate::source_edit::SourceEdit::InsertActor {
            ty: "Group".into(),
            label: group_label.clone(),
            props: group_props,
            container: None,
            time_s: 0.0,
        };
        // Take snapshot for rollback
        let snapshot = stmts.clone();
        if let Err(e) = crate::source_edit::apply_edit(stmts, edit) {
            *stmts = snapshot;
            preview_store.preview.status = format!("Group failed: {}", e);
            return vec![];
        }
        // Reparent each selected actor into the group
        for actor in &labels {
            let reparent = crate::source_edit::SourceEdit::Reparent {
                actor: actor.clone(),
                new_parent: Some(group_label.clone()),
            };
            if let Err(e) = crate::source_edit::apply_edit(stmts, reparent) {
                *stmts = snapshot;
                preview_store.preview.status =
                    format!("Group failed while reparenting '{}': {}", actor, e);
                return vec![];
            }
        }
        let (new_source, source_index) = (
            animatix_syntax::to_source::stmts_to_source(stmts),
            animatix_syntax::source_index::SourceIndex::build(stmts),
        );
        document_store.commit_source(new_source, source_index);
        preview_store.pending_rebuild_at =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(100));
        ui_store.selection.selected_actors.clear();
        ui_store.selection.selected_actors.insert(group_label.clone());
        preview_store.preview.status =
            format!("Grouped {} actors into {}", labels.len(), group_label);
    }
    vec![]
}

pub fn handle_ungroup_selected_actors(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
) -> Vec<Effect> {
    let groups: Vec<String> = ui_store
        .selection
        .selected_actors
        .iter()
        .filter(|a| {
            document_store
                .source
                .document
                .active_timeline()
                .and_then(|tl| tl.get_track(a))
                .map(|t| t.kind == animatix::timeline::ActorKindId::Group)
                .unwrap_or(false)
        })
        .cloned()
        .collect();

    if groups.is_empty() {
        preview_store.preview.status = "No Group selected to ungroup".to_string();
        return vec![];
    }

    document_store.snapshot(UndoLabel::UngroupSelectedActors);

    // Collect children from timeline outside the mutable borrow on raw_statements.
    let mut group_children: Vec<(String, Vec<String>)> = Vec::new();
    if let Some(timeline) = document_store.source.document.active_timeline() {
        for group in &groups {
            let children: Vec<String> =
                timeline.get_track(group).map(|t| t.children.clone()).unwrap_or_default();
            group_children.push((group.clone(), children));
        }
    }

    if let Some(ref mut stmts) = document_store.source.document.raw_statements {
        let snapshot = stmts.clone();
        let mut ungrouped = 0;
        for (group, children) in &group_children {
            for child in children {
                let reparent = crate::source_edit::SourceEdit::Reparent {
                    actor: child.clone(),
                    new_parent: None,
                };
                if let Err(e) = crate::source_edit::apply_edit(stmts, reparent) {
                    *stmts = snapshot;
                    preview_store.preview.status = format!("Ungroup failed: {}", e);
                    return vec![];
                }
                ungrouped += 1;
            }

            // Remove the group actor
            let delete = crate::source_edit::SourceEdit::DeleteActor {
                label: group.clone(),
            };
            if let Err(e) = crate::source_edit::apply_edit(stmts, delete) {
                *stmts = snapshot;
                preview_store.preview.status = format!("Ungroup failed: {}", e);
                return vec![];
            }
        }

        let (new_source, source_index) = (
            animatix_syntax::to_source::stmts_to_source(stmts),
            animatix_syntax::source_index::SourceIndex::build(stmts),
        );
        document_store.commit_source(new_source, source_index);
        preview_store.pending_rebuild_at =
            Some(std::time::Instant::now() + std::time::Duration::from_millis(100));
        for group in &groups {
            ui_store.selection.selected_actors.remove(group);
        }
        preview_store.preview.status = format!("Ungrouped {} children", ungrouped);
    }
    vec![]
}
