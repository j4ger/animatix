use egui::{Color32, Context};

use crate::tokens::semantic;
use crate::tokens::spatial::STROKE_WIDTH;

use super::*;
use crate::tokens::util::{WCAG_AA_TEXT, WCAG_AA_UI, contrast_ratio};

fn assert_contrast(fg: Color32, bg: Color32, threshold: f64, label: &str) {
    let ratio = contrast_ratio(fg, bg);
    assert!(
        ratio >= threshold,
        "{label} contrast is {ratio:.2}:1, expected at least {threshold:.1}:1"
    );
}

#[test]
fn light_core_text_surface_pairs_meet_wcag_aa() {
    let t = Theme::light();
    let surfaces = [
        ("base", t.palette.surface.base),
        ("panel", t.palette.surface.panel),
        ("surface", t.palette.surface.surface),
        ("widget", t.palette.surface.widget),
    ];

    for (surface_name, bg) in surfaces {
        assert_contrast(
            t.palette.text.primary,
            bg,
            WCAG_AA_TEXT,
            &format!("primary/{surface_name}"),
        );
        assert_contrast(
            t.palette.text.secondary,
            bg,
            WCAG_AA_TEXT,
            &format!("secondary/{surface_name}"),
        );
        assert_contrast(t.palette.text.muted, bg, WCAG_AA_TEXT, &format!("muted/{surface_name}"));
    }

    assert_contrast(t.palette.text.primary, t.palette.surface.hover, WCAG_AA_TEXT, "primary/hover");
    assert_contrast(
        t.palette.text.primary,
        t.palette.surface.active,
        WCAG_AA_TEXT,
        "primary/active",
    );
}

#[test]
fn light_accent_button_pairs_meet_wcag_aa() {
    let t = Theme::light();
    assert_contrast(
        t.components.button.primary.normal.fg,
        t.components.button.primary.normal.bg,
        WCAG_AA_UI,
        "primary button normal",
    );
    assert_contrast(
        t.components.button.primary.hover.fg,
        t.components.button.primary.hover.bg,
        WCAG_AA_UI,
        "primary button hover",
    );
    assert_contrast(
        t.components.button.primary.active.fg,
        t.components.button.primary.active.bg,
        WCAG_AA_UI,
        "primary button active",
    );

    // Danger button in light mode
    assert_contrast(
        t.components.button.danger.normal.fg,
        t.components.button.danger.normal.bg,
        WCAG_AA_TEXT,
        "light danger button normal",
    );
    assert_contrast(
        t.components.button.danger.hover.fg,
        t.components.button.danger.hover.bg,
        WCAG_AA_UI,
        "light danger button hover",
    );
}

#[test]
fn dark_button_contrast_meets_wcag_aa() {
    let t = Theme::dark();
    assert_contrast(
        t.components.button.primary.normal.fg,
        t.components.button.primary.normal.bg,
        WCAG_AA_UI,
        "dark primary button normal",
    );
    assert_contrast(
        t.components.button.danger.normal.fg,
        t.components.button.danger.normal.bg,
        WCAG_AA_TEXT,
        "dark danger button normal",
    );
    assert_contrast(
        t.components.button.danger.hover.fg,
        t.components.button.danger.hover.bg,
        WCAG_AA_UI,
        "dark danger button hover",
    );
}

