//! BehaviorId, BehaviorSource, and BehaviorDef.

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
