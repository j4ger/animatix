//!
//! # Layout Design Contract
//!
//! Layout is a **declaration-time measure/place contract**, not a per-frame reflow.
//! It is evaluated once when a layout container (Row, Col, Grid, Stack) is applied,
//! and does not re-sample when animated tracks change later.
//!
//! 1. **Declaration-time measure**: Layout-managed children publish their local half-extents into a
//!    dedicated layout-size track. Authored shapes seed this from declared geometry; text, math,
//!    code, image, and SVG seed it from measured or intrinsic bounds.
//!
//! 2. **Container placement**: Containers (Row, Col, Grid, Stack) consume child half-extents and
//!    place children deterministically based on declared order, gap, padding, and alignment.
//!    Placement is frozen at declaration time.
//!
//! 3. **Authored `at` opts out**: A child with an authored `at` value is skipped by container
//!    placement; the author takes full responsibility for its position.
//!
//! 4. **Root default**: Root layout containers that are layout-managed with no `at` default to
//!    scene center (0, 0).
//!
//! 5. **Visual transforms independent**: Visual transforms (scale, rotation) do not affect layout
//!    size under the current contract; they are purely presentational.
//!
//! 6. **No sampled relayout**: When layout-size or `position` tracks animate later, layout does not
//!    re-evaluate. This is a deliberate trade-off for predictability.

use std::collections::BTreeMap;

use tracing::warn;

use super::taffy_layout::{
    ChildSizeSpec, SizeConstraints, compute_taffy_grid_layout,
    compute_taffy_grid_layout_with_specs, compute_taffy_linear_layout,
    compute_taffy_linear_layout_with_baselines, compute_taffy_linear_layout_with_specs,
};
use super::{
    AnimationTrack, ContainerLayoutChild, Diagnostic, Easing, PlacementMode, Timeline,
    TrackAccessor,
};
use crate::diagnostics::{DiagnosticCode, DiagnosticPhase};
use crate::renderer::text::TextKind;

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
    children
        .iter()
        .map(|child| {
            // Use the child's own half-size to position relative to origin
            // "start" aligns to top-left, "center" at origin, "end" aligns to bottom-right
            match align {
                "start" => [-child.half_size[0], -child.half_size[1]],
                "end" => [child.half_size[0], child.half_size[1]],
                _ => [0.0, 0.0], // center
            }
        })
        .collect()
}

/// Pure layout computation for Row/Col containers using Taffy.
/// Returns the positions of each child relative to the container center.
fn compute_linear_layout(
    children: &[ChildExtent],
    is_row: bool,
    gap: [f32; 2],
    padding: [f32; 4],
    align: &str,
    metadata: &ContainerMetadata,
    child_baselines: &[f32],
) -> Vec<[f32; 2]> {
    compute_linear_layout_with_specs_inner(
        children,
        is_row,
        gap,
        padding,
        align,
        metadata,
        child_baselines,
        &[],
        &[],
        [0.0, 0.0],
    )
}

/// Like `compute_linear_layout` but supports size specs and constraints.
fn compute_linear_layout_with_specs_inner(
    children: &[ChildExtent],
    is_row: bool,
    gap: [f32; 2],
    padding: [f32; 4],
    align: &str,
    metadata: &ContainerMetadata,
    child_baselines: &[f32],
    size_specs: &[Option<super::taffy_layout::ChildSizeSpec>],
    constraints: &[super::taffy_layout::SizeConstraints],
    parent_content_size: [f32; 2],
) -> Vec<[f32; 2]> {
    let layout_type = if is_row {
        super::LayoutType::Row
    } else {
        super::LayoutType::Col
    };

    let output = if size_specs.is_empty() && constraints.is_empty() {
        // Legacy path: no specs
        compute_taffy_linear_layout_with_baselines(
            children,
            layout_type,
            gap,
            padding,
            align,
            child_baselines,
            &metadata.vertical_align,
        )
    } else {
        compute_taffy_linear_layout_with_specs(
            children,
            layout_type,
            gap,
            padding,
            align,
            child_baselines,
            &metadata.vertical_align,
            size_specs,
            constraints,
            parent_content_size,
        )
    };
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
    compute_grid_layout_with_specs_inner(children, gap, padding, cols, &[], &[], [0.0, 0.0])
}

