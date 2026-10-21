use super::defs::*;
use super::shapes::*;
use super::transform::*;
use super::*;
// The path-data parser moved to an ungated module (so the `slim` web build can
// still compile `Path, icon:`); the importer's tests exercise its helpers too.
use crate::timeline::path_data::{parse_svg_path_data, tokenize_path_data};

#[test]
fn test_parse_hex_color() {
    let expr = parse_svg_color("#ff0000", None).unwrap();
    assert_eq!(
        expr,
        Expr::Call("rgb".into(), vec![Expr::Num(255.0), Expr::Num(0.0), Expr::Num(0.0)])
    );
}

#[test]
fn test_parse_short_hex_color() {
    let expr = parse_svg_color("#f00", None).unwrap();
    assert_eq!(
        expr,
        Expr::Call("rgb".into(), vec![Expr::Num(255.0), Expr::Num(0.0), Expr::Num(0.0)])
    );
}

#[test]
fn test_parse_named_color() {
    let expr = parse_svg_color("red", None).unwrap();
    assert_eq!(expr, Expr::Ident("red".into()));
}

#[test]
fn test_parse_none_color() {
    let expr = parse_svg_color("none", None);
    assert!(expr.is_none());
}

#[test]
fn test_parse_rgb_color() {
    let expr = parse_svg_color("rgb(255, 0, 0)", None).unwrap();
    assert_eq!(
        expr,
        Expr::Call("rgb".into(), vec![Expr::Num(255.0), Expr::Num(0.0), Expr::Num(0.0)])
    );
}

#[test]
fn test_parse_transform_translate() {
    let t = parse_transform("translate(10, 20)");
    assert!((t.tx - 10.0).abs() < 1e-6);
    assert!((t.ty - 20.0).abs() < 1e-6);
}

#[test]
fn test_parse_transform_rotate() {
    let t = parse_transform("rotate(45)");
    assert!((t.rotation_deg - 45.0).abs() < 1e-6);
}

#[test]
fn test_parse_transform_scale() {
    let t = parse_transform("scale(2)");
    assert!((t.sx - 2.0).abs() < 1e-6);
    assert!((t.sy - 2.0).abs() < 1e-6);
}

#[test]
fn test_parse_transform_combined() {
    let t = parse_transform("translate(5, 10) rotate(30) scale(1.5)");
    assert!((t.tx - 5.0).abs() < 1e-6);
    assert!((t.ty - 10.0).abs() < 1e-6);
    assert!((t.rotation_deg - 30.0).abs() < 1e-6);
    assert!((t.sx - 1.5).abs() < 1e-6);
    assert!((t.sy - 1.5).abs() < 1e-6);
}

#[test]
fn test_tokenize_path_data() {
    let tokens = tokenize_path_data("M10 20 L30 40");
    assert_eq!(tokens, vec!["M", "10", "20", "L", "30", "40"]);
}

#[test]
fn test_parse_svg_path_simple() {
    let expr = parse_svg_path_data("M10 20 L30 40 Z");
    let expected = Expr::List(vec![
        Expr::Call("move_to".into(), vec![Expr::Num(10.0), Expr::Num(20.0)]),
        Expr::Call("line_to".into(), vec![Expr::Num(30.0), Expr::Num(40.0)]),
        Expr::Call("close".into(), vec![]),
    ]);
    assert_eq!(expr, expected);
}

#[test]
fn test_parse_svg_path_curve() {
    let expr = parse_svg_path_data("M0 0 C10 10 20 20 30 0 Z");
    let expected = Expr::List(vec![
        Expr::Call("move_to".into(), vec![Expr::Num(0.0), Expr::Num(0.0)]),
        Expr::Call(
            "curve_to".into(),
            vec![
                Expr::Num(10.0),
                Expr::Num(10.0),
                Expr::Num(20.0),
                Expr::Num(20.0),
                Expr::Num(30.0),
                Expr::Num(0.0),
            ],
        ),
        Expr::Call("close".into(), vec![]),
    ]);
    assert_eq!(expr, expected);
}

