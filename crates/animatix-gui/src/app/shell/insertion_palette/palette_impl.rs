use super::*;

impl InsertionPalette {
    /// Populate items from the core registries.
    pub fn populate(
        &mut self,
        timeline: Option<&animatix::timeline::Timeline>,
        components: &std::collections::HashMap<String, animatix_syntax::module::ComponentEntry>,
        theme: eparts::Theme,
    ) {
        self.param_form = None;
        self.items.clear();

        // Primitives
        let registry = timeline
            .map(animatix::timeline::Timeline::primitive_registry_snapshot)
            .unwrap_or_else(|| std::sync::Arc::new(animatix::primitives::PrimitiveRegistry::new()));
        for (prim, info) in registry.iter_with_info() {
            self.items.push(PaletteItem {
                label: info.display_name.to_string(),
                detail: prim.type_name().to_string(),
                icon: info.icon_id.to_string(),
                color: category_color(info.category, theme),
                kind: ItemKind::Primitive {
                    type_name: prim.type_name().to_string(),
                },
            });
        }

        // Effects (post-processing stages owned by a Filter scope)
        for effect in animatix::timeline::effects::EFFECTS {
            let params: Vec<&str> = effect.params().iter().map(|spec| spec.name.as_ref()).collect();
            self.items.push(PaletteItem {
                label: effect.display_name().to_string(),
                detail: if params.is_empty() {
                    format!("Effect — {}", effect.type_name())
                } else {
                    format!("Effect — {} ({})", effect.type_name(), params.join(", "))
                },
                icon: egui_phosphor::regular::MAGIC_WAND.to_string(),
                color: theme.palette.status.warning,
                kind: ItemKind::Effect {
                    type_name: effect.type_name().to_string(),
                },
            });
        }

        // Actions
        let mut action_signatures = animatix::timeline::actions::get_action_signatures();
        if let Some(timeline) = timeline {
            for signature in timeline.extension_action_signatures() {
                if !action_signatures.iter().any(|existing| existing.name == signature.name) {
                    action_signatures.push(signature);
                }
            }
        }
        for sig in action_signatures {
            self.items.push(PaletteItem {
                label: sig.name.clone(),
                detail: sig.description.clone(),
                icon: "⚡".to_string(),
                color: action_category_color(&sig.category, theme),
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
                color: theme.palette.accent.cyan,
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

    pub(super) fn rebuild_filter(&mut self) {
        self.filtered.clear();
        let q = self.query.to_lowercase();
        for (i, item) in self.items.iter().enumerate() {
            let matches_mode = match self.mode {
                PaletteMode::Universal => true,
                PaletteMode::Primitives => matches!(item.kind, ItemKind::Primitive { .. }),
                PaletteMode::Effects => matches!(item.kind, ItemKind::Effect { .. }),
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

    pub(super) fn selected_item(&self) -> Option<&PaletteItem> {
        self.filtered.get(self.selected_index).and_then(|&idx| self.items.get(idx))
    }
}
