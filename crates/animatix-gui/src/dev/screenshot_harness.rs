//! Widget Screenshot Harness
//!
//! Renders isolated GUI components for visual inspection.
//! Used by the `widget-screenshot` binary and future AI sessions.

use egui::Vec2;

use crate::app::components;
use crate::app::panels::inspector::property_groups::{
    PropertyEntry, PropertyGroup, PropertyKind, render_property_group, render_property_row,
};
use crate::app::commands::CommandQueue;
use crate::app::design_tokens::*;

/// Registry of all screenshot-able widgets.
pub const WIDGET_REGISTRY: &[(&str, &str)] = &[
    ("property-row-vec2", "Vec2 property row (x, y)"),
    ("property-row-float", "Float property row with unit suffix"),
    ("property-row-slider", "Slider row (0-1 range)"),
    ("property-row-color", "Color picker row with hex"),
    ("property-row-text", "Text/ComboBox row"),
    ("property-group", "Expanded property group header + rows"),
    ("inspector", "Full inspector panel (multiple groups)"),
    ("card", "Card container with shadow"),
    ("field", "Input field frame (DragValue)"),
    ("section-header", "Section header with accent line"),
    ("row", "Interactive list row"),
    ("icon-button", "Icon button with hover"),
    ("empty-state", "Empty state placeholder"),
    ("file-tree", "File tree with folders and files"),
    ("layer-tree", "Layer tree with actors and children"),
];

/// Render a widget by name into the given UI.
pub fn render_widget(ui: &mut egui::Ui, name: &str) {
    ui.set_width(ui.available_width());
    ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);

    match name {
        "property-row-vec2" => render_demo_property_row(ui, demo_vec2_entry()),
        "property-row-float" => render_demo_property_row(ui, demo_float_entry()),
        "property-row-slider" => render_demo_property_row(ui, demo_slider_entry()),
        "property-row-color" => render_demo_property_row(ui, demo_color_entry()),
        "property-row-text" => render_demo_property_row(ui, demo_text_entry()),
        "property-group" => render_demo_property_group(ui),
        "inspector" => render_demo_inspector(ui),
        "card" => render_demo_card(ui),
        "field" => render_demo_field(ui),
        "section-header" => render_demo_section_header(ui),
        "row" => render_demo_row(ui),
        "icon-button" => render_demo_icon_button(ui),
        "empty-state" => render_demo_empty_state(ui),
        "file-tree" => render_demo_file_tree(ui),
        "layer-tree" => render_demo_layer_tree(ui),
        _ => {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(
                    egui::RichText::new(format!("Unknown widget: {name}"))
                        .size(16.0)
                        .color(TEXT_SECONDARY),
                );
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("Available widgets:")
                        .size(12.0)
                        .color(TEXT_MUTED),
                );
                for (id, desc) in WIDGET_REGISTRY {
                    ui.label(
                        egui::RichText::new(format!("  {id} — {desc}"))
                            .size(11.0)
                            .color(TEXT_MUTED),
                    );
                }
            });
        }
    }
}

// ─── Property Row Demos ───────────────────────────────────────────────────

fn render_demo_property_row(ui: &mut egui::Ui, entry: PropertyEntry) {
    let mut commands = CommandQueue::new();
    render_property_row(ui, "actor1", &entry, &mut commands, false);
}

fn demo_vec2_entry() -> PropertyEntry {
    PropertyEntry {
        name: "position",
        kind: PropertyKind::Vec2 { x: 320.0, y: 240.0 },
        has_keyframes: true,
        has_keyframe_at_current_time: false,
    }
}

fn demo_float_entry() -> PropertyEntry {
    PropertyEntry {
        name: "rotation",
        kind: PropertyKind::Float(45.0),
        has_keyframes: false,
        has_keyframe_at_current_time: false,
    }
}

fn demo_slider_entry() -> PropertyEntry {
    PropertyEntry {
        name: "opacity",
        kind: PropertyKind::Float(0.75),
        has_keyframes: true,
        has_keyframe_at_current_time: true,
    }
}

fn demo_color_entry() -> PropertyEntry {
    PropertyEntry {
        name: "color",
        kind: PropertyKind::Color([1.0, 0.2, 0.4, 1.0]),
        has_keyframes: false,
        has_keyframe_at_current_time: false,
    }
}

