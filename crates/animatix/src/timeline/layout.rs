//!
//! # Layout Design Contract
//!
//! Layout is a **declaration-time measure/place contract**, not a per-frame reflow.
//! It is evaluated once when a layout container (Row, Col, Grid, Stack) is applied,
//! and does not re-sample when animated tracks change later.
//!
//! 1. **Declaration-time measure**: Layout-managed children publish their local
//!    half-extents into a dedicated layout-size track. Authored shapes seed this from
//!    declared geometry; text, math, code, image, and SVG seed it from measured
//!    or intrinsic bounds.
//!
//! 2. **Container placement**: Containers (Row, Col, Grid, Stack) consume child
//!    half-extents and place children deterministically based on declared order,
//!    gap, padding, and alignment. Placement is frozen at declaration time.
//!
//! 3. **Authored `at` opts out**: A child with an authored `at` value is skipped
//!    by container placement; the author takes full responsibility for its position.
//!
//! 4. **Root default**: Root layout containers that are layout-managed with no `at`
//!    default to scene center (0, 0).
//!
//! 5. **Visual transforms independent**: Visual transforms (scale, rotation) do
//!    not affect layout size under the current contract; they are purely presentational.
//!
//! 6. **No sampled relayout**: When layout-size or `position` tracks animate later, layout
//!    does not re-evaluate. This is a deliberate trade-off for predictability.

use std::collections::BTreeMap;

use super::{AnimationTrack, ContainerLayoutChild, Diagnostic, Easing, PlacementMode, Timeline, TrackAccessor};
use crate::renderer::text::TextKind;
use crate::diagnostics::{DiagnosticCode, DiagnosticPhase};
use tracing::warn;

use super::taffy_layout::{compute_taffy_linear_layout, compute_taffy_grid_layout};

/// Represents a child's layout-relevant size at a specific point in time.
#[derive(Clone, Debug)]
pub struct ChildExtent {
    pub label: String,
    pub half_size: [f32; 2],
    pub placement_mode: PlacementMode,
}

/// Pure layout computation for a Stack container.
/// Stack places all children at origin (0, 0) on the main axis,
/// but honors `align` for cross-axis positioning.
pub(crate) fn compute_stack_layout(children: &[ChildExtent], align: &str) -> Vec<[f32; 2]> {
    // Stack is an overlap container — all children share the same origin,
    // creating a layered visual effect. The `align` property controls
    // cross-axis positioning within the stack's conceptual extent.
    //
    // Since Stack has no intrinsic main/cross axis direction, we apply
    // alignment symmetrically: both X and Y axes respond to align.
    //
    // - "start":  children are shifted toward negative X/Y (top-left)
    // - "center": children stay at origin (default)
    // - "end":    children are shifted toward positive X/Y (bottom-right)
    children.iter().map(|child| {
        // Use the child's own half-size to position relative to origin
        // "start" aligns to top-left, "center" at origin, "end" aligns to bottom-right
        match align {
            "start" => [-child.half_size[0], -child.half_size[1]],
            "end" => [child.half_size[0], child.half_size[1]],
            _ => [0.0, 0.0], // center
        }
    }).collect()
}

/// Pure layout computation for Row/Col containers using Taffy.
/// Returns the positions of each child relative to the container center.
fn compute_linear_layout(
    children: &[ChildExtent],
    is_row: bool,
    gap: [f32; 2],
    padding: [f32; 4],
    align: &str,
) -> Vec<[f32; 2]> {
    let layout_type = if is_row {
        super::LayoutType::Row
    } else {
        super::LayoutType::Col
    };

    let output = compute_taffy_linear_layout(children, layout_type, gap, padding, align);
    output.positions.into_iter().map(|r| r.position).collect()
}

/// Pure layout computation for Grid containers using Taffy.
/// Returns the positions of each child relative to the container center.
fn compute_grid_layout(
    children: &[ChildExtent],
    gap: [f32; 2],
    padding: [f32; 4],
    cols: usize,
) -> Vec<[f32; 2]> {
    let output = compute_taffy_grid_layout(children, gap, padding, cols);
    output.positions.into_iter().map(|r| r.position).collect()
}

