//! Probe grid occlusion detection for review.
//!
//! Evaluates whether text actors are occluded by subsequent actors in the draw
//! order using a uniform probe grid. Valid probe points are those with ink
//! relative to the frame's reference/modal color; empty regions are excluded
//! from the denominator.

use crate::frame_source::{Box2, FrameView};

/// 一个参与遮挡判定的 actor，按绘制顺序传入（Vec 顺序 = 绘制顺序，后者在上）。
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProbeActor {
    pub scene: String,
    pub label: String,
    pub bounds: Box2,
    /// 继承后的有效不透明度 0.0..=1.0
    pub opacity: f32,
    pub is_text: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OcclusionFact {
    pub scene: String,
    pub label: String,
    /// 覆盖该 actor 探针点最多的那个遮挡者 label
    pub occluder: Option<String>,
    /// covered_points / probed_points，0.0..=1.0
    pub probe_coverage: f64,
    pub probed_points: usize,
    pub covered_points: usize,
}

/// 默认 3 行 x 9 列。
pub(crate) const PROBE_ROWS: usize = 3;
pub(crate) const PROBE_COLS: usize = 9;

pub(crate) fn probe_points(bounds: Box2, rows: usize, cols: usize) -> Vec<(f64, f64)> {
    if rows == 0 || cols == 0 || bounds.x1 <= bounds.x0 || bounds.y1 <= bounds.y0 {
        return Vec::new();
    }
    let dx = (bounds.x1 - bounds.x0) / cols as f64;
    let dy = (bounds.y1 - bounds.y0) / rows as f64;
    let mut points = Vec::with_capacity(rows * cols);
    for r in 0..rows {
        let y = bounds.y0 + (r as f64 + 0.5) * dy;
        for c in 0..cols {
            let x = bounds.x0 + (c as f64 + 0.5) * dx;
            points.push((x, y));
        }
    }
    points
}

fn box_contains_point(bounds: Box2, px: f64, py: f64) -> bool {
    let (min_x, max_x) = if bounds.x0 <= bounds.x1 {
        (bounds.x0, bounds.x1)
    } else {
        (bounds.x1, bounds.x0)
    };
    let (min_y, max_y) = if bounds.y0 <= bounds.y1 {
        (bounds.y0, bounds.y1)
    } else {
        (bounds.y1, bounds.y0)
    };
    px >= min_x && px <= max_x && py >= min_y && py <= max_y
}

fn pixel_has_ink(pixel: [u8; 4], reference: [u8; 4], tolerance: f64) -> bool {
    (0..3).any(|c| (pixel[c] as f64 - reference[c] as f64).abs() / 255.0 > tolerance)
}

/// 对 draw_order 中每个 is_text actor 计算遮挡事实。
/// reference 是该帧的 modal_color；tolerance 用 DEFAULT_TOLERANCE。
pub(crate) fn probe_occlusion(
    frame: &FrameView<'_>,
    draw_order: &[ProbeActor],
    reference: [u8; 4],
    tolerance: f64,
) -> Vec<OcclusionFact> {
    let mut facts = Vec::new();

    for (idx, actor) in draw_order.iter().enumerate() {
        // 1. 只对每个 is_text == true 且 opacity > 0.05 的 actor 出一条 OcclusionFact。
        if !actor.is_text || actor.opacity <= 0.05 {
            continue;
        }

        // 2. 在它 bounds 内按 probe_points 取 rows*cols 个探针点（点取格子中心，不要落在边界像素上；用 Box2::clamped 夹到画布内）。
        let Some((cx0, cy0, cx1, cy1)) = actor.bounds.clamped(frame.width(), frame.height()) else {
            facts.push(OcclusionFact {
                scene: actor.scene.clone(),
                label: actor.label.clone(),
                occluder: None,
                probe_coverage: 0.0,
                probed_points: 0,
                covered_points: 0,
            });
            continue;
        };

        let clamped_bounds = Box2::new(
            actor.bounds.x0.max(cx0 as f64),
            actor.bounds.y0.max(cy0 as f64),
            actor.bounds.x1.min(cx1 as f64),
            actor.bounds.y1.min(cy1 as f64),
        );

        let points = probe_points(clamped_bounds, PROBE_ROWS, PROBE_COLS);

        // 3. 有效探针点：该点相对 reference 有 ink（任一 RGB 通道差异 > tolerance）。无 ink 的点不计入分母。
        let valid_points: Vec<(f64, f64)> = points
            .into_iter()
            .filter(|&(px, py)| {
                let x = (px.floor() as u32).min(frame.width().saturating_sub(1));
                let y = (py.floor() as u32).min(frame.height().saturating_sub(1));
                let pixel = frame.pixel(x, y);
                pixel_has_ink(pixel, reference, tolerance)
            })
            .collect();

        let probed_points = valid_points.len();
        if probed_points == 0 {
            // 5. probed_points == 0 时返回 0.0 且 occluder: None（不要除零）。
            facts.push(OcclusionFact {
                scene: actor.scene.clone(),
                label: actor.label.clone(),
                occluder: None,
                probe_coverage: 0.0,
                probed_points: 0,
                covered_points: 0,
            });
            continue;
        }

        // 4. 被覆盖判定：对每个有效探针点，在 draw_order 中找排在该 text actor 之后、
        // 且 opacity > 0.5、且 bounds 包含该点的 actor；若存在则记为 covered。
        // 取覆盖点数最多的那个作为 occluder（并列时取绘制顺序最靠后的）。
        let subsequent_actors = &draw_order[(idx + 1)..];

        let covered_points = valid_points
            .iter()
            .filter(|&&(px, py)| {
                subsequent_actors
                    .iter()
                    .any(|occ| occ.opacity > 0.5 && box_contains_point(occ.bounds, px, py))
            })
            .count();

        let mut best_occluder: Option<String> = None;
        let mut best_count = 0;

        for occluder in subsequent_actors {
            if occluder.opacity <= 0.5 {
                continue;
            }
            let count = valid_points
                .iter()
                .filter(|&&(px, py)| box_contains_point(occluder.bounds, px, py))
                .count();
            if count > 0 && count >= best_count {
                best_count = count;
                best_occluder = Some(occluder.label.clone());
            }
        }

        let probe_coverage = covered_points as f64 / probed_points as f64;

        facts.push(OcclusionFact {
            scene: actor.scene.clone(),
            label: actor.label.clone(),
            occluder: best_occluder,
            probe_coverage,
            probed_points,
            covered_points,
        });
    }

    facts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_source::DEFAULT_TOLERANCE;

    fn create_test_frame(width: u32, height: u32, bg: [u8; 4]) -> Vec<u8> {
        let mut buf = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..(width * height) {
            buf.extend_from_slice(&bg);
        }
        buf
    }

    fn fill_rect(buf: &mut [u8], width: u32, x0: u32, y0: u32, x1: u32, y1: u32, color: [u8; 4]) {
        for y in y0..y1 {
            for x in x0..x1 {
                let idx = ((y * width + x) * 4) as usize;
                buf[idx..idx + 4].copy_from_slice(&color);
            }
        }
    }

    #[test]
    fn test_probe_points_inside_bounds() {
        let bounds = Box2::new(10.0, 20.0, 100.0, 50.0);
        let pts = probe_points(bounds, PROBE_ROWS, PROBE_COLS);
        assert_eq!(pts.len(), 27);
        let dx = (100.0 - 10.0) / 9.0;
        let dy = (50.0 - 20.0) / 3.0;
        for (idx, &(x, y)) in pts.iter().enumerate() {
            let r = idx / 9;
            let c = idx % 9;
            let expected_x = 10.0 + (c as f64 + 0.5) * dx;
            let expected_y = 20.0 + (r as f64 + 0.5) * dy;
            assert!((x - expected_x).abs() < 1e-9);
            assert!((y - expected_y).abs() < 1e-9);
            assert!(x > bounds.x0 && x < bounds.x1);
            assert!(y > bounds.y0 && y < bounds.y1);
        }
    }

    #[test]
    fn test_full_occlusion() {
        let width = 100;
        let height = 100;
        let bg = [255, 255, 255, 255];
        let ink = [0, 0, 0, 255];
        let mut buf = create_test_frame(width, height, bg);
        fill_rect(&mut buf, width, 0, 0, 90, 30, ink);
        let frame = FrameView::new(width, height, &buf).unwrap();

        let text = ProbeActor {
            scene: "main".into(),
            label: "title".into(),
            bounds: Box2::new(0.0, 0.0, 90.0, 30.0),
            opacity: 1.0,
            is_text: true,
        };
        let rect = ProbeActor {
            scene: "main".into(),
            label: "blocker".into(),
            bounds: Box2::new(0.0, 0.0, 90.0, 30.0),
            opacity: 1.0,
            is_text: false,
        };

        let facts = probe_occlusion(&frame, &[text, rect], bg, DEFAULT_TOLERANCE);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].scene, "main");
        assert_eq!(facts[0].label, "title");
        assert_eq!(facts[0].probed_points, 27);
        assert_eq!(facts[0].covered_points, 27);
        assert!((facts[0].probe_coverage - 1.0).abs() < 1e-6);
        assert_eq!(facts[0].occluder.as_deref(), Some("blocker"));
    }

    #[test]
    fn test_partial_occlusion_two_points() {
        let width = 100;
        let height = 100;
        let bg = [255, 255, 255, 255];
        let ink = [0, 0, 0, 255];
        let mut buf = create_test_frame(width, height, bg);
        fill_rect(&mut buf, width, 0, 0, 90, 30, ink);
        let frame = FrameView::new(width, height, &buf).unwrap();

        let text = ProbeActor {
            scene: "main".into(),
            label: "title".into(),
            bounds: Box2::new(0.0, 0.0, 90.0, 30.0),
            opacity: 1.0,
            is_text: true,
        };
        // In 3x9 grid for [0..90] x [0..30], grid cell centers are:
        // row 0: y=5; row 1: y=15; row 2: y=25.
        // cols: x=5, 15, 25, 35, 45, 55, 65, 75, 85.
        // Box [0, 0, 20, 10] covers exactly (5, 5) and (15, 5).
        let occ = ProbeActor {
            scene: "main".into(),
            label: "partial_box".into(),
            bounds: Box2::new(0.0, 0.0, 20.0, 10.0),
            opacity: 1.0,
            is_text: false,
        };

        let facts = probe_occlusion(&frame, &[text, occ], bg, DEFAULT_TOLERANCE);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].probed_points, 27);
        assert_eq!(facts[0].covered_points, 2);
        assert!((facts[0].probe_coverage - 2.0 / 27.0).abs() < 1e-6);
        assert_eq!(facts[0].occluder.as_deref(), Some("partial_box"));
    }

    #[test]
    fn test_no_ink_points_excluded_from_denominator() {
        let width = 100;
        let height = 100;
        let bg = [255, 255, 255, 255];
        let ink = [0, 0, 0, 255];
        let mut buf = create_test_frame(width, height, bg);
        // Fill only top row [0, 90] x [0, 10] with ink; rest is bg (white).
        fill_rect(&mut buf, width, 0, 0, 90, 10, ink);
        let frame = FrameView::new(width, height, &buf).unwrap();

        let text = ProbeActor {
            scene: "main".into(),
            label: "title".into(),
            bounds: Box2::new(0.0, 0.0, 90.0, 30.0),
            opacity: 1.0,
            is_text: true,
        };
        // Occluder covers all 9 valid ink points in row 0.
        let occ = ProbeActor {
            scene: "main".into(),
            label: "box".into(),
            bounds: Box2::new(0.0, 0.0, 90.0, 10.0),
            opacity: 1.0,
            is_text: false,
        };

        let facts = probe_occlusion(&frame, &[text, occ], bg, DEFAULT_TOLERANCE);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].probed_points, 9);
        assert_eq!(facts[0].covered_points, 9);
        assert!((facts[0].probe_coverage - 1.0).abs() < 1e-6);
        assert_eq!(facts[0].occluder.as_deref(), Some("box"));
    }

    #[test]
    fn test_underneath_actor_not_occluder() {
        let width = 100;
        let height = 100;
        let bg = [255, 255, 255, 255];
        let ink = [0, 0, 0, 255];
        let mut buf = create_test_frame(width, height, bg);
        fill_rect(&mut buf, width, 0, 0, 90, 30, ink);
        let frame = FrameView::new(width, height, &buf).unwrap();

        let under = ProbeActor {
            scene: "main".into(),
            label: "bg_rect".into(),
            bounds: Box2::new(0.0, 0.0, 90.0, 30.0),
            opacity: 1.0,
            is_text: false,
        };
        let text = ProbeActor {
            scene: "main".into(),
            label: "title".into(),
            bounds: Box2::new(0.0, 0.0, 90.0, 30.0),
            opacity: 1.0,
            is_text: true,
        };

        // under is drawn BEFORE text in draw_order.
        let facts = probe_occlusion(&frame, &[under, text], bg, DEFAULT_TOLERANCE);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].probed_points, 27);
        assert_eq!(facts[0].covered_points, 0);
        assert_eq!(facts[0].probe_coverage, 0.0);
        assert_eq!(facts[0].occluder, None);
    }

    #[test]
    fn test_probed_points_zero_all_background() {
        let width = 100;
        let height = 100;
        let bg = [255, 255, 255, 255];
        let buf = create_test_frame(width, height, bg);
        let frame = FrameView::new(width, height, &buf).unwrap();

        let text = ProbeActor {
            scene: "main".into(),
            label: "title".into(),
            bounds: Box2::new(0.0, 0.0, 90.0, 30.0),
            opacity: 1.0,
            is_text: true,
        };
        let blocker = ProbeActor {
            scene: "main".into(),
            label: "blocker".into(),
            bounds: Box2::new(0.0, 0.0, 90.0, 30.0),
            opacity: 1.0,
            is_text: false,
        };

        let facts = probe_occlusion(&frame, &[text, blocker], bg, DEFAULT_TOLERANCE);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].probed_points, 0);
        assert_eq!(facts[0].covered_points, 0);
        assert_eq!(facts[0].probe_coverage, 0.0);
        assert_eq!(facts[0].occluder, None);
    }

    #[test]
    fn test_opacity_threshold_text() {
        let width = 100;
        let height = 100;
        let bg = [255, 255, 255, 255];
        let ink = [0, 0, 0, 255];
        let mut buf = create_test_frame(width, height, bg);
        fill_rect(&mut buf, width, 0, 0, 90, 30, ink);
        let frame = FrameView::new(width, height, &buf).unwrap();

        let text_hidden = ProbeActor {
            scene: "main".into(),
            label: "hidden".into(),
            bounds: Box2::new(0.0, 0.0, 90.0, 30.0),
            opacity: 0.05,
            is_text: true,
        };
        let facts = probe_occlusion(&frame, &[text_hidden], bg, DEFAULT_TOLERANCE);
        assert!(facts.is_empty());

        let text_visible = ProbeActor {
            scene: "main".into(),
            label: "visible".into(),
            bounds: Box2::new(0.0, 0.0, 90.0, 30.0),
            opacity: 0.051,
            is_text: true,
        };
        let facts2 = probe_occlusion(&frame, &[text_visible], bg, DEFAULT_TOLERANCE);
        assert_eq!(facts2.len(), 1);
        assert_eq!(facts2[0].label, "visible");
    }

    #[test]
    fn test_occluder_tie_breaking_later_draw_order_wins() {
        let width = 100;
        let height = 100;
        let bg = [255, 255, 255, 255];
        let ink = [0, 0, 0, 255];
        let mut buf = create_test_frame(width, height, bg);
        fill_rect(&mut buf, width, 0, 0, 90, 30, ink);
        let frame = FrameView::new(width, height, &buf).unwrap();

        let text = ProbeActor {
            scene: "main".into(),
            label: "text".into(),
            bounds: Box2::new(0.0, 0.0, 90.0, 30.0),
            opacity: 1.0,
            is_text: true,
        };
        let occ1 = ProbeActor {
            scene: "main".into(),
            label: "occ_early".into(),
            bounds: Box2::new(0.0, 0.0, 90.0, 30.0),
            opacity: 1.0,
            is_text: false,
        };
        let occ2 = ProbeActor {
            scene: "main".into(),
            label: "occ_late".into(),
            bounds: Box2::new(0.0, 0.0, 90.0, 30.0),
            opacity: 1.0,
            is_text: false,
        };

        let facts = probe_occlusion(&frame, &[text, occ1, occ2], bg, DEFAULT_TOLERANCE);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].occluder.as_deref(), Some("occ_late"));
    }

    #[test]
    fn test_occluder_opacity_threshold() {
        let width = 100;
        let height = 100;
        let bg = [255, 255, 255, 255];
        let ink = [0, 0, 0, 255];
        let mut buf = create_test_frame(width, height, bg);
        fill_rect(&mut buf, width, 0, 0, 90, 30, ink);
        let frame = FrameView::new(width, height, &buf).unwrap();

        let text = ProbeActor {
            scene: "main".into(),
            label: "text".into(),
            bounds: Box2::new(0.0, 0.0, 90.0, 30.0),
            opacity: 1.0,
            is_text: true,
        };
        let semi_occ = ProbeActor {
            scene: "main".into(),
            label: "semi".into(),
            bounds: Box2::new(0.0, 0.0, 90.0, 30.0),
            opacity: 0.5,
            is_text: false,
        };

        let facts = probe_occlusion(&frame, &[text, semi_occ], bg, DEFAULT_TOLERANCE);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].covered_points, 0);
        assert_eq!(facts[0].probe_coverage, 0.0);
        assert_eq!(facts[0].occluder, None);
    }

    #[test]
    fn test_actor_outside_canvas() {
        let width = 100;
        let height = 100;
        let bg = [255, 255, 255, 255];
        let buf = create_test_frame(width, height, bg);
        let frame = FrameView::new(width, height, &buf).unwrap();

        let text = ProbeActor {
            scene: "main".into(),
            label: "offscreen".into(),
            bounds: Box2::new(-100.0, -100.0, -50.0, -50.0),
            opacity: 1.0,
            is_text: true,
        };

        let facts = probe_occlusion(&frame, &[text], bg, DEFAULT_TOLERANCE);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].probed_points, 0);
        assert_eq!(facts[0].covered_points, 0);
        assert_eq!(facts[0].probe_coverage, 0.0);
        assert_eq!(facts[0].occluder, None);
    }
}
