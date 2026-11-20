use animatix_syntax::ast::{Stmt, Time};
use animatix_syntax::formatter::Formatter;
use animatix_syntax::narration::{
    AnchorEdge, NarrationAnchor, decode_anchor, encode_anchor, is_implicit_anchor, paragraphs,
    phrase_ranges,
};
use animatix_syntax::parser::parse_source;

fn parse(source: &str) -> Vec<Stmt> {
    let (ast, errors) = parse_source(source);
    assert!(errors.is_empty(), "{errors:?}");
    ast.expect("parsed source")
}

#[test]
fn source_paragraphs_preserve_scope_and_utf8_ranges() {
    let source = "# First\r\n  /// 你看，猫。\r\n  /// 还有猫。\r\n\r\n/// 你看猫。还有猫。\r\ncard: Rect // /// small\r\nrow: Row {\r\n  /// nested\r\n  Rect\r\n}\r\n//// note\r\n# Next\r\n/// 你看猫。还有猫。";
    let paragraphs = paragraphs(source);

    assert_eq!(paragraphs.len(), 3);
    assert_eq!(paragraphs[0].text, "你看，猫。 还有猫。");
    assert_eq!(&source[paragraphs[0].span.clone()], "  /// 你看，猫。\r\n  /// 还有猫。\r\n");
    assert_eq!(
        paragraphs[0]
            .content_spans
            .iter()
            .map(|span| &source[span.clone()])
            .collect::<Vec<_>>(),
        ["你看，猫。", "还有猫。"]
    );
    assert_eq!(paragraphs[0].scene.as_deref(), Some("First"));
    assert_eq!(paragraphs[0].scene_span, 0..source.find("# Next").unwrap());
    assert_eq!(paragraphs[1].index, 2);
    assert_eq!(paragraphs[1].occurrence, 2);
    assert_eq!(paragraphs[2].scene.as_deref(), Some("Next"));
    assert_eq!(paragraphs[2].index, 1);
    assert_eq!(paragraphs[2].occurrence, 1);
}

#[test]
fn selectors_distinguish_the_second_phrase_and_keep_source_ranges() {
    let paragraph = "Ａ猫，下一只 A猫。";
    let anchor = NarrationAnchor {
        paragraph: paragraph.into(),
        paragraph_occurrence: 2,
        phrase: Some("a猫".into()),
        phrase_occurrence: 2,
        edge: AnchorEdge::End,
    };
    let marker = encode_anchor(&anchor).expect("second phrase");
    let key = decode_anchor(&marker).expect("decoded compact marker");
    let ranges = phrase_ranges(paragraph, "a猫");

    assert!(!marker.contains("猫"));
    assert_eq!(ranges.len(), 2);
    assert_eq!(&paragraph[ranges[0].clone()], "Ａ猫");
    assert_eq!(&paragraph[ranges[1].clone()], "A猫");
    assert_eq!(key.phrase_range(paragraph), Some(ranges[1].clone()));
    assert_eq!(key.edge(), AnchorEdge::End);
    assert_eq!(key.paragraph_occurrence(), 2);
    assert_eq!(key.phrase_range("Edited paragraph"), None);
    assert!(
        encode_anchor(&NarrationAnchor {
            phrase_occurrence: 3,
            ..anchor
        })
        .is_none()
    );
    assert_eq!(phrase_ranges("哈哈哈", "哈哈"), [0..6, 3..9]);
}

#[test]
fn stale_phrase_suggestions_rank_context_and_keep_other_possible_matches() {
    let original = "开头试读：你好，开始试验。后来又写下：你好，继续计算。";
    let edited = "插播了一段。开头试读：你好，开始试验。后来又写下：你好，回顾结果。";
    let marker = encode_anchor(&NarrationAnchor {
        paragraph: original.into(),
        paragraph_occurrence: 1,
        phrase: Some("你好".into()),
        phrase_occurrence: 2,
        edge: AnchorEdge::End,
    })
    .expect("second phrase");
    let key = decode_anchor(&marker).expect("new selector");
    let expected = phrase_ranges(edited, "你好");

    assert_eq!(key.phrase_range(edited), None, "the anchor still needs a rebind");
    assert_eq!(key.repair_ranges(edited), [expected[1].clone(), expected[0].clone()]);
    assert!(key.repair_ranges("整个句子已经改写了。").is_empty());
    assert_eq!(key.edge(), AnchorEdge::End);

    let (base, _) = marker.rsplit_once('_').expect("new repair hint");
    let legacy = base.replacen("__narration_v2_", "__narration_v1_", 1);
    let legacy_key = decode_anchor(&legacy).expect("legacy selector");
    assert_eq!(legacy_key.phrase_range(original), key.phrase_range(original));
    assert!(legacy_key.repair_ranges(edited).is_empty());
}

#[test]
fn explicit_and_implicit_timing_keep_their_actions_and_spans() {
    let ast = parse(
        "card: Rect\n#1s\n/// Spoken first.\ncard.opacity = 1\n/// Spoken second.\ncard.opacity = 0\n/// Another paragraph.\n#8s\ncard.opacity = 1\n",
    );
    let [_, explicit, first, second, third, after] = ast.as_slice() else {
        panic!("{ast:#?}");
    };
    assert!(
        matches!(explicit, Stmt::Keyframe { time: Time::Seconds(1.0), body, .. } if body.len() == 1)
    );
    assert!(matches!(first, Stmt::Keyframe { body, .. } if body.is_empty()));
    assert!(matches!(second, Stmt::Keyframe {
        time: Time::SpeechMarker { marker, .. }, body, span: Some(span)
    } if is_implicit_anchor(marker) && span.start_line == 5 &&
        matches!(&body[0], Stmt::Assignment { span: Some(span), .. } if span.start_line == 6)));
    assert!(matches!(third, Stmt::Keyframe { body, .. } if body.is_empty()));
    assert!(
        matches!(after, Stmt::Keyframe { time: Time::Seconds(8.0), body, .. } if body.len() == 1)
    );
}

#[test]
fn formatter_preserves_paragraph_boundaries_and_default_timing() {
    let source = "# Intro\ncard:Rect\n/// First paragraph.\n\n/// Second paragraph.\n/// Its second line.\ncard.opacity=1\n";
    let formatter = Formatter::default();
    let ast_only = formatter.format(&parse(source));
    assert_eq!(
        paragraphs(&ast_only).iter().map(|p| p.text.as_str()).collect::<Vec<_>>(),
        ["First paragraph.", "Second paragraph. Its second line."]
    );
    let formatted = formatter.format_source(source, &parse(source));
    assert_eq!(
        paragraphs(&formatted).iter().map(|p| p.text.as_str()).collect::<Vec<_>>(),
        ["First paragraph.", "Second paragraph. Its second line."]
    );
    assert!(!formatted.contains("__narration"));
    let formatted_ast = parse(&formatted);
    assert_eq!(formatted, formatter.format_source(&formatted, &formatted_ast));
    let Stmt::Scene { body, .. } = &formatted_ast[0] else {
        panic!("{formatted_ast:#?}");
    };
    assert!(matches!(&body[2], Stmt::Keyframe {
        time: Time::SpeechMarker { marker, .. }, body, ..
    } if is_implicit_anchor(marker) && body.len() == 1));
}
