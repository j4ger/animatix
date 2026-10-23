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
    /// Seeded from the current `semantic` module consts/functions.
    ///
    /// Guarantees `Theme::dark() == tokens::semantic::*` field-for-field because every value
    /// is sourced directly from `crate::tokens::semantic`.
    pub fn dark() -> Self {
        let slots = dark_component_slots();
        Self {
            palette: Palette {
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
                    subtle: semantic::border::SUBTLE,
                    rim_highlight: semantic::border::RIM_HIGHLIGHT,
                },
                overlay: Overlay {
                    backdrop: semantic::overlay::backdrop(),
                    badge_bg: semantic::overlay::badge_bg(),
                    tooltip_bg: semantic::overlay::tooltip_bg(),
                    shadow_ambient: semantic::overlay::shadow_ambient(),
                    shadow_direct: semantic::overlay::shadow_direct(),
                    floating_hud_bg: semantic::overlay::floating_hud_bg(),
                },
                lines: Lines {
                    grid: semantic::lines::grid_line(),
                    guide: semantic::lines::guide_line(),
                },
            },
            components: Components {
                button: dark_button_slots(),
                list: dark_list_slots(),
                tab: dark_tab_slots(),
                menu_item: dark_menu_item_slots(),
                input: dark_input_slots(),
                scrollbar: ScrollbarSlots {
                    thumb: semantic::border::HOVER,
                    thumb_hover: semantic::text::MUTED,
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
                raised: Shadow {
                    offset: [0, 2],
                    blur: 4,
                    spread: 0,
                    color: Color32::from_rgba_unmultiplied(0, 0, 0, 50),
                },
                overlay: Shadow {
                    offset: [0, 8],
                    blur: 24,
                    spread: 0,
                    color: Color32::from_rgba_unmultiplied(0, 0, 0, 90),
                },
            },
        }
    }
}

// ── Dark component-slot seeding (matches widget/button.rs painting) ─────

fn dark_button_slots() -> ButtonSlots {
    let transparent = Color32::TRANSPARENT;
    let focus = Slot {
        bg: transparent,
        fg: transparent,
        border: semantic::border::FOCUS,
    };
    ButtonSlots {
        primary: ButtonStateSlots {
            normal: Slot {
                bg: semantic::accent::PRIMARY,
                fg: Color32::WHITE,
                border: semantic::accent::PRIMARY,
            },
            hover: Slot {
                bg: semantic::accent::PRIMARY_HOVER,
                fg: Color32::WHITE,
                border: semantic::accent::PRIMARY_HOVER,
            },
            active: Slot {
                bg: semantic::accent::PRIMARY_ACTIVE,
                fg: Color32::WHITE,
                border: semantic::accent::PRIMARY_ACTIVE,
            },
            selected: Slot {
                bg: semantic::accent::PRIMARY_ACTIVE,
                fg: Color32::WHITE,
                border: semantic::accent::PRIMARY_ACTIVE,
            },
            disabled: Slot {
                bg: semantic::surface::WIDGET,
                fg: semantic::text::DISABLED,
                border: semantic::surface::WIDGET,
            },
            focus,
        },
        secondary: ButtonStateSlots {
            normal: Slot {
                bg: semantic::surface::WIDGET,
                fg: semantic::text::PRIMARY,
                border: semantic::border::DEFAULT,
            },
            hover: Slot {
                bg: semantic::surface::HOVER,
                fg: semantic::text::PRIMARY,
                border: semantic::accent::PRIMARY,
            },
            active: Slot {
                bg: semantic::surface::ACTIVE,
                fg: semantic::text::PRIMARY,
                border: semantic::accent::PRIMARY,
            },
            selected: Slot {
                bg: semantic::surface::ACTIVE,
                fg: semantic::text::PRIMARY,
                border: semantic::accent::PRIMARY,
            },
            disabled: Slot {
                bg: semantic::surface::WIDGET,
                fg: semantic::text::DISABLED,
                border: semantic::border::DEFAULT,
            },
            focus,
        },
        ghost: ButtonStateSlots {
            normal: Slot {
                bg: transparent,
                fg: semantic::text::SECONDARY,
                border: transparent,
            },
            hover: Slot {
                bg: semantic::surface::HOVER,
                fg: semantic::text::PRIMARY,
                border: transparent,
            },
            active: Slot {
                bg: semantic::surface::ACTIVE,
                fg: semantic::accent::PRIMARY,
                border: transparent,
            },
            selected: Slot {
                bg: semantic::surface::ACTIVE,
                fg: semantic::accent::PRIMARY,
                border: semantic::accent::PRIMARY,
            },
            disabled: Slot {
                bg: transparent,
                fg: semantic::text::DISABLED,
                border: transparent,
            },
            focus,
        },
        icon: ButtonStateSlots {
            normal: Slot {
                bg: transparent,
                fg: semantic::text::SECONDARY,
                border: transparent,
            },
            hover: Slot {
                bg: semantic::surface::HOVER,
                fg: semantic::text::PRIMARY,
                border: semantic::accent::PRIMARY,
            },
            active: Slot {
                bg: semantic::surface::ACTIVE,
                fg: semantic::text::PRIMARY,
                border: semantic::accent::PRIMARY,
            },
            selected: Slot {
                bg: semantic::surface::ACTIVE,
                fg: semantic::text::PRIMARY,
                border: semantic::accent::PRIMARY,
            },
            disabled: Slot {
                bg: transparent,
                fg: semantic::text::DISABLED,
                border: transparent,
            },
            focus,
        },
        danger: ButtonStateSlots {
            normal: Slot {
                bg: Color32::from_rgb(74, 29, 36),
                fg: Color32::from_rgb(255, 133, 149),
                border: Color32::from_rgb(224, 49, 75),
            },
            hover: Slot {
                bg: semantic::status::ERROR,
                fg: Color32::WHITE,
                border: semantic::status::ERROR,
            },
            active: Slot {
                bg: Color32::from_rgb(180, 30, 50),
                fg: Color32::WHITE,
                border: Color32::from_rgb(180, 30, 50),
            },
            selected: Slot {
                bg: semantic::status::ERROR,
                fg: Color32::WHITE,
                border: semantic::status::ERROR,
            },
            disabled: Slot {
                bg: semantic::surface::WIDGET,
                fg: semantic::text::DISABLED,
                border: semantic::surface::WIDGET,
            },
            focus,
        },
    }
}

