//! Single-buffer `.amx` source editor with diagnostics, navigation and completion.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use animatix_analyzer::Analyzer;
use egui::text::{CCursor, CCursorRange};
use egui::text_edit::TextEditState;
use egui::{Align, Id, Key, Pos2, Rect, Response, ScrollArea};

use crate::app::design_tokens::typography::TextRole;
use crate::completion_popup::{CompletionInput, CompletionPopup};
use crate::editor::completion::CompletionAnchor;
use crate::highlighting::{FindHighlight, HighlightCache, HighlightDiagnostic, SourceLayout};

mod cell_view;
pub(crate) mod cells;
mod completion;
mod diagnostics;
mod find_match;
pub(crate) mod narration;
mod particle_seed;
pub(crate) mod source_patch;
#[cfg(test)]
mod tests;
mod viewport;

pub(crate) use cell_view::CellPlayback;

static NEXT_EDITOR_INSTANCE: AtomicU64 = AtomicU64::new(1);

pub struct EditorBuffer {
    text: String,
    document_path: PathBuf,
    // Queries explicitly requiring the current document can complete the deferred
    // analysis. Retain its import and extension state across draft revisions.
    analyzer: OnceLock<Analyzer>,
    previous_analyzer: Mutex<Option<Analyzer>>,
    analysis_due: Option<Instant>,
    completion: CompletionPopup,
    completion_anchor: Option<CompletionAnchor>,
    /// Caret position where an automatically opened popup was dismissed.
    completion_auto_armed: Option<usize>,
    pub pending_scroll_to_line: Option<usize>,
    pub highlighted_line: Option<usize>,
    pub cursor_line: Option<usize>,
    pub(super) cursor_char: Option<usize>,
    pub(super) pending_cursor_char: Option<usize>,
    pending_cursor_focus: bool,
    pub(super) diagnostics: Vec<HighlightDiagnostic>,
    pub(super) find_highlights: Vec<FindHighlight>,
    highlight_cache: Option<HighlightCache>,
    instance_id: u64,
    source_revision: u64,
    narration: narration::NarrationEditor,
    state_generation: u64,
    text_edit_id: Id,
    retired_state_ids: Vec<(Id, Option<Id>)>,
    current_scroll_area_id: Option<Id>,
    viewport_position: Option<viewport::Position>,
}

impl EditorBuffer {
    pub fn new(path: &Path, text: String) -> Self {
        let instance_id = NEXT_EDITOR_INSTANCE.fetch_add(1, Ordering::Relaxed);
        let text_edit_id = Id::unique(("animatix-source-editor", path, instance_id, 0_u64));
        Self {
            analyzer: OnceLock::from(analyzer_for_path(path, &text)),
            previous_analyzer: Mutex::new(None),
            analysis_due: None,
            text,
            document_path: path.to_path_buf(),
            completion: CompletionPopup::new(),
            completion_anchor: None,
            completion_auto_armed: None,
            pending_scroll_to_line: None,
            highlighted_line: None,
            cursor_line: None,
            cursor_char: None,
            pending_cursor_char: None,
            pending_cursor_focus: false,
            diagnostics: Vec::new(),
            find_highlights: Vec::new(),
            highlight_cache: None,
            instance_id,
            source_revision: 0,
            narration: narration::NarrationEditor::default(),
            state_generation: 0,
            text_edit_id,
            retired_state_ids: Vec::new(),
            current_scroll_area_id: None,
            viewport_position: None,
        }
    }

    /// Replace the active document and isolate its caret/undo state from the
    /// previously open file.
    pub fn set_document(&mut self, path: &Path, text: String) {
        self.retire_widget_state();
        self.source_revision = self.source_revision.wrapping_add(1);
        self.document_path = path.to_path_buf();
        self.text = text;
        self.clear_narration_anchor_changes();
        self.analyzer = OnceLock::from(analyzer_for_path(path, &self.text));
        self.previous_analyzer = Mutex::new(None);
        self.analysis_due = None;
        self.reset_ephemeral_state();
        self.advance_widget_generation();
    }

