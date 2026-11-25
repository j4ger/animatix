//! Source ranges only. Every glyph, caret and selection belongs to a single TextEdit.
use std::collections::BTreeMap;

use animatix_syntax::ast::Stmt;
use animatix_syntax::parser::parse_source;
use animatix_syntax::token::LineIndex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Setup,
    Scene,
    Time,
}

#[derive(Clone, Debug)]
pub(crate) struct Cell {
    pub start: usize,
    pub end: usize, // exclusive, with trailing blank rows left outside the card
    pub byte: usize,
    pub kind: Kind,
}

pub(crate) fn scene_headers(stmts: &[Stmt]) -> BTreeMap<usize, Kind> {
    stmts
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::Scene {
                span: Some(span), ..
            } => Some((span.start_line.saturating_sub(1), Kind::Scene)),
            _ => None,
        })
        .collect()
}

/// While semantic analysis is pending, carry intact headers across the edit.
/// Only a hash in the new lexer stream can become a card (see source_cells).
pub(crate) fn unchanged_headers(old: &str, cells: &[Cell], new: &str) -> BTreeMap<usize, Kind> {
    let before: Vec<_> = old.split('\n').collect();
    let after: Vec<_> = new.split('\n').collect();
    let prefix = before.iter().zip(&after).take_while(|(a, b)| a == b).count();
    let suffix = before[prefix..]
        .iter()
        .rev()
        .zip(after[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    cells
        .iter()
        .filter(|cell| cell.kind != Kind::Setup)
        .filter_map(|cell| {
            let line = if cell.start < prefix {
                cell.start
            } else if cell.start >= before.len() - suffix {
                after.len() - (before.len() - cell.start)
            } else {
                return None;
            };
            Some((line, cell.kind))
        })
        .collect()
}

pub(crate) fn source_cells(
    source: &str,
    roles: &[(usize, usize, &str)],
    headers: BTreeMap<usize, Kind>,
    recover: bool,
) -> Vec<Cell> {
    let index = LineIndex::new(source);
    let lines: Vec<_> = source.split('\n').collect();
    let mut cells = vec![Cell {
        start: 0,
        end: 1,
        byte: 0,
        kind: Kind::Setup,
    }];
    for &(start, end, role) in roles {
        let hash = role == "punctuation" && &source[start..end] == "#";
        if role != "keyframe" && !hash {
            continue;
        }
        let (line, _) = index.byte_to_line_col(start);
        let text = lines[line].trim_start();
        if start != index.line_col_to_byte(line, 0) + lines[line].len() - text.len() {
            continue;
        }
        let kind = if role == "keyframe" {
            Some(Kind::Time)
        } else {
            headers
                .get(&line)
                .copied()
                .or_else(|| recover.then(|| recovered_header(text)).flatten())
        };
        if let Some(kind) = kind {
            if line == 0 {
                cells.clear();
            }
            cells.push(Cell {
                start: line,
                end: line + 1,
                byte: start,
                kind,
            });
        }
    }
    let mut next = lines.len();
    for cell in cells.iter_mut().rev() {
        cell.end = next;
        while cell.end > cell.start + 1 && lines[cell.end - 1].trim().is_empty() {
            cell.end -= 1;
        }
        next = cell.start;
    }
    cells
}

// After an invalid statement, the parser can recover later than the next
// header. Consult the same grammar locally only for an unclassified hash.
fn recovered_header(text: &str) -> Option<Kind> {
    let (ast, errors) = parse_source(text);
    if !errors.is_empty() {
        return None;
    }
    match ast.as_deref()?.first()? {
        Stmt::Scene { .. } => Some(Kind::Scene),
        stmt @ (Stmt::Keyframe { .. } | Stmt::RelativeKeyframe { .. })
            if !stmt.is_cue_only_keyframe() =>
        {
            Some(Kind::Time)
        },
        Stmt::Step { .. } => Some(Kind::Time),
        _ => None,
    }
}
