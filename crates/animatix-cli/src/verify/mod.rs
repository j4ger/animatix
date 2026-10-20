use super::BuildTarget;
use animatix::timeline::{DebugRenderOptions, SceneDimensions};
use animatix::verify::{self as animatix_verify, Box2, FrameView};
use std::path::Path;

pub(crate) struct VerifyCheck {
    kind: VerifyKind,
    time_s: f64,
    second_time_s: Option<f64>,
    label: Option<String>,
    threshold: Option<f64>,
    pub line: usize,
    pub raw: String,
}

/// The four supported check kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VerifyKind {
    /// The actor put ink on screen inside its evaluated bounds.
    Visible,
    /// The actor did not (e.g. a hidden-by-default declaration never revealed).
    Invisible,
    /// The whole frame is not blank.
    Ink,
    /// Two times render differently — the animation actually animates.
    Differs,
    /// The actor's region changed between a hidden time and a shown time — the
    /// robust "did it actually appear?" check for actors whose bounds contain
    /// other painted content (e.g. a curve inside a Graph that paints axes).
    Reveals,
}

fn verify_error(path: &Path, line_no: usize, line: &str, msg: &str) -> String {
    format!("{}:{line_no}: {msg} (in `{line}`)", path.display())
}

/// Parse a line-based checks file. One check per line following the grammar
/// documented in `dogfood/README.md`; blank lines and `#` comments are ignored.
pub(crate) fn parse_verify_checks(path: &Path) -> Result<Vec<VerifyCheck>, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("Cannot read checks file {}: {e}", path.display()))?;
    let mut checks = Vec::new();
    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line_no = idx + 1;
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let parse_time = |token: &str| -> Result<f64, String> {
            token.parse::<f64>().map_err(|_| {
                verify_error(path, line_no, line, &format!("`{token}` is not a number"))
            })
        };
        let mut check = VerifyCheck {
            kind: match tokens[0] {
                "visible" => VerifyKind::Visible,
                "invisible" => VerifyKind::Invisible,
                "ink" => VerifyKind::Ink,
                "differs" => VerifyKind::Differs,
                "reveals" => VerifyKind::Reveals,
                other => {
                    return Err(verify_error(
                        path,
                        line_no,
                        line,
                        &format!(
                            "unknown check kind `{other}` (expected visible|invisible|reveals|ink|differs)"
                        ),
                    ));
                },
            },
            time_s: 0.0,
            second_time_s: None,
            label: None,
            threshold: None,
            line: line_no,
            raw: line.to_string(),
        };
        match check.kind {
            VerifyKind::Visible | VerifyKind::Invisible => {
                if tokens.len() != 3 {
                    return Err(verify_error(
                        path,
                        line_no,
                        line,
                        "expected `<kind> <time_s> <actor-label>`",
                    ));
                }
                check.time_s = parse_time(tokens[1])?;
                check.label = Some(tokens[2].to_string());
            },
            VerifyKind::Ink => {
                if tokens.len() != 3 {
                    return Err(verify_error(
                        path,
                        line_no,
                        line,
                        "expected `ink <time_s> <min_frame_ink_fraction>`",
                    ));
                }
                check.time_s = parse_time(tokens[1])?;
                check.threshold = Some(tokens[2].parse().map_err(|_| {
                    verify_error(path, line_no, line, &format!("`{}` is not a number", tokens[2]))
                })?);
            },
            VerifyKind::Differs => {
                if tokens.len() < 3 || tokens.len() > 4 {
                    return Err(verify_error(
                        path,
                        line_no,
                        line,
                        "expected `differs <t1> <t2> [min_changed_fraction]`",
                    ));
                }
                check.time_s = parse_time(tokens[1])?;
                check.second_time_s = Some(parse_time(tokens[2])?);
                if let Some(token) = tokens.get(3) {
                    check.threshold = Some(token.parse().map_err(|_| {
                        verify_error(path, line_no, line, &format!("`{token}` is not a number"))
                    })?);
                }
            },
            VerifyKind::Reveals => {
                if tokens.len() < 4 || tokens.len() > 5 {
                    return Err(verify_error(
                        path,
                        line_no,
                        line,
                        "expected `reveals <actor-label> <t_before> <t_after> [min_changed_fraction]`",
                    ));
                }
                check.label = Some(tokens[1].to_string());
                check.time_s = parse_time(tokens[2])?;
                check.second_time_s = Some(parse_time(tokens[3])?);
                if let Some(token) = tokens.get(4) {
                    check.threshold = Some(token.parse().map_err(|_| {
                        verify_error(path, line_no, line, &format!("`{token}` is not a number"))
                    })?);
                }
            },
        }
        checks.push(check);
    }
    if checks.is_empty() {
        return Err(format!("{}: no checks found", path.display()));
    }
    Ok(checks)
}

