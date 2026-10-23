use egui::{Color32, Shadow};

use crate::tokens::semantic;

use super::components::{ComponentSlots, Components, alert_level_slots, toast_level_slots};
use super::elevation::Elevation;
use super::model::Theme;
use super::palette::{Accent, Border, Lines, Overlay, Palette, Status, Surface, Text};
use super::slots::{
    AlertSlots, ButtonSlots, ButtonStateSlots, ColorPickerSlots, EasingCurveSlots, Fill,
    InputSlots, ListSlots, MenuItemSlots, ProgressSlots, ScrollbarSlots, SkeletonSlots, Slot,
    TabSlot, TabSlots, TagSlots, ToastSlots, ToggleSlots,
};

impl Theme {
    /// Hand-authored light-mode palette.
    ///
    /// Light-mode values are authored directly here (not derived from the
    /// dark-oriented `primitive` palette). All raw `Color32` literals below are
    /// intentional light-palette entries. Accent and status hues are kept the
    /// same as dark mode so the brand identity is consistent; surfaces are
    /// near-white with subtle gray steps and text is dark for contrast.
    pub fn light() -> Self {
        // Surfaces (light grays / near-white).
        let base = Color32::from_rgb(248, 249, 250);
        let panel = Color32::from_rgb(255, 255, 255);
        let surf = Color32::from_rgb(250, 251, 252);
        let widget = Color32::from_rgb(240, 241, 243);
        let hover = Color32::from_rgb(227, 229, 233);
        let active = Color32::from_rgb(210, 212, 216);
        // Text (dark on light).
        let primary = Color32::from_rgb(20, 24, 30);
        let secondary = Color32::from_rgb(90, 97, 112);
        let muted = Color32::from_rgb(103, 109, 123);
        let disabled = Color32::from_rgb(180, 185, 192);
        // Dark text on accent fills matches the dark theme's ON_ACCENT role and
        // keeps primary/hover/accent button states within WCAG AA UI contrast.
        let on_accent = Color32::from_rgb(10, 12, 16);
        // Borders (mid grays).
        let border_default = Color32::from_rgb(200, 204, 209);
        let border_strong = Color32::from_rgb(160, 165, 172);
        let border_focus = semantic::border::FOCUS;
        // Danger active (shared with dark): a darkened error red.
        let danger_active = Color32::from_rgb(200, 40, 40);
        // Overlay fills (light variants); shared by the overlay tokens and the
        // badge/kbd component slots below.
        let badge_bg = Color32::from_rgba_unmultiplied(248, 249, 250, 235);
        let tooltip_bg = Color32::from_rgba_unmultiplied(255, 255, 255, 245);
        let slots = light_component_slots(LightComponentPaint {
            base,
            surface: surf,
            widget,
            hover,
            primary,
            secondary,
            disabled,
            on_accent,
            border_default,
            border_strong,
            badge_bg,
            tooltip_bg,
        });

        Self {
            palette: Palette {
                surface: Surface {
                    base,
                    panel,
                    surface: surf,
                    widget,
                    hover,
                    active,
                    floating_card_bg: Color32::from_rgba_unmultiplied(255, 255, 255, 245),
                },
                text: Text {
                    primary,
                    secondary,
                    muted,
                    disabled,
                    on_accent,
                    faint: Color32::from_rgba_unmultiplied(20, 24, 30, 80),
                    subtle: Color32::from_rgba_unmultiplied(20, 24, 30, 140),
                    hover: Color32::from_rgba_unmultiplied(20, 24, 30, 200),
                    dim: Color32::from_rgba_unmultiplied(20, 24, 30, 150),
                },
                // Accent/status hues kept identical to dark for brand consistency.
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
                    selection: Color32::from_rgba_unmultiplied(
                        semantic::accent::PRIMARY.r(),
                        semantic::accent::PRIMARY.g(),
                        semantic::accent::PRIMARY.b(),
                        60,
                    ),
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
                    default: border_default,
                    strong: border_strong,
                    focus: border_focus,
                    subtle: Color32::from_rgba_unmultiplied(0, 0, 0, 15),
                    rim_highlight: Color32::from_rgba_unmultiplied(255, 255, 255, 80),
                },
                overlay: Overlay {
                    backdrop: Color32::from_rgba_unmultiplied(0, 0, 0, 140),
                    badge_bg,
                    tooltip_bg,
                    shadow_ambient: Color32::from_rgba_unmultiplied(0, 0, 0, 30),
                    shadow_direct: Color32::from_rgba_unmultiplied(0, 0, 0, 50),
                    floating_hud_bg: Color32::from_rgba_unmultiplied(255, 255, 255, 235),
                },
                // White-alpha lines are invisible on a near-white surface; light
                // surfaces need dark ink instead.
                lines: Lines {
                    grid: semantic::lines::grid_line_light(),
                    guide: semantic::lines::guide_line_light(),
                },
            },
            components: Components {
                button: light_button_slots(
                    widget,
                    hover,
                    active,
                    primary,
                    disabled,
                    on_accent,
                    border_default,
                    border_focus,
                    secondary,
                    danger_active,
                ),
                list: ListSlots {
                    even: Fill {
                        bg: surf,
                        fg: primary,
                    },
                    odd: Fill {
                        bg: widget,
                        fg: primary,
                    },
                    selected: Fill {
                        bg: Color32::from_rgba_unmultiplied(
                            semantic::accent::PRIMARY.r(),
                            semantic::accent::PRIMARY.g(),
                            semantic::accent::PRIMARY.b(),
                            60,
                        ),
                        fg: primary,
                    },
                    hover: Fill {
                        bg: hover,
                        fg: primary,
                    },
                },
                tab: TabSlots {
                    active: TabSlot {
                        bg: surf,
                        fg: primary,
                        indicator: semantic::accent::PRIMARY,
                    },
                    inactive: TabSlot {
                        bg: widget,
                        fg: secondary,
                        indicator: Color32::TRANSPARENT,
                    },
                    hover: TabSlot {
                        bg: hover,
                        fg: primary,
                        indicator: Color32::TRANSPARENT,
                    },
                },
                menu_item: MenuItemSlots {
                    normal: Slot {
                        bg: Color32::TRANSPARENT,
                        fg: primary,
                        border: Color32::TRANSPARENT,
                    },
                    hover: Slot {
                        bg: hover,
                        fg: primary,
                        border: Color32::TRANSPARENT,
                    },
                    active: Slot {
                        bg: active,
                        fg: primary,
                        border: Color32::TRANSPARENT,
                    },
                    disabled: Slot {
                        bg: Color32::TRANSPARENT,
                        fg: disabled,
                        border: Color32::TRANSPARENT,
                    },
                },
                input: InputSlots {
                    normal: Slot {
                        bg: widget,
                        fg: primary,
                        border: border_default,
                    },
                    hover: Slot {
                        bg: widget,
                        fg: primary,
                        border: border_strong,
                    },
                    focus: Slot {
                        bg: widget,
                        fg: primary,
                        border: border_focus,
                    },
                    invalid: Slot {
                        bg: widget,
                        fg: primary,
                        border: semantic::status::ERROR,
                    },
                    disabled: Slot {
                        bg: widget,
                        fg: disabled,
                        border: border_default,
                    },
                },
                scrollbar: ScrollbarSlots {
                    thumb: border_strong,
                    thumb_hover: semantic::accent::PRIMARY,
                },
                toggle: slots.toggle,
                progress: slots.progress,
                badge: slots.badge,
                tag: slots.tag,
                kbd: slots.kbd,
                tooltip: slots.tooltip,
                alert: slots.alert,
                toast: slots.toast,
                skeleton: slots.skeleton,
                color_picker: slots.color_picker,
                easing_curve: slots.easing_curve,
            },
            elevation: Elevation {
                // Light: raised shadow — slightly stronger offset/blur for light bg
                raised: Shadow {
                    offset: [0, 3],
                    blur: 6,
                    spread: 0,
                    color: Color32::from_rgba_unmultiplied(0, 0, 0, 50),
                },
                // Light: overlay shadow — larger for dialog/modal
                overlay: Shadow {
                    offset: [0, 12],
                    blur: 32,
                    spread: 0,
                    color: Color32::from_rgba_unmultiplied(0, 0, 0, 60),
                },
            },
        }
    }
}

