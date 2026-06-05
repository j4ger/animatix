//! Symbol table extraction from the AST.

use animatix_syntax::ast::*;
use animatix_syntax::to_source::ToSource;
use std::collections::{HashMap, HashSet};

/// Expected type for a property value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertyType {
    /// Any type is acceptable.
    Any,
    /// Numeric value (integer or float).
    Num,
    /// String literal.
    String,
    /// Boolean value.
    Bool,
    /// 2D vector (x, y).
    Vec2,
    /// Color value (named color, hex, or color token).
    Color,
    /// Duration in milliseconds or seconds.
    Duration,
    /// Easing function name.
    Easing,
    /// Array/list of values.
    Array,
    /// Nested actor or component.
    Actor,
}

/// Extracted symbols from a source file.
#[derive(Debug, Default, Clone)]
pub struct SymbolTable {
    /// All labels defined in the file (actor labels, let bindings).
    pub labels: HashMap<String, LabelInfo>,
    /// Built-in types: Text, Math, Ellipse, etc.
    pub types: HashSet<String>,
    /// Components defined in this file.
    pub components: HashMap<String, ComponentInfo>,
    /// Scenes defined in this file.
    pub scenes: HashMap<String, SceneInfo>,
    /// Properties available per type: "Text" → ["content", "position", ...].
    pub properties: HashMap<String, Vec<String>>,
    /// Expected types per property: ("Text", "font_size") → PropertyType::Num.
    pub property_types: HashMap<(String, String), PropertyType>,
    /// Keywords and built-in actions.
    pub keywords: HashSet<String>,
    /// Built-in action verbs (e.g., "fade-in", "move", "rotate").
    pub actions: HashSet<String>,
    /// Imports declared in this file.
    pub imports: Vec<ImportInfo>,
    /// Labels referenced in actions/assignments (for unused label detection).
    pub referenced_labels: HashSet<String>,
}

/// Information about an import declaration.
#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// The import path (e.g., `"lib.amx"`).
    pub path: String,
    /// Optional alias for the import.
    pub alias: Option<String>,
    /// Full source span of the import statement.
    pub span: Option<Span>,
}

/// Information about a labeled entity.
#[derive(Debug, Clone)]
pub struct LabelInfo {
    /// The name of the label.
    pub name: String,
    /// The kind of label (actor, let binding, etc.).
    pub kind: LabelKind,
    /// The 1-based line number of the declaration.
    pub line: usize,
    /// The 1-based column number of the declaration.
    pub col: usize,
    /// Full source span (line/col range) for precise source write-back.
    pub span: Option<Span>,
    /// The type of the actor (e.g., "Text", "Button"), if applicable.
    pub ty: Option<String>,
}

/// The kind of labeled entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LabelKind {
    /// Actor declaration: `label: Type { ... }`
    Actor,
    /// Let binding: `let name = value`
    Let,
    /// For loop variable: `for item in ...`
    For,
    /// Always block: `always { ... }`
    Always,
    /// Component definition: `component Name { ... }`
    Component,
}

/// Information about a component definition.
#[derive(Debug, Clone)]
pub struct ComponentInfo {
    /// The component name.
    pub name: String,
    /// The list of parameters for this component.
    pub params: Vec<ParamInfo>,
    /// The 1-based line number of the declaration.
    pub line: usize,
    /// The 1-based column number of the declaration.
    pub col: usize,
    /// Full source span (line/col range) for precise source write-back.
    pub span: Option<Span>,
}

/// Information about a component parameter.
#[derive(Debug, Clone)]
pub struct ParamInfo {
    /// The parameter name.
    pub name: String,
    /// The type annotation, if any (e.g. `Num`, `Vec2`).
    pub param_type: Option<TypeAnnotation>,
    /// The default value, if any, as a source string.
    pub default: Option<String>,
}

/// Information about a scene declaration.
#[derive(Debug, Clone)]
pub struct SceneInfo {
    /// The scene name.
    pub name: String,
    /// The 1-based line number of the declaration.
    pub line: usize,
    /// The 1-based column number of the declaration.
    pub col: usize,
    /// Full source span (line/col range) for precise source write-back.
    pub span: Option<Span>,
}

