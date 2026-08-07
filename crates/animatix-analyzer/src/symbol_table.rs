//! Symbol table extraction from the AST.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use animatix_syntax::ast::*;
use animatix_syntax::to_source::ToSource;
use animatix_syntax::typing;

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
    /// Expected types per property: ("Text", "font_size") → `typing::Type::Num`.
    pub property_types: HashMap<(String, String), typing::Type>,
    /// Keywords and built-in actions.
    pub keywords: HashSet<String>,
    /// Built-in action verbs (e.g., "fade-in", "move", "rotate").
    pub actions: HashSet<String>,
    /// Imports declared in this file.
    pub imports: Vec<ImportInfo>,
    /// Labels referenced in actions/assignments (for unused label detection).
    pub referenced_labels: HashSet<String>,
    /// Labels of array-indexed actors (e.g., `bars[i]: Rect`).
    /// Targets like `bars__0`, `bars__1` are considered defined.
    pub array_labels: HashSet<String>,
    /// Labels declared inside component templates, which are not scene actors.
    pub component_internal_labels: HashSet<String>,
    /// Namespaced symbols from aliased imports (e.g., "foo" → SymbolTable).
    pub namespaces: HashMap<String, SymbolTable>,
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
    /// Inferred expression type for variables and actors.
    pub inferred_type: Option<typing::Type>,
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
    // Shapes
    "Rect",
    "Ellipse",
    "Line",
    "Arrow",
    "Polygon",
    "Path",
    // Text
    "Text",
    "Code",
    "Typst",
    // Media
    "Image",
    "Svg",
    "Audio",
    // Plots
    "Graph",
    "PlotCurve",
    "VectorField",
    "Heatmap",
    "ContourSet",
    "NumberPlane",
    "BarChart",
    // Containers
    "Row",
    "Col",
    "Grid",
    "Stack",
    "Group",
    "Mask",
    "Filter",
    // Equation / Fragment
    "Equation",
    "Fragment",
    // Annotations
    "Callout",
    "Legend",
    // Built-in component (handled by component system, not a primitive)
    "Button",
];

/// Known built-in actions.
const BUILTIN_ACTIONS: &[&str] = &[
    "fade-in",
    "draw-in",
    "wipe-in",
    "reveal-in",
    "fade-out",
    "wipe-out",
    "reveal-out",
    "draw-out",
    "move",
    "shift",
    "rotate",
    "scale",
    "shake",
    "pulse",
    "bounce",
    "highlight",
    "unhighlight",
    "persist",
    "remove",
    "swap",
    "reorder",
];

/// Known keywords.
const KEYWORDS: &[&str] = &[
    "let",
    "import",
    "always",
    "if",
    "else",
    "for",
    "in",
    "pub",
    "component",
    "sequence",
    "stagger",
];

/// Known properties per type.
fn known_properties() -> &'static HashMap<String, Vec<String>> {
    static CACHE: OnceLock<HashMap<String, Vec<String>>> = OnceLock::new();
    CACHE.get_or_init(|| {
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
            "at".to_string(),
        ];

        // Text-specific
        let mut text_props = common.clone();
        text_props.extend([
            "content".to_string(),
            "text".to_string(),
            "font_size".to_string(),
            "font_family".to_string(),
            "font_weight".to_string(),
            "font_style".to_string(),
            "line_height".to_string(),
            "letter_spacing".to_string(),
            "word_spacing".to_string(),
            "max_width".to_string(),
            "text_align".to_string(),
            "overflow".to_string(),
        ]);
        map.insert("Text".to_string(), text_props);

        // Typst-specific (shares Text props)
        let mut typst_props = common.clone();
        typst_props.extend([
            "content".to_string(),
            "font_size".to_string(),
            "font_family".to_string(),
            "font_weight".to_string(),
            "font_style".to_string(),
            "line_height".to_string(),
            "letter_spacing".to_string(),
            "word_spacing".to_string(),
            "max_width".to_string(),
            "text_align".to_string(),
            "overflow".to_string(),
        ]);
        map.insert("Typst".to_string(), typst_props);

        // Code-specific
        let mut code_props = common.clone();
        code_props.extend([
            "code".to_string(),
            "content".to_string(),
            "language".to_string(),
            "font_weight".to_string(),
            "font_style".to_string(),
            "line_height".to_string(),
            "letter_spacing".to_string(),
            "word_spacing".to_string(),
            "max_width".to_string(),
            "text_align".to_string(),
            "overflow".to_string(),
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
        media_props.extend(["url".to_string(), "size".to_string()]);
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
    })
}