/// A rendered verification frame: pixels plus the per-actor bounds from the
/// same evaluation.
struct VerifyFrame {
    frame: animatix_render::offscreen::RenderedFrame,
    bounds: std::collections::HashMap<String, Box2>,
}

fn verify_time_key(time_s: f64) -> u64 {
    (time_s * 1000.0).round() as u64
}

/// Render one frame at global `time_s`, resolving the active scene (and any
/// transition blend) for multi-scene files.
fn render_verify_frame(
    renderer: &mut animatix_render::offscreen::OffscreenRenderer,
    target: &BuildTarget,
    dimensions: SceneDimensions,
    time_s: f64,
) -> Result<VerifyFrame, String> {
    match target {
        BuildTarget::SingleScene(timeline) => {
            let (frame, program) = renderer.render_timeline_observable(
                timeline,
                time_s,
                dimensions,
                DebugRenderOptions::default(),
            )?;
            Ok(VerifyFrame {
                frame,
                bounds: animatix_verify::bounds_map(&program),
            })
        },
        BuildTarget::MultiScene(composition) => {
            if !composition.has_scenes() {
                return Err("Composition has no scenes to render".into());
            }
            let (scene_name, local_time_s, blend) = composition.evaluate(time_s);
            if let Some(blend) = blend {
                let from_scene = composition
                    .scenes
                    .get(&blend.from_scene)
                    .ok_or_else(|| format!("From scene '{}' not found", blend.from_scene))?;
                let to_scene = composition
                    .scenes
                    .get(&blend.to_scene)
                    .ok_or_else(|| format!("To scene '{}' not found", blend.to_scene))?;
                let frame = renderer.render_transition(
                    &from_scene.timeline,
                    blend.from_local,
                    &to_scene.timeline,
                    blend.to_local,
                    blend.progress as f32,
                    blend.id.clone(),
                    blend.easing,
                    dimensions,
                    DebugRenderOptions::default(),
                )?;
                // The compositor does not hand back bounds, so a `visible`
                // check during a blend consults both scenes and accepts a hit
                // in either (the actor may be in either side of the crossfade).
                let mut fb = None;
                let from_program = from_scene.timeline.evaluate_program_with_debug(
                    blend.from_local,
                    dimensions,
                    DebugRenderOptions::default(),
                    &mut fb,
                );
                let mut bounds = animatix_verify::bounds_map(&from_program);
                let mut fb = None;
                let to_program = to_scene.timeline.evaluate_program_with_debug(
                    blend.to_local,
                    dimensions,
                    DebugRenderOptions::default(),
                    &mut fb,
                );
                for (label, rect) in animatix_verify::bounds_map(&to_program) {
                    bounds.entry(label).or_insert(rect);
                }
                Ok(VerifyFrame { frame, bounds })
            } else {
                let scene = composition
                    .scenes
                    .get(&scene_name)
                    .ok_or_else(|| format!("Scene '{scene_name}' not found"))?;
                let (frame, program) = renderer.render_timeline_observable(
                    &scene.timeline,
                    local_time_s,
                    dimensions,
                    DebugRenderOptions::default(),
                )?;
                Ok(VerifyFrame {
                    frame,
                    bounds: animatix_verify::bounds_map(&program),
                })
            }
        },
    }
}

/// Result of running one check.
pub(crate) struct VerifyOutcome {
    pub line: usize,
    pub raw: String,
    pub passed: bool,
    pub detail: String,
}

