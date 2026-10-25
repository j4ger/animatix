//! Public timeline data types shared by build, evaluation and the GUI.
//!
//! Split out of `timeline/mod.rs` so the module root stays a table of
//! contents and re-exports. Behaviour lives with the systems that use these
//! types.

use std::collections::BTreeMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::timeline::PlacementMode;
use crate::timeline::World;
use crate::timeline::entity::EntityId;
use crate::timeline::env::Value;
use crate::timeline::property_track::TrackAccessor;

// ─────────────────────────────────────────────────────────────
// Build quality levels (Phase 6.3)
// ─────────────────────────────────────────────────────────────

// Build quality levels (Phase 6.3)
// ─────────────────────────────────────────────────────────────

/// Controls plot sampling fidelity during timeline build.
///
/// - `Draft` — fastest, used during live GUI editing.
/// - `Preview` — balanced, used when paused or scrubbing.
/// - `Production` — maximum fidelity, used for export.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum BuildQuality {
    /// Fastest quality for live GUI editing.
    #[default]
    Draft,
    /// Balanced quality for paused/scrubbing preview.
    Preview,
    /// Maximum fidelity for export.
    Production,
}

impl BuildQuality {
    /// Apply quality scaling to plot sampling parameters.
    pub fn scale_plot_params(
        &self,
        tolerance: &mut f64,
        max_depth: &mut usize,
        resolution: &mut usize,
    ) {
        match self {
            BuildQuality::Draft => {
                *tolerance = (*tolerance * 4.0).max(0.5);
                *max_depth = max_depth.saturating_sub(4).max(6);
                *resolution = (*resolution / 4).max(16);
            },
            BuildQuality::Preview => {
                *tolerance = (*tolerance * 2.0).max(0.5);
                *max_depth = max_depth.saturating_sub(2).max(6);
                *resolution = (*resolution / 2).max(16);
            },
            BuildQuality::Production => {
                // No scaling — use values as-is (already clamped by caller)
            },
        }
    }
}

/// Layout strategy for container actors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum LayoutType {
    /// Horizontal left-to-right flow.
    Row,
    /// Vertical top-to-bottom flow.
    Col,
    /// CSS-like grid with configurable columns.
    Grid,
    /// Absolute/manual positioning.
    Stack,
}

impl LayoutType {
    /// Parse a container type string into a `LayoutType`.
    ///
    /// Defaults to `Row` for unrecognized names.
    pub fn from_container_ty(container_ty: &str) -> Self {
        match container_ty {
            "Row" => Self::Row,
            "Col" => Self::Col,
            "Grid" => Self::Grid,
            "Stack" => Self::Stack,
            _ => Self::Row,
        }
    }

    /// Return the string representation of this layout type.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Row => "Row",
            Self::Col => "Col",
            Self::Grid => "Grid",
            Self::Stack => "Stack",
        }
    }
}

/// A child actor admitted into a container's layout system.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContainerLayoutChild {
    /// Layout-admitted child label.
    ///
    /// This is a validated subset of scene-graph children used only for layout
    /// membership/order. Children excluded here still remain in the entity's
    /// `SceneHierarchyComponent` for scene traversal/rendering.
    pub label: String,
    /// Cached placement mode from build time to avoid repeated track lookups.
    pub placement_mode: PlacementMode,
}

/// Metadata describing the layout configuration of a container actor.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ContainerMetadata {
    /// Row, Col, Grid, or Stack.
    pub layout_type: LayoutType,
    /// Gap between children in logical pixels.
    /// `[0]` = main-axis gap, `[1]` = cross-axis gap.
    pub gap: [f32; 2],
    /// Padding inside the container bounds in logical pixels.
    /// `[left, top, right, bottom]`.
    pub padding: [f32; 4],
    /// Cross-axis alignment string (e.g. "center", "start").
    pub align: String,
    /// Vertical alignment string ("center", "baseline", "top", "bottom").
    /// Default is "center" for backward compatibility.
    pub vertical_align: String,
    /// Number of columns when `layout_type` is `Grid`.
    pub cols: Option<usize>,
    /// Raw authored child order snapshot for this container, as entity ids.
    ///
    /// This is retained for debugging/tests and for preserving the distinction
    /// between authored scene-graph membership and admitted layout membership.
    /// Layout-admitted children are derived from this on demand via
    /// `Timeline::layout_children_for` or `ContainerMetadata::layout_children`.
    pub child_order: Vec<EntityId>,
}

/// Create a uniform `[f32; 2]` gap array where both axes use the same value.
pub fn gap_uniform(v: f32) -> [f32; 2] {
    [v, v]
}