#[allow(clippy::too_many_arguments)] // Light slot seeding threads independent light-palette colors; grouping them is unnecessary indirection.
fn light_button_slots(
    widget: Color32,
    hover: Color32,
    active: Color32,
    text_primary: Color32,
    disabled: Color32,
    _on_accent: Color32,
    border_default: Color32,
    border_focus: Color32,
    secondary: Color32,
    danger_active: Color32,
) -> ButtonSlots {
    let transparent = Color32::TRANSPARENT;
    let focus = Slot {
        bg: transparent,
        fg: transparent,
        border: border_focus,
    };
    let a = semantic::accent::PRIMARY;
    let a_hover = semantic::accent::PRIMARY_HOVER;
    let a_active = semantic::accent::PRIMARY_ACTIVE;
    ButtonSlots {
        primary: ButtonStateSlots {
            normal: Slot {
                bg: a,
                fg: Color32::WHITE,
                border: a,
            },
            hover: Slot {
                bg: a_hover,
                fg: Color32::WHITE,
                border: a_hover,
            },
            active: Slot {
                bg: a_active,
                fg: Color32::WHITE,
                border: a_active,
            },
            selected: Slot {
                bg: a_active,
                fg: Color32::WHITE,
                border: a_active,
            },
            disabled: Slot {
                bg: widget,
                fg: disabled,
                border: widget,
            },
            focus,
        },
        secondary: ButtonStateSlots {
            normal: Slot {
                bg: widget,
                fg: text_primary,
                border: border_default,
            },
            hover: Slot {
                bg: hover,
                fg: text_primary,
                border: a,
            },
            active: Slot {
                bg: active,
                fg: text_primary,
                border: a,
            },
            selected: Slot {
                bg: active,
                fg: text_primary,
                border: a,
            },
            disabled: Slot {
                bg: widget,
                fg: disabled,
                border: border_default,
            },
            focus,
        },
        ghost: ButtonStateSlots {
            normal: Slot {
                bg: transparent,
                fg: secondary,
                border: transparent,
            },
            hover: Slot {
                bg: hover,
                fg: text_primary,
                border: transparent,
            },
            active: Slot {
                bg: active,
                fg: a,
                border: transparent,
            },
            selected: Slot {
                bg: active,
                fg: a,
                border: a,
            },
            disabled: Slot {
                bg: transparent,
                fg: disabled,
                border: transparent,
            },
            focus,
        },
        icon: ButtonStateSlots {
            normal: Slot {
                bg: transparent,
                fg: secondary,
                border: transparent,
            },
            hover: Slot {
                bg: hover,
                fg: text_primary,
                border: a,
            },
            active: Slot {
                bg: active,
                fg: text_primary,
                border: a,
            },
            selected: Slot {
                bg: active,
                fg: text_primary,
                border: a,
            },
            disabled: Slot {
                bg: transparent,
                fg: disabled,
                border: transparent,
            },
            focus,
        },
        danger: ButtonStateSlots {
            normal: Slot {
                bg: Color32::from_rgb(254, 226, 226),
                fg: Color32::from_rgb(153, 27, 27),
                border: Color32::from_rgb(248, 113, 113),
            },
            hover: Slot {
                bg: semantic::status::ERROR,
                fg: Color32::WHITE,
                border: semantic::status::ERROR,
            },
            active: Slot {
                bg: danger_active,
                fg: Color32::WHITE,
                border: danger_active,
            },
            selected: Slot {
                bg: semantic::status::ERROR,
                fg: Color32::WHITE,
                border: semantic::status::ERROR,
            },
            disabled: Slot {
                bg: widget,
                fg: disabled,
                border: widget,
            },
            focus,
        },
    }
}

