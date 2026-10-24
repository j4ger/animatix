use super::*;

impl GuiShell {
    /// The selected actor, when it is a `Filter` compositing scope — the only
    /// container that can own effect stages.
    fn selected_effect_scope(&self) -> Option<String> {
        self.ui_store
            .selection
            .selected_actors
            .iter()
            .next()
            .cloned()
            .filter(|selected| {
                self.document_store.source.document.active_timeline().is_some_and(|timeline| {
                    timeline
                        .get_track(selected)
                        .is_some_and(|track| timeline.world().caps(track.id).is_effect_scope())
                })
            })
    }

    pub(crate) fn insertion_palette_ui(&mut self, ui: &mut egui::Ui) {
        let theme = eparts::theme(ui);
        let sp = crate::app::design_tokens::spatial::spatial(ui);

        if !self.insertion_palette.open {
            return;
        }

        // Repopulate on every open to reflect component/action changes
        self.insertion_palette.populate(
            self.document_store.source.document.active_timeline(),
            &self.document_store.source.document.components,
            theme,
        );

        let screen_rect = ui.ctx().viewport_rect();

        // Dark semi-transparent backdrop
        ui.painter().rect_filled(screen_rect, 0.0, theme.palette.overlay.backdrop);

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

        // Elevation overlay shadow & background
        ui.painter().add(theme.elevation.overlay.as_shape(palette_rect, RADIUS_XL));
        ui.painter()
            .rect_filled(palette_rect, RADIUS_XL as u8, theme.palette.surface.base);
        ui.painter().rect_stroke(
            palette_rect,
            RADIUS_XL as u8,
            Stroke::new(STROKE_WIDTH, theme.palette.border.default),
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
                    .color(theme.palette.text.primary)
                    .strong(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let close_btn = ui.add(Button::ghost("").with_icon(egui_phosphor::regular::X));
                text_tooltip(ui, close_btn.id.with("close_tip"), &close_btn, "Close (Esc)");
                if close_btn.clicked() {
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
                    .color(theme.palette.text.primary)
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
                        RichText::new(label)
                            .size(TextRole::BodyS.size())
                            .color(theme.palette.text.secondary),
                    );

                    // Type-specific widget
                    match param_type.as_deref() {
                        Some("Num") => {
                            let mut val = param_value.parse::<f64>().unwrap_or(0.0);
                            let resp =
                                NumberField::new(&mut val).speed(1.0).desired_width(120.0).show(ui);
                            if resp.changed() || resp.lost_focus() {
                                *param_value = format_num(val);
                            }
                        },
                        Some("Bool") => {
                            let mut bool_val = param_value == "true" || param_value == "1";
                            if ui.add(Checkbox::new(&mut bool_val)).changed() {
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
                            let rx = NumberField::new(&mut x)
                                .prefix("X")
                                .speed(1.0)
                                .desired_width(70.0)
                                .show(ui);
                            let ry = NumberField::new(&mut y)
                                .prefix("Y")
                                .speed(1.0)
                                .desired_width(70.0)
                                .show(ui);
                            if rx.changed() || rx.lost_focus() || ry.changed() || ry.lost_focus() {
                                *param_value = format!("({}, {})", format_num(x), format_num(y));
                            }
                        },
                        _ => {
                            // Default: text field
                            let field_w = ui.available_width().max(60.0);
                            TextField::new(param_value).desired_width(field_w).show(ui);
                        },
                    }
                });
                content.add_space(sp.base.space_2);
            }
            content.add_space(sp.base.space_3);
            let mut should_insert = false;
            let mut should_back = false;
            content.horizontal(|ui| {
                if ui.add(Button::primary("Insert")).clicked() {
                    should_insert = true;
                }
                if ui.add(Button::secondary("Back")).clicked() {
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
                                    crate::app::actions::track_is_nestable_container(t, tr)
                                })
                            })
                        }),
                };
                let template = crate::app::primitives::ActorTemplate::with_suggested_label(
                    &type_name,
                    None,
                    None,
                    self.document_store.source.document.scene_dimensions,
                    self.document_store.source.document.active_timeline(),
                    props,
                );
                let request = InsertionRequest::Primitive {
                    type_name: type_name.clone(),
                    suggested_label: Some(template.label),
                    props: template.props,
                };
                if let Some(edit) = request.into_source_edit(&ctx) {
                    // Snapshot for undo before palette mutation
                    let ui_before = self.ui_store.snapshot_with_preview(&self.preview_store);
                    self.document_store.snapshot(UndoLabel::InsertionFromPalette, ui_before);
                    if let Some(ref mut stmts) = self.document_store.source.document.raw_statements
                    {
                        if crate::source_edit::apply_edit(stmts, edit).is_ok() {
                            let (new_source, source_index) = (
                                animatix_syntax::to_source::stmts_to_source(stmts),
                                animatix_syntax::source_index::SourceIndex::build(stmts),
                            );
                            let ui_after = self.ui_store.snapshot_with_preview(&self.preview_store);
                            self.document_store.commit_source(new_source, source_index, ui_after);
                            self.preview_store.pending_rebuild_at = Some(
                                std::time::Instant::now()
                                    + std::time::Duration::from_millis(
                                        self.ui_store.rebuild_debounce_ms,
                                    ),
                            );
                            self.preview_store.preview.status = format!("Inserted {}", type_name);
                        } else {
                            self.document_store.abort_snapshot();
                            self.preview_store.preview.status =
                                format!("Failed to insert {}", type_name);
                        }
                    } else {
                        self.document_store.abort_snapshot();
                        self.preview_store.preview.status =
                            format!("Failed to insert {}", type_name);
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
        let query_response = TextField::new(&mut self.insertion_palette.query)
            .placeholder("Type to filter…")
            .cleanable(true)
            .desired_width(content.available_width())
            .show(&mut content);
        if query_response.changed {
            self.insertion_palette.rebuild_filter();
        }
        content.add_space(sp.base.space_3);

        // Mode tabs
        let modes = [
            PaletteMode::Universal,
            PaletteMode::Primitives,
            PaletteMode::Effects,
            PaletteMode::Actions,
            PaletteMode::Snippets,
            PaletteMode::Components,
        ];
        let labels = [
            "All",
            "Primitives",
            "Effects",
            "Actions",
            "Snippets",
            "Components",
        ];
        let mut selected =
            modes.iter().position(|&mode| mode == self.insertion_palette.mode).unwrap_or(0);
        TabBar::new("insertion_mode_tabs", &mut selected, &labels).show(&mut content);
        if let Some(mode) = modes.get(selected).copied() {
            if mode != self.insertion_palette.mode {
                self.insertion_palette.mode = mode;
                self.insertion_palette.rebuild_filter();
            }
        }
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
                            theme.palette.accent.primary.linear_multiply(0.2)
                        } else {
                            theme.palette.surface.widget
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
                                        theme.palette.text.primary
                                    } else {
                                        theme.palette.text.secondary
                                    })
                                    .strong(),
                            );
                            if !item.detail.is_empty() {
                                ui.label(
                                    RichText::new(&item.detail)
                                        .size(TextRole::Micro.size())
                                        .color(theme.palette.text.muted),
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
                            crate::app::actions::track_is_nestable_container(t, tr)
                        })
                    })
                }),
        };

        let request = match item.kind {
            ItemKind::Primitive { type_name } => {
                let template = crate::app::primitives::ActorTemplate::for_primitive(
                    &type_name,
                    None,
                    self.document_store.source.document.scene_dimensions,
                    self.document_store.source.document.active_timeline(),
                );
                InsertionRequest::Primitive {
                    type_name,
                    suggested_label: Some(template.label),
                    props: template.props,
                }
            },
            ItemKind::Effect { type_name } => {
                let Some(scope) = self.selected_effect_scope() else {
                    self.preview_store.preview.status =
                        "Select a Filter scope to add an effect".to_string();
                    return;
                };
                let label = {
                    let timeline = self.document_store.source.document.active_timeline();
                    crate::app::utils::labels::unique_effect_label(timeline, &scope, &type_name)
                };
                InsertionRequest::Effect {
                    type_name,
                    scope,
                    suggested_label: Some(label),
                }
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
                let template = crate::app::primitives::ActorTemplate::for_primitive(
                    &type_name,
                    None,
                    self.document_store.source.document.scene_dimensions,
                    self.document_store.source.document.active_timeline(),
                );
                InsertionRequest::Primitive {
                    type_name,
                    suggested_label: Some(template.label),
                    props: template.props,
                }
            },
            ItemKind::Snippet { text } => {
                // Parse snippet into AST fragment and insert via SourceEdit.
                self.insertion_palette.close();
                // Snapshot for undo before palette mutation
                let ui_before = self.ui_store.snapshot_with_preview(&self.preview_store);
                self.document_store.snapshot(UndoLabel::InsertionFromPalette, ui_before);
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
                            let ui_after = self.ui_store.snapshot_with_preview(&self.preview_store);
                            self.document_store.commit_source(new_source, source_index, ui_after);
                            self.preview_store.pending_rebuild_at = Some(
                                std::time::Instant::now()
                                    + std::time::Duration::from_millis(
                                        self.ui_store.rebuild_debounce_ms,
                                    ),
                            );
                            self.preview_store.preview.status =
                                format!("Inserted snippet: {}", item.label);
                        } else {
                            self.document_store.abort_snapshot();
                            self.preview_store.preview.status =
                                format!("Failed to insert snippet: {}", item.label);
                        }
                    } else {
                        self.document_store.abort_snapshot();
                        self.preview_store.preview.status =
                            format!("Failed to insert snippet: {}", item.label);
                    }
                } else {
                    // Fallback: insert raw text if parsing fails.
                    let source = self.document_store.source.editor.text();
                    let new_source = if source.ends_with('\n') || source.is_empty() {
                        format!("{}{}\n", source, text)
                    } else {
                        format!("{}\n{}\n", source, text)
                    };
                    let ui_after = self.ui_store.snapshot_with_preview(&self.preview_store);
                    self.document_store.replace_text_with_ui(new_source, ui_after);
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
            let ui_before = self.ui_store.snapshot_with_preview(&self.preview_store);
            self.document_store.snapshot(UndoLabel::InsertionFromPalette, ui_before);
            if let Some(ref mut stmts) = self.document_store.source.document.raw_statements {
                if crate::source_edit::apply_edit(stmts, edit).is_ok() {
                    let (new_source, source_index) = (
                        animatix_syntax::to_source::stmts_to_source(stmts),
                        animatix_syntax::source_index::SourceIndex::build(stmts),
                    );
                    let ui_after = self.ui_store.snapshot_with_preview(&self.preview_store);
                    self.document_store.commit_source(new_source, source_index, ui_after);
                    self.preview_store.pending_rebuild_at = Some(
                        std::time::Instant::now()
                            + std::time::Duration::from_millis(self.ui_store.rebuild_debounce_ms),
                    );
                    self.preview_store.preview.status = format!("Inserted {}", item.label);
                } else {
                    tracing::warn!("apply_edit failed for insertion: {}", item.label);
                    self.document_store.abort_snapshot();
                    self.preview_store.preview.status = format!("Failed to insert {}", item.label);
                }
            } else {
                self.document_store.abort_snapshot();
                self.preview_store.preview.status = format!("Failed to insert {}", item.label);
            }
        } else {
            self.preview_store.preview.status = "No target selected for action".to_string();
        }

        self.insertion_palette.close();
    }
}