fn dark_list_slots() -> ListSlots {
    ListSlots {
        even: Fill {
            bg: semantic::surface::SURFACE,
            fg: semantic::text::PRIMARY,
        },
        odd: Fill {
            bg: semantic::surface::WIDGET,
            fg: semantic::text::PRIMARY,
        },
        selected: Fill {
            bg: semantic::accent::selection(),
            fg: semantic::text::PRIMARY,
        },
        hover: Fill {
            bg: semantic::surface::HOVER,
            fg: semantic::text::PRIMARY,
        },
    }
}

fn dark_tab_slots() -> TabSlots {
    TabSlots {
        active: TabSlot {
            bg: semantic::surface::SURFACE,
            fg: semantic::text::PRIMARY,
            indicator: semantic::accent::PRIMARY,
        },
        inactive: TabSlot {
            bg: semantic::surface::WIDGET,
            fg: semantic::text::SECONDARY,
            indicator: Color32::TRANSPARENT,
        },
        hover: TabSlot {
            bg: semantic::surface::HOVER,
            fg: semantic::text::PRIMARY,
            indicator: Color32::TRANSPARENT,
        },
    }
}

fn dark_menu_item_slots() -> MenuItemSlots {
    MenuItemSlots {
        normal: Slot {
            bg: Color32::TRANSPARENT,
            fg: semantic::text::PRIMARY,
            border: Color32::TRANSPARENT,
        },
        hover: Slot {
            bg: semantic::surface::HOVER,
            fg: semantic::text::PRIMARY,
            border: Color32::TRANSPARENT,
        },
        active: Slot {
            bg: semantic::surface::ACTIVE,
            fg: semantic::text::PRIMARY,
            border: Color32::TRANSPARENT,
        },
        disabled: Slot {
            bg: Color32::TRANSPARENT,
            fg: semantic::text::DISABLED,
            border: Color32::TRANSPARENT,
        },
    }
}

fn dark_input_slots() -> InputSlots {
    InputSlots {
        normal: Slot {
            bg: semantic::surface::WIDGET,
            fg: semantic::text::PRIMARY,
            border: semantic::border::DEFAULT,
        },
        hover: Slot {
            bg: semantic::surface::WIDGET,
            fg: semantic::text::PRIMARY,
            border: semantic::border::HOVER,
        },
        focus: Slot {
            bg: semantic::surface::WIDGET,
            fg: semantic::text::PRIMARY,
            border: semantic::border::FOCUS,
        },
        invalid: Slot {
            bg: semantic::surface::WIDGET,
            fg: semantic::text::PRIMARY,
            border: semantic::status::ERROR,
        },
        disabled: Slot {
            bg: semantic::surface::WIDGET,
            fg: semantic::text::DISABLED,
            border: semantic::border::DEFAULT,
        },
    }
}