/// Compact path data glues a minus onto the following number, so `7-7` must
/// split into two tokens. The old tokenizer only split a sign after a
/// delimiter, silently turning `7-7`/`.53.53` into a single unparseable
/// token (and then `0.0`).
#[test]
fn test_tokenize_splits_compact_numbers() {
    assert_eq!(tokenize_path_data("M5 12h14"), vec!["M", "5", "12", "h", "14"]);
    assert_eq!(
        tokenize_path_data("m19 12-7 7-7-7"),
        vec!["m", "19", "12", "-7", "7", "-7", "-7"]
    );
    assert_eq!(tokenize_path_data(".53.53"), vec![".53", ".53"]);
    assert_eq!(tokenize_path_data("l-5-5"), vec!["l", "-5", "-5"]);
}

/// Only the first pair after a moveto opens a subpath; later pairs are an
/// implicit lineto (SVG rule). This is what makes a diagonal such as
/// `M17 7 7 17` render as a line instead of two disconnected dots.
#[test]
fn test_parse_svg_path_moveto_chains_to_lineto() {
    let expr = parse_svg_path_data("M17 7 7 17");
    assert_eq!(
        expr,
        Expr::List(vec![
            Expr::Call("move_to".into(), vec![Expr::Num(17.0), Expr::Num(7.0)]),
            Expr::Call("line_to".into(), vec![Expr::Num(7.0), Expr::Num(17.0)]),
        ])
    );
}

/// The exact shape produced from the `check` icon's path data — used by the
/// `icon:` property to expand a bundled stroke icon into `Path` commands.
#[test]
fn test_parse_svg_path_check_icon_data() {
    let expr = parse_svg_path_data("M20 6 9 17l-5-5");
    assert_eq!(
        expr,
        Expr::List(vec![
            Expr::Call("move_to".into(), vec![Expr::Num(20.0), Expr::Num(6.0)]),
            Expr::Call("line_to".into(), vec![Expr::Num(9.0), Expr::Num(17.0)]),
            Expr::Call("line_to".into(), vec![Expr::Num(4.0), Expr::Num(12.0)]),
        ])
    );
}

#[test]
fn test_import_simple_svg_file() {
    // Create a temporary SVG file
    let svg_content = r#"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
  <rect x="10" y="10" width="50" height="30" fill="red"/>
  <circle cx="70" cy="50" r="20" fill="blue"/>
  <g transform="translate(10, 0)">
    <rect x="0" y="0" width="20" height="20" fill="green"/>
  </g>
</svg>"#;
    let dir = std::env::temp_dir();
    let path = dir.join("test_import_simple.svg");
    std::fs::write(&path, svg_content).unwrap();

    let stmts = import_svg(&path).unwrap();
    std::fs::remove_file(&path).ok();

    // First statement should be a @config from width/height attributes
    assert_eq!(
        stmts.len(),
        4,
        "expected 4 root statements (config + 3 actors), got {}",
        stmts.len()
    );
    if let Stmt::Config { settings, .. } = &stmts[0] {
        assert_eq!(settings.len(), 1);
        assert_eq!(settings[0].name, "size");
        assert_eq!(settings[0].value, Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(100.0)]));
    } else {
        panic!("Expected Config as first statement");
    }

    // Next should be a Rect
    if let Stmt::ActorDecl {
        label, ty, props, ..
    } = &stmts[1]
    {
        assert_eq!(label, "rect_0");
        assert_eq!(ty, "Rect");
        // size should be (50, 30)
        let size_prop = props.iter().find(|p| p.name == "size").unwrap();
        assert_eq!(size_prop.value, Expr::Tuple(vec![Expr::Num(50.0), Expr::Num(30.0)]));
        // at should be centered: (10 + 25, 10 + 15) = (35, 25)
        let at_prop = props.iter().find(|p| p.name == "at").unwrap();
        assert_eq!(at_prop.value, Expr::Tuple(vec![Expr::Num(35.0), Expr::Num(25.0)]));
        // color should be red
        let color_prop = props.iter().find(|p| p.name == "color").unwrap();
        assert_eq!(color_prop.value, Expr::Ident("red".into()));
    } else {
        panic!("Expected ActorDecl");
    }

    // Next should be an Ellipse (for circle)
    if let Stmt::ActorDecl { label, ty, .. } = &stmts[2] {
        assert_eq!(label, "circle_1");
        assert_eq!(ty, "Ellipse");
    }

    // Next should be a Group
    if let Stmt::ActorDecl {
        label,
        ty,
        children,
        ..
    } = &stmts[3]
    {
        assert_eq!(label, "group_2");
        assert_eq!(ty, "Group");
        // Group should have one child (rect_3)
        assert_eq!(children.len(), 1, "Group should have 1 child");
    } else {
        panic!("Expected Group ActorDecl");
    }
}

