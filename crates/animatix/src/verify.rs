//! Content-level frame verification primitives.
//!
//! The dogfood loop's blind spot is *silent* visual failure: an actor that
//! should be on screen is not drawn, or an animation that should move is
//! static, and nothing errors. `check`/`lint` operate on the build model and a
//! whole-frame "is this blank?" smoke test stays green while one actor among
//! many silently disappears — `examples/data/07_plots.amx` shipped with an
//! invisible headline curve through both gates for that reason.
//!
//! These helpers turn rendered RGBA pixels plus evaluated actor bounds into
//! explicit facts, so a `verify.txt` beside a dogfood project can assert "the
//! curve is actually visible at t=5.0" instead of relying on a human to notice
//! its absence in one frame.
//!
//! The reference color for "inks differs from background" checks is the frame's
//! own modal color ([`FrameView::modal_color`]); comparing pixels to pixels
//! sidesteps any sRGB/linear mismatch between the render target and the
//! declared scene background.

use std::collections::HashMap;

use kurbo::Rect;

use crate::timeline::scene_program::SceneProgram;

/// Per-channel difference above which a pixel counts as "ink" (fraction of the
/// 0..255 range). 0.02 ≈ 5/255 — above dithering/rounding noise, below any
/// deliberate color choice.
pub const DEFAULT_TOLERANCE: f64 = 0.02;

/// Minimum number of ink pixels inside an actor's bounds before it is called
/// visible. Three keeps a stray antialiasing pixel from passing a check.
pub const DEFAULT_MIN_INK_PIXELS: u32 = 3;

/// Pixels added around an actor's bounds before sampling. A horizontal `Line`
/// or `Arrow` has a zero-height bounding box; without padding its own stroke
/// would fall outside the sampled region and read as invisible.
pub const BOUNDS_PAD: f64 = 2.0;

/// A borrowed RGBA8 frame in row-major order (4 bytes per pixel).
pub struct FrameView<'a> {
    width: u32,
    height: u32,
    rgba: &'a [u8],
}

impl<'a> FrameView<'a> {
    /// Wrap a pixel buffer. Returns `None` if it is smaller than
    /// `width * height * 4` bytes.
    pub fn new(width: u32, height: u32, rgba: &'a [u8]) -> Option<Self> {
        let expected = width as usize * height as usize * 4;
        (rgba.len() >= expected).then_some(Self {
            width,
            height,
            rgba,
        })
    }

    /// Frame width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Frame height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The RGBA pixel at `(x, y)`.
    ///
    /// # Panics
    /// Panics if the coordinates are outside the frame.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y as usize * self.width as usize) + x as usize) * 4;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }

    /// The frame's dominant color, quantized to 5 bits per channel.
    ///
    /// In normal content this is the scene background; a scene where a single
    /// actor covers most of the canvas would instead report that actor, so
    /// [`actor_visibility`] on such an actor may under-report. Fine for a
    /// verification tool whose scenes are background-dominated.
    pub fn modal_color(&self) -> [u8; 4] {
        let mut counts: HashMap<[u8; 4], u32> = HashMap::new();
        for y in 0..self.height {
            for x in 0..self.width {
                let p = self.pixel(x, y);
                let key = [p[0] >> 3, p[1] >> 3, p[2] >> 3, p[3] >> 3];
                *counts.entry(key).or_insert(0) += 1;
            }
        }
        let best = counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(key, _)| key)
            .unwrap_or([0, 0, 0, 31]);
        // Reconstruct the bucket center so a flat region reads as its own color.
        [
            (best[0] << 3) | 0x04,
            (best[1] << 3) | 0x04,
            (best[2] << 3) | 0x04,
            (best[3] << 3) | 0x04,
        ]
    }
}

/// An axis-aligned box in scene/world coordinates, decoupled from `kurbo` so
/// callers do not need a direct dependency on it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Box2 {
    /// Left edge.
    pub x0: f64,
    /// Top edge.
    pub y0: f64,
    /// Right edge.
    pub x1: f64,
    /// Bottom edge.
    pub y1: f64,
}

impl Box2 {
    /// Construct a box from its edges.
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self { x0, y0, x1, y1 }
    }

    /// Convert an evaluated actor's `kurbo::Rect` bounds.
    pub fn from_rect(rect: Rect) -> Self {
        Self::new(rect.x0, rect.y0, rect.x1, rect.y1)
    }

    /// Grow the box by `pad` on every side.
    pub fn padded(self, pad: f64) -> Self {
        Self::new(self.x0 - pad, self.y0 - pad, self.x1 + pad, self.y1 + pad)
    }

    /// Clamp to `[0, width] x [0, height]` and return integer pixel bounds as
    /// `(x0, y0, x1_exclusive, y1_exclusive)`. `None` when the box lies fully
    /// outside the frame or has no area.
    pub fn clamped(self, width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
        let x0 = self.x0.max(0.0).min(width as f64).floor() as u32;
        let y0 = self.y0.max(0.0).min(height as f64).floor() as u32;
        let x1 = self.x1.max(0.0).min(width as f64).ceil() as u32;
        let y1 = self.y1.max(0.0).min(height as f64).ceil() as u32;
        (x1 > x0 && y1 > y0).then_some((x0, y0, x1, y1))
    }
}

