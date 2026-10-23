//! Context types passed to [`Primitive`](super::Primitive) methods and the
//! render recursion.

use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::timeline::callout_geometry::TargetResolver;
use crate::timeline::{
    AnimationTrack, Environment, SceneDimensions, Timeline, Value, VectorShapeState,
    VectorShapeStyle, VelloPath,
};

/// Context passed to `Primitive::build()`.
pub struct BuildCtx<'a> {
    /// The timeline being built.
    pub timeline: &'a mut Timeline,
    /// Current time in milliseconds.
    pub time_ms: f64,
    /// Optional parent actor label.
    pub parent_label: Option<&'a str>,
    /// Build diagnostics collector.
    pub diagnostics: &'a mut Vec<Diagnostic>,
    /// Optional active coordinate context from parent host (e.g. `Graph`).
    pub coordinate_context: Option<crate::timeline::CoordinateContext>,
}

/// Timing and resource context for `Primitive::handle_assignment()`.
pub struct AssignmentCtx<'a> {
    /// Animation start time in milliseconds.
    pub t_start_ms: u64,
    /// Animation end time in milliseconds.
    pub t_end_ms: u64,
    /// Easing function for the animation.
    pub easing: Easing,
    /// Whether the animation is instant but delayed.
    pub instant_delayed: bool,
    /// Animation duration in milliseconds.
    pub duration_ms: f64,
    /// Font rendering context.
    pub font_context: &'a crate::renderer::text::FontContext,
    /// Text compiler for recompilation.
    pub text_compiler: &'a mut crate::renderer::text::TextCompiler,
    /// Shared asset cache for loading media referenced by assignments.
    pub asset_cache: &'a mut crate::timeline::assets::AssetCache,
}

/// Context passed to `Primitive::render()`.
pub struct RenderCtx<'a> {
    /// Current vector shape state.
    pub state: &'a VectorShapeState,
    /// Shape style (color, stroke, fill).
    pub style: VectorShapeStyle,
    /// Current time in milliseconds.
    pub time_ms: u64,
    /// The actor's track — memoization home for the shape→path conversion
    /// (PF-6); see `AnimationTrack::shape_path_memoized`.
    pub track: &'a AnimationTrack,
}

/// Context passed to `Primitive::evaluate()`.
///
/// All fields are immutable. Text-specific mutable state is in [`TextCompileCtx`].
pub struct EvaluateCtx<'a> {
    /// The animation track for this actor.
    pub track: &'a AnimationTrack,
    /// Current time in milliseconds.
    pub time_ms: u64,
    /// Local transform (parent * position * rotation * scale).
    pub local_transform: kurbo::Affine,
    /// Inherited opacity multiplier.
    pub opacity: f32,
    /// Scene dimensions.
    pub scene_dimensions: SceneDimensions,
    /// Sampled scene background color at this frame.
    pub background_color: [f32; 4],
    /// Property overrides from modifiers.
    pub overrides: Option<&'a std::collections::HashMap<String, Value>>,
    /// Pre-sampled vector paths (includes procedural plot sampling).
    pub vector_paths: &'a [VelloPath],
    /// Shared image cache for native render commands that specify a URL.
    pub asset_cache: &'a crate::timeline::assets::AssetCache,
    /// Narrow resolver for target actor bounds (targeted callout mode).
    /// Replaces the previous broad `Option<&Timeline>` field.
    pub target_resolver: Option<&'a dyn TargetResolver>,
}

/// Frame-invariant, read-only state shared by every node rendered this frame.
///
/// Bundling these is what keeps the render recursion's argument list short:
/// `evaluate_node` and friends used to take fourteen positional arguments, and a
/// mis-ordered argument there is a silent semantic change rather than a compile
/// error. Per-container values (`layout_positions`) and per-call values
/// (`allow_pending_composites`) deliberately stay parameters — they are not
/// frame-invariant.
#[derive(Clone, Copy)]
pub(crate) struct RenderFrame<'a> {
    /// Current time in milliseconds.
    pub(crate) time_ms: u64,
    /// Scene dimensions for this frame.
    pub(crate) scene_dimensions: SceneDimensions,
    /// Debug overlay options for this frame.
    pub(crate) debug_options: crate::timeline::DebugRenderOptions,
    /// Property overrides produced by modifiers, keyed by actor label.
    pub(crate) overrides:
        &'a std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    /// The frame environment, when modifiers or procedural plots needed one.
    pub(crate) frame_env: Option<&'a Environment>,
}

