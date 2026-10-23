use std::path::Path;

use crate::tokens::theme::Theme;

use super::*;

/// The macro guard: `full_from` must populate every field, and applying the
/// result back over a *default* theme must reproduce the original exactly.
/// If `partial_group!` ever drops a field (or forgets to bridge a leaf type),
/// the defaulted base shows through and this fails.
fn assert_full_round_trip(original: Theme) {
    let partial = PartialTheme::full_from(original);
    // Every field is present (not the all-empty `PartialTheme::default()`).
    assert_ne!(partial, PartialTheme::default(), "full_from produced no fields");
    let restored = partial.apply_to(Theme::default());
    assert_eq!(restored, original, "full_from -> apply_to(default) changed the theme");
}

#[test]
fn full_from_round_trips_dark_theme() {
    assert_full_round_trip(Theme::dark());
}

#[test]
fn full_from_round_trips_light_theme() {
    assert_full_round_trip(Theme::light());
}

/// `apply_to` with an empty partial must be an identity merge over any base.
#[test]
fn empty_partial_is_identity() {
    assert_eq!(PartialTheme::default().apply_to(Theme::dark()), Theme::dark());
    assert_eq!(PartialTheme::default().apply_to(Theme::light()), Theme::light());
}

#[test]
fn builtin_theme_file_roundtrips_both_modes() {
    let file = ThemeFile::builtin();
    let json = file.to_json_pretty().expect("serialize");
    let parsed = ThemeFile::from_json(&json).expect("parse");
    assert_eq!(parsed.dark_theme(), Theme::dark());
    assert_eq!(parsed.light_theme(), Theme::light());
    assert_eq!(parsed.name.as_deref(), Some("Animatix"));
}

#[test]
fn partial_override_keeps_unset_dark_tokens() {
    let json = r##"{
        "name": "Custom",
        "dark": { "surface": { "base": "#112233" } }
    }"##;
    let file = ThemeFile::from_json(json).expect("parse");
    let theme = file.dark_theme();
    assert_eq!(theme.palette.surface.base, egui::Color32::from_rgb(0x11, 0x22, 0x33));
    assert_eq!(theme.palette.surface.panel, Theme::dark().palette.surface.panel);
    assert_eq!(theme.palette.text.primary, Theme::dark().palette.text.primary);
    assert_eq!(
        theme.components.button.primary.normal.bg,
        Theme::dark().components.button.primary.normal.bg
    );
    assert_eq!(theme.elevation.raised, Theme::dark().elevation.raised);
}

#[test]
fn nested_slots_and_shadows_roundtrip() {
    let json = r##"{
        "dark": {
            "button": { "primary": { "hover": { "bg": "#ff0000" } } },
            "elevation": { "raised": { "offset": [0, 4], "blur": 8, "spread": 1, "color": "#000000aa" } }
        }
    }"##;
    let file = ThemeFile::from_json(json).expect("parse");
    let theme = file.dark_theme();
    assert_eq!(theme.components.button.primary.hover.bg, egui::Color32::from_rgb(0xff, 0, 0));
    assert_eq!(theme.elevation.raised.offset, [0, 4]);
    assert_eq!(theme.elevation.raised.blur, 8);
    assert_eq!(theme.elevation.raised.spread, 1);
    assert_eq!(
        theme.elevation.raised.color,
        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 0xaa)
    );
}

#[test]
fn unknown_fields_are_rejected() {
    let json = r##"{ "dark": { "unknown_token": "#ff0000" } }"##;
    assert!(ThemeFile::from_json(json).is_err());
}

