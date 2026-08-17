//! Lossless tokenizer for `.amx` source.
//!
//! This is the single lexical definition for the language. The tokenizer
//! always succeeds (even on malformed input), so it can drive syntax
//! highlighting and cursor-position queries without a full semantic parse.
//! The parser consumes this token stream to build the semantic AST.
//!
//! Keyword and operator spelling lives here and nowhere else, so there is no
//! second grammar to keep in sync.

use crate::ast::ByteSpan;

/// A lexical token kind.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    /// A structural keyword, lowercased.
    Keyword(String),
    /// An identifier (labels, variables, type names, etc.).
    Ident(String),
    /// A numeric literal.
    Number(f64),
    /// A time literal such as `2s` or `500ms`.
    Time {
        /// Numeric magnitude.
        value: f64,
        /// `true` for milliseconds, `false` for seconds.
        ms: bool,
    },
    /// A percentage literal such as `50%`.
    Percent(f64),
    /// A string literal (without quotes).
    Str(String),
    /// A `//` line comment (without the leading `//`).
    Comment(String),
    /// A `$$ ... $$` Typst shorthand body (without the delimiters).
    Typst(String),
    /// A boolean literal.
    Bool(bool),
    /// The `null` literal.
    Null,
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `%` used as a modulo operator.
    PercentOp,
    /// `^`
    Caret,
    /// `==`
    Eq,
    /// `!=`
    Neq,
    /// `<`
    Lt,
    /// `>`
    Gt,
    /// `<=`
    Le,
    /// `>=`
    Ge,
    /// `&&`
    And,
    /// `||`
    Or,
    /// `!`
    Not,
    /// `=`
    Assign,
    /// `:=`
    ReactiveAssign,
    /// `=>`
    Arrow,
    /// `..=`
    RangeInclusive,
    /// `|`
    Pipe,
    /// `::`
    ColonColon,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `:`
    Colon,
    /// `,`
    Comma,
    /// `.`
    Dot,
    /// `#`
    Hash,
    /// `@`
    At,
    /// `@slot`
    AtSlot,
    /// `_`
    Underscore,
}

/// A token with its byte range in the source.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// The lexical kind of this token.
    pub kind: TokenKind,
    /// Byte range of this token in the source.
    pub span: ByteSpan,
}

impl std::fmt::Display for TokenKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenKind::Keyword(s) | TokenKind::Ident(s) => write!(f, "{s}"),
            TokenKind::Number(n) => write!(f, "{n}"),
            TokenKind::Time { value, ms } => write!(f, "{value}{}", if *ms { "ms" } else { "s" }),
            TokenKind::Percent(n) => write!(f, "{n}%"),
            TokenKind::Str(s) => write!(f, "\"{s}\""),
            TokenKind::Comment(_) => write!(f, "comment"),
            TokenKind::Typst(s) => write!(f, "$$ {s} $$"),
            TokenKind::Bool(b) => write!(f, "{b}"),
            TokenKind::Null => write!(f, "null"),
            TokenKind::Plus => write!(f, "+"),
            TokenKind::Minus => write!(f, "-"),
            TokenKind::Star => write!(f, "*"),
            TokenKind::Slash => write!(f, "/"),
            TokenKind::PercentOp => write!(f, "%"),
            TokenKind::Caret => write!(f, "^"),
            TokenKind::Eq => write!(f, "=="),
            TokenKind::Neq => write!(f, "!="),
            TokenKind::Lt => write!(f, "<"),
            TokenKind::Gt => write!(f, ">"),
            TokenKind::Le => write!(f, "<="),
            TokenKind::Ge => write!(f, ">="),
            TokenKind::And => write!(f, "&&"),
            TokenKind::Or => write!(f, "||"),
            TokenKind::Not => write!(f, "!"),
            TokenKind::Assign => write!(f, "="),
            TokenKind::ReactiveAssign => write!(f, ":="),
            TokenKind::Arrow => write!(f, "=>"),
            TokenKind::RangeInclusive => write!(f, "..="),
            TokenKind::Pipe => write!(f, "|"),
            TokenKind::ColonColon => write!(f, "::"),
            TokenKind::LParen => write!(f, "("),
            TokenKind::RParen => write!(f, ")"),
            TokenKind::LBracket => write!(f, "["),
            TokenKind::RBracket => write!(f, "]"),
            TokenKind::LBrace => write!(f, "{{"),
            TokenKind::RBrace => write!(f, "}}"),
            TokenKind::Colon => write!(f, ":"),
            TokenKind::Comma => write!(f, ","),
            TokenKind::Dot => write!(f, "."),
            TokenKind::Hash => write!(f, "#"),
            TokenKind::At => write!(f, "@"),
            TokenKind::AtSlot => write!(f, "@slot"),
            TokenKind::Underscore => write!(f, "_"),
        }
    }
}

