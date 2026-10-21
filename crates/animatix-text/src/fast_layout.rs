use animatix_core::error::RenderError;
use kurbo::{Affine, BezPath, Shape};

use crate::font_context::{BUNDLED_FONTS, FontContext, bundled_face, resolve_font_family};
use crate::typst_world::{PathBuilder, center_text_paths, is_cjk_char};
use crate::{CompiledText, TextMetrics, TextPath, f32_color_to_rgba8};

/// Font face with both `ttf-parser` and `rustybuzz` representations for fast-path shaping.
pub struct FastPathFace {
    pub ttf: ttf_parser::Face<'static>,
    pub rb: rustybuzz::Face<'static>,
    pub family: String,
    pub font_data: &'static [u8],
    pub font_index: u32,
}

impl FastPathFace {
    pub fn from_data(data: &'static [u8], index: u32, family: String) -> Option<Self> {
        let ttf = ttf_parser::Face::parse(data, index).ok()?;
        let rb = rustybuzz::Face::from_slice(data, index)?;
        Some(Self {
            ttf,
            rb,
            family,
            font_data: data,
            font_index: index,
        })
    }
}

/// A compact hardware glyph descriptor for direct Vello hardware glyph run rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct HardwareGlyph {
    /// Font glyph ID.
    pub id: u32,
    /// X position in local scene coordinates (points) relative to text center.
    pub x: f32,
    /// Y position in local scene coordinates (points) relative to text center.
    pub y: f32,
}

/// A contiguous run of glyphs sharing the same font face and font size.
#[derive(Debug, Clone)]
pub struct HardwareGlyphRun {
    /// Pointer to static font bytes.
    pub font_data: &'static [u8],
    /// Font index inside font collection (ttc).
    pub font_index: u32,
    /// Font size in points.
    pub font_size: f32,
    /// Stream of positioned glyphs.
    pub glyphs: Vec<HardwareGlyph>,
}

/// A complete hardware glyph layout comprised of one or more runs (supporting font fallback).
#[derive(Debug, Clone, Default)]
pub struct HardwareTextLayout {
    /// Sequence of runs in draw order.
    pub runs: Vec<HardwareGlyphRun>,
}

struct HardwarePendingGlyph {
    face_idx: usize,
    id: u32,
    x: f32,
    y: f32,
}

struct ShapedGlyph {
    face_idx: usize,
    glyph_id: u16,
    x_offset: f64,
    y_offset: f64,
}

fn shape_token(
    token_text: &str,
    faces: &[FastPathFace],
    size: f32,
    letter_spacing: f32,
) -> (f64, Vec<ShapedGlyph>) {
    if token_text.is_empty() || faces.is_empty() {
        return (0.0, Vec::new());
    }

    let mut best_face_idx = 0;
    let mut best_missing = usize::MAX;
    let mut best_buffer: Option<rustybuzz::GlyphBuffer> = None;

    for (idx, face) in faces.iter().enumerate() {
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(token_text);
        let shaped = rustybuzz::shape(&face.rb, &[], buffer);
        let missing_count = shaped.glyph_infos().iter().filter(|g| g.glyph_id == 0).count();
        if missing_count == 0 {
            best_face_idx = idx;
            best_buffer = Some(shaped);
            break;
        }
        if missing_count < best_missing {
            best_missing = missing_count;
            best_face_idx = idx;
            best_buffer = Some(shaped);
        }
    }

    let shaped = match best_buffer {
        Some(b) => b,
        None => {
            let mut buffer = rustybuzz::UnicodeBuffer::new();
            buffer.push_str(token_text);
            rustybuzz::shape(&faces[0].rb, &[], buffer)
        },
    };

    let face = &faces[best_face_idx];
    let units_per_em = face.ttf.units_per_em() as f32;
    let font_scale = (size / units_per_em) as f64;
    let letter_spacing_f64 = letter_spacing as f64;

    let infos = shaped.glyph_infos();
    let positions = shaped.glyph_positions();

    let mut glyphs = Vec::with_capacity(infos.len());
    let mut total_width = 0.0f64;

    for (info, pos) in infos.iter().zip(positions.iter()) {
        if info.glyph_id == 0 && token_text.trim().is_empty() {
            total_width += pos.x_advance as f64 * font_scale + letter_spacing_f64;
            continue;
        }
        let x_adv = pos.x_advance as f64 * font_scale + letter_spacing_f64;
        let x_off = pos.x_offset as f64 * font_scale;
        let y_off = pos.y_offset as f64 * font_scale;

        glyphs.push(ShapedGlyph {
            face_idx: best_face_idx,
            glyph_id: info.glyph_id as u16,
            x_offset: total_width + x_off,
            y_offset: y_off,
        });
        total_width += x_adv;
    }

    (total_width, glyphs)
}

