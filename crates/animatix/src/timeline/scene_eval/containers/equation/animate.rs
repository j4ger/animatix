use super::*;
use crate::primitives::{RenderCommand, RenderFrame, RenderOutputs};

impl Timeline {
    pub(super) fn render_equation_transition(
        &self,
        id: EntityId,
        after: &[FragInfo],
        transform: kurbo::Affine,
        opacity: f32,
        frame: &RenderFrame<'_>,
        out: &mut RenderOutputs<'_, '_>,
    ) -> bool {
        let time = frame.time_ms;
        let duration =
            (self.world.property_plan(id).sample_f32_prop("equation_transition", time, 0.0)
                * 1000.0) as u64;
        if duration == 0 || time == 0 {
            return false;
        }
        let mut changes = std::collections::BTreeSet::new();
        for &child in &self.world.hierarchy(id).children {
            changes.insert(self.world.first_seen(child));
            if let Some(track) = self.world.text(child).and_then(|text| text.text_content.as_ref())
            {
                changes.extend(track.keyframes().keys().copied());
            }
            if let Some(key) = crate::timeline::property_id("fragment_order") {
                changes.extend(self.world.property_plan(child).keyframe_times(key));
            }
        }
        let first = self
            .world
            .first_seen(id)
            .saturating_add(1)
            .max(time.saturating_sub(duration - 1));
        if first > time {
            return false;
        }
        for &change in changes.range(first..=time).rev() {
            let before = self.collect_equation_fragments(
                id,
                &RenderFrame {
                    time_ms: change - 1,
                    ..*frame
                },
            );
            if same_state(&before, after) {
                continue;
            }
            let progress = (time - change) as f64 / duration as f64;
            let Ok(source) = self.compile_equation(id, time, &before) else {
                return false; // The normal draw path reports the active layout's compile failure.
            };
            let Ok(target) = self.compile_equation(id, time, after) else {
                return false; // The normal draw path reports the active layout's compile failure.
            };
            let mut paths = Vec::new();
            let mut overlays = Vec::new();
            for (index, fragment) in before.iter().enumerate() {
                let from = fragment_paths(&source, index);
                if let Some((index, destination)) =
                    after.iter().enumerate().find(|(_, next)| next.id == fragment.id)
                {
                    let to = fragment_paths(&target, index);
                    move_fragment(
                        &mut paths,
                        &mut overlays,
                        fragment,
                        destination,
                        from,
                        to,
                        progress,
                    );
                } else {
                    append(
                        &mut paths,
                        &mut overlays,
                        fragment,
                        from,
                        kurbo::Vec2::ZERO,
                        1.0 - progress,
                    );
                }
            }
            for (index, fragment) in after.iter().enumerate() {
                if !before.iter().any(|old| old.id == fragment.id) {
                    let to = fragment_paths(&target, index);
                    append(&mut paths, &mut overlays, fragment, to, kurbo::Vec2::ZERO, progress);
                }
            }
            draw_highlights(&overlays, true, out.scene, &transform, opacity);
            RenderCommand::Text {
                paths: paths.into(),
                fill_gradient: None,
                clip_path: None,
                tint_color: None,
                hardware_layout: None,
            }
            .execute(out.scene, &transform, opacity);
            draw_highlights(&overlays, false, out.scene, &transform, opacity);
            return true;
        }
        false
    }
}

fn fragment_paths(compiled: &animatix_text::CachedGroupedText, index: usize) -> &[TextPath] {
    compiled
        .ranges
        .get(index)
        .and_then(|range| compiled.glyphs.get(range.clone()))
        .unwrap_or_default()
}

fn same_state(a: &[FragInfo], b: &[FragInfo]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.id == b.id && a.content == b.content)
}

fn move_fragment(
    paths: &mut Vec<TextPath>,
    overlays: &mut Vec<RenderCommand>,
    old: &FragInfo,
    new: &FragInfo,
    from: &[TextPath],
    to: &[TextPath],
    progress: f64,
) {
    let center = |paths: &[TextPath]| {
        paths.iter().map(|p| p.bounds).reduce(|a, b| a.union(b)).map(|r| r.center())
    };
    let offset = center(from).zip(center(to)).map_or(kurbo::Vec2::ZERO, |(a, b)| b - a);
    if old.content == new.content
        && from.len() == to.len()
        && from.iter().zip(to).all(|(a, b)| {
            (a.bounds.width() - b.bounds.width()).abs() < 0.01
                && (a.bounds.height() - b.bounds.height()).abs() < 0.01
                && ((b.bounds.center() - a.bounds.center()) - offset).hypot() < 0.01
        })
    {
        append(paths, overlays, new, from, offset * progress, 1.0);
    } else {
        append(paths, overlays, old, from, offset * progress, 1.0 - progress);
        append(paths, overlays, new, to, -offset * (1.0 - progress), progress);
    }
}

fn append(
    paths: &mut Vec<TextPath>,
    overlays: &mut Vec<RenderCommand>,
    fragment: &FragInfo,
    slice: &[TextPath],
    translation: kurbo::Vec2,
    alpha: f64,
) {
    if alpha <= 0.0 || slice.is_empty() {
        return;
    }
    let start = paths.len();
    paths.extend(slice.iter().cloned().map(|mut glyph| {
        glyph.translate((translation.x, translation.y));
        glyph.opacity *= alpha as f32;
        if let Some(color) = fragment.paint {
            glyph.color = color;
        }
        glyph
    }));
    if let Some(command) = equation_highlight(fragment, &paths[start..], alpha as f32) {
        overlays.push(command);
    }
}
