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

use tracing::warn;

use super::taffy_layout::{
    ChildSizeSpec, SizeConstraints, compute_taffy_grid_layout,
    compute_taffy_grid_layout_with_specs, compute_taffy_linear_layout,
    compute_taffy_linear_layout_with_specs,
};
use super::{
    ContainerLayoutChild, ContainerMetadata, Diagnostic, Easing, LayoutEngine, PlacementMode,
    Timeline, TrackAccessor,
};
use crate::diagnostics::{DiagnosticCode, DiagnosticPhase};
use crate::renderer::text::TextKind;

mod engine;
mod grid;
mod linear;
mod positions;

pub(crate) use engine::LayoutContainerCache;
pub use positions::LayoutPositions;
pub(crate) use positions::empty_layout_positions;

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
#[allow(dead_code)] // Used by unit tests in tests/layout.rs
pub(crate) fn compute_stack_layout(children: &[ChildExtent], align: &str) -> Vec<[f32; 2]> {
    compute_stack_layout_results(children, align)
        .into_iter()
        .map(|r| r.position)
        .collect()
}

pub(crate) fn compute_stack_layout_results(
    children: &[ChildExtent],
    align: &str,
) -> Vec<super::taffy_layout::TaffyLayoutResult> {
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
            let position = match align {
                "start" => [-child.half_size[0], -child.half_size[1]],
                "end" => [child.half_size[0], child.half_size[1]],
                _ => [0.0, 0.0], // center
            };
            super::taffy_layout::TaffyLayoutResult {
                position,
                size: [child.half_size[0] * 2.0, child.half_size[1] * 2.0],
            }
        })
        .collect()
}
#[allow(dead_code)] // Used by unit tests in taffy_layout.rs
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
    /// Syntax-highlighting language for `Code` children (empty = plain).
    pub language: String,
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

        let track = self.world.tracks.get(child_label)?;

        // Check if this is a text-type actor (Text, Typst, Code)
        let text_kind = match self.world.caps(track.id).text {
            Some(crate::timeline::TextKind::Text) => TextKind::Text,
            Some(crate::timeline::TextKind::Typst) => TextKind::Typst,
            Some(crate::timeline::TextKind::Code) => TextKind::Code,
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
            language: track.text.language.get(time_ms, String::new()),
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

        // An explicitly set max_width is authoritative: never override it with
        // the container's propagated width. The previous guard only honored an
        // explicit value when it was tighter than (or equal to) available_width,
        // so a value looser than a collapsed container width (the CJK "wraps
        // after 2-3 chars" case) was overwritten. Honoring any explicit value
        // matches the roadmap's intent ("width propagation should not override
        // an explicitly set text_max_width"); overflow is the author's choice.
        if props.existing_max_width > 0.0 {
            tracing::debug!(
                "Width propagation: child '{}' has explicit max_width={}, not overriding with available_width={}",
                child_label,
                props.existing_max_width,
                available_width
            );
            return;
        }

        // No explicit value: propagate the container's available width.
        let effective_max_width = available_width;

        tracing::debug!(
            "Width propagation: recompiling text '{}' with max_width={} (container width={}, existing={})",
            child_label,
            effective_max_width,
            available_width,
            props.existing_max_width
        );

        if matches!(props.text_kind, TextKind::Math) {
            // Math shouldn't reach here at build time, but handle gracefully
            return;
        }

        // Re-compile through the process-wide memoized path. Width propagation
        // re-runs on every rebuild with the same container geometry, so
        // unchanged text children are served from the cache. The fast path
        // stays disabled to match the previous engine routing exactly.
        let compiled = match crate::renderer::text::compile_text_cached(
            props.text_kind,
            &props.content,
            &props.language,
            // Match the render path's palette so width-recompiles share its
            // cache entry (and highlight identically).
            self.text_compiler.borrow().highlight_palette.as_ref(),
            &props.font_family,
            props.font_size,
            props.font_weight,
            &props.font_style,
            props.line_height,
            props.letter_spacing,
            props.word_spacing,
            props.color,
            effective_max_width,
            None,
            &props.text_align,
            &props.overflow,
            false,
            self.font_context.as_ref(),
        ) {
            Ok(cached) => cached,
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
        let new_half_size = compiled.half_size;

        tracing::debug!(
            "Width propagation: text '{}' remeasured to half_size={:?}",
            child_label,
            new_half_size
        );

        let Some(track) = self.world.tracks.get_mut(child_label) else {
            return;
        };

        // Update max_width track
        track.text.text_max_width.ensure(0.0).add_keyframe(
            time_ms,
            effective_max_width,
            Easing::Linear,
        );

        let text_anchor = track.text.text_anchor.get(time_ms, "center".to_string());
        let mut paths_vec = compiled.paths.to_vec();
        let anchor_offset_x = match text_anchor.as_str() {
            "left" => new_half_size[0],
            "right" => -new_half_size[0],
            _ => 0.0,
        };
        if anchor_offset_x != 0.0 {
            let affine = kurbo::Affine::translate((anchor_offset_x as f64, 0.0));
            for p in &mut paths_vec {
                p.path.apply_affine(affine);
                p.glyph_center[0] += anchor_offset_x;
            }
        }

        // Update text_paths, size, layout_size, and metrics tracks
        track
            .text
            .text_paths
            .ensure(Vec::new())
            .add_keyframe(time_ms, paths_vec, Easing::Linear);
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
        let Some(metadata) = self.container_layout(container_label).cloned() else {
            return Vec::new();
        };

        let Some(container_id) = self.world.get_id(container_label) else {
            return Vec::new();
        };

        let mut jobs = Vec::new();

        for (child_index, child_label) in self.world.child_labels(container_id).enumerate() {
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
                label: child_label.to_string(),
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
            let Some(track) = self.world.tracks.get(child_label) else {
                warn!("layout: child '{}' not found in tracks, skipping", child_label);
                continue;
            };

            let placement_mode = track.geometry.placement_mode.last(PlacementMode::LayoutManaged);

            // Only children with seeded layout_size are admitted into the layout set.
            // Manual children may still exist in the scene graph via the hierarchy,
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
    pub(crate) fn collect_child_layout_specs(
        &self,
        container_label: &str,
        children: &[ContainerLayoutChild],
        time_ms: u64,
    ) -> (Vec<Option<ChildSizeSpec>>, Vec<SizeConstraints>, [f32; 2]) {
        let mut size_specs: Vec<Option<ChildSizeSpec>> = Vec::with_capacity(children.len());
        let mut constraints: Vec<SizeConstraints> = Vec::with_capacity(children.len());

        let parent_content_size = self
            .world
            .tracks
            .get(container_label)
            .and_then(|t| {
                t.layout_size_get(time_ms)
                    .or_else(|| t.layout_size_last())
                    .or_else(|| t.geometry.size.get_or_default(time_ms))
                    .or_else(|| {
                        t.geometry.size.last_time().map(|_| t.geometry.size.last([0.0, 0.0]))
                    })
            })
            .map(|s| [s[0] * 2.0, s[1] * 2.0])
            .unwrap_or([0.0, 0.0]);

        for cl in children {
            if let Some(track) = self.world.tracks.get(&cl.label) {
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
        let Some(metadata) = self.container_layout(container_label).cloned() else {
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
                let track = self.world.tracks.get(&cl.label)?;
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
            .map(|cl| self.world.tracks.get(&cl.label).map(|t| t.baseline_get(t_ms)).unwrap_or(0.0))
            .collect();

        // Collect size specs and constraints for phase 7
        let (size_specs, constraints, parent_content_size) =
            self.collect_child_layout_specs(container_label, &children, t_ms);

        let layout_results = LayoutEngine::compute_layout_with_specs(
            &metadata,
            &child_extents,
            &child_baselines,
            &size_specs,
            &constraints,
            parent_content_size,
        );

        // Write positions and resolved sizes to tracks, only for LayoutManaged children
        for (i, child) in children.iter().enumerate() {
            if child.placement_mode == PlacementMode::LayoutManaged {
                if let Some(track) = self.world.tracks.get_mut(&child.label) {
                    track.geometry.position.ensure([0.0, 0.0]).add_keyframe(
                        t_ms,
                        layout_results[i].position,
                        Easing::Linear,
                    );

                    let resolved_half_size = [
                        layout_results[i].size[0] / 2.0,
                        layout_results[i].size[1] / 2.0,
                    ];
                    let has_dynamic_sizing =
                        size_specs.get(i).copied().flatten().is_some_and(|spec| !spec.is_fixed());
                    if has_dynamic_sizing
                        && resolved_half_size[0] > 0.0
                        && resolved_half_size[1] > 0.0
                    {
                        track.ensure_layout_size(resolved_half_size).add_keyframe(
                            t_ms,
                            resolved_half_size,
                            Easing::Linear,
                        );
                        track.geometry.size.ensure(resolved_half_size).add_keyframe(
                            t_ms,
                            resolved_half_size,
                            Easing::Linear,
                        );
                    }
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