/// Create a uniform `[f32; 4]` padding array where all sides use the same value.
pub fn padding_uniform(v: f32) -> [f32; 4] {
    [v, v, v, v]
}

impl ContainerMetadata {
    /// Compute layout-admitted children on demand from `child_order` + the world.
    ///
    /// `child_order` holds entity ids; Taffy/layout admission consumes labels,
    /// so each id is mapped once here and the child's track supplies its
    /// placement mode. Ids without a live track (a stale carried order) are
    /// dropped, matching the old label lookup's missing-label behaviour.
    pub fn layout_children(&self, world: &World) -> Vec<ContainerLayoutChild> {
        self.child_order
            .iter()
            .filter_map(|child_id| {
                if !world.exists(*child_id) {
                    return None;
                }
                let geometry = world.geometry(*child_id);
                geometry.layout_size.as_ref()?;
                Some(ContainerLayoutChild {
                    label: world.label(*child_id)?.to_string(),
                    placement_mode: geometry.placement_mode.last(PlacementMode::LayoutManaged),
                })
            })
            .collect()
    }
}

/// Layout engine for computing child positions inside containers.
///
/// Caches layout results per-container to avoid redundant Taffy computation
/// when children's layout sizes haven't changed between frames.
#[derive(Clone, Debug)]
pub struct LayoutEngine {
    /// Per-container cache bucket keyed by container label (PF-4: the label is
    /// borrowed for lookup — no `String` is allocated per frame). Static layout
    /// inputs are implied by the bucket; only per-frame dynamic inputs
    /// (`layout_size`, baselines) are compared inside it.
    pub(crate) cache: std::cell::RefCell<
        std::collections::HashMap<String, crate::timeline::layout::LayoutContainerCache>,
    >,
}

impl Default for LayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Width and height of the output scene in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SceneDimensions {
    /// Scene width in pixels.
    pub width: u32,
    /// Scene height in pixels.
    pub height: u32,
}

/// Optional debug overlays for the renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct DebugRenderOptions {
    /// Draw bounding-box outlines around actors.
    pub draw_bounds: bool,
    /// P2.24: When true, compute hit regions during evaluation.
    /// The GUI sets this to true when it needs click-to-select data.
    pub compute_hit_regions: bool,
    /// Show layout-specific debug info (container labels, slot outlines, sizes).
    pub draw_layout_debug: bool,
    /// Show padding and gap regions as semi-transparent overlays.
    pub draw_spacing: bool,
}

impl Default for SceneDimensions {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
        }
    }
}

/// A single audio segment to be muxed during export.
#[derive(Clone, Debug)]
pub struct AudioSegment {
    /// Path or identifier of the audio asset.
    pub source: String,
    /// Start time within the global timeline in seconds.
    pub start_time_s: f64,
    /// Playback duration in seconds.
    pub duration_s: Option<f64>,
    /// Playback volume multiplier (1.0 = full volume).
    pub volume: f32,
    /// Optional animated volume envelope (sample_time_s, gain 0.0..1.0).
    pub volume_envelope: Option<Vec<(f64, f32)>>,
}

impl AudioSegment {
    /// Evaluate the volume gain at a given timeline time in seconds.
    pub fn volume_at(&self, time_s: f64) -> f32 {
        if let Some(env) = &self.volume_envelope {
            if env.is_empty() {
                return self.volume;
            }
            if time_s <= env[0].0 {
                return env[0].1;
            }
            if time_s >= env.last().unwrap().0 {
                return env.last().unwrap().1;
            }
            for i in 0..env.len() - 1 {
                let (t0, v0) = env[i];
                let (t1, v1) = env[i + 1];
                if time_s >= t0 && time_s <= t1 {
                    if (t1 - t0).abs() < 1e-6 {
                        return v1;
                    }
                    let factor = ((time_s - t0) / (t1 - t0)) as f32;
                    return v0 + (v1 - v0) * factor;
                }
            }
            self.volume
        } else {
            self.volume
        }
    }
}

/// A piecewise-constant variable track defined by `let` declarations in keyframes.
/// Evaluates to the value of the most recent keyframe at or before the query time.
#[derive(Clone, Debug, Default)]
pub struct VariableTrack {
    /// Map from time in milliseconds to the variable value at that keyframe.
    pub keyframes: BTreeMap<u64, Value>,
}

impl VariableTrack {
    /// Evaluate the variable at the given time, returning the most recent keyframe value.
    pub fn evaluate(&self, time_ms: u64) -> Option<Value> {
        self.keyframes.range(..=time_ms).next_back().map(|(_, v)| v.clone())
    }
}