/// Like `compute_grid_layout` but supports size specs and constraints.
fn compute_grid_layout_with_specs_inner(
    children: &[ChildExtent],
    gap: [f32; 2],
    padding: [f32; 4],
    cols: usize,
    size_specs: &[Option<super::taffy_layout::ChildSizeSpec>],
    constraints: &[super::taffy_layout::SizeConstraints],
    parent_content_size: [f32; 2],
) -> Vec<[f32; 2]> {
    let output = if size_specs.is_empty() && constraints.is_empty() {
        compute_taffy_grid_layout(children, gap, padding, cols)
    } else {
        compute_taffy_grid_layout_with_specs(
            children,
            gap,
            padding,
            cols,
            size_specs,
            constraints,
            parent_content_size,
        )
    };
    output.positions.into_iter().map(|r| r.position).collect()
}

/// Compute the container's total size [width, height] from Taffy layout.
pub(crate) fn compute_container_size(
    children: &[ChildExtent],
    metadata: &ContainerMetadata,
) -> [f32; 2] {
    compute_container_size_with_specs(children, metadata, &[], &[], [0.0, 0.0])
}

/// Like `compute_container_size` but supports size specs and constraints.
pub(crate) fn compute_container_size_with_specs(
    children: &[ChildExtent],
    metadata: &ContainerMetadata,
    size_specs: &[Option<super::taffy_layout::ChildSizeSpec>],
    constraints: &[super::taffy_layout::SizeConstraints],
    parent_content_size: [f32; 2],
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
        let output = if size_specs.is_empty() && constraints.is_empty() {
            compute_taffy_linear_layout(
                children,
                layout_type,
                metadata.gap,
                metadata.padding,
                &metadata.align,
            )
        } else {
            compute_taffy_linear_layout_with_specs(
                children,
                layout_type,
                metadata.gap,
                metadata.padding,
                &metadata.align,
                &[],
                "center",
                size_specs,
                constraints,
                parent_content_size,
            )
        };
        output.container_size
    } else if is_grid {
        let output = if size_specs.is_empty() && constraints.is_empty() {
            compute_taffy_grid_layout(
                children,
                metadata.gap,
                metadata.padding,
                metadata.cols.unwrap_or(1).max(1),
            )
        } else {
            compute_taffy_grid_layout_with_specs(
                children,
                metadata.gap,
                metadata.padding,
                metadata.cols.unwrap_or(1).max(1),
                size_specs,
                constraints,
                parent_content_size,
            )
        };
        output.container_size
    } else {
        // Stack or unknown: no meaningful container size
        [0.0, 0.0]
    }
}

