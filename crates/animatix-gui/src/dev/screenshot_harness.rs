//! Bounded widget screenshot rendering for visual review.
//!
//! The harness renders isolated eparts surfaces into an eframe window, requests
//! one screenshot, saves it as PNG, and closes the window. It intentionally
//! does not launch an interactive app session.

use egui::Vec2;

use crate::app::components::{button, layout, row};
use crate::app::design_tokens::typography::TextRole;

/// Registry of screenshot-able widgets.
pub const WIDGET_REGISTRY: &[(&str, &str)] = &[
    ("overview", "Combined theme-aware surface overview"),
    ("buttons", "Primary, ghost, danger, and icon buttons"),
    ("rows", "Interactive row states"),
    ("card", "Card container"),
    ("section-headers", "Section header variants"),
    ("field", "Input field frames"),
    ("empty-state", "Empty state placeholder"),
    ("palette", "Theme color palette swatches"),
];

/// Render a widget by name into the given UI.
pub fn render_widget(ui: &mut egui::Ui, name: &str, theme: eparts::Theme) {
    ui.set_width(ui.available_width());
    ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);

    match name {
        "overview" => render_overview(ui, theme),
        "buttons" => render_buttons(ui),
        "rows" => render_rows(ui),
        "card" => render_card(ui),
        "section-headers" => render_section_headers(ui),
        "field" => render_field(ui),
        "empty-state" => render_empty_state(ui),
        "palette" => render_palette(ui, theme),
        _ => render_unknown_widget(ui, theme, name),
    }
}

fn render_overview(ui: &mut egui::Ui, theme: eparts::Theme) {
    ui.add_space(8.0);
    layout::card(ui, |ui| {
        layout::section_header(ui, egui_phosphor::regular::PUZZLE_PIECE, "Overview", None);
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Theme-aware custom surfaces")
                .size(TextRole::Title.size())
                .color(theme.text.primary)
                .strong(),
        );
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("Buttons, rows, fields, and cards share the runtime theme.")
                .size(TextRole::BodyS.size())
                .color(theme.text.secondary),
        );
        ui.add_space(8.0);
        render_buttons(ui);
        ui.add_space(6.0);
        render_rows(ui);
        ui.add_space(6.0);
        render_field(ui);
    });
}

fn render_buttons(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [120.0, 32.0],
            button::Button::primary("Create").with_icon(egui_phosphor::regular::PLUS),
        );
        ui.add_sized(
            [100.0, 32.0],
            button::Button::ghost("Inspect").with_icon(egui_phosphor::regular::MAGNIFYING_GLASS),
        );
        ui.add_sized(
            [100.0, 32.0],
            button::Button::danger("Delete").with_icon(egui_phosphor::regular::TRASH),
        );
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add(button::Button::icon(egui_phosphor::regular::PLAY).with_tooltip("Play"));
        ui.add(button::Button::icon(egui_phosphor::regular::PAUSE).with_tooltip("Pause"));
        ui.add(button::Button::icon(egui_phosphor::regular::EYE).with_tooltip("Visibility"));
    });
}

fn render_rows(ui: &mut egui::Ui) {
    let row1 = row::Row::new("Actor 1")
        .icon(Some(egui_phosphor::regular::SQUARE))
        .has_children(true)
        .expanded(true);
    row1.show(ui, ui.id().with("harness_row_1"));

    let row2 = row::Row::new("Actor 2")
        .icon(Some(egui_phosphor::regular::CIRCLE))
        .selected(true);
    row2.show(ui, ui.id().with("harness_row_2"));

    let row3 = row::Row::new("Actor 3")
        .icon(Some(egui_phosphor::regular::TRIANGLE))
        .label_color(eparts::theme(ui).text.muted);
    row3.show(ui, ui.id().with("harness_row_3"));
}

fn render_card(ui: &mut egui::Ui) {
    layout::card(ui, |ui| {
        let theme = eparts::theme(ui);
        ui.label(
            egui::RichText::new("Card Content")
                .size(TextRole::Title.size())
                .color(theme.text.primary),
        );
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("This is inside an eparts card.")
                .size(TextRole::BodyS.size())
                .color(theme.text.secondary),
        );
    });
}

fn render_section_headers(ui: &mut egui::Ui) {
    layout::section_header(ui, egui_phosphor::regular::WRENCH, "Properties", Some(3));
    ui.add_space(2.0);
    layout::section_header(ui, egui_phosphor::regular::KEY, "Keyframes", None);
}

fn render_field(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let mut val = 42.0;
        eparts::NumberField::new(&mut val).suffix(" px").show(ui);
        ui.add_space(8.0);
        let mut text = "hello".to_string();
        eparts::TextField::new(&mut text).desired_width(160.0).show(ui);
    });
}

fn render_empty_state(ui: &mut egui::Ui) {
    layout::empty_state(
        ui,
        egui_phosphor::regular::FILM_STRIP,
        "No timeline loaded",
        "Open or create a scene to begin",
    );
}

fn render_palette(ui: &mut egui::Ui, theme: eparts::Theme) {
    let swatches: [(&str, egui::Color32); 8] = [
        ("Base", theme.surface.base),
        ("Panel", theme.surface.panel),
        ("Surface", theme.surface.surface),
        ("Widget", theme.surface.widget),
        ("Accent", theme.accent.primary),
        ("Cyan", theme.accent.cyan),
        ("Success", theme.status.success),
        ("Warning", theme.status.warning),
    ];
    ui.horizontal_wrapped(|ui| {
        for (label, color) in swatches {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(96.0, 72.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 6.0, color);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                label,
                TextRole::Micro.font_id(),
                theme.text.on_accent,
            );
            ui.add_space(4.0);
        }
    });
}

fn render_unknown_widget(ui: &mut egui::Ui, theme: eparts::Theme, name: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(24.0);
        ui.label(
            egui::RichText::new(format!("Unknown widget: {name}"))
                .size(TextRole::Title.size())
                .color(theme.text.secondary),
        );
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Available widgets:")
                .size(TextRole::BodyS.size())
                .color(theme.text.muted),
        );
        for (id, desc) in WIDGET_REGISTRY {
            ui.label(
                egui::RichText::new(format!("  {id} — {desc}"))
                    .size(TextRole::Micro.size())
                    .color(theme.text.muted),
            );
        }
    });
}

/// Install the chosen theme and Phosphor icons for the harness window.
pub fn install_theme(ctx: &egui::Context, theme: eparts::Theme, dark: bool) {
    eparts::set_theme(ctx, theme);

    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);

    let mut style = (*ctx.global_style()).clone();
    style.spacing.item_spacing = egui::vec2(4.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.window_margin = egui::Margin::same(8);
    style.visuals = theme.to_visuals(dark);
    ctx.set_global_style(style);
}
