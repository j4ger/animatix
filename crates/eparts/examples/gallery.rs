//! Gallery example for the `eparts` crate.
//!
//! Run with: `cargo run -p eparts --example gallery`
//!
//! This example doubles as a manual visual-regression check for dark/light theming.
//! Use the theme switcher at the top to toggle between Light, Dark, and Auto,
//! and verify that all widgets render correctly in both themes.

use eframe::CreationContext;
use eframe::egui::{self, Color32};
#[cfg(feature = "theme-json")]
use eparts::ThemeFile;
use eparts::tokens::typography::TextRole;
use eparts::widget;
use eparts::widget::button::Button;
use eparts::widget::feedback::{Alert, AlertLevel, Badge, ProgressBar, Tag};
use eparts::widget::form::{Field, Form};
use eparts::widget::input::{NumberField, TextField};
use eparts::widget::kbd::Kbd;
use eparts::widget::label::Label;
use eparts::widget::layout::{card, group_box, section_header, separator};
use eparts::widget::popover::Popover;
use eparts::widget::select::Select;
use eparts::widget::slider::Slider;
use eparts::widget::spinner::Spinner;
use eparts::widget::tabs::TabBar;
use eparts::widget::toggle::{Checkbox, Side, Switch};
use eparts::widget::tooltip::text_tooltip;
use eparts::{AppThemeChoice, Theme, set_theme};

// ── Application state ──────────────────────────────────────────────

struct GalleryApp {
    // Theme
    theme_choice: AppThemeChoice,
    /// Optional JSON theme loaded from `examples/gallery.theme.json`.
    #[cfg(feature = "theme-json")]
    theme_file: Option<ThemeFile>,
    #[cfg(feature = "theme-json")]
    json_theme: bool,

    // Buttons
    loading: bool,
    disabled: bool,
    checkbox: bool,
    switch: bool,

    // Inputs
    text: String,
    number: f64,
    slider: f64,
    slider_log: f64,

    // Selection
    select: Option<usize>,

    // Form
    form_name: String,
    form_email: String,

    // Feedback
    progress: f32,

    // Layout
    tab_selected: usize,

    // Color
    color: Color32,
}

impl Default for GalleryApp {
    fn default() -> Self {
        Self {
            theme_choice: AppThemeChoice::Dark,
            #[cfg(feature = "theme-json")]
            theme_file: None,
            #[cfg(feature = "theme-json")]
            json_theme: false,
            loading: false,
            disabled: false,
            checkbox: true,
            switch: false,
            text: String::new(),
            number: 50.0,
            slider: 0.5,
            slider_log: 1.0,
            select: None,
            form_name: String::new(),
            form_email: String::new(),
            progress: 0.42,
            tab_selected: 0,
            color: Color32::from_rgb(100, 150, 200),
        }
    }
}

impl GalleryApp {
    fn new(cc: &CreationContext) -> Self {
        let app = Self::default();
        set_theme(&cc.egui_ctx, Theme::dark());
        app
    }

    fn apply_theme(&self, ctx: &egui::Context) {
        #[cfg(feature = "theme-json")]
        if self.json_theme
            && let Some(file) = &self.theme_file
        {
            let is_dark = self.theme_choice.is_dark(None);
            let theme = if is_dark {
                file.dark_theme()
            } else {
                file.light_theme()
            };
            set_theme(ctx, theme);
            ctx.set_visuals(theme.to_visuals(is_dark));
            return;
        }
        let is_dark = self.theme_choice.is_dark(None);
        let theme = self.theme_choice.resolve(None);
        set_theme(ctx, theme);
        ctx.set_visuals(theme.to_visuals(is_dark));
    }
}

// ── eframe App impl ────────────────────────────────────────────────

