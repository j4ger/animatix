#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::timeline::property_track::{Interpolate, PropertyTrack};

/// Default half-size for layout bounds (`[50.0, 50.0]`).
pub const DEFAULT_LAYOUT_HALF_SIZE: [f32; 2] = [50.0, 50.0];
/// Default white color in RGBA (`[1.0, 1.0, 1.0, 1.0]`).
pub const DEFAULT_WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// Placement side for a targeted callout annotation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum CalloutPlace {
    /// Place on the right side of the target (default).
    #[default]
    Right,
    /// Place on the left side of the target.
    Left,
    /// Place above the target.
    Top,
    /// Place below the target.
    Bottom,
    /// Automatically pick the best side.
    Auto,
}

impl CalloutPlace {
    /// Parse from a string identifier or quoted string.
    /// Accepts `right`, `left`, `top`, `above`, `bottom`, `below`, `auto`.
    /// Returns `None` for unrecognised values.
    // Inherent `from_str` returns `Option` (not `Result`) and predates any
    // `FromStr` need; implementing the std trait would change the signature and
    // break call sites that rely on the `Option` return.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "right" => Some(Self::Right),
            "left" => Some(Self::Left),
            "top" | "above" => Some(Self::Top),
            "bottom" | "below" => Some(Self::Bottom),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }

    /// Canonical string representation (lowercase, matches source syntax).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Right => "right",
            Self::Left => "left",
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Auto => "auto",
        }
    }
}

impl std::fmt::Display for CalloutPlace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Controls whether an actor's position is managed by a layout container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum PlacementMode {
    /// Position is computed by the parent layout container.
    LayoutManaged,
    /// Position is set manually, ignoring layout.
    Manual,
}

impl PlacementMode {
    /// Return the stable source-level name of this mode.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::LayoutManaged => "layout_managed",
            Self::Manual => "manual",
        }
    }

    /// Parse a stable source-level mode name.
    // Inherent `from_str` returns `Option` (not `Result`) to match the
    // established parser helpers in this module; implementing the std trait
    // would change the signature and break call sites.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "layout_managed" => Some(Self::LayoutManaged),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }
}

impl std::fmt::Display for PlacementMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Controls how an actor's dimensions are interpreted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeMode {
    /// Absolute width and height.
    Size,
    /// Uniform scale factor.
    Scale,
}

impl Interpolate for PlacementMode {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { *self } else { *other }
    }
}

/// Anchor point on a 3×3 scene grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum SceneAnchor {
    /// Top-left corner.
    TopLeft,
    /// Top edge center.
    Top,
    /// Top-right corner.
    TopRight,
    /// Left edge center.
    Left,
    /// Center of the scene.
    Center,
    /// Right edge center.
    Right,
    /// Bottom-left corner.
    BottomLeft,
    /// Bottom edge center.
    Bottom,
    /// Bottom-right corner.
    BottomRight,
}

impl SceneAnchor {
    /// Parse a 9-point anchor name (the `scene.*` vocabulary) into a `SceneAnchor`.
    /// Returns `None` for unrecognised names.
    // Inherent `from_str` returns `Option` (not `Result`) to match the style of
    // `CalloutPlace::from_str`; implementing the std trait would change the
    // signature and break the pattern.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "top_left" => Some(Self::TopLeft),
            "top" => Some(Self::Top),
            "top_right" => Some(Self::TopRight),
            "left" => Some(Self::Left),
            "center" => Some(Self::Center),
            "right" => Some(Self::Right),
            "bottom_left" => Some(Self::BottomLeft),
            "bottom" => Some(Self::Bottom),
            "bottom_right" => Some(Self::BottomRight),
            _ => None,
        }
    }

    /// Canonical string representation (lowercase, matches source syntax).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TopLeft => "top_left",
            Self::Top => "top",
            Self::TopRight => "top_right",
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
            Self::BottomLeft => "bottom_left",
            Self::Bottom => "bottom",
            Self::BottomRight => "bottom_right",
        }
    }
}

impl Interpolate for SceneAnchor {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { *self } else { *other }
    }
}