    /// Replace source from an external operation such as undo, reload or
    /// Find/Replace All. Normal keystrokes never call this and retain egui's
    /// native undo and IME composition state.
    pub fn replace_text(&mut self, text: String) {
        self.retire_widget_state();
        self.source_revision = self.source_revision.wrapping_add(1);
        self.text = text;
        self.defer_analysis();
        self.finish_analysis();
        self.reset_ephemeral_state();
        self.advance_widget_generation();
    }

    pub fn set_extension_manifest(&mut self, manifest: animatix_analyzer::ExtensionManifest) {
        self.finish_analysis();
        self.analyzer
            .get_mut()
            .expect("analysis is current")
            .set_extension_manifest(manifest);
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn document_path(&self) -> &Path {
        &self.document_path
    }

    pub(crate) fn source_identity(&self) -> (u64, u64) {
        (self.instance_id, self.source_revision)
    }

    pub fn scroll_to_line(&mut self, line: usize) {
        self.pending_scroll_to_line = Some(line);
    }

    /// Focus the source location and move the caret to the end of its token.
    /// Line and column are zero-based Unicode-scalar positions.
    pub fn focus_diagnostic(&mut self, line: usize, column: usize) {
        self.set_source_view(narration::SourceView::Full);
        let index = animatix_syntax::token::LineIndex::new(&self.text);
        let start_byte = index.line_col_to_byte(line, column);
        let mut caret = self.text[..start_byte].chars().count();
        let chars: Vec<char> = self.text.chars().collect();
        while caret < chars.len() && chars[caret].is_whitespace() {
            caret += 1;
        }
        while caret < chars.len() && !chars[caret].is_whitespace() {
            caret += 1;
        }
        self.pending_cursor_char = Some(caret);
        self.pending_cursor_focus = true;
        self.scroll_to_line(line);
    }

    pub fn set_highlighted_line(&mut self, line: Option<usize>) {
        self.highlighted_line = line;
    }

    pub fn analyzer(&self) -> &Analyzer {
        self.analyzer.get_or_init(|| {
            let mut previous = self
                .previous_analyzer
                .lock()
                .expect("editor analysis")
                .take()
                .expect("previous analysis retained across edits");
            previous.update(&self.text);
            previous
        })
    }

    fn defer_analysis(&mut self) {
        if let Some(previous) = self.analyzer.take() {
            *self.previous_analyzer.get_mut().expect("editor analysis") = Some(previous);
        }
        self.analysis_due = Some(Instant::now() + Duration::from_millis(120));
    }

    fn finish_analysis(&mut self) {
        if self.analysis_due.take().is_some() {
            self.analyzer();
            self.highlight_cache = None;
        }
    }

    /// Format valid source with the canonical DSL formatter.
    pub fn format_source(&mut self) -> bool {
        let (statements, errors) = animatix_syntax::parser::parse_source(&self.text);
        if let Some(statements) = statements
            && errors.is_empty()
        {
            let formatted = animatix_syntax::formatter::Formatter::default()
                .format_source(&self.text, &statements);
            if formatted != self.text {
                self.replace_text(formatted);
                return true;
            }
        }
        false
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> Response {
        self.show_cells(ui, &mut cell_view::CellPlayback::default())
    }

    pub(crate) fn show_cells(
        &mut self,
        ui: &mut egui::Ui,
        playback: &mut CellPlayback<'_>,
    ) -> Response {
        self.clear_retired_widget_state(ui.ctx());
        let mut completion_changed = self.process_completion_input(ui.ctx());
        self.ensure_highlight_cache(ui);
        let cache = self.highlight_cache.as_ref().expect("highlight cache prepared");
        let cache_source = cache.source.as_str();
        let cached_layout = &cache.layout;
        let line_count = cache.line_count;
        let max_line_chars = cache.max_line_chars;
        let edit_id = self.text_edit_id;
        let scroll_salt = ("source-editor-scroll", self.instance_id, self.state_generation);
        let font_id = TextRole::Mono.font_id();
        let line_height = (ui.fonts_mut(|fonts| fonts.row_height(&font_id)) + 5.0).max(20.0);
        let mut laid_out_edit: Option<(String, SourceLayout)> = None;
        let mut edit_caret_rect = None;
        let mut pending_line = self.pending_scroll_to_line.take();
        let pending_cursor = self.pending_cursor_char.take();
        let should_focus = std::mem::take(&mut self.pending_cursor_focus);
        let mut edit_state = TextEditState::load(ui.ctx(), edit_id).unwrap_or_default();
        if let Some(char_idx) = pending_cursor {
            edit_state
                .cursor
                .set_char_range(Some(CCursorRange::one(CCursor::new(char_idx))));
        }
        if self.editor_has_focus(ui.ctx()) {
            pending_line =
                cell_view::navigate_headers(ui, &mut edit_state, &self.text, &cached_layout.cells)
                    .or(pending_line);
        }
        let prior_cursor = edit_state.cursor.char_range();
        edit_state.store(ui.ctx(), edit_id);

        let available = ui.available_size_before_wrap();
        let char_width = ui.fonts_mut(|fonts| fonts.glyph_width(&TextRole::Mono.font_id(), '0'));
        let wrap = available.x < 520.0;
        let gutter = cell_view::gutter_width(line_count, wrap);
        let viewport_width = (available.x - gutter - 10.0).max(40.0);
        let text_width = if wrap {
            viewport_width
        } else {
            (max_line_chars as f32 * char_width + 12.0).max(viewport_width)
        };
        let mut reflow =
            viewport::Reflow::new(self.viewport_position.take(), ui, available.x, wrap);
        let navigating = pending_line.is_some() || pending_cursor.is_some();
        let scroll = ScrollArea::both()
            .id_salt(scroll_salt)
            .auto_shrink([false, false])
            .animated(!reflow.resizing || navigating)
            .show_viewport(ui, |scroll_ui, viewport| {
                let background = scroll_ui.painter().add(egui::Shape::Noop);
                let left = scroll_ui.next_widget_position().x;
                let frame = egui::Frame::NONE.inner_margin(egui::Margin {
                    left: gutter as i8,
                    right: 5,
                    top: 3,
                    bottom: 3,
                });
                frame
                    .show(scroll_ui, |edit_ui| {
                        let mut layouter =
                            |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, width: f32| {
                                let source = buffer.as_str();
                                let layout = if source == cache_source {
                                    cached_layout
                                } else {
                                    &laid_out_edit
                                        .insert((
                                            source.to_owned(),
                                            SourceLayout::new(
                                                source,
                                                ui.style(),
                                                None,
                                                Some(cache),
                                                &[],
                                                None,
                                                &[],
                                            ),
                                        ))
                                        .1
                                };
                                cell_view::galley(
                                    ui,
                                    layout,
                                    line_height,
                                    if wrap { width } else { f32::INFINITY },
                                )
                            };
                        let output = egui::TextEdit::multiline(&mut self.text)
                            .id(edit_id)
                            .code_editor()
                            .desired_width(text_width)
                            .desired_rows(line_count.max(1))
                            .frame(egui::Frame::NONE)
                            .margin(0.0)
                            .layouter(&mut layouter)
                            .show(edit_ui);
                        let layout = laid_out_edit
                            .as_ref()
                            .filter(|(source, _)| *source == self.text)
                            .map_or(cached_layout, |(_, layout)| layout);
                        cell_view::paint(
                            edit_ui,
                            &output,
                            &layout.cells,
                            left,
                            background,
                            playback,
                            self.text == cache_source,
                        );
                        let pos = output.galley_pos.to_vec2();
                        edit_caret_rect = output
                            .cursor_range
                            .map(|r| output.galley.pos_from_cursor(r.primary).translate(pos));
                        if let Some(line) = pending_line {
                            let char_index = char_index_at_line_start(&self.text, line);
                            let target = output
                                .galley
                                .pos_from_cursor(CCursor::new(char_index))
                                .translate(pos);
                            let viewport = edit_ui.clip_rect();
                            edit_ui.scroll_to_rect(
                                Rect::from_min_max(
                                    Pos2::new(viewport.left(), target.top()),
                                    Pos2::new(viewport.right(), target.bottom()),
                                ),
                                Some(Align::Center),
                            );
                        } else if !navigating
                            && self.text == cache_source
                            && (!output.response.response.has_focus()
                                || prior_cursor == output.cursor_range)
                        {
                            reflow.restore(edit_ui, &output, viewport.min.y);
                        }
                        (output, viewport.min.y)
                    })
                    .inner
            });
        self.current_scroll_area_id = Some(scroll.id);
        let (mut output, drawn_scroll_y) = scroll.inner;
        self.viewport_position = Some(reflow.finish(
            &output,
            drawn_scroll_y,
            scroll.state.unclamped_offset().y,
            navigating,
        ));
        let mut response = output.response.response.clone();
        let source_changed = response.changed() || self.text != cache_source;
        if source_changed && (cache_source.contains("///") || self.text.contains("///")) {
            let previous = cache_source.to_owned();
            if self.rebase_full_source(&previous, output.cursor_range.as_mut()) {
                output.state.cursor.set_char_range(output.cursor_range);
                output.state.store(ui.ctx(), edit_id);
            }
        }

        if should_focus {
            response.request_focus();
        }
        if let Some(cursor) = output.cursor_range
            && response.has_focus()
        {
            let cursor_char = cursor.primary.index.0;
            self.cursor_char = Some(cursor_char);
            self.cursor_line = Some(
                animatix_syntax::token::LineIndex::new(&self.text)
                    .byte_to_line_col(char_to_byte_offset(&self.text, cursor_char))
                    .0,
            );
        }
        if source_changed {
            self.after_source_replaced();
            response.mark_changed();
        }

        if self.editor_has_focus(ui.ctx()) {
            let triggered =
                ui.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, Key::Space));
            if triggered {
                self.trigger_completion();
            }
        }
        if !completion_changed {
            self.sync_completion_session();
        }
        completion_changed |= self.show_completion_popup(ui, edit_caret_rect, response.rect);
        if completion_changed {
            response.mark_changed();
        }
        response
    }