#[test]
fn test_import_with_transform() {
    let svg_content = r##"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg">
  <rect x="0" y="0" width="100" height="50"
        transform="translate(200, 150) rotate(45) scale(2)"
        fill="#ff8800" stroke="#333" stroke-width="3"/>
</svg>"##;
    let dir = std::env::temp_dir();
    let path = dir.join("test_transform.svg");
    std::fs::write(&path, svg_content).unwrap();

    let stmts = import_svg(&path).unwrap();
    std::fs::remove_file(&path).ok();

    assert_eq!(stmts.len(), 1);
    if let Stmt::ActorDecl { props, .. } = &stmts[0] {
        // at: (0 + 50 + 200, 0 + 25 + 150) = (250, 175) after translate
        let at_prop = props.iter().find(|p| p.name == "at").unwrap();
        assert_eq!(at_prop.value, Expr::Tuple(vec![Expr::Num(250.0), Expr::Num(175.0)]));

        // rotation should be 45
        let rot_prop = props.iter().find(|p| p.name == "rotation").unwrap();
        assert_eq!(rot_prop.value, Expr::Num(45.0));

        // scale should be ~2
        let scale_prop = props.iter().find(|p| p.name == "scale").unwrap();
        assert!(
            (2.0 - match &scale_prop.value {
                Expr::Num(n) => *n,
                _ => 0.0,
            })
            .abs()
                < 1e-6
        );

        // stroke_color
        let stroke_prop = props.iter().find(|p| p.name == "stroke_color").unwrap();
        assert_eq!(
            stroke_prop.value,
            Expr::Call(
                "rgb".into(),
                vec![
                    Expr::Num(0x33 as f64),
                    Expr::Num(0x33 as f64),
                    Expr::Num(0x33 as f64)
                ]
            )
        );

        // stroke_width
        let sw_prop = props.iter().find(|p| p.name == "stroke_width").unwrap();
        assert_eq!(sw_prop.value, Expr::Num(3.0));
    }
}

#[test]
fn test_import_file_not_found() {
    let result = import_svg(Path::new("/nonexistent/file.svg"));
    assert!(result.is_err());
    match result.unwrap_err() {
        SvgImportError::Io(_) => {}, // expected
        other => panic!("Expected Io error, got: {other}"),
    }
}

#[test]
fn test_parse_numbers() {
    let nums = parse_numbers("10, 20, 30");
    assert_eq!(nums, vec![10.0, 20.0, 30.0]);

    let nums = parse_numbers("1.5 2.5 3.5");
    assert_eq!(nums, vec![1.5, 2.5, 3.5]);
}

#[test]
fn test_compose_transforms() {
    let a = Transform {
        tx: 10.0,
        ty: 20.0,
        rotation_deg: 0.0,
        sx: 1.0,
        sy: 1.0,
    };
    let b = Transform {
        tx: 5.0,
        ty: 5.0,
        rotation_deg: 30.0,
        sx: 2.0,
        sy: 2.0,
    };
    let c = a.compose(&b);
    assert!((c.tx - 15.0).abs() < 1e-6); // 10 + 5*1
    assert!((c.ty - 25.0).abs() < 1e-6); // 20 + 5*1
    assert!((c.rotation_deg - 30.0).abs() < 1e-6);
    assert!((c.sx - 2.0).abs() < 1e-6);
    assert!((c.sy - 2.0).abs() < 1e-6);
}

// -----------------------------------------------------------------------
// New feature tests: viewBox, stroke-dasharray, gradients, polyline/polygon
// -----------------------------------------------------------------------

#[test]
fn test_viewbox_config() {
    let svg_content = r#"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 600">
  <rect x="10" y="10" width="50" height="30" fill="red"/>
</svg>"#;
    let dir = std::env::temp_dir();
    let path = dir.join("test_viewbox.svg");
    std::fs::write(&path, svg_content).unwrap();

    let stmts = import_svg(&path).unwrap();
    std::fs::remove_file(&path).ok();

    // First statement should be the @config from viewBox
    assert!(!stmts.is_empty());
    if let Stmt::Config { settings, .. } = &stmts[0] {
        assert_eq!(settings.len(), 1);
        assert_eq!(settings[0].name, "size");
        assert_eq!(settings[0].value, Expr::Tuple(vec![Expr::Num(800.0), Expr::Num(600.0)]));
    } else {
        panic!("Expected Config as first statement from viewBox");
    }
}

