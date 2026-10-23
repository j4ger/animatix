use super::*;

mod tests {
    use super::*;

    #[test]
    fn unterminated_string_is_reported_and_does_not_cross_newlines() {
        let source = "a: Text, text: \"oops\nb: Text, text: \"fine\"\n";
        let (tokens, issues) = tokenize_with_issues(source);

        assert_eq!(issues.len(), 1, "one unterminated literal: {issues:?}");
        assert_eq!(issues[0].kind, TokenIssueKind::UnterminatedString);
        // The issue span starts at the opening quote on line 0.
        assert_eq!(issues[0].span.start, source.find('"').unwrap());

        // The newline ends the malformed literal, so line 1 still lexes as its
        // own statement rather than being swallowed into the string.
        let idents: Vec<&str> = tokens
            .iter()
            .filter_map(|t| match &t.kind {
                TokenKind::Ident(name) => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert!(idents.contains(&"a"), "first statement present: {idents:?}");
        assert!(idents.contains(&"b"), "second statement survives: {idents:?}");
    }

    #[test]
    fn terminated_string_reports_no_issue() {
        let (_, issues) = tokenize_with_issues("a: Text, text: \"fine\"\n");
        assert!(issues.is_empty(), "well-formed source has no issues: {issues:?}");
    }

    #[test]
    fn escaped_quote_does_not_end_the_literal() {
        let (tokens, issues) = tokenize_with_issues("a: Text, text: \"say \\\"hi\\\"\"\n");
        assert!(issues.is_empty(), "escaped quotes are fine: {issues:?}");
        assert!(
            tokens.iter().any(|t| matches!(&t.kind, TokenKind::Str(s) if s.contains("say"))),
            "the literal is one token: {tokens:?}"
        );
    }

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