    pub(super) fn byte_to_line_col(&self, byte_offset: usize) -> (usize, usize) {
        animatix_syntax::token::LineIndex::new(&self.text).byte_to_line_col(byte_offset)
    }

    pub(super) fn after_source_replaced(&mut self) {
        self.source_revision = self.source_revision.wrapping_add(1);
        self.viewport_position = None;
        self.defer_analysis();
        self.completion_anchor = None;
        self.completion_auto_armed = None;
    }

    fn ensure_highlight_cache(&mut self, ui: &egui::Ui) {
        if let Some(due) = self.analysis_due {
            let remaining = due.saturating_duration_since(Instant::now());
            if remaining.is_zero() || self.analyzer.get().is_some() {
                self.finish_analysis();
            } else {
                ui.ctx().request_repaint_after(remaining);
            }
        }
        let style = ui.style();
        let dark_mode = style.visuals.dark_mode;
        let fresh = self.highlight_cache.as_ref().is_some_and(|cache| {
            cache.source == self.text
                && cache.dark_mode == dark_mode
                && cache.font_id == TextRole::Mono.font_id()
                && cache.highlighted_line == self.highlighted_line
                && cache.diagnostics == self.diagnostics
                && cache.find_highlights == self.find_highlights
        });
        if fresh {
            return;
        }
        let job = SourceLayout::new(
            &self.text,
            style,
            self.analyzer.get(),
            self.highlight_cache.as_ref(),
            &self.diagnostics,
            self.highlighted_line,
            &self.find_highlights,
        );
        self.cache_layout_for_current_source(job, style);
    }