#[test]
fn test_viewbox_fallback_width_height() {
    // No viewBox, but width/height attributes should also generate config
    let svg_content = r#"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="640" height="480">
  <rect x="0" y="0" width="100" height="100" fill="blue"/>
</svg>"#;
    let dir = std::env::temp_dir();
    let path = dir.join("test_viewbox_fallback.svg");
    std::fs::write(&path, svg_content).unwrap();

    let stmts = import_svg(&path).unwrap();
    std::fs::remove_file(&path).ok();

    assert!(!stmts.is_empty());
    if let Stmt::Config { settings, .. } = &stmts[0] {
        assert_eq!(settings[0].name, "size");
        assert_eq!(settings[0].value, Expr::Tuple(vec![Expr::Num(640.0), Expr::Num(480.0)]));
    } else {
        panic!("Expected Config as first statement from width/height");
    }
}

#[test]
fn test_no_viewbox_no_config() {
    // No viewBox, no width/height → no config emitted
    let svg_content = r#"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg">
  <rect x="0" y="0" width="100" height="100" fill="green"/>
</svg>"#;
    let dir = std::env::temp_dir();
    let path = dir.join("test_no_viewbox.svg");
    std::fs::write(&path, svg_content).unwrap();

    let stmts = import_svg(&path).unwrap();
    std::fs::remove_file(&path).ok();

    // First statement should be the rect, not a config
    assert_eq!(stmts.len(), 1);
    if let Stmt::ActorDecl { ty, .. } = &stmts[0] {
        assert_eq!(ty, "Rect");
    } else {
        panic!("Expected ActorDecl (Rect) without any config");
    }
}

#[test]
fn test_stroke_dasharray() {
    let svg_content = r#"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
  <rect x="10" y="10" width="50" height="30"
        fill="none" stroke="black" stroke-width="2"
        stroke-dasharray="5,3,2,3"/>
</svg>"#;
    let dir = std::env::temp_dir();
    let path = dir.join("test_dasharray.svg");
    std::fs::write(&path, svg_content).unwrap();

    let stmts = import_svg(&path).unwrap();
    std::fs::remove_file(&path).ok();

    // Second statement (after config) should be the rect with stroke_dasharray
    assert_eq!(stmts.len(), 2);
    if let Stmt::ActorDecl { props, .. } = &stmts[1] {
        let dash_prop = props.iter().find(|p| p.name == "stroke_dasharray").unwrap();
        assert_eq!(
            dash_prop.value,
            Expr::Tuple(vec![
                Expr::Num(5.0),
                Expr::Num(3.0),
                Expr::Num(2.0),
                Expr::Num(3.0),
            ])
        );
    } else {
        panic!("Expected ActorDecl with stroke_dasharray");
    }
}

#[test]
fn test_stroke_dasharray_space_separated() {
    let svg_content = r#"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg">
  <path d="M10 10 L100 10" fill="none" stroke="red"
        stroke-dasharray="10 5 2 5"/>
</svg>"#;
    let dir = std::env::temp_dir();
    let path = dir.join("test_dasharray_space.svg");
    std::fs::write(&path, svg_content).unwrap();

    let stmts = import_svg(&path).unwrap();
    std::fs::remove_file(&path).ok();

    assert_eq!(stmts.len(), 1);
    if let Stmt::ActorDecl { props, .. } = &stmts[0] {
        let dash_prop = props.iter().find(|p| p.name == "stroke_dasharray").unwrap();
        assert_eq!(
            dash_prop.value,
            Expr::Tuple(vec![
                Expr::Num(10.0),
                Expr::Num(5.0),
                Expr::Num(2.0),
                Expr::Num(5.0),
            ])
        );
    } else {
        panic!("Expected ActorDecl with stroke_dasharray");
    }
}

