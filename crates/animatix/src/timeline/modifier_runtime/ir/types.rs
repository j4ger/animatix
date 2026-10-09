use std::collections::HashMap;
use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::ast::{BinaryOp, LoopPattern, UnaryOp};
use crate::timeline::Value;
use crate::timeline::animation_track::SceneAnchor;

/// Built-in mathematical and utility functions available in modifier expressions.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum BuiltinFn {
    /// Sine function.
    Sin,
    /// Cosine function.
    Cos,
    /// Linear interpolation between two values.
    Lerp,
    /// String formatting function.
    Format,
    /// Tangent function.
    Tan,
    /// Square root.
    Sqrt,
    /// Exponential function.
    Exp,
    /// Logarithm.
    Log,
    /// Two-argument arctangent.
    Atan2,
    /// Clamp a value between bounds.
    Clamp,
    /// Absolute value.
    Abs,
    /// Minimum of values.
    Min,
    /// Maximum of values.
    Max,
    /// Floor function.
    Floor,
    /// Ceiling function.
    Ceil,
    /// Convert degrees to radians.
    Deg,
    /// Convert radians to degrees.
    Rad,
    /// Swap two elements in a list (returns new list).
    ListSwap,
    /// Set an element in a list (returns new list).
    ListSet,
    /// Signum function.
    Signum,
    /// Fractional part function.
    Fract,
    /// Euclidean distance between two points.
    Hypot,
    /// Exponentiation.
    Pow,
    /// Floating-point remainder.
    Rem,
    /// Step function (`x < edge ? 0 : 1`).
    Step,
    /// Round to nearest integer.
    Round,
    /// Factorial of a non-negative integer.
    Factorial,
    /// Sum of a list of numbers.
    SumList,
    /// Sample a piecewise-linear curve at time t.
    CurveAt,
    /// Analytically smooth a curve with exponential filter kernel tau.
    CurveSmooth,
}

/// A compiled expression in the modifier IR.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum CompiledExpr {
    /// A constant value.
    Const(Value),
    /// Load a value from the environment by name.
    LoadEnv(String),
    /// Construct a vector of expressions (from paren tuples): coerces 2-4
    /// numeric elements to Vec2/3/4.
    MakeVec(Vec<CompiledExpr>),
    /// Construct a value list (from brace literals): elements are preserved
    /// as-is, mirroring the tree-walker's `Value::List`.
    MakeList(Vec<CompiledExpr>),
    /// Unary operation.
    Unary(UnaryOp, Box<CompiledExpr>),
    /// Binary operation.
    Binary(Box<CompiledExpr>, BinaryOp, Box<CompiledExpr>),
    /// Ternary conditional selection.
    Select(Box<CompiledExpr>, Box<CompiledExpr>, Box<CompiledExpr>),
    /// Call a built-in function.
    CallBuiltin(BuiltinFn, Vec<CompiledExpr>),
    /// Call a function stored in the evaluation environment by name.
    CallEnv(String, Vec<CompiledExpr>),
    /// Index into a collection.
    Index(Box<CompiledExpr>, Box<CompiledExpr>),
    /// Call a method on an expression.
    Method(Box<CompiledExpr>, String, Vec<CompiledExpr>),
    /// Create a closure value (parameter names, compiled body expression).
    /// The environment is captured at evaluation time.
    Closure(Vec<String>, Box<CompiledExpr>),
    /// A property-slot reference (`&label.prop`). Evaluates to
    /// `Value::PropRef` for query functions (`is_animating`).
    PropRef {
        /// The actor label owning the property.
        label: String,
        /// The property name.
        prop: String,
    },
    /// Block-bodied closure body: sequential `let` bindings and a tail
    /// expression, evaluated with lexical shadowing in a restored-after scope.
    LetChain(Vec<(String, CompiledExpr)>, Box<CompiledExpr>),
    /// Construct an object value (type name, compiled field expressions).
    Construct(String, Vec<(String, CompiledExpr)>),
    /// Lazily resolve an actor anchor point from the frame environment.
    /// `{actor}.{anchor}` → reads `{actor}.at` + `{actor}.size` from env.
    AnchorLookup {
        /// Actor label whose anchor point to resolve.
        actor: String,
        /// Which anchor point (top, right, center, etc.).
        anchor: SceneAnchor,
    },
}

