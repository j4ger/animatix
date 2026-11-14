use super::*;
use animatix_syntax::ast::Span;

// The parser currently leaves ActorDecl.span empty. Synthetic provenance below
// exercises the resolver contract without pretending parser spans exist today.
fn statements(source: &str) -> Vec<Stmt> {
    let mut statements = animatix_syntax::parser::parse_source_complete(source)
        .ast
        .expect("fixture must parse");
    fn add_fixture_spans(statements: &mut [Stmt], line: &mut usize) {
        for stmt in statements {
            match stmt {
                Stmt::ActorDecl { span, .. } => {
                    *span = Some(Span::new(*line, 1, *line, 30));
                    *line += 1;
                },
                Stmt::Scene { body, .. }
                | Stmt::Keyframe { body, .. }
                | Stmt::RelativeKeyframe { body, .. } => add_fixture_spans(body, line),
                _ => {},
            }
        }
    }
    add_fixture_spans(&mut statements, &mut 1);
    statements
}

#[test]
fn unannotated_parser_declarations_do_not_offer_guessed_navigation() {
    let statements = animatix_syntax::parser::parse_source_complete("box: Rect, opacity: 1")
        .ast
        .expect("fixture must parse");
    assert_eq!(source::direct_declaration(&statements, None, "box", "Rect", None), None);
    assert_eq!(
        source::direct_declaration(&statements, None, "box", "Rect", Some(&Span::new(1, 1, 1, 30))),
        None
    );
}

fn declaration_span(statements: &[Stmt]) -> Span {
    match &statements[0] {
        Stmt::ActorDecl {
            span: Some(span), ..
        } => *span,
        _ => panic!("expected actor: {statements:#?}"),
    }
}

#[test]
fn direct_source_requires_unique_matching_runtime_declaration() {
    let stmts = statements("box: Rect, size: (100, 80), opacity: 1");
    let span = declaration_span(&stmts);
    assert_eq!(source::direct_declaration(&stmts, None, "box", "Rect", Some(&span)), Some(span));
    assert_eq!(source::direct_declaration(&stmts, None, "box", "Circle", Some(&span)), None);
    assert_eq!(source::direct_declaration(&stmts, None, "box", "Rect", None), None);
    let mut repeated = stmts.clone();
    repeated.extend(stmts);
    assert_eq!(source::direct_declaration(&repeated, None, "box", "Rect", Some(&span)), None);
}

#[test]
fn source_navigation_does_not_guess_generated_or_imported_origins() {
    let direct = statements("box: Rect, opacity: 1");
    let span = declaration_span(&direct);
    let generated = statements("for i in {0, 1, 2} { box[i]: Rect, opacity: 1 }");
    assert_eq!(
        source::direct_declaration(&generated, None, "box[0]", "Rect", Some(&span)),
        None
    );
    assert_eq!(source::direct_declaration(&direct, None, "imported", "Rect", Some(&span)), None);
    assert_eq!(
        source::direct_declaration(&direct, None, "box", "Rect", Some(&Span::new(9, 1, 9, 30))),
        None
    );
}

#[test]
fn source_navigation_qualifies_scene_and_skips_other_scenes() {
    let stmts = statements("# First\nbox: Rect, opacity: 1\n# Second\nbox: Rect, opacity: 1");
    let Stmt::Scene { body, .. } = &stmts[1] else {
        panic!("expected scene")
    };
    let span = declaration_span(body);
    assert_eq!(
        source::direct_declaration(&stmts, Some("Second"), "box", "Rect", Some(&span)),
        Some(span)
    );
    assert_eq!(
        source::direct_declaration(&stmts, Some("First"), "box", "Rect", Some(&span)),
        None
    );
}

