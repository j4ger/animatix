//! Hover information provider.

use animatix_syntax::token::{Token, TokenKind, byte_to_line_col, line_col_to_byte, token_at_byte};

use crate::symbol_table::{LabelKind, SymbolTable};
use crate::types::HoverInfo;

/// Get hover information at a cursor position.
pub fn hover_at(
    symbols: &SymbolTable,
    tokens: &[Token],
    source: &str,
    line: usize,
    col: usize,
) -> Option<HoverInfo> {
    let byte = line_col_to_byte(source, line, col);
    let token = token_at_byte(tokens, byte)?;
    let text = token_text(source, token)?;
    let range = Some(token_range(source, token));

    match &token.kind {
        TokenKind::Str(_) | TokenKind::Typst(_) => {
            return Some(HoverInfo {
                contents: format!("**String** `{}`", text),
                range,
            });
        },
        TokenKind::Number(_) | TokenKind::Time { .. } | TokenKind::Percent(_) => {
            return Some(HoverInfo {
                contents: format!("**Number** `{}`", text),
                range,
            });
        },
        TokenKind::Comment(_) => {
            return Some(HoverInfo {
                contents: format!("*Comment*\n\n{}", text),
                range,
            });
        },
        TokenKind::Ident(_) | TokenKind::Keyword(_) => {},
        _ => return None,
    }

    // Labels, types, actions, and keywords take precedence over property names.
    if let Some(info) = symbols.labels.get(text) {
        let kind = match info.kind {
            LabelKind::Actor => "Actor",
            LabelKind::Let => "Variable",
            LabelKind::For => "Loop variable",
            LabelKind::Always => "Always block",
            LabelKind::Component => "Component",
        };
        let ty = info.ty.as_deref().unwrap_or("unknown");
        return Some(HoverInfo {
            contents: format!("**{}** `{}`\n\nType: {}", kind, text, ty),
            range,
        });
    }
    if symbols.types.contains(text) {
        return Some(HoverInfo {
            contents: format!("**Type** `{}`\n\n{}", text, type_documentation(text)),
            range,
        });
    }
    if symbols.actions.contains(text) {
        return Some(HoverInfo {
            contents: format!("**Action** `{}`\n\n{}", text, action_documentation(text)),
            range,
        });
    }
    if symbols.keywords.contains(text) {
        return Some(HoverInfo {
            contents: format!("**Keyword** `{}`\n\n{}", text, keyword_documentation(text)),
            range,
        });
    }
    if symbols.components.contains_key(text) {
        let info = &symbols.components[text];
        let params_str = info
            .params
            .iter()
            .map(|p| match &p.param_type {
                Some(ty) => format!("{}: {:?}", p.name, ty),
                None => p.name.clone(),
            })
            .collect::<Vec<_>>()
            .join(", ");
        return Some(HoverInfo {
            contents: format!("**Component** `{}`\n\nParameters: ({})", text, params_str),
            range,
        });
    }

    // Property names are identifiers followed by `:` in a property list or
    // preceded by `.` in an access path.
    if is_property_position(tokens, token) {
        if let Some(doc) = crate::completer::property_documentation(text) {
            return Some(HoverInfo {
                contents: format!("**Property** `{}`\n\n{}", text, doc),
                range,
            });
        }
    }

    None
}

fn token_text<'a>(source: &'a str, token: &Token) -> Option<&'a str> {
    source.get(token.span.start..token.span.end)
}

fn token_range(source: &str, token: &Token) -> (usize, usize, usize, usize) {
    let (start_line, start_col) = byte_to_line_col(source, token.span.start);
    let (end_line, end_col) = byte_to_line_col(source, token.span.end);
    (start_line, start_col, end_line, end_col)
}

fn is_property_position(tokens: &[Token], token: &Token) -> bool {
    let idx = tokens.iter().position(|t| t.span == token.span);
    match idx {
        Some(i) => {
            let next_is_colon =
                tokens.get(i + 1).is_some_and(|t| matches!(t.kind, TokenKind::Colon));
            let prev_is_dot = i > 0 && matches!(tokens[i - 1].kind, TokenKind::Dot);
            next_is_colon || prev_is_dot
        },
        None => false,
    }
}

/// Documentation for a type.
pub fn type_documentation(name: &str) -> &'static str {
    match name {
        "Text" => "Text element with content and styling properties.",
        "Code" => "Code block with syntax highlighting.",
        "Svg" => "SVG image element.",
        "Image" => "Raster image element.",
        "Rect" => "Rectangle shape with fill and stroke.",
        "Ellipse" => "Ellipse, circle, arc, or dot shape.",
        "Line" => "Line segment or arrow with optional head.",
        "Polygon" => "Polygon or regular polygon shape.",
        "Path" => "SVG path element.",
        "Graph" => "Function graph.",
        "PlotCurve" => "Plot curve with configurable sampling kind.",
        "Button" => "Interactive button element.",
        _ => "Unknown type.",
    }
}

/// Documentation for an action.
pub fn action_documentation(name: &str) -> &'static str {
    match name {
        "fade-in" => "Fade in from transparent.",
        "draw-in" => "Draw in (like handwriting).",
        "wipe-in" => "Wipe in from edge.",
        "fade-out" => "Fade out to transparent.",
        "wipe-out" => "Wipe out to edge.",
        "reveal-out" => "Reveal out (reverse draw).",
        "draw-out" => "Draw out (reverse handwriting).",
        "move" => "Move to position: `move target to (x, y)`",
        "shift" => "Shift by offset: `shift target by (dx, dy)`",
        "rotate" => "Rotate: `rotate target by 90`",
        "scale" => "Scale: `scale target to 2`",
        "persist" => "Mark actor(s) to carry into the next scene: `persist actor1, actor2`",
        "remove" => "Fade out and stop persisting: `remove actor [500ms]`",
        _ => "Unknown action.",
    }
}

/// Documentation for a keyword.
pub fn keyword_documentation(name: &str) -> &'static str {
    match name {
        "let" => "Declare a variable: `let name = value`",
        "import" => "Import another file: `import \"path\"`",
        "always" => "Reactive block that runs continuously.",
        "if" => "Conditional: `if condition { ... }`",
        "else" => "Else branch: `if ... { } else { }`",
        "for" => "Loop: `for item in collection { ... }`",
        "in" => "Used in for loops.",
        "pub" => "Make visible to other files.",
        "component" => "Define a reusable component.",
        "sequence" => "Run actions in sequence.",
        "stagger" => "Stagger actions with delay.",
        _ => "Keyword.",
    }
}
