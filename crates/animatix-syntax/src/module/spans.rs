use crate::ast::{Span, Stmt};
use crate::walk::walk_stmts_mut;

/// Walk the AST and convert `Action.byte_span` into `Stmt::Action` line/col spans.
pub(super) fn set_action_spans(stmts: &mut [Stmt], source: &str) {
    walk_stmts_mut(stmts, &mut |stmt| {
        if let Stmt::Action(action, span) = stmt
            && let Some(byte_span) = action.byte_span
        {
            *span = Some(Span::from_byte_span(source, byte_span));
        }
    });
}
