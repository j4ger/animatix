//! SVG path-data (`d="…"`) parsing, shared by the SVG importer and the `Path`
//! primitive's `icon:` sugar.
//!
//! This lives outside `svg_import` because it is *not* part of SVG asset loading:
//! the bundled stroke icons are path-data strings compiled at build time, so the
//! `slim` web profile — which turns off the `svg` feature and with it `usvg`
//! document decoding — still has to parse them. Gating it with the importer made
//! `Path, icon: "plus"` (and therefore the `--no-default-features` build the site
//! and CI both use) fail to compile.

use crate::ast::Expr;

/// Parse an SVG `d` attribute into Animatix path command expressions.
///
/// Supports absolute and relative commands: M/m, L/l, Q/q, C/c, H/h, V/v,
/// S/s, T/t, A/a, Z/z. Returns a list expression:
/// `{move_to(...), line_to(...), ...}` — the same shape the `commands` property
/// consumes, so callers can reuse it to expand path data into actor commands
/// (this is how the bundled `icon:` set becomes `Path` geometry).
///
/// Two SVG-grammar rules are honored here because compact real-world path data
/// depends on both, and neither was handled before: a trailing coordinate pair
/// after `M`/`m` is an *implicit* `line_to` (only the first pair is a moveto),
/// and `tokenize_path_data` splits numbers at every sign / second decimal point
/// (see its doc comment). Arc commands (`A`/`a`) are still approximated as a
/// straight chord, so callers that need smooth curves should pass curve data.
pub(crate) fn parse_svg_path_data(d: &str) -> Expr {
    let tokens = tokenize_path_data(d);
    let mut commands: Vec<Expr> = Vec::new();
    let mut i = 0;
    let mut current_pos = (0.0, 0.0); // for relative commands

    while i < tokens.len() {
        let cmd = tokens[i].as_str();
        i += 1;

        match cmd {
            "M" | "m" => {
                let abs = cmd == "M";
                // SVG rule: only the *first* coordinate pair after a moveto is a
                // new subpath; every following pair is an implicit lineto. The
                // old code emitted `move_to` for all of them, which silently
                // dropped the connecting segment (a diagonal `M17 7 7 17`
                // rendered as two invisible dots). Track the first pair so the
                // rest lower to `line_to`.
                let mut first_pair = true;
                while i + 1 < tokens.len() && is_number(&tokens[i]) {
                    let x = parse_token_num(&tokens[i]);
                    let y = parse_token_num(&tokens[i + 1]);
                    i += 2;
                    let (abs_x, abs_y) = if abs {
                        (x, y)
                    } else {
                        (current_pos.0 + x, current_pos.1 + y)
                    };
                    let name = if first_pair { "move_to" } else { "line_to" };
                    first_pair = false;
                    commands
                        .push(Expr::Call(name.into(), vec![Expr::Num(abs_x), Expr::Num(abs_y)]));
                    current_pos = (abs_x, abs_y);
                }
            },
            "L" | "l" => {
                let abs = cmd == "L";
                while i + 1 < tokens.len() && is_number(&tokens[i]) {
                    let x = parse_token_num(&tokens[i]);
                    let y = parse_token_num(&tokens[i + 1]);
                    i += 2;
                    let (abs_x, abs_y) = if abs {
                        (x, y)
                    } else {
                        (current_pos.0 + x, current_pos.1 + y)
                    };
                    commands.push(Expr::Call(
                        "line_to".into(),
                        vec![Expr::Num(abs_x), Expr::Num(abs_y)],
                    ));
                    current_pos = (abs_x, abs_y);
                }
            },
            "Q" | "q" => {
                let abs = cmd == "Q";
                while i + 3 < tokens.len() && is_number(&tokens[i]) {
                    let x1 = parse_token_num(&tokens[i]);
                    let y1 = parse_token_num(&tokens[i + 1]);
                    let x = parse_token_num(&tokens[i + 2]);
                    let y = parse_token_num(&tokens[i + 3]);
                    i += 4;
                    let (abs_x1, abs_y1, abs_x, abs_y) = if abs {
                        (x1, y1, x, y)
                    } else {
                        (
                            current_pos.0 + x1,
                            current_pos.1 + y1,
                            current_pos.0 + x,
                            current_pos.1 + y,
                        )
                    };
                    commands.push(Expr::Call(
                        "quad_to".into(),
                        vec![
                            Expr::Num(abs_x1),
                            Expr::Num(abs_y1),
                            Expr::Num(abs_x),
                            Expr::Num(abs_y),
                        ],
                    ));
                    current_pos = (abs_x, abs_y);
                }
            },
            "C" | "c" => {
                let abs = cmd == "C";
                while i + 5 < tokens.len() && is_number(&tokens[i]) {
                    let x1 = parse_token_num(&tokens[i]);
                    let y1 = parse_token_num(&tokens[i + 1]);
                    let x2 = parse_token_num(&tokens[i + 2]);
                    let y2 = parse_token_num(&tokens[i + 3]);
                    let x = parse_token_num(&tokens[i + 4]);
                    let y = parse_token_num(&tokens[i + 5]);
                    i += 6;
                    let (abs_x1, abs_y1, abs_x2, abs_y2, abs_x, abs_y) = if abs {
                        (x1, y1, x2, y2, x, y)
                    } else {
                        (
                            current_pos.0 + x1,
                            current_pos.1 + y1,
                            current_pos.0 + x2,
                            current_pos.1 + y2,
                            current_pos.0 + x,
                            current_pos.1 + y,
                        )
                    };
                    commands.push(Expr::Call(
                        "curve_to".into(),
                        vec![
                            Expr::Num(abs_x1),
                            Expr::Num(abs_y1),
                            Expr::Num(abs_x2),
                            Expr::Num(abs_y2),
                            Expr::Num(abs_x),
                            Expr::Num(abs_y),
                        ],
                    ));
                    current_pos = (abs_x, abs_y);
                }
            },
            "H" | "h" => {
                let abs = cmd == "H";
                while i < tokens.len() && is_number(&tokens[i]) {
                    let x = parse_token_num(&tokens[i]);
                    i += 1;
                    let abs_x = if abs { x } else { current_pos.0 + x };
                    commands.push(Expr::Call(
                        "line_to".into(),
                        vec![Expr::Num(abs_x), Expr::Num(current_pos.1)],
                    ));
                    current_pos = (abs_x, current_pos.1);
                }
            },
            "V" | "v" => {
                let abs = cmd == "V";
                while i < tokens.len() && is_number(&tokens[i]) {
                    let y = parse_token_num(&tokens[i]);
                    i += 1;
                    let abs_y = if abs { y } else { current_pos.1 + y };
                    commands.push(Expr::Call(
                        "line_to".into(),
                        vec![Expr::Num(current_pos.0), Expr::Num(abs_y)],
                    ));
                    current_pos = (current_pos.0, abs_y);
                }
            },
            "S" | "s" => {
                // Smooth cubic bezier: control point is reflection of previous control point
                let abs = cmd == "S";
                while i + 3 < tokens.len() && is_number(&tokens[i]) {
                    let x2 = parse_token_num(&tokens[i]);
                    let y2 = parse_token_num(&tokens[i + 1]);
                    let x = parse_token_num(&tokens[i + 2]);
                    let y = parse_token_num(&tokens[i + 3]);
                    i += 4;
                    // For simplicity, use current_pos as first control point
                    // (proper implementation would reflect previous control point)
                    let (abs_x1, abs_y1, abs_x2, abs_y2, abs_x, abs_y) = if abs {
                        (current_pos.0, current_pos.1, x2, y2, x, y)
                    } else {
                        (
                            current_pos.0,
                            current_pos.1,
                            current_pos.0 + x2,
                            current_pos.1 + y2,
                            current_pos.0 + x,
                            current_pos.1 + y,
                        )
                    };
                    commands.push(Expr::Call(
                        "curve_to".into(),
                        vec![
                            Expr::Num(abs_x1),
                            Expr::Num(abs_y1),
                            Expr::Num(abs_x2),
                            Expr::Num(abs_y2),
                            Expr::Num(abs_x),
                            Expr::Num(abs_y),
                        ],
                    ));
                    current_pos = (abs_x, abs_y);
                }
            },
            "T" | "t" => {
                // Smooth quadratic bezier: control point is reflection of previous control point
                let abs = cmd == "T";
                while i + 1 < tokens.len() && is_number(&tokens[i]) {
                    let x = parse_token_num(&tokens[i]);
                    let y = parse_token_num(&tokens[i + 1]);
                    i += 2;
                    // For simplicity, use current_pos as control point
                    let (abs_x1, abs_y1, abs_x, abs_y) = if abs {
                        (current_pos.0, current_pos.1, x, y)
                    } else {
                        (current_pos.0, current_pos.1, current_pos.0 + x, current_pos.1 + y)
                    };
                    commands.push(Expr::Call(
                        "quad_to".into(),
                        vec![
                            Expr::Num(abs_x1),
                            Expr::Num(abs_y1),
                            Expr::Num(abs_x),
                            Expr::Num(abs_y),
                        ],
                    ));
                    current_pos = (abs_x, abs_y);
                }
            },
            "A" | "a" => {
                // Arc: A rx x-rotation large-arc-flag sweep-flag x y
                let abs = cmd == "A";
                while i + 6 < tokens.len() && is_number(&tokens[i]) {
                    let _rx = parse_token_num(&tokens[i]);
                    let _ry = parse_token_num(&tokens[i + 1]);
                    let _x_rotation = parse_token_num(&tokens[i + 2]);
                    let _large_arc = parse_token_num(&tokens[i + 3]) as i32;
                    let _sweep = parse_token_num(&tokens[i + 4]) as i32;
                    let x = parse_token_num(&tokens[i + 5]);
                    let y = parse_token_num(&tokens[i + 6]);
                    i += 7;
                    let (abs_x, abs_y) = if abs {
                        (x, y)
                    } else {
                        (current_pos.0 + x, current_pos.1 + y)
                    };
                    // Arcs are complex — approximate as line_to for now
                    // Full arc support requires calculating arc segment points
                    commands.push(Expr::Call(
                        "line_to".into(),
                        vec![Expr::Num(abs_x), Expr::Num(abs_y)],
                    ));
                    current_pos = (abs_x, abs_y);
                }
            },
            "Z" | "z" => {
                commands.push(Expr::Call("close".into(), vec![]));
            },
            _ => {
                // Unknown command — skip and continue parsing rest of path
                continue;
            },
        }
    }

    Expr::List(commands)
}

