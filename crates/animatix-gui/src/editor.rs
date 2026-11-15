//! Single-buffer `.amx` source editor with diagnostics, navigation and completion.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use animatix_analyzer::Analyzer;
use egui::text::LayoutJob;
use egui::text::{CCursor, CCursorRange};
use egui::text_edit::TextEditState;
use egui::{Align, Id, Key, Pos2, Rect, Response, ScrollArea, Vec2};

use crate::app::design_tokens::typography::TextRole;
use crate::completion_popup::{CompletionInput, CompletionPopup};
use crate::editor::completion::CompletionAnchor;
use crate::highlighting::{FindHighlight, HighlightCache, HighlightDiagnostic, highlight_source};

mod completion;
mod diagnostics;
mod find_match;
#[cfg(test)]
mod tests;

static NEXT_EDITOR_INSTANCE: AtomicU64 = AtomicU64::new(1);

pub struct EditorBuffer {
    text: String,
    document_path: PathBuf,
    analyzer: Analyzer,
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
    state_generation: u64,
    text_edit_id: Id,
    retired_state_ids: Vec<(Id, Option<Id>)>,
    current_scroll_area_id: Option<Id>,
}

impl EditorBuffer {
    pub fn new(path: &Path, text: String) -> Self {
        let instance_id = NEXT_EDITOR_INSTANCE.fetch_add(1, Ordering::Relaxed);
        let text_edit_id = Id::unique(("animatix-source-editor", path, instance_id, 0_u64));
        Self {
            analyzer: analyzer_for_path(path, &text),
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
            state_generation: 0,
            text_edit_id,
            retired_state_ids: Vec::new(),
            current_scroll_area_id: None,
        }
    }

    /// Replace the active document and isolate its caret/undo state from the
    /// previously open file.
    pub fn set_document(&mut self, path: &Path, text: String) {
        self.retire_widget_state();
        self.document_path = path.to_path_buf();
        self.text = text;
        self.analyzer = analyzer_for_path(path, &self.text);
        self.reset_ephemeral_state();
        self.advance_widget_generation();
    }

    /// Replace source from an external operation such as undo, reload or
    /// Find/Replace All. Normal keystrokes never call this and retain egui's
    /// native undo and IME composition state.
    pub fn replace_text(&mut self, text: String) {
        self.retire_widget_state();
        self.text = text;
        self.analyzer.update(&self.text);
        self.reset_ephemeral_state();
        self.advance_widget_generation();
    }

