//! Display identities for completion details whose labels and insertions are DSL.

/// A completion's localizable detail. Signatures and DSL type names remain source text.
#[derive(Debug, Clone)]
pub enum CompletionDetail {
    /// Actor declaration.
    ActorDeclaration,
    /// Keyframe block.
    KeyframeBlock,
    /// Relative keyframe.
    RelativeKeyframe,
    /// Named time anchor.
    NamedTimeAnchor,
    /// Component definition.
    ComponentDefinition,
    /// Conditional block.
    ConditionalBlock,
    /// Loop block.
    LoopBlock,
    /// Always block.
    AlwaysBlock,
    /// Sequence block.
    SequenceBlock,
    /// Stagger block.
    StaggerBlock,
    /// Keyword.
    Keyword,
    /// Type.
    Type,
    /// Variable.
    Variable,
    /// Loop variable.
    LoopVariable,
    /// Component.
    Component,
    /// Boolean.
    Boolean,
    /// Null value.
    Null,
    /// Tuple.
    Tuple,
    /// Scene.
    Scene,
    /// Color.
    Color,
    /// String.
    String,
    /// Number.
    Number,
    /// Overflow.
    Overflow,
    /// Alignment.
    Alignment,
    /// Anchor.
    Anchor,
    /// Anchor point.
    AnchorPoint,
    /// Enum choice.
    EnumChoice,
    /// Modifier.
    Modifier,
    /// Function.
    Function,
    /// Common property.
    CommonProperty,
    /// Vec2 · read-only geometry.
    ReadOnlyGeometry,
    /// Num · read-only length.
    ReadOnlyLength,
    /// Vec2 · scene anchor.
    SceneAnchor,
    /// Actor: the supplied name.
    Actor(String),
    /// the supplied name parameter.
    EffectParameter(String),
    /// Property of the supplied name.
    Property(String),
    /// Extension property of the supplied name.
    ExtensionProperty(String),
}

impl CompletionDetail {
    /// Stable display identifier and optional authored name.
    pub fn presentation(&self) -> (&'static str, Option<&str>) {
        match self {
            Self::ActorDeclaration => ("completion-actor-declaration", None),
            Self::KeyframeBlock => ("completion-keyframe-block", None),
            Self::RelativeKeyframe => ("completion-relative-keyframe", None),
            Self::NamedTimeAnchor => ("completion-named-time-anchor", None),
            Self::ComponentDefinition => ("completion-component-definition", None),
            Self::ConditionalBlock => ("completion-conditional-block", None),
            Self::LoopBlock => ("completion-loop-block", None),
            Self::AlwaysBlock => ("completion-always-block", None),
            Self::SequenceBlock => ("completion-sequence-block", None),
            Self::StaggerBlock => ("completion-stagger-block", None),
            Self::Keyword => ("completion-keyword", None),
            Self::Type => ("completion-type", None),
            Self::Variable => ("completion-variable", None),
            Self::LoopVariable => ("completion-loop-variable", None),
            Self::Component => ("completion-component", None),
            Self::Boolean => ("completion-boolean", None),
            Self::Null => ("completion-null", None),
            Self::Tuple => ("completion-tuple", None),
            Self::Scene => ("completion-scene", None),
            Self::Color => ("completion-color", None),
            Self::String => ("completion-string", None),
            Self::Number => ("completion-number", None),
            Self::Overflow => ("completion-overflow", None),
            Self::Alignment => ("completion-alignment", None),
            Self::Anchor => ("completion-anchor", None),
            Self::AnchorPoint => ("completion-anchor-point", None),
            Self::EnumChoice => ("completion-enum-choice", None),
            Self::Modifier => ("completion-modifier", None),
            Self::Function => ("completion-function", None),
            Self::CommonProperty => ("completion-common-property", None),
            Self::ReadOnlyGeometry => ("completion-read-only-geometry", None),
            Self::ReadOnlyLength => ("completion-read-only-length", None),
            Self::SceneAnchor => ("completion-scene-anchor", None),
            Self::Actor(name) => ("completion-actor", Some(name)),
            Self::EffectParameter(name) => ("completion-effect-parameter", Some(name)),
            Self::Property(name) => ("completion-property", Some(name)),
            Self::ExtensionProperty(name) => ("completion-extension-property", Some(name)),
        }
    }
}