/// The macro must preserve the hand-written wire representation exactly:
/// absent fields are omitted (not emitted as `null`), present nested groups
/// use the same key names, and colors serialize as hex strings.
#[test]
fn partial_serde_representation_matches_hand_written_shape() {
    // An all-`None` partial serializes to an empty object (omit-when-None).
    assert_eq!(serde_json::to_string(&PartialTheme::default()).unwrap(), "{}");
    assert_eq!(serde_json::to_string(&PartialSlot::default()).unwrap(), "{}");

    // A populated partial emits the original key names and hex colors.
    let partial = PartialTheme::full_from(Theme::dark());
    let value: serde_json::Value = serde_json::to_value(&partial).unwrap();
    let obj = value.as_object().expect("theme object");
    for key in [
        "surface",
        "text",
        "accent",
        "status",
        "border",
        "overlay",
        "lines",
        "button",
        "list",
        "tab",
        "menu_item",
        "input",
        "scrollbar",
        "toggle",
        "progress",
        "badge",
        "tag",
        "kbd",
        "tooltip",
        "alert",
        "toast",
        "skeleton",
        "color_picker",
        "easing_curve",
        "elevation",
    ] {
        assert!(obj.contains_key(key), "missing group {key:?}");
    }
    assert_eq!(value["surface"]["base"], Theme::dark().palette.surface.base.to_hex());
    // A nested slot keeps `bg`/`fg`/`border`, and a nested shadow keeps its
    // object shape (`offset`/`blur`/`spread`/`color`).
    assert!(value["button"]["primary"]["normal"]["bg"].is_string());
    assert!(value["elevation"]["raised"]["offset"].is_array());
}

#[test]
fn invalid_color_is_rejected() {
    let json = r##"{ "dark": { "surface": { "base": "not-a-color" } } }"##;
    assert!(ThemeFile::from_json(json).is_err());
}

#[test]
fn save_and_load_roundtrip() {
    let dir = std::env::temp_dir().join(format!("eparts-theme-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("theme.json");
    let file = ThemeFile::builtin();
    file.save(&path).expect("save");
    let loaded = ThemeFile::load(&path).expect("load");
    assert_eq!(loaded.dark_theme(), file.dark_theme());
    assert_eq!(loaded.light_theme(), file.light_theme());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Serde-compat guard: the checked-in example theme file (which predates the
/// macro refactor and the new component groups) must keep loading and
/// merging over the built-in bases.
#[test]
fn existing_example_theme_file_still_loads() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/gallery.theme.json");
    let file = ThemeFile::load(Path::new(path)).expect("load gallery.theme.json");
    assert_eq!(file.name.as_deref(), Some("Gallery JSON Theme"));
    let theme = file.dark_theme();
    // The override applies and missing groups inherit the built-in dark base.
    assert_eq!(theme.palette.surface.base, egui::Color32::from_rgb(0x19, 0x1a, 0x1e));
    assert_eq!(
        theme.components.button.primary.normal.bg,
        Theme::dark().components.button.primary.normal.bg
    );
    assert_eq!(theme.components.toggle.checked.bg, Theme::dark().components.toggle.checked.bg);
    assert_eq!(theme.components.toast.error.accent, Theme::dark().components.toast.error.accent);
}

#[test]
fn serialized_theme_json_stays_flat_after_grouping() {
    // The runtime `Theme` gained `palette`/`components` groups, but the
    // on-disk DTO must keep the historical flat shape so existing
    // `theme.json` files (and the schema) stay valid.
    let file = ThemeFile::builtin();
    let json = file.to_json_pretty().expect("serialize");
    let value: serde_json::Value = serde_json::from_str(&json).expect("parse");
    let dark = value["dark"].as_object().expect("dark object");
    for key in [
        "surface", "text", "accent", "status", "border", "overlay", "lines", "button",
    ] {
        assert!(dark.contains_key(key), "missing flat top-level key {key:?}");
    }
    assert!(!dark.contains_key("palette"), "JSON must not nest under `palette`");
    assert!(!dark.contains_key("components"), "JSON must not nest under `components`");
    assert!(dark["surface"]["base"].is_string());
}

#[test]
fn schema_is_valid_json_and_describes_theme_file() {
    let schema = theme_schema_json();
    let value: serde_json::Value = serde_json::from_str(&schema).expect("valid JSON");
    assert_eq!(value["$schema"], "https://json-schema.org/draft/2020-12/schema");
    assert!(value["$defs"]["theme"].is_object());
    assert!(value["$defs"]["elevation"].is_object());
}
