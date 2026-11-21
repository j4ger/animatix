//! Read-only actor geometry members shared by the language and authoring tools.

/// The canvas and layout-box anchor names.
pub const BOX_POINT_MEMBERS: &[&str] = &[
    "top_left",
    "top",
    "top_right",
    "left",
    "center",
    "right",
    "bottom_left",
    "bottom",
    "bottom_right",
];

/// Additional read-only points on an actor.
pub const ACTOR_GEOMETRY_MEMBERS: &[&str] = &["origin", "centroid"];