#[test]
fn test_gradient_url_fill_approximation() {
    // SVG with gradient definition and a rect using url(#grad)
    let svg_content = r##"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
  <defs>
    <linearGradient id="grad">
      <stop offset="0%" stop-color="#ff0000"/>
      <stop offset="100%" stop-color="#0000ff"/>
    </linearGradient>
  </defs>
  <rect x="10" y="10" width="100" height="80" fill="url(#grad)"/>
</svg>"##;
    let dir = std::env::temp_dir();
    let path = dir.join("test_gradient_url.svg");
    std::fs::write(&path, svg_content).unwrap();

    let stmts = import_svg(&path).unwrap();
    std::fs::remove_file(&path).ok();

    // Should have config + rect
    assert_eq!(stmts.len(), 2);
    if let Stmt::ActorDecl { props, .. } = &stmts[1] {
        // The ramp survives import: a horizontal SVG gradient becomes a
        // 90deg `linear(...)` call with both stops, not an averaged solid.
        let grad = props.iter().find(|p| p.name == "fill_gradient").unwrap();
        match &grad.value {
            Expr::Call(name, args) => {
                assert_eq!(name, "linear");
                assert!(
                    matches!(args[0], Expr::Num(v) if (v - 90.0).abs() < 1e-6),
                    "expected a 90deg ramp, got {:?}",
                    args[0]
                );
                let Expr::List(stops) = &args[1] else {
                    panic!("expected a stop list, got {:?}", args[1]);
                };
                assert_eq!(stops.len(), 2, "both stops must carry over");
            },
            other => panic!("expected a linear() call, got {other:?}"),
        }
        assert!(
            !props.iter().any(|p| p.name == "color"),
            "a ramped fill must not also emit the flattened solid color"
        );
    } else {
        panic!("Expected ActorDecl with gradient fill");
    }
}

#[test]
fn test_gradient_url_stroke() {
    let svg_content = r##"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
  <defs>
    <linearGradient id="mygrad">
      <stop offset="0%" stop-color="red"/>
      <stop offset="50%" stop-color="green"/>
      <stop offset="100%" stop-color="blue"/>
    </linearGradient>
  </defs>
  <rect x="0" y="0" width="50" height="50" fill="none" stroke="url(#mygrad)" stroke-width="3"/>
</svg>"##;
    let dir = std::env::temp_dir();
    let path = dir.join("test_gradient_stroke.svg");
    std::fs::write(&path, svg_content).unwrap();

    let stmts = import_svg(&path).unwrap();
    std::fs::remove_file(&path).ok();

    assert_eq!(stmts.len(), 2);
    if let Stmt::ActorDecl { props, .. } = &stmts[1] {
        // A gradient stroke imports as `stroke_gradient:`, not a solid.
        let stroke_prop = props.iter().find(|p| p.name == "stroke_gradient").unwrap();
        match &stroke_prop.value {
            Expr::Call(name, args) => {
                assert_eq!(name, "linear");
                let Expr::List(stops) = &args[1] else {
                    panic!("expected a stop list, got {:?}", args[1]);
                };
                assert_eq!(stops.len(), 3, "the middle stop must survive");
            },
            other => panic!("expected a linear() call, got {other:?}"),
        }
    } else {
        panic!("Expected ActorDecl with gradient stroke");
    }
}

#[test]
fn test_unknown_gradient_url_fill_none() {
    // url(#nonexistent) — gradient not in defs, result should be transparent (no color)
    let svg_content = r##"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg">
  <rect x="0" y="0" width="50" height="50" fill="url(#missing)"/>
</svg>"##;
    let dir = std::env::temp_dir();
    let path = dir.join("test_unknown_gradient.svg");
    std::fs::write(&path, svg_content).unwrap();

    let stmts = import_svg(&path).unwrap();
    std::fs::remove_file(&path).ok();

    // Unresolvable url(#...) → fill_opacity: 0.0 (transparent)
    assert_eq!(stmts.len(), 1);
    if let Stmt::ActorDecl { props, .. } = &stmts[0] {
        let fill_opacity = props.iter().find(|p| p.name == "fill_opacity").unwrap();
        assert_eq!(fill_opacity.value, Expr::Num(0.0));
        // There should be no 'color' property
        assert!(props.iter().find(|p| p.name == "color").is_none());
    } else {
        panic!("Expected ActorDecl");
    }
}

