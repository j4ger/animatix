//! The lowering pass itself: one orchestration function over the named stages.
//!
//! Stage order is load-bearing: seed the environment, walk statements, lower
//! the `always` bodies, validate, then finalize derived state.

use super::*;

impl Timeline {
    /// Internal build implementation shared by all public build entry points.
    pub(super) fn build_impl(
        ast: &[Stmt],
        namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
        options: BuildOptions<'_>,
    ) -> BuildReport<Self> {
        let _rebuild_stage = crate::perf::ScopedStage::new(crate::perf::stage::REBUILD);
        // Clear expression evaluation cache at the start of each build.
        crate::timeline::utils::clear_eval_cache();

        let font_context = options
            .font_context
            .clone()
            .unwrap_or_else(|| std::sync::Arc::new(crate::renderer::text::FontContext::new()));
        let mut timeline = Self::new_with_font_context(font_context);
        let mut diagnostics = Vec::new();

        // 1. Seed the build environment and lower speech-anchored keyframe times.
        let seeded = super::seed::seed_environment(
            &mut timeline,
            ast,
            namespaces,
            &options,
            &mut diagnostics,
        );

        // 2. Lower declarations, keyframes and actions into tracks; attach
        //    the transcripts the seeding pass collected.
        super::walk::walk_statements(
            &mut timeline,
            &seeded.ast,
            seeded.transcripts,
            &mut diagnostics,
        );

        // 3. Lower `always` bodies to IR and validate their property references.
        super::walk::lower_modifiers(&mut timeline, &mut diagnostics);

        // 4. Post-walk validation: reveals, duration/loop seam, registrations.
        super::validate::validate_reveals(&timeline, &mut diagnostics);
        super::validate::validate_duration_and_seam(&mut timeline, &mut diagnostics);
        super::validate::validate_actors(&mut timeline, &mut diagnostics);

        // 5. Freeze the environment and derive legend/idle/audio state.
        super::finalize::finalize_assets_legend(&mut timeline, &mut diagnostics);

        BuildReport::new(timeline, diagnostics)
    }
}