/// Known property types per (type, property) pair.
fn known_property_types() -> &'static HashMap<(String, String), typing::Type> {
    static CACHE: OnceLock<HashMap<(String, String), typing::Type>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mut map = HashMap::new();

        // Common properties
        for ty in &[
            "Text",
            "Code",
            "Rect",
            "Ellipse",
            "Polygon",
            "Line",
            "Button",
            "Svg",
            "Image",
            "Graph",
            "PlotCurve",
        ] {
            map.insert((ty.to_string(), "position".to_string()), typing::Type::Vec2);
            map.insert((ty.to_string(), "offset".to_string()), typing::Type::Vec2);
            map.insert((ty.to_string(), "scale".to_string()), typing::Type::Num);
            map.insert((ty.to_string(), "rotation".to_string()), typing::Type::Num);
            map.insert((ty.to_string(), "opacity".to_string()), typing::Type::Num);
            map.insert((ty.to_string(), "color".to_string()), typing::Type::Color);
            map.insert((ty.to_string(), "at".to_string()), typing::Type::Vec2);
        }

        // Text-specific
        map.insert(("Text".to_string(), "text".to_string()), typing::Type::Str);
        map.insert(("Text".to_string(), "content".to_string()), typing::Type::Str);
        map.insert(("Text".to_string(), "font_size".to_string()), typing::Type::Num);
        map.insert(("Text".to_string(), "font_family".to_string()), typing::Type::Str);
        map.insert(("Text".to_string(), "font_weight".to_string()), typing::Type::Num);
        map.insert(("Text".to_string(), "font_style".to_string()), typing::Type::Str);
        map.insert(("Text".to_string(), "line_height".to_string()), typing::Type::Num);
        map.insert(("Text".to_string(), "letter_spacing".to_string()), typing::Type::Num);
        map.insert(("Text".to_string(), "word_spacing".to_string()), typing::Type::Num);
        map.insert(("Text".to_string(), "max_width".to_string()), typing::Type::Num);
        map.insert(("Text".to_string(), "text_align".to_string()), typing::Type::Str);
        map.insert(("Text".to_string(), "overflow".to_string()), typing::Type::Str);

        // Typst-specific
        map.insert(("Typst".to_string(), "content".to_string()), typing::Type::Str);
        map.insert(("Typst".to_string(), "font_size".to_string()), typing::Type::Num);
        map.insert(("Typst".to_string(), "font_family".to_string()), typing::Type::Str);
        map.insert(("Typst".to_string(), "font_weight".to_string()), typing::Type::Num);
        map.insert(("Typst".to_string(), "font_style".to_string()), typing::Type::Str);
        map.insert(("Typst".to_string(), "line_height".to_string()), typing::Type::Num);
        map.insert(("Typst".to_string(), "letter_spacing".to_string()), typing::Type::Num);
        map.insert(("Typst".to_string(), "word_spacing".to_string()), typing::Type::Num);
        map.insert(("Typst".to_string(), "max_width".to_string()), typing::Type::Num);
        map.insert(("Typst".to_string(), "text_align".to_string()), typing::Type::Str);
        map.insert(("Typst".to_string(), "overflow".to_string()), typing::Type::Str);

        // Code-specific
        map.insert(("Code".to_string(), "code".to_string()), typing::Type::Str);
        map.insert(("Code".to_string(), "content".to_string()), typing::Type::Str);
        map.insert(("Code".to_string(), "language".to_string()), typing::Type::Str);
        map.insert(("Code".to_string(), "font_weight".to_string()), typing::Type::Num);
        map.insert(("Code".to_string(), "font_style".to_string()), typing::Type::Str);
        map.insert(("Code".to_string(), "line_height".to_string()), typing::Type::Num);
        map.insert(("Code".to_string(), "letter_spacing".to_string()), typing::Type::Num);
        map.insert(("Code".to_string(), "word_spacing".to_string()), typing::Type::Num);
        map.insert(("Code".to_string(), "max_width".to_string()), typing::Type::Num);
        map.insert(("Code".to_string(), "text_align".to_string()), typing::Type::Str);
        map.insert(("Code".to_string(), "overflow".to_string()), typing::Type::Str);

        // Shape-specific
        for shape in &["Rect", "Ellipse", "Polygon"] {
            map.insert((shape.to_string(), "fill".to_string()), typing::Type::Color);
            map.insert((shape.to_string(), "stroke".to_string()), typing::Type::Color);
            map.insert((shape.to_string(), "stroke_width".to_string()), typing::Type::Num);
            map.insert((shape.to_string(), "size".to_string()), typing::Type::Vec2);
            map.insert((shape.to_string(), "radius".to_string()), typing::Type::Num);
        }

        // Line
        map.insert(("Line".to_string(), "start".to_string()), typing::Type::Vec2);
        map.insert(("Line".to_string(), "end".to_string()), typing::Type::Vec2);
        map.insert(("Line".to_string(), "stroke".to_string()), typing::Type::Color);
        map.insert(("Line".to_string(), "stroke_width".to_string()), typing::Type::Num);

        // Button
        map.insert(("Button".to_string(), "text".to_string()), typing::Type::Str);
        map.insert(("Button".to_string(), "size".to_string()), typing::Type::Vec2);
        map.insert(("Button".to_string(), "fill".to_string()), typing::Type::Color);
        map.insert(("Button".to_string(), "stroke".to_string()), typing::Type::Color);

        // Svg/Image
        for media in &["Svg", "Image"] {
            map.insert((media.to_string(), "url".to_string()), typing::Type::Str);
            map.insert((media.to_string(), "size".to_string()), typing::Type::Vec2);
        }

        // Graph
        map.insert(("Graph".to_string(), "x_range".to_string()), typing::Type::Vec2);
        map.insert(("Graph".to_string(), "y_range".to_string()), typing::Type::Vec2);
        map.insert(("Graph".to_string(), "function".to_string()), typing::Type::Str);

        map
    })
}