#[test]
fn test_defs_skip_gracefully() {
    // <defs> should not crash and should not produce any actor statements
    let svg_content = r##"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="50" height="50">
  <defs>
    <linearGradient id="g1">
      <stop offset="0%" stop-color="#fff"/>
      <stop offset="100%" stop-color="#000"/>
    </linearGradient>
    <radialGradient id="g2">
      <stop offset="0%" stop-color="red"/>
      <stop offset="100%" stop-color="blue"/>
    </radialGradient>
    <clipPath id="clip">
      <circle cx="25" cy="25" r="20"/>
    </clipPath>
  </defs>
  <rect x="0" y="0" width="50" height="50" fill="black"/>
</svg>"##;
    let dir = std::env::temp_dir();
    let path = dir.join("test_defs_skip.svg");
    std::fs::write(&path, svg_content).unwrap();

    let stmts = import_svg(&path).unwrap();
    std::fs::remove_file(&path).ok();

    // Should have config + rect (defs/clipPath produce no statements directly)
    assert_eq!(stmts.len(), 2);
    if let Stmt::ActorDecl { ty, .. } = &stmts[1] {
        assert_eq!(ty, "Rect");
    }
}

#[test]
fn test_polygon_conversion() {
    let svg_content = r##"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
  <polygon points="0,0 50,0 50,50 0,50" fill="#ff8800"/>
</svg>"##;
    let dir = std::env::temp_dir();
    let path = dir.join("test_polygon.svg");
    std::fs::write(&path, svg_content).unwrap();

    let stmts = import_svg(&path).unwrap();
    std::fs::remove_file(&path).ok();

    assert_eq!(stmts.len(), 2);
    if let Stmt::ActorDecl { ty, props, .. } = &stmts[1] {
        assert_eq!(ty, "Path");
        // Should have commands with move_to, line_to, line_to, line_to, close
        let cmd_prop = props.iter().find(|p| p.name == "commands").unwrap();
        if let Expr::List(commands) = &cmd_prop.value {
            assert_eq!(commands.len(), 5, "polygon should have 5 commands (4 lines + close)");
            assert_eq!(
                commands[0],
                Expr::Call("move_to".into(), vec![Expr::Num(0.0), Expr::Num(0.0)])
            );
            assert_eq!(commands[4], Expr::Call("close".into(), vec![]));
        } else {
            panic!("Expected List of commands");
        }
    } else {
        panic!("Expected Path actor for polygon");
    }
}

#[test]
fn test_polyline_conversion() {
    let svg_content = r#"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
  <polyline points="10,20 30,40 50,60" fill="none" stroke="blue" stroke-width="2"/>
</svg>"#;
    let dir = std::env::temp_dir();
    let path = dir.join("test_polyline.svg");
    std::fs::write(&path, svg_content).unwrap();

    let stmts = import_svg(&path).unwrap();
    std::fs::remove_file(&path).ok();

    assert_eq!(stmts.len(), 2);
    if let Stmt::ActorDecl { ty, props, .. } = &stmts[1] {
        assert_eq!(ty, "Path");
        let cmd_prop = props.iter().find(|p| p.name == "commands").unwrap();
        if let Expr::List(commands) = &cmd_prop.value {
            assert_eq!(
                commands.len(),
                3,
                "polyline should have 3 commands (move + 2 lines, no close)"
            );
            assert_eq!(
                commands[0],
                Expr::Call("move_to".into(), vec![Expr::Num(10.0), Expr::Num(20.0)])
            );
            // No Z/close command
            assert!(!commands.iter().any(|c| matches!(c, Expr::Call(name, ..) if name == "close")));
        } else {
            panic!("Expected List of commands");
        }
    } else {
        panic!("Expected Path actor for polyline");
    }
}

