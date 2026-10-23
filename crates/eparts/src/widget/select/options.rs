use super::*;

pub(super) fn visible_positions(flat_options: &[FlatOption], filter: &str) -> Vec<usize> {
    let mut pending_header: Option<usize> = None;
    let mut out: Vec<usize> = Vec::new();
    let filter_lower = filter.to_lowercase();

    for (i, opt) in flat_options.iter().enumerate() {
        match opt {
            FlatOption::Header(_) => pending_header = Some(i),
            FlatOption::Item { label, .. } => {
                let matches = filter.is_empty() || label.to_lowercase().contains(&filter_lower);
                if matches {
                    if let Some(h) = pending_header.take() {
                        out.push(h);
                    }
                    out.push(i);
                }
            },
        }
    }
    out
}

/// Move the highlighted position by `delta` within `visible` (wrapping-free,
/// clamped) and return the new position. `None` starts at the first row.
pub(super) fn step_highlight(
    visible: &[usize],
    current: Option<usize>,
    delta: i32,
) -> Option<usize> {
    if visible.is_empty() {
        return None;
    }
    let Some(current) = current else {
        return visible.first().copied();
    };
    let Some(idx) = visible.iter().position(|p| *p == current) else {
        return visible.first().copied();
    };
    let next = if delta >= 0 {
        (idx + 1).min(visible.len() - 1)
    } else {
        idx.saturating_sub(1)
    };
    visible.get(next).copied()
}

/// Paint one option row using the `theme.components.list.*` slots.
pub(super) fn paint_option_row(
    ui: &egui::Ui,
    rect: Rect,
    hovered: bool,
    selected: bool,
    highlighted: bool,
    label: &str,
    t: &crate::tokens::theme::Theme,
) {
    let s = crate::spatial(ui);
    let bg = if selected {
        t.components.list.selected.bg
    } else if highlighted {
        t.palette.accent.ghost
    } else if hovered {
        t.components.list.hover.bg
    } else {
        egui::Color32::TRANSPARENT
    };
    let painter = ui.painter_at(rect);
    if bg != egui::Color32::TRANSPARENT {
        painter.rect_filled(rect, egui::CornerRadius::same(2), bg);
    }
    let fg = if selected {
        t.components.list.selected.fg
    } else {
        t.palette.text.primary
    };
    painter.text(
        egui::pos2(rect.min.x + s.space_3, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        TextRole::BodyS.font_id(),
        fg,
    );
}