/// Known built-in types in the Animatix DSL.
const BUILTIN_TYPES: &[&str] = &[
    "Text", "Math", "Code", "Svg", "Image",
    "Rect", "Ellipse", "Line", "Polygon", "Path",
    "Graph", "PlotCurve",
    "Button",
];

/// Known built-in actions.
const BUILTIN_ACTIONS: &[&str] = &[
    "fade-in", "draw-in", "wipe-in",
    "fade-out", "wipe-out", "reveal-out", "draw-out",
    "move", "shift", "rotate", "scale",
];

/// Known keywords.
const KEYWORDS: &[&str] = &[
    "let", "import", "always", "if", "else", "for", "in",
    "pub", "component", "sequence", "stagger",
];

/// Known properties per type.
fn known_properties() -> HashMap<String, Vec<String>> {
    let mut map = HashMap::new();

    // Common properties shared by most actors
    let common = vec![
        "position".to_string(),
        "anchor".to_string(),
        "offset".to_string(),
        "scale".to_string(),
        "rotation".to_string(),
        "opacity".to_string(),
        "color".to_string(),
    ];

    // Text-specific
    let mut text_props = common.clone();
    text_props.extend([
        "content".to_string(),
        "font_size".to_string(),
        "font_family".to_string(),
        "text_align".to_string(),
    ]);
    map.insert("Text".to_string(), text_props);

    // Math-specific
    let mut math_props = common.clone();
    math_props.extend([
        "content".to_string(),
        "font_size".to_string(),
    ]);
    map.insert("Math".to_string(), math_props);

    // Code-specific
    let mut code_props = common.clone();
    code_props.extend([
        "content".to_string(),
        "language".to_string(),
    ]);
    map.insert("Code".to_string(), code_props);

    // Shape-specific (Rect, Ellipse, etc.)
    let mut shape_props = common.clone();
    shape_props.extend([
        "fill".to_string(),
        "stroke".to_string(),
        "stroke_width".to_string(),
        "size".to_string(),
        "radius".to_string(),
    ]);
    for shape in &["Rect", "Ellipse", "Polygon"] {
        map.insert(shape.to_string(), shape_props.clone());
    }

    // Line
    let mut line_props = common.clone();
    line_props.extend([
        "start".to_string(),
        "end".to_string(),
        "stroke".to_string(),
        "stroke_width".to_string(),
    ]);
    map.insert("Line".to_string(), line_props);

    // Button
    let mut button_props = common.clone();
    button_props.extend([
        "text".to_string(),
        "size".to_string(),
        "fill".to_string(),
        "stroke".to_string(),
    ]);
    map.insert("Button".to_string(), button_props);

    // Svg/Image
    let mut media_props = common.clone();
    media_props.extend([
        "url".to_string(),
        "size".to_string(),
    ]);
    map.insert("Svg".to_string(), media_props.clone());
    map.insert("Image".to_string(), media_props);

    // Graph types
    let mut graph_props = common.clone();
    graph_props.extend([
        "x_range".to_string(),
        "y_range".to_string(),
        "function".to_string(),
    ]);
    for graph in &["Graph", "PlotCurve"] {
        map.insert(graph.to_string(), graph_props.clone());
    }

    map
}