/// Light-mode inputs the component slots need. Threading these avoids
/// re-deriving the light palette inside the seeding function (the palette is
/// authored once in `Theme::light()`).
struct LightComponentPaint {
    base: Color32,
    surface: Color32,
    widget: Color32,
    hover: Color32,
    primary: Color32,
    secondary: Color32,
    disabled: Color32,
    on_accent: Color32,
    border_default: Color32,
    border_strong: Color32,
    badge_bg: Color32,
    tooltip_bg: Color32,
}

fn light_component_slots(p: LightComponentPaint) -> ComponentSlots {
    let transparent = Color32::TRANSPARENT;
    ComponentSlots {
        toggle: ToggleSlots {
            checked: Slot {
                bg: semantic::accent::PRIMARY,
                fg: p.on_accent,
                border: semantic::accent::PRIMARY,
            },
            unchecked: Slot {
                bg: p.widget,
                fg: p.on_accent,
                border: p.border_default,
            },
            hover: Slot {
                bg: p.hover,
                fg: p.on_accent,
                border: semantic::accent::PRIMARY,
            },
            disabled: Slot {
                bg: p.widget,
                fg: p.disabled,
                border: p.border_default,
            },
            thumb: p.on_accent,
            mark: p.on_accent,
        },
        progress: ProgressSlots {
            track: p.widget,
            fill: semantic::accent::PRIMARY,
            label: p.primary,
            label_on_fill: p.on_accent,
        },
        badge: Slot {
            bg: p.badge_bg,
            fg: semantic::accent::PRIMARY,
            border: transparent,
        },
        tag: TagSlots {
            normal: Slot {
                bg: p.widget,
                fg: p.primary,
                border: p.border_default,
            },
            hover: Slot {
                bg: p.hover,
                fg: p.primary,
                border: semantic::accent::PRIMARY,
            },
        },
        kbd: Slot {
            bg: p.badge_bg,
            fg: p.secondary,
            border: p.border_default,
        },
        tooltip: Slot {
            bg: p.tooltip_bg,
            fg: p.primary,
            border: p.border_default,
        },
        alert: AlertSlots {
            info: alert_level_slots(semantic::accent::faint(), semantic::status::INFO, p.primary),
            success: alert_level_slots(
                semantic::status::success_faint(),
                semantic::status::SUCCESS,
                p.primary,
            ),
            warning: alert_level_slots(
                semantic::status::warning_subtle(),
                semantic::status::WARNING,
                p.primary,
            ),
            error: alert_level_slots(
                semantic::status::error_faint(),
                semantic::status::ERROR,
                p.primary,
            ),
        },
        toast: ToastSlots {
            info: toast_level_slots(
                p.surface,
                p.primary,
                p.border_default,
                semantic::accent::PRIMARY,
            ),
            success: toast_level_slots(
                p.surface,
                p.primary,
                p.border_default,
                semantic::status::SUCCESS,
            ),
            warning: toast_level_slots(
                p.surface,
                p.primary,
                p.border_default,
                semantic::status::WARNING,
            ),
            error: toast_level_slots(
                p.surface,
                p.primary,
                p.border_default,
                semantic::status::ERROR,
            ),
        },
        skeleton: SkeletonSlots {
            base: p.widget,
            shimmer: semantic::accent::PRIMARY,
        },
        color_picker: ColorPickerSlots {
            swatch_border: p.border_default,
            swatch_hover: p.border_strong,
            checker_light: Color32::WHITE,
            checker_dark: p.disabled,
        },
        easing_curve: EasingCurveSlots {
            bg: p.base,
            grid: semantic::lines::grid_line_light(),
            curve: semantic::accent::PRIMARY,
            handle: semantic::accent::PRIMARY,
            handle_active: semantic::status::WARNING,
        },
    }
}
