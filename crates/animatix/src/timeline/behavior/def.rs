//! BehaviorId, BehaviorSource, and BehaviorDef.

use crate::primitives::{CaptionCue, EquationFragment};
use crate::timeline::TrackAccessor as _;
use animatix_core::caps::ActorCaps;

/// Compact identifier for an actor behavior in the behavior table.
///
/// Built-in primitives occupy indices `0..36` in [`crate::primitives::BUILT_INS`] order;
/// extension primitives receive dynamically assigned indices `>= 36`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BehaviorId(pub u16);

impl BehaviorId {
    /// Sentinel for an unresolved or unknown behavior.
    pub const UNKNOWN: Self = Self(u16::MAX);

    /// Convert to a `usize` index into the behavior registry.
    #[inline(always)]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// Check if this behavior ID is valid (not `UNKNOWN`).
    #[inline(always)]
    pub const fn is_valid(self) -> bool {
        self.0 != u16::MAX
    }
}

impl From<u16> for BehaviorId {
    #[inline(always)]
    fn from(id: u16) -> Self {
        Self(id)
    }
}

impl From<usize> for BehaviorId {
    #[inline(always)]
    fn from(idx: usize) -> Self {
        Self(idx as u16)
    }
}

/// Function pointer signature for behavior evaluation.
pub type EvaluateFn = fn(
    &crate::primitives::EvaluateCtx,
    Option<&mut crate::primitives::TextCompileCtx>,
) -> Result<
    Option<Vec<crate::primitives::RenderCommand>>,
    crate::renderer::error::RenderError,
>;

/// Function pointer signature for behavior custom build logic.
///
/// `ty` is the canonical built-in type name from the owning `BUILT_INS` row, so
/// a shared family implementation can dispatch without re-deriving it.
pub type BuildExtraFn = fn(
    &mut crate::timeline::Timeline,
    label: &str,
    ty: &'static str,
    props: &[crate::ast::Property],
    modifiers: &[crate::ast::Modifier],
    children: &[crate::ast::InlineItem],
    time_ms: f64,
    parent_label: Option<&str>,
    diagnostics: &mut Vec<crate::diagnostics::Diagnostic>,
) -> Result<(), Vec<crate::diagnostics::Diagnostic>>;

/// Behavior implementation for a built-in primitive family.
#[derive(Clone, Copy)]
pub struct FamilyBehavior {
    /// Function pointer for evaluating draw commands.
    pub evaluate: EvaluateFn,
    /// Optional function pointer for custom actor declaration build logic.
    pub build_extra: Option<BuildExtraFn>,
}

/// Storage source for a primitive behavior implementation.
#[derive(Clone)]
pub(crate) enum BehaviorSource {
    /// A built-in primitive backed by a family behavior system.
    Family(FamilyBehavior),
    /// A runtime-registered extension or plugin primitive.
    Extension(std::sync::Arc<dyn crate::primitives::Primitive>),
}

/// Complete immutable definition for an actor behavior in the engine.
#[derive(Clone)]
pub struct BehaviorDef {
    /// Compact unique identifier for this behavior.
    pub id: BehaviorId,
    /// Authored type name / registration identifier.
    pub name: Box<str>,
    /// Capability projection for actors with this behavior.
    pub caps: ActorCaps,
    pub(crate) source: BehaviorSource,
}

impl BehaviorDef {
    /// Evaluate this behavior's draw commands.
    pub fn evaluate(
        &self,
        ctx: &crate::primitives::EvaluateCtx,
        text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        match &self.source {
            BehaviorSource::Family(fam) => (fam.evaluate)(ctx, text_ctx),
            BehaviorSource::Extension(ext) => ext.evaluate(ctx, text_ctx),
        }
    }

    /// If this behavior is backed by a runtime extension primitive, return a reference to it.
    pub fn extension_primitive(&self) -> Option<&dyn crate::primitives::Primitive> {
        match &self.source {
            BehaviorSource::Family(_) => None,
            BehaviorSource::Extension(ext) => Some(ext.as_ref()),
        }
    }

    /// Return the subtitle cue data if this behavior represents a caption cue.
    pub fn caption_cue(&self, ctx: &crate::primitives::EvaluateCtx) -> Option<CaptionCue> {
        match &self.source {
            BehaviorSource::Family(_) => {
                if self.name.as_ref() == "Cue" {
                    let text = ctx
                        .text
                        .map(|t| t.text_content.get(ctx.time_ms, String::new()))
                        .unwrap_or_default();
                    let start_s = ctx.sample_f32_prop("start", ctx.time_ms, 0.0);
                    let end_s = ctx.sample_f32_prop("end", ctx.time_ms, 0.0);
                    let start_ms = (start_s * 1000.0).round() as u64;
                    let end_ms = (end_s * 1000.0).round() as u64;
                    let active_color = ctx
                        .sample_property_named("active_color", ctx.time_ms)
                        .and_then(|pv| match pv {
                            crate::timeline::PropertyValue::Color(c)
                            | crate::timeline::PropertyValue::Vec4(c) => Some(c),
                            _ => None,
                        });

                    Some(CaptionCue {
                        text,
                        start_ms,
                        end_ms,
                        active_color,
                    })
                } else {
                    None
                }
            },
            BehaviorSource::Extension(ext) => ext.caption_cue(ctx),
        }
    }

    /// Return the equation fragment data if this behavior contributes equation fragments.
    pub fn equation_fragment(
        &self,
        ctx: &crate::primitives::EvaluateCtx,
    ) -> Option<EquationFragment> {
        match &self.source {
            BehaviorSource::Family(_) => {
                if let (Some(text), Some(highlight)) = (ctx.text, ctx.highlight) {
                    Some(EquationFragment {
                        content: text.text_content.get(ctx.time_ms, String::new()),
                        highlight_color: highlight
                            .highlight_color
                            .get(ctx.time_ms, [0.3, 0.5, 1.0, 1.0]),
                        highlight_opacity: highlight.highlight_opacity.get(ctx.time_ms, 0.0),
                        highlight_padding: highlight.highlight_padding.get(ctx.time_ms, 4.0),
                        highlight_radius: highlight.highlight_radius.get(ctx.time_ms, 3.0),
                        highlight_blend: highlight.highlight_blend,
                    })
                } else {
                    None
                }
            },
            BehaviorSource::Extension(ext) => ext.equation_fragment(ctx),
        }
    }
}

impl std::fmt::Debug for BehaviorDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BehaviorDef")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("caps", &self.caps)
            .finish()
    }
}
