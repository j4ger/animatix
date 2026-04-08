use kurbo::{BezPath, CubicBez, ParamCurve, PathEl, Point};

/// LEVEL 1: Align Lists of Paths
pub fn align_path_lists(source: &[BezPath], target: &[BezPath]) -> Vec<(BezPath, BezPath)> {
    let mut result = Vec::new();
    let max_len = source.len().max(target.len());

    let empty_path = BezPath::new();

    for i in 0..max_len {
        let src = if i < source.len() {
            source[i].clone()
        } else {
            // Degenerate source
            let centroid = get_centroid(target.get(i).unwrap_or(&empty_path));
            BezPath::from_vec(vec![PathEl::MoveTo(centroid)])
        };

        let tgt = if i < target.len() {
            target[i].clone()
        } else {
            // Degenerate target
            let centroid = get_centroid(&src);
            BezPath::from_vec(vec![PathEl::MoveTo(centroid)])
        };

        result.push((src, tgt));
    }

    result
}

/// LEVEL 2: Align Subpaths within a Path
pub fn align_subpaths(source: &BezPath, target: &BezPath) -> (BezPath, BezPath) {
    let src_subs = extract_subpaths(source);
    let tgt_subs = extract_subpaths(target);

    let max_len = src_subs.len().max(tgt_subs.len());

    let mut new_src = BezPath::new();
    let mut new_tgt = BezPath::new();

    for i in 0..max_len {
        let src_sub = if i < src_subs.len() {
            src_subs[i].clone()
        } else {
            let centroid = get_subpath_centroid(tgt_subs.get(i).unwrap_or(&Vec::new()));
            vec![PathEl::MoveTo(centroid)]
        };

        let tgt_sub = if i < tgt_subs.len() {
            tgt_subs[i].clone()
        } else {
            let centroid = get_subpath_centroid(&src_sub);
            vec![PathEl::MoveTo(centroid)]
        };

        // Align segments within this subpath
        let (aligned_src, aligned_tgt) = align_segments(&src_sub, &tgt_sub);
        for el in aligned_src {
            new_src.push(el);
        }
        for el in aligned_tgt {
            new_tgt.push(el);
        }
    }

    (new_src, new_tgt)
}

/// LEVEL 3: Align Segments within a Subpath
pub fn align_segments(
    source_subpath: &[PathEl],
    target_subpath: &[PathEl],
) -> (Vec<PathEl>, Vec<PathEl>) {
    let mut src_curves = to_curves(source_subpath);
    let mut tgt_curves = to_curves(target_subpath);

    // Make sure they have the same number of elements
    while src_curves.len() < tgt_curves.len() && !src_curves.is_empty() {
        if !split_longest(&mut src_curves) {
            break;
        }
    }
    while tgt_curves.len() < src_curves.len() && !tgt_curves.is_empty() {
        if !split_longest(&mut tgt_curves) {
            break;
        }
    }

    // If one is completely empty but the other isn't (e.g. degenerate single point), pad the empty one
    let src_has_curve = src_curves
        .iter()
        .any(|el| matches!(el, PathEl::CurveTo(..)));
    let tgt_has_curve = tgt_curves
        .iter()
        .any(|el| matches!(el, PathEl::CurveTo(..)));

    if tgt_has_curve && !src_has_curve {
        let pt = get_subpath_centroid(source_subpath);
        if src_curves.is_empty() {
            src_curves.push(PathEl::MoveTo(pt));
        }
        while src_curves.len() < tgt_curves.len() {
            src_curves.push(PathEl::CurveTo(pt, pt, pt));
        }
    } else if src_has_curve && !tgt_has_curve {
        let pt = get_subpath_centroid(target_subpath);
        if tgt_curves.is_empty() {
            tgt_curves.push(PathEl::MoveTo(pt));
        }
        while tgt_curves.len() < src_curves.len() {
            tgt_curves.push(PathEl::CurveTo(pt, pt, pt));
        }
    } else if !src_has_curve && !tgt_has_curve {
        let pt_src = get_subpath_centroid(source_subpath);
        let pt_tgt = get_subpath_centroid(target_subpath);
        if src_curves.is_empty() {
            src_curves.push(PathEl::MoveTo(pt_src));
        }
        if tgt_curves.is_empty() {
            tgt_curves.push(PathEl::MoveTo(pt_tgt));
        }
        while src_curves.len() < tgt_curves.len() {
            src_curves.push(PathEl::CurveTo(pt_src, pt_src, pt_src));
        }
        while tgt_curves.len() < src_curves.len() {
            tgt_curves.push(PathEl::CurveTo(pt_tgt, pt_tgt, pt_tgt));
        }
    }

    (src_curves, tgt_curves)
}

