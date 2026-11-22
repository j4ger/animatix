//! Visual roles shared by the engine, type checker, and editor.

/// A standard role with a stable source spelling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Title,
    Body,
    Note,
    Formula,
    Metric,
    Eyebrow,
    Reference,
    Focus,
}

/// The standard roles in completion and documentation order.
pub const ROLES: &[(Role, &str)] = &[
    (Role::Title, "title"),
    (Role::Body, "body"),
    (Role::Note, "note"),
    (Role::Formula, "formula"),
    (Role::Metric, "metric"),
    (Role::Eyebrow, "eyebrow"),
    (Role::Reference, "reference"),
    (Role::Focus, "focus"),
];

/// Actor types covered by the built-in role property.
pub const ROLE_ACTORS: &[&str] = &["Text", "Typst", "Math", "Counter", "PlotCurve"];

/// Numeric layout values supplied by every scene style.
pub const STYLE_TOKENS: &[&str; 4] = &["margin", "gap", "inset", "radius"];

impl Role {
    /// Look up a role by its source spelling.
    pub fn from_name(name: &str) -> Option<Self> {
        ROLES.iter().find_map(|&(role, spelling)| (spelling == name).then_some(role))
    }

    /// Whether an actor accepts this role.
    pub fn applies_to(self, actor: &str) -> bool {
        match self {
            Self::Reference | Self::Focus => actor == "PlotCurve",
            _ => actor != "PlotCurve" && ROLE_ACTORS.contains(&actor),
        }
    }
}
