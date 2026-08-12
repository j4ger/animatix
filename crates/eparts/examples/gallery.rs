//! Gallery example for the `eparts` crate.
//!
//! Run with: `cargo run -p eparts --example gallery`
//!
//! This example doubles as a manual visual-regression check for dark/light theming.
//! Use the theme switcher at the top to toggle between Light, Dark, and Auto,
//! and verify that all widgets render correctly in both themes.

use std::collections::HashSet;

use eframe::CreationContext;
use eframe::egui::{self, Color32};
#[cfg(feature = "theme-json")]
use eparts::ThemeFile;
use eparts::tokens::typography::TextRole;
use eparts::widget::button::Button;
use eparts::widget::feedback::{Alert, AlertLevel, Badge, ProgressBar, Skeleton, Tag};
use eparts::widget::form::{Field, Form};
use eparts::widget::input::{NumberField, TextField};
use eparts::widget::kbd::Kbd;
use eparts::widget::label::Label;
use eparts::widget::layout::{card, group_box, section_header, separator};
use eparts::widget::link::Link;
use eparts::widget::list::{List, SearchableList};
use eparts::widget::popover::Popover;
use eparts::widget::select::Select;
use eparts::widget::slider::Slider;
use eparts::widget::spinner::Spinner;
use eparts::widget::tabs::TabBar;
use eparts::widget::toggle::{Checkbox, Radio, Side, Switch};
use eparts::widget::tooltip::text_tooltip;
use eparts::widget::tree::{Tree, TreeAction, TreeItem};
use eparts::widget::{self};
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
    radio: u8,

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
    accordion_open: Option<usize>,

    // Color
    color: Color32,

    // Tree
    expanded: HashSet<String>,
    tree_selected: Option<String>,
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
            radio: 0,
            text: String::new(),
            number: 50.0,
            slider: 0.5,
            slider_log: 1.0,
            select: None,
            form_name: String::new(),
            form_email: String::new(),
            progress: 0.42,
            tab_selected: 0,
            accordion_open: Some(0),
            color: Color32::from_rgb(100, 150, 200),
            expanded: HashSet::new(),
            tree_selected: None,
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
        if self.json_theme {
            if let Some(file) = &self.theme_file {
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
        }
        let is_dark = self.theme_choice.is_dark(None);
        let theme = self.theme_choice.resolve(None);
        set_theme(ctx, theme);
        ctx.set_visuals(theme.to_visuals(is_dark));
    }
}

// ── eframe App impl ────────────────────────────────────────────────

impl eframe::App for GalleryApp {
    #[allow(deprecated)]
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_theme(ctx);

        // Theme switcher bar
        egui::TopBottomPanel::top("theme_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Theme:");
                if ui
                    .selectable_value(&mut self.theme_choice, AppThemeChoice::Light, "Light")
                    .clicked()
                {
                    self.apply_theme(ctx);
                }
                if ui
                    .selectable_value(&mut self.theme_choice, AppThemeChoice::Dark, "Dark")
                    .clicked()
                {
                    self.apply_theme(ctx);
                }
                if ui
                    .selectable_value(&mut self.theme_choice, AppThemeChoice::Auto, "Auto")
                    .clicked()
                {
                    self.apply_theme(ctx);
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
                    self.apply_theme(ctx);
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
        egui::CentralPanel::default().show(ctx, |ui| {
            self.gallery_body(ui);
        });
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.gallery_body(ui);
    }
}

