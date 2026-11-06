use super::*;

impl Default for InsertionPalette {
    fn default() -> Self {
        Self {
            open: false,
            query: String::new(),
            selected_index: 0,
            mode: PaletteMode::Universal,
            items: Vec::new(),
            filtered: Vec::new(),
            param_form: None,
        }
    }
}

pub(super) fn category_color(
    category: animatix::timeline::ActorCategory,
    theme: eparts::Theme,
) -> Color32 {
    match category {
        animatix::timeline::ActorCategory::Shape => theme.palette.accent.primary,
        animatix::timeline::ActorCategory::Container => theme.palette.status.success,
        animatix::timeline::ActorCategory::Text => theme.palette.status.warning,
        animatix::timeline::ActorCategory::Media => category::ACTION,
        animatix::timeline::ActorCategory::Plot => theme.palette.accent.cyan,
        animatix::timeline::ActorCategory::Annotation => theme.palette.accent.cyan,
    }
}

pub(super) fn action_category_color(category: &str, theme: eparts::Theme) -> Color32 {
    match category {
        "Entrance" => theme.palette.status.success,
        "Exit" => theme.palette.status.error,
        "Motion" => theme.palette.accent.primary,
        "Effects" => theme.palette.status.warning,
        "Reveal" => category::ACTION,
        "Reorder" => theme.palette.status.success,
        _ => theme.palette.text.secondary,
    }
}

pub(super) fn format_num(val: f64) -> String {
    if val.fract() == 0.0 && val.abs() < 1_000_000.0 {
        format!("{}", val as i64)
    } else if (val * 100.0).fract() == 0.0 {
        format!("{:.2}", val)
    } else {
        format!("{}", val)
    }
}

/// Parse a Vec2 value string like "(100, 200)" or "100, 200".
pub(super) fn parse_vec2_value(s: &str) -> (f64, f64) {
    let s = s.trim().trim_start_matches('(').trim_end_matches(')');
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() >= 2 {
        let x = parts[0].trim().parse::<f64>().unwrap_or(0.0);
        let y = parts[1].trim().parse::<f64>().unwrap_or(0.0);
        (x, y)
    } else {
        (0.0, 0.0)
    }
}