/// LEVEL 4: Interpolate
pub fn morph_paths(source: &BezPath, target: &BezPath, t: f64) -> BezPath {
    let mut result = BezPath::new();

    let (aligned_src, aligned_tgt) = align_subpaths(source, target);

    for (s, tr) in aligned_src
        .elements()
        .iter()
        .zip(aligned_tgt.elements().iter())
    {
        match (s, tr) {
            (PathEl::MoveTo(p1), PathEl::MoveTo(p2)) => {
                result.push(PathEl::MoveTo(lerp_point(*p1, *p2, t)));
            }
            (PathEl::LineTo(p1), PathEl::LineTo(p2)) => {
                result.push(PathEl::LineTo(lerp_point(*p1, *p2, t)));
            }
            (PathEl::QuadTo(p1a, p1b), PathEl::QuadTo(p2a, p2b)) => {
                result.push(PathEl::QuadTo(
                    lerp_point(*p1a, *p2a, t),
                    lerp_point(*p1b, *p2b, t),
                ));
            }
            (PathEl::CurveTo(p1a, p1b, p1c), PathEl::CurveTo(p2a, p2b, p2c)) => {
                result.push(PathEl::CurveTo(
                    lerp_point(*p1a, *p2a, t),
                    lerp_point(*p1b, *p2b, t),
                    lerp_point(*p1c, *p2c, t),
                ));
            }
            (PathEl::ClosePath, PathEl::ClosePath) => {
                result.push(PathEl::ClosePath);
            }
            _ => {
                // If they are not the same type, fallback to target (should not happen if aligned properly)
                result.push(*tr);
            }
        }
    }

    result
}

// Helpers

fn lerp_point(p1: Point, p2: Point, t: f64) -> Point {
    Point::new(p1.x + (p2.x - p1.x) * t, p1.y + (p2.y - p1.y) * t)
}

fn get_centroid(path: &BezPath) -> Point {
    get_subpath_centroid(path.elements())
}

fn get_subpath_centroid(subpath: &[PathEl]) -> Point {
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut count = 0.0;

    for el in subpath {
        match el {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => {
                sum_x += p.x;
                sum_y += p.y;
                count += 1.0;
            }
            PathEl::QuadTo(_, p) => {
                sum_x += p.x;
                sum_y += p.y;
                count += 1.0;
            }
            PathEl::CurveTo(_, _, p) => {
                sum_x += p.x;
                sum_y += p.y;
                count += 1.0;
            }
            PathEl::ClosePath => {}
        }
    }

    if count == 0.0 {
        Point::ZERO
    } else {
        Point::new(sum_x / count, sum_y / count)
    }
}

fn extract_subpaths(path: &BezPath) -> Vec<Vec<PathEl>> {
    let mut subs = Vec::new();
    let mut current = Vec::new();

    for el in path.elements() {
        if matches!(el, PathEl::MoveTo(_)) && !current.is_empty() {
            subs.push(current);
            current = Vec::new();
        }
        current.push(*el);
    }
    if !current.is_empty() {
        subs.push(current);
    }

    subs
}

fn to_curves(subpath: &[PathEl]) -> Vec<PathEl> {
    let mut result = Vec::new();
    let mut last_pt = Point::ZERO;
    let mut first_pt = Point::ZERO;

    for el in subpath {
        match el {
            PathEl::MoveTo(p) => {
                result.push(PathEl::MoveTo(*p));
                last_pt = *p;
                first_pt = *p;
            }
            PathEl::LineTo(p) => {
                // convert line to curve
                let p1 = lerp_point(last_pt, *p, 1.0 / 3.0);
                let p2 = lerp_point(last_pt, *p, 2.0 / 3.0);
                result.push(PathEl::CurveTo(p1, p2, *p));
                last_pt = *p;
            }
            PathEl::QuadTo(p1, p2) => {
                let cp1 = lerp_point(last_pt, *p1, 2.0 / 3.0);
                let cp2 = lerp_point(*p2, *p1, 2.0 / 3.0);
                result.push(PathEl::CurveTo(cp1, cp2, *p2));
                last_pt = *p2;
            }
            PathEl::CurveTo(p1, p2, p3) => {
                result.push(PathEl::CurveTo(*p1, *p2, *p3));
                last_pt = *p3;
            }
            PathEl::ClosePath => {
                // Optional: convert ClosePath to line/curve back to first_pt
                let p = first_pt;
                let p1 = lerp_point(last_pt, p, 1.0 / 3.0);
                let p2 = lerp_point(last_pt, p, 2.0 / 3.0);
                result.push(PathEl::CurveTo(p1, p2, p));
                result.push(PathEl::ClosePath);
                last_pt = p;
            }
        }
    }
    result
}