#[test]
fn dark_matches_semantic_constants() {
    let t = Theme::dark();
    assert_eq!(t.palette.surface.base, semantic::surface::BASE);
    assert_eq!(t.palette.surface.panel, semantic::surface::PANEL);
    assert_eq!(t.palette.surface.surface, semantic::surface::SURFACE);
    assert_eq!(t.palette.surface.widget, semantic::surface::WIDGET);
    assert_eq!(t.palette.surface.hover, semantic::surface::HOVER);
    assert_eq!(t.palette.surface.active, semantic::surface::ACTIVE);
    assert_eq!(t.palette.surface.floating_card_bg, semantic::surface::floating_card_bg());

    assert_eq!(t.palette.text.primary, semantic::text::PRIMARY);
    assert_eq!(t.palette.text.secondary, semantic::text::SECONDARY);
    assert_eq!(t.palette.text.muted, semantic::text::MUTED);
    assert_eq!(t.palette.text.disabled, semantic::text::DISABLED);
    assert_eq!(t.palette.text.on_accent, semantic::text::ON_ACCENT);
    assert_eq!(t.palette.text.faint, semantic::text::faint());
    assert_eq!(t.palette.text.subtle, semantic::text::subtle());
    assert_eq!(t.palette.text.hover, semantic::text::hover());
    assert_eq!(t.palette.text.dim, semantic::text::dim());

    assert_eq!(t.palette.accent.primary, semantic::accent::PRIMARY);
    assert_eq!(t.palette.accent.cyan, semantic::accent::CYAN);
    assert_eq!(t.palette.accent.primary_hover, semantic::accent::PRIMARY_HOVER);
    assert_eq!(t.palette.accent.primary_active, semantic::accent::PRIMARY_ACTIVE);
    assert_eq!(t.palette.accent.faint, semantic::accent::faint());
    assert_eq!(t.palette.accent.ghost, semantic::accent::ghost());
    assert_eq!(t.palette.accent.subtle, semantic::accent::subtle());
    assert_eq!(t.palette.accent.hover, semantic::accent::hover());
    assert_eq!(t.palette.accent.strong, semantic::accent::strong());
    assert_eq!(t.palette.accent.selection, semantic::accent::selection());

    assert_eq!(t.palette.status.success, semantic::status::SUCCESS);
    assert_eq!(t.palette.status.warning, semantic::status::WARNING);
    assert_eq!(t.palette.status.error, semantic::status::ERROR);
    assert_eq!(t.palette.status.info, semantic::status::INFO);
    assert_eq!(t.palette.status.playing_text, semantic::status::PLAYING_TEXT);
    assert_eq!(t.palette.status.diagnostic_error, semantic::status::DIAGNOSTIC_ERROR);
    assert_eq!(t.palette.status.diagnostic_warning, semantic::status::DIAGNOSTIC_WARNING);
    assert_eq!(t.palette.status.success_faint, semantic::status::success_faint());
    assert_eq!(t.palette.status.success_ultra_faint, semantic::status::success_ultra_faint());
    assert_eq!(t.palette.status.warning_subtle, semantic::status::warning_subtle());
    assert_eq!(t.palette.status.error_faint, semantic::status::error_faint());
    assert_eq!(t.palette.status.error_ultra_faint, semantic::status::error_ultra_faint());

    assert_eq!(t.palette.border.default, semantic::border::DEFAULT);
    assert_eq!(t.palette.border.strong, semantic::border::HOVER);
    assert_eq!(t.palette.border.focus, semantic::border::FOCUS);

    assert_eq!(t.palette.overlay.backdrop, semantic::overlay::backdrop());
    assert_eq!(t.palette.overlay.badge_bg, semantic::overlay::badge_bg());
    assert_eq!(t.palette.overlay.tooltip_bg, semantic::overlay::tooltip_bg());
    assert_eq!(t.palette.overlay.shadow_ambient, semantic::overlay::shadow_ambient());
    assert_eq!(t.palette.overlay.shadow_direct, semantic::overlay::shadow_direct());
    assert_eq!(t.palette.overlay.floating_hud_bg, semantic::overlay::floating_hud_bg());

    // Elevation shadows are non-zero (not default).
    assert!(t.elevation.raised.blur > 0);
    assert!(t.elevation.overlay.blur > t.elevation.raised.blur);
    assert!(t.elevation.overlay.color.a() > t.elevation.raised.color.a());

    assert_eq!(t.palette.lines.grid, semantic::lines::grid_line());
    assert_eq!(t.palette.lines.guide, semantic::lines::guide_line());
}

#[test]
fn theme_memory_round_trip() {
    let ctx = Context::default();
    let original = Theme::dark();
    set_theme(&ctx, original);
    let read = theme_from_ctx(&ctx);
    assert_eq!(read.palette.surface.base, original.palette.surface.base);
    assert_eq!(read.palette.text.primary, original.palette.text.primary);
    assert_eq!(read.palette.accent.primary, original.palette.accent.primary);
    assert_eq!(read.palette.status.success, original.palette.status.success);
    assert_eq!(read.palette.border.default, original.palette.border.default);
    assert_eq!(read.palette.overlay.backdrop, original.palette.overlay.backdrop);
    assert_eq!(read.elevation.raised, original.elevation.raised);
    assert_eq!(read.elevation.overlay, original.elevation.overlay);
}

