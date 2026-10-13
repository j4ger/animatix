//! Storyboard contact sheet rendering and composition.
//!
//! Renders keyframe beats and resting composition frames into a single
//! multi-panel contact sheet (grid) image. This allows human authors and AI
//! agents to review spatial layout, cue timing, transitions, and resting
//! plate completeness in a single inspection pass.

use animatix::composition::Composition;
use animatix::timeline::{DebugRenderOptions, SceneDimensions, Timeline};
use tracing::info;

use super::ExportError;
use crate::offscreen::OffscreenRenderer;

// ---------------------------------------------------------------------------
// Embedded 5x7 ASCII Bitmap Font (ASCII 32 ' ' to 126 '~')
// ---------------------------------------------------------------------------

/// 5 bytes per glyph, 1 column per byte, LSB at top (bit 0 = row 0, bit 6 = row 6).
#[rustfmt::skip]
const FONT_5X7: [[u8; 5]; 95] = [
    [0x00, 0x00, 0x00, 0x00, 0x00], // 32 ' '
    [0x00, 0x00, 0x5F, 0x00, 0x00], // 33 '!'
    [0x00, 0x07, 0x00, 0x07, 0x00], // 34 '"'
    [0x14, 0x7F, 0x14, 0x7F, 0x14], // 35 '#'
    [0x24, 0x2A, 0x7F, 0x2A, 0x12], // 36 '$'
    [0x23, 0x13, 0x08, 0x64, 0x62], // 37 '%'
    [0x36, 0x49, 0x55, 0x22, 0x50], // 38 '&'
    [0x00, 0x05, 0x03, 0x00, 0x00], // 39 '\''
    [0x00, 0x1C, 0x22, 0x41, 0x00], // 40 '('
    [0x00, 0x41, 0x22, 0x1C, 0x00], // 41 ')'
    [0x14, 0x08, 0x3E, 0x08, 0x14], // 42 '*'
    [0x08, 0x08, 0x3E, 0x08, 0x08], // 43 '+'
    [0x00, 0x50, 0x30, 0x00, 0x00], // 44 ','
    [0x08, 0x08, 0x08, 0x08, 0x08], // 45 '-'
    [0x00, 0x60, 0x60, 0x00, 0x00], // 46 '.'
    [0x20, 0x10, 0x08, 0x04, 0x02], // 47 '/'
    [0x3E, 0x51, 0x49, 0x45, 0x3E], // 48 '0'
    [0x00, 0x42, 0x7F, 0x40, 0x00], // 49 '1'
    [0x42, 0x61, 0x51, 0x49, 0x46], // 50 '2'
    [0x21, 0x41, 0x45, 0x4B, 0x31], // 51 '3'
    [0x18, 0x14, 0x12, 0x7F, 0x10], // 52 '4'
    [0x27, 0x45, 0x45, 0x45, 0x39], // 53 '5'
    [0x3C, 0x4A, 0x49, 0x49, 0x30], // 54 '6'
    [0x01, 0x71, 0x09, 0x05, 0x03], // 55 '7'
    [0x36, 0x49, 0x49, 0x49, 0x36], // 56 '8'
    [0x06, 0x49, 0x49, 0x29, 0x1E], // 57 '9'
    [0x00, 0x36, 0x36, 0x00, 0x00], // 58 ':'
    [0x00, 0x56, 0x36, 0x00, 0x00], // 59 ';'
    [0x08, 0x14, 0x22, 0x41, 0x00], // 60 '<'
    [0x14, 0x14, 0x14, 0x14, 0x14], // 61 '='
    [0x00, 0x41, 0x22, 0x14, 0x08], // 62 '>'
    [0x02, 0x01, 0x51, 0x09, 0x06], // 63 '?'
    [0x32, 0x49, 0x79, 0x41, 0x3E], // 64 '@'
    [0x7E, 0x11, 0x11, 0x11, 0x7E], // 65 'A'
    [0x7F, 0x49, 0x49, 0x49, 0x36], // 66 'B'
    [0x3E, 0x41, 0x41, 0x41, 0x22], // 67 'C'
    [0x7F, 0x41, 0x41, 0x22, 0x1C], // 68 'D'
    [0x7F, 0x49, 0x49, 0x49, 0x41], // 69 'E'
    [0x7F, 0x09, 0x09, 0x09, 0x01], // 70 'F'
    [0x3E, 0x41, 0x49, 0x49, 0x7A], // 71 'G'
    [0x7F, 0x08, 0x08, 0x08, 0x7F], // 72 'H'
    [0x00, 0x41, 0x7F, 0x41, 0x00], // 73 'I'
    [0x20, 0x40, 0x41, 0x3F, 0x01], // 74 'J'
    [0x7F, 0x08, 0x14, 0x22, 0x41], // 75 'K'
    [0x7F, 0x40, 0x40, 0x40, 0x40], // 76 'L'
    [0x7F, 0x02, 0x0C, 0x02, 0x7F], // 77 'M'
    [0x7F, 0x04, 0x08, 0x10, 0x7F], // 78 'N'
    [0x3E, 0x41, 0x41, 0x41, 0x3E], // 79 'O'
    [0x7F, 0x09, 0x09, 0x09, 0x06], // 80 'P'
    [0x3E, 0x41, 0x51, 0x21, 0x5E], // 81 'Q'
    [0x7F, 0x09, 0x19, 0x29, 0x46], // 82 'R'
    [0x46, 0x49, 0x49, 0x49, 0x31], // 83 'S'
    [0x01, 0x01, 0x7F, 0x01, 0x01], // 84 'T'
    [0x3F, 0x40, 0x40, 0x40, 0x3F], // 85 'U'
    [0x1F, 0x20, 0x40, 0x20, 0x1F], // 86 'V'
    [0x3F, 0x40, 0x38, 0x40, 0x3F], // 87 'W'
    [0x63, 0x14, 0x08, 0x14, 0x63], // 88 'X'
    [0x07, 0x08, 0x70, 0x08, 0x07], // 89 'Y'
    [0x61, 0x51, 0x49, 0x45, 0x43], // 90 'Z'
    [0x00, 0x7F, 0x41, 0x41, 0x00], // 91 '['
    [0x02, 0x04, 0x08, 0x10, 0x20], // 92 '\\'
    [0x00, 0x41, 0x41, 0x7F, 0x00], // 93 ']'
    [0x04, 0x02, 0x01, 0x02, 0x04], // 94 '^'
    [0x40, 0x40, 0x40, 0x40, 0x40], // 95 '_'
    [0x00, 0x01, 0x02, 0x04, 0x00], // 96 '`'
    [0x20, 0x54, 0x54, 0x54, 0x78], // 97 'a'
    [0x7F, 0x48, 0x44, 0x44, 0x38], // 98 'b'
    [0x38, 0x44, 0x44, 0x44, 0x20], // 99 'c'
    [0x38, 0x44, 0x44, 0x48, 0x7F], // 100 'd'
    [0x38, 0x54, 0x54, 0x54, 0x18], // 101 'e'
    [0x08, 0x7E, 0x09, 0x01, 0x02], // 102 'f'
    [0x0C, 0x52, 0x52, 0x52, 0x3E], // 103 'g'
    [0x7F, 0x08, 0x04, 0x04, 0x78], // 104 'h'
    [0x00, 0x44, 0x7D, 0x40, 0x00], // 105 'i'
    [0x20, 0x40, 0x44, 0x3D, 0x00], // 106 'j'
    [0x7F, 0x10, 0x28, 0x44, 0x00], // 107 'k'
    [0x00, 0x41, 0x7F, 0x40, 0x00], // 108 'l'
    [0x7C, 0x04, 0x18, 0x04, 0x78], // 109 'm'
    [0x7C, 0x08, 0x04, 0x04, 0x78], // 110 'n'
    [0x38, 0x44, 0x44, 0x44, 0x38], // 111 'o'
    [0x7C, 0x14, 0x14, 0x14, 0x08], // 112 'p'
    [0x08, 0x14, 0x14, 0x18, 0x7C], // 113 'q'
    [0x7C, 0x08, 0x04, 0x04, 0x08], // 114 'r'
    [0x48, 0x54, 0x54, 0x54, 0x20], // 115 's'
    [0x04, 0x3F, 0x44, 0x40, 0x20], // 116 't'
    [0x3C, 0x40, 0x40, 0x20, 0x7C], // 117 'u'
    [0x1C, 0x20, 0x40, 0x20, 0x1C], // 118 'v'
    [0x3C, 0x40, 0x30, 0x40, 0x3C], // 119 'w'
    [0x44, 0x28, 0x10, 0x28, 0x44], // 120 'x'
    [0x0C, 0x50, 0x50, 0x50, 0x3C], // 121 'y'
    [0x44, 0x64, 0x54, 0x4C, 0x44], // 122 'z'
    [0x00, 0x08, 0x36, 0x41, 0x00], // 123 '{'
    [0x00, 0x00, 0x77, 0x00, 0x00], // 124 '|'
    [0x00, 0x41, 0x36, 0x08, 0x00], // 125 '}'
    [0x08, 0x04, 0x08, 0x10, 0x08], // 126 '~'
];

