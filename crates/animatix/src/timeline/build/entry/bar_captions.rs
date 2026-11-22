//! Prebuild category captions so frame evaluation only moves existing text.

use std::collections::{BTreeMap, HashMap};

use crate::diagnostics::Diagnostic;
use crate::primitives::{BarCaptionTransition, BarCaptions};
use crate::timeline::{BarChartLayout, EntityId, Timeline, plot::interpolate_bar_data};

pub(super) fn prepare(timeline: &mut Timeline, diagnostics: &mut Vec<Diagnostic>) {
    let charts: Vec<_> = timeline
        .world
        .tracks
        .iter()
        .filter_map(|(label, id)| {
            let plot = timeline.world.plot_payload(id)?;
            let layout = plot.bar_layout.as_ref()?;
            (layout.has_captions() && !plot.bar_data_transitions.is_empty()).then(|| {
                (
                    label.to_owned(),
                    id,
                    layout.clone(),
                    plot.bar_data.clone(),
                    plot.bar_data_transitions.clone(),
                )
            })
        })
        .collect();
    for (label, id, layout, data, transitions) in charts {
        let time_ms = timeline.world.meta(id).first_seen_ms as f64;
        let mut actors = CaptionActors {
            timeline,
            parent: &label,
            time_ms,
            next_index: data.len(),
            by_key: HashMap::new(),
            diagnostics,
        };
        for (i, ((key, _), anchor)) in
            data.iter().zip(layout.caption_anchors(data.len())).enumerate()
        {
            if let Some(entity) =
                actors.timeline.world.tracks.get_id(&format!("{label}_bar_label_{i}"))
            {
                actors.by_key.entry(key.clone()).or_default().push((entity, anchor));
            }
        }
        let initial = actors.placements(&layout, &data);
        let transitions = transitions
            .iter()
            .map(|transition| {
                let visible = interpolate_bar_data(&transition.from, &transition.to, 0.0);
                let active = actors.placements(&layout, &visible);
                let settled = actors.placements(&layout, &transition.to);
                BarCaptionTransition { active, settled }
            })
            .collect();
        let entities = actors.by_key.values().flatten().map(|(id, _)| *id).collect();
        timeline.world.ensure_plot_payload(id).bar_captions = Some(Box::new(BarCaptions {
            entities,
            initial,
            transitions,
        }));
    }
}

struct CaptionActors<'a> {
    timeline: &'a mut Timeline,
    parent: &'a str,
    time_ms: f64,
    next_index: usize,
    by_key: HashMap<String, Vec<(EntityId, [f64; 2])>>,
    diagnostics: &'a mut Vec<Diagnostic>,
}

impl CaptionActors<'_> {
    fn placements(
        &mut self,
        layout: &BarChartLayout,
        data: &[(String, f32)],
    ) -> BTreeMap<EntityId, [f64; 2]> {
        let mut occurrences = HashMap::<&str, usize>::new();
        data.iter()
            .zip(layout.caption_anchors(data.len()))
            .filter_map(|((key, _), [x, y])| {
                let index = occurrences.entry(key).or_default();
                let captions = self.by_key.entry(key.clone()).or_default();
                if *index == captions.len() {
                    let label = loop {
                        let candidate = format!("{}_bar_label_{}", self.parent, self.next_index);
                        self.next_index += 1;
                        if self.timeline.world.tracks.get_id(&candidate).is_none() {
                            break candidate;
                        }
                    };
                    self.timeline.spawn_plot_label(
                        self.parent,
                        &label,
                        [0.0; 2],
                        key.clone(),
                        self.time_ms,
                        self.diagnostics,
                    );
                    let id = self.timeline.world.tracks.get_id(&label)?;
                    captions.push((id, [0.0; 2]));
                }
                let (id, [origin_x, origin_y]) = captions[*index];
                *index += 1;
                Some((id, [x - origin_x, y - origin_y]))
            })
            .collect()
    }
}