/// Compute the container's total size [width, height] from Taffy layout.
pub(crate) fn compute_container_size(
    children: &[ChildExtent],
    metadata: &ContainerMetadata,
) -> [f32; 2] {
    let is_row = metadata.layout_type == super::LayoutType::Row;
    let is_col = metadata.layout_type == super::LayoutType::Col;
    let is_grid = metadata.layout_type == super::LayoutType::Grid;

    if is_row || is_col {
        let layout_type = if is_row {
            super::LayoutType::Row
        } else {
            super::LayoutType::Col
        };
        let output = compute_taffy_linear_layout(
            children,
            layout_type,
            metadata.gap,
            metadata.padding,
            &metadata.align,
        );
        output.container_size
    } else if is_grid {
        let output = compute_taffy_grid_layout(
            children,
            metadata.gap,
            metadata.padding,
            metadata.cols.unwrap_or(1).max(1),
        );
        output.container_size
    } else {
        // Stack or unknown: no meaningful container size
        [0.0, 0.0]
    }
}

use super::LayoutEngine;

use super::ContainerMetadata;

/// Cached layout computation result for a single container.
#[derive(Clone, Debug)]
pub(crate) struct LayoutCacheEntry {
    /// Input fingerprint: (half_size_x, half_size_y, placement_mode_idx) per child.
    child_fingerprints: Vec<([f32; 2], u8)>,
    /// Cached output positions.
    positions: BTreeMap<String, [f32; 2]>,
}

impl LayoutEngine {
    /// Create a new layout engine with an empty cache.
    pub fn new() -> Self {
        Self {
            cache: std::cell::RefCell::new(std::collections::HashMap::new()),
        }
    }

    /// Invalidate the layout cache (e.g. after a timeline rebuild).
    pub fn invalidate_cache(&self) {
        self.cache.borrow_mut().clear();
    }
    pub(crate) fn compute_positions(
        metadata: &ContainerMetadata,
        children: &[ChildExtent],
    ) -> Vec<[f32; 2]> {
        let is_row = metadata.layout_type == super::LayoutType::Row;
        let is_col = metadata.layout_type == super::LayoutType::Col;
        let is_stack = metadata.layout_type == super::LayoutType::Stack;
        let is_grid = metadata.layout_type == super::LayoutType::Grid;

        if !is_row && !is_col && !is_stack && !is_grid {
            return Vec::new();
        }

        if is_stack {
            compute_stack_layout(children, &metadata.align)
        } else if is_grid {
            compute_grid_layout(
                children,
                metadata.gap,
                metadata.padding,
                metadata.cols.unwrap_or(1).max(1),
            )
        } else {
            compute_linear_layout(children, is_row, metadata.gap, metadata.padding, &metadata.align)
        }
    }

    /// Computes layout positions for all children of a container at a specific time.
    /// Returns a BTreeMap mapping child labels to their computed positions.
    ///
    /// Uses a per-container cache keyed on children's layout sizes. If the
    /// children's half-sizes and placement modes haven't changed since the last
    /// call, the cached positions are returned without recomputing via Taffy.
    pub fn compute_layout_for_time(
        &self,
        metadata: &ContainerMetadata,
        layout_children: &[ContainerLayoutChild],
        time_ms: u64,
        tracks: &BTreeMap<String, AnimationTrack>,
    ) -> BTreeMap<String, [f32; 2]> {
        // Sample child extents at current time
        let child_extents: Vec<ChildExtent> = layout_children
            .iter()
            .filter_map(|child| {
                let track = tracks.get(&child.label)?;
                let half_size = track.layout_size_get(time_ms)?;
                Some(ChildExtent {
                    label: child.label.clone(),
                    half_size,
                    placement_mode: child.placement_mode,
                })
            })
            .collect();

        // Build fingerprint for cache lookup
        let fingerprints: Vec<([f32; 2], u8)> = child_extents
            .iter()
            .map(|c| (c.half_size, c.placement_mode as u8))
            .collect();

        // Check cache — use container label from first child's parent context.
        // We key on the child labels themselves to detect structural changes.
        let cache_key = metadata.child_order.join("|");
        {
            let cache = self.cache.borrow();
            if let Some(entry) = cache.get(&cache_key) {
                if entry.child_fingerprints == fingerprints {
                    return entry.positions.clone();
                }
            }
        }

        let positions = Self::compute_positions(metadata, &child_extents);

        // Build result BTreeMap, only including LayoutManaged children
        let mut result = BTreeMap::new();
        for (i, child) in child_extents.iter().enumerate() {
            if child.placement_mode == PlacementMode::LayoutManaged {
                result.insert(child.label.clone(), positions[i]);
            }
        }

        // Store in cache
        self.cache.borrow_mut().insert(
            cache_key,
            LayoutCacheEntry {
                child_fingerprints: fingerprints,
                positions: result.clone(),
            },
        );

        result
    }
}