/// Mutable outputs of one render call.
///
/// The three table outputs and the backend handle are reborrowed unchanged when
/// a container renders its children into a different scene; see
/// [`Self::with_scene`].
pub(crate) struct RenderOutputs<'s, 'f> {
    /// Scene receiving the draw commands.
    pub(crate) scene: &'s mut vello::Scene,
    /// Hit regions (world bounds) collected for this frame, keyed by entity id.
    pub(crate) hit_regions: &'s mut Vec<(crate::timeline::EntityId, kurbo::Rect)>,
    /// Observable scene items, when collection was requested.
    pub(crate) program_items: &'s mut Option<Vec<crate::timeline::scene_program::SceneItem>>,
    /// The active filter backend, when one is available.
    pub(crate) filter_backend: &'s mut Option<&'f mut dyn crate::timeline::effects::FilterBackend>,
}

impl RenderOutputs<'_, '_> {
    /// Run `f` with the draw target temporarily replaced by `scene`.
    ///
    /// The Filter strategy renders its children into an offscreen sub-scene —
    /// a stack local with a shorter lifetime than this context — so the swap
    /// happens through a fresh borrow rather than an assignment to
    /// [`Self::scene`].
    pub(crate) fn with_scene<R>(
        &mut self,
        scene: &mut vello::Scene,
        f: impl FnOnce(&mut RenderOutputs<'_, '_>) -> R,
    ) -> R {
        let mut swapped = RenderOutputs {
            scene,
            hit_regions: &mut *self.hit_regions,
            program_items: &mut *self.program_items,
            filter_backend: &mut *self.filter_backend,
        };
        f(&mut swapped)
    }

    /// [`Self::with_scene`] plus a replacement item sink.
    ///
    /// The static-subtree cache renders a subtree into a scratch scene *and*
    /// collects its items into a per-subtree slot, so both differ from the
    /// frame's; everything else is reborrowed unchanged.
    pub(crate) fn with_scene_and_items<R>(
        &mut self,
        scene: &mut vello::Scene,
        program_items: &mut Option<Vec<crate::timeline::scene_program::SceneItem>>,
        f: impl FnOnce(&mut RenderOutputs<'_, '_>) -> R,
    ) -> R {
        let mut swapped = RenderOutputs {
            scene,
            hit_regions: &mut *self.hit_regions,
            program_items,
            filter_backend: &mut *self.filter_backend,
        };
        f(&mut swapped)
    }
}

/// Mutable context passed to [`crate::primitives::Primitive::render_children`].
///
/// The scene-subtree renderer hands each container primitive this context so the
/// primitive drives its own recursion, instead of the pipeline branching on a
/// `ChildProcessing` value. `scene`, `hit_regions`, `program_items`, and
/// `filter_backend` are the caller-local outputs; every other field is
/// read-only frame state, handed to the recursion entry point
/// (`Timeline::evaluate_node`) through `Self::render_frame` and
/// `RenderOutputs`. The frame caches are reached through `Self::timeline`.
pub struct RenderChildrenCtx<'a, 'b, 'c> {
    /// The timeline being rendered.
    pub timeline: &'a Timeline,
    /// Entity ID of the container whose children are being rendered.
    pub host_id: crate::timeline::EntityId,
    /// Label of the container whose children are being rendered.
    pub node_label: &'a str,
    /// Current time in milliseconds.
    pub time_ms: u64,
    /// The container's world transform.
    pub global_transform: kurbo::Affine,
    /// The container's inherited opacity.
    pub global_opacity: f32,
    /// Scene dimensions.
    pub scene_dimensions: SceneDimensions,
    /// Debug overlay options for this frame.
    pub debug_options: crate::timeline::DebugRenderOptions,
    /// Property overrides from modifiers, keyed by actor label.
    pub overrides: &'a std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    /// Resolved child layout positions for this frame (shared by refcount, so
    /// storing it by value is a cheap clone).
    pub layout_positions: std::sync::Arc<crate::timeline::layout::LayoutPositions>,
    /// The frame environment, when one was built.
    pub frame_env: Option<&'a Environment>,
    /// Whether the caller may park GPU filter composites for a later blit.
    pub allow_pending_composites: bool,
    /// Output scene for this frame.
    pub scene: &'b mut vello::Scene,
    /// Output hit regions (world bounds) for this frame, keyed by entity id.
    pub hit_regions: &'b mut Vec<(crate::timeline::EntityId, kurbo::Rect)>,
    /// Output observable scene items, when item collection was requested.
    pub program_items: &'b mut Option<Vec<crate::timeline::scene_program::SceneItem>>,
    /// The active filter backend, when one is available.
    pub filter_backend: &'b mut Option<&'c mut dyn crate::timeline::effects::FilterBackend>,
}

