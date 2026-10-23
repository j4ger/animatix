use super::*;

// ── Side ────────────────────────────────────────────────────────

#[test]
fn side_default_is_right() {
    assert_eq!(Side::default(), Side::Right);
}

// ── Checkbox builder ───────────────────────────────────────────

#[test]
fn checkbox_builder_defaults() {
    let mut val = false;
    let ptr = &mut val as *mut _;
    let cb = Checkbox::new(&mut val);
    assert_eq!(cb.value as *const _, ptr as *const _);
    assert!(cb.label.is_none());
    assert_eq!(cb.label_side, Side::Right);
    assert_eq!(cb.tooltip, "");
}

#[test]
fn checkbox_builder_label() {
    let mut val = false;
    let cb = Checkbox::new(&mut val).label("Hello");
    assert_eq!(cb.label, Some("Hello"));
}

#[test]
fn checkbox_builder_label_side() {
    let mut val = false;
    let cb = Checkbox::new(&mut val).label_side(Side::Left);
    assert_eq!(cb.label_side, Side::Left);
}

#[test]
fn checkbox_builder_tooltip() {
    let mut val = false;
    let cb = Checkbox::new(&mut val).tooltip("tip");
    assert_eq!(cb.tooltip, "tip");
}

#[test]
fn checkbox_builder_chaining() {
    let mut val = true;
    let cb = Checkbox::new(&mut val).label("L").label_side(Side::Left).tooltip("T");
    assert_eq!(cb.label, Some("L"));
    assert_eq!(cb.label_side, Side::Left);
    assert_eq!(cb.tooltip, "T");
}

// ── Radio builder ──────────────────────────────────────────────

#[test]
fn radio_builder_defaults() {
    let mut val = 0u32;
    let r = Radio::new(&mut val, 1);
    assert_eq!(r.this_value, 1);
    assert!(r.label.is_none());
    assert_eq!(r.label_side, Side::Right);
    assert_eq!(r.tooltip, "");
}

#[test]
fn radio_builder_label() {
    let mut val = 0u32;
    let r = Radio::new(&mut val, 1).label("Opt");
    assert_eq!(r.label, Some("Opt"));
}

#[test]
fn radio_builder_label_side() {
    let mut val = 0u32;
    let r = Radio::new(&mut val, 1).label_side(Side::Left);
    assert_eq!(r.label_side, Side::Left);
}

#[test]
fn radio_builder_tooltip() {
    let mut val = 0u32;
    let r = Radio::new(&mut val, 1).tooltip("tip");
    assert_eq!(r.tooltip, "tip");
}

#[test]
fn radio_builder_chaining() {
    let mut val = 0u32;
    let r = Radio::new(&mut val, 2).label("L").label_side(Side::Left).tooltip("T");
    assert_eq!(r.this_value, 2);
    assert_eq!(r.label, Some("L"));
    assert_eq!(r.label_side, Side::Left);
    assert_eq!(r.tooltip, "T");
}

#[test]
fn radio_selection_logic() {
    let mut val = 1u32;
    let r = Radio::new(&mut val, 1);
    assert_eq!(*r.value, r.this_value);
}

// ── Switch builder ─────────────────────────────────────────────

#[test]
fn switch_builder_defaults() {
    let mut val = false;
    let s = Switch::new(&mut val);
    assert!(s.label.is_none());
    assert_eq!(s.label_side, Side::Right);
    assert_eq!(s.tooltip, "");
}

#[test]
fn switch_builder_label() {
    let mut val = false;
    let s = Switch::new(&mut val).label("On");
    assert_eq!(s.label, Some("On"));
}

#[test]
fn switch_builder_label_side() {
    let mut val = false;
    let s = Switch::new(&mut val).label_side(Side::Left);
    assert_eq!(s.label_side, Side::Left);
}

#[test]
fn switch_builder_tooltip() {
    let mut val = false;
    let s = Switch::new(&mut val).tooltip("tip");
    assert_eq!(s.tooltip, "tip");
}