impl CompiledExpr {
    /// Collect every property-slot reference (`&label.prop`) reachable from
    /// this expression, as `(label, prop)` pairs.
    pub fn collect_prop_refs(&self, out: &mut Vec<(String, String)>) {
        match self {
            CompiledExpr::Const(_) | CompiledExpr::LoadEnv(_) => {},
            CompiledExpr::PropRef { label, prop } => out.push((label.clone(), prop.clone())),
            CompiledExpr::MakeList(items) => {
                for item in items {
                    item.collect_prop_refs(out);
                }
            },
            CompiledExpr::MakeVec(items) => {
                for item in items {
                    item.collect_prop_refs(out);
                }
            },
            CompiledExpr::Unary(_, expr) => expr.collect_prop_refs(out),
            CompiledExpr::Binary(left, _, right) => {
                left.collect_prop_refs(out);
                right.collect_prop_refs(out);
            },
            CompiledExpr::Select(cond, then_expr, else_expr) => {
                cond.collect_prop_refs(out);
                then_expr.collect_prop_refs(out);
                else_expr.collect_prop_refs(out);
            },
            CompiledExpr::CallBuiltin(_, args) | CompiledExpr::CallEnv(_, args) => {
                for arg in args {
                    arg.collect_prop_refs(out);
                }
            },
            CompiledExpr::Index(base, index) => {
                base.collect_prop_refs(out);
                index.collect_prop_refs(out);
            },
            CompiledExpr::Method(base, _, args) => {
                base.collect_prop_refs(out);
                for arg in args {
                    arg.collect_prop_refs(out);
                }
            },
            CompiledExpr::Construct(_, fields) => {
                for (_, field) in fields {
                    field.collect_prop_refs(out);
                }
            },
            CompiledExpr::AnchorLookup { .. } => {},
            CompiledExpr::Closure(_, body) => body.collect_prop_refs(out),
            CompiledExpr::LetChain(bindings, tail) => {
                for (_, value) in bindings {
                    value.collect_prop_refs(out);
                }
                tail.collect_prop_refs(out);
            },
        }
    }

    /// Returns `true` if this compiled expression references the given identifier.
    pub fn references_ident(&self, name: &str) -> bool {
        match self {
            CompiledExpr::LoadEnv(id) => id == name,
            CompiledExpr::MakeVec(items) => items.iter().any(|item| item.references_ident(name)),
            CompiledExpr::MakeList(items) => items.iter().any(|item| item.references_ident(name)),
            CompiledExpr::Unary(_, expr) => expr.references_ident(name),
            CompiledExpr::Binary(left, _, right) => {
                left.references_ident(name) || right.references_ident(name)
            },
            CompiledExpr::Select(cond, then_expr, else_expr) => {
                cond.references_ident(name)
                    || then_expr.references_ident(name)
                    || else_expr.references_ident(name)
            },
            CompiledExpr::CallBuiltin(_, args) => args.iter().any(|arg| arg.references_ident(name)),
            CompiledExpr::CallEnv(_, args) => args.iter().any(|arg| arg.references_ident(name)),
            CompiledExpr::Index(container, index) => {
                container.references_ident(name) || index.references_ident(name)
            },
            CompiledExpr::Method(receiver, _, args) => {
                receiver.references_ident(name) || args.iter().any(|arg| arg.references_ident(name))
            },
            CompiledExpr::Closure(_, body) => body.references_ident(name),
            CompiledExpr::PropRef { label, .. } => label == name,
            CompiledExpr::LetChain(bindings, tail) => {
                bindings.iter().any(|(_, expr)| expr.references_ident(name))
                    || tail.references_ident(name)
            },
            CompiledExpr::Construct(_, fields) => {
                fields.iter().any(|(_, value)| value.references_ident(name))
            },
            CompiledExpr::Const(_) | CompiledExpr::AnchorLookup { .. } => false,
        }
    }
}