    fn cache_layout_for_current_source(&mut self, job: SourceLayout, style: &egui::Style) {
        self.highlight_cache = Some(HighlightCache::new(
            self.text.clone(),
            style.visuals.dark_mode,
            TextRole::Mono.font_id(),
            self.highlighted_line,
            self.diagnostics.clone(),
            self.find_highlights.clone(),
            job,
        ));
    }

    fn editor_has_focus(&self, ctx: &egui::Context) -> bool {
        ctx.memory(|memory| memory.has_focus(self.text_edit_id))
    }

    fn process_completion_input(&mut self, ctx: &egui::Context) -> bool {
        let mut changed = false;
        match self.completion.handle_input(ctx) {
            CompletionInput::Confirmed => {
                if let Some(insert_text) = self
                    .completion
                    .selected_item()
                    .map(|item| item.insert_text.as_deref().unwrap_or(&item.label).to_string())
                {
                    self.insert_completion(&insert_text);
                    changed = true;
                }
                self.completion.hide();
                self.completion_auto_armed = None;
            },
            CompletionInput::Dismissed => {
                self.completion_auto_armed = self.cursor_char;
            },
            CompletionInput::Consumed | CompletionInput::None => {},
        }
        changed
    }

    fn show_completion_popup(
        &mut self,
        ui: &mut egui::Ui,
        caret_rect: Option<Rect>,
        editor_rect: Rect,
    ) -> bool {
        if !self.completion.is_visible() {
            return false;
        }
        let anchor = caret_rect.unwrap_or(editor_rect);
        if let Some(insert_text) = self.completion.ui(ui, anchor) {
            self.insert_completion(&insert_text);
            self.completion.hide();
            self.completion_auto_armed = None;
            return true;
        }
        false
    }

