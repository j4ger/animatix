//! Unified insertion palette — fuzzy-searchable overlay for primitives, actions, and snippets.

use egui::{Color32, Pos2, Rect, RichText, Stroke, Vec2};

use crate::app::commands::Command;
use crate::app::design_tokens::*;
use crate::app::insertion::{InsertionContext, InsertionRequest};
use crate::app::GuiShell;

/// Mode filter for the palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteMode {
    Universal,
    Primitives,
    Actions,
    Snippets,
    Components,
}

/// A single item in the palette list.
#[derive(Debug, Clone)]
pub struct PaletteItem {
    pub label: String,
    pub detail: String,
    pub icon: String,
    pub color: Color32,
    pub kind: ItemKind,
}

#[derive(Debug, Clone)]
pub enum ItemKind {
    Primitive { type_name: String },
    Action { verb: String },
    #[allow(dead_code)]
    Snippet { text: String },
    Component { type_name: String, params: Vec<(String, Option<String>)> },
}

/// State for the insertion palette overlay.
pub struct InsertionPalette {
    pub open: bool,
    pub query: String,
    pub selected_index: usize,
    pub mode: PaletteMode,
    items: Vec<PaletteItem>,
    filtered: Vec<usize>, // indices into items
    param_form: Option<ParamFormState>,
}

#[derive(Debug, Clone)]
struct ParamFormState {
    type_name: String,
    params: Vec<(String, String)>, // (name, value)
}

impl Default for InsertionPalette {
    fn default() -> Self {
        Self {
            open: false,
            query: String::new(),
            selected_index: 0,
            mode: PaletteMode::Universal,
            items: Vec::new(),
            filtered: Vec::new(),
            param_form: None,
        }
    }
}

impl InsertionPalette {
    /// Populate items from the core registries.
    pub fn populate(&mut self,
        _timeline: Option<&animatix::timeline::Timeline>,
        components: &std::collections::HashMap<String, animatix_syntax::module::ComponentEntry>,
    ) {
        self.param_form = None;
        self.items.clear();

        // Primitives
        for prim in animatix::primitives::PRIMITIVES.iter() {
            self.items.push(PaletteItem {
                label: prim.display_name().to_string(),
                detail: prim.type_name().to_string(),
                icon: prim.icon_id().to_string(),
                color: category_color(prim.category()),
                kind: ItemKind::Primitive {
                    type_name: prim.type_name().to_string(),
                },
            });
        }

        // Actions
        for sig in animatix::timeline::actions::get_action_signatures() {
            self.items.push(PaletteItem {
                label: sig.name.clone(),
                detail: sig.description.clone(),
                icon: "⚡".to_string(),
                color: action_category_color(&sig.category),
                kind: ItemKind::Action {
                    verb: sig.name.clone(),
                },
            });
        }

        // Snippets
        for snippet in animatix_analyzer::all_snippets() {
            self.items.push(PaletteItem {
                label: snippet.label.clone(),
                detail: snippet.detail.unwrap_or_default(),
                icon: egui_phosphor::regular::CODE.to_string(),
                color: Color32::from_rgb(108, 153, 187),
                kind: ItemKind::Snippet {
                    text: snippet.insert_text.unwrap_or(snippet.label),
                },
            });
        }

        // Components
        for (name, entry) in components {
            let params_info: Vec<(String, Option<String>)> = entry.definition.params.iter().map(|p| {
                let default_str = p.default.as_ref().map(|e| animatix_syntax::to_source::expr_to_source(e));
                (p.name.clone(), default_str)
            }).collect();
            let params_display: Vec<String> = entry.definition.params.iter().map(|p| {
                p.default.as_ref().map(|_| p.name.clone()).unwrap_or_else(|| format!("{}?", p.name))
            }).collect();
            self.items.push(PaletteItem {
                label: name.clone(),
                detail: if params_display.is_empty() { "Component".into() } else { format!("Component — {}", params_display.join(", ")) },
                icon: egui_phosphor::regular::CUBE.to_string(),
                color: Color32::from_rgb(160, 180, 220),
                kind: ItemKind::Component {
                    type_name: name.clone(),
                    params: params_info,
                },
            });
        }

        self.rebuild_filter();
    }

