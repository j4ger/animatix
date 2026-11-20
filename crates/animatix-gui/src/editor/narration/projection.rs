use std::ops::Range;

use animatix_syntax::narration::{NarrationParagraph, paragraphs};

/// Offset of a character boundary in a projected widget. Egui counts Unicode
/// scalars, while the source and narration index both use UTF-8 byte ranges.
pub(super) fn cursor_to_byte(text: &str, cursor: usize) -> usize {
    text.char_indices().nth(cursor).map_or(text.len(), |(offset, _)| offset)
}

fn one_change<'a>(before: &str, after: &'a str) -> Option<(Range<usize>, &'a str)> {
    if before == after {
        return None;
    }
    let prefix: usize = before
        .chars()
        .zip(after.chars())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum();
    let suffix: usize = before[prefix..]
        .chars()
        .rev()
        .zip(after[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum();
    Some((prefix..before.len() - suffix, &after[prefix..after.len() - suffix]))
}

pub(super) struct ScriptProjection {
    pub text: String,
    lines: Vec<SourceLine>,
    original: Range<usize>,
}

struct SourceLine {
    script: Range<usize>,
    source: Range<usize>,
    prefix: String,
    ending: String,
}

impl ScriptProjection {
    pub fn new(source: &str, paragraph: &NarrationParagraph, previous_end: Option<usize>) -> Self {
        let original = editable_paragraph_span(source, &paragraph.span, previous_end);
        let mut text = String::new();
        let mut source_start = original.start;
        let mut lines = Vec::new();
        for line in source[original.clone()].split_inclusive('\n') {
            if !lines.is_empty() {
                text.push('\n');
            }
            let body = line.trim_end_matches(['\r', '\n']);
            let prefix_len = body.len() - body.trim_start().len() + 3;
            let Some(payload) = body.get(prefix_len..) else {
                break;
            };
            let unspaced = payload.strip_prefix(' ').unwrap_or(payload);
            let prefix_len = body.len() - unspaced.len();
            let script_start = text.len();
            text.push_str(unspaced);
            lines.push(SourceLine {
                script: script_start..text.len(),
                source: source_start + prefix_len..source_start + body.len(),
                prefix: body[..prefix_len].to_owned(),
                ending: line[body.len()..].to_owned(),
            });
            source_start += line.len();
        }
        Self {
            text,
            lines,
            original,
        }
    }

    fn source_position(&self, byte: usize) -> Option<usize> {
        self.lines.iter().find_map(|line| {
            (line.script.start <= byte && byte <= line.script.end)
                .then(|| line.source.start + byte - line.script.start)
        })
    }

    fn script_position(&self, byte: usize) -> Option<usize> {
        self.lines.iter().find_map(|line| {
            (line.source.start <= byte && byte <= line.source.end)
                .then(|| line.script.start + byte - line.source.start)
        })
    }

    pub fn phrase_selection(
        &self,
        paragraph: &NarrationParagraph,
        script_range: Range<usize>,
    ) -> Option<Range<usize>> {
        let start = self.source_position(script_range.start)?;
        let end = self.source_position(script_range.end)?;
        let mut parts = paragraph_text_ranges(paragraph).filter_map(|(plain, span)| {
            let first = start.max(span.start);
            let last = end.min(span.end);
            (first < last)
                .then(|| plain.start + first - span.start..plain.start + last - span.start)
        });
        let first = parts.next()?;
        let end = parts.last().map_or(first.end, |last| last.end);
        Some(first.start..end)
    }

    pub fn script_selection(
        &self,
        paragraph: &NarrationParagraph,
        phrase_range: Range<usize>,
    ) -> Option<Range<usize>> {
        let mut parts = paragraph_text_ranges(paragraph).filter_map(|(plain, span)| {
            let first = phrase_range.start.max(plain.start);
            let last = phrase_range.end.min(plain.end);
            (first < last)
                .then(|| span.start + first - plain.start..span.start + last - plain.start)
        });
        let first = parts.next()?;
        let end = parts.last().map_or(first.end, |last| last.end);
        Some(self.script_position(first.start)?..self.script_position(end)?)
    }

    pub fn edit(&self, after: &str) -> Option<(Range<usize>, String)> {
        let (changed, replacement) = one_change(&self.text, after)?;
        if after.is_empty() {
            return Some((self.original.clone(), String::new()));
        }
        let start = self.source_position(changed.start)?;
        let end = self.source_position(changed.end)?;
        let line = self.lines.iter().find(|line| {
            line.script.contains(&changed.start) || line.script.end == changed.start
        })?;
        let newline = if line.ending.is_empty() {
            "\n"
        } else {
            &line.ending
        };
        let prefix = if line.prefix.ends_with(' ') {
            line.prefix.clone()
        } else {
            format!("{} ", line.prefix)
        };
        let separator = format!("{newline}{prefix}");
        let mut replacement = replacement.replace("\r\n", "\n").replace('\n', &separator);
        if start == line.source.start && replacement.starts_with('/') && !line.prefix.ends_with(' ')
        {
            replacement.insert(0, ' ');
        }
        Some((start..end, replacement))
    }
}

fn editable_paragraph_span(
    source: &str,
    paragraph: &Range<usize>,
    previous_end: Option<usize>,
) -> Range<usize> {
    // Keep newly typed blank script lines visible so the user can type into
    // them. Consecutive empty delimiters following a paragraph belong to it.
    let mut start = paragraph.start;
    while start > 0 && Some(start) != previous_end {
        let before_line = source[..start - 1].rfind('\n').map_or(0, |offset| offset + 1);
        if source[before_line..start].trim() != "///" {
            break;
        }
        start = before_line;
    }
    if Some(start) == previous_end {
        start = paragraph.start;
    }
    let mut end = paragraph.end;
    for line in source[end..].split_inclusive('\n').take_while(|line| line.trim() == "///") {
        end += line.len();
    }
    start..end
}

fn paragraph_text_ranges(
    paragraph: &NarrationParagraph,
) -> impl Iterator<Item = (Range<usize>, &Range<usize>)> {
    paragraph.content_spans.iter().scan(0, |cursor, span| {
        let start = *cursor;
        let end = start + span.len();
        *cursor = end + 1;
        Some((start..end, span))
    })
}

pub(super) struct CodeProjection {
    pub text: String,
    pieces: Vec<(Range<usize>, usize)>,
}

impl CodeProjection {
    pub fn new(source: &str, script: &[ScriptProjection]) -> Self {
        let mut text = String::new();
        let mut pieces = Vec::new();
        let mut start = 0;
        for end in script
            .iter()
            .map(|p| (p.original.start, p.original.end))
            .chain(std::iter::once((source.len(), source.len())))
        {
            let offset = text.len();
            text.push_str(&source[start..end.0]);
            pieces.push((offset..text.len(), start));
            start = end.1;
        }
        Self { text, pieces }
    }

    pub fn edit(
        &self,
        source: &str,
        after: &str,
        old_paragraphs: &[NarrationParagraph],
    ) -> Option<(Range<usize>, String)> {
        let (changed, replacement) = one_change(&self.text, after)?;
        let mut pieces = self
            .pieces
            .iter()
            .filter(|(visible, _)| visible.start <= changed.start && changed.end <= visible.end);
        let (visible, start) = pieces.next()?;
        // An insertion exactly at the seam has two possible source positions.
        if pieces.next().is_some() {
            return None;
        }
        let range = start + changed.start - visible.start..start + changed.end - visible.start;
        let mut candidate = source.to_owned();
        candidate.replace_range(range.clone(), replacement);
        // A change next to a paragraph can turn its first line into an inline
        // comment. Validate that all hidden text still participates in Script.
        let same_paragraphs = paragraphs(&candidate)
            .iter()
            .map(|p| &candidate[p.span.clone()])
            .eq(old_paragraphs.iter().map(|p| &source[p.span.clone()]));
        same_paragraphs.then(|| (range, replacement.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_edit_preserves_hidden_paragraphs_and_rejects_crossing_their_boundary() {
        let source = "#1s\r\n/// 开场\r\n#2s\r\n";
        let paragraphs = paragraphs(source);
        let script = ScriptProjection::new(source, &paragraphs[0], None);
        let view = CodeProjection::new(source, &[script]);
        assert_eq!(view.text, "#1s\r\n#2s\r\n");
        let (range, replacement) = view.edit(source, "#1s\r\n#3s\r\n", &paragraphs).unwrap();
        let mut edited = source.to_owned();
        edited.replace_range(range, &replacement);
        assert_eq!(edited, "#1s\r\n/// 开场\r\n#3s\r\n");
        assert!(view.edit(source, "#3s\r\n", &paragraphs).is_none());
        assert!(view.edit(source, "#1s#2s\r\n", &paragraphs).is_none());
    }

    #[test]
    fn script_edit_preserves_comment_wrappers_and_unrelated_source_bytes() {
        let source = "#1s\r\n  /// 开场 第二次 \r\n  /// 开场\r\n#2s\r\n";
        let paragraphs = paragraphs(source);
        let view = ScriptProjection::new(source, &paragraphs[0], None);
        assert_eq!(view.text, "开场 第二次 \n开场");
        let (range, replacement) = view.edit("开场 第三次 \n开场\n结束").unwrap();
        let mut edited = source.to_owned();
        edited.replace_range(range, &replacement);
        assert_eq!(edited, "#1s\r\n  /// 开场 第三次 \r\n  /// 开场\r\n  /// 结束\r\n#2s\r\n");
    }

    #[test]
    fn enter_leaves_an_editable_blank_script_line() {
        let source = "/// 开场\n#1s\n";
        let paragraph = paragraphs(source).remove(0);
        let view = ScriptProjection::new(source, &paragraph, None);
        let (range, replacement) = view.edit("开场\n").unwrap();
        let mut edited = source.to_owned();
        edited.replace_range(range, &replacement);
        let paragraph = paragraphs(&edited).remove(0);
        let view = ScriptProjection::new(&edited, &paragraph, None);
        assert_eq!(view.text, "开场\n");
        let (range, replacement) = view.edit("开场\n欢迎").unwrap();
        edited.replace_range(range, &replacement);
        assert_eq!(edited, "/// 开场\n/// 欢迎\n#1s\n");
    }
}
