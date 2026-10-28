//! Palette and component slots that dark and light mode seed identically.
//!
//! Only roles whose colour does not depend on the background live here: the
//! accent and status hues (brand and signal colours), plus the alert/toast
//! slots, which take the caller's surface-roles as parameters. Surfaces, text,
//! borders and the remaining per-theme component slots stay hand-authored in
//! `dark.rs` / `light.rs`.

use egui::Color32;

use crate::tokens::semantic;

use super::components::{alert_level_slots, toast_level_slots};
use super::palette::{Accent, Status};
use super::slots::{AlertSlots, ToastSlots};

/// Accent roles — the brand hues do not change with the background.
pub(super) fn accent() -> Accent {
    Accent {
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
    }
}

/// Status roles — a warning reads the same on either background.
pub(super) fn status() -> Status {
    Status {
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
    }
}

/// Alert slots — level tints are shared, the body text is the caller's.
pub(super) fn alert_levels(text: Color32) -> AlertSlots {
    AlertSlots {
        info: alert_level_slots(semantic::accent::faint(), semantic::status::INFO, text),
        success: alert_level_slots(
            semantic::status::success_faint(),
            semantic::status::SUCCESS,
            text,
        ),
        warning: alert_level_slots(
            semantic::status::warning_subtle(),
            semantic::status::WARNING,
            text,
        ),
        error: alert_level_slots(semantic::status::error_faint(), semantic::status::ERROR, text),
    }
}

/// Toast slots — surface/text/border are the caller's, the level accent is shared.
pub(super) fn toast_levels(surface: Color32, text: Color32, border: Color32) -> ToastSlots {
    ToastSlots {
        info: toast_level_slots(surface, text, border, semantic::accent::PRIMARY),
        success: toast_level_slots(surface, text, border, semantic::status::SUCCESS),
        warning: toast_level_slots(surface, text, border, semantic::status::WARNING),
        error: toast_level_slots(surface, text, border, semantic::status::ERROR),
    }
}
