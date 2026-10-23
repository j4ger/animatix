//! Components tab.

use super::*;

pub(super) fn components_content_ui(ctx: &mut ComponentsContext<'_>, ui: &mut egui::Ui) {
    let t = eparts::theme(ui);
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    if ctx.components.is_empty() {
        layout::empty_state(
            ui,
            egui_phosphor::regular::CUBE,
            "No components",
            "Import modules with pub component definitions to see them here.",
        );
        return;
    }

    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        let mut names: Vec<&String> = ctx.components.keys().collect();
        names.sort();
        for name in names {
            let entry = &ctx.components[name];
            let row_id = ui.id().with(name);
            let response = row::Row::new(name)
                .icon(Some(egui_phosphor::regular::CUBE))
                .label_color(t.palette.text.secondary)
                .sense(egui::Sense::click_and_drag())
                .right(|ui| {
                    let jump_btn = ui.add(
                        Button::icon(egui_phosphor::regular::ARROW_SQUARE_OUT)
                            .with_size(eparts::Size::Sm),
                    );
                    text_tooltip(ui, jump_btn.id.with("jump_tip"), &jump_btn, "Jump to definition");
                    if jump_btn.clicked() {
                        let patterns = [
                            format!("pub component {}", name),
                            format!("component {}", name),
                        ];
                        let found_line = ctx
                            .source_text
                            .lines()
                            .position(|line| patterns.iter().any(|p| line.trim().starts_with(p)));
                        if let Some(line) = found_line {
                            ctx.commands
                                .push_back(ShellAction::Command(Command::ScrollToLine(line, 0)));
                            // The editor is no longer a sidebar tab; open the
                            // Code tab in the detail region instead.
                            ctx.commands.push_back(ShellAction::View(ViewAction::ShowCode));
                        }
                    }
                })
                .show(ui, row_id);

            let pos = [
                ctx.scene_dimensions.width as f32 / 2.0,
                ctx.scene_dimensions.height as f32 / 2.0,
            ];
            let template = crate::app::primitives::ActorTemplate::for_primitive(
                name,
                Some(pos),
                ctx.scene_dimensions,
                None,
            );

            if response.response.double_clicked() {
                // Instantiate component with default props
                ctx.commands.push_back(
                    ActorCommand::CreateActor {
                        ty: template.ty.clone(),
                        label: template.label.clone(),
                        position: pos,
                        props: template.props.clone(),
                    }
                    .into(),
                );
            }

            // Drag-to-place: hand the preview panel the actor to create at the
            // drop point. The payload mirrors the double-click instantiate above.
            if response.drag_started {
                crate::app::panels::set_library_drag(
                    ui.ctx(),
                    crate::app::panels::LibraryDragPayload {
                        ty: template.ty,
                        props: template.props,
                    },
                );
            }

            // Slots display
            let slots: Vec<String> = entry
                .definition
                .body
                .iter()
                .filter_map(|stmt| {
                    if let animatix_syntax::ast::Stmt::ActorDecl {
                        label, children, ..
                    } = stmt
                    {
                        if children.iter().any(|item| {
                            matches!(item, animatix_syntax::ast::InlineItem::SlotMarker)
                        }) {
                            Some(label.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect();

            if !slots.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(sp.base.component.icon_slot_width + sp.base.space_2);
                    ui.label(
                        egui::RichText::new(format!("@slots: {}", slots.join(", ")))
                            .size(TextRole::Micro.size())
                            .color(t.palette.accent.cyan),
                    );
                });
            }

            // Show params as sub-label
            if !entry.definition.params.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(sp.base.component.icon_slot_width + sp.base.space_2);
                    let params: Vec<String> = entry
                        .definition
                        .params
                        .iter()
                        .map(|p| {
                            let default = p
                                .default
                                .as_ref()
                                .map(|v| format!(" = {}", v.to_source()))
                                .unwrap_or_default();
                            format!("{}{}", p.name, default)
                        })
                        .collect();
                    ui.label(
                        egui::RichText::new(params.join(", "))
                            .size(TextRole::Micro.size())
                            .color(t.palette.text.muted),
                    );
                });
            }

            response.response.context_menu(|ui| {
                let entries = vec![MenuEntry::item_with_icon(
                    egui_phosphor::regular::PLUS,
                    "Instantiate",
                )];
                if render_menu(ui, &entries).is_some() {
                    let pos = [
                        ctx.scene_dimensions.width as f32 / 2.0,
                        ctx.scene_dimensions.height as f32 / 2.0,
                    ];
                    let template = crate::app::primitives::ActorTemplate::for_primitive(
                        name,
                        Some(pos),
                        ctx.scene_dimensions,
                        None,
                    );
                    ctx.commands.push_back(
                        ActorCommand::CreateActor {
                            ty: template.ty,
                            label: template.label,
                            position: pos,
                            props: template.props,
                        }
                        .into(),
                    );
                    ui.close();
                }
            });
        }
    });
}

// ─── Assets Tab ───────────────────────────────────────────────────────────
