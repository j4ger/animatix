use super::*;

pub(super) fn build_timeline_with_context(
    statements: &[Stmt],
    namespaces: &HashMap<String, Namespace>,
    font_context: std::sync::Arc<crate::renderer::text::FontContext>,
    build_quality: crate::timeline::BuildQuality,
    asset_cache: Option<std::sync::Arc<crate::timeline::assets::AssetCache>>,
    context: Option<std::sync::Arc<crate::extension_context::ExtensionContext>>,
) -> BuildReport<Timeline> {
    match context {
        Some(ctx) => {
            Timeline::build_with_diagnostics_and_font_context_and_asset_cache_and_extension_context(
                statements,
                namespaces,
                font_context,
                build_quality,
                asset_cache,
                ctx,
            )
        },
        None => Timeline::build_with_diagnostics_and_font_context_and_asset_cache(
            statements,
            namespaces,
            font_context,
            build_quality,
            asset_cache,
        ),
    }
}

pub(super) fn build_timeline_with_carry_context(
    statements: &[Stmt],
    namespaces: &HashMap<String, Namespace>,
    font_context: std::sync::Arc<crate::renderer::text::FontContext>,
    build_quality: crate::timeline::BuildQuality,
    carry: Option<&crate::timeline::persistence::CarryBag>,
    source_timeline: Option<&Timeline>,
    source_duration_ms: u64,
    dims: [f64; 2],
    asset_cache: Option<std::sync::Arc<crate::timeline::assets::AssetCache>>,
    context: Option<std::sync::Arc<crate::extension_context::ExtensionContext>>,
) -> BuildReport<Timeline> {
    match context {
        Some(ctx) => Timeline::build_with_carry_and_asset_cache_and_extension_context(
            statements,
            namespaces,
            font_context,
            build_quality,
            carry,
            source_timeline,
            source_duration_ms,
            dims,
            asset_cache,
            ctx,
        ),
        None => Timeline::build_with_carry_and_asset_cache(
            statements,
            namespaces,
            font_context,
            build_quality,
            carry,
            source_timeline,
            source_duration_ms,
            dims,
            asset_cache,
        ),
    }
}

pub(super) fn validate_play_target(
    target: &str,
    scenes: &BTreeMap<String, CompositionScene>,
    namespaces: &HashMap<String, Namespace>,
    diagnostics: &mut Vec<Diagnostic>,
    source_scene: &str,
) -> bool {
    if target.contains('.') {
        let parts: Vec<&str> = target.split('.').collect();
        if parts.len() >= 2 {
            let (namespace_parts, scene_name) = parts.split_at(parts.len() - 1);
            let scene_name = scene_name[0];
            if let Some(ns) = resolve_namespace(namespaces, namespace_parts) {
                // If the namespace has scene data, verify the specific scene exists.
                // If scenes are empty (legacy/test namespace), accept any name.
                if ns.scenes.is_empty() || ns.scenes.contains_key(scene_name) {
                    return true;
                }
                diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode::PlayTargetNotFound,
                        DiagnosticPhase::Build,
                        format!(
                            "Scene '{}' plays non-existent scene '{}' in namespace '{}'.",
                            source_scene,
                            scene_name,
                            namespace_parts.join(".")
                        ),
                    )
                    .with_subject(source_scene),
                );
                return false;
            }
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::PlayTargetNotFound,
                    DiagnosticPhase::Build,
                    format!(
                        "Scene '{}' plays non-existent module/scene '{}'.",
                        source_scene, target
                    ),
                )
                .with_subject(source_scene),
            );
            return false;
        }
    }

    if scenes.contains_key(target) {
        true
    } else {
        diagnostics.push(
            Diagnostic::error(
                DiagnosticCode::PlayTargetNotFound,
                DiagnosticPhase::Build,
                format!("Scene '{}' plays non-existent scene '{}'.", source_scene, target),
            )
            .with_subject(source_scene),
        );
        false
    }
}

pub(super) fn resolve_namespace<'a>(
    namespaces: &'a HashMap<String, Namespace>,
    parts: &[&str],
) -> Option<&'a Namespace> {
    let mut current = namespaces.get(*parts.first()?)?;
    for part in &parts[1..] {
        current = current.namespaces.get(*part)?;
    }
    Some(current)
}