/// Draw ASCII text onto an RGBA image buffer using the embedded 5x7 font.
fn draw_text_5x7(
    img: &mut image::RgbaImage,
    text: &str,
    mut x: u32,
    y: u32,
    scale: u32,
    color: [u8; 4],
) {
    let (img_w, img_h) = img.dimensions();
    for ch in text.chars() {
        let code = ch as u32;
        if (32..=126).contains(&code) {
            let glyph = FONT_5X7[(code - 32) as usize];
            for (col, &col_bits) in glyph.iter().enumerate() {
                let px = x + (col as u32) * scale;
                for row in 0..7 {
                    if (col_bits & (1 << row)) != 0 {
                        let py = y + (row as u32) * scale;
                        for dx in 0..scale {
                            for dy in 0..scale {
                                if px + dx < img_w && py + dy < img_h {
                                    img.put_pixel(px + dx, py + dy, image::Rgba(color));
                                }
                            }
                        }
                    }
                }
            }
        }
        x += 6 * scale; // 5 columns + 1 column letter-spacing
    }
}

// ---------------------------------------------------------------------------
// Storyboard Settings & Data Types
// ---------------------------------------------------------------------------

/// Configuration options for rendering a storyboard contact sheet.
#[derive(Debug, Clone)]
pub struct StoryboardSettings {
    /// Number of columns in the grid. If `None`, chosen automatically based on frame count.
    pub cols: Option<usize>,
    /// Tile width in pixels. Default: 480.
    pub tile_width: u32,
    /// Tile height in pixels. Default: 270.
    pub tile_height: u32,
    /// Native scene render resolution width. If specified and different from tile_width,
    /// frames are rendered at this width and downscaled smoothly to tile_width.
    pub render_width: Option<u32>,
    /// Native scene render resolution height. If specified and different from tile_height,
    /// frames are rendered at this height and downscaled smoothly to tile_height.
    pub render_height: Option<u32>,
    /// Timestamps in seconds to sample.
    pub times: Vec<f64>,
    /// Gutter between tiles in pixels. Default: 8.
    pub gutter: u32,
    /// Outer margin around the grid in pixels. Default: 12.
    pub margin: u32,
    /// Height of the header bar above each tile in pixels. Default: 24.
    pub header_height: u32,
}