impl<'a, 'b, 'c> RenderChildrenCtx<'a, 'b, 'c> {
    /// The container track these children belong to.
    pub fn track(&self) -> Option<&AnimationTrack> {
        self.timeline
            .world
            .tracks
            .get_by_id(self.host_id)
            .or_else(|| self.timeline.world.tracks.get(self.node_label))
    }

    /// This context's read-only frame state, for the render recursion.
    ///
    /// The returned frame borrows the *frame data* (`'a`), not `self`, so the
    /// caller can still take the mutable outputs afterwards.
    fn render_frame(&self) -> RenderFrame<'a> {
        RenderFrame {
            time_ms: self.time_ms,
            scene_dimensions: self.scene_dimensions,
            debug_options: self.debug_options,
            overrides: self.overrides,
            frame_env: self.frame_env,
        }
    }

    /// Render one child into this context's scene, preserving the caller's
    /// Render one child by its label into this context's scene, forwarding the
    /// `allow_pending_composites` flag.
    pub fn render_child(&mut self, child: &str) {
        if let Some(child_id) = self.timeline.world.tracks.get_id(child) {
            self.render_child_by_id(child_id);
        } else {
            let frame = self.render_frame();
            let allow_pending_composites = self.allow_pending_composites;
            let mut out = RenderOutputs {
                scene: &mut *self.scene,
                hit_regions: &mut *self.hit_regions,
                program_items: &mut *self.program_items,
                filter_backend: &mut *self.filter_backend,
            };
            self.timeline.evaluate_node(
                child,
                self.global_transform,
                self.global_opacity,
                &self.layout_positions,
                allow_pending_composites,
                &frame,
                &mut out,
            );
        }
    }

    /// Render one child directly by its `EntityId` into this context's scene,
    /// avoiding string hashing overhead on the hot render path.
    pub fn render_child_by_id(&mut self, child_id: crate::timeline::EntityId) {
        let frame = self.render_frame();
        let allow_pending_composites = self.allow_pending_composites;
        let mut out = RenderOutputs {
            scene: &mut *self.scene,
            hit_regions: &mut *self.hit_regions,
            program_items: &mut *self.program_items,
            filter_backend: &mut *self.filter_backend,
        };
        self.timeline.evaluate_node_by_id(
            child_id,
            self.global_transform,
            self.global_opacity,
            &self.layout_positions,
            allow_pending_composites,
            &frame,
            &mut out,
        );
    }

    /// Render one child into a caller-provided scene (the Filter strategy
    /// renders into an offscreen sub-scene, which never parks composites).
    pub fn render_child_into(&mut self, scene: &mut vello::Scene, child: &str) {
        if let Some(child_id) = self.timeline.world.tracks.get_id(child) {
            self.render_child_into_by_id(scene, child_id);
        } else {
            let frame = self.render_frame();
            let mut out = RenderOutputs {
                scene,
                hit_regions: &mut *self.hit_regions,
                program_items: &mut *self.program_items,
                filter_backend: &mut *self.filter_backend,
            };
            self.timeline.evaluate_node(
                child,
                self.global_transform,
                self.global_opacity,
                &self.layout_positions,
                false,
                &frame,
                &mut out,
            );
        }
    }

    /// Render one child directly by its `EntityId` into a caller-provided scene.
    pub fn render_child_into_by_id(
        &mut self,
        scene: &mut vello::Scene,
        child_id: crate::timeline::EntityId,
    ) {
        let frame = self.render_frame();
        let mut out = RenderOutputs {
            scene,
            hit_regions: &mut *self.hit_regions,
            program_items: &mut *self.program_items,
            filter_backend: &mut *self.filter_backend,
        };
        self.timeline.evaluate_node_by_id(
            child_id,
            self.global_transform,
            self.global_opacity,
            &self.layout_positions,
            false,
            &frame,
            &mut out,
        );
    }

    /// Default child rendering: every child in order into this context's scene.
    pub fn render_children_default(&mut self, children: &[&str]) {
        for child in children {
            self.render_child(child);
        }
    }

    /// Render children directly by `EntityId` into this context's scene,
    /// avoiding string lookups on the hot render path.
    pub fn render_children_ids(&mut self, children_ids: &[crate::timeline::EntityId]) {
        for &child_id in children_ids {
            self.render_child_by_id(child_id);
        }
    }

    /// Alias for `render_children_ids`.
    pub fn render_children_by_id(&mut self, children_ids: &[crate::timeline::EntityId]) {
        self.render_children_ids(children_ids);
    }

    /// Render children directly by `EntityId` into a caller-provided scene.
    pub fn render_children_into_by_id(
        &mut self,
        scene: &mut vello::Scene,
        children_ids: &[crate::timeline::EntityId],
    ) {
        for &child_id in children_ids {
            self.render_child_into_by_id(scene, child_id);
        }
    }

    /// Push a runtime render diagnostic (surfaced through the frame's
    /// diagnostics, e.g. for a filter fallback).
    pub fn push_diagnostic(&self, diagnostic: Diagnostic) {
        self.timeline.eval_caches.runtime_diagnostics.borrow_mut().push(diagnostic);
    }
}