    /// Open the palette with the given default mode.
    pub fn open(&mut self, mode: PaletteMode) {
        self.open = true;
        self.mode = mode;
        self.query.clear();
        self.selected_index = 0;
        self.param_form = None;
        self.rebuild_filter();
    }

    /// Close the palette.
    pub fn close(&mut self) {
        self.open = false;
    }

    fn rebuild_filter(&mut self) {
        self.filtered.clear();
        let q = self.query.to_lowercase();
        for (i, item) in self.items.iter().enumerate() {
            let matches_mode = match self.mode {
                PaletteMode::Universal => true,
                PaletteMode::Primitives => matches!(item.kind, ItemKind::Primitive { .. }),
                PaletteMode::Actions => matches!(item.kind, ItemKind::Action { .. }),
                PaletteMode::Snippets => matches!(item.kind, ItemKind::Snippet { .. }),
                PaletteMode::Components => matches!(item.kind, ItemKind::Component { .. }),
            };
            if !matches_mode {
                continue;
            }
            let matches_query = q.is_empty()
                || item.label.to_lowercase().contains(&q)
                || item.detail.to_lowercase().contains(&q);
            if matches_query {
                self.filtered.push(i);
            }
        }
        self.selected_index = self.selected_index.min(self.filtered.len().saturating_sub(1));
    }

    fn selected_item(&self) -> Option<&PaletteItem> {
        self.filtered.get(self.selected_index).and_then(|&idx| self.items.get(idx))
    }
}

fn category_color(category: animatix::timeline::ActorCategory) -> Color32 {
    match category {
        animatix::timeline::ActorCategory::Shape => Color32::from_rgb(120, 170, 240),
        animatix::timeline::ActorCategory::Container => Color32::from_rgb(160, 220, 140),
        animatix::timeline::ActorCategory::Text => Color32::from_rgb(230, 170, 120),
        animatix::timeline::ActorCategory::Media => Color32::from_rgb(200, 140, 220),
        animatix::timeline::ActorCategory::Plot => Color32::from_rgb(140, 200, 220),
    }
}

fn action_category_color(category: &str) -> Color32 {
    match category {
        "Entrance" => Color32::from_rgb(100, 200, 100),
        "Exit" => Color32::from_rgb(220, 100, 100),
        "Motion" => Color32::from_rgb(100, 160, 220),
        "Effects" => Color32::from_rgb(220, 180, 80),
        "Reveal" => Color32::from_rgb(180, 140, 220),
        "Reorder" => Color32::from_rgb(140, 200, 180),
        _ => Color32::from_rgb(160, 160, 160),
    }
}

impl GuiShell {
    pub(crate) fn insertion_palette_ui(&mut self,
        ui: &mut egui::Ui,
    ) {
        if !self.insertion_palette.open {
            return;
        }

        // Lazily populate items on first open
        if self.insertion_palette.items.is_empty() {
            self.insertion_palette.populate(
                self.document_store.source.document.active_timeline(),
                &self.document_store.source.document.components,
            );
        }

        let screen_rect = ui.ctx().viewport_rect();

        // Dark semi-transparent backdrop
        ui.painter().rect_filled(screen_rect, 0.0, overlay_backdrop());

        // Capture clicks on backdrop to close
        let backdrop_response = ui.interact(
            screen_rect,
            ui.id().with("insertion_palette_backdrop"),
            egui::Sense::click(),
        );
        if backdrop_response.clicked() {
            self.insertion_palette.close();
        }

        // Close on Escape
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.insertion_palette.close();
        }

