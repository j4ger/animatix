use super::*;

pub(super) struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    tokens: Vec<Token>,
    issues: Vec<TokenIssue>,
}

impl<'a> Lexer<'a> {
    pub(super) fn new(src: &'a str) -> Self {
        Lexer {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            tokens: Vec::new(),
            issues: Vec::new(),
        }
    }

    pub(super) fn run(mut self) -> (Vec<Token>, Vec<TokenIssue>) {
        while self.pos < self.bytes.len() {
            if self.skip_ws_and_comments() {
                continue;
            }
            let start = self.pos;
            let kind = self.lex_token();
            self.push(kind, start, self.pos);
        }
        (self.tokens, self.issues)
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
        // Keywords are all-lowercase words; `Fn` stays an identifier so the
        // uppercase type keyword does not collide with the `fn` declaration.
        if text == lower
            && (crate::builtins::KEYWORDS.contains(&lower.as_str())
                || crate::builtins::RESERVED_KEYWORDS.contains(&lower.as_str()))
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
        // Beat literal: number immediately followed by `b`. Checked after `s`
        // and `ms` so no time unit is stolen from another suffix.
        if self.peek() == b'b' {
            self.pos += 1;
            return TokenKind::BeatTime { value };
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
        let open = self.pos;
        self.pos += 1; // opening quote
        let start = self.pos;
        let mut terminated = false;
        while self.pos < self.bytes.len() {
            if self.peek() == quote {
                terminated = true;
                break;
            }
            if self.peek() == b'\\' {
                self.pos += 2; // skip escape
            } else if self.peek() == b'\n' {
                // A newline before the closing quote means the literal was
                // never terminated; stop here so the error stays on one line
                // instead of swallowing the rest of the file.
                break;
            } else {
                self.pos += 1;
            }
        }
        let text = self.src[start..self.pos.min(self.src.len())].to_string();
        if terminated {
            self.pos += 1; // closing quote
        } else {
            self.issues.push(TokenIssue {
                kind: TokenIssueKind::UnterminatedString,
                span: crate::ast::ByteSpan {
                    start: open,
                    end: self.pos,
                },
            });
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
            b'-' if self.peek_n(1) == b'>' => {
                self.pos += 2;
                TokenKind::ThinArrow
            },
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
            b'&' => self.advance(TokenKind::Amp),
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
