use super::*;

/// Source location span for AST nodes.
/// Used for editor navigation, diagnostics, and bidirectional sync.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Span {
    /// Start line number (1-indexed).
    pub start_line: usize,
    /// Start column number (1-indexed).
    pub start_col: usize,
    /// End line number (1-indexed).
    pub end_line: usize,
    /// End column number (1-indexed).
    pub end_col: usize,
}

/// Byte-offset range into source text.
///
/// Used during parsing where only byte offsets are available (e.g. from chumsky
/// spans). Convert to [`Span`] via [`Span::from_byte_span`] for editor-facing
/// positions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ByteSpan {
    /// Start byte offset into the source text.
    pub start: usize,
    /// End byte offset into the source text.
    pub end: usize,
}

impl Span {
    /// Create a new span with the given line and column bounds.
    pub fn new(start_line: usize, start_col: usize, end_line: usize, end_col: usize) -> Self {
        Self {
            start_line,
            start_col,
            end_line,
            end_col,
        }
    }

    /// Convert a `ByteSpan` to a `Span` given the source text.
    pub fn from_byte_span(source: &str, byte_span: ByteSpan) -> Self {
        Self::from_range(source, byte_span.start..byte_span.end)
    }

    /// Convert a byte-offset range to a `Span` given the source text.
    pub fn from_range(source: &str, range: std::ops::Range<usize>) -> Self {
        let mut line = 1;
        let mut col = 1;
        let mut start_line = 1;
        let mut start_col = 1;
        let mut end_line = 1;
        let mut end_col = 1;

        for (i, ch) in source.char_indices() {
            if i == range.start {
                start_line = line;
                start_col = col;
            }
            if i == range.end {
                end_line = line;
                end_col = col;
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }

        // Handle edge case where end is at EOF
        if range.end == source.len() {
            end_line = line;
            end_col = col;
        }

        Self {
            start_line,
            start_col,
            end_line,
            end_col,
        }
    }
}
