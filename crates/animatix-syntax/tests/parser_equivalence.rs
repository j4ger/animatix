//! Semantic equivalence checks between the Chumsky semantic parser and the
//! tree-sitter CST converter.
//!
//! The two backends are allowed to differ in position metadata, but the
//! structural AST they feed to the runtime, typechecker, and analyzer must
//! agree. These corpus tests pin that contract.

use animatix_syntax::ast::Stmt;

/// Corpus of high-risk constructs where the two converters previously drifted.
const CORPUS: &[(&str, &str)] = &[
    ("simple actor", "#0s\nbox: Rect, size: (100, 100)\n"),
    ("action duration modifier", "#0s\nfade-in label [500ms]\n"),
    (
        "scene with config and keyframe",
        "# Intro\nconfig { duration: 5.0 }\n#0s\ntitle: Text, text: \"Hi\"\n",
    ),
    (
        "play transition",
        "# Intro\n#0s\ntitle: Text\nplay scenes.FadeIn [fade, 300ms]\n",
    ),
    ("assignment", "#0s\nbox.color = red\n"),
    ("closure", "#0s\nlet f = x => x + 1\n"),
    ("text shorthand", "#0s\nlabel: \"Hello\"\n"),
    ("typst shorthand", "#0s\nlabel: $$Hello$$\n"),
    (
        "for loop with index",
        "#0s\nfor v, i in {1, 2, 3} { box[i]: Rect, size: (10, v) }\n",
    ),
    (
        "for loop tuple pattern",
        "#0s\nfor (x, y) in {(1, 2), (3, 4)} { point: Rect, at: (x, y) }\n",
    ),
    (
        "reactive binding",
        "#0s\nbox: Rect, size: (10, 10)\nalways {\n  box.color := red\n}\n",
    ),
    ("relative keyframe", "#+500ms\nbox.color = blue\n"),
    (
        "sequence",
        "#0s\nsequence {\n  move box [to: (10, 10), 500ms]\n  move box [to: (20, 20), 500ms]\n}\n",
    ),
    (
        "stagger",
        "#0s\nstagger [200ms] {\n  fade-in a [100ms]\n  fade-in b [100ms]\n}\n",
    ),
    (
        "component and action definition",
        "pub component Card(title: Str = \"Untitled\") {\n  action pulse(count: Num = 2) {\n    self.scale = 1.2\n  }\n}\n",
    ),
    (
        "if statement",
        "#0s\nbox: Rect\nif t > 1 {\n  box.color = red\n} else {\n  box.color = blue\n}\n",
    ),
    (
        "method call and if expression",
        "#0s\nbox: Rect, size: (10, 10)\nalways {\n  box.color = if box.alpha() > 0.5 { red } else { blue }\n}\n",
    ),
    (
        "property default with expression",
        "pub component Badge(size: Vec2 = (20, 20), label: Str = \"OK\") {\n  frame: Rect, size: size\n  title: Text, text: label\n}\n",
    ),
    (
        "reactive index assignment",
        "#0s\nfor v, i in {1, 2, 3} {\n  box[i]: Rect, size: (10, v)\n}\nalways {\n  box[i].color := red\n}\n",
    ),
    (
        "match statement without arm commas",
        "#0s\nmatch floor(t) % 2 {\n  0 => { box.color = red }\n  _ => { box.color = blue }\n}\n",
    ),
    (
        "match statement with arm commas",
        "#0s\nmatch floor(t) % 2 {\n  0 => { box.color = red },\n  _ => { box.color = blue },\n}\n",
    ),
    (
        "match expression",
        "#0s\nbox: Rect\nalways {\n  box.color = match floor(t) % 2 {\n    0 => red,\n    _ => blue,\n  }\n}\n",
    ),
    (
        "match expression range",
        "#0s\nbox: Rect\nalways {\n  box.opacity = match t {\n    0..=0 => 1.0,\n    _ => 0.0,\n  }\n}\n",
    ),
    (
        "pub declarations and import",
        "import \"./theme.amx\" as theme\npub let accent = (0.1, 0.2, 0.3)\npub type Accent = Color\npub bars[0]: Rect, size: (10, 10)\n",
    ),
    (
        "config with dotted keys and trailing comma",
        "config {\n  scene.background: (0.02, 0.03, 0.04),\n  duration: 5.0,\n}\n",
    ),
    (
        "nested path assignments",
        "#0s\npanel.child.opacity = 0.5\npanel.child.align := right\n",
    ),
    (
        "multi-scene composition",
        "import \"./scenes.amx\" as scenes\n# Intro\nconfig { duration: 3.0 }\n#0s\ntitle: Text, text: \"Intro\"\nplay scenes.Outro [fade, 300ms]\n# Outro\n#0s\ntitle: Text, text: \"Outro\"\n",
    ),
    (
        "inline children with anonymous and nested actors",
        "#0s\nrow: Row {\n  a: Rect, size: (10, 10),\n  Text, text: \"hello\",\n  nested: Row {\n    b: Rect, size: (5, 5)\n  }\n}\n",
    ),
    (
        "inline for loop with index",
        "#0s\nrow: Row {\n  for item, i in {1, 2, 3} {\n    box[i]: Rect, size: (10, item)\n  }\n}\n",
    ),
    (
        "inline for loop tuple pattern",
        "#0s\nrow: Row {\n  for (x, y) in {(1, 2), (3, 4)} {\n    point: Rect, at: (x, y)\n  }\n}\n",
    ),
    (
        "slot marker and fill",
        "pub component Card {\n  header: Row { @slot }\n}\ncard: Card {\n  @header {\n    title: Text, text: \"Hi\"\n  }\n}\n",
    ),
    (
        "complex expression forms",
        "#0s\nbox: Rect\nalways {\n  let pairs = {(1, 2), (3, 4)}\n  let button = Button { text: \"OK\" }\n  box.color = mix(red, blue, t)\n  box.opacity = clamp(t, 0, 1)\n  box.rotation = -t\n  box.visible = !flag\n  box.scale = 50%\n  box.label = null\n}\n",
    ),
    (
        "action targets paths and indices",
        "#0s\nmove parent.child [300ms]\nfade-in bars[0] [300ms]\n",
    ),
    (
        "match statement complex patterns",
        "#0s\nmatch state {\n  (\"ready\", 0) => { box.color = green }\n  (\"waiting\", 1) | (\"paused\", _) => { box.color = yellow }\n  true => { box.color = white }\n  _ => { box.color = red }\n}\n",
    ),
    (
        "match expression complex patterns",
        "#0s\nbox: Rect\nalways {\n  box.color = match state {\n    (\"ready\", 0) => green,\n    (\"waiting\", 1) | (\"paused\", _) => yellow,\n    _ => red,\n  }\n}\n",
    ),
    (
        "multi-parameter closure and function arguments",
        "#0s\nlet lerp2 = (a, b, t) => a + (b - a) * t\nbox.color = mix(red, blue, 0.5)\n",
    ),
    (
        "if expression and logical operators",
        "#0s\nbox: Rect\nalways {\n  box.visible = t > 0 && t < 1 || flag\n  box.color = if ready { green } else { red }\n}\n",
    ),
    ("operator precedence with power", "#0s\nlet x = a + b * c ^ d\n"),
    (
        "rich type annotations",
        "type P3 = Vec3\ntype Pair = Tuple<Str, Num>\ntype Mapper = Fn(Num, Num) => Num\npub component App(p: Vec3, pair: Tuple<Str, Num>, mapper: Fn(Num, Num) => Num) {}\n",
    ),
    (
        "nested type annotations",
        "type Grid = List<Tuple<Num, Num>>\ntype Handler = Fn(Str) => Tuple<Num, Bool>\ntype Mode = Tuple<Num, Num> | Fn(Num) => Num\n",
    ),
];

