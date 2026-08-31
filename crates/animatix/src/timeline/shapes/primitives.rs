// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::timeline::VectorShapeState;
    use crate::timeline::shapes::EllipseState;

    #[test]
    fn ellipse_default_size_is_standard() {
        use crate::primitives::Primitive;
        let mut state = VectorShapeState::Ellipse(EllipseState {
            size: [50.0, 50.0],
            arc_angles: [0.0, 0.0],
            rotation: 0.0,
        });
        crate::primitives::ELLIPSE.apply_defaults(&mut state);
        // Ellipse uses standard size defaults
        assert_eq!(state.size(), [50.0, 50.0]);
    }

    #[test]
    fn line_reports_tip_lookup_support() {
        use crate::timeline::ShapeType;
        use crate::timeline::shapes::vector_shape_exposes_tip_size;
        assert!(vector_shape_exposes_tip_size(ShapeType::Line));
        assert!(!vector_shape_exposes_tip_size(ShapeType::Rect));
    }

    #[test]
    fn polygon_shapes_report_custom_path_usage() {
        use crate::timeline::ShapeType;
        use crate::timeline::shapes::vector_shape_uses_custom_path;
        assert!(vector_shape_uses_custom_path(ShapeType::Polygon));
        assert!(vector_shape_uses_custom_path(ShapeType::Path));
        assert!(!vector_shape_uses_custom_path(ShapeType::Rect));
    }
}
