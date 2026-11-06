//! Inherent impl and dispatch helpers for the built-in primitive catalog.

use std::sync::OnceLock;

use super::{BuiltIn, Primitive, PrimitiveInfo, PrimitiveRegistry};
use crate::timeline::ActorCategory;
use crate::timeline::behavior::{BuildExtraFn, EvaluateFn, FamilyBehavior};

/// Map the runtime UI category onto the schema category used by tooling.
pub(crate) fn actor_category_to_primitive_category(
    category: ActorCategory,
) -> animatix_syntax::schema::PrimitiveCategory {
    match category {
        ActorCategory::Shape => animatix_syntax::schema::PrimitiveCategory::Shape,
        ActorCategory::Text => animatix_syntax::schema::PrimitiveCategory::Text,
        ActorCategory::Media => animatix_syntax::schema::PrimitiveCategory::Media,
        ActorCategory::Plot => animatix_syntax::schema::PrimitiveCategory::Plot,
        ActorCategory::Container => animatix_syntax::schema::PrimitiveCategory::Container,
        ActorCategory::Annotation => animatix_syntax::schema::PrimitiveCategory::Annotation,
    }
}

impl BuiltIn {
    /// Pair a catalog card with its behaviour implementation and family hooks.
    pub const fn new(
        info: &'static PrimitiveInfo,
        behavior: &'static dyn Primitive,
        evaluate: EvaluateFn,
        build_extra: Option<BuildExtraFn>,
    ) -> Self {
        Self {
            info,
            behavior,
            family: FamilyBehavior {
                evaluate,
                build_extra,
            },
        }
    }
}

/// The built-in primitive catalog (the `animatix-std` identity cards).
pub fn primitive_catalog() -> &'static [&'static PrimitiveInfo] {
    animatix_std::CATALOG
}

/// Look up built-in metadata by the actor's authored type name.
pub fn primitive_info_by_name(name: &str) -> Option<&'static PrimitiveInfo> {
    animatix_std::catalog_lookup(name)
}

/// Expose built-in primitive metadata through the shared schema model.
///
/// Single conversion implementation lives on [`PrimitiveRegistry::specs`];
/// this free function is a thin wrapper over the built-in registry so the
/// spec-building logic is not duplicated.
pub fn primitive_specs() -> Vec<animatix_syntax::schema::PrimitiveSpec> {
    builtin_primitive_registry().specs()
}

// ── Dispatch helpers ────────────────────────────────────────────────────

/// Look up a built-in primitive by type name.
///
/// # Deprecated
/// Built-in runtime evaluation and build behaviors have moved to family systems
/// under `timeline::behavior::builtin`. This compatibility helper remains for
/// tooling and extension inspection.
#[deprecated(
    since = "0.5.0",
    note = "built-in runtime behavior moved to timeline::behavior::builtin"
)]
pub fn find_primitive(ty: &str) -> Option<&'static dyn Primitive> {
    builtin_primitive_registry().find(ty)
}

fn builtin_primitive_registry() -> &'static PrimitiveRegistry {
    static REGISTRY: OnceLock<PrimitiveRegistry> = OnceLock::new();
    REGISTRY.get_or_init(PrimitiveRegistry::new)
}