fn split_longest(curves: &mut Vec<PathEl>) -> bool {
    let mut max_len = -1.0;
    let mut max_idx = 0;
    let mut last_pt = Point::ZERO;
    let mut max_last_pt = Point::ZERO;

    for (i, el) in curves.iter().enumerate() {
        match el {
            PathEl::MoveTo(p) => last_pt = *p,
            PathEl::CurveTo(p1, p2, p3) => {
                let dist = last_pt.distance(*p1) + p1.distance(*p2) + p2.distance(*p3);
                if dist > max_len {
                    max_len = dist;
                    max_idx = i;
                    max_last_pt = last_pt;
                }
                last_pt = *p3;
            }
            _ => {}
        }
    }

    if max_len >= 0.0 {
        if let PathEl::CurveTo(p1, p2, p3) = curves[max_idx] {
            let bez = CubicBez::new(max_last_pt, p1, p2, p3);
            let b1 = bez.subsegment(0.0..0.5);
            let b2 = bez.subsegment(0.5..1.0);
            curves[max_idx] = PathEl::CurveTo(b1.p1, b1.p2, b1.p3);
            curves.insert(max_idx + 1, PathEl::CurveTo(b2.p1, b2.p2, b2.p3));
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_alignment() {
        let p1 = BezPath::from_vec(vec![
            PathEl::MoveTo(Point::new(0.0, 0.0)),
            PathEl::LineTo(Point::new(10.0, 0.0)),
        ]);
        let p2 = BezPath::from_vec(vec![
            PathEl::MoveTo(Point::new(0.0, 10.0)),
            PathEl::LineTo(Point::new(10.0, 10.0)),
        ]);

        let source = vec![p1.clone(), p2.clone()];
        let target = vec![p1.clone(), p2.clone(), p1.clone(), p2.clone(), p1.clone()];

        let aligned = align_path_lists(&source, &target);
        assert_eq!(aligned.len(), 5);

        // Check that 3rd item in source is degenerate
        let (s3, _t3) = &aligned[2];
        assert_eq!(s3.elements().len(), 1);
        assert!(matches!(s3.elements()[0], PathEl::MoveTo(_)));
    }

    #[test]
    fn test_subpath_alignment() {
        let mut source = BezPath::new();
        source.move_to(Point::new(0.0, 0.0));
        source.line_to(Point::new(10.0, 0.0));

        let mut target = BezPath::new();
        target.move_to(Point::new(0.0, 0.0));
        target.line_to(Point::new(10.0, 0.0));
        target.move_to(Point::new(20.0, 0.0));
        target.line_to(Point::new(30.0, 0.0));

        let (a_src, a_tgt) = align_subpaths(&source, &target);

        // 2 subpaths each
        let src_subs = extract_subpaths(&a_src);
        let tgt_subs = extract_subpaths(&a_tgt);
        assert_eq!(src_subs.len(), 2);
        assert_eq!(tgt_subs.len(), 2);
    }

    #[test]
    fn test_segment_alignment() {
        // Triangle
        let mut source = BezPath::new();
        source.move_to(Point::new(0.0, 0.0));
        source.line_to(Point::new(10.0, 0.0));
        source.line_to(Point::new(5.0, 10.0));
        source.close_path();

        // Square
        let mut target = BezPath::new();
        target.move_to(Point::new(0.0, 0.0));
        target.line_to(Point::new(10.0, 0.0));
        target.line_to(Point::new(10.0, 10.0));
        target.line_to(Point::new(0.0, 10.0));
        target.close_path();

        let subs_src = extract_subpaths(&source);
        let subs_tgt = extract_subpaths(&target);

        let (a_src, a_tgt) = align_segments(&subs_src[0], &subs_tgt[0]);
        assert_eq!(a_src.len(), a_tgt.len());

        // Everything should be MoveTo, CurveTo, or ClosePath
        for el in &a_src {
            assert!(matches!(
                el,
                PathEl::MoveTo(_) | PathEl::CurveTo(_, _, _) | PathEl::ClosePath
            ));
        }
    }

    #[test]
    fn test_morphing() {
        let mut source = BezPath::new();
        source.move_to(Point::new(0.0, 0.0));
        source.line_to(Point::new(10.0, 0.0));

        let mut target = BezPath::new();
        target.move_to(Point::new(0.0, 10.0));
        target.line_to(Point::new(10.0, 10.0));

        let t50 = morph_paths(&source, &target, 0.5);
        let els = t50.elements();

        if let PathEl::MoveTo(p) = els[0] {
            assert_eq!(p.y, 5.0);
        } else {
            panic!("Expected MoveTo");
        }
    }
}