#[test]
fn switch_builder_chaining() {
    let mut val = true;
    let s = Switch::new(&mut val).label("L").label_side(Side::Left).tooltip("T");
    assert_eq!(s.label, Some("L"));
    assert_eq!(s.label_side, Side::Left);
    assert_eq!(s.tooltip, "T");
}

// ── Animation id namespacing ───────────────────────────────────

/// Collect `animation_id` results for the same value pointer under
/// different parent `Ui` ids.
fn ids_for_same_ptr() -> (egui::Id, egui::Id, egui::Id, egui::Id) {
    let ctx = egui::Context::default();
    let value = 0u32;
    let mut under_a = egui::Id::NULL;
    let mut under_b = egui::Id::NULL;
    let mut top_level = egui::Id::NULL;
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        top_level = animation_id(ui, &value as *const u32);
        ui.push_id("panel_a", |ui| {
            under_a = animation_id(ui, &value as *const u32);
        });
        ui.push_id("panel_b", |ui| {
            under_b = animation_id(ui, &value as *const u32);
        });
    });
    // The pre-fix construction: keyed on the raw address alone.
    let legacy = egui::Id::new(&value as *const u32);
    (under_a, under_b, top_level, legacy)
}

#[test]
fn animation_id_is_namespaced_by_parent_ui() {
    let (under_a, under_b, top_level, legacy) = ids_for_same_ptr();
    assert_ne!(
        under_a, under_b,
        "the same value rendered under different parent Uis must not share animation state"
    );
    assert_ne!(under_a, top_level);
    assert_ne!(under_b, top_level);
    assert_ne!(under_a, legacy);
    assert_ne!(under_b, legacy);
}

#[test]
fn toggle_widgets_render_with_namespaced_animation_ids() {
    let ctx = egui::Context::default();
    let mut flag = false;
    let mut choice = 0u32;
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.push_id("ns_panel", |ui| {
            ui.add(Checkbox::new(&mut flag).label("Check"));
            ui.add(Switch::new(&mut flag).label("Switch"));
            ui.add(Radio::new(&mut choice, 1).label("Radio"));
        });
    });
}

/// Representative slot-resolution test: the toggle control's colour is
/// chosen from `theme.components.toggle.*` and the dark/light seeds differ as expected.
#[test]
fn toggle_slot_resolves_per_state_in_both_themes() {
    let dark = theme::Theme::dark();
    let light = theme::Theme::light();

    // State selection is theme-independent: disabled > checked > hover > unchecked.
    for t in [&dark, &light] {
        assert_eq!(toggle_slot(t, true, false, true), t.components.toggle.disabled);
        assert_eq!(toggle_slot(t, true, true, false), t.components.toggle.checked);
        assert_eq!(toggle_slot(t, false, true, true), t.components.toggle.hover);
        assert_eq!(toggle_slot(t, false, true, false), t.components.toggle.unchecked);
    }

    // The checked fill comes from the accent slot, not a widget literal, and
    // the light unchecked fill differs from the dark one.
    assert_eq!(dark.components.toggle.checked.bg, crate::tokens::semantic::accent::PRIMARY);
    assert_eq!(light.components.toggle.checked.bg, crate::tokens::semantic::accent::PRIMARY);
    assert_ne!(dark.components.toggle.unchecked.bg, light.components.toggle.unchecked.bg);
    assert_ne!(dark.components.toggle.mark, light.components.toggle.disabled.fg);
}

/// The toggle widgets render through the slots in both themes.
#[test]
fn toggle_widgets_render_in_both_themes() {
    let ctx = egui::Context::default();
    for t in [theme::Theme::dark(), theme::Theme::light()] {
        crate::tokens::theme::set_theme(&ctx, t);
        let mut flag = false;
        let mut choice = 0u32;
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.add(Checkbox::new(&mut flag).label("Check"));
            ui.add(Switch::new(&mut flag).label("Switch"));
            ui.add(Radio::new(&mut choice, 1).label("Radio"));
            ui.add_enabled(false, Checkbox::new(&mut flag).label("Disabled"));
            ui.add_enabled(false, Switch::new(&mut flag).label("Disabled"));
        });
    }
}