impl eframe::App for GalleryApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.apply_theme(ui.ctx());

        // Theme switcher bar
        egui::Panel::top("theme_bar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Theme:");
                if ui
                    .selectable_value(&mut self.theme_choice, AppThemeChoice::Light, "Light")
                    .clicked()
                {
                    self.apply_theme(ui.ctx());
                }
                if ui
                    .selectable_value(&mut self.theme_choice, AppThemeChoice::Dark, "Dark")
                    .clicked()
                {
                    self.apply_theme(ui.ctx());
                }
                if ui
                    .selectable_value(&mut self.theme_choice, AppThemeChoice::Auto, "Auto")
                    .clicked()
                {
                    self.apply_theme(ui.ctx());
                }
                ui.separator();
                #[cfg(feature = "theme-json")]
                if ui
                    .selectable_label(self.json_theme, "JSON theme")
                    .on_hover_text("Loads gallery.theme.json")
                    .clicked()
                {
                    self.json_theme = !self.json_theme;
                    if self.json_theme && self.theme_file.is_none() {
                        self.theme_file =
                            ThemeFile::load(std::path::Path::new("examples/gallery.theme.json"))
                                .ok();
                        if self.theme_file.is_none() {
                            self.json_theme = false;
                            ui.label("Missing examples/gallery.theme.json");
                        }
                    }
                    self.apply_theme(ui.ctx());
                }
                ui.separator();
                ui.label(format!(
                    "Effective: {}",
                    if self.theme_choice.is_dark(None) {
                        "Dark"
                    } else {
                        "Light"
                    }
                ));
            });
        });

        // Main content
        egui::CentralPanel::default().show(ui, |ui| {
            self.gallery_body(ui);
        });
    }
}

