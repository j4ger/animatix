use crate::ast::{Span, is_array_member_label};
use crate::diagnostics::{
    Diagnostic, DiagnosticCode, DiagnosticLocation, DiagnosticPhase, DiagnosticSeverity,
};
use crate::symbol_table::SymbolTable;
use crate::token::{Token, TokenKind, byte_to_line_col};
use crate::typing;

pub(crate) fn span_diagnostic(
    severity: DiagnosticSeverity,
    code: DiagnosticCode,
    message: String,
    line: usize,
    col: usize,
    end_col: usize,
) -> Diagnostic {
    Diagnostic {
        severity,
        phase: DiagnosticPhase::Build,
        code,
        message,
        location: DiagnosticLocation {
            line: Some(line),
            column: Some(col),
            end_line: Some(line),
            end_col: Some(end_col),
            span: None,
            path: None,
            subject: None,
        },
    }
}

/// A `type-mismatch` warning for `owner.property`.
///
/// Property assignments and effect-stage parameters all report the same shape;
/// one constructor keeps the message and span rules from drifting between the
/// actor-declaration, assignment, and stage paths.
pub(crate) fn type_mismatch_diagnostic(
    owner: &str,
    property: &str,
    expected: &typing::Type,
    actual: &typing::Type,
    line: usize,
    col: usize,
    end_col: usize,
) -> Diagnostic {
    span_diagnostic(
        DiagnosticSeverity::Warning,
        DiagnosticCode::TypeMismatch,
        format!("Type mismatch for '{owner}.{property}': expected {expected:?}, found {actual:?}"),
        line,
        col,
        end_col,
    )
}

pub(crate) fn range_diagnostic(
    severity: DiagnosticSeverity,
    code: DiagnosticCode,
    message: String,
    start: (usize, usize),
    end: (usize, usize),
) -> Diagnostic {
    Diagnostic {
        severity,
        phase: DiagnosticPhase::Build,
        code,
        message,
        location: DiagnosticLocation {
            line: Some(start.0 + 1),
            column: Some(start.1 + 1),
            end_line: Some(end.0 + 1),
            end_col: Some(end.1 + 1),
            span: None,
            path: None,
            subject: None,
        },
    }
}

pub(crate) fn span_positions(span: &Option<Span>) -> (usize, usize, usize, usize) {
    match span {
        Some(s) => (s.start_line, s.start_col, s.end_line, s.end_col),
        None => (1, 1, 1, 1),
    }
}

/// Positions for a property diagnostic: the property's own value span when
/// the parser recorded one, falling back to the actor declaration position.
///
/// Returns 1-based `(line, col, end_line, end_col)`, matching the diagnostic
/// location convention used by this module.
pub(crate) fn property_positions(
    prop: &crate::ast::Property,
    source: &str,
    actor_line: usize,
    actor_col: usize,
    actor_end_col: usize,
) -> (usize, usize, usize, usize) {
    let Some(byte_span) = prop.value_span else {
        return (actor_line, actor_col, actor_line, actor_end_col);
    };
    if source.is_empty() || byte_span.end > source.len() {
        return (actor_line, actor_col, actor_line, actor_end_col);
    }
    let span = Span::from_byte_span(source, byte_span);
    (span.start_line, span.start_col, span.end_line, span.end_col)
}

pub(crate) fn is_component_array_member(symbols: &SymbolTable, label: &str) -> bool {
    let Some(base) = is_array_member_label(label) else {
        return false;
    };
    let Some((instance, _)) = base.rsplit_once('.') else {
        return false;
    };
    let Some(info) = symbols.labels.get(instance) else {
        return false;
    };
    info.ty.as_deref().is_some_and(|ty| symbols.components.contains_key(ty))
}

/// Detect a `scope.stage.param = value` assignment so the caller can skip
/// host-property validation.
///
/// The shape is: at least two target segments with the last naming an inline
/// child (a stage). Whatever that stage's effect declares, the property is a
/// *stage parameter*, not a property of the host actor — validating
/// `panel.pix.size = 6` against `Filter.size` (Vec2) produced false
/// `type-mismatch` warnings for every plugin effect whose parameter list is
/// not in the static schema (e.g. `Pixelate` before its library loads).
///
/// Returns the stage label when the shape matches.
pub(crate) fn stage_param_segment(
    target: &[crate::ast::TargetSegment],
    symbols: &SymbolTable,
    _property: &str,
) -> Option<String> {
    if target.len() < 2 {
        return None;
    }
    let label = target.last()?.label_str();
    let stage_ty = symbols.inline_child_type(label)?;
    // A recognized effect is validated by the effect branch above; an
    // unrecognized stage type is left to the unknown-type diagnostic.
    if symbols.is_effect_type(stage_ty) {
        return None;
    }
    Some(label.to_string())
}

pub(crate) fn find_ident_range(
    tokens: &[Token],
    source: &str,
    text: &str,
    range: Option<std::ops::Range<usize>>,
) -> Option<((usize, usize), (usize, usize))> {
    tokens
        .iter()
        .filter(|t| range.as_ref().is_none_or(|r| t.span.start >= r.start && t.span.end <= r.end))
        .find(|t| matches!(&t.kind, TokenKind::Ident(name) if name == text))
        .map(|t| {
            let start = byte_to_line_col(source, t.span.start);
            let end = byte_to_line_col(source, t.span.end);
            (start, end)
        })
}

/// Locate the `Type` identifier in a `label: Type` declaration pair.
///
/// Matching on the label first keeps several declarations that share a type
/// (`dot: Pulse`, `ring: Pulse`, `cross: Pulse`) pointing at their own line
/// instead of all resolving to the first occurrence of the type name.
/// Returns 0-based `((line, col), (line, col))`.
pub(crate) fn find_decl_type_range(
    tokens: &[Token],
    source: &str,
    label: &str,
    ty: &str,
) -> Option<((usize, usize), (usize, usize))> {
    let label_index = tokens
        .iter()
        .position(|t| matches!(&t.kind, TokenKind::Ident(name) if name == label))?;
    // Scan forward for `: Type` immediately after the label.
    let mut index = label_index + 1;
    while index + 1 < tokens.len() {
        if matches!(tokens[index].kind, TokenKind::Colon) {
            if let TokenKind::Ident(name) = &tokens[index + 1].kind
                && name == ty
            {
                let token = &tokens[index + 1];
                return Some((
                    byte_to_line_col(source, token.span.start),
                    byte_to_line_col(source, token.span.end),
                ));
            }
            break; // a different type name on this declaration
        }
        if matches!(tokens[index].kind, TokenKind::Ident(_)) {
            break; // moved past the label without finding a colon
        }
        index += 1;
    }
    None
}

/// Return the source-level identifier text for a resolved target string.
pub(crate) fn target_source_text(target: &str) -> &str {
    let first = target.split('.').next().unwrap_or(target);
    is_array_member_label(first).unwrap_or(first)
}
