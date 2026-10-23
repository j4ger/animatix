use super::*;

#[test]
fn line_reports_tip_lookup_support() {
    assert!(vector_shape_exposes_tip_size(ShapeType::Line));
    assert!(!vector_shape_exposes_tip_size(ShapeType::Rect));
}

#[test]
fn polygon_shapes_report_custom_path_usage() {
    assert!(vector_shape_uses_custom_path(ShapeType::Polygon));
    assert!(vector_shape_uses_custom_path(ShapeType::Path));
    assert!(!vector_shape_uses_custom_path(ShapeType::Rect));
}

#[test]
fn stroke_drawn_plots_get_a_default_width() {
    // Regression: VectorField/ContourSet draw arrows/contours via `stroke`,
    // so a zero default stroke width made them invisible when the user set
    // only `color:` (which is used as their stroke color).
    assert_eq!(default_stroke_width("VectorField"), 2.0);
    assert_eq!(default_stroke_width("ContourSet"), 2.0);
    // Filled shapes stay stroke-less by default.
    assert_eq!(default_stroke_width("Rect"), 0.0);
    assert_eq!(default_stroke_width("Ellipse"), 0.0);
}