impl Default for StoryboardSettings {
    fn default() -> Self {
        Self {
            cols: None,
            tile_width: 480,
            tile_height: 270,
            render_width: None,
            render_height: None,
            times: Vec::new(),
            gutter: 8,
            margin: 12,
            header_height: 24,
        }
    }
}

/// Metadata for a single panel in the storyboard.
struct PanelMeta {
    time_s: f64,
    label: String,
}

fn normalize_frame_to_tile(
    frame: crate::offscreen::RenderedFrame,
    tile_width: u32,
    tile_height: u32,
) -> Result<crate::offscreen::RenderedFrame, ExportError> {
    if frame.width == tile_width && frame.height == tile_height {
        return Ok(frame);
    }
    let src_img = image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba.to_vec())
        .ok_or_else(|| {
            ExportError::ImageEncode("Failed to create image buffer for resizing".into())
        })?;
    let resized = image::imageops::resize(
        &src_img,
        tile_width,
        tile_height,
        image::imageops::FilterType::Triangle,
    );
    Ok(crate::offscreen::RenderedFrame {
        width: tile_width,
        height: tile_height,
        rgba: std::sync::Arc::new(resized.into_raw()),
    })
}

// ---------------------------------------------------------------------------
// Public Export Functions
// ---------------------------------------------------------------------------

/// Render a single scene timeline to a storyboard image file.
///
/// Returns the number of frames rendered.
pub fn render_storyboard_timeline(
    timeline: Timeline,
    settings: &StoryboardSettings,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
) -> Result<usize, ExportError> {
    pollster::block_on(render_storyboard_timeline_async(
        timeline,
        settings,
        output_file,
        debug_options,
    ))
}