#[test]
fn chumsky_and_tree_sitter_are_semantically_equivalent() {
    for (name, source) in CORPUS {
        let parsed_semantic = animatix_syntax::parser::parse_canonical(source);
        let semantic = parsed_semantic
            .statements
            .unwrap_or_else(|| panic!("semantic parser should accept corpus case '{name}'"));
        assert!(
            parsed_semantic.parse_errors.is_empty(),
            "semantic parser errors for '{name}': {:?}",
            parsed_semantic.parse_errors
        );
        let tree_sitter = animatix_syntax::ts_convert::parse_source(source)
            .expect("tree-sitter should parse corpus")
            .statements;

        assert_eq!(
            semantic_signature(&semantic),
            semantic_signature(&tree_sitter),
            "semantic AST mismatch for corpus case '{name}'"
        );
    }
}

/// Remove position metadata from a Debug-formatted AST so structural equality
/// can be compared across parsers.
fn semantic_signature(stmts: &[Stmt]) -> String {
    let debug = format!("{stmts:#?}");
    strip_fields(&debug)
}

fn strip_fields(input: &str) -> String {
    let mut result = input.to_string();
    for field in [
        "trailing_comment: ",
        "value_span: ",
        "byte_span: ",
        "span: ",
    ] {
        let mut out = String::new();
        let mut rest = result.as_str();
        while let Some(pos) = rest.find(field) {
            out.push_str(&rest[..pos]);
            rest = &rest[pos + field.len()..];
            let (consumed, _) = consume_value(rest);
            rest = &rest[consumed..];
        }
        out.push_str(rest);
        result = out;
    }
    strip_span_tuples(&result)
}

