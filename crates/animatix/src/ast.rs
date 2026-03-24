#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Num(f64),
    Str(String),
    Ident(String),
    Tuple(Vec<Expr>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Property {
    pub name: String,
    pub value: Expr,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    Keyframe(String),
    LetDecl {
        name: String,
        value: Expr,
    },
    ActorDecl {
        label: String,
        ty: String,
        props: Vec<Property>,
    },
    Assignment {
        target: String,
        value: Expr,
    },
}
