use super::BuildTarget;
use animatix::timeline::SceneDimensions;
use std::path::Path;

use crate::frame_source::{
    BOUNDS_PAD, Box2, DEFAULT_MIN_INK_PIXELS, DEFAULT_TOLERANCE, FrameView, ObservableFrame,
    actor_visibility, differing_fraction, frame_ink_fraction, region_differing_fraction,
    render_frame_observable, verify_time_key,
};

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
    let mut cache: std::collections::HashMap<u64, ObservableFrame> =
        std::collections::HashMap::new();
    for check in checks {
        for time_s in std::iter::once(check.time_s).chain(check.second_time_s) {
            let key = verify_time_key(time_s);
            if let std::collections::hash_map::Entry::Vacant(slot) = cache.entry(key) {
                slot.insert(render_frame_observable(renderer, target, dimensions, time_s)?);
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
                let visibility = actor_visibility(
                    &view,
                    bounds,
                    reference,
                    DEFAULT_TOLERANCE,
                    DEFAULT_MIN_INK_PIXELS,
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
                let fraction = frame_ink_fraction(&view, DEFAULT_TOLERANCE);
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
                let changed = differing_fraction(&view, &other_view, DEFAULT_TOLERANCE);
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
                        let region = bounds.padded(BOUNDS_PAD);
                        let changed = region_differing_fraction(
                            &view,
                            &after_view,
                            region,
                            DEFAULT_TOLERANCE,
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
