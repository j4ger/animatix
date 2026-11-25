//! In-app settings route. The workbench state is retained while it is hidden.

use super::*;
use crate::app::i18n::current_language;
use egui::{Align, Frame, Layout, Margin, Rect, Sense, Stroke, Ui, UiBuilder, pos2, vec2};

type CopyPair = (&'static str, &'static str);
const MAX_FORM_WIDTH: f32 = 760.0;
const NAV_WIDTH: f32 = 176.0;
const NAV_GAP: f32 = 36.0;
const SIDE_NAV_MIN: f32 = 760.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum Section {
    #[default]
    Appearance,
    Editor,
    Timeline,
    Recovery,
    Shortcuts,
}

impl Section {
    const ALL: [Self; 5] = [
        Self::Appearance,
        Self::Editor,
        Self::Timeline,
        Self::Recovery,
        Self::Shortcuts,
    ];

    fn title(self, ui: &Ui) -> &'static str {
        let (en, zh) = match self {
            Self::Appearance => ("Appearance & language", "外观与语言"),
            Self::Editor => ("Editing", "编辑"),
            Self::Timeline => ("Preview & timeline", "预览与时间轴"),
            Self::Recovery => ("Save & recovery", "保存与恢复"),
            Self::Shortcuts => ("Keyboard shortcuts", "快捷键"),
        };
        copy(ui, en, zh)
    }
}

pub(super) fn copy(ui: &Ui, en: &'static str, zh: &'static str) -> &'static str {
    match current_language(ui.ctx()) {
        Language::English => en,
        Language::SimplifiedChinese => zh,
    }
}

impl GuiShell {
    pub(crate) fn settings_page_ui(&mut self, ui: &mut Ui) {
        let theme = eparts::theme(ui);
        let section_id = egui::Id::unique("application_settings_section");
        let current = ui.ctx().data(|d| d.get_temp::<Section>(section_id)).unwrap_or_default();
        let mut section = current;
        let mut close = false;
        let recording = self.ui_store.recording_shortcut.is_some();
        let focus_owns_escape = ui.ctx().text_edit_focused()
            || egui::Popup::is_any_open(ui.ctx())
            || eparts::widget::overlay::has_open_overlay(ui.ctx());
        let escape = !focus_owns_escape
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if escape && recording {
            self.ui_store.recording_shortcut = None;
        } else {
            close = escape;
        }

        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(theme.palette.surface.panel)
                    .inner_margin(Margin::symmetric(24, 18)),
            )
            .show(ui, |ui| {
                let back = copy(ui, "Back to workbench", "返回工作台");
                if ui
                    .add(Button::ghost(back).with_icon(egui_phosphor::regular::ARROW_LEFT))
                    .clicked()
                {
                    close = true;
                }
                ui.add_space(12.0);
                ui.separator();
                ui.add_space(28.0);
                let available = ui.available_width();
                let wide = available >= SIDE_NAV_MIN;
                let width = available.min(if wide {
                    NAV_WIDTH + NAV_GAP + MAX_FORM_WIDTH
                } else {
                    MAX_FORM_WIDTH
                });
                let pad = ((available - width) / 2.0).max(0.0);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.add_space(pad);
                    if wide {
                        ui.vertical(|ui| {
                            ui.set_width(NAV_WIDTH);
                            ui.set_max_width(NAV_WIDTH);
                            ui.label(
                                RichText::new(copy(ui, "Settings", "设置"))
                                    .font(TextRole::Heading.font_id()),
                            );
                            ui.add_space(17.0);
                            for choice in Section::ALL {
                                let label = choice.title(ui);
                                let (rect, response) =
                                    ui.allocate_exact_size(vec2(NAV_WIDTH, 32.0), Sense::click());
                                response.widget_info(|| {
                                    egui::WidgetInfo::selected(
                                        egui::Role::ListItem,
                                        true,
                                        section == choice,
                                        label,
                                    )
                                });
                                if section == choice || response.hovered() {
                                    let background = if section == choice {
                                        theme.components.list.selected.bg
                                    } else {
                                        theme.components.list.hover.bg
                                    };
                                    ui.painter().rect_filled(rect, 4.0, background);
                                }
                                ui.painter().text(
                                    pos2(rect.left() + 10.0, rect.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    label,
                                    TextRole::Body.font_id(),
                                    if section == choice {
                                        theme.palette.text.primary
                                    } else {
                                        theme.palette.text.secondary
                                    },
                                );
                                if response.clicked() {
                                    section = choice;
                                }
                            }
                        });
                        ui.add_space(NAV_GAP);
                    }
                    let form_width = if wide {
                        width - NAV_WIDTH - NAV_GAP
                    } else {
                        width
                    };
                    ui.vertical(|ui| {
                        ui.set_width(form_width);
                        ui.set_max_width(form_width);
                        if !wide {
                            let label = section.title(ui);
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new(copy(ui, "Settings", "设置"))
                                        .font(TextRole::Heading.font_id()),
                                );
                                ui.add_space(12.0);
                                egui::ComboBox::from_id_salt("settings_sections_compact")
                                    .selected_text(label)
                                    .show_ui(ui, |ui| {
                                        for choice in Section::ALL {
                                            let label = choice.title(ui);
                                            ui.selectable_value(&mut section, choice, label);
                                        }
                                    });
                            });
                            ui.add_space(20.0);
                        }
                        if section != current {
                            self.ui_store.recording_shortcut = None;
                        }
                        let jump = (section != current).then_some(section);
                        if let Some(active) = self.settings_groups(ui, jump) {
                            section = active;
                        }
                    });
                });
            });
        if section != current {
            ui.ctx().data_mut(|d| d.insert_temp(section_id, section));
        }
        if close {
            self.ui_store.recording_shortcut = None;
            self.ui_store.view.settings_open = false;
            ui.ctx().request_repaint();
        }
    }

    fn settings_groups(&mut self, ui: &mut Ui, jump: Option<Section>) -> Option<Section> {
        let mut active = Section::Appearance;
        egui::ScrollArea::vertical()
            .id_salt("settings_page_rows")
            .auto_shrink([false, false])
            .animated(false)
            .show_viewport(ui, |ui, viewport| {
                let visible_top = ui.min_rect().top() + viewport.top();
                ui.spacing_mut().item_spacing.y = 0.0;
                for choice in Section::ALL {
                    if choice != Section::Appearance {
                        ui.add_space(31.0);
                    }
                    let heading =
                        ui.label(RichText::new(choice.title(ui)).font(TextRole::Heading.font_id()));
                    if jump == Some(choice) {
                        heading.scroll_to_me(Some(Align::Min));
                    }
                    if heading.rect.top() <= visible_top + 24.0 {
                        active = choice;
                    }
                    ui.add_space(10.0);
                    match choice {
                        Section::Appearance => self.settings_appearance_section(ui),
                        Section::Editor => self.settings_editor_section(ui),
                        Section::Timeline => self.settings_preview_section(ui),
                        Section::Recovery => self.settings_autosave_section(ui),
                        Section::Shortcuts => self.settings_shortcuts_section(ui),
                    }
                }
                ui.add_space(18.0);
            });
        jump.or(Some(active))
    }
}

