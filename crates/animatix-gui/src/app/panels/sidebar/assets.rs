//! Asset inventory, not a canvas insertion palette.
use super::*;
pub(super) fn assets_content_ui(
    document: &crate::app::document::DocumentSession,
    ui: &mut egui::Ui,
) {
    let timelines = document.timeline.iter().chain(
        document
            .composition
            .iter()
            .flat_map(|composition| composition.scenes.values().map(|scene| &scene.timeline)),
    );
    let mut assets = std::collections::BTreeSet::new();
    for timeline in timelines {
        let cache = timeline.asset_cache();
        let referenced: HashSet<_> = cache.asset_usage().map(|(path, _)| path.as_str()).collect();
        assets.extend(
            cache
                .images()
                .map(|(path, _)| (path.as_str(), "gui-assets-image"))
                .chain(cache.svg_paths().map(|(path, _)| (path.as_str(), "gui-project-asset-svg")))
                .filter(|(path, _)| referenced.contains(path)),
        );
    }
    if assets.is_empty() {
        ui.label(
            egui::RichText::new(crate::app::i18n::text(ui, "gui-project-assets-empty"))
                .small()
                .weak(),
        );
        return;
    }
    for (path, kind) in assets {
        let filename = Path::new(path).file_name().and_then(|p| p.to_str()).unwrap_or(path);
        ui.add(egui::Label::new(filename).truncate())
            .on_hover_text(crate::app::i18n::render(
                ui,
                "gui-assets-entry",
                &[
                    ("kind", crate::app::i18n::text(ui, kind)),
                    ("name", path.to_owned()),
                ],
            ));
    }
}
