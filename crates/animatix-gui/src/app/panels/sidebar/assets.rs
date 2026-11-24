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
                .map(|(path, _)| (path.as_str(), "Image"))
                .chain(cache.svg_paths().map(|(path, _)| (path.as_str(), "SVG")))
                .filter(|(path, _)| referenced.contains(path)),
        );
    }
    if assets.is_empty() {
        ui.label(egui::RichText::new("No images or SVGs in the current build.").small().weak());
        return;
    }
    for (path, kind) in assets {
        let filename = Path::new(path).file_name().and_then(|p| p.to_str()).unwrap_or(path);
        ui.add(egui::Label::new(filename).truncate())
            .on_hover_text(format!("{kind} · {path}"));
    }
}
