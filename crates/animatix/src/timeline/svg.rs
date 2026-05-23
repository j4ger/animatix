use super::VelloPath;
use kurbo::{BezPath, Shape};
use usvg::{Node, Options, Tree};
use vello::peniko::Color;

/// Compute the half-size bounding box of a set of SVG paths.
pub fn measure_svg_paths(paths: &[VelloPath]) -> [f32; 2] {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for path in paths {
        let bounds = path.path.bounding_box();
        min_x = min_x.min(bounds.x0);
        max_x = max_x.max(bounds.x1);
        min_y = min_y.min(bounds.y0);
        max_y = max_y.max(bounds.y1);
    }

    if min_x.is_finite() && max_x.is_finite() && min_y.is_finite() && max_y.is_finite() {
        [
            ((max_x - min_x) as f32) / 2.0,
            ((max_y - min_y) as f32) / 2.0,
        ]
    } else {
        [0.0, 0.0]
    }
}

/// Center a set of SVG paths around the origin by translating their bounding box.
pub fn center_svg_paths(paths: &mut [VelloPath]) {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for path in paths.iter() {
        let bounds = path.path.bounding_box();
        min_x = min_x.min(bounds.x0);
        max_x = max_x.max(bounds.x1);
        min_y = min_y.min(bounds.y0);
        max_y = max_y.max(bounds.y1);
    }

    if min_x.is_finite() && max_x.is_finite() && min_y.is_finite() && max_y.is_finite() {
        let center_x = (min_x + max_x) / 2.0;
        let center_y = (min_y + max_y) / 2.0;
        let offset = kurbo::Affine::translate((-center_x, -center_y));

        for path in paths.iter_mut() {
            path.path.apply_affine(offset);
        }
    }
}

/// Parse an SVG string into a list of `VelloPath` objects.
pub fn parse_svg(svg_data: &str) -> Result<Vec<VelloPath>, String> {
    let opt = Options::default();
    let tree = Tree::from_str(svg_data, &opt).map_err(|error| format!("{error:?}"))?;

    let mut paths = Vec::new();
    let root = tree.root();

    fn visit(group: &usvg::Group, paths: &mut Vec<VelloPath>) {
        for node in group.children() {
            match node {
                Node::Group(g) => visit(g, paths),
                Node::Path(p) => {
                    let mut bez_path = BezPath::new();
                    for segment in p.data().segments() {
                        match segment {
                            usvg::tiny_skia_path::PathSegment::MoveTo(pt) => {
                                bez_path.move_to((pt.x as f64, pt.y as f64));
                            }
                            usvg::tiny_skia_path::PathSegment::LineTo(pt) => {
                                bez_path.line_to((pt.x as f64, pt.y as f64));
                            }
                            usvg::tiny_skia_path::PathSegment::QuadTo(p1, p2) => {
                                bez_path.quad_to(
                                    (p1.x as f64, p1.y as f64),
                                    (p2.x as f64, p2.y as f64),
                                );
                            }
                            usvg::tiny_skia_path::PathSegment::CubicTo(p1, p2, p3) => {
                                bez_path.curve_to(
                                    (p1.x as f64, p1.y as f64),
                                    (p2.x as f64, p2.y as f64),
                                    (p3.x as f64, p3.y as f64),
                                );
                            }
                            usvg::tiny_skia_path::PathSegment::Close => {
                                bez_path.close_path();
                            }
                        }
                    }

                    let mut fill_color = None;
                    if let Some(fill) = p.fill() {
                        if let usvg::Paint::Color(c) = fill.paint() {
                            fill_color = Some(Color::from_rgba8(
                                c.red,
                                c.green,
                                c.blue,
                                fill.opacity().to_u8(),
                            ));
                        } else {
                            fill_color = Some(Color::BLACK);
                        }
                    }

                    let mut stroke_opts = None;
                    if let Some(stroke) = p.stroke() {
                        if let usvg::Paint::Color(c) = stroke.paint() {
                            stroke_opts = Some((
                                Color::from_rgba8(c.red, c.green, c.blue, stroke.opacity().to_u8()),
                                stroke.width().get(),
                            ));
                        } else {
                            stroke_opts = Some((Color::BLACK, stroke.width().get()));
                        }
                    }

                    let t = p.abs_transform();
                    let affine = kurbo::Affine::new([
                        t.sx as f64,
                        t.ky as f64,
                        t.kx as f64,
                        t.sy as f64,
                        t.tx as f64,
                        t.ty as f64,
                    ]);
                    let bez_path = affine * bez_path;

                    paths.push(VelloPath {
                        path: bez_path,
                        fill: fill_color,
                        stroke: stroke_opts,
                    });
                }
                Node::Image(_) => {}
                Node::Text(_) => {}
            }
        }
    }

    visit(root, &mut paths);
    center_svg_paths(&mut paths);

    Ok(paths)
}

/// Read and parse an SVG file.
pub fn parse_svg_file(path: &str) -> Result<Vec<VelloPath>, String> {
    let svg_content =
        std::fs::read_to_string(path).map_err(|error| format!("Failed to read SVG file '{path}': {error}"))?;
    parse_svg(&svg_content)
}