impl chumsky::span::Span for ByteSpan {
    type Context = ();
    type Offset = usize;

    fn new(_context: Self::Context, range: std::ops::Range<Self::Offset>) -> Self {
        ByteSpan {
            start: range.start,
            end: range.end,
        }
    }

    fn context(&self) -> Self::Context {}

    fn start(&self) -> Self::Offset {
        self.start
    }

    fn end(&self) -> Self::Offset {
        self.end
    }
}

/// Tokenize `source` into a lossless token stream.
///
/// Whitespace is skipped. Tokens are emitted in source order; the slice's end
/// serves as end-of-input, so no explicit EOF token is produced. Malformed
/// input is recovered by emitting [`TokenKind::Ident`] for unrecognized runs
/// instead of failing.
pub fn tokenize(source: &str) -> Vec<Token> {
    Lexer::new(source).run()
}

struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Lexer {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            tokens: Vec::new(),
        }
    }

    fn run(mut self) -> Vec<Token> {
        while self.pos < self.bytes.len() {
            if self.skip_ws_and_comments() {
                continue;
            }
            let start = self.pos;
            let kind = self.lex_token();
            self.push(kind, start, self.pos);
        }
        self.tokens
    }

    fn skip_ws_and_comments(&mut self) -> bool {
        let mut skipped = false;
        loop {
            // Whitespace
            while self.pos < self.bytes.len() && self.peek().is_ascii_whitespace() {
                self.pos += 1;
                skipped = true;
            }
            // Line comment
            if self.peek() == b'/' && self.peek_n(1) == b'/' {
                let start = self.pos;
                while self.pos < self.bytes.len() && self.peek() != b'\n' {
                    self.pos += 1;
                }
                let text = self.src[start + 2..self.pos].to_string();
                self.push(TokenKind::Comment(text), start, self.pos);
                skipped = true;
                continue;
            }
            break;
        }
        skipped
    }

    fn lex_token(&mut self) -> TokenKind {
        match self.peek() {
            // A lone `_` is the match wildcard; `_foo` and `__` are identifiers.
            b'_' if !self.peek_n(1).is_ascii_alphanumeric() && self.peek_n(1) != b'_' => {
                self.advance(TokenKind::Underscore)
            },
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.lex_ident_or_keyword(),
            b'0'..=b'9' => self.lex_number(),
            b'"' | b'\'' => self.lex_string(),
            _ => self.lex_operator_or_punct(),
        }
    }

    fn lex_ident_or_keyword(&mut self) -> TokenKind {
        let start = self.pos;
        self.consume_ident();
        // Hyphenated identifiers: `fade-in`, `move-to`. The hyphen must be
        // immediately followed by an identifier start with no whitespace, which
        // mirrors the PEG parser's `ident().then(just('-').then(ident()).repeated())`.
        while self.peek() == b'-'
            && (self.peek_n(1).is_ascii_alphabetic() || self.peek_n(1) == b'_')
        {
            self.pos += 1; // '-'
            self.consume_ident();
        }

        let text = &self.src[start..self.pos];
        let lower = text.to_ascii_lowercase();
        if crate::builtins::KEYWORDS.contains(&lower.as_str())
            || crate::builtins::RESERVED_KEYWORDS.contains(&lower.as_str())
        {
            TokenKind::Keyword(lower)
        } else if lower == "true" {
            TokenKind::Bool(true)
        } else if lower == "false" {
            TokenKind::Bool(false)
        } else if lower == "null" {
            TokenKind::Null
        } else {
            TokenKind::Ident(text.to_string())
        }
    }

    fn consume_ident(&mut self) {
        if let Some(c) = self.peek_char() {
            if c.is_ascii_alphabetic() || c == '_' {
                self.pos += 1;
            } else {
                return;
            }
        } else {
            return;
        }
        while let Some(c) = self.peek_char() {
            if c.is_ascii_alphanumeric() || c == '_' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn lex_number(&mut self) -> TokenKind {
        let start = self.pos;
        while self.pos < self.bytes.len() && self.peek().is_ascii_digit() {
            self.pos += 1;
        }
        if self.peek() == b'.' && self.peek_n(1).is_ascii_digit() {
            self.pos += 1;
            while self.pos < self.bytes.len() && self.peek().is_ascii_digit() {
                self.pos += 1;
            }
        }
        let value: f64 = self.src[start..self.pos].parse().unwrap_or(0.0);

        // Time literal: number immediately followed by `s` or `ms`.
        if self.peek() == b'm' && self.peek_n(1) == b's' {
            self.pos += 2;
            return TokenKind::Time { value, ms: true };
        }
        if self.peek() == b's' {
            self.pos += 1;
            return TokenKind::Time { value, ms: false };
        }
        // Percentage literal: number immediately followed by `%`.
        if self.peek() == b'%' {
            self.pos += 1;
            return TokenKind::Percent(value);
        }
        TokenKind::Number(value)
    }

    fn lex_string(&mut self) -> TokenKind {
        let quote = self.peek();
        self.pos += 1; // opening quote
        let start = self.pos;
        while self.pos < self.bytes.len() && self.peek() != quote {
            if self.peek() == b'\\' {
                self.pos += 2; // skip escape
            } else {
                self.pos += 1;
            }
        }
        let text = self.src[start..self.pos].to_string();
        if self.pos < self.bytes.len() {
            self.pos += 1; // closing quote
        }
        TokenKind::Str(text)
    }

    fn lex_operator_or_punct(&mut self) -> TokenKind {
        match self.peek() {
            b'(' => self.advance(TokenKind::LParen),
            b')' => self.advance(TokenKind::RParen),
            b'[' => self.advance(TokenKind::LBracket),
            b']' => self.advance(TokenKind::RBracket),
            b'{' => self.advance(TokenKind::LBrace),
            b'}' => self.advance(TokenKind::RBrace),
            b',' => self.advance(TokenKind::Comma),
            b'.' if self.peek_n(1) == b'.' && self.peek_n(2) == b'=' => {
                self.pos += 3;
                TokenKind::RangeInclusive
            },
            b'.' => self.advance(TokenKind::Dot),
            b':' if self.peek_n(1) == b':' => {
                self.pos += 2;
                TokenKind::ColonColon
            },
            b':' if self.peek_n(1) == b'=' => {
                self.pos += 2;
                TokenKind::ReactiveAssign
            },
            b':' => self.advance(TokenKind::Colon),
            b'=' if self.peek_n(1) == b'=' => {
                self.pos += 2;
                TokenKind::Eq
            },
            b'=' if self.peek_n(1) == b'>' => {
                self.pos += 2;
                TokenKind::Arrow
            },
            b'=' => self.advance(TokenKind::Assign),
            b'+' => self.advance(TokenKind::Plus),
            b'-' => self.advance(TokenKind::Minus),
            b'*' => self.advance(TokenKind::Star),
            b'/' => self.advance(TokenKind::Slash),
            b'%' => self.advance(TokenKind::PercentOp),
            b'^' => self.advance(TokenKind::Caret),
            b'!' if self.peek_n(1) == b'=' => {
                self.pos += 2;
                TokenKind::Neq
            },
            b'!' => self.advance(TokenKind::Not),
            b'<' if self.peek_n(1) == b'=' => {
                self.pos += 2;
                TokenKind::Le
            },
            b'<' => self.advance(TokenKind::Lt),
            b'>' if self.peek_n(1) == b'=' => {
                self.pos += 2;
                TokenKind::Ge
            },
            b'>' => self.advance(TokenKind::Gt),
            b'&' if self.peek_n(1) == b'&' => {
                self.pos += 2;
                TokenKind::And
            },
            b'|' if self.peek_n(1) == b'|' => {
                self.pos += 2;
                TokenKind::Or
            },
            b'|' => self.advance(TokenKind::Pipe),
            b'#' => self.advance(TokenKind::Hash),
            b'@' if self.peek_n(1) == b's'
                && self.peek_n(2) == b'l'
                && self.peek_n(3) == b'o'
                && self.peek_n(4) == b't' =>
            {
                self.pos += 5;
                TokenKind::AtSlot
            },
            b'@' => self.advance(TokenKind::At),
            b'$' if self.peek_n(1) == b'$' => self.lex_typst(),
            b'_' => self.advance(TokenKind::Underscore),
            _ => {
                // Unknown byte: recover by consuming one full UTF-8 character.
                let start = self.pos;
                if let Some(c) = self.peek_char() {
                    self.pos += c.len_utf8();
                } else {
                    self.pos += 1;
                }
                TokenKind::Ident(self.src[start..self.pos].to_string())
            },
        }
    }

    fn advance(&mut self, kind: TokenKind) -> TokenKind {
        self.pos += 1;
        kind
    }

    /// Lex a `$$ ... $$` Typst shorthand body as a single token.
    ///
    /// The returned token carries the trimmed content without the delimiters.
    /// An unterminated `$$` consumes to end of input, matching the parser's
    /// previous raw-text recovery.
    fn lex_typst(&mut self) -> TokenKind {
        self.pos += 2; // opening `$$`
        let content_start = self.pos;
        while self.pos + 1 < self.bytes.len() && !(self.peek() == b'$' && self.peek_n(1) == b'$') {
            self.pos += 1;
        }
        let content = self.src[content_start..self.pos].trim().to_string();
        if self.pos + 1 < self.bytes.len() {
            self.pos += 2; // closing `$$`
        }
        TokenKind::Typst(content)
    }

    fn push(&mut self, kind: TokenKind, start: usize, end: usize) {
        self.tokens.push(Token {
            kind,
            span: ByteSpan { start, end },
        });
    }

    fn peek(&self) -> u8 {
        if self.pos < self.bytes.len() {
            self.bytes[self.pos]
        } else {
            0
        }
    }

    fn peek_n(&self, n: usize) -> u8 {
        let idx = self.pos + n;
        if idx < self.bytes.len() {
            self.bytes[idx]
        } else {
            0
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_keywords_identifiers_and_literals() {
        let tokens = tokenize("config { let x = 42 }");
        let kinds: Vec<TokenKind> = tokens.into_iter().map(|t| t.kind).collect();
        assert_eq!(
            kinds,
            vec![
                TokenKind::Keyword("config".into()),
                TokenKind::LBrace,
                TokenKind::Keyword("let".into()),
                TokenKind::Ident("x".into()),
                TokenKind::Assign,
                TokenKind::Number(42.0),
                TokenKind::RBrace,
            ]
        );
    }

    #[test]
    fn tokenizes_time_percent_and_strings() {
        let tokens = tokenize(r#"2.5s 500ms 50% "hi" true false null"#);
        let kinds: Vec<TokenKind> = tokens.into_iter().map(|t| t.kind).collect();
        assert_eq!(
            kinds,
            vec![
                TokenKind::Time {
                    value: 2.5,
                    ms: false
                },
                TokenKind::Time {
                    value: 500.0,
                    ms: true
                },
                TokenKind::Percent(50.0),
                TokenKind::Str("hi".into()),
                TokenKind::Bool(true),
                TokenKind::Bool(false),
                TokenKind::Null,
            ]
        );
    }

    #[test]
    fn tokenizes_hyphenated_identifiers() {
        let tokens = tokenize("fade-in move-to a - b");
        let idents: Vec<String> = tokens
            .iter()
            .filter_map(|t| match &t.kind {
                TokenKind::Ident(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(idents, vec!["fade-in", "move-to", "a", "b"]);
    }

    #[test]
    fn tokenizes_operators_and_punct() {
        let tokens = tokenize(":= => == != <= >= && || ..= :: @ @slot _");
        let kinds: Vec<TokenKind> = tokens.into_iter().map(|t| t.kind).collect();
        assert_eq!(
            kinds,
            vec![
                TokenKind::ReactiveAssign,
                TokenKind::Arrow,
                TokenKind::Eq,
                TokenKind::Neq,
                TokenKind::Le,
                TokenKind::Ge,
                TokenKind::And,
                TokenKind::Or,
                TokenKind::RangeInclusive,
                TokenKind::ColonColon,
                TokenKind::At,
                TokenKind::AtSlot,
                TokenKind::Underscore,
            ]
        );
    }

    #[test]
    fn tokenizes_typst_shorthand() {
        let tokens = tokenize("$$ x^2 + y^2 $$");
        assert_eq!(tokens[0].kind, TokenKind::Typst("x^2 + y^2".to_string()));
    }

    #[test]
    fn tokenizes_multibyte_without_panicking() {
        let tokens = tokenize("box: Rect, size你: (100, 100)");
        assert!(tokens.iter().any(|t| matches!(&t.kind, TokenKind::Ident(s) if s == "你")));
    }

    #[test]
    fn skips_comments() {
        let tokens = tokenize("// hi\nlet x = 1 // tail");
        assert!(matches!(tokens[0].kind, TokenKind::Comment(_)));
        assert!(matches!(tokens.last().unwrap().kind, TokenKind::Comment(_)));
    }
}

#[cfg(test)]
mod line_index_tests {
    use super::*;

    #[test]
    fn utf16_columns_count_code_units() {
        let idx = LineIndex::new("a你b\ncd");
        // '你' is 3 bytes and one UTF-16 code unit.
        assert_eq!(idx.byte_to_line_col_utf16("a你".len()), (0, 2));
        assert_eq!(idx.byte_to_line_col_utf16("a你b\n".len()), (1, 0));
    }
}
