use std::sync::Arc;

use animatix_text::CachedGroupedText;
use kurbo::{Affine, Vec2};

use super::{EntityId, TextPath, Timeline};
use crate::primitives::RenderCommand;
use crate::renderer::error::RenderError;

const PROPERTY: &str = "equation_source";
const TERM_HELPER: &str = r#"#let term(key, body) = {
  metadata("animatix:equation-fragment:start:name:" + key)
  body
  metadata("animatix:equation-fragment:end:name:" + key)
}
"#;

impl Timeline {
    pub(super) fn has_equation_source(&self, id: EntityId) -> bool {
        crate::timeline::property_id(PROPERTY)
            .is_some_and(|key| self.world.property_plan(id).keyframe_count(key) > 0)
    }

    pub(super) fn equation_source(&self, id: EntityId, time: u64) -> String {
        self.world.property_plan(id).sample_string_prop(PROPERTY, time, "")
    }

    pub(super) fn compile_source_equation(
        &self,
        id: EntityId,
        time: u64,
        source: &str,
    ) -> Result<Arc<CachedGroupedText>, RenderError> {
        let source = source.trim();
        if source.is_empty() {
            return Ok(Arc::new(CachedGroupedText {
                glyphs: Arc::from([]),
                ranges: Arc::from([]),
                names: Arc::from([]),
            }));
        }
        let body = source.strip_prefix('$').and_then(|s| s.strip_suffix('$')).unwrap_or(source);
        self.compile_equation_markup(id, time, &format!("{TERM_HELPER}$ {body} $"))
    }

    pub(super) fn render_source_equation(
        &self,
        id: EntityId,
        time: u64,
        transform: Affine,
        opacity: f32,
        scene: &mut crate::scene::Scene,
    ) {
        let source = self.equation_source(id, time);
        let paths = match self.compile_source_equation(id, time, &source) {
            Ok(compiled) => {
                self.previous_source(id, time, &source)
                    .and_then(|(previous, progress)| {
                        // The build validates all authored sources. If an older source
                        // still fails at a sampled typography, keep the current frame.
                        match self.compile_source_equation(id, time, &previous) {
                            Ok(previous) => {
                                Some(tween_formula(&previous, &compiled, progress).into())
                            },
                            Err(error) => {
                                tracing::warn!(
                                    "could not compile previous equation_source: {error}"
                                );
                                None
                            },
                        }
                    })
                    .unwrap_or_else(|| Arc::clone(&compiled.glyphs))
            },
            Err(error) => {
                use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
                self.eval_caches.runtime_diagnostics.borrow_mut().push(
                    Diagnostic::error(
                        DiagnosticCode::RenderFailure,
                        DiagnosticPhase::Render,
                        error.to_string(),
                    )
                    .with_subject(self.world.label(id).unwrap_or("Equation")),
                );
                return;
            },
        };
        RenderCommand::Text {
            paths,
            fill_gradient: None,
            clip_path: None,
            tint_color: None,
            hardware_layout: None,
        }
        .execute(scene, &transform, opacity);
    }

    fn previous_source(&self, id: EntityId, time: u64, source: &str) -> Option<(String, f64)> {
        let plan = self.world.property_plan(id);
        let duration =
            (f64::from(plan.sample_f32_prop("equation_transition", time, 0.0)) * 1000.0) as u64;
        if duration == 0 {
            return None;
        }
        let key = crate::timeline::property_id(PROPERTY)?;
        let seen = self.world.first_seen(id);
        plan.keyframe_times(key)
            .into_iter()
            .rev()
            .filter(|&change| change > seen && change <= time && time - change < duration)
            .find_map(|change| {
                let previous = self.equation_source(id, change - 1);
                (previous != source).then_some((previous, (time - change) as f64 / duration as f64))
            })
    }
}

fn tween_formula(from: &CachedGroupedText, to: &CachedGroupedText, progress: f64) -> Vec<TextPath> {
    let mut out = Vec::with_capacity(from.glyphs.len().max(to.glyphs.len()));
    for (old_index, name) in from.names.iter().enumerate() {
        let old = group(from, old_index);
        if let Some(index) = to.names.iter().position(|next| next == name) {
            let new = group(to, index);
            let offset = center(old).zip(center(new)).map_or(Vec2::ZERO, |(a, b)| b - a);
            let same = !old.is_empty()
                && old.len() == new.len()
                && old.iter().zip(new).all(|(a, b)| {
                    a.clips.is_empty()
                        && b.clips.is_empty()
                        && a.color == b.color
                        && a.outline_matches(b, Affine::translate(offset), 0.01)
                });
            if same {
                append(&mut out, old, offset * progress, 1.0);
            } else {
                append(&mut out, old, offset * progress, 1.0 - progress);
                append(&mut out, new, -offset * (1.0 - progress), progress);
            }
        } else {
            append(&mut out, old, Vec2::ZERO, 1.0 - progress);
        }
    }
    for (new_index, name) in to.names.iter().enumerate() {
        if !from.names.contains(name) {
            append(&mut out, group(to, new_index), Vec2::ZERO, progress);
        }
    }
    append_unmarked(&mut out, from, 1.0 - progress);
    append_unmarked(&mut out, to, progress);
    out
}

fn group(formula: &CachedGroupedText, index: usize) -> &[TextPath] {
    &formula.glyphs[formula.ranges[index].clone()]
}

fn center(group: &[TextPath]) -> Option<kurbo::Point> {
    group
        .iter()
        .map(|glyph| glyph.bounds)
        .reduce(|a, b| a.union(b))
        .map(|ink| ink.center())
}

fn append_unmarked(out: &mut Vec<TextPath>, formula: &CachedGroupedText, opacity: f64) {
    let mut start = 0;
    for range in formula.ranges.iter().take(formula.names.len()) {
        append(out, &formula.glyphs[start..range.start], Vec2::ZERO, opacity);
        start = range.end;
    }
    append(out, &formula.glyphs[start..], Vec2::ZERO, opacity);
}

fn append(out: &mut Vec<TextPath>, group: &[TextPath], offset: Vec2, opacity: f64) {
    if opacity > 0.0 {
        out.extend(group.iter().cloned().map(|mut glyph| {
            glyph.translate(offset);
            glyph.opacity *= opacity as f32;
            glyph
        }));
    }
}