#[test]
fn test_radial_gradient_url_fill() {
    let svg_content = r##"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
  <defs>
    <radialGradient id="rgrad">
      <stop offset="0%" stop-color="#ffffff"/>
      <stop offset="100%" stop-color="#000000"/>
    </radialGradient>
  </defs>
  <circle cx="50" cy="50" r="40" fill="url(#rgrad)"/>
</svg>"##;
    let dir = std::env::temp_dir();
    let path = dir.join("test_radial_gradient.svg");
    std::fs::write(&path, svg_content).unwrap();

    let stmts = import_svg(&path).unwrap();
    std::fs::remove_file(&path).ok();

    assert_eq!(stmts.len(), 2);
    if let Stmt::ActorDecl { props, .. } = &stmts[1] {
        let grad = props.iter().find(|p| p.name == "fill_gradient").unwrap();
        // Default radialGeometry is centered with r=50%, so the imported
        // call is `radial((0.5, 0.5), 0.5, {…})`.
        match &grad.value {
            Expr::Call(name, args) => {
                assert_eq!(name, "radial");
                assert_eq!(args[0], Expr::Tuple(vec![Expr::Num(0.5), Expr::Num(0.5)]));
                assert_eq!(args[1], Expr::Num(0.5));
                let Expr::List(stops) = &args[2] else {
                    panic!("expected a stop list, got {:?}", args[2]);
                };
                assert_eq!(stops.len(), 2);
            },
            other => panic!("expected a radial() call, got {other:?}"),
        }
    } else {
        panic!("Expected ActorDecl with radial gradient fill");
    }
}

#[test]
fn test_stop_color_parse_named() {
    assert_eq!(parse_stop_color("red"), (255, 0, 0));
    assert_eq!(parse_stop_color("Blue"), (0, 0, 255));
    assert_eq!(parse_stop_color("GREEN"), (0, 128, 0));
    assert_eq!(parse_stop_color("black"), (0, 0, 0));
    assert_eq!(parse_stop_color("white"), (255, 255, 255));
    assert_eq!(parse_stop_color("orange"), (255, 165, 0));
}

#[test]
fn test_stop_color_parse_hex() {
    assert_eq!(parse_stop_color("#ff0000"), (255, 0, 0));
    assert_eq!(parse_stop_color("#00ff00"), (0, 255, 0));
    assert_eq!(parse_stop_color("#0000ff"), (0, 0, 255));
    assert_eq!(parse_stop_color("#f00"), (255, 0, 0));
    assert_eq!(parse_stop_color("#abc"), (170, 187, 204));
}

#[test]
fn test_stop_color_fallback() {
    assert_eq!(parse_stop_color("unknown-color"), (0, 0, 0));
    assert_eq!(parse_stop_color(""), (0, 0, 0));
}

#[test]
fn test_gradient_approximation_single_stop() {
    let def = GradientDef::Linear {
        x1: 0.0,
        y1: 0.0,
        x2: 1.0,
        y2: 0.0,
        stops: vec![GradientStop {
            offset: 0.0,
            r: 200,
            g: 150,
            b: 100,
            opacity: 1.0,
        }],
    };
    let (r, g, b) = def.approximate_solid_color();
    assert_eq!((r, g, b), (200, 150, 100));
}

#[test]
fn test_gradient_approximation_two_stops() {
    let def = GradientDef::Linear {
        x1: 0.0,
        y1: 0.0,
        x2: 1.0,
        y2: 0.0,
        stops: vec![
            GradientStop {
                offset: 0.0,
                r: 255,
                g: 0,
                b: 0,
                opacity: 1.0,
            },
            GradientStop {
                offset: 1.0,
                r: 0,
                g: 0,
                b: 255,
                opacity: 1.0,
            },
        ],
    };
    let (r, g, b) = def.approximate_solid_color();
    // Average of (255,0,0) and (0,0,255) = (127.5, 0, 127.5) → (128, 0, 128)
    assert_eq!((r, g, b), (128, 0, 128));
}

#[test]
fn test_gradient_approximation_three_stops() {
    let def = GradientDef::Linear {
        x1: 0.0,
        y1: 0.0,
        x2: 1.0,
        y2: 0.0,
        stops: vec![
            GradientStop {
                offset: 0.0,
                r: 0,
                g: 0,
                b: 0,
                opacity: 1.0,
            },
            GradientStop {
                offset: 0.5,
                r: 128,
                g: 128,
                b: 128,
                opacity: 1.0,
            },
            GradientStop {
                offset: 1.0,
                r: 255,
                g: 255,
                b: 255,
                opacity: 1.0,
            },
        ],
    };
    let (r, g, b) = def.approximate_solid_color();
    // spans: 0.0→0.5: avg(0,128)=64 each ×0.5 = 32
    //        0.5→1.0: avg(128,255)=191.5 each ×0.5 = 95.75
    // total: 32 + 95.75 = 127.75 ≈ 128
    assert_eq!((r, g, b), (128, 128, 128));
}
