fn main() {
    let g: typst::visualize::Geometry = unreachable!();
    match g {
        typst::visualize::Geometry::Line(_) => {}
        typst::visualize::Geometry::Rect(_) => {}
        typst::visualize::Geometry::Path(_) => {}
        typst::visualize::Geometry::Polygon(_) => {}
        _ => {}
    }
}