#[test]
fn display_preserves_zero_false_and_empty_values() {
    assert_eq!(read_only::display_value(&PropertyKind::Float(0.0)), "0.000");
    assert_eq!(read_only::display_value(&PropertyKind::Bool(false)), "false");
    assert_eq!(read_only::display_value(&PropertyKind::Text(String::new())), "\"\"");
    assert_eq!(
        read_only::display_value(&PropertyKind::Color([1.0, 0.0, 0.0, 0.0])),
        "rgba(255,0,0,0.00)"
    );
}

#[test]
fn inspector_rendering_is_read_only_and_removes_stale_selection() {
    let document =
        crate::app::test_support::make_document_store("box: Rect, size: (100, 80), opacity: 1");
    let doc = &document.source.document;
    let preview = crate::app::test_support::preview_store(1.0, doc.scene_dimensions);
    let timeline = doc.active_timeline();
    let built = timeline.expect("built fixture");
    let id = built.get_entity_id("box").expect("built box");
    let location = source::parsed_declaration(
        &doc.source_text,
        doc.raw_statements.as_deref().unwrap(),
        doc.active_scene.as_deref(),
        "box",
        built.world().actor_type(id),
        built.world().decl_span(id),
    )
    .expect("real built direct actor must be navigable");
    assert_eq!((location.start_line, location.start_col), (1, 1));

    let mut selected = HashSet::from(["box".to_string(), "deleted".to_string()]);
    let mut commands = ActionQueue::default();
    let mut inspector = InspectorContext {
        preview: &preview.preview,
        timeline,
        composition: None,
        active_scene: None,
        selected_actors: &mut selected,
        commands: &mut commands,
        scene_dimensions: doc.scene_dimensions,
        raw_statements: doc.raw_statements.as_deref(),
        source_path: &doc.file_path,
        source_text: &doc.source_text,
        source_current: true,
    };
    let egui = egui::Context::default();
    egui.run_ui(egui::RawInput::default(), |ui| inspector_panel_ui(&mut inspector, ui))
        .drop_without_applying_deltas();
    assert!(commands.is_empty(), "read-only observation must not enqueue a document edit");
    assert_eq!(selected, HashSet::from(["box".to_string()]));
    assert_eq!(document.source.document.source_text, "box: Rect, size: (100, 80), opacity: 1");
}

#[test]
fn actual_parser_locates_direct_keyframe_actor_and_scene_duplicates() {
    let text = "# First\n#0s\nbox: Rect, opacity: 1\n# Second\n#0s\nbox: Rect, opacity: 1";
    let ast = animatix_syntax::parser::parse_source_complete(text).ast.unwrap();
    let first = source::parsed_declaration(text, &ast, Some("First"), "box", "Rect", None).unwrap();
    let second =
        source::parsed_declaration(text, &ast, Some("Second"), "box", "Rect", None).unwrap();
    assert_eq!((first.start_line, first.start_col), (3, 1));
    assert_eq!((second.start_line, second.start_col), (6, 1));
    assert_eq!(
        source::parsed_declaration(text, &ast, Some("First"), "box", "Circle", None),
        None
    );
    assert_eq!(
        source::parsed_declaration(text, &ast, Some("First"), "box", "Rect", Some(&second)),
        None
    );
}

#[test]
fn actual_parser_locates_relative_keyframes_but_rejects_generated_and_ambiguous_origins() {
    let text = "#0s\n# +1s\nbox: Rect, opacity: 1";
    let ast = animatix_syntax::parser::parse_source_complete(text).ast.unwrap();
    assert_eq!(
        source::parsed_declaration(text, &ast, None, "box", "Rect", None)
            .unwrap()
            .start_line,
        3
    );
    let text = "box: Rect, opacity: 1\nfor i in {0, 1} { box[i]: Rect, opacity: 1 }";
    let ast = animatix_syntax::parser::parse_source_complete(text).ast.unwrap();
    assert_eq!(source::parsed_declaration(text, &ast, None, "box", "Rect", None), None);
    assert_eq!(source::parsed_declaration(text, &ast, None, "box[0]", "Rect", None), None);
}