    fn retire_widget_state(&mut self) {
        self.retired_state_ids
            .push((self.text_edit_id, self.current_scroll_area_id.take()));
    }

    fn clear_retired_widget_state(&mut self, ctx: &egui::Context) {
        if self.retired_state_ids.is_empty() {
            return;
        }
        let retired = std::mem::take(&mut self.retired_state_ids);
        ctx.data_mut(|data| {
            for (edit_id, scroll_id) in retired {
                data.remove::<TextEditState>(edit_id);
                if let Some(scroll_id) = scroll_id {
                    data.remove::<egui::scroll_area::State>(scroll_id);
                }
            }
        });
    }

    fn reset_ephemeral_state(&mut self) {
        self.viewport_position = None;
        self.narration.reset_widgets();
        self.completion.hide();
        self.completion_anchor = None;
        self.completion_auto_armed = None;
        self.pending_scroll_to_line = None;
        self.highlighted_line = None;
        self.cursor_line = None;
        self.cursor_char = None;
        self.pending_cursor_char = None;
        self.pending_cursor_focus = false;
        self.diagnostics.clear();
        self.find_highlights.clear();
        self.highlight_cache = None;
    }

    fn advance_widget_generation(&mut self) {
        self.state_generation = self.state_generation.wrapping_add(1);
        self.text_edit_id = Id::unique((
            "animatix-source-editor",
            &self.document_path,
            self.instance_id,
            self.state_generation,
        ));
    }
}

fn analyzer_for_path(path: &Path, text: &str) -> Analyzer {
    let mut analyzer = Analyzer::new_with_path(text, Some(path.to_path_buf()));
    analyzer.set_extension_manifest(crate::app::document::extension_manifest_for_path(path));
    analyzer.merge_import_symbols();
    analyzer
}

fn char_to_byte_offset(text: &str, char_offset: usize) -> usize {
    text.char_indices().nth(char_offset).map(|(byte, _)| byte).unwrap_or(text.len())
}

fn char_index_at_line_start(source: &str, line: usize) -> usize {
    let index = animatix_syntax::token::LineIndex::new(source);
    let byte = index.line_col_to_byte(line, 0);
    source[..byte].chars().count()
}
