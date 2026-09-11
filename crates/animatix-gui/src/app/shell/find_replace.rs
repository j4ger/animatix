//! Find / Replace dialog for the source editor.
//!
//! Supports case-sensitive, whole-word and regular-expression search. Literal
//! queries are escaped so a replacement never interprets `$`.

use crate::app::GuiShell;
use crate::app::commands::UndoLabel;
use crate::app::components::button::Button;
use crate::app::components::dialog;
use crate::app::design_tokens::typography::TextRole;

/// Build the search regex for the current options.
pub(crate) fn build_pattern(
    query: &str,
    case_sensitive: bool,
    whole_word: bool,
    is_regex: bool,
) -> Result<regex::Regex, String> {
    let mut pattern = if is_regex {
        query.to_string()
    } else {
        regex::escape(query)
    };
    if whole_word {
        pattern = format!(r"\b(?:{pattern})\b");
    }
    if !case_sensitive {
        pattern = format!("(?i){pattern}");
    }
    regex::Regex::new(&pattern).map_err(|err| err.to_string())
}

impl GuiShell {
    pub(crate) fn find_replace_ui(&mut self, ui: &mut egui::Ui) {
        let theme = eparts::theme(ui);
        let sp = crate::app::design_tokens::spatial::spatial(ui);

        let spec =
            dialog::DialogSpec::new("find_replace", [460.0, 220.0]).with_min_size([400.0, 200.0]);

        let open = dialog::modal(ui, &spec, |ui, _dc| -> bool {
            let close = dialog::title_row(ui, "Find & Replace");
            ui.add_space(sp.base.space_3);
            ui.separator();
            ui.add_space(sp.base.space_3);

            ui.label(
                egui::RichText::new("Find")
                    .size(TextRole::BodyS.size())
                    .color(theme.palette.text.secondary),
            );
            let find_resp = ui.add(
                egui::TextEdit::singleline(&mut self.ui_store.find_query)
                    .desired_width(f32::INFINITY)
                    .hint_text("Search term…"),
            );
            // Reset the cursor-relative position whenever the query changes.
            if find_resp.changed() {
                self.ui_store.find_last_match = None;
            }

            // Options
            ui.add_space(sp.base.space_2);
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.ui_store.find_case_sensitive, "Case");
                ui.checkbox(&mut self.ui_store.find_whole_word, "Word");
                ui.checkbox(&mut self.ui_store.find_regex, "Regex");

                // Live match count / pattern error, right-aligned.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.ui_store.find_query.is_empty() {
                        return;
                    }
                    match build_pattern(
                        &self.ui_store.find_query,
                        self.ui_store.find_case_sensitive,
                        self.ui_store.find_whole_word,
                        self.ui_store.find_regex,
                    ) {
                        Ok(re) => {
                            let count =
                                re.find_iter(self.document_store.source.editor.text()).count();
                            ui.label(
                                egui::RichText::new(format!("{count} match(es)"))
                                    .size(TextRole::Caption.size())
                                    .color(if count == 0 {
                                        theme.palette.status.warning
                                    } else {
                                        theme.palette.text.muted
                                    }),
                            );
                        },
                        Err(err) => {
                            ui.label(
                                egui::RichText::new(format!("Invalid pattern: {err}"))
                                    .size(TextRole::Caption.size())
                                    .color(theme.palette.status.error),
                            );
                        },
                    }
                });
            });
            ui.add_space(sp.base.space_2);

            ui.label(
                egui::RichText::new("Replace with")
                    .size(TextRole::BodyS.size())
                    .color(theme.palette.text.secondary),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.ui_store.replace_query)
                    .desired_width(f32::INFINITY)
                    .hint_text("Replacement…"),
            );
            ui.add_space(sp.base.space_3);

            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let replace_all =
                        ui.add_sized([100.0, sp.base.row_m], Button::primary("Replace All"));
                    if replace_all.clicked() {
                        self.perform_find_replace_all();
                    }

                    let find_next = ui.add_sized([90.0, sp.base.row_m], Button::ghost("Find Next"));
                    if find_next.clicked() {
                        self.find_next_in_editor();
                    }
                });
            });

            close
        });

        if !open {
            self.ui_store.view.find_replace_open = false;
        }
    }

    fn current_find_pattern(&self) -> Result<regex::Regex, String> {
        build_pattern(
            &self.ui_store.find_query,
            self.ui_store.find_case_sensitive,
            self.ui_store.find_whole_word,
            self.ui_store.find_regex,
        )
    }

    fn perform_find_replace_all(&mut self) {
        if self.ui_store.find_query.is_empty() {
            self.preview_store.preview.status = "Find query is empty".to_string();
            return;
        }
        let re = match self.current_find_pattern() {
            Ok(re) => re,
            Err(err) => {
                self.preview_store.preview.status = format!("Invalid pattern: {err}");
                return;
            },
        };

        let text = self.document_store.source.editor.text().to_string();
        let count = re.find_iter(&text).count();
        if count == 0 {
            self.preview_store.preview.status = "No matches found".to_string();
            return;
        }

        let replace = self.ui_store.replace_query.clone();
        let new_text = if self.ui_store.find_regex {
            re.replace_all(&text, replace.as_str()).to_string()
        } else {
            // Literal replacement: never interpret `$1` in the replacement text.
            re.replace_all(&text, regex::NoExpand(&replace)).to_string()
        };

        let ui_before = self.ui_store.snapshot_with_preview(&self.preview_store);
        self.document_store.snapshot(UndoLabel::FindReplaceAll, ui_before);
        let ui_after = self.ui_store.snapshot_with_preview(&self.preview_store);
        self.document_store.replace_text_with_ui(new_text, ui_after);
        self.document_store.source.document.raw_statements = None;
        self.document_store.source.document.expanded_statements = None;
        self.preview_store.pending_rebuild_at = Some(
            std::time::Instant::now()
                + std::time::Duration::from_millis(self.ui_store.rebuild_debounce_ms),
        );
        self.ui_store.find_last_match = None;
        self.preview_store.preview.status = format!("Replaced {} occurrence(s)", count);
    }

    fn find_next_in_editor(&mut self) {
        if self.ui_store.find_query.is_empty() {
            self.preview_store.preview.status = "Find query is empty".to_string();
            return;
        }
        let re = match self.current_find_pattern() {
            Ok(re) => re,
            Err(err) => {
                self.preview_store.preview.status = format!("Invalid pattern: {err}");
                return;
            },
        };

        let text = self.document_store.source.editor.text().to_string();
        let len = text.len();
        // `find_last_match` holds the end of the previous match (a char
        // boundary), so searching from it advances past the last hit.
        let start = self.ui_store.find_last_match.unwrap_or(0).min(len);
        let found = re.find_at(&text, start).or_else(|| re.find(&text));

        match found {
            Some(m) => {
                self.ui_store.find_last_match = Some(m.end());
                let (line, _col) = self.document_store.source.editor.byte_to_line_col(m.start());
                self.document_store.source.editor.scroll_to_line(line);
                self.preview_store.preview.status = format!("Found at line {}", line + 1);
            },
            None => {
                self.ui_store.find_last_match = None;
                self.preview_store.preview.status = "No matches found".to_string();
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::build_pattern;

    #[test]
    fn literal_query_is_escaped() {
        let re = build_pattern("a.b", true, false, false).expect("valid");
        assert!(re.is_match("a.b"));
        assert!(!re.is_match("axb"), "literal '.' must not act as a wildcard");
    }

    #[test]
    fn case_insensitive_by_default() {
        let re = build_pattern("Hello", false, false, false).expect("valid");
        assert!(re.is_match("hello"));
        let sensitive = build_pattern("Hello", true, false, false).expect("valid");
        assert!(!sensitive.is_match("hello"));
    }

    #[test]
    fn whole_word_bounds_matches() {
        let re = build_pattern("cat", true, true, false).expect("valid");
        assert!(re.is_match("a cat here"));
        assert!(!re.is_match("concatenate"));
    }

    #[test]
    fn regex_mode_enables_pattern_syntax() {
        let re = build_pattern(r"a\d+", true, false, true).expect("valid");
        assert!(re.is_match("a123"));
        assert!(!re.is_match("abc"));
    }

    #[test]
    fn invalid_regex_reports_error() {
        assert!(build_pattern("(", true, false, true).is_err());
    }
}
