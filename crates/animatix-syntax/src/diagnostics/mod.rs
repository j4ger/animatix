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
    /// A Stack container declared `gap`, which it does not use.
    IgnoredStackGap,
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
    /// A `format()` template uses a placeholder specifier the evaluator does
    /// not understand; the value falls back to default formatting so brace
    /// syntax never reaches the render.
    UnsupportedFormatSpecifier,
}

mod code;
mod format;
mod model;
#[cfg(test)]
mod tests;

pub use format::*;
pub use model::*;