/// Compile plain text into glyph paths using `rustybuzz` and `ttf_parser` directly,
/// bypassing Typst entirely. This is the fast path.
///
/// The returned paths use the same coordinate convention as `extract_glyphs`
/// (centered around origin), so `measure_text_paths` works unchanged.
pub fn compile_text_fast(
    content: &str,
    family: &str,
    weight: f32,
    style: &str,
    size: f32,
    color: [f32; 4],
    letter_spacing: f32,
    word_spacing: f32,
    font_ctx: &FontContext,
) -> Result<CompiledText, RenderError> {
    // Early return for empty string: no glyphs, zero metrics
    if content.is_empty() {
        return Ok(CompiledText {
            glyphs: Vec::new(),
            ascent: 0.0,
            descent: 0.0,
            baseline_offset: 0.0,
            hardware_layout: None,
        });
    }

    if content.contains('\n') {
        return compile_text_fast_wrapped(
            content,
            family,
            weight,
            style,
            size,
            1.2,
            color,
            letter_spacing,
            word_spacing,
            font_ctx,
            0.0,
            None,
            "left",
            "visible",
        );
    }

    let resolved_family = resolve_font_family(family, font_ctx);
    let (primary_data, primary_index) = 'font: {
        if let Some(bf) = bundled_face(&resolved_family, weight, style) {
            break 'font (bf.data, 0);
        }
        if let Some((data, idx)) = font_ctx.load_face_data(&resolved_family, weight, style) {
            break 'font (data, idx);
        }
        return Err(RenderError::TextCompilation(format!(
            "Failed to load font '{}' (weight={}, style={}) for fast path",
            resolved_family, weight, style
        )));
    };

    let Some(primary_face) =
        FastPathFace::from_data(primary_data, primary_index, resolved_family.clone())
    else {
        return Err(RenderError::TextCompilation(format!(
            "Failed to parse font '{}' (weight={}, style={}) for fast path",
            resolved_family, weight, style
        )));
    };

    let units_per_em = primary_face.ttf.units_per_em() as f32;
    let font_scale = size / units_per_em;
    let ascent = primary_face.ttf.ascender() as f32;
    let descent = primary_face.ttf.descender() as f32;

    let mut faces = vec![primary_face];
    faces.extend(
        BUNDLED_FONTS
            .iter()
            .filter(|bf| bf.family != resolved_family)
            .filter_map(|bf| FastPathFace::from_data(bf.data, 0, bf.family.to_string())),
    );

    let rgba = f32_color_to_rgba8(&color);

    let mut char_indices = content.char_indices().peekable();
    let mut pending_space = false;
    let mut word_start: Option<usize> = None;
    let mut raw_tokens: Vec<(&str, bool)> = Vec::new();

    while let Some((idx, ch)) = char_indices.next() {
        if ch == ' ' || ch == '\t' {
            if let Some(start) = word_start.take() {
                raw_tokens.push((&content[start..idx], pending_space));
            }
            pending_space = true;
        } else if is_cjk_char(ch) {
            if let Some(start) = word_start.take() {
                raw_tokens.push((&content[start..idx], pending_space));
                pending_space = false;
            }
            let next_idx = char_indices.peek().map(|(i, _)| *i).unwrap_or(content.len());
            raw_tokens.push((&content[idx..next_idx], pending_space));
            pending_space = false;
        } else if word_start.is_none() {
            word_start = Some(idx);
        }
    }
    if let Some(start) = word_start {
        raw_tokens.push((&content[start..], pending_space));
    }

    let space_advance = {
        let (w, _) = shape_token(" ", &faces, size, letter_spacing);
        if w > 0.0 {
            w + word_spacing as f64
        } else {
            (size as f64 * 0.25) + word_spacing as f64
        }
    };

    let mut glyphs: Vec<TextPath> = Vec::new();
    let mut x_curr: f64 = 0.0;
    let mut pending_hardware: Vec<HardwarePendingGlyph> = Vec::new();

    for (wi_idx, (w, space_before)) in raw_tokens.into_iter().enumerate() {
        if wi_idx > 0 && space_before {
            x_curr += space_advance;
        }
        let (token_width, shaped_glyphs) = shape_token(w, &faces, size, letter_spacing);
        for g in shaped_glyphs {
            let face = &faces[g.face_idx];
            let glyph_scale = size / (face.ttf.units_per_em() as f32);

            pending_hardware.push(HardwarePendingGlyph {
                face_idx: g.face_idx,
                id: g.glyph_id as u32,
                x: (x_curr + g.x_offset) as f32,
                y: g.y_offset as f32,
            });

            let mut builder = PathBuilder(BezPath::new());
            if face.ttf.outline_glyph(ttf_parser::GlyphId(g.glyph_id), &mut builder).is_some() {
                let path = builder.0;
                let scale_affine =
                    Affine::scale_non_uniform(glyph_scale as f64, -glyph_scale as f64);
                let translate =
                    Affine::translate(kurbo::Vec2::new(x_curr + g.x_offset, g.y_offset));
                let final_affine = translate * scale_affine;
                let mut final_path = path;
                final_path.apply_affine(final_affine);

                let c_idx = glyphs.len() as u16;
                let bounds = final_path.bounding_box();
                glyphs.push(TextPath {
                    path: final_path,
                    color: rgba,
                    opacity: 1.0,
                    line_idx: 0,
                    word_idx: wi_idx as u16,
                    char_idx: c_idx,
                    glyph_center: [0.0, 0.0],
                    bounds,
                });
            }
        }
        x_curr += token_width;
    }

    // Compute bounding box BEFORE centering to find center_x and center_y
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for path in glyphs.iter() {
        let b = path.bounds;
        min_x = min_x.min(b.x0);
        max_x = max_x.max(b.x1);
        min_y = min_y.min(b.y0);
        max_y = max_y.max(b.y1);
    }

    let center_x = if min_x.is_finite() && max_x.is_finite() {
        (min_x + max_x) / 2.0
    } else {
        0.0
    };
    let center_y = if min_y.is_finite() && max_y.is_finite() {
        (min_y + max_y) / 2.0
    } else {
        0.0
    };

    // Center paths around origin (same as center_text_paths)
    // capture baseline offset: was at Y=0 before centering
    let baseline_offset = center_text_paths(&mut glyphs);

    let ascent_scaled = ascent * font_scale;
    let descent_scaled = descent * font_scale;

    let hardware_layout = if !pending_hardware.is_empty() {
        let mut runs: Vec<HardwareGlyphRun> = Vec::new();
        for pg in pending_hardware {
            let face = &faces[pg.face_idx];
            let centered_x = pg.x - center_x as f32;
            let centered_y = pg.y - center_y as f32;
            if let Some(last_run) = runs.last_mut() {
                if std::ptr::eq(last_run.font_data, face.font_data)
                    && last_run.font_index == face.font_index
                {
                    last_run.glyphs.push(HardwareGlyph {
                        id: pg.id,
                        x: centered_x,
                        y: centered_y,
                    });
                    continue;
                }
            }
            runs.push(HardwareGlyphRun {
                font_data: face.font_data,
                font_index: face.font_index,
                font_size: size,
                glyphs: vec![HardwareGlyph {
                    id: pg.id,
                    x: centered_x,
                    y: centered_y,
                }],
            });
        }
        Some(HardwareTextLayout { runs })
    } else {
        None
    };

    Ok(CompiledText {
        glyphs,
        ascent: ascent_scaled,
        descent: descent_scaled,
        baseline_offset,
        hardware_layout,
    })
}