/// Remove `Some(Span { ... })` values in tuple fields such as
/// `Stmt::Action(action, Some(span))`.
fn strip_span_tuples(input: &str) -> String {
    let mut out = String::new();
    let mut rest = input;
    while let Some(pos) = rest.find("Some(") {
        out.push_str(&rest[..pos]);
        let bytes = rest.as_bytes();
        let mut depth = 1i32;
        let mut i = pos + "Some(".len();
        while i < bytes.len() {
            match bytes[i] {
                b'(' | b'{' | b'[' => depth += 1,
                b')' | b'}' | b']' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                },
                _ => {},
            }
            i += 1;
        }
        let end = (i + 1).min(rest.len());
        let candidate = &rest[pos..end];
        if candidate.contains("Span {") || candidate.contains("ByteSpan {") {
            out.push_str("None");
            rest = &rest[end..];
        } else {
            out.push_str(candidate);
            rest = &rest[end..];
        }
    }
    out.push_str(rest);
    out
}

/// Consume a single Rust Debug value, balancing nested delimiters. Returns the
/// consumed byte length and the trailing delimiter found.
fn consume_value(input: &str) -> (usize, char) {
    let bytes = input.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && (bytes[i] as char).is_whitespace() {
        i += 1;
    }
    if i >= bytes.len() {
        return (i, '\0');
    }

    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'(' {
        // Constructor like `Some(...)` or `Ok(...)`: scan balanced delimiters.
        let mut depth = 1i32;
        i += 1;
        while i < bytes.len() {
            match bytes[i] {
                b'(' | b'{' | b'[' => depth += 1,
                b')' | b'}' | b']' => {
                    depth -= 1;
                    if depth == 0 {
                        return (i + 1, bytes[i] as char);
                    }
                },
                _ => {},
            }
            i += 1;
        }
        return (bytes.len(), '\0');
    }

    // Scalar: consume up to the next comma or closing brace.
    while i < bytes.len() {
        match bytes[i] {
            b',' | b'}' => return (i, bytes[i] as char),
            _ => i += 1,
        }
    }
    (bytes.len(), '\0')
}