/// A statement in the modifier IR.
#[derive(Clone, Debug, PartialEq)]
pub enum ModifierIrStmt {
    /// Assign a value to a target object's property (all-static target path).
    Assign {
        /// Object path segments.
        target: Vec<String>,
        /// Property name to assign.
        property: String,
        /// Value expression.
        value: CompiledExpr,
    },
    /// Assign a value to a runtime-indexed target (e.g. `bars[i].color = red`).
    /// The base is the array label, index is compiled to a frame-time expression,
    /// property is the last segment (always static), and value is the RHS.
    AssignIndexed {
        /// Array base label (e.g. "bars").
        base: String,
        /// Frame-time index expression.
        index: CompiledExpr,
        /// Property name to assign.
        property: String,
        /// Value expression.
        value: CompiledExpr,
    },
    /// Bind a local variable.
    Let {
        /// Variable name.
        name: String,
        /// Bound expression.
        value: CompiledExpr,
    },
    /// Conditional statement.
    If {
        /// Condition expression.
        condition: CompiledExpr,
        /// Statements if true.
        then_branch: Vec<ModifierIrStmt>,
        /// Statements if false.
        else_branch: Vec<ModifierIrStmt>,
    },
    /// Loop over an iterable.
    For {
        /// Loop variable pattern (single or tuple destructuring).
        var: LoopPattern,
        /// Optional index variable name (e.g. `i` in `for item, i in items`).
        index_var: Option<String>,
        /// Iterable expression.
        iterable: CompiledExpr,
        /// Loop body statements.
        body: Vec<ModifierIrStmt>,
    },
    /// No-op statement (used as a placeholder during lowering).
    Noop,
}

/// A program in the modifier IR.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModifierIrProgram {
    /// Top-level statements.
    pub statements: Vec<ModifierIrStmt>,
}

impl ModifierIrProgram {
    /// Collect every property-slot reference (`&label.prop`) reachable from
    /// all statements (used by the build-time reference validation pass).
    pub fn collect_prop_refs(&self, out: &mut Vec<(String, String)>) {
        fn walk_stmts(stmts: &[ModifierIrStmt], out: &mut Vec<(String, String)>) {
            for stmt in stmts {
                match stmt {
                    ModifierIrStmt::Assign { value, .. }
                    | ModifierIrStmt::AssignIndexed { value, .. }
                    | ModifierIrStmt::Let { value, .. } => value.collect_prop_refs(out),
                    ModifierIrStmt::If {
                        condition,
                        then_branch,
                        else_branch,
                    } => {
                        condition.collect_prop_refs(out);
                        walk_stmts(then_branch, out);
                        walk_stmts(else_branch, out);
                    },
                    ModifierIrStmt::For { iterable, body, .. } => {
                        iterable.collect_prop_refs(out);
                        walk_stmts(body, out);
                    },
                    ModifierIrStmt::Noop => {},
                }
            }
        }
        walk_stmts(&self.statements, out);
    }

    /// Collect every target actor path (e.g. `"bg.wash"` or `"hero"`) written to
    /// by any assignment statement.
    pub fn collect_written_targets(&self, out: &mut std::collections::HashSet<String>) {
        fn walk_stmts(stmts: &[ModifierIrStmt], out: &mut std::collections::HashSet<String>) {
            for stmt in stmts {
                match stmt {
                    ModifierIrStmt::Assign { target, .. } => {
                        out.insert(target.join("."));
                        if let Some(first) = target.first() {
                            out.insert(first.clone());
                        }
                    },
                    ModifierIrStmt::AssignIndexed { base, .. } => {
                        out.insert(base.clone());
                    },
                    ModifierIrStmt::If {
                        then_branch,
                        else_branch,
                        ..
                    } => {
                        walk_stmts(then_branch, out);
                        walk_stmts(else_branch, out);
                    },
                    ModifierIrStmt::For { body, .. } => {
                        walk_stmts(body, out);
                    },
                    _ => {},
                }
            }
        }
        walk_stmts(&self.statements, out);
    }
}

/// Overrides for modifier properties, keyed by object and property name.
pub type ModifierOverrides = HashMap<String, HashMap<String, Value>>;

/// Errors during lowering to modifier IR.
#[derive(Clone, Debug, PartialEq)]
pub enum IrLowerError {
    /// A statement kind that is not supported.
    UnsupportedStatement(&'static str),
}

impl fmt::Display for IrLowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IrLowerError::UnsupportedStatement(kind) => {
                write!(f, "Unsupported IR statement: {kind}")
            },
        }
    }
}

impl std::error::Error for IrLowerError {}
