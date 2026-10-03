//! A scene's `config` block must reach `Scene.config` whatever precedes it.
//!
//! `group_scenes` used to promote a `config { … }` into the scene only while the
//! scene was still empty, so one statement in front of it — most commonly an
//! `import`, which belongs to no keyframe — pushed the block down into the scene
//! *body*. `Scene.config` was then empty, and the composition fell back to
//! deriving the duration from keyframe spans (`composition/build.rs`), which is
//! what made the web player's clock wrap at the wrong time for scenes that
//! declare `duration:` longer than their last keyframe.

use animatix_syntax::ast::{Expr, Stmt};

fn parse(src: &str) -> Vec<Stmt> {
    let (stmts, errors) = animatix_syntax::parser::parse_source(src);
    assert!(
        errors.is_empty(),
        "parse errors: {:?}",
        errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
    stmts.expect("no statements")
}

fn scene_named<'a>(stmts: &'a [Stmt], name: &str) -> &'a Stmt {
    stmts
        .iter()
        .find(|stmt| matches!(stmt, Stmt::Scene { name: scene, .. } if scene == name))
        .unwrap_or_else(|| panic!("no scene {name:?} in {stmts:#?}"))
}

fn setting<'a>(scene: &'a Stmt, key: &str) -> Option<&'a Expr> {
    match scene {
        Stmt::Scene { config, .. } => config.iter().find(|p| p.name == key).map(|p| &p.value),
        _ => None,
    }
}

/// The shape that broke: an `import` first, then the scene's config block.
#[test]
fn a_config_after_an_import_still_belongs_to_the_scene() {
    let stmts = parse(
        "\
# Intro
import \"./lib/tokens.amx\" as tk
config { resolution: (400, 300), duration: 4 }
b: Rect, size: (20, 20), color: (1.0, 0.5, 0.2, 1.0), at: (10, 10)
#1s
b.opacity = 1.0
",
    );
    let scene = scene_named(&stmts, "Intro");
    assert_eq!(
        setting(scene, "duration"),
        Some(&Expr::Num(4.0)),
        "the scene's authored duration never reached Scene.config"
    );
    assert!(setting(scene, "resolution").is_some(), "resolution was lost with it");
    match scene {
        Stmt::Scene { body, .. } => assert!(
            !body.iter().any(|s| matches!(s, Stmt::Config { .. })),
            "a config block was left behind in the scene body"
        ),
        _ => unreachable!(),
    }
}

/// The empty-scene case that already worked must keep working, and a config
/// arriving after real content is still the scene's configuration — position in
/// the scene says nothing about which block owns it.
#[test]
fn a_config_anywhere_in_a_scene_is_promoted() {
    let stmts = parse(
        "\
# Intro
config { resolution: (400, 300) }
b: Rect, size: (20, 20), color: (1.0, 0.5, 0.2, 1.0), at: (10, 10)
#1s
b.opacity = 1.0
config { duration: 6 }
",
    );
    let scene = scene_named(&stmts, "Intro");
    assert_eq!(setting(scene, "duration"), Some(&Expr::Num(6.0)));
    assert!(setting(scene, "resolution").is_some(), "the first block was lost");
}

/// A config in no scene at all is document-level and stays where it is.
#[test]
fn a_leading_config_outside_any_scene_is_untouched() {
    let stmts = parse(
        "config { resolution: (400, 300), duration: 2 }\nb: Rect, size: (20, 20), color: (1.0, 0.5, 0.2, 1.0), at: (10, 10)\n",
    );
    assert!(
        stmts.iter().any(|s| matches!(s, Stmt::Config { .. })),
        "the document-level config block disappeared: {stmts:#?}"
    );
    assert!(
        !stmts.iter().any(|s| matches!(s, Stmt::Scene { .. })),
        "a config block invented a scene"
    );
}