/// Known property types per (type, property) pair.
fn known_property_types() -> HashMap<(String, String), PropertyType> {
    let mut map = HashMap::new();

    // Common properties
    for ty in &["Text", "Math", "Code", "Rect", "Ellipse", "Polygon", "Line", "Button", "Svg", "Image", "Graph", "PlotCurve"] {
        map.insert((ty.to_string(), "position".to_string()), PropertyType::Vec2);
        map.insert((ty.to_string(), "offset".to_string()), PropertyType::Vec2);
        map.insert((ty.to_string(), "scale".to_string()), PropertyType::Num);
        map.insert((ty.to_string(), "rotation".to_string()), PropertyType::Num);
        map.insert((ty.to_string(), "opacity".to_string()), PropertyType::Num);
        map.insert((ty.to_string(), "color".to_string()), PropertyType::Color);
    }

    // Text-specific
    map.insert(("Text".to_string(), "content".to_string()), PropertyType::String);
    map.insert(("Text".to_string(), "font_size".to_string()), PropertyType::Num);
    map.insert(("Text".to_string(), "font_family".to_string()), PropertyType::String);
    map.insert(("Text".to_string(), "text_align".to_string()), PropertyType::String);

    // Math-specific
    map.insert(("Math".to_string(), "content".to_string()), PropertyType::String);
    map.insert(("Math".to_string(), "font_size".to_string()), PropertyType::Num);

    // Code-specific
    map.insert(("Code".to_string(), "content".to_string()), PropertyType::String);
    map.insert(("Code".to_string(), "language".to_string()), PropertyType::String);

    // Shape-specific
    for shape in &["Rect", "Ellipse", "Polygon"] {
        map.insert((shape.to_string(), "fill".to_string()), PropertyType::Color);
        map.insert((shape.to_string(), "stroke".to_string()), PropertyType::Color);
        map.insert((shape.to_string(), "stroke_width".to_string()), PropertyType::Num);
        map.insert((shape.to_string(), "size".to_string()), PropertyType::Vec2);
        map.insert((shape.to_string(), "radius".to_string()), PropertyType::Num);
    }

    // Line
    map.insert(("Line".to_string(), "start".to_string()), PropertyType::Vec2);
    map.insert(("Line".to_string(), "end".to_string()), PropertyType::Vec2);
    map.insert(("Line".to_string(), "stroke".to_string()), PropertyType::Color);
    map.insert(("Line".to_string(), "stroke_width".to_string()), PropertyType::Num);

    // Button
    map.insert(("Button".to_string(), "text".to_string()), PropertyType::String);
    map.insert(("Button".to_string(), "size".to_string()), PropertyType::Vec2);
    map.insert(("Button".to_string(), "fill".to_string()), PropertyType::Color);
    map.insert(("Button".to_string(), "stroke".to_string()), PropertyType::Color);

    // Svg/Image
    for media in &["Svg", "Image"] {
        map.insert((media.to_string(), "url".to_string()), PropertyType::String);
        map.insert((media.to_string(), "size".to_string()), PropertyType::Vec2);
    }

    // Graph
    map.insert(("Graph".to_string(), "x_range".to_string()), PropertyType::Vec2);
    map.insert(("Graph".to_string(), "y_range".to_string()), PropertyType::Vec2);
    map.insert(("Graph".to_string(), "function".to_string()), PropertyType::String);

    map
}

impl SymbolTable {
    /// Build a symbol table from parsed AST statements.
    pub fn build_from_ast(stmts: &[Stmt]) -> Self {
        let mut table = Self {
            types: BUILTIN_TYPES.iter().map(|s| s.to_string()).collect(),
            keywords: KEYWORDS.iter().map(|s| s.to_string()).collect(),
            actions: BUILTIN_ACTIONS.iter().map(|s| s.to_string()).collect(),
            properties: known_properties(),
            property_types: known_property_types(),
            scenes: HashMap::new(),
            ..Default::default()
        };

        for stmt in stmts {
            table.collect_stmt(stmt);
        }

        table
    }

    fn collect_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::LetDecl { name, span, .. } => {
                self.labels.insert(name.clone(), LabelInfo {
                    name: name.clone(),
                    kind: LabelKind::Let,
                    line: 0, // populated by Analyzer::enrich_positions from tree-sitter
                    col: 0,   // populated by Analyzer::enrich_positions from tree-sitter
                    span: *span,
                    ty: None,
                });
            }

            Stmt::ActorDecl { label, ty, span, .. } => {
                self.labels.insert(label.clone(), LabelInfo {
                    name: label.clone(),
                    kind: LabelKind::Actor,
                    line: 0, // populated by Analyzer::enrich_positions from tree-sitter
                    col: 0,   // populated by Analyzer::enrich_positions from tree-sitter
                    span: *span,
                    ty: Some(ty.clone()),
                });
            }

            Stmt::ComponentDef(def, span) => {
                self.components.insert(def.name.clone(), ComponentInfo {
                    name: def.name.clone(),
                    params: def.params.iter().map(|p| ParamInfo {
                        name: p.name.clone(),
                        param_type: p.param_type.clone(),
                        default: p.default.as_ref().map(|e| e.to_source()),
                    }).collect(),
                    line: 0, // populated by Analyzer::enrich_positions from tree-sitter
                    col: 0,   // populated by Analyzer::enrich_positions from tree-sitter
                    span: *span,
                });

                // Recurse into component body
                for stmt in &def.body {
                    self.collect_stmt(stmt);
                }
            }