#[test]
fn light_distinct_from_dark() {
    let d = Theme::dark();
    let l = Theme::light();
    assert_ne!(l.palette.surface.base, d.palette.surface.base);
    assert_ne!(l.palette.surface.panel, d.palette.surface.panel);
    assert_ne!(l.palette.text.primary, d.palette.text.primary);
    assert_ne!(l.palette.border.default, d.palette.border.default);
    assert_ne!(l.palette.overlay.backdrop, d.palette.overlay.backdrop);
    assert_ne!(l.palette.overlay.floating_hud_bg, d.palette.overlay.floating_hud_bg);
    // Light surface is bright, light text is dark (contrast sanity).
    assert!(
        l.palette.surface.base.r() > 200
            && l.palette.surface.base.g() > 200
            && l.palette.surface.base.b() > 200
    );
    assert!(
        l.palette.text.primary.r() < 80
            && l.palette.text.primary.g() < 80
            && l.palette.text.primary.b() < 80
    );
    // Raised is smaller than overlay in both themes.
    assert!(d.elevation.overlay.blur > d.elevation.raised.blur);
    assert!(l.elevation.overlay.blur > l.elevation.raised.blur);
    // Light raised is slightly stronger than dark (light surfaces need more contrast).
    assert!(l.elevation.raised.blur >= d.elevation.raised.blur);
}

#[test]
fn focus_ring_matches_border_focus() {
    assert_eq!(Theme::dark().focus_ring(), Theme::dark().palette.border.focus);
    assert_eq!(Theme::light().focus_ring(), Theme::light().palette.border.focus);
}

#[test]
fn accessor_methods_return_correct_values() {
    let d = Theme::dark();
    assert_eq!(d.elevation_raised(), d.elevation.raised);
    assert_eq!(d.elevation_overlay(), d.elevation.overlay);
    let l = Theme::light();
    assert_eq!(l.elevation_raised(), l.elevation.raised);
    assert_eq!(l.elevation_overlay(), l.elevation.overlay);
}

/// The grouped `Theme` accessors must keep exposing the exact
/// `tokens::semantic` values, and each group literal must be exhaustive
/// (a field dropped from `Palette`/`Components` fails to compile here).
#[test]
fn grouped_access_matches_semantic_sources() {
    let dark = Theme::dark();
    // Palette reads route to the same semantic consts as before grouping.
    assert_eq!(dark.palette.surface.base, semantic::surface::BASE);
    assert_eq!(dark.palette.text.primary, semantic::text::PRIMARY);
    assert_eq!(dark.palette.accent.primary, semantic::accent::PRIMARY);
    assert_eq!(dark.palette.status.error, semantic::status::ERROR);
    assert_eq!(dark.palette.border.focus, semantic::border::FOCUS);
    assert_eq!(dark.palette.overlay.backdrop, semantic::overlay::backdrop());
    assert_eq!(dark.palette.lines.grid, semantic::lines::grid_line());

    // Group literals enumerate every field; omitting one is a compile error.
    let palette = Palette {
        surface: dark.palette.surface,
        text: dark.palette.text,
        accent: dark.palette.accent,
        status: dark.palette.status,
        border: dark.palette.border,
        overlay: dark.palette.overlay,
        lines: dark.palette.lines,
    };
    assert_eq!(dark.palette, palette);
    let components = Components {
        button: dark.components.button,
        list: dark.components.list,
        tab: dark.components.tab,
        menu_item: dark.components.menu_item,
        input: dark.components.input,
        scrollbar: dark.components.scrollbar,
        toggle: dark.components.toggle,
        progress: dark.components.progress,
        badge: dark.components.badge,
        tag: dark.components.tag,
        kbd: dark.components.kbd,
        tooltip: dark.components.tooltip,
        alert: dark.components.alert,
        toast: dark.components.toast,
        skeleton: dark.components.skeleton,
        color_picker: dark.components.color_picker,
        easing_curve: dark.components.easing_curve,
    };
    assert_eq!(dark.components, components);

    // Light still routes its line tokens to the light-specific variants.
    assert_eq!(Theme::light().palette.lines.grid, semantic::lines::grid_line_light());
    // And both scopes differ between dark and light.
    assert_ne!(Theme::dark().palette, Theme::light().palette);
    assert_ne!(Theme::dark().components, Theme::light().components);
}

