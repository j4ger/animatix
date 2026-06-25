//! Runtime `Theme` struct + immediate-mode accessors.
//!
//! `Theme` mirrors `tokens::semantic` so that `Theme::dark()` is *by construction*
//! equal to the current compile-time consts.  The const modules stay intact;
//! this is purely additive and existing call sites do not need to change.
//!
//! Access pattern (per §3a Step 2 of the roadmap):
//! ```ignore
//! let t = eparts::theme(ui);
//! let bg = t.surface.base;
//! ```
//! Set once per frame (or on theme switch):
//! ```ignore
//! eparts::set_theme(ctx, Theme::light());
//! ```

use egui::{Color32, Context};

use crate::tokens::semantic;

// ── Nested slot structs ───────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Default)]
pub struct Surface {
    pub base: Color32,
    pub panel: Color32,
    pub surface: Color32,
    pub widget: Color32,
    pub hover: Color32,
    pub active: Color32,
    pub floating_card_bg: Color32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Text {
    pub primary: Color32,
    pub secondary: Color32,
    pub muted: Color32,
    pub disabled: Color32,
    pub on_accent: Color32,
    pub faint: Color32,
    pub subtle: Color32,
    pub hover: Color32,
    pub dim: Color32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Accent {
    pub primary: Color32,
    pub cyan: Color32,
    pub primary_hover: Color32,
    pub primary_active: Color32,
    pub faint: Color32,
    pub ghost: Color32,
    pub subtle: Color32,
    pub hover: Color32,
    pub strong: Color32,
    pub selection: Color32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Status {
    pub success: Color32,
    pub warning: Color32,
    pub error: Color32,
    pub info: Color32,
    pub playing_text: Color32,
    pub diagnostic_error: Color32,
    pub diagnostic_warning: Color32,
    pub success_faint: Color32,
    pub success_ultra_faint: Color32,
    pub warning_subtle: Color32,
    pub error_faint: Color32,
    pub error_ultra_faint: Color32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Border {
    pub default: Color32,
    /// `strong` mirrors `semantic::border::HOVER` (the strongest neutral border).
    pub strong: Color32,
    pub focus: Color32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Overlay {
    pub backdrop: Color32,
    pub badge_bg: Color32,
    pub tooltip_bg: Color32,
    pub shadow_ambient: Color32,
    pub shadow_direct: Color32,
}

// ── Theme ─────────────────────────────────────────────────────────────

/// The runtime theme.  Each field is a `Color32`; the struct is `Copy` so cloning is cheap.
#[derive(Clone, Copy, Debug, Default)]
pub struct Theme {
    pub surface: Surface,
    pub text: Text,
    pub accent: Accent,
    pub status: Status,
    pub border: Border,
    pub overlay: Overlay,
}

impl Theme {
    /// Seeded from the current `semantic` module consts/functions.
    ///
    /// Guarantees `Theme::dark() == tokens::semantic::*` field-for-field because every value
    /// is sourced directly from `crate::tokens::semantic`.
    pub fn dark() -> Self {
        Self {
            surface: Surface {
                base: semantic::surface::BASE,
                panel: semantic::surface::PANEL,
                surface: semantic::surface::SURFACE,
                widget: semantic::surface::WIDGET,
                hover: semantic::surface::HOVER,
                active: semantic::surface::ACTIVE,
                floating_card_bg: semantic::surface::floating_card_bg(),
            },
            text: Text {
                primary: semantic::text::PRIMARY,
                secondary: semantic::text::SECONDARY,
                muted: semantic::text::MUTED,
                disabled: semantic::text::DISABLED,
                on_accent: semantic::text::ON_ACCENT,
                faint: semantic::text::faint(),
                subtle: semantic::text::subtle(),
                hover: semantic::text::hover(),
                dim: semantic::text::dim(),
            },
            accent: Accent {
                primary: semantic::accent::PRIMARY,
                cyan: semantic::accent::CYAN,
                primary_hover: semantic::accent::PRIMARY_HOVER,
                primary_active: semantic::accent::PRIMARY_ACTIVE,
                faint: semantic::accent::faint(),
                ghost: semantic::accent::ghost(),
                subtle: semantic::accent::subtle(),
                hover: semantic::accent::hover(),
                strong: semantic::accent::strong(),
                selection: semantic::accent::selection(),
            },
            status: Status {
                success: semantic::status::SUCCESS,
                warning: semantic::status::WARNING,
                error: semantic::status::ERROR,
                info: semantic::status::INFO,
                playing_text: semantic::status::PLAYING_TEXT,
                diagnostic_error: semantic::status::DIAGNOSTIC_ERROR,
                diagnostic_warning: semantic::status::DIAGNOSTIC_WARNING,
                success_faint: semantic::status::success_faint(),
                success_ultra_faint: semantic::status::success_ultra_faint(),
                warning_subtle: semantic::status::warning_subtle(),
                error_faint: semantic::status::error_faint(),
                error_ultra_faint: semantic::status::error_ultra_faint(),
            },
            border: Border {
                default: semantic::border::DEFAULT,
                strong: semantic::border::HOVER,
                focus: semantic::border::FOCUS,
            },
            overlay: Overlay {
                backdrop: semantic::overlay::backdrop(),
                badge_bg: semantic::overlay::badge_bg(),
                tooltip_bg: semantic::overlay::tooltip_bg(),
                shadow_ambient: semantic::overlay::shadow_ambient(),
                shadow_direct: semantic::overlay::shadow_direct(),
            },
        }
    }

    // TODO(M2/B6): pub fn light()
}

// ── Immediate-mode Memory accessors ───────────────────────────────────

/// Read the current `Theme` from the `Ui`'s `egui::Context`.
pub fn theme(ui: &egui::Ui) -> Theme {
    theme_from_ctx(ui.ctx())
}

/// Read the current `Theme` from an `egui::Context`.
pub fn theme_from_ctx(ctx: &Context) -> Theme {
    ctx.data(|d| d.get_temp::<Theme>(egui::Id::new("eparts_theme")))
        .unwrap_or_default()
}

/// Store a new `Theme` in the `egui::Context`'s `Memory`.
pub fn set_theme(ctx: &Context, theme: Theme) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("eparts_theme"), theme));
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_matches_semantic_constants() {
        let t = Theme::dark();
        assert_eq!(t.surface.base, semantic::surface::BASE);
        assert_eq!(t.surface.panel, semantic::surface::PANEL);
        assert_eq!(t.surface.surface, semantic::surface::SURFACE);
        assert_eq!(t.surface.widget, semantic::surface::WIDGET);
        assert_eq!(t.surface.hover, semantic::surface::HOVER);
        assert_eq!(t.surface.active, semantic::surface::ACTIVE);
        assert_eq!(
            t.surface.floating_card_bg,
            semantic::surface::floating_card_bg()
        );

        assert_eq!(t.text.primary, semantic::text::PRIMARY);
        assert_eq!(t.text.secondary, semantic::text::SECONDARY);
        assert_eq!(t.text.muted, semantic::text::MUTED);
        assert_eq!(t.text.disabled, semantic::text::DISABLED);
        assert_eq!(t.text.on_accent, semantic::text::ON_ACCENT);
        assert_eq!(t.text.faint, semantic::text::faint());
        assert_eq!(t.text.subtle, semantic::text::subtle());
        assert_eq!(t.text.hover, semantic::text::hover());
        assert_eq!(t.text.dim, semantic::text::dim());

        assert_eq!(t.accent.primary, semantic::accent::PRIMARY);
        assert_eq!(t.accent.cyan, semantic::accent::CYAN);
        assert_eq!(t.accent.primary_hover, semantic::accent::PRIMARY_HOVER);
        assert_eq!(t.accent.primary_active, semantic::accent::PRIMARY_ACTIVE);
        assert_eq!(t.accent.faint, semantic::accent::faint());
        assert_eq!(t.accent.ghost, semantic::accent::ghost());
        assert_eq!(t.accent.subtle, semantic::accent::subtle());
        assert_eq!(t.accent.hover, semantic::accent::hover());
        assert_eq!(t.accent.strong, semantic::accent::strong());
        assert_eq!(t.accent.selection, semantic::accent::selection());

        assert_eq!(t.status.success, semantic::status::SUCCESS);
        assert_eq!(t.status.warning, semantic::status::WARNING);
        assert_eq!(t.status.error, semantic::status::ERROR);
        assert_eq!(t.status.info, semantic::status::INFO);
        assert_eq!(t.status.playing_text, semantic::status::PLAYING_TEXT);
        assert_eq!(
            t.status.diagnostic_error,
            semantic::status::DIAGNOSTIC_ERROR
        );
        assert_eq!(
            t.status.diagnostic_warning,
            semantic::status::DIAGNOSTIC_WARNING
        );
        assert_eq!(t.status.success_faint, semantic::status::success_faint());
        assert_eq!(
            t.status.success_ultra_faint,
            semantic::status::success_ultra_faint()
        );
        assert_eq!(t.status.warning_subtle, semantic::status::warning_subtle());
        assert_eq!(t.status.error_faint, semantic::status::error_faint());
        assert_eq!(
            t.status.error_ultra_faint,
            semantic::status::error_ultra_faint()
        );

        assert_eq!(t.border.default, semantic::border::DEFAULT);
        assert_eq!(t.border.strong, semantic::border::HOVER);
        assert_eq!(t.border.focus, semantic::border::FOCUS);

        assert_eq!(t.overlay.backdrop, semantic::overlay::backdrop());
        assert_eq!(t.overlay.badge_bg, semantic::overlay::badge_bg());
        assert_eq!(t.overlay.tooltip_bg, semantic::overlay::tooltip_bg());
        assert_eq!(t.overlay.shadow_ambient, semantic::overlay::shadow_ambient());
        assert_eq!(t.overlay.shadow_direct, semantic::overlay::shadow_direct());
    }

    #[test]
    fn theme_memory_round_trip() {
        let ctx = Context::default();
        let original = Theme::dark();
        set_theme(&ctx, original);
        let read = theme_from_ctx(&ctx);
        assert_eq!(read.surface.base, original.surface.base);
        assert_eq!(read.text.primary, original.text.primary);
        assert_eq!(read.accent.primary, original.accent.primary);
        assert_eq!(read.status.success, original.status.success);
        assert_eq!(read.border.default, original.border.default);
        assert_eq!(read.overlay.backdrop, original.overlay.backdrop);
    }
}