        // Centered palette
        let palette_w = 420.0;
        let palette_h = 480.0;
        let palette_pos = Pos2::new(
            (screen_rect.width() - palette_w) / 2.0 + screen_rect.min.x,
            (screen_rect.height() - palette_h) / 2.0 + screen_rect.min.y,
        );
        let palette_rect = Rect::from_min_size(palette_pos, Vec2::new(palette_w, palette_h));

        // Background
        ui.painter().rect_filled(palette_rect, RADIUS_XL as u8, BG_BASE);
        ui.painter().rect_stroke(
            palette_rect,
            RADIUS_XL as u8,
            Stroke::new(STROKE_WIDTH, BORDER),
            egui::StrokeKind::Outside,
        );

        // Content
        let mut content = ui.new_child(egui::UiBuilder::new().max_rect(palette_rect));
        content.set_clip_rect(palette_rect);
        content.add_space(SPACE_L);

        // Title + close
        content.horizontal(|ui| {
            ui.label(
                RichText::new("Insert")
                    .size(FONT_SIZE_XL)
                    .color(TEXT_PRIMARY)
                    .strong(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(egui_phosphor::regular::X)
                    .on_hover_text("Close (Esc)")
                    .clicked()
                {
                    self.insertion_palette.close();
                }
            });
        });
        content.add_space(SPACE_M);

