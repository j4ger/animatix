//! Assets tab.

use super::*;

pub(super) fn assets_content_ui(ctx: &mut AssetsContext<'_>, ui: &mut egui::Ui) {
    let t = eparts::theme(ui);
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    let Some(cache) = ctx.asset_cache else {
        layout::empty_state(
            ui,
            egui_phosphor::regular::IMAGES,
            "No assets loaded",
            "Add Image or SVG actors to populate the asset cache.",
        );
        return;
    };

    let images: Vec<(String, &animatix::timeline::image::SceneImage)> =
        cache.images().map(|(k, v)| (k.clone(), v)).collect();
    let svgs: Vec<(String, &Vec<animatix::timeline::vello_path::VelloPath>)> =
        cache.svg_paths().map(|(k, v)| (k.clone(), v)).collect();

    if images.is_empty() && svgs.is_empty() {
        layout::empty_state(
            ui,
            egui_phosphor::regular::IMAGES,
            "No assets loaded",
            "Add Image or SVG actors to populate the asset cache.",
        );
        return;
    }

    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        if !images.is_empty() {
            layout::section_header(ui, egui_phosphor::regular::IMAGE, "Images", Some(images.len()));
            for (path, _img) in &images {
                let row_id = ui.id().with(path);
                let filename =
                    std::path::Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path);
                let response = row::Row::new(filename)
                    .icon(Some(egui_phosphor::regular::IMAGE))
                    .label_color(t.palette.text.secondary)
                    .sense(egui::Sense::click_and_drag())
                    .show(ui, row_id);

                let props = vec![animatix_syntax::ast::Property {
                    name: "url".into(),
                    value: animatix_syntax::ast::Expr::Str(path.clone()),
                    value_span: None,
                    trailing_comment: None,
                }];
                if response.response.double_clicked() {
                    let label = crate::app::utils::labels::unique_label(None, "image");
                    let pos = [
                        ctx.scene_dimensions.width as f32 / 2.0,
                        ctx.scene_dimensions.height as f32 / 2.0,
                    ];
                    ctx.commands.push_back(
                        ActorCommand::CreateActor {
                            ty: "Image".into(),
                            label,
                            position: pos,
                            props: props.clone(),
                        }
                        .into(),
                    );
                }

                // Drag-to-place onto the canvas (see `components_content_ui`).
                if response.drag_started {
                    crate::app::panels::set_library_drag(
                        ui.ctx(),
                        crate::app::panels::LibraryDragPayload {
                            ty: "Image".into(),
                            props: props.clone(),
                        },
                    );
                }
            }
            ui.add_space(sp.base.space_3);
        }

        if !svgs.is_empty() {
            layout::section_header(ui, egui_phosphor::regular::FILE_SVG, "SVGs", Some(svgs.len()));
            for (path, _svg) in &svgs {
                let row_id = ui.id().with(path);
                let filename =
                    std::path::Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path);
                let response = row::Row::new(filename)
                    .icon(Some(egui_phosphor::regular::FILE_SVG))
                    .label_color(t.palette.text.secondary)
                    .sense(egui::Sense::click_and_drag())
                    .show(ui, row_id);

                let props = vec![animatix_syntax::ast::Property {
                    name: "url".into(),
                    value: animatix_syntax::ast::Expr::Str(path.clone()),
                    value_span: None,
                    trailing_comment: None,
                }];
                if response.response.double_clicked() {
                    let label = crate::app::utils::labels::unique_label(None, "svg");
                    let pos = [
                        ctx.scene_dimensions.width as f32 / 2.0,
                        ctx.scene_dimensions.height as f32 / 2.0,
                    ];
                    ctx.commands.push_back(
                        ActorCommand::CreateActor {
                            ty: "Svg".into(),
                            label,
                            position: pos,
                            props: props.clone(),
                        }
                        .into(),
                    );
                }

                // Drag-to-place onto the canvas (see `components_content_ui`).
                if response.drag_started {
                    crate::app::panels::set_library_drag(
                        ui.ctx(),
                        crate::app::panels::LibraryDragPayload {
                            ty: "Svg".into(),
                            props: props.clone(),
                        },
                    );
                }
            }
        }
    });
}