    pub fn set_extension_manifest(&mut self, manifest: animatix_analyzer::ExtensionManifest) {
        self.analyzer.set_extension_manifest(manifest);
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn document_path(&self) -> &Path {
        &self.document_path
    }

    pub fn scroll_to_line(&mut self, line: usize) {
        self.pending_scroll_to_line = Some(line);
    }

    /// Focus the source location and move the caret to the end of its token.
    /// Line and column are zero-based Unicode-scalar positions.
    pub fn focus_diagnostic(&mut self, line: usize, column: usize) {
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
        &self.analyzer
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

    /// Render one ordinary multiline text field. The outer ScrollArea owns
    /// both axes so long source lines and long documents remain navigable.
    pub fn show(&mut self, ui: &mut egui::Ui) -> Response {
        self.clear_retired_widget_state(ui.ctx());
        let mut completion_changed = self.process_completion_input(ui.ctx());
        self.ensure_highlight_cache(ui.style());
        let cache = self.highlight_cache.as_ref().expect("highlight cache prepared");
        let cache_source = cache.source.as_str();
        let cache_job = &cache.job;
        let line_count = cache.line_count;
        let max_line_chars = cache.max_line_chars;
        let edit_id = self.text_edit_id;
        let scroll_salt = ("source-editor-scroll", self.instance_id, self.state_generation);
        let diagnostics = self.diagnostics.clone();
        let finds = self.find_highlights.clone();
        let highlighted_line = self.highlighted_line;
        let style = ui.style().clone();
        let font_id = TextRole::Mono.font_id();
        let line_height = ui.fonts_mut(|fonts| fonts.row_height(&font_id));
        let mut laid_out_edit: Option<(String, LayoutJob)> = None;
        let mut edit_caret_rect = None;
        let pending_line = self.pending_scroll_to_line.take();
        let pending_cursor = self.pending_cursor_char.take();
        let should_focus = std::mem::take(&mut self.pending_cursor_focus);
        let mut edit_state = TextEditState::load(ui.ctx(), edit_id).unwrap_or_default();
        if let Some(char_idx) = pending_cursor {
            edit_state
                .cursor
                .set_char_range(Some(CCursorRange::one(CCursor::new(char_idx))));
        }
        edit_state.store(ui.ctx(), edit_id);

        let available = ui.available_size_before_wrap();
        let char_width = ui.fonts_mut(|fonts| fonts.glyph_width(&TextRole::Mono.font_id(), '0'));
        let content_width = (max_line_chars as f32 * char_width + 24.0).max(available.x);
        let content_height = (line_count as f32 * line_height + 8.0).max(available.y);
        let scroll = ScrollArea::both().id_salt(scroll_salt).auto_shrink([false, false]).show(
            ui,
            |scroll_ui| {
                scroll_ui
                    .allocate_ui_with_layout(
                        Vec2::new(content_width, content_height),
                        egui::Layout::top_down(Align::Min),
                        |edit_ui| {
                            let mut layouter =
                                |layouter_ui: &egui::Ui,
                                 buffer: &dyn egui::TextBuffer,
                                 wrap_width: f32| {
                                    let source = buffer.as_str();
                                    let mut job = if source == cache_source {
                                        cache_job.clone()
                                    } else {
                                        let job = highlight_source(
                                            source,
                                            &style,
                                            &diagnostics,
                                            highlighted_line,
                                            &finds,
                                        );
                                        laid_out_edit = Some((source.to_owned(), job.clone()));
                                        job
                                    };
                                    for section in &mut job.sections {
                                        section.format.line_height = Some(line_height);
                                    }
                                    job.wrap.max_width = wrap_width;
                                    layouter_ui.fonts_mut(|fonts| fonts.layout_job(job))
                                };
                            let output = egui::TextEdit::multiline(&mut self.text)
                                .id(edit_id)
                                .code_editor()
                                .desired_width(content_width)
                                .desired_rows(cache.line_count.max(1))
                                .frame(egui::Frame::NONE)
                                .margin(0.0)
                                .layouter(&mut layouter)
                                .show(edit_ui);
                            let cursor = output.cursor_range.map(|range| range.primary);
                            edit_caret_rect = cursor.map(|cursor| {
                                output
                                    .galley
                                    .pos_from_cursor(cursor)
                                    .translate(output.galley_pos.to_vec2())
                            });
                            if let Some(line) = pending_line {
                                let char_index = char_index_at_line_start(&self.text, line);
                                let target = output
                                    .galley
                                    .pos_from_cursor(CCursor::new(char_index))
                                    .translate(output.galley_pos.to_vec2());
                                let viewport = edit_ui.clip_rect();
                                edit_ui.scroll_to_rect(
                                    Rect::from_min_max(
                                        Pos2::new(viewport.left(), target.top()),
                                        Pos2::new(viewport.right(), target.bottom()),
                                    ),
                                    Some(Align::Center),
                                );
                            }
                            output
                        },
                    )
                    .inner
            },
        );
        self.current_scroll_area_id = Some(scroll.id);
        let output = scroll.inner;
        let mut response = output.response.response.clone();

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
        let source_changed = response.changed() || self.text != cache_source;
        if source_changed {
            self.after_source_replaced();
            response.mark_changed();
            if let Some((source, job)) = laid_out_edit
                && source == self.text
            {
                self.cache_layout_for_current_source(job, ui.style());
            }
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
        self.analyzer.update(&self.text);
        self.highlight_cache = None;
        self.completion_anchor = None;
        self.completion_auto_armed = None;
    }

    fn ensure_highlight_cache(&mut self, style: &egui::Style) {
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
        let job = highlight_source(
            &self.text,
            style,
            &self.diagnostics,
            self.highlighted_line,
            &self.find_highlights,
        );
        self.cache_layout_for_current_source(job, style);
    }

    fn cache_layout_for_current_source(&mut self, job: LayoutJob, style: &egui::Style) {
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