#[test]
fn to_visuals_maps_theme_fields() {
    let t = Theme::dark();
    let v = t.to_visuals(true);
    assert_eq!(v.panel_fill, t.palette.surface.panel);
    assert_eq!(v.selection.bg_fill, t.palette.accent.selection);
    assert_eq!(v.extreme_bg_color, t.palette.surface.base);

    let lt = Theme::light();
    let lv = lt.to_visuals(false);
    assert_eq!(lv.panel_fill, lt.palette.surface.panel);
    assert_eq!(v.widgets.open.corner_radius, v.widgets.hovered.corner_radius);
    assert_eq!(v.widgets.open.bg_stroke.width, STROKE_WIDTH);
    assert_eq!(v.widgets.open.expansion, 0.0);
    assert_eq!(lv.widgets.open.corner_radius, lv.widgets.hovered.corner_radius);
    assert_eq!(lv.widgets.open.bg_stroke.width, STROKE_WIDTH);
    assert_eq!(lv.widgets.open.expansion, 0.0);
}

#[test]
fn component_slots_seeded() {
    let t = Theme::dark();
    // Primary button normal bg is the accent; danger path exists.
    assert_eq!(t.components.button.primary.normal.bg, semantic::accent::PRIMARY);
    assert_eq!(t.components.button.danger.hover.bg, semantic::status::ERROR);
    assert_eq!(t.components.tab.active.indicator, semantic::accent::PRIMARY);
    assert_eq!(t.components.input.focus.border, semantic::border::FOCUS);
}

/// Every per-component slot group this change introduced must be defined in
/// both themes, with non-transparent values where the widget paints them.
#[test]
fn per_component_slots_defined_in_both_themes() {
    for (label, t) in [("dark", Theme::dark()), ("light", Theme::light())] {
        // Toggle: all four states plus thumb/mark must be visible colours.
        for (state, slot) in [
            ("checked", t.components.toggle.checked),
            ("unchecked", t.components.toggle.unchecked),
            ("hover", t.components.toggle.hover),
            ("disabled", t.components.toggle.disabled),
        ] {
            assert_ne!(slot.bg, Color32::TRANSPARENT, "{label} toggle.{state}.bg");
            assert_ne!(slot.border, Color32::TRANSPARENT, "{label} toggle.{state}.border");
            assert_ne!(slot.fg, Color32::TRANSPARENT, "{label} toggle.{state}.fg");
        }
        assert_ne!(t.components.toggle.thumb, Color32::TRANSPARENT, "{label} toggle.thumb");
        assert_ne!(t.components.toggle.mark, Color32::TRANSPARENT, "{label} toggle.mark");

        // Progress.
        assert_ne!(t.components.progress.track, Color32::TRANSPARENT, "{label} progress.track");
        assert_ne!(t.components.progress.fill, Color32::TRANSPARENT, "{label} progress.fill");
        assert_ne!(t.components.progress.label, Color32::TRANSPARENT, "{label} progress.label");
        assert_ne!(
            t.components.progress.label_on_fill,
            Color32::TRANSPARENT,
            "{label} progress.label_on_fill"
        );

        // Badge / kbd / tooltip single Slots.
        assert_ne!(t.components.badge.bg, Color32::TRANSPARENT, "{label} badge.bg");
        assert_ne!(t.components.badge.fg, Color32::TRANSPARENT, "{label} badge.fg");
        assert_ne!(t.components.kbd.bg, Color32::TRANSPARENT, "{label} kbd.bg");
        assert_ne!(t.components.kbd.fg, Color32::TRANSPARENT, "{label} kbd.fg");
        assert_ne!(t.components.kbd.border, Color32::TRANSPARENT, "{label} kbd.border");
        assert_ne!(t.components.tooltip.bg, Color32::TRANSPARENT, "{label} tooltip.bg");
        assert_ne!(t.components.tooltip.fg, Color32::TRANSPARENT, "{label} tooltip.fg");
        assert_ne!(t.components.tooltip.border, Color32::TRANSPARENT, "{label} tooltip.border");

        // Tag normal + hover.
        for (state, slot) in [
            ("normal", t.components.tag.normal),
            ("hover", t.components.tag.hover),
        ] {
            assert_ne!(slot.bg, Color32::TRANSPARENT, "{label} tag.{state}.bg");
            assert_ne!(slot.fg, Color32::TRANSPARENT, "{label} tag.{state}.fg");
            assert_ne!(slot.border, Color32::TRANSPARENT, "{label} tag.{state}.border");
        }

        // Alert: all four levels need a visible bg/accent/icon and fg.
        for (level, slots) in [
            ("info", t.components.alert.info),
            ("success", t.components.alert.success),
            ("warning", t.components.alert.warning),
            ("error", t.components.alert.error),
        ] {
            assert_ne!(slots.bg, Color32::TRANSPARENT, "{label} alert.{level}.bg");
            assert_ne!(slots.fg, Color32::TRANSPARENT, "{label} alert.{level}.fg");
            assert_ne!(slots.accent, Color32::TRANSPARENT, "{label} alert.{level}.accent");
            assert_ne!(slots.icon, Color32::TRANSPARENT, "{label} alert.{level}.icon");
        }

        // Toast: all four levels need visible bg/fg/border/accent.
        for (level, slots) in [
            ("info", t.components.toast.info),
            ("success", t.components.toast.success),
            ("warning", t.components.toast.warning),
            ("error", t.components.toast.error),
        ] {
            assert_ne!(slots.bg, Color32::TRANSPARENT, "{label} toast.{level}.bg");
            assert_ne!(slots.fg, Color32::TRANSPARENT, "{label} toast.{level}.fg");
            assert_ne!(slots.border, Color32::TRANSPARENT, "{label} toast.{level}.border");
            assert_ne!(slots.accent, Color32::TRANSPARENT, "{label} toast.{level}.accent");
        }

        // Skeleton / colour picker / easing curve.
        assert_ne!(t.components.skeleton.base, Color32::TRANSPARENT, "{label} skeleton.base");
        assert_ne!(t.components.skeleton.shimmer, Color32::TRANSPARENT, "{label} skeleton.shimmer");
        assert_ne!(
            t.components.color_picker.swatch_border,
            Color32::TRANSPARENT,
            "{label} color_picker.swatch_border"
        );
        assert_ne!(
            t.components.color_picker.swatch_hover,
            Color32::TRANSPARENT,
            "{label} color_picker.swatch_hover"
        );
        assert_eq!(
            t.components.color_picker.checker_light,
            Color32::WHITE,
            "{label} checker_light"
        );
        assert_ne!(
            t.components.color_picker.checker_dark,
            Color32::TRANSPARENT,
            "{label} checker_dark"
        );
        for (name, value) in [
            ("bg", t.components.easing_curve.bg),
            ("curve", t.components.easing_curve.curve),
            ("handle", t.components.easing_curve.handle),
            ("handle_active", t.components.easing_curve.handle_active),
        ] {
            assert_ne!(value, Color32::TRANSPARENT, "{label} easing_curve.{name}");
        }
        // The grid line is a low-alpha neutral; assert it is a real colour
        // (alpha > 0) rather than fully transparent.
        assert!(t.components.easing_curve.grid.a() > 0, "{label} easing_curve.grid alpha");
    }
}