// ─────────────────────────────────────────────────────────────
// Width propagation: container → text children
// ─────────────────────────────────────────────────────────────

/// Collected text child properties for width propagation (immutable read phase).
#[derive(Clone, Debug)]
pub(crate) struct TextChildProps {
    /// Text kind (Text, Typst, Code).
    pub text_kind: TextKind,
    /// Raw text content.
    pub content: String,
    /// Font family name.
    pub font_family: String,
    /// Font size in points.
    pub font_size: f32,
    /// Font weight (100–900).
    pub font_weight: f32,
    /// Font style ("normal" | "italic").
    pub font_style: String,
    /// Line height multiplier.
    pub line_height: f32,
    /// Letter spacing in points.
    pub letter_spacing: f32,
    /// Word spacing in points.
    pub word_spacing: f32,
    /// Color in RGBA.
    pub color: [f32; 4],
    /// Text alignment ("left", "center", "right", "justify").
    pub text_align: String,
    /// Overflow behavior ("visible", "clip", "ellipsis").
    pub overflow: String,
    /// Existing max_width value (0 = no explicit width).
    pub existing_max_width: f32,
}

impl Timeline {
    /// Read text child properties from the track (immutable borrow).
    /// Returns None if the child is not a text type or has no content.
    pub(crate) fn read_text_child_props(
        &self,
        child_label: &str,
        time_ms: u64,
    ) -> Option<TextChildProps> {
        use crate::timeline::TrackAccessor;

        let track = self.tracks.get(child_label)?;

        // Check if this is a text-type actor (Text, Typst, Code)
        let text_kind = match track.kind {
            super::ActorKindId::Text => TextKind::Text,
            super::ActorKindId::Typst => TextKind::Typst,
            super::ActorKindId::Code => TextKind::Code,
            _ => return None,
        };

        let content = track.text_content.get(time_ms, String::new());
        if content.is_empty() {
            return None;
        }

        let font_family = track.font_family.get(time_ms, String::new());
        let font_family = if font_family.is_empty() {
            crate::renderer::text::DEFAULT_FONT_FAMILY.to_string()
        } else {
            font_family
        };

        Some(TextChildProps {
            text_kind,
            content,
            font_family,
            font_size: track.font_size.get(time_ms, 48.0),
            font_weight: track.font_weight.get(time_ms, 400.0),
            font_style: track.font_style.get(time_ms, "normal".to_string()),
            line_height: track.line_height.get(time_ms, 1.2),
            letter_spacing: track.letter_spacing.get(time_ms, 0.0),
            word_spacing: track.word_spacing.get(time_ms, 0.0),
            color: track.color.get(time_ms, [1.0, 1.0, 1.0, 1.0]),
            text_align: track.text_align.get(time_ms, "left".to_string()),
            overflow: track.overflow.get(time_ms, "visible".to_string()),
            existing_max_width: track.max_width.get(time_ms, 0.0),
        })
    }

    /// Re-compile a text child with the given max_width and update its paths/size/layout_size.
    /// Takes ownership of `props` to avoid borrow conflicts.
    fn recompile_text_with_width(
        &mut self,
        child_label: &str,
        props: TextChildProps,
        available_width: f32,
        time_ms: u64,
    ) {
        use crate::timeline::TrackAccessor;

        // Check if max_width is already explicitly set and tighter than available width
        if props.existing_max_width > 0.0 && props.existing_max_width <= available_width {
            tracing::debug!(
                "Width propagation: child '{}' has explicit max_width={}, not overriding with available_width={}",
                child_label, props.existing_max_width, available_width
            );
            return;
        }

        let effective_max_width = if props.existing_max_width > 0.0 && props.existing_max_width < available_width {
            props.existing_max_width
        } else {
            available_width
        };

        tracing::debug!(
            "Width propagation: recompiling text '{}' with max_width={} (container width={}, existing={})",
            child_label, effective_max_width, available_width, props.existing_max_width
        );

        // Re-compile text with wrapping
        let typst_color = typst::visualize::Color::from_u8(
            (props.color[0] * 255.0) as u8,
            (props.color[1] * 255.0) as u8,
            (props.color[2] * 255.0) as u8,
            (props.color[3] * 255.0) as u8,
        );

        let result = match props.text_kind {
            TextKind::Text => {
                crate::renderer::text::compile_text(
                    &props.content,
                    props.font_size,
                    typst_color,
                    &props.font_family,
                    self.font_context.as_ref(),
                    props.font_weight,
                    &props.font_style,
                    props.line_height,
                    props.letter_spacing,
                    props.word_spacing,
                    effective_max_width,
                    &props.text_align,
                    &props.overflow,
                )
            }
            TextKind::Typst => {
                crate::renderer::text::compile_typst(
                    &props.content,
                    props.font_size,
                    typst_color,
                    &props.font_family,
                    self.font_context.as_ref(),
                    props.font_weight,
                    &props.font_style,
                    props.line_height,
                    props.letter_spacing,
                    props.word_spacing,
                    effective_max_width,
                    &props.text_align,
                    &props.overflow,
                )
            }
            TextKind::Code => {
                crate::renderer::text::compile_code(
                    &props.content,
                    props.font_size,
                    typst_color,
                    &props.font_family,
                    self.font_context.as_ref(),
                    props.font_weight,
                    &props.font_style,
                    props.line_height,
                    props.letter_spacing,
                    props.word_spacing,
                    effective_max_width,
                    &props.text_align,
                    &props.overflow,
                )
            }
            TextKind::Math => {
                // Math shouldn't reach here at build time, but handle gracefully
                return;
            }
        };

        let frame = match result {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!(
                    "Width propagation: failed to recompile text '{}' with max_width={}: {}",
                    child_label, effective_max_width, e
                );
                return;
            }
        };

