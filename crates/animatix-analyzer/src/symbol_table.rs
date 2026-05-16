//! Symbol table extraction from the AST.

use animatix::ast::*;
use animatix::to_source::ToSource;
use std::collections::{HashMap, HashSet};

/// Extracted symbols from a source file.
#[derive(Debug, Default, Clone)]
pub struct SymbolTable {
    /// All labels defined in the file (actor labels, let bindings).
    pub labels: HashMap<String, LabelInfo>,
    /// Built-in types: Text, Math, Circle, etc.
    pub types: HashSet<String>,
    /// Components defined in this file.
    pub components: HashMap<String, ComponentInfo>,
    /// Properties available per type: "Text" → ["content", "position", ...].
    pub properties: HashMap<String, Vec<String>>,
    /// Keywords and built-in actions.
    pub keywords: HashSet<String>,
    pub actions: HashSet<String>,
    /// Imports declared in this file.
    pub imports: Vec<ImportInfo>,
}

/// Information about an import declaration.
#[derive(Debug, Clone)]
pub struct ImportInfo {
    pub path: String,
    pub alias: Option<String>,
}

/// Information about a labeled entity.
#[derive(Debug, Clone)]
pub struct LabelInfo {
    pub name: String,
    pub kind: LabelKind,
    pub line: usize,
    pub col: usize,
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
    /// Labeled always block: `label: always { ... }`
    Always,
    /// Component definition: `component Name { ... }`
    Component,
}

/// Information about a component definition.
#[derive(Debug, Clone)]
pub struct ComponentInfo {
    pub name: String,
    pub params: Vec<ParamInfo>,
    pub line: usize,
    pub col: usize,
}

/// Information about a component parameter.
#[derive(Debug, Clone)]
pub struct ParamInfo {
    pub name: String,
    pub default: Option<String>,
}