/// Render every time a check needs (once) and evaluate all checks against the
/// cached frames.
pub(crate) fn run_verify_checks(
    renderer: &mut animatix_render::offscreen::OffscreenRenderer,
    target: &BuildTarget,
    checks: &[VerifyCheck],
    dimensions: SceneDimensions,
) -> Result<Vec<VerifyOutcome>, String> {
    let mut cache: std::collections::HashMap<u64, VerifyFrame> = std::collections::HashMap::new();
    for check in checks {
        for time_s in std::iter::once(check.time_s).chain(check.second_time_s) {
            let key = verify_time_key(time_s);
            if let std::collections::hash_map::Entry::Vacant(slot) = cache.entry(key) {
                slot.insert(render_verify_frame(renderer, target, dimensions, time_s)?);
            }
        }
    }

    let mut outcomes = Vec::with_capacity(checks.len());
    for check in checks {
        let entry = cache
            .get(&verify_time_key(check.time_s))
            .expect("frame rendered in the pass above");
        let view = FrameView::new(entry.frame.width, entry.frame.height, &entry.frame.rgba)
            .ok_or_else(|| "rendered frame buffer has an unexpected size".to_string())?;
        let reference = view.modal_color();
        let (passed, detail) = match check.kind {
            VerifyKind::Visible | VerifyKind::Invisible => {
                let label = check.label.as_deref().unwrap_or_default();
                let bounds = entry.bounds.get(label).copied();
                let visibility = animatix_verify::actor_visibility(
                    &view,
                    bounds,
                    reference,
                    animatix_verify::DEFAULT_TOLERANCE,
                    animatix_verify::DEFAULT_MIN_INK_PIXELS,
                );
                let want_visible = check.kind == VerifyKind::Visible;
                let detail = match bounds {
                    Some(bounds) => format!(
                        "`{label}` ink {} px ({:.2}%) in {:.0}x{:.0}px bounds",
                        visibility.ink_pixels,
                        visibility.ink_fraction * 100.0,
                        bounds.x1 - bounds.x0,
                        bounds.y1 - bounds.y0
                    ),
                    None => format!("`{label}` was not evaluated this frame (no bounds)"),
                };
                (want_visible == visibility.visible, detail)
            },
            VerifyKind::Ink => {
                let fraction =
                    animatix_verify::frame_ink_fraction(&view, animatix_verify::DEFAULT_TOLERANCE);
                let min = check.threshold.unwrap_or(0.005);
                (
                    f64::from(fraction) >= min,
                    format!("frame ink {:.3}% (min {:.3}%)", fraction * 100.0, min * 100.0),
                )
            },
            VerifyKind::Differs => {
                let second = check.second_time_s.expect("differs checks carry a second time");
                let other =
                    cache.get(&verify_time_key(second)).expect("frame rendered in the pass above");
                let other_view =
                    FrameView::new(other.frame.width, other.frame.height, &other.frame.rgba)
                        .ok_or_else(|| {
                            "rendered frame buffer has an unexpected size".to_string()
                        })?;
                let changed = animatix_verify::differing_fraction(
                    &view,
                    &other_view,
                    animatix_verify::DEFAULT_TOLERANCE,
                );
                let min = check.threshold.unwrap_or(0.001);
                (
                    f64::from(changed) >= min,
                    format!(
                        "{:.2}% of pixels changed between {:.3}s and {:.3}s (min {:.2}%)",
                        changed * 100.0,
                        check.time_s,
                        second,
                        min * 100.0
                    ),
                )
            },
            VerifyKind::Reveals => {
                let label = check.label.as_deref().unwrap_or_default();
                let after_time = check.second_time_s.expect("reveals checks carry a second time");
                let after = cache
                    .get(&verify_time_key(after_time))
                    .expect("frame rendered in the pass above");
                let after_view =
                    FrameView::new(after.frame.width, after.frame.height, &after.frame.rgba)
                        .ok_or_else(|| {
                            "rendered frame buffer has an unexpected size".to_string()
                        })?;
                let min = check.threshold.unwrap_or(0.01);
                match after.bounds.get(label).copied() {
                    Some(bounds) => {
                        let bounds: Box2 = bounds;
                        let region = bounds.padded(animatix_verify::BOUNDS_PAD);
                        let changed = animatix_verify::region_differing_fraction(
                            &view,
                            &after_view,
                            region,
                            animatix_verify::DEFAULT_TOLERANCE,
                        );
                        (
                            f64::from(changed) >= min,
                            format!(
                                "`{label}` region changed {:.2}% between {:.3}s and {:.3}s (min {:.2}%)",
                                changed * 100.0,
                                check.time_s,
                                after_time,
                                min * 100.0
                            ),
                        )
                    },
                    None => (
                        false,
                        format!(
                            "`{label}` has no bounds at {after_time:.3}s; cannot verify reveal"
                        ),
                    ),
                }
            },
        };
        outcomes.push(VerifyOutcome {
            line: check.line,
            raw: check.raw.clone(),
            passed,
            detail,
        });
    }
    Ok(outcomes)
}
