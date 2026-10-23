use std::fmt;
use std::ops::Range;
use std::path::PathBuf;

/// Severity level of a diagnostic message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    /// A non-fatal issue that should be reported but does not stop processing.
    Warning,
    /// A fatal issue that prevents successful completion of the current phase.
    Error,
    /// Informational feedback that is not a problem by itself.
    Info,
    /// A helpful suggestion or hint.
    Hint,
}

impl fmt::Display for DiagnosticSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagnosticSeverity::Warning => write!(f, "warning"),
            DiagnosticSeverity::Error => write!(f, "error"),
            DiagnosticSeverity::Info => write!(f, "info"),
            DiagnosticSeverity::Hint => write!(f, "hint"),
        }
    }
}

/// Phase of the pipeline that produced a diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticPhase {
    /// The parsing phase.
    Parse,
    /// The build (compilation) phase.
    Build,
    /// The render phase.
    Render,
}

impl fmt::Display for DiagnosticPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagnosticPhase::Parse => write!(f, "parse"),
            DiagnosticPhase::Build => write!(f, "build"),
            DiagnosticPhase::Render => write!(f, "render"),
        }
    }
}

/// Unique code identifying the kind of diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DiagnosticCode {
    /// Failed to load a source file.
    SourceLoadFailure,
    /// A syntax or parse error was encountered.
    ParseError,
    /// The render step failed.
    RenderFailure,
    /// A modifier key is not supported in the current context.
    UnsupportedModifierKey,
    /// An assignment property is not supported.
    UnsupportedAssignmentProperty,
    /// The target of an assignment is invalid.
    InvalidAssignmentTarget,
    /// A modifier value is invalid.
    InvalidModifierValue,
    /// An `always` block writes a property that is not animatable; the
    /// per-frame write is ignored.
    AlwaysWriteNotAnimatable,
    /// A configuration value is invalid.
    InvalidConfigValue,
    /// Two modifier keys conflict with each other.
    ConflictingModifierKey,
    /// The referenced action is unknown.
    UnknownAction,
    /// The referenced colorscheme is unknown.
    UnknownColorscheme,
    /// The referenced color is unknown.
    UnknownColorReference,
    /// The target of an action is not supported.
    UnsupportedActionTarget,
    /// A statement inside a sequence is not supported.
    UnsupportedSequenceStatement,
    /// The target path could not be resolved.
    UnknownTargetPath,
    /// The lookup path could not be resolved.
    UnknownLookupPath,
    /// A statement inside a stagger is not supported.
    UnsupportedStaggerStatement,
    /// Failed to load a media asset.
    MediaLoadFailure,
    /// Layout size had to fall back to a default.
    LayoutSizeFallback,
    /// A media assignment is not supported.
    UnsupportedMediaAssignment,
    /// Colorscheme data is malformed or invalid.
    InvalidColorschemeData,
    /// A cycle was detected in colorscheme inheritance.
    ColorschemeInheritanceCycle,
    /// Evaluation of a module export failed.
    ModuleExportEvalError,
    /// A modifier failed to compile.
    ModifierCompilationError,
    /// Coordinate system friction: two position bindings conflict.
    ConflictingPositionBinding,
    /// Coordinate system friction: an offset was ignored.
    IgnoredOffset,
    /// Multi-scene composition: a scene name was used more than once.
    DuplicateSceneName,
    /// Multi-scene composition: the play target was not found.
    PlayTargetNotFound,
    /// Multi-scene composition: a play cycle was detected.
    PlayCycleDetected,
    /// Multi-scene composition: a scene has multiple `play` statements (only the first is used).
    MultiplePlayTargets,
    /// A `play` statement sits inside a keyframe body where the composition
    /// builder never sees it.
    PlayInsideKeyframe,
    /// An actor is seeded hidden (pre-keyframe) and no entrance action ever
    /// lifted it — it will never be visible.
    NeverRevealed,
    /// `config { duration: N }` is shorter than the last keyframe, so the
    /// timeline ends before that animation is reached.
    DurationShorterThanContent,
    /// A scene declared `config { seamless_loop: true }` but at least one
    /// keyframed value differs between the first and last frame, so every
    /// replay jumps at the seam.
    LoopNotSeamless,
    /// A `config` block names a key the build does not recognise, so it is
    /// dropped: almost always a typo that silently does nothing.
    UnknownConfigKey,
    /// A Grid lacks an explicit `cols` and will lay out in a single column.
    MissingGridCols,
    /// Multi-scene composition: a scene is unreachable (no `play` edge leads to it).
    OrphanScene,
    /// Scene persistence: `persist` was given a duration argument, which is ignored.
    PersistIgnoresDuration,
    /// Scene persistence: `persist` targets a layout-managed leaf child directly.
    PersistLayoutManagedChild,
    /// Scene persistence: `persist` is used in a single-scene file or the last scene (no
    /// successor).
    PersistTargetNotCarried,
    /// Scene persistence: a scene has multiple predecessors in the play graph.
    CarryAmbiguousPredecessor,
    /// Scene persistence: `persist` follows `remove` in the same scene.
    PersistAfterRemove,
    /// The plot function is invalid.
    InvalidPlotFunc,
    /// The actor type is unknown.
    UnknownActorType,
    /// A type mismatch was detected during type checking.
    TypeMismatch,
    /// A modifier failed at runtime during frame evaluation.
    ModifierRuntimeError,
    /// A component instance property does not match any defined parameter.
    UnknownComponentProperty,
    /// An `always` block writes to a property that also has keyframes.
    AlwaysOverridesKeyframes,
    /// An `at` or `position` property was set on a layout-managed child.
    AbsolutePositionOnLayoutManagedChild,
    /// A deprecated primitive was used; the user should migrate to the replacement.
    DeprecatedPrimitive,
    /// A property value is invalid for the expected type or shape.
    InvalidPropertyValue,
    /// Actor label uses reserved `__` prefix.
    ReservedLabelPrefix,
    /// A property was placed inside braces without a preceding actor, so it was dropped.
    BracedPropertySilentDrop,
    /// A Callout `target` references an actor that does not exist in the timeline.
    CalloutTargetNotFound,
    /// A `match` expression or block is missing a required `_` wildcard arm.
    MissingWildcardArm,
    /// A type alias references an alias that is not defined, or forms a cycle.
    UnknownTypeAlias,
    /// The same actor/binding label is declared more than once.
    DuplicateLabel,
    /// A declared actor/binding is never referenced.
    UnusedLabel,
    /// An action or assignment target references a label that is not declared.
    UndefinedLabel,
    /// A property is not part of the known surface for its actor type.
    UnknownProperty,
    /// A property name the build knows, but which this actor's primitive never
    /// reads, so the authored value is dropped.
    InapplicableProperty,
    /// A property the build accepts and stores, but whose effect is not
    /// implemented yet, so authoring it does nothing. Distinct from
    /// `InapplicableProperty` (wrong actor for the property) and
    /// `UnknownProperty` (typo): the value is understood, it just has no
    /// consumer.
    UnimplementedProperty,
    /// An actor type name is not recognized.
    UnknownType,
    /// Text content contains characters the shaping path has no glyph for; they
    /// will render blank or as tofu boxes.
    MissingGlyph,
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagnosticCode::SourceLoadFailure => write!(f, "source-load-failure"),
            DiagnosticCode::ParseError => write!(f, "parse-error"),
            DiagnosticCode::RenderFailure => write!(f, "render-failure"),
            DiagnosticCode::UnsupportedModifierKey => write!(f, "unsupported-modifier-key"),
            DiagnosticCode::UnsupportedAssignmentProperty => {
                write!(f, "unsupported-assignment-property")
            },
            DiagnosticCode::InvalidAssignmentTarget => {
                write!(f, "invalid-assignment-target")
            },
            DiagnosticCode::InvalidModifierValue => write!(f, "invalid-modifier-value"),
            DiagnosticCode::AlwaysWriteNotAnimatable => {
                write!(f, "always-write-not-animatable")
            },
            DiagnosticCode::InvalidConfigValue => write!(f, "invalid-config-value"),
            DiagnosticCode::ConflictingModifierKey => write!(f, "conflicting-modifier-key"),
            DiagnosticCode::UnknownAction => write!(f, "unknown-action"),
            DiagnosticCode::UnknownColorscheme => write!(f, "unknown-colorscheme"),
            DiagnosticCode::UnknownColorReference => write!(f, "unknown-color-reference"),
            DiagnosticCode::UnsupportedActionTarget => write!(f, "unsupported-action-target"),
            DiagnosticCode::UnsupportedSequenceStatement => {
                write!(f, "unsupported-sequence-statement")
            },
            DiagnosticCode::UnknownTargetPath => write!(f, "unknown-target-path"),
            DiagnosticCode::UnknownLookupPath => write!(f, "unknown-lookup-path"),
            DiagnosticCode::UnsupportedStaggerStatement => {
                write!(f, "unsupported-stagger-statement")
            },
            DiagnosticCode::MediaLoadFailure => write!(f, "media-load-failure"),
            DiagnosticCode::LayoutSizeFallback => write!(f, "layout-size-fallback"),
            DiagnosticCode::UnsupportedMediaAssignment => write!(f, "unsupported-media-assignment"),
            DiagnosticCode::InvalidColorschemeData => write!(f, "invalid-colorscheme-data"),
            DiagnosticCode::ColorschemeInheritanceCycle => {
                write!(f, "colorscheme-inheritance-cycle")
            },
            DiagnosticCode::ModuleExportEvalError => write!(f, "module-export-eval-error"),
            DiagnosticCode::ModifierCompilationError => write!(f, "modifier-compilation-error"),
            DiagnosticCode::ConflictingPositionBinding => {
                write!(f, "conflicting-position-binding")
            },
            DiagnosticCode::IgnoredOffset => write!(f, "ignored-offset"),
            DiagnosticCode::DuplicateSceneName => write!(f, "duplicate-scene-name"),
            DiagnosticCode::PlayTargetNotFound => write!(f, "play-target-not-found"),
            DiagnosticCode::PlayCycleDetected => write!(f, "play-cycle-detected"),
            DiagnosticCode::MultiplePlayTargets => write!(f, "multiple-play-targets"),
            DiagnosticCode::PlayInsideKeyframe => write!(f, "play-inside-keyframe"),
            DiagnosticCode::NeverRevealed => write!(f, "never-revealed"),
            DiagnosticCode::DurationShorterThanContent => {
                write!(f, "duration-shorter-than-content")
            },
            DiagnosticCode::LoopNotSeamless => write!(f, "loop-not-seamless"),
            DiagnosticCode::UnknownConfigKey => write!(f, "unknown-config-key"),
            DiagnosticCode::MissingGridCols => write!(f, "missing-grid-cols"),
            DiagnosticCode::OrphanScene => write!(f, "orphan-scene"),
            DiagnosticCode::PersistIgnoresDuration => write!(f, "persist-ignores-duration"),
            DiagnosticCode::PersistLayoutManagedChild => write!(f, "persist-layout-managed-child"),
            DiagnosticCode::PersistTargetNotCarried => write!(f, "persist-target-not-carried"),
            DiagnosticCode::CarryAmbiguousPredecessor => write!(f, "carry-ambiguous-predecessor"),
            DiagnosticCode::PersistAfterRemove => write!(f, "persist-after-remove"),
            DiagnosticCode::InvalidPlotFunc => write!(f, "invalid-plot-func"),
            DiagnosticCode::UnknownActorType => write!(f, "unknown-actor-type"),
            DiagnosticCode::TypeMismatch => write!(f, "type-mismatch"),
            DiagnosticCode::ModifierRuntimeError => write!(f, "modifier-runtime-error"),
            DiagnosticCode::UnknownComponentProperty => write!(f, "unknown-component-property"),
            DiagnosticCode::AlwaysOverridesKeyframes => {
                write!(f, "always-overrides-keyframes")
            },
            DiagnosticCode::AbsolutePositionOnLayoutManagedChild => {
                write!(f, "absolute-position-on-layout-managed-child")
            },
            DiagnosticCode::DeprecatedPrimitive => write!(f, "deprecated-primitive"),
            DiagnosticCode::InvalidPropertyValue => write!(f, "invalid-property-value"),
            DiagnosticCode::ReservedLabelPrefix => write!(f, "reserved-label-prefix"),
            DiagnosticCode::BracedPropertySilentDrop => {
                write!(f, "braced-property-silent-drop")
            },
            DiagnosticCode::CalloutTargetNotFound => write!(f, "callout-target-not-found"),
            DiagnosticCode::MissingWildcardArm => write!(f, "missing-wildcard-arm"),
            DiagnosticCode::UnknownTypeAlias => write!(f, "unknown-type-alias"),
            DiagnosticCode::DuplicateLabel => write!(f, "duplicate-label"),
            DiagnosticCode::UnusedLabel => write!(f, "unused-label"),
            DiagnosticCode::UndefinedLabel => write!(f, "undefined-label"),
            DiagnosticCode::UnknownProperty => write!(f, "unknown-property"),
            DiagnosticCode::InapplicableProperty => write!(f, "inapplicable-property"),
            DiagnosticCode::UnimplementedProperty => write!(f, "unimplemented-property"),
            DiagnosticCode::MissingGlyph => write!(f, "missing-glyph"),
            DiagnosticCode::UnknownType => write!(f, "unknown-type"),
        }
    }
}

mod format;
mod model;
#[cfg(test)]
mod tests;

pub use format::*;
pub use model::*;