/// Strategy for binding an actor's position to a reference frame.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum PositionBinding {
    /// Absolute coordinates in the scene.
    Absolute,
    /// Offset from a [`SceneAnchor`].
    SceneAnchor {
        /// Anchor point on the scene.
        anchor: SceneAnchor,
        /// Pixel offset from the anchor.
        offset: [f32; 2],
    },
    /// Percentage-based position within the scene, plus offset.
    ScenePercent {
        /// Horizontal percentage (0-1).
        x: f32,
        /// Vertical percentage (0-1).
        y: f32,
        /// Pixel offset from the computed position.
        offset: [f32; 2],
    },
    /// Default position inside a container, anchored at a [`SceneAnchor`].
    ContainerDefault {
        /// Anchor point within the container.
        anchor: SceneAnchor,
    },
    /// Percentage-based position inside a container.
    ContainerPercent {
        /// Horizontal percentage (0-1).
        x: f32,
        /// Vertical percentage (0-1).
        y: f32,
    },
}

impl Interpolate for PositionBinding {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        match (*self, *other) {
            (Self::Absolute, Self::Absolute) => Self::Absolute,
            (
                Self::SceneAnchor {
                    anchor,
                    offset: start_offset,
                },
                Self::SceneAnchor {
                    anchor: other_anchor,
                    offset: end_offset,
                },
            ) if anchor == other_anchor => Self::SceneAnchor {
                anchor,
                offset: start_offset.interpolate(&end_offset, t),
            },
            (
                Self::ScenePercent {
                    x: start_x,
                    y: start_y,
                    offset: start_offset,
                },
                Self::ScenePercent {
                    x: end_x,
                    y: end_y,
                    offset: end_offset,
                },
            ) => Self::ScenePercent {
                x: start_x.interpolate(&end_x, t),
                y: start_y.interpolate(&end_y, t),
                offset: start_offset.interpolate(&end_offset, t),
            },
            (
                Self::ContainerDefault { anchor },
                Self::ContainerDefault {
                    anchor: other_anchor,
                },
            ) if anchor == other_anchor => Self::ContainerDefault { anchor },
            (Self::ContainerPercent { x: x1, y: y1 }, Self::ContainerPercent { x: x2, y: y2 }) => {
                Self::ContainerPercent {
                    x: x1.interpolate(&x2, t),
                    y: y1.interpolate(&y2, t),
                }
            },
            _ => {
                if t < 0.5 {
                    *self
                } else {
                    *other
                }
            },
        }
    }
}

impl Interpolate for Option<crate::timeline::image::SceneImage> {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { self.clone() } else { other.clone() }
    }
}

impl Interpolate for Option<Vec<crate::renderer::types::VelloPath>> {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { self.clone() } else { other.clone() }
    }
}

// ─────────────────────────────────────────────────────────────
// GeometryTracks sub-struct
// ─────────────────────────────────────────────────────────────

/// Sub-struct holding all geometry-related property tracks.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GeometryTracks {
    /// Position track (x, y).
    pub position: Option<PropertyTrack<[f32; 2]>>,
    /// Motion offset applied after layout.
    pub motion_offset: Option<PropertyTrack<[f32; 2]>>,
    /// Rotation angle in radians.
    pub rotation: Option<PropertyTrack<f32>>,
    /// Scale factor.
    pub scale: Option<PropertyTrack<f32>>,
    /// 2×3 affine transform matrix.
    pub transform: Option<PropertyTrack<[f32; 6]>>,
    /// Whether the actor is layout-managed or manually placed.
    pub placement_mode: Option<PropertyTrack<PlacementMode>>,
    /// Position binding strategy.
    pub position_binding: Option<PropertyTrack<PositionBinding>>,
    /// Width and height.
    pub size: Option<PropertyTrack<[f32; 2]>>,
    /// Size allocated by the layout system.
    pub layout_size: Option<PropertyTrack<[f32; 2]>>,
    /// Size specification for percentage/auto/fill sizing (non-animated, set at build time).
    pub size_spec: Option<crate::timeline::taffy_layout::ChildSizeSpec>,
    /// Minimum width constraint.
    pub min_width: Option<PropertyTrack<f32>>,
    /// Minimum height constraint.
    pub min_height: Option<PropertyTrack<f32>>,
    /// Maximum height constraint.
    pub max_height: Option<PropertyTrack<f32>>,
    /// Label position offset for callout annotations.
    pub label_at: Option<PropertyTrack<[f32; 2]>>,
    /// Target actor path for targeted callout mode.
    pub callout_target: Option<PropertyTrack<String>>,
    /// Placement hint for targeted callout.
    pub callout_place: Option<PropertyTrack<CalloutPlace>>,
    /// Standoff distance between callout tip and target.
    pub callout_standoff: Option<PropertyTrack<f32>>,
    /// Offset applied to the callout anchor on the target side.
    pub callout_to_offset: Option<PropertyTrack<[f32; 2]>>,
    /// Connector start endpoint (actor anchor or coordinate point).
    pub connector_from: Option<PropertyTrack<ConnectorEndpoint>>,
    /// Connector end endpoint (actor anchor or coordinate point).
    pub connector_to: Option<PropertyTrack<ConnectorEndpoint>>,
    /// Connector routing strategy (Elbow, LBend, Straight).
    pub connector_routing: Option<PropertyTrack<ConnectorRouting>>,
    /// Connector arrowhead toggle.
    pub connector_arrow: Option<PropertyTrack<bool>>,
    /// Audio playback volume track.
    pub volume: Option<PropertyTrack<f32>>,
}

