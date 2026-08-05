use std::fmt;

use super::types::{CompiledExpr, ModifierExpr, ModifierIrProgram, ModifierIrStmt};

impl fmt::Display for ModifierIrProgram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for stmt in &self.statements {
            writeln!(f, "{}", DisplayStmt(stmt))?;
        }
        Ok(())
    }
}

struct DisplayStmt<'a>(&'a ModifierIrStmt);

impl fmt::Display for DisplayStmt<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            ModifierIrStmt::Assign {
                target,
                property,
                value,
            } => write!(f, "assign {}.{} = {}", target.join("."), property, DisplayExpr(value)),
            ModifierIrStmt::Let { name, value } => {
                write!(f, "let {} = {}", name, DisplayExpr(value))
            },
            ModifierIrStmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                write!(f, "if {} {{ ", DisplayExpr(condition))?;
                for (idx, stmt) in then_branch.iter().enumerate() {
                    if idx > 0 {
                        write!(f, "; ")?;
                    }
                    write!(f, "{}", DisplayStmt(stmt))?;
                }
                write!(f, " }}")?;
                if !else_branch.is_empty() {
                    write!(f, " else {{ ")?;
                    for (idx, stmt) in else_branch.iter().enumerate() {
                        if idx > 0 {
                            write!(f, "; ")?;
                        }
                        write!(f, "{}", DisplayStmt(stmt))?;
                    }
                    write!(f, " }}")?;
                }
                Ok(())
            },
            ModifierIrStmt::For {
                var,
                index_var,
                iterable,
                body,
            } => {
                if let Some(iv) = index_var {
                    write!(f, "for {}, {} in {} {{ ", var, iv, DisplayCompiledExpr(iterable))?;
                } else {
                    write!(f, "for {} in {} {{ ", var, DisplayCompiledExpr(iterable))?;
                }
                for (idx, stmt) in body.iter().enumerate() {
                    if idx > 0 {
                        write!(f, "; ")?;
                    }
                    write!(f, "{}", DisplayStmt(stmt))?;
                }
                write!(f, " }}")
            },
            ModifierIrStmt::AssignIndexed {
                base,
                index: _,
                property,
                value,
            } => write!(f, "assign {}[<expr>].{} = {}", base, property, DisplayExpr(value)),
            ModifierIrStmt::Noop => write!(f, "noop"),
        }
    }
}

struct DisplayExpr<'a>(&'a ModifierExpr);

impl fmt::Display for DisplayExpr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            ModifierExpr::Compiled(expr) => write!(f, "{}", DisplayCompiledExpr(expr)),
            ModifierExpr::Unsupported(expr) => write!(f, "unsupported({expr:?})"),
        }
    }
}

struct DisplayCompiledExpr<'a>(&'a CompiledExpr);

impl fmt::Display for DisplayCompiledExpr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            CompiledExpr::Const(value) => write!(f, "const({value:?})"),
            CompiledExpr::LoadEnv(name) => write!(f, "load({name})"),
            CompiledExpr::MakeVec(items) => {
                write!(f, "vec(")?;
                for (idx, item) in items.iter().enumerate() {
                    if idx > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", DisplayCompiledExpr(item))?;
                }
                write!(f, ")")
            },
            CompiledExpr::Unary(op, expr) => {
                write!(f, "({op:?} {})", DisplayCompiledExpr(expr))
            },
            CompiledExpr::Binary(left, op, right) => {
                write!(f, "({} {op:?} {})", DisplayCompiledExpr(left), DisplayCompiledExpr(right))
            },
            CompiledExpr::Select(cond, then_expr, else_expr) => write!(
                f,
                "if {} then {} else {}",
                DisplayCompiledExpr(cond),
                DisplayCompiledExpr(then_expr),
                DisplayCompiledExpr(else_expr)
            ),
            CompiledExpr::CallBuiltin(name, args) => {
                write!(f, "{name:?}(")?;
                for (idx, arg) in args.iter().enumerate() {
                    if idx > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", DisplayCompiledExpr(arg))?;
                }
                write!(f, ")")
            },
            CompiledExpr::Index(container, index) => {
                write!(f, "{}[{}]", DisplayCompiledExpr(container), DisplayCompiledExpr(index))
            },
            CompiledExpr::Method(receiver, name, args) => {
                write!(f, "{}.{name}(", DisplayCompiledExpr(receiver))?;
                for (idx, arg) in args.iter().enumerate() {
                    if idx > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", DisplayCompiledExpr(arg))?;
                }
                write!(f, ")")
            },
            CompiledExpr::Closure(params, _body) => {
                write!(f, "closure({:?})", params)
            },
            CompiledExpr::Construct(name, fields) => {
                write!(f, "{name}{{")?;
                for (idx, (field, expr)) in fields.iter().enumerate() {
                    if idx > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{field}: {}", DisplayCompiledExpr(expr))?;
                }
                write!(f, "}}")
            },
            CompiledExpr::AnchorLookup { actor, anchor } => {
                write!(f, "anchor({}.{})", actor, anchor.as_str())
            },
        }
    }
}
