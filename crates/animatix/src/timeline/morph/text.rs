//! Text/Vello path sequence interpolation.

use super::alignment_cache::{AlignmentCache, with_alignment};
use super::interpolate::lerp_color;
use super::*;

/// Interpolate between two `Vec<TextPath>` using morph options.
pub fn interpolate_text_paths(
    source: &Vec<TextPath>,
    target: &Vec<TextPath>,
    t: f32,
    options: MorphOptions,
) -> Vec<TextPath> {
    sample_text_paths(source, target, t, options, None)
}

pub(in crate::timeline) fn sample_text_paths(
    source: &Vec<TextPath>,
    target: &Vec<TextPath>,
    t: f32,
    options: MorphOptions,
    cache: Option<&AlignmentCache>,
) -> Vec<TextPath> {
    if options.strategy == MorphStrategy::Fade {
        if t <= 0.0 {
            return source.clone();
        }
        if t >= 1.0 {
            return target.clone();
        }
        let source_alpha = 1.0 - t;
        let target_alpha = t;
        let mut result = Vec::with_capacity(source.len() + target.len());
        for path in source {
            result.push(TextPath {
                path: path.path.clone(),
                clips: path.clips.clone(),
                color: path.color,
                opacity: path.opacity * source_alpha,
                line_idx: path.line_idx,
                word_idx: path.word_idx,
                char_idx: path.char_idx,
                baseline_y: path.baseline_y,
                glyph_center: path.glyph_center,
                bounds: path.bounds,
            });
        }
        for path in target {
            result.push(TextPath {
                path: path.path.clone(),
                clips: path.clips.clone(),
                color: path.color,
                opacity: path.opacity * target_alpha,
                line_idx: path.line_idx,
                word_idx: path.word_idx,
                char_idx: path.char_idx,
                baseline_y: path.baseline_y,
                glyph_center: path.glyph_center,
                bounds: path.bounds,
            });
        }
        return result;
    }

    with_alignment(
        cache,
        source,
        target,
        |path| &path.path,
        options,
        |paths| {
            paths
                .iter()
                .enumerate()
                .map(|(index, aligned)| {
                    use kurbo::Shape as _;
                    let ref_path = if t < 0.5 {
                        source.get(index)
                    } else {
                        target.get(index)
                    };
                    let morphed = aligned.sample(t as f64, options.path_arc);
                    let b = morphed.bounding_box();
                    TextPath {
                        path: morphed,
                        clips: Vec::new(),
                        color: if t < 0.5 {
                            source.get(index).map(|path| path.color).unwrap_or_else(|| {
                                target
                                    .get(index)
                                    .map(|path| path.color)
                                    .unwrap_or_else(|| [0, 0, 0, 255])
                            })
                        } else {
                            target.get(index).map(|path| path.color).unwrap_or_else(|| {
                                source
                                    .get(index)
                                    .map(|path| path.color)
                                    .unwrap_or_else(|| [0, 0, 0, 255])
                            })
                        },
                        opacity: 1.0,
                        line_idx: ref_path.map(|p| p.line_idx).unwrap_or(0),
                        word_idx: ref_path.map(|p| p.word_idx).unwrap_or(0),
                        char_idx: ref_path.map(|p| p.char_idx).unwrap_or(index as u16),
                        baseline_y: ref_path.map_or(0.0, |p| p.baseline_y),
                        glyph_center: ref_path.map(|p| p.glyph_center).unwrap_or([0.0, 0.0]),
                        bounds: b,
                    }
                })
                .collect()
        },
    )
}

/// Interpolate between two `Vec<VelloPath>` using morph options.
pub fn interpolate_vello_paths(
    source: &Vec<VelloPath>,
    target: &Vec<VelloPath>,
    t: f32,
    options: MorphOptions,
) -> Vec<VelloPath> {
    sample_vello_paths(source, target, t, options, None)
}

pub(in crate::timeline) fn sample_vello_paths(
    source: &Vec<VelloPath>,
    target: &Vec<VelloPath>,
    t: f32,
    options: MorphOptions,
    cache: Option<&AlignmentCache>,
) -> Vec<VelloPath> {
    if options.strategy == MorphStrategy::Fade {
        if t <= 0.0 {
            return source.clone();
        }
        if t >= 1.0 {
            return target.clone();
        }
        let source_alpha = 1.0 - t;
        let target_alpha = t;
        let mut result = Vec::with_capacity(source.len() + target.len());
        for path in source {
            result.push(VelloPath {
                path: path.path.clone(),
                fill: path.fill.map(|c| c.multiply_alpha(source_alpha)),
                stroke: path.stroke.map(|(c, w)| (c.multiply_alpha(source_alpha), w)),
                line_cap: path.line_cap,
                line_join: path.line_join,
                dash_pattern: path.dash_pattern.clone(),
                dash_offset: path.dash_offset,
                fill_gradient: None,
                stroke_gradient: None,
            });
        }
        for path in target {
            result.push(VelloPath {
                path: path.path.clone(),
                fill: path.fill.map(|c| c.multiply_alpha(target_alpha)),
                stroke: path.stroke.map(|(c, w)| (c.multiply_alpha(target_alpha), w)),
                line_cap: path.line_cap,
                line_join: path.line_join,
                dash_pattern: path.dash_pattern.clone(),
                dash_offset: path.dash_offset,
                fill_gradient: None,
                stroke_gradient: None,
            });
        }
        return result;
    }

    with_alignment(
        cache,
        source,
        target,
        |path| path.path.as_ref(),
        options,
        |paths| {
            paths
                .iter()
                .enumerate()
                .map(|(index, aligned)| {
                    let source_element = source.get(index);
                    let target_element = target.get(index);
                    VelloPath {
                        path: std::sync::Arc::new(aligned.sample(t as f64, options.path_arc)),
                        fill: match (
                            source_element.and_then(|e| e.fill),
                            target_element.and_then(|e| e.fill),
                        ) {
                            (Some(c1), Some(c2)) => Some(lerp_color(c1, c2, t)),
                            (Some(c), None) => Some(if t < 0.5 {
                                c
                            } else {
                                peniko::Color::TRANSPARENT
                            }),
                            (None, Some(c)) => Some(if t >= 0.5 {
                                c
                            } else {
                                peniko::Color::TRANSPARENT
                            }),
                            (None, None) => None,
                        },
                        stroke: match (
                            source_element.and_then(|e| e.stroke),
                            target_element.and_then(|e| e.stroke),
                        ) {
                            (Some((c1, w1)), Some((c2, w2))) => {
                                Some((lerp_color(c1, c2, t), w1 + (w2 - w1) * t))
                            },
                            (Some((c, w)), None) => Some((
                                if t < 0.5 {
                                    c
                                } else {
                                    peniko::Color::TRANSPARENT
                                },
                                if t < 0.5 { w } else { 0.0 },
                            )),
                            (None, Some((c, w))) => Some((
                                if t >= 0.5 {
                                    c
                                } else {
                                    peniko::Color::TRANSPARENT
                                },
                                if t >= 0.5 { w } else { 0.0 },
                            )),
                            (None, None) => None,
                        },
                        line_cap: source_element.map(|e| e.line_cap).unwrap_or(0),
                        line_join: source_element.map(|e| e.line_join).unwrap_or(0),
                        dash_pattern: None,
                        dash_offset: 0.0,
                        fill_gradient: None,
                        stroke_gradient: None,
                    }
                })
                .collect()
        },
    )
}
