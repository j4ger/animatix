//! Unified insertion palette — fuzzy-searchable overlay for primitives, actions, and snippets.

use egui::{Color32, Pos2, Rect, RichText, Stroke, Vec2};

use crate::app::GuiShell;
use crate::app::commands::UndoLabel;
use crate::app::design_tokens::semantic::editor::SNIPPET_BLUE;
use crate::app::design_tokens::semantic::{
    accent, border, category, overlay, status, surface, text,
};
use crate::app::design_tokens::spatial::{RADIUS_M, RADIUS_S, RADIUS_XL, STROKE_WIDTH};
use crate::app::design_tokens::typography::TextRole;
use crate::app::insertion::{InsertionContext, InsertionRequest};

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
    Primitive {
        type_name: String,
    },
    Action {
        verb: String,
    },
    #[allow(dead_code)] // Reserved for future snippet insertion type in palette
    Snippet {
        text: String,
    },
    Component {
        type_name: String,
        params: Vec<ParamInfo>,
    },
}

/// Parameter info with type annotation and default value.
#[derive(Debug, Clone)]
pub(crate) struct ParamInfo {
    pub name: String,
    pub param_type: Option<String>,
    pub default_str: Option<String>,
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
    /// (name, optional type name, current value)
    params: Vec<(String, Option<String>, String)>,
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
    pub fn populate(
        &mut self,
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
                color: SNIPPET_BLUE,
                kind: ItemKind::Snippet {
                    text: snippet.insert_text.unwrap_or(snippet.label),
                },
            });
        }

        // Components
        for (name, entry) in components {
            let params_info: Vec<ParamInfo> = entry
                .definition
                .params
                .iter()
                .map(|p| {
                    let default_str =
                        p.default.as_ref().map(animatix_syntax::to_source::expr_to_source);
                    let type_name = p.param_type.as_ref().map(|t| format!("{:?}", t));
                    ParamInfo {
                        name: p.name.clone(),
                        param_type: type_name,
                        default_str,
                    }
                })
                .collect();
            let params_display: Vec<String> = entry
                .definition
                .params
                .iter()
                .map(|p| {
                    p.default
                        .as_ref()
                        .map(|_| p.name.clone())
                        .unwrap_or_else(|| format!("{}?", p.name))
                })
                .collect();
            self.items.push(PaletteItem {
                label: name.clone(),
                detail: if params_display.is_empty() {
                    "Component".into()
                } else {
                    format!("Component — {}", params_display.join(", "))
                },
                icon: egui_phosphor::regular::CUBE.to_string(),
                color: accent::CYAN,
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
        animatix::timeline::ActorCategory::Shape => accent::PRIMARY,
        animatix::timeline::ActorCategory::Container => status::SUCCESS,
        animatix::timeline::ActorCategory::Text => status::WARNING,
        animatix::timeline::ActorCategory::Media => category::ACTION,
        animatix::timeline::ActorCategory::Plot => accent::CYAN,
        animatix::timeline::ActorCategory::Annotation => accent::CYAN,
    }
}

fn action_category_color(category: &str) -> Color32 {
    match category {
        "Entrance" => status::SUCCESS,
        "Exit" => status::ERROR,
        "Motion" => accent::PRIMARY,
        "Effects" => status::WARNING,
        "Reveal" => category::ACTION,
        "Reorder" => status::SUCCESS,
        _ => text::SECONDARY,
    }
}

impl GuiShell {
    pub(crate) fn insertion_palette_ui(&mut self, ui: &mut egui::Ui) {
        let sp = crate::app::design_tokens::spatial::spatial(ui);

        if !self.insertion_palette.open {
            return;
        }

        // Repopulate on every open to reflect component/action changes
        self.insertion_palette.populate(
            self.document_store.source.document.active_timeline(),
            &self.document_store.source.document.components,
        );

        let screen_rect = ui.ctx().viewport_rect();

        // Dark semi-transparent backdrop
        ui.painter().rect_filled(screen_rect, 0.0, overlay::backdrop());

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
        ui.painter().rect_filled(palette_rect, RADIUS_XL as u8, surface::BASE);
        ui.painter().rect_stroke(
            palette_rect,
            RADIUS_XL as u8,
            Stroke::new(STROKE_WIDTH, border::DEFAULT),
            egui::StrokeKind::Outside,
        );

        // Content
        let mut content = ui.new_child(egui::UiBuilder::new().max_rect(palette_rect));
        content.set_clip_rect(palette_rect);
        content.add_space(sp.base.space_4);

        // Title + close
        content.horizontal(|ui| {
            ui.label(
                RichText::new("Insert")
                    .size(TextRole::Heading.size())
                    .color(text::PRIMARY)
                    .strong(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(egui_phosphor::regular::X).on_hover_text("Close (Esc)").clicked() {
                    self.insertion_palette.close();
                }
            });
        });
        content.add_space(sp.base.space_3);

        // ── Component parameter form ──────────────────────────────
        if let Some(ref mut form) = self.insertion_palette.param_form {
            let type_name = form.type_name.clone();
            content.label(
                RichText::new(format!("Configure {}", type_name))
                    .size(TextRole::Title.size())
                    .color(text::PRIMARY)
                    .strong(),
            );
            content.add_space(sp.base.space_3);
            for (param_name, param_type, param_value) in &mut form.params {
                content.horizontal(|ui| {
                    // Parameter name + type hint
                    let label = if let Some(t) = param_type {
                        format!("{}: {}:", param_name, t)
                    } else {
                        format!("{}:", param_name)
                    };
                    ui.label(
                        RichText::new(label).size(TextRole::BodyS.size()).color(text::SECONDARY),
                    );

                    // Type-specific widget
                    match param_type.as_deref() {
                        Some("Num") => {
                            let mut val = param_value.parse::<f64>().unwrap_or(0.0);
                            let resp = ui.add(
                                egui::DragValue::new(&mut val)
                                    .speed(1.0)
                                    .range(f64::NEG_INFINITY..=f64::INFINITY),
                            );
                            if resp.changed() || resp.lost_focus() {
                                *param_value = format_num(val);
                            }
                        },
                        Some("Bool") => {
                            let mut bool_val = param_value == "true" || param_value == "1";
                            if ui.checkbox(&mut bool_val, "").changed() {
                                *param_value = if bool_val {
                                    "true".to_string()
                                } else {
                                    "false".to_string()
                                };
                            }
                        },
                        Some("Vec2") => {
                            // Parse existing value or show field pair
                            let (mut x, mut y) = parse_vec2_value(param_value);
                            ui.add_space(2.0);
                            ui.label("x:");
                            let rx = ui.add(egui::DragValue::new(&mut x).speed(1.0));
                            ui.label("y:");
                            let ry = ui.add(egui::DragValue::new(&mut y).speed(1.0));
                            if rx.changed() || rx.lost_focus() || ry.changed() || ry.lost_focus() {
                                *param_value = format!("({}, {})", format_num(x), format_num(y));
                            }
                        },
                        _ => {
                            // Default: text field
                            ui.add(
                                egui::TextEdit::singleline(param_value)
                                    .desired_width(f32::INFINITY),
                            );
                        },
                    }
                });
                content.add_space(sp.base.space_2);
            }
            content.add_space(sp.base.space_3);
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
                    .filter_map(|(name, _type, value)| {
                        if value.trim().is_empty() {
                            return None;
                        }
                        let source = format!("let _ = {}", value.trim());
                        let (stmts, errors) = animatix_syntax::parser::parse_source(&source);
                        let expr = if errors.is_empty() {
                            stmts
                                .and_then(|v| v.into_iter().next())
                                .and_then(|stmt| match stmt {
                                    animatix_syntax::ast::Stmt::LetDecl { value, .. } => {
                                        Some(value)
                                    },
                                    _ => None,
                                })
                                .unwrap_or_else(|| {
                                    animatix_syntax::ast::Expr::Str(value.trim().to_string())
                                })
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
                    selected_container: self
                        .ui_store
                        .selection
                        .selected_actors
                        .iter()
                        .next()
                        .cloned()
                        .filter(|sel| {
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
                    self.document_store.snapshot(UndoLabel::InsertionFromPalette);
                    if let Some(ref mut stmts) = self.document_store.source.document.raw_statements
                    {
                        if crate::source_edit::apply_edit(stmts, edit).is_ok() {
                            let (new_source, source_index) = (
                                animatix_syntax::to_source::stmts_to_source(stmts),
                                animatix_syntax::source_index::SourceIndex::build(stmts),
                            );
                            self.document_store.commit_source(new_source, source_index);
                            self.preview_store.pending_rebuild_at = Some(
                                std::time::Instant::now()
                                    + std::time::Duration::from_millis(
                                        self.ui_store.rebuild_debounce_ms,
                                    ),
                            );
                            self.preview_store.preview.status = format!("Inserted {}", type_name);
                        } else {
                            self.preview_store.preview.status =
                                format!("Failed to insert {}", type_name);
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
        content.add_space(sp.base.space_3);

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
                    egui::Button::new(RichText::new(label).size(TextRole::BodyS.size()).color(
                        if selected {
                            text::PRIMARY
                        } else {
                            text::SECONDARY
                        },
                    ))
                    .fill(if selected {
                        surface::WIDGET
                    } else {
                        surface::BASE
                    })
                    .corner_radius(RADIUS_M)
                    .stroke(Stroke::new(
                        STROKE_WIDTH,
                        if selected {
                            border::HOVER
                        } else {
                            border::DEFAULT
                        },
                    )),
                );
                if btn.clicked() {
                    self.insertion_palette.mode = mode;
                    self.insertion_palette.rebuild_filter();
                }
            }
        });
        content.add_space(sp.base.space_3);

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
                let current =
                    modes.iter().position(|&m| m == self.insertion_palette.mode).unwrap_or(0);
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
        let available_h = palette_rect.max.y - content.cursor().min.y - sp.base.space_4;
        let item_h = 36.0f32;

        let mut clicked_item: Option<PaletteItem> = None;

        egui::ScrollArea::vertical().max_height(available_h).show(&mut content, |ui| {
            for (vis_idx, &item_idx) in self.insertion_palette.filtered.iter().enumerate() {
                let item = &self.insertion_palette.items[item_idx];
                let is_selected = vis_idx == self.insertion_palette.selected_index;

                let row_rect = ui.available_rect_before_wrap();
                let row_rect =
                    Rect::from_min_size(row_rect.min, Vec2::new(row_rect.width(), item_h));

                let row_id = ui.id().with(format!("pal_item_{}", vis_idx));
                let row_resp = ui.interact(row_rect, row_id, egui::Sense::click());

                if row_resp.hovered() || is_selected {
                    ui.painter().rect_filled(
                        row_rect,
                        RADIUS_S as u8,
                        if is_selected {
                            accent::PRIMARY.linear_multiply(0.2)
                        } else {
                            surface::WIDGET
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
                            RichText::new(&item.icon).size(TextRole::Body.size()).color(item.color),
                        );
                        ui.add_space(sp.base.space_2);
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(&item.label)
                                    .size(TextRole::BodyS.size())
                                    .color(if is_selected {
                                        text::PRIMARY
                                    } else {
                                        text::SECONDARY
                                    })
                                    .strong(),
                            );
                            if !item.detail.is_empty() {
                                ui.label(
                                    RichText::new(&item.detail)
                                        .size(TextRole::Micro.size())
                                        .color(text::MUTED),
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

    fn execute_palette_item(&mut self, item: PaletteItem) {
        let ctx = InsertionContext {
            current_time_s: self.preview_store.preview.playback.current_time_s(),
            selected_actors: self.ui_store.selection.selected_actors.clone(),
            cursor_cell_time_s: self.ui_store.cursor_time_s,
            selected_container: self
                .ui_store
                .selection
                .selected_actors
                .iter()
                .next()
                .cloned()
                .filter(|sel| {
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
                        params: params
                            .into_iter()
                            .map(|p| (p.name, p.param_type, p.default_str.unwrap_or_default()))
                            .collect(),
                    });
                    return;
                }
                InsertionRequest::Primitive {
                    type_name,
                    suggested_label: None,
                    props: vec![],
                }
            },
            ItemKind::Snippet { text } => {
                // Parse snippet into AST fragment and insert via SourceEdit.
                self.insertion_palette.close();
                // Snapshot for undo before palette mutation
                self.document_store.snapshot(UndoLabel::InsertionFromPalette);
                if let Some(fragment) = animatix_syntax::parser::parse_snippet(&text) {
                    let time_s = ctx.cursor_cell_time_s.or(Some(ctx.current_time_s));
                    let container = ctx.selected_container.clone();
                    let edit = crate::source_edit::SourceEdit::InsertSnippet {
                        stmts: fragment,
                        time_s,
                        container,
                    };
                    if let Some(ref mut stmts) = self.document_store.source.document.raw_statements
                    {
                        if crate::source_edit::apply_edit(stmts, edit).is_ok() {
                            let (new_source, source_index) = (
                                animatix_syntax::to_source::stmts_to_source(stmts),
                                animatix_syntax::source_index::SourceIndex::build(stmts),
                            );
                            self.document_store.commit_source(new_source, source_index);
                            self.preview_store.pending_rebuild_at = Some(
                                std::time::Instant::now()
                                    + std::time::Duration::from_millis(
                                        self.ui_store.rebuild_debounce_ms,
                                    ),
                            );
                            self.preview_store.preview.status =
                                format!("Inserted snippet: {}", item.label);
                        } else {
                            self.preview_store.preview.status =
                                format!("Failed to insert snippet: {}", item.label);
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
                    self.preview_store.preview.status =
                        format!("Inserted snippet (raw): {}", item.label);
                }
                return;
            },
        };

        if let Some(edit) = request.into_source_edit(&ctx) {
            // Snapshot for undo before palette mutation
            self.document_store.snapshot(UndoLabel::InsertionFromPalette);
            if let Some(ref mut stmts) = self.document_store.source.document.raw_statements {
                if crate::source_edit::apply_edit(stmts, edit).is_ok() {
                    let (new_source, source_index) = (
                        animatix_syntax::to_source::stmts_to_source(stmts),
                        animatix_syntax::source_index::SourceIndex::build(stmts),
                    );
                    self.document_store.commit_source(new_source, source_index);
                    self.preview_store.pending_rebuild_at = Some(
                        std::time::Instant::now()
                            + std::time::Duration::from_millis(self.ui_store.rebuild_debounce_ms),
                    );
                    self.preview_store.preview.status = format!("Inserted {}", item.label);
                } else {
                    tracing::warn!("apply_edit failed for insertion: {}", item.label);
                    self.preview_store.preview.status = format!("Failed to insert {}", item.label);
                }
            }
        } else {
            self.preview_store.preview.status = "No target selected for action".to_string();
        }

        self.insertion_palette.close();
    }
}

// ── Type-specific widget helpers ─────────────────────────────────────────

/// Format a number without unnecessary trailing zeros.
fn format_num(val: f64) -> String {
    if val.fract() == 0.0 && val.abs() < 1_000_000.0 {
        format!("{}", val as i64)
    } else if (val * 100.0).fract() == 0.0 {
        format!("{:.2}", val)
    } else {
        format!("{}", val)
    }
}

/// Parse a Vec2 value string like "(100, 200)" or "100, 200".
fn parse_vec2_value(s: &str) -> (f64, f64) {
    let s = s.trim().trim_start_matches('(').trim_end_matches(')');
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() >= 2 {
        let x = parts[0].trim().parse::<f64>().unwrap_or(0.0);
        let y = parts[1].trim().parse::<f64>().unwrap_or(0.0);
        (x, y)
    } else {
        (0.0, 0.0)
    }
}