/// The alert levels must keep their distinct status accents in both themes.
#[test]
fn alert_slots_use_distinct_status_accents() {
    for t in [Theme::dark(), Theme::light()] {
        assert_eq!(t.components.alert.success.accent, semantic::status::SUCCESS);
        assert_eq!(t.components.alert.warning.accent, semantic::status::WARNING);
        assert_eq!(t.components.alert.error.accent, semantic::status::ERROR);
        assert_eq!(t.components.alert.info.accent, semantic::status::INFO);
        // Info borrows the accent faint tint (no `info_faint` status token).
        assert_eq!(t.components.alert.info.bg, semantic::accent::faint());
    }
}

#[test]
fn app_theme_choice_resolves() {
    // Explicit choices ignore the system hint.
    assert!(!AppThemeChoice::Light.is_dark(Some(true)));
    assert!(AppThemeChoice::Dark.is_dark(Some(false)));
    // Auto follows the system, dark-fallback when unknown.
    assert!(AppThemeChoice::Auto.is_dark(Some(true)));
    assert!(!AppThemeChoice::Auto.is_dark(Some(false)));
    assert!(AppThemeChoice::Auto.is_dark(None));
    // resolve() picks the matching Theme.
    assert_eq!(
        AppThemeChoice::Light.resolve(None).palette.surface.base,
        Theme::light().palette.surface.base
    );
    assert_eq!(
        AppThemeChoice::Auto.resolve(Some(true)).palette.surface.base,
        Theme::dark().palette.surface.base
    );
    assert_eq!(AppThemeChoice::default(), AppThemeChoice::Auto);
}