/// Tokenize an SVG path `d` attribute into command letters and numbers.
///
/// Compact SVG path data omits separators between numbers wherever the grammar
/// is unambiguous, so the split rules are:
/// - a command letter always starts a new token;
/// - a `,`/whitespace ends the current token;
/// - a `+`/`-` starts a new number (unless it is the leading sign of a token that has not begun),
///   which is what turns `7-7` into `7` and `-7`;
/// - a `.` ends the current number if it already contains one, which turns the radii pair `.53.53`
///   into `.53` and `.53` (a number has at most one point).
///
/// Without this, real-world data (Lucide, most icon sets) produced single
/// unparseable tokens like `7-7`/`.53.53` that `parse_token_num` silently turned
/// into `0.0`, corrupting the geometry.
pub(crate) fn tokenize_path_data(d: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in d.chars() {
        if ch.is_ascii_alphabetic() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            tokens.push(ch.to_string());
        } else if ch == '-' || ch == '+' {
            // A sign begins a new number whenever a number is already open; the
            // lone leading sign of a fresh token just accumulates.
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            current.push(ch);
        } else if ch == '.' {
            if current.contains('.') {
                tokens.push(std::mem::take(&mut current));
            }
            current.push(ch);
        } else if ch.is_ascii_digit() {
            current.push(ch);
        } else {
            // Delimiters: comma and any whitespace.
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

pub(crate) fn is_number(s: &str) -> bool {
    s.parse::<f64>().is_ok()
}

pub(crate) fn parse_token_num(s: &str) -> f64 {
    s.parse::<f64>().unwrap_or(0.0)
}

// ---------------------------------------------------------------------------
// Element conversion
// ---------------------------------------------------------------------------
