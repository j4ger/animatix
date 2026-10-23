use super::lexer::Lexer;
use super::*;

/// A lexical problem that does not stop tokenization but changes how the
/// remaining source is read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenIssue {
    /// What went wrong.
    pub kind: TokenIssueKind,
    /// Byte span of the malformed token.
    pub span: ByteSpan,
}

/// The kind of lexical problem in a [`TokenIssue`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenIssueKind {
    /// A string literal opened with a quote is never closed. The lexer
    /// swallows the rest of the line (and, because it stops only at the quote
    /// or end of input, potentially the rest of the file), so the parser then
    /// reports unrelated errors further down.
    UnterminatedString,
}

impl std::fmt::Display for TokenIssueKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenIssueKind::UnterminatedString => {
                write!(f, "unterminated string literal (missing closing quote)")
            },
        }
    }
}

/// Tokenize `source`, also returning any lexical issues.
pub fn tokenize_with_issues(source: &str) -> (Vec<Token>, Vec<TokenIssue>) {
    Lexer::new(source).run()
}
