use std::ops::Range;

use super::Timeline;
use crate::timeline::{EntityId, TrackAccessor};

impl Timeline {
    /// Whether a caption from the specified audio transcript and segment range
    /// contributes at this scene time. Used by export preflight, off the render path.
    pub fn has_narration_caption_at(
        &self,
        source: EntityId,
        segments: Option<&Range<usize>>,
        time_s: f64,
    ) -> bool {
        let time_ms = crate::timeline::utils::seconds_to_millis(time_s) as u64;
        let time_s = time_ms as f64 / 1000.0;
        let Some(source) = self.world.transcript.get(source) else {
            return false;
        };
        let solo = self.resolve_solo_state(time_ms);
        self.world.tracks.iter().any(|(label, id)| {
            if self.world.actor_type(id) != "Captions"
                || self.world.first_seen(id) > time_ms
                || !self.is_actor_visible(label)
                || (solo.is_active() && !solo.is_soloed(id))
                || !self.caption_has_opacity(id, time_ms)
                || self.world.hierarchy(id).children.iter().any(|&child| {
                    self.world.first_seen(child) <= time_ms && self.world.actor_type(child) == "Cue"
                })
            {
                return false;
            }
            let Some((transcript, offset, limit)) = self.caption_transcript(id, time_ms) else {
                return false;
            };
            if !std::ptr::eq(transcript, &source.0) || time_s >= limit {
                return false;
            }
            let time = time_s - offset;
            let active = transcript
                .segments
                .iter()
                .enumerate()
                .filter(|(_, cue)| cue.start_s <= time && time < cue.end_s)
                .max_by(|(_, a), (_, b)| a.start_s.total_cmp(&b.start_s));
            active.is_some_and(|(index, cue)| {
                if cue.text.trim().is_empty()
                    || segments.is_some_and(|range| !range.contains(&index))
                {
                    return false;
                }
                let plan = self.world.property_plan(id);
                let enter = plan
                    .sample_f32_prop("caption_enter", time_ms, 0.0)
                    .max(plan.sample_f32_prop("transition_time", time_ms, 0.0));
                enter <= 0.0 || time > cue.start_s
            })
        })
    }

    fn caption_has_opacity(&self, mut id: EntityId, time_ms: u64) -> bool {
        loop {
            if self.world.style(id).opacity.get(time_ms, 1.0) <= 0.0 {
                return false;
            }
            match self.world.hierarchy(id).parent {
                Some(parent) => id = parent,
                None => return true,
            }
        }
    }

    /// Collect logical subtitle cues on the scene clock. An optional label
    /// selects one Captions actor; each actor resolves its own overlapping cues.
    pub fn subtitle_cues(
        &self,
        limit_s: f64,
        track: Option<&str>,
    ) -> Vec<animatix_text::transcript::SubtitleCue> {
        let overrides = std::collections::HashMap::new();
        let scene_dimensions =
            self.configured_canvas_size().map_or_else(Default::default, |(width, height)| {
                crate::timeline::SceneDimensions { width, height }
            });
        let mut result = Vec::new();
        for (label, id) in self.world.tracks.iter() {
            if self.world.actor_type(id) != "Captions" || track.is_some_and(|name| name != label) {
                continue;
            }
            let mut changes = Vec::new();
            for (entity, properties) in std::iter::once((id, &["source"][..])).chain(
                self.world
                    .hierarchy(id)
                    .children
                    .iter()
                    .map(|&child| (child, &["start", "end", "text"][..])),
            ) {
                changes.push(self.world.first_seen(entity));
                let plan = self.world.property_plan(entity);
                for property in properties {
                    if let Some(key) = crate::timeline::property_id(property) {
                        changes.extend(plan.keyframe_times(key));
                    }
                }
                if let Some(text) =
                    self.world.text(entity).and_then(|text| text.text_content.as_ref())
                {
                    changes.extend(text.keyframes().keys().copied());
                }
            }
            let first_seen = self.world.first_seen(id);
            changes.retain(|&time| time >= first_seen && (time as f64 / 1000.0) < limit_s);
            changes.sort_unstable();
            changes.dedup();
            let mut selected = Vec::new();
            for (index, &time_ms) in changes.iter().enumerate() {
                let start = time_ms as f64 / 1000.0;
                let end = changes.get(index + 1).map_or(limit_s, |next| *next as f64 / 1000.0);
                let frame = crate::primitives::RenderFrame {
                    paint_omissions: &[],
                    time_ms,
                    scene_dimensions,
                    debug_options: Default::default(),
                    overrides: &overrides,
                    frame_env: None,
                };
                let cues = self.collect_caption_cues(id, label, &frame);
                append_visible_intervals(&mut selected, &cues, start, end);
            }
            result.extend(selected);
        }
        result.sort_by(|a, b| a.start_s.total_cmp(&b.start_s));
        result
    }
}

fn append_visible_intervals(
    selected: &mut Vec<animatix_text::transcript::SubtitleCue>,
    cues: &[super::ActiveCue],
    window_start: f64,
    window_end: f64,
) {
    // Sort by the same precedence as the renderer, retaining authored order
    // when two cues start at the same time. Active ranks form a sweep line.
    let mut ordered: Vec<_> = cues
        .iter()
        .filter(|cue| {
            cue.start_s.is_finite()
                && cue.end_s.is_finite()
                && cue.start_s < window_end
                && cue.end_s > window_start
                && cue.start_s < cue.end_s
        })
        .collect();
    ordered.sort_by(|a, b| a.start_s.total_cmp(&b.start_s));
    let mut events = Vec::with_capacity(ordered.len() * 2);
    for (rank, cue) in ordered.iter().enumerate() {
        events.push((cue.start_s.max(window_start), rank, true));
        events.push((cue.end_s.min(window_end), rank, false));
    }
    events.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    let mut active = std::collections::BTreeSet::new();
    let mut previous_time = window_start;
    for (time, rank, enters) in events {
        if previous_time < time
            && let Some(&visible) = active.last()
        {
            let cue: &super::ActiveCue = ordered[visible];
            if !cue.text.trim().is_empty() {
                if let Some(previous) = selected.last_mut()
                    && previous.end_s == previous_time
                    && previous.text == cue.text
                {
                    previous.end_s = time;
                } else {
                    selected.push(animatix_text::transcript::SubtitleCue {
                        start_s: previous_time,
                        end_s: time,
                        text: cue.text.clone(),
                    });
                }
            }
        }
        if enters {
            active.insert(rank);
        } else {
            active.remove(&rank);
        }
        previous_time = time;
    }
}
