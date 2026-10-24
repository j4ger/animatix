use super::*;

pub(crate) fn render_empty_property_lane(
    ui: &mut egui::Ui,
    timeline: &animatix::timeline::Timeline,
    track: &animatix::timeline::AnimationTrack,
    effects: Option<&animatix::timeline::effects::EffectChainTrack>,
    actor_label: &str,
    prop_name: &str,
    track_idx: usize,
    prop_bar_area: Rect,
    coords: &TimelineCoords,
    group_col: Color32,
    preview: &PreviewPaneState,
    commands: &mut ActionQueue,
    theme: eparts::Theme,
) {
    let painter = ui.painter();
    let actor_type = timeline.world().actor_type(track.id);
    let caps = timeline.world().caps(track.id);
    let playhead_s = preview.playback.current_time_s();
    let add_id = ui.id().with(("lane_add", track_idx, prop_name));
    let add_resp = ui.interact(prop_bar_area, add_id, Sense::click());
    if add_resp.hovered() {
        painter.rect_filled(
            prop_bar_area,
            0.0,
            theme.palette.accent.selection.linear_multiply(0.4),
        );
    }
    // Ghost diamond at the playhead as an affordance.
    let gx = coords.time_to_x(playhead_s);
    if gx >= prop_bar_area.left() && gx <= prop_bar_area.right() {
        painter.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(gx, prop_bar_area.center().y - KF_DIAMOND_HALF),
                Pos2::new(gx + KF_DIAMOND_HALF, prop_bar_area.center().y),
                Pos2::new(gx, prop_bar_area.center().y + KF_DIAMOND_HALF),
                Pos2::new(gx - KF_DIAMOND_HALF, prop_bar_area.center().y),
            ],
            group_col.linear_multiply(0.35),
            Stroke::new(STROKE_WIDTH, group_col),
        ));
    }
    text_tooltip(
        ui,
        add_id.with("tooltip"),
        &add_resp,
        &format!("{}: add keyframe at playhead", prop_name),
    );
    if add_resp.clicked() {
        if prop_name.contains('.') {
            let value = effects
                .and_then(|effects| effects.sample_lane(prop_name, (playhead_s * 1000.0) as u64))
                .unwrap_or(animatix::timeline::PropertyValue::F32(0.0));
            commands.push_back(
                DocumentCommand::PropertyEdit(PropertyEdit {
                    actor: actor_label.to_string(),
                    property: prop_name.to_string(),
                    value,
                    create_keyframe: true,
                    time_s: Some(playhead_s),
                })
                .into(),
            );
        } else if let Some(schema) = lane_schema(&caps, actor_type, prop_name) {
            // Read the seed value through the World field bundle so a shape-tier
            // lane (corner_radius, line_from, …) keyframes its live value
            // instead of the schema default.
            let value = timeline
                .world()
                .read_property_value(track.id, schema.field, (playhead_s * 1000.0) as u64)
                .unwrap_or_else(|| (schema.default_value)(&caps));
            commands.push_back(
                DocumentCommand::PropertyEdit(PropertyEdit {
                    actor: actor_label.to_string(),
                    property: schema.name.to_string(),
                    value,
                    create_keyframe: true,
                    time_s: Some(playhead_s),
                })
                .into(),
            );
        } else {
            tracing::warn!(
                property = %prop_name,
                "no writable schema for property lane; cannot add keyframe"
            );
        }
    }
}