            Stmt::ForLoop { var, body, span, .. } => {
                self.labels.insert(var.clone(), LabelInfo {
                    name: var.clone(),
                    kind: LabelKind::For,
                    line: 0, // populated by Analyzer::enrich_positions from tree-sitter
                    col: 0,   // populated by Analyzer::enrich_positions from tree-sitter
                    span: *span,
                    ty: None,
                });

                for stmt in body {
                    self.collect_stmt(stmt);
                }
            }

            // Recurse into blocks
            Stmt::Keyframe { body, .. } | Stmt::RelativeKeyframe { body, .. } => {
                for stmt in body {
                    self.collect_stmt(stmt);
                }
            }
            Stmt::Sequence { body, .. } | Stmt::Stagger { body, .. } | Stmt::Always { body, .. } => {
                for stmt in body {
                    self.collect_stmt(stmt);
                }
            }
            Stmt::Conditional { then_branch, else_branch, .. } => {
                for stmt in then_branch {
                    self.collect_stmt(stmt);
                }
                if let Some(else_stmts) = else_branch {
                    for stmt in else_stmts {
                        self.collect_stmt(stmt);
                    }
                }
            }

            Stmt::Scene { name, span, .. } => {
                self.scenes.insert(name.clone(), SceneInfo {
                    name: name.clone(),
                    line: 0, // populated by Analyzer::enrich_positions from tree-sitter
                    col: 0,   // populated by Analyzer::enrich_positions from tree-sitter
                    span: *span,
                });
                // Recurse into scene body
                if let Stmt::Scene { body, .. } = stmt {
                    for s in body {
                        self.collect_stmt(s);
                    }
                }
            }

            Stmt::Play { .. } => {
                // No new symbols to declare, but we could track play references here.
            }

            Stmt::Import { path, alias, span, .. } => {
                self.imports.push(ImportInfo {
                    path: path.clone(),
                    alias: alias.clone(),
                    span: *span,
                });
            }