/// Per-actor world-space bounds recorded during a frame, keyed by label.
///
/// Reads [`SceneProgram::precise_bounds`], which is only populated on the
/// observable evaluation path — use
/// `animatix_render::offscreen::OffscreenRenderer::render_timeline_observable`
/// so the bounds and the pixels come from the same evaluation.
pub fn bounds_map(program: &SceneProgram) -> HashMap<String, Box2> {
    program
        .precise_bounds
        .iter()
        .map(|(label, rect)| (label.clone(), Box2::from_rect(*rect)))
        .collect()
}

/// Whether an actor actually put ink on screen.
#[derive(Clone, Copy, Debug)]
pub struct Visibility {
    /// Ink pixels found inside the actor's padded bounds.
    pub ink_pixels: u32,
    /// Ink pixels divided by sampled pixels (0.0 when the region was empty).
    pub ink_fraction: f32,
    /// `true` when `ink_pixels >= min_ink_pixels`.
    pub visible: bool,
}

fn tolerance_to_u8(tolerance: f64) -> u8 {
    (tolerance.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn pixel_differs(pixel: [u8; 4], reference: [u8; 4], tolerance: u8) -> bool {
    pixel[..3].iter().zip(&reference[..3]).any(|(a, b)| a.abs_diff(*b) > tolerance)
}

/// Count pixels inside `region` that differ from `reference` by more than
/// `tolerance` (a 0..1 per-channel fraction). The region is clamped to the
/// frame; pixels outside it are ignored.
pub fn ink_pixels(frame: &FrameView, region: Box2, reference: [u8; 4], tolerance: f64) -> u32 {
    let Some((x0, y0, x1, y1)) = region.clamped(frame.width(), frame.height()) else {
        return 0;
    };
    let tol = tolerance_to_u8(tolerance);
    let mut count = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            if pixel_differs(frame.pixel(x, y), reference, tol) {
                count += 1;
            }
        }
    }
    count
}

/// Fraction of pixels inside `region` that differ from `reference`. Returns
/// `0.0` when the region does not intersect the frame.
pub fn ink_fraction(frame: &FrameView, region: Box2, reference: [u8; 4], tolerance: f64) -> f32 {
    let Some((x0, y0, x1, y1)) = region.clamped(frame.width(), frame.height()) else {
        return 0.0;
    };
    let total = (x1 - x0) as f32 * (y1 - y0) as f32;
    ink_pixels(frame, region, reference, tolerance) as f32 / total.max(1.0)
}

/// Fraction of the whole frame that differs from its own modal color. A value
/// near `0.0` means the frame is blank.
pub fn frame_ink_fraction(frame: &FrameView, tolerance: f64) -> f32 {
    let reference = frame.modal_color();
    let region = Box2::new(0.0, 0.0, frame.width() as f64, frame.height() as f64);
    ink_fraction(frame, region, reference, tolerance)
}

/// Visibility of an actor given its bounds. `bounds == None` (the label was not
/// evaluated this frame) or an off-canvas box reports invisible.
pub fn actor_visibility(
    frame: &FrameView,
    bounds: Option<Box2>,
    reference: [u8; 4],
    tolerance: f64,
    min_ink_pixels: u32,
) -> Visibility {
    let Some(bounds) = bounds else {
        return Visibility {
            ink_pixels: 0,
            ink_fraction: 0.0,
            visible: false,
        };
    };
    let region = bounds.padded(BOUNDS_PAD);
    let ink_pixels = ink_pixels(frame, region, reference, tolerance);
    let fraction = ink_fraction(frame, region, reference, tolerance);
    Visibility {
        ink_pixels,
        ink_fraction: fraction,
        visible: ink_pixels >= min_ink_pixels,
    }
}

/// Fraction of pixels that differ between two same-sized frames. Returns `1.0`
/// for mismatched dimensions (treat as "changed" rather than hiding the
/// problem).
pub fn differing_fraction(a: &FrameView, b: &FrameView, tolerance: f64) -> f32 {
    region_differing_fraction(a, b, Box2::new(0.0, 0.0, a.width as f64, a.height as f64), tolerance)
}

