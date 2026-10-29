//! Pre-spawn actor-anchor reference pass.

use super::*;
use crate::ast::{Expr, Property};

impl Timeline {
    /// Record `from:`/`to:` actor-anchor refs (`pen.center`) in the track's
    /// shape side channel for frame-time resolution.
    ///
    /// This runs before `begin_actor_decl` spawns the track: only a track that
    /// already exists can receive the side channel here, so a first declaration
    /// writes nothing and must not spawn one just to carry the ref. The order is
    /// deliberate; moving this after the spawn is not an equivalent refactor.
    pub(super) fn apply_pre_spawn_line_anchor_refs(
        &mut self,
        label: &str,
        caps: &ActorCaps,
        props: &[Property],
    ) {
        if !matches!(caps.shape, Some(ShapeKind::Line | ShapeKind::Arrow)) {
            return;
        }
        let Some(id) = self.world.get_id(label) else {
            return;
        };
        let shape = self.world.ensure_shape(id);
        for prop in props {
            if prop.name != "from" && prop.name != "to" {
                continue;
            }
            let Expr::Path(segments) = &prop.value else {
                continue;
            };
            if segments.len() != 2 {
                continue;
            }
            let Some(anchor) = SceneAnchor::from_str(&segments[1]) else {
                continue;
            };
            if prop.name == "from" {
                shape.from_anchor = Some((segments[0].clone(), anchor));
            } else {
                shape.to_anchor = Some((segments[0].clone(), anchor));
            }
        }
    }
}