            // Actions, assignments, etc. — no symbols to extract
            _ => {}
        }
    }

    /// Collect all label references from the AST (for unused label detection).
    pub fn collect_references(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            self.collect_refs_from_stmt(stmt);
        }
    }

    fn collect_refs_from_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Action(action, ..) => {
                for target in &action.targets {
                    self.referenced_labels.insert(target.clone());
                }
            }
            Stmt::Assignment { target, .. } => {
                for label in target {
                    self.referenced_labels.insert(label.clone());
                }
            }
            Stmt::Play { scene_name, .. } => {
                self.referenced_labels.insert(scene_name.clone());
            }
            // Recurse into blocks
            Stmt::Keyframe { body, .. } | Stmt::RelativeKeyframe { body, .. } => {
                for stmt in body {
                    self.collect_refs_from_stmt(stmt);
                }
            }
            Stmt::Sequence { body, .. } | Stmt::Stagger { body, .. } | Stmt::Always { body, .. } => {
                for stmt in body {
                    self.collect_refs_from_stmt(stmt);
                }
            }
            Stmt::Conditional { then_branch, else_branch, .. } => {
                for stmt in then_branch {
                    self.collect_refs_from_stmt(stmt);
                }
                if let Some(else_stmts) = else_branch {
                    for stmt in else_stmts {
                        self.collect_refs_from_stmt(stmt);
                    }
                }
            }
            Stmt::ForLoop { body, .. } => {
                for stmt in body {
                    self.collect_refs_from_stmt(stmt);
                }
            }
            Stmt::ComponentDef(def, ..) => {
                for stmt in &def.body {
                    self.collect_refs_from_stmt(stmt);
                }
            }
            Stmt::Scene { body, .. } => {
                for stmt in body {
                    self.collect_refs_from_stmt(stmt);
                }
            }
            Stmt::ReactiveBinding { target, .. } => {
                for label in target {
                    self.referenced_labels.insert(label.clone());
                }
            }
            _ => {}
        }
    }

    /// Merge symbols from another table into this one.
    /// Used for cross-file analysis: imported file symbols are merged
    /// into the local symbol table.
    pub fn merge(&mut self, other: &SymbolTable) {
        for (name, info) in &other.labels {
            if !self.labels.contains_key(name) {
                self.labels.insert(name.clone(), info.clone());
            }
        }
        for (name, info) in &other.components {
            if !self.components.contains_key(name) {
                self.components.insert(name.clone(), info.clone());
            }
        }
        for (name, info) in &other.scenes {
            if !self.scenes.contains_key(name) {
                self.scenes.insert(name.clone(), info.clone());
            }
        }
        for (name, props) in &other.properties {
            let entry = self.properties.entry(name.clone()).or_default();
            for prop in props {
                if !entry.contains(prop) {
                    entry.push(prop.clone());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_types_populated() {
        let stmts = vec![];
        let table = SymbolTable::build_from_ast(&stmts);
        assert!(table.types.contains("Text"));
        assert!(table.types.contains("Ellipse"));
        assert!(table.types.contains("Button"));
    }

    #[test]
    fn extracts_actor_labels() {
        let stmts = vec![
            Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "btn".to_string(),
                ty: "Button".to_string(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
                span: None,
            },
        ];
        let table = SymbolTable::build_from_ast(&stmts);
        assert!(table.labels.contains_key("btn"));
        assert_eq!(table.labels["btn"].ty.as_deref(), Some("Button"));
    }

    #[test]
    fn extracts_let_bindings() {
        let stmts = vec![
            Stmt::LetDecl {
                is_pub: false,
                name: "x".to_string(),
                value: Expr::Num(42.0),
                span: None,
            },
        ];
        let table = SymbolTable::build_from_ast(&stmts);
        assert!(table.labels.contains_key("x"));
        assert_eq!(table.labels["x"].kind, LabelKind::Let);
    }

    #[test]
    fn extracts_component_definitions() {
        let stmts = vec![
            Stmt::ComponentDef(ComponentDef {
                is_pub: false,
                name: "MyButton".to_string(),
                params: vec![
                    ParamDef {
                        name: "text".to_string(),
                        param_type: None,
                        default: Some(Expr::Str("Click".to_string())),
                    },
                ],
                body: vec![],
            }, None),
        ];
        let table = SymbolTable::build_from_ast(&stmts);
        assert!(table.components.contains_key("MyButton"));
        assert_eq!(table.components["MyButton"].params.len(), 1);
    }

    #[test]
    fn collects_properties_from_actors() {
        let stmts = vec![
            Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "title".to_string(),
                ty: "Text".to_string(),
                props: vec![
                    Property {
                        name: "content".to_string(),
                        value: Expr::Str("Hello".to_string()),
                        value_span: None,
                        trailing_comment: None,
                    },
                    Property {
                        name: "font_size".to_string(),
                        value: Expr::Num(24.0),
                        value_span: None,
                        trailing_comment: None,
                    },
                ],
                modifiers: vec![],
                children: vec![],
                span: None,
            },
        ];
        let table = SymbolTable::build_from_ast(&stmts);
        let text_props = table.properties.get("Text").unwrap();
        assert!(text_props.contains(&"content".to_string()));
        assert!(text_props.contains(&"font_size".to_string()));
    }
}

/// Infer the type of an expression for type checking.
pub fn infer_expr_type(expr: &Expr) -> PropertyType {
    match expr {
        Expr::Num(_) => PropertyType::Num,
        Expr::Percent(_) => PropertyType::Num,
        Expr::Str(_) => PropertyType::String,
        Expr::Bool(_) => PropertyType::Bool,
        Expr::Null => PropertyType::Any,
        Expr::Tuple(elements) => {
            if elements.len() == 2 {
                PropertyType::Vec2
            } else {
                PropertyType::Array
            }
        }
        Expr::Ident(_) => PropertyType::Any,
        Expr::Path(_) => PropertyType::Any, // e.g., text.primary
        Expr::Index(_, _) => PropertyType::Any,
        Expr::Binary(_, _, _) => PropertyType::Num,
        Expr::Unary(_, _) => PropertyType::Num,
        Expr::Call(_, _) => PropertyType::Any,
        Expr::Method(_, _, _) => PropertyType::Any,
        Expr::Closure(_, _) => PropertyType::Any,
        Expr::Conditional(_, _, _) => PropertyType::Any,
        Expr::Construct(_, _) => PropertyType::Actor,
    }
}