/// Known built-in types in the Animatix DSL.
const BUILTIN_TYPES: &[&str] = &[
    "Text", "Math", "Code", "Svg", "Image",
    "Circle", "Dot", "Rect", "Square",
    "Line", "Arrow", "Ellipse", "Arc",
    "Polygon", "RegularPolygon", "Path",
    "Graph", "CartesianPlot", "PolarPlot",
    "ParametricPlot", "ImplicitPlot",
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

    // Shape-specific (Circle, Rect, etc.)
    let mut shape_props = common.clone();
    shape_props.extend([
        "fill".to_string(),
        "stroke".to_string(),
        "stroke_width".to_string(),
        "size".to_string(),
        "radius".to_string(),
    ]);
    for shape in &["Circle", "Dot", "Rect", "Square", "Ellipse", "Arc", "Polygon", "RegularPolygon"] {
        map.insert(shape.to_string(), shape_props.clone());
    }

    // Line/Arrow
    let mut line_props = common.clone();
    line_props.extend([
        "start".to_string(),
        "end".to_string(),
        "stroke".to_string(),
        "stroke_width".to_string(),
    ]);
    map.insert("Line".to_string(), line_props.clone());
    map.insert("Arrow".to_string(), line_props);

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
    for graph in &["Graph", "CartesianPlot", "PolarPlot", "ParametricPlot", "ImplicitPlot"] {
        map.insert(graph.to_string(), graph_props.clone());
    }

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
            ..Default::default()
        };

        for stmt in stmts {
            table.collect_stmt(stmt);
        }

        table
    }

    fn collect_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::LetDecl { name, .. } => {
                self.labels.insert(name.clone(), LabelInfo {
                    name: name.clone(),
                    kind: LabelKind::Let,
                    line: 0, // populated by Analyzer::enrich_positions from tree-sitter
                    col: 0,   // populated by Analyzer::enrich_positions from tree-sitter
                    ty: None,
                });
            }

            Stmt::ActorDecl { label, ty, .. } => {
                self.labels.insert(label.clone(), LabelInfo {
                    name: label.clone(),
                    kind: LabelKind::Actor,
                    line: 0, // populated by Analyzer::enrich_positions from tree-sitter
                    col: 0,   // populated by Analyzer::enrich_positions from tree-sitter
                    ty: Some(ty.clone()),
                });
            }

            Stmt::Text { label, props, .. } => {
                if let Some(label) = label {
                    self.labels.insert(label.clone(), LabelInfo {
                        name: label.clone(),
                        kind: LabelKind::Actor,
                        line: 0,
                        col: 0,
                        ty: Some("Text".to_string()),
                    });
                    // Collect properties seen in this actor
                    self.collect_actor_properties("Text", props);
                }
            }

            Stmt::Math { label, props, .. } => {
                if let Some(label) = label {
                    self.labels.insert(label.clone(), LabelInfo {
                        name: label.clone(),
                        kind: LabelKind::Actor,
                        line: 0,
                        col: 0,
                        ty: Some("Math".to_string()),
                    });
                    self.collect_actor_properties("Math", props);
                }
            }

            Stmt::Code { label, props, .. } => {
                if let Some(label) = label {
                    self.labels.insert(label.clone(), LabelInfo {
                        name: label.clone(),
                        kind: LabelKind::Actor,
                        line: 0,
                        col: 0,
                        ty: Some("Code".to_string()),
                    });
                    self.collect_actor_properties("Code", props);
                }
            }

            Stmt::ComponentDef(def, ..) => {
                self.components.insert(def.name.clone(), ComponentInfo {
                    name: def.name.clone(),
                    params: def.params.iter().map(|p| ParamInfo {
                        name: p.name.clone(),
                        default: p.default.as_ref().map(|e| e.to_source()),
                    }).collect(),
                    line: 0, // populated by Analyzer::enrich_positions from tree-sitter
                    col: 0,   // populated by Analyzer::enrich_positions from tree-sitter
                });

                // Recurse into component body
                for stmt in &def.body {
                    self.collect_stmt(stmt);
                }
            }

            Stmt::ForLoop { var, body, .. } => {
                self.labels.insert(var.clone(), LabelInfo {
                    name: var.clone(),
                    kind: LabelKind::For,
                    line: 0, // populated by Analyzer::enrich_positions from tree-sitter
                    col: 0,   // populated by Analyzer::enrich_positions from tree-sitter
                    ty: None,
                });

                for stmt in body {
                    self.collect_stmt(stmt);
                }
            }

            Stmt::LabeledAlways { label, body, .. } => {
                self.labels.insert(label.clone(), LabelInfo {
                    name: label.clone(),
                    kind: LabelKind::Always,
                    line: 0, // populated by Analyzer::enrich_positions from tree-sitter
                    col: 0,   // populated by Analyzer::enrich_positions from tree-sitter
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

            Stmt::Import { path, alias, .. } => {
                self.imports.push(ImportInfo {
                    path: path.clone(),
                    alias: alias.clone(),
                });
            }

            // Actions, assignments, etc. — no symbols to extract
            _ => {}
        }
    }

    /// Collect properties seen in an actor declaration.
    fn collect_actor_properties(&mut self, ty: &str, props: &[Property]) {
        let entry = self.properties.entry(ty.to_string()).or_default();
        for prop in props {
            if !entry.contains(&prop.name) {
                entry.push(prop.name.clone());
            }
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
        assert!(table.types.contains("Circle"));
        assert!(table.types.contains("Button"));
    }

    #[test]
    fn extracts_actor_labels() {
        let stmts = vec![
            Stmt::ActorDecl {
                is_pub: false,
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
            Stmt::Text {
                label: Some("title".to_string()),
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
                span: None,
            },
        ];
        let table = SymbolTable::build_from_ast(&stmts);
        let text_props = table.properties.get("Text").unwrap();
        assert!(text_props.contains(&"content".to_string()));
        assert!(text_props.contains(&"font_size".to_string()));
    }
}
