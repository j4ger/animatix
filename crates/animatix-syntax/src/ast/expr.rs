use super::*;

/// Expression types for the Animatix language.
/// Represents any value-computing element: literals, math, logic, function
/// calls, and more. Used in property values, conditions, and reactive blocks.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Expr {
    /// Numeric literal (e.g. `42`, `3.14`).
    Num(f64),
    /// Percentage literal (e.g. `50%`).
    Percent(f64),
    /// String literal (e.g. `"hello"`).
    Str(String),
    /// Boolean literal (`true` or `false`).
    Bool(bool),
    /// Null literal.
    Null,

    // Identifiers & Access
    /// Variable identifier (e.g. `x`, `slider_value`).
    Ident(String),
    /// Path access for nested identifiers (e.g. `container.child`).
    Path(Vec<String>),
    /// Index or subscript expression (e.g. `items[0]`, `children[i]`).
    Index(Box<Expr>, Box<Expr>),

    // Collections
    /// Tuple/vector literal `(x, y)`. Fixed-size: Vec2, Vec4, Color, domains.
    /// Length 2-4 is inferred as the corresponding vector type at the type level.
    Tuple(Vec<Expr>),
    /// List literal `{a, b, c}`. Variadic/homogeneous array.
    /// Used for points, commands, levels, data, for-iterables, etc.
    List(Vec<Expr>),

    // Operators
    /// Binary operation (e.g. `x + y`, `a > b`).
    Binary(Box<Expr>, BinaryOp, Box<Expr>),
    /// Unary operation (e.g. `-x`, `!flag`).
    Unary(UnaryOp, Box<Expr>),

    // Functions & Methods
    /// Function call (e.g. `format("{}", x)`).
    Call(String, Vec<Expr>),
    /// Method call on a receiver (e.g. `graph.plot(func)`).
    Method(Box<Expr>, String, Vec<Expr>),
    /// Closure / lambda expression (e.g. `(x) => x ^ 2`).
    Closure(Vec<String>, Box<Expr>),
    /// Block-bodied closure body with sequential `let` bindings and a tail
    /// expression: `(x) => { let a = x * 2; let b = a + 1; b }`. Bindings
    /// shadow lexically in order; the tail is the value.
    LetChain(Vec<(String, Expr)>, Box<Expr>),

    // Conditionals
    /// Conditional expression (e.g. `if cond { a } else { b }`).
    Conditional(Box<Expr>, Box<Expr>, Box<Expr>),

    // Match
    /// Match expression (e.g. `match v { 0 => red, _ => white }`).
    /// First matching arm's value is returned; `_` wildcard arm is required.
    Match(Box<Expr>, Vec<(MatchPattern, Box<Expr>)>),

    // Type Construction (for inline morph targets)
    /// Type construction for inline morph targets (e.g. `Button, text: "OK"`).
    Construct(String, Vec<Property>),
}

impl Expr {
    /// Returns true if this expression (or any sub-expression) references
    /// the given identifier name.
    pub fn references_ident(&self, name: &str) -> bool {
        match self {
            Expr::Ident(ident) => ident == name,
            Expr::Path(parts) => parts.iter().any(|p| p == name),
            Expr::Index(container, index) => {
                container.references_ident(name) || index.references_ident(name)
            },
            Expr::Tuple(items) => items.iter().any(|item| item.references_ident(name)),
            Expr::List(items) => items.iter().any(|item| item.references_ident(name)),
            Expr::Binary(left, _, right) => {
                left.references_ident(name) || right.references_ident(name)
            },
            Expr::Unary(_, expr) => expr.references_ident(name),
            Expr::Call(_, args) => args.iter().any(|arg| arg.references_ident(name)),
            Expr::Method(receiver, _, args) => {
                receiver.references_ident(name) || args.iter().any(|arg| arg.references_ident(name))
            },
            Expr::Closure(_, body) => body.references_ident(name),
            Expr::LetChain(bindings, tail) => {
                bindings.iter().any(|(_, expr)| expr.references_ident(name))
                    || tail.references_ident(name)
            },
            Expr::Conditional(cond, then_branch, else_branch) => {
                cond.references_ident(name)
                    || then_branch.references_ident(name)
                    || else_branch.references_ident(name)
            },
            Expr::Match(scrutinee, arms) => {
                if scrutinee.references_ident(name) {
                    return true;
                }
                for (_pattern, arm_expr) in arms {
                    if arm_expr.references_ident(name) {
                        return true;
                    }
                }
                false
            },
            Expr::Construct(_, props) => props.iter().any(|p| p.value.references_ident(name)),
            // Literals never reference an identifier
            _ => false,
        }
    }
}

/// Binary operators supported by the Animatix language.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum BinaryOp {
    /// Addition (`+`).
    Add,
    /// Subtraction (`-`).
    Sub,
    /// Multiplication (`*`).
    Mul,
    /// Division (`/`).
    Div,
    /// Modulo (`%`).
    Mod,
    /// Power (`^`).
    Pow,
    /// Equality (`==`).
    Eq,
    /// Inequality (`!=`).
    Neq,
    /// Less than (`<`).
    Lt,
    /// Greater than (`>`).
    Gt,
    /// Less than or equal (`<=`).
    Lte,
    /// Greater than or equal (`>=`).
    Gte,
    /// Logical AND (`&&`).
    And,
    /// Logical OR (`||`).
    Or,
}

/// Unary operators supported by the Animatix language.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum UnaryOp {
    /// Negation (`-`).
    Neg,
    /// Logical NOT (`!`).
    Not,
    /// Property reference (`&actor.prop`): packages the slot as a
    /// `PropRef` value for property-state query functions. The operand is
    /// restricted to a two-segment `label.property` path at parse time.
    Ref,
}

// ----------------------------------------------------------------------------
// 2. Properties & Modifiers
// ----------------------------------------------------------------------------
// Key-value pairs used for actor configuration and action modifiers.
// ----------------------------------------------------------------------------
