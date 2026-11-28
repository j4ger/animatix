//! Pixel attribution for the rendered audit tier. Bounds only select candidates;
//! the real compositor decides which pixels each actor contributes.

use animatix::composition::BuildTarget;
use animatix::timeline::SceneDimensions;
use animatix_render::offscreen::{OffscreenRenderer, RenderedFrame};

use crate::frame_source::{
    BOUNDS_PAD, Box2, DEFAULT_TOLERANCE, FrameTarget, FrameView, resolve_frame_target,
};
use crate::review::facts::ReviewFacts;

/// A primitive in the structural draw order, not necessarily visible after compositing.
pub(crate) struct ProbeActor<'a> {
    pub scene: &'a str,
    pub label: &'a str,
    pub bounds: Box2,
    pub opacity: f32,
    pub is_text: bool,
    pub can_omit: bool,
}

impl<'a> ProbeActor<'a> {
    fn key(&self) -> (&'a str, &'a str) {
        (self.scene, self.label)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct OcclusionFact {
    pub scene: String,
    pub label: String,
    /// None when the candidate stack, rather than one isolated layer, was tested.
    pub occluder: Option<String>,
    pub probe_coverage: f64,
    pub probed_points: usize,
    pub covered_points: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct RevealCandidate {
    pub scene: String,
    pub label: String,
    pub hidden_s: f64,
    pub visible_s: f64,
    pub bounds: Box2,
}

/// Select a strongly visible sample after an authored hidden state. The very
/// first nonzero sample of a fade is not evidence of a failed reveal.
pub(crate) fn reveal_candidates(facts: &ReviewFacts) -> Vec<RevealCandidate> {
    let mut out = Vec::new();
    for actor in &facts.actors {
        if !actor.visual
            || actor.is_group_like
            || actor.is_effect_scope
            || actor.is_non_content
            || actor.is_background
            || actor.label.contains("__")
        {
            continue;
        }
        let mut hidden = None;
        let mut shown: Option<(f64, f32, Box2)> = None;
        // Extra structural rule probes are not necessarily rendered.
        for frame in &facts.frames {
            if frame.scene != actor.scene {
                continue;
            }
            let Some(m) = facts.actor_frame(frame, &actor.scene, &actor.label) else {
                continue;
            };
            let (Some(opacity), Some(bounds)) = (m.opacity, m.bounds) else {
                continue;
            };
            if m.padded_on_frame_fraction <= 0.0 {
                continue;
            }
            if opacity <= 0.01 {
                hidden.get_or_insert(frame.time_s);
            } else if hidden.is_some()
                && m.draw_commands.unwrap_or(0) > 0
                && shown.is_none_or(|(_, best, _)| opacity > best)
            {
                shown = Some((frame.time_s, opacity, bounds));
            }
        }
        if let (Some(hidden_s), Some((visible_s, _, bounds))) = (hidden, shown) {
            out.push(RevealCandidate {
                scene: actor.scene.clone(),
                label: actor.label.clone(),
                hidden_s,
                visible_s,
                bounds,
            });
        }
    }
    out
}

/// Cap extra readbacks for large documents. A skipped probe is not evidence.
const MAX_VARIANTS: usize = 96;

pub(crate) struct AuditPaintProbes {
    remaining: usize,
    pub inconclusive: usize,
}

impl Default for AuditPaintProbes {
    fn default() -> Self {
        Self {
            remaining: MAX_VARIANTS,
            inconclusive: 0,
        }
    }
}

impl AuditPaintProbes {
    fn reserve(&mut self, n: usize) -> bool {
        if n > self.remaining {
            self.inconclusive += 1;
            return false;
        }
        self.remaining -= n;
        true
    }
}

/// Keep the production compositor, dimensions, time, and scene inputs.
struct ProbeFrame<'a> {
    target: FrameTarget<'a>,
    time_s: f64,
    dimensions: SceneDimensions,
}

impl ProbeFrame<'_> {
    fn render(
        &self,
        renderer: &mut OffscreenRenderer,
        omit: &[(&str, &str)],
    ) -> Result<RenderedFrame, String> {
        match &self.target {
            FrameTarget::Single { timeline } => {
                let labels: Vec<_> = omit.iter().map(|(_, label)| *label).collect();
                renderer.render_canvas_omitting_paint(
                    timeline,
                    self.time_s,
                    self.dimensions,
                    &labels,
                )
            },
            FrameTarget::Scene {
                scene,
                local_time_s,
            } => {
                let labels: Vec<_> = omit.iter().map(|(_, label)| *label).collect();
                renderer.render_canvas_omitting_paint(
                    &scene.timeline,
                    *local_time_s,
                    self.dimensions,
                    &labels,
                )
            },
            FrameTarget::Transition { from, to, blend } => {
                let labels = |name: &str| {
                    omit.iter()
                        .filter_map(|(scene, label)| (*scene == name).then_some(*label))
                        .collect::<Vec<_>>()
                };
                renderer.render_canvas_transition_omitting_paint(
                    &from.timeline,
                    blend.from_local,
                    &to.timeline,
                    blend.to_local,
                    blend.progress as f32,
                    blend.id.clone(),
                    blend.easing,
                    self.dimensions,
                    (&labels(&from.name), &labels(&to.name)),
                )
            },
        }
    }

    fn shared_plan(&self) -> Option<(&str, &str, animatix_render::transition::SharedActorPlan)> {
        let FrameTarget::Transition { from, to, blend } = &self.target else {
            return None;
        };
        Some((
            &from.name,
            &to.name,
            animatix_render::transition::plan_shared_actors(
                &from.timeline,
                blend.from_local,
                &to.timeline,
                blend.to_local,
            ),
        ))
    }

    fn region(&self, bounds: Box2) -> Box2 {
        if matches!(&self.target, FrameTarget::Transition { .. }) {
            // The compositor may translate/wipe the layers or interpolate an
            // overlay. The original actor's box is not a safe pixel crop.
            Box2::new(0.0, 0.0, self.dimensions.width as f64, self.dimensions.height as f64)
        } else {
            bounds.padded(BOUNDS_PAD)
        }
    }
}

#[derive(Default)]
pub(crate) struct PaintFindings {
    pub occlusions: Vec<OcclusionFact>,
    pub missed_reveals: Vec<RevealCandidate>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn inspect_paint(
    budget: &mut AuditPaintProbes,
    renderer: &mut OffscreenRenderer,
    target: &BuildTarget,
    time_s: f64,
    dimensions: SceneDimensions,
    frame: &FrameView<'_>,
    order: &[ProbeActor<'_>],
    reveals: &[RevealCandidate],
) -> Result<PaintFindings, String> {
    let context = ProbeFrame {
        target: resolve_frame_target(target, time_s)?,
        time_s,
        dimensions,
    };
    let mut missed_reveals = Vec::new();
    for candidate in reveals.iter().filter(|c| c.visible_s == time_s) {
        if !budget.reserve(1) {
            continue;
        }
        let key = (candidate.scene.as_str(), candidate.label.as_str());
        let omitted = match context.render(renderer, &[key]) {
            Ok(frame) => frame,
            Err(error) => {
                tracing::debug!(%error, "inconclusive audit reveal paint comparison");
                budget.inconclusive += 1;
                continue;
            },
        };
        if crate::frame_source::region_differing_fraction(
            frame,
            &view(&omitted)?,
            context.region(candidate.bounds),
            DEFAULT_TOLERANCE,
        ) == 0.0
        {
            missed_reveals.push(candidate.clone());
        }
    }
    let occlusions = inspect_occlusions(budget, renderer, &context, frame, order)?;
    Ok(PaintFindings {
        occlusions,
        missed_reveals,
    })
}

fn intersects(a: Box2, b: Box2) -> bool {
    a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1
}

fn inspect_occlusions(
    budget: &mut AuditPaintProbes,
    renderer: &mut OffscreenRenderer,
    context: &ProbeFrame<'_>,
    frame: &FrameView<'_>,
    order: &[ProbeActor<'_>],
) -> Result<Vec<OcclusionFact>, String> {
    let shared = context.shared_plan();
    let mut out = Vec::new();
    for (index, text) in order
        .iter()
        .enumerate()
        .filter(|(_, a)| a.can_omit && a.is_text && a.opacity > 0.05)
    {
        // Frame parts can contain both names for a single shared overlay.
        // Omitting either name removes that whole overlay, never treat the
        // second name as a potential blocker of the first.
        let alias = shared.as_ref().and_then(|(from, to, plan)| {
            plan.pairs.iter().find_map(|(a, b)| {
                if text.key() == (*from, a.as_str()) {
                    Some((*to, b.as_str()))
                } else if text.key() == (*to, b.as_str()) {
                    Some((*from, a.as_str()))
                } else {
                    None
                }
            })
        });
        let mut blockers: Vec<_> = order[index + 1..]
            .iter()
            .filter(|candidate| {
                candidate.can_omit
                    && candidate.opacity > 0.5
                    && Some(candidate.key()) != alias
                    && intersects(text.bounds, candidate.bounds)
            })
            .map(ProbeActor::key)
            .collect();
        if blockers.is_empty() || !budget.reserve(3) {
            continue;
        }
        let variant = (|| {
            let without_text = context.render(renderer, &[text.key()])?;
            let without_blockers = context.render(renderer, &blockers)?;
            blockers.push(text.key());
            let without_both = context.render(renderer, &blockers)?;
            blockers.pop();
            Ok::<_, String>((without_text, without_blockers, without_both))
        })();
        let (without_text, without_blockers, without_both) = match variant {
            Ok(frames) => frames,
            Err(error) => {
                tracing::debug!(%error, "inconclusive audit occlusion paint comparison");
                budget.inconclusive += 1;
                continue;
            },
        };
        let (probed, covered) = measure_hidden_glyphs(
            frame,
            &view(&without_text)?,
            &view(&without_blockers)?,
            &view(&without_both)?,
            context.region(text.bounds),
        );
        if covered > 0 {
            out.push(OcclusionFact {
                scene: text.scene.into(),
                label: text.label.into(),
                occluder: (blockers.len() == 1).then(|| blockers[0].1.into()),
                probe_coverage: covered as f64 / probed as f64,
                probed_points: probed,
                covered_points: covered,
            });
        }
    }
    Ok(out)
}

fn view(frame: &RenderedFrame) -> Result<FrameView<'_>, String> {
    FrameView::new(frame.width, frame.height, &frame.rgba)
        .ok_or_else(|| "paint probe frame buffer has an unexpected size".to_string())
}

fn delta(a: [u8; 4], b: [u8; 4]) -> f64 {
    let max_channel = a.into_iter().zip(b).take(3).map(|(x, y)| x.abs_diff(y)).max().unwrap_or(0);
    f64::from(max_channel) / 255.0
}

fn measure_hidden_glyphs(
    full: &FrameView<'_>,
    no_text: &FrameView<'_>,
    no_blockers: &FrameView<'_>,
    neither: &FrameView<'_>,
    region: Box2,
) -> (usize, usize) {
    let Some((x0, y0, x1, y1)) = region.clamped(full.width(), full.height()) else {
        return (0, 0);
    };
    let mut probed = 0;
    let mut covered = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            // Subtract the text's own signal with AND without the candidates.
            // Comparing the two full colors directly credits a rectangle's
            // fill as if it were text, and misses same-color blockers.
            let restored = delta(no_blockers.pixel(x, y), neither.pixel(x, y));
            if restored > DEFAULT_TOLERANCE {
                probed += 1;
                if delta(full.pixel(x, y), no_text.pixel(x, y)) <= DEFAULT_TOLERANCE {
                    covered += 1;
                }
            }
        }
    }
    (probed, covered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_recovered_glyphs_not_the_blockers_color() {
        let dark = [24, 24, 24, 255];
        let light = [250, 250, 250, 255];
        let red = [200, 0, 0, 255];
        let visible = [light, dark].concat();
        let under = [dark, dark].concat();
        let rect = [red, red].concat();
        let frame = |rgba| FrameView::new(2, 1, rgba).unwrap();
        let bounds = Box2::new(0., 0., 2., 1.);
        let check = |full, no_text| {
            measure_hidden_glyphs(
                &frame(full),
                &frame(no_text),
                &frame(&visible),
                &frame(&under),
                bounds,
            )
        };
        assert_eq!(check(&under, &under), (1, 1));
        assert_eq!(check(&rect, &rect), (1, 1));
        assert_eq!(check(&visible, &under), (1, 0));
    }
}