/// Compile plain text into glyph paths with word wrapping (fast path).
///
/// Like `compile_text_fast`, but wraps text at `max_width` points.
/// Honors `text_align` per line and `overflow` mode.
#[allow(clippy::too_many_arguments)]
pub fn compile_text_fast_wrapped(
    content: &str,
    family: &str,
    weight: f32,
    style: &str,
    size: f32,
    line_height: f32,
    color: [f32; 4],
    letter_spacing: f32,
    word_spacing: f32,
    font_ctx: &FontContext,
    max_width: f32,
    max_lines: Option<usize>,
    text_align: &str,
    overflow: &str,
) -> Result<CompiledText, RenderError> {
    let resolved_family = resolve_font_family(family, font_ctx);
    let (primary_data, primary_index) = 'font: {
        if let Some(bf) = bundled_face(&resolved_family, weight, style) {
            break 'font (bf.data, 0);
        }
        if let Some((data, idx)) = font_ctx.load_face_data(&resolved_family, weight, style) {
            break 'font (data, idx);
        }
        return Err(RenderError::TextCompilation(format!(
            "Failed to load font '{}' (weight={}, style={}) for fast path wrapped",
            resolved_family, weight, style
        )));
    };

    let Some(primary_face) =
        FastPathFace::from_data(primary_data, primary_index, resolved_family.clone())
    else {
        return Err(RenderError::TextCompilation(format!(
            "Failed to parse font '{}' (weight={}, style={}) for fast path wrapped",
            resolved_family, weight, style
        )));
    };

    let units_per_em = primary_face.ttf.units_per_em() as f32;
    let font_scale = size / units_per_em;

    let ascent = primary_face.ttf.ascender() as f32;
    let descent = primary_face.ttf.descender() as f32;
    let line_gap = primary_face.ttf.line_gap() as f32;
    let _metrics = TextMetrics {
        ascent,
        descent,
        line_gap,
        units_per_em,
    };

    let mut faces = vec![primary_face];
    faces.extend(
        BUNDLED_FONTS
            .iter()
            .filter(|bf| bf.family != resolved_family)
            .filter_map(|bf| FastPathFace::from_data(bf.data, 0, bf.family.to_string())),
    );

    let rgba = f32_color_to_rgba8(&color);

    // Line height in absolute points
    let lh_mult = if line_height > 0.0 { line_height } else { 1.2 };
    let line_height_pts = (ascent - descent + line_gap) * font_scale * lh_mult;

    if content.is_empty() {
        return Ok(CompiledText {
            glyphs: Vec::new(),
            ascent: ascent * font_scale,
            descent: descent * font_scale,
            baseline_offset: 0.0,
            hardware_layout: None,
        });
    }

    struct WordInfo {
        #[allow(dead_code)]
        text: String,
        width: f64, // total advance in scene coords
        glyphs: Vec<ShapedGlyph>,
        space_before: bool,
    }

    struct LineInfo {
        words: Vec<usize>, // indices into word_infos
        total_width: f64,
    }

    let mut word_infos: Vec<WordInfo> = Vec::new();
    let mut lines: Vec<LineInfo> = Vec::new();
    let wrap_threshold = if max_width > 0.0 {
        max_width as f64
    } else {
        f64::INFINITY
    };

    let space_advance = {
        let (w, _) = shape_token(" ", &faces, size, letter_spacing);
        if w > 0.0 {
            w + word_spacing as f64
        } else {
            (size as f64 * 0.25) + word_spacing as f64
        }
    };

    for paragraph in content.split('\n') {
        let p = paragraph.strip_suffix('\r').unwrap_or(paragraph);
        let mut char_indices = p.char_indices().peekable();
        let mut pending_space = false;
        let mut word_start: Option<usize> = None;
        let mut raw_tokens: Vec<(&str, bool)> = Vec::new();

        while let Some((idx, ch)) = char_indices.next() {
            if ch == ' ' || ch == '\t' {
                if let Some(start) = word_start.take() {
                    raw_tokens.push((&p[start..idx], pending_space));
                }
                pending_space = true;
            } else if is_cjk_char(ch) {
                if let Some(start) = word_start.take() {
                    raw_tokens.push((&p[start..idx], pending_space));
                    pending_space = false;
                }
                let next_idx = char_indices.peek().map(|(i, _)| *i).unwrap_or(p.len());
                raw_tokens.push((&p[idx..next_idx], pending_space));
                pending_space = false;
            } else if word_start.is_none() {
                word_start = Some(idx);
            }
        }
        if let Some(start) = word_start {
            raw_tokens.push((&p[start..], pending_space));
        }

        if raw_tokens.is_empty() {
            lines.push(LineInfo {
                words: Vec::new(),
                total_width: 0.0,
            });
            continue;
        }

        let mut paragraph_word_indices = Vec::with_capacity(raw_tokens.len());
        for (w, space_before) in raw_tokens {
            let (token_width, shaped_glyphs) = shape_token(w, &faces, size, letter_spacing);
            let wi_idx = word_infos.len();
            word_infos.push(WordInfo {
                text: w.to_string(),
                width: token_width,
                glyphs: shaped_glyphs,
                space_before,
            });
            paragraph_word_indices.push(wi_idx);
        }

        let mut current_line = LineInfo {
            words: Vec::new(),
            total_width: 0.0,
        };

        for &wi_idx in &paragraph_word_indices {
            let wi = &word_infos[wi_idx];
            let space_needed = if current_line.words.is_empty() {
                0.0
            } else if wi.space_before {
                space_advance
            } else {
                0.0
            };

            if current_line.total_width + space_needed + wi.width > wrap_threshold
                && !current_line.words.is_empty()
            {
                lines.push(current_line);
                current_line = LineInfo {
                    words: Vec::new(),
                    total_width: 0.0,
                };
            }

            if !current_line.words.is_empty() && wi.space_before {
                current_line.total_width += space_advance;
            }
            current_line.words.push(wi_idx);
            current_line.total_width += wi.width;
        }
        if !current_line.words.is_empty() {
            lines.push(current_line);
        }
    }

    if word_infos.is_empty() {
        return Ok(CompiledText {
            glyphs: Vec::new(),
            ascent: ascent * font_scale,
            descent: descent * font_scale,
            baseline_offset: 0.0,
            hardware_layout: None,
        });
    }

    // Handle overflow. An explicit `max_lines` (the `max_lines` property) wins;
    // otherwise ellipsis/clip keeps the old "one page worth" guard so unbounded
    // growth is still prevented.
    let line_limit = max_lines.or(if overflow == "ellipsis" || overflow == "clip" {
        // Limit to roughly the visible area: one page worth (no explicit height limit yet)
        // For now, just prevent unbounded growth
        Some(100usize)
    } else {
        None
    });

    let truncated_lines: &[LineInfo] = if let Some(limit) = line_limit {
        if lines.len() > limit {
            &lines[..limit]
        } else {
            &lines
        }
    } else {
        &lines
    };

    let mut glyphs: Vec<TextPath> = Vec::new();
    let mut y_curr: f64 = -(truncated_lines.len() as f64 * line_height_pts as f64) / 2.0; // start at top, centered vertically
    let mut pending_hardware: Vec<HardwarePendingGlyph> = Vec::new();

    for (line_idx, line) in truncated_lines.iter().enumerate() {
        let mut x_curr: f64 = 0.0;

        // Compute line width from word widths
        let line_width = line.total_width;

        // Compute x offset based on text_align
        let x_offset = match text_align {
            "center" => (wrap_threshold - line_width) / 2.0,
            "right" => wrap_threshold - line_width,
            "justify" => 0.0, // handled per-word below
            _ => 0.0,         // left
        };

        let is_last_line = line_idx == truncated_lines.len() - 1;
        let is_justify = text_align == "justify" && !is_last_line && line.words.len() > 1;
        let extra_space_per_gap = if is_justify {
            (wrap_threshold - line_width) / (line.words.len() - 1) as f64
        } else {
            0.0
        };

        for (wi_idx_in_line, &wi_idx) in line.words.iter().enumerate() {
            let wi = &word_infos[wi_idx];

            // Add space before word (except first)
            if wi_idx_in_line > 0 && wi.space_before {
                if is_justify {
                    x_curr += space_advance + extra_space_per_gap;
                } else {
                    x_curr += space_advance;
                }
            }

            // Render each glyph in the word using pre-computed offsets that include kerning
            for g in &wi.glyphs {
                let face = &faces[g.face_idx];
                let glyph_scale = size / (face.ttf.units_per_em() as f32);

                pending_hardware.push(HardwarePendingGlyph {
                    face_idx: g.face_idx,
                    id: g.glyph_id as u32,
                    x: (x_curr + x_offset + g.x_offset) as f32,
                    y: (y_curr + g.y_offset) as f32,
                });

                let mut builder = PathBuilder(BezPath::new());
                if face.ttf.outline_glyph(ttf_parser::GlyphId(g.glyph_id), &mut builder).is_some() {
                    let path = builder.0;
                    let scale_affine =
                        Affine::scale_non_uniform(glyph_scale as f64, -glyph_scale as f64);
                    let translate = Affine::translate(kurbo::Vec2::new(
                        x_curr + x_offset + g.x_offset,
                        y_curr + g.y_offset,
                    ));
                    let final_affine = translate * scale_affine;

                    let mut final_path = path;
                    final_path.apply_affine(final_affine);

                    let c_idx = glyphs.len() as u16;
                    let bounds = final_path.bounding_box();
                    glyphs.push(TextPath {
                        path: final_path,
                        color: rgba,
                        opacity: 1.0,
                        line_idx: line_idx as u16,
                        word_idx: wi_idx as u16,
                        char_idx: c_idx,
                        glyph_center: [0.0, 0.0],
                        bounds,
                    });
                }
            }
            // Advance x_curr by the total word width (includes kerning)
            x_curr += wi.width;
        }

        // Add ellipsis for overflow: "ellipsis" on last visible line if truncated
        if overflow == "ellipsis"
            && line_idx == truncated_lines.len() - 1
            && lines.len() > truncated_lines.len()
        {
            let (dot_w, dot_glyphs) = shape_token("…", &faces, size, 0.0);
            let (_ell_w, ell_glyphs) = if dot_glyphs.is_empty() || dot_glyphs[0].glyph_id == 0 {
                shape_token(".", &faces, size, 0.0)
            } else {
                (dot_w, dot_glyphs)
            };
            for g in ell_glyphs {
                let face = &faces[g.face_idx];
                let glyph_scale = size / (face.ttf.units_per_em() as f32);

                pending_hardware.push(HardwarePendingGlyph {
                    face_idx: g.face_idx,
                    id: g.glyph_id as u32,
                    x: (x_curr + x_offset + g.x_offset) as f32,
                    y: (y_curr + g.y_offset) as f32,
                });

                let mut builder = PathBuilder(BezPath::new());
                if face.ttf.outline_glyph(ttf_parser::GlyphId(g.glyph_id), &mut builder).is_some() {
                    let path = builder.0;
                    let scale_affine =
                        Affine::scale_non_uniform(glyph_scale as f64, -glyph_scale as f64);
                    let translate = kurbo::Affine::translate(kurbo::Vec2::new(
                        x_curr + x_offset + g.x_offset,
                        y_curr + g.y_offset,
                    ));
                    let final_affine = translate * scale_affine;
                    let mut final_path = path;
                    final_path.apply_affine(final_affine);
                    let c_idx = glyphs.len() as u16;
                    let last_w_idx = line.words.last().copied().unwrap_or(0) as u16;
                    let bounds = final_path.bounding_box();
                    glyphs.push(TextPath {
                        path: final_path,
                        color: rgba,
                        opacity: 1.0,
                        line_idx: line_idx as u16,
                        word_idx: last_w_idx,
                        char_idx: c_idx,
                        glyph_center: [0.0, 0.0],
                        bounds,
                    });
                }
            }
        }

        y_curr += line_height_pts as f64;
    }

    // Compute bounding box BEFORE centering to find center_x and center_y
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for path in glyphs.iter() {
        let b = path.bounds;
        min_x = min_x.min(b.x0);
        max_x = max_x.max(b.x1);
        min_y = min_y.min(b.y0);
        max_y = max_y.max(b.y1);
    }

    let center_x = if min_x.is_finite() && max_x.is_finite() {
        (min_x + max_x) / 2.0
    } else {
        0.0
    };
    let center_y = if min_y.is_finite() && max_y.is_finite() {
        (min_y + max_y) / 2.0
    } else {
        0.0
    };

    // Center all paths around origin
    let baseline_offset = center_text_paths(&mut glyphs);

    let ascent_scaled = ascent * font_scale;
    let descent_scaled = descent * font_scale;

    let hardware_layout = if !pending_hardware.is_empty() {
        let mut runs: Vec<HardwareGlyphRun> = Vec::new();
        for pg in pending_hardware {
            let face = &faces[pg.face_idx];
            let centered_x = pg.x - center_x as f32;
            let centered_y = pg.y - center_y as f32;
            if let Some(last_run) = runs.last_mut() {
                if std::ptr::eq(last_run.font_data, face.font_data)
                    && last_run.font_index == face.font_index
                {
                    last_run.glyphs.push(HardwareGlyph {
                        id: pg.id,
                        x: centered_x,
                        y: centered_y,
                    });
                    continue;
                }
            }
            runs.push(HardwareGlyphRun {
                font_data: face.font_data,
                font_index: face.font_index,
                font_size: size,
                glyphs: vec![HardwareGlyph {
                    id: pg.id,
                    x: centered_x,
                    y: centered_y,
                }],
            });
        }
        Some(HardwareTextLayout { runs })
    } else {
        None
    };

    Ok(CompiledText {
        glyphs,
        ascent: ascent_scaled,
        descent: descent_scaled,
        baseline_offset,
        hardware_layout,
    })
}