fn dark_component_slots() -> ComponentSlots {
    let transparent = Color32::TRANSPARENT;
    ComponentSlots {
        toggle: ToggleSlots {
            checked: Slot {
                bg: semantic::accent::PRIMARY,
                fg: semantic::text::ON_ACCENT,
                border: semantic::accent::PRIMARY,
            },
            unchecked: Slot {
                bg: semantic::surface::WIDGET,
                fg: semantic::text::ON_ACCENT,
                border: semantic::border::DEFAULT,
            },
            hover: Slot {
                bg: semantic::surface::HOVER,
                fg: semantic::text::ON_ACCENT,
                border: semantic::accent::PRIMARY,
            },
            disabled: Slot {
                bg: semantic::surface::WIDGET,
                fg: semantic::text::DISABLED,
                border: semantic::border::DEFAULT,
            },
            thumb: semantic::text::ON_ACCENT,
            mark: semantic::text::ON_ACCENT,
        },
        progress: ProgressSlots {
            track: semantic::surface::WIDGET,
            fill: semantic::accent::PRIMARY,
            label: semantic::text::PRIMARY,
            label_on_fill: semantic::text::ON_ACCENT,
        },
        badge: Slot {
            bg: semantic::overlay::badge_bg(),
            fg: semantic::accent::PRIMARY,
            border: transparent,
        },
        tag: TagSlots {
            normal: Slot {
                bg: semantic::surface::WIDGET,
                fg: semantic::text::PRIMARY,
                border: semantic::border::DEFAULT,
            },
            hover: Slot {
                bg: semantic::surface::HOVER,
                fg: semantic::text::PRIMARY,
                border: semantic::accent::PRIMARY,
            },
        },
        kbd: Slot {
            bg: semantic::overlay::badge_bg(),
            fg: semantic::text::SECONDARY,
            border: semantic::border::DEFAULT,
        },
        tooltip: Slot {
            bg: semantic::overlay::tooltip_bg(),
            fg: semantic::text::PRIMARY,
            border: semantic::border::DEFAULT,
        },
        alert: AlertSlots {
            info: alert_level_slots(
                semantic::accent::faint(),
                semantic::status::INFO,
                semantic::text::PRIMARY,
            ),
            success: alert_level_slots(
                semantic::status::success_faint(),
                semantic::status::SUCCESS,
                semantic::text::PRIMARY,
            ),
            warning: alert_level_slots(
                semantic::status::warning_subtle(),
                semantic::status::WARNING,
                semantic::text::PRIMARY,
            ),
            error: alert_level_slots(
                semantic::status::error_faint(),
                semantic::status::ERROR,
                semantic::text::PRIMARY,
            ),
        },
        toast: ToastSlots {
            info: toast_level_slots(
                semantic::surface::SURFACE,
                semantic::text::PRIMARY,
                semantic::border::DEFAULT,
                semantic::accent::PRIMARY,
            ),
            success: toast_level_slots(
                semantic::surface::SURFACE,
                semantic::text::PRIMARY,
                semantic::border::DEFAULT,
                semantic::status::SUCCESS,
            ),
            warning: toast_level_slots(
                semantic::surface::SURFACE,
                semantic::text::PRIMARY,
                semantic::border::DEFAULT,
                semantic::status::WARNING,
            ),
            error: toast_level_slots(
                semantic::surface::SURFACE,
                semantic::text::PRIMARY,
                semantic::border::DEFAULT,
                semantic::status::ERROR,
            ),
        },
        skeleton: SkeletonSlots {
            base: semantic::surface::WIDGET,
            shimmer: semantic::accent::PRIMARY,
        },
        color_picker: ColorPickerSlots {
            swatch_border: semantic::border::DEFAULT,
            swatch_hover: semantic::border::HOVER,
            // Checkerboard: light square is a fixed white, dark square uses the
            // disabled-neutral gray (matching the previous inline painting).
            checker_light: Color32::WHITE,
            checker_dark: semantic::text::DISABLED,
        },
        easing_curve: EasingCurveSlots {
            bg: semantic::surface::BASE,
            grid: semantic::lines::grid_line(),
            curve: semantic::accent::PRIMARY,
            handle: semantic::accent::PRIMARY,
            handle_active: semantic::status::WARNING,
        },
    }
}