fn demo_text_entry() -> PropertyEntry {
    PropertyEntry {
        name: "shape_type",
        kind: PropertyKind::Text("Ellipse".to_string()),
        has_keyframes: false,
        has_keyframe_at_current_time: false,
    }
}

// ─── Component Demos ──────────────────────────────────────────────────────

fn render_demo_property_group(ui: &mut egui::Ui) {
    let group = PropertyGroup {
        name: "Transform",
        icon: egui_phosphor::regular::ARROWS_OUT_CARDINAL,
        properties: vec![
            demo_vec2_entry(),
            demo_float_entry(),
            demo_slider_entry(),
        ],
    };
    let mut commands = CommandQueue::new();
    render_property_group(ui, &group, "actor1", &mut commands, false);
}

fn render_demo_inspector(ui: &mut egui::Ui) {
    components::card(ui, |ui| {
        components::section_header(ui, egui_phosphor::regular::WRENCH, "Properties", None);

        // Transform group
        let transform = PropertyGroup {
            name: "Transform",
            icon: egui_phosphor::regular::ARROWS_OUT_CARDINAL,
            properties: vec![
                PropertyEntry {
                    name: "position",
                    kind: PropertyKind::Vec2 { x: 320.0, y: 240.0 },
                    has_keyframes: true,
                    has_keyframe_at_current_time: false,
                },
                PropertyEntry {
                    name: "rotation",
                    kind: PropertyKind::Float(45.0),
                    has_keyframes: false,
                    has_keyframe_at_current_time: false,
                },
                PropertyEntry {
                    name: "scale",
                    kind: PropertyKind::Float(1.5),
                    has_keyframes: false,
                    has_keyframe_at_current_time: false,
                },
            ],
        };

        // Style group
        let style = PropertyGroup {
            name: "Style",
            icon: egui_phosphor::regular::PAINT_BRUSH,
            properties: vec![
                PropertyEntry {
                    name: "color",
                    kind: PropertyKind::Color([1.0, 0.2, 0.4, 1.0]),
                    has_keyframes: false,
                    has_keyframe_at_current_time: false,
                },
                PropertyEntry {
                    name: "opacity",
                    kind: PropertyKind::Float(0.75),
                    has_keyframes: true,
                    has_keyframe_at_current_time: true,
                },
                PropertyEntry {
                    name: "stroke_width",
                    kind: PropertyKind::Float(2.0),
                    has_keyframes: false,
                    has_keyframe_at_current_time: false,
                },
            ],
        };

        let mut commands = CommandQueue::new();
        render_property_group(ui, &transform, "actor1", &mut commands, false);
        render_property_group(ui, &style, "actor1", &mut commands, false);
    });
}

fn render_demo_card(ui: &mut egui::Ui) {
    components::card(ui, |ui| {
        ui.label(egui::RichText::new("Card Content").color(TEXT_PRIMARY));
        ui.add_space(SPACE_S);
        ui.label(egui::RichText::new("This is inside a card with shadow and padding.").size(FONT_SIZE_S).color(TEXT_SECONDARY));
    });
}

fn render_demo_field(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let mut val = 42.0;
        components::field_sized(ui, Some(80.0), |ui| {
            ui.add(egui::DragValue::new(&mut val).speed(1.0).suffix(" px"));
        });
        ui.add_space(SPACE_M);
        let mut text = "hello".to_string();
        components::field(ui, |ui| {
            ui.add(egui::TextEdit::singleline(&mut text));
        });
    });
}

fn render_demo_section_header(ui: &mut egui::Ui) {
    components::section_header(ui, egui_phosphor::regular::WRENCH, "Properties", Some(3));
    ui.add_space(SPACE_S);
    components::section_header(ui, egui_phosphor::regular::KEY, "Keyframes", None);
}

fn render_demo_row(ui: &mut egui::Ui) {
    let row1 = components::Row::new("Actor 1")
        .height(ROW_M)
        .icon(Some(egui_phosphor::regular::SQUARE))
        .has_children(true)
        .expanded(true);
    row1.show(ui, ui.id().with("row1"));

    ui.add_space(2.0);

    let row2 = components::Row::new("Actor 2")
        .height(ROW_M)
        .icon(Some(egui_phosphor::regular::CIRCLE))
        .has_children(false)
        .selected(true);
    row2.show(ui, ui.id().with("row2"));
}