/// Endpoint of a smart connector (either an explicit point or a target actor anchor).
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ConnectorEndpoint {
    /// Explicit (x, y) coordinate point.
    Point([f32; 2]),
    /// Named actor with an optional side anchor.
    Target {
        /// Target actor label.
        actor: String,
        /// Anchor side on the target actor.
        anchor: Option<SceneAnchor>,
    },
}

impl Default for ConnectorEndpoint {
    fn default() -> Self {
        Self::Point([0.0, 0.0])
    }
}

impl Interpolate for ConnectorEndpoint {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        match (self, other) {
            (Self::Point(a), Self::Point(b)) => {
                Self::Point([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t])
            },
            _ => {
                if t < 0.5 {
                    self.clone()
                } else {
                    other.clone()
                }
            },
        }
    }
}

/// Routing algorithm for smart connectors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ConnectorRouting {
    /// 2-bend orthogonal routing with bus margin.
    #[default]
    Elbow,
    /// 1-bend L-shape orthogonal routing.
    LBend,
    /// Direct straight point-to-point connection.
    Straight,
}

impl std::str::FromStr for ConnectorRouting {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "elbow" => Ok(Self::Elbow),
            "l-bend" | "l_bend" | "lbend" => Ok(Self::LBend),
            "straight" => Ok(Self::Straight),
            _ => Err(()),
        }
    }
}

impl Interpolate for ConnectorRouting {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { *self } else { *other }
    }
}

impl GeometryTracks {
    /// Latest keyframe across the geometry property tracks.
    pub fn max_keyframe_time(&self) -> Option<u64> {
        macro_rules! latest {
            ($($field:ident),* $(,)?) => {
                [$(self.$field.as_ref().and_then(|t| t.last_keyframe_time())),*]
                    .into_iter()
                    .flatten()
                    .max()
            };
        }
        latest!(
            position,
            motion_offset,
            rotation,
            scale,
            transform,
            placement_mode,
            position_binding,
            size,
            layout_size,
            min_width,
            min_height,
            max_height,
            label_at,
            callout_target,
            callout_place,
            callout_standoff,
            callout_to_offset,
            connector_from,
            connector_to,
            connector_routing,
            connector_arrow,
            volume,
        )
    }

    /// Whether any geometry property track has animated keyframes.
    pub fn has_any_keyframes(&self) -> bool {
        macro_rules! any_animated {
            ($($field:ident),* $(,)?) => {
                false $(|| self.$field.as_ref().is_some_and(|t| !t.is_effectively_static()))*
            };
        }
        any_animated!(
            position,
            motion_offset,
            rotation,
            scale,
            transform,
            placement_mode,
            position_binding,
            size,
            layout_size,
            min_width,
            min_height,
            max_height,
            label_at,
            callout_target,
            callout_place,
            callout_standoff,
            callout_to_offset,
            connector_from,
            connector_to,
            connector_routing,
            connector_arrow,
            volume,
        )
    }
}
