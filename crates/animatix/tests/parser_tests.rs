use animatix::ast::{Action, Expr, Modifier, Property, Stmt, Time};
use animatix::parser::parser;
use chumsky::Parser;

// Helper function to extract a single statement from the implicit 0s keyframe wrapper
fn parse_single_stmt(src: &str) -> Stmt {
    let ast = parser().parse(src).into_result().unwrap();
    if let Stmt::Keyframe { body, .. } = &ast[0] {
        body[0].clone()
    } else {
        panic!("Expected implicit Keyframe wrapper");
    }
}

#[test]
fn test_let_decl_types() {
    assert_eq!(
        parse_single_stmt("let a = 42"),
        Stmt::LetDecl {
            name: "a".to_string(),
            value: Expr::Num(42.0)
        }
    );
    assert_eq!(
        parse_single_stmt("let b = 3.14"),
        Stmt::LetDecl {
            name: "b".to_string(),
            value: Expr::Num(3.14)
        }
    );
    assert_eq!(
        parse_single_stmt("let c = true"),
        Stmt::LetDecl {
            name: "c".to_string(),
            value: Expr::Bool(true)
        }
    );
    assert_eq!(
        parse_single_stmt("let d = false"),
        Stmt::LetDecl {
            name: "d".to_string(),
            value: Expr::Bool(false)
        }
    );
    assert_eq!(
        parse_single_stmt("let e = null"),
        Stmt::LetDecl {
            name: "e".to_string(),
            value: Expr::Null
        }
    );
    assert_eq!(
        parse_single_stmt("let f = \"string\""),
        Stmt::LetDecl {
            name: "f".to_string(),
            value: Expr::Str("string".to_string())
        }
    );
}

#[test]
fn test_collections() {
    assert_eq!(
        parse_single_stmt("let coords = (10, 20.5)"),
        Stmt::LetDecl {
            name: "coords".to_string(),
            value: Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.5)])
        }
    );
    assert_eq!(
        parse_single_stmt("let arr = {a, b}"),
        Stmt::LetDecl {
            name: "arr".to_string(),
            value: Expr::Tuple(vec![
                Expr::Ident("a".to_string()),
                Expr::Ident("b".to_string())
            ])
        }
    );
}

#[test]
fn test_assignments_and_paths() {
    assert_eq!(
        parse_single_stmt("btn.color = \"red\""),
        Stmt::Assignment {
            target: "btn".to_string(),
            property: "color".to_string(),
            value: Expr::Str("red".to_string())
        }
    );
    assert_eq!(
        parse_single_stmt("let x = container.child"),
        Stmt::LetDecl {
            name: "x".to_string(),
            value: Expr::Path(vec!["container".to_string(), "child".to_string()])
        }
    );
}

#[test]
fn test_actor_decl_full() {
    assert_eq!(
        parse_single_stmt("circle: Circle, radius: 50, color: blue [2s, ease: bounce]"),
        Stmt::ActorDecl {
            label: "circle".to_string(),
            ty: "Circle".to_string(),
            props: vec![
                Property {
                    name: "radius".to_string(),
                    value: Expr::Num(50.0)
                },
                Property {
                    name: "color".to_string(),
                    value: Expr::Ident("blue".to_string())
                }
            ],
            modifiers: vec![
                Modifier {
                    name: None,
                    value: Expr::Ident("2s".to_string())
                },
                Modifier {
                    name: Some("ease".to_string()),
                    value: Expr::Ident("bounce".to_string())
                }
            ],
            children: vec![],
        }
    );
}

#[test]
fn test_action() {
    assert_eq!(
        parse_single_stmt("fade-out ball [1s]"),
        Stmt::Action(Action {
            verb: "fade-out".to_string(),
            targets: vec!["ball".to_string()],
            args: vec![],
            modifiers: vec![Modifier {
                name: None,
                value: Expr::Ident("1s".to_string())
            }],
        })
    );
}

#[test]
fn test_comments() {
    assert_eq!(
        parse_single_stmt("// This is a comment"),
        Stmt::Comment(" This is a comment".to_string())
    );
}

#[test]
fn test_keyframes() {
    let src = r#"
        #500ms
        let x = 1
        #2.5s
        let y = 2
    "#;
    let ast = parser().parse(src).into_result().unwrap();
    assert_eq!(ast.len(), 2);

    if let Stmt::Keyframe { time, body } = &ast[0] {
        assert_eq!(*time, Time::Milliseconds(500));
        assert_eq!(body.len(), 1);
    } else {
        panic!("Expected Keyframe");
    }

    if let Stmt::Keyframe { time, body } = &ast[1] {
        assert_eq!(*time, Time::Seconds(2.5));
        assert_eq!(body.len(), 1);
    } else {
        panic!("Expected Keyframe");
    }
}