impl GalleryApp {
    fn gallery_body(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new().inner_margin(egui::Margin::same(8)).show(ui, |ui| {
            ui.vertical(|ui| {
                // ── Buttons ──────────────────────────────────────────────
                section_header(ui, "🔘", "Buttons", None);
                ui.horizontal(|ui| {
                    let _ = ui.add(Button::ghost("Ghost"));
                    let _ = ui.add(Button::primary("Primary"));
                    let play_icon = widget::button::play_pause_icon(true);
                    let _ = ui.add(Button::icon(play_icon).with_tooltip("Play/Pause"));
                    if self.loading {
                        ui.add(Button::icon("⟳").loading(true).with_tooltip("Loading..."));
                    }
                });
                ui.horizontal(|ui| {
                    ui.add(Button::ghost("Disabled").active(false));
                    ui.checkbox(&mut self.disabled, "Disabled");
                });
                ui.add_space(8.0);

                // ── Toggles ────────────────────────────────────────────────
                section_header(ui, "\u{2611}", "Toggles", None);
                ui.add(Checkbox::new(&mut self.checkbox).label("Checkbox"));
                ui.add(Switch::new(&mut self.switch).label("Switch").label_side(Side::Right));
                ui.add_space(8.0);

                // ── Labels + Kbd ──────────────────────────────────────────
                section_header(ui, "📝", "Labels, Kbd", None);
                ui.horizontal(|ui| {
                    ui.add(Label::new("Body label"));
                    ui.add(Label::new("Required").role(TextRole::BodyS).required(true));
                    ui.add(Kbd::new("Ctrl+S"));
                });
                ui.add_space(8.0);

                // ── Inputs ────────────────────────────────────────────────
                section_header(ui, "✏️", "Inputs", None);
                ui.horizontal(|ui| {
                    TextField::new(&mut self.text)
                        .placeholder("Type something...")
                        .cleanable(true)
                        .show(ui);
                });
                ui.horizontal(|ui| {
                    NumberField::new(&mut self.number).range(0.0..=100.0).suffix(" %").show(ui);
                    ui.add(Slider::new(&mut self.slider, 0.0..=1.0).show_value(true));
                    ui.add(
                        Slider::new(&mut self.slider_log, 0.1..=10.0)
                            .logarithmic(true)
                            .suffix("x")
                            .show_value(true),
                    );
                });
                ui.add_space(8.0);

                // ── Selection ─────────────────────────────────────────────
                section_header(ui, "📋", "Selection", None);
                ui.horizontal(|ui| {
                    ui.label("Select:");
                    ui.add(
                        Select::new("gallery_select", &mut self.select, &["Red", "Green", "Blue"])
                            .placeholder("Pick a color"),
                    );
                });
                ui.add_space(8.0);

                // ── Form ──────────────────────────────────────────────────
                section_header(ui, "📄", "Form", None);
                Form::new("gallery_form").label_width(100.0).show(ui, |f: &mut Field| {
                    f.field("Name", |ui| {
                        TextField::new(&mut self.form_name).placeholder("Your name").show(ui);
                    });
                    f.required_field("Email", |ui| {
                        TextField::new(&mut self.form_email)
                            .placeholder("you@example.com")
                            .show(ui);
                    });
                });
                ui.add_space(8.0);

                // ── Feedback ──────────────────────────────────────────────
                section_header(ui, "💬", "Feedback", None);
                ui.horizontal(|ui| {
                    ui.add(Badge::new("3"));
                    ui.add(Tag::new("filter"));
                    ui.add(Tag::new("removable").removable(true));
                });
                ui.horizontal(|ui| {
                    ui.add(Alert::new("Operation succeeded!", AlertLevel::Success));
                });
                ui.horizontal(|ui| {
                    ui.add(Alert::new("Warning: check your input", AlertLevel::Warning));
                });
                ui.horizontal(|ui| {
                    ui.add(
                        Alert::new("Error: something went wrong", AlertLevel::Error)
                            .title(Some("Error".into())),
                    );
                });
                ui.add(ProgressBar::new(self.progress).show_percentage(true));
                ui.horizontal(|ui| {
                    ui.add(Spinner::new());
                });
                ui.add_space(8.0);

                // ── Layout ────────────────────────────────────────────────
                section_header(ui, "📐", "Layout", None);
                separator(ui);
                ui.label("Above and below are separators");
                separator(ui);
                ui.add_space(4.0);

                card(ui, |ui| {
                    ui.label("Inside a card");
                    ui.small("Cards have surface background, rounded corners, and shadow.");
                });
                ui.add_space(4.0);

                group_box(ui, "Group Box", |ui| {
                    ui.label("Inside a group box");
                    ui.small("Titled bordered container.");
                });
                ui.add_space(4.0);

                // TabBar
                ui.add_space(4.0);
                ui.label("TabBar:");
                TabBar::new("gallery_tabs", &mut self.tab_selected, &["Files", "Edit", "View"])
                    .show(ui);
                ui.add_space(8.0);

                // ── Overlays ──────────────────────────────────────────────
                section_header(ui, "🪟", "Overlays", None);
                Popover::new("gallery_popover").below().max_width(200.0).show(
                    ui,
                    &ui.response(),
                    |ui| {
                        ui.label("Popover content!");
                        ui.small("Click outside to dismiss.");
                    },
                );
                ui.add_space(4.0);
                let tooltip_trigger = ui.button("Hover for tooltip");
                text_tooltip(
                    ui,
                    ui.scope_id().with("gallery_tooltip"),
                    &tooltip_trigger,
                    "Tooltip text here!",
                );
                ui.add_space(8.0);

                // ── ColorPicker ───────────────────────────────────────────
                section_header(ui, "🎨", "Color Picker", None);
                let picker_resp = widget::ColorPicker::new("gallery_color", &mut self.color)
                    .alpha(true)
                    .swatches(&[
                        Color32::RED,
                        Color32::GREEN,
                        Color32::BLUE,
                        Color32::YELLOW,
                        Color32::from_rgb(255, 128, 0),
                    ])
                    .show(ui);
                if picker_resp.changed {
                    // color updated
                }
                ui.add_space(8.0);
            });
        });
    }
}

// ── Main ───────────────────────────────────────────────────────────

fn main() -> eframe::Result {
    let options = eframe::NativeOptions::default();
    eframe::run_native("eparts Gallery", options, Box::new(|cc| Ok(Box::new(GalleryApp::new(cc)))))
}
