use super::*;

/// Convert a 0-based line/column position to a byte offset in `source`.
pub fn line_col_to_byte(source: &str, line: usize, col: usize) -> usize {
    let mut current_line = 0usize;
    let mut current_col = 0usize;
    let mut byte_offset = 0usize;
    for ch in source.chars() {
        if current_line == line && current_col >= col {
            return byte_offset;
        }
        if ch == '\n' {
            current_line += 1;
            current_col = 0;
        } else {
            current_col += 1;
        }
        byte_offset += ch.len_utf8();
    }
    source.len()
}

/// Convert a byte offset to a 0-based (line, column) pair.
pub fn byte_to_line_col(source: &str, byte: usize) -> (usize, usize) {
    let mut line = 0usize;
    let mut col = 0usize;
    for (i, ch) in source.char_indices() {
        if i >= byte {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Return the token whose byte range contains `byte`, if any.
pub fn token_at_byte(tokens: &[Token], byte: usize) -> Option<&Token> {
    tokens.iter().find(|t| t.span.start <= byte && byte < t.span.end)
}

/// Precomputed line starts for fast byte/position conversion.
pub struct LineIndex<'a> {
    source: &'a str,
    line_starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    /// Build a line index for `source`.
    pub fn new(source: &'a str) -> Self {
        let mut line_starts = vec![0usize];
        for (idx, ch) in source.char_indices() {
            if ch == '\n' {
                line_starts.push(idx + 1);
            }
        }
        Self {
            source,
            line_starts,
        }
    }

    /// Convert a 0-based (line, column) position to a byte offset.
    pub fn line_col_to_byte(&self, line: usize, col: usize) -> usize {
        let Some(start) = self.line_starts.get(line).copied() else {
            return self.source.len();
        };
        let mut offset = start;
        for _ in 0..col {
            let Some(ch) = self.source[offset..].chars().next() else {
                break;
            };
            if ch == '\n' {
                break;
            }
            offset += ch.len_utf8();
        }
        offset
    }

    /// Convert a byte offset to a 0-based (line, column) pair.
    pub fn byte_to_line_col(&self, byte: usize) -> (usize, usize) {
        let byte = byte.min(self.source.len());
        let line = self.line_starts.partition_point(|&start| start <= byte).saturating_sub(1);
        let start = self.line_starts.get(line).copied().unwrap_or(0);
        let col = self.source[start..byte].chars().count();
        (line, col)
    }

    /// Convert a byte offset to a 0-based (line, UTF-16 column) pair.
    ///
    /// LSP positions use UTF-16 code units, so this variant is used at the LSP
    /// boundary rather than the char-counting [`LineIndex::byte_to_line_col`].
    pub fn byte_to_line_col_utf16(&self, byte: usize) -> (usize, usize) {
        let byte = byte.min(self.source.len());
        let line = self.line_starts.partition_point(|&start| start <= byte).saturating_sub(1);
        let start = self.line_starts.get(line).copied().unwrap_or(0);
        let col = self.source[start..byte].encode_utf16().count();
        (line, col)
    }
}
