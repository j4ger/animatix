//! Message identities and arguments for localized diagnostic displays.

/// One grammar expectation from the parser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseExpectation {
    /// A label for a grammar rule.
    Rule(String),
    /// A token or keyword in canonical AMX notation.
    Syntax(String),
    /// Any token.
    Any,
    /// The end of the source.
    EndOfInput,
}

/// Data used to format a parse error in the workbench.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParsePresentation {
    /// A mismatch between a token and the active grammar rules.
    Mismatch {
        /// The expected grammar rules or tokens.
        expected: Vec<ParseExpectation>,
        /// The found token, if present.
        found: Option<String>,
    },
    /// A parser rule with its own message identity.
    Rule {
        /// The message ID.
        id: String,
        /// The source fragment supplied to the message, if present.
        argument: Option<String>,
    },
}

/// Display data carried alongside a diagnostic's canonical message.
///
/// The public diagnostic code still identifies the class of problem, which
/// can have several distinct explanations. CLI and LSP use the canonical
/// message independently of this display data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiagnosticPresentation {
    /// A parse error with typed expectations and source context.
    Parse {
        /// The failure reported by the grammar.
        problem: ParsePresentation,
        /// Enclosing grammar rules, in parser order.
        context: Vec<String>,
    },
    /// The width or height was not a positive integer literal.
    InvalidCanvasSize,
    /// The document declared the canvas size more than once.
    DuplicateCanvasSize,
    /// The retired resolution key was used.
    RemovedResolution,
    /// A scene declared a composition-level canvas size.
    SceneCanvasSize,
    /// A target label cannot be resolved.
    UndefinedLabel,
    /// A declaration names an unknown type.
    UnknownType(String),
    /// A declaration property has no consumer in the build.
    UnknownDeclarationProperty {
        /// Authored actor label.
        actor: String,
        /// Actor type.
        actor_type: String,
        /// Authored property name.
        property: String,
    },
    /// An effect lacks the named parameter.
    UnknownEffectParameter {
        /// Effect type.
        effect: String,
        /// Authored parameter name.
        parameter: String,
    },
    /// The actor type does not normally use the named property.
    UncommonProperty {
        /// Actor type.
        actor_type: String,
        /// Authored property name.
        property: String,
    },
    /// An actor property received an incompatible value.
    PropertyTypeMismatch {
        /// Actor or type name.
        owner: String,
        /// Authored property name.
        property: String,
        /// Expected DSL type, in canonical display form.
        expected: String,
        /// Inferred DSL type, in canonical display form.
        actual: String,
    },
    /// An effect parameter received an incompatible value.
    EffectParameterTypeMismatch {
        /// Authored parameter name.
        parameter: String,
        /// Expected DSL type, in canonical display form.
        expected: String,
        /// Inferred DSL type, in canonical display form.
        actual: String,
    },
    /// A declared actor is never referenced.
    UnusedActor,
    /// A declared binding is never referenced.
    UnusedBinding,
    /// A declared component is never referenced.
    UnusedComponent,
    /// A declared label is never referenced.
    UnusedLabel,
    /// An actor has no mechanism to become visible.
    NeverRevealed,
    /// An actor uses an unregistered primitive.
    UnregisteredActorType(String),
}

impl DiagnosticPresentation {
    /// Whether the diagnostic's structured subject is an actor label.
    pub fn actor_subject(&self) -> bool {
        matches!(self, Self::NeverRevealed | Self::UnregisteredActorType(_))
    }
}