/// Fraction of pixels that differ between two same-sized frames *inside
/// `region`* (clamped to the overlap). Returns `1.0` for mismatched
/// dimensions. `0.0` when the region has no area.
///
/// This is the robust "did the actor actually appear?" measure: a Graph paints
/// generated axes inside its child curve's bounds, so raw ink cannot tell a
/// revealed curve from a missing one, but the *change* inside the region
/// between a hidden time and a shown time can.
pub fn region_differing_fraction(
    a: &FrameView,
    b: &FrameView,
    region: Box2,
    tolerance: f64,
) -> f32 {
    if a.width != b.width || a.height != b.height {
        return 1.0;
    }
    let Some((x0, y0, x1, y1)) = region.clamped(a.width, a.height) else {
        return 0.0;
    };
    let total = (x1 - x0) as f32 * (y1 - y0) as f32;
    let tol = tolerance_to_u8(tolerance);
    let mut differing = 0u32;
    for y in y0..y1 {
        for x in x0..x1 {
            if pixel_differs(a.pixel(x, y), b.pixel(x, y), tol) {
                differing += 1;
            }
        }
    }
    differing as f32 / total.max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A black frame with a white 2x2 block at (3, 3).
    fn frame_with_block() -> Vec<u8> {
        let (w, h) = (8u32, 8u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 3..5 {
            for x in 3..5 {
                let i = ((y * w + x) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
        rgba
    }

    #[test]
    fn modal_color_is_the_dominant_flat_color() {
        let rgba = frame_with_block();
        let frame = FrameView::new(8, 8, &rgba).unwrap();
        let modal = frame.modal_color();
        assert!(modal[0] < 16 && modal[1] < 16 && modal[2] < 16, "modal should be black");
    }

    #[test]
    fn ink_locates_a_drawn_block_and_ignores_background() {
        let rgba = frame_with_block();
        let frame = FrameView::new(8, 8, &rgba).unwrap();
        let reference = frame.modal_color();
        let on_block = Box2::new(2.0, 2.0, 6.0, 6.0);
        let empty = Box2::new(0.0, 0.0, 2.0, 2.0);
        assert!(ink_pixels(&frame, on_block, reference, DEFAULT_TOLERANCE) > 0);
        assert_eq!(ink_pixels(&frame, empty, reference, DEFAULT_TOLERANCE), 0);
    }

    #[test]
    fn actor_visibility_needs_minimum_ink_and_bounds() {
        let rgba = frame_with_block();
        let frame = FrameView::new(8, 8, &rgba).unwrap();
        let reference = frame.modal_color();
        let visible = actor_visibility(
            &frame,
            Some(Box2::new(2.0, 2.0, 6.0, 6.0)),
            reference,
            DEFAULT_TOLERANCE,
            DEFAULT_MIN_INK_PIXELS,
        );
        assert!(visible.visible, "block inside bounds should be visible");
        let missing = actor_visibility(
            &frame,
            Some(Box2::new(0.0, 0.0, 2.0, 2.0)),
            reference,
            DEFAULT_TOLERANCE,
            DEFAULT_MIN_INK_PIXELS,
        );
        assert!(!missing.visible, "empty region should be invisible");
        let no_bounds =
            actor_visibility(&frame, None, reference, DEFAULT_TOLERANCE, DEFAULT_MIN_INK_PIXELS);
        assert!(!no_bounds.visible, "absent bounds should be invisible");
    }

    #[test]
    fn off_canvas_bounds_are_invisible_not_panicking() {
        let rgba = frame_with_block();
        let frame = FrameView::new(8, 8, &rgba).unwrap();
        let visibility = actor_visibility(
            &frame,
            Some(Box2::new(100.0, 100.0, 120.0, 120.0)),
            frame.modal_color(),
            DEFAULT_TOLERANCE,
            DEFAULT_MIN_INK_PIXELS,
        );
        assert_eq!(visibility.ink_pixels, 0);
        assert!(!visibility.visible);
    }

    #[test]
    fn differing_fraction_detects_change_and_mismatched_sizes() {
        let a = frame_with_block();
        let mut b = frame_with_block();
        let i = ((4 * 8 + 4) * 4) as usize;
        b[i] = 251; // red 255 → 251: a 4/255 diff, below the 0.02 tolerance
        let fa = FrameView::new(8, 8, &a).unwrap();
        let fb = FrameView::new(8, 8, &b).unwrap();
        assert_eq!(differing_fraction(&fa, &fb, DEFAULT_TOLERANCE), 0.0);
        assert_eq!(differing_fraction(&fa, &fa, DEFAULT_TOLERANCE), 0.0);

        let mut c = vec![0u8; 8 * 8 * 4];
        c.fill(255);
        let fc = FrameView::new(8, 8, &c).unwrap();
        // `a` keeps its 4 white pixels; the other 60 change.
        assert_eq!(differing_fraction(&fa, &fc, DEFAULT_TOLERANCE), 60.0 / 64.0);

        let small_pixels = vec![0u8; 4 * 4 * 4];
        let small = FrameView::new(4, 4, &small_pixels).unwrap();
        assert_eq!(differing_fraction(&fa, &small, DEFAULT_TOLERANCE), 1.0);
    }

    #[test]
    fn frame_ink_fraction_separates_blank_from_content() {
        let blank = vec![10u8; 16 * 16 * 4];
        let frame = FrameView::new(16, 16, &blank).unwrap();
        assert_eq!(frame_ink_fraction(&frame, DEFAULT_TOLERANCE), 0.0);

        let rgba = frame_with_block();
        let frame = FrameView::new(8, 8, &rgba).unwrap();
        assert!(frame_ink_fraction(&frame, DEFAULT_TOLERANCE) > 0.0);
    }
}
