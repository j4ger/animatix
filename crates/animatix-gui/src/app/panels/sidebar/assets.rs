//! Asset inventory, not a canvas insertion palette.
use super::*;
pub(super) fn assets_content_ui(ctx: &mut AssetsContext<'_>, ui: &mut egui::Ui) {
    let Some(cache) = ctx.asset_cache else {
        layout::empty_state(
            ui,
            egui_phosphor::regular::IMAGES,
            "No cached assets",
            "Declare image and SVG sources in code.",
        );
        return;
    };
    ui.label(egui::RichText::new("Assets from the current build").small().weak());
    let mut assets: Vec<_> = cache
        .images()
        .map(|(path, _)| (path.as_str(), "Image"))
        .chain(cache.svg_paths().map(|(path, _)| (path.as_str(), "SVG")))
        .collect();
    assets.sort();
    egui::ScrollArea::vertical().show(ui, |ui| {
        for (path, kind) in assets {
            ui.label(format!(
                "{kind} · {}",
                Path::new(path).file_name().and_then(|p| p.to_str()).unwrap_or(path)
            ))
            .on_hover_text(path);
        }
    });
}