impl SymbolTable {
    /// Build a symbol table from parsed AST statements.
    pub fn build_from_ast(stmts: &[Stmt]) -> Self {
        let mut table = Self {
            types: BUILTIN_TYPES.iter().map(|s| s.to_string()).collect(),
            keywords: KEYWORDS.iter().map(|s| s.to_string()).collect(),
            actions: BUILTIN_ACTIONS.iter().map(|s| s.to_string()).collect(),
            properties: known_properties().clone(),
            property_types: known_property_types().clone(),
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
            Stmt::LetDecl {
                name, value, span, ..
            } => {
                let inferred = typing::infer_expr_type(value, &typing::TypeEnv::with_stdlib());
                self.labels.insert(
                    name.clone(),
                    LabelInfo {
                        name: name.clone(),
                        kind: LabelKind::Let,
                        line: 0, // populated by Analyzer::enrich_positions from tree-sitter
                        col: 0,  // populated by Analyzer::enrich_positions from tree-sitter
                        span: *span,
                        ty: None,
                        inferred_type: Some(inferred),
                    },
                );
            },

            Stmt::ActorDecl {
                label,
                array_index,
                ty,
                span,
                children,
                ..
            } => {
                self.labels.insert(
                    label.clone(),
                    LabelInfo {
                        name: label.clone(),
                        kind: LabelKind::Actor,
                        line: 0, // populated by Analyzer::enrich_positions from tree-sitter
                        col: 0,  // populated by Analyzer::enrich_positions from tree-sitter
                        span: *span,
                        ty: Some(ty.clone()),
                        inferred_type: Some(if self.components.contains_key(ty) {
                            typing::Type::Component(ty.clone())
                        } else {
                            typing::Type::Actor(ty.clone())
                        }),
                    },
                );
                if array_index.is_some() {
                    self.array_labels.insert(label.clone());
                }
                for child in children {
                    self.collect_inline_item(child);
                }
            },

            Stmt::ComponentDef(def, span) => {
                self.types.insert(def.name.clone());
                collect_component_internal_labels(&def.body, &mut self.component_internal_labels);
                self.components.insert(
                    def.name.clone(),
                    ComponentInfo {
                        name: def.name.clone(),
                        params: def
                            .params
                            .iter()
                            .map(|p| ParamInfo {
                                name: p.name.clone(),
                                param_type: p.param_type.clone(),
                                default: p.default.as_ref().map(|e| e.to_source()),
                            })
                            .collect(),
                        line: 0, // populated by Analyzer::enrich_positions from tree-sitter
                        col: 0,  // populated by Analyzer::enrich_positions from tree-sitter
                        span: *span,
                    },
                );

                // Recurse into component body
                for stmt in &def.body {
                    self.collect_stmt(stmt);
                }
            },

            Stmt::ComponentAction { name, body, .. } => {
                self.actions.insert(name.clone());
                for stmt in body {
                    self.collect_stmt(stmt);
                }
            },

            Stmt::ForLoop {
                var,
                index_var,
                body,
                span,
                ..
            } => {
                let var_names: Vec<String> = match var {
                    LoopPattern::Single(name) => vec![name.clone()],
                    LoopPattern::Tuple(names) => names.clone(),
                };
                for name in &var_names {
                    self.labels.insert(
                        name.clone(),
                        LabelInfo {
                            name: name.clone(),
                            kind: LabelKind::For,
                            line: 0, // populated by Analyzer::enrich_positions from tree-sitter
                            col: 0,  // populated by Analyzer::enrich_positions from tree-sitter
                            span: *span,
                            ty: None,
                            inferred_type: None,
                        },
                    );
                }

                if let Some(iv) = index_var {
                    self.labels.insert(
                        iv.clone(),
                        LabelInfo {
                            name: iv.clone(),
                            kind: LabelKind::For,
                            line: 0, // populated by Analyzer::enrich_positions from tree-sitter
                            col: 0,  // populated by Analyzer::enrich_positions from tree-sitter
                            span: *span,
                            ty: None,
                            inferred_type: None,
                        },
                    );
                }

                for stmt in body {
                    self.collect_stmt(stmt);
                }
            },

            // Recurse into blocks
            Stmt::Keyframe { body, .. } | Stmt::RelativeKeyframe { body, .. } => {
                for stmt in body {
                    self.collect_stmt(stmt);
                }
            },
            Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. } => {
                for stmt in body {
                    self.collect_stmt(stmt);
                }
            },
            Stmt::Conditional {
                then_branch,
                else_branch,
                ..
            } => {
                for stmt in then_branch {
                    self.collect_stmt(stmt);
                }
                if let Some(else_stmts) = else_branch {
                    for stmt in else_stmts {
                        self.collect_stmt(stmt);
                    }
                }
            },
            Stmt::Match { arms, .. } => {
                for (_, body) in arms {
                    for stmt in body {
                        self.collect_stmt(stmt);
                    }
                }
            },

            Stmt::Scene { name, span, .. } => {
                self.scenes.insert(
                    name.clone(),
                    SceneInfo {
                        name: name.clone(),
                        line: 0, // populated by Analyzer::enrich_positions from tree-sitter
                        col: 0,  // populated by Analyzer::enrich_positions from tree-sitter
                        span: *span,
                    },
                );
                // Recurse into scene body
                if let Stmt::Scene { body, .. } = stmt {
                    for s in body {
                        self.collect_stmt(s);
                    }
                }
            },

            Stmt::Play { .. } => {
                // No new symbols to declare, but we could track play references here.
            },

            Stmt::Import {
                path, alias, span, ..
            } => {
                self.imports.push(ImportInfo {
                    path: path.clone(),
                    alias: alias.clone(),
                    span: *span,
                });
            },

            // Actions, assignments, etc. — no symbols to extract
            _ => {},
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
            Stmt::ActorDecl { ty, props, .. } => {
                if ty == "Callout" {
                    for prop in props {
                        if prop.name == "target" {
                            self.collect_callout_target_ref(&prop.value);
                        }
                    }
                }
            },
            Stmt::Action(action, ..) => {
                for target in &action.targets {
                    self.referenced_labels.insert(target.clone());
                }
            },
            Stmt::Assignment {
                target,
                property,
                value,
                ..
            } => {
                for seg in target {
                    match seg {
                        TargetSegment::Static(label) => {
                            self.referenced_labels.insert(label.clone());
                        },
                        TargetSegment::Indexed { base, index } => {
                            self.referenced_labels.insert(base.clone());
                            self.collect_refs_from_expr(index);
                        },
                    }
                }
                if property == "target" && self.target_is_callout(target) {
                    self.collect_callout_target_ref(value);
                }
            },
            Stmt::Play { scene_name, .. } => {
                self.referenced_labels.insert(scene_name.clone());
            },
            // Recurse into blocks
            Stmt::Keyframe { body, .. } | Stmt::RelativeKeyframe { body, .. } => {
                for stmt in body {
                    self.collect_refs_from_stmt(stmt);
                }
            },
            Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. } => {
                for stmt in body {
                    self.collect_refs_from_stmt(stmt);
                }
            },
            Stmt::Conditional {
                then_branch,
                else_branch,
                ..
            } => {
                for stmt in then_branch {
                    self.collect_refs_from_stmt(stmt);
                }
                if let Some(else_stmts) = else_branch {
                    for stmt in else_stmts {
                        self.collect_refs_from_stmt(stmt);
                    }
                }
            },
            Stmt::Match { arms, .. } => {
                for (_, body) in arms {
                    for stmt in body {
                        self.collect_refs_from_stmt(stmt);
                    }
                }
            },
            Stmt::ForLoop { body, .. } => {
                for stmt in body {
                    self.collect_refs_from_stmt(stmt);
                }
            },
            Stmt::ComponentDef(def, ..) => {
                for stmt in &def.body {
                    self.collect_refs_from_stmt(stmt);
                }
            },
            Stmt::Scene { body, .. } => {
                for stmt in body {
                    self.collect_refs_from_stmt(stmt);
                }
            },
            Stmt::ReactiveBinding {
                target,
                property,
                value,
                ..
            } => {
                for seg in target {
                    self.referenced_labels.insert(seg.label_str().to_string());
                    if let TargetSegment::Indexed { index, .. } = seg {
                        self.collect_refs_from_expr(index);
                    }
                }
                if property == "target" && self.target_is_callout(target) {
                    self.collect_callout_target_ref(value);
                }
            },
            _ => {},
        }
    }

    fn target_is_callout(&self, target: &[TargetSegment]) -> bool {
        target.first().is_some_and(|seg| {
            self.labels
                .get(seg.label_str())
                .is_some_and(|info| info.ty.as_deref() == Some("Callout"))
        })
    }

    fn collect_callout_target_ref(&mut self, expr: &Expr) {
        match expr {
            Expr::Ident(name) => {
                self.referenced_labels.insert(name.clone());
            },
            Expr::Path(parts) => {
                if let Some(first) = parts.first() {
                    self.referenced_labels.insert(first.clone());
                }
            },
            Expr::Str(name) => {
                self.referenced_labels.insert(name.clone());
            },
            _ => self.collect_refs_from_expr(expr),
        }
    }

    /// Collect inline item children into the symbol table.
    /// Walk an expression and mark any identifier references into `referenced_labels`.
    fn collect_refs_from_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Ident(name) => {
                self.referenced_labels.insert(name.clone());
            },
            Expr::Path(parts) => {
                // First part is the receiver object (label reference)
                if let Some(first) = parts.first() {
                    self.referenced_labels.insert(first.clone());
                }
            },
            Expr::Index(target, index) => {
                self.collect_refs_from_expr(target);
                self.collect_refs_from_expr(index);
            },
            Expr::Tuple(items) | Expr::List(items) => {
                for item in items {
                    self.collect_refs_from_expr(item);
                }
            },
            Expr::Binary(left, _, right) => {
                self.collect_refs_from_expr(left);
                self.collect_refs_from_expr(right);
            },
            Expr::Unary(_, inner) => {
                self.collect_refs_from_expr(inner);
            },
            Expr::Call(_, args) => {
                for arg in args {
                    self.collect_refs_from_expr(arg);
                }
            },
            Expr::Method(receiver, _, args) => {
                self.collect_refs_from_expr(receiver);
                for arg in args {
                    self.collect_refs_from_expr(arg);
                }
            },
            Expr::Closure(_, body) => {
                self.collect_refs_from_expr(body);
            },
            Expr::Conditional(cond, then_branch, else_branch) => {
                self.collect_refs_from_expr(cond);
                self.collect_refs_from_expr(then_branch);
                self.collect_refs_from_expr(else_branch);
            },
            Expr::Match(scrutinee, arms) => {
                self.collect_refs_from_expr(scrutinee);
                for (_pat, arm_expr) in arms {
                    self.collect_refs_from_expr(arm_expr);
                }
            },
            Expr::Construct(_, props) => {
                for prop in props {
                    self.collect_refs_from_expr(&prop.value);
                }
            },
            // Literals (Num, Percent, Str, Bool, Null) have no identifier references
            _ => {},
        }
    }

    /// Collect inline item children into the symbol table.
    fn collect_inline_item(&mut self, item: &InlineItem) {
        match item {
            InlineItem::Labeled {
                label,
                array_index,
                children,
                ..
            } => {
                self.labels.insert(
                    label.clone(),
                    LabelInfo {
                        name: label.clone(),
                        kind: LabelKind::Actor,
                        line: 0,
                        col: 0,
                        span: None,
                        ty: None,
                        inferred_type: None,
                    },
                );
                if array_index.is_some() {
                    self.array_labels.insert(label.clone());
                }
                for child in children {
                    self.collect_inline_item(child);
                }
            },
            InlineItem::Anonymous { children, .. } => {
                for child in children {
                    self.collect_inline_item(child);
                }
            },
            InlineItem::ForLoop {
                var,
                index_var,
                body,
                ..
            } => {
                let var_names: Vec<String> = match var {
                    LoopPattern::Single(name) => vec![name.clone()],
                    LoopPattern::Tuple(names) => names.clone(),
                };
                for name in &var_names {
                    self.labels.insert(
                        name.clone(),
                        LabelInfo {
                            name: name.clone(),
                            kind: LabelKind::For,
                            line: 0,
                            col: 0,
                            span: None,
                            ty: None,
                            inferred_type: None,
                        },
                    );
                }
                if let Some(iv) = index_var {
                    self.labels.insert(
                        iv.clone(),
                        LabelInfo {
                            name: iv.clone(),
                            kind: LabelKind::For,
                            line: 0,
                            col: 0,
                            span: None,
                            ty: None,
                            inferred_type: None,
                        },
                    );
                }
                for item in body {
                    self.collect_inline_item(item);
                }
            },
            InlineItem::SlotMarker => {},
            InlineItem::SlotFill { items, .. } => {
                for item in items {
                    self.collect_inline_item(item);
                }
            },
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

    /// Resolve a namespace table by qualified path (e.g., "lib.inner").
    pub fn namespace_table(&self, qualified_name: &str) -> Option<&SymbolTable> {
        let Some((namespace, rest)) = qualified_name.split_once('.') else {
            return self.namespaces.get(qualified_name);
        };
        let ns = self.namespaces.get(namespace)?;
        ns.namespace_table(rest)
    }

    /// Look up a label by namespace-qualified name (e.g., "lib.inner.value").
    /// Returns the label info if found in the specified namespace.
    pub fn resolve_namespaced_label(&self, qualified_name: &str) -> Option<&LabelInfo> {
        let (namespace, rest) = qualified_name.split_once('.')?;
        let ns = self.namespaces.get(namespace)?;
        if rest.contains('.') {
            ns.resolve_namespaced_label(rest)
        } else {
            ns.labels.get(rest)
        }
    }

    /// Look up a component by namespace-qualified name (e.g., "lib.inner.MyComponent").
    pub fn resolve_namespaced_component(&self, qualified_name: &str) -> Option<&ComponentInfo> {
        let (namespace, rest) = qualified_name.split_once('.')?;
        let ns = self.namespaces.get(namespace)?;
        if rest.contains('.') {
            ns.resolve_namespaced_component(rest)
        } else {
            ns.components.get(rest)
        }
    }

    /// Get all labels in a specific namespace (for completions).
    pub fn namespace_labels(&self, namespace: &str) -> Vec<&str> {
        self.namespace_table(namespace)
            .map(|ns| ns.labels.keys().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }

    /// Get all components in a specific namespace (for completions).
    pub fn namespace_components(&self, namespace: &str) -> Vec<&str> {
        self.namespace_table(namespace)
            .map(|ns| ns.components.keys().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }
}

impl SymbolTable {
    /// Build a symbol-aware type environment from the current table.
    pub fn type_env(&self) -> typing::TypeEnv {
        let mut env = typing::TypeEnv::with_stdlib();
        for (name, info) in &self.labels {
            if let Some(inferred) = &info.inferred_type {
                match inferred {
                    typing::Type::Component(component) => {
                        env.declare_component_instance(name, component);
                    },
                    typing::Type::Actor(actor_ty) => {
                        env.declare_actor(name, actor_ty);
                    },
                    _ => {
                        env.bind(name, inferred.clone());
                    },
                }
            } else if let Some(ty) = &info.ty {
                if self.components.contains_key(ty) {
                    env.declare_component_instance(name, ty);
                } else {
                    env.declare_actor(name, ty);
                }
            }
        }
        for (name, info) in &self.components {
            let mut signature = typing::ComponentSignature::default();
            for param in &info.params {
                if let Some(annotation) = &param.param_type {
                    signature
                        .params
                        .insert(param.name.clone(), typing::Type::from_annotation(annotation));
                }
            }
            env.register_component(name, signature);
        }
        for (alias, namespace) in &self.namespaces {
            env.register_namespace(alias, namespace_type_from_symbols(namespace));
        }
        env
    }

    /// Infer an expression type using this table's symbols.
    pub fn infer_expr_type(&self, expr: &Expr) -> typing::Type {
        let env = self.type_env();
        typing::infer_expr_type(expr, &env)
    }
}

fn collect_component_internal_labels(stmts: &[Stmt], out: &mut HashSet<String>) {
    animatix_syntax::walk::walk_stmts(stmts, &mut |stmt| {
        if let Stmt::ActorDecl {
            label, children, ..
        } = stmt
        {
            out.insert(label.clone());
            animatix_syntax::walk::walk_inline_items(children, &mut |item| {
                if let InlineItem::Labeled { label, .. } = item {
                    out.insert(label.clone());
                }
            });
        }
    });
}

fn namespace_type_from_symbols(table: &SymbolTable) -> typing::NamespaceType {
    let mut values = HashMap::new();
    for (name, info) in &table.labels {
        let ty = match &info.inferred_type {
            Some(ty) => ty.clone(),
            None => match &info.ty {
                Some(actor_ty) if table.components.contains_key(actor_ty) => {
                    typing::Type::Component(actor_ty.clone())
                },
                Some(actor_ty) => typing::Type::Actor(actor_ty.clone()),
                None => typing::Type::Any,
            },
        };
        values.insert(name.clone(), typing::NamespaceType::Value(ty));
    }
    for (alias, namespace) in &table.namespaces {
        values.insert(alias.clone(), namespace_type_from_symbols(namespace));
    }
    typing::NamespaceType::Namespace(values)
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
        let stmts = vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "btn".to_string(),
            array_index: None,
            ty: "Button".to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![],
            span: None,
        }];
        let table = SymbolTable::build_from_ast(&stmts);
        assert!(table.labels.contains_key("btn"));
        assert_eq!(table.labels["btn"].ty.as_deref(), Some("Button"));
    }

    #[test]
    fn extracts_let_bindings() {
        let stmts = vec![Stmt::LetDecl {
            is_pub: false,
            name: "x".to_string(),
            value: Expr::Num(42.0),
            span: None,
        }];
        let table = SymbolTable::build_from_ast(&stmts);
        assert!(table.labels.contains_key("x"));
        assert_eq!(table.labels["x"].kind, LabelKind::Let);
    }

    #[test]
    fn extracts_component_definitions() {
        let stmts = vec![Stmt::ComponentDef(
            ComponentDef {
                is_pub: false,
                name: "MyButton".to_string(),
                params: vec![ParamDef {
                    name: "text".to_string(),
                    param_type: None,
                    default: Some(Expr::Str("Click".to_string())),
                }],
                body: vec![],
            },
            None,
        )];
        let table = SymbolTable::build_from_ast(&stmts);
        assert!(table.components.contains_key("MyButton"));
        assert_eq!(table.components["MyButton"].params.len(), 1);
    }

    #[test]
    fn collects_properties_from_actors() {
        let stmts = vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "title".to_string(),
            array_index: None,
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
        }];
        let table = SymbolTable::build_from_ast(&stmts);
        let text_props = table.properties.get("Text").unwrap();
        assert!(text_props.contains(&"content".to_string()));
        assert!(text_props.contains(&"font_size".to_string()));
    }

    #[test]
    fn colorscheme_paths_infer_color() {
        let env = typing::TypeEnv::with_stdlib();
        // accent.*, text.*, surface.*, stroke.* with ≥2 segments → Color
        for ns in &["accent", "text", "surface", "stroke"] {
            let path = Expr::Path(vec![ns.to_string(), "primary".to_string()]);
            assert_eq!(
                typing::infer_expr_type(&path, &env),
                typing::Type::Color,
                "{ns}.primary should be Color"
            );
        }
        // scene.* stays Any (mixes colors and anchors)
        let scene = Expr::Path(vec!["scene".to_string(), "background".to_string()]);
        assert_eq!(typing::infer_expr_type(&scene, &env), typing::Type::Any);
        // single-segment stays Any
        let single = Expr::Path(vec!["accent".to_string()]);
        assert_eq!(typing::infer_expr_type(&single, &env), typing::Type::Any);
    }

    #[test]
    fn callout_target_references_count_as_usage() {
        let actor = |label: &str| Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: label.to_string(),
            array_index: None,
            ty: "Rect".to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![],
            span: None,
        };
        let callout = Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "note".to_string(),
            array_index: None,
            ty: "Callout".to_string(),
            props: vec![Property {
                name: "target".to_string(),
                value: Expr::Ident("box1".to_string()),
                value_span: None,
                trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![],
            span: None,
        };
        let assignment = Stmt::Assignment {
            target: vec![TargetSegment::Static("note".to_string())],
            property: "target".to_string(),
            value: Expr::Str("box2".to_string()),
            modifiers: vec![],
            easing: None,
            value_span: None,
            span: None,
        };
        let stmts = vec![actor("box1"), actor("box2"), callout, assignment];
        let mut table = SymbolTable::build_from_ast(&stmts);
        table.collect_references(&stmts);
        assert!(table.referenced_labels.contains("box1"));
        assert!(table.referenced_labels.contains("box2"));
    }

    #[test]
    fn indexed_assignment_marks_runtime_var_as_referenced() {
        // `always { let sel = 1; bars[sel].color = red }`
        // `sel` should be in `referenced_labels` after collect_references.
        let stmts = vec![
            Stmt::LetDecl {
                is_pub: false,
                name: "sel".to_string(),
                value: Expr::Num(1.0),
                span: None,
            },
            Stmt::Always {
                body: vec![Stmt::Assignment {
                    target: vec![
                        TargetSegment::Indexed {
                            base: "bars".to_string(),
                            index: Box::new(Expr::Ident("sel".to_string())),
                        },
                        TargetSegment::Static("color".to_string()),
                    ],
                    property: "color".to_string(),
                    value: Expr::Path(vec!["red".to_string()]),
                    modifiers: vec![],
                    easing: None,
                    value_span: None,
                    span: None,
                }],
                span: None,
            },
        ];
        let mut table = SymbolTable::build_from_ast(&stmts);
        table.collect_references(&stmts);
        assert!(
            table.referenced_labels.contains("sel"),
            "'sel' should be referenced by bars[sel].color = red"
        );
        assert!(
            table.referenced_labels.contains("bars"),
            "'bars' should be referenced as the array base"
        );
    }
}
