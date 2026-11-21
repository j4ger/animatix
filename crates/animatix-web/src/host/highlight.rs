use std::collections::HashMap;

use animatix_syntax::{
    highlight::lexical_role,
    parser::parse_source_complete,
    token::{Token, TokenKind},
};

/// Return token ranges in UTF-16 code units for browser text controls.
///
/// The parser supplies identifier roles where available. Its lossless token
/// stream still supplies lexical colors when the program is incomplete.
pub fn highlight_source(source: &str) -> Vec<(usize, usize, &'static str)> {
    let parsed = parse_source_complete(source);
    let roles: HashMap<_, _> = parsed
        .occurrences
        .iter()
        .map(|item| (item.span.start, item.kind.token_role()))
        .collect();
    let mut tokens = parsed.tokens.as_slice();
    let mut ranges = Vec::with_capacity(tokens.len());
    let mut byte = 0;
    let mut utf16 = 0;
    while let Some(token) = tokens.first() {
        let (count, role) = if let Some(count) = keyframe_prefix(tokens) {
            (count, "keyframe")
        } else {
            let role = roles
                .get(&token.span.start)
                .filter(|_| matches!(token.kind, TokenKind::Ident(_)))
                .copied()
                .unwrap_or_else(|| lexical_role(token));
            (1, role)
        };
        let end = tokens[count - 1].span.end;
        let start_utf16 = utf16 + source[byte..token.span.start].encode_utf16().count();
        utf16 = start_utf16 + source[token.span.start..end].encode_utf16().count();
        ranges.push((start_utf16, utf16, role));
        byte = end;
        tokens = &tokens[count..];
    }
    ranges
}

fn keyframe_prefix(tokens: &[Token]) -> Option<usize> {
    use TokenKind::{BeatTime, Hash, Ident, Keyword, Plus, Time};
    match tokens {
        [
            Token { kind: Hash, .. },
            Token {
                kind: Time { .. } | BeatTime { .. },
                ..
            },
            ..,
        ] => Some(2),
        [
            Token { kind: Hash, .. },
            Token { kind: Plus, .. },
            Token {
                kind: Time { .. } | BeatTime { .. },
                ..
            },
            ..,
        ] => Some(3),
        [
            Token { kind: Hash, .. },
            Token {
                kind: Keyword(name) | Ident(name),
                ..
            },
            ..,
        ] if matches!(name.as_str(), "step" | "pause") => Some(2),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::highlight_source;

    #[test]
    fn colors_amx_roles_at_utf16_offsets_with_an_incomplete_tail() {
        let source = "#0s\n  label: Text, text: \"中文🎬\"\n#+2b\n  // 下一拍\n  label.opacity =";
        let units: Vec<_> = source.encode_utf16().collect();
        let marked: Vec<_> = highlight_source(source)
            .into_iter()
            .map(|(from, to, role)| (String::from_utf16(&units[from..to]).unwrap(), role))
            .collect();
        for (text, role) in [
            ("#0s", "keyframe"),
            ("Text", "type"),
            ("text", "property"),
            ("\"中文🎬\"", "string"),
            ("#+2b", "keyframe"),
            ("// 下一拍", "comment"),
            ("=", "operator"),
        ] {
            assert!(marked.iter().any(|(s, r)| s == text && *r == role), "{marked:?}");
        }
    }
}
