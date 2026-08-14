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

/// Structural and reserved keywords recognized by the tokenizer.
pub const KEYWORDS: &[&str] = &[
    "config",
    "import",
    "as",
    "let",
    "pub",
    "type",
    "component",
    "action",
    "sequence",
    "stagger",
    "always",
    "for",
    "in",
    "if",
    "else",
    "match",
    "play",
    // Reserved for future constructs; rejected as identifiers today.
    "loop",
    "yield",
    "stop",
    "pause",
    "resume",
];

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
    /// `$$`
    DollarDollar,
    /// `_`
    Underscore,
    /// End of input.
    Eof,
}

/// A token with its byte range in the source.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// The lexical kind of this token.
    pub kind: TokenKind,
    /// Byte range of this token in the source.
    pub span: ByteSpan,
}

/// Tokenize `source` into a lossless token stream.
///
/// Whitespace is skipped. Tokens are emitted in source order. A trailing
/// [`TokenKind::Eof`] token is always appended. Malformed input is recovered
/// by emitting [`TokenKind::Ident`] for unrecognized runs instead of failing.
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
        self.push(TokenKind::Eof, self.pos, self.pos);
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
        if KEYWORDS.contains(&lower.as_str()) {
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
            b'$' if self.peek_n(1) == b'$' => {
                self.pos += 2;
                TokenKind::DollarDollar
            },
            b'_' => self.advance(TokenKind::Underscore),
            _ => {
                // Unknown byte: recover by consuming one byte as an identifier.
                let start = self.pos;
                self.pos += 1;
                TokenKind::Ident(self.src[start..self.pos].to_string())
            },
        }
    }

    fn advance(&mut self, kind: TokenKind) -> TokenKind {
        self.pos += 1;
        kind
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
                TokenKind::Eof,
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
                TokenKind::Eof,
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
        let tokens = tokenize(":= => == != <= >= && || ..= :: @ @slot $$ _");
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
                TokenKind::DollarDollar,
                TokenKind::Underscore,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn skips_comments() {
        let tokens = tokenize("// hi\nlet x = 1 // tail");
        assert!(matches!(tokens[0].kind, TokenKind::Comment(_)));
        assert!(matches!(tokens.last().unwrap().kind, TokenKind::Comment(_) | TokenKind::Eof));
    }
}
