//! Keep source coordinates, rather than stale pixel offsets, through soft wrapping.
use egui::text::{CCursor, Galley};
use egui::vec2;

#[derive(Clone, Copy)]
pub(super) struct Anchor {
    cursor: CCursor,
    offset_in_row: f32,
}

impl Anchor {
    pub(super) fn capture(galley: &Galley, top: f32) -> Self {
        // Sample the first column of the first visible visual row. An offscreen
        // caret or a horizontally scrolled long line must not pull the view away.
        let cursor = galley.cursor_from_pos(vec2(0.0, top + 0.01));
        let offset_in_row = top - galley.pos_from_cursor(cursor).top();
        Self {
            cursor,
            offset_in_row,
        }
    }

    pub(super) fn top(self, galley: &Galley) -> f32 {
        let row = galley.pos_from_cursor(self.cursor);
        row.top() + self.offset_in_row.min((row.height() - 0.01).max(0.0))
    }

    #[cfg(test)]
    pub(super) fn char_index(self) -> usize {
        self.cursor.index.0
    }
}

pub(super) struct Position {
    pub anchor: Anchor,
    pub scroll_y: f32,
    pub width: f32,
    pub wrap: bool,
}

impl Position {
    pub(super) fn reflows(&self, width: f32, wrap: bool) -> bool {
        self.wrap != wrap || (wrap && (self.width - width).abs() > 0.01)
    }
}

/// One frame's viewport bookkeeping; explicit source movement is handled by
/// the editor before asking this helper to restore an anchor.
pub(super) struct Reflow {
    previous: Option<Position>,
    pub resizing: bool,
    visible_top: f32,
    width: f32,
    wrap: bool,
    restored: Option<(Anchor, f32)>,
}

impl Reflow {
    pub(super) fn new(previous: Option<Position>, ui: &egui::Ui, width: f32, wrap: bool) -> Self {
        let resizing = previous.as_ref().is_some_and(|p| p.reflows(width, wrap));
        let visible_top = ui.available_rect_before_wrap().top().max(ui.clip_rect().top());
        Self {
            previous,
            resizing,
            visible_top,
            width,
            wrap,
            restored: None,
        }
    }

    pub(super) fn restore(
        &mut self,
        ui: &egui::Ui,
        output: &egui::text_edit::TextEditOutput,
        drawn_scroll_y: f32,
    ) {
        if self.resizing
            && let Some(previous) = &self.previous
            && (previous.scroll_y - drawn_scroll_y).abs() < 0.75
        {
            let target = previous.anchor.top(&output.galley);
            let delta = target - (self.visible_top - output.galley_pos.y);
            if delta.abs() >= 0.75 {
                ui.scroll_with_delta(vec2(0.0, -delta));
                // ScrollArea installs this offset after painting. Run one more
                // layout pass so a splitter drag never presents a frame at
                // the old pixel offset in the newly wrapped buffer.
                ui.ctx().request_discard("Restore source viewport after wrapping");
            }
            self.restored = Some((previous.anchor, drawn_scroll_y + delta));
        }
    }

    pub(super) fn finish(
        self,
        output: &egui::text_edit::TextEditOutput,
        drawn_scroll_y: f32,
        scroll_y: f32,
        navigating: bool,
    ) -> Position {
        let top = self.visible_top - output.galley_pos.y + scroll_y - drawn_scroll_y;
        let anchor = self
            .restored
            .or_else(|| {
                // Keep the same source character over successive resize frames,
                // even when multiple characters land on the same widened row.
                self.previous
                    .filter(|_| !self.resizing && !navigating)
                    .map(|previous| (previous.anchor, previous.scroll_y))
            })
            .filter(|(_, expected)| (scroll_y - expected).abs() < 0.75)
            .map_or_else(|| Anchor::capture(&output.galley, top), |(anchor, _)| anchor);
        Position {
            anchor,
            scroll_y,
            width: self.width,
            wrap: self.wrap,
        }
    }
}