fn render_demo_icon_button(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        components::icon_button(ui, egui_phosphor::regular::PLAY, "Play");
        ui.add_space(SPACE_S);
        components::icon_button(ui, egui_phosphor::regular::PAUSE, "Pause");
        ui.add_space(SPACE_S);
        components::icon_button(ui, egui_phosphor::regular::TRASH, "Delete");
    });
}

fn render_demo_empty_state(ui: &mut egui::Ui) {
    components::empty_state(
        ui,
        egui_phosphor::regular::FILM_STRIP,
        "No timeline loaded",
        "Open or create a scene to begin",
    );
}

fn render_demo_file_tree(ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);

    // Folder: src (expanded)
    let row1 = components::Row::new("src")
        .height(ROW_M)
        .icon(Some(egui_phosphor::regular::FOLDER_OPEN))
        .has_children(true)
        .expanded(true);
    row1.show(ui, ui.id().with("src"));

    // File: src/main.rs (selected)
    let row2 = components::Row::new("main.rs")
        .height(ROW_M)
        .indent(14.0)
        .icon(Some(egui_phosphor::regular::FILM_STRIP))
        .has_children(false)
        .selected(true)
        .label_color(ACCENT_BLUE);
    row2.show(ui, ui.id().with("main.rs"));

    // File: src/lib.rs
    let row3 = components::Row::new("lib.rs")
        .height(ROW_M)
        .indent(14.0)
        .icon(Some(egui_phosphor::regular::FILE))
        .has_children(false)
        .selected(false);
    row3.show(ui, ui.id().with("lib.rs"));

    // Folder: assets (collapsed)
    let row4 = components::Row::new("assets")
        .height(ROW_M)
        .icon(Some(egui_phosphor::regular::FOLDER))
        .has_children(true)
        .expanded(false);
    row4.show(ui, ui.id().with("assets"));

    // File: README.md
    let row5 = components::Row::new("README.md")
        .height(ROW_M)
        .icon(Some(egui_phosphor::regular::FILE_TEXT))
        .has_children(false)
        .selected(false);
    row5.show(ui, ui.id().with("README.md"));
}

fn render_demo_layer_tree(ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);

    // Root actor: Background (visible, expanded)
    let row1 = components::Row::new("background")
        .height(ROW_M)
        .icon(Some(egui_phosphor::regular::SQUARE))
        .has_children(true)
        .expanded(true)
        .selected(false)
        .right(|ui| {
            components::icon_button_colored(ui, egui_phosphor::regular::EYE, "Hide", TEXT_SECONDARY, TEXT_PRIMARY);
        });
    row1.show(ui, ui.id().with("bg"));

    // Child: Ellipse (visible, selected)
    let row2 = components::Row::new("circle1")
        .height(ROW_M)
        .indent(14.0)
        .icon(Some(egui_phosphor::regular::CIRCLE))
        .has_children(false)
        .selected(true)
        .right(|ui| {
            components::icon_button_colored(ui, egui_phosphor::regular::EYE, "Hide", TEXT_SECONDARY, TEXT_PRIMARY);
        });
    row2.show(ui, ui.id().with("circle1"));

    // Child: Text (hidden)
    let row3 = components::Row::new("title_text")
        .height(ROW_M)
        .indent(14.0)
        .icon(Some(egui_phosphor::regular::TEXT_T))
        .has_children(false)
        .selected(false)
        .label_color(TEXT_DISABLED)
        .right(|ui| {
            components::icon_button_colored(ui, egui_phosphor::regular::EYE_CLOSED, "Show", TEXT_DISABLED, TEXT_SECONDARY);
        });
    row3.show(ui, ui.id().with("title"));

    // Root actor: anon ghost (visible)
    let row4 = components::Row::new("anon")
        .height(ROW_M)
        .icon(Some(egui_phosphor::regular::GHOST))
        .has_children(false)
        .selected(false)
        .label_color(TEXT_MUTED)
        .right(|ui| {
            components::icon_button_colored(ui, egui_phosphor::regular::EYE, "Hide", TEXT_SECONDARY, TEXT_PRIMARY);
        });
    row4.show(ui, ui.id().with("anon"));
}