/// Render a multi-scene composition to a storyboard image file.
///
/// Returns the number of frames rendered.
pub fn render_storyboard_composition(
    composition: &Composition,
    settings: &StoryboardSettings,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
) -> Result<usize, ExportError> {
    pollster::block_on(render_storyboard_composition_async(
        composition,
        settings,
        output_file,
        debug_options,
    ))
}

// ---------------------------------------------------------------------------
// Internal Async Renderers
// ---------------------------------------------------------------------------

async fn render_storyboard_timeline_async(
    timeline: Timeline,
    settings: &StoryboardSettings,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
) -> Result<usize, ExportError> {
    if settings.times.is_empty() {
        return Err(ExportError::FrameRender {
            frame: 0,
            message: "No sample times provided for storyboard".into(),
        });
    }

    let duration_s = timeline.playback_duration_seconds();
    let panels = build_panel_metadata(&settings.times, duration_s);

    let mut renderer = OffscreenRenderer::new().map_err(ExportError::RendererCreation)?;
    let rw = settings.render_width.unwrap_or(settings.tile_width);
    let rh = settings.render_height.unwrap_or(settings.tile_height);
    let dims = SceneDimensions {
        width: rw,
        height: rh,
    };

    let mut rendered_frames = Vec::with_capacity(panels.len());
    for (idx, panel) in panels.iter().enumerate() {
        let frame = renderer
            .render_timeline_with_debug(&timeline, panel.time_s, dims, debug_options)
            .map_err(|e| ExportError::FrameRender {
                frame: idx,
                message: e,
            })?;
        let tile_frame = normalize_frame_to_tile(frame, settings.tile_width, settings.tile_height)?;
        rendered_frames.push(tile_frame);
    }

    assemble_and_save_grid(settings, &panels, &rendered_frames, output_file)?;
    info!("Storyboard saved to {} ({} frames)", output_file.display(), panels.len());
    Ok(panels.len())
}

async fn render_storyboard_composition_async(
    composition: &Composition,
    settings: &StoryboardSettings,
    output_file: &std::path::Path,
    debug_options: DebugRenderOptions,
) -> Result<usize, ExportError> {
    if settings.times.is_empty() {
        return Err(ExportError::FrameRender {
            frame: 0,
            message: "No sample times provided for storyboard".into(),
        });
    }

    if !composition.has_scenes() {
        return Err(ExportError::FrameRender {
            frame: 0,
            message: "Composition has no scenes to render".into(),
        });
    }

    let duration_s = composition.global_duration_s;
    let panels = build_panel_metadata(&settings.times, duration_s);

    let mut renderer = OffscreenRenderer::new().map_err(ExportError::RendererCreation)?;
    let rw = settings.render_width.unwrap_or(settings.tile_width);
    let rh = settings.render_height.unwrap_or(settings.tile_height);
    let dims = SceneDimensions {
        width: rw,
        height: rh,
    };

    let mut rendered_frames = Vec::with_capacity(panels.len());
    for (idx, panel) in panels.iter().enumerate() {
        let (scene_name, local_time_s, transition_blend) = composition.evaluate(panel.time_s);

        let frame = if let Some(blend) = transition_blend {
            let from_scene = composition.scenes.get(&blend.from_scene).ok_or_else(|| {
                ExportError::FrameRender {
                    frame: idx,
                    message: format!("From scene '{}' not found in composition", blend.from_scene),
                }
            })?;
            let to_scene = composition.scenes.get(&blend.to_scene).ok_or_else(|| {
                ExportError::FrameRender {
                    frame: idx,
                    message: format!("To scene '{}' not found in composition", blend.to_scene),
                }
            })?;
            renderer
                .render_transition(
                    &from_scene.timeline,
                    blend.from_local,
                    &to_scene.timeline,
                    blend.to_local,
                    blend.progress as f32,
                    blend.id.clone(),
                    blend.easing,
                    dims,
                    debug_options,
                )
                .map_err(|e| ExportError::FrameRender {
                    frame: idx,
                    message: e,
                })?
        } else {
            let scene =
                composition.scenes.get(&scene_name).ok_or_else(|| ExportError::FrameRender {
                    frame: idx,
                    message: format!("Scene '{}' not found in composition", scene_name),
                })?;
            renderer
                .render_timeline_with_debug(&scene.timeline, local_time_s, dims, debug_options)
                .map_err(|e| ExportError::FrameRender {
                    frame: idx,
                    message: e,
                })?
        };

        let tile_frame = normalize_frame_to_tile(frame, settings.tile_width, settings.tile_height)?;
        rendered_frames.push(tile_frame);
    }

    assemble_and_save_grid(settings, &panels, &rendered_frames, output_file)?;
    info!("Storyboard saved to {} ({} frames)", output_file.display(), panels.len());
    Ok(panels.len())
}