pub(super) fn row(
    ui: &mut Ui,
    id: &str,
    title: CopyPair,
    description: Option<CopyPair>,
    control: impl FnOnce(&mut Ui),
) {
    let title = copy(ui, title.0, title.1);
    let description = description.map(|(en, zh)| copy(ui, en, zh));
    row_labels(ui, id, title, description, control);
}

pub(super) fn row_labels(
    ui: &mut Ui,
    id: &str,
    title: &str,
    description: Option<&str>,
    control: impl FnOnce(&mut Ui),
) {
    let theme = eparts::theme(ui);
    let width = ui.available_width();
    let gap = 18.0;
    let control_width = (width * 0.43).min(200.0);
    let text_width = (width - control_width - gap).max(1.0);
    let title = ui.painter().layout(
        title.to_owned(),
        TextRole::BodyMedium.font_id(),
        theme.palette.text.primary,
        text_width,
    );
    let description = description.map(|text| {
        ui.painter().layout(
            text.to_owned(),
            TextRole::BodyS.font_id(),
            theme.palette.text.secondary,
            text_width,
        )
    });
    let description_gap = if description.is_some() { 3.0 } else { 0.0 };
    let height =
        title.size().y + description_gap + description.as_ref().map_or(0.0, |g| g.size().y);
    let row_height = (height + 22.0).max(44.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, row_height), Sense::hover());
    let text_base = pos2(rect.left(), rect.center().y - height / 2.0);
    let detail_y = text_base.y + title.size().y + description_gap;
    ui.painter().galley(text_base, title, theme.palette.text.primary);
    if let Some(description) = description {
        ui.painter()
            .galley(pos2(rect.left(), detail_y), description, theme.palette.text.secondary);
    }
    ui.push_id(id, |ui| {
        let controls =
            Rect::from_min_max(pos2(rect.right() - control_width, rect.top()), rect.right_bottom());
        let mut ui = ui.new_child(
            UiBuilder::new().max_rect(controls).layout(Layout::right_to_left(Align::Center)),
        );
        control(&mut ui);
    });
    let divider_y = ui.painter().round_to_pixel_center(rect.bottom());
    ui.painter().line_segment(
        [pos2(rect.left(), divider_y), pos2(rect.right(), divider_y)],
        Stroke::new(1.0, theme.palette.border.subtle),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_groups_keep_order_and_localize_only_the_display() {
        let ctx = egui::Context::default();
        crate::fonts::install_fonts(&ctx);
        for (language, expected) in [
            (Language::English, "Preview & timeline"),
            (Language::SimplifiedChinese, "预览与时间轴"),
        ] {
            crate::app::i18n::set_language(&ctx, language);
            let mut output = ctx.run_ui(Default::default(), |ui| {
                assert_eq!(Section::ALL[2], Section::Timeline);
                assert_eq!(Section::Timeline.title(ui), expected);
            });
            output.textures_delta.clear();
        }
    }

    #[test]
    fn settings_rows_with_a_wrapped_description_keep_the_same_trailing_edge() {
        let ctx = egui::Context::default();
        crate::fonts::install_fonts(&ctx);
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(360.0, 250.0))),
                ..Default::default()
            },
            |ui| {
                let mut first = Rect::NOTHING;
                let mut second = Rect::NOTHING;
                row_labels(ui, "short", "Theme", None, |ui| {
                    first = ui.allocate_exact_size(vec2(70.0, 20.0), Sense::hover()).0;
                });
                row_labels(ui, "long", "Grid size", Some("This setting changes the distance between lines in the preview grid. It does not alter the scene."), |ui| {
                    second = ui.allocate_exact_size(vec2(34.0, 24.0), Sense::hover()).0;
                });
                assert!((first.right() - second.right()).abs() < 0.5);
                assert!(second.top() > first.bottom());
            },
        );
        output.textures_delta.clear();
    }
}