        let new_paths = crate::renderer::text::extract_glyphs(&frame);
        let new_half_size = crate::renderer::text::measure_text_paths(&new_paths);

        tracing::debug!(
            "Width propagation: text '{}' remeasured to half_size={:?}",
            child_label, new_half_size
        );

        let Some(track) = self.tracks.get_mut(child_label) else {
            return;
        };

        // Update max_width track
        track.max_width.ensure(0.0).add_keyframe(time_ms, effective_max_width, Easing::Linear);

        // Update text_paths, size, and layout_size tracks
        track.text_paths.ensure(Vec::new()).add_keyframe(time_ms, new_paths, Easing::Linear);
        track.size.ensure(crate::timeline::DEFAULT_LAYOUT_HALF_SIZE)
            .add_keyframe(time_ms, new_half_size, Easing::Linear);
        track.ensure_layout_size(crate::timeline::DEFAULT_LAYOUT_HALF_SIZE)
            .add_keyframe(time_ms, new_half_size, Easing::Linear);
    }

    /// Compute available width for a text child based on container type.
    pub(crate) fn compute_available_width(
        &self,
        container_size: [f32; 2],
        metadata: &ContainerMetadata,
        child_index: usize,
    ) -> f32 {
        let padding_h = metadata.padding[0] + metadata.padding[2]; // left + right

        match metadata.layout_type {
            super::LayoutType::Row => {
                // Row: unbounded width per child (unless child has explicit max_width)
                f32::MAX
            }
            super::LayoutType::Col => {
                // Col: container width minus horizontal padding
                let avail = container_size[0] - padding_h;
                avail.max(1.0) // ensure at least 1px to avoid degenerate layout
            }
            super::LayoutType::Grid => {
                // Grid: per-cell width = (container_width - padding - gaps) / cols
                let cols = metadata.cols.unwrap_or(1).max(1);
                let total_gaps = metadata.gap[0] * (cols - 1) as f32;
                let avail = (container_size[0] - padding_h - total_gaps) / cols as f32;
                avail.max(1.0)
            }
            super::LayoutType::Stack => {
                // Stack: no meaningful width constraint
                f32::MAX
            }
        }
    }

    /// Collected text child info for width propagation.
    /// Holds all data needed to recompile a text child, gathered during the
    /// immutable-read phase to avoid borrow conflicts.
    struct TextChildRecompile {
        label: String,
        props: TextChildProps,
        available_width: f32,
    }

    /// Collect text child properties (immutable read phase) and return a list of
    /// recompile jobs. Each job can be processed later with a mutable borrow.
    fn collect_text_child_recompile_jobs(
        &self,
        container_label: &str,
        container_size: [f32; 2],
        time_ms: u64,
    ) -> Vec<TextChildRecompile> {
        let Some(metadata) = self.container_metadata.get(container_label).cloned() else {
            return Vec::new();
        };

        let Some(parent_track) = self.tracks.get(container_label) else {
            return Vec::new();
        };

        let mut jobs = Vec::new();

        for (child_index, child_label) in parent_track.children.iter().enumerate() {
            let Some(props) = self.read_text_child_props(child_label, time_ms) else {
                continue;
            };

            let available_width = self.compute_available_width(container_size, &metadata, child_index);

            if available_width >= f32::MAX - 1.0 {
                // Row or Stack: unbounded, no wrapping needed
                tracing::debug!(
                    "Width propagation: child '{}' in {} '{}' has unbounded width, skipping",
                    child_label, metadata.layout_type.as_str(), container_label
                );
                continue;
            }

            jobs.push(TextChildRecompile {
                label: child_label.clone(),
                props,
                available_width,
            });
        }

        jobs
    }

    /// Propagate container width to text children so they wrap automatically.
    /// Called after container layout is computed but before final position application.
    ///
    /// Uses a two-phase approach:
    /// 1. Immutable read phase: collect text child properties
    /// 2. Mutable write phase: recompile text and update tracks
    pub(crate) fn propagate_text_child_widths(
        &mut self,
        container_label: &str,
        container_size: [f32; 2],
        time_ms: u64,
        _diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Phase 1: Collect recompile jobs (immutable borrow)
        let jobs = self.collect_text_child_recompile_jobs(container_label, container_size, time_ms);

        // Phase 2: Execute recompile jobs (mutable borrow)
        for job in jobs {
            self.recompile_text_with_width(&job.label, job.props, job.available_width, time_ms);
        }
    }

    fn push_layout_size_exclusion_diagnostic(
        diagnostics: &mut Vec<Diagnostic>,
        container_label: &str,
        container_ty: &str,
        child_label: &str,
    ) {
        diagnostics.push(
            Diagnostic::warning(
                DiagnosticCode::LayoutSizeFallback,
                DiagnosticPhase::Build,
                format!(
                    "Layout-managed child '{child_label}' in {container_ty} container '{container_label}' had no seeded layout_size and was excluded from layout admission."
                ),
            )
            .with_subject(child_label),
        );
    }

    pub(super) fn build_layout_children(
        &self,
        container_label: &str,
        container_ty: &str,
        child_order: &[String],
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Vec<ContainerLayoutChild> {
        let mut layout_children = Vec::with_capacity(child_order.len());

        for child_label in child_order {
            let Some(track) = self.tracks.get(child_label) else {
                warn!("layout: child '{}' not found in tracks, skipping", child_label);
                continue;
            };

            let placement_mode = track.placement_mode.last(PlacementMode::LayoutManaged);

            // Only children with seeded layout_size are admitted into the layout set.
            // Manual children may still exist in the scene graph via track.children,
            // but they are excluded from layout spacing/placement when unmeasured.
            let is_admitted = if !track.has_layout_size() {
                if placement_mode == PlacementMode::LayoutManaged {
                    Self::push_layout_size_exclusion_diagnostic(
                        diagnostics,
                        container_label,
                        container_ty,
                        child_label,
                    );
                }
                false
            } else {
                true
            };

            if is_admitted {
                layout_children.push(ContainerLayoutChild {
                    label: child_label.clone(),
                    placement_mode,
                });
            }
        }

        layout_children
    }

    pub(super) fn apply_container_layout(
        &mut self,
        container_label: &str,
        time_ms: f64,
        _diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(metadata) = self.container_metadata.get(container_label).cloned() else {
            return;
        };

        let children = self.layout_children_for(container_label);

        // Layout containers consume child-local half-extents from the dedicated
        // layout-size track. Authored shapes seed this track from declared geometry;
        // text, math, code, image, and SVG paths seed it from measured or
        // intrinsic bounds. Placement is declaration-time and does not promise
        // sampled per-frame relayout when those tracks animate later.
        let child_extents: Vec<ChildExtent> = children
            .iter()
            .filter_map(|cl| {
                let track = self.tracks.get(&cl.label)?;
                let half_size = track.layout_size_last()?;
                Some(ChildExtent {
                    label: cl.label.clone(),
                    half_size,
                    placement_mode: cl.placement_mode,
                })
            })
            .collect();

        let t_ms = time_ms as u64;

        let positions = LayoutEngine::compute_positions(&metadata, &child_extents);

        // Write positions to tracks, only for LayoutManaged children
        for (i, child) in children.iter().enumerate() {
            if child.placement_mode == PlacementMode::LayoutManaged {
                if let Some(track) = self.tracks.get_mut(&child.label) {
                    track.position.ensure([0.0, 0.0]).add_keyframe(t_ms, positions[i], Easing::Linear);
                }
            }
        }
    }
}
