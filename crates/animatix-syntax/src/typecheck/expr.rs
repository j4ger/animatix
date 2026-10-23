//! Expression summaries for diagnostics.

use crate::ast::Expr;

/// Short summary of an expression for error messages.
pub(super) fn expr_summary(expr: &Expr) -> String {
    match expr {
        Expr::Num(n) => format!("number {}", n),
        Expr::Percent(p) => format!("percent {}%", p),
        Expr::Str(s) => format!("string {:?}", s),
        Expr::Bool(b) => format!("boolean {}", b),
        Expr::Null => "null".to_string(),
        Expr::List(items) => format!("list of {} items", items.len()),
        Expr::Tuple(items) => format!("tuple of {} items", items.len()),
        Expr::Ident(name) => format!("identifier '{}'", name),
        Expr::Path(parts) if parts.len() == 1 => format!("identifier '{}'", parts[0]),
        Expr::Construct(name, _) => format!("construct '{}'", name),
        Expr::Binary(_, op, _) => format!("binary {:?} expression", op),
        Expr::Unary(op, _) => format!("unary {:?} expression", op),
        Expr::Call(name, args) => format!("call '{}({})'", name, args.len()),
        _ => "expression".to_string(),
    }
}
