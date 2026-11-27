//! Identical local vector artwork can travel as one drawing in a FLIP overlay.

use animatix::primitives::RenderCommand;

pub(super) fn same_artwork(from: &[RenderCommand], to: &[RenderCommand]) -> bool {
    from.len() == to.len()
        && from.iter().zip(to).all(|(a, b)| match (a, b) {
            (RenderCommand::Paths { paths: a }, RenderCommand::Paths { paths: b }) => a == b,
            (
                RenderCommand::RoundedSurface {
                    rect,
                    radius,
                    fill,
                    stroke,
                    stroke_width,
                },
                RenderCommand::RoundedSurface {
                    rect: other_rect,
                    radius: other_radius,
                    fill: other_fill,
                    stroke: other_stroke,
                    stroke_width: other_width,
                },
            ) => {
                (rect, radius, fill, stroke, stroke_width)
                    == (other_rect, other_radius, other_fill, other_stroke, other_width)
            },
            // Other command families retain their own handoff or the two-drawing blend.
            _ => false,
        })
}