/// Mutable context for text recompilation.
///
/// Only text primitives need this. Shape, image, and SVG primitives
/// can ignore it entirely.
pub struct TextCompileCtx<'a> {
    /// Text compiler for runtime text recompilation.
    pub text_compiler: &'a mut crate::renderer::text::TextCompiler,
    /// Font context for text rendering.
    pub font_context: &'a crate::renderer::text::FontContext,
}

/// Data one child actor contributes to a parent `Equation`'s Typst document.
///
/// Returned by [`crate::primitives::Primitive::equation_fragment`]; the Equation container collects
/// every child whose primitive returns `Some`, in source order, and joins them
/// into one compiled document. Making this a trait query (rather than a match on
/// the actor's type name) lets any primitive — including an extension — be an
/// equation fragment.
#[derive(Clone, Debug, PartialEq)]
pub struct EquationFragment {
    /// Raw fragment markup; the Equation container escapes and joins it.
    pub content: String,
    /// Highlight rectangle fill color.
    pub highlight_color: [f32; 4],
    /// Highlight layer opacity (`0.0` = no highlight).
    pub highlight_opacity: f32,
    /// Highlight padding around the fragment.
    pub highlight_padding: f32,
    /// Highlight corner radius.
    pub highlight_radius: f32,
    /// Highlight blend mode.
    pub highlight_blend: vello::peniko::Mix,
}

/// Returned by [`crate::primitives::Primitive::caption_cue`]; the Captions container collects
/// every child whose primitive returns `Some`, in source order.
#[derive(Clone, Debug, PartialEq)]
pub struct CaptionCue {
    /// Cue subtitle text.
    pub text: String,
    /// Cue start time in milliseconds.
    pub start_ms: u64,
    /// Cue end time in milliseconds.
    pub end_ms: u64,
    /// Optional active word highlight color override for this cue.
    pub active_color: Option<[f32; 4]>,
}
