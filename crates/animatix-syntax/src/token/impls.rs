use super::*;

impl std::fmt::Display for TokenKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenKind::Keyword(s) | TokenKind::Ident(s) => write!(f, "{s}"),
            TokenKind::Number(n) => write!(f, "{n}"),
            TokenKind::Time { value, ms } => write!(f, "{value}{}", if *ms { "ms" } else { "s" }),
            TokenKind::BeatTime { value } => write!(f, "{value}b"),
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
            TokenKind::ThinArrow => write!(f, "->"),
            TokenKind::RangeInclusive => write!(f, "..="),
            TokenKind::Pipe => write!(f, "|"),
            TokenKind::Amp => write!(f, "&"),
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
