use super::*;
use crate::parser::parse_source_complete;

fn roles(source: &str, valid: bool) -> Vec<(&str, &'static str)> {
    let parsed = parse_source_complete(source);
    assert_eq!(
        parsed.errors.is_empty() && parsed.token_issues.is_empty(),
        valid,
        "{:?}",
        parsed.errors
    );
    ranges(&parsed.tokens, &parsed.occurrences, &parsed.headers)
        .into_iter()
        .map(|(a, b, role)| (&source[a..b], role))
        .collect()
}

#[test]
fn modifiers_use_grammar_context_and_keep_ordinary_names() {
    let source = "#0s\nspring: Rect, opacity: 1\n#1s\nspring.opacity = 0.5 [500ms, ease: spring]\n#2s\nspring.opacity = 1 [500ms, ease: spring(6, 9)]";
    let tokens = roles(source, true);
    let springs: Vec<_> = tokens.iter().filter(|(s, _)| *s == "spring").map(|x| x.1).collect();
    assert_eq!(springs, ["label", "label", "enumMember", "label", "enumMember"], "{tokens:?}");
    assert!(tokens.contains(&("ease", "modifier")));
    let tokens = roles("let spring = 3\nlet x = sin(spring)", true);
    assert!(tokens.contains(&("spring", "variable")));
    assert!(tokens.contains(&("sin", "function")));
    let tokens = roles(
        "# Intro\nfade: Rect\nplay Next [fade, 300ms, ease: spring]\n# Next\norb: Rect",
        true,
    );
    assert!(
        tokens.contains(&("fade", "label")) && tokens.contains(&("fade", "enumMember")),
        "{tokens:?}"
    );
    assert!(
        roles("for i in {0,1} [step: 250ms] { dot[i]: Rect }", true)
            .contains(&("step", "modifier"))
    );
}

#[test]
fn arguments_types_and_shorthands_keep_their_roles() {
    let source = "type Count = Num\nfn f(x: List<Num>) -> Count { return 1 }\n#0s\na: \"hi\"\nc: $$x$$\nlet v = 1\nlet k = sin(v)\nlet j = f(x: {1})";
    let tokens = roles(source, true);
    for expected in [
        ("List", "type"),
        ("Num", "type"),
        ("Count", "type"),
        ("a", "label"),
        ("c", "label"),
        ("v", "variable"),
        ("x", "parameter"),
    ] {
        assert!(tokens.contains(&expected), "{expected:?} in {tokens:?}");
    }
    assert!(!tokens.contains(&("v", "parameter")));
}

#[test]
fn preserves_header_roles_comments_and_recovered_call() {
    let source = "fn show() -> Timeline { return #{\n#1s\nlocal: Rect\n} }\n# // 中文🎬\r\n0s\n#cue:mark\n#+2s\n#(1s + 1s)\n#voice@intro+250ms\n#voice[\"hello\", 2]+250ms";
    let tokens = roles(source, true);
    for expected in [
        ("#1s", "keyframe.local"),
        ("#", "keyframe"),
        ("// 中文🎬\r", "comment"),
        ("0s", "keyframe.part"),
        ("#cue:mark", "cue"),
        ("#+2s", "keyframe"),
        ("#(1s + 1s)", "keyframe"),
        ("#voice@intro+250ms", "keyframe"),
        ("#voice[\"hello\", 2]+250ms", "keyframe"),
    ] {
        assert!(tokens.contains(&expected), "{expected:?} in {tokens:?}");
    }
    let tokens = roles("#0s\norb: Rect\nfade_in(target: orb,\n#2s\nnext: Rect", false);
    for expected in [
        ("fade_in", "function"),
        ("target", "parameter"),
        ("orb", "variable"),
        ("#2s", "keyframe"),
        ("next", "label"),
    ] {
        assert!(tokens.contains(&expected), "{expected:?} in {tokens:?}");
    }
}