use super::{ContainerMetadata, LayoutEngine};

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
        Self::compute_positions_with_baselines(metadata, children, &[])
    }

    /// Like `compute_positions` but supports baseline alignment.
    /// `child_baselines` are per-child baseline offsets from text center (f32).
    /// Empty slice means no baseline info available.
    pub(crate) fn compute_positions_with_baselines(
        metadata: &ContainerMetadata,
        children: &[ChildExtent],
        child_baselines: &[f32],
    ) -> Vec<[f32; 2]> {
        Self::compute_positions_with_specs(
            metadata,
            children,
            child_baselines,
            &[],
            &[],
            [0.0, 0.0],
        )
    }

    /// Like `compute_positions_with_baselines` but supports size specs and constraints.
    pub(crate) fn compute_positions_with_specs(
        metadata: &ContainerMetadata,
        children: &[ChildExtent],
        child_baselines: &[f32],
        size_specs: &[Option<super::taffy_layout::ChildSizeSpec>],
        constraints: &[super::taffy_layout::SizeConstraints],
        parent_content_size: [f32; 2],
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
            if size_specs.is_empty() && constraints.is_empty() {
                compute_grid_layout(
                    children,
                    metadata.gap,
                    metadata.padding,
                    metadata.cols.unwrap_or(1).max(1),
                )
            } else {
                compute_grid_layout_with_specs_inner(
                    children,
                    metadata.gap,
                    metadata.padding,
                    metadata.cols.unwrap_or(1).max(1),
                    size_specs,
                    constraints,
                    parent_content_size,
                )
            }
        } else {
            if size_specs.is_empty() && constraints.is_empty() {
                compute_linear_layout(
                    children,
                    is_row,
                    metadata.gap,
                    metadata.padding,
                    &metadata.align,
                    metadata,
                    child_baselines,
                )
            } else {
                compute_linear_layout_with_specs_inner(
                    children,
                    is_row,
                    metadata.gap,
                    metadata.padding,
                    &metadata.align,
                    metadata,
                    child_baselines,
                    size_specs,
                    constraints,
                    parent_content_size,
                )
            }
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
        let fingerprints: Vec<([f32; 2], u8)> =
            child_extents.iter().map(|c| (c.half_size, c.placement_mode as u8)).collect();

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

        // Sample child baselines for baseline alignment
        let child_baselines: Vec<f32> = layout_children
            .iter()
            .map(|child| tracks.get(&child.label).map(|t| t.baseline_get(time_ms)).unwrap_or(0.0))
            .collect();

        let positions =
            Self::compute_positions_with_baselines(metadata, &child_extents, &child_baselines);

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

        let content = track.text.text_content.get(time_ms, String::new());
        if content.is_empty() {
            return None;
        }

        let font_family = track.text.font_family.get(time_ms, String::new());
        let font_family = if font_family.is_empty() {
            crate::renderer::text::DEFAULT_FONT_FAMILY.to_string()
        } else {
            font_family
        };

        Some(TextChildProps {
            text_kind,
            content,
            font_family,
            font_size: track.text.font_size.get(time_ms, 48.0),
            font_weight: track.text.font_weight.get(time_ms, 400.0),
            font_style: track.text.font_style.get(time_ms, "normal".to_string()),
            line_height: track.text.line_height.get(time_ms, 1.2),
            letter_spacing: track.text.letter_spacing.get(time_ms, 0.0),
            word_spacing: track.text.word_spacing.get(time_ms, 0.0),
            color: track.style.color.get(time_ms, [1.0, 1.0, 1.0, 1.0]),
            text_align: track.text.text_align.get(time_ms, "left".to_string()),
            overflow: track.text.overflow.get(time_ms, "visible".to_string()),
            existing_max_width: track.text.text_max_width.get(time_ms, 0.0),
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
                child_label,
                props.existing_max_width,
                available_width
            );
            return;
        }

        let effective_max_width =
            if props.existing_max_width > 0.0 && props.existing_max_width < available_width {
                props.existing_max_width
            } else {
                available_width
            };

        tracing::debug!(
            "Width propagation: recompiling text '{}' with max_width={} (container width={}, existing={})",
            child_label,
            effective_max_width,
            available_width,
            props.existing_max_width
        );

        // Re-compile text with wrapping
        let typst_color = typst::visualize::Color::from_u8(
            (props.color[0] * 255.0) as u8,
            (props.color[1] * 255.0) as u8,
            (props.color[2] * 255.0) as u8,
            (props.color[3] * 255.0) as u8,
        );

        let result = match props.text_kind {
            TextKind::Text => crate::renderer::text::compile_text(
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
            ),
            TextKind::Typst => crate::renderer::text::compile_typst(
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
            ),
            TextKind::Code => crate::renderer::text::compile_code(
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
            ),
            TextKind::Math => {
                // Math shouldn't reach here at build time, but handle gracefully
                return;
            },
        };

        let frame = match result {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!(
                    "Width propagation: failed to recompile text '{}' with max_width={}: {}",
                    child_label,
                    effective_max_width,
                    e
                );
                return;
            },
        };

        let compiled = crate::renderer::text::extract_glyphs_with_metrics(&frame);
        let new_half_size = crate::renderer::text::measure_text_paths(&compiled.glyphs);

        tracing::debug!(
            "Width propagation: text '{}' remeasured to half_size={:?}",
            child_label,
            new_half_size
        );

        let Some(track) = self.tracks.get_mut(child_label) else {
            return;
        };

        // Update max_width track
        track.text.text_max_width.ensure(0.0).add_keyframe(
            time_ms,
            effective_max_width,
            Easing::Linear,
        );

        // Update text_paths, size, layout_size, and metrics tracks
        track.text.text_paths.ensure(Vec::new()).add_keyframe(
            time_ms,
            compiled.glyphs,
            Easing::Linear,
        );
        track
            .geometry
            .size
            .ensure(crate::timeline::DEFAULT_LAYOUT_HALF_SIZE)
            .add_keyframe(time_ms, new_half_size, Easing::Linear);
        track
            .ensure_layout_size(crate::timeline::DEFAULT_LAYOUT_HALF_SIZE)
            .add_keyframe(time_ms, new_half_size, Easing::Linear);
        track.set_metrics(time_ms, compiled.ascent, compiled.descent, compiled.baseline_offset);
    }

    /// Compute available width for a text child based on container type.
    pub(crate) fn compute_available_width(
        &self,
        container_size: [f32; 2],
        metadata: &ContainerMetadata,
        _child_index: usize,
    ) -> f32 {
        let padding_h = metadata.padding[0] + metadata.padding[2]; // left + right

        match metadata.layout_type {
            super::LayoutType::Row => {
                // Row: unbounded width per child (unless child has explicit max_width)
                f32::MAX
            },
            super::LayoutType::Col => {
                // Col: container width minus horizontal padding
                let avail = container_size[0] - padding_h;
                avail.max(1.0) // ensure at least 1px to avoid degenerate layout
            },
            super::LayoutType::Grid => {
                // Grid: per-cell width = (container_width - padding - gaps) / cols
                let cols = metadata.cols.unwrap_or(1).max(1);
                let total_gaps = metadata.gap[0] * (cols - 1) as f32;
                let avail = (container_size[0] - padding_h - total_gaps) / cols as f32;
                avail.max(1.0)
            },
            super::LayoutType::Stack => {
                // Stack: no meaningful width constraint
                f32::MAX
            },
        }
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

            let available_width =
                self.compute_available_width(container_size, &metadata, child_index);

            if available_width >= f32::MAX - 1.0 {
                // Row or Stack: unbounded, no wrapping needed
                tracing::debug!(
                    "Width propagation: child '{}' in {} '{}' has unbounded width, skipping",
                    child_label,
                    metadata.layout_type.as_str(),
                    container_label
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

            let placement_mode = track.geometry.placement_mode.last(PlacementMode::LayoutManaged);

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

    /// Collect size specs and constraints for a container's children from tracks.
    fn collect_child_layout_specs(
        &self,
        children: &[ContainerLayoutChild],
        time_ms: u64,
    ) -> (Vec<Option<ChildSizeSpec>>, Vec<SizeConstraints>, [f32; 2]) {
        let mut size_specs: Vec<Option<ChildSizeSpec>> = Vec::with_capacity(children.len());
        let mut constraints: Vec<SizeConstraints> = Vec::with_capacity(children.len());

        // Compute parent content size first (from container's own layout_size)
        // If the container has no layout_size, use a default
        let parent_content_size = [0.0f32, 0.0f32];

        for cl in children {
            if let Some(track) = self.tracks.get(&cl.label) {
                size_specs.push(track.geometry.size_spec);
                let min_w = track.geometry.min_width.get(time_ms, 0.0);
                let max_w = f32::INFINITY;
                let min_h = track.geometry.min_height.get(time_ms, 0.0);
                let max_h = track.geometry.max_height.get(time_ms, f32::INFINITY);
                constraints.push(SizeConstraints {
                    min_width: if min_w > 0.0 { Some(min_w) } else { None },
                    max_width: if !max_w.is_infinite() && !max_w.is_nan() {
                        Some(max_w)
                    } else {
                        None
                    },
                    min_height: if min_h > 0.0 { Some(min_h) } else { None },
                    max_height: if !max_h.is_infinite() && !max_h.is_nan() {
                        Some(max_h)
                    } else {
                        None
                    },
                });
            } else {
                size_specs.push(None);
                constraints.push(SizeConstraints::default());
            }
        }
        (size_specs, constraints, parent_content_size)
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

        // Sample child baselines for baseline alignment
        let child_baselines: Vec<f32> = children
            .iter()
            .map(|cl| self.tracks.get(&cl.label).map(|t| t.baseline_get(t_ms)).unwrap_or(0.0))
            .collect();

        // Collect size specs and constraints for phase 7
        let (size_specs, constraints, _parent_content_size) =
            self.collect_child_layout_specs(&children, t_ms);

        // Use the parent container's own layout_size as the content box for percentage resolution.
        // For the container itself, we may need to do a two-pass: first compute the container size
        // without percentage children, then resolve percentages against it.
        let parent_content_size = self
            .tracks
            .get(container_label)
            .and_then(|t| t.layout_size_last())
            .map(|s| [s[0] * 2.0, s[1] * 2.0])
            .unwrap_or([0.0, 0.0]);

        let positions = LayoutEngine::compute_positions_with_specs(
            &metadata,
            &child_extents,
            &child_baselines,
            &size_specs,
            &constraints,
            parent_content_size,
        );

        // Write positions to tracks, only for LayoutManaged children
        for (i, child) in children.iter().enumerate() {
            if child.placement_mode == PlacementMode::LayoutManaged {
                if let Some(track) = self.tracks.get_mut(&child.label) {
                    track.geometry.position.ensure([0.0, 0.0]).add_keyframe(
                        t_ms,
                        positions[i],
                        Easing::Linear,
                    );
                }
            }
        }
    }
}

/// Collected text child info for width propagation.
/// Holds all data needed to recompile a text child, gathered during the
/// immutable-read phase to avoid borrow conflicts.
pub(crate) struct TextChildRecompile {
    /// Child label.
    pub label: String,
    /// Text child properties.
    pub props: TextChildProps,
    /// Available width for text wrapping.
    pub available_width: f32,
}