impl GalleryApp {
    fn gallery_body(&mut self, ui: &mut egui::Ui) {
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
            ui.horizontal(|ui| {
                ui.add(Radio::new(&mut self.radio, 0u8).label("One"));
                ui.add(Radio::new(&mut self.radio, 1u8).label("Two"));
                ui.add(Radio::new(&mut self.radio, 2u8).label("Three"));
            });
            ui.add_space(8.0);

            // ── Labels + Kbd + Link ────────────────────────────────────
            section_header(ui, "📝", "Labels, Kbd, Link", None);
            ui.horizontal(|ui| {
                ui.add(Label::new("Body label"));
                ui.add(Label::new("Required").role(TextRole::BodyS).required(true));
                ui.add(Kbd::new("Ctrl+S"));
                Link::new("Docs").url(Some("https://example.com".into())).show(ui);
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
            ui.horizontal(|ui| {
                ui.label("List:");
                let list_items: Vec<&str> = vec!["Alpha", "Beta", "Gamma", "Delta", "Epsilon"];
                List::new(&list_items).row_height(24.0).show(ui, "gallery_list");
            });
            ui.horizontal(|ui| {
                ui.label("Searchable:");
                let searchable_items: Vec<&str> =
                    vec!["Apple", "Banana", "Cherry", "Date", "Elderberry"];
                SearchableList::new(&searchable_items)
                    .placeholder("Filter fruits...")
                    .show(ui, "gallery_searchable");
            });
            ui.add_space(8.0);

            // ── Form ──────────────────────────────────────────────────
            section_header(ui, "📄", "Form", None);
            Form::new("gallery_form").label_width(100.0).show(ui, |f: &mut Field| {
                f.field("Name", |ui| {
                    TextField::new(&mut self.form_name).placeholder("Your name").show(ui);
                });
                f.required_field("Email", |ui| {
                    TextField::new(&mut self.form_email).placeholder("you@example.com").show(ui);
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
                ui.add(Skeleton::new(egui::vec2(120.0, 16.0)).width(200.0));
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

            // Accordion
            ui.label("Accordion:");
            let accordion = widget::Accordion::new("gallery_accordion", self.accordion_open);
            accordion.section(ui, 0, "Section A", |ui| {
                ui.label("Content for section A");
            });
            accordion.section(ui, 1, "Section B", |ui| {
                ui.label("Content for section B");
            });

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
                ui.id().with("gallery_tooltip"),
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

            // ── Tree ──────────────────────────────────────────────────
            section_header(ui, "🌳", "Tree", None);
            let flat_items = self.build_tree_items();
            let tree_resp = Tree::new(&flat_items).show(ui, "gallery_tree");
            if let Some(action) = tree_resp.action {
                match action {
                    TreeAction::Toggled(id) => {
                        if self.expanded.contains(&id) {
                            self.expanded.remove(&id);
                        } else {
                            self.expanded.insert(id);
                        }
                    },
                    TreeAction::Selected(id) => {
                        self.tree_selected = Some(id);
                    },
                }
            }
            if let Some(ref sel) = self.tree_selected {
                ui.small(format!("Selected: {sel}"));
            }
        });
    }

    fn build_tree_items(&self) -> Vec<TreeItem> {
        let mut items = Vec::new();
        // Root: src
        let src_expanded = self.expanded.contains("src");
        items.push(TreeItem {
            id: "src".into(),
            label: "src".into(),
            depth: 0,
            has_children: true,
            expanded: src_expanded,
        });
        if src_expanded {
            items.push(TreeItem {
                id: "src/lib".into(),
                label: "lib.rs".into(),
                depth: 1,
                has_children: false,
                expanded: false,
            });
            items.push(TreeItem {
                id: "src/main".into(),
                label: "main.rs".into(),
                depth: 1,
                has_children: false,
                expanded: false,
            });
            let widgets_expanded = self.expanded.contains("src/widgets");
            items.push(TreeItem {
                id: "src/widgets".into(),
                label: "widgets".into(),
                depth: 1,
                has_children: true,
                expanded: widgets_expanded,
            });
            if widgets_expanded {
                for name in ["button", "label", "slider", "tree"] {
                    items.push(TreeItem {
                        id: format!("src/widgets/{name}"),
                        label: format!("{name}.rs"),
                        depth: 2,
                        has_children: false,
                        expanded: false,
                    });
                }
            }
        }
        // Root: Cargo.toml
        items.push(TreeItem {
            id: "cargo".into(),
            label: "Cargo.toml".into(),
            depth: 0,
            has_children: false,
            expanded: false,
        });
        items
    }
}

// ── Main ───────────────────────────────────────────────────────────

fn main() -> eframe::Result {
    let options = eframe::NativeOptions::default();
    eframe::run_native("eparts Gallery", options, Box::new(|cc| Ok(Box::new(GalleryApp::new(cc)))))
}