        // ── Component parameter form ──────────────────────────────
        if let Some(ref mut form) = self.insertion_palette.param_form {
            let type_name = form.type_name.clone();
            content.label(
                RichText::new(format!("Configure {}", type_name))
                    .size(FONT_SIZE_L)
                    .color(TEXT_PRIMARY)
                    .strong(),
            );
            content.add_space(SPACE_M);
            for (param_name, param_value) in &mut form.params {
                content.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{}:", param_name))
                            .size(FONT_SIZE_S)
                            .color(TEXT_SECONDARY),
                    );
                    ui.add(
                        egui::TextEdit::singleline(param_value)
                            .desired_width(f32::INFINITY),
                    );
                });
                content.add_space(SPACE_S);
            }
            content.add_space(SPACE_M);
            let mut should_insert = false;
            let mut should_back = false;
            content.horizontal(|ui| {
                if ui.button("Insert").clicked() {
                    should_insert = true;
                }
                if ui.button("Back").clicked() {
                    should_back = true;
                }
            });
            if should_insert {
                let props: Vec<animatix_syntax::ast::Property> = form
                    .params
                    .iter()
                    .filter_map(|(name, value)| {
                        if value.trim().is_empty() {
                            return None;
                        }
                        let source = format!("let _ = {}", value.trim());
                        let (stmts, errors) = animatix_syntax::parser::parse_source(&source);
                        let expr = if errors.is_empty() {
                            stmts
                                .and_then(|v| v.into_iter().next())
                                .and_then(|stmt| match stmt {
                                    animatix_syntax::ast::Stmt::LetDecl { value, .. } => Some(value),
                                    _ => None,
                                })
                                .unwrap_or_else(|| animatix_syntax::ast::Expr::Str(value.trim().to_string()))
                        } else {
                            animatix_syntax::ast::Expr::Str(value.trim().to_string())
                        };
                        Some(animatix_syntax::ast::Property {
                            name: name.clone(),
                            value: expr,
                            value_span: None,
                            trailing_comment: None,
                        })
                    })
                    .collect();
                self.insertion_palette.param_form = None;
                let ctx = InsertionContext {
                    current_time_s: self.preview_store.preview.playback.current_time_s(),
                    selected_actors: self.ui_store.selection.selected_actors.clone(),
                    cursor_cell_time_s: self.ui_store.cursor_time_s,
                    selected_container: self.ui_store.selection.selected_actors.iter().next().cloned().filter(|sel| {
                        self.document_store.source.document.active_timeline().is_some_and(|t| {
                            t.get_track(sel).is_some_and(|tr| {
                                matches!(
                                    tr.kind,
                                    animatix::timeline::ActorKindId::Row
                                        | animatix::timeline::ActorKindId::Col
                                        | animatix::timeline::ActorKindId::Grid
                                        | animatix::timeline::ActorKindId::Stack
                                        | animatix::timeline::ActorKindId::Group
                                )
                            })
                        })
                    }),
                };
                let request = InsertionRequest::Primitive {
                    type_name: type_name.clone(),
                    suggested_label: None,
                    props,
                };
                if let Some(edit) = request.into_source_edit(&ctx) {
                    // Snapshot for undo before palette mutation
                    self.document_store.snapshot(Command::InsertionFromPalette);
                    if let Some(ref mut stmts) = self.document_store.source.document.raw_statements {
                        if crate::source_edit::apply_edit(stmts, edit).is_ok() {
                            let (new_source, source_index) = (animatix_syntax::to_source::stmts_to_source(stmts), animatix_syntax::source_index::SourceIndex::build(stmts));
                            self.document_store.source.commit_source(new_source, source_index);
                            self.preview_store.pending_rebuild_at = Some(
                                std::time::Instant::now()
                                    + std::time::Duration::from_millis(self.ui_store.rebuild_debounce_ms),
                            );
                            self.preview_store.preview.status = format!("Inserted {}", type_name);
                        } else {
                            self.preview_store.preview.status = format!("Failed to insert {}", type_name);
                        }
                    }
                }
                self.insertion_palette.close();
                return;
            }
            if should_back {
                self.insertion_palette.param_form = None;
                return;
            }
            return;
        }

        // Query input
        let query_id = ui.id().with("insertion_query");
        let query_response = content.add(
            egui::TextEdit::singleline(&mut self.insertion_palette.query)
                .id(query_id)
                .desired_width(f32::INFINITY)
                .hint_text("Type to filter..."),
        );
        if query_response.changed() {
            self.insertion_palette.rebuild_filter();
        }
        content.add_space(SPACE_M);

        // Mode tabs
        content.horizontal(|ui| {
            let modes = [
                (PaletteMode::Universal, "All"),
                (PaletteMode::Primitives, "Primitives"),
                (PaletteMode::Actions, "Actions"),
                (PaletteMode::Snippets, "Snippets"),
                (PaletteMode::Components, "Components"),
            ];
            for (mode, label) in modes {
                let selected = self.insertion_palette.mode == mode;
                let btn = ui.add(
                    egui::Button::new(
                        RichText::new(label)
                            .size(FONT_SIZE_S)
                            .color(if selected { TEXT_PRIMARY } else { TEXT_SECONDARY }),
                    )
                    .fill(if selected { BG_WIDGET } else { BG_BASE })
                    .corner_radius(RADIUS_M)
                    .stroke(Stroke::new(STROKE_WIDTH, if selected { BORDER_HOVER } else { BORDER })),
                );
                if btn.clicked() {
                    self.insertion_palette.mode = mode;
                    self.insertion_palette.rebuild_filter();
                }
            }
        });
        content.add_space(SPACE_M);

        // Keyboard navigation
        let mut enter_pressed = false;
        ui.input(|i| {
            if i.key_pressed(egui::Key::ArrowDown) {
                let len = self.insertion_palette.filtered.len();
                if len > 0 {
                    self.insertion_palette.selected_index =
                        (self.insertion_palette.selected_index + 1) % len;
                }
            }
            if i.key_pressed(egui::Key::ArrowUp) {
                let len = self.insertion_palette.filtered.len();
                if len > 0 {
                    self.insertion_palette.selected_index =
                        (self.insertion_palette.selected_index + len - 1) % len;
                }
            }
            if i.key_pressed(egui::Key::Tab) {
                let modes = [
                    PaletteMode::Universal,
                    PaletteMode::Primitives,
                    PaletteMode::Actions,
                    PaletteMode::Snippets,
                    PaletteMode::Components,
                ];
                let current = modes.iter().position(|&m| m == self.insertion_palette.mode).unwrap_or(0);
                self.insertion_palette.mode = modes[(current + 1) % modes.len()];
                self.insertion_palette.rebuild_filter();
            }
            if i.key_pressed(egui::Key::Enter) {
                enter_pressed = true;
            }
        });
        if enter_pressed {
            if let Some(item) = self.insertion_palette.selected_item().cloned() {
                self.execute_palette_item(item);
                return;
            }
        }

        // Item list
        let available_h = palette_rect.max.y - content.cursor().min.y - SPACE_L;
        let item_h = 36.0f32;

        let mut clicked_item: Option<PaletteItem> = None;

        egui::ScrollArea::vertical()
            .max_height(available_h)
            .show(&mut content, |ui| {
                for (vis_idx, &item_idx) in self.insertion_palette.filtered.iter().enumerate() {
                    let item = &self.insertion_palette.items[item_idx];
                    let is_selected = vis_idx == self.insertion_palette.selected_index;

                    let row_rect = ui.available_rect_before_wrap();
                    let row_rect = Rect::from_min_size(
                        row_rect.min,
                        Vec2::new(row_rect.width(), item_h),
                    );

                    let row_id = ui.id().with(format!("pal_item_{}", vis_idx));
                    let row_resp = ui.interact(row_rect, row_id, egui::Sense::click());

                    if row_resp.hovered() || is_selected {
                        ui.painter().rect_filled(
                            row_rect,
                            RADIUS_S as u8,
                            if is_selected {
                                ACCENT_BLUE.linear_multiply(0.2)
                            } else {
                                BG_WIDGET
                            },
                        );
                    }

                    if row_resp.clicked() {
                        clicked_item = Some(item.clone());
                    }

                    #[allow(deprecated)]
                    ui.allocate_ui_at_rect(row_rect.shrink(4.0), |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(&item.icon)
                                    .size(FONT_SIZE_M)
                                    .color(item.color),
                            );
                            ui.add_space(SPACE_S);
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new(&item.label)
                                        .size(FONT_SIZE_S)
                                        .color(if is_selected {
                                            TEXT_PRIMARY
                                        } else {
                                            TEXT_SECONDARY
                                        })
                                        .strong(),
                                );
                                if !item.detail.is_empty() {
                                    ui.label(
                                        RichText::new(&item.detail)
                                            .size(FONT_SIZE_XS)
                                            .color(TEXT_MUTED),
                                    );
                                }
                            });
                        });
                    });

                    ui.allocate_rect(row_rect, egui::Sense::hover());
                }
            });

        if let Some(item) = clicked_item {
            self.execute_palette_item(item);
        }
    }

    fn execute_palette_item(
        &mut self,
        item: PaletteItem,
    ) {
        let ctx = InsertionContext {
            current_time_s: self.preview_store.preview.playback.current_time_s(),
            selected_actors: self.ui_store.selection.selected_actors.clone(),
            cursor_cell_time_s: self.ui_store.cursor_time_s,
            selected_container: self.ui_store.selection.selected_actors.iter().next().cloned().filter(|sel| {
                self.document_store.source.document.active_timeline().is_some_and(|t| {
                    t.get_track(sel).is_some_and(|tr| {
                        matches!(
                            tr.kind,
                            animatix::timeline::ActorKindId::Row
                                | animatix::timeline::ActorKindId::Col
                                | animatix::timeline::ActorKindId::Grid
                                | animatix::timeline::ActorKindId::Stack
                                | animatix::timeline::ActorKindId::Group
                        )
                    })
                })
            }),
        };

        let request = match item.kind {
            ItemKind::Primitive { type_name } => InsertionRequest::Primitive {
                type_name,
                suggested_label: None,
                props: vec![],
            },
            ItemKind::Action { verb } => InsertionRequest::Action {
                verb,
                targets: Vec::new(),
            },
            ItemKind::Component { type_name, params } => {
                if !params.is_empty() {
                    self.insertion_palette.param_form = Some(ParamFormState {
                        type_name,
                        params: params.into_iter().map(|(name, default)| (name, default.unwrap_or_default())).collect(),
                    });
                    return;
                }
                InsertionRequest::Primitive {
                    type_name,
                    suggested_label: None,
                    props: vec![],
                }
            }
            ItemKind::Snippet { text } => {
                // Parse snippet into AST fragment and insert via SourceEdit.
                self.insertion_palette.close();
                // Snapshot for undo before palette mutation
                self.document_store.snapshot(Command::InsertionFromPalette);
                if let Some(fragment) = animatix_syntax::parser::parse_snippet(&text) {
                    let time_s = ctx.cursor_cell_time_s.or(Some(ctx.current_time_s));
                    let container = ctx.selected_container.clone();
                    let edit = crate::source_edit::SourceEdit::InsertSnippet {
                        stmts: fragment,
                        time_s,
                        container,
                    };
                    if let Some(ref mut stmts) = self.document_store.source.document.raw_statements {
                        if crate::source_edit::apply_edit(stmts, edit).is_ok() {
                            let (new_source, source_index) = (animatix_syntax::to_source::stmts_to_source(stmts), animatix_syntax::source_index::SourceIndex::build(stmts));
                            self.document_store.source.commit_source(new_source, source_index);
                            self.preview_store.pending_rebuild_at = Some(
                                std::time::Instant::now()
                                    + std::time::Duration::from_millis(self.ui_store.rebuild_debounce_ms),
                            );
                            self.preview_store.preview.status = format!("Inserted snippet: {}", item.label);
                        } else {
                            self.preview_store.preview.status = format!("Failed to insert snippet: {}", item.label);
                        }
                    }
                } else {
                    // Fallback: insert raw text if parsing fails.
                    let source = self.document_store.source.editor.text();
                    let new_source = if source.ends_with('\n') || source.is_empty() {
                        format!("{}{}\n", source, text)
                    } else {
                        format!("{}\n{}\n", source, text)
                    };
                    self.document_store.source.editor.replace_text(new_source.clone());
                    self.document_store.source.document.source_text = new_source;
                    self.document_store.source.document.is_dirty = true;
                    self.preview_store.pending_rebuild_at = Some(
                        std::time::Instant::now()
                            + std::time::Duration::from_millis(self.ui_store.rebuild_debounce_ms),
                    );
                    self.preview_store.preview.status = format!("Inserted snippet (raw): {}", item.label);
                }
                return;
            }
        };

        if let Some(edit) = request.into_source_edit(&ctx) {
            // Snapshot for undo before palette mutation
            self.document_store.snapshot(Command::InsertionFromPalette);
            if let Some(ref mut stmts) = self.document_store.source.document.raw_statements {
                if crate::source_edit::apply_edit(stmts, edit).is_ok() {
                    let (new_source, source_index) = (animatix_syntax::to_source::stmts_to_source(stmts), animatix_syntax::source_index::SourceIndex::build(stmts));
                    self.document_store.source.commit_source(new_source, source_index);
                    self.preview_store.pending_rebuild_at = Some(
                        std::time::Instant::now()
                            + std::time::Duration::from_millis(self.ui_store.rebuild_debounce_ms),
                    );
                    self.preview_store.preview.status =
                        format!("Inserted {}", item.label);
                } else {
                    tracing::warn!("apply_edit failed for insertion: {}", item.label);
                    self.preview_store.preview.status =
                        format!("Failed to insert {}", item.label);
                }
            }
        } else {
            self.preview_store.preview.status =
                "No target selected for action".to_string();
        }

        self.insertion_palette.close();
    }
}