// ---------------------------------------------------------------------------
// Grid Assembly & Layout Helpers
// ---------------------------------------------------------------------------

fn build_panel_metadata(times: &[f64], duration_s: f64) -> Vec<PanelMeta> {
    times
        .iter()
        .map(|&t| {
            let label = if (t - duration_s).abs() < 0.001 || (t >= duration_s && duration_s > 0.0) {
                format!("Rest: {:.2}s", t)
            } else if t <= 0.001 {
                "Start: 0.00s".to_string()
            } else {
                format!("t = {:.2}s", t)
            };
            PanelMeta { time_s: t, label }
        })
        .collect()
}

fn assemble_and_save_grid(
    settings: &StoryboardSettings,
    panels: &[PanelMeta],
    frames: &[crate::offscreen::RenderedFrame],
    output_file: &std::path::Path,
) -> Result<(), ExportError> {
    let count = panels.len();
    let cols = settings.cols.unwrap_or(match count {
        1 => 1,
        2 => 2,
        3..=4 => 2,
        5..=6 => 3,
        _ => 4,
    });
    let rows = count.div_ceil(cols);

    let margin = settings.margin;
    let gutter = settings.gutter;
    let tile_w = settings.tile_width;
    let tile_h = settings.tile_height;
    let hdr_h = settings.header_height;

    let total_width =
        2 * margin + (cols as u32) * tile_w + ((cols as u32).saturating_sub(1)) * gutter;
    let total_height =
        2 * margin + (rows as u32) * (tile_h + hdr_h) + ((rows as u32).saturating_sub(1)) * gutter;

    // Dark sleek background: #18181b [24, 24, 27, 255]
    let mut canvas =
        image::RgbaImage::from_pixel(total_width, total_height, image::Rgba([24, 24, 27, 255]));

    for (idx, (panel, frame)) in panels.iter().zip(frames.iter()).enumerate() {
        let col = (idx % cols) as u32;
        let row = (idx / cols) as u32;
        let x0 = margin + col * (tile_w + gutter);
        let y0 = margin + row * (tile_h + hdr_h + gutter);

        // Header bar background: #27272a [39, 39, 42, 255]
        for hx in 0..tile_w {
            for hy in 0..hdr_h {
                canvas.put_pixel(x0 + hx, y0 + hy, image::Rgba([39, 39, 42, 255]));
            }
        }

        // Draw header text in soft white #f4f4f5 (scale 2 gives 10x14 px, vertically centered in 24px bar)
        let text_y = y0 + (hdr_h.saturating_sub(14)) / 2;
        draw_text_5x7(&mut canvas, &panel.label, x0 + 8, text_y, 2, [244, 244, 245, 255]);

        // Place rendered frame pixels at (x0, y0 + hdr_h)
        let tile_y = y0 + hdr_h;
        for ty in 0..tile_h {
            for tx in 0..tile_w {
                let src_idx = ((ty * frame.width + tx) * 4) as usize;
                if src_idx + 3 < frame.rgba.len() {
                    let r = frame.rgba[src_idx];
                    let g = frame.rgba[src_idx + 1];
                    let b = frame.rgba[src_idx + 2];
                    let a = frame.rgba[src_idx + 3];
                    canvas.put_pixel(x0 + tx, tile_y + ty, image::Rgba([r, g, b, a]));
                }
            }
        }

        // 1px border around tile (header + frame): #3f3f46 [63, 63, 70, 255]
        let box_h = hdr_h + tile_h;
        for bx in 0..tile_w {
            canvas.put_pixel(x0 + bx, y0, image::Rgba([63, 63, 70, 255]));
            canvas.put_pixel(x0 + bx, y0 + box_h - 1, image::Rgba([63, 63, 70, 255]));
        }
        for by in 0..box_h {
            canvas.put_pixel(x0, y0 + by, image::Rgba([63, 63, 70, 255]));
            canvas.put_pixel(x0 + tile_w - 1, y0 + by, image::Rgba([63, 63, 70, 255]));
        }
    }

    canvas
        .save(output_file)
        .map_err(|e| ExportError::ImageEncode(format!("{e:?}")))?;
    Ok(())
}
